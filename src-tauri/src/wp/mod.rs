//! WordPress lens: detection, PHP value decoding, entity documents and the options
//! inspector. All WordPress knowledge lives here; the UI only renders the results.

pub mod cache;
pub mod changes;
pub mod context;
pub mod dashboard;
pub mod detail;
pub mod entity;
pub mod hints;
pub mod lists;
pub mod logs;
pub mod media;
pub mod options;
pub mod peek;
pub mod snapshot;

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use serde::Serialize;
use ts_rs::TS;

use self::context::WpContext;
use self::entity::{Catalog, EntityDoc, TableEntity};
use crate::db::Database;
use crate::db::{Cell, ResultSet, escape_like};
use crate::error::{AppError, AppResult};

const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RELATED: u32 = 100;

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct EntitySummary {
    pub name: String,
    pub label: String,
}

/// The list screens of WordPress mode.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "lowercase")]
pub enum WpList {
    Orders,
    Subscriptions,
    Users,
    Products,
    Actions,
    Sessions,
    Content,
    Settings,
}

#[derive(Serialize, Debug, Clone, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct WpInfo {
    pub context: WpContext,
    /// Entities that can be opened by ID.
    pub entities: Vec<EntitySummary>,
    pub table_entities: BTreeMap<String, TableEntity>,
}

impl WpInfo {
    pub fn new(context: WpContext, catalog: &Catalog) -> Self {
        Self {
            table_entities: catalog.table_entities(&context),
            entities: catalog.entity_names(),
            context,
        }
    }
}

fn text(cell: &Cell) -> Option<String> {
    match cell {
        Cell::Number(s) | Cell::Text(s) | Cell::Temporal(s) | Cell::Json(s) => Some(s.clone()),
        _ => None,
    }
}

pub(crate) async fn fetch(db: &dyn Database, sql: &str) -> AppResult<ResultSet> {
    db.fetch(sql, TIMEOUT).await
}

/// Probes a database; `None` when it isn't a WordPress install.
pub async fn detect(db: &dyn Database, database: &str) -> AppResult<Option<WpContext>> {
    let tables: Vec<String> = db
        .tables(database)
        .await?
        .into_iter()
        .map(|t| t.name)
        .collect();
    let Some(prefix) = context::main_prefix(&tables) else {
        return Ok(None);
    };
    let dialect = db.dialect();
    let names: Vec<String> = context::PROBED_OPTIONS
        .iter()
        .map(|n| dialect.sql_string(n))
        .collect();
    let rs = fetch(
        db,
        &format!(
            "SELECT option_name, option_value FROM {}.{} WHERE option_name IN ({})",
            dialect.quote_ident(database),
            dialect.quote_ident(&format!("{prefix}options")),
            names.join(", ")
        ),
    )
    .await?;
    let options: HashMap<String, String> = rs
        .rows
        .iter()
        .filter_map(|r| Some((text(&r[0])?, text(&r[1])?)))
        .collect();

    let mut blogs = HashMap::new();
    if tables.contains(&format!("{prefix}blogs")) {
        let rs = fetch(
            db,
            &format!(
                "SELECT blog_id, domain, path FROM {}.{}",
                dialect.quote_ident(database),
                dialect.quote_ident(&format!("{prefix}blogs"))
            ),
        )
        .await?;
        for r in &rs.rows {
            if let (Some(id), Some(domain), Some(path)) = (text(&r[0]), text(&r[1]), text(&r[2]))
                && let Ok(id) = id.parse()
            {
                blogs.insert(id, (domain, path));
            }
        }
    }
    Ok(context::build(&tables, &options, &blogs))
}

