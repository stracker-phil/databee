//! Edits to a project (`project_update`). Each change is checked against the database before it
//! is stored: the tables and columns a rule names must exist, and an error names the field.

mod checks;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::catalog::PresetCatalog;
use super::icons::IconRule;
use super::kinds::{KindRule, StyleRule};
use super::project::Project;
use super::types::{FieldError, Preset, Reference};
use crate::error::{AppError, AppResult};
use crate::schema::{self, DatabaseSchema};
use crate::session::Session;
use checks::{
    Tables, check_index, check_prefix, check_presets, check_rule_id, new_reference_id,
    new_style_id, reference_index,
};

/// Kind rules have no id, so they are addressed by their index in the project.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum ProjectChange {
    /// Without an `id`, the reference gets its default id (docs/presets.md). One with a preset
    /// reference's id overrides that reference.
    AddReference {
        reference: Reference,
    },
    /// An empty `id` in `reference` keeps the old one.
    UpdateReference {
        id: String,
        reference: Reference,
    },
    RemoveReference {
        id: String,
    },
    AddKind {
        rule: KindRule,
    },
    UpdateKind {
        index: usize,
        rule: KindRule,
    },
    RemoveKind {
        index: usize,
    },
    /// Without an `id`, the style gets its default id. One that is a preset style's id overrides
    /// that style and switches it on.
    AddStyle {
        rule: StyleRule,
    },
    /// A project style, or a preset style, which gets a project override. An empty `id` in `rule`
    /// keeps the old one.
    UpdateStyle {
        id: String,
        rule: StyleRule,
    },
    /// Removing an override brings back the preset style.
    RemoveStyle {
        id: String,
    },
    /// Switches a preset or project style on or off by its id.
    EnableStyle {
        id: String,
    },
    DisableStyle {
        id: String,
    },
    /// Switches off a project, preset or inferred reference by its id.
    Disable {
        id: String,
    },
    Enable {
        id: String,
    },
    /// Stores the project's icon for `rule.table`, replacing one it has.
    SetIcon {
        rule: IconRule,
    },
    /// Removing the project's icon brings back the presets' one.
    RemoveIcon {
        table: String,
    },
    SetPrefix {
        prefix: Option<String>,
    },
    SetNetworkPrefix {
        prefix: Option<String>,
    },
    SetPresets {
        presets: Option<Vec<String>>,
    },
}

pub struct ChangeContext {
    pub schema: Arc<DatabaseSchema>,
    /// Every preset that loaded: `setPresets` names must be among them, and their marker and
    /// network tables suggest the prefixes when the project sets none.
    pub presets: Vec<Arc<Preset>>,
}

impl ChangeContext {
    pub async fn load(
        session: &Session,
        database: &str,
        presets: &PresetCatalog,
    ) -> AppResult<Self> {
        Ok(Self {
            schema: schema::database(session, database).await?,
            presets: presets.presets(),
        })
    }
}

fn invalid(error: FieldError) -> AppError {
    AppError::InvalidField {
        field: error.field,
        message: error.message,
    }
}
pub fn apply(project: &mut Project, change: ProjectChange, ctx: &ChangeContext) -> AppResult<()> {
    apply_checked(project, change, ctx).map_err(invalid)
}

/// Stores `reference` as `apply` does for `addReference`, or for `updateReference` of the
/// reference `replacing`, and returns the id it is stored under.
pub fn put_reference(
    project: &mut Project,
    reference: Reference,
    replacing: Option<&str>,
    ctx: &ChangeContext,
) -> AppResult<String> {
    put_checked(project, reference, replacing, ctx).map_err(invalid)
}

fn put_checked(
    project: &mut Project,
    mut reference: Reference,
    replacing: Option<&str>,
    ctx: &ChangeContext,
) -> Result<String, FieldError> {
    let index = replacing
        .map(|id| reference_index(&project.references, id))
        .transpose()?;
    Tables::new(project, ctx).check_reference(&reference)?;
    reference.id = new_reference_id(&project.references, &reference, index)?;
    let id = reference.id.clone();
    match index {
        Some(index) => project.references[index] = reference,
        None => project.references.push(reference),
    }
    Ok(id)
}

