//! What kind of WordPress install a database holds. Probed once, then every resolver
//! works from this context instead of guessing.

use std::collections::{BTreeSet, HashMap};

use serde::Serialize;

use super::hints::DEFAULT_PRICE_DECIMALS;

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SiteInfo {
    pub blog_id: u64,
    pub prefix: String,
    pub domain: Option<String>,
    pub path: Option<String>,
}

/// Site-level facts read from `wp_options`.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SiteMeta {
    pub name: Option<String>,
    pub url: Option<String>,
    pub currency: Option<String>,
    /// From `gmt_offset` (hours); 0 when the site uses a named timezone.
    pub gmt_offset_secs: i64,
    pub theme: Option<String>,
    /// Plugin slugs from `active_plugins`, e.g. `woocommerce`.
    pub active_plugins: Vec<String>,
    /// Base URL for uploads, from `upload_url_path` or `<siteurl>/wp-content/uploads`.
    pub uploads_url: Option<String>,
    /// From `woocommerce_price_num_decimals`.
    pub price_decimals: u32,
}

impl Default for SiteMeta {
    fn default() -> Self {
        Self {
            name: None,
            url: None,
            currency: None,
            gmt_offset_secs: 0,
            theme: None,
            active_plugins: Vec::new(),
            uploads_url: None,
            price_decimals: DEFAULT_PRICE_DECIMALS,
        }
    }
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HposMode {
    Off,
    On,
    OnWithSync,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WcContext {
    pub version: Option<String>,
    pub hpos: HposMode,
    pub has_action_scheduler: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
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

/// The main WordPress prefix: the one with `options`, `posts` and `users` tables
/// (multisite sub-sites have no users table of their own).
pub fn main_prefix(tables: &[String]) -> Option<String> {
    let has = |t: &str| tables.iter().any(|x| x == t);
    let mut candidates: Vec<String> = tables
        .iter()
        .filter_map(|t| t.strip_suffix("options"))
        .filter(|p| has(&format!("{p}posts")) && has(&format!("{p}users")))
        .map(String::from)
        .collect();
    // Prefer the shortest prefix (`wp_` over `wp_2_` in odd setups).
    candidates.sort_by_key(String::len);
    candidates.into_iter().next()
}

/// Sub-site prefixes like `wp_2_` for main prefix `wp_`.
pub fn site_prefixes(tables: &[String], main: &str) -> Vec<(u64, String)> {
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
    "template",
    "stylesheet",
    "active_plugins",
    "upload_url_path",
];

fn site_meta(options: &HashMap<String, String>) -> SiteMeta {
    let opt = |k: &str| options.get(k).filter(|v| !v.is_empty()).cloned();
    let url = opt("siteurl").map(|u| u.trim_end_matches('/').to_string());
    let active_plugins = opt("active_plugins")
        .and_then(|raw| super::php::parse(raw.as_bytes()).ok())
        .map(|v| match v {
            super::php::PhpValue::Array(items) => items
                .into_iter()
                .filter_map(|(_, v)| match v {
                    super::php::PhpValue::Str(s) => String::from_utf8(s).ok(),
                    _ => None,
                })
                .map(|path| {
                    path.split('/')
                        .next()
                        .unwrap_or(&path)
                        .trim_end_matches(".php")
                        .to_string()
                })
                .collect(),
            _ => Vec::new(),
        })
        .unwrap_or_default();
    SiteMeta {
        name: opt("blogname"),
        uploads_url: opt("upload_url_path")
            .map(|u| u.trim_end_matches('/').to_string())
            .or_else(|| url.as_ref().map(|u| format!("{u}/wp-content/uploads"))),
        url,
        currency: opt("woocommerce_currency"),
        gmt_offset_secs: opt("gmt_offset")
            .and_then(|o| o.parse::<f64>().ok())
            .map_or(0, |h| (h * 3600.0) as i64),
        theme: opt("stylesheet").or_else(|| opt("template")),
        active_plugins,
        price_decimals: opt("woocommerce_price_num_decimals")
            .and_then(|d| d.trim().parse().ok())
            .unwrap_or(DEFAULT_PRICE_DECIMALS),
    }
}

/// Builds the context from the table list and the `PROBED_OPTIONS` values.
pub fn build(
    tables: &[String],
    options: &HashMap<String, String>,
    blogs: &HashMap<u64, (String, String)>,
) -> Option<WpContext> {
    let prefix = main_prefix(tables)?;
    let has = |t: &str| tables.iter().any(|x| *x == format!("{prefix}{t}"));
    let opt = |k: &str| options.get(k).cloned();

    let wc = (opt("woocommerce_version").is_some()
        || has("wc_orders")
        || has("woocommerce_order_items"))
    .then(|| {
        let enabled = opt("woocommerce_custom_orders_table_enabled").as_deref() == Some("yes")
            && has("wc_orders");
        let sync =
            opt("woocommerce_custom_orders_table_data_sync_enabled").as_deref() == Some("yes");
        WcContext {
            version: opt("woocommerce_version"),
            hpos: match (enabled, sync) {
                (false, _) => HposMode::Off,
                (true, false) => HposMode::On,
                (true, true) => HposMode::OnWithSync,
            },
            has_action_scheduler: has("actionscheduler_actions"),
        }
    });

    let sites = site_prefixes(tables, &prefix)
        .into_iter()
        .map(|(blog_id, site_prefix)| {
            let (domain, path) = blogs.get(&blog_id).cloned().unzip();
            SiteInfo {
                blog_id,
                prefix: site_prefix,
                domain,
                path,
            }
        })
        .collect();

    Some(WpContext {
        version: opt("db_version"),
        prefix,
        sites,
        wc,
        site: site_meta(options),
        tables: tables.iter().cloned().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
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
            build(
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
            build(&t, &opts(&[]), &HashMap::new()).unwrap().prefix,
            "shop_"
        );
    }

    #[test]
    fn plain_wordpress_has_no_woocommerce() {
        let ctx = build(
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
            build(&t, &opts(o), &HashMap::new())
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
        let ctx = build(
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
        let ctx = build(
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
        assert_eq!(
            ctx.site.uploads_url.as_deref(),
            Some("https://wc-pp.ddev.site/wp-content/uploads")
        );
        assert_eq!(ctx.site.gmt_offset_secs, -19800);
        assert_eq!(
            ctx.site.active_plugins,
            ["woocommerce", "woocommerce-paypal-payments"]
        );
        assert_eq!(ctx.site.theme.as_deref(), Some("storefront"));
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
            let ctx = build(&tables(CORE), &opts(&pairs), &HashMap::new()).unwrap();
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
        let ctx = build(&t, &opts(&[]), &blogs).unwrap();
        assert_eq!(ctx.prefix, "wp_");
        let ids: Vec<_> = ctx
            .sites
            .iter()
            .map(|s| (s.blog_id, s.prefix.as_str()))
            .collect();
        assert_eq!(ids, [(2, "wp_2_"), (10, "wp_10_")]);
        assert_eq!(ctx.sites[0].domain.as_deref(), Some("shop.test"));
    }
}
