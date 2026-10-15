//! WordPress lens: detection, PHP value decoding, entity documents and the options
//! inspector. All WordPress knowledge lives here; the UI only renders the results.

pub mod context;
pub mod entity;
pub mod options;
pub mod php;
pub mod value;

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use serde::Serialize;

use self::context::WpContext;
use self::entity::{Catalog, EntityDoc, TableEntity};
use crate::db::mysql::MySqlSession;
use crate::db::{Cell, ResultSet, quote_ident};
use crate::error::{AppError, AppResult};

const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RELATED: u32 = 100;

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WpInfo {
    pub context: WpContext,
    /// (name, label) of entities that can be opened by ID.
    pub entities: Vec<(String, String)>,
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

fn sql_string(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "''"))
}

async fn fetch(db: &MySqlSession, sql: &str) -> AppResult<ResultSet> {
    db.fetch(sql, TIMEOUT).await
}

/// Probes a database; `None` when it isn't a WordPress install.
pub async fn detect(db: &MySqlSession, database: &str) -> AppResult<Option<WpContext>> {
    let tables: Vec<String> = db
        .tables(database)
        .await?
        .into_iter()
        .map(|t| t.name)
        .collect();
    let Some(prefix) = context::main_prefix(&tables) else {
        return Ok(None);
    };
    let names: Vec<String> = context::PROBED_OPTIONS
        .iter()
        .map(|n| sql_string(n))
        .collect();
    let rs = fetch(
        db,
        &format!(
            "SELECT option_name, option_value FROM {}.{} WHERE option_name IN ({})",
            quote_ident(database),
            quote_ident(&format!("{prefix}options")),
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
                quote_ident(database),
                quote_ident(&format!("{prefix}blogs"))
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
    db: &MySqlSession,
    ctx: &WpContext,
    catalog: &Catalog,
    database: &str,
    entity: &str,
    ids: &[String],
) -> AppResult<Vec<EntityDoc>> {
    let def = catalog.get(entity)?;
    let ids = entity::parse_ids(ids)?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = fetch(db, &entity::rows_sql(database, ctx, def, &ids)).await?;
    let meta = match (&def.meta, rows.rows.is_empty()) {
        (Some(meta), false) => Some(fetch(db, &entity::meta_sql(database, ctx, meta, &ids)).await?),
        _ => None,
    };
    let mut counts = HashMap::new();
    for rel in &def.relations {
        let Ok(target) = catalog.get(&rel.entity) else {
            continue;
        };
        let rs = fetch(
            db,
            &entity::relation_count_sql(database, ctx, rel, target, &ids),
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
    Ok(docs)
}

pub async fn expand(
    db: &MySqlSession,
    ctx: &WpContext,
    catalog: &Catalog,
    database: &str,
    entity: &str,
    id: &str,
    relation: &str,
) -> AppResult<Vec<EntityDoc>> {
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
        &entity::relation_ids_sql(database, ctx, rel, target, id, MAX_RELATED),
    )
    .await?;
    let ids: Vec<String> = rs.rows.iter().filter_map(|r| text(&r[0])).collect();
    load_docs(db, ctx, catalog, database, &rel.entity, &ids).await
}

pub async fn options_overview(
    db: &MySqlSession,
    ctx: &WpContext,
    database: &str,
) -> AppResult<options::OptionsOverview> {
    let table = ctx.table("options");
    let from = format!("{}.{}", quote_ident(database), quote_ident(&table));
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
            "SELECT option_name, option_value FROM {from} \
             WHERE option_name LIKE '\\_transient\\_timeout\\_%' OR option_name LIKE '\\_site\\_transient\\_timeout\\_%'"
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
