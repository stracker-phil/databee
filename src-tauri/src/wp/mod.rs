//! WordPress screens over the effective rules of the `wordpress`, `woocommerce` and
//! `action-scheduler` presets: documents, lists, search, snapshots and the options inspector.

pub mod catalog;
pub mod changes;
pub mod commands;
pub mod dashboard;
pub mod document;
pub mod find;
pub mod lists;
pub mod media;
pub mod options;
pub mod rows;
pub mod search;
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
use crate::db::{ResultSet, escape_like};
use crate::error::{AppError, AppResult};
use crate::rules::effective::RulesInfo;

const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RELATED: u32 = 100;

/// Registered in `build.rs` too (`WP_COMMANDS`), which grants them to the main window through
/// `capabilities/wp.json`.
pub const COMMANDS: &[&str] = &[
    "wp_detect",
    "wp_options",
    "wp_dashboard",
    "wp_browser",
    "wp_orders",
    "wp_users",
    "wp_products",
    "wp_actions",
    "wp_action_histogram",
    "wp_failed_by_hook",
    "wp_sessions",
    "wp_content",
    "wp_search",
    "wp_find_value",
    "wp_options_group",
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
            commands::wp_options,
            commands::wp_dashboard,
            commands::wp_browser,
            commands::wp_orders,
            commands::wp_users,
            commands::wp_products,
            commands::wp_actions,
            commands::wp_action_histogram,
            commands::wp_failed_by_hook,
            commands::wp_sessions,
            commands::wp_content,
            commands::wp_search,
            commands::wp_find_value,
            commands::wp_options_group,
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
pub async fn post_images(
    db: &dyn Database,
    ctx: &WpContext,
    database: &str,
    post_ids: &[String],
) -> AppResult<HashMap<String, String>> {
    let dialect = db.dialect();
    let Ok(ids) = catalog::parse_ids(post_ids) else {
        return Ok(HashMap::new());
    };
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let postmeta = sql::table_ref(dialect, database, ctx, "postmeta");
    let thumbs = fetch(
        db,
        &format!(
            "SELECT post_id, meta_value FROM {postmeta} WHERE meta_key = '_thumbnail_id' AND post_id IN ({})",
            catalog::id_list(&ids)
        ),
    )
    .await?;
    let thumb_of: HashMap<String, u64> = thumbs
        .rows
        .iter()
        .filter_map(|r| Some((string(&r[0])?, string(&r[1])?.parse().ok()?)))
        .collect();
    let attachment_ids: Vec<u64> = thumb_of.values().copied().collect();
    if attachment_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let id_list = catalog::id_list(&attachment_ids);
    let files = fetch(
        db,
        &format!(
            "SELECT post_id, meta_key, meta_value FROM {postmeta} \
             WHERE meta_key IN ('_wp_attached_file', '_wp_attachment_metadata') AND post_id IN ({id_list})"
        ),
    )
    .await?;
    let mut attached: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
    for r in &files.rows {
        let (Some(id), Some(key)) = (string(&r[0]), string(&r[1])) else {
            continue;
        };
        let slot = attached.entry(id).or_default();
        if key == "_wp_attached_file" {
            slot.0 = string(&r[2]);
        } else {
            slot.1 = string(&r[2]);
        }
    }
    let guids = attachment_guids(db, ctx, database, &id_list).await?;
    Ok(thumb_of
        .into_iter()
        .filter_map(|(post, att)| {
            let att = att.to_string();
            let (file, meta) = attached.get(&att)?;
            let file = file.as_deref()?;
            let base = media::uploads_base_from_guid(guids.get(&att)?, file)?;
            Some((post, media::attachment_url(base, file, meta.as_deref())))
        })
        .collect())
}

/// `ID → guid` of attachment posts; `id_list` is a comma-separated list of IDs.
async fn attachment_guids(
    db: &dyn Database,
    ctx: &WpContext,
    database: &str,
    id_list: &str,
) -> AppResult<HashMap<String, String>> {
    let rs = fetch(
        db,
        &format!(
            "SELECT ID, guid FROM {} WHERE ID IN ({id_list})",
            sql::table_ref(db.dialect(), database, ctx, "posts")
        ),
    )
    .await?;
    Ok(rs
        .rows
        .iter()
        .filter_map(|r| Some((string(&r[0])?, string(&r[1])?)))
        .collect())
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
    let id = catalog::parse_ids(&[id.to_string()])?[0];
    let rs = fetch(
        db,
        &catalog::relation_ids_sql(dialect, database, rel, target, id, MAX_RELATED),
    )
    .await?;
    let ids: Vec<String> = rs.rows.iter().filter_map(|r| string(&r[0])).collect();
    load_docs(db, ctx, catalog, database, &rel.entity, &ids).await
}

pub async fn options_overview(
    db: &dyn Database,
    ctx: &WpContext,
    database: &str,
) -> AppResult<options::OptionsOverview> {
    let dialect = db.dialect();
    let table = ctx.table("options");
    let from = sql::table_ref(dialect, database, ctx, "options");
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
                name: string(&r[0])?,
                bytes: string(&r[1]).and_then(|b| b.parse().ok()).unwrap_or(0),
                autoload: string(&r[2]).unwrap_or_default(),
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
        .filter_map(|r| Some((string(&r[0])?, string(&r[1])?.trim().parse().ok()?)))
        .collect();
    Ok(options::overview(
        &table,
        &rows,
        &timeouts,
        chrono::Utc::now().timestamp(),
    ))
}

/// Options whose name starts with `prefix` (or all transients for `transients`), as fields
/// with decoded values and the kinds of the options' `[[meta]]` rules.
pub async fn options_group(
    db: &dyn Database,
    ctx: &WpContext,
    catalog: &Catalog,
    database: &str,
    prefix: &str,
) -> AppResult<Vec<document::DocField>> {
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
    let table = ctx.table("options");
    let rs = fetch(
        db,
        &format!(
            "SELECT option_name, option_value, autoload FROM {} WHERE {filter} ORDER BY option_name LIMIT 500",
            sql::table_ref(dialect, database, ctx, "options"),
        ),
    )
    .await?;
    let meta = ctx.rules.meta_table(&table);
    Ok(rs
        .rows
        .iter()
        .filter_map(|r| {
            let name = string(&r[0])?;
            let rule = meta.and_then(|m| ctx.rules.meta_kind_rule(m, &name));
            let mut f = document::field(catalog, rule, (&[], &[]), name, r[1].clone(), None);
            f.autoload = string(&r[2]).map(|value| document::Autoload {
                on: options::is_autoloaded(&value),
                value,
            });
            Some(f)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::document::{Autoload, Hint, Sensitive};
    use super::testing::{HPOS, site, site_with};
    use super::*;
    use crate::db::{Cell, ColumnMeta, FakeDatabase};

    fn info_of(ctx: &WpContext, catalog: &Catalog, legacy: Option<&Path>) -> WpInfo {
        let loaded = ctx.rules.applied.presets.clone();
        WpInfo::new(RulesInfo::new(&ctx.rules, &loaded), catalog, legacy)
    }

    fn options_result(rows: Vec<Vec<Cell>>) -> ResultSet {
        ResultSet {
            columns: ["option_name", "option_value", "autoload"]
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

    #[tokio::test]
    async fn options_group_masks_secret_options_and_reads_unix_timeouts() {
        let (ctx, catalog) = site(&["options"]);
        let cell = |s: &str| Cell::Text(s.into());
        let db = FakeDatabase::new().on(
            "SELECT option_name, option_value, autoload FROM",
            options_result(vec![
                vec![cell("blogname"), cell("Shop"), cell("yes")],
                vec![cell("gateway_api_key"), cell("sk_test"), cell("no")],
                vec![
                    cell("_transient_timeout_fresh"),
                    cell("4102444800"),
                    cell("no"),
                ],
            ]),
        );

        let fields = options_group(&db, &ctx, &catalog, "wp", "transients")
            .await
            .unwrap();

        let kinds: Vec<_> = fields
            .iter()
            .map(|f| (f.name.as_str(), f.hint.clone(), f.sensitive))
            .collect();
        assert_eq!(
            kinds,
            [
                ("blogname", None, None),
                ("gateway_api_key", None, Some(Sensitive::Secret)),
                (
                    "_transient_timeout_fresh",
                    Some(Hint::Time {
                        unix: 4_102_444_800
                    }),
                    None
                ),
            ]
        );
    }

    #[tokio::test]
    async fn options_group_reports_each_options_stored_autoload_and_whether_it_is_on() {
        let (ctx, catalog) = site(&["options"]);
        let cell = |s: &str| Cell::Text(s.into());
        let db = FakeDatabase::new().on(
            "SELECT option_name, option_value, autoload FROM",
            options_result(vec![
                vec![cell("woocommerce_a"), cell("1"), cell("yes")],
                vec![cell("woocommerce_b"), cell("2"), cell("auto-off")],
                vec![cell("woocommerce_c"), cell("3"), cell("on")],
            ]),
        );

        let fields = options_group(&db, &ctx, &catalog, "wp", "woocommerce")
            .await
            .unwrap();

        let autoloads: Vec<_> = fields
            .iter()
            .map(|f| (f.name.as_str(), f.autoload.clone()))
            .collect();
        let autoload = |value: &str, on: bool| {
            Some(Autoload {
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
