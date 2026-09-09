use crate::text::casefold;
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub id: String,
    pub name: String,
    pub host: String,
    #[serde(deserialize_with = "deserialize_port")]
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Machine {
    pub id: String,
    pub name: String,
    pub user: String,
    pub routes: Vec<Route>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u8,
    machines: Vec<Machine>,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub machines: Vec<Machine>,
    pub revision: String,
}

pub fn id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}
pub fn digest(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}

pub fn identifier(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 80
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)),
        "Use an ID of 1–80 letters, numbers, underscores or hyphens."
    );
    Ok(())
}
pub fn label(value: &str) -> Result<String> {
    ensure!(
        !value.trim().is_empty()
            && value.chars().count() <= 100
            && !value.chars().any(|c| c.is_ascii_control()),
        "Use a readable name of 1–100 characters."
    );
    Ok(value.trim().into())
}
pub fn address(value: &str) -> Result<String> {
    let value = label(value)?;
    if value.parse::<std::net::IpAddr>().is_ok() {
        return Ok(value);
    }
    if let Some((ip, scope)) = value.split_once('%')
        && ip.parse::<std::net::Ipv6Addr>().is_ok()
        && !scope.is_empty()
        && !scope.contains('%')
    {
        return Ok(value);
    }
    ensure!(
        value
            .as_bytes()
            .first()
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_'),
        "Enter an IP address, hostname or SSH alias."
    );
    ensure!(
        value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c)),
        "Enter an IP address, hostname or SSH alias, without a username."
    );
    Ok(value)
}
pub fn login(value: &str) -> Result<String> {
    let value = label(value)?;
    ensure!(
        !value.starts_with('-') && !value.chars().any(char::is_whitespace),
        "Enter an SSH username without spaces or a leading hyphen."
    );
    Ok(value)
}
pub fn port(value: &str) -> Result<u16> {
    ensure!(
        !value.is_empty() && value.bytes().all(|c| c.is_ascii_digit()),
        "Port must be a number from 1 to 65535."
    );
    let port: u16 = value
        .parse()
        .context("Port must be a number from 1 to 65535.")?;
    ensure!(port > 0, "Port must be a number from 1 to 65535.");
    Ok(port)
}
fn deserialize_port<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<u16, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    let value = match value {
        serde_json::Value::String(value) => value,
        serde_json::Value::Number(value) if value.is_u64() => value.to_string(),
        _ => {
            return Err(serde::de::Error::custom(
                "Port must be an integer or numeric string from 1 to 65535.",
            ));
        }
    };
    port(&value).map_err(serde::de::Error::custom)
}
pub fn group_path(value: &str) -> Result<String> {
    ensure!(
        value.chars().count() <= 160,
        "Groups allow at most 160 characters."
    );
    let value = value.trim();
    if value.is_empty() {
        return Ok(String::new());
    }
    ensure!(
        value.chars().count() <= 160 && value.split('/').count() <= 8,
        "Groups allow up to 8 levels and 160 characters."
    );
    value
        .split('/')
        .map(|part| {
            let part = label(part)?;
            ensure!(part != "." && part != "..", "Group names cannot be . or ..");
            Ok(part)
        })
        .collect::<Result<Vec<_>>>()
        .map(|parts| parts.join("/"))
}
pub fn tags(values: impl IntoIterator<Item = String>) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let mut count = 0;
    for value in values {
        count += 1;
        ensure!(count <= 30, "Use at most 30 tags.");
        let value = label(&value)?;
        ensure!(
            value.chars().count() <= 40 && !value.contains(','),
            "Tags allow up to 40 characters, without commas."
        );
        if !result
            .iter()
            .any(|s: &String| casefold(s) == casefold(&value))
        {
            result.push(value);
        }
    }
    Ok(result)
}
pub fn parse_tags(value: &str) -> Result<Vec<String>> {
    if value.trim().is_empty() {
        Ok(Vec::new())
    } else {
        tags(value.split(',').map(str::to_owned))
    }
}

