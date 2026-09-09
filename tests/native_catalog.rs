use serde_json::json;
use ssh_sessions::{
    catalog::*,
    favorites::{Favorites, LOCAL},
    organization, ssh_import,
};
use std::{fs, path::Path};

fn sample() -> Machine {
    Machine {
        id: "workbox".into(),
        name: "Work box 開發".into(),
        user: "developer".into(),
        group: String::new(),
        tags: Vec::new(),
        routes: vec![
            Route {
                id: "lan".into(),
                name: "LAN".into(),
                host: "192.0.2.10".into(),
                port: 22,
                ssh_alias: None,
            },
            Route {
                id: "vpn".into(),
                name: "VPN".into(),
                host: "workbox.example.test".into(),
                port: 2222,
                ssh_alias: None,
            },
        ],
    }
}
fn catalog(root: &Path) -> Catalog {
    Catalog::new(&root.join("shared/catalog.json"), &root.join("device")).unwrap()
}
#[test]
fn empty_load_and_unicode_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = catalog(temp.path());
    assert!(catalog.load().unwrap().machines.is_empty());
    assert!(!catalog.path.exists());
    assert!(!catalog.state_dir.exists());
    let machine = sample();
    let raw = encode(std::slice::from_ref(&machine)).unwrap();
    assert_eq!(decode(&raw).unwrap(), [machine]);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&raw).unwrap()["version"],
        1
    );
}
#[test]
fn legacy_numeric_string_ports_and_full_casefold_matching() {
    let mut value = json!({"version":1,"machines":[sample()]});
    value["machines"][0]["routes"][0]["port"] = json!("00022");
    assert_eq!(
        decode(&serde_json::to_vec(&value).unwrap()).unwrap()[0].routes[0].port,
        22
    );
    let mut machine = sample();
    machine.group = "Stra\u{df}e/Lab".into();
    machine.tags = vec!["\u{fb03}".into()];
    assert!(organization::matches(&machine, "group:STRASSE tag:ffi"));
    assert_eq!(
        tags(vec!["Stra\u{df}e".into(), "STRASSE".into()])
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn nonexistent_parent_navigation_and_scoped_ipv6_preserve_legacy_inputs() {
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(
        absolute(&temp.path().join("new/missing/../catalog.json")).unwrap(),
        absolute(temp.path()).unwrap().join("new/catalog.json")
    );
    assert_eq!(address("fe80::1%ethernet").unwrap(), "fe80::1%ethernet");
    assert!(address("fe80::1%").is_err());
    assert!(address("fe80::1%a%b").is_err());
    assert!(address(&"a".repeat(101)).is_err());
}
#[test]
fn reject_bad_fields_types_duplicates_and_options() {
    let valid = serde_json::to_value(json!({"version":1,"machines":[sample()]})).unwrap();
    for level in 0..3 {
        let mut value = valid.clone();
        let target = match level {
            0 => &mut value,
            1 => &mut value["machines"][0],
            _ => &mut value["machines"][0]["routes"][0],
        };
        target["private_key"] = json!("forbidden");
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    for port in [
        json!(0),
        json!(65536),
        json!(true),
        json!(-1),
        json!(22.0),
        json!("２２"),
    ] {
        let mut value = valid.clone();
        value["machines"][0]["routes"][0]["port"] = port;
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    assert!(encode(&[sample(), sample()]).is_err());
    let mut machine = sample();
    machine.routes[1].id = "lan".into();
    assert!(encode(&[machine]).is_err());
    for bad in [
        "-oProxyCommand=bad",
        "ssh://host",
        "user@host",
        "host;touch",
        "host\nother",
        "host name",
        "/tmp/socket",
        "",
    ] {
        assert!(address(bad).is_err(), "{bad}");
    }
    for good in [
        "::1",
        "2001:db8::1",
        "192.0.2.1",
        "existing-alias",
        "_alias",
    ] {
        assert_eq!(address(good).unwrap(), good);
    }
}
#[test]
fn stale_saves_and_invalid_existing_files_are_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = catalog(temp.path());
    let before = catalog
        .save(&[sample()], &catalog.load().unwrap().revision)
        .unwrap();
    let mut changed = sample();
    changed.name = "Another tab".into();
    catalog.save(&[changed], &before.revision).unwrap();
    let latest = fs::read(&catalog.path).unwrap();
    assert!(catalog.save(&[sample()], &before.revision).is_err());
    assert_eq!(fs::read(&catalog.path).unwrap(), latest);
    for raw in [b"".as_slice(), b"{bad", b"[]"] {
        fs::write(&catalog.path, raw).unwrap();
        assert!(catalog.load().is_err());
        assert!(catalog.save(&[], &digest(raw)).is_err());
        assert_eq!(fs::read(&catalog.path).unwrap(), raw);
    }
}
#[test]
fn per_device_routes_stable_favorites_and_stale_favorite_edits() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = catalog(temp.path());
    let machine = sample();
    let saved = catalog
        .save(
            std::slice::from_ref(&machine),
            &catalog.load().unwrap().revision,
        )
        .unwrap();
    let shared = fs::read(&catalog.path).unwrap();
    assert!(
        catalog
            .preferred(&machine, &catalog.preferences().unwrap())
            .is_none()
    );
    catalog.choose(&machine, "lan").unwrap();
    assert_eq!(
        catalog
            .preferred(&machine, &catalog.preferences().unwrap())
            .unwrap()
            .id,
        "lan"
    );
    let other = Catalog::new(&catalog.path, &temp.path().join("device-b")).unwrap();
    assert!(
        other
            .preferred(&machine, &other.preferences().unwrap())
            .is_none()
    );
    other.choose(&machine, "vpn").unwrap();
    assert_eq!(fs::read(&catalog.path).unwrap(), shared);
    Favorites::assign(&catalog, "1", Some(&machine.id), None).unwrap();
    Favorites::assign(&catalog, "2", Some(LOCAL), None).unwrap();
    let revision = Favorites::load(&catalog).unwrap().1;
    Favorites::assign(&catalog, "3", Some(LOCAL), None).unwrap();
    assert!(Favorites::assign(&catalog, "4", Some(LOCAL), Some(&revision)).is_err());
    assert!(!Favorites::load(&catalog).unwrap().0.slots.contains_key("2"));
    catalog.save(&[], &saved.revision).unwrap();
    assert_eq!(Favorites::load(&catalog).unwrap().0.slots["1"], "workbox");
}
#[test]
fn grouping_and_bulk_changes_preserve_ids_routes_and_favorites() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = catalog(temp.path());
    let snapshot = catalog
        .save(&[sample()], &catalog.load().unwrap().revision)
        .unwrap();
    Favorites::assign(&catalog, "1", Some("workbox"), None).unwrap();
    let snapshot = organization::edit_many(
        &catalog,
        &["workbox".into()],
        &snapshot.revision,
        Some("Work/Lab"),
        &["GPU".into()],
        &[],
    )
    .unwrap();
    assert_eq!(
        organization::groups(&snapshot.machines).get("Work"),
        Some(&1)
    );
    assert!(organization::matches(
        &snapshot.machines[0],
        "group:work tag:gpu"
    ));
    let changed =
        organization::rename_group(&catalog, "work", "Projects", &snapshot.revision).unwrap();
    assert_eq!(changed.machines[0].group, "Projects/Lab");
    assert_eq!(changed.machines[0].routes, sample().routes);
    assert_eq!(Favorites::load(&catalog).unwrap().0.slots["1"], "workbox");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(&catalog.path).unwrap()).unwrap()["version"],
        2
    );
    let mut invalid = json!({"version":1,"machines":[sample()]});
    invalid["machines"][0]["group"] = json!("");
    assert!(decode(&serde_json::to_vec(&invalid).unwrap()).is_err());
}
#[test]
fn import_preserves_aliases_local_bindings_and_existing_route_choice() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = catalog(temp.path());
    let mut machine = sample();
    machine.routes.truncate(1);
    catalog
        .save(&[machine], &catalog.load().unwrap().revision)
        .unwrap();
    let config = temp.path().join("custom config");
    fs::write(&config,"Host web\n HostName = 192.0.2.10\n User developer\n IdentityFile sensitive-path\n ProxyCommand never-run %h\n").unwrap();
    let scan = ssh_import::scan(Some(&config)).unwrap();
    let imported = ssh_import::import(
        &catalog,
        &scan,
        &["web".into()],
        &catalog.load().unwrap().revision,
        "",
    )
    .unwrap();
    assert_eq!((imported.added_machines, imported.added_routes), (0, 1));
    let snapshot = catalog.load().unwrap();
    let machine = &snapshot.machines[0];
    assert_eq!(
        catalog
            .preferred(machine, &catalog.preferences().unwrap())
            .unwrap()
            .id,
        "lan"
    );
    let route = &machine.routes[1];
    assert_eq!(route.ssh_alias.as_deref(), Some("web"));
    assert_eq!(
        ssh_import::connection_config(&catalog, machine, route).unwrap(),
        Some(absolute(&config).unwrap())
    );
    let before = fs::read(&catalog.path).unwrap();
    let text = String::from_utf8(before.clone()).unwrap();
    assert!(
        !text.contains("sensitive-path")
            && !text.contains("ProxyCommand")
            && !text.contains("custom config")
    );
    assert_eq!(
        ssh_import::import(&catalog, &scan, &["web".into()], &snapshot.revision, "")
            .unwrap()
            .bound_existing,
        1
    );
    assert_eq!(fs::read(&catalog.path).unwrap(), before);
    fs::write(&config, "Host web\n HostName 192.0.2.99\n User developer\n").unwrap();
    assert!(ssh_import::import(&catalog, &scan, &["web".into()], &snapshot.revision, "").is_err());
    let fresh = ssh_import::scan(Some(&config)).unwrap();
    assert!(ssh_import::import(&catalog, &fresh, &["web".into()], &snapshot.revision, "").is_err());
    assert_eq!(fs::read(&catalog.path).unwrap(), before);
}
#[test]
fn static_import_include_scope_negation_and_conditional_refusal() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config");
    let included = temp.path().join("included file");
    fs::write(
        &included,
        "Host web second\n HostName 192.0.2.10\n User dev\n Port 2222\nHost other\n User other\n",
    )
    .unwrap();
    fs::write(
        &config,
        format!(
            "Include \"{}\"\nHost * !other\n User fallback\nHost web\n User too-late\n",
            included.display()
        ),
    )
    .unwrap();
    let scan = ssh_import::scan(Some(&config)).unwrap();
    let web = scan.entries.iter().find(|e| e.alias == "web").unwrap();
    assert_eq!(
        (&*web.host, &*web.user, web.port),
        ("192.0.2.10", "dev", 2222)
    );
    fs::write(
        &config,
        "Host web\n HostName 192.0.2.10\n User dev\nMatch exec \"never-run\"\n Port 2222\n",
    )
    .unwrap();
    assert!(
        ssh_import::scan(Some(&config)).unwrap().entries[0]
            .problem
            .contains("Conditional")
    );
    fs::write(&config, format!("Include \"{}\"", config.display())).unwrap();
    assert!(
        ssh_import::scan(Some(&config))
            .unwrap_err()
            .to_string()
            .contains("cycle")
    );
}
