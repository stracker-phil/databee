//! Kind rules (what a column or meta value holds) and value styles (which values get a color).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::condition::Matcher;
use super::types::{Check, ColumnGlob, FieldError};

/// Which zone a timestamp is stored in. `local` is the site's zone, which is not stored, so such
/// values are shown raw and never converted.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "lowercase")]
pub enum Zone {
    Unix,
    Utc,
    Local,
}

/// `currency` lists names to try in order: a column of the row, a column of the owner row, a
/// meta key of the same document. Empty: an amount without currency.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(deny_unknown_fields)]
pub struct Money {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub currency: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueRule {
    Money(Money),
    Timestamp(Zone),
    Enum,
    Secret,
    Number,
}

/// Applies to `columns` (globs) and to rows of meta tables whose key matches a `meta_keys` glob,
/// in every applied meta table unless `in_meta` names some.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[serde(try_from = "KindRuleFields", into = "KindRuleFields")]
#[ts(export, export_to = "src/api/types.gen.ts", as = "KindRuleFields")]
pub struct KindRule {
    pub columns: Vec<ColumnGlob>,
    pub meta_keys: Vec<String>,
    pub in_meta: Vec<String>,
    pub value: ValueRule,
}

/// The stored shape of a `KindRule`: one of `money`, `timestamp`, `enum`, `secret`, `number`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(deny_unknown_fields)]
pub struct KindRuleFields {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<ColumnGlob>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub meta_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub r#in: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub money: Option<Money>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<Zone>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub r#enum: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub secret: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub number: bool,
}

impl TryFrom<KindRuleFields> for KindRule {
    type Error = String;

    fn try_from(f: KindRuleFields) -> Result<Self, String> {
        let set = [
            f.money.is_some(),
            f.timestamp.is_some(),
            f.r#enum,
            f.secret,
            f.number,
        ];
        if set.iter().filter(|on| **on).count() != 1 {
            return Err(
                "a kind rule sets exactly one of money, timestamp, enum = true, secret = true, number = true"
                    .into(),
            );
        }
        let value = match (f.money, f.timestamp) {
            (Some(money), _) => ValueRule::Money(money),
            (_, Some(zone)) => ValueRule::Timestamp(zone),
            _ if f.r#enum => ValueRule::Enum,
            _ if f.secret => ValueRule::Secret,
            _ => ValueRule::Number,
        };
        check_targets(&f.columns, &f.meta_keys, &f.r#in)?;
        Ok(Self {
            columns: f.columns,
            meta_keys: f.meta_keys,
            in_meta: f.r#in,
            value,
        })
    }
}

impl From<KindRule> for KindRuleFields {
    fn from(rule: KindRule) -> Self {
        let mut fields = KindRuleFields {
            columns: rule.columns,
            meta_keys: rule.meta_keys,
            r#in: rule.in_meta,
            money: None,
            timestamp: None,
            r#enum: false,
            secret: false,
            number: false,
        };
        match rule.value {
            ValueRule::Money(money) => fields.money = Some(money),
            ValueRule::Timestamp(zone) => fields.timestamp = Some(zone),
            ValueRule::Enum => fields.r#enum = true,
            ValueRule::Secret => fields.secret = true,
            ValueRule::Number => fields.number = true,
        }
        fields
    }
}

/// Kind and style rules are always scoped: a rule that names nothing would apply nowhere, and
/// `in` narrows meta keys only.
fn check_targets(
    columns: &[ColumnGlob],
    meta_keys: &[String],
    in_meta: &[String],
) -> Result<(), String> {
    if columns.is_empty() && meta_keys.is_empty() {
        return Err("a rule names `columns` or `meta_keys`".into());
    }
    if !in_meta.is_empty() && meta_keys.is_empty() {
        return Err("`in` narrows `meta_keys`, which is empty".into());
    }
    Ok(())
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub enum StyleMatch {
    #[serde(rename = "=")]
    Eq,
    #[serde(rename = "LIKE")]
    Like,
    #[serde(rename = "REGEXP")]
    Regexp,
}

/// The scheme's style colors (`--style-<name>`).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "lowercase")]
pub enum StyleColor {
    Red,
    Green,
    Yellow,
    Blue,
    Purple,
}

impl StyleColor {
    /// The name `Value.style` carries (`--style-<name>`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Green => "green",
            Self::Yellow => "yellow",
            Self::Blue => "blue",
            Self::Purple => "purple",
        }
    }
}

/// Colors values of its columns or meta keys that match. Never global: the same text in another
/// column stays uncolored.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(deny_unknown_fields)]
pub struct StyleRule {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<ColumnGlob>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub meta_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub r#in: Vec<String>,
    #[serde(rename = "match")]
    pub matching: StyleMatch,
    pub value: String,
    pub color: StyleColor,
    /// Preset styles ship with `enabled = false`; a project switches them on.
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

fn enabled_by_default() -> bool {
    true
}

impl Check for StyleRule {
    fn check(&self) -> Result<(), FieldError> {
        check_targets(&self.columns, &self.meta_keys, &self.r#in)
            .map_err(|message| FieldError::new("columns", message))?;
        self.matcher()
            .map(drop)
            .map_err(|message| FieldError::new("value", message))
    }
}

impl StyleRule {
    pub fn matcher(&self) -> Result<Matcher, String> {
        match self.matching {
            StyleMatch::Eq => Ok(Matcher::Eq(self.value.clone())),
            StyleMatch::Like => Matcher::like(&self.value),
            StyleMatch::Regexp => Matcher::regexp(&self.value),
        }
    }
}

/// The shape was checked while deserializing.
impl Check for KindRule {}
