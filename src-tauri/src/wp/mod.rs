//! WordPress screens over the effective rules of the `wordpress`, `woocommerce` and
//! `action-scheduler` presets: documents, the dashboard and snapshots.

pub mod catalog;
pub mod changes;
pub mod commands;
pub mod dashboard;
pub mod document;
pub mod rows;
#[cfg(test)]
mod server_tests;
pub mod site;
pub mod snapshot;
pub mod snapshots_store;
pub mod sql;
#[cfg(test)]
mod testing;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use tauri::plugin::TauriPlugin;
use tauri::{Manager, Wry};
use ts_rs::TS;

use self::catalog::{Catalog, TableEntity};
use self::document::{EntityDoc, Fetched};
use self::rows::string;
use self::site::WpContext;
use crate::db::Database;
use crate::db::ResultSet;
use crate::error::{AppError, AppResult};
use crate::rules::effective::RulesInfo;

const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RELATED: u32 = 100;

/// Registered in `build.rs` too (`WP_COMMANDS`), which grants them to the main window through
/// `capabilities/wp.json`.
pub const COMMANDS: &[&str] = &[
    "wp_detect",
    "wp_dashboard",
    "wp_snapshot_take",
    "wp_snapshots",
    "wp_snapshot_diff",
    "wp_snapshot_delete",
    "wp_plan_change",
];

/// The plugin's managed state.
pub struct WpState {
    pub snapshots: snapshots_store::SnapshotStore,
    pub legacy_entities: Option<PathBuf>,
}

impl WpState {
    pub fn open(config_dir: &Path) -> Self {
        Self {
            snapshots: snapshots_store::SnapshotStore::open(config_dir),
            legacy_entities: legacy_entities(config_dir),
        }
    }
}

/// Pre-preset `entities.toml`: reported, never read.
fn legacy_entities(config_dir: &Path) -> Option<PathBuf> {
    let path = config_dir.join("entities.toml");
    if !path.exists() {
        return None;
    }
    log::warn!(
        "{} is no longer read; user presets replace it (docs/presets.md)",
        path.display()
    );
    Some(path)
}

/// Installs WordPress mode: its commands and the snapshot store in `config_dir`.
pub fn init(config_dir: &Path) -> TauriPlugin<Wry> {
    let config_dir = config_dir.to_path_buf();
    tauri::plugin::Builder::new("wp")
        .invoke_handler(tauri::generate_handler![
            commands::wp_detect,
            commands::wp_dashboard,
            commands::wp_snapshot_take,
            commands::wp_snapshots,
            commands::wp_snapshot_diff,
            commands::wp_snapshot_delete,
            commands::wp_plan_change,
        ])
        .setup(move |app, _| {
            app.manage(WpState::open(&config_dir));
            Ok(())
        })
        .build()
}

/// `rules_info` in the shape the WordPress screens read.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct WpInfo {
    pub prefix: String,
    /// The applied presets, each after the presets it requires.
    pub presets: Vec<String>,
    /// Entity names, once each, in the order of `RulesInfo.entities`.
    pub entities: Vec<String>,
    /// Real table → the entity its rows open as.
    pub table_entities: BTreeMap<String, TableEntity>,
    pub legacy_entities_file: Option<String>,
}

impl WpInfo {
    pub fn new(info: RulesInfo, catalog: &Catalog, legacy: Option<&Path>) -> Self {
        let mut entities: Vec<String> = Vec::new();
        for applied in info.entities {
            if !entities.contains(&applied.entity.name) {
                entities.push(applied.entity.name);
            }
        }
        Self {
            prefix: info.prefix,
            presets: info.applied_presets,
            entities,
            table_entities: catalog.table_entities(),
            legacy_entities_file: legacy.map(|p| p.display().to_string()),
        }
    }
}

pub(crate) async fn fetch(db: &dyn Database, sql: &str) -> AppResult<ResultSet> {
    db.fetch(sql, TIMEOUT).await
}

