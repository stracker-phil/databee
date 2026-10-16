//! What a stored value *means*: the key registry (`meta_keys.toml`) plus value sniffing
//! turn raw text into render hints (money, time, flag, …), labels, groups and
//! sensitivity. The UI only draws the hints.

use std::sync::OnceLock;

use chrono::{DateTime, NaiveDateTime};
use serde::{Deserialize, Serialize};

use super::php::{self, PhpValue};

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Render {
    Money,
    Time,
    Flag,
    Url,
    UserAgent,
    Email,
    Roles,
    Country,
    GuestOrUser,
    Text,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Sensitive {
    /// Masked until clicked.
    Secret,
}

#[derive(Deserialize, Debug, Clone)]
struct KeyRule {
    #[serde(rename = "match")]
    pattern: String,
    group: Option<String>,
    label: Option<String>,
    render: Option<Render>,
    sensitive: Option<Sensitive>,
}

#[derive(Deserialize)]
struct RuleFile {
    key: Vec<KeyRule>,
}

fn rules() -> &'static [KeyRule] {
    static RULES: OnceLock<Vec<KeyRule>> = OnceLock::new();
    RULES.get_or_init(|| {
        toml::from_str::<RuleFile>(include_str!("meta_keys.toml"))
            .expect("built-in meta_keys.toml is valid")
            .key
    })
}

/// Display order of registry groups; unknown prefix groups follow.
pub const GROUP_ORDER: &[&str] = &[
    "Profile",
    "Customer",
    "Billing address",
    "Shipping address",
    "Payment",
    "PayPal",
    "Order",
    "Line item",
    "Subscription",
    "Pricing",
    "Inventory",
    "Product",
    "Shipping",
    "Attribution",
    "Fraud check",
    "Notifications",
    "Activity",
    "Sessions",
    "Security",
    "Analytics",
    "Reviews",
    "File",
    "Admin UI",
];

/// `*` matches any run of characters.
fn glob(pattern: &str, key: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == key;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !key.starts_with(first) || key.len() < first.len() + last.len() || !key.ends_with(last) {
        return false;
    }
    let mut rest = &key[first.len()..key.len() - last.len()];
    for mid in &parts[1..parts.len() - 1] {
        match rest.find(mid) {
            Some(i) => rest = &rest[i + mid.len()..],
            None => return false,
        }
    }
    true
}

