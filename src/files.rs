//! Optional companion handoff. Discovery never launches an executable or SSH.
use crate::{
    catalog::{Catalog, Machine, Route},
    ssh_import,
};
use anyhow::{Context, Result, ensure};
use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct Launch {
    pub argv: Vec<OsString>,
    pub title: String,
    pub machine_id: String,
    pub route_id: String,
}

fn resolve_with(
    explicit: Option<&OsStr>,
    current_executable: &Path,
    lookup: impl FnOnce(&str) -> Option<PathBuf>,
) -> Result<PathBuf> {
    if let Some(value) = explicit {
        let path = Path::new(value);
        ensure!(
            path.is_absolute() && path.is_file(),
            "SSH_FILES_BIN must name an existing absolute executable path."
        );
        return Ok(path.to_path_buf());
    }
    let name = if cfg!(windows) {
        "ssh-files.exe"
    } else {
        "ssh-files"
    };
    if let Some(parent) = current_executable.parent() {
        let bundled = parent.join(name);
        if bundled.is_file() {
            return Ok(bundled);
        }
    }
    lookup(name).context(
        "SSH Files is not installed. See https://github.com/brant92good/ssh-files#try-the-source-build, then press X again.",
    )
}

pub fn executable() -> Result<PathBuf> {
    resolve_with(
        std::env::var_os("SSH_FILES_BIN").as_deref(),
        &std::env::current_exe()?,
        |name| which::which(name).ok(),
    )
}

