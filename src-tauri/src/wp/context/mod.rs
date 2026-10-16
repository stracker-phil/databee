//! What kind of WordPress install a database holds. Probed once (`probe`), then built into a
//! `WpContext` without I/O (`build`); every resolver works from it instead of guessing.

mod probe;
mod timezone;

use std::collections::{BTreeSet, HashMap};

use serde::Serialize;
use ts_rs::TS;

pub use self::probe::Probe;
pub use self::timezone::SiteZone;
use super::hints::DEFAULT_PRICE_DECIMALS;
use crate::db::Database;
use crate::decode::php::{self, PhpKey, PhpValue};
use crate::error::AppResult;

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct SiteInfo {
    pub blog_id: u64,
    pub prefix: String,
    pub domain: Option<String>,
    pub path: Option<String>,
}

/// Plugins listed in one option.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct PluginList {
    /// The option the list was read from: `active_plugins` (in `options`) or
    /// `active_sitewide_plugins` (in `sitemeta`).
    #[ts(type = "\"active_plugins\" | \"active_sitewide_plugins\"")]
    pub source: &'static str,
    /// Plugin slugs (the directory or file name of each stored plugin path), e.g. `woocommerce`.
    pub plugins: Vec<String>,
}

impl PluginList {
    fn empty(source: &'static str) -> Self {
        Self {
            source,
            plugins: Vec::new(),
        }
    }
}

/// The uploads base URL as stored, with the option it came from.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct UploadsUrl {
    pub url: String,
    #[ts(type = "\"upload_url_path\"")]
    pub source: &'static str,
}

/// Site-level facts read from `wp_options` (and `wp_sitemeta` on multisite).
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct SiteMeta {
    pub name: Option<String>,
    pub url: Option<String>,
    pub currency: Option<String>,
    #[ts(type = "string")]
    pub zone: SiteZone,
    /// The option `zone` was read from: `timezone_string` or `gmt_offset`.
    #[ts(type = "\"timezone_string\" | \"gmt_offset\"")]
    pub zone_source: &'static str,
    pub theme: Option<String>,
    /// From `active_plugins`.
    pub active_plugins: PluginList,
    /// Network-activated plugins from `active_sitewide_plugins`, when the network stores it.
    pub network_plugins: Option<PluginList>,
    /// From `upload_url_path` when set. WordPress derives the default from `WP_CONTENT_URL`
    /// and `upload_path`, which the database does not prove, so it is not guessed.
    pub uploads_url: Option<UploadsUrl>,
    /// From `woocommerce_price_num_decimals`.
    pub price_decimals: u32,
}

impl Default for SiteMeta {
    fn default() -> Self {
        Self {
            name: None,
            url: None,
            currency: None,
            zone: SiteZone::default(),
            zone_source: "gmt_offset",
            theme: None,
            active_plugins: PluginList::empty("active_plugins"),
            network_plugins: None,
            uploads_url: None,
            price_decimals: DEFAULT_PRICE_DECIMALS,
        }
    }
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "snake_case")]
pub enum HposMode {
    Off,
    On,
    OnWithSync,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct WcContext {
    pub version: Option<String>,
    pub hpos: HposMode,
    pub has_action_scheduler: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct WpContext {
    pub prefix: String,
    pub version: Option<String>,
    /// Sub-sites of a multisite network (blog 1 uses the main prefix).
    pub sites: Vec<SiteInfo>,
    pub wc: Option<WcContext>,
    pub site: SiteMeta,
    /// All tables in the database, so definitions for missing plugin tables can be skipped.
    #[serde(skip)]
    pub tables: BTreeSet<String>,
}

impl WpContext {
    /// Whether `{prefix}{name}` exists.
    pub fn has_table(&self, name: &str) -> bool {
        self.tables.contains(&self.table(name))
    }

    pub fn table(&self, name: &str) -> String {
        format!("{}{name}", self.prefix)
    }

    pub fn hpos(&self) -> bool {
        self.wc.as_ref().is_some_and(|wc| wc.hpos != HposMode::Off)
    }
}

/// Probes a database; `None` when it isn't a WordPress install.
pub async fn detect(db: &dyn Database, database: &str) -> AppResult<Option<WpContext>> {
    Ok(probe::probe(db, database).await?.map(build))
}

/// The main WordPress prefix: one with `options`, `posts` and `users` tables (multisite
/// sub-sites have no users table of their own). The shortest wins (`wp_` over `wp_2_` in odd
/// setups); among equally long ones the alphabetically first, so the choice is stable.
pub fn main_prefix(tables: &BTreeSet<String>) -> Option<String> {
    let has = |prefix: &str, name: &str| tables.contains(format!("{prefix}{name}").as_str());
    tables
        .iter()
        .filter_map(|t| t.strip_suffix("options"))
        .filter(|prefix| has(prefix, "posts") && has(prefix, "users"))
        .min_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)))
        .map(String::from)
}