/// Merges all matching rules: the first rule that sets a property wins, so specific
/// rules (listed first) can be complemented by generic ones like `*_token`.
fn rule_for(key: &str) -> KeyRule {
    let mut merged = KeyRule {
        pattern: String::new(),
        group: None,
        label: None,
        render: None,
        sensitive: None,
    };
    for r in rules().iter().filter(|r| glob(&r.pattern, key)) {
        merged.group = merged.group.or_else(|| r.group.clone());
        merged.label = merged.label.or_else(|| r.label.clone());
        merged.render = merged.render.or(r.render);
        merged.sensitive = merged.sensitive.or(r.sensitive);
    }
    merged
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Hint {
    Money {
        text: String,
    },
    /// `utc: false` means a site-local time converted with the site's GMT offset.
    Time {
        unix: i64,
        utc: bool,
    },
    Flag {
        on: bool,
    },
    Url {
        href: String,
    },
    UserAgent {
        text: String,
    },
    Email,
    Roles {
        roles: Vec<String>,
    },
    Country {
        code: String,
    },
    Guest,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Annotation {
    pub label: Option<String>,
    pub group: Option<String>,
    pub hint: Option<Hint>,
    pub sensitive: Option<Sensitive>,
}

/// Site facts that affect rendering.
#[derive(Debug, Clone, Default)]
pub struct HintContext {
    pub currency: Option<String>,
    pub gmt_offset_secs: i64,
}

pub fn annotate(key: &str, raw: Option<&str>, ctx: &HintContext) -> Annotation {
    let rule = rule_for(key);
    let mut a = Annotation {
        label: rule.label,
        group: rule.group,
        hint: None,
        sensitive: rule.sensitive,
    };
    let Some(raw) = raw.map(str::trim) else {
        return a;
    };
    a.hint = match rule.render {
        Some(Render::Text) => None,
        Some(render) => hint_for(render, key, raw, ctx),
        None => sniff(key, raw, ctx),
    };
    a
}

fn hint_for(render: Render, key: &str, raw: &str, ctx: &HintContext) -> Option<Hint> {
    match render {
        Render::Money => money(raw, ctx.currency.as_deref()).map(|text| Hint::Money { text }),
        Render::Time => time(key, raw, ctx),
        Render::Flag => flag(raw, true),
        Render::Url => url(raw),
        Render::UserAgent => (!raw.is_empty()).then(|| Hint::UserAgent {
            text: user_agent(raw),
        }),
        Render::Email => is_email(raw).then_some(Hint::Email),
        Render::Roles => roles(raw).map(|roles| Hint::Roles { roles }),
        Render::Country => (raw.len() == 2 && raw.chars().all(|c| c.is_ascii_uppercase()))
            .then(|| Hint::Country { code: raw.into() }),
        Render::GuestOrUser => matches!(raw, "" | "0").then_some(Hint::Guest),
        Render::Text => None,
    }
}

const TIME_WORDS: &[&str] = &[
    "date",
    "time",
    "_ts",
    "expir",
    "created",
    "modified",
    "updated",
    "last_",
    "_at",
    "since",
    "_gmt",
    "registered",
    "timestamp",
    "_sent",
    "_claimed",
];

/// Guesses a renderer for keys the registry doesn't know.
fn sniff(key: &str, raw: &str, ctx: &HintContext) -> Option<Hint> {
    let lower = key.to_ascii_lowercase();
    if raw.starts_with("http://") || raw.starts_with("https://") {
        return url(raw);
    }
    if raw.starts_with("Mozilla/") || lower.contains("user_agent") {
        return Some(Hint::UserAgent {
            text: user_agent(raw),
        });
    }
    if is_email(raw) {
        return Some(Hint::Email);
    }
    if (TIME_WORDS.iter().any(|w| lower.contains(w)) || looks_like_datetime(raw))
        && let Some(t) = time(key, raw, ctx)
    {
        return Some(t);
    }
    flag(raw, false)
}

fn looks_like_datetime(raw: &str) -> bool {
    NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S").is_ok()
}

fn time(key: &str, raw: &str, ctx: &HintContext) -> Option<Hint> {
    if let Ok(n) = raw.parse::<i64>() {
        // Plausible epoch range 2001–2100 in seconds or milliseconds.
        let secs = match n {
            1_000_000_000..=4_102_444_800 => n,
            1_000_000_000_000..=4_102_444_800_000 => n / 1000,
            _ => return None,
        };
        return Some(Hint::Time {
            unix: secs,
            utc: true,
        });
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Some(Hint::Time {
            unix: dt.timestamp(),
            utc: true,
        });
    }
    let naive = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S").ok()?;
    let gmt = key.to_ascii_lowercase().contains("gmt");
    let unix = naive.and_utc().timestamp() - if gmt { 0 } else { ctx.gmt_offset_secs };
    Some(Hint::Time { unix, utc: gmt })
}

fn flag(raw: &str, numeric: bool) -> Option<Hint> {
    let on = match raw.to_ascii_lowercase().as_str() {
        "yes" | "true" | "on" => true,
        "no" | "false" | "off" => false,
        "1" if numeric => true,
        "0" | "" if numeric => false,
        _ => return None,
    };
    Some(Hint::Flag { on })
}

fn url(raw: &str) -> Option<Hint> {
    let ok = (raw.starts_with("http://") || raw.starts_with("https://"))
        && !raw.contains(char::is_whitespace);
    ok.then(|| Hint::Url { href: raw.into() })
}

fn is_email(raw: &str) -> bool {
    let Some((local, domain)) = raw.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && raw.len() < 255
        && !raw.contains(char::is_whitespace)
}

fn currency_symbol(code: &str) -> Option<&'static str> {
    Some(match code {
        "USD" => "$",
        "EUR" => "€",
        "GBP" => "£",
        "JPY" => "¥",
        "AUD" => "A$",
        "CAD" => "CA$",
        "NZD" => "NZ$",
        "BRL" => "R$",
        "INR" => "₹",
        _ => return None,
    })
}

/// `95.40000000` + `USD` → `$95.40`; amounts without currency get two decimals.
pub fn money(raw: &str, currency: Option<&str>) -> Option<String> {
    let value: f64 = raw.trim().parse().ok()?;
    let fixed = format!("{:.2}", value.abs());
    let (int_part, cents) = fixed.split_once('.').unwrap_or((&fixed, "00"));
    let grouped: Vec<String> = int_part
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect();
    let amount = format!("{}.{cents}", grouped.join(","));
    let sign = if value < 0.0 { "−" } else { "" };
    Some(match currency {
        Some(code) => match currency_symbol(code) {
            Some(sym) => format!("{sign}{sym}{amount}"),
            None => format!("{sign}{amount} {code}"),
        },
        None => format!("{sign}{amount}"),
    })
}

/// Role names from a serialized capabilities array (`a:1:{s:8:"customer";b:1;}`).
fn roles(raw: &str) -> Option<Vec<String>> {
    let PhpValue::Array(items) = php::parse(raw.as_bytes()).ok()? else {
        return None;
    };
    Some(
        items
            .into_iter()
            .filter(|(_, v)| matches!(v, PhpValue::Bool(true)))
            .filter_map(|(k, _)| match k {
                php::PhpKey::Str(s) => String::from_utf8(s).ok(),
                php::PhpKey::Int(_) => None,
            })
            .collect(),
    )
}

/// `Mozilla/5.0 (Macintosh; …) Chrome/154.0.0.0 Safari/537.36` → `Chrome 154 · macOS`.
pub fn user_agent(ua: &str) -> String {
    let version_after = |marker: &str| {
        ua.find(marker).map(|i| {
            ua[i + marker.len()..]
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .unwrap_or("")
                .to_string()
        })
    };
    let browsers: &[(&str, &str)] = &[
        ("Edg/", "Edge"),
        ("OPR/", "Opera"),
        ("Firefox/", "Firefox"),
        ("Chrome/", "Chrome"),
        ("CriOS/", "Chrome"),
        ("Version/", "Safari"),
        ("curl/", "curl"),
        ("WordPress/", "WordPress"),
        ("PostmanRuntime/", "Postman"),
        ("GuzzleHttp/", "Guzzle"),
    ];
    let browser = browsers
        .iter()
        .find_map(|(marker, name)| {
            version_after(marker).map(|v| {
                if v.is_empty() {
                    name.to_string()
                } else {
                    format!("{name} {v}")
                }
            })
        })
        .unwrap_or_else(|| ua.split(['/', ' ']).next().unwrap_or(ua).to_string());
    let os = [
        ("iPhone", "iOS"),
        ("iPad", "iPadOS"),
        ("Android", "Android"),
        ("Mac OS X", "macOS"),
        ("Windows NT", "Windows"),
        ("CrOS", "ChromeOS"),
        ("Linux", "Linux"),
    ]
    .iter()
    .find(|(marker, _)| ua.contains(marker))
    .map(|(_, name)| *name);
    match os {
        Some(os) => format!("{browser} · {os}"),
        None => browser,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> HintContext {
        HintContext {
            currency: Some("USD".into()),
            gmt_offset_secs: 3600,
        }
    }

    fn hint(key: &str, raw: &str) -> Option<Hint> {
        annotate(key, Some(raw), &ctx()).hint
    }

    #[test]
    fn globs_match_prefixes_suffixes_and_middles() {
        assert!(glob("billing_*", "billing_city"));
        assert!(glob("*_amount", "total_amount"));
        assert!(glob("*user-settings*", "wp_user-settings-time"));
        assert!(!glob("billing_*", "shipping_city"));
        assert!(glob("_price", "_price"));
    }

    #[test]
    fn registry_groups_known_keys_instead_of_first_word() {
        let group = |k: &str| annotate(k, Some("x"), &ctx()).group;
        assert_eq!(group("last_name").as_deref(), Some("Profile"));
        assert_eq!(group("last_update").as_deref(), Some("Activity"));
        assert_eq!(group("show_admin_bar_front").as_deref(), Some("Admin UI"));
        assert_eq!(group("billing_city").as_deref(), Some("Billing address"));
        assert_eq!(group("_ppcp_paypal_order_id").as_deref(), Some("PayPal"));
        assert_eq!(group("my_plugin_thing"), None);
    }

    #[test]
    fn money_uses_the_currency_and_two_decimals() {
        assert_eq!(
            hint("total_amount", "95.40000000"),
            Some(Hint::Money {
                text: "$95.40".into()
            })
        );
        assert_eq!(money("-10", Some("EUR")).unwrap(), "−€10.00");
        assert_eq!(money("5", Some("SEK")).unwrap(), "5.00 SEK");
        assert_eq!(money("1999", Some("USD")).unwrap(), "$1,999.00");
        assert_eq!(money("1234567.891", None).unwrap(), "1,234,567.89");
    }

    #[test]
    fn epoch_seconds_and_milliseconds_become_times() {
        assert_eq!(
            hint("_wc_push_notification_sent", "1791906926"),
            Some(Hint::Time {
                unix: 1791906926,
                utc: true
            })
        );
        assert_eq!(
            hint("expires_at", "1768390956989"),
            Some(Hint::Time {
                unix: 1768390956,
                utc: true
            })
        );
        assert_eq!(hint("expires_at", "42"), None);
    }

    #[test]
    fn local_datetimes_use_the_site_offset_gmt_ones_do_not() {
        assert_eq!(
            hint("post_date", "2026-01-01 01:00:00"),
            Some(Hint::Time {
                unix: 1767225600,
                utc: false
            })
        );
        assert_eq!(
            hint("date_created_gmt", "2026-01-01 00:00:00"),
            Some(Hint::Time {
                unix: 1767225600,
                utc: true
            })
        );
        assert_eq!(hint("post_date", "0000-00-00 00:00:00"), None);
    }

    #[test]
    fn flags_from_yes_no_and_registry_numbers() {
        assert_eq!(hint("anything", "yes"), Some(Hint::Flag { on: true }));
        assert_eq!(
            hint("rich_editing", "false"),
            Some(Hint::Flag { on: false })
        );
        assert_eq!(hint("paying_customer", "1"), Some(Hint::Flag { on: true }));
        assert_eq!(hint("some_count", "1"), None);
    }

    #[test]
    fn urls_emails_and_user_agents_are_recognized() {
        assert_eq!(
            hint("x", "https://shop.test/a"),
            Some(Hint::Url {
                href: "https://shop.test/a".into()
            })
        );
        assert_eq!(hint("contact", "jane@shop.test"), Some(Hint::Email));
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/154.0.0.0 Safari/537.36";
        assert_eq!(
            hint("_wc_order_attribution_user_agent", ua),
            Some(Hint::UserAgent {
                text: "Chrome 154 · macOS".into()
            })
        );
        assert_eq!(
            user_agent(
                "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1"
            ),
            "Safari 17 · iOS"
        );
        assert_eq!(
            user_agent("WordPress/6.8; https://shop.test"),
            "WordPress 6"
        );
    }

    #[test]
    fn roles_are_read_from_capabilities() {
        assert_eq!(
            hint(
                "wp_capabilities",
                r#"a:2:{s:13:"administrator";b:1;s:6:"editor";b:0;}"#
            ),
            Some(Hint::Roles {
                roles: vec!["administrator".into()]
            })
        );
    }

    #[test]
    fn guest_customers_and_countries() {
        assert_eq!(hint("customer_id", "0"), Some(Hint::Guest));
        assert_eq!(hint("customer_id", "12"), None);
        assert_eq!(
            hint("billing_country", "AT"),
            Some(Hint::Country { code: "AT".into() })
        );
    }

    #[test]
    fn secrets_are_flagged() {
        for key in [
            "user_pass",
            "session_tokens",
            "_order_key",
            "client_secret",
            "my_api_key",
            "access_token",
        ] {
            assert_eq!(
                annotate(key, Some("x"), &ctx()).sensitive,
                Some(Sensitive::Secret),
                "{key}"
            );
        }
        assert_eq!(
            annotate("_ppcp_paypal_order_id", Some("x"), &ctx()).sensitive,
            None
        );
    }
}
