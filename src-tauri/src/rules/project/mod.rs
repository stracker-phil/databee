//! Projects: the rules a user adds per connection and database (`projects.json`), on top of the
//! presets. Rules are stored in their preset shape, so a project exports as a preset file.

mod store;

use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

use super::icons::IconRule;
use super::kinds::{KindRule, StyleRule};
use super::preset;
use super::queries::{Home, Pinned};
use super::types::{Preset, Reference};
use crate::error::{AppError, AppResult};
use crate::store::settings::is_accent;
use crate::value::RowKey;

pub use store::{ProjectQueries, ProjectsStore};

/// One database of one connection. Fields use the snake_case of the rule types, so the stored
/// rules and the project around them read alike.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct Project {
    /// The connection slug (`databee://<slug>/…`).
    pub connection: String,
    pub database: String,
    /// `None`: suggested from the presets' marker tables.
    #[serde(default)]
    pub prefix: Option<String>,
    /// Where `network_tables` live (multisite). `None`: suggested, else the prefix.
    #[serde(default)]
    pub network_prefix: Option<String>,
    /// Enabled presets by name. `None`: every preset whose marker tables exist.
    #[serde(default)]
    pub presets: Option<Vec<String>>,
    /// Added rules, and rules that override the preset rule with the same id, or the same `from`
    /// and `when`.
    #[serde(default)]
    pub references: Vec<Reference>,
    /// Ids of project, preset or inferred references switched off, and of preset styles that ship
    /// on and are switched off.
    #[serde(default)]
    pub disabled: Vec<String>,
    /// Ids of preset styles that ship off and are switched on.
    #[serde(default)]
    pub enabled: Vec<String>,
    #[serde(default)]
    pub kinds: Vec<KindRule>,
    /// Added styles, and styles that override the preset style with the same `id`.
    #[serde(default)]
    pub styles: Vec<StyleRule>,
    /// Table icons; one per `table`, which beats a preset's icon for the tables it names.
    #[serde(default, deserialize_with = "known_icons")]
    pub icons: Vec<IconRule>,
    #[serde(default)]
    pub saved: Vec<SavedQuery>,
    /// The accent while this database is open, one of the settings' accents; `None`: the app's.
    /// Personalization, not a rule: exports leave it out.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "known_accent"
    )]
    #[ts(optional)]
    pub accent: Option<String>,
    /// Values shown on the readout, in the order they were added. Personal, not a rule: exports
    /// leave them out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional = nullable)]
    pub watched: Vec<WatchedValue>,
}

/// A value the readout shows: `column` of one row of `table`, or the node at `path` in its
/// decoded (PHP or JSON) value.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct WatchedValue {
    pub id: String,
    /// The user's name for it; `None`: `<table>.<column> › path`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
    pub table: String,
    pub column: String,
    /// The row by its primary key, or by a unique key, which survives the row being re-created.
    pub row: RowKey,
    /// Child keys from the decoded value's root, as the value tree shows them (`decoded` steps
    /// into a serialized string); empty: the whole cell.
    #[serde(default)]
    pub path: Vec<String>,
}

impl WatchedValue {
    fn same_value(&self, other: &WatchedValue) -> bool {
        self.table == other.table
            && self.column == other.column
            && self.row == other.row
            && self.path == other.path
    }

    /// The stored label, else `<table>.<column>` and the path.
    pub fn label(&self) -> String {
        if let Some(label) = &self.label {
            return label.clone();
        }
        let path: String = self.path.iter().map(|step| format!(" › {step}")).collect();
        format!("{}.{}{path}", self.table, self.column)
    }
}

/// A stored accent that is no longer (or never was) an accent loads as `None`, the app's accent.
fn known_accent<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = serde_json::Value::deserialize(deserializer)?;
    Ok(match raw {
        serde_json::Value::String(name) if is_accent(&name) => Some(name),
        serde_json::Value::Null => None,
        other => {
            log::warn!("Ignoring the unknown project accent {other}");
            None
        }
    })
}

/// A stored icon rule that no longer parses (an icon the set dropped, such as `none`) is left out,
/// so the presets' icon or `table` applies again.
fn known_icons<'de, D>(deserializer: D) -> Result<Vec<IconRule>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = Vec::<serde_json::Value>::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .filter_map(|rule| match IconRule::deserialize(&rule) {
            Ok(rule) => Some(rule),
            Err(e) => {
                log::warn!("Ignoring the project icon rule {rule}: {e}");
                None
            }
        })
        .collect())
}

