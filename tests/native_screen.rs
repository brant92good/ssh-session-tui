//! Real local shell -> test-only SSH executable -> picker, without desktop/network.
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use ssh_sessions::{
    catalog::{Catalog, Machine, Route},
    favorites::{Favorites, LOCAL},
};
use std::{
    io::{Read, Write},
    path::Path,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

struct Session {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    _pair: portable_pty::PtyPair,
    input: Box<dyn Write + Send>,
    output: mpsc::Receiver<Vec<u8>>,
    parser: vt100::Parser,
    raw: Vec<u8>,
    checked: usize,
}
impl Session {
    fn send(&mut self, value: &str) {
        self.input.write_all(value.as_bytes()).unwrap();
        self.input.flush().unwrap();
    }
    fn pump(&mut self) {
        if let Ok(bytes) = self.output.recv_timeout(Duration::from_millis(40)) {
            self.raw.extend(&bytes);
            self.parser.process(&bytes);
            // ConPTY cursor position query can straddle reads.
            let tail = &self.raw[self.checked.saturating_sub(3)..];
            let replies = tail.windows(4).filter(|s| *s == b"\x1b[6n").count();
            self.checked = self.raw.len();
            for _ in 0..replies {
                self.send("\x1b[1;1R");
            }
        }
    }
    fn expect(&mut self, text: &str) {
        let end = Instant::now() + Duration::from_secs(20);
        while !self.parser.screen().contents().contains(text) {
            assert!(
                Instant::now() < end,
                "Missing {text:?}: {}",
                self.parser.screen().contents()
            );
            self.pump();
        }
    }
    fn quiet(&mut self) {
        let end = Instant::now() + Duration::from_millis(200);
        while Instant::now() < end {
            self.pump();
        }
    }
    fn exit(&mut self) {
        let end = Instant::now() + Duration::from_secs(8);
        loop {
            self.pump();
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success(), "Picker exited unsuccessfully: {status:?}");
                self.quiet();
                return;
            }
            assert!(Instant::now() < end, "Picker did not exit");
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
        }
    }
}

