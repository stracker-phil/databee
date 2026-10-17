//! Text searches over the entities' `search` (contains) and `search_exact` (exact) columns and
//! meta keys: `@` in the entities marked `people`, free text in all of them plus the non-key
//! targets of rule references (external ids, such as a payment provider's order id).

use std::collections::HashSet;

use futures_util::future::{join, join_all};

use super::hits;
use super::{PaletteHit, SECTION_CAP};
use crate::query::Ctx;
use crate::rules::condition::conditions_sql;
use crate::rules::effective::{AppliedEntity, MetaRef, RuleReference};
use crate::rules::types::Entity;

const ALIAS: &str = "t";
const META_ALIAS: &str = "m";

pub async fn people(ctx: &Ctx<'_>, text: &str) -> Vec<PaletteHit> {
    let section = format!("people matching {text}");
    let entities = ctx.rules.entities.iter().filter(|e| e.entity.people);
    let found = join_all(entities.map(|e| entity_hits(ctx, e, text, &section))).await;
    found.into_iter().flatten().take(SECTION_CAP).collect()
}

/// External ids first: an exact stored id says more than a contained text.
pub async fn free(ctx: &Ctx<'_>, text: &str) -> Vec<PaletteHit> {
    let section = format!("objects matching {text}");
    let entities = ctx
        .rules
        .entities
        .iter()
        .filter(|e| !e.entity.search.is_empty() || !e.entity.search_exact.is_empty());
    let (found, external) = join(
        join_all(entities.map(|e| entity_hits(ctx, e, text, &section))),
        external_ids(ctx, text),
    )
    .await;
    let mut out = external;
    out.extend(found.into_iter().flatten().take(SECTION_CAP));
    out
}

#[derive(Clone, Copy)]
enum Matching {
    Contains,
    Exact,
}

/// A column or meta key the entity is searched on.
struct Term<'e> {
    name: &'e str,
    matching: Matching,
}

fn terms(entity: &Entity) -> Vec<Term<'_>> {
    let mut terms = named(&entity.search, Matching::Contains);
    terms.extend(named(&entity.search_exact, Matching::Exact));
    terms
}

fn named(names: &[String], matching: Matching) -> Vec<Term<'_>> {
    names.iter().map(|name| Term { name, matching }).collect()
}

fn value_match(ctx: &Ctx<'_>, column: &str, term: &Term<'_>, text: &str) -> String {
    match term.matching {
        Matching::Contains => format!("{column} LIKE {}", ctx.dialect.like_contains(text)),
        Matching::Exact => format!("{column} = {}", ctx.literal(text)),
    }
}

fn owner_meta<'r>(ctx: &'r Ctx<'_>, table: &str) -> Option<&'r MetaRef> {
    ctx.rules.owned_meta(table).next()
}

/// The entity's newest rows where `predicate` holds besides its `filter`.
fn select_entities(ctx: &Ctx<'_>, entity: &Entity, predicate: &str) -> String {
    let table = &entity.table;
    let filter = entity
        .filter
        .as_ref()
        .map(|filter| format!("({}) AND ", ctx.preset_sql(filter)))
        .unwrap_or_default();
    let order = ctx.single_key(table).map_or_else(
        || "1".to_string(),
        |key| format!("{} DESC", ctx.column(Some(ALIAS), key)),
    );
    format!(
        "SELECT {ALIAS}.* FROM {} AS {ALIAS} WHERE {filter}({predicate}) ORDER BY {order} LIMIT {SECTION_CAP}",
        ctx.table_ref(table),
    )
}

