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
    fn start(command: CommandBuilder, rows: u16, cols: u16) -> Self {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let child = pair.slave.spawn_command(command).unwrap();
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
        Self {
            child,
            _pair: pair,
            input,
            output,
            parser: vt100::Parser::new(rows, cols, 0),
            raw_tail: vec![],
        }
    }
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

#[test]
#[ignore = "Requires explicit disposable local SFTP fixture and compiled Files companion"]
fn actual_files_chooser_opens_saved_remote_path_and_returns_to_the_tree() {
    use ssh_sessions::file_presets::{FilePresets, Preset};
    let config = std::env::var_os("SSH_FILES_TEST_CONFIG").expect("SSH_FILES_TEST_CONFIG");
    let server = std::path::PathBuf::from(
        std::env::var_os("SSH_FILES_TEST_REMOTE").expect("SSH_FILES_TEST_REMOTE"),
    );
    let port: u16 = std::env::var("SSH_FILES_TEST_PORT")
        .expect("SSH_FILES_TEST_PORT")
        .parse()
        .unwrap();
    let user = std::env::var("SSH_FILES_TEST_USER").expect("SSH_FILES_TEST_USER");
    let companion = std::env::var_os("SSH_FILES_TEST_BINARY").expect("SSH_FILES_TEST_BINARY");
    let remote = tempfile::Builder::new()
        .prefix("chooser-owned-")
        .tempdir_in(server)
        .unwrap();
    fs::write(
        remote.path().join("remote-preset-reached.txt"),
        b"read-only navigation marker",
    )
    .unwrap();
    let local = tempfile::tempdir().unwrap();
    let catalog = Catalog::new(
        &local.path().join("catalog.json"),
        &local.path().join("device"),
    )
    .unwrap();
    let machine = Machine {
        id: "owned".into(),
        name: "Owned test server".into(),
        user,
        group: "Fixtures".into(),
        tags: vec![],
        routes: vec![Route {
            id: "loopback".into(),
            name: "Loopback".into(),
            host: "127.0.0.1".into(),
            port,
            ssh_alias: Some("fixture".into()),
        }],
    };
    catalog
        .save(&[machine], &catalog.load().unwrap().revision)
        .unwrap();
    write_json(
        &catalog.device_path.with_extension("ssh-configs.json"),
        &std::collections::BTreeMap::from([(
            "owned/loopback",
            std::path::Path::new(&config).to_str().unwrap(),
        )]),
    )
    .unwrap();
    let (mut presets, revision) = FilePresets::load(&catalog).unwrap();
    presets
        .upsert(
            &catalog,
            "owned",
            Preset {
                id: "project".into(),
                name: "Saved project".into(),
                path: remote.path().to_str().unwrap().replace('\\', "/"),
            },
            &catalog.load().unwrap().revision,
            &revision,
        )
        .unwrap();
    let original = fs::read(&catalog.path).unwrap();
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_ssh-sessions"));
    command.arg("--catalog");
    command.arg(&catalog.path);
    command.arg("--state-dir");
    command.arg(&catalog.state_dir);
    command.arg("files");
    command.env("SSH_FILES_BIN", companion);
    command.env("TERM", "xterm-256color");
    command.cwd(local.path());
    let mut session = Session::start(command, 30, 140);
    session.expect("Choose a server");
    session.send("\x1b[B\x1b[C");
    session.expect("Saved project");
    session.send("\x1b[B\r");
    session.expect("remote-preset-reached.txt");
    assert!(
        !session
            .parser
            .screen()
            .contents()
            .contains("Choose a server")
    );
    session.send("\x1b[21~");
    session.expect("Files closed. Choose another server or path.");
    assert!(session.parser.screen().contents().contains("Saved project"));
    assert_eq!(original, fs::read(&catalog.path).unwrap());
    assert!(
        !catalog.device_path.exists(),
        "Single-route Files launch created a device preference"
    );
    session.send("\x1b");
    session.exit();
}
impl Drop for Session {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
        }
    }
}

