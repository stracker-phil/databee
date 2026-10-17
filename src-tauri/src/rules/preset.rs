//! Parses one preset file. Every rule is read on its own, so an error names the file, the rule
//! (`[[reference]] #3, line 40`) and the field.

use std::collections::{HashMap, HashSet};
use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::actions::{Action, ActionScope};
use super::kinds::{KindRule, StyleRule};
use super::queries::{Anomaly, AnomalyOpen, Pattern, Pinned};
use super::types::{Check, Entity, FieldError, FindSource, MetaTable, Preset, Reference};

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
    };
    fill_reference_ids(&mut preset.references);
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
    let defaults: Vec<Option<String>> = references
        .iter()
        .map(|r| r.id.is_empty().then(|| r.default_id()))
        .collect();
    for (i, reference) in references.iter_mut().enumerate() {
        let Some(id) = &defaults[i] else { continue };
        let shared = defaults.iter().filter(|d| d.as_ref() == Some(id)).count() > 1;
        reference.id = if shared {
            reference.qualified_id()
        } else {
            id.clone()
        };
    }
}

/// A cross-rule problem: the rule's section and 0-based index.
struct Bad {
    section: &'static str,
    index: usize,
    field: String,
    message: String,
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

fn check_preset(preset: &Preset) -> Result<(), Bad> {
    if preset.requires.contains(&preset.name) {
        let message = "a preset cannot require itself".into();
        return Err(Bad::new("preset", 0, "requires", message));
    }
    unique(
        "reference",
        preset.references.iter().map(|r| r.id.clone()),
        "id",
    )?;
    unique("pinned", preset.pinned.iter().map(|p| p.id.clone()), "id")?;
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

#[derive(Serialize)]
struct FileOut<'a> {
    preset: Header,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    meta: &'a [MetaTable],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    reference: &'a [Reference],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    entity: &'a [Entity],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    kind: &'a [KindRule],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    style: &'a [StyleRule],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pattern: &'a [Pattern],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pinned: &'a [Pinned],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    anomaly: &'a [Anomaly],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    action: &'a [Action],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    find: &'a [FindSource],
}

/// `preset` as a preset file; `parse` reads it back to an equal preset.
pub fn to_toml(preset: &Preset) -> Result<String, toml::ser::Error> {
    toml::to_string(&FileOut {
        preset: Header {
            name: preset.name.clone(),
            requires: preset.requires.clone(),
            marker_tables: preset.marker_tables.clone(),
            network_tables: preset.network_tables.clone(),
        },
        meta: &preset.meta,
        reference: &preset.references,
        entity: &preset.entities,
        kind: &preset.kinds,
        style: &preset.styles,
        pattern: &preset.patterns,
        pinned: &preset.pinned,
        anomaly: &preset.anomalies,
        action: &preset.actions,
        find: &preset.finds,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::super::actions::{ActionEffect, DeleteSpec, Param, ParamType, SetValue, UpdateStep};
    use super::super::catalog::BUILT_IN;
    use super::super::kinds::{Money, StyleColor, StyleMatch, ValueRule, Zone};
    use super::super::queries::Home;
    use super::super::types::{ColumnGlob, Conditions, FindMatch, Match};
    use super::*;

    /// A file whose `[preset]` header takes lines 1 to 3, so `body` starts on line 4.
    fn file(body: &str) -> String {
        format!("[preset]\nname = \"t\"\n\n{body}")
    }

    fn parsed(body: &str) -> Preset {
        parse("t.toml", &file(body)).unwrap_or_else(|e| panic!("{e}"))
    }

    fn rejected(body: &str) -> PresetError {
        parse("shop.toml", &file(body)).expect_err(body)
    }

    fn rule_of(error: &PresetError) -> &RuleAt {
        error.rule.as_ref().expect("the error names a rule")
    }

    fn conditions<const N: usize>(entries: [(&str, Match); N]) -> Conditions {
        entries
            .into_iter()
            .map(|(column, m)| (column.to_string(), m))
            .collect()
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn set<const N: usize>(entries: [(&str, SetValue); N]) -> BTreeMap<String, SetValue> {
        entries
            .into_iter()
            .map(|(column, v)| (column.to_string(), v))
            .collect()
    }

    fn text(value: &str) -> SetValue {
        SetValue::Text(value.to_string())
    }

    fn sql(value: &str) -> SetValue {
        SetValue::Sql {
            sql: value.to_string(),
        }
    }

    #[test]
    fn error_names_file_rule_and_field() {
        let error = rejected("[[reference]]\nfrom = \"a.b\"\nto = \"c\"\n");
        let rule = rule_of(&error);
        assert_eq!(
            (error.file.as_str(), rule.section, rule.index),
            ("shop.toml", "reference", 1)
        );
        assert!((4..=5).contains(&rule.line), "{error}");
        assert_eq!(error.field.as_deref(), Some("to"));
        assert!(
            error.message.contains("expected `table.column`, got `c`"),
            "{error}"
        );
        let prefix = format!("shop.toml: [[reference]] #1 (line {}): `to`: ", rule.line);
        assert!(error.to_string().starts_with(&prefix), "{error}");
    }

    #[test]
    fn error_counts_rules_per_section_from_one() {
        let error = rejected(
            "[[reference]]\nfrom = \"a.b\"\nto = \"c.d\"\n\n[[reference]]\nfrom = \"a.b\"\nto = \"c\"\n",
        );
        let rule = rule_of(&error);
        assert_eq!((rule.section, rule.index), ("reference", 2));
        assert!((8..=9).contains(&rule.line), "{error}");
    }

    #[test]
    fn invalid_rules_are_rejected_with_their_section_and_reason() {
        let cases: [(&str, &str, Option<&str>, &str); 9] = [
            (
                "[[reference]]\nfrom = \"a.b\"\nto = \"c\"\n",
                "reference",
                Some("to"),
                "expected `table.column`, got `c`",
            ),
            (
                "[[reference]]\nfrom = \"a.b\"\nwhen = { k = { glib = \"x\" } }\nto = \"c.d\"\n",
                "reference",
                Some("when.k"),
                "unknown condition `glib`, expected glob, like, regexp or not",
            ),
            (
                "[[reference]]\nfrom = \"a.b\"\nwhen = { k = 1.5 }\nto = \"c.d\"\n",
                "reference",
                Some("when.k"),
                "expected a string or an integer, got float",
            ),
            (
                "[[kind]]\ncolumns = [\"a.b\"]\nsecrect = true\n",
                "kind",
                None,
                "secrect",
            ),
            (
                "[[style]]\ncolumns = [\"a.b\"]\nmatch = \"=\"\nvalue = \"x\"\ncolor = \"pink\"\n",
                "style",
                Some("color"),
                "pink",
            ),
            (
                "[[kind]]\ncolumns = [\"a.b\"]\nsecret = true\nnumber = true\n",
                "kind",
                None,
                "a kind rule sets exactly one of",
            ),
            (
                "[[kind]]\ncolumns = [\"a.b\"]\n",
                "kind",
                None,
                "a kind rule sets exactly one of",
            ),
            (
                "[[kind]]\ncolumns = [\"a.b\"]\nin = [\"m\"]\nsecret = true\n",
                "kind",
                None,
                "`in` narrows `meta_keys`, which is empty",
            ),
            (
                "[[meta]]\ntable = \"m\"\nowner = \"o\"\nkey = \"k\"\nvalue = \"v\"\n",
                "meta",
                Some("fk"),
                "`owner` needs `fk`",
            ),
        ];
        for (body, section, field, needle) in cases {
            let error = rejected(body);
            assert_eq!(rule_of(&error).section, section, "{body}");
            assert_eq!(error.field.as_deref(), field, "{body}");
            assert!(error.message.contains(needle), "{body}: {error}");
        }
    }

    #[test]
    fn fk_without_owner_is_rejected() {
        let error = rejected("[[meta]]\ntable = \"m\"\nfk = \"f\"\nkey = \"k\"\nvalue = \"v\"\n");
        assert_eq!(error.field.as_deref(), Some("owner"));
        assert_eq!(error.message, "`fk` needs `owner`");
    }

    #[test]
    fn unknown_section_is_rejected_without_a_rule() {
        let error = rejected("[[bogus]]\nname = \"x\"\n");
        assert_eq!(error.rule, None);
        assert!(error.message.contains("bogus"), "{error}");
    }

    #[test]
    fn error_in_the_preset_header_names_the_header() {
        let error = parse("shop.toml", "[preset]\nname = 1\n").unwrap_err();
        let rule = rule_of(&error);
        assert_eq!((rule.section, rule.index), ("preset", 1));
        assert!((1..=2).contains(&rule.line), "{error}");
        assert!(
            error
                .to_string()
                .starts_with(&format!("shop.toml: [preset] (line {}): ", rule.line)),
            "{error}"
        );
    }

    #[test]
    fn toml_syntax_error_keeps_its_line() {
        let error = parse("shop.toml", "[preset]\nname = \"t\"\n[[reference]\n").unwrap_err();
        assert_eq!(error.rule, None);
        assert!(error.message.contains("line 3"), "{error}");
    }

    #[test]
    fn id_placeholder_is_rejected_where_there_is_no_row() {
        let pinned = rejected("[[pinned]]\nid = \"a\"\nsql = \"SELECT {id}\"\n");
        assert_eq!(rule_of(&pinned).section, "pinned");
        assert_eq!(pinned.field.as_deref(), Some("sql"));
        assert_eq!(pinned.message, "`{id}` is not available here");

        let action = rejected(
            "[[action]]\nname = \"a\"\ntable = \"t\"\nscope = \"table\"\ndelete = { where = \"id = {id}\" }\n",
        );
        assert_eq!(rule_of(&action).section, "action");
        assert_eq!(action.field.as_deref(), Some("delete.where"));
        assert_eq!(action.message, "`{id}` is not available here");
    }

    #[test]
    fn param_placeholder_needs_a_declared_param() {
        let error = rejected(
            "[[action]]\nname = \"a\"\ntable = \"t\"\nupdate = [{ set = { status = \"{param.x}\" } }]\n",
        );
        assert_eq!(error.field.as_deref(), Some("update[0].set.status"));
        assert_eq!(error.message, "`{param.x}` names no declared param");
    }

    #[test]
    fn declared_param_is_accepted() {
        let preset = parsed(
            "[[action]]\nname = \"a\"\ntable = \"t\"\nparams = [{ name = \"x\" }]\nupdate = [{ set = { status = \"{param.x}\" } }]\n",
        );
        assert_eq!(preset.actions.len(), 1);
    }

    #[test]
    fn braces_that_are_regexp_quantifiers_are_left_alone() {
        let preset =
            parsed("[[pinned]]\nid = \"a\"\nsql = \"SELECT 1 WHERE x REGEXP '^[0-9a-f]{32}$'\"\n");
        assert_eq!(
            preset.pinned[0].sql,
            "SELECT 1 WHERE x REGEXP '^[0-9a-f]{32}$'"
        );
    }

    #[test]
    fn preset_cannot_require_itself() {
        let error = parse(
            "shop.toml",
            "[preset]\nname = \"shop\"\nrequires = [\"shop\"]\n",
        )
        .unwrap_err();
        assert_eq!(rule_of(&error).section, "preset");
        assert_eq!(error.field.as_deref(), Some("requires"));
        assert_eq!(error.message, "a preset cannot require itself");
    }

    #[test]
    fn pinned_ids_are_unique() {
        let error = rejected(
            "[[pinned]]\nid = \"a\"\nsql = \"SELECT 1\"\n\n[[pinned]]\nid = \"a\"\nsql = \"SELECT 2\"\n",
        );
        let rule = rule_of(&error);
        assert_eq!((rule.section, rule.index), ("pinned", 2));
        assert_eq!(error.field.as_deref(), Some("id"));
        assert_eq!(error.message, "`a` is used twice");
    }

    #[test]
    fn action_names_are_unique_per_table() {
        let one = "[[action]]\nname = \"x\"\ntable = \"t\"\nupdate = [{ set = { a = \"1\" } }]\n";
        let error = rejected(&format!("{one}\n{one}"));
        let rule = rule_of(&error);
        assert_eq!((rule.section, rule.index), ("action", 2));
        assert_eq!(error.field.as_deref(), Some("name"));
        assert_eq!(error.message, "`x on t` is used twice");

        let other_table = one.replace("table = \"t\"", "table = \"u\"");
        let preset = parsed(&format!("{one}\n{other_table}"));
        assert_eq!(preset.actions.len(), 2);
    }

    #[test]
    fn anomaly_open_pinned_must_exist_in_the_preset() {
        let error = rejected(
            "[[anomaly]]\nid = \"a\"\nsql = \"SELECT 1\"\ntext = \"t\"\nopen = { pinned = \"nope\" }\n",
        );
        assert_eq!(rule_of(&error).section, "anomaly");
        assert_eq!(error.field.as_deref(), Some("open.pinned"));
        assert_eq!(error.message, "no [[pinned]] with id `nope` in this preset");
    }

    #[test]
    fn anomaly_fix_must_be_a_table_scope_action_of_the_preset() {
        let anomaly =
            "[[anomaly]]\nid = \"a\"\nsql = \"SELECT 1\"\ntext = \"t\"\nfix = \"purge\"\n";
        let message = "no [[action]] `purge` with scope = \"table\" in this preset";

        let missing = rejected(anomaly);
        assert_eq!(missing.field.as_deref(), Some("fix"));
        assert_eq!(missing.message, message);

        let row_action = "[[action]]\nname = \"purge\"\ntable = \"t\"\ndelete = {}\n";
        let wrong_scope = rejected(&format!("{anomaly}\n{row_action}"));
        assert_eq!(wrong_scope.field.as_deref(), Some("fix"));
        assert_eq!(wrong_scope.message, message);

        let table_action = "[[action]]\nname = \"purge\"\ntable = \"t\"\nscope = \"table\"\ndelete = { where = \"x = 1\" }\n";
        let preset = parsed(&format!("{anomaly}\n{table_action}"));
        assert_eq!(preset.anomalies[0].fix.as_deref(), Some("purge"));
    }

    #[test]
    fn table_scope_actions_select_their_rows_by_where() {
        let message = "a table action selects its rows by `where`";
        let delete =
            rejected("[[action]]\nname = \"a\"\ntable = \"t\"\nscope = \"table\"\ndelete = {}\n");
        assert_eq!(delete.field.as_deref(), Some("delete.where"));
        assert_eq!(delete.message, message);

        let update = rejected(
            "[[action]]\nname = \"a\"\ntable = \"t\"\nscope = \"table\"\nupdate = [{ set = { a = \"1\" } }]\n",
        );
        assert_eq!(update.field.as_deref(), Some("update[0].where"));
        assert_eq!(update.message, message);
    }

    #[test]
    fn limit_goes_only_with_show_rows() {
        for show in ["count", "value"] {
            let body = format!(
                "[[pinned]]\nid = \"a\"\nsql = \"SELECT 1\"\nhome = {{ show = \"{show}\", limit = 3 }}\n"
            );
            let error = rejected(&body);
            assert!(
                error.message.contains("`limit` goes with show = \"rows\""),
                "{show}: {error}"
            );
        }
    }

    #[test]
    fn preview_sql_goes_only_with_sql() {
        let error = rejected(
            "[[action]]\nname = \"a\"\ntable = \"t\"\ndelete = {}\npreview_sql = \"SELECT 1\"\n",
        );
        assert_eq!(rule_of(&error).section, "action");
        assert!(
            error.message.contains("`preview_sql` goes with `sql`"),
            "{error}"
        );
    }

    #[test]
    fn default_reference_ids_follow_the_source_and_condition() {
        let preset = parsed(
            r#"
[[reference]]
from = "m.v"
when = { k = "a" }
to = "u.id"

[[reference]]
from = "m.v"
when = { k = ["b", "c"] }
to = "u.id"

[[reference]]
from = "m.v"
when = { k = { not = { glob = "x*" } }, j = { like = "%y" } }
to = "u.id"

[[reference]]
from = "m.plain"
to = "u.id"
"#,
        );
        let ids: Vec<&str> = preset.references.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "m.v?k=a",
                "m.v?k=[b,c]",
                "m.v?j=like(%y)&k!=glob(x*)",
                "m.plain"
            ]
        );
    }

    #[test]
    fn colliding_default_ids_get_the_target_appended() {
        let preset = parsed(
            r#"
[[reference]]
from = "i.order_id"
to = "orders.id"

[[reference]]
from = "i.order_id"
to = "posts.id"

[[reference]]
from = "i.other"
to = "orders.id"
"#,
        );
        let ids: Vec<&str> = preset.references.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            ["i.order_id->orders.id", "i.order_id->posts.id", "i.other"]
        );
    }

    #[test]
    fn explicit_reference_id_is_kept() {
        let preset = parsed(
            r#"
[[reference]]
id = "custom"
from = "i.order_id"
to = "orders.id"

[[reference]]
from = "i.order_id"
to = "posts.id"
"#,
        );
        let ids: Vec<&str> = preset.references.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["custom", "i.order_id"]);
    }

