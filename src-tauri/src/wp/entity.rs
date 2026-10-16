//! Entity definitions (data, not code) and the pure parts of the generic resolver:
//! choosing definitions for a context, building SQL, and assembling documents.

use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::context::WpContext;
use super::hints::{Hint, HintContext, Sensitive, annotate};
use super::value::{self, DecodedValue};
use crate::db::{Cell, ResultSet, quote_ident};
use crate::error::{AppError, AppResult};

const BUILTIN: &str = include_str!("entities.toml");

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum When {
    #[default]
    Always,
    Woocommerce,
    Hpos,
    LegacyOrders,
}

impl When {
    pub(super) fn applies(self, ctx: &WpContext) -> bool {
        match self {
            When::Always => true,
            When::Woocommerce => ctx.wc.is_some(),
            When::Hpos => ctx.hpos(),
            When::LegacyOrders => ctx.wc.is_some() && !ctx.hpos(),
        }
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct MetaDef {
    pub table: String,
    pub fk: String,
    #[serde(default = "default_meta_key")]
    pub key: String,
    #[serde(default = "default_meta_value")]
    pub value: String,
}

fn default_meta_key() -> String {
    "meta_key".into()
}

fn default_meta_value() -> String {
    "meta_value".into()
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct RelationDef {
    pub name: String,
    pub label: String,
    pub entity: String,
    pub fk: String,
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub when: When,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct EntityDef {
    pub name: String,
    pub label: String,
    #[serde(default)]
    pub when: When,
    pub table: String,
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub hidden: Vec<String>,
    #[serde(default)]
    pub meta: Option<MetaDef>,
    #[serde(default)]
    pub links: BTreeMap<String, String>,
    #[serde(default)]
    pub meta_links: BTreeMap<String, String>,
    #[serde(default)]
    pub relations: Vec<RelationDef>,
}

#[derive(Deserialize)]
struct DefFile {
    #[serde(default)]
    entity: Vec<EntityDef>,
}

fn builtin() -> &'static [EntityDef] {
    static DEFS: OnceLock<Vec<EntityDef>> = OnceLock::new();
    DEFS.get_or_init(|| {
        toml::from_str::<DefFile>(BUILTIN)
            .expect("built-in entities.toml is valid")
            .entity
    })
}

/// Built-in definitions plus user ones. A user definition replaces a built-in one with
/// the same name and `when`; anything else is added.
pub fn definitions(user_toml: Option<&str>) -> Result<Vec<EntityDef>, String> {
    let mut defs = builtin().to_vec();
    if let Some(src) = user_toml {
        let user = toml::from_str::<DefFile>(src).map_err(|e| e.to_string())?;
        for def in user.entity {
            match defs
                .iter_mut()
                .find(|d| d.name == def.name && d.when == def.when)
            {
                Some(existing) => *existing = def,
                None => defs.push(def),
            }
        }
    }
    Ok(defs)
}

/// The definitions that apply to one install; the first matching definition per name wins.
#[derive(Debug, Clone)]
pub struct Catalog {
    defs: Vec<EntityDef>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TableEntity {
    pub entity: String,
    pub id_column: String,
}

impl Catalog {
    pub fn new(all: &[EntityDef], ctx: &WpContext) -> Self {
        let mut defs: Vec<EntityDef> = Vec::new();
        for def in all
            .iter()
            .filter(|d| d.when.applies(ctx) && ctx.has_table(&d.table))
        {
            if !defs.iter().any(|d| d.name == def.name) {
                let mut def = def.clone();
                if def.meta.as_ref().is_some_and(|m| !ctx.has_table(&m.table)) {
                    def.meta = None;
                }
                defs.push(def);
            }
        }
        Self { defs }
    }

    pub fn get(&self, name: &str) -> AppResult<&EntityDef> {
        self.defs
            .iter()
            .find(|d| d.name == name)
            .ok_or_else(|| AppError::InvalidInput(format!("Unknown entity '{name}'")))
    }

    fn has(&self, name: &str) -> bool {
        self.defs.iter().any(|d| d.name == name)
    }

    /// Full table name → entity, so table rows can be opened as documents.
    pub fn table_entities(&self, ctx: &WpContext) -> BTreeMap<String, TableEntity> {
        let mut map = BTreeMap::new();
        for def in &self.defs {
            map.entry(ctx.table(&def.table))
                .or_insert_with(|| TableEntity {
                    entity: def.name.clone(),
                    id_column: def.id.clone(),
                });
        }
        map
    }

    pub fn entity_names(&self) -> Vec<(String, String)> {
        self.defs
            .iter()
            .map(|d| (d.name.clone(), d.label.clone()))
            .collect()
    }
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

fn id_list(ids: &[u64]) -> String {
    ids.iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn table_ref(database: &str, ctx: &WpContext, table: &str) -> String {
    format!(
        "{}.{}",
        quote_ident(database),
        quote_ident(&ctx.table(table))
    )
}

fn and_filters(filters: &[&Option<String>]) -> String {
    filters
        .iter()
        .filter_map(|f| f.as_deref())
        .map(|f| format!(" AND ({f})"))
        .collect()
}

pub fn rows_sql(database: &str, ctx: &WpContext, def: &EntityDef, ids: &[u64]) -> String {
    format!(
        "SELECT * FROM {} WHERE {} IN ({}){}",
        table_ref(database, ctx, &def.table),
        quote_ident(&def.id),
        id_list(ids),
        and_filters(&[&def.filter]),
    )
}

pub fn meta_sql(database: &str, ctx: &WpContext, meta: &MetaDef, ids: &[u64]) -> String {
    format!(
        "SELECT {fk}, {key}, {value} FROM {table} WHERE {fk} IN ({ids}) ORDER BY {fk}",
        fk = quote_ident(&meta.fk),
        key = quote_ident(&meta.key),
        value = quote_ident(&meta.value),
        table = table_ref(database, ctx, &meta.table),
        ids = id_list(ids),
    )
}

pub fn relation_count_sql(
    database: &str,
    ctx: &WpContext,
    rel: &RelationDef,
    target: &EntityDef,
    ids: &[u64],
) -> String {
    format!(
        "SELECT {fk}, COUNT(*) FROM {table} WHERE {fk} IN ({ids}){filters} GROUP BY {fk}",
        fk = quote_ident(&rel.fk),
        table = table_ref(database, ctx, &target.table),
        ids = id_list(ids),
        filters = and_filters(&[&rel.filter, &target.filter]),
    )
}

pub fn relation_ids_sql(
    database: &str,
    ctx: &WpContext,
    rel: &RelationDef,
    target: &EntityDef,
    id: u64,
    limit: u32,
) -> String {
    format!(
        "SELECT {tid} FROM {table} WHERE {fk} = {id}{filters} ORDER BY {tid} LIMIT {limit}",
        tid = quote_ident(&target.id),
        table = table_ref(database, ctx, &target.table),
        fk = quote_ident(&rel.fk),
        filters = and_filters(&[&rel.filter, &target.filter]),
    )
}

// ---------- documents ----------

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EntityLink {
    pub entity: String,
    pub id: String,
    /// Title of the target (e.g. product name), resolved in a batch after assembly.
    pub label: Option<String>,
    pub image: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocField {
    pub name: String,
    pub label: Option<String>,
    pub value: Cell,
    pub decoded: Option<DecodedValue>,
    pub link: Option<EntityLink>,
    pub hint: Option<Hint>,
    pub sensitive: Option<Sensitive>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetaGroup {
    pub name: String,
    /// All keys start with `_` (hidden from the WordPress UI).
    pub private: bool,
    pub entries: Vec<DocField>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RelationInfo {
    pub name: String,
    pub label: String,
    pub entity: String,
    pub count: u64,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EntityDoc {
    pub entity: String,
    pub label: String,
    pub id: String,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    /// Featured image (posts/products), resolved after assembly.
    pub image: Option<String>,
    /// Where the row lives, so the UI can jump to the table view.
    pub table: String,
    pub id_column: String,
    pub fields: Vec<DocField>,
    pub meta: Vec<MetaGroup>,
    /// Where the meta rows live, so the UI can open them filtered in the table view.
    pub meta_source: Option<MetaSource>,
    pub relations: Vec<RelationInfo>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetaSource {
    pub table: String,
    /// Column holding the entity ID, e.g. `order_id`.
    pub fk: String,
    /// Column holding the key, e.g. `meta_key`.
    pub key: String,
}

impl EntityDoc {
    /// Every field and meta entry, e.g. to resolve links in one batch.
    pub fn all_fields_mut(&mut self) -> impl Iterator<Item = &mut DocField> {
        self.fields
            .iter_mut()
            .chain(self.meta.iter_mut().flat_map(|g| g.entries.iter_mut()))
    }

    pub fn all_fields(&self) -> impl Iterator<Item = &DocField> {
        self.fields
            .iter()
            .chain(self.meta.iter().flat_map(|g| g.entries.iter()))
    }

    pub fn field(&self, name: &str) -> Option<&DocField> {
        self.all_fields().find(|f| f.name == name)
    }

    pub fn text(&self, name: &str) -> Option<&str> {
        self.field(name)
            .and_then(|f| cell_str(&f.value))
            .filter(|s| !s.is_empty())
    }
}

pub fn cell_str(cell: &Cell) -> Option<&str> {
    match cell {
        Cell::Number(s) | Cell::Text(s) | Cell::Temporal(s) | Cell::Json(s) => Some(s),
        _ => None,
    }
}

fn link_for(catalog: &Catalog, entity: Option<&String>, cell: &Cell) -> Option<EntityLink> {
    let entity = entity?;
    let id = cell_str(cell)?.trim();
    (catalog.has(entity) && id.parse::<u64>().is_ok_and(|n| n > 0)).then(|| EntityLink {
        entity: entity.clone(),
        id: id.to_string(),
        label: None,
        image: None,
    })
}

pub(crate) fn field(
    catalog: &Catalog,
    hints: &HintContext,
    name: String,
    value: Cell,
    link_target: Option<&String>,
) -> DocField {
    let a = annotate(&name, cell_str(&value), hints);
    DocField {
        decoded: cell_str(&value).and_then(value::decode),
        link: link_for(catalog, link_target, &value),
        label: None,
        hint: a.hint,
        sensitive: a.sensitive,
        name,
        value,
    }
}

/// `_ppcp_paypal_order_id` → `ppcp`; `billing_email` → `billing`.
fn meta_prefix(key: &str) -> &str {
    let trimmed = key.trim_start_matches('_');
    trimmed
        .split(['_', '-'])
        .next()
        .filter(|p| !p.is_empty() && p.len() < trimmed.len())
        .unwrap_or("")
}

/// Groups meta by its literal key prefix (`_ppcp*`, `billing*`), public and `_private` keys
/// apart; keys that share their prefix with no other key go to "Other". Groups and keys are
/// sorted by name, "Other" last.
pub fn group_meta(entries: Vec<DocField>) -> Vec<MetaGroup> {
    let pattern = |e: &DocField| {
        let prefix = meta_prefix(&e.name);
        (!prefix.is_empty()).then(|| {
            let underscore = if e.name.starts_with('_') { "_" } else { "" };
            format!("{underscore}{prefix}*")
        })
    };
    let mut counts: HashMap<String, usize> = HashMap::new();
    for p in entries.iter().filter_map(pattern) {
        *counts.entry(p).or_default() += 1;
    }
    let mut groups: Vec<MetaGroup> = Vec::new();
    for e in entries {
        let name = pattern(&e)
            .filter(|p| counts[p] > 1)
            .unwrap_or_else(|| "Other".to_string());
        let private = e.name.starts_with('_');
        match groups.iter_mut().find(|g| g.name == name) {
            Some(g) => {
                g.private &= private;
                g.entries.push(e);
            }
            None => groups.push(MetaGroup {
                name,
                private,
                entries: vec![e],
            }),
        }
    }
    for g in &mut groups {
        g.entries.sort_by(|a, b| a.name.cmp(&b.name));
    }
    groups.sort_by(|a, b| {
        (a.name == "Other")
            .cmp(&(b.name == "Other"))
            .then_with(|| a.name.cmp(&b.name))
    });
    groups
}

fn column_index(rs: &ResultSet, name: &str) -> Option<usize> {
    rs.columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case(name))
}

/// Builds documents from the entity rows, their meta rows (fk, key, value) and relation
/// counts (relation name → id → count). Documents keep the order of `rows`.
pub fn assemble(
    catalog: &Catalog,
    ctx: &WpContext,
    def: &EntityDef,
    rows: &ResultSet,
    meta: Option<&ResultSet>,
    counts: &HashMap<String, HashMap<String, u64>>,
) -> Vec<EntityDoc> {
    let id_idx = column_index(rows, &def.id);
    let currency_idx = column_index(rows, "currency");
    let text_of = |row: &[Cell], col: &Option<String>| {
        col.as_ref()
            .and_then(|c| column_index(rows, c))
            .and_then(|i| cell_str(&row[i]).map(String::from))
            .filter(|s| !s.is_empty())
    };

    // Raw meta first: the currency (and other doc-wide facts) must be known before hinting.
    let mut raw_meta: HashMap<String, Vec<(String, Cell)>> = HashMap::new();
    if let Some(meta_rows) = meta {
        for r in &meta_rows.rows {
            if let (Some(fk), Some(key)) = (cell_str(&r[0]), cell_str(&r[1])) {
                raw_meta
                    .entry(fk.to_string())
                    .or_default()
                    .push((key.to_string(), r[2].clone()));
            }
        }
    }

    rows.rows
        .iter()
        .map(|row| {
            let id = id_idx
                .and_then(|i| cell_str(&row[i]))
                .unwrap_or_default()
                .to_string();
            let doc_meta = raw_meta.remove(&id).unwrap_or_default();
            let currency = currency_idx
                .and_then(|i| cell_str(&row[i]).map(String::from))
                .or_else(|| {
                    doc_meta
                        .iter()
                        .find(|(k, _)| k == "_order_currency")
                        .and_then(|(_, v)| cell_str(v).map(String::from))
                })
                .filter(|c| !c.is_empty())
                .or_else(|| ctx.site.currency.clone());
            let hints = HintContext {
                currency,
                gmt_offset_secs: ctx.site.gmt_offset_secs,
            };

            let fields = rows
                .columns
                .iter()
                .zip(row)
                .filter(|(c, _)| !def.hidden.iter().any(|h| h.eq_ignore_ascii_case(&c.name)))
                .map(|(c, v)| {
                    field(
                        catalog,
                        &hints,
                        c.name.clone(),
                        v.clone(),
                        def.links.get(&c.name),
                    )
                })
                .collect();
            let meta_fields = doc_meta
                .into_iter()
                .map(|(k, v)| {
                    let link = def.meta_links.get(&k).cloned();
                    field(catalog, &hints, k, v, link.as_ref())
                })
                .collect();
            let relations = def
                .relations
                .iter()
                .filter(|r| r.when.applies(ctx) && catalog.has(&r.entity))
                .map(|r| RelationInfo {
                    name: r.name.clone(),
                    label: r.label.clone(),
                    entity: r.entity.clone(),
                    count: counts
                        .get(&r.name)
                        .and_then(|m| m.get(&id))
                        .copied()
                        .unwrap_or(0),
                })
                .collect();
            EntityDoc {
                entity: def.name.clone(),
                label: def.label.clone(),
                title: text_of(row, &def.title),
                subtitle: text_of(row, &def.subtitle),
                image: None,
                table: ctx.table(&def.table),
                id_column: def.id.clone(),
                fields,
                meta: group_meta(meta_fields),
                meta_source: def.meta.as_ref().map(|m| MetaSource {
                    table: ctx.table(&m.table),
                    fk: m.fk.clone(),
                    key: m.key.clone(),
                }),
                relations,
                id,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ColumnMeta;
    use crate::wp::context::{HposMode, WcContext};

    /// A full WooCommerce install: every table any built-in definition needs exists.
    fn ctx(hpos: HposMode) -> WpContext {
        let tables = definitions(None)
            .unwrap()
            .iter()
            .flat_map(|d| {
                [
                    Some(d.table.clone()),
                    d.meta.as_ref().map(|m| m.table.clone()),
                ]
            })
            .flatten()
            .map(|t| format!("wp_{t}"))
            .collect();
        WpContext {
            prefix: "wp_".into(),
            version: None,
            sites: vec![],
            wc: Some(WcContext {
                version: Some("9.3.0".into()),
                hpos,
                has_action_scheduler: true,
            }),
            site: Default::default(),
            tables,
        }
    }

    fn catalog(hpos: HposMode) -> Catalog {
        Catalog::new(&definitions(None).unwrap(), &ctx(hpos))
    }

    fn text(s: &str) -> Cell {
        Cell::Text(s.into())
    }

    fn num(s: &str) -> Cell {
        Cell::Number(s.into())
    }

    fn rs(cols: &[&str], rows: Vec<Vec<Cell>>) -> ResultSet {
        ResultSet {
            columns: cols
                .iter()
                .map(|c| ColumnMeta {
                    name: c.to_string(),
                    type_name: "VARCHAR".into(),
                })
                .collect(),
            rows,
            returns_rows: true,
            ..Default::default()
        }
    }

    fn meta_field(name: &str) -> DocField {
        field(
            &catalog(HposMode::On),
            &HintContext::default(),
            name.into(),
            text("x"),
            None,
        )
    }

    #[test]
    fn builtin_definitions_parse() {
        assert!(definitions(None).unwrap().len() >= 8);
    }

    #[test]
    fn order_entity_follows_hpos_mode() {
        let ctx_on = ctx(HposMode::On);
        assert_eq!(
            catalog(HposMode::On).get("order").unwrap().table,
            "wc_orders"
        );
        assert_eq!(catalog(HposMode::Off).get("order").unwrap().table, "posts");
        assert_eq!(
            catalog(HposMode::On).table_entities(&ctx_on)["wp_wc_orders"].entity,
            "order"
        );
    }

    #[test]
    fn woocommerce_entities_need_woocommerce() {
        let plain = WpContext {
            wc: None,
            ..ctx(HposMode::Off)
        };
        let c = Catalog::new(&definitions(None).unwrap(), &plain);
        assert!(c.get("post").is_ok());
        assert!(c.get("order").is_err());
    }

    #[test]
    fn entities_and_meta_need_their_tables() {
        let mut partial = ctx(HposMode::Off);
        partial.tables.remove("wp_woocommerce_payment_tokens");
        partial.tables.remove("wp_commentmeta");
        let c = Catalog::new(&definitions(None).unwrap(), &partial);
        assert!(c.get("payment_token").is_err());
        assert_eq!(c.get("comment").unwrap().meta, None);
    }

    #[test]
    fn user_definitions_override_and_extend() {
        let user = r#"
            [[entity]]
            name = "post"
            label = "Custom post"
            table = "posts"
            id = "ID"

            [[entity]]
            name = "vault_token"
            label = "PayPal vault token"
            table = "ppcp_vault"
            id = "id"
        "#;
        let mut with_vault = ctx(HposMode::On);
        with_vault.tables.insert("wp_ppcp_vault".into());
        let c = Catalog::new(&definitions(Some(user)).unwrap(), &with_vault);
        assert_eq!(c.get("post").unwrap().label, "Custom post");
        assert_eq!(c.get("vault_token").unwrap().table, "ppcp_vault");
        assert!(definitions(Some("not toml [")).is_err());
    }

    #[test]
    fn ids_must_be_numeric() {
        assert_eq!(parse_ids(&["12".into(), " 7 ".into()]).unwrap(), [12, 7]);
        assert!(parse_ids(&["1 OR 1=1".into()]).is_err());
    }

    #[test]
    fn builds_batched_sql_with_prefix_and_filters() {
        let c = catalog(HposMode::Off);
        let order = c.get("order").unwrap();
        assert_eq!(
            rows_sql("db", &ctx(HposMode::Off), order, &[1, 2]),
            "SELECT * FROM `db`.`wp_posts` WHERE `ID` IN (1, 2) AND (post_type IN ('shop_order', 'shop_order_refund'))"
        );
        assert_eq!(
            meta_sql(
                "db",
                &ctx(HposMode::Off),
                order.meta.as_ref().unwrap(),
                &[1, 2]
            ),
            "SELECT `post_id`, `meta_key`, `meta_value` FROM `db`.`wp_postmeta` WHERE `post_id` IN (1, 2) ORDER BY `post_id`"
        );
        let notes = order.relations.iter().find(|r| r.name == "notes").unwrap();
        assert_eq!(
            relation_count_sql(
                "db",
                &ctx(HposMode::Off),
                notes,
                c.get("comment").unwrap(),
                &[1]
            ),
            "SELECT `comment_post_ID`, COUNT(*) FROM `db`.`wp_comments` WHERE `comment_post_ID` IN (1) AND (comment_type = 'order_note') GROUP BY `comment_post_ID`"
        );
    }

    #[test]
    fn meta_groups_by_literal_key_prefix() {
        let groups = group_meta(
            [
                "_ppcp_paypal_order_id",
                "_ppcp_capture_id",
                "_transaction_id",
                "billing_email",
                "billing_country",
                "foo_a",
                "foo_b",
                "_myplugin_a",
                "_myplugin_b",
                "color",
                "_lonely",
            ]
            .into_iter()
            .map(meta_field)
            .collect(),
        );
        let shape: Vec<_> = groups
            .iter()
            .map(|g| (g.name.as_str(), g.private, g.entries.len()))
            .collect();
        assert_eq!(
            shape,
            [
                ("_myplugin*", true, 2),
                ("_ppcp*", true, 2),
                ("billing*", false, 2),
                ("foo*", false, 2),
                ("Other", false, 3),
            ]
        );
        let other: Vec<_> = groups[4].entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(other, ["_lonely", "_transaction_id", "color"]);
    }

    #[test]
    fn assembles_documents_with_links_meta_decoding_and_counts() {
        let c = catalog(HposMode::On);
        let def = c.get("order").unwrap();
        let rows = rs(
            &[
                "id",
                "status",
                "billing_email",
                "customer_id",
                "parent_order_id",
            ],
            vec![vec![
                num("4711"),
                text("wc-processing"),
                text("a@b.at"),
                num("12"),
                num("0"),
            ]],
        );
        let meta = rs(
            &["order_id", "meta_key", "meta_value"],
            vec![
                vec![num("4711"), text("_ppcp_paypal_order_id"), text("8XY")],
                vec![
                    num("4711"),
                    text("_ppcp_fees"),
                    text(r#"a:1:{s:3:"fee";s:4:"1.20";}"#),
                ],
            ],
        );
        let counts = HashMap::from([(
            "items".to_string(),
            HashMap::from([("4711".to_string(), 2)]),
        )]);

        let doc = &assemble(&c, &ctx(HposMode::On), def, &rows, Some(&meta), &counts)[0];
        assert_eq!(doc.id, "4711");
        assert_eq!(doc.title.as_deref(), Some("a@b.at"));
        assert_eq!(doc.subtitle.as_deref(), Some("wc-processing"));
        assert_eq!(doc.table, "wp_wc_orders");

        let customer = doc.fields.iter().find(|f| f.name == "customer_id").unwrap();
        assert_eq!(
            customer.link,
            Some(EntityLink {
                entity: "user".into(),
                id: "12".into(),
                label: None,
                image: None,
            })
        );
        let parent = doc
            .fields
            .iter()
            .find(|f| f.name == "parent_order_id")
            .unwrap();
        assert_eq!(parent.link, None, "0 is not a reference");

        assert_eq!(doc.meta[0].name, "_ppcp*");
        assert!(doc.meta[0].private);
        let total = doc.fields.iter().find(|f| f.name == "status").unwrap();
        assert_eq!(total.hint, None);
        assert_eq!(
            doc.field("_ppcp_fees")
                .unwrap()
                .decoded
                .as_ref()
                .unwrap()
                .format,
            "php"
        );

        let items = doc.relations.iter().find(|r| r.name == "items").unwrap();
        assert_eq!(items.count, 2);
        assert_eq!(
            doc.relations
                .iter()
                .find(|r| r.name == "notes")
                .unwrap()
                .count,
            0
        );
    }

    #[test]
    fn hidden_columns_are_left_out() {
        let c = catalog(HposMode::Off);
        let rows = rs(
            &["ID", "user_login", "user_pass"],
            vec![vec![num("1"), text("admin"), text("$P$hash")]],
        );
        let doc = &assemble(
            &c,
            &ctx(HposMode::Off),
            c.get("user").unwrap(),
            &rows,
            None,
            &HashMap::new(),
        )[0];
        assert!(doc.fields.iter().all(|f| f.name != "user_pass"));
    }
}
