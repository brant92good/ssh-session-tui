use crate::catalog::{Catalog, Machine, Snapshot, group_path, tags};
use crate::text::casefold;
use anyhow::{Result, ensure};
use std::collections::{BTreeMap, HashSet};

pub fn within(group: &str, parent: &str) -> bool {
    let group = casefold(group);
    let parent = casefold(parent);
    group == parent || !parent.is_empty() && group.starts_with(&(parent + "/"))
}
pub fn groups(machines: &[Machine]) -> BTreeMap<String, usize> {
    let mut result = BTreeMap::new();
    let mut canonical = BTreeMap::new();
    for machine in machines {
        if machine.group.is_empty() {
            continue;
        }
        let parts: Vec<_> = machine.group.split('/').collect();
        for depth in 1..=parts.len() {
            let name = parts[..depth].join("/");
            let key = canonical.entry(casefold(&name)).or_insert(name);
            *result.entry(key.clone()).or_insert(0) += 1;
        }
    }
    result
}
pub fn matches(machine: &Machine, query: &str) -> bool {
    let mut words = vec![
        machine.name.clone(),
        machine.user.clone(),
        machine.group.clone(),
    ];
    words.extend(machine.tags.clone());
    for route in &machine.routes {
        words.extend([
            route.host.clone(),
            route.name.clone(),
            route.ssh_alias.clone().unwrap_or_default(),
        ]);
    }
    let haystack = casefold(&words.join(" "));
    shlex::split(query)
        .unwrap_or_else(|| query.split_whitespace().map(str::to_owned).collect())
        .iter()
        .all(|term| {
            let term = casefold(term);
            if let Some(tag) = term.strip_prefix("tag:") {
                machine.tags.iter().any(|t| casefold(t) == tag)
            } else if let Some(group) = term.strip_prefix("group:") {
                within(&machine.group, group)
            } else {
                haystack.contains(&term)
            }
        })
}
pub fn filtered<'a>(
    machines: &'a [Machine],
    query: &str,
    group: Option<&str>,
    tag: Option<&str>,
) -> Vec<&'a Machine> {
    machines
        .iter()
        .filter(|m| {
            group.is_none_or(|g| within(&m.group, g))
                && matches(m, query)
                && tag.is_none_or(|t| m.tags.iter().any(|s| casefold(s) == casefold(t)))
        })
        .collect()
}
pub fn edit_many(
    catalog: &Catalog,
    ids: &[String],
    expected: &str,
    group: Option<&str>,
    add: &[String],
    remove: &[String],
) -> Result<Snapshot> {
    let mut snapshot = catalog.load()?;
    ensure!(
        snapshot.revision == expected,
        "Catalog changed. Reload before editing."
    );
    ensure!(
        !ids.is_empty()
            && ids
                .iter()
                .all(|id| snapshot.machines.iter().any(|m| &m.id == id)),
        "Select machines from the current catalog."
    );
    let group = group.map(group_path).transpose()?.map(|g| {
        groups(&snapshot.machines)
            .into_keys()
            .find(|old| casefold(old) == casefold(&g))
            .unwrap_or(g)
    });
    let add = tags(add.to_vec())?;
    let remove: HashSet<_> = tags(remove.to_vec())?
        .into_iter()
        .map(|t| casefold(&t))
        .collect();
    ensure!(
        !add.iter().any(|t| remove.contains(&casefold(t))),
        "A tag cannot be added and removed in the same edit."
    );
    for machine in &mut snapshot.machines {
        if ids.contains(&machine.id) {
            if let Some(group) = &group {
                machine.group = group.clone();
            }
            machine.tags.retain(|t| !remove.contains(&casefold(t)));
            machine.tags.extend(add.clone());
            machine.tags = tags(machine.tags.clone())?;
        }
    }
    catalog.save(&snapshot.machines, expected)
}
pub fn rename_group(catalog: &Catalog, old: &str, new: &str, expected: &str) -> Result<Snapshot> {
    let old = group_path(old)?;
    let new = group_path(new)?;
    let mut snapshot = catalog.load()?;
    ensure!(
        !old.is_empty()
            && !new.is_empty()
            && snapshot.machines.iter().any(|m| within(&m.group, &old)),
        "Choose an existing group and a nonempty new name."
    );
    ensure!(
        !casefold(&new).starts_with(&(casefold(&old) + "/")),
        "A group cannot be moved inside itself."
    );
    ensure!(
        casefold(&old) == casefold(&new)
            || !groups(&snapshot.machines)
                .keys()
                .any(|g| casefold(g) == casefold(&new)),
        "That group exists. Use Move to merge machines."
    );
    for machine in &mut snapshot.machines {
        if within(&machine.group, &old) {
            let suffix = machine
                .group
                .split('/')
                .skip(old.split('/').count())
                .collect::<Vec<_>>()
                .join("/");
            machine.group = group_path(&format!(
                "{new}{}{suffix}",
                if suffix.is_empty() { "" } else { "/" }
            ))?;
        }
    }
    catalog.save(&snapshot.machines, expected)
}
