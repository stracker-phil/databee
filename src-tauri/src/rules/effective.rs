//! The effective rules of one database: the applied presets ⊕ the project, with real table names
//! and compiled conditions (docs/presets.md, "Effective rules").
//!
//! - A project reference with the same id, or the same `from` and `when`, as a preset reference
//!   replaces it.
//! - `disabled` drops project and preset references by id (any preset) and inferred references
//!   (`inferred_id`).
//! - Rules are kept in precedence order: project rules first, then the presets from the last
//!   applied to the first (a preset refines the presets it requires).
//! - Rules whose tables do not exist are dropped: presets describe optional tables too.

use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use ts_rs::TS;

use super::ProjectsStore;
use super::actions::Action;
use super::applied::Applied;
use super::catalog::{PresetCatalog, PresetEntry, loaded};
use super::condition::{self, Condition, Matcher, column_glob_match, glob_match};
use super::icons::{self, TableIcon};
use super::kinds::{KindRule, StyleRule};
use super::project::Project;
use super::queries::Pattern;
use super::types::{ColumnGlob, ColumnRef, Entity, FindSource, Preset, Reference};
use crate::db::ColumnEquals;
use crate::error::AppResult;
use crate::lens::{ColumnsRef, ResolvedReference};
use crate::lock::MutexExt;
use crate::schema::{self, DatabaseSchema};
use crate::session::Session;
use crate::value::ReferenceSource;

#[derive(Clone, Copy)]
pub struct RuleSources<'a> {
    pub presets: &'a PresetCatalog,
    pub projects: &'a ProjectsStore,
}

/// Rules of one source with the same `from` and `when` (one column pointing at two tables) share
/// it, so they become one reference with several targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupKey {
    /// `None` for the project.
    pub preset: Option<String>,
    /// The rule's default id: its `from` and `when`.
    pub rule: String,
}

#[derive(Debug, Clone)]
pub struct RuleReference {
    pub by: ReferenceSource,
    pub group: GroupKey,
    pub from: ColumnRef,
    pub when: Vec<Condition>,
    pub to: ColumnRef,
    /// On the target row; coverage joins on all of them.
    pub to_when: Vec<Condition>,
    /// The `=` conditions of `to_when`, which the target's address carries as a filter.
    pub to_filter: Vec<ColumnEquals>,
    /// Names the incoming group on the target's document.
    pub label: Option<String>,
    /// The incoming group's columns: source columns, or keys of the source's meta table.
    pub columns: Vec<String>,
}

impl RuleReference {
    pub fn resolved(&self) -> ResolvedReference {
        ResolvedReference {
            from: ColumnsRef::new(&self.from.table, vec![self.from.column.clone()]),
            to: ColumnsRef::new(&self.to.table, vec![self.to.column.clone()]),
            by: self.by.clone(),
        }
    }
}

/// A `[[pattern]]` of an applied preset, with real table names and `owner_when` compiled.
#[derive(Debug, Clone)]
pub struct RulePattern {
    /// `owner` and `table` are real table names; `where` is as written.
    pub rule: Pattern,
    pub owner_when: Vec<Condition>,
}

#[derive(Debug, Clone)]
pub struct MetaRef {
    pub table: String,
    /// The name rules use (`in`).
    pub name: String,
    /// For an owned table: the real owner table and the column holding the owner's key.
    pub owner: Option<String>,
    pub fk: Option<String>,
    pub key: String,
    pub value: String,
}

/// An `[[action]]` of an applied preset: `table` is the real table name, `when` compiled.
#[derive(Debug, Clone)]
pub struct RuleAction {
    pub preset: String,
    pub rule: Action,
    pub when: Vec<Condition>,
}

/// A value style that is switched on, ready to match.
#[derive(Debug, Clone)]
pub struct RuleStyle {
    pub rule: StyleRule,
    pub matcher: Matcher,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct AppliedEntity {
    pub preset: String,
    /// `table` is the real table name.
    pub entity: Entity,
}

#[derive(Debug, Clone)]
pub struct EffectiveRules {
    pub applied: Applied,
    pub references: Vec<RuleReference>,
    pub kinds: Vec<KindRule>,
    pub styles: Vec<RuleStyle>,
    pub meta: Vec<MetaRef>,
    /// Filtered entities before unfiltered ones, so a row takes the first whose filter holds.
    pub entities: Vec<AppliedEntity>,
    /// In apply order.
    pub patterns: Vec<RulePattern>,
    /// In apply order.
    pub actions: Vec<RuleAction>,
    /// `[[find]]` sources with real table names, in apply order.
    pub finds: Vec<FindSource>,
    pub disabled: HashSet<String>,
    /// The icon of each real table that has one (`icons::table_icons`).
    pub icons: BTreeMap<String, TableIcon>,
}

/// The id that disables the same-name inference of `table.column` (real names).
pub fn inferred_id(table: &str, column: &str) -> String {
    format!("inferred:{table}.{column}")
}

impl EffectiveRules {
    pub fn build(project: &Project, schema: &DatabaseSchema, loaded: &[Arc<Preset>]) -> Self {
        let applied = Applied::new(project, schema, loaded);
        let disabled: HashSet<String> = project.disabled.iter().cloned().collect();
        let icons = icons::table_icons(&applied, &project.icons, schema);
        let mut rules = Self {
            applied,
            references: Vec::new(),
            kinds: project.kinds.clone(),
            styles: Vec::new(),
            meta: Vec::new(),
            entities: Vec::new(),
            patterns: Vec::new(),
            actions: Vec::new(),
            finds: Vec::new(),
            disabled,
            icons,
        };
        rules.add_styles(project);
        let (own_on, own_off): (Vec<Reference>, Vec<Reference>) = project
            .references
            .iter()
            .cloned()
            .partition(|r| !rules.disabled.contains(&r.id));
        rules.add_references(&own_on, None);
        rules.keep_inference_off(&own_off);
        let preset_rules: Vec<_> = rules.preset_reference_rules(schema).collect();
        for (preset, references) in preset_rules {
            let kept: Vec<Reference> = references
                .into_iter()
                .filter(|r| !rules.disabled.contains(&r.id) && !is_overridden(project, r))
                .collect();
            rules.add_references(&kept, Some(&preset.name));
            rules.kinds.extend(preset.kinds.iter().cloned());
        }
        let presets = rules.applied.presets.clone();
        for preset in &presets {
            rules.add_preset_tables(preset);
        }
        // Stable, so file order holds among filtered and among unfiltered entities.
        rules.entities.sort_by_key(|e| e.entity.filter.is_none());
        rules
    }

    /// The rule behind `reference`: the same source, `from` and `to`; `None` for declared and
    /// inferred references.
    pub fn rule_for(&self, reference: &ResolvedReference) -> Option<&RuleReference> {
        let ([from], [to]) = (
            reference.from.columns.as_slice(),
            reference.to.columns.as_slice(),
        ) else {
            return None;
        };
        self.references.iter().find(|r| {
            r.by == reference.by
                && r.from.is(&reference.from.table, from)
                && r.to.is(&reference.to.table, to)
        })
    }

    /// Whether the rules change nothing the schema lens shows.
    pub fn is_empty(&self) -> bool {
        self.references.is_empty()
            && self.kinds.is_empty()
            && self.styles.is_empty()
            && self.disabled.is_empty()
    }