#[test]
fn files_chooser_waits_for_explicit_selection_preserves_paths_and_errors() {
    use ssh_sessions::file_presets::{FilePresets, Preset};
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("chooser-companion.rs");
    fs::write(&source, r#"
use std::io::{self, Write};
fn main() {
    std::fs::write(std::env::var_os("FILES_FIXTURE_LOG").unwrap(), std::env::args().collect::<Vec<_>>().join("\0")).unwrap();
    println!("CHOOSER_OWNED_FILES_READY");
    io::stdout().flush().unwrap();
    let mut input = String::new(); io::stdin().read_line(&mut input).unwrap();
    println!("OWNED_AUTH_ERROR_RETAINED");
    std::process::exit(255);
}
"#).unwrap();
    let stub = temp.path().join(if cfg!(windows) {
        "ssh-files.exe"
    } else {
        "ssh-files"
    });
    assert!(
        Command::new("rustc")
            .arg(&source)
            .arg("--crate-name")
            .arg("chooser_companion")
            .arg("-o")
            .arg(&stub)
            .status()
            .unwrap()
            .success()
    );
    let catalog = Catalog::new(
        &temp.path().join("catalog.json"),
        &temp.path().join("device"),
    )
    .unwrap();
    let machine = Machine {
        id: "lab".into(),
        name: "Build box".into(),
        user: "dev".into(),
        group: "Work/Lab".into(),
        tags: vec![],
        routes: (0..25)
            .map(|i| Route {
                id: format!("route-{i:02}"),
                name: format!("Route {i:02}"),
                host: format!("192.0.2.{}", i + 1),
                port: 22,
                ssh_alias: None,
            })
            .collect(),
    };
    catalog
        .save(
            std::slice::from_ref(&machine),
            &catalog.load().unwrap().revision,
        )
        .unwrap();
    catalog.choose(&machine, "route-00").unwrap();
    let original_catalog = fs::read(&catalog.path).unwrap();
    let original_device = fs::read(&catalog.device_path).unwrap();
    let log = temp.path().join("argv");
    let binary = env!("CARGO_BIN_EXE_ssh-sessions");
    let invalid = Command::new(binary)
        .args(["--catalog"])
        .arg(&catalog.path)
        .arg("--state-dir")
        .arg(&catalog.state_dir)
        .args(["files", "--json"])
        .env("SSH_FILES_BIN", &stub)
        .env("FILES_FIXTURE_LOG", &log)
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(!log.exists());
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 22,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut launch = CommandBuilder::new(binary);
    launch.arg("--catalog");
    launch.arg(&catalog.path);
    launch.arg("--state-dir");
    launch.arg(&catalog.state_dir);
    launch.arg("files");
    launch.env("SSH_FILES_BIN", &stub);
    launch.env("FILES_FIXTURE_LOG", &log);
    launch.env("TERM", "xterm-256color");
    launch.cwd(temp.path());
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
        parser: vt100::Parser::new(22, 100, 0),
        raw_tail: vec![],
    };
    session.expect("Choose a server");
    session.expect("Work/Lab");
    assert!(!log.exists(), "Entering Files connected without a choice");
    session.send("\x1b[Ba");
    session.expect("Saved remote path");
    session.send("Project\t\x15/srv/code ; $HOME/開發\r\r\r");
    session.expect("Saved. Press Esc");
    for _ in 0..4 {
        session.pump();
    }
    assert!(
        !log.exists(),
        "Trailing editor newlines launched a companion"
    );
    let (presets, _) = FilePresets::load(&catalog).unwrap();
    assert_eq!(presets.for_machine("lab")[0].path, "/srv/code ; $HOME/開發");
    session.send("\x1b");
    session.expect("Project");
    session.send("r");
    session.expect("Use route once");
    session.send(&"\x1b[B".repeat(24));
    session.expect("Route 24");
    assert!(!log.exists(), "Moving the route cursor connected");
    session.send("\r");
    session.expect("CHOOSER_OWNED_FILES_READY");
    let argv: Vec<_> = fs::read_to_string(&log)
        .unwrap()
        .split('\0')
        .map(str::to_owned)
        .collect();
    assert!(argv.windows(2).any(|v| v == ["--route-id", "route-24"]));
    assert!(argv.windows(2).any(|v| v == ["--host", "192.0.2.25"]));
    assert!(argv.contains(&"--remote=/srv/code ; $HOME/開發".to_string()));
    assert_eq!(original_device, fs::read(&catalog.device_path).unwrap());
    assert!(!session.parser.screen().alternate_screen());
    session.send("done\r");
    session.expect("press Enter to return");
    assert!(
        session
            .parser
            .screen()
            .contents()
            .contains("OWNED_AUTH_ERROR_RETAINED")
    );
    assert!(!session.parser.screen().alternate_screen());
    session.send("\r");
    session.expect("no fallback was attempted");
    assert!(session.parser.screen().alternate_screen());
    session.send("e");
    session.expect("Saved remote path");
    let (mut concurrent, revision) = FilePresets::load(&catalog).unwrap();
    concurrent
        .upsert(
            &catalog,
            "lab",
            Preset {
                id: "other".into(),
                name: "Other tab".into(),
                path: "/other".into(),
            },
            &catalog.load().unwrap().revision,
            &revision,
        )
        .unwrap();
    session.send("\x15Uncommitted\r");
    session.expect("Saved paths changed in another tab");
    assert!(session.parser.screen().contents().contains("Uncommitted"));
    assert_eq!(
        FilePresets::load(&catalog).unwrap().0.for_machine("lab")[0].name,
        "Project"
    );
    assert_eq!(
        argv,
        fs::read_to_string(&log)
            .unwrap()
            .split('\0')
            .collect::<Vec<_>>()
    );
    assert_eq!(original_catalog, fs::read(&catalog.path).unwrap());
    assert_eq!(original_device, fs::read(&catalog.device_path).unwrap());
    session.send("\x1b");
    session.expect("Choose a server");
    session.send("\x1b");
    session.exit();
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
