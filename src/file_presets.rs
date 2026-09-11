//! Shared remote-path metadata. No discovery, credentials, route preferences,
//! local-path expansion or companion process is involved in this module.
use crate::catalog::{self, Catalog};
use crate::text::casefold;
use anyhow::{Context, Result, ensure};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{MapAccess, Visitor},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fs::{self, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};

pub const MAX_BYTES: usize = 128 * 1024;
pub const MAX_PER_MACHINE: usize = 64;
pub const MAX_TOTAL: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub path: String,
}
impl Preset {
    pub fn validate(&self) -> Result<()> {
        catalog::identifier(&self.id)?;
        ensure!(
            !self.name.trim().is_empty()
                && self.name.chars().count() <= 80
                && !self.name.chars().any(char::is_control),
            "Use a path name of 1–80 characters without terminal controls."
        );
        validate_path(&self.path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilePresets {
    version: u8,
    #[serde(deserialize_with = "unique_records")]
    machines: BTreeMap<String, Vec<Preset>>,
}
fn unique_records<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, Vec<Preset>>, D::Error> {
    struct Records;
    impl<'de> Visitor<'de> for Records {
        type Value = BTreeMap<String, Vec<Preset>>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("unique machine records")
        }
        fn visit_map<M: MapAccess<'de>>(
            self,
            mut map: M,
        ) -> std::result::Result<Self::Value, M::Error> {
            let mut result = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, Vec<Preset>>()? {
                if result.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("Duplicate machine record."));
                }
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(Records)
}
impl Default for FilePresets {
    fn default() -> Self {
        Self {
            version: 1,
            machines: BTreeMap::new(),
        }
    }
}

pub fn sidecar_path(catalog: &Catalog) -> PathBuf {
    let mut path = catalog.path.as_os_str().to_os_string();
    path.push(".files.json");
    PathBuf::from(path)
}
pub fn validate_path(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control),
        "Use a nonempty remote path of at most 4096 UTF-8 bytes without terminal controls."
    );
    Ok(())
}

/// Reject symlinks/reparse points and non-files; a missing optional file is OK.
pub(crate) fn regular_file(path: &Path, optional: bool) -> Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if optional && error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(error).with_context(|| format!("Cannot inspect {}", path.display()));
        }
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "Metadata files must not be reparse points."
        );
    }
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "Metadata must be a regular file, not a symlink or directory."
    );
    Ok(true)
}

