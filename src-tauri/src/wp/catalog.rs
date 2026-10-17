//! The entities of the WordPress screens, read from the effective rules: each `[[entity]]` with
//! its owned `[[meta]]` table, the references that leave its rows (links) and the labelled ones
//! that arrive (relations), plus the SQL that loads them.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;
use ts_rs::TS;

use crate::db::Dialect;
use crate::error::{AppError, AppResult};
use crate::rules::condition::Condition;
use crate::rules::coverage;
use crate::rules::effective::{EffectiveRules, RuleReference};
use crate::rules::types::{Entity, Match};
use crate::schema::DatabaseSchema;

/// An owned key/value table; all names are real.
#[derive(Debug, Clone, PartialEq)]
pub struct MetaDef {
    pub table: String,
    pub fk: String,
    pub key: String,
    pub value: String,
}

/// Rows of `table` whose `fk` holds the entity's id and whose `when` holds; named by the
/// reference's label.
#[derive(Debug, Clone)]
pub struct RelationDef {
    pub name: String,
    pub entity: String,
    pub table: String,
    pub fk: String,
    pub when: Vec<Condition>,
}

#[derive(Debug, Clone)]
pub struct EntityDef {
    pub name: String,
    /// The real table.
    pub table: String,
    pub id: String,
    /// Columns or meta keys shown as the document's title.
    pub summary: Vec<String>,
    /// The summary entries that are columns of `table`.
    pub summary_columns: Vec<String>,
    pub filter: Option<String>,
    pub meta: Option<MetaDef>,
    /// Column → entity of the row it references.
    pub links: BTreeMap<String, String>,
    /// Meta key → entity of the row its value references.
    pub meta_links: BTreeMap<String, String>,
    pub relations: Vec<RelationDef>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct TableEntity {
    pub entity: String,
    pub id_column: String,
}

/// One definition per entity name; an unfiltered entity wins over a filtered one.
#[derive(Debug, Clone)]
pub struct Catalog {
    defs: Vec<EntityDef>,
}

impl Catalog {
    pub fn new(rules: &EffectiveRules, schema: &DatabaseSchema) -> Self {
        let chosen = choose(rules, schema);
        let keys: Vec<EntityKey<'_>> = chosen
            .iter()
            .map(|(e, _)| entity_key(&e.name, &e.table, &e.filter))
            .collect();
        let table_entity = |table: &str| entity_of_table(&keys, table);
        let defs = chosen
            .iter()
            .map(|(e, id)| entity_def(rules, schema, e, id, &table_entity))
            .collect();
        Self { defs }
    }

    pub fn get(&self, name: &str) -> AppResult<&EntityDef> {
        self.defs
            .iter()
            .find(|d| d.name == name)
            .ok_or_else(|| AppError::InvalidInput(format!("Unknown entity '{name}'")))
    }

    pub fn has(&self, name: &str) -> bool {
        self.defs.iter().any(|d| d.name == name)
    }

    /// Real table → the entity its rows open as, so table rows can be opened as documents.
    pub fn table_entities(&self) -> BTreeMap<String, TableEntity> {
        let keys: Vec<EntityKey<'_>> = self
            .defs
            .iter()
            .map(|d| entity_key(&d.name, &d.table, &d.filter))
            .collect();
        self.defs
            .iter()
            .filter_map(|d| {
                let entity = entity_of_table(&keys, &d.table)?;
                Some((
                    d.table.clone(),
                    TableEntity {
                        entity,
                        id_column: d.id.clone(),
                    },
                ))
            })
            .collect()
    }
}

/// Name, real table and filter of an entity.
type EntityKey<'a> = (&'a str, &'a str, Option<&'a str>);

fn entity_key<'a>(name: &'a str, table: &'a str, filter: &'a Option<String>) -> EntityKey<'a> {
    (name, table, filter.as_deref())
}