    #[test]
    fn conditions_parse_every_match_shape() {
        let preset = parsed(
            r#"
[[reference]]
from = "m.v"
to = "u.id"
when = { eq = "x", num = 5, in = ["p", "q"], glob = { glob = "g*" }, like = { like = "%l%" }, re = { regexp = "^[0-9]+$" }, not = { not = "n" } }
"#,
        );
        assert_eq!(
            preset.references[0].when,
            conditions([
                ("eq", Match::Eq("x".into())),
                ("num", Match::Eq("5".into())),
                ("in", Match::In(strings(&["p", "q"]))),
                ("glob", Match::Glob { glob: "g*".into() }),
                ("like", Match::Like { like: "%l%".into() }),
                (
                    "re",
                    Match::Regexp {
                        regexp: "^[0-9]+$".into()
                    }
                ),
                (
                    "not",
                    Match::Not {
                        not: Box::new(Match::Eq("n".into()))
                    }
                ),
            ])
        );
    }

    #[test]
    fn reference_carries_target_condition_label_and_columns() {
        let preset = parsed(
            r#"
[[reference]]
id = "same_email"
from = "orders.email"
when = { customer_id = "0" }
to = "orders.email"
to_when = { customer_id = "0" }
label = "same email"
columns = ["id", "status", "_qty"]
"#,
        );
        let reference = &preset.references[0];
        assert_eq!(
            reference.to_when,
            conditions([("customer_id", Match::Eq("0".into()))])
        );
        assert_eq!(reference.label.as_deref(), Some("same email"));
        assert_eq!(reference.columns, strings(&["id", "status", "_qty"]));
    }