/// Loads documents for the given IDs, in that order, with meta and relation counts
/// fetched in one query each.
pub async fn load_docs(
    db: &dyn Database,
    ctx: &WpContext,
    catalog: &Catalog,
    database: &str,
    entity: &str,
    ids: &[String],
) -> AppResult<Vec<EntityDoc>> {
    let dialect = db.dialect();
    let def = catalog.get(entity)?;
    let ids = entity::parse_ids(ids)?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = fetch(db, &entity::rows_sql(dialect, database, ctx, def, &ids)).await?;
    let meta = match (&def.meta, rows.rows.is_empty()) {
        (Some(meta), false) => {
            Some(fetch(db, &entity::meta_sql(dialect, database, ctx, meta, &ids)).await?)
        }
        _ => None,
    };
    let mut counts = HashMap::new();
    for rel in def.relations.iter().filter(|r| r.when.applies(ctx)) {
        let Ok(target) = catalog.get(&rel.entity) else {
            continue;
        };
        let rs = fetch(
            db,
            &entity::relation_count_sql(dialect, database, ctx, rel, target, &ids),
        )
        .await?;
        let by_id: HashMap<String, u64> = rs
            .rows
            .iter()
            .filter_map(|r| Some((text(&r[0])?, text(&r[1])?.parse().ok()?)))
            .collect();
        counts.insert(rel.name.clone(), by_id);
    }
    let mut docs = entity::assemble(catalog, ctx, def, &rows, meta.as_ref(), &counts);
    docs.sort_by_key(|d| ids.iter().position(|id| id.to_string() == d.id));
    resolve_links(db, ctx, catalog, database, &mut docs).await?;
    if def.table == "posts" {
        let ids: Vec<String> = docs.iter().map(|d| d.id.clone()).collect();
        let images = post_images(db, ctx, database, &ids).await?;
        for doc in &mut docs {
            doc.image = images.get(&doc.id).cloned();
        }
    }
    Ok(docs)
}

