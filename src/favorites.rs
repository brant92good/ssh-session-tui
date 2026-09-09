use crate::catalog::{Catalog, digest, identifier, locked, write_json};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
};

pub const LOCAL: &str = "@local";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Favorites {
    pub version: u8,
    pub slots: BTreeMap<String, String>,
}
impl Default for Favorites {
    fn default() -> Self {
        Self {
            version: 1,
            slots: BTreeMap::new(),
        }
    }
}
impl Favorites {
    pub fn load(catalog: &Catalog) -> Result<(Self, String)> {
        let path = catalog.device_path.with_extension("favorites.json");
        if !path.exists() {
            return Ok((Self::default(), digest(b"")));
        }
        let raw = fs::read(path)?;
        ensure!(raw.len() <= 8192, "Favorites file is too large.");
        let value: Self = serde_json::from_slice(&raw)
            .context("Invalid favorites file. Keep a backup before repairing it.")?;
        value.validate()?;
        Ok((value, digest(&raw)))
    }
    fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "Unsupported favorites version.");
        let mut targets = HashSet::new();
        for (slot, target) in &self.slots {
            validate_slot(slot)?;
            if target != LOCAL {
                identifier(target)?;
            }
            ensure!(
                targets.insert(target),
                "A target cannot occupy multiple favorite slots."
            );
        }
        Ok(())
    }
    pub fn assign(
        catalog: &Catalog,
        slot: &str,
        target: Option<&str>,
        expected: Option<&str>,
    ) -> Result<()> {
        validate_slot(slot)?;
        let _guard = locked(&catalog.lock_path)?;
        let (mut value, revision) = Self::load(catalog)?;
        if let Some(expected) = expected {
            ensure!(
                revision == expected,
                "Favorites changed in another tab. Reload before saving."
            );
        }
        if let Some(target) = target {
            ensure!(
                target == LOCAL || catalog.load()?.machines.iter().any(|m| m.id == target),
                "Select a machine from the current catalog."
            );
            value.slots.retain(|_, t| t != target);
            value.slots.insert(slot.into(), target.into());
        } else {
            value.slots.remove(slot);
        }
        value.validate()?;
        write_json(
            &catalog.device_path.with_extension("favorites.json"),
            &value,
        )
    }
}
pub fn validate_slot(slot: &str) -> Result<()> {
    ensure!(
        slot.len() == 1 && (b'1'..=b'9').contains(&slot.as_bytes()[0]),
        "Choose a favorite number from 1 to 9."
    );
    Ok(())
}
