//! Parses one preset file. Every rule is read on its own, so an error names the file, the rule
//! (`[[reference]] #3, line 40`) and the field.

mod check;
mod write;

use std::collections::HashMap;
use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::kinds::StyleRule;
use super::types::{Check, FieldError, Preset, Reference};
use check::check_preset;

pub use write::to_toml;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetError {
    /// The path of a user preset, `built-in <file>` for a compiled-in one.
    pub file: String,
    /// `[[reference]] #3` (1-based) with its line, when one rule is at fault.
    pub rule: Option<RuleAt>,
    pub field: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleAt {
    pub section: &'static str,
    pub index: usize,
    pub line: usize,
}

impl fmt::Display for PresetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", self.file)?;
        match &self.rule {
            Some(RuleAt {
                section: "preset",
                line,
                ..
            }) => write!(f, "[preset] (line {line}): ")?,
            Some(rule) => write!(
                f,
                "[[{}]] #{} (line {}): ",
                rule.section, rule.index, rule.line
            )?,
            None => {}
        }
        if let Some(field) = &self.field {
            write!(f, "`{field}`: ")?;
        }
        write!(f, "{}", self.message)
    }
}

/// The file as written: sections stay untyped tables so each rule can fail on its own.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    preset: toml::Spanned<toml::Table>,
    #[serde(default)]
    meta: Vec<toml::Spanned<toml::Table>>,
    #[serde(default)]
    reference: Vec<toml::Spanned<toml::Table>>,
    #[serde(default)]
    entity: Vec<toml::Spanned<toml::Table>>,
    #[serde(default)]
    kind: Vec<toml::Spanned<toml::Table>>,
    #[serde(default)]
    style: Vec<toml::Spanned<toml::Table>>,
    #[serde(default)]
    pattern: Vec<toml::Spanned<toml::Table>>,
    #[serde(default)]
    pinned: Vec<toml::Spanned<toml::Table>>,
    #[serde(default)]
    anomaly: Vec<toml::Spanned<toml::Table>>,
    #[serde(default)]
    action: Vec<toml::Spanned<toml::Table>>,
    #[serde(default)]
    find: Vec<toml::Spanned<toml::Table>>,
    #[serde(default)]
    icon: Vec<toml::Spanned<toml::Table>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    requires: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    marker_tables: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    network_tables: Vec<String>,
}

struct Reader<'a> {
    file: &'a str,
    src: &'a str,
}

impl Reader<'_> {
    fn error(&self, rule: Option<RuleAt>, field: Option<String>, message: String) -> PresetError {
        PresetError {
            file: self.file.to_string(),
            rule,
            field,
            message: message.trim_end().to_string(),
        }
    }

    fn line_of(&self, span: std::ops::Range<usize>) -> usize {
        self.src[..span.start.min(self.src.len())]
            .matches('\n')
            .count()
            + 1
    }

    fn at(&self, section: &'static str, index: usize, span: std::ops::Range<usize>) -> RuleAt {
        RuleAt {
            section,
            index: index + 1,
            line: self.line_of(span),
        }
    }

    fn toml_error(&self, rule: Option<RuleAt>, e: toml::de::Error) -> PresetError {
        let (field, message) = split_field(&e.to_string());
        self.error(rule, field, message)
    }

    fn decode<T: DeserializeOwned>(
        &self,
        at: &RuleAt,
        table: toml::Table,
    ) -> Result<T, PresetError> {
        toml::Value::Table(table)
            .try_into()
            .map_err(|e| self.toml_error(Some(at.clone()), e))
    }

    fn rules<T: DeserializeOwned + Check>(
        &self,
        section: &'static str,
        raw: Vec<toml::Spanned<toml::Table>>,
    ) -> Result<Vec<T>, PresetError> {
        raw.into_iter()
            .enumerate()
            .map(|(i, spanned)| {
                let at = self.at(section, i, spanned.span());
                let rule: T = self.decode(&at, spanned.into_inner())?;
                rule.check().map_err(|FieldError { field, message }| {
                    self.error(Some(at), Some(field), message)
                })?;
                Ok(rule)
            })
            .collect()
    }
}

