use serde::Serialize;
use ts_rs::TS;

use crate::lens::{ColumnsRef, ResolvedReference};
use crate::rules::kinds::{KindRule, StyleRule};
use crate::rules::project::Project;
use crate::rules::types::Reference;

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RuleOrigin {
    Project,
    Preset {
        preset: String,
    },
    ForeignKey {
        constraint: String,
    },
    /// Same-name inference; `indexed` when an index starts with the column.
    Inferred {
        indexed: bool,
    },
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ReferenceRow {
    pub origin: RuleOrigin,
    /// What `disable`, `enable` and `removeReference` take; `None` for a foreign key (a fact).
    pub id: Option<String>,
    /// Real table names.
    pub from: ColumnsRef,
    pub to: ColumnsRef,
    /// The rule's `when` and `to_when` as SQL-like text; empty without conditions.
    pub when: String,
    pub to_when: String,
    pub label: Option<String>,
    pub disabled: bool,
    /// What `reference_coverage` takes; `None` while the reference does not apply.
    pub reference: Option<ResolvedReference>,
    /// The rule as stored (table names without prefix): a project or preset rule, or the project
    /// rule that promotes an inferred reference.
    pub rule: Option<Reference>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct KindRow {
    pub origin: RuleOrigin,
    /// The index `updateKind` and `removeKind` take, for project rules.
    pub index: Option<usize>,
    pub rule: KindRule,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct StyleRow {
    pub origin: RuleOrigin,
    pub index: Option<usize>,
    pub rule: StyleRule,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct PresetRow {
    pub name: String,
    /// Real table names under the prefix.
    pub marker_tables: Vec<String>,
    pub applied: bool,
}

/// A table and the name rules write it as (without prefix).
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct RuleTable {
    pub table: String,
    pub name: String,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ProjectRules {
    pub project: Project,
    pub prefix: String,
    pub suggested_prefix: Option<String>,
    pub presets: Vec<PresetRow>,
    pub foreign_keys: usize,
    pub references: Vec<ReferenceRow>,
    pub kinds: Vec<KindRow>,
    pub styles: Vec<StyleRow>,
    pub tables: Vec<RuleTable>,
}