/// Sub-site prefixes like `wp_2_` for main prefix `wp_`: one per `{main}{N}_options` table.
fn site_prefixes(tables: &BTreeSet<String>, main: &str) -> Vec<(u64, String)> {
    let mut sites: Vec<(u64, String)> = tables
        .iter()
        .filter_map(|t| t.strip_prefix(main)?.strip_suffix("_options"))
        .filter_map(|id| id.parse::<u64>().ok().map(|n| (n, format!("{main}{n}_"))))
        .collect();
    sites.sort();
    sites
}

pub const PROBED_OPTIONS: &[&str] = &[
    "db_version",
    "woocommerce_version",
    "woocommerce_custom_orders_table_enabled",
    "woocommerce_custom_orders_table_data_sync_enabled",
    "siteurl",
    "blogname",
    "woocommerce_currency",
    "woocommerce_price_num_decimals",
    "gmt_offset",
    "timezone_string",
    "template",
    "stylesheet",
    "active_plugins",
    "upload_url_path",
];

/// Read from `{prefix}sitemeta` on multisite.
pub const PROBED_NETWORK_OPTIONS: &[&str] = &["active_sitewide_plugins"];

/// A stored, non-NULL value.
fn stored<'a>(options: &'a HashMap<String, Option<String>>, name: &str) -> Option<&'a str> {
    options.get(name)?.as_deref()
}

/// A stored value that is not empty.
fn non_empty(options: &HashMap<String, Option<String>>, name: &str) -> Option<String> {
    stored(options, name)
        .filter(|v| !v.is_empty())
        .map(String::from)
}

/// The items of a serialized PHP array; anything else (unserialized, broken) has none.
fn serialized_array(raw: Option<&str>) -> Vec<(PhpKey, PhpValue)> {
    match raw.and_then(|raw| php::parse(raw.as_bytes()).ok()) {
        Some(PhpValue::Array(items)) => items,
        _ => Vec::new(),
    }
}

/// `woocommerce/woocommerce.php` → `woocommerce`, `hello.php` → `hello`.
fn plugin_slug(path: &[u8]) -> Option<String> {
    let path = std::str::from_utf8(path).ok()?;
    let first = path.split('/').next().unwrap_or(path);
    Some(first.trim_end_matches(".php").to_string())
}

/// `active_plugins` stores the plugin paths as array values.
fn active_plugins(raw: Option<&str>) -> PluginList {
    let plugins = serialized_array(raw)
        .into_iter()
        .filter_map(|(_, value)| match value {
            PhpValue::Str(path) => plugin_slug(&path),
            _ => None,
        })
        .collect();
    PluginList {
        source: "active_plugins",
        plugins,
    }
}

/// `active_sitewide_plugins` stores the plugin paths as array keys (values are activation times).
fn network_plugins(raw: &str) -> PluginList {
    let plugins = serialized_array(Some(raw))
        .into_iter()
        .filter_map(|(key, _)| match key {
            PhpKey::Str(path) => plugin_slug(&path),
            PhpKey::Int(_) => None,
        })
        .collect();
    PluginList {
        source: "active_sitewide_plugins",
        plugins,
    }
}

