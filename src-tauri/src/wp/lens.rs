//! Plugin lens: which plugin owns what in the database. Collects keys from options, every
//! meta table, Action Scheduler hooks, log sources and table names (pure SQL), attributes
//! them with the registry → prefix clustering → gateway values → manual assignments, and
//! sums storage per plugin. Plugins with data but not in `active_plugins` are "leftover".

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::context::WpContext;
use super::lists::{epoch, num, text};
use super::options::{is_autoloaded, option_prefix};
use super::plugins::{Lookup, Plugin};
use super::{fetch, quote_ident, sql_string};
use crate::db::mysql::MySqlSession;
use crate::error::AppResult;
use crate::store::json_file::JsonFile;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case", tag = "kind", content = "table")]
pub enum KeyKind {
    Option,
    Transient,
    /// Meta key in the given table (without prefix), e.g. `postmeta`.
    Meta(String),
    Hook,
    LogSource,
    Table,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct KeyRef {
    #[serde(flatten)]
    pub kind: KeyKind,
    pub name: String,
}

impl KeyRef {
    /// Stable id for manual assignments: `option:name`, `meta:postmeta:_key`, `hook:name`…
    pub fn id(&self) -> String {
        match &self.kind {
            KeyKind::Option => format!("option:{}", self.name),
            KeyKind::Transient => format!("transient:{}", self.name),
            KeyKind::Meta(table) => format!("meta:{table}:{}", self.name),
            KeyKind::Hook => format!("hook:{}", self.name),
            KeyKind::LogSource => format!("log:{}", self.name),
            KeyKind::Table => format!("table:{}", self.name),
        }
    }