/// Loads documents for the given IDs, in that order, with meta and relation counts
/// fetched in one query each. Snapshots flatten them into paths.
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
    let ids = catalog::parse_ids(ids)?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = fetch(db, &catalog::rows_sql(dialect, database, def, &ids)).await?;
    let meta = match (&def.meta, rows.rows.is_empty()) {
        (Some(meta), false) => {
            Some(fetch(db, &catalog::meta_sql(dialect, database, meta, &ids)).await?)
        }
        _ => None,
    };
    let mut counts = HashMap::new();
    for rel in &def.relations {
        let Ok(target) = catalog.get(&rel.entity) else {
            continue;
        };
        let sql = catalog::relation_count_sql(dialect, database, rel, target, &ids);
        let rs = fetch(db, &sql).await?;
        let by_id: HashMap<String, u64> = rs
            .rows
            .iter()
            .filter_map(|r| Some((string(&r[0])?, string(&r[1])?.parse().ok()?)))
            .collect();
        counts.insert(rel.name.clone(), by_id);
    }
    let fetched = Fetched {
        rows: &rows,
        meta: meta.as_ref(),
        counts: &counts,
    };
    let mut docs = document::assemble(&ctx.rules, catalog, def, fetched);
    docs.sort_by_key(|d| ids.iter().position(|id| id.to_string() == d.id));
    Ok(docs)
}

/// Featured-image thumbnail URL per post ID (posts without one are left out). Each
/// attachment's uploads base is what its `guid` proves for its file.
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
    let id = catalog::parse_ids(&[id.to_string()])?[0];
    let rs = fetch(
        db,
        &catalog::relation_ids_sql(dialect, database, rel, target, id, MAX_RELATED),
    )
    .await?;
    let ids: Vec<String> = rs.rows.iter().filter_map(|r| string(&r[0])).collect();
    load_docs(db, ctx, catalog, database, &rel.entity, &ids).await
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::testing::{HPOS, site_with};
    use super::*;

    fn info_of(ctx: &WpContext, catalog: &Catalog, legacy: Option<&Path>) -> WpInfo {
        let loaded = ctx.rules.applied.presets.clone();
        WpInfo::new(RulesInfo::new(&ctx.rules, &loaded), catalog, legacy)
    }

    #[test]
    fn every_command_is_granted_in_build_rs() {
        let granted = crate::test_support::build_rs_commands("wp_");
        for name in COMMANDS {
            assert!(granted.contains(name), "{name} missing from build.rs");
        }
    }

    #[test]
    fn every_wp_command_in_build_rs_is_registered() {
        for name in crate::test_support::build_rs_commands("wp_") {
            assert!(COMMANDS.contains(&name), "{name} missing from COMMANDS");
        }
    }

    #[test]
    fn wp_info_names_each_entity_once_with_filtered_ones_first() {
        let (ctx, catalog) = site_with("wp_", HPOS, &["wordpress", "woocommerce"]);

        let info = info_of(&ctx, &catalog, None);

        assert_eq!(info.prefix, "wp_");
        assert_eq!(info.presets, ["wordpress", "woocommerce"]);
        assert_eq!(
            info.entities,
            [
                "order",
                "product",
                "post",
                "user",
                "comment",
                "option",
                "order_item",
                "order_address"
            ]
        );
        assert_eq!(info.table_entities, catalog.table_entities());
    }

    #[test]
    fn wp_info_serializes_with_camel_case_fields_and_the_legacy_file_path() {
        let (ctx, catalog) = site_with("wp_", &["options", "posts", "users"], &["wordpress"]);

        let with_file = info_of(&ctx, &catalog, Some(Path::new("/cfg/entities.toml")));
        let without = info_of(&ctx, &catalog, None);

        assert_eq!(
            serde_json::to_value(&with_file).unwrap(),
            serde_json::json!({
                "prefix": "wp_",
                "presets": ["wordpress"],
                "entities": ["post", "user", "option"],
                "tableEntities": {
                    "wp_options": { "entity": "option", "idColumn": "option_id" },
                    "wp_posts": { "entity": "post", "idColumn": "ID" },
                    "wp_users": { "entity": "user", "idColumn": "ID" },
                },
                "legacyEntitiesFile": "/cfg/entities.toml",
            })
        );
        assert_eq!(
            serde_json::to_value(&without).unwrap()["legacyEntitiesFile"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn an_old_entities_file_in_the_config_dir_is_reported_and_not_read() {
        let dir = tempdir().unwrap();
        assert_eq!(WpState::open(dir.path()).legacy_entities, None);

        std::fs::write(dir.path().join("entities.toml"), "[[entity]]").unwrap();

        assert_eq!(
            WpState::open(dir.path()).legacy_entities,
            Some(dir.path().join("entities.toml"))
        );
    }
}