/// toml reports a nested field as a trailing "in `path`" line.
fn split_field(message: &str) -> (Option<String>, String) {
    let message = message.trim_end();
    match message.rsplit_once("\nin `") {
        Some((head, path)) => (
            Some(path.trim_end_matches('`').to_string()),
            head.trim_end().to_string(),
        ),
        None => (None, message.to_string()),
    }
}

pub fn parse(file: &str, src: &str) -> Result<Preset, PresetError> {
    let reader = Reader { file, src };
    let raw: RawFile = toml::from_str(src).map_err(|e| reader.toml_error(None, e))?;
    // Cross-rule checks run after the tables are consumed, so their lines are taken first.
    let lines_of = |rules: &[toml::Spanned<toml::Table>]| -> Vec<usize> {
        rules.iter().map(|r| reader.line_of(r.span())).collect()
    };
    let rule_lines: HashMap<&str, Vec<usize>> = HashMap::from([
        ("preset", lines_of(std::slice::from_ref(&raw.preset))),
        ("reference", lines_of(&raw.reference)),
        ("pinned", lines_of(&raw.pinned)),
        ("anomaly", lines_of(&raw.anomaly)),
        ("action", lines_of(&raw.action)),
        ("style", lines_of(&raw.style)),
        ("icon", lines_of(&raw.icon)),
    ]);
    let header_at = reader.at("preset", 0, raw.preset.span());
    let header: Header = reader.decode(&header_at, raw.preset.into_inner())?;
    let mut preset = Preset {
        name: header.name,
        requires: header.requires,
        marker_tables: header.marker_tables,
        network_tables: header.network_tables,
        meta: reader.rules("meta", raw.meta)?,
        references: reader.rules("reference", raw.reference)?,
        entities: reader.rules("entity", raw.entity)?,
        kinds: reader.rules("kind", raw.kind)?,
        styles: reader.rules("style", raw.style)?,
        patterns: reader.rules("pattern", raw.pattern)?,
        pinned: reader.rules("pinned", raw.pinned)?,
        anomalies: reader.rules("anomaly", raw.anomaly)?,
        actions: reader.rules("action", raw.action)?,
        finds: reader.rules("find", raw.find)?,
        icons: reader.rules("icon", raw.icon)?,
    };
    fill_reference_ids(&mut preset.references);
    fill_style_ids(&mut preset.styles);
    check_preset(&preset).map_err(|bad| {
        let line = rule_lines
            .get(bad.section)
            .and_then(|lines| lines.get(bad.index).copied())
            .unwrap_or(1);
        let rule = RuleAt {
            section: bad.section,
            index: bad.index + 1,
            line,
        };
        reader.error(Some(rule), Some(bad.field), bad.message)
    })?;
    Ok(preset)
}

/// Default ids are `<from>[?<when>]`. Two rules that would share one (the same column pointing
/// at two tables) both get `-><to>` appended, so either can be disabled on its own.
fn fill_reference_ids(references: &mut [Reference]) {
    fill_ids(
        references,
        |r| &mut r.id,
        Reference::default_id,
        Reference::qualified_id,
    );
}

/// Default ids are `style:<scope><match>`; two styles that would share one both get `-><color>`.
fn fill_style_ids(styles: &mut [StyleRule]) {
    fill_ids(
        styles,
        |s| &mut s.id,
        StyleRule::default_id,
        StyleRule::qualified_id,
    );
}

fn fill_ids<R>(
    rules: &mut [R],
    id: fn(&mut R) -> &mut String,
    default_id: fn(&R) -> String,
    qualified_id: fn(&R) -> String,
) {
    let defaults: Vec<Option<String>> = rules
        .iter_mut()
        .map(|r| {
            let empty = id(r).is_empty();
            empty.then(|| default_id(r))
        })
        .collect();
    for (i, rule) in rules.iter_mut().enumerate() {
        let Some(default) = &defaults[i] else {
            continue;
        };
        let shared = defaults
            .iter()
            .filter(|d| d.as_ref() == Some(default))
            .count()
            > 1;
        *id(rule) = if shared {
            qualified_id(rule)
        } else {
            default.clone()
        };
    }
}

#[cfg(test)]
mod tests;
