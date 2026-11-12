//! Cross-rule checks of a parsed preset: unique ids, and references between its rules.

use std::collections::HashSet;

use crate::rules::actions::ActionScope;
use crate::rules::queries::AnomalyOpen;
use crate::rules::types::Preset;

/// A cross-rule problem: the rule's section and 0-based index.
pub(super) struct Bad {
    pub(super) section: &'static str,
    pub(super) index: usize,
    pub(super) field: String,
    pub(super) message: String,
}

impl Bad {
    fn new(section: &'static str, index: usize, field: &str, message: String) -> Self {
        Self {
            section,
            index,
            field: field.into(),
            message,
        }
    }
}

pub(super) fn check_preset(preset: &Preset) -> Result<(), Bad> {
    if preset.requires.contains(&preset.name) {
        let message = "a preset cannot require itself".into();
        return Err(Bad::new("preset", 0, "requires", message));
    }
    unique(
        "reference",
        preset.references.iter().map(|r| r.id.clone()),
        "id",
    )?;
    unique("style", preset.styles.iter().map(|s| s.id.clone()), "id")?;
    unique("pinned", preset.pinned.iter().map(|p| p.id.clone()), "id")?;
    unique(
        "icon",
        preset.icons.iter().map(|i| i.table.clone()),
        "table",
    )?;
    unique(
        "anomaly",
        preset.anomalies.iter().map(|a| a.id.clone()),
        "id",
    )?;
    let action_keys = preset
        .actions
        .iter()
        .map(|a| format!("{} on {}", a.name, a.table));
    unique("action", action_keys, "name")?;
    for (i, anomaly) in preset.anomalies.iter().enumerate() {
        if let Some(AnomalyOpen::Pinned { pinned }) = &anomaly.open
            && !preset.pinned.iter().any(|p| &p.id == pinned)
        {
            let message = format!("no [[pinned]] with id `{pinned}` in this preset");
            return Err(Bad::new("anomaly", i, "open.pinned", message));
        }
        if let Some(fix) = &anomaly.fix
            && !preset
                .actions
                .iter()
                .any(|a| &a.name == fix && a.scope == ActionScope::Table)
        {
            let message = format!("no [[action]] `{fix}` with scope = \"table\" in this preset");
            return Err(Bad::new("anomaly", i, "fix", message));
        }
    }
    Ok(())
}

fn unique(
    section: &'static str,
    ids: impl Iterator<Item = String>,
    field: &str,
) -> Result<(), Bad> {
    let mut seen = HashSet::new();
    for (i, id) in ids.enumerate() {
        if !seen.insert(id.clone()) {
            return Err(Bad::new(section, i, field, format!("`{id}` is used twice")));
        }
    }
    Ok(())
}
