//! Rule actions (`[[action]]`): listed on a row's document while their `when` holds, or on a
//! table (`scope = "table"`), and planned through the core plan machinery (`plan`).

mod params;
mod plan;
#[cfg(test)]
mod server_tests;

use serde::Serialize;
use ts_rs::TS;

use super::context::Ctx;
use super::own::Own;
use crate::rules::actions::{ActionEffect, ActionScope};
use crate::rules::condition::holds;
use crate::rules::effective::RuleAction;

pub use params::ActionParam;
pub use plan::plan;

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ObjectAction {
    pub name: String,
    /// The preset that defines it.
    pub preset: String,
    pub kind: ObjectActionKind,
    /// Applying it can be undone.
    pub undo: bool,
    pub params: Vec<ActionParam>,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub enum ObjectActionKind {
    Update,
    Delete,
    Sql,
}

impl ObjectActionKind {
    fn of(effect: &ActionEffect) -> Self {
        match effect {
            ActionEffect::Update(_) => Self::Update,
            ActionEffect::Delete(_) => Self::Delete,
            ActionEffect::Sql { .. } => Self::Sql,
        }
    }

    /// Structured effects keep a before-image; free SQL does not.
    fn can_undo(self) -> bool {
        self != Self::Sql
    }
}

/// The row actions of the document's table whose `when` holds on its row.
pub async fn for_row(ctx: &Ctx<'_>, own: &Own) -> Vec<ObjectAction> {
    let mut actions = Vec::new();
    for action in rules(ctx, own.table(), ActionScope::Row) {
        if holds(&action.when, |column| own.column_text(column)) {
            actions.push(describe(ctx, action).await);
        }
    }
    actions
}

/// The table actions of `table`.
pub async fn for_table(ctx: &Ctx<'_>, table: &str) -> Vec<ObjectAction> {
    let mut actions = Vec::new();
    for action in rules(ctx, table, ActionScope::Table) {
        actions.push(describe(ctx, action).await);
    }
    actions
}

fn rules<'c>(
    ctx: &'c Ctx<'_>,
    table: &'c str,
    scope: ActionScope,
) -> impl Iterator<Item = &'c RuleAction> {
    ctx.rules
        .actions
        .iter()
        .filter(move |a| a.rule.table == table && a.rule.scope == scope)
}

async fn describe(ctx: &Ctx<'_>, action: &RuleAction) -> ObjectAction {
    let kind = ObjectActionKind::of(&action.rule.effect);
    ObjectAction {
        name: action.rule.name.clone(),
        preset: action.preset.clone(),
        kind,
        undo: kind.can_undo(),
        params: params::describe(ctx, &action.rule.params).await,
    }
}