/// A query the user saved, optionally pinned to the readout home. `pin` shares the shape of a
/// preset's `[[pinned]] home`, so a project query exports as a pinned query.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct SavedQuery {
    pub id: String,
    pub title: String,
    pub sql: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<Home>,
}

impl Project {
    pub fn new(connection: &str, database: &str) -> Self {
        Self {
            connection: connection.to_string(),
            database: database.to_string(),
            ..Self::default()
        }
    }

    pub fn key(&self) -> String {
        project_key(&self.connection, &self.database)
    }

    /// The project's rules as a preset named `<connection>-<database>`. `disabled`, the prefixes
    /// and the auto preset choice have no preset form; explicit presets become `requires`.
    pub fn to_preset(&self) -> Preset {
        Preset {
            name: format!("{}-{}", self.connection, self.database),
            requires: self.presets.clone().unwrap_or_default(),
            references: self.references.clone(),
            kinds: self.kinds.clone(),
            styles: self.styles.clone(),
            icons: self.icons.clone(),
            pinned: self.saved.iter().map(SavedQuery::to_pinned).collect(),
            ..Preset::default()
        }
    }

    /// Whether the preset style `rule` colors values here: one that ships on unless switched off,
    /// one that ships off when switched on.
    pub fn preset_style_on(&self, rule: &StyleRule) -> bool {
        if rule.enabled {
            !self.disabled.contains(&rule.id)
        } else {
            self.enabled.contains(&rule.id)
        }
    }

    /// Switches the preset style `rule` on or off; switching twice changes nothing.
    pub fn switch_preset_style(&mut self, rule: &StyleRule, on: bool) {
        let ids = if rule.enabled {
            &mut self.disabled
        } else {
            &mut self.enabled
        };
        let listed = on != rule.enabled;
        ids.retain(|id| *id != rule.id);
        if listed {
            ids.push(rule.id.clone());
        }
    }

    /// The project style that replaces the preset style `id`.
    pub fn style_override(&self, id: &str) -> Option<&StyleRule> {
        self.styles.iter().find(|s| s.id == id)
    }

    /// Gives styles stored before styles had ids their ids. A copy of a preset style (the old way
    /// to switch one on) becomes that style's switch, and an exact duplicate is dropped. Returns
    /// whether anything changed.
    pub fn migrate_styles(&mut self, presets: &[Arc<Preset>]) -> bool {
        if self.styles.iter().all(|s| !s.id.is_empty()) {
            return false;
        }
        let shipped: Vec<&StyleRule> = presets.iter().flat_map(|p| &p.styles).collect();
        let stored = std::mem::take(&mut self.styles);
        let mut taken: Vec<String> = stored
            .iter()
            .filter(|s| !s.id.is_empty())
            .map(|s| s.id.clone())
            .collect();
        for mut style in stored {
            if !style.id.is_empty() {
                self.styles.push(style);
                continue;
            }
            if let Some(copied) = shipped
                .iter()
                .find(|p| style.enabled && p.same_style(&style))
            {
                self.switch_preset_style(copied, true);
                continue;
            }
            if self.styles.iter().any(|s| s.same_style(&style)) {
                continue;
            }
            style.id = [style.default_id(), style.qualified_id()]
                .into_iter()
                .find(|id| !taken.contains(id))
                .unwrap_or_else(|| format!("{}#{}", style.qualified_id(), taken.len()));
            if let Some(overridden) = shipped.iter().find(|p| p.id == style.id) {
                self.switch_preset_style(overridden, true);
            }
            taken.push(style.id.clone());
            self.styles.push(style);
        }
        true
    }

    pub fn export(&self) -> AppResult<String> {
        preset::to_toml(&self.to_preset())
            .map_err(|e| AppError::Io(format!("Exporting {} failed: {e}", self.key())))
    }
}

impl SavedQuery {
    fn to_pinned(&self) -> Pinned {
        Pinned {
            id: self.id.clone(),
            title: (!self.title.is_empty()).then(|| self.title.clone()),
            sql: self.sql.clone(),
            home: self.pin.clone(),
        }
    }
}

/// `slug/database`. Slugs never hold a `/`, so the first one splits the key.
pub fn project_key(connection: &str, database: &str) -> String {
    format!("{connection}/{database}")
}

#[cfg(test)]
mod tests;
