//! Plugin registry (`plugins.toml`): which plugin owns an option prefix, meta key, hook,
//! log source or table. Used for human names in charts now and by the plugin lens later.

use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct Plugin {
    pub slug: String,
    pub name: String,
    pub option_prefixes: Vec<String>,
    pub meta_prefixes: Vec<String>,
    pub hook_prefixes: Vec<String>,
    pub log_sources: Vec<String>,
    /// Table name prefixes after the WordPress prefix, e.g. `wc_`; exact names otherwise.
    pub tables: Vec<String>,
}

#[derive(Deserialize)]
struct Registry {
    plugin: Vec<Plugin>,
}

pub fn registry() -> &'static [Plugin] {
    static REGISTRY: OnceLock<Vec<Plugin>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        toml::from_str::<Registry>(include_str!("plugins.toml"))
            .expect("built-in plugins.toml is valid")
            .plugin
    })
}

pub fn by_slug(slug: &str) -> Option<&'static Plugin> {
    registry().iter().find(|p| p.slug == slug)
}

/// The owner of an option-name prefix as produced by `options::option_prefix`.
pub fn for_option_prefix(prefix: &str) -> Option<&'static Plugin> {
    registry()
        .iter()
        .find(|p| p.option_prefixes.iter().any(|x| x == prefix))
}

/// Human label for an option prefix: the plugin name, or the raw prefix.
pub fn option_prefix_label(prefix: &str) -> String {
    match prefix {
        "(single)" => "Unprefixed".to_string(),
        p => for_option_prefix(p)
            .map(|pl| pl.name.clone())
            .unwrap_or_else(|| p.to_string()),
    }
}

/// The owner of a table (name without the WordPress prefix).
pub fn for_table(name: &str) -> Option<&'static Plugin> {
    registry().iter().find(|p| {
        p.tables.iter().any(|t| {
            if t.ends_with('_') {
                name.starts_with(t.as_str())
            } else {
                name == t
            }
        })
    })
}

pub fn for_hook(hook: &str) -> Option<&'static Plugin> {
    let mut best: Option<(&Plugin, usize)> = None;
    for p in registry() {
        for prefix in &p.hook_prefixes {
            if (hook == prefix || hook.starts_with(&format!("{prefix}_")))
                && best.is_none_or(|(_, len)| prefix.len() > len)
            {
                best = Some((p, prefix.len()));
            }
        }
    }
    best.map(|(p, _)| p)
}

pub fn for_log_source(source: &str) -> Option<&'static Plugin> {
    registry()
        .iter()
        .find(|p| p.log_sources.iter().any(|s| s == source))
}

/// Meta keys: leading underscores are ignored, then the first `_`/`-` token is matched.
pub fn for_meta_key(key: &str) -> Option<&'static Plugin> {
    let trimmed = key.trim_start_matches('_');
    let mut best: Option<(&Plugin, usize)> = None;
    for p in registry() {
        for prefix in &p.meta_prefixes {
            let prefix = prefix.trim_start_matches('_');
            if (trimmed == prefix
                || trimmed.starts_with(&format!("{prefix}_"))
                || trimmed.starts_with(&format!("{prefix}-")))
                && best.is_none_or(|(_, len)| prefix.len() > len)
            {
                best = Some((p, prefix.len()));
            }
        }
    }
    best.map(|(p, _)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_prefixes_get_human_names_with_raw_fallback() {
        assert_eq!(option_prefix_label("ppcp"), "PayPal Payments");
        assert_eq!(option_prefix_label("woocommerce"), "WooCommerce");
        assert_eq!(option_prefix_label("rewrite"), "Rewrite rules · core");
        assert_eq!(option_prefix_label("wp"), "WordPress core");
        assert_eq!(option_prefix_label("(single)"), "Unprefixed");
        assert_eq!(option_prefix_label("transients"), "Transients");
        assert_eq!(option_prefix_label("someplugin"), "someplugin");
    }

    #[test]
    fn tables_hooks_logs_and_meta_resolve_to_plugins() {
        assert_eq!(for_table("wc_orders_meta").unwrap().slug, "woocommerce");
        assert_eq!(
            for_table("actionscheduler_logs").unwrap().slug,
            "action-scheduler"
        );
        assert_eq!(for_table("posts").unwrap().slug, "core");
        assert!(for_table("acme_widgets").is_none());
        assert_eq!(
            for_hook("woocommerce_paypal_payments_check_pui_payment_captured")
                .unwrap()
                .slug,
            "woocommerce-paypal-payments"
        );
        assert_eq!(
            for_hook("woocommerce_cleanup_draft_orders").unwrap().slug,
            "woocommerce"
        );
        assert_eq!(
            for_log_source("woocommerce-paypal-payments").unwrap().slug,
            "woocommerce-paypal-payments"
        );
        assert_eq!(
            for_meta_key("_ppcp_paypal_order_id").unwrap().slug,
            "woocommerce-paypal-payments"
        );
        assert_eq!(for_meta_key("_billing_email").unwrap().slug, "woocommerce");
        assert!(for_meta_key("_acme_x").is_none());
    }
}