fn site_meta(
    options: &HashMap<String, Option<String>>,
    network_options: &HashMap<String, Option<String>>,
) -> SiteMeta {
    let opt = |name: &str| non_empty(options, name);
    let (zone, zone_source) = timezone::site_zone(
        stored(options, "timezone_string"),
        stored(options, "gmt_offset"),
    );
    SiteMeta {
        name: opt("blogname"),
        url: opt("siteurl").map(|u| u.trim_end_matches('/').to_string()),
        currency: opt("woocommerce_currency"),
        zone,
        zone_source,
        theme: opt("stylesheet").or_else(|| opt("template")),
        active_plugins: active_plugins(stored(options, "active_plugins")),
        network_plugins: stored(network_options, "active_sitewide_plugins").map(network_plugins),
        uploads_url: opt("upload_url_path").map(|url| UploadsUrl {
            url: url.trim_end_matches('/').to_string(),
            source: "upload_url_path",
        }),
        price_decimals: opt("woocommerce_price_num_decimals")
            .and_then(|d| d.trim().parse().ok())
            .unwrap_or(DEFAULT_PRICE_DECIMALS),
    }
}

/// `own_tables` are the table names without the main prefix.
fn woocommerce(
    options: &HashMap<String, Option<String>>,
    own_tables: &BTreeSet<&str>,
) -> Option<WcContext> {
    let has = |name: &str| own_tables.contains(name);
    let version = non_empty(options, "woocommerce_version");
    if version.is_none() && !has("wc_orders") && !has("woocommerce_order_items") {
        return None;
    }
    let yes = |name: &str| stored(options, name) == Some("yes");
    let enabled = yes("woocommerce_custom_orders_table_enabled") && has("wc_orders");
    let hpos = match (
        enabled,
        yes("woocommerce_custom_orders_table_data_sync_enabled"),
    ) {
        (false, _) => HposMode::Off,
        (true, false) => HposMode::On,
        (true, true) => HposMode::OnWithSync,
    };
    Some(WcContext {
        version,
        hpos,
        has_action_scheduler: has("actionscheduler_actions"),
    })
}