fn apply_checked(
    project: &mut Project,
    change: ProjectChange,
    ctx: &ChangeContext,
) -> Result<(), FieldError> {
    match change {
        ProjectChange::AddReference { reference } => {
            put_checked(project, reference, None, ctx)?;
        }
        ProjectChange::UpdateReference { id, reference } => {
            put_checked(project, reference, Some(&id), ctx)?;
        }
        ProjectChange::RemoveReference { id } => {
            let index = reference_index(&project.references, &id)?;
            project.references.remove(index);
        }
        ProjectChange::AddKind { rule } => {
            Tables::new(project, ctx).check_globs("kind", &rule.columns, &rule.in_meta)?;
            project.kinds.push(rule);
        }
        ProjectChange::UpdateKind { index, rule } => {
            check_index(index, project.kinds.len())?;
            Tables::new(project, ctx).check_globs("kind", &rule.columns, &rule.in_meta)?;
            project.kinds[index] = rule;
        }
        ProjectChange::RemoveKind { index } => {
            check_index(index, project.kinds.len())?;
            project.kinds.remove(index);
        }
        ProjectChange::AddStyle { mut rule } => {
            Tables::new(project, ctx).check_style(&rule)?;
            rule.id = new_style_id(&project.styles, &rule)?;
            if let Some(shipped) = preset_style(ctx, &rule.id) {
                project.switch_preset_style(shipped, true);
            }
            project.styles.push(rule);
        }
        ProjectChange::UpdateStyle { id, mut rule } => {
            let index = project.styles.iter().position(|s| s.id == id);
            if index.is_none() && preset_style(ctx, &id).is_none() {
                let message = format!("no value style `{id}` in the project or its presets");
                return Err(FieldError::new("id", message));
            }
            Tables::new(project, ctx).check_style(&rule)?;
            if rule.id.is_empty() || index.is_none() {
                rule.id = id;
            } else if project
                .styles
                .iter()
                .enumerate()
                .any(|(i, s)| Some(i) != index && s.id == rule.id)
            {
                let message = format!("`{}` is already used in this project", rule.id);
                return Err(FieldError::new("style.id", message));
            }
            match index {
                Some(index) => project.styles[index] = rule,
                None => project.styles.push(rule),
            }
        }
        ProjectChange::RemoveStyle { id } => {
            let index = project
                .styles
                .iter()
                .position(|s| s.id == id)
                .ok_or_else(|| {
                    FieldError::new("id", format!("the project has no value style `{id}`"))
                })?;
            project.styles.remove(index);
        }
        ProjectChange::EnableStyle { id } => switch_style(project, ctx, &id, true)?,
        ProjectChange::DisableStyle { id } => switch_style(project, ctx, &id, false)?,
        ProjectChange::Disable { id } => {
            check_rule_id(&id)?;
            if !project.disabled.contains(&id) {
                project.disabled.push(id);
            }
        }
        ProjectChange::Enable { id } => {
            check_rule_id(&id)?;
            project.disabled.retain(|d| *d != id);
        }
        ProjectChange::SetIcon { rule } => {
            Tables::new(project, ctx).check_icon(&rule)?;
            match project.icons.iter_mut().find(|i| i.table == rule.table) {
                Some(own) => *own = rule,
                None => project.icons.push(rule),
            }
        }
        ProjectChange::RemoveIcon { table } => {
            let index = project
                .icons
                .iter()
                .position(|i| i.table == table)
                .ok_or_else(|| {
                    FieldError::new("table", format!("the project has no icon for `{table}`"))
                })?;
            project.icons.remove(index);
        }
        ProjectChange::SetPrefix { prefix } => {
            check_prefix(&ctx.schema, "prefix", prefix.as_deref())?;
            project.prefix = prefix;
        }
        ProjectChange::SetNetworkPrefix { prefix } => {
            check_prefix(&ctx.schema, "network_prefix", prefix.as_deref())?;
            project.network_prefix = prefix;
        }
        ProjectChange::SetPresets { presets } => {
            check_presets(presets.as_deref(), &ctx.presets)?;
            project.presets = presets;
        }
    }
    Ok(())
}

/// The style with `id` in one of the loaded presets.
fn preset_style<'c>(ctx: &'c ChangeContext, id: &str) -> Option<&'c StyleRule> {
    ctx.presets
        .iter()
        .flat_map(|p| &p.styles)
        .find(|s| s.id == id)
}

fn switch_style(
    project: &mut Project,
    ctx: &ChangeContext,
    id: &str,
    on: bool,
) -> Result<(), FieldError> {
    if let Some(shipped) = preset_style(ctx, id) {
        project.switch_preset_style(shipped, on);
        return Ok(());
    }
    let own = project
        .styles
        .iter_mut()
        .find(|s| s.id == id)
        .ok_or_else(|| {
            FieldError::new(
                "id",
                format!("no value style `{id}` in the project or its presets"),
            )
        })?;
    own.enabled = on;
    Ok(())
}

#[cfg(test)]
mod tests;
