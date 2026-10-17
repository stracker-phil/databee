//! Rule actions (`[[action]]`): listed on a row's document while their `when` holds, or on a
//! table (`scope = "table"`), and planned through the core plan machinery (`plan`).

mod params;
mod plan;
mod preview;
#[cfg(test)]
mod server_tests;

use serde::Serialize;
use ts_rs::TS;

use super::own::Own;
use crate::query::Ctx;
use crate::rules::actions::{ActionEffect, ActionScope};
use crate::rules::condition::holds;
use crate::rules::effective::RuleAction;

pub use params::ActionParam;
pub use plan::{plan, planned};
pub use preview::{ActionPreview, Previews};

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ObjectAction {
    pub name: String,
    pub preset: String,
    pub kind: ObjectActionKind,
    pub undo: bool,
    pub params: Vec<ActionParam>,
    pub writes: Vec<ActionWrite>,
}

/// What one step of an action writes, on the table as stored (with its prefix).
#[derive(Serialize, Debug, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "write", rename_all = "camelCase")]
pub enum ActionWrite {
    Set { table: String, columns: Vec<String> },
    DeleteRows { table: String },
    FreeSql { table: String },
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
        writes: writes(ctx, action),
    }
}

/// The steps in planning order; an update step on a table this database lacks is left out, as
/// planning skips it.
fn writes(ctx: &Ctx<'_>, action: &RuleAction) -> Vec<ActionWrite> {
    let table = action.rule.table.clone();
    match &action.rule.effect {
        ActionEffect::Update(steps) => steps
            .iter()
            .filter_map(|step| {
                let table = match &step.table {
                    None => table.clone(),
                    Some(name) => ctx.rules.applied.existing_table(name)?,
                };
                let columns = step.set.keys().cloned().collect();
                Some(ActionWrite::Set { table, columns })
            })
            .collect(),
        ActionEffect::Delete(_) => vec![ActionWrite::DeleteRows { table }],
        ActionEffect::Sql { .. } => vec![ActionWrite::FreeSql { table }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::own;
    use crate::query::testing::{Rig, num, result, shop, text};

    const ACTIONS: &str = r#"
[[action]]
name = "set_status"
table = "orders"
params = [{ name = "status" }]
update = [
  { set = { status = "{param.status}", created_gmt = { sql = "UTC_TIMESTAMP()" } } },
  { table = "log", key = "order_id", set = { msg = "{param.status}", at_gmt = { sql = "NOW()" } } },
  { table = "missing", set = { state = "{param.status}" } },
]

[[action]]
name = "set_status"
table = "log"
params = [{ name = "status" }]
update = [
  { set = { msg = "{param.status}" } },
  { table = "orders", key = "id", set = { status = "{param.status}" } },
]

[[action]]
name = "purge"
table = "log"
scope = "table"
sql = "DELETE FROM {p}log WHERE msg = ''"

[[action]]
name = "drop_people"
table = "people"
scope = "table"
delete = { where = "name = ''" }
"#;

    fn set(table: &str, columns: &[&str]) -> ActionWrite {
        ActionWrite::Set {
            table: table.into(),
            columns: columns.iter().map(|c| c.to_string()).collect(),
        }
    }

    async fn row_writes(rig: &Rig, table: &str) -> Vec<ActionWrite> {
        let ctx = rig.ctx().await;
        let address = ctx.row_address(table, "1");
        let (table, filter) = own::row_filter(&ctx, &address).unwrap();
        let row = own::load(&ctx, &table, &filter).await.unwrap();
        let actions = for_row(&ctx, &row).await;
        assert_eq!(actions.len(), 1, "{actions:?}");
        assert!(actions[0].undo);
        actions[0].writes.clone()
    }

    #[tokio::test]
    async fn an_update_writes_the_columns_of_each_step_on_its_stored_table() {
        let db = shop().on(
            "SELECT t.*",
            result(&["id", "status"], vec![vec![num("1"), text("open")]]),
        );
        let rig = Rig::new(db, ACTIONS);

        let writes = row_writes(&rig, "f_orders").await;

        assert_eq!(
            writes,
            [
                set("f_orders", &["created_gmt", "status"]),
                set("f_log", &["at_gmt", "msg"]),
            ]
        );
    }

    #[tokio::test]
    async fn the_mirrored_update_writes_its_own_table_first() {
        let db = shop().on(
            "SELECT t.*",
            result(&["log_id", "msg"], vec![vec![num("1"), text("x")]]),
        );
        let rig = Rig::new(db, ACTIONS);

        let writes = row_writes(&rig, "f_log").await;

        assert_eq!(
            writes,
            [set("f_log", &["msg"]), set("f_orders", &["status"])]
        );
    }

    #[tokio::test]
    async fn free_sql_and_a_delete_name_their_table_without_columns() {
        let rig = Rig::new(shop(), ACTIONS);
        let ctx = rig.ctx().await;

        let purge = &for_table(&ctx, "f_log").await[0];
        let drop = &for_table(&ctx, "f_people").await[0];

        assert_eq!(
            purge.writes,
            [ActionWrite::FreeSql {
                table: "f_log".into()
            }]
        );
        assert!(!purge.undo);
        assert_eq!(
            drop.writes,
            [ActionWrite::DeleteRows {
                table: "f_people".into()
            }]
        );
        assert!(drop.undo);
    }
}