fn entity_def(
    rules: &EffectiveRules,
    schema: &DatabaseSchema,
    e: &Entity,
    id: &str,
    table_entity: &impl Fn(&str) -> Option<String>,
) -> EntityDef {
    let meta = owned_meta(rules, &e.table);
    EntityDef {
        name: e.name.clone(),
        links: links(rules, schema, &e.table, table_entity),
        meta_links: meta
            .as_ref()
            .map(|m| meta_links(rules, schema, m, table_entity))
            .unwrap_or_default(),
        relations: relations(rules, &e.table, id, e.filter.is_some(), table_entity),
        summary_columns: e
            .summary
            .iter()
            .filter(|s| has_column(schema, &e.table, s))
            .cloned()
            .collect(),
        summary: e.summary.clone(),
        filter: e.filter.clone(),
        table: e.table.clone(),
        id: id.to_string(),
        meta,
    }
}

/// One entity per name, unfiltered ones first, each with its table's single-column key.
fn choose<'r>(rules: &'r EffectiveRules, schema: &'r DatabaseSchema) -> Vec<(&'r Entity, &'r str)> {
    let unfiltered_first = rules
        .entities
        .iter()
        .filter(|e| e.entity.filter.is_none())
        .chain(rules.entities.iter().filter(|e| e.entity.filter.is_some()));
    let mut chosen: Vec<(&Entity, &str)> = Vec::new();
    for applied in unfiltered_first {
        let entity = &applied.entity;
        if chosen.iter().any(|(e, _)| e.name == entity.name) {
            continue;
        }
        if let Some(id) = single_key(schema, &entity.table) {
            chosen.push((entity, id));
        }
    }
    chosen
}

/// The entity a row of `table` opens as when nothing narrows it: the unfiltered one, else the
/// first.
fn entity_of_table(chosen: &[EntityKey<'_>], table: &str) -> Option<String> {
    let on_table = || chosen.iter().filter(|(_, t, _)| *t == table);
    on_table()
        .find(|(_, _, filter)| filter.is_none())
        .or_else(|| on_table().next())
        .map(|(name, _, _)| name.to_string())
}

fn single_key<'s>(schema: &'s DatabaseSchema, table: &str) -> Option<&'s str> {
    match schema.keys(table)?.primary_key.as_slice() {
        [key] => Some(key),
        _ => None,
    }
}

fn has_column(schema: &DatabaseSchema, table: &str, column: &str) -> bool {
    schema
        .columns(table)
        .iter()
        .any(|c| c.eq_ignore_ascii_case(column))
}

fn owned_meta(rules: &EffectiveRules, table: &str) -> Option<MetaDef> {
    let meta = rules
        .meta
        .iter()
        .find(|m| m.owner.as_deref() == Some(table))?;
    Some(MetaDef {
        table: meta.table.clone(),
        fk: meta.fk.clone()?,
        key: meta.key.clone(),
        value: meta.value.clone(),
    })
}

/// A reference to the single-column primary key of a table with an entity: its entity.
fn target_entity(
    schema: &DatabaseSchema,
    rule: &RuleReference,
    table_entity: &impl Fn(&str) -> Option<String>,
) -> Option<String> {
    let key = single_key(schema, &rule.to.table)?;
    if !key.eq_ignore_ascii_case(&rule.to.column) || !rule.to_when.is_empty() {
        return None;
    }
    table_entity(&rule.to.table)
}

fn links(
    rules: &EffectiveRules,
    schema: &DatabaseSchema,
    table: &str,
    table_entity: &impl Fn(&str) -> Option<String>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let unconditional = rules
        .references
        .iter()
        .filter(|r| r.from.table == table && r.when.is_empty());
    for rule in unconditional {
        if let Some(entity) = target_entity(schema, rule, table_entity) {
            out.entry(rule.from.column.clone()).or_insert(entity);
        }
    }
    out
}

/// References from the meta value column whose only condition names meta keys (`=` or IN).
fn meta_links(
    rules: &EffectiveRules,
    schema: &DatabaseSchema,
    meta: &MetaDef,
    table_entity: &impl Fn(&str) -> Option<String>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let from_values = rules
        .references
        .iter()
        .filter(|r| r.from.table == meta.table && r.from.column == meta.value);
    for rule in from_values {
        let [condition] = rule.when.as_slice() else {
            continue;
        };
        if condition.column != meta.key {
            continue;
        }
        let keys = match &condition.rule {
            Match::Eq(key) => vec![key.clone()],
            Match::In(keys) => keys.clone(),
            _ => continue,
        };
        let Some(entity) = target_entity(schema, rule, table_entity) else {
            continue;
        };
        for key in keys {
            out.entry(key).or_insert_with(|| entity.clone());
        }
    }
    out
}