    /// The project's own styles that are on, then the applied presets' styles (the last applied
    /// first) the project switches on, each replaced by its project override.
    fn add_styles(&mut self, project: &Project) {
        let presets = &self.applied.presets;
        let shipped = |id: &str| presets.iter().any(|p| p.styles.iter().any(|s| s.id == id));
        let own = project
            .styles
            .iter()
            .filter(|s| s.enabled && !shipped(&s.id));
        let switched_on = presets
            .iter()
            .rev()
            .flat_map(|p| &p.styles)
            .filter(|s| project.preset_style_on(s))
            .map(|s| project.style_override(&s.id).unwrap_or(s));
        self.styles = own.chain(switched_on).filter_map(to_rule_style).collect();
    }

    /// A switched-off unconditional project reference still replaces the same-name inference on
    /// its column, so switching off an edited inferred reference switches the reference off.
    fn keep_inference_off(&mut self, switched_off: &[Reference]) {
        for rule in switched_off.iter().filter(|r| r.when.is_empty()) {
            if let Some(from) = self.column(&rule.from) {
                self.disabled.insert(inferred_id(&from.table, &from.column));
            }
        }
    }

    /// The references of `preset`, or of the project (`None`). Conditional rules of a source
    /// come before its unconditional ones, which hold for every row and would hide them.
    fn add_references(&mut self, references: &[Reference], preset: Option<&str>) {
        let mut resolved: Vec<RuleReference> = references
            .iter()
            .filter_map(|r| self.resolve_reference(r, preset))
            .collect();
        resolved.sort_by_key(|r| r.when.is_empty());
        self.references.extend(resolved);
    }

    fn resolve_reference(&self, rule: &Reference, preset: Option<&str>) -> Option<RuleReference> {
        let from = self.column(&rule.from)?;
        let to = self.column(&rule.to)?;
        let (when, to_when) = match (
            condition::compile(&rule.when),
            condition::compile(&rule.to_when),
        ) {
            (Ok(when), Ok(to_when)) => (when, to_when),
            (Err(e), _) | (_, Err(e)) => {
                log::warn!("Reference {} skipped: {e}", rule.id);
                return None;
            }
        };
        let to_filter = to_when
            .iter()
            .filter_map(|c| {
                Some(ColumnEquals {
                    column: c.column.clone(),
                    value: Some(c.matcher.equals()?.to_string()),
                })
            })
            .collect();
        let id = rule.id.clone();
        let by = match preset {
            Some(preset) => ReferenceSource::Preset {
                preset: preset.to_string(),
                id,
            },
            None => ReferenceSource::Project { id },
        };
        Some(RuleReference {
            group: GroupKey {
                preset: preset.map(str::to_string),
                rule: rule.default_id(),
            },
            by,
            from,
            when,
            to,
            to_when,
            to_filter,
            label: rule.label.clone(),
            columns: rule.columns.clone(),
        })
    }

    /// The reference rules of each applied preset in precedence order (the last applied first):
    /// its own and those its meta tables imply. Disabled and overridden rules are kept.
    pub(super) fn preset_reference_rules<'a>(
        &'a self,
        schema: &'a DatabaseSchema,
    ) -> impl Iterator<Item = (Arc<Preset>, Vec<Reference>)> + 'a {
        self.applied.presets.iter().rev().map(move |preset| {
            let implied = self.meta_references(preset, schema);
            let references = preset.references.iter().chain(&implied).cloned().collect();
            (preset.clone(), references)
        })
    }

    /// The reference `fk → owner` that each owned `[[meta]]` table declares, to the owner's
    /// single-column primary key.
    fn meta_references(&self, preset: &Preset, schema: &DatabaseSchema) -> Vec<Reference> {
        preset
            .meta
            .iter()
            .filter_map(|meta| {
                let (owner, fk) = (meta.owner.as_ref()?, meta.fk.as_ref()?);
                let key = schema.single_key(&self.applied.existing_table(owner)?)?;
                let from = ColumnRef {
                    table: meta.table.clone(),
                    column: fk.clone(),
                };
                let to = ColumnRef {
                    table: owner.clone(),
                    column: key.to_string(),
                };
                Some(Reference::plain(from, to))
            })
            .collect()
    }

    fn column(&self, column: &ColumnRef) -> Option<ColumnRef> {
        Some(ColumnRef {
            table: self.applied.existing_table(&column.table)?,
            column: column.column.clone(),
        })
    }

    fn add_preset_tables(&mut self, preset: &Preset) {
        for meta in &preset.meta {
            let Some(table) = self.applied.existing_table(&meta.table) else {
                continue;
            };
            self.meta.push(MetaRef {
                table,
                name: meta.table.clone(),
                owner: meta
                    .owner
                    .as_ref()
                    .and_then(|owner| self.applied.existing_table(owner)),
                fk: meta.fk.clone(),
                key: meta.key.clone(),
                value: meta.value.clone(),
            });
        }
        for entity in &preset.entities {
            let Some(table) = self.applied.existing_table(&entity.table) else {
                continue;
            };
            self.entities.push(AppliedEntity {
                preset: preset.name.clone(),
                entity: Entity {
                    table,
                    ..entity.clone()
                },
            });
        }
        for pattern in &preset.patterns {
            self.add_pattern(pattern);
        }
        for action in &preset.actions {
            self.add_action(&preset.name, action);
        }
        for find in &preset.finds {
            if let Some(table) = self.applied.existing_table(&find.table) {
                self.finds.push(FindSource {
                    table,
                    ..find.clone()
                });
            }
        }
    }

    fn add_action(&mut self, preset: &str, action: &Action) {
        let Some(table) = self.applied.existing_table(&action.table) else {
            return;
        };
        let when = match condition::compile(&action.when) {
            Ok(conditions) => conditions,
            Err(e) => {
                log::warn!("Action {} skipped: {e}", action.name);
                return;
            }
        };
        self.actions.push(RuleAction {
            preset: preset.to_string(),
            rule: Action {
                table,
                ..action.clone()
            },
            when,
        });
    }

    fn add_pattern(&mut self, pattern: &Pattern) {
        let (Some(owner), Some(table)) = (
            self.applied.existing_table(&pattern.owner),
            self.applied.existing_table(&pattern.table),
        ) else {
            return;
        };
        let owner_when = match condition::compile(&pattern.owner_when) {
            Ok(conditions) => conditions,
            Err(e) => {
                log::warn!("Pattern {} skipped: {e}", pattern.name);
                return;
            }
        };
        self.patterns.push(RulePattern {
            rule: Pattern {
                owner,
                table,
                ..pattern.clone()
            },
            owner_when,
        });
    }

    pub fn meta_table(&self, table: &str) -> Option<&MetaRef> {
        self.meta.iter().find(|m| m.table == table)
    }

    /// The meta tables that describe rows of the real table `owner`; each has an `fk`.
    pub fn owned_meta<'s>(&'s self, owner: &str) -> impl Iterator<Item = &'s MetaRef> {
        self.meta
            .iter()
            .filter(move |m| m.owner.as_deref() == Some(owner) && m.fk.is_some())
    }

    /// The entities of the real `table`, filtered ones first.
    pub fn entities_of<'s>(
        &'s self,
        table: &str,
    ) -> impl Iterator<Item = &'s AppliedEntity> + Clone {
        self.entities
            .iter()
            .filter(move |e| e.entity.table == table)
    }

    /// The patterns whose owner is the real table `owner`, in apply order.
    pub fn patterns_of<'s>(&'s self, owner: &str) -> impl Iterator<Item = &'s RulePattern> {
        self.patterns.iter().filter(move |p| p.rule.owner == owner)
    }

    /// Whether one of `globs` covers the real `table.column`.
    pub fn columns_match(&self, globs: &[ColumnGlob], table: &str, column: &str) -> bool {
        let names = self.applied.rule_names(table);
        globs.iter().any(|glob| {
            column_glob_match(&glob.column, column)
                && names.iter().any(|name| glob_match(&glob.table, name))
        })
    }

    /// The first kind rule for the real `table.column`.
    pub fn column_kind_rule(&self, table: &str, column: &str) -> Option<&KindRule> {
        self.kinds
            .iter()
            .find(|rule| self.columns_match(&rule.columns, table, column))
    }

    /// The first kind rule for rows of `meta` stored under `key`.
    pub fn meta_kind_rule(&self, meta: &MetaRef, key: &str) -> Option<&KindRule> {
        self.kinds
            .iter()
            .find(|rule| meta_keys_match(&rule.meta_keys, &rule.in_meta, meta, key))
    }

    /// Entity names per real table, filtered ones first.
    pub fn table_entities(&self) -> BTreeMap<String, Vec<String>> {
        let mut map = BTreeMap::<String, Vec<String>>::new();
        for applied in &self.entities {
            let names = map.entry(applied.entity.table.clone()).or_default();
            if !names.contains(&applied.entity.name) {
                names.push(applied.entity.name.clone());
            }
        }
        map
    }
}