/// `id → title` for entities of one kind, in one query.
pub async fn titles(
    db: &dyn Database,
    ctx: &WpContext,
    catalog: &Catalog,
    database: &str,
    entity: &str,
    ids: &[String],
) -> AppResult<HashMap<String, String>> {
    let dialect = db.dialect();
    let def = catalog.get(entity)?;
    let (Some(title), Ok(ids)) = (&def.title, entity::parse_ids(ids)) else {
        return Ok(HashMap::new());
    };
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rs = fetch(
        db,
        &format!(
            "SELECT {}, {} FROM {}.{} WHERE {} IN ({})",
            dialect.quote_ident(&def.id),
            dialect.quote_ident(title),
            dialect.quote_ident(database),
            dialect.quote_ident(&ctx.table(&def.table)),
            dialect.quote_ident(&def.id),
            ids.iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    )
    .await?;
    Ok(rs
        .rows
        .iter()
        .filter_map(|r| Some((text(&r[0])?, text(&r[1]).filter(|t| !t.is_empty())?)))
        .collect())
}

/// Featured-image thumbnail URL per post ID (posts without one are left out).
pub async fn post_images(
    db: &dyn Database,
    ctx: &WpContext,
    database: &str,
    post_ids: &[String],
) -> AppResult<HashMap<String, String>> {
    let dialect = db.dialect();
    let (Some(uploads), Ok(ids)) = (&ctx.site.uploads_url, entity::parse_ids(post_ids)) else {
        return Ok(HashMap::new());
    };
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let postmeta = format!(
        "{}.{}",
        dialect.quote_ident(database),
        dialect.quote_ident(&ctx.table("postmeta"))
    );
    let list = |ids: &[u64]| {
        ids.iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let thumbs = fetch(
        db,
        &format!(
            "SELECT post_id, meta_value FROM {postmeta} WHERE meta_key = '_thumbnail_id' AND post_id IN ({})",
            list(&ids)
        ),
    )
    .await?;
    let thumb_of: HashMap<String, u64> = thumbs
        .rows
        .iter()
        .filter_map(|r| Some((text(&r[0])?, text(&r[1])?.parse().ok()?)))
        .collect();
    let attachment_ids: Vec<u64> = thumb_of.values().copied().collect();
    if attachment_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let files = fetch(
        db,
        &format!(
            "SELECT post_id, meta_key, meta_value FROM {postmeta} \
             WHERE meta_key IN ('_wp_attached_file', '_wp_attachment_metadata') AND post_id IN ({})",
            list(&attachment_ids)
        ),
    )
    .await?;
    let mut attached: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
    for r in &files.rows {
        let (Some(id), Some(key)) = (text(&r[0]), text(&r[1])) else {
            continue;
        };
        let slot = attached.entry(id).or_default();
        if key == "_wp_attached_file" {
            slot.0 = text(&r[2]);
        } else {
            slot.1 = text(&r[2]);
        }
    }
    Ok(thumb_of
        .into_iter()
        .filter_map(|(post, att)| {
            let (file, meta) = attached.get(&att.to_string())?;
            Some((
                post,
                media::attachment_url(uploads, file.as_deref()?, meta.as_deref()),
            ))
        })
        .collect())
}

/// Fills `label` (and `image` for posts) of every link in the documents, one query per
/// target entity.
async fn resolve_links(
    db: &dyn Database,
    ctx: &WpContext,
    catalog: &Catalog,
    database: &str,
    docs: &mut [EntityDoc],
) -> AppResult<()> {
    let mut wanted: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for doc in docs.iter() {
        for f in doc.all_fields() {
            if let Some(link) = &f.link {
                wanted
                    .entry(link.entity.clone())
                    .or_default()
                    .push(link.id.clone());
            }
        }
    }
    for (entity, mut ids) in wanted {
        ids.sort();
        ids.dedup();
        let names = titles(db, ctx, catalog, database, &entity, &ids).await?;
        let images = if catalog.get(&entity)?.table == "posts" {
            post_images(db, ctx, database, &ids).await?
        } else {
            HashMap::new()
        };
        for doc in docs.iter_mut() {
            for f in doc.all_fields_mut() {
                if let Some(link) = f.link.as_mut().filter(|l| l.entity == entity) {
                    link.label = names.get(&link.id).cloned();
                    link.image = images.get(&link.id).cloned();
                }
            }
        }
    }
    Ok(())
}

pub async fn expand(
    db: &dyn Database,
    ctx: &WpContext,
    catalog: &Catalog,
    database: &str,
    entity: &str,
    id: &str,
    relation: &str,
) -> AppResult<Vec<EntityDoc>> {
    let dialect = db.dialect();
    let def = catalog.get(entity)?;
    let rel = def
        .relations
        .iter()
        .find(|r| r.name == relation)
        .ok_or_else(|| AppError::InvalidInput(format!("Unknown relation '{relation}'")))?;
    let target = catalog.get(&rel.entity)?;
    let id = entity::parse_ids(&[id.to_string()])?[0];
    let rs = fetch(
        db,
        &entity::relation_ids_sql(dialect, database, ctx, rel, target, id, MAX_RELATED),
    )
    .await?;
    let ids: Vec<String> = rs.rows.iter().filter_map(|r| text(&r[0])).collect();
    load_docs(db, ctx, catalog, database, &rel.entity, &ids).await
}

pub async fn options_overview(
    db: &dyn Database,
    ctx: &WpContext,
    database: &str,
) -> AppResult<options::OptionsOverview> {
    let dialect = db.dialect();
    let table = ctx.table("options");
    let from = format!(
        "{}.{}",
        dialect.quote_ident(database),
        dialect.quote_ident(&table)
    );
    let rows = fetch(
        db,
        &format!("SELECT option_name, LENGTH(option_value), autoload FROM {from}"),
    )
    .await?;
    let rows: Vec<options::OptionRow> = rows
        .rows
        .iter()
        .filter_map(|r| {
            Some(options::OptionRow {
                name: text(&r[0])?,
                bytes: text(&r[1]).and_then(|b| b.parse().ok()).unwrap_or(0),
                autoload: text(&r[2]).unwrap_or_default(),
            })
        })
        .collect();
    let timeouts = fetch(
        db,
        &format!(
            "SELECT option_name, option_value FROM {from} WHERE option_name LIKE {} OR option_name LIKE {}",
            dialect.like(r"\_transient\_timeout\_%"),
            dialect.like(r"\_site\_transient\_timeout\_%"),
        ),
    )
    .await?;
    let timeouts: HashMap<String, i64> = timeouts
        .rows
        .iter()
        .filter_map(|r| Some((text(&r[0])?, text(&r[1])?.trim().parse().ok()?)))
        .collect();
    Ok(options::overview(
        &table,
        &rows,
        &timeouts,
        chrono::Utc::now().timestamp(),
    ))
}

/// Options whose name starts with `prefix` (or all transients for `transients`), as
/// annotated fields with decoded values.
pub async fn options_group(
    db: &dyn Database,
    ctx: &WpContext,
    catalog: &Catalog,
    database: &str,
    prefix: &str,
) -> AppResult<Vec<entity::DocField>> {
    let dialect = db.dialect();
    let either = |a: &str, b: &str| {
        format!(
            "option_name LIKE {} OR option_name LIKE {}",
            dialect.like(a),
            dialect.like(b)
        )
    };
    let filter = match prefix {
        "transients" => either(r"\_transient\_%", r"\_site\_transient\_%"),
        "(single)" => format!(
            "option_name NOT LIKE {} AND option_name NOT LIKE '%-%'",
            dialect.like(r"%\_%")
        ),
        p => {
            let p = escape_like(p);
            either(&format!("{p}%"), &format!(r"\_{p}%"))
        }
    };
    let rs = fetch(
        db,
        &format!(
            "SELECT option_name, option_value, autoload FROM {}.{} WHERE {filter} ORDER BY option_name LIMIT 500",
            dialect.quote_ident(database),
            dialect.quote_ident(&ctx.table("options"))
        ),
    )
    .await?;
    let hints = hints::HintContext {
        currency: ctx.site.currency.clone(),
        zone: ctx.site.zone,
        price_decimals: ctx.site.price_decimals,
    };
    Ok(rs
        .rows
        .iter()
        .filter_map(|r| {
            let name = text(&r[0])?;
            let mut f = entity::field(catalog, &hints, name, r[1].clone(), None);
            f.autoload = text(&r[2]).map(|value| entity::Autoload {
                on: options::is_autoloaded(&value),
                value,
            });
            Some(f)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ColumnMeta, FakeDatabase};
    use crate::wp::entity::definitions;

    fn ctx() -> WpContext {
        WpContext {
            prefix: "wp_".into(),
            version: None,
            sites: vec![],
            wc: None,
            site: Default::default(),
            tables: ["wp_options".to_string(), "wp_users".to_string()].into(),
        }
    }

    #[test]
    fn wp_lists_serialize_to_lowercase_wire_strings() {
        for (list, wire) in [
            (WpList::Orders, "orders"),
            (WpList::Subscriptions, "subscriptions"),
            (WpList::Users, "users"),
            (WpList::Products, "products"),
            (WpList::Actions, "actions"),
            (WpList::Sessions, "sessions"),
            (WpList::Content, "content"),
            (WpList::Settings, "settings"),
        ] {
            assert_eq!(serde_json::to_value(list).unwrap(), wire);
        }
    }

    #[test]
    fn wp_info_lists_openable_entities_with_name_and_label() {
        let ctx = ctx();
        let catalog = Catalog::new(&definitions(None).unwrap(), &ctx);

        let info = serde_json::to_value(WpInfo::new(ctx, &catalog)).unwrap();

        assert!(
            info["entities"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!({ "name": "user", "label": "User" })),
            "{info}"
        );
    }

    #[tokio::test]
    async fn options_group_reports_each_options_stored_autoload_and_whether_it_is_on() {
        let ctx = ctx();
        let catalog = Catalog::new(&definitions(None).unwrap(), &ctx);
        let cell = |s: &str| Cell::Text(s.into());
        let db = FakeDatabase::new().on(
            "SELECT option_name, option_value, autoload FROM",
            ResultSet {
                columns: ["option_name", "option_value", "autoload"]
                    .iter()
                    .map(|name| ColumnMeta {
                        name: name.to_string(),
                        type_name: "TEXT".into(),
                    })
                    .collect(),
                rows: vec![
                    vec![cell("woocommerce_a"), cell("1"), cell("yes")],
                    vec![cell("woocommerce_b"), cell("2"), cell("auto-off")],
                    vec![cell("woocommerce_c"), cell("3"), cell("on")],
                ],
                returns_rows: true,
                ..Default::default()
            },
        );

        let fields = options_group(&db, &ctx, &catalog, "wp", "woocommerce")
            .await
            .unwrap();

        let autoloads: Vec<_> = fields
            .iter()
            .map(|f| (f.name.as_str(), f.autoload.clone()))
            .collect();
        let autoload = |value: &str, on: bool| {
            Some(entity::Autoload {
                value: value.into(),
                on,
            })
        };
        assert_eq!(
            autoloads,
            [
                ("woocommerce_a", autoload("yes", true)),
                ("woocommerce_b", autoload("auto-off", false)),
                ("woocommerce_c", autoload("on", true)),
            ]
        );
    }
}
