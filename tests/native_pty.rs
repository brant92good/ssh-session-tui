//! Own a real OS pseudo-terminal. Never open or activate a desktop window.
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use ssh_sessions::{
    catalog::Catalog,
    favorites::{Favorites, LOCAL},
};
use std::{
    io::{Read, Write},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

struct Session {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    pair: portable_pty::PtyPair,
    input: Box<dyn Write + Send>,
    output: mpsc::Receiver<Vec<u8>>,
    seen: Vec<u8>,
}
impl Session {
    fn expect(&mut self, text: &str) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            // ConPTY asks the terminal host for its initial cursor position.
            // A real terminal replies; this owned harness must do so as well.
            if let Some(index) = self.seen.windows(4).position(|part| part == b"\x1b[6n") {
                self.seen.drain(index..index + 4);
                self.send("\x1b[1;1R");
            }
            if let Some(index) = self
                .seen
                .windows(text.len())
                .position(|part| part == text.as_bytes())
            {
                self.seen.drain(..index + text.len());
                return;
            }
            assert!(
                Instant::now() < deadline,
                "Did not receive {text:?}: {}",
                String::from_utf8_lossy(&self.seen)
            );
            if let Ok(bytes) = self.output.recv_timeout(Duration::from_millis(100)) {
                self.seen.extend(bytes);
            }
        }
    }
    fn send(&mut self, value: &str) {
        self.input.write_all(value.as_bytes()).unwrap();
        self.input.flush().unwrap();
    }
    fn marker(&mut self) {
        let marker = format!("shell-{}", uuid::Uuid::new_v4().simple());
        #[cfg(windows)]
        let command = format!(
            "[Console]::WriteLine([string]::Concat([char[]]({})))\r",
            marker
                .bytes()
                .map(|b| b.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        #[cfg(not(windows))]
        let command = format!(
            "printf '{}\\n'\n",
            marker
                .bytes()
                .map(|b| format!("\\{b:03o}"))
                .collect::<String>()
        );
        self.send(&command);
        self.expect(&marker);
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
        }
    }
}

#[test]
fn real_local_shell_interrupt_resize_return_and_clean_exit() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::new(
        &temp.path().join("catalog.json"),
        &temp.path().join("device"),
    )
    .unwrap();
    catalog
        .save(&[], &catalog.load().unwrap().revision)
        .unwrap();
    Favorites::assign(&catalog, "2", Some(LOCAL), None).unwrap();
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 110,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_ssh-sessions"));
    command.args([
        "--catalog",
        catalog.path.to_str().unwrap(),
        "--state-dir",
        catalog.state_dir.to_str().unwrap(),
    ]);
    command.cwd(temp.path());
    command.env("TERM", "xterm-256color");
    command.env("SHELL", "/bin/sh");
    command.env("PS1", "ssh-test-ready> ");
    command.env_remove("ENV");
    let child = pair.slave.spawn_command(command).unwrap();
    let mut reader = pair.master.try_clone_reader().unwrap();
    let input = pair.master.take_writer().unwrap();
    let (sender, output) = mpsc::channel();
    thread::spawn(move || {
        let mut buffer = [0; 8192];
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 || sender.send(buffer[..count].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut session = Session {
        child,
        pair,
        input,
        output,
        seen: Vec::new(),
    };
    session.expect("Local terminal");
    session.send("2\r");
    #[cfg(windows)]
    session.expect("PS ");
    #[cfg(not(windows))]
    session.expect("ssh-test-ready> ");
    session.marker();
    #[cfg(windows)]
    session.send("Start-Sleep -Seconds 30\r");
    #[cfg(not(windows))]
    session.send("sleep 30\n");
    thread::sleep(Duration::from_millis(300));
    session.send("\x03");
    thread::sleep(Duration::from_millis(300));
    session.marker();
    session
        .pair
        .master
        .resize(PtySize {
            rows: 30,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    session.send(if cfg!(windows) { "exit\r" } else { "exit\n" });
    session.expect("Local shell ended (exit 0)");
    session.send("q");
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(status) = session.child.try_wait().unwrap() {
            assert!(
                status.success(),
                "Picker exit after Ctrl+C and shell exit: {status:?}"
            );
            break;
        }
        assert!(Instant::now() < deadline, "Picker did not exit after Q");
        let _ = session.output.recv_timeout(Duration::from_millis(50));
    }
}
