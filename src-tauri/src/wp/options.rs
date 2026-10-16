//! Options inspector: groups `wp_options` by plugin prefix, weighs autoloaded data and
//! lists transients with their expiry.

use std::collections::HashMap;

use serde::Serialize;
use ts_rs::TS;

use super::sql::AUTOLOAD_ON;

#[derive(Debug, Clone, PartialEq)]
pub struct OptionRow {
    pub name: String,
    pub bytes: u64,
    pub autoload: String,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct OptionGroup {
    pub prefix: String,
    /// The literal name pattern the group covers, e.g. `woocommerce*`.
    pub label: String,
    pub count: u64,
    pub bytes: u64,
    pub autoload_count: u64,
    pub autoload_bytes: u64,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct TransientInfo {
    pub name: String,
    pub site_wide: bool,
    pub bytes: u64,
    /// Unix timestamp; `None` means the transient never expires.
    pub expires_at: Option<i64>,
    pub expired: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct OptionsOverview {
    pub table: String,
    pub total_count: u64,
    pub autoload_bytes: u64,
    pub groups: Vec<OptionGroup>,
    /// The heaviest autoloaded options, largest first.
    pub largest_autoload: Vec<OptionSize>,
    pub transients: Vec<TransientInfo>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct OptionSize {
    pub name: String,
    pub bytes: u64,
}

const LARGEST_AUTOLOAD: usize = 15;

/// `woocommerce` → `woocommerce*` (the group also holds `_woocommerce*`).
fn prefix_label(prefix: &str) -> String {
    match prefix {
        "transients" => "_transient_* / _site_transient_*".into(),
        "(single)" => "(no prefix)".into(),
        p => format!("{p}*"),
    }
}

/// WordPress 6.6+ uses on/auto/auto-on; older versions yes.
pub fn is_autoloaded(flag: &str) -> bool {
    AUTOLOAD_ON.contains(&flag)
}

/// `woocommerce_ppcp-gateway_settings` → `woocommerce`; `_transient_x` → `transients`.
pub fn option_prefix(name: &str) -> String {
    if transient_name(name).is_some() || name.contains("_transient_timeout_") {
        return "transients".into();
    }
    let trimmed = name.trim_start_matches('_');
    match trimmed.split(['_', '-']).next() {
        Some(p) if !p.is_empty() && p.len() < trimmed.len() => p.to_string(),
        _ => "(single)".into(),
    }
}

/// `(name, site_wide)` for transient value rows, `None` for anything else.
fn transient_name(option: &str) -> Option<(&str, bool)> {
    if option.contains("_transient_timeout_") {
        return None;
    }
    option
        .strip_prefix("_site_transient_")
        .map(|n| (n, true))
        .or_else(|| option.strip_prefix("_transient_").map(|n| (n, false)))
}

pub fn overview(
    table: &str,
    rows: &[OptionRow],
    timeouts: &HashMap<String, i64>,
    now: i64,
) -> OptionsOverview {
    let mut groups: HashMap<String, OptionGroup> = HashMap::new();
    let mut transients = Vec::new();
    let mut autoload_bytes = 0;

    for row in rows {
        let auto = is_autoloaded(&row.autoload);
        if auto {
            autoload_bytes += row.bytes;
        }
        let prefix = option_prefix(&row.name);
        let g = groups.entry(prefix.clone()).or_insert_with(|| OptionGroup {
            label: prefix_label(&prefix),
            prefix,
            count: 0,
            bytes: 0,
            autoload_count: 0,
            autoload_bytes: 0,
        });
        g.count += 1;
        g.bytes += row.bytes;
        if auto {
            g.autoload_count += 1;
            g.autoload_bytes += row.bytes;
        }

        if let Some((name, site_wide)) = transient_name(&row.name) {
            let timeout_key = if site_wide {
                format!("_site_transient_timeout_{name}")
            } else {
                format!("_transient_timeout_{name}")
            };
            let expires_at = timeouts.get(&timeout_key).copied();
            transients.push(TransientInfo {
                name: name.to_string(),
                site_wide,
                bytes: row.bytes,
                expires_at,
                expired: expires_at.is_some_and(|t| t < now),
            });
        }
    }

    let mut groups: Vec<OptionGroup> = groups.into_values().collect();
    groups.sort_by(|a, b| {
        b.autoload_bytes
            .cmp(&a.autoload_bytes)
            .then(b.bytes.cmp(&a.bytes))
    });
    transients.sort_by(|a, b| b.expired.cmp(&a.expired).then(b.bytes.cmp(&a.bytes)));
    let mut largest_autoload: Vec<OptionSize> = rows
        .iter()
        .filter(|r| is_autoloaded(&r.autoload))
        .map(|r| OptionSize {
            name: r.name.clone(),
            bytes: r.bytes,
        })
        .collect();
    largest_autoload.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));
    largest_autoload.truncate(LARGEST_AUTOLOAD);

    OptionsOverview {
        table: table.to_string(),
        total_count: rows.len() as u64,
        autoload_bytes,
        groups,
        largest_autoload,
        transients,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, bytes: u64, autoload: &str) -> OptionRow {
        OptionRow {
            name: name.into(),
            bytes,
            autoload: autoload.into(),
        }
    }

    #[test]
    fn prefixes_group_plugin_options() {
        assert_eq!(
            option_prefix("woocommerce_ppcp-gateway_settings"),
            "woocommerce"
        );
        assert_eq!(option_prefix("wpseo_titles"), "wpseo");
        assert_eq!(option_prefix("_transient_wc_count_comments"), "transients");
        assert_eq!(
            option_prefix("_site_transient_timeout_theme_roots"),
            "transients"
        );
        assert_eq!(option_prefix("siteurl"), "(single)");
    }

    #[test]
    fn recognizes_old_and_new_autoload_flags() {
        for flag in ["yes", "on", "auto", "auto-on"] {
            assert!(is_autoloaded(flag), "{flag}");
        }
        for flag in ["no", "off", "auto-off"] {
            assert!(!is_autoloaded(flag), "{flag}");
        }
    }

    #[test]
    fn groups_are_sorted_by_autoload_weight() {
        let o = overview(
            "wp_options",
            &[
                row("woocommerce_a", 100, "yes"),
                row("woocommerce_b", 50, "no"),
                row("wpseo_titles", 900, "on"),
            ],
            &HashMap::new(),
            0,
        );
        assert_eq!(o.autoload_bytes, 1000);
        assert_eq!(o.groups[0].prefix, "wpseo");
        assert_eq!(o.groups[0].label, "wpseo*");
        assert_eq!(o.groups[1].count, 2);
        assert_eq!(o.groups[1].autoload_bytes, 100);
        let largest: Vec<&str> = o.largest_autoload.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(largest, ["wpseo_titles", "woocommerce_a"]);
    }

    #[test]
    fn transients_get_expiry_from_their_timeout_rows() {
        let timeouts = HashMap::from([
            ("_transient_timeout_old".to_string(), 100),
            ("_site_transient_timeout_fresh".to_string(), 5000),
        ]);
        let o = overview(
            "wp_options",
            &[
                row("_transient_old", 10, "no"),
                row("_transient_timeout_old", 10, "no"),
                row("_site_transient_fresh", 20, "no"),
                row("_transient_forever", 5, "no"),
            ],
            &timeouts,
            1000,
        );
        let t: Vec<_> = o
            .transients
            .iter()
            .map(|t| (t.name.as_str(), t.expired, t.expires_at))
            .collect();
        assert_eq!(
            t,
            [
                ("old", true, Some(100)),
                ("fresh", false, Some(5000)),
                ("forever", false, None)
            ]
        );
        assert!(o.transients[1].site_wide);
    }
}
