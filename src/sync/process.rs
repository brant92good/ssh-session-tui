//! Bounded Git subprocesses: no blocking pipe joins or detached reader threads.
use anyhow::{Context, Result, bail, ensure};
use std::{
    io::{self, Read},
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

const MAX_OUTPUT: usize = 16 * 1024 * 1024;
trait Stream: Read {
    fn prepare(&self) -> io::Result<()>;
    /// None means EOF, Some(0) means no bytes ready yet.
    fn available(&self) -> io::Result<Option<usize>>;
}
#[cfg(unix)]
impl<T: Read + std::os::fd::AsRawFd> Stream for T {
    fn prepare(&self) -> io::Result<()> {
        // This pipe has one reader, owned by this call. O_NONBLOCK affects only
        // its read end, not the child's distinct write-end file description.
        unsafe {
            let flags = libc::fcntl(self.as_raw_fd(), libc::F_GETFL);
            if flags < 0
                || libc::fcntl(self.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) < 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
    fn available(&self) -> io::Result<Option<usize>> {
        Ok(Some(8192))
    }
}
#[cfg(windows)]
impl<T: Read + std::os::windows::io::AsRawHandle> Stream for T {
    fn prepare(&self) -> io::Result<()> {
        Ok(())
    }
    fn available(&self) -> io::Result<Option<usize>> {
        use windows_sys::Win32::{Foundation::ERROR_BROKEN_PIPE, System::Pipes::PeekNamedPipe};
        let mut count = 0;
        // No other thread reads this handle. Read only the bytes reported ready;
        // anonymous-pipe peeking does not consume data or wait for a future write.
        let ok = unsafe {
            PeekNamedPipe(
                self.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut count,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
                return Ok(None);
            }
            return Err(error);
        }
        Ok(Some(count as usize))
    }
}
struct Pipe<T> {
    stream: T,
    bytes: Vec<u8>,
    done: bool,
}
impl<T: Stream> Pipe<T> {
    fn new(stream: T) -> Result<Self> {
        stream.prepare()?;
        Ok(Self {
            stream,
            bytes: Vec::new(),
            done: false,
        })
    }
    fn drain(&mut self) -> Result<()> {
        if self.done {
            return Ok(());
        }
        let mut buffer = [0; 8192];
        // Bound each turn even when the child continuously floods output.
        for _ in 0..8 {
            let Some(count) = self.stream.available()? else {
                self.done = true;
                break;
            };
            if count == 0 {
                break;
            }
            let count = count.min(buffer.len());
            match self.stream.read(&mut buffer[..count]) {
                Ok(0) => {
                    self.done = true;
                    break;
                }
                Ok(count) => {
                    ensure!(
                        self.bytes.len() + count <= MAX_OUTPUT,
                        "Git output exceeded 16 MiB. Review this operation outside the app."
                    );
                    self.bytes.extend_from_slice(&buffer[..count]);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
}

struct OwnedChild {
    child: Child,
    group: platform::Group,
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        self.group.stop();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
pub(super) fn run(mut command: Command, timeout: Duration) -> Result<(i32, Vec<u8>, Vec<u8>)> {
    let deadline = Instant::now() + timeout;
    let (child, group) = platform::spawn(&mut command)?;
    let mut owned = OwnedChild { child, group };
    let mut stdout = Pipe::new(owned.child.stdout.take().context("Missing Git stdout")?)?;
    let mut stderr = Pipe::new(owned.child.stderr.take().context("Missing Git stderr")?)?;
    loop {
        stdout.drain()?;
        stderr.drain()?;
        // Keep the child unreaped while descendants hold its pipes. Besides
        // preserving one deadline, this keeps its process-group ID reserved.
        if stdout.done
            && stderr.done
            && let Some(status) = owned.child.try_wait()?
        {
            return Ok((status.code().unwrap_or(1), stdout.bytes, stderr.bytes));
        }
        if Instant::now() >= deadline {
            bail!(
                "Git timed out. Its process and output did not finish. Check the connection or Git hooks and try again."
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::os::unix::process::CommandExt;
    pub(super) struct Group(libc::pid_t);
    impl Group {
        pub(super) fn stop(&mut self) {
            if self.0 > 0 {
                // Negative PID targets only this call's newly created group.
                unsafe {
                    libc::kill(-self.0, libc::SIGKILL);
                }
                self.0 = 0;
            }
        }
    }
    pub(super) fn spawn(command: &mut Command) -> Result<(Child, Group)> {
        command.process_group(0);
        let child = command.spawn().context("Could not start Git")?;
        let group = Group(child.id() as libc::pid_t);
        Ok((child, group))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Write, process::Stdio};

    fn fixture_command(role: &str, directory: &std::path::Path) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "sync::process::tests::fixture_entry",
                "--ignored",
                "--nocapture",
            ])
            .env("SSH_PROCESS_FIXTURE_ROLE", role)
            .env("SSH_PROCESS_FIXTURE_DIR", directory)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
    #[test]
    #[ignore = "Subprocess fixture, launched only by the owned-process tests"]
    fn fixture_entry() {
        let Ok(role) = std::env::var("SSH_PROCESS_FIXTURE_ROLE") else {
            return;
        };
        let directory =
            std::path::PathBuf::from(std::env::var_os("SSH_PROCESS_FIXTURE_DIR").unwrap());
        match role.as_str() {
            "parent" => {
                let mut command = fixture_command("hold", &directory);
                command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
                }
                let child = command.spawn().unwrap();
                fs::write(directory.join("child.pid"), child.id().to_string()).unwrap();
                // Deliberately leave a descendant holding both inherited pipes.
                std::process::exit(0);
            }
            "hold" => loop {
                thread::sleep(Duration::from_secs(1));
            },
            "both-pipes" => {
                for _ in 0..128 {
                    io::stdout().write_all(&[b'o'; 8192]).unwrap();
                    io::stderr().write_all(&[b'e'; 8192]).unwrap();
                }
            }
            _ => panic!("Unknown isolated subprocess fixture"),
        }
    }
    #[test]
    fn drains_both_full_pipes_without_reader_threads() {
        let temp = tempfile::tempdir().unwrap();
        let (code, stdout, stderr) = run(
            fixture_command("both-pipes", temp.path()),
            Duration::from_secs(10),
        )
        .unwrap();
        assert_eq!(code, 0);
        assert!(stdout.len() >= 1024 * 1024);
        assert!(stderr.len() >= 1024 * 1024);
    }
    #[test]
    fn exited_parent_with_pipe_holding_descendant_is_bounded_and_cleaned_up() {
        let temp = tempfile::tempdir().unwrap();
        let started = Instant::now();
        let result = run(
            fixture_command("parent", temp.path()),
            Duration::from_secs(1),
        );
        assert!(result.unwrap_err().to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(4));
        let pid: u32 = fs::read_to_string(temp.path().join("child.pid"))
            .unwrap()
            .parse()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while running(pid) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !running(pid),
            "Owned pipe-holding descendant must not survive timeout"
        );
    }
    #[cfg(windows)]
    fn running(pid: u32) -> bool {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, WAIT_TIMEOUT},
            System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
        };
        unsafe {
            let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if handle.is_null() {
                return false;
            }
            let running = WaitForSingleObject(handle, 0) == WAIT_TIMEOUT;
            CloseHandle(handle);
            running
        }
    }
    #[cfg(unix)]
    fn running(pid: u32) -> bool {
        #[cfg(target_os = "linux")]
        if let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat"))
            && stat
                .rsplit_once(')')
                .is_some_and(|(_, fields)| fields.trim_start().starts_with('Z'))
        {
            return false; // Dead orphan waiting for the CI container's init to reap it.
        }
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }
}
#[cfg(windows)]
mod platform {
    use super::*;
    use std::os::windows::{io::AsRawHandle, process::CommandExt};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First,
                Thread32Next,
            },
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            },
            Threading::{
                CREATE_NO_WINDOW, CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
            },
        },
    };
    struct Handle(HANDLE);
    impl Handle {
        fn new(value: HANDLE) -> io::Result<Self> {
            if value.is_null() || value == INVALID_HANDLE_VALUE {
                Err(io::Error::last_os_error())
            } else {
                Ok(Self(value))
            }
        }
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    pub(super) struct Group(Option<Handle>);
    impl Group {
        pub(super) fn stop(&mut self) {
            self.0.take();
        }
    }
    fn resume_primary(pid: u32) -> Result<()> {
        let snapshot = Handle::new(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) })?;
        let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
        let mut found = unsafe { Thread32First(snapshot.0, &mut entry) };
        while found != 0 {
            if entry.th32OwnerProcessID == pid {
                let thread = Handle::new(unsafe {
                    OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID)
                })?;
                ensure!(
                    unsafe { ResumeThread(thread.0) } != u32::MAX,
                    "Could not resume Git: {}",
                    io::Error::last_os_error()
                );
                return Ok(());
            }
            found = unsafe { Thread32Next(snapshot.0, &mut entry) };
        }
        bail!("Could not find the suspended Git thread");
    }
    pub(super) fn spawn(command: &mut Command) -> Result<(Child, Group)> {
        let job = Handle::new(unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) })?;
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        ensure!(
            unsafe {
                SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as _,
                    std::mem::size_of_val(&limits) as u32,
                )
            } != 0,
            "Could not configure Git process ownership: {}",
            io::Error::last_os_error()
        );
        command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        let mut child = command.spawn().context("Could not start Git")?;
        // The child cannot execute code or create descendants before assignment.
        let setup = (|| -> Result<()> {
            ensure!(
                unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle()) } != 0,
                "Could not own Git process: {}",
                io::Error::last_os_error()
            );
            resume_primary(child.id())
        })();
        if let Err(error) = setup {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        Ok((child, Group(Some(job))))
    }
}
