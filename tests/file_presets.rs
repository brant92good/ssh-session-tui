use ssh_sessions::{
    catalog::{Catalog, Machine, Route, encode},
    file_presets::{self, FilePresets, Preset},
};
use std::fs;

fn fixture() -> (tempfile::TempDir, Catalog) {
    let temp = tempfile::tempdir().unwrap();
    let machine = |id: &str| Machine {
        id: id.into(),
        name: id.into(),
        user: "dev".into(),
        group: String::new(),
        tags: vec![],
        routes: vec![Route {
            id: "lan".into(),
            name: "LAN".into(),
            host: "example.invalid".into(),
            port: 22,
            ssh_alias: None,
        }],
    };
    let path = temp.path().join("catalog.special.json");
    fs::write(&path, encode(&[machine("one"), machine("two")]).unwrap()).unwrap();
    let catalog = Catalog::new(&path, &temp.path().join("state")).unwrap();
    (temp, catalog)
}
fn preset(id: &str, name: &str, path: &str) -> Preset {
    Preset {
        id: id.into(),
        name: name.into(),
        path: path.into(),
    }
}

#[test]
fn missing_reads_are_inert_and_roundtrip_preserves_literal_paths_and_other_machines() {
    let (_temp, catalog) = fixture();
    let original = fs::read(&catalog.path).unwrap();
    let snapshot = catalog.load().unwrap();
    let (mut paths, missing) = FilePresets::load(&catalog).unwrap();
    assert!(!file_presets::sidecar_path(&catalog).exists());
    assert!(!catalog.lock_path.exists());
    file_presets::validate_snapshot(&catalog, &snapshot.revision, &missing).unwrap();
    assert!(!catalog.lock_path.exists());
    assert_eq!(
        file_presets::sidecar_path(&catalog).file_name().unwrap(),
        "catalog.special.json.files.json"
    );
    let revision = paths
        .upsert(
            &catalog,
            "one",
            preset("first", "Home", " ~/../$HOME/開發 "),
            &snapshot.revision,
            &missing,
        )
        .unwrap();
    let revision = paths
        .upsert(
            &catalog,
            "two",
            preset("second", "Logs", "-literal"),
            &snapshot.revision,
            &revision,
        )
        .unwrap();
    let (loaded, loaded_revision) = FilePresets::load(&catalog).unwrap();
    assert_eq!(loaded_revision, revision);
    assert_eq!(
        loaded.get("one", "first").unwrap().path,
        " ~/../$HOME/開發 "
    );
    assert_eq!(fs::read(&catalog.path).unwrap(), original);
    paths
        .delete(&catalog, "one", "first", &snapshot.revision, &revision)
        .unwrap();
    assert!(paths.for_machine("one").is_empty());
    assert_eq!(paths.get("two", "second").unwrap().path, "-literal");
}

#[test]
fn stale_catalog_or_sidecar_never_overwrites_and_orphan_records_survive() {
    let (_temp, catalog) = fixture();
    let snapshot = catalog.load().unwrap();
    let (mut paths, missing) = FilePresets::load(&catalog).unwrap();
    let revision = paths
        .upsert(
            &catalog,
            "one",
            preset("first", "Home", "."),
            &snapshot.revision,
            &missing,
        )
        .unwrap();
    let before = fs::read(file_presets::sidecar_path(&catalog)).unwrap();
    assert!(
        paths
            .upsert(
                &catalog,
                "one",
                preset("first", "Changed", "/other"),
                &snapshot.revision,
                &missing
            )
            .is_err()
    );
    assert_eq!(
        fs::read(file_presets::sidecar_path(&catalog)).unwrap(),
        before
    );
    catalog
        .save(&snapshot.machines[1..], &snapshot.revision)
        .unwrap();
    assert!(
        paths
            .delete(&catalog, "one", "first", &snapshot.revision, &revision)
            .is_err()
    );
    assert!(file_presets::validate_snapshot(&catalog, &snapshot.revision, &revision).is_err());
    let current = catalog.load().unwrap();
    assert!(
        paths
            .delete(&catalog, "one", "first", &current.revision, &revision)
            .is_err()
    );
    paths
        .upsert(
            &catalog,
            "two",
            preset("second", "Other", "."),
            &current.revision,
            &revision,
        )
        .unwrap();
    assert!(
        paths.get("one", "first").is_some(),
        "Deleted machine's metadata must remain recoverable."
    );
}

#[test]
fn schema_limits_duplicates_and_malformed_files_fail_without_repair_writes() {
    let (_temp, catalog) = fixture();
    for raw in [
        r#"{"version":2,"machines":{}}"#,
        r#"{"version":1,"machines":{},"password":"no"}"#,
        r#"{"version":1,"machines":{"one":[],"one":[]}}"#,
        r#"{"version":1,"machines":{"one":[{"id":"x","name":"One","path":"."},{"id":"x","name":"Two","path":"."}]}}"#,
        r#"{"version":1,"machines":{"one":[{"id":"x","name":"Straße","path":"."},{"id":"y","name":"STRASSE","path":"."}]}}"#,
        r#"{"version":1,"machines":{"one":[{"id":"x","name":"One","path":"\u001b[2J"}]}}"#,
    ] {
        fs::write(file_presets::sidecar_path(&catalog), raw).unwrap();
        assert!(FilePresets::load(&catalog).is_err(), "{raw}");
        assert_eq!(
            fs::read(file_presets::sidecar_path(&catalog)).unwrap(),
            raw.as_bytes()
        );
    }
    assert!(preset("x", "Valid", &"x".repeat(4097)).validate().is_err());
    assert!(preset("x", "Valid", &"開".repeat(1366)).validate().is_err());
    assert!(preset("x", "Bad\n", ".").validate().is_err());
    let many: Vec<_> = (0..65)
        .map(|i| preset(&format!("p{i}"), &format!("Path {i}"), "."))
        .collect();
    assert!(
        FilePresets::decode(
            &serde_json::to_vec(&serde_json::json!({"version":1,"machines":{"one":many}})).unwrap()
        )
        .is_err()
    );
    let machines: std::collections::BTreeMap<_, _> = (0..33)
        .map(|machine| {
            (
                format!("m{machine}"),
                (0..if machine == 32 { 1 } else { 64 })
                    .map(|i| preset(&format!("p{i}"), &format!("N{i}"), "."))
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let oversized_count =
        serde_json::to_vec(&serde_json::json!({"version":1,"machines":machines})).unwrap();
    assert!(
        oversized_count.len() < file_presets::MAX_BYTES,
        "Entry-count regression must not be merely the file-size check."
    );
    assert!(FilePresets::decode(&oversized_count).is_err());
    fs::write(
        file_presets::sidecar_path(&catalog),
        vec![b' '; file_presets::MAX_BYTES + 1],
    )
    .unwrap();
    assert!(FilePresets::load(&catalog).is_err());
    assert_eq!(
        fs::metadata(file_presets::sidecar_path(&catalog))
            .unwrap()
            .len(),
        (file_presets::MAX_BYTES + 1) as u64
    );
}

#[cfg(unix)]
#[test]
fn symlink_sidecar_is_not_followed_or_replaced() {
    use std::os::unix::fs::symlink;
    let (temp, catalog) = fixture();
    let target = temp.path().join("outside.json");
    fs::write(&target, b"private").unwrap();
    symlink(&target, file_presets::sidecar_path(&catalog)).unwrap();
    assert!(FilePresets::load(&catalog).is_err());
    assert_eq!(fs::read(target).unwrap(), b"private");
}
