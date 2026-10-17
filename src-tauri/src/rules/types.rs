//! The rule types of a preset file (docs/presets.md). Each type has one serialized form, the
//! TOML shape, which projects (JSON) and the page share, so an export is the stored rule verbatim.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

use super::actions::Action;
use super::condition::Matcher;
use super::kinds::{KindRule, StyleRule};
use super::queries::{Anomaly, Pattern, Pinned};

/// One preset: rules for tables named without the prefix.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct Preset {
    pub name: String,
    /// Presets that apply with this one and come before it.
    pub requires: Vec<String>,
    /// The preset applies when all of these exist under the project's prefix.
    pub marker_tables: Vec<String>,
    /// Tables that exist once per network, resolved under the network prefix (`{np}`).
    pub network_tables: Vec<String>,
    pub meta: Vec<MetaTable>,
    pub references: Vec<Reference>,
    pub entities: Vec<Entity>,
    pub kinds: Vec<KindRule>,
    pub styles: Vec<StyleRule>,
    pub patterns: Vec<Pattern>,
    pub pinned: Vec<Pinned>,
    pub anomalies: Vec<Anomaly>,
    pub actions: Vec<Action>,
    pub finds: Vec<FindSource>,
}

/// A rule's problem in one field, before the loader adds the file and the rule index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

impl FieldError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

/// Checks a rule beyond its shape (placeholders, fields that need each other).
pub trait Check {
    /// Rules whose shape says it all keep the default.
    fn check(&self) -> Result<(), FieldError> {
        Ok(())
    }
}

/// `table.column`, split at the first dot, with no `*`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, TS)]
#[serde(try_from = "String", into = "String")]
#[ts(export, export_to = "src/api/types.gen.ts", type = "string")]
pub struct ColumnRef {
    pub table: String,
    pub column: String,
}

/// `table.column` where either side may hold `*` globs (`*.*_gmt`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, TS)]
#[serde(try_from = "String", into = "String")]
#[ts(export, export_to = "src/api/types.gen.ts", type = "string")]
pub struct ColumnGlob {
    pub table: String,
    pub column: String,
}

fn split_column(text: &str) -> Result<(String, String), String> {
    match text.split_once('.') {
        Some((table, column)) if !table.is_empty() && !column.is_empty() => {
            Ok((table.to_string(), column.to_string()))
        }
        _ => Err(format!("expected `table.column`, got `{text}`")),
    }
}

impl TryFrom<String> for ColumnRef {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        if text.contains('*') {
            return Err(format!(
                "`{text}` names one column; globs are not allowed here"
            ));
        }
        let (table, column) = split_column(&text)?;
        Ok(Self { table, column })
    }
}

impl TryFrom<String> for ColumnGlob {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        let (table, column) = split_column(&text)?;
        Ok(Self { table, column })
    }
}

impl fmt::Display for ColumnRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.table, self.column)
    }
}

impl fmt::Display for ColumnGlob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.table, self.column)
    }
}

impl From<ColumnRef> for String {
    fn from(c: ColumnRef) -> String {
        c.to_string()
    }
}

impl From<ColumnGlob> for String {
    fn from(c: ColumnGlob) -> String {
        c.to_string()
    }
}

/// How one column's value must look: `"x"` (=), `["a", "b"]` (IN), `{ glob }`, `{ like }`,
/// `{ regexp }` or `{ not = <match> }`. Integers are read as their text.
#[derive(Serialize, Debug, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(untagged)]
pub enum Match {
    Eq(String),
    In(Vec<String>),
    Glob { glob: String },
    Like { like: String },
    Regexp { regexp: String },
    Not { not: Box<Match> },
}

/// Column name to match, all of which must hold (AND).
pub type Conditions = BTreeMap<String, Match>;

impl Match {
    /// Parsed by hand so a wrong shape says what is allowed instead of "no variant matched".
    fn from_value(value: &toml::Value) -> Result<Self, String> {
        match value {
            toml::Value::Array(items) => items
                .iter()
                .map(scalar)
                .collect::<Result<_, _>>()
                .map(Match::In),
            toml::Value::Table(table) => Self::from_operator(table),
            other => scalar(other).map(Match::Eq),
        }
    }