    #[test]
    fn pattern_parses_owner_condition_and_placeholders() {
        let preset = parsed(
            r#"
[[pattern]]
name = "log"
owner = "posts"
owner_when = { post_type = ["order", "refund"] }
table = "log"
where = "id = {id} AND d > {owner.created} AND m = {meta.key}"
order = "created"
limit = 100
"#,
        );
        let pattern = &preset.patterns[0];
        assert_eq!(
            pattern.owner_when,
            conditions([("post_type", Match::In(strings(&["order", "refund"])))])
        );
        assert_eq!(pattern.order.as_deref(), Some("created"));
        assert_eq!(pattern.limit, Some(100));
    }

    #[test]
    fn entities_parse_with_and_without_a_filter() {
        let preset = parsed(
            r#"
[[entity]]
name = "post"
table = "posts"

[[entity]]
name = "order"
table = "posts"
filter = "post_type = 'order'"
summary = ["post_status", "_total"]
created = "post_date_gmt"
search = ["post_title"]
search_exact = ["_txn_id"]
people = true
"#,
        );
        assert_eq!(preset.entities[0].filter, None);
        assert!(!preset.entities[0].people);
        let order = &preset.entities[1];
        assert_eq!(order.filter.as_deref(), Some("post_type = 'order'"));
        assert_eq!(order.summary, strings(&["post_status", "_total"]));
        assert_eq!(order.created.as_deref(), Some("post_date_gmt"));
        assert_eq!(order.search, strings(&["post_title"]));
        assert_eq!(order.search_exact, strings(&["_txn_id"]));
        assert!(order.people);
    }

