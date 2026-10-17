//! The row itself: read with the filter of every entity of its table, so the row picks its entity
//! in the same query (the first entity whose filter holds; filtered ones come first).

use super::context::{Ctx, Rows};
use crate::address::{Address, Target};
use crate::db::ColumnEquals;
use crate::error::{AppError, AppResult};
use crate::rules::effective::AppliedEntity;
use crate::rules::placeholder::with_prefixes;

const ENTITY_COLUMN: &str = "__databee_entity_";

pub struct Own {
    pub rows: Rows,
    pub entity: Option<AppliedEntity>,
}

impl Own {
    pub fn table(&self) -> &str {
        &self.rows.table
    }

    /// The row's text in `column`: `None` without such a column or for NULL.
    pub fn text(&self, column: &str) -> Option<&str> {
        self.column_text(column).flatten()
    }

    /// `None` without such a column, `Some(None)` for NULL, as conditions test it.
    pub fn column_text(&self, column: &str) -> Option<Option<&str>> {
        self.rows.text(0, column)
    }

    /// The single-column key's value, for `{id}` and the `fk` of meta rows.
    pub fn id(&self, ctx: &Ctx<'_>) -> Option<&str> {
        self.text(ctx.single_key(self.table())?)
    }
}

/// The table and the `=` conditions that select the row of `address`: a single-column key, or
/// a filter on every column of a composite key.
pub fn row_filter(ctx: &Ctx<'_>, address: &Address) -> AppResult<(String, Vec<ColumnEquals>)> {
    match &address.target {
        Target::Row { table, key } => {
            let column = ctx.single_key(table).ok_or_else(|| {
                AppError::InvalidInput(format!("`{table}` has no single-column primary key"))
            })?;
            let filter = vec![ColumnEquals {
                column: column.to_string(),
                value: Some(key.clone()),
            }];
            Ok((table.clone(), filter))
        }
        Target::Table { table, filter } if covers_key(ctx.primary_key(table), filter) => {
            Ok((table.clone(), filter.clone()))
        }
        _ => Err(AppError::InvalidInput(
            "A document needs the address of one row".into(),
        )),
    }
}

fn covers_key(key: &[String], filter: &[ColumnEquals]) -> bool {
    !key.is_empty()
        && key
            .iter()
            .all(|k| filter.iter().any(|f| f.column.eq_ignore_ascii_case(k)))
}

pub async fn load(ctx: &Ctx<'_>, table: &str, filter: &[ColumnEquals]) -> AppResult<Own> {
    let entities: Vec<&AppliedEntity> = ctx
        .rules
        .entities
        .iter()
        .filter(|e| e.entity.table == table)
        .collect();
    let tests: Vec<String> = entities
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let test = match &e.entity.filter {
                Some(filter) => format!("({})", entity_filter(ctx, filter)),
                None => "1".into(),
            };
            format!(", {test} AS {}", ctx.ident(&format!("{ENTITY_COLUMN}{i}")))
        })
        .collect();
    let sql = format!(
        "SELECT t.*{} FROM {} AS t WHERE {} LIMIT 2",
        tests.concat(),
        ctx.table_ref(table),
        equals_sql(ctx, Some("t"), filter)
    );
    let mut result = ctx.fetch(&sql).await?;
    if result.rows.len() != 1 {
        return Err(AppError::InvalidInput(format!(
            "`{table}` has {} rows for this key",
            if result.rows.is_empty() {
                "no"
            } else {
                "several"
            }
        )));
    }
    let own_columns = result.columns.len() - entities.len();
    let row = result.rows.remove(0);
    let entity = row[own_columns..]
        .iter()
        .position(|cell| cell.as_text().is_some_and(|t| t != "0"))
        .map(|i| entities[i].clone());
    result.columns.truncate(own_columns);
    result.rows = vec![row[..own_columns].to_vec()];
    Ok(Own {
        rows: ctx.annotate(table, result),
        entity,
    })
}

/// An entity `filter` with the prefixes filled in, its tables qualified with the database.
pub fn entity_filter(ctx: &Ctx<'_>, filter: &str) -> String {
    let applied = &ctx.rules.applied;
    with_prefixes(
        &ctx.qualified(filter),
        &applied.prefix,
        &applied.network_prefix,
    )
}

/// `a = 'x' AND b IS NULL`, the columns qualified with `alias`.
pub fn equals_sql(ctx: &Ctx<'_>, alias: Option<&str>, filter: &[ColumnEquals]) -> String {
    filter
        .iter()
        .map(|f| match &f.value {
            Some(value) => format!("{} = {}", ctx.column(alias, &f.column), ctx.literal(value)),
            None => format!("{} IS NULL", ctx.column(alias, &f.column)),
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}