/// Labelled references to the entity's key. One with a target condition counts only for a
/// filtered entity, whose filter is what such a condition narrows to.
fn relations(
    rules: &EffectiveRules,
    table: &str,
    id: &str,
    filtered: bool,
    table_entity: &impl Fn(&str) -> Option<String>,
) -> Vec<RelationDef> {
    let mut out: Vec<RelationDef> = Vec::new();
    let mut seen = HashSet::new();
    let incoming = rules.references.iter().filter(|r| {
        r.to.table == table
            && r.to.column.eq_ignore_ascii_case(id)
            && (filtered || r.to_when.is_empty())
    });
    for rule in incoming {
        let (Some(name), Some(entity)) = (&rule.label, table_entity(&rule.from.table)) else {
            continue;
        };
        if !seen.insert(name.clone()) {
            continue;
        }
        out.push(RelationDef {
            name: name.clone(),
            entity,
            table: rule.from.table.clone(),
            fk: rule.from.column.clone(),
            when: rule.when.clone(),
        });
    }
    out
}

// ---------- SQL ----------

/// WordPress IDs are unsigned integers; anything else is rejected so IDs can be inlined.
pub fn parse_ids(ids: &[String]) -> AppResult<Vec<u64>> {
    ids.iter()
        .map(|id| {
            id.trim()
                .parse::<u64>()
                .map_err(|_| AppError::InvalidInput(format!("'{id}' is not a valid ID")))
        })
        .collect()
}