    #[test]
    fn kinds_parse_money_with_and_without_currency() {
        let preset = parsed(
            r#"
[[kind]]
columns = ["orders.total"]
money = { currency = ["currency", "_currency"] }

[[kind]]
meta_keys = ["_price"]
in = ["postmeta"]
money = {}
"#,
        );
        assert_eq!(
            preset.kinds[0].value,
            ValueRule::Money(Money {
                currency: strings(&["currency", "_currency"])
            })
        );
        assert_eq!(
            preset.kinds[0].columns,
            [ColumnGlob {
                table: "orders".into(),
                column: "total".into()
            }]
        );
        assert_eq!(preset.kinds[1].value, ValueRule::Money(Money::default()));
        assert_eq!(preset.kinds[1].meta_keys, strings(&["_price"]));
        assert_eq!(preset.kinds[1].in_meta, strings(&["postmeta"]));
    }

    #[test]
    fn kinds_parse_the_three_timestamp_zones_and_the_flags() {
        let preset = parsed(
            r#"
[[kind]]
columns = ["*.*_gmt"]
timestamp = "utc"

[[kind]]
columns = ["a.b"]
timestamp = "local"

[[kind]]
meta_keys = ["_paid_at"]
timestamp = "unix"

[[kind]]
columns = ["a.status"]
enum = true

[[kind]]
columns = ["a.pass"]
secret = true

[[kind]]
columns = ["a.qty"]
number = true
"#,
        );
        let values: Vec<&ValueRule> = preset.kinds.iter().map(|k| &k.value).collect();
        assert_eq!(
            values,
            [
                &ValueRule::Timestamp(Zone::Utc),
                &ValueRule::Timestamp(Zone::Local),
                &ValueRule::Timestamp(Zone::Unix),
                &ValueRule::Enum,
                &ValueRule::Secret,
                &ValueRule::Number,
            ]
        );
        assert_eq!(
            preset.kinds[0].columns,
            [ColumnGlob {
                table: "*".into(),
                column: "*_gmt".into()
            }]
        );
    }

