//! Uses only temporary local repositories. Run outside the owner's sandbox.
use ssh_sessions::{
    catalog::{Catalog, Machine, Route, encode},
    sync::GitSync,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
fn git(root: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    remote: PathBuf,
    catalog: Catalog,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("device A");
        let remote = temp.path().join("remote.git");
        fs::create_dir_all(&remote).unwrap();
        git(&remote, &["init", "--bare", "--initial-branch=main"]);
        git(
            temp.path(),
            &["clone", remote.to_str().unwrap(), root.to_str().unwrap()],
        );
        for (key, value) in [
            ("user.name", "Fixture"),
            ("user.email", "fixture@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.autocrlf", "false"),
        ] {
            git(&root, &["config", key, value]);
        }
        let machine = Machine {
            id: "demo".into(),
            name: "Demo".into(),
            user: "dev".into(),
            group: String::new(),
            tags: vec![],
            routes: vec![Route {
                id: "lan".into(),
                name: "LAN".into(),
                host: "192.0.2.10".into(),
                port: 22,
                ssh_alias: None,
            }],
        };
        fs::write(root.join("catalog.json"), encode(&[machine]).unwrap()).unwrap();
        fs::write(root.join("README.md"), "Fixture").unwrap();
        git(&root, &["add", "catalog.json", "README.md"]);
        git(&root, &["commit", "-m", "Fixture"]);
        git(&root, &["push", "-u", "origin", "main"]);
        let catalog =
            Catalog::new(&root.join("catalog.json"), &temp.path().join("device")).unwrap();
        Self {
            _temp: temp,
            root,
            remote,
            catalog,
        }
    }
}
#[test]
fn publish_preserves_unrelated_index_and_does_not_publish_local_state() {
    let fixture = Fixture::new();
    let mut snapshot = fixture.catalog.load().unwrap();
    snapshot.machines[0].name = "New name".into();
    fixture
        .catalog
        .save(&snapshot.machines, &snapshot.revision)
        .unwrap();
    fixture
        .catalog
        .choose(&snapshot.machines[0], "lan")
        .unwrap();
    fs::write(fixture.root.join("unrelated.txt"), "Leave staged").unwrap();
    git(&fixture.root, &["add", "unrelated.txt"]);
    GitSync::new(&fixture.catalog)
        .unwrap()
        .run("publish")
        .unwrap();
    assert!(git(&fixture.root, &["status", "--porcelain"]).contains("A  unrelated.txt"));
    assert!(git(&fixture.remote, &["show", "main:catalog.json"]).contains("New name"));
    assert!(!git(&fixture.remote, &["ls-tree", "-r", "--name-only", "main"]).contains("device"));
}
#[test]
fn reverted_unrelated_history_and_repaired_invalid_catalog_are_refused() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("README.md"), "Private draft").unwrap();
    git(&fixture.root, &["commit", "-am", "Private draft"]);
    git(&fixture.root, &["revert", "--no-edit", "HEAD"]);
    assert!(
        GitSync::new(&fixture.catalog)
            .unwrap()
            .run("publish")
            .is_err()
    );
    assert_eq!(git(&fixture.remote, &["show", "main:README.md"]), "Fixture");
    let other = Fixture::new();
    let good = fs::read(&other.catalog.path).unwrap();
    fs::write(
        &other.catalog.path,
        r#"{"version":1,"machines":[],"password":"forbidden"}"#,
    )
    .unwrap();
    git(&other.root, &["commit", "-am", "Invalid draft"]);
    fs::write(&other.catalog.path, good).unwrap();
    git(&other.root, &["commit", "-am", "Repair"]);
    assert!(
        GitSync::new(&other.catalog)
            .unwrap()
            .run("publish")
            .is_err()
    );
    assert_eq!(
        git(&other.remote, &["log", "--format=%s", "-1", "main"]).trim(),
        "Fixture"
    );
}
#[test]
fn publication_checks_merge_changes_against_each_parent() {
    for unrelated in [true, false] {
        let fixture = Fixture::new();
        git(&fixture.root, &["checkout", "-b", "side"]);
        git(&fixture.root, &["commit", "--allow-empty", "-m", "Side"]);
        git(&fixture.root, &["checkout", "main"]);
        git(&fixture.root, &["commit", "--allow-empty", "-m", "Main"]);
        git(&fixture.root, &["merge", "--no-ff", "--no-commit", "side"]);
        if unrelated {
            fs::write(
                fixture.root.join("unrelated.txt"),
                "Merge-only private draft",
            )
            .unwrap();
            git(&fixture.root, &["add", "unrelated.txt"]);
        } else {
            let mut snapshot = fixture.catalog.load().unwrap();
            snapshot.machines[0].name = "Merged catalog".into();
            fixture
                .catalog
                .save(&snapshot.machines, &snapshot.revision)
                .unwrap();
            git(&fixture.root, &["add", "catalog.json"]);
        }
        git(&fixture.root, &["commit", "-m", "Merge result"]);
        let result = GitSync::new(&fixture.catalog).unwrap().run("publish");
        if unrelated {
            assert!(
                result.is_err(),
                "Merge-only non-catalog file must be refused"
            );
            assert_eq!(
                git(&fixture.remote, &["log", "--format=%s", "-1", "main"]).trim(),
                "Fixture"
            );
            assert!(
                !git(&fixture.remote, &["ls-tree", "-r", "--name-only", "main"])
                    .contains("unrelated.txt")
            );
        } else {
            result.unwrap();
            assert!(
                git(&fixture.remote, &["show", "main:catalog.json"]).contains("Merged catalog")
            );
        }
    }
}
#[test]
fn pull_refuses_dirty_work_and_accepts_clean_fast_forward() {
    let fixture = Fixture::new();
    let other = fixture._temp.path().join("device B");
    git(
        fixture._temp.path(),
        &[
            "clone",
            fixture.remote.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    git(&other, &["config", "user.name", "Fixture"]);
    git(&other, &["config", "user.email", "fixture@example.invalid"]);
    git(&other, &["config", "commit.gpgsign", "false"]);
    let catalog_b = Catalog::new(
        &other.join("catalog.json"),
        &fixture._temp.path().join("device-b"),
    )
    .unwrap();
    let mut snapshot = catalog_b.load().unwrap();
    snapshot.machines[0].group = "Work/Lab".into();
    catalog_b
        .save(&snapshot.machines, &snapshot.revision)
        .unwrap();
    git(&other, &["commit", "-am", "Group on B"]);
    git(&other, &["push"]);
    fs::write(fixture.root.join("draft.txt"), "Local work").unwrap();
    assert!(GitSync::new(&fixture.catalog).unwrap().run("pull").is_err());
    fs::remove_file(fixture.root.join("draft.txt")).unwrap();
    GitSync::new(&fixture.catalog).unwrap().run("pull").unwrap();
    assert_eq!(
        fixture.catalog.load().unwrap().machines[0].group,
        "Work/Lab"
    );
}