fn exercise(python: Option<&Path>) {
    let temp = tempfile::tempdir().unwrap();
    let stub_source = temp.path().join("ssh_fixture.rs");
    std::fs::write(
        &stub_source,
        r#"
use std::io::{self, Write};
fn main() {
    if std::env::current_exe().unwrap().file_stem().unwrap() == "pwsh" {
        #[cfg(windows)]
        unsafe {
            unsafe extern "system" { fn SetConsoleCtrlHandler(handler: Option<unsafe extern "system" fn(u32) -> i32>, add: i32) -> i32; }
            unsafe extern "system" fn keep_wrapper(_: u32) -> i32 { 1 }
            assert_ne!(SetConsoleCtrlHandler(Some(keep_wrapper), 1), 0);
        }
        // Actual PowerShell, without loading owner profiles or writing PSReadLine
        // history. Only this fixture executable injects these test arguments.
        let code = std::process::Command::new(std::env::var_os("SSH_SCREEN_TEST_REAL_PWSH").unwrap())
            .args(["-NoProfile", "-NoLogo", "-NoExit", "-Command", "Set-PSReadLineOption -HistorySaveStyle SaveNothing"])
            .status().unwrap();
        std::process::exit(code.code().unwrap_or(130));
    }
    if std::env::args().last().as_deref() == Some("failure.invalid") {
        println!("OWNED_SSH_FAILURE_255");
        std::process::exit(255);
    }
    println!("OWNED_REMOTE_READY");
    io::stdout().flush().unwrap();
    let mut line = String::new();
    io::stdin().read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "exit");
    for _ in 0..55 { println!("OWNED_REMOTE_HISTORY"); }
    println!("OWNED_REMOTE_LOGOUT");
}
"#,
    )
    .unwrap();
    let stub = temp
        .path()
        .join(if cfg!(windows) { "ssh.exe" } else { "ssh" });
    assert!(
        std::process::Command::new("rustc")
            .arg(&stub_source)
            .arg("-o")
            .arg(&stub)
            .status()
            .unwrap()
            .success()
    );
    #[cfg(windows)]
    std::fs::copy(&stub, temp.path().join("pwsh.exe")).unwrap();
    let catalog = Catalog::new(
        &temp.path().join("catalog.json"),
        &temp.path().join("device"),
    )
    .unwrap();
    let machine = |id: &str, host: &str| Machine {
        id: id.into(),
        name: id.into(),
        user: "fixture".into(),
        group: String::new(),
        tags: vec![],
        routes: vec![Route {
            id: "one".into(),
            name: "Fixture route".into(),
            host: host.into(),
            port: 22,
            ssh_alias: None,
        }],
    };
    catalog
        .save(
            &[
                machine("remote", "remote.invalid"),
                machine("failure", "failure.invalid"),
            ],
            &catalog.load().unwrap().revision,
        )
        .unwrap();
    Favorites::assign(&catalog, "1", Some("remote"), None).unwrap();
    Favorites::assign(&catalog, "2", Some(LOCAL), None).unwrap();
    Favorites::assign(&catalog, "3", Some("failure"), None).unwrap();
    // History sentinel files are fixture-only. The application must not alter them.
    let history = temp.path().join("history-sentinel");
    std::fs::write(&history, b"history must survive\n").unwrap();
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 30,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = if let Some(python) = python {
        let mut command = CommandBuilder::new(python);
        command.args(["-E", "-s", concat!(env!("CARGO_MANIFEST_DIR"), "/app.py")]);
        command
    } else {
        CommandBuilder::new(env!("CARGO_BIN_EXE_ssh-sessions"))
    };
    command.args([
        "--catalog",
        catalog.path.to_str().unwrap(),
        "--state-dir",
        catalog.state_dir.to_str().unwrap(),
    ]);
    let paths = std::iter::once(temp.path().to_path_buf())
        .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap()))
        .collect::<Vec<_>>();
    command.env("PATH", std::env::join_paths(paths).unwrap());
    #[cfg(windows)]
    command.env(
        "SSH_SCREEN_TEST_REAL_PWSH",
        which::which("pwsh.exe").unwrap(),
    );
    command.env("TERM", "xterm-256color");
    command.env("SHELL", "/bin/sh");
    command.env("PS1", "OWNED_LOCAL_PROMPT> ");
    command.env("HISTFILE", &history);
    command.env_remove("ENV");
    command.cwd(temp.path());
    let child = pair.slave.spawn_command(command).unwrap();
    let mut reader = pair.master.try_clone_reader().unwrap();
    let input = pair.master.take_writer().unwrap();
    let (sender, output) = mpsc::channel();
    thread::spawn(move || {
        let mut data = [0; 8192];
        while let Ok(n) = reader.read(&mut data) {
            if n == 0 || sender.send(data[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut session = Session {
        child,
        _pair: pair,
        input,
        output,
        parser: vt100::Parser::new(30, 120, 500),
        raw: vec![],
        checked: 0,
    };
    session.expect("Local terminal");
    session.send("2\r");
    session.expect(if cfg!(windows) {
        "PS "
    } else {
        "OWNED_LOCAL_PROMPT> "
    });
    // Commands contain split strings, so seeing the sentinel proves execution, not echo.
    session.send(if cfg!(windows) { "1..55 | ForEach-Object { [Console]::WriteLine(('OWNED_LOCAL_' + 'HISTORY')) }; [Console]::WriteLine(('OWNED_LOCAL_' + 'DONE'))\r" } else { "i=0; while [ $i -lt 55 ]; do printf 'OWNED_LOCAL_%s\\n' HISTORY; i=$((i+1)); done; printf 'OWNED_LOCAL_%s\\n' DONE\n" });
    session.expect("OWNED_LOCAL_DONE");
    session.send(if cfg!(windows) {
        "Start-Sleep -Seconds 30\r"
    } else {
        "sleep 30\n"
    });
    session.quiet();
    session.send("\x03");
    session.quiet();
    session.send(if cfg!(windows) {
        "[Console]::WriteLine(('OWNED_AFTER_' + 'INTERRUPT'))\r"
    } else {
        "printf 'OWNED_AFTER_%s\\n' INTERRUPT\n"
    });
    session.expect("OWNED_AFTER_INTERRUPT");
    assert!(
        session.child.try_wait().unwrap().is_none(),
        "Ctrl+C stopped the picker parent"
    );
    assert!(
        !session.parser.screen().alternate_screen(),
        "Ctrl+C returned to the picker before the local shell exited"
    );
    session.send(if cfg!(windows) { "exit\r" } else { "exit\n" });
    session.expect("Local shell ended (exit 0)");
    assert!(
        session.child.try_wait().unwrap().is_none(),
        "Picker exited instead of returning"
    );
    session.send("1\r");
    session.expect("OWNED_REMOTE_READY");
    let remote_screen = session.parser.screen().contents();
    assert!(
        !remote_screen.contains("OWNED_LOCAL_HISTORY"),
        "Previous local output leaked into SSH screen: {remote_screen}"
    );
    assert!(
        !remote_screen.contains("PS "),
        "Old PowerShell prompt leaked into SSH screen: {remote_screen}"
    );
    session.send(if cfg!(windows) { "exit\r" } else { "exit\n" });
    session.expect("SSH session ended (exit 0)");
    assert!(
        session.child.try_wait().unwrap().is_none(),
        "Picker exited after remote logout"
    );
    session.send("3\r");
    session.expect("OWNED_SSH_FAILURE_255");
    session.expect("press Enter");
    session.quiet();
    assert!(
        session
            .parser
            .screen()
            .contents()
            .contains("OWNED_SSH_FAILURE_255"),
        "SSH failure was cleared before acknowledgement"
    );
    session.send("\r");
    session.expect(if python.is_some() {
        "choose another route"
    } else {
        "Select another route"
    });
    session.send("\x1b");
    session.quiet();
    session.send("q");
    session.exit();
    let final_screen = session.parser.screen().contents();
    for old in [
        "OWNED_LOCAL_HISTORY",
        "OWNED_REMOTE_HISTORY",
        "OWNED_REMOTE_LOGOUT",
        "OWNED_SSH_FAILURE_255",
        "PS ",
    ] {
        assert!(
            !final_screen.contains(old),
            "Stale {old} after picker exit: {final_screen}"
        );
    }
    // vt100 0.16 models visible/alternate buffers but not ED3 scrollback purge.
    // Verify the actual terminal stream separately contains ED3 at each boundary.
    assert!(
        session.raw.windows(4).filter(|s| *s == b"\x1b[3J").count() >= 6,
        "Missing scrollback purge requests"
    );
    assert_eq!(std::fs::read(&history).unwrap(), b"history must survive\n");
}

#[test]
fn native_local_remote_return_clears_visible_history() {
    exercise(None);
}

#[test]
#[ignore = "Developer-only compatibility runtime: set SSH_SESSIONS_TEST_PYTHON"]
fn python_local_remote_return_clears_visible_history() {
    let python =
        std::env::var_os("SSH_SESSIONS_TEST_PYTHON").expect("Set Python with Textual installed");
    exercise(Some(Path::new(&python)));
}
