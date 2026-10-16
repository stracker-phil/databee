//! The detection queries: the table list, the probed options and, on multisite, the
//! network options and the `blogs` table. `context::build` turns the result into a `WpContext`.

use std::collections::{BTreeSet, HashMap};

use super::{PROBED_NETWORK_OPTIONS, PROBED_OPTIONS, main_prefix};
use crate::db::{Database, Dialect};
use crate::error::AppResult;
use crate::wp::fetch;
use crate::wp::rows::{cell_str, count, string};

/// WordPress's default network ID (`SITE_ID_CURRENT_SITE`); network options are read for it.
const MAIN_NETWORK: u64 = 1;

/// What the database holds, read without interpretation.
#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    /// The `main_prefix` of `tables`.
    pub prefix: String,
    pub tables: BTreeSet<String>,
    /// `PROBED_OPTIONS` that exist, by name; a NULL value is `None`.
    pub options: HashMap<String, Option<String>>,
    /// `PROBED_NETWORK_OPTIONS` from `{prefix}sitemeta` of the main network.
    pub network_options: HashMap<String, Option<String>>,
    /// `blog_id → (domain, path)` from `{prefix}blogs`.
    pub blogs: HashMap<u64, (String, String)>,
}

/// Reads what detection needs; `None` when no prefix has the core tables.
pub async fn probe(db: &dyn Database, database: &str) -> AppResult<Option<Probe>> {
    let tables: BTreeSet<String> = db
        .tables(database)
        .await?
        .into_iter()
        .map(|t| t.name)
        .collect();
    let Some(prefix) = main_prefix(&tables) else {
        return Ok(None);
    };
    let dialect = db.dialect();
    let table = |name: &str| dialect.table_ref(database, &format!("{prefix}{name}"));

    let options = name_values(
        db,
        &format!(
            "SELECT option_name, option_value FROM {} WHERE option_name IN ({})",
            table("options"),
            sql_list(dialect, PROBED_OPTIONS),
        ),
    )
    .await?;

    let network_options = if tables.contains(&format!("{prefix}sitemeta")) {
        name_values(
            db,
            &format!(
                "SELECT meta_key, meta_value FROM {} WHERE site_id = {MAIN_NETWORK} AND meta_key IN ({}) ORDER BY meta_id",
                table("sitemeta"),
                sql_list(dialect, PROBED_NETWORK_OPTIONS),
            ),
        )
        .await?
    } else {
        HashMap::new()
    };

    let blogs = if tables.contains(&format!("{prefix}blogs")) {
        read_blogs(db, &table("blogs")).await?
    } else {
        HashMap::new()
    };

    Ok(Some(Probe {
        prefix,
        tables,
        options,
        network_options,
        blogs,
    }))
}

fn sql_list(dialect: Dialect, values: &[&str]) -> String {
    values
        .iter()
        .map(|v| dialect.sql_string(v))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `name → value` from a two-column query; the first row of a repeated name wins.
async fn name_values(db: &dyn Database, sql: &str) -> AppResult<HashMap<String, Option<String>>> {
    let rs = fetch(db, sql).await?;
    let mut values = HashMap::new();
    for row in &rs.rows {
        if let Some(name) = string(&row[0]) {
            values
                .entry(name)
                .or_insert_with(|| cell_str(&row[1]).map(String::from));
        }
    }
    Ok(values)
}

async fn read_blogs(db: &dyn Database, table: &str) -> AppResult<HashMap<u64, (String, String)>> {
    let rs = fetch(db, &format!("SELECT blog_id, domain, path FROM {table}")).await?;
    Ok(rs
        .rows
        .iter()
        .filter_map(|r| {
            let id = count(&r[0])?;
            Some((id, (string(&r[1])?, string(&r[2])?)))
        })
        .collect())
}