    fn from_operator(table: &toml::Table) -> Result<Self, String> {
        let mut entries = table.iter();
        let (Some((op, arg)), None) = (entries.next(), entries.next()) else {
            return Err("a condition table holds exactly one of glob, like, regexp, not".into());
        };
        match op.as_str() {
            "glob" => scalar(arg).map(|glob| Match::Glob { glob }),
            "like" => scalar(arg).map(|like| Match::Like { like }),
            "regexp" => {
                let regexp = scalar(arg)?;
                // Checked on load, so a broken pattern names its rule instead of never matching.
                Matcher::regexp(&regexp)?;
                Ok(Match::Regexp { regexp })
            }
            "not" => Self::from_value(arg).map(|m| Match::Not { not: Box::new(m) }),
            other => Err(format!(
                "unknown condition `{other}`, expected glob, like, regexp or not"
            )),
        }
    }
}

fn scalar(value: &toml::Value) -> Result<String, String> {
    match value {
        toml::Value::String(s) => Ok(s.clone()),
        toml::Value::Integer(i) => Ok(i.to_string()),
        other => Err(format!(
            "expected a string or an integer, got {}",
            other.type_str()
        )),
    }
}

impl<'de> Deserialize<'de> for Match {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = toml::Value::deserialize(deserializer)?;
        Match::from_value(&value).map_err(D::Error::custom)
    }
}

/// Compact form for default ids: `=x`, `=[a,b]`, `=glob(x)`, `!=x`.
impl fmt::Display for Match {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Match::Eq(v) => write!(f, "={v}"),
            Match::In(vs) => write!(f, "=[{}]", vs.join(",")),
            Match::Glob { glob } => write!(f, "=glob({glob})"),
            Match::Like { like } => write!(f, "=like({like})"),
            Match::Regexp { regexp } => write!(f, "=regexp({regexp})"),
            Match::Not { not } => write!(f, "!{not}"),
        }
    }
}

/// A key/value table. With `owner` and `fk` it describes the owner's rows and declares the
/// reference `fk → owner`; without them it stands alone (an options table).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(deny_unknown_fields)]
pub struct MetaTable {
    pub table: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fk: Option<String>,
    pub key: String,
    pub value: String,
}

impl Check for MetaTable {
    fn check(&self) -> Result<(), FieldError> {
        match (&self.owner, &self.fk) {
            (Some(_), None) => Err(FieldError::new("fk", "`owner` needs `fk`")),
            (None, Some(_)) => Err(FieldError::new("owner", "`fk` needs `owner`")),
            _ => Ok(()),
        }
    }
}

/// A virtual foreign key. `when` holds on the source row, `to_when` on the target row (for
/// targets that are not a primary key, such as external ids).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(deny_unknown_fields)]
pub struct Reference {
    /// Empty until the loader fills in the default (`default_id`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub from: ColumnRef,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub when: Conditions,
    pub to: ColumnRef,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub to_when: Conditions,
    /// Names the incoming group on the target's document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The incoming group's columns; keys of the source's meta table are pivoted in.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<String>,
}

impl Reference {
    /// `<from>[?<column><match>&…]`, e.g. `postmeta.meta_value?meta_key=_customer_user`.
    pub fn default_id(&self) -> String {
        let mut id = self.from.to_string();
        for (i, (column, matcher)) in self.when.iter().enumerate() {
            id.push(if i == 0 { '?' } else { '&' });
            id.push_str(column);
            id.push_str(&matcher.to_string());
        }
        id
    }
}

impl Check for Reference {
    fn check(&self) -> Result<(), FieldError> {
        if self.label.as_deref() == Some("") {
            return Err(FieldError::new("label", "is empty; leave it out instead"));
        }
        Ok(())
    }
}

/// A name, header and search columns for rows of one table. A `filter` (SQL) narrows the rows;
/// filtered entities win over unfiltered ones on the same table.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(deny_unknown_fields)]
pub struct Entity {
    pub name: String,
    pub table: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// Columns or meta keys shown in the object header.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub summary: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// Searched with "contains".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub search: Vec<String>,
    /// Searched for an exact match; may name meta keys.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub search_exact: Vec<String>,
    /// Found by the palette's `@` prefix.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub people: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "lowercase")]
pub enum FindMatch {
    #[default]
    Exact,
    Contains,
}

/// Columns that find everywhere searches besides meta values and entity search columns.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(deny_unknown_fields)]
pub struct FindSource {
    pub table: String,
    pub columns: Vec<String>,
    #[serde(default, rename = "match")]
    pub matching: FindMatch,
}

impl Check for Entity {}

impl Check for FindSource {
    fn check(&self) -> Result<(), FieldError> {
        if self.columns.is_empty() {
            return Err(FieldError::new("columns", "names no column"));
        }
        Ok(())
    }
}
