//! Render the real chooser states; demonstration metadata lives only in a TempDir.
use super::*;
use crate::catalog::Route;
use std::{fs, path::Path};

pub fn capture(output: &Path) -> Result<()> {
    let temp = tempfile::tempdir()?;
    let catalog = Catalog::new(
        &temp.path().join("catalog.json"),
        &temp.path().join("device"),
    )?;
    let machines: Vec<_> = [
        ("training", "Training box", "Lab"),
        ("development", "Development", "Work"),
        ("staging", "Staging", "Work"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (id, name, group))| Machine {
        id: id.into(),
        name: name.into(),
        user: "dev".into(),
        group: group.into(),
        tags: vec![],
        routes: vec![
            Route {
                id: "lan".into(),
                name: "LAN".into(),
                host: format!("192.0.2.{}", index + 10),
                port: 22,
                ssh_alias: None,
            },
            Route {
                id: "vpn".into(),
                name: "Tailscale".into(),
                host: format!("{id}.example.test"),
                port: 22,
                ssh_alias: None,
            },
        ],
    })
    .collect();
    catalog.save(&machines, &catalog.load()?.revision)?;
    let (mut presets, mut revision) = FilePresets::load(&catalog)?;
    let catalog_revision = catalog.load()?.revision;
    for (machine, id, name, path) in [
        ("development", "api", "API source", "/srv/api"),
        ("development", "logs", "Service logs", "/var/log/api"),
        ("development", "releases", "Releases", "/srv/releases"),
        ("training", "datasets", "Datasets", "/srv/datasets"),
        ("staging", "deploy", "Deployment", "/srv/staging"),
    ] {
        revision = presets.upsert(
            &catalog,
            machine,
            Preset {
                id: id.into(),
                name: name.into(),
                path: path.into(),
            },
            &catalog_revision,
            &revision,
        )?;
    }
    let mut picker = Picker::new(catalog)?;
    picker.expanded.insert("development".into());
    picker.rebuild(true);
    picker.select_key(&RowKey::Path("development".into(), "api".into()));
    fs::create_dir_all(output)?;
    let save = |picker: &Picker, filename: &str, title: &str| {
        crate::ui::export(&output.join(filename), title, 100, 22, |frame| {
            picker.draw(frame)
        })
    };
    save(
        &picker,
        "files-picker.svg",
        "Files chooser with demonstration servers and saved remote paths",
    )?;
    picker.edit(true)?;
    save(
        &picker,
        "files-path.svg",
        "Files chooser editing the API source saved path",
    )?;
    picker.modal = Some(Modal::Routes {
        target: Target {
            machine: "development".into(),
            preset: Some("api".into()),
        },
        cursor: 1,
    });
    save(
        &picker,
        "files-route.svg",
        "Files chooser selecting a route for this session only",
    )?;
    Ok(())
}