fn binding_bytes(catalog: &Catalog) -> Result<Option<Vec<u8>>> {
    let path = catalog.device_path.with_extension("ssh-configs.json");
    match fs::read(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn arguments(
    program: &Path,
    machine: &Machine,
    route: &Route,
    config: Option<&Path>,
    local: &Path,
) -> Vec<OsString> {
    let mut argv = vec![
        program.as_os_str().into(),
        "--host".into(),
        route.ssh_alias.as_ref().unwrap_or(&route.host).into(),
    ];
    if route.ssh_alias.is_some() {
        argv.extend(["--hostname".into(), route.host.clone().into()]);
    }
    if let Some(config) = config {
        argv.extend(["--config".into(), config.as_os_str().into()]);
    }
    argv.extend([
        "--user".into(),
        machine.user.clone().into(),
        "--port".into(),
        route.port.to_string().into(),
        // Equals keeps a label beginning with '-' a value in the companion CLI.
        format!("--label={} | {}", machine.name, route.name).into(),
        "--machine-id".into(),
        machine.id.clone().into(),
        "--route-id".into(),
        route.id.clone().into(),
        "--local".into(),
        local.as_os_str().into(),
    ]);
    argv
}

/// Freeze one validated selection before giving the terminal to the companion.
/// Optimistic checks retain the read-only command's no-lock-file/no-write contract.
pub fn prepare(
    catalog: &Catalog,
    revision: &str,
    machine: &Machine,
    route: &Route,
    program: &Path,
    cwd: &Path,
) -> Result<Launch> {
    ensure!(
        cwd.is_absolute() && cwd.is_dir(),
        "The local working directory is unavailable."
    );
    let current = catalog.load()?;
    ensure!(
        current.revision == revision
            && current.machines.iter().any(|m| m == machine)
            && machine.routes.contains(route),
        "Catalog changed. Reload and choose the machine and route again."
    );
    let bindings = binding_bytes(catalog)?;
    let config = ssh_import::connection_config(catalog, machine, route)?.map(|path| {
        if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        }
    });
    ensure!(
        binding_bytes(catalog)? == bindings,
        "SSH import bindings changed. Reload and choose the route again."
    );
    ensure!(
        catalog.load()?.revision == revision,
        "Catalog changed. Reload and choose the machine and route again."
    );
    Ok(Launch {
        argv: arguments(program, machine, route, config.as_deref(), cwd),
        title: format!("Files | {} | {}", machine.name, route.name),
        machine_id: machine.id.clone(),
        route_id: route.id.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_override_never_falls_through_and_bundle_precedes_path() {
        let temp = tempfile::tempdir().unwrap();
        let program = temp.path().join("ssh-sessions");
        let companion = temp.path().join(if cfg!(windows) {
            "ssh-files.exe"
        } else {
            "ssh-files"
        });
        fs::write(&companion, b"fixture").unwrap();
        assert!(
            resolve_with(Some(OsStr::new("relative")), &program, |_| panic!(
                "Must not fall through"
            ))
            .is_err()
        );
        assert!(
            resolve_with(Some(OsStr::new("")), &program, |_| panic!(
                "Must not fall through"
            ))
            .is_err()
        );
        assert_eq!(
            resolve_with(None, &program, |_| panic!("Bundle should win")).unwrap(),
            companion
        );
        let explicit = temp.path().join("explicit");
        fs::write(&explicit, b"fixture").unwrap();
        assert_eq!(
            resolve_with(Some(explicit.as_os_str()), &program, |_| None).unwrap(),
            explicit
        );
        fs::remove_file(companion).unwrap();
        assert_eq!(
            resolve_with(None, &program, |_| Some(explicit.clone())).unwrap(),
            explicit
        );
        assert!(
            resolve_with(None, &program, |_| None)
                .unwrap_err()
                .to_string()
                .contains("not installed")
        );
    }

    #[test]
    fn selected_alias_address_and_literal_paths_are_separate_arguments() {
        let route = Route {
            id: "vpn".into(),
            name: "VPN".into(),
            host: "192.0.2.18".into(),
            port: 2222,
            ssh_alias: Some("work-box".into()),
        };
        let machine = Machine {
            id: "lab".into(),
            name: "- Lab 開發".into(),
            user: "dev".into(),
            routes: vec![route.clone()],
            group: String::new(),
            tags: vec![],
        };
        let args = arguments(
            Path::new("companion"),
            &machine,
            &route,
            Some(Path::new("config with 'quote' and 空白")),
            Path::new("local with spaces"),
        );
        let strings: Vec<_> = args.iter().map(|v| v.to_str().unwrap()).collect();
        assert_eq!(
            strings,
            [
                "companion",
                "--host",
                "work-box",
                "--hostname",
                "192.0.2.18",
                "--config",
                "config with 'quote' and 空白",
                "--user",
                "dev",
                "--port",
                "2222",
                "--label=- Lab 開發 | VPN",
                "--machine-id",
                "lab",
                "--route-id",
                "vpn",
                "--local",
                "local with spaces"
            ]
        );
    }

    #[test]
    fn handoff_freezes_binding_and_route_and_rejects_a_stale_catalog() {
        let temp = tempfile::tempdir().unwrap();
        let catalog = Catalog::new(
            &temp.path().join("catalog.json"),
            &temp.path().join("device"),
        )
        .unwrap();
        let config = temp.path().join("config 空白 with spaces");
        fs::write(&config, b"Host work-box\n  HostName old.example.test\n").unwrap();
        let route = Route {
            id: "lan".into(),
            name: "LAN".into(),
            host: "192.0.2.18".into(),
            port: 2222,
            ssh_alias: Some("work-box".into()),
        };
        let machine = Machine {
            id: "lab".into(),
            name: "Lab".into(),
            user: "dev".into(),
            routes: vec![route.clone()],
            group: String::new(),
            tags: vec![],
        };
        catalog
            .save(
                std::slice::from_ref(&machine),
                &catalog.load().unwrap().revision,
            )
            .unwrap();
        let bindings_path = catalog.device_path.with_extension("ssh-configs.json");
        let bindings = std::collections::BTreeMap::from([("lab/lan", config.to_str().unwrap())]);
        crate::catalog::write_json(&bindings_path, &bindings).unwrap();
        let revision = catalog.load().unwrap().revision;
        let launch = prepare(
            &catalog,
            &revision,
            &machine,
            &route,
            Path::new("companion"),
            temp.path(),
        )
        .unwrap();
        let frozen = launch.argv.clone();
        let replacement = temp.path().join("new config");
        fs::write(
            &replacement,
            b"Host work-box\n HostName newer.example.test\n",
        )
        .unwrap();
        crate::catalog::write_json(
            &bindings_path,
            &std::collections::BTreeMap::from([("lab/lan", replacement.to_str().unwrap())]),
        )
        .unwrap();
        assert_eq!(launch.argv, frozen);
        assert!(launch.argv.contains(&config.into_os_string()));
        assert!(!launch.argv.contains(&replacement.into_os_string()));
        let mut changed = machine.clone();
        changed.routes[0].host = "192.0.2.99".into();
        catalog.save(&[changed], &revision).unwrap();
        assert!(
            prepare(
                &catalog,
                &revision,
                &machine,
                &route,
                Path::new("companion"),
                temp.path()
            )
            .unwrap_err()
            .to_string()
            .contains("Catalog changed")
        );
    }
}