/// Whether a `meta_keys` scope (`globs`, narrowed to the meta tables `in_meta`) covers `key` in `meta`.
pub fn meta_keys_match(globs: &[String], in_meta: &[String], meta: &MetaRef, key: &str) -> bool {
    let in_scope = in_meta.is_empty() || in_meta.contains(&meta.name);
    in_scope && globs.iter().any(|glob| glob_match(glob, key))
}

/// A project reference with the same id, or the same `from` and `when`, replaces the preset rule.
pub(super) fn overrides(own: &Reference, rule: &Reference) -> bool {
    own.id == rule.id || (own.from == rule.from && own.when == rule.when)
}

fn is_overridden(project: &Project, rule: &Reference) -> bool {
    project.references.iter().any(|own| overrides(own, rule))
}

/// A pattern that does not compile was rejected on load, so it is only logged here.
fn to_rule_style(rule: &StyleRule) -> Option<RuleStyle> {
    match rule.matcher() {
        Ok(matcher) => Some(RuleStyle {
            rule: rule.clone(),
            matcher,
        }),
        Err(e) => {
            log::warn!("Value style skipped: {e}");
            None
        }
    }
}

/// What `rules_info` returns: the prefixes, the presets and the entities of a database.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct RulesInfo {
    pub prefix: String,
    pub network_prefix: String,
    pub suggested_prefix: Option<String>,
    /// In apply order: each after the presets it requires.
    pub applied_presets: Vec<String>,
    pub available_presets: Vec<String>,
    pub entities: Vec<AppliedEntity>,
    /// Entity names by real table, filtered entities first.
    pub table_entities: BTreeMap<String, Vec<String>>,
    /// The icon by real table, for tables that have one.
    pub table_icons: BTreeMap<String, TableIcon>,
}

impl RulesInfo {
    pub fn new(rules: &EffectiveRules, loaded: &[Arc<Preset>]) -> Self {
        let applied = &rules.applied;
        Self {
            prefix: applied.prefix.clone(),
            network_prefix: applied.network_prefix.clone(),
            suggested_prefix: applied.suggested_prefix.clone(),
            applied_presets: applied.preset_names(),
            available_presets: loaded.iter().map(|p| p.name.clone()).collect(),
            entities: rules.entities.clone(),
            table_entities: rules.table_entities(),
            table_icons: rules.icons.clone(),
        }
    }
}

/// What a cached entry was built from. The schema and the preset list are compared by pointer:
/// DDL, a refresh or a preset file change replaces them. The project is compared by value.
struct Cached {
    schema: Arc<DatabaseSchema>,
    presets: Arc<[PresetEntry]>,
    project: Project,
    rules: Arc<EffectiveRules>,
}

/// Lives in `Session.ext`, by database.
#[derive(Default)]
struct RulesCache(Mutex<BTreeMap<String, Cached>>);

/// The effective rules of `database` under `schema`, built once per schema, preset and project
/// version.
pub fn effective(
    session: &Session,
    database: &str,
    schema: &Arc<DatabaseSchema>,
    sources: RuleSources<'_>,
) -> Arc<EffectiveRules> {
    let cache = session.ext.get_or_insert_with(RulesCache::default);
    let entries = sources.presets.entries();
    let project = sources.projects.get(&session.slug(), database);
    let mut cached = cache.0.lock_unpoisoned();
    if let Some(hit) = cached.get(database)
        && Arc::ptr_eq(&hit.schema, schema)
        && Arc::ptr_eq(&hit.presets, &entries)
        && hit.project == project
    {
        return hit.rules.clone();
    }
    let rules = Arc::new(EffectiveRules::build(&project, schema, &loaded(&entries)));
    let entry = Cached {
        schema: schema.clone(),
        presets: entries,
        project,
        rules: rules.clone(),
    };
    cached.insert(database.to_string(), entry);
    rules
}

/// The schema of `database` and its effective rules.
pub async fn load(
    session: &Session,
    database: &str,
    sources: RuleSources<'_>,
) -> AppResult<(Arc<DatabaseSchema>, Arc<EffectiveRules>)> {
    let schema = schema::database(session, database).await?;
    let rules = effective(session, database, &schema, sources);
    Ok((schema, rules))
}

