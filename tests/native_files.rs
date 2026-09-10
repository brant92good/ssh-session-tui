//! Real terminal handoff to an owned no-network companion, using literal argv.
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use ssh_sessions::{
    catalog::{Catalog, Machine, Route, write_json},
    favorites::Favorites,
};
use std::{
    fs,
    io::{Read, Write},
    process::Command,
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
    raw_tail: Vec<u8>,
}
impl Session {
    fn send(&mut self, value: &str) {
        self.input.write_all(value.as_bytes()).unwrap();
        self.input.flush().unwrap();
    }
    fn pump(&mut self) {
        if let Ok(bytes) = self.output.recv_timeout(Duration::from_millis(40)) {
            self.parser.process(&bytes);
            self.raw_tail.extend(bytes);
            let queries = self
                .raw_tail
                .windows(4)
                .filter(|s| *s == b"\x1b[6n")
                .count();
            let keep = self.raw_tail.len().saturating_sub(3);
            self.raw_tail.drain(..keep);
            for _ in 0..queries {
                self.send("\x1b[1;1R");
            }
        }
    }
    fn expect(&mut self, text: &str) {
        let end = Instant::now() + Duration::from_secs(15);
        while !self.parser.screen().contents().contains(text) {
            assert!(
                Instant::now() < end,
                "Missing {text}: {}",
                self.parser.screen().contents()
            );
            self.pump();
        }
    }
    fn exit(&mut self) {
        let end = Instant::now() + Duration::from_secs(8);
        loop {
            self.pump();
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            assert!(Instant::now() < end, "Picker did not quit");
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

#[test]
fn files_json_is_read_only_and_tui_handoff_preserves_the_frozen_route() {
    let temp = tempfile::tempdir().unwrap();
    let local = temp.path().join("local 空白 'quoted'");
    fs::create_dir(&local).unwrap();
    let source = temp.path().join("companion.rs");
    fs::write(
        &source,
        r#"
use std::io::{self, Write};
fn main() {
    let log = std::env::var_os("FILES_FIXTURE_LOG").unwrap();
    std::fs::write(log, std::env::args().collect::<Vec<_>>().join("\0")).unwrap();
    println!("OWNED_FILES_READY");
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    println!("OWNED_FILES_ERROR_255");
    std::process::exit(255);
}
"#,
    )
    .unwrap();
    let stub = temp.path().join(if cfg!(windows) {
        "ssh-files.exe"
    } else {
        "ssh-files"
    });
    assert!(
        Command::new("rustc")
            .arg(&source)
            .arg("-o")
            .arg(&stub)
            .status()
            .unwrap()
            .success()
    );
    let log = temp.path().join("literal-argv");
    let config = temp.path().join("config 開發 'quoted'");
    fs::write(
        &config,
        b"Host fixture-alias\n HostName stale.example.test\n User different\n",
    )
    .unwrap();
    let catalog = Catalog::new(
        &temp.path().join("catalog.json"),
        &temp.path().join("device"),
    )
    .unwrap();
    let lan = Route {
        id: "lan".into(),
        name: "LAN".into(),
        host: "192.0.2.18".into(),
        port: 2222,
        ssh_alias: Some("fixture-alias".into()),
    };
    let vpn = Route {
        id: "vpn".into(),
        name: "VPN".into(),
        host: "192.0.2.19".into(),
        port: 22,
        ssh_alias: None,
    };
    let machine = Machine {
        id: "lab".into(),
        name: "- Lab 開發".into(),
        user: "dev".into(),
        routes: vec![lan, vpn],
        group: String::new(),
        tags: vec![],
    };
    catalog
        .save(
            std::slice::from_ref(&machine),
            &catalog.load().unwrap().revision,
        )
        .unwrap();
    write_json(
        &catalog.device_path.with_extension("ssh-configs.json"),
        &std::collections::BTreeMap::from([("lab/lan", config.to_str().unwrap())]),
    )
    .unwrap();
    let binary = env!("CARGO_BIN_EXE_ssh-sessions");
    let command = |override_path: &std::path::Path| {
        let mut command = Command::new(binary);
        command
            .arg("--catalog")
            .arg(&catalog.path)
            .arg("--state-dir")
            .arg(&catalog.state_dir)
            .args(["files", "lab", "--route", "lan", "--json"])
            .env("SSH_FILES_BIN", override_path)
            .env("FILES_FIXTURE_LOG", &log)
            .current_dir(&local);
        command
    };
    let before = fs::read(&catalog.path).unwrap();
    let result = command(&stub).output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(json["route_id"], "lan");
    assert!(!log.exists(), "Read-only lookup executed the companion");
    assert!(
        !catalog.device_path.exists(),
        "Read-only lookup wrote a route preference"
    );
    assert_eq!(before, fs::read(&catalog.path).unwrap());
    let invalid = command(&temp.path().join("missing")).output().unwrap();
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stdout).contains("SSH_FILES_BIN"));
    assert!(!log.exists());
    catalog.choose(&machine, "lan").unwrap();
    Favorites::assign(&catalog, "1", Some("lab"), None).unwrap();
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 30,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut launch = CommandBuilder::new(binary);
    launch.arg("--catalog");
    launch.arg(&catalog.path);
    launch.arg("--state-dir");
    launch.arg(&catalog.state_dir);
    launch.env("SSH_FILES_BIN", &stub);
    launch.env("FILES_FIXTURE_LOG", &log);
    launch.env("TERM", "xterm-256color");
    launch.cwd(&local);
    let child = pair.slave.spawn_command(launch).unwrap();
    let input = pair.master.take_writer().unwrap();
    let mut reader = pair.master.try_clone_reader().unwrap();
    let (sender, output) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = [0; 8192];
        while let Ok(count) = reader.read(&mut bytes) {
            if count == 0 || sender.send(bytes[..count].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut session = Session {
        child,
        _pair: pair,
        input,
        output,
        parser: vt100::Parser::new(30, 120, 0),
        raw_tail: vec![],
    };
    session.expect("Local terminal");
    session.send("1x");
    session.expect("OWNED_FILES_READY");
    assert!(
        !session.parser.screen().alternate_screen(),
        "Picker did not release the terminal"
    );
    let actual: Vec<_> = fs::read_to_string(&log)
        .unwrap()
        .split('\0')
        .map(str::to_owned)
        .collect();
    let expected: Vec<_> = json["argv"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        actual, expected,
        "Literal paths or selected route changed during handoff"
    );
    catalog.choose(&machine, "vpn").unwrap();
    assert_eq!(
        actual,
        fs::read_to_string(&log)
            .unwrap()
            .split('\0')
            .collect::<Vec<_>>()
    );
    session.send("done\r");
    session.expect("press Enter to return");
    assert!(
        !session.parser.screen().alternate_screen(),
        "Failure detail cleared before acknowledgement"
    );
    session.send("\r");
    session.expect("try X again");
    assert!(session.parser.screen().alternate_screen());
    assert!(session.parser.screen().contents().contains("X files"));
    assert_eq!(
        catalog.preferences().unwrap()["lab"],
        "vpn",
        "Files failure rewrote the device preference"
    );
    assert!(session.child.try_wait().unwrap().is_none());
    session.send("q");
    session.exit();
}
