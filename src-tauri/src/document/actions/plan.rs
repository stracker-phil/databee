//! Plans an action on a row or a table. Structured effects become `UpdateWhere`/`DeleteWhere`
//! requests, which read the rows first and keep their before-image for undo; free SQL is planned
//! as written, without undo, its `preview_sql` result in the description.

use std::collections::BTreeMap;

use super::super::context::Ctx;
use super::super::own::{self, equals_sql};
use super::params::{self, ParamValue};
use super::rules;
use crate::address::{Address, Target};
use crate::db::changes::{self, ChangeRequest, Plan};
use crate::db::where_changes::{RowsDelete, RowsUpdate};
use crate::error::{AppError, AppResult};
use crate::rules::actions::{ActionEffect, ActionScope, DeleteSpec, SetValue, UpdateStep};
use crate::rules::condition::holds;
use crate::rules::coverage;
use crate::rules::effective::RuleAction;
use crate::rules::placeholder::{Placeholder, scan};

/// What an action's SQL is rendered with.
struct Scope<'a> {
    ctx: &'a Ctx<'a>,
    action: &'a RuleAction,
    /// The row's key filter, for a row action.
    row: Option<String>,
    /// The row's single-column key, for `{id}`.
    id: Option<String>,
    params: BTreeMap<String, ParamValue>,
}

/// How a row action's step finds its rows; a table action selects by `when` and `where` only.
#[derive(Clone, Copy)]
enum Selection<'a> {
    /// The action's row itself.
    Row,
    /// The rows whose `key` (default: the table's primary key) holds `{id}`.
    ByKey(Option<&'a str>),
}

/// The plan of action `name` on the row or table at `address` (a table address without a filter
/// for a table action).
pub async fn plan(
    ctx: &Ctx<'_>,
    address: &Address,
    name: &str,
    given: &BTreeMap<String, String>,
) -> AppResult<Plan> {
    let (table, filter) = match &address.target {
        Target::Table { table, filter } if filter.is_empty() => (table.clone(), None),
        _ => {
            let (table, filter) = own::row_filter(ctx, address)?;
            (table, Some(filter))
        }
    };
    let scope = if filter.is_some() {
        ActionScope::Row
    } else {
        ActionScope::Table
    };
    let action = rules(ctx, &table, scope)
        .find(|a| a.rule.name == name)
        .ok_or_else(|| AppError::InvalidInput(format!("`{table}` has no action `{name}`")))?;
    let (row, id) = match &filter {
        Some(filter) => {
            let loaded = own::load(ctx, &table, filter).await?;
            if !holds(&action.when, |column| loaded.column_text(column)) {
                return Err(AppError::InvalidInput(format!(
                    "`{name}` does not apply to this row"
                )));
            }
            let id = loaded.id(ctx).map(str::to_string);
            (Some(equals_sql(ctx, None, filter)), id)
        }
        None => (None, None),
    };
    let params = params::values(ctx, name, &action.rule.params, given).await?;
    let scope = Scope {
        ctx,
        action,
        row,
        id,
        params,
    };
    let request = match &action.rule.effect {
        ActionEffect::Update(steps) => scope.update_request(steps)?,
        ActionEffect::Delete(spec) => scope.delete_request(spec)?,
        ActionEffect::Sql { sql, preview_sql } => {
            let plan = scope.free_sql(sql, preview_sql.as_deref()).await?;
            return Ok(named(name, plan));
        }
    };
    let plan = changes::plan(ctx.session.db.as_ref(), &request, ctx.timeout).await?;
    Ok(named(name, plan))
}

fn named(name: &str, plan: Plan) -> Plan {
    Plan {
        description: format!("{name}: {}", plan.description),
        ..plan
    }
}

/// A value placeholder that is the whole text. A lone `{p}` is text, not a value.
fn is_lone_placeholder(text: &str) -> bool {
    match scan(text).as_slice() {
        [(range, placeholder)] => {
            range.len() == text.len()
                && !matches!(
                    placeholder,
                    Placeholder::Prefix | Placeholder::NetworkPrefix
                )
        }
        _ => false,
    }
}

