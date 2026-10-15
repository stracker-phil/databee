//! What kind of WordPress install a database holds. Probed once, then every resolver
//! works from this context instead of guessing.

use std::collections::{BTreeSet, HashMap};

use serde::Serialize;

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SiteInfo {
    pub blog_id: u64,
    pub prefix: String,
    pub domain: Option<String>,
    pub path: Option<String>,
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
];

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
