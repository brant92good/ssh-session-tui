//! Static SSH metadata discovery. This module never starts SSH or evaluates directives.
use crate::catalog::{
    self, Catalog, Machine, Route, absolute, address, encode, group_path, locked, login, port,
    write_json,
};
use crate::connection::home;
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

pub fn default_config() -> PathBuf {
    home().join(".ssh/config")
}
fn system_config() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(std::env::var_os("PROGRAMDATA").unwrap_or_else(|| "C:/ProgramData".into()))
            .join("ssh/ssh_config")
    } else {
        PathBuf::from("/etc/ssh/ssh_config")
    }
}
pub fn expand_home(value: &str) -> PathBuf {
    if value == "~" {
        home()
    } else if let Some(rest) = value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
    {
        home().join(rest)
    } else {
        value.into()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub alias: String,
    pub host: String,
    pub user: String,
    pub port: u16,
    pub problem: String,
}
#[derive(Debug, Clone)]
pub struct Scan {
    pub config: PathBuf,
    pub entries: Vec<Entry>,
    pub digest: String,
}
#[derive(Debug, Clone)]
struct Row {
    keyword: String,
    values: Vec<String>,
    children: Vec<Option<Arc<Vec<Row>>>>,
}
#[derive(Default)]
struct Reader {
    files: BTreeMap<(PathBuf, PathBuf), ParsedFile>,
    aliases: BTreeSet<String>,
    total: usize,
}
type ParsedFile = (Vec<u8>, Arc<Vec<Row>>);

/// SSH quotes preserve Windows backslashes. Comments only count outside quotes.
fn tokens(line: &str) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let mut token = String::new();
    let mut quote = None;
    let mut started = false;
    for c in line.chars() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            } else {
                token.push(c);
            }
        } else if c == '#' {
            break;
        } else if c == '\'' || c == '"' {
            quote = Some(c);
            started = true;
        } else if c.is_whitespace()
            || (c == '=' && (result.is_empty() || (result.len() == 1 && !started)))
        {
            if started {
                result.push(std::mem::take(&mut token));
                started = false;
            }
        } else {
            token.push(c);
            started = true;
        }
    }
    ensure!(quote.is_none(), "Unclosed quote in SSH configuration.");
    if started {
        result.push(token);
    }
    Ok(result)
}
impl Reader {
    fn read(&mut self, path: &Path, stack: &[PathBuf], base: &Path) -> Result<Arc<Vec<Row>>> {
        let path = absolute(path)?;
        let key = (path.clone(), base.to_path_buf());
        ensure!(
            !stack.contains(&path) && stack.len() < 16,
            "SSH Include cycle or excessive nesting. Review configuration before importing."
        );
        if let Some((_, rows)) = self.files.get(&key) {
            return Ok(rows.clone());
        }
        ensure!(
            self.files.len() < 128,
            "Too many SSH Include files (limit 128)."
        );
        let raw = fs::read(&path).with_context(|| format!("Could not read {}", path.display()))?;
        self.total += raw.len();
        ensure!(
            raw.len() <= 1024 * 1024 && self.total <= 4 * 1024 * 1024,
            "SSH configuration exceeds the import size limit."
        );
        let mut rows = Vec::new();
        for (number, line) in
            std::str::from_utf8(raw.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&raw))?
                .lines()
                .enumerate()
        {
            let mut values =
                tokens(line).with_context(|| format!("{}, line {}", path.display(), number + 1))?;
            if values.is_empty() {
                continue;
            }
            let keyword = values.remove(0).to_lowercase();
            let mut children = Vec::new();
            if keyword == "host" {
                for value in &values {
                    if !value.contains(['*', '?', '!', '[']) && address(value).is_ok() {
                        self.aliases.insert(value.clone());
                    }
                }
                ensure!(
                    self.aliases.len() <= 1000,
                    "Too many SSH aliases (limit 1,000)."
                );
            }
            if keyword == "include" {
                for value in &values {
                    if value.contains('%') || value.contains("${") {
                        children.push(None);
                        continue;
                    }
                    let mut candidate = expand_home(value);
                    if !candidate.is_absolute() {
                        candidate = base.join(candidate);
                    }
                    let mut next_stack = stack.to_vec();
                    next_stack.push(path.clone());
                    let mut matches = glob::glob(&candidate.to_string_lossy())?
                        .collect::<std::result::Result<Vec<_>, _>>()?;
                    matches.sort();
                    for child in matches {
                        if child.is_file() {
                            children.push(Some(self.read(&child, &next_stack, base)?));
                        }
                    }
                }
            }
            rows.push(Row {
                keyword,
                values,
                children,
            });
        }
        let rows = Arc::new(rows);
        self.files.insert(key, (raw, rows.clone()));
        Ok(rows)
    }
}
fn wildcard(text: &str, pattern: &str) -> bool {
    // Only * and ? have special meaning in OpenSSH Host patterns.
    let text: Vec<_> = text.chars().collect();
    let pattern: Vec<_> = pattern.chars().collect();
    let (mut t, mut p, mut star, mut retry) = (0, 0, None, 0);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            t += 1;
            p += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            p += 1;
            retry = t;
        } else if let Some(s) = star {
            retry += 1;
            t = retry;
            p = s + 1;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}