    #[test]
    fn styles_default_to_enabled() {
        let preset = parsed(
            r#"
[[style]]
columns = ["orders.status"]
match = "="
value = "failed"
color = "red"

[[style]]
meta_keys = ["_x"]
match = "REGEXP"
value = "^a"
color = "purple"
enabled = false
"#,
        );
        assert!(preset.styles[0].enabled);
        assert_eq!(preset.styles[0].matching, StyleMatch::Eq);
        assert_eq!(preset.styles[0].color, StyleColor::Red);
        assert!(!preset.styles[1].enabled);
        assert_eq!(preset.styles[1].matching, StyleMatch::Regexp);
    }

    #[test]
    fn meta_tables_parse_with_and_without_an_owner() {
        let preset = parsed(
            r#"
[[meta]]
table = "order_meta"
owner = "orders"
fk = "order_id"
key = "meta_key"
value = "meta_value"

[[meta]]
table = "options"
key = "option_name"
value = "option_value"
"#,
        );
        assert_eq!(preset.meta[0].owner.as_deref(), Some("orders"));
        assert_eq!(preset.meta[0].fk.as_deref(), Some("order_id"));
        assert_eq!(preset.meta[1].owner, None);
        assert_eq!(preset.meta[1].fk, None);
    }

    #[test]
    fn pinned_parses_the_three_home_forms_and_none() {
        let preset = parsed(
            r#"
[[pinned]]
id = "a"
sql = "SELECT 1"
home = { show = "rows", limit = 5 }

[[pinned]]
id = "b"
sql = "SELECT 1"
home = { show = "rows" }

[[pinned]]
id = "c"
sql = "SELECT 1"
home = { show = "count" }

[[pinned]]
id = "d"
sql = "SELECT 1"
home = { show = "value" }

[[pinned]]
id = "e"
title = "{p}orders"
sql = "SELECT 1"
"#,
        );
        let homes: Vec<Option<&Home>> = preset.pinned.iter().map(|p| p.home.as_ref()).collect();
        assert_eq!(
            homes,
            [
                Some(&Home::Rows { limit: Some(5) }),
                Some(&Home::Rows { limit: None }),
                Some(&Home::Count),
                Some(&Home::Value),
                None,
            ]
        );
        assert_eq!(preset.pinned[4].title.as_deref(), Some("{p}orders"));
    }