    pub fn parse(id: &str) -> Option<KeyRef> {
        let (kind, rest) = id.split_once(':')?;
        Some(match kind {
            "option" => KeyRef {
                kind: KeyKind::Option,
                name: rest.into(),
            },
            "transient" => KeyRef {
                kind: KeyKind::Transient,
                name: rest.into(),
            },
            "meta" => {
                let (table, name) = rest.split_once(':')?;
                KeyRef {
                    kind: KeyKind::Meta(table.into()),
                    name: name.into(),
                }
            }
            "hook" => KeyRef {
                kind: KeyKind::Hook,
                name: rest.into(),
            },
            "log" => KeyRef {
                kind: KeyKind::LogSource,
                name: rest.into(),
            },
            "table" => KeyRef {
                kind: KeyKind::Table,
                name: rest.into(),
            },
            _ => return None,
        })
    }
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyStat {
    #[serde(flatten)]
    pub key: KeyRef,
    /// Rows for meta/hooks/logs/tables, 1 for options.
    pub count: u64,
    pub bytes: u64,
    pub autoload: bool,
    pub last_written: Option<i64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Exact,
    Likely,
    Guess,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttributionSource {
    Registry,
    Prefix,
    Value,
    Manual,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Attributed {
    #[serde(flatten)]
    pub stat: KeyStat,
    /// `None` = unattributed.
    pub plugin: Option<String>,
    pub confidence: Option<Confidence>,
    pub source: Option<AttributionSource>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct MetaTotals {
    pub table: String,
    pub keys: u64,
    pub rows: u64,
    pub bytes: u64,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginReport {
    pub slug: String,
    pub name: String,
    /// Listed in `active_plugins` (core, transients and other pseudo-plugins count as active).
    pub active: bool,
    pub option_count: u64,
    pub option_bytes: u64,
    pub autoload_bytes: u64,
    pub transient_count: u64,
    pub transient_bytes: u64,
    pub meta: Vec<MetaTotals>,
    pub hook_count: u64,
    pub log_entries: u64,
    pub log_bytes: u64,
    pub tables: Vec<KeyStat>,
    /// Orders paid through this plugin's gateways (value attribution).
    pub orders: u64,
    pub last_written: Option<i64>,
    /// Total bytes we can account for (options, transients, meta, logs, tables).
    pub total_bytes: u64,
    pub keys: Vec<Attributed>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LensReport {
    pub active_plugins: Vec<String>,
    pub plugins: Vec<PluginReport>,
    pub unattributed: Vec<Attributed>,
}

/// Manual assignments (`<config>/attributions.json`): key id → plugin slug.
pub type Assignments = BTreeMap<String, String>;

pub fn assignments_file(config_dir: &Path) -> JsonFile {
    JsonFile::new(config_dir.join("attributions.json"))
}

// ---------- collection ----------

const META_TABLES: &[(&str, &str)] = &[
    ("postmeta", "meta_key"),
    ("usermeta", "meta_key"),
    ("termmeta", "meta_key"),
    ("commentmeta", "meta_key"),
    ("wc_orders_meta", "meta_key"),
    ("woocommerce_order_itemmeta", "meta_key"),
];

pub async fn collect(session: &MySqlSession, ctx: &WpContext, db: &str) -> AppResult<Vec<KeyStat>> {
    let t = |table: &str| format!("{}.{}", quote_ident(db), quote_ident(&ctx.table(table)));
    let mut out = Vec::new();

    let rs = fetch(
        session,
        &format!(
            "SELECT option_name, LENGTH(option_value), autoload FROM {}",
            t("options")
        ),
    )
    .await?;
    for r in &rs.rows {
        let Some(name) = text(&r[0]) else { continue };
        let transient = name.starts_with("_transient_") || name.starts_with("_site_transient_");
        out.push(KeyStat {
            key: KeyRef {
                kind: if transient {
                    KeyKind::Transient
                } else {
                    KeyKind::Option
                },
                name,
            },
            count: 1,
            bytes: num(&r[1]).unwrap_or(0),
            autoload: text(&r[2]).is_some_and(|a| is_autoloaded(&a)),
            last_written: None,
        });
    }

    for (table, col) in META_TABLES {
        if !ctx.has_table(table) {
            continue;
        }
        let rs = fetch(
            session,
            &format!(
                "SELECT {col}, COUNT(*), SUM(LENGTH(meta_value)) FROM {} GROUP BY {col} ORDER BY COUNT(*) DESC LIMIT 3000",
                t(table)
            ),
        )
        .await?;
        for r in &rs.rows {
            let Some(name) = text(&r[0]) else { continue };
            out.push(KeyStat {
                key: KeyRef {
                    kind: KeyKind::Meta(table.to_string()),
                    name,
                },
                count: num(&r[1]).unwrap_or(0),
                bytes: num(&r[2]).unwrap_or(0),
                autoload: false,
                last_written: None,
            });
        }
    }

    if ctx.has_table("actionscheduler_actions") {
        let rs = fetch(
            session,
            &format!(
                "SELECT hook, COUNT(*), MAX({}) FROM {} GROUP BY hook ORDER BY COUNT(*) DESC LIMIT 1000",
                epoch("scheduled_date_gmt"),
                t("actionscheduler_actions")
            ),
        )
        .await?;
        for r in &rs.rows {
            let Some(name) = text(&r[0]) else { continue };
            out.push(KeyStat {
                key: KeyRef {
                    kind: KeyKind::Hook,
                    name,
                },
                count: num(&r[1]).unwrap_or(0),
                bytes: 0,
                autoload: false,
                last_written: super::lists::int(&r[2]),
            });
        }
    }

    if ctx.has_table("woocommerce_log") {
        let rs = fetch(
            session,
            &format!(
                "SELECT source, COUNT(*), SUM(LENGTH(message)), MAX({}) FROM {} GROUP BY source ORDER BY COUNT(*) DESC LIMIT 500",
                epoch("timestamp"),
                t("woocommerce_log")
            ),
        )
        .await?;
        for r in &rs.rows {
            let Some(name) = text(&r[0]) else { continue };
            out.push(KeyStat {
                key: KeyRef {
                    kind: KeyKind::LogSource,
                    name,
                },
                count: num(&r[1]).unwrap_or(0),
                bytes: num(&r[2]).unwrap_or(0),
                autoload: false,
                last_written: super::lists::int(&r[3]),
            });
        }
    }

    for table in session.tables(db).await? {
        if table.is_view {
            continue;
        }
        let Some(short) = table.name.strip_prefix(&ctx.prefix) else {
            continue;
        };
        out.push(KeyStat {
            key: KeyRef {
                kind: KeyKind::Table,
                name: short.to_string(),
            },
            count: table.rows_estimate.unwrap_or(0),
            bytes: table.size_bytes.unwrap_or(0),
            autoload: false,
            last_written: None,
        });
    }
    Ok(out)
}

/// Orders per gateway id, for value attribution.
pub async fn gateway_orders(
    session: &MySqlSession,
    ctx: &WpContext,
    db: &str,
) -> AppResult<HashMap<String, u64>> {
    if ctx.wc.is_none() {
        return Ok(HashMap::new());
    }
    let sql = if ctx.hpos() {
        format!(
            "SELECT payment_method, COUNT(*) FROM {}.{} WHERE type = 'shop_order' AND payment_method <> '' GROUP BY payment_method",
            quote_ident(db),
            quote_ident(&ctx.table("wc_orders"))
        )
    } else {
        format!(
            "SELECT m.meta_value, COUNT(*) FROM {db}.{posts} p JOIN {db}.{pm} m ON m.post_id = p.ID AND m.meta_key = '_payment_method' \
             WHERE p.post_type = 'shop_order' AND m.meta_value <> '' GROUP BY m.meta_value",
            db = quote_ident(db),
            posts = quote_ident(&ctx.table("posts")),
            pm = quote_ident(&ctx.table("postmeta"))
        )
    };
    let rs = fetch(session, &sql).await?;
    Ok(rs
        .rows
        .iter()
        .filter_map(|r| Some((text(&r[0])?, num(&r[1]).unwrap_or(0))))
        .collect())
}

// ---------- attribution ----------

fn tokens(name: &str) -> Vec<String> {
    name.trim_start_matches('_')
        .split(['_', '-'])
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect()
}

/// Prefix clustering: the key's leading token(s) match an active plugin slug or one of its
/// slug tokens (`yoast_x` → `wordpress-seo` only via an alias; `acme_x` → `acme-widgets`).
pub fn cluster_match<'a>(name: &str, active: &'a [String]) -> Option<&'a String> {
    let toks = tokens(name);
    if toks.is_empty() {
        return None;
    }
    let mut best: Option<(&String, usize)> = None;
    for slug in active {
        let slug_toks: Vec<String> = slug.split(['-', '_']).map(|s| s.to_lowercase()).collect();
        // Longest run of leading key tokens that also leads the slug tokens.
        let mut run = 0;
        while run < toks.len() && run < slug_toks.len() && toks[run] == slug_toks[run] {
            run += 1;
        }
        if run == 0 && toks[0].len() >= 4 && slug_toks.iter().any(|s| *s == toks[0]) {
            run = 1;
        }
        let score = run * 10 + usize::from(run == slug_toks.len());
        if run > 0 && best.is_none_or(|(_, s)| score > s) {
            best = Some((slug, score));
        }
    }
    best.map(|(s, _)| s)
}

fn registry_owner<'a>(lookup: &Lookup<'a>, key: &KeyRef) -> Option<&'a Plugin> {
    match &key.kind {
        KeyKind::Option => lookup.for_option_prefix(&option_prefix(&key.name)),
        KeyKind::Transient => {
            let inner = key
                .name
                .trim_start_matches("_site_transient_")
                .trim_start_matches("_transient_")
                .trim_start_matches("timeout_");
            lookup.for_option_prefix(&option_prefix(inner))
        }
        KeyKind::Meta(_) => lookup.for_meta_key(&key.name),
        KeyKind::Hook => lookup.for_hook(&key.name),
        KeyKind::LogSource => lookup.for_log_source(&key.name),
        KeyKind::Table => lookup.for_table(&key.name),
    }
}

pub fn attribute(
    stats: Vec<KeyStat>,
    lookup: &Lookup<'_>,
    active: &[String],
    manual: &Assignments,
) -> Vec<Attributed> {
    stats
        .into_iter()
        .map(|stat| {
            if let Some(slug) = manual.get(&stat.key.id()) {
                return Attributed {
                    stat,
                    plugin: Some(slug.clone()),
                    confidence: Some(Confidence::Exact),
                    source: Some(AttributionSource::Manual),
                };
            }
            if let Some(p) = registry_owner(lookup, &stat.key) {
                return Attributed {
                    stat,
                    plugin: Some(p.slug.clone()),
                    confidence: Some(Confidence::Exact),
                    source: Some(AttributionSource::Registry),
                };
            }
            let name = match &stat.key.kind {
                KeyKind::Transient => stat
                    .key
                    .name
                    .trim_start_matches("_site_transient_")
                    .trim_start_matches("_transient_")
                    .trim_start_matches("timeout_")
                    .to_string(),
                _ => stat.key.name.clone(),
            };
            if let Some(slug) = cluster_match(&name, active) {
                return Attributed {
                    stat,
                    plugin: Some(slug.clone()),
                    confidence: Some(Confidence::Likely),
                    source: Some(AttributionSource::Prefix),
                };
            }
            Attributed {
                stat,
                plugin: None,
                confidence: None,
                source: None,
            }
        })
        .collect()
}

/// Pseudo-plugins that are always "active" (they are not in `active_plugins`).
const ALWAYS_ACTIVE: &[&str] = &["core", "rewrite", "transients", "action-scheduler"];

pub fn report(
    attributed: Vec<Attributed>,
    lookup: &Lookup<'_>,
    active: &[String],
    orders_by_gateway: &HashMap<String, u64>,
) -> LensReport {
    let mut by_plugin: BTreeMap<String, PluginReport> = BTreeMap::new();
    let mut unattributed = Vec::new();
    let is_active = |slug: &str| ALWAYS_ACTIVE.contains(&slug) || active.iter().any(|a| a == slug);
    let name_of = |slug: &str| {
        lookup
            .by_slug(slug)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| slug.to_string())
    };
    for a in attributed {
        let Some(slug) = a.plugin.clone() else {
            unattributed.push(a);
            continue;
        };
        let p = by_plugin
            .entry(slug.clone())
            .or_insert_with(|| PluginReport {
                name: name_of(&slug),
                active: is_active(&slug),
                slug: slug.clone(),
                option_count: 0,
                option_bytes: 0,
                autoload_bytes: 0,
                transient_count: 0,
                transient_bytes: 0,
                meta: Vec::new(),
                hook_count: 0,
                log_entries: 0,
                log_bytes: 0,
                tables: Vec::new(),
                orders: 0,
                last_written: None,
                total_bytes: 0,
                keys: Vec::new(),
            });
        match &a.stat.key.kind {
            KeyKind::Option => {
                p.option_count += 1;
                p.option_bytes += a.stat.bytes;
                if a.stat.autoload {
                    p.autoload_bytes += a.stat.bytes;
                }
            }
            KeyKind::Transient => {
                p.transient_count += 1;
                p.transient_bytes += a.stat.bytes;
            }
            KeyKind::Meta(table) => {
                let m = match p.meta.iter_mut().find(|m| m.table == *table) {
                    Some(m) => m,
                    None => {
                        p.meta.push(MetaTotals {
                            table: table.clone(),
                            ..Default::default()
                        });
                        p.meta.last_mut().unwrap()
                    }
                };
                m.keys += 1;
                m.rows += a.stat.count;
                m.bytes += a.stat.bytes;
            }
            KeyKind::Hook => p.hook_count += a.stat.count,
            KeyKind::LogSource => {
                p.log_entries += a.stat.count;
                p.log_bytes += a.stat.bytes;
            }
            KeyKind::Table => p.tables.push(a.stat.clone()),
        }
        if let Some(t) = a.stat.last_written {
            p.last_written = Some(p.last_written.map_or(t, |x| x.max(t)));
        }
        p.keys.push(a);
    }
    for (gateway, n) in orders_by_gateway {
        if let Some(pl) = lookup.for_gateway(gateway) {
            let p = by_plugin
                .entry(pl.slug.clone())
                .or_insert_with(|| PluginReport {
                    name: pl.name.clone(),
                    active: is_active(&pl.slug),
                    slug: pl.slug.clone(),
                    option_count: 0,
                    option_bytes: 0,
                    autoload_bytes: 0,
                    transient_count: 0,
                    transient_bytes: 0,
                    meta: Vec::new(),
                    hook_count: 0,
                    log_entries: 0,
                    log_bytes: 0,
                    tables: Vec::new(),
                    orders: 0,
                    last_written: None,
                    total_bytes: 0,
                    keys: Vec::new(),
                });
            p.orders += n;
        }
    }
    let mut plugins: Vec<PluginReport> = by_plugin
        .into_values()
        .map(|mut p| {
            p.total_bytes = p.option_bytes
                + p.transient_bytes
                + p.meta.iter().map(|m| m.bytes).sum::<u64>()
                + p.log_bytes
                + p.tables.iter().map(|t| t.bytes).sum::<u64>();
            p.keys.sort_by(|a, b| {
                b.stat
                    .bytes
                    .cmp(&a.stat.bytes)
                    .then(a.stat.key.cmp(&b.stat.key))
            });
            p
        })
        .collect();
    plugins.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes).then(a.name.cmp(&b.name)));
    unattributed.sort_by_key(|a| std::cmp::Reverse(a.stat.bytes));
    LensReport {
        active_plugins: active.to_vec(),
        plugins,
        unattributed,
    }
}