fn matches(alias: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .any(|p| !p.starts_with('!') && wildcard(alias, p))
        && !patterns
            .iter()
            .filter_map(|p| p.strip_prefix('!'))
            .any(|p| wildcard(alias, p))
}
fn visit(
    rows: &[Row],
    alias: &str,
    mut active: Option<bool>,
    values: &mut BTreeMap<String, String>,
    uncertain: &mut HashSet<String>,
) {
    for row in rows {
        match row.keyword.as_str() {
            "host" => active = Some(matches(alias, &row.values)),
            "match" => {
                active = if row.values == ["all"] {
                    Some(true)
                } else {
                    None
                }
            }
            "include" if active != Some(false) => {
                for child in &row.children {
                    if child.is_none() || active.is_none() {
                        for key in ["hostname", "user", "port"] {
                            if !values.contains_key(key) {
                                uncertain.insert(key.into());
                            }
                        }
                    } else if let Some(child) = child {
                        visit(child, alias, active, values, uncertain);
                    }
                }
            }
            "hostname" | "user" | "port" | "canonicalizehostname"
                if active != Some(false) && !values.contains_key(&row.keyword) =>
            {
                if active.is_none() || row.values.len() != 1 {
                    uncertain.insert(row.keyword.clone());
                } else {
                    values.insert(row.keyword.clone(), row.values[0].clone());
                }
            }
            _ => {}
        }
    }
}
pub fn scan(config: Option<&Path>) -> Result<Scan> {
    let source = absolute(config.unwrap_or(&default_config()))?;
    let mut reader = Reader::default();
    let rows = reader.read(&source, &[], &home().join(".ssh"))?;
    let mut aliases: Vec<_> = reader.aliases.iter().cloned().collect();
    aliases.sort_by_key(|a| a.to_lowercase());
    let system = system_config();
    let defaults = if source == absolute(&default_config())? && system.is_file() {
        reader.read(
            &system,
            &[],
            system.parent().context("Invalid system config path")?,
        )?
    } else {
        Arc::new(Vec::new())
    };
    let mut entries = Vec::new();
    for alias in aliases {
        let mut values = BTreeMap::new();
        let mut uncertain = HashSet::new();
        visit(&rows, &alias, Some(true), &mut values, &mut uncertain);
        visit(&defaults, &alias, Some(true), &mut values, &mut uncertain);
        let resolve = || -> Result<Entry> {
            ensure!(
                uncertain.is_empty()
                    && matches!(
                        values
                            .get("canonicalizehostname")
                            .map(|s| s.to_lowercase())
                            .as_deref(),
                        None | Some("no" | "false")
                    ),
                "Conditional or canonicalized address settings need manual setup; no commands were evaluated."
            );
            let host = values.get("hostname").unwrap_or(&alias).to_lowercase();
            ensure!(
                !host.contains('%') && !host.contains("${"),
                "Address tokens need manual setup."
            );
            let user = values.get("user").cloned().unwrap_or_else(|| {
                std::env::var("USER")
                    .or_else(|_| std::env::var("USERNAME"))
                    .unwrap_or_default()
            });
            Ok(Entry {
                alias: alias.clone(),
                host: address(&host)?,
                user: login(&user)?,
                port: port(values.get("port").map(String::as_str).unwrap_or("22"))?,
                problem: String::new(),
            })
        };
        entries.push(resolve().unwrap_or_else(|error| Entry {
            alias,
            host: String::new(),
            user: String::new(),
            port: 22,
            problem: error.to_string(),
        }));
    }
    let mut hash = Sha256::new();
    for ((path, base), (raw, _)) in reader.files {
        hash.update(path.to_string_lossy().as_bytes());
        hash.update([0]);
        hash.update(base.to_string_lossy().as_bytes());
        hash.update([0]);
        hash.update(raw);
        hash.update([0]);
    }
    Ok(Scan {
        config: source,
        entries,
        digest: format!("{hash:x}", hash = hash.finalize()),
    })
}
pub const READY: &str = "Ready to import";
pub const IMPORTED: &str = "Already imported (select to bind this device)";
pub fn status(machines: &[Machine], entry: &Entry) -> String {
    if !entry.problem.is_empty() {
        return entry.problem.clone();
    }
    for machine in machines {
        for route in &machine.routes {
            if route.ssh_alias.as_deref() == Some(&entry.alias) {
                return if route.host == entry.host
                    && route.port == entry.port
                    && machine.user == entry.user
                {
                    IMPORTED
                } else {
                    "Alias already saved with different details; edit or remove that route first."
                }
                .into();
            }
        }
    }
    READY.into()
}
fn bindings(catalog: &Catalog) -> Result<BTreeMap<String, String>> {
    let path = catalog.device_path.with_extension("ssh-configs.json");
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let raw = fs::read(path)?;
    ensure!(
        raw.len() <= 2 * 1024 * 1024,
        "Local SSH paths file is too large."
    );
    serde_json::from_slice(&raw)
        .context("Local SSH import paths are invalid. Keep a backup before repairing the file.")
}
#[derive(Debug, Serialize)]
pub struct Imported {
    pub added_machines: usize,
    pub added_routes: usize,
    pub bound_existing: usize,
}
pub fn import(
    catalog: &Catalog,
    preview: &Scan,
    aliases: &[String],
    expected: &str,
    group: &str,
) -> Result<Imported> {
    let group = group_path(group)?;
    ensure!(
        !aliases.is_empty()
            && aliases
                .iter()
                .all(|a| preview.entries.iter().any(|e| &e.alias == a)),
        "Select at least one listed SSH alias."
    );
    ensure!(
        scan(Some(&preview.config))?.digest == preview.digest,
        "SSH config changed after preview. Reload before importing."
    );
    let _guard = locked(&catalog.lock_path)?;
    let snapshot = catalog.load()?;
    ensure!(
        snapshot.revision == expected,
        "Catalog changed in another tab. Reload before importing."
    );
    let mut machines = snapshot.machines.clone();
    let mut bindings = bindings(catalog)?;
    let mut preferences = catalog.preferences()?;
    let mut result = Imported {
        added_machines: 0,
        added_routes: 0,
        bound_existing: 0,
    };
    for entry in preview
        .entries
        .iter()
        .filter(|e| aliases.contains(&e.alias))
    {
        let status = status(&machines, entry);
        ensure!(
            status == READY || status == IMPORTED,
            "{}: {status}",
            entry.alias
        );
        let existing = machines.iter().find_map(|m| {
            m.routes
                .iter()
                .find(|r| r.ssh_alias.as_deref() == Some(&entry.alias))
                .map(|r| (m.id.clone(), r.id.clone()))
        });
        let (machine_id, route_id) = if let Some(existing) = existing {
            result.bound_existing += 1;
            existing
        } else {
            let route = Route {
                id: catalog::id(),
                name: format!("SSH: {}", entry.alias.chars().take(95).collect::<String>()),
                host: entry.host.clone(),
                port: entry.port,
                ssh_alias: Some(entry.alias.clone()),
            };
            let route_id = route.id.clone();
            if let Some(machine) = machines.iter_mut().find(|m| {
                m.user == entry.user
                    && m.routes
                        .iter()
                        .any(|r| r.host == entry.host && r.port == entry.port)
            }) {
                if let Some(preferred) = catalog.preferred(machine, &preferences) {
                    preferences.insert(machine.id.clone(), preferred.id.clone());
                }
                machine.routes.push(route);
                result.added_routes += 1;
                (machine.id.clone(), route_id)
            } else {
                let machine_id = catalog::id();
                machines.push(Machine {
                    id: machine_id.clone(),
                    name: entry.alias.clone(),
                    user: entry.user.clone(),
                    routes: vec![route],
                    group: group.clone(),
                    tags: Vec::new(),
                });
                result.added_machines += 1;
                (machine_id, route_id)
            }
        };
        bindings.insert(
            format!("{machine_id}/{route_id}"),
            preview.config.to_string_lossy().into_owned(),
        );
    }
    let raw = encode(&machines)?;
    // Local files first: an interrupted catalog write leaves only unused local bindings.
    write_json(
        &catalog.device_path.with_extension("ssh-configs.json"),
        &bindings,
    )?;
    catalog.write_preferences(preferences)?;
    if machines != snapshot.machines {
        catalog::atomic_write(&catalog.path, &raw)?;
    }
    Ok(result)
}
pub fn connection_config(
    catalog: &Catalog,
    machine: &Machine,
    route: &Route,
) -> Result<Option<PathBuf>> {
    let Some(alias) = &route.ssh_alias else {
        return Ok(None);
    };
    let config = bindings(catalog)?
        .get(&format!("{}/{}", machine.id, route.id))
        .map(PathBuf::from)
        .unwrap_or_else(default_config);
    if !config.is_file()
        || !scan(Some(&config))?
            .entries
            .iter()
            .any(|e| &e.alias == alias)
    {
        bail!(
            "SSH alias \"{alias}\" is missing on this computer. Press I to import it, or choose another route."
        );
    }
    Ok(if absolute(&config)? == absolute(&default_config())? {
        None
    } else {
        Some(config)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ssh_patterns_and_quoting() {
        assert!(matches("gpu1", &["gpu*".into(), "!gpu2".into()]));
        assert!(!matches("gpu2", &["gpu*".into(), "!gpu2".into()]));
        assert!(!wildcard("a", "[a]"));
        assert_eq!(
            tokens(r#"IdentityFile "C:\Users\demo\my key" # comment"#).unwrap(),
            vec!["IdentityFile", r"C:\Users\demo\my key"]
        );
        assert_eq!(
            tokens("HostName = demo.example").unwrap(),
            vec!["HostName", "demo.example"]
        );
    }
}
