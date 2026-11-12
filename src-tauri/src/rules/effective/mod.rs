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

mod build;
mod cache;

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;
use ts_rs::TS;

use super::ProjectsStore;
use super::actions::Action;
use super::applied::Applied;
use super::catalog::PresetCatalog;
use super::condition::{Condition, Matcher, column_glob_match, glob_match};
use super::icons::TableIcon;
use super::kinds::{KindRule, StyleRule};
use super::queries::Pattern;
use super::types::{ColumnGlob, ColumnRef, Entity, FindSource};
use crate::db::ColumnEquals;
use crate::lens::{ColumnsRef, ResolvedReference};
use crate::value::ReferenceSource;

pub(super) use build::overrides;
pub use cache::{RulesInfo, effective, load, rules_info};

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
    /// The icon of each real table (`icons::table_icons`).
    pub icons: BTreeMap<String, TableIcon>,
}

/// The id that disables the same-name inference of `table.column` (real names).
pub fn inferred_id(table: &str, column: &str) -> String {
    format!("inferred:{table}.{column}")
}

impl EffectiveRules {
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

#[cfg(test)]
mod tests;