pub async fn rules_info(
    session: &Session,
    database: &str,
    sources: RuleSources<'_>,
) -> AppResult<RulesInfo> {
    let (_, rules) = load(session, database, sources).await?;
    Ok(RulesInfo::new(&rules, &sources.presets.presets()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    use tempfile::{TempDir, tempdir};

    use super::*;
    use crate::connection::ConnectionSpec;
    use crate::db::{CellLimit, ColumnInfo, FakeDatabase, ResultSet, TableStructure};
    use crate::rules::icons::IconRule;
    use crate::rules::kinds::{StyleColor, StyleMatch, ValueRule, Zone};
    use crate::rules::testing::{
        TableSpec, forum, forum_schema, preset, project, reference, schema as schema_of,
    };
    use crate::rules::types::{ColumnGlob, Conditions, Match};
    use crate::test_support::{connect_to, servers, spec};

    const TIMEOUT: Duration = Duration::from_secs(5);

    fn build(project: &Project, presets: &[Arc<Preset>]) -> EffectiveRules {
        EffectiveRules::build(project, &forum_schema(), presets)
    }

    fn when(column: &str, rule: Match) -> Conditions {
        Conditions::from([(column.to_string(), rule)])
    }

    fn by_preset(preset: &str, id: &str) -> ReferenceSource {
        ReferenceSource::Preset {
            preset: preset.into(),
            id: id.into(),
        }
    }

    fn by_project(id: &str) -> ReferenceSource {
        ReferenceSource::Project { id: id.into() }
    }

    fn sources_from(rules: &EffectiveRules, table: &str) -> Vec<ReferenceSource> {
        rules
            .references
            .iter()
            .filter(|r| r.from.table == table)
            .map(|r| r.by.clone())
            .collect()
    }

    const CLOSED: &str = "style:threads.status=closed";
    const TONE: &str = "style:|tone|settings=like(wa%)";
    const DRAFT: &str = "style:threads.title=draft";

    fn forum_project(change: impl FnOnce(&mut Project)) -> Project {
        let mut own = Project {
            prefix: Some("f_".into()),
            ..project()
        };
        change(&mut own);
        own
    }

    fn style_colors(rules: &EffectiveRules) -> Vec<(&str, &str)> {
        rules
            .styles
            .iter()
            .map(|s| (s.rule.id.as_str(), s.rule.color.name()))
            .collect()
    }

    fn style(id: &str, columns: &str, value: &str, color: StyleColor) -> StyleRule {
        StyleRule {
            id: id.into(),
            columns: vec![ColumnGlob::try_from(columns.to_string()).unwrap()],
            meta_keys: Vec::new(),
            r#in: Vec::new(),
            matching: StyleMatch::Eq,
            value: value.into(),
            color,
            enabled: true,
        }
    }

    #[test]
    fn preset_styles_apply_as_shipped_until_the_project_switches_them_by_id() {
        let shipped = build(&forum_project(|_| {}), &[forum()]);
        let switched = build(
            &forum_project(|p| {
                p.enabled = vec![DRAFT.into()];
                p.disabled = vec![CLOSED.into()];
            }),
            &[forum()],
        );

        assert_eq!(style_colors(&shipped), [(CLOSED, "red"), (TONE, "green")]);
        assert_eq!(style_colors(&switched), [(TONE, "green"), (DRAFT, "blue")]);
    }

    #[test]
    fn a_project_override_replaces_the_preset_style_in_place_and_follows_its_switch() {
        let edited = |p: &mut Project| {
            p.styles = vec![
                style(CLOSED, "threads.status", "closed", StyleColor::Purple),
                style("mine", "threads.title", "x", StyleColor::Yellow),
            ];
        };
        let on = build(&forum_project(edited), &[forum()]);
        let off = build(
            &forum_project(|p| {
                edited(p);
                p.disabled = vec![CLOSED.into()];
            }),
            &[forum()],
        );

        assert_eq!(
            style_colors(&on),
            [("mine", "yellow"), (CLOSED, "purple"), (TONE, "green")]
        );
        assert_eq!(style_colors(&off), [("mine", "yellow"), (TONE, "green")]);
    }

    const BASE: &str = r#"
[preset]
name = "base"
marker_tables = ["threads"]

[[reference]]
from = "threads.author"
to = "people.person_id"
"#;

    const EXT: &str = r#"
[preset]
name = "ext"
requires = ["base"]

[[reference]]
from = "threads.parent_id"
to = "threads.thread_id"

[[reference]]
from = "notes.v"
when = { k = "_ref" }
to = "threads.thread_id"
"#;

    fn ext_project() -> Project {
        Project {
            prefix: Some("f_".into()),
            presets: Some(vec!["ext".into()]),
            references: vec![reference("p", "replies.by", "bots.bot_id")],
            ..project()
        }
    }

    #[test]
    fn inferred_ids_name_the_real_table_and_column() {
        assert_eq!(
            inferred_id("f_threads", "person_id"),
            "inferred:f_threads.person_id"
        );
    }

    #[test]
    fn rules_that_change_nothing_the_schema_lens_shows_are_empty() {
        let entities_only = preset(
            "[preset]\nname = \"e\"\nmarker_tables = [\"threads\"]\n[[entity]]\nname = \"thread\"\ntable = \"threads\"\n",
        );
        let disabled = Project {
            disabled: vec!["threads.author".into()],
            ..project()
        };

        assert!(build(&project(), &[]).is_empty());
        assert!(build(&project(), &[entities_only]).is_empty());
        assert!(!build(&project(), &[forum()]).is_empty());
        assert!(!build(&disabled, &[]).is_empty());
    }

    #[test]
    fn references_are_ordered_project_first_then_presets_from_the_last_applied() {
        let rules = build(&ext_project(), &[preset(EXT), preset(BASE)]);

        assert_eq!(rules.applied.preset_names(), ["base", "ext"]);
        assert_eq!(
            rules
                .references
                .iter()
                .map(|r| r.by.clone())
                .collect::<Vec<_>>(),
            [
                by_project("p"),
                by_preset("ext", "notes.v?k=_ref"),
                by_preset("ext", "threads.parent_id"),
                by_preset("base", "threads.author"),
            ]
        );
    }

    #[test]
    fn the_rule_for_a_reference_matches_its_source_from_and_to_with_any_column_case() {
        let narrowed = Reference {
            when: when("title", Match::Eq("x".into())),
            ..reference("p", "threads.person_id", "people.person_id")
        };
        let own = Project {
            prefix: Some("f_".into()),
            references: vec![
                narrowed,
                reference("p", "threads.author", "people.person_id"),
            ],
            ..project()
        };
        let rules = build(&own, &[]);
        let resolved = |from: &str| ResolvedReference {
            from: ColumnsRef::new("f_threads", vec![from.into()]),
            to: ColumnsRef::new("f_people", vec!["Person_ID".into()]),
            by: by_project("p"),
        };

        let rule = rules.rule_for(&resolved("AUTHOR")).unwrap();

        assert_eq!(rule.from.column, "author");
        assert!(rule.when.is_empty());
        assert!(rules.rule_for(&resolved("title")).is_none());
    }

    #[test]
    fn disabled_ids_drop_the_references_of_any_preset() {
        let own = Project {
            disabled: vec!["threads.author".into(), "threads.parent_id".into()],
            ..ext_project()
        };

        let rules = build(&own, &[preset(EXT), preset(BASE)]);

        assert!(sources_from(&rules, "f_threads").is_empty());
        assert_eq!(
            sources_from(&rules, "f_notes"),
            [by_preset("ext", "notes.v?k=_ref")]
        );
    }

    #[test]
    fn a_project_reference_replaces_only_the_preset_rule_with_the_same_from_and_when() {
        let replacing = Reference {
            when: when("k", Match::Eq("_ref".into())),
            ..reference("p", "notes.v", "people.person_id")
        };
        let narrowing = Reference {
            when: when("title", Match::Eq("x".into())),
            ..reference("q", "threads.author", "threads.thread_id")
        };
        let own = Project {
            references: vec![replacing, narrowing],
            ..project()
        };

        let rules = build(&own, &[forum()]);

        assert_eq!(
            sources_from(&rules, "f_notes"),
            [
                by_project("p"),
                by_preset("forum", "notes.v?k=glob(_who*)"),
                by_preset("forum", "notes.v?k=_ext"),
                by_preset("forum", "notes.thread_id"),
            ]
        );
        assert_eq!(
            sources_from(&rules, "f_threads"),
            [by_project("q"), by_preset("forum", "threads.author")]
        );
    }

    #[test]
    fn a_disabled_project_reference_is_dropped() {
        let own = Project {
            references: vec![reference("mine", "replies.by", "bots.bot_id")],
            disabled: vec!["mine".into()],
            ..prefixed()
        };
        let enabled = Project {
            disabled: Vec::new(),
            ..own.clone()
        };

        assert_eq!(
            sources_from(&build(&enabled, &[]), "f_replies"),
            [by_project("mine")]
        );
        assert!(build(&own, &[]).references.is_empty());
    }

    #[test]
    fn a_disabled_unconditional_project_reference_keeps_the_inference_on_its_column_off() {
        let conditional = Reference {
            when: when("title", Match::Eq("x".into())),
            ..reference("cond", "threads.parent_id", "threads.thread_id")
        };
        let own = Project {
            references: vec![
                reference("mine", "threads.person_id", "people.name"),
                conditional,
            ],
            disabled: vec!["mine".into(), "cond".into()],
            ..prefixed()
        };

        let rules = build(&own, &[]);

        assert!(rules.references.is_empty());
        let mut disabled: Vec<&str> = rules.disabled.iter().map(String::as_str).collect();
        disabled.sort();
        assert_eq!(disabled, ["cond", "inferred:f_threads.person_id", "mine"]);
    }

    #[test]
    fn a_project_reference_with_a_preset_rules_id_replaces_it_whatever_its_from() {
        let own = Project {
            references: vec![reference("threads.author", "replies.by", "bots.bot_id")],
            ..project()
        };

        let rules = build(&own, &[forum()]);

        assert_eq!(
            sources_from(&rules, "f_replies"),
            [by_project("threads.author")]
        );
        assert!(sources_from(&rules, "f_threads").is_empty());
    }

    #[test]
    fn preset_reference_rules_keep_the_overridden_and_disabled_rules() {
        let own = Project {
            references: vec![reference("mine", "threads.author", "people.name")],
            disabled: vec!["notes.thread_id".into()],
            ..project()
        };
        let rules = build(&own, &[forum()]);

        let ids: Vec<String> = rules
            .preset_reference_rules(&forum_schema())
            .flat_map(|(_, references)| references)
            .map(|r| r.id)
            .collect();

        assert!(ids.contains(&"threads.author".to_string()));
        assert!(ids.contains(&"notes.thread_id".to_string()));
    }

    #[test]
    fn conditional_rules_of_a_source_come_before_its_unconditional_ones() {
        let rules = build(&project(), &[forum()]);

        let order: Vec<ReferenceSource> = rules.references.iter().map(|r| r.by.clone()).collect();

        assert_eq!(
            order,
            [
                by_preset("forum", "notes.v?k=_ref"),
                by_preset("forum", "notes.v?k=glob(_who*)"),
                by_preset("forum", "notes.v?k=_ext"),
                by_preset("forum", "sessions.skey?skey=regexp(^[0-9]+$)"),
                by_preset("forum", "threads.author"),
                by_preset("forum", "replies.by->people.person_id"),
                by_preset("forum", "replies.by->bots.bot_id"),
                by_preset("forum", "notes.thread_id"),
            ]
        );
    }

    fn owned_meta(owner_table: &str) -> Arc<Preset> {
        preset(&format!(
            "[preset]\nname = \"owned\"\nmarker_tables = [\"notes\"]\n\n[[meta]]\ntable = \"notes\"\nowner = \"{owner_table}\"\nfk = \"thread_id\"\nkey = \"k\"\nvalue = \"v\"\n"
        ))
    }

    fn prefixed() -> Project {
        Project {
            prefix: Some("f_".into()),
            ..project()
        }
    }

    #[test]
    fn an_owned_meta_table_implies_a_reference_from_its_fk_to_the_owners_key() {
        let rules = build(&project(), &[forum()]);

        let implied: Vec<&RuleReference> = rules
            .references
            .iter()
            .filter(|r| r.from.table == "f_notes" && r.from.column == "thread_id")
            .collect();

        assert_eq!(implied.len(), 1);
        let reference = implied[0];
        assert_eq!(reference.by, by_preset("forum", "notes.thread_id"));
        assert_eq!(
            reference.group,
            GroupKey {
                preset: Some("forum".into()),
                rule: "notes.thread_id".into()
            }
        );
        assert_eq!(
            (reference.to.table.as_str(), reference.to.column.as_str()),
            ("f_threads", "thread_id")
        );
        assert!(reference.when.is_empty());
        assert!(reference.to_when.is_empty());
        assert_eq!(reference.label, None);
    }

    #[test]
    fn a_meta_table_without_an_owner_implies_no_reference() {
        let rules = build(&project(), &[forum()]);

        assert!(sources_from(&rules, "f_settings").is_empty());
    }

    #[test]
    fn the_implied_reference_is_disabled_by_its_id() {
        let own = Project {
            disabled: vec!["notes.thread_id".into()],
            ..project()
        };

        let rules = build(&own, &[forum()]);

        assert!(!sources_from(&rules, "f_notes").contains(&by_preset("forum", "notes.thread_id")));
        assert_eq!(
            rules.meta_table("f_notes").unwrap().fk.as_deref(),
            Some("thread_id")
        );
    }

    #[test]
    fn a_project_reference_from_the_fk_column_replaces_the_implied_one() {
        let own = Project {
            references: vec![reference("mine", "notes.thread_id", "people.person_id")],
            ..project()
        };

        let rules = build(&own, &[forum()]);

        let from_fk: Vec<ReferenceSource> = sources_from(&rules, "f_notes")
            .into_iter()
            .filter(|by| *by == by_project("mine") || *by == by_preset("forum", "notes.thread_id"))
            .collect();
        assert_eq!(from_fk, [by_project("mine")]);
    }

    #[test]
    fn the_owner_key_must_be_a_single_column_primary_key() {
        let notes: TableSpec<'_> = ("f_notes", &["note_id", "thread_id", "k", "v"], &["note_id"]);
        let single: TableSpec<'_> = ("f_threads", &["a"], &["a"]);
        let composite: TableSpec<'_> = ("f_threads", &["a", "b"], &["a", "b"]);
        let keyless: TableSpec<'_> = ("f_threads", &["a"], &[]);
        let cases = [
            ("single key", vec![notes, single], true),
            ("composite key", vec![notes, composite], false),
            ("no key", vec![notes, keyless], false),
            ("missing owner", vec![notes], false),
        ];
        for (name, tables, implied) in cases {
            let rules =
                EffectiveRules::build(&prefixed(), &schema_of(&tables), &[owned_meta("threads")]);

            assert_eq!(
                sources_from(&rules, "f_notes").contains(&by_preset("owned", "notes.thread_id")),
                implied,
                "{name}"
            );
        }
    }

    #[test]
    fn the_implied_reference_targets_the_owners_primary_key_column_whatever_its_name() {
        let tables = schema_of(&[
            ("f_notes", &["note_id", "thread_id", "k", "v"], &["note_id"]),
            ("f_threads", &["thread_id", "uid"], &["uid"]),
        ]);

        let rules = EffectiveRules::build(&prefixed(), &tables, &[owned_meta("threads")]);

        let to = &rules.references[0].to;
        assert_eq!(
            (to.table.as_str(), to.column.as_str()),
            ("f_threads", "uid")
        );
    }

    #[test]
    fn a_reference_label_names_the_incoming_group_and_defaults_to_none() {
        let labelled = preset(
            "[preset]\nname = \"l\"\nmarker_tables = [\"threads\"]\n\n[[reference]]\nfrom = \"threads.parent_id\"\nto = \"threads.thread_id\"\nlabel = \"children\"\n\n[[reference]]\nfrom = \"threads.author\"\nto = \"people.person_id\"\n",
        );

        let rules = build(&prefixed(), &[labelled]);

        let labels: Vec<(&str, Option<&str>)> = rules
            .references
            .iter()
            .map(|r| (r.from.column.as_str(), r.label.as_deref()))
            .collect();
        assert_eq!(labels, [("parent_id", Some("children")), ("author", None)]);
    }

    #[test]
    fn a_reference_carries_the_columns_of_its_rule_and_an_implied_one_has_none() {
        let columned = preset(
            "[preset]\nname = \"c\"\nmarker_tables = [\"threads\"]\n\n[[meta]]\ntable = \"notes\"\nowner = \"threads\"\nfk = \"thread_id\"\nkey = \"k\"\nvalue = \"v\"\n\n[[reference]]\nfrom = \"threads.parent_id\"\nto = \"threads.thread_id\"\ncolumns = [\"title\", \"status\"]\n\n[[reference]]\nfrom = \"threads.author\"\nto = \"people.person_id\"\n",
        );

        let rules = build(&prefixed(), &[columned]);

        let columns: Vec<(&str, Vec<&str>)> = rules
            .references
            .iter()
            .map(|r| {
                (
                    r.from.column.as_str(),
                    r.columns.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        assert_eq!(
            columns,
            [
                ("parent_id", vec!["title", "status"]),
                ("author", vec![]),
                ("thread_id", vec![]),
            ]
        );
    }

    const PATTERNS: &str = r#"
[preset]
name = "pat"
marker_tables = ["threads"]

[[pattern]]
name = "audit"
owner = "threads"
owner_when = { status = "open" }
table = "notes"
where = "thread_id = {id} AND k = {meta._key}"
order = "note_id"
limit = 5

[[pattern]]
name = "audit"
owner = "threads"
table = "replies"
where = "thread_id = {id}"

[[pattern]]
name = "ghosts"
owner = "threads"
table = "ghosts"
where = "1"

[[pattern]]
name = "orphan"
owner = "ghosts"
table = "notes"
where = "1"
"#;

    #[test]
    fn patterns_name_real_tables_and_keep_the_rule_as_written() {
        let rules = build(&prefixed(), &[preset(PATTERNS)]);

        let patterns: Vec<(&str, &str, &str)> = rules
            .patterns
            .iter()
            .map(|p| {
                (
                    p.rule.name.as_str(),
                    p.rule.owner.as_str(),
                    p.rule.table.as_str(),
                )
            })
            .collect();
        assert_eq!(
            patterns,
            [
                ("audit", "f_threads", "f_notes"),
                ("audit", "f_threads", "f_replies"),
            ]
        );
        let first = &rules.patterns[0];
        assert_eq!(first.rule.r#where, "thread_id = {id} AND k = {meta._key}");
        assert_eq!(first.rule.order.as_deref(), Some("note_id"));
        assert_eq!(first.rule.limit, Some(5));
        assert_eq!(
            first
                .owner_when
                .iter()
                .map(|c| c.column.as_str())
                .collect::<Vec<_>>(),
            ["status"]
        );
        assert!(rules.patterns[1].owner_when.is_empty());
    }

    #[test]
    fn a_pattern_whose_owner_or_table_does_not_exist_is_dropped() {
        let rules = build(&prefixed(), &[preset(PATTERNS)]);

        let names: Vec<&str> = rules
            .patterns
            .iter()
            .map(|p| p.rule.name.as_str())
            .collect();

        assert!(!names.contains(&"ghosts"));
        assert!(!names.contains(&"orphan"));
    }

    #[test]
    fn meta_tables_name_their_real_owner_and_fk() {
        let rules = build(&project(), &[forum()]);

        let owners: Vec<(&str, Option<&str>, Option<&str>)> = rules
            .meta
            .iter()
            .map(|m| (m.table.as_str(), m.owner.as_deref(), m.fk.as_deref()))
            .collect();

        assert_eq!(
            owners,
            [
                ("f_notes", Some("f_threads"), Some("thread_id")),
                ("f_settings", None, None),
            ]
        );
    }

    #[test]
    fn a_meta_table_whose_owner_does_not_exist_has_no_owner_table() {
        let tables = schema_of(&[("f_notes", &["note_id", "thread_id", "k", "v"], &["note_id"])]);

        let rules = EffectiveRules::build(&prefixed(), &tables, &[owned_meta("threads")]);

        let meta = rules.meta_table("f_notes").unwrap();
        assert_eq!(meta.owner, None);
        assert_eq!(meta.fk.as_deref(), Some("thread_id"));
    }

    #[test]
    fn column_kind_rules_match_real_table_names_through_the_prefix() {
        let rules = build(&project(), &[forum()]);
        let cases = [
            ("f_people", "secret_code", Some(ValueRule::Secret)),
            (
                "f_people",
                "joined_gmt",
                Some(ValueRule::Timestamp(Zone::Utc)),
            ),
            (
                "f_threads",
                "closed_local",
                Some(ValueRule::Timestamp(Zone::Local)),
            ),
            ("f_threads", "status", Some(ValueRule::Enum)),
            ("f_people", "name", None),
            ("people", "secret_code", None),
        ];
        for (table, column, expected) in cases {
            assert_eq!(
                rules
                    .column_kind_rule(table, column)
                    .map(|r| r.value.clone()),
                expected,
                "{table}.{column}"
            );
        }
    }

    #[test]
    fn meta_kind_rules_apply_by_key_and_only_in_the_meta_tables_they_name() {
        let rules = build(&project(), &[forum()]);
        let notes = rules.meta_table("f_notes").unwrap();
        let settings = rules.meta_table("f_settings").unwrap();
        let cases = [
            (notes, "api_key", Some(ValueRule::Secret)),
            (settings, "api_key", Some(ValueRule::Secret)),
            (
                settings,
                "expires_at",
                Some(ValueRule::Timestamp(Zone::Unix)),
            ),
            (notes, "expires_at", None),
            (settings, "tone", Some(ValueRule::Enum)),
            (notes, "tone", None),
            (settings, "mood", None),
        ];
        for (meta, key, expected) in cases {
            assert_eq!(
                rules.meta_kind_rule(meta, key).map(|r| r.value.clone()),
                expected,
                "{} {key}",
                meta.table
            );
        }
    }

    #[test]
    fn meta_keys_match_needs_a_glob_and_a_table_in_scope() {
        let rules = build(&project(), &[forum()]);
        let settings = rules.meta_table("f_settings").unwrap();
        let globs = vec!["api_*".to_string()];
        let cases = [
            (&globs[..], vec![], "api_key", true),
            (&globs[..], vec!["settings".to_string()], "api_key", true),
            (&globs[..], vec!["notes".to_string()], "api_key", false),
            (&globs[..], vec![], "other", false),
            (&[][..], vec![], "api_key", false),
        ];
        for (globs, in_meta, key, expected) in cases {
            assert_eq!(
                meta_keys_match(globs, &in_meta, settings, key),
                expected,
                "{globs:?} in {in_meta:?} {key}"
            );
        }
    }

    #[test]
    fn columns_match_covers_the_prefixed_table_and_column_globs() {
        let rules = build(&project(), &[forum()]);
        let globs = [
            ColumnGlob::try_from("people.secret_code".to_string()).unwrap(),
            ColumnGlob::try_from("*.*_gmt".to_string()).unwrap(),
        ];
        let cases = [
            ("f_people", "secret_code", true),
            ("f_threads", "opened_gmt", true),
            ("f_threads", "title", false),
            ("people", "secret_code", false),
        ];
        for (table, column, expected) in cases {
            assert_eq!(
                rules.columns_match(&globs, table, column),
                expected,
                "{table}.{column}"
            );
        }
    }

    #[test]
    fn only_equality_conditions_of_to_when_narrow_the_target_address() {
        let narrow = preset(
            r#"
[preset]
name = "narrow"

[[reference]]
from = "notes.v"
when = { k = "_a" }
to = "notes.v"
to_when = { k = "_a" }

[[reference]]
from = "notes.v"
when = { k = "_b" }
to = "notes.v"
to_when = { k = ["x", "y"] }

[[reference]]
from = "notes.v"
when = { k = "_c" }
to = "notes.v"
to_when = { k = { not = "x" } }
"#,
        );
        let own = Project {
            prefix: Some("f_".into()),
            presets: Some(vec!["narrow".into()]),
            ..project()
        };

        let rules = build(&own, &[narrow]);

        let filters: Vec<Vec<ColumnEquals>> = rules
            .references
            .iter()
            .map(|r| r.to_filter.clone())
            .collect();
        assert_eq!(
            filters,
            [
                vec![ColumnEquals {
                    column: "k".into(),
                    value: Some("_a".into())
                }],
                Vec::new(),
                Vec::new(),
            ]
        );
    }

    #[test]
    fn rules_on_tables_that_do_not_exist_are_dropped() {
        let missing = preset(
            "[preset]\nname = \"missing\"\nmarker_tables = [\"threads\"]\n\n[[reference]]\nfrom = \"threads.author\"\nto = \"extras.extra_id\"\n\n[[reference]]\nfrom = \"extras.owner\"\nto = \"people.person_id\"\n",
        );

        let rules = build(&project(), &[missing]);

        assert!(rules.references.is_empty());
    }

    #[test]
    fn rules_with_a_pattern_that_does_not_compile_are_skipped() {
        let project_with = |pattern: &str| {
            let rule = Reference {
                when: when(
                    "k",
                    Match::Regexp {
                        regexp: pattern.into(),
                    },
                ),
                ..reference("p", "notes.v", "people.person_id")
            };
            let style = StyleRule {
                id: String::new(),
                columns: vec![ColumnGlob::try_from("threads.title".to_string()).unwrap()],
                meta_keys: Vec::new(),
                r#in: Vec::new(),
                matching: StyleMatch::Regexp,
                value: pattern.into(),
                color: StyleColor::Red,
                enabled: true,
            };
            Project {
                prefix: Some("f_".into()),
                references: vec![rule],
                styles: vec![style],
                ..project()
            }
        };

        let valid = build(&project_with("^x"), &[]);
        let broken = build(&project_with("("), &[]);

        assert_eq!(valid.references.len(), 1);
        assert_eq!(valid.styles.len(), 1);
        assert!(broken.references.is_empty());
        assert!(broken.styles.is_empty());
    }

    #[test]
    fn filtered_entities_come_first_and_name_the_real_table() {
        let rules = build(&project(), &[forum()]);

        let entities: Vec<(&str, &str)> = rules
            .entities
            .iter()
            .map(|e| (e.entity.name.as_str(), e.entity.table.as_str()))
            .collect();

        assert_eq!(
            entities,
            [
                ("announcement", "f_threads"),
                ("thread", "f_threads"),
                ("person", "f_people"),
            ]
        );
        assert_eq!(
            rules.table_entities(),
            BTreeMap::from([
                ("f_people".to_string(), vec!["person".to_string()]),
                (
                    "f_threads".to_string(),
                    vec!["announcement".to_string(), "thread".to_string()]
                ),
            ])
        );
    }

    #[test]
    fn meta_tables_keep_their_real_name_and_the_name_rules_use() {
        let rules = build(&project(), &[forum()]);

        let meta: Vec<(&str, &str, &str, &str)> = rules
            .meta
            .iter()
            .map(|m| {
                (
                    m.table.as_str(),
                    m.name.as_str(),
                    m.key.as_str(),
                    m.value.as_str(),
                )
            })
            .collect();

        assert_eq!(
            meta,
            [
                ("f_notes", "notes", "k", "v"),
                ("f_settings", "settings", "name", "val"),
            ]
        );
    }

    fn mini() -> Arc<Preset> {
        preset(
            "[preset]\nname = \"mini\"\nmarker_tables = [\"threads\"]\n\n[[entity]]\nname = \"thread\"\ntable = \"threads\"\n",
        )
    }

    #[test]
    fn rules_info_serializes_the_prefixes_presets_and_entities_for_the_page() {
        let rules = build(&project(), &[mini()]);

        let info = RulesInfo::new(&rules, &[mini()]);

        assert_eq!(
            serde_json::to_value(&info).unwrap(),
            serde_json::json!({
                "prefix": "f_",
                "networkPrefix": "f_",
                "suggestedPrefix": "f_",
                "appliedPresets": ["mini"],
                "availablePresets": ["mini"],
                "entities": [{
                    "preset": "mini",
                    "entity": { "name": "thread", "table": "f_threads" },
                }],
                "tableEntities": { "f_threads": ["thread"] },
                "tableIcons": {},
            })
        );
    }

    #[test]
    fn rules_info_keys_icons_by_real_table_and_leaves_none_out() {
        let icons = preset(
            "[preset]\nname = \"icons\"\nmarker_tables = [\"threads\"]\n\n[[icon]]\ntable = \"threads\"\nicon = \"list\"\n\n[[icon]]\ntable = \"people\"\nicon = \"users\"\n",
        );
        let project = Project {
            icons: vec![IconRule {
                table: "people".into(),
                icon: TableIcon::None,
            }],
            ..project()
        };
        let rules = build(&project, std::slice::from_ref(&icons));

        let info = RulesInfo::new(&rules, std::slice::from_ref(&icons));

        assert_eq!(
            serde_json::to_value(&info.table_icons).unwrap(),
            serde_json::json!({ "f_threads": "list" })
        );
    }

    #[test]
    fn rules_info_of_a_database_no_preset_fits_is_empty_but_lists_the_available_presets() {
        let rules = EffectiveRules::build(
            &project(),
            &crate::rules::testing::tables(&["authors", "books"]),
            &[mini()],
        );

        let info = RulesInfo::new(&rules, &[mini()]);

        assert_eq!(
            serde_json::to_value(&info).unwrap(),
            serde_json::json!({
                "prefix": "",
                "networkPrefix": "",
                "suggestedPrefix": null,
                "appliedPresets": [],
                "availablePresets": ["mini"],
                "entities": [],
                "tableEntities": {},
                "tableIcons": {},
            })
        );
    }

    fn structure(columns: &[&str], primary_key: &str) -> TableStructure {
        let column = |name: &&str| ColumnInfo {
            name: name.to_string(),
            column_type: "bigint".into(),
            nullable: false,
            default: None,
            key: String::new(),
            extra: String::new(),
            comment: String::new(),
            auto_increment: false,
            generated: false,
        };
        TableStructure {
            columns: columns.iter().map(column).collect(),
            indexes: Vec::new(),
            primary_key: vec![primary_key.into()],
            foreign_keys: Vec::new(),
            ddl: None,
        }
    }

    const LINKS: &str = r#"
[preset]
name = "links"
marker_tables = ["threads"]

[[reference]]
from = "threads.author"
to = "people.person_id"

[[reference]]
from = "threads.editor"
to = "extras.extra_id"
"#;

    /// A session over a fake `wp` database with `f_threads` and `f_people`, a preset directory
    /// and a projects file, both in a tempdir.
    struct Rig {
        dir: TempDir,
        db: Arc<FakeDatabase>,
        session: Session,
        catalog: PresetCatalog,
        projects: ProjectsStore,
    }

    impl Rig {
        fn new() -> Self {
            let dir = tempdir().unwrap();
            let db = Arc::new(
                FakeDatabase::new()
                    .with_database("wp")
                    .with_table(
                        "wp",
                        "f_threads",
                        structure(&["thread_id", "author"], "thread_id"),
                    )
                    .with_table("wp", "f_people", structure(&["person_id"], "person_id"))
                    .on("ALTER TABLE", ResultSet::default()),
            );
            let connection = ConnectionSpec {
                name: None,
                host: "127.0.0.1".into(),
                port: 3306,
                socket: None,
                user: "root".into(),
                database: Some("wp".into()),
            };
            let session =
                Session::new("s".into(), connection, "local".into(), Box::new(db.clone()));
            let catalog = PresetCatalog::with_built_in(dir.path().join("presets"), &[]);
            let projects = ProjectsStore::open(dir.path());
            Self {
                dir,
                db,
                session,
                catalog,
                projects,
            }
        }

        /// Writes `presets/<file>` and pins its mtime, so a rewrite is seen whatever the
        /// filesystem's timestamp resolution.
        fn write_preset(&self, file: &str, src: &str, mtime_secs: u64) -> PathBuf {
            let presets = self.dir.path().join("presets");
            fs::create_dir_all(&presets).unwrap();
            let path = presets.join(file);
            fs::write(&path, src).unwrap();
            let handle = fs::OpenOptions::new().write(true).open(&path).unwrap();
            handle
                .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(mtime_secs))
                .unwrap();
            path
        }

        fn sources(&self) -> RuleSources<'_> {
            RuleSources {
                presets: &self.catalog,
                projects: &self.projects,
            }
        }

        async fn rules(&self) -> Arc<EffectiveRules> {
            load(&self.session, "wp", self.sources()).await.unwrap().1
        }
    }

    #[tokio::test]
    async fn loading_gives_the_cached_schema_and_its_rules() {
        let rig = Rig::new();
        rig.write_preset("links.toml", LINKS, 100);

        let (schema, rules) = load(&rig.session, "wp", rig.sources()).await.unwrap();

        assert!(Arc::ptr_eq(
            &schema,
            &schema::database(&rig.session, "wp").await.unwrap()
        ));
        assert!(Arc::ptr_eq(&rules, &rig.rules().await));
        assert_eq!(rules.references.len(), 1);
    }

    #[tokio::test]
    async fn rules_info_lists_every_loaded_preset_and_the_applied_ones() {
        let rig = Rig::new();
        rig.write_preset("links.toml", LINKS, 100);
        rig.write_preset("broken.toml", "[preset", 100);
        rig.write_preset(
            "other.toml",
            "[preset]\nname = \"other\"\nmarker_tables = [\"nowhere\"]\n",
            100,
        );

        let info = rules_info(&rig.session, "wp", rig.sources()).await.unwrap();

        assert_eq!(info.applied_presets, ["links"]);
        assert_eq!(info.available_presets, ["links", "other"]);
        assert_eq!(info.prefix, "f_");
    }

    #[tokio::test]
    async fn unchanged_inputs_give_the_same_rules() {
        let rig = Rig::new();
        rig.write_preset("links.toml", LINKS, 100);

        let first = rig.rules().await;
        let second = rig.rules().await;

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.references.len(), 1);
    }

    #[tokio::test]
    async fn a_project_change_rebuilds_the_rules() {
        let rig = Rig::new();
        rig.write_preset("links.toml", LINKS, 100);
        assert_eq!(rig.rules().await.references.len(), 1);

        rig.projects
            .update("local", "wp", |p| {
                p.disabled.push("threads.author".into());
                Ok(())
            })
            .await
            .unwrap();

        assert!(rig.rules().await.references.is_empty());
    }

    #[tokio::test]
    async fn a_preset_file_change_rebuilds_the_rules() {
        let rig = Rig::new();
        rig.write_preset("links.toml", LINKS, 100);
        assert_eq!(rig.rules().await.references.len(), 1);

        rig.write_preset(
            "links.toml",
            "[preset]\nname = \"links\"\nmarker_tables = [\"threads\"]\n",
            200,
        );
        let edited = rig.rules().await;
        rig.write_preset(
            "other.toml",
            "[preset]\nname = \"other\"\nmarker_tables = [\"threads\"]\n",
            100,
        );
        let added = rig.rules().await;

        assert!(edited.references.is_empty());
        assert_eq!(edited.applied.preset_names(), ["links"]);
        assert_eq!(added.applied.preset_names(), ["links", "other"]);
    }

    #[tokio::test]
    async fn ddl_through_the_session_rebuilds_the_rules_for_the_new_schema() {
        let rig = Rig::new();
        rig.write_preset("links.toml", LINKS, 100);
        assert_eq!(rig.rules().await.references.len(), 1);

        rig.db
            .set_table("wp", "f_extras", structure(&["extra_id"], "extra_id"));
        assert_eq!(rig.rules().await.references.len(), 1);
        rig.session
            .execute(
                "ALTER TABLE f_extras ADD COLUMN x int",
                10,
                CellLimit::NONE,
                TIMEOUT,
                None,
            )
            .await
            .unwrap();

        assert_eq!(rig.rules().await.references.len(), 2);
    }

    #[tokio::test]
    async fn a_refresh_rebuilds_the_rules_for_the_new_schema() {
        let rig = Rig::new();
        rig.write_preset("links.toml", LINKS, 100);
        assert_eq!(rig.rules().await.references.len(), 1);

        rig.db
            .set_table("wp", "f_extras", structure(&["extra_id"], "extra_id"));
        schema::refresh(&rig.session);

        assert_eq!(rig.rules().await.references.len(), 2);
    }

    async fn info_on(sources: RuleSources<'_>, database: &str, host: &str, port: u16) -> RulesInfo {
        let db = connect_to(host, port, database).await;
        let session = Session::new(
            "it".into(),
            spec(host, port, database),
            format!("it-{port}"),
            Box::new(db),
        );
        rules_info(&session, database, sources)
            .await
            .unwrap_or_else(|e| panic!("{host}:{port}: {e:?}"))
    }

    #[tokio::test]
    #[ignore]
    async fn the_built_in_presets_apply_to_the_wordpress_fixture_and_not_to_the_plain_one() {
        let dir = tempdir().unwrap();
        let catalog = PresetCatalog::open(dir.path());
        let projects = ProjectsStore::open(dir.path());
        let sources = RuleSources {
            presets: &catalog,
            projects: &projects,
        };
        for (host, port) in servers() {
            let wordpress = info_on(sources, "wp_test", &host, port).await;
            let plain = info_on(sources, "databee_test", &host, port).await;

            let server = format!("{host}:{port}");
            assert_eq!(wordpress.prefix, "wp_", "{server}");
            assert_eq!(wordpress.network_prefix, "wp_", "{server}");
            assert_eq!(
                wordpress.suggested_prefix.as_deref(),
                Some("wp_"),
                "{server}"
            );
            assert_eq!(
                wordpress.applied_presets,
                ["wordpress", "woocommerce", "action-scheduler"],
                "{server}"
            );
            assert_eq!(
                wordpress.available_presets,
                ["wordpress", "woocommerce", "action-scheduler"],
                "{server}"
            );
            assert_eq!(
                wordpress.table_entities.get("wp_posts"),
                Some(&vec![
                    "order".to_string(),
                    "product".to_string(),
                    "post".to_string()
                ]),
                "{server}"
            );
            assert_eq!(plain.prefix, "", "{server}");
            assert_eq!(plain.suggested_prefix, None, "{server}");
            assert!(plain.applied_presets.is_empty(), "{server}");
            assert!(plain.entities.is_empty(), "{server}");
            assert!(plain.table_entities.is_empty(), "{server}");
            assert_eq!(
                plain.available_presets,
                ["wordpress", "woocommerce", "action-scheduler"],
                "{server}"
            );
        }
    }
}