    #[test]
    fn anomaly_parses_both_open_forms() {
        let preset = parsed(
            r#"
[[pinned]]
id = "p1"
sql = "SELECT 1"

[[anomaly]]
id = "a"
sql = "SELECT 0"
text = "t"
open = { pinned = "p1" }

[[anomaly]]
id = "b"
sql = "SELECT 0"
text = "t"
open = { table = "jobs", where = "status = 'failed'" }

[[anomaly]]
id = "c"
sql = "SELECT 0"
text = "t"
open = { table = "jobs" }
"#,
        );
        assert_eq!(
            preset.anomalies[0].open,
            Some(AnomalyOpen::Pinned {
                pinned: "p1".into()
            })
        );
        assert_eq!(
            preset.anomalies[1].open,
            Some(AnomalyOpen::Table {
                table: "jobs".into(),
                r#where: Some("status = 'failed'".into())
            })
        );
        assert_eq!(
            preset.anomalies[2].open,
            Some(AnomalyOpen::Table {
                table: "jobs".into(),
                r#where: None
            })
        );
    }

    #[test]
    fn update_action_runs_several_tables_and_takes_sql_values() {
        let preset = parsed(
            r#"
[[action]]
name = "set_status"
table = "orders"
when = { status = ["pending"] }
params = [{ name = "status", choices_sql = "SELECT DISTINCT status FROM {p}orders", free = true }]
update = [
  { set = { status = "{param.status}", updated = { sql = "UTC_TIMESTAMP()" } } },
  { table = "posts", key = "id", where = "type = 'order'", set = { status = "{param.status}" } },
]
"#,
        );
        let action = &preset.actions[0];
        assert_eq!(action.scope, ActionScope::Row);
        assert_eq!(
            action.when,
            conditions([("status", Match::In(strings(&["pending"])))])
        );
        assert_eq!(
            action.params,
            [Param {
                name: "status".into(),
                choices_sql: Some("SELECT DISTINCT status FROM {p}orders".into()),
                free: true,
                r#type: ParamType::Text,
                default: None,
                optional: false,
            }]
        );
        assert_eq!(
            action.effect,
            ActionEffect::Update(vec![
                UpdateStep {
                    table: None,
                    key: None,
                    r#where: None,
                    set: set([
                        ("status", text("{param.status}")),
                        ("updated", sql("UTC_TIMESTAMP()")),
                    ]),
                },
                UpdateStep {
                    table: Some("posts".into()),
                    key: Some("id".into()),
                    r#where: Some("type = 'order'".into()),
                    set: set([("status", text("{param.status}"))]),
                },
            ])
        );
    }

