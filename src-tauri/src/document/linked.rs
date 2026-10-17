//! A row without its whole document, for the palette's search: the row with its entity and the
//! rows that point at it (incoming references and patterns). Meta rows are read only when a
//! pattern of the table needs them.

use futures_util::future::join;

use super::build::needed_meta_keys;
use super::context::Ctx;
use super::meta::{self, MetaValues};
use super::own::{self, Own};
use super::{Group, incoming, patterns};
use crate::db::ColumnEquals;
use crate::error::{AppError, AppResult};

/// The row of `table` whose single-column key is `key`.
pub async fn row(ctx: &Ctx<'_>, table: &str, key: &str) -> AppResult<Own> {
    let column = ctx.single_key(table).ok_or_else(|| {
        AppError::InvalidInput(format!("`{table}` has no single-column primary key"))
    })?;
    let filter = [ColumnEquals {
        column: column.to_string(),
        value: Some(key.to_string()),
    }];
    own::load(ctx, table, &filter).await
}

/// The incoming reference and pattern groups of the row's document.
pub async fn referencing(ctx: &Ctx<'_>, own: &Own) -> Vec<Group> {
    let has_patterns = ctx
        .rules
        .patterns
        .iter()
        .any(|p| p.rule.owner == own.table());
    let meta_rows = if has_patterns {
        meta::load(ctx, own, &needed_meta_keys(ctx, own)).await
    } else {
        Vec::new()
    };
    let meta_values = MetaValues::new(&meta_rows);
    let (mut groups, found) = join(
        incoming::groups(ctx, own),
        patterns::groups(ctx, own, &meta_values),
    )
    .await;
    let (pattern_groups, _orders) = patterns::split(found);
    groups.extend(pattern_groups);
    groups
}