async fn entity_hits(
    ctx: &Ctx<'_>,
    applied: &AppliedEntity,
    text: &str,
    section: &str,
) -> Vec<PaletteHit> {
    let entity = &applied.entity;
    let columns = ctx.schema.columns(&entity.table);
    let is_column = |term: &Term<'_>| columns.iter().any(|c| c.eq_ignore_ascii_case(term.name));
    let (column_terms, meta_terms): (Vec<_>, Vec<_>) =
        terms(entity).into_iter().partition(is_column);
    let queries: Vec<String> = [
        column_sql(ctx, entity, &column_terms, text),
        meta_sql(ctx, entity, &meta_terms, text),
    ]
    .into_iter()
    .flatten()
    .collect();
    let results = join_all(queries.iter().map(|sql| ctx.fetch(sql))).await;
    let mut out = Vec::new();
    for result in results {
        match result {
            Ok(result) => {
                let rows = ctx.annotate(&entity.table, result);
                out.extend(hits::from_rows(ctx, &rows, section, Some(entity), None));
            }
            Err(e) => log::debug!("Search in `{}` skipped: {e}", entity.table),
        }
    }
    out
}

fn column_sql(ctx: &Ctx<'_>, entity: &Entity, terms: &[Term<'_>], text: &str) -> Option<String> {
    if terms.is_empty() {
        return None;
    }
    let matches: Vec<String> = terms
        .iter()
        .map(|t| value_match(ctx, &ctx.column(Some(ALIAS), t.name), t, text))
        .collect();
    Some(select_entities(ctx, entity, &matches.join(" OR ")))
}

/// Terms that are not columns are keys of the entity's own `[[meta]]` table.
fn meta_sql(ctx: &Ctx<'_>, entity: &Entity, terms: &[Term<'_>], text: &str) -> Option<String> {
    if terms.is_empty() {
        return None;
    }
    let table = &entity.table;
    let meta = owner_meta(ctx, table)?;
    let (fk, key) = (meta.fk.as_ref()?, ctx.single_key(table)?);
    let stored_key = ctx.column(Some(META_ALIAS), &meta.key);
    let value = ctx.column(Some(META_ALIAS), &meta.value);
    let matches: Vec<String> = terms
        .iter()
        .map(|t| {
            let matched = value_match(ctx, &value, t, text);
            format!("({stored_key} = {} AND {matched})", ctx.literal(t.name))
        })
        .collect();
    let predicate = format!(
        "{} IN (SELECT {} FROM {} AS {META_ALIAS} WHERE {})",
        ctx.column(Some(ALIAS), key),
        ctx.column(Some(META_ALIAS), fk),
        ctx.table_ref(&meta.table),
        matches.join(" OR "),
    );
    Some(select_entities(ctx, entity, &predicate))
}

/// Rule references whose target is not the target table's key, each target column once.
fn external_targets<'r>(ctx: &'r Ctx<'_>) -> Vec<(&'r RuleReference, Vec<String>)> {
    let mut seen = HashSet::new();
    ctx.rules
        .references
        .iter()
        .filter(|r| {
            ctx.single_key(&r.to.table)
                .is_none_or(|key| !key.eq_ignore_ascii_case(&r.to.column))
        })
        .map(|r| (r, conditions_sql(ctx.dialect, ALIAS, &r.to_when)))
        .filter(|(r, to_when)| {
            seen.insert((r.to.table.clone(), r.to.column.clone(), to_when.clone()))
        })
        .collect()
}