impl FilePresets {
    pub fn load(catalog: &Catalog) -> Result<(Self, String)> {
        let path = sidecar_path(catalog);
        if !regular_file(&path, true)? {
            return Ok((
                Self::default(),
                catalog::digest(b"ssh-sessions:missing-file-presets:v1"),
            ));
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x00200000); // FILE_FLAG_OPEN_REPARSE_POINT
        }
        let file = options
            .open(&path)
            .with_context(|| format!("Cannot read {}", path.display()))?;
        let metadata = file.metadata()?;
        ensure!(metadata.is_file(), "Saved paths must be a regular file.");
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "Saved paths must not be a reparse point."
            );
        }
        let mut raw = Vec::new();
        file.take((MAX_BYTES + 1) as u64).read_to_end(&mut raw)?;
        Ok((Self::decode(&raw)?, catalog::digest(&raw)))
    }
    pub fn decode(raw: &[u8]) -> Result<Self> {
        ensure!(
            raw.len() <= MAX_BYTES,
            "Saved paths exceed the 128 KiB limit."
        );
        let value: Self =
            serde_json::from_slice(raw.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(raw))
                .context("Invalid saved paths. Keep a backup before repairing this file.")?;
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "Unsupported saved-path version.");
        ensure!(
            self.machines.len() <= MAX_TOTAL,
            "Too many saved-path machine records."
        );
        let mut total = 0;
        for (machine, presets) in &self.machines {
            catalog::identifier(machine)?;
            ensure!(
                presets.len() <= MAX_PER_MACHINE,
                "Each machine supports at most 64 saved paths."
            );
            total += presets.len();
            let (mut ids, mut names) = (BTreeSet::new(), BTreeSet::new());
            for preset in presets {
                preset.validate()?;
                ensure!(ids.insert(&preset.id), "Duplicate saved-path ID.");
                ensure!(
                    names.insert(casefold(preset.name.trim())),
                    "Saved path names must be distinct within a machine."
                );
            }
        }
        ensure!(
            total <= MAX_TOTAL,
            "At most 2048 paths can be saved in one catalog."
        );
        Ok(())
    }
    pub fn for_machine(&self, machine: &str) -> &[Preset] {
        self.machines.get(machine).map(Vec::as_slice).unwrap_or(&[])
    }
    pub fn get(&self, machine: &str, id: &str) -> Option<&Preset> {
        self.for_machine(machine)
            .iter()
            .find(|preset| preset.id == id)
    }
    pub fn upsert(
        &mut self,
        catalog: &Catalog,
        machine: &str,
        preset: Preset,
        expected_catalog: &str,
        expected_sidecar: &str,
    ) -> Result<String> {
        preset.validate()?;
        self.change(
            catalog,
            machine,
            expected_catalog,
            expected_sidecar,
            |presets| {
                if let Some(old) = presets.iter_mut().find(|old| old.id == preset.id) {
                    *old = preset;
                } else {
                    presets.push(preset);
                }
                Ok(())
            },
        )
    }
    pub fn delete(
        &mut self,
        catalog: &Catalog,
        machine: &str,
        id: &str,
        expected_catalog: &str,
        expected_sidecar: &str,
    ) -> Result<String> {
        catalog::identifier(id)?;
        self.change(
            catalog,
            machine,
            expected_catalog,
            expected_sidecar,
            |presets| {
                let index = presets
                    .iter()
                    .position(|preset| preset.id == id)
                    .context("Saved path was removed. Reload before editing.")?;
                presets.remove(index);
                Ok(())
            },
        )
    }
    fn change(
        &mut self,
        catalog: &Catalog,
        machine: &str,
        expected_catalog: &str,
        expected_sidecar: &str,
        change: impl FnOnce(&mut Vec<Preset>) -> Result<()>,
    ) -> Result<String> {
        catalog::identifier(machine)?;
        let _lock = catalog::locked(&catalog.lock_path)?;
        let current_catalog = catalog.load()?;
        ensure!(
            current_catalog.revision == expected_catalog,
            "Catalog changed. Reload before saving paths."
        );
        ensure!(
            current_catalog
                .machines
                .iter()
                .any(|entry| entry.id == machine),
            "Machine was removed. Reload before saving paths."
        );
        let (mut current, revision) = Self::load(catalog)?;
        ensure!(
            revision == expected_sidecar,
            "Saved paths changed in another tab. Reload before saving."
        );
        let before = current.clone();
        change(current.machines.entry(machine.into()).or_default())?;
        current.validate()?;
        if current == before {
            *self = current;
            return Ok(revision);
        }
        let mut raw = serde_json::to_vec_pretty(&current)?;
        raw.push(b'\n');
        ensure!(
            raw.len() <= MAX_BYTES,
            "Saved paths exceed the 128 KiB limit."
        );
        regular_file(&sidecar_path(catalog), true)?;
        catalog::atomic_write(&sidecar_path(catalog), &raw)?;
        *self = current;
        Ok(catalog::digest(&raw))
    }
}

pub fn validate_snapshot(
    catalog: &Catalog,
    expected_catalog: &str,
    expected_sidecar: &str,
) -> Result<()> {
    ensure!(
        catalog.load()?.revision == expected_catalog,
        "Catalog changed. Review the selected machine again."
    );
    ensure!(
        FilePresets::load(catalog)?.1 == expected_sidecar,
        "Saved paths changed. Review the selected path again."
    );
    ensure!(
        catalog.load()?.revision == expected_catalog,
        "Catalog changed. Review the selected machine again."
    );
    Ok(())
}