/// Builds the context from what `probe` read.
fn build(probe: Probe) -> WpContext {
    let prefix = probe.prefix;
    let own_tables: BTreeSet<&str> = probe
        .tables
        .iter()
        .filter_map(|t| t.strip_prefix(prefix.as_str()))
        .collect();
    let wc = woocommerce(&probe.options, &own_tables);

    let sites = site_prefixes(&probe.tables, &prefix)
        .into_iter()
        .map(|(blog_id, site_prefix)| {
            let (domain, path) = probe.blogs.get(&blog_id).cloned().unzip();
            SiteInfo {
                blog_id,
                prefix: site_prefix,
                domain,
                path,
            }
        })
        .collect();

    WpContext {
        version: non_empty(&probe.options, "db_version"),
        site: site_meta(&probe.options, &probe.network_options),
        prefix,
        sites,
        wc,
        tables: probe.tables,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, FixedOffset, Utc};

    use super::*;

    fn tables(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    /// The probe `probe::probe` would read for these tables, options and blogs.
    fn build_from(
        tables: &[String],
        options: &HashMap<String, String>,
        blogs: &HashMap<u64, (String, String)>,
    ) -> Option<WpContext> {
        let tables: BTreeSet<String> = tables.iter().cloned().collect();
        Some(build(Probe {
            prefix: main_prefix(&tables)?,
            tables,
            options: options
                .iter()
                .map(|(k, v)| (k.clone(), Some(v.clone())))
                .collect(),
            network_options: HashMap::new(),
            blogs: blogs.clone(),
        }))
    }

    fn opts(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    const CORE: &[&str] = &[
        "wp_options",
        "wp_posts",
        "wp_postmeta",
        "wp_users",
        "wp_usermeta",
    ];

    #[test]
    fn non_wordpress_databases_have_no_context() {
        assert_eq!(
            build_from(
                &tables(&["orders", "customers"]),
                &opts(&[]),
                &HashMap::new()
            ),
            None
        );
    }

    #[test]
    fn detects_custom_prefix() {
        let t = tables(&["shop_options", "shop_posts", "shop_users", "other"]);
        assert_eq!(
            build_from(&t, &opts(&[]), &HashMap::new()).unwrap().prefix,
            "shop_"
        );
    }

    #[test]
    fn plain_wordpress_has_no_woocommerce() {
        let ctx = build_from(
            &tables(CORE),
            &opts(&[("db_version", "58975")]),
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(ctx.wc, None);
        assert_eq!(ctx.version.as_deref(), Some("58975"));
    }

    #[test]
    fn hpos_mode_follows_options() {
        let mut t = tables(CORE);
        t.extend(tables(&["wp_wc_orders", "wp_actionscheduler_actions"]));
        let mode = |o: &[(&str, &str)]| {
            build_from(&t, &opts(o), &HashMap::new())
                .unwrap()
                .wc
                .unwrap()
                .hpos
        };

        assert_eq!(mode(&[("woocommerce_version", "9.3.0")]), HposMode::Off);
        assert_eq!(
            mode(&[
                ("woocommerce_version", "9.3.0"),
                ("woocommerce_custom_orders_table_enabled", "yes")
            ]),
            HposMode::On
        );
        assert_eq!(
            mode(&[
                ("woocommerce_version", "9.3.0"),
                ("woocommerce_custom_orders_table_enabled", "yes"),
                ("woocommerce_custom_orders_table_data_sync_enabled", "yes"),
            ]),
            HposMode::OnWithSync
        );
    }

    #[test]
    fn hpos_needs_the_orders_table_even_if_the_option_says_yes() {
        let ctx = build_from(
            &tables(CORE),
            &opts(&[
                ("woocommerce_version", "9"),
                ("woocommerce_custom_orders_table_enabled", "yes"),
            ]),
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(ctx.wc.unwrap().hpos, HposMode::Off);
    }

    #[test]
    fn site_facts_come_from_options() {
        let ctx = build_from(
            &tables(CORE),
            &opts(&[
                ("siteurl", "https://wc-pp.ddev.site/"),
                ("woocommerce_currency", "USD"),
                ("gmt_offset", "-5.5"),
                ("stylesheet", "storefront"),
                (
                    "active_plugins",
                    r#"a:2:{i:0;s:27:"woocommerce/woocommerce.php";i:1;s:59:"woocommerce-paypal-payments/woocommerce-paypal-payments.php";}"#,
                ),
            ]),
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(ctx.site.url.as_deref(), Some("https://wc-pp.ddev.site"));
        assert_eq!(ctx.site.uploads_url, None);
        assert_eq!(
            ctx.site.zone,
            SiteZone::Fixed(FixedOffset::west_opt(19800).unwrap())
        );
        assert_eq!(
            ctx.site.active_plugins.plugins,
            ["woocommerce", "woocommerce-paypal-payments"]
        );
        assert_eq!(ctx.site.theme.as_deref(), Some("storefront"));
    }

    fn site_of(pairs: &[(&str, &str)]) -> SiteMeta {
        build_from(&tables(CORE), &opts(pairs), &HashMap::new())
            .unwrap()
            .site
    }

    fn utc(rfc3339: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(rfc3339)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn timezone_string_gives_a_named_zone_with_dst() {
        let site = site_of(&[("timezone_string", "Europe/Berlin"), ("gmt_offset", "9")]);
        assert_eq!(site.zone_source, "timezone_string");
        assert_eq!(
            site.zone.offset_at(utc("2026-01-15T12:00:00Z")),
            FixedOffset::east_opt(3600).unwrap()
        );
        assert_eq!(
            site.zone.offset_at(utc("2026-07-15T12:00:00Z")),
            FixedOffset::east_opt(7200).unwrap()
        );
        assert_eq!(
            serde_json::to_value(site.zone).unwrap(),
            serde_json::json!("Europe/Berlin")
        );
    }

    #[test]
    fn gmt_offset_is_the_fallback_zone() {
        let fractional = site_of(&[("timezone_string", ""), ("gmt_offset", "5.5")]);
        assert_eq!(
            fractional.zone,
            SiteZone::Fixed(FixedOffset::east_opt(19800).unwrap())
        );
        assert_eq!(fractional.zone_source, "gmt_offset");
        assert_eq!(
            serde_json::to_value(fractional.zone).unwrap(),
            serde_json::json!("+05:30")
        );

        let invalid_name = site_of(&[("timezone_string", "Mars/Olympus"), ("gmt_offset", "-2")]);
        assert_eq!(
            invalid_name.zone,
            SiteZone::Fixed(FixedOffset::west_opt(7200).unwrap())
        );
        assert_eq!(invalid_name.zone_source, "gmt_offset");
    }

    #[test]
    fn without_zone_options_the_site_is_on_utc() {
        let site = site_of(&[]);
        assert_eq!(
            site.zone.offset_at(utc("2026-07-15T12:00:00Z")),
            FixedOffset::east_opt(0).unwrap()
        );
        assert_eq!(site.zone_source, "gmt_offset");
    }

    #[test]
    fn price_decimals_come_from_the_option_and_default_to_two() {
        for (option, expected) in [
            (Some("3"), 3),
            (Some("0"), 0),
            (Some("abc"), 2),
            (Some(""), 2),
            (None, 2),
        ] {
            let pairs: Vec<(&str, &str)> = option
                .map(|v| ("woocommerce_price_num_decimals", v))
                .into_iter()
                .collect();
            let ctx = build_from(&tables(CORE), &opts(&pairs), &HashMap::new()).unwrap();
            assert_eq!(ctx.site.price_decimals, expected, "{option:?}");
        }
    }

    #[test]
    fn multisite_sub_sites_are_listed_with_their_domains() {
        let mut t = tables(CORE);
        t.extend(tables(&[
            "wp_blogs",
            "wp_2_options",
            "wp_2_posts",
            "wp_10_options",
        ]));
        let blogs = HashMap::from([(2, ("shop.test".to_string(), "/".to_string()))]);
        let ctx = build_from(&t, &opts(&[]), &blogs).unwrap();
        assert_eq!(ctx.prefix, "wp_");
        let ids: Vec<_> = ctx
            .sites
            .iter()
            .map(|s| (s.blog_id, s.prefix.as_str()))
            .collect();
        assert_eq!(ids, [(2, "wp_2_"), (10, "wp_10_")]);
        assert_eq!(ctx.sites[0].domain.as_deref(), Some("shop.test"));
    }

    type Pairs<'a> = &'a [(&'a str, Option<&'a str>)];

    fn stored_values(pairs: Pairs) -> HashMap<String, Option<String>> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.map(String::from)))
            .collect()
    }

    fn probe_of(table_names: &[&str], options: Pairs, network_options: Pairs) -> Probe {
        let tables: BTreeSet<String> = table_names.iter().map(|t| t.to_string()).collect();
        Probe {
            prefix: main_prefix(&tables).unwrap(),
            tables,
            options: stored_values(options),
            network_options: stored_values(network_options),
            blogs: HashMap::new(),
        }
    }

    fn context_of(options: Pairs, network_options: Pairs) -> WpContext {
        build(probe_of(CORE, options, network_options))
    }

    const SITEWIDE: &str =
        r#"a:2:{s:27:"woocommerce/woocommerce.php";i:1700000000;s:9:"hello.php";i:1;}"#;

    #[test]
    fn hpos_off_option_wins_over_enabled_sync() {
        let mut names = CORE.to_vec();
        names.push("wp_wc_orders");
        let ctx = build(probe_of(
            &names,
            &[
                ("woocommerce_version", Some("9.3.0")),
                ("woocommerce_custom_orders_table_enabled", Some("no")),
                (
                    "woocommerce_custom_orders_table_data_sync_enabled",
                    Some("yes"),
                ),
            ],
            &[],
        ));
        assert_eq!(ctx.wc.as_ref().unwrap().hpos, HposMode::Off);
        assert!(!ctx.hpos());
    }

    #[test]
    fn empty_gmt_offset_leaves_the_site_on_utc() {
        let site = context_of(
            &[("gmt_offset", Some("")), ("timezone_string", Some(""))],
            &[],
        )
        .site;
        assert_eq!(site.zone, SiteZone::default());
        assert_eq!(site.zone_source, "gmt_offset");
    }

    #[test]
    fn options_without_a_value_count_as_unset() {
        let ctx = context_of(
            &[
                ("siteurl", None),
                ("blogname", None),
                ("db_version", None),
                ("gmt_offset", None),
                ("active_plugins", None),
                ("upload_url_path", None),
                ("stylesheet", None),
                ("template", Some("twentytwentyfive")),
            ],
            &[("active_sitewide_plugins", None)],
        );
        assert_eq!(ctx.version, None);
        assert_eq!(ctx.site.url, None);
        assert_eq!(ctx.site.name, None);
        assert_eq!(ctx.site.zone, SiteZone::default());
        assert_eq!(ctx.site.active_plugins.plugins, Vec::<String>::new());
        assert_eq!(ctx.site.uploads_url, None);
        assert_eq!(ctx.site.network_plugins, None);
        assert_eq!(ctx.site.theme.as_deref(), Some("twentytwentyfive"));
    }

    #[test]
    fn active_plugins_that_are_not_serialized_list_nothing() {
        for raw in ["", "woocommerce/woocommerce.php", "a:1:{i:0;s:5:\"broken"] {
            let list = context_of(&[("active_plugins", Some(raw))], &[])
                .site
                .active_plugins;
            assert_eq!(
                list,
                PluginList {
                    source: "active_plugins",
                    plugins: Vec::new()
                },
                "{raw:?}"
            );
        }
    }

    #[test]
    fn upload_url_path_is_the_uploads_url_and_names_its_source() {
        for (stored, expected) in [
            ("https://cdn.test/uploads", "https://cdn.test/uploads"),
            ("https://cdn.test/uploads/", "https://cdn.test/uploads"),
        ] {
            assert_eq!(
                context_of(&[("upload_url_path", Some(stored))], &[])
                    .site
                    .uploads_url,
                Some(UploadsUrl {
                    url: expected.to_string(),
                    source: "upload_url_path"
                }),
                "{stored}"
            );
        }
    }

    #[test]
    fn uploads_url_is_not_guessed_from_the_site_url() {
        for options in [
            vec![("siteurl", Some("https://shop.test"))],
            vec![
                ("siteurl", Some("https://shop.test")),
                ("upload_url_path", Some("")),
            ],
        ] {
            assert_eq!(
                context_of(&options, &[]).site.uploads_url,
                None,
                "{options:?}"
            );
        }
    }

    #[test]
    fn equally_long_prefixes_pick_the_alphabetically_first() {
        let ctx = build(probe_of(
            &[
                "zz_options",
                "zz_posts",
                "zz_users",
                "aa_options",
                "aa_posts",
                "aa_users",
            ],
            &[],
            &[],
        ));
        assert_eq!(ctx.prefix, "aa_");
    }

    #[test]
    fn a_sub_site_prefix_without_an_options_table_is_not_a_site() {
        let mut names = CORE.to_vec();
        names.extend(["wp_2_options", "wp_3_posts", "wp_3_postmeta"]);
        let ctx = build(probe_of(&names, &[], &[]));
        let ids: Vec<_> = ctx.sites.iter().map(|s| s.blog_id).collect();
        assert_eq!(ids, [2]);
    }

    #[test]
    fn order_items_table_alone_marks_woocommerce() {
        let mut names = CORE.to_vec();
        names.push("wp_woocommerce_order_items");
        let wc = build(probe_of(&names, &[], &[])).wc;
        assert_eq!(
            wc,
            Some(WcContext {
                version: None,
                hpos: HposMode::Off,
                has_action_scheduler: false,
            })
        );
    }

    #[test]
    fn network_plugins_come_from_the_keys_of_active_sitewide_plugins() {
        let site = context_of(
            &[("active_plugins", Some(r#"a:1:{i:0;s:9:"local.php";}"#))],
            &[("active_sitewide_plugins", Some(SITEWIDE))],
        )
        .site;
        assert_eq!(
            site.network_plugins,
            Some(PluginList {
                source: "active_sitewide_plugins",
                plugins: vec!["woocommerce".to_string(), "hello".to_string()],
            })
        );
        assert_eq!(site.active_plugins.plugins, ["local"]);
    }

    #[test]
    fn without_a_network_option_there_are_no_network_plugins() {
        assert_eq!(context_of(&[], &[]).site.network_plugins, None);
    }

    #[test]
    fn plugin_lists_serialize_with_their_source() {
        let site = context_of(&[], &[("active_sitewide_plugins", Some(SITEWIDE))]).site;
        let json = serde_json::to_value(&site).unwrap();
        assert_eq!(
            json["networkPlugins"],
            serde_json::json!({
                "source": "active_sitewide_plugins",
                "plugins": ["woocommerce", "hello"],
            })
        );
        assert_eq!(
            json["activePlugins"],
            serde_json::json!({ "source": "active_plugins", "plugins": [] })
        );
        assert_eq!(json["uploadsUrl"], serde_json::Value::Null);
    }
}