impl Route {
    pub fn validate(&mut self) -> Result<()> {
        identifier(&self.id)?;
        self.name = label(&self.name)?;
        self.host = address(&self.host)?;
        ensure!(self.port > 0, "Port must be between 1 and 65535.");
        if let Some(alias) = &mut self.ssh_alias {
            *alias = address(alias)?;
        }
        Ok(())
    }
}
impl Machine {
    pub fn validate(&mut self) -> Result<()> {
        identifier(&self.id)?;
        self.name = label(&self.name)?;
        self.user = login(&self.user)?;
        self.group = group_path(&self.group)?;
        self.tags = tags(self.tags.clone())?;
        ensure!(
            !self.routes.is_empty() && self.routes.len() <= 30,
            "Each machine needs 1–30 routes."
        );
        let mut ids = HashSet::new();
        for route in &mut self.routes {
            route.validate()?;
            ensure!(ids.insert(route.id.clone()), "Duplicate route ID.");
        }
        Ok(())
    }
}

pub fn decode(raw: &[u8]) -> Result<Vec<Machine>> {
    ensure!(
        raw.len() <= 512 * 1024,
        "Catalog exceeds the 512 KiB limit."
    );
    let raw = raw.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(raw);
    let value: serde_json::Value =
        serde_json::from_slice(raw).context("Catalog is not valid JSON.")?;
    if value.get("version") == Some(&serde_json::json!(1))
        && let Some(machines) = value.get("machines").and_then(|v| v.as_array())
    {
        ensure!(
            !machines
                .iter()
                .any(|m| m.get("group").is_some() || m.get("tags").is_some()),
            "Groups and tags require catalog version 2."
        );
    }
    let mut document: Document =
        serde_json::from_value(value).context("Catalog fields are invalid.")?;
    ensure!(
        matches!(document.version, 1 | 2),
        "Unsupported catalog version."
    );
    ensure!(
        document.machines.len() <= 1000,
        "Catalog allows at most 1,000 machines."
    );
    let mut ids = HashSet::new();
    for machine in &mut document.machines {
        machine.validate()?;
        ensure!(ids.insert(machine.id.clone()), "Duplicate machine ID.");
    }
    Ok(document.machines)
}
pub fn encode(machines: &[Machine]) -> Result<Vec<u8>> {
    let document = Document {
        version: if machines
            .iter()
            .any(|m| !m.group.is_empty() || !m.tags.is_empty())
        {
            2
        } else {
            1
        },
        machines: machines.to_vec(),
    };
    let mut raw = serde_json::to_vec_pretty(&document)?;
    raw.push(b'\n');
    decode(&raw)?;
    Ok(raw)
}
pub fn atomic_write(path: &Path, raw: &[u8]) -> Result<()> {
    let parent = path.parent().context("File needs a parent directory.")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(raw)?;
    file.as_file().sync_all()?;
    file.persist(path)
        .map_err(|e| e.error)
        .with_context(|| format!("Could not replace {}", path.display()))?;
    Ok(())
}
pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut raw = serde_json::to_vec_pretty(value)?;
    raw.push(b'\n');
    atomic_write(path, &raw)
}

