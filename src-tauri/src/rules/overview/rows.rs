use serde::Serialize;
use ts_rs::TS;

use super::changes::ProjectChangeLine;
use crate::lens::{ColumnsRef, ResolvedReference};
use crate::rules::actions::ActionScope;
use crate::rules::icons::TableIcon;
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
    /// Where the reference comes from; an edited preset or inferred reference keeps its origin.
    pub origin: RuleOrigin,
    /// What `disable` and `enable` take: the project rule's id for a project or edited row (also
    /// what `updateReference` and `removeReference` take), else the preset or inferred id; `None`
    /// for a foreign key (a fact).
    pub id: Option<String>,
    /// Real table names.
    pub from: ColumnsRef,
    pub to: ColumnsRef,
    /// The rule's `when` and `to_when` as SQL-like text; empty without conditions.
    pub when: String,
    pub to_when: String,
    pub label: Option<String>,
    pub disabled: bool,
    /// A preset or inferred reference a project rule replaces (`removeReference` resets it).
    pub edited: bool,
    /// What `reference_coverage` takes; `None` while the reference does not apply.
    pub reference: Option<ResolvedReference>,
    /// The rule as stored (table names without prefix): a project or preset rule, or for an
    /// inferred reference the project rule that would replace it (empty id).
    pub rule: Option<Reference>,
}

/// An incoming reference, listed on its target table.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ReferencedBy {
    pub from: ColumnsRef,
    pub to: ColumnsRef,
    pub when: String,
    pub disabled: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct EntityLine {
    pub preset: String,
    pub name: String,
    pub filter: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ActionLine {
    pub preset: String,
    pub name: String,
    pub scope: ActionScope,
}

/// The rules of one real table: references from it, the references to it from other tables, the
/// value styles and kinds whose scope covers one of its columns (or its meta keys, for a meta
/// table), and the entities and actions on it. `table` is `None` for styles and kinds that cover
/// no table of this database.
#[derive(Serialize, Debug, Clone, PartialEq, Default, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct TableRules {
    pub table: Option<String>,
    pub references: Vec<ReferenceRow>,
    pub referenced_by: Vec<ReferencedBy>,
    pub styles: Vec<StyleRow>,
    pub kinds: Vec<KindRow>,
    pub entities: Vec<EntityLine>,
    pub actions: Vec<ActionLine>,
    pub icon: Option<IconRow>,
}

/// The icon rule that applies to a table, `none` included.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct IconRow {
    /// `project` or `preset`.
    pub origin: RuleOrigin,
    pub icon: TableIcon,
    /// The rule's `table` as written: a name without prefix or a glob.
    pub pattern: String,
    /// The presets' rule for the table, which a project rule overrides and `removeIcon` brings
    /// back.
    pub shipped: Option<ShippedIcon>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ShippedIcon {
    pub preset: String,
    pub icon: TableIcon,
    pub pattern: String,
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
    /// What `updateStyle`, `removeStyle`, `enableStyle` and `disableStyle` take.
    pub id: String,
    /// The rule that applies: for a preset style, its project override if there is one.
    pub rule: StyleRule,
    /// A preset style the project has not switched on, or a project style stored off.
    pub disabled: bool,
    /// A preset style the project overrides (`removeStyle` brings back the preset's).
    pub edited: bool,
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
    pub tables: Vec<RuleTable>,
    /// What the project changes about the presets as they ship.
    pub changes: Vec<ProjectChangeLine>,
    /// By real table name, then the group without a table.
    pub groups: Vec<TableRules>,
}