pub fn id_list(ids: &[u64]) -> String {
    ids.iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn and_filter(filter: Option<&str>) -> String {
    filter.map(|f| format!(" AND ({f})")).unwrap_or_default()
}

pub fn rows_sql(dialect: Dialect, database: &str, def: &EntityDef, ids: &[u64]) -> String {
    format!(
        "SELECT * FROM {} WHERE {} IN ({}){}",
        dialect.table_ref(database, &def.table),
        dialect.quote_ident(&def.id),
        id_list(ids),
        and_filter(def.filter.as_deref()),
    )
}

pub fn meta_sql(dialect: Dialect, database: &str, meta: &MetaDef, ids: &[u64]) -> String {
    format!(
        "SELECT {fk}, {key}, {value} FROM {table_ref} WHERE {fk} IN ({ids}) ORDER BY {fk}",
        fk = dialect.quote_ident(&meta.fk),
        key = dialect.quote_ident(&meta.key),
        value = dialect.quote_ident(&meta.value),
        table_ref = dialect.table_ref(database, &meta.table),
        ids = id_list(ids),
    )
}

/// The relation's `when` and the related entity's filter.
fn relation_filter(
    dialect: Dialect,
    table_ref: &str,
    rel: &RelationDef,
    target: &EntityDef,
) -> String {
    let mut out: String = coverage::conditions(dialect, table_ref, &rel.when)
        .into_iter()
        .map(|c| format!(" AND {c}"))
        .collect();
    out += &and_filter(target.filter.as_deref());
    out
}

pub fn relation_count_sql(
    dialect: Dialect,
    database: &str,
    rel: &RelationDef,
    target: &EntityDef,
    ids: &[u64],
) -> String {
    let table_ref = dialect.table_ref(database, &rel.table);
    format!(
        "SELECT {fk}, COUNT(*) FROM {table_ref} WHERE {fk} IN ({ids}){filters} GROUP BY {fk}",
        fk = dialect.quote_ident(&rel.fk),
        ids = id_list(ids),
        filters = relation_filter(dialect, &table_ref, rel, target),
    )
}

pub fn relation_ids_sql(
    dialect: Dialect,
    database: &str,
    rel: &RelationDef,
    target: &EntityDef,
    id: u64,
    limit: u32,
) -> String {
    let table_ref = dialect.table_ref(database, &rel.table);
    format!(
        "SELECT {tid} FROM {table_ref} WHERE {fk} = {id}{filters} ORDER BY {tid} LIMIT {limit}",
        tid = dialect.quote_ident(&target.id),
        fk = dialect.quote_ident(&rel.fk),
        filters = relation_filter(dialect, &table_ref, rel, target),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use crate::wp::testing::{HPOS, LEGACY, site};

    const D: Dialect = Dialect::MYSQL;

    const ORDER_POSTS: &str =
        "post_type IN ('shop_order', 'shop_order_refund', 'shop_subscription')";

    fn catalog(tables: &[&str]) -> Catalog {
        site(tables).1
    }

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn relations(catalog: &Catalog, entity: &str) -> Vec<(String, String)> {
        catalog
            .get(entity)
            .unwrap()
            .relations
            .iter()
            .map(|r| (r.name.clone(), r.entity.clone()))
            .collect()
    }

    fn pair(name: &str, entity: &str) -> (String, String) {
        (name.to_string(), entity.to_string())
    }

    fn relation<'c>(catalog: &'c Catalog, entity: &str, name: &str) -> &'c RelationDef {
        catalog
            .get(entity)
            .unwrap()
            .relations
            .iter()
            .find(|r| r.name == name)
            .unwrap()
    }

    #[test]
    fn an_entity_on_a_table_of_its_own_wins_over_a_filtered_one_of_the_same_name() {
        let hpos = catalog(HPOS);
        let legacy = catalog(LEGACY);

        let on_orders = hpos.get("order").unwrap();
        let on_posts = legacy.get("order").unwrap();

        assert_eq!(
            (
                on_orders.table.as_str(),
                on_orders.id.as_str(),
                on_orders.filter.as_deref()
            ),
            ("wp_wc_orders", "id", None)
        );
        assert_eq!(
            (
                on_posts.table.as_str(),
                on_posts.id.as_str(),
                on_posts.filter.as_deref()
            ),
            ("wp_posts", "ID", Some(ORDER_POSTS))
        );
    }

    #[test]
    fn entities_without_a_table_or_a_single_column_key_are_left_out() {
        let catalog = catalog(&["options", "posts", "users", "terms"]);

        let err = catalog.get("term").unwrap_err();

        assert!(catalog.has("post"));
        assert!(!catalog.has("term"), "terms has no primary key");
        assert!(!catalog.has("session"), "no sessions table");
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(err.to_string(), "Unknown entity 'term'");
    }

    #[test]
    fn summary_columns_are_the_summary_entries_that_are_columns_of_the_table() {
        let legacy = catalog(LEGACY);
        let order = legacy.get("order").unwrap();

        assert_eq!(
            order.summary,
            [
                "post_type",
                "post_status",
                "_payment_method",
                "_order_total",
                "post_date_gmt"
            ]
        );
        assert_eq!(
            order.summary_columns,
            ["post_type", "post_status", "post_date_gmt"]
        );
    }

    #[test]
    fn links_follow_unconditional_references_to_the_key_of_a_table_with_an_entity() {
        let hpos = catalog(HPOS);

        assert_eq!(
            hpos.get("order").unwrap().links,
            map(&[("customer_id", "user"), ("parent_order_id", "order")])
        );
        assert_eq!(
            hpos.get("post").unwrap().links,
            map(&[("post_author", "user"), ("post_parent", "post")])
        );
    }

    #[test]
    fn meta_links_name_the_meta_keys_whose_values_reference_an_entity() {
        let hpos = catalog(HPOS);

        assert_eq!(
            hpos.get("order").unwrap().meta_links,
            map(&[
                ("_subscription_renewal", "order"),
                ("_subscription_resubscribe", "order"),
                ("_subscription_switch", "order"),
            ])
        );
        assert_eq!(
            hpos.get("post").unwrap().meta_links,
            map(&[
                ("_customer_user", "user"),
                ("_edit_last", "user"),
                ("_subscription_renewal", "post"),
                ("_subscription_resubscribe", "post"),
                ("_subscription_switch", "post"),
                ("_thumbnail_id", "post"),
            ])
        );
    }

    #[test]
    fn the_meta_table_is_the_one_the_entitys_table_owns() {
        let hpos = catalog(HPOS);

        assert_eq!(
            hpos.get("order").unwrap().meta,
            Some(MetaDef {
                table: "wp_wc_orders_meta".into(),
                fk: "order_id".into(),
                key: "meta_key".into(),
                value: "meta_value".into(),
            })
        );
        assert_eq!(
            hpos.get("post")
                .unwrap()
                .meta
                .as_ref()
                .map(|m| m.table.as_str()),
            Some("wp_postmeta")
        );
        assert_eq!(
            hpos.get("comment").unwrap().meta,
            None,
            "no commentmeta table"
        );
    }

    #[test]
    fn labelled_references_that_arrive_are_the_relations_of_an_entity() {
        let hpos = catalog(HPOS);

        assert_eq!(
            relations(&hpos, "order"),
            [
                pair("notes", "comment"),
                pair("children", "order"),
                pair("addresses", "order_address"),
                pair("items", "order_item"),
            ]
        );
    }

    #[test]
    fn a_relation_remembers_its_table_key_and_condition() {
        let hpos = catalog(HPOS);

        let notes = relation(&hpos, "order", "notes");
        let items = relation(&hpos, "order", "items");

        assert_eq!(
            (notes.table.as_str(), notes.fk.as_str(), notes.when.len()),
            ("wp_comments", "comment_post_ID", 1)
        );
        assert_eq!(notes.when[0].column, "comment_type");
        assert_eq!(
            (items.table.as_str(), items.fk.as_str(), items.when.len()),
            ("wp_woocommerce_order_items", "order_id", 0)
        );
    }

    #[test]
    fn references_with_a_target_condition_are_relations_only_of_filtered_entities() {
        let legacy = catalog(LEGACY);

        assert_eq!(
            relations(&legacy, "post"),
            [pair("children", "post"), pair("comments", "comment")],
            "the unfiltered post does not own the order items"
        );
        assert_eq!(
            relations(&legacy, "order"),
            [
                pair("items", "order_item"),
                pair("children", "post"),
                pair("comments", "comment"),
            ]
        );
        assert_eq!(relations(&legacy, "product"), relations(&legacy, "order"));
    }

    #[test]
    fn rows_of_a_table_open_as_its_unfiltered_entity() {
        let legacy = catalog(LEGACY);
        let te = |entity: &str, id_column: &str| TableEntity {
            entity: entity.into(),
            id_column: id_column.into(),
        };

        assert_eq!(
            legacy.table_entities(),
            BTreeMap::from([
                ("wp_comments".to_string(), te("comment", "comment_ID")),
                ("wp_options".to_string(), te("option", "option_id")),
                ("wp_posts".to_string(), te("post", "ID")),
                ("wp_users".to_string(), te("user", "ID")),
                (
                    "wp_woocommerce_order_items".to_string(),
                    te("order_item", "order_item_id")
                ),
            ])
        );
    }

    #[test]
    fn hpos_orders_open_from_wc_orders_and_not_from_posts() {
        let table_entities = catalog(HPOS).table_entities();

        assert_eq!(
            table_entities.get("wp_wc_orders"),
            Some(&TableEntity {
                entity: "order".into(),
                id_column: "id".into()
            })
        );
        assert_eq!(table_entities["wp_posts"].entity, "post");
    }

    #[test]
    fn only_unsigned_integers_are_ids() {
        let ids = |ids: &[&str]| parse_ids(&ids.iter().map(|i| i.to_string()).collect::<Vec<_>>());

        assert_eq!(ids(&["4711", " 7 "]).unwrap(), [4711, 7]);
        assert_eq!(ids(&[]).unwrap(), Vec::<u64>::new());
        for bad in ["abc", "-1", "1; DROP TABLE t", "1.5", ""] {
            let err = ids(&["1", bad]).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{bad:?}");
            assert_eq!(err.to_string(), format!("'{bad}' is not a valid ID"));
        }
    }

    #[test]
    fn ids_are_listed_for_an_in_clause() {
        assert_eq!(id_list(&[1, 22, 333]), "1, 22, 333");
        assert_eq!(id_list(&[]), "");
    }

    #[test]
    fn rows_are_selected_by_key_and_by_the_entitys_filter() {
        let hpos = catalog(HPOS);
        let legacy = catalog(LEGACY);

        assert_eq!(
            rows_sql(D, "wp", hpos.get("order").unwrap(), &[4711, 4713]),
            "SELECT * FROM `wp`.`wp_wc_orders` WHERE `id` IN (4711, 4713)"
        );
        assert_eq!(
            rows_sql(D, "wp", legacy.get("product").unwrap(), &[123]),
            "SELECT * FROM `wp`.`wp_posts` WHERE `ID` IN (123) AND (post_type IN ('product', 'product_variation'))"
        );
    }

    #[test]
    fn meta_rows_are_selected_by_owner_in_one_query() {
        let hpos = catalog(HPOS);
        let meta = hpos.get("order").unwrap().meta.as_ref().unwrap();

        assert_eq!(
            meta_sql(D, "wp", meta, &[4711, 4713]),
            "SELECT `order_id`, `meta_key`, `meta_value` FROM `wp`.`wp_wc_orders_meta` \
             WHERE `order_id` IN (4711, 4713) ORDER BY `order_id`"
        );
    }

    #[test]
    fn relation_counts_group_by_owner_and_apply_the_relations_condition() {
        let hpos = catalog(HPOS);
        let notes = relation(&hpos, "order", "notes");
        let items = relation(&hpos, "order", "items");

        assert_eq!(
            relation_count_sql(D, "wp", notes, hpos.get("comment").unwrap(), &[4711, 4712]),
            "SELECT `comment_post_ID`, COUNT(*) FROM `wp`.`wp_comments` \
             WHERE `comment_post_ID` IN (4711, 4712) \
             AND (`wp`.`wp_comments`.`comment_type` = 'order_note' \
             AND CAST(`wp`.`wp_comments`.`comment_type` AS BINARY) = 'order_note') \
             GROUP BY `comment_post_ID`"
        );
        assert_eq!(
            relation_count_sql(D, "wp", items, hpos.get("order_item").unwrap(), &[4711]),
            "SELECT `order_id`, COUNT(*) FROM `wp`.`wp_woocommerce_order_items` \
             WHERE `order_id` IN (4711) GROUP BY `order_id`"
        );
    }

    #[test]
    fn relation_ids_are_listed_in_key_order_up_to_the_limit() {
        let hpos = catalog(HPOS);
        let notes = relation(&hpos, "order", "notes");

        assert_eq!(
            relation_ids_sql(D, "wp", notes, hpos.get("comment").unwrap(), 4711, 100),
            "SELECT `comment_ID` FROM `wp`.`wp_comments` WHERE `comment_post_ID` = 4711 \
             AND (`wp`.`wp_comments`.`comment_type` = 'order_note' \
             AND CAST(`wp`.`wp_comments`.`comment_type` AS BINARY) = 'order_note') \
             ORDER BY `comment_ID` LIMIT 100"
        );
    }

    #[test]
    fn a_relation_into_a_filtered_entity_applies_that_entitys_filter() {
        let legacy = catalog(LEGACY);
        let rel = RelationDef {
            name: "children".into(),
            entity: "order".into(),
            table: "wp_posts".into(),
            fk: "post_parent".into(),
            when: Vec::new(),
        };
        let target = legacy.get("order").unwrap();

        assert_eq!(
            relation_count_sql(D, "wp", &rel, target, &[501]),
            format!(
                "SELECT `post_parent`, COUNT(*) FROM `wp`.`wp_posts` \
                 WHERE `post_parent` IN (501) AND ({ORDER_POSTS}) GROUP BY `post_parent`"
            )
        );
        assert_eq!(
            relation_ids_sql(D, "wp", &rel, target, 501, 5),
            format!(
                "SELECT `ID` FROM `wp`.`wp_posts` WHERE `post_parent` = 501 AND ({ORDER_POSTS}) \
                 ORDER BY `ID` LIMIT 5"
            )
        );
    }
}