async fn external_ids(ctx: &Ctx<'_>, text: &str) -> Vec<PaletteHit> {
    let section = format!("external id {text}");
    let targets = external_targets(ctx);
    let queries = targets.iter().map(|(reference, to_when)| {
        let mut parts = vec![format!(
            "{} = {}",
            ctx.column(Some(ALIAS), &reference.to.column),
            ctx.literal(text)
        )];
        parts.extend(to_when.iter().cloned());
        let sql = format!(
            "SELECT * FROM {} AS {ALIAS} WHERE {} LIMIT {SECTION_CAP}",
            ctx.table_ref(&reference.to.table),
            parts.join(" AND ")
        );
        async move { (*reference, ctx.fetch(&sql).await) }
    });
    let mut out = Vec::new();
    for (reference, result) in join_all(queries).await {
        let table = &reference.to.table;
        let result = match result {
            Ok(result) => result,
            Err(e) => {
                log::debug!("External ids in `{table}` skipped: {e}");
                continue;
            }
        };
        let rows = ctx.annotate(table, result);
        let found = match ctx.rules.meta_table(table) {
            Some(meta) => hits::from_meta(ctx, &rows, meta, &section),
            None => {
                let entity = hits::table_entity(ctx, table);
                hits::from_rows(ctx, &rows, &section, entity, Some(&reference.to.column))
            }
        };
        out.extend(found);
    }
    out.truncate(SECTION_CAP);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ResultSet;
    use crate::query::testing::{Rig, num, result, shop, text};

    fn numbered(columns: &[&str; 2], count: usize) -> ResultSet {
        let rows = (1..=count)
            .map(|i| vec![num(&i.to_string()), text("sandbox")])
            .collect();
        result(columns, rows)
    }

    fn summary(hits: &[PaletteHit]) -> Vec<(&str, &str)> {
        hits.iter()
            .map(|hit| (hit.section.as_str(), hit.source.as_str()))
            .collect()
    }

    #[tokio::test]
    async fn external_id_hits_come_before_entity_hits_and_entity_hits_stop_at_the_cap() {
        let rules = r#"
[[entity]]
name = "order"
table = "orders"
search = ["status"]

[[entity]]
name = "entry"
table = "log"
search = ["msg"]

[[reference]]
from = "orders.currency"
to = "people.name"
"#;
        let rig = Rig::new(
            shop()
                .on("`f_people`", numbered(&["person_id", "name"], 1))
                .on("`f_orders`", numbered(&["id", "status"], 15))
                .on("`f_log`", numbered(&["log_id", "msg"], 15)),
            rules,
        );
        let ctx = rig.ctx().await;

        let found = free(&ctx, "sandbox").await;

        let mut expected = vec![("external id sandbox", "f_people.name")];
        expected.extend([("objects matching sandbox", "f_orders"); 15]);
        expected.extend([("objects matching sandbox", "f_log"); SECTION_CAP - 15]);
        assert_eq!(summary(&found), expected);
    }

    #[tokio::test]
    async fn exact_columns_use_equals_and_contains_columns_escaped_like_inside_the_entity_filter() {
        let rules = r#"
[[entity]]
name = "order"
table = "orders"
filter = "status <> 'trash'"
search = ["status"]
search_exact = ["currency"]
"#;
        let rig = Rig::new(
            shop().on("`f_orders`", numbered(&["id", "status"], 1)),
            rules,
        );
        let ctx = rig.ctx().await;

        let found = free(&ctx, "a%b").await;

        assert_eq!(
            rig.db.queries(),
            [
                r"SELECT t.* FROM `wp`.`f_orders` AS t WHERE (status <> 'trash') AND (t.`status` LIKE '%a\\%b%' OR t.`currency` = 'a%b') ORDER BY t.`id` DESC LIMIT 20"
            ]
        );
        assert_eq!(summary(&found), [("objects matching a%b", "f_orders")]);
    }

    #[tokio::test]
    async fn people_reads_only_entities_marked_people() {
        let rules = r#"
[[entity]]
name = "order"
table = "orders"
search = ["status"]

[[entity]]
name = "person"
table = "people"
people = true
search = ["name"]
"#;
        let rig = Rig::new(
            shop()
                .on("`f_orders`", numbered(&["id", "status"], 1))
                .on("`f_people`", numbered(&["person_id", "name"], 1)),
            rules,
        );
        let ctx = rig.ctx().await;

        let found = people(&ctx, "jane").await;

        assert_eq!(
            rig.db.queries(),
            [
                "SELECT t.* FROM `wp`.`f_people` AS t WHERE (t.`name` LIKE '%jane%') ORDER BY t.`person_id` DESC LIMIT 20"
            ]
        );
        assert_eq!(summary(&found), [("people matching jane", "f_people")]);
    }
}
