//! What the WordPress screens know about a database, read from its effective rules: the prefix,
//! the tables and the applied presets.

use std::sync::Arc;

use crate::rules::effective::EffectiveRules;

const WORDPRESS: &str = "wordpress";
const WOOCOMMERCE: &str = "woocommerce";

#[derive(Debug, Clone)]
pub struct WpContext {
    /// The site prefix (`wp_`, `wp_2_`); network tables may live under another one.
    pub prefix: String,
    pub rules: Arc<EffectiveRules>,
}

impl WpContext {
    /// `None` unless the `wordpress` preset applies.
    pub fn new(rules: Arc<EffectiveRules>) -> Option<Self> {
        if !rules.applied.presets.iter().any(|p| p.name == WORDPRESS) {
            return None;
        }
        Some(Self {
            prefix: rules.applied.prefix.clone(),
            rules,
        })
    }

    /// The real name of the WordPress table `name`, under the network prefix for network tables.
    pub fn table(&self, name: &str) -> String {
        let applied = &self.rules.applied;
        applied
            .existing_table(name)
            .unwrap_or_else(|| applied.table(name))
    }

    pub fn has_table(&self, name: &str) -> bool {
        self.rules.applied.existing_table(name).is_some()
    }

    pub fn woocommerce(&self) -> bool {
        self.rules
            .applied
            .presets
            .iter()
            .any(|p| p.name == WOOCOMMERCE)
    }

    /// Whether the order screens read `wc_orders` instead of `posts`. There is no HPOS
    /// detection: orders are read from `wc_orders` wherever the table exists.
    pub fn hpos(&self) -> bool {
        self.woocommerce() && self.has_table("wc_orders")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::project::Project;
    use crate::wp::testing::{project, rules, site_with, tables};

    #[test]
    fn there_is_no_context_unless_the_wordpress_preset_applies() {
        let incomplete = tables("wp_", &["options", "posts"]);
        let complete = tables("wp_", &["options", "posts", "users"]);

        let missing_markers = WpContext::new(rules(&Project::new("local", "wp"), &incomplete));
        let no_presets = WpContext::new(rules(&project("wp_", &[]), &complete));

        assert!(missing_markers.is_none());
        assert!(no_presets.is_none());
    }

    #[test]
    fn the_prefix_is_read_from_the_tables_the_wordpress_markers_exist_under() {
        let schema = tables("xy_", &["options", "posts", "users"]);

        let ctx = WpContext::new(rules(&Project::new("local", "wp"), &schema)).unwrap();

        assert_eq!(ctx.prefix, "xy_");
        assert_eq!(ctx.table("options"), "xy_options");
        assert!(ctx.has_table("posts"));
    }

    #[test]
    fn a_table_that_does_not_exist_resolves_to_its_would_be_name() {
        let (ctx, _) = site_with("wp_", &["options"], &["wordpress"]);

        assert_eq!(ctx.table("woocommerce_log"), "wp_woocommerce_log");
        assert!(!ctx.has_table("woocommerce_log"));
    }

    #[test]
    fn network_tables_resolve_under_the_network_prefix_and_site_tables_under_the_site_prefix() {
        let mut schema = tables("wp_", &["users", "usermeta", "blogs"]);
        let sub_site = tables("wp_2_", &["options", "posts", "postmeta"]);
        schema.tables.extend(sub_site.tables);
        schema.keys.extend(sub_site.keys);
        schema.columns.extend(sub_site.columns);
        let project = Project {
            network_prefix: Some("wp_".into()),
            ..project("wp_2_", &["wordpress"])
        };

        let ctx = WpContext::new(rules(&project, &schema)).unwrap();

        assert_eq!(ctx.prefix, "wp_2_");
        assert_eq!(ctx.table("posts"), "wp_2_posts");
        assert_eq!(ctx.table("users"), "wp_users");
        assert_eq!(ctx.table("blogs"), "wp_blogs");
        assert!(ctx.has_table("users"));
        assert!(ctx.has_table("posts"));
    }

    #[test]
    fn orders_come_from_wc_orders_only_where_woocommerce_applies_and_the_table_exists() {
        let wordpress = ["options", "posts", "users"];
        for (extra, presets, woocommerce, hpos) in [
            (
                vec!["woocommerce_order_items", "wc_orders"],
                vec!["wordpress", "woocommerce"],
                true,
                true,
            ),
            (
                vec!["woocommerce_order_items"],
                vec!["wordpress", "woocommerce"],
                true,
                false,
            ),
            (vec!["wc_orders"], vec!["wordpress"], false, false),
            (vec![], vec!["wordpress"], false, false),
        ] {
            let names: Vec<&str> = wordpress.iter().copied().chain(extra.clone()).collect();

            let (ctx, _) = site_with("wp_", &names, &presets);

            assert_eq!(ctx.woocommerce(), woocommerce, "{names:?} {presets:?}");
            assert_eq!(ctx.hpos(), hpos, "{names:?} {presets:?}");
        }
    }
}