impl Scope<'_> {
    fn update_request(&self, steps: &[UpdateStep]) -> AppResult<ChangeRequest> {
        let updates = steps
            .iter()
            .filter_map(|step| self.update(step).transpose())
            .collect::<AppResult<Vec<_>>>()?;
        Ok(ChangeRequest::UpdateWhere {
            database: self.ctx.database.clone(),
            updates,
        })
    }

    fn delete_request(&self, spec: &DeleteSpec) -> AppResult<ChangeRequest> {
        let table = &self.action.rule.table;
        Ok(ChangeRequest::DeleteWhere {
            database: self.ctx.database.clone(),
            delete: RowsDelete {
                table: table.clone(),
                key_columns: self.key_columns(table)?,
                filter: self.filter(table, Selection::Row, spec.r#where.as_deref())?,
            },
        })
    }

    /// `None` for a step on a table this database does not have.
    fn update(&self, step: &UpdateStep) -> AppResult<Option<RowsUpdate>> {
        let table = match &step.table {
            None => self.action.rule.table.clone(),
            Some(name) => {
                let Some(table) = self.ctx.rules.applied.existing_table(name) else {
                    return Ok(None);
                };
                table
            }
        };
        let key = step.key.as_deref();
        let selection = if step.table.is_none() && key.is_none() {
            Selection::Row
        } else {
            Selection::ByKey(key)
        };
        let set = step
            .set
            .iter()
            .map(|(column, value)| {
                let expr = match value {
                    SetValue::Text(text) => self.text(text)?,
                    SetValue::Sql { sql } => self.sql(sql)?,
                };
                Ok((column.clone(), expr))
            })
            .collect::<AppResult<Vec<_>>>()?;
        Ok(Some(RowsUpdate {
            key_columns: self.key_columns(&table)?,
            filter: self.filter(&table, selection, step.r#where.as_deref())?,
            table,
            set,
        }))
    }

    fn key_columns(&self, table: &str) -> AppResult<Vec<String>> {
        let key = self.ctx.primary_key(table);
        if key.is_empty() {
            return Err(AppError::InvalidInput(format!(
                "`{table}` has no primary key"
            )));
        }
        Ok(key.to_vec())
    }

    /// The `selection` on a row action, the action's `when` on its own table, the step's `where`.
    fn filter(
        &self,
        table: &str,
        selection: Selection<'_>,
        r#where: Option<&str>,
    ) -> AppResult<String> {
        let ctx = self.ctx;
        let mut parts = Vec::new();
        if let Some(row) = &self.row {
            match selection {
                Selection::Row => parts.push(row.clone()),
                Selection::ByKey(key) => {
                    let column = key.or_else(|| ctx.single_key(table)).ok_or_else(|| {
                        AppError::InvalidInput(format!(
                            "`{table}` has no single-column primary key"
                        ))
                    })?;
                    parts.push(format!("{} = {}", ctx.ident(column), self.id_literal()?));
                }
            }
        }
        if table == self.action.rule.table {
            let table_ref = ctx.table_ref(table);
            parts.extend(coverage::conditions(
                ctx.dialect,
                &table_ref,
                &self.action.when,
            ));
        }
        if let Some(w) = r#where {
            parts.push(format!("({})", self.sql(w)?));
        }
        if parts.is_empty() {
            return Err(AppError::InvalidInput(
                "A table action selects its rows by `where`".into(),
            ));
        }
        Ok(parts.join(" AND "))
    }

    /// `{id}` as a number when it is one, else as a string literal.
    fn id_literal(&self) -> AppResult<String> {
        let id = self.id.as_deref().ok_or_else(|| {
            AppError::InvalidInput("`{id}` needs a row with a single-column key".into())
        })?;
        Ok(match id.parse::<u64>() {
            Ok(n) => n.to_string(),
            Err(_) => self.ctx.literal(id),
        })
    }

    /// Preset SQL with every placeholder as an SQL literal.
    fn sql(&self, sql: &str) -> AppResult<String> {
        self.render(sql, true)
    }

    /// A text `set` value: a lone placeholder is its literal (NULL stays NULL); otherwise the
    /// placeholders are filled in as text and the whole is one string literal.
    fn text(&self, text: &str) -> AppResult<String> {
        if is_lone_placeholder(text) {
            return self.render(text, true);
        }
        Ok(self.ctx.literal(&self.render(text, false)?))
    }

    fn render(&self, sql: &str, literals: bool) -> AppResult<String> {
        let sql = if literals {
            self.ctx.qualified(sql)
        } else {
            sql.to_string()
        };
        let mut out = String::with_capacity(sql.len());
        let mut last = 0;
        for (range, placeholder) in scan(&sql) {
            let value = self.value(placeholder, literals).ok_or_else(|| {
                AppError::InvalidInput(format!("`{}` has no value here", &sql[range.clone()]))
            })?;
            out.push_str(&sql[last..range.start]);
            out.push_str(&value);
            last = range.end;
        }
        out.push_str(&sql[last..]);
        Ok(out)
    }

    fn value(&self, placeholder: Placeholder<'_>, literal: bool) -> Option<String> {
        let applied = &self.ctx.rules.applied;
        match placeholder {
            Placeholder::Prefix => Some(applied.prefix.clone()),
            Placeholder::NetworkPrefix => Some(applied.network_prefix.clone()),
            Placeholder::Id if literal => self.id_literal().ok(),
            Placeholder::Id => self.id.clone(),
            Placeholder::Param(name) => {
                let value = self.params.get(name)?;
                Some(if literal {
                    value.literal(self.ctx)
                } else {
                    value.text()
                })
            }
            Placeholder::Owner(_) | Placeholder::Meta(_) => None,
        }
    }

    async fn free_sql(&self, sql: &str, preview_sql: Option<&str>) -> AppResult<Plan> {
        let ctx = self.ctx;
        let statement = self.sql(sql)?;
        let preview = match preview_sql {
            Some(preview) => {
                let result = ctx.fetch(&self.sql(preview)?).await?;
                let value = result
                    .rows
                    .first()
                    .and_then(|row| row.first()?.as_text().map(str::to_string))
                    .unwrap_or_else(|| "NULL".into());
                format!("; preview_sql: {value}")
            }
            None => String::new(),
        };
        Ok(Plan {
            database: ctx.database.clone(),
            description: format!("free SQL on {} (no undo{preview})", self.action.rule.table),
            statements: vec![statement],
            undo: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::context::testing::{Rig, count, num, result, shop, text};

    const ACTIONS: &str = r#"
[[action]]
name = "rename"
table = "orders"
params = [{ name = "status" }]
update = [{ set = { status = "{param.status}" } }]

[[action]]
name = "purge"
table = "log"
scope = "table"
params = [{ name = "msg" }]
sql = "DELETE FROM {p}log WHERE msg = {param.msg}"
preview_sql = "SELECT COUNT(*) FROM {p}log WHERE msg = {param.msg}"
"#;

    fn given(name: &str, value: &str) -> BTreeMap<String, String> {
        BTreeMap::from([(name.to_string(), value.to_string())])
    }

    #[tokio::test]
    async fn a_quote_in_a_param_value_is_escaped_in_an_update_statement() {
        let db = shop()
            .on(
                "SELECT t.*",
                result(&["id", "status"], vec![vec![num("1"), text("open")]]),
            )
            .on(
                "SELECT `id`, `status`",
                result(&["id", "status"], vec![vec![num("1"), text("open")]]),
            );
        let rig = Rig::new(db, ACTIONS);
        let ctx = rig.ctx().await;
        let address = ctx.row_address("f_orders", "1");

        let plan = plan(&ctx, &address, "rename", &given("status", "it's"))
            .await
            .unwrap();

        assert_eq!(plan.description, "rename: Update 1 f_orders row");
        assert_eq!(
            plan.statements,
            ["UPDATE `wp`.`f_orders` SET `status` = 'it''s' WHERE `id` IN ('1')"]
        );
        assert_eq!(
            plan.undo,
            ["UPDATE `wp`.`f_orders` SET `status` = 'open' WHERE `id` = '1'"]
        );
    }

    #[tokio::test]
    async fn a_quote_in_a_param_value_is_escaped_in_free_sql_and_its_preview() {
        let rig = Rig::new(shop().on("SELECT COUNT(*)", count(3)), ACTIONS);
        let ctx = rig.ctx().await;
        let address = ctx.address(Target::Table {
            table: "f_log".into(),
            filter: Vec::new(),
        });

        let plan = plan(&ctx, &address, "purge", &given("msg", "it's"))
            .await
            .unwrap();

        assert_eq!(
            plan.statements,
            ["DELETE FROM `wp`.f_log WHERE msg = 'it''s'"]
        );
        assert!(plan.undo.is_empty());
        assert_eq!(
            plan.description,
            "purge: free SQL on f_log (no undo; preview_sql: 3)"
        );
        assert_eq!(
            rig.db.queries(),
            ["SELECT COUNT(*) FROM `wp`.f_log WHERE msg = 'it''s'"]
        );
    }

    #[tokio::test]
    async fn an_action_the_table_does_not_have_is_refused() {
        let rig = Rig::new(shop(), ACTIONS);
        let ctx = rig.ctx().await;
        let address = ctx.address(Target::Table {
            table: "f_log".into(),
            filter: Vec::new(),
        });

        let err = plan(&ctx, &address, "rename", &BTreeMap::new())
            .await
            .unwrap_err();

        assert!(
            matches!(&err, AppError::InvalidInput(m) if m == "`f_log` has no action `rename`"),
            "{err:?}"
        );
    }
}
