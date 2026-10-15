//! Plugin registry (`plugins.toml`): which plugin owns an option prefix, meta key, hook,
//! log source, table or payment gateway. Users can extend it with `<config dir>/plugins.toml`
//! in the same format; user entries take precedence.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
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
    /// Payment gateway ids (`_payment_method`) the plugin provides.
    pub gateways: Vec<String>,
}

#[derive(Deserialize)]
struct Registry {
    #[serde(default)]
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

/// Built-in registry plus the user's `plugins.toml`; user plugins come first so they win.
pub fn merged(user_toml: Option<&str>) -> Result<Vec<Plugin>, String> {
    let mut out = Vec::new();
    if let Some(text) = user_toml {
        let user: Registry = toml::from_str(text).map_err(|e| e.to_string())?;
        out.extend(user.plugin);
    }
    out.extend(registry().iter().cloned());
    Ok(out)
}

/// Resolution over a plugin list (built-in by default).
pub struct Lookup<'a>(pub &'a [Plugin]);

impl Default for Lookup<'static> {
    fn default() -> Self {
        Lookup(registry())
    }
}

impl<'a> Lookup<'a> {
    pub fn by_slug(&self, slug: &str) -> Option<&'a Plugin> {
        self.0.iter().find(|p| p.slug == slug)
    }

    /// The owner of an option-name prefix as produced by `options::option_prefix`.
    pub fn for_option_prefix(&self, prefix: &str) -> Option<&'a Plugin> {
        self.0
            .iter()
            .find(|p| p.option_prefixes.iter().any(|x| x == prefix))
    }

    /// Human label for an option prefix: the plugin name, or the raw prefix.
    pub fn option_prefix_label(&self, prefix: &str) -> String {
        match prefix {
            "(single)" => "Unprefixed".to_string(),
            p => self
                .for_option_prefix(p)
                .map(|pl| pl.name.clone())
                .unwrap_or_else(|| p.to_string()),
        }
    }

    /// The owner of a table (name without the WordPress prefix).
    pub fn for_table(&self, name: &str) -> Option<&'a Plugin> {
        self.0.iter().find(|p| {
            p.tables.iter().any(|t| {
                if t.ends_with('_') {
                    name.starts_with(t.as_str())
                } else {
                    name == t
                }
            })
        })
    }

    fn longest_prefix<'b>(
        &self,
        name: &str,
        pick: impl Fn(&'a Plugin) -> &'a [String],
        sep: &[char],
    ) -> Option<&'a Plugin>
    where
        'a: 'b,
    {
        let mut best: Option<(&Plugin, usize)> = None;
        for p in self.0 {
            for prefix in pick(p) {
                let prefix = prefix.trim_start_matches('_');
                let matches = name == prefix
                    || sep
                        .iter()
                        .any(|s| name.starts_with(&format!("{prefix}{s}")));
                if matches && best.is_none_or(|(_, len)| prefix.len() > len) {
                    best = Some((p, prefix.len()));
                }
            }
        }
        best.map(|(p, _)| p)
    }

    pub fn for_hook(&self, hook: &str) -> Option<&'a Plugin> {
        self.longest_prefix(hook, |p| &p.hook_prefixes, &['_'])
    }

    /// Meta keys: leading underscores are ignored, then the longest registered prefix wins.
    pub fn for_meta_key(&self, key: &str) -> Option<&'a Plugin> {
        self.longest_prefix(
            key.trim_start_matches('_'),
            |p| &p.meta_prefixes,
            &['_', '-'],
        )
    }

    pub fn for_log_source(&self, source: &str) -> Option<&'a Plugin> {
        self.0
            .iter()
            .find(|p| p.log_sources.iter().any(|s| s == source))
    }

    pub fn for_gateway(&self, gateway: &str) -> Option<&'a Plugin> {
        self.0
            .iter()
            .find(|p| p.gateways.iter().any(|g| g == gateway))
    }
}

pub fn by_slug(slug: &str) -> Option<&'static Plugin> {
    Lookup::default().by_slug(slug)
}

pub fn for_option_prefix(prefix: &str) -> Option<&'static Plugin> {
    Lookup::default().for_option_prefix(prefix)
}

pub fn option_prefix_label(prefix: &str) -> String {
    Lookup::default().option_prefix_label(prefix)
}

pub fn for_table(name: &str) -> Option<&'static Plugin> {
    Lookup::default().for_table(name)
}

pub fn for_hook(hook: &str) -> Option<&'static Plugin> {
    Lookup::default().for_hook(hook)
}

pub fn for_log_source(source: &str) -> Option<&'static Plugin> {
    Lookup::default().for_log_source(source)
}

pub fn for_meta_key(key: &str) -> Option<&'static Plugin> {
    Lookup::default().for_meta_key(key)
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
    fn tables_hooks_logs_meta_and_gateways_resolve_to_plugins() {
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
        assert_eq!(
            Lookup::default().for_gateway("ppcp-gateway").unwrap().slug,
            "woocommerce-paypal-payments"
        );
    }

    #[test]
    fn user_registry_entries_take_precedence() {
        let user = r#"
[[plugin]]
slug = "acme"
name = "Acme Widgets"
option_prefixes = ["acme", "ppcp"]
tables = ["acme_"]
"#;
        let all = merged(Some(user)).unwrap();
        let l = Lookup(&all);
        assert_eq!(l.for_option_prefix("acme").unwrap().slug, "acme");
        assert_eq!(l.for_option_prefix("ppcp").unwrap().slug, "acme");
        assert_eq!(l.for_table("acme_widgets").unwrap().name, "Acme Widgets");
        assert_eq!(l.for_table("wc_orders").unwrap().slug, "woocommerce");
        assert!(merged(Some("not = [toml")).is_err());
    }
}
