//! Rules that carry SQL: patterns (groups matched by SQL), pinned queries and anomalies.
//! Preset SQL is trusted configuration in the MySQL/MariaDB dialect; only its placeholders are
//! checked here (`placeholder.rs`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::placeholder::{self, Allowed};
use super::types::{Check, Conditions, FieldError};

/// `text` (SQL or a title) uses only the placeholders `allowed` names.
pub(super) fn checked_field(
    field: &str,
    sql: &str,
    allowed: Allowed<'_>,
) -> Result<(), FieldError> {
    placeholder::check(sql, allowed).map_err(|message| FieldError::new(field, message))
}

/// Rows of `table` shown as a group on an `owner` row's document. Rules with the same `name`
/// and `owner` are one group (OR); a rule whose `{owner.…}`/`{meta.…}` value is NULL or `''`
/// is skipped.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(deny_unknown_fields)]
pub struct Pattern {
    pub name: String,
    pub owner: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub owner_when: Conditions,
    pub table: String,
    pub r#where: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

impl Check for Pattern {
    fn check(&self) -> Result<(), FieldError> {
        let allowed = Allowed {
            id: true,
            owner: true,
            meta: true,
            params: None,
        };
        checked_field("where", &self.r#where, allowed)
    }
}

/// Where a pinned query shows on the readout home.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "show", rename_all = "lowercase", try_from = "HomeFields")]
pub enum Home {
    Rows {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u32>,
    },
    Count,
    /// A single value in the readout header line.
    Value,
}

/// Read flat so a `limit` next to `count` or `value` is an error, not ignored.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HomeFields {
    show: String,
    limit: Option<u32>,
}

impl TryFrom<HomeFields> for Home {
    type Error = String;

    fn try_from(f: HomeFields) -> Result<Self, String> {
        match (f.show.as_str(), f.limit) {
            ("rows", limit) => Ok(Home::Rows { limit }),
            ("count", None) => Ok(Home::Count),
            ("value", None) => Ok(Home::Value),
            ("count" | "value", Some(_)) => Err("`limit` goes with show = \"rows\"".into()),
            (other, _) => Err(format!(
                "unknown show `{other}`, expected rows, count or value"
            )),
        }
    }
}

/// A saved query a preset ships. Without `home` it is offered in the palette only.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(deny_unknown_fields)]
pub struct Pinned {
    pub id: String,
    /// May name tables with `{p}`/`{np}` (`{p}orders`), so it is checked like SQL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub sql: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<Home>,
}

impl Check for Pinned {
    fn check(&self) -> Result<(), FieldError> {
        checked_field("sql", &self.sql, Allowed::default())?;
        let title = self.title.as_deref().unwrap_or_default();
        checked_field("title", title, Allowed::default())
    }
}

/// What an anomaly opens: a pinned query of the same preset, or table rows with a SQL filter.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(untagged, try_from = "OpenFields")]
pub enum AnomalyOpen {
    Pinned {
        pinned: String,
    },
    Table {
        table: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        r#where: Option<String>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenFields {
    pinned: Option<String>,
    table: Option<String>,
    r#where: Option<String>,
}

impl TryFrom<OpenFields> for AnomalyOpen {
    type Error = String;

    fn try_from(f: OpenFields) -> Result<Self, String> {
        match (f.pinned, f.table, f.r#where) {
            (Some(pinned), None, None) => Ok(AnomalyOpen::Pinned { pinned }),
            (None, Some(table), r#where) => Ok(AnomalyOpen::Table { table, r#where }),
            _ => Err("`open` is either { pinned } or { table, where }".into()),
        }
    }
}

/// Shown on the readout while `sql` returns a non-zero number.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(deny_unknown_fields)]
pub struct Anomaly {
    pub id: String,
    pub sql: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<AnomalyOpen>,
    /// A `scope = "table"` action of the same preset, offered next to the anomaly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

impl Check for Anomaly {
    fn check(&self) -> Result<(), FieldError> {
        checked_field("sql", &self.sql, Allowed::default())?;
        checked_field("text", &self.text, Allowed::default())?;
        match &self.open {
            Some(AnomalyOpen::Table {
                r#where: Some(w), ..
            }) => checked_field("open.where", w, Allowed::default()),
            _ => Ok(()),
        }
    }
}