    #[test]
    fn delete_action_with_table_scope() {
        let preset = parsed(
            "[[action]]\nname = \"purge\"\ntable = \"jobs\"\nscope = \"table\"\ndelete = { where = \"status = 'failed'\" }\n",
        );
        let action = &preset.actions[0];
        assert_eq!(action.scope, ActionScope::Table);
        assert_eq!(
            action.effect,
            ActionEffect::Delete(DeleteSpec {
                r#where: Some("status = 'failed'".into())
            })
        );
    }

    #[test]
    fn sql_action_with_preview_and_typed_params() {
        let preset = parsed(
            r#"
[[action]]
name = "trim"
table = "log"
scope = "table"
params = [
  { name = "days", type = "int", default = "30" },
  { name = "source", optional = true },
]
sql = "DELETE FROM {p}log WHERE d < NOW() - INTERVAL {param.days} DAY AND ({param.source} IS NULL OR s = {param.source})"
preview_sql = "SELECT COUNT(*) FROM {p}log WHERE d < NOW() - INTERVAL {param.days} DAY"
"#,
        );
        let action = &preset.actions[0];
        assert_eq!(action.params[0].r#type, ParamType::Int);
        assert_eq!(action.params[0].default.as_deref(), Some("30"));
        assert!(action.params[1].optional);
        assert_eq!(
            action.effect,
            ActionEffect::Sql {
                sql: "DELETE FROM {p}log WHERE d < NOW() - INTERVAL {param.days} DAY AND ({param.source} IS NULL OR s = {param.source})".into(),
                preview_sql: Some("SELECT COUNT(*) FROM {p}log WHERE d < NOW() - INTERVAL {param.days} DAY".into()),
            }
        );
    }

    #[test]
    fn find_sources_default_to_exact_matching() {
        let preset = parsed(
            r#"
[[find]]
table = "log"
columns = ["message"]
match = "contains"

[[find]]
table = "orders"
columns = ["email"]
"#,
        );
        assert_eq!(preset.finds[0].matching, FindMatch::Contains);
        assert_eq!(preset.finds[1].matching, FindMatch::Exact);
    }

    #[test]
    fn find_source_without_columns_is_rejected() {
        let error = rejected("[[find]]\ntable = \"log\"\ncolumns = []\n");
        assert_eq!(error.field.as_deref(), Some("columns"));
        assert_eq!(error.message, "names no column");
    }

    #[test]
    fn header_fields_are_read() {
        let preset = parse(
            "t.toml",
            "[preset]\nname = \"shop\"\nrequires = [\"blog\"]\nmarker_tables = [\"orders\"]\nnetwork_tables = [\"users\"]\n",
        )
        .unwrap();
        assert_eq!(preset.name, "shop");
        assert_eq!(preset.requires, strings(&["blog"]));
        assert_eq!(preset.marker_tables, strings(&["orders"]));
        assert_eq!(preset.network_tables, strings(&["users"]));
    }

    #[test]
    fn to_toml_parses_back_to_an_equal_preset() {
        let preset = parse("t.toml", ROUND_TRIP).unwrap_or_else(|e| panic!("{e}"));
        let written = to_toml(&preset).unwrap();
        let again = parse("t.toml", &written).unwrap_or_else(|e| panic!("{e}\n{written}"));
        assert_eq!(again, preset);
    }

    fn built_in(file: &str) -> Preset {
        let (_, src) = BUILT_IN
            .iter()
            .find(|(name, _)| *name == file)
            .unwrap_or_else(|| panic!("no built-in {file}"));
        parse(file, src).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn built_in_presets_parse_and_round_trip() {
        assert_eq!(
            BUILT_IN.iter().map(|(file, _)| *file).collect::<Vec<_>>(),
            [
                "wordpress.toml",
                "woocommerce.toml",
                "action-scheduler.toml"
            ]
        );
        for (file, src) in BUILT_IN {
            let preset = parse(file, src).unwrap_or_else(|e| panic!("{e}"));
            let written = to_toml(&preset).unwrap();
            let again = parse(file, &written).unwrap_or_else(|e| panic!("{file}: {e}\n{written}"));
            assert_eq!(again, preset, "{file}");
        }
    }

    #[test]
    fn built_in_presets_name_what_they_need_and_where_they_apply() {
        let cases = [
            (
                "wordpress.toml",
                "wordpress",
                vec![],
                vec!["options", "posts", "users"],
            ),
            (
                "woocommerce.toml",
                "woocommerce",
                vec!["wordpress"],
                vec!["woocommerce_order_items"],
            ),
            (
                "action-scheduler.toml",
                "action-scheduler",
                vec!["wordpress"],
                vec!["actionscheduler_actions"],
            ),
        ];
        for (file, name, requires, markers) in cases {
            let preset = built_in(file);
            assert_eq!(preset.name, name, "{file}");
            assert_eq!(preset.requires, requires, "{file}");
            assert_eq!(preset.marker_tables, markers, "{file}");
        }
    }

    #[test]
    fn woocommerce_disambiguates_the_item_references_to_both_order_shapes() {
        let preset = built_in("woocommerce.toml");
        let ids: Vec<&str> = preset.references.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"woocommerce_order_items.order_id->wc_orders.id"));
        assert!(ids.contains(&"woocommerce_order_items.order_id->posts.ID"));
        assert!(ids.contains(&"guest_orders"));
    }