pub struct Lock(File);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}
pub fn locked(path: &Path) -> Result<Lock> {
    fs::create_dir_all(path.parent().context("Lock needs a parent directory")?)?;
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    if file.metadata()?.len() == 0 {
        file.write_all(b"0")?;
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(Lock(file)),
            Err(error) => {
                if Instant::now() >= deadline {
                    bail!("Another tab is saving. Try again. ({error})");
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

/// Resolve existing ancestors as Python Path.resolve does, without keeping Windows' extended prefix.
pub fn absolute(path: &Path) -> Result<PathBuf> {
    let path = std::path::absolute(path)?;
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::CurDir => {}
            _ => result.push(part.as_os_str()),
        }
        if matches!(part, std::path::Component::Normal(_)) && result.exists() {
            result = fs::canonicalize(&result)?;
            #[cfg(windows)]
            {
                let text = result.to_string_lossy();
                result = if let Some(unc) = text.strip_prefix("\\\\?\\UNC\\") {
                    PathBuf::from(format!("\\\\{unc}"))
                } else {
                    PathBuf::from(text.strip_prefix("\\\\?\\").unwrap_or(&text))
                };
            }
        }
    }
    Ok(result)
}

#[derive(Debug, Clone)]
pub struct Catalog {
    pub path: PathBuf,
    pub state_dir: PathBuf,
    pub device_path: PathBuf,
    pub lock_path: PathBuf,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preferences {
    version: u8,
    routes: BTreeMap<String, String>,
}
impl Catalog {
    pub fn new(path: &Path, state_dir: &Path) -> Result<Self> {
        let path = absolute(path)?;
        let state_dir = absolute(state_dir)?;
        let identity = path.to_string_lossy().to_string();
        #[cfg(windows)]
        let identity = casefold(&identity);
        let key = digest(identity.as_bytes())[..24].to_owned();
        Ok(Self {
            path,
            device_path: state_dir.join(format!("{key}.device.json")),
            lock_path: state_dir.join(format!("{key}.lock")),
            state_dir,
        })
    }
    pub fn load(&self) -> Result<Snapshot> {
        match fs::read(&self.path) {
            Ok(raw) => Ok(Snapshot {
                machines: decode(&raw)?,
                revision: digest(&raw),
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Snapshot {
                machines: Vec::new(),
                revision: digest(b""),
            }),
            Err(error) => Err(error.into()),
        }
    }
    pub fn save(&self, machines: &[Machine], expected: &str) -> Result<Snapshot> {
        let raw = encode(machines)?;
        let _guard = locked(&self.lock_path)?;
        ensure!(
            self.load()?.revision == expected,
            "Catalog changed in another tab. Reload before saving."
        );
        atomic_write(&self.path, &raw)?;
        self.load()
    }
    pub fn preferences(&self) -> Result<BTreeMap<String, String>> {
        if !self.device_path.exists() {
            return Ok(BTreeMap::new());
        }
        let data = fs::read(&self.device_path)?;
        ensure!(
            data.len() <= 512 * 1024,
            "Device preferences are too large."
        );
        let document: Preferences =
            serde_json::from_slice(data.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&data))
                .context("Invalid device preferences. Keep a backup before repairing the file.")?;
        ensure!(
            document.version == 1,
            "Unsupported device preferences version."
        );
        for (machine, route) in &document.routes {
            identifier(machine)?;
            identifier(route)?;
        }
        Ok(document.routes)
    }
    pub fn write_preferences(&self, routes: BTreeMap<String, String>) -> Result<()> {
        write_json(&self.device_path, &Preferences { version: 1, routes })
    }
    pub fn preferred<'a>(
        &self,
        machine: &'a Machine,
        preferences: &BTreeMap<String, String>,
    ) -> Option<&'a Route> {
        preferences
            .get(&machine.id)
            .and_then(|id| machine.routes.iter().find(|r| &r.id == id))
            .or_else(|| {
                if machine.routes.len() == 1 {
                    machine.routes.first()
                } else {
                    None
                }
            })
    }
    pub fn choose(&self, machine: &Machine, route: &str) -> Result<()> {
        let _guard = locked(&self.lock_path)?;
        let snapshot = self.load()?;
        let current = snapshot
            .machines
            .iter()
            .find(|m| m.id == machine.id)
            .context("Machine was removed. Reload the catalog.")?;
        ensure!(
            current.routes.iter().any(|r| r.id == route),
            "Route was removed. Reload the catalog."
        );
        let mut preferences = self.preferences()?;
        preferences.insert(machine.id.clone(), route.into());
        self.write_preferences(preferences)
    }
}
