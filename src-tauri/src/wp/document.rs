//! Documents of the WordPress screens: an entity's row, its meta rows and its relation counts,
//! with the kinds of the effective rules (money, timestamps, secrets).

use std::collections::HashMap;

use serde::Serialize;
use ts_rs::TS;

use super::catalog::{Catalog, EntityDef, MetaDef};
use super::rows::{cell_str, column_index, money};
use crate::db::{Cell, ResultSet};
use crate::decode::value::{self, DecodedValue};
use crate::rules::effective::EffectiveRules;
use crate::rules::kinds::{KindRule, ValueRule};
use crate::value::utc_seconds;

/// What a kind rule says about a stored value, next to it; never instead of it.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Hint {
    /// The stored amount with the currency of the row or its document.
    Money { text: String },
    /// A timestamp whose zone is stored (`unix` or `utc` rules).
    Time { unix: i64 },
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "snake_case")]
pub enum Sensitive {
    /// Masked until clicked.
    Secret,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct EntityLink {
    pub entity: String,
    pub id: String,
    /// The target's summary, resolved in a batch after assembly.
    pub label: Option<String>,
    pub image: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct DocField {
    pub name: String,
    pub label: Option<String>,
    pub value: Cell,
    pub decoded: Option<DecodedValue>,
    pub link: Option<EntityLink>,
    pub hint: Option<Hint>,
    pub sensitive: Option<Sensitive>,
    /// An option's stored `autoload` value; `None` for other fields.
    pub autoload: Option<Autoload>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct Autoload {
    /// As stored: `yes`/`no`, or since WordPress 6.6 `on`/`off`/`auto`/`auto-on`/`auto-off`.
    pub value: String,
    pub on: bool,
}

/// The meta rows of a document, in stored order, named by their table.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct MetaGroup {
    pub name: String,
    /// All keys start with `_`.
    pub private: bool,
    pub entries: Vec<DocField>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct RelationInfo {
    pub name: String,
    pub label: String,
    pub entity: String,
    pub count: u64,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct EntityDoc {
    pub entity: String,
    pub label: String,
    pub id: String,
    /// The entity's summary values, joined.
    pub title: Option<String>,
    pub subtitle: Option<String>,
    /// Featured image (posts), resolved after assembly.
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

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
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
        self.field(name).and_then(|f| non_empty_str(&f.value))
    }
}

/// The stored text, unless blank.
fn non_empty_str(cell: &Cell) -> Option<&str> {
    cell_str(cell).filter(|s| !s.is_empty())
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

/// The text of the first of `names` that has one, as a column, else as a meta key.
fn first_text(
    names: &[String],
    columns: &[(String, Cell)],
    meta: &[(String, Cell)],
) -> Option<String> {
    let text = |entries: &[(String, Cell)], name: &str| {
        entries
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, v)| non_empty_str(v))
            .map(String::from)
    };
    names
        .iter()
        .find_map(|n| text(columns, n).or_else(|| text(meta, n)))
}

/// A document's `(name, value)` pairs: its columns or its meta rows.
type Entries = [(String, Cell)];

/// A field as one kind rule reads it: a money or time hint, or secret.
pub(crate) fn field(
    catalog: &Catalog,
    rule: Option<&KindRule>,
    doc: (&Entries, &Entries),
    name: String,
    value: Cell,
    link_target: Option<&String>,
) -> DocField {
    let raw = cell_str(&value).map(str::trim);
    let (hint, sensitive) = match (rule.map(|r| &r.value), raw) {
        (Some(ValueRule::Secret), _) => (None, Some(Sensitive::Secret)),
        (Some(ValueRule::Money(m)), Some(raw)) if raw.parse::<f64>().is_ok() => {
            let hint = first_text(&m.currency, doc.0, doc.1)
                .and_then(|currency| money(raw, Some(&currency)))
                .map(|text| Hint::Money { text });
            (hint, None)
        }
        (Some(ValueRule::Timestamp(zone)), Some(raw)) => {
            let hint = utc_seconds(raw, Some(*zone)).map(|unix| Hint::Time { unix });
            (hint, None)
        }
        _ => (None, None),
    };
    DocField {
        decoded: cell_str(&value).and_then(value::decode),
        link: link_for(catalog, link_target, &value),
        label: None,
        hint,
        sensitive,
        autoload: None,
        name,
        value,
    }
}

/// Each owner id's `(key, value)` pairs, in stored order.
fn meta_by_owner(meta: Option<&ResultSet>) -> HashMap<String, Vec<(String, Cell)>> {
    let mut out: HashMap<String, Vec<(String, Cell)>> = HashMap::new();
    for r in meta.map(|m| m.rows.as_slice()).unwrap_or_default() {
        let (Some(fk), Some(key), Some(value)) = (
            r.first().and_then(cell_str),
            r.get(1).and_then(cell_str),
            r.get(2),
        ) else {
            continue;
        };
        out.entry(fk.to_string())
            .or_default()
            .push((key.to_string(), value.clone()));
    }
    out
}

/// Summary parts, joined with ` · `.
fn summary(def: &EntityDef, columns: &[(String, Cell)], meta: &[(String, Cell)]) -> Option<String> {
    let parts: Vec<String> = def
        .summary
        .iter()
        .filter_map(|name| first_text(std::slice::from_ref(name), columns, meta))
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// The documents' inputs: the entity rows, their meta rows `(fk, key, value)` and the relation
/// counts (relation name → id → count).
pub struct Fetched<'a> {
    pub rows: &'a ResultSet,
    pub meta: Option<&'a ResultSet>,
    pub counts: &'a HashMap<String, HashMap<String, u64>>,
}

/// Documents in the order of the rows.
pub fn assemble(
    rules: &EffectiveRules,
    catalog: &Catalog,
    def: &EntityDef,
    fetched: Fetched<'_>,
) -> Vec<EntityDoc> {
    let id_idx = column_index(fetched.rows, &def.id);
    let mut raw_meta = meta_by_owner(fetched.meta);
    let meta_ref = def.meta.as_ref().and_then(|m| rules.meta_table(&m.table));
    fetched
        .rows
        .rows
        .iter()
        .map(|row| {
            let id = id_idx
                .and_then(|i| cell_str(&row[i]))
                .unwrap_or_default()
                .to_string();
            let columns: Vec<(String, Cell)> = fetched
                .rows
                .columns
                .iter()
                .zip(row)
                .map(|(c, v)| (c.name.clone(), v.clone()))
                .collect();
            let doc_meta = raw_meta.remove(&id).unwrap_or_default();
            let doc = (columns.as_slice(), doc_meta.as_slice());
            let fields = columns
                .iter()
                .map(|(name, v)| {
                    let rule = rules.column_kind_rule(&def.table, name);
                    field(
                        catalog,
                        rule,
                        doc,
                        name.clone(),
                        v.clone(),
                        def.links.get(name),
                    )
                })
                .collect();
            let entries = doc_meta
                .iter()
                .map(|(key, v)| {
                    let rule = meta_ref.and_then(|m| rules.meta_kind_rule(m, key));
                    let link = def.meta_links.get(key);
                    field(catalog, rule, doc, key.clone(), v.clone(), link)
                })
                .collect();
            EntityDoc {
                entity: def.name.clone(),
                label: def.name.clone(),
                title: summary(def, &columns, &doc_meta),
                subtitle: None,
                image: None,
                table: def.table.clone(),
                id_column: def.id.clone(),
                fields,
                meta: meta_groups(def.meta.as_ref(), entries),
                meta_source: def.meta.as_ref().map(|m| MetaSource {
                    table: m.table.clone(),
                    fk: m.fk.clone(),
                    key: m.key.clone(),
                }),
                relations: relations(catalog, def, fetched.counts, &id),
                id,
            }
        })
        .collect()
}

fn meta_groups(meta: Option<&MetaDef>, entries: Vec<DocField>) -> Vec<MetaGroup> {
    let Some(meta) = meta.filter(|_| !entries.is_empty()) else {
        return Vec::new();
    };
    vec![MetaGroup {
        name: meta.table.clone(),
        private: entries.iter().all(|e| e.name.starts_with('_')),
        entries,
    }]
}

fn relations(
    catalog: &Catalog,
    def: &EntityDef,
    counts: &HashMap<String, HashMap<String, u64>>,
    id: &str,
) -> Vec<RelationInfo> {
    def.relations
        .iter()
        .filter(|r| catalog.has(&r.entity))
        .map(|r| RelationInfo {
            name: r.name.clone(),
            label: r.name.clone(),
            entity: r.entity.clone(),
            count: counts
                .get(&r.name)
                .and_then(|m| m.get(id))
                .copied()
                .unwrap_or(0),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ColumnMeta;
    use crate::wp::testing::{HPOS, LEGACY, site};

    fn t(s: &str) -> Cell {
        Cell::Text(s.into())
    }

    fn texts(cells: &[&str]) -> Vec<Cell> {
        cells.iter().map(|c| t(c)).collect()
    }

    fn result(columns: &[&str], rows: Vec<Vec<Cell>>) -> ResultSet {
        ResultSet {
            columns: columns
                .iter()
                .map(|name| ColumnMeta {
                    name: name.to_string(),
                    type_name: "TEXT".into(),
                    origin: None,
                })
                .collect(),
            rows,
            returns_rows: true,
            ..Default::default()
        }
    }

    type Counts = HashMap<String, HashMap<String, u64>>;

    /// The documents of `entity` for `rows`, with meta rows `(owner id, key, value)`.
    fn build(
        tables: &[&str],
        entity: &str,
        columns: &[&str],
        rows: &[&[&str]],
        meta: &[(&str, &str, &str)],
        counts: &Counts,
    ) -> Vec<EntityDoc> {
        let (ctx, catalog) = site(tables);
        let def = catalog.get(entity).unwrap();
        let rows = result(columns, rows.iter().map(|r| texts(r)).collect());
        let meta = result(
            &["owner", "key", "value"],
            meta.iter()
                .map(|(id, k, v)| texts(&[*id, *k, *v]))
                .collect(),
        );
        let fetched = Fetched {
            rows: &rows,
            meta: def.meta.as_ref().map(|_| &meta),
            counts,
        };
        assemble(&ctx.rules, &catalog, def, fetched)
    }

    /// The only document of `entity`, whose row is `row` and whose meta rows are `meta`.
    fn document(
        tables: &[&str],
        entity: &str,
        columns: &[&str],
        row: &[&str],
        meta: &[(&str, &str)],
    ) -> EntityDoc {
        let id = row[0];
        let meta: Vec<(&str, &str, &str)> = meta.iter().map(|(k, v)| (id, *k, *v)).collect();
        let mut docs = build(tables, entity, columns, &[row], &meta, &Counts::new());
        assert_eq!(docs.len(), 1);
        docs.remove(0)
    }

    fn hint(doc: &EntityDoc, name: &str) -> Option<Hint> {
        doc.field(name).unwrap().hint.clone()
    }

    fn money(text: &str) -> Option<Hint> {
        Some(Hint::Money { text: text.into() })
    }

    #[test]
    fn an_amount_keeps_its_stored_text_and_gains_the_currency_of_its_row() {
        let doc = document(
            HPOS,
            "order",
            &["id", "type", "status", "currency", "total_amount"],
            &["4711", "shop_order", "wc-processing", "EUR", "49.90000000"],
            &[],
        );

        let total = doc.field("total_amount").unwrap();

        assert_eq!(total.value, t("49.90000000"));
        assert_eq!(total.hint, money("49.90000000 EUR"));
        assert_eq!(hint(&doc, "status"), None);
    }

    #[test]
    fn money_gets_a_hint_only_for_a_number_with_a_known_currency() {
        for (amount, currency, expected) in [
            ("49.90", "EUR", money("49.90 EUR")),
            ("-10.00", "EUR", money("-10.00 EUR")),
            (" 5 ", "USD", money("5 USD")),
            ("abc", "EUR", None),
            ("", "EUR", None),
            ("49.90", "", None),
        ] {
            let doc = document(
                HPOS,
                "order",
                &["id", "currency", "total_amount"],
                &["1", currency, amount],
                &[],
            );

            assert_eq!(
                hint(&doc, "total_amount"),
                expected,
                "{amount:?} in {currency:?}"
            );
        }
    }

    #[test]
    fn a_meta_amount_takes_the_currency_of_a_sibling_meta_row() {
        let doc = document(
            LEGACY,
            "order",
            &["ID", "post_type"],
            &["501", "shop_order"],
            &[("_order_total", "30.00"), ("_order_currency", "USD")],
        );

        assert_eq!(hint(&doc, "_order_total"), money("30.00 USD"));
        assert_eq!(hint(&doc, "_order_currency"), None);
    }

    #[test]
    fn a_meta_amount_without_a_currency_rule_has_no_hint() {
        let doc = document(
            HPOS,
            "order_item",
            &["order_item_id", "order_item_name", "order_id"],
            &["1", "Mug", "4711"],
            &[("_line_total", "45.00"), ("_qty", "2")],
        );

        assert_eq!(hint(&doc, "_line_total"), None);
        assert_eq!(doc.field("_line_total").unwrap().value, t("45.00"));
    }

    #[test]
    fn timestamps_get_a_unix_hint_only_where_the_zone_is_stored() {
        let doc = document(
            LEGACY,
            "order",
            &[
                "ID",
                "post_date",
                "post_date_gmt",
                "post_modified_gmt",
                "post_title",
            ],
            &[
                "501",
                "2026-01-02 11:00:00",
                "2026-01-02 10:00:00",
                "0000-00-00 00:00:00",
                "2026-01-02 10:00:00",
            ],
            &[
                ("_date_paid", "1700000000"),
                ("_edit_lock", "1700000100:3"),
                ("_paid_date", "2026-01-02 11:00:00"),
            ],
        );

        let hints: Vec<_> = [
            "post_date",
            "post_date_gmt",
            "post_modified_gmt",
            "post_title",
            "_date_paid",
            "_edit_lock",
            "_paid_date",
        ]
        .iter()
        .map(|name| (*name, hint(&doc, name)))
        .collect();

        assert_eq!(
            hints,
            [
                ("post_date", None),
                (
                    "post_date_gmt",
                    Some(Hint::Time {
                        unix: 1_767_348_000
                    })
                ),
                ("post_modified_gmt", None),
                ("post_title", None),
                (
                    "_date_paid",
                    Some(Hint::Time {
                        unix: 1_700_000_000
                    })
                ),
                (
                    "_edit_lock",
                    Some(Hint::Time {
                        unix: 1_700_000_100
                    })
                ),
                ("_paid_date", None),
            ]
        );
    }

    #[test]
    fn hints_serialize_with_a_type_tag_and_secrets_as_a_word() {
        let doc = document(
            HPOS,
            "user",
            &["ID", "user_pass", "user_registered"],
            &["12", "$P$x", "2026-01-02 10:00:00"],
            &[],
        );

        let registered = serde_json::to_value(hint(&doc, "user_registered")).unwrap();
        let pass = serde_json::to_value(doc.field("user_pass").unwrap().sensitive).unwrap();

        assert_eq!(
            registered,
            serde_json::json!({ "type": "time", "unix": 1_767_348_000 })
        );
        assert_eq!(pass, "secret");
        assert_eq!(
            serde_json::to_value(Hint::Money {
                text: "1 EUR".into()
            })
            .unwrap(),
            serde_json::json!({ "type": "money", "text": "1 EUR" })
        );
    }

    #[test]
    fn secret_columns_and_meta_keys_are_marked_and_keep_their_stored_value() {
        let doc = document(
            HPOS,
            "user",
            &["ID", "user_login", "user_pass"],
            &["12", "jane", "$P$secret"],
            &[("session_tokens", "a:0:{}"), ("first_name", "Jane")],
        );

        let marked: Vec<_> = ["user_login", "user_pass", "session_tokens", "first_name"]
            .iter()
            .map(|name| (*name, doc.field(name).unwrap().sensitive))
            .collect();

        assert_eq!(
            marked,
            [
                ("user_login", None),
                ("user_pass", Some(Sensitive::Secret)),
                ("session_tokens", Some(Sensitive::Secret)),
                ("first_name", None),
            ]
        );
        let pass = doc.field("user_pass").unwrap();
        assert_eq!(pass.value, t("$P$secret"));
        assert_eq!(pass.hint, None);
    }

    #[test]
    fn meta_rows_form_one_group_named_by_their_table_in_stored_order() {
        let doc = document(
            HPOS,
            "order",
            &["id", "type"],
            &["4711", "shop_order"],
            &[("_b", "1"), ("_a", "2"), ("_b", "3")],
        );

        assert_eq!(doc.meta.len(), 1);
        let group = &doc.meta[0];
        let entries: Vec<_> = group
            .entries
            .iter()
            .map(|e| (e.name.as_str(), e.value.clone()))
            .collect();
        assert_eq!(group.name, "wp_wc_orders_meta");
        assert!(group.private);
        assert_eq!(entries, [("_b", t("1")), ("_a", t("2")), ("_b", t("3"))]);
        assert_eq!(
            doc.meta_source,
            Some(MetaSource {
                table: "wp_wc_orders_meta".into(),
                fk: "order_id".into(),
                key: "meta_key".into(),
            })
        );
    }

    #[test]
    fn a_group_with_a_public_key_is_not_private() {
        let doc = document(
            HPOS,
            "order",
            &["id"],
            &["4711"],
            &[("_hidden", "1"), ("visible", "2")],
        );

        assert!(!doc.meta[0].private);
    }

    #[test]
    fn a_document_without_meta_rows_has_no_group_but_names_where_its_meta_lives() {
        let order = document(HPOS, "order", &["id"], &["4711"], &[]);
        let comment = document(HPOS, "comment", &["comment_ID"], &["1"], &[]);

        assert!(order.meta.is_empty());
        assert_eq!(
            order.meta_source.map(|m| m.table),
            Some("wp_wc_orders_meta".to_string())
        );
        assert_eq!(comment.meta_source, None, "no commentmeta table");
    }

    #[test]
    fn each_document_gets_only_the_meta_rows_of_its_owner() {
        let docs = build(
            HPOS,
            "order",
            &["id"],
            &[&["4711"], &["4713"]],
            &[("4713", "_b", "2"), ("4711", "_a", "1")],
            &Counts::new(),
        );

        let names = |doc: &EntityDoc| -> Vec<String> {
            doc.meta
                .iter()
                .flat_map(|g| g.entries.iter().map(|e| e.name.clone()))
                .collect()
        };

        assert_eq!(
            docs.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
            ["4711", "4713"]
        );
        assert_eq!(names(&docs[0]), ["_a"]);
        assert_eq!(names(&docs[1]), ["_b"]);
    }

    #[test]
    fn the_title_joins_the_summary_entries_from_columns_and_meta() {
        let doc = document(
            LEGACY,
            "order",
            &["ID", "post_type", "post_status", "post_date_gmt"],
            &["501", "shop_order", "wc-completed", "2026-01-02 10:00:00"],
            &[
                ("_payment_method", "ppcp-gateway"),
                ("_order_total", "30.00"),
            ],
        );

        assert_eq!(
            doc.title.as_deref(),
            Some("shop_order · wc-completed · ppcp-gateway · 30.00 · 2026-01-02 10:00:00")
        );
        assert_eq!(doc.subtitle, None);
    }

    #[test]
    fn blank_summary_entries_are_left_out_of_the_title() {
        let doc = document(
            LEGACY,
            "order",
            &["ID", "post_type", "post_status", "post_date_gmt"],
            &["501", "shop_order", "", "2026-01-02 10:00:00"],
            &[],
        );

        assert_eq!(
            doc.title.as_deref(),
            Some("shop_order · 2026-01-02 10:00:00")
        );
    }

    #[test]
    fn a_document_without_summary_text_has_no_title() {
        let doc = document(LEGACY, "order", &["ID"], &["501"], &[]);

        assert_eq!(doc.title, None);
    }

    #[test]
    fn a_document_names_its_entity_table_and_key_column() {
        let doc = document(HPOS, "order", &["id"], &["4711"], &[]);

        assert_eq!(
            (
                doc.entity.as_str(),
                doc.label.as_str(),
                doc.id.as_str(),
                doc.table.as_str(),
                doc.id_column.as_str()
            ),
            ("order", "order", "4711", "wp_wc_orders", "id")
        );
    }

    #[test]
    fn an_id_column_that_references_another_row_becomes_a_link() {
        for (stored, expected) in [
            ("12", Some("12")),
            (" 12 ", Some("12")),
            ("0", None),
            ("", None),
            ("abc", None),
        ] {
            let doc = document(
                HPOS,
                "order",
                &["id", "customer_id"],
                &["4711", stored],
                &[],
            );

            let link = doc.field("customer_id").unwrap().link.clone();

            assert_eq!(
                link,
                expected.map(|id| EntityLink {
                    entity: "user".into(),
                    id: id.into(),
                    label: None,
                    image: None,
                }),
                "{stored:?}"
            );
        }
    }

    #[test]
    fn a_meta_key_that_references_another_row_becomes_a_link() {
        let doc = document(
            LEGACY,
            "order",
            &["ID"],
            &["501"],
            &[("_customer_user", "7"), ("_payment_method", "bacs")],
        );

        assert_eq!(
            doc.field("_customer_user").unwrap().link,
            Some(EntityLink {
                entity: "user".into(),
                id: "7".into(),
                label: None,
                image: None,
            })
        );
        assert_eq!(doc.field("_payment_method").unwrap().link, None);
    }

    #[test]
    fn relations_carry_the_counts_of_their_owner_and_zero_where_there_are_none() {
        let counts: Counts = HashMap::from([
            (
                "items".to_string(),
                HashMap::from([("4711".to_string(), 2)]),
            ),
            (
                "notes".to_string(),
                HashMap::from([("4713".to_string(), 1)]),
            ),
        ]);

        let docs = build(
            HPOS,
            "order",
            &["id"],
            &[&["4711"], &["4713"]],
            &[],
            &counts,
        );

        let shape = |doc: &EntityDoc| -> Vec<(String, String, u64)> {
            doc.relations
                .iter()
                .map(|r| (r.name.clone(), r.entity.clone(), r.count))
                .collect()
        };
        let row =
            |name: &str, entity: &str, count: u64| (name.to_string(), entity.to_string(), count);
        assert_eq!(
            shape(&docs[0]),
            [
                row("notes", "comment", 0),
                row("children", "order", 0),
                row("addresses", "order_address", 0),
                row("items", "order_item", 2),
            ]
        );
        assert_eq!(shape(&docs[1])[0], row("notes", "comment", 1));
        assert_eq!(docs[0].relations[3].label, "items");
    }

    #[test]
    fn a_document_serializes_with_camel_case_keys() {
        let doc = document(HPOS, "order", &["id"], &["4711"], &[]);

        let value = serde_json::to_value(&doc).unwrap();

        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "entity",
                "fields",
                "id",
                "idColumn",
                "image",
                "label",
                "meta",
                "metaSource",
                "relations",
                "subtitle",
                "table",
                "title"
            ]
        );
    }
}