    #[test]
    fn wordpress_declares_its_meta_tables_with_their_owners() {
        let preset = built_in("wordpress.toml");
        let owners: Vec<(&str, Option<&str>, Option<&str>)> = preset
            .meta
            .iter()
            .map(|m| (m.table.as_str(), m.owner.as_deref(), m.fk.as_deref()))
            .collect();
        assert_eq!(
            owners,
            [
                ("postmeta", Some("posts"), Some("post_id")),
                ("usermeta", Some("users"), Some("user_id")),
                ("commentmeta", Some("comments"), Some("comment_id")),
                ("termmeta", Some("terms"), Some("term_id")),
                ("sitemeta", Some("site"), Some("site_id")),
                ("blogmeta", Some("blogs"), Some("blog_id")),
                ("options", None, None),
            ]
        );
    }

    const ROUND_TRIP: &str = r#"
[preset]
name = "shop"
requires = ["blog"]
marker_tables = ["orders"]
network_tables = ["users"]

[[meta]]
table = "order_meta"
owner = "orders"
fk = "order_id"
key = "meta_key"
value = "meta_value"

[[meta]]
table = "options"
key = "option_name"
value = "option_value"

[[reference]]
from = "orders.customer_id"
to = "users.id"

[[reference]]
from = "order_meta.meta_value"
when = { meta_key = ["_a", "_b"], other = { not = { glob = "x*" } } }
to = "users.id"
to_when = { kind = { regexp = "^[0-9]+$" } }
label = "buyers"
columns = ["id", "_qty"]

[[entity]]
name = "order"
table = "posts"
filter = "post_type = 'order'"
summary = ["post_status"]
created = "post_date_gmt"
search = ["post_title"]
search_exact = ["_txn_id"]
people = true

[[kind]]
columns = ["orders.total"]
money = { currency = ["currency"] }

[[kind]]
meta_keys = ["_price"]
in = ["order_meta"]
money = {}

[[kind]]
columns = ["*.*_gmt"]
timestamp = "utc"

[[kind]]
columns = ["a.status"]
enum = true

[[style]]
columns = ["orders.status"]
match = "LIKE"
value = "fail%"
color = "red"
enabled = false

[[pattern]]
name = "log"
owner = "orders"
owner_when = { type = "order" }
table = "log"
where = "message LIKE CONCAT('%', {meta._txn_id}, '%') AND id = {id}"
order = "created"
limit = 100

[[pinned]]
id = "orders"
title = "{p}orders"
sql = "SELECT id FROM {p}orders"
home = { show = "rows", limit = 5 }

[[pinned]]
id = "count"
sql = "SELECT COUNT(*) FROM {p}orders"
home = { show = "count" }

[[anomaly]]
id = "failed"
sql = "SELECT COUNT(*) FROM {p}jobs WHERE status = 'failed'"
text = "{p}jobs failed"
open = { table = "jobs", where = "status = 'failed'" }
fix = "purge"

[[anomaly]]
id = "slow"
sql = "SELECT 0"
text = "slow"
open = { pinned = "orders" }

[[action]]
name = "set_status"
table = "orders"
when = { status = ["pending"] }
params = [{ name = "status", choices_sql = "SELECT DISTINCT status FROM {p}orders", free = true }]
update = [
  { set = { status = "{param.status}", updated = { sql = "UTC_TIMESTAMP()" } } },
  { table = "posts", key = "id", where = "type = 'order'", set = { status = "{param.status}" } },
]

[[action]]
name = "purge"
table = "jobs"
scope = "table"
delete = { where = "status = 'failed'" }

[[action]]
name = "trim"
table = "log"
scope = "table"
params = [{ name = "days", type = "int", default = "30", optional = true }]
sql = "DELETE FROM {p}log WHERE d < NOW() - INTERVAL {param.days} DAY"
preview_sql = "SELECT COUNT(*) FROM {p}log"

[[find]]
table = "log"
columns = ["message"]
match = "contains"
"#;
}
