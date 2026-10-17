//! `=<value>`, find everywhere: the value column of every `[[meta]]` table (exact), the `[[find]]`
//! sources, and the entities' `search` (contains) and `search_exact` (exact) columns, one section
//! per table and column.

use futures_util::future::join_all;

use super::hits;
use super::{PaletteHit, SECTION_CAP};
use crate::document::Ctx;
use crate::rules::types::FindMatch;

const ALIAS: &str = "t";

/// One searched column; a column searched both ways is searched with "contains".
#[derive(Debug, Clone, PartialEq, Eq)]
struct Source {
    table: String,
    column: String,
    contains: bool,
}

pub async fn everywhere(ctx: &Ctx<'_>, value: &str) -> Vec<PaletteHit> {
    let sources = sources(ctx);
    let found = join_all(sources.iter().map(|source| source_hits(ctx, source, value))).await;
    found.into_iter().flatten().collect()
}

fn sources(ctx: &Ctx<'_>) -> Vec<Source> {
    let mut sources: Vec<Source> = Vec::new();
    let mut add = |table: &str, column: &str, contains: bool| {
        if !ctx
            .schema
            .columns(table)
            .iter()
            .any(|c| c.eq_ignore_ascii_case(column))
        {
            return;
        }
        let same = |s: &&mut Source| s.table == table && s.column.eq_ignore_ascii_case(column);
        match sources.iter_mut().find(same) {
            Some(source) => source.contains |= contains,
            None => sources.push(Source {
                table: table.to_string(),
                column: column.to_string(),
                contains,
            }),
        }
    };
    for meta in &ctx.rules.meta {
        add(&meta.table, &meta.value, false);
    }
    for find in &ctx.rules.finds {
        for column in &find.columns {
            add(&find.table, column, find.matching == FindMatch::Contains);
        }
    }
    for applied in &ctx.rules.entities {
        let entity = &applied.entity;
        for column in &entity.search {
            add(&entity.table, column, true);
        }
        for column in &entity.search_exact {
            add(&entity.table, column, false);
        }
    }
    sources
}

async fn source_hits(ctx: &Ctx<'_>, source: &Source, value: &str) -> Vec<PaletteHit> {
    let column = ctx.column(Some(ALIAS), &source.column);
    let held = if source.contains {
        format!("{column} LIKE {}", ctx.dialect.like_contains(value))
    } else {
        format!("{column} = {}", ctx.literal(value))
    };
    let order = ctx.single_key(&source.table).map_or_else(
        || "1".to_string(),
        |key| format!("{} DESC", ctx.column(Some(ALIAS), key)),
    );
    let sql = format!(
        "SELECT * FROM {} AS {ALIAS} WHERE {held} ORDER BY {order} LIMIT {SECTION_CAP}",
        ctx.table_ref(&source.table)
    );
    let Ok(result) = ctx.fetch(&sql).await else {
        return Vec::new();
    };
    let rows = ctx.annotate(&source.table, result);
    let section = format!("{}.{}", source.table, source.column);
    match ctx.rules.meta_table(&source.table) {
        Some(meta) => hits::from_meta(ctx, &rows, meta, &section),
        None => {
            let entity = hits::table_entity(ctx, &source.table);
            hits::from_rows(ctx, &rows, &section, entity, Some(&source.column))
        }
    }
}