pub async fn lens(
    session: &MySqlSession,
    ctx: &WpContext,
    db: &str,
    lookup: &Lookup<'_>,
    manual: &Assignments,
) -> AppResult<LensReport> {
    let stats = collect(session, ctx, db).await?;
    let orders = gateway_orders(session, ctx, db).await?;
    let attributed = attribute(stats, lookup, &ctx.site.active_plugins, manual);
    Ok(report(
        attributed,
        lookup,
        &ctx.site.active_plugins,
        &orders,
    ))
}

/// DELETE statements for a set of key ids (options, transients and meta rows only; tables
/// and logs are listed for manual review, never dropped).
pub fn cleanup_statements(db: &str, ctx: &WpContext, key_ids: &[String]) -> Vec<String> {
    let mut options: Vec<String> = Vec::new();
    let mut meta: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for id in key_ids {
        let Some(key) = KeyRef::parse(id) else {
            continue;
        };
        match key.kind {
            KeyKind::Option | KeyKind::Transient => options.push(sql_string(&key.name)),
            KeyKind::Meta(table) => meta.entry(table).or_default().push(sql_string(&key.name)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    if !options.is_empty() {
        out.push(format!(
            "DELETE FROM {}.{} WHERE option_name IN ({})",
            quote_ident(db),
            quote_ident(&ctx.table("options")),
            options.join(", ")
        ));
    }
    for (table, keys) in meta {
        out.push(format!(
            "DELETE FROM {}.{} WHERE meta_key IN ({})",
            quote_ident(db),
            quote_ident(&ctx.table(&table)),
            keys.join(", ")
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stat(kind: KeyKind, name: &str, bytes: u64) -> KeyStat {
        KeyStat {
            key: KeyRef {
                kind,
                name: name.into(),
            },
            count: 1,
            bytes,
            autoload: false,
            last_written: None,
        }
    }

    fn ctx() -> WpContext {
        WpContext {
            prefix: "wp_".into(),
            version: None,
            sites: vec![],
            wc: None,
            site: Default::default(),
            tables: Default::default(),
        }
    }

    #[test]
    fn key_ids_round_trip() {
        for k in [
            KeyRef {
                kind: KeyKind::Option,
                name: "woocommerce_x".into(),
            },
            KeyRef {
                kind: KeyKind::Meta("postmeta".into()),
                name: "_ppcp:weird".into(),
            },
            KeyRef {
                kind: KeyKind::Hook,
                name: "h".into(),
            },
            KeyRef {
                kind: KeyKind::Table,
                name: "wc_orders".into(),
            },
        ] {
            assert_eq!(KeyRef::parse(&k.id()).unwrap(), k);
        }
        assert!(KeyRef::parse("nope").is_none());
    }

    #[test]
    fn prefix_clustering_matches_active_plugin_slugs() {
        let active = vec![
            "acme-widgets".to_string(),
            "wordpress-seo".to_string(),
            "woocommerce".to_string(),
        ];
        assert_eq!(
            cluster_match("acme_widgets_version", &active).unwrap(),
            "acme-widgets"
        );
        assert_eq!(
            cluster_match("acme_cache", &active).unwrap(),
            "acme-widgets"
        );
        assert_eq!(
            cluster_match("_widgets_tmp", &active).unwrap(),
            "acme-widgets"
        );
        assert!(cluster_match("yoast_indexable", &active).is_none());
        assert!(cluster_match("x", &active).is_none());
    }

    #[test]
    fn attribution_prefers_manual_then_registry_then_prefix() {
        let active = vec![
            "acme-widgets".to_string(),
            "woocommerce-paypal-payments".to_string(),
        ];
        let mut manual = Assignments::new();
        manual.insert("option:mystery_key".into(), "acme-widgets".into());
        let stats = vec![
            stat(KeyKind::Option, "mystery_key", 10),
            stat(KeyKind::Option, "woocommerce_ppcp-settings", 20),
            stat(KeyKind::Option, "acme_widgets_version", 30),
            stat(
                KeyKind::Meta("postmeta".into()),
                "_ppcp_paypal_order_id",
                40,
            ),
            stat(KeyKind::Transient, "_transient_acme_cache", 50),
            stat(KeyKind::Option, "zzz_unknown", 60),
        ];
        let out = attribute(stats, &Lookup::default(), &active, &manual);
        let find = |n: &str| out.iter().find(|a| a.stat.key.name == n).unwrap();
        assert_eq!(
            (
                find("mystery_key").plugin.as_deref(),
                find("mystery_key").source
            ),
            (Some("acme-widgets"), Some(AttributionSource::Manual))
        );
        assert_eq!(
            (
                find("woocommerce_ppcp-settings").plugin.as_deref(),
                find("woocommerce_ppcp-settings").confidence
            ),
            (Some("woocommerce"), Some(Confidence::Exact))
        );
        assert_eq!(
            (
                find("acme_widgets_version").plugin.as_deref(),
                find("acme_widgets_version").confidence
            ),
            (Some("acme-widgets"), Some(Confidence::Likely))
        );
        assert_eq!(
            find("_ppcp_paypal_order_id").plugin.as_deref(),
            Some("woocommerce-paypal-payments")
        );
        assert_eq!(
            find("_transient_acme_cache").plugin.as_deref(),
            Some("acme-widgets")
        );
        assert_eq!(find("zzz_unknown").plugin, None);
    }

    #[test]
    fn report_sums_storage_marks_leftovers_and_counts_gateway_orders() {
        let active = vec!["woocommerce".to_string()];
        let attributed = attribute(
            vec![
                stat(KeyKind::Option, "woocommerce_ppcp-settings", 100),
                stat(
                    KeyKind::Meta("postmeta".into()),
                    "_ppcp_paypal_order_id",
                    300,
                ),
                stat(KeyKind::Option, "woocommerce_currency", 5),
                stat(KeyKind::Option, "nobody_knows", 7),
            ],
            &Lookup::default(),
            &active,
            &Assignments::new(),
        );
        let mut orders = HashMap::new();
        orders.insert("ppcp-gateway".to_string(), 12u64);
        let r = report(attributed, &Lookup::default(), &active, &orders);
        let ppcp = r
            .plugins
            .iter()
            .find(|p| p.slug == "woocommerce-paypal-payments")
            .unwrap();
        assert!(!ppcp.active, "not in active_plugins → leftover");
        assert_eq!(
            (ppcp.orders, ppcp.total_bytes, ppcp.meta[0].rows),
            (12, 300, 1)
        );
        let wc = r.plugins.iter().find(|p| p.slug == "woocommerce").unwrap();
        assert!(wc.active);
        assert_eq!((wc.option_count, wc.option_bytes), (2, 105));
        assert_eq!(r.unattributed.len(), 1);
        assert_eq!(r.unattributed[0].stat.key.name, "nobody_knows");
    }

    #[test]
    fn cleanup_deletes_options_and_meta_only() {
        let sql = cleanup_statements(
            "db",
            &ctx(),
            &[
                "option:woocommerce_ppcp-settings".into(),
                "transient:_transient_ppcp_x".into(),
                "meta:postmeta:_ppcp_paypal_order_id".into(),
                "meta:wc_orders_meta:_ppcp_paypal_order_id".into(),
                "table:ppcp_things".into(),
                "log:woocommerce-paypal-payments".into(),
                "garbage".into(),
            ],
        );
        assert_eq!(
            sql,
            [
                "DELETE FROM `db`.`wp_options` WHERE option_name IN ('woocommerce_ppcp-settings', '_transient_ppcp_x')",
                "DELETE FROM `db`.`wp_postmeta` WHERE meta_key IN ('_ppcp_paypal_order_id')",
                "DELETE FROM `db`.`wp_wc_orders_meta` WHERE meta_key IN ('_ppcp_paypal_order_id')",
            ]
        );
    }
}
