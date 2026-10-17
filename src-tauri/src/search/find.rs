//! `=<value>`, find everywhere: the value column of every `[[meta]]` table (exact), the `[[find]]`
//! sources, and the entities' `search` (contains) and `search_exact` (exact) columns, one section
//! per table and column. The sources of one table share one query.

use std::collections::BTreeMap;

use futures_util::future::join_all;

use super::hits;
use super::{PaletteHit, SECTION_CAP};
use crate::db::ResultSet;
use crate::query::{Ctx, KeyOrder};
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
    let mut by_table: BTreeMap<&str, Vec<&Source>> = BTreeMap::new();
    for source in &sources {
        by_table.entry(&source.table).or_default().push(source);
    }
    let found = join_all(
        by_table
            .into_values()
            .map(|same_table| table_hits(ctx, same_table, value)),
    )
    .await;
    let mut found: Vec<(&Source, Vec<PaletteHit>)> = found.into_iter().flatten().collect();
    found.sort_by_key(|(source, _)| sources.iter().position(|s| s == *source));
    found.into_iter().flat_map(|(_, hits)| hits).collect()
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

/// The hits of sources on one table, read in one query.
async fn table_hits<'s>(
    ctx: &Ctx<'_>,
    sources: Vec<&'s Source>,
    value: &str,
) -> Vec<(&'s Source, Vec<PaletteHit>)> {
    let table = &sources[0].table;
    let filters: Vec<String> = sources.iter().map(|s| held(ctx, s, value)).collect();
    let key: Vec<String> = ctx
        .single_key(table)
        .map(str::to_string)
        .into_iter()
        .collect();
    let order = KeyOrder {
        columns: &key,
        descending: true,
    };
    let results = ctx
        .rows_per_filter(table, ALIAS, &filters, &order, SECTION_CAP)
        .await
        .unwrap_or_default();
    let hits = join_all(
        sources
            .iter()
            .zip(results)
            .map(|(source, result)| source_hits(ctx, source, result)),
    )
    .await;
    sources.into_iter().zip(hits).collect()
}

fn held(ctx: &Ctx<'_>, source: &Source, value: &str) -> String {
    let column = ctx.column(Some(ALIAS), &source.column);
    if source.contains {
        format!("{column} LIKE {}", ctx.dialect.like_contains(value))
    } else {
        format!("{column} = {}", ctx.literal(value))
    }
}

async fn source_hits(ctx: &Ctx<'_>, source: &Source, result: ResultSet) -> Vec<PaletteHit> {
    let rows = ctx.annotate(&source.table, result);
    let section = format!("{}.{}", source.table, source.column);
    match ctx.rules.meta_table(&source.table) {
        Some(meta) => hits::from_meta(ctx, &rows, meta, &section),
        None => hits::from_rows(ctx, &rows, None, &section, Some(&source.column)).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::testing::{Rig, num, result, shop, text};

    const FIND_LOG_MESSAGES: &str = r#"
[[find]]
table = "log"
columns = ["msg"]
match = "contains"
"#;

    #[tokio::test]
    async fn sections_follow_the_declaration_order_not_the_table_names() {
        let meta = result(
            &["meta_id", "order_id", "k", "v"],
            vec![vec![num("1"), num("1"), text("_note"), text("sandbox")]],
        );
        let log = result(
            &["log_id", "order_id", "at_gmt", "msg"],
            vec![vec![num("7"), num("1"), text(""), text("sandbox ok")]],
        );
        let rig = Rig::new(
            shop().on("`f_ordermeta`", meta).on("`f_log`", log),
            FIND_LOG_MESSAGES,
        );
        let ctx = rig.ctx().await;

        let found = everywhere(&ctx, "sandbox").await;

        let sections: Vec<&str> = found.iter().map(|hit| hit.section.as_str()).collect();
        assert_eq!(sections, ["f_ordermeta.v", "f_log.msg"]);
    }
}
