//! How a stored value reads: the key registry (`meta_keys.toml`) plus value sniffing turn
//! raw text into render hints (money, time, flag, …) and sensitivity. Hints annotate the
//! raw value in the UI; they never replace it or rename the key.

use std::str::FromStr;
use std::sync::OnceLock;

use chrono::{DateTime, NaiveDateTime};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

use super::context::SiteZone;
use crate::decode::php::{self, PhpValue};

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
        render: None,
        sensitive: None,
    };
    for r in rules().iter().filter(|r| glob(&r.pattern, key)) {
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
    pub hint: Option<Hint>,
    pub sensitive: Option<Sensitive>,
}

/// Site facts that affect rendering.
#[derive(Debug, Clone)]
pub struct HintContext {
    pub currency: Option<String>,
    pub zone: SiteZone,
    pub price_decimals: u32,
}

impl Default for HintContext {
    fn default() -> Self {
        Self {
            currency: None,
            zone: SiteZone::default(),
            price_decimals: DEFAULT_PRICE_DECIMALS,
        }
    }
}

pub fn annotate(key: &str, raw: Option<&str>, ctx: &HintContext) -> Annotation {
    let rule = rule_for(key);
    let mut a = Annotation {
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
        Render::Money => amount(raw).map(|value| Hint::Money {
            text: format_money(value, ctx.currency.as_deref(), ctx.price_decimals),
        }),
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
    let unix = if gmt {
        naive.and_utc().timestamp()
    } else {
        ctx.zone.to_utc(naive).timestamp()
    };
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

/// `woocommerce_price_num_decimals` when the option is not set.
pub const DEFAULT_PRICE_DECIMALS: u32 = 2;

/// A stored amount as an exact decimal; `None` for text, `inf`, `NaN` or more than 28 digits.
fn amount(raw: &str) -> Option<Decimal> {
    let raw = raw.trim();
    Decimal::from_str(raw)
        .or_else(|_| Decimal::from_scientific(raw))
        .ok()
}

/// `95.40000000` + `USD` + 2 → `$95.40`, rounded half away from zero; never `−0.00`.
fn format_money(value: Decimal, currency: Option<&str>, decimals: u32) -> String {
    let rounded = value.round_dp_with_strategy(decimals, RoundingStrategy::MidpointAwayFromZero);
    let mut magnitude = rounded.abs();
    magnitude.rescale(decimals);
    let fixed = magnitude.to_string();
    let (int_part, fraction) = fixed.split_once('.').unwrap_or((&fixed, ""));
    let grouped: Vec<String> = int_part
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect();
    let amount = match fraction {
        "" => grouped.join(","),
        f => format!("{}.{f}", grouped.join(",")),
    };
    let sign = if rounded.is_sign_negative() && !rounded.is_zero() {
        "−"
    } else {
        ""
    };
    match currency {
        Some(code) => match currency_symbol(code) {
            Some(sym) => format!("{sign}{sym}{amount}"),
            None => format!("{sign}{amount} {code}"),
        },
        None => format!("{sign}{amount}"),
    }
}

/// Formats a stored amount; unlike the `Render::Money` hint, non-numeric text comes back as stored (empty as `None`).
pub fn money(raw: &str, currency: Option<&str>, decimals: u32) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(match amount(trimmed) {
        Some(value) => format_money(value, currency, decimals),
        None => trimmed.to_string(),
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
            zone: SiteZone::Fixed(chrono::FixedOffset::east_opt(3600).unwrap()),
            price_decimals: 2,
        }
    }

    fn hint(key: &str, raw: &str) -> Option<Hint> {
        annotate(key, Some(raw), &ctx()).hint
    }

    #[test]
    fn local_datetimes_follow_dst_of_a_named_zone() {
        let berlin = HintContext {
            zone: SiteZone::Named(chrono_tz::Europe::Berlin),
            ..HintContext::default()
        };
        for (raw, unix) in [
            ("2026-07-01 12:00:00", 1782900000),
            ("2026-01-15 12:00:00", 1768474800),
        ] {
            assert_eq!(
                annotate("_order_date", Some(raw), &berlin).hint,
                Some(Hint::Time { unix, utc: false }),
                "{raw}"
            );
        }
        assert_eq!(
            annotate("date_created_gmt", Some("2026-07-01 12:00:00"), &berlin).hint,
            Some(Hint::Time {
                unix: 1782907200,
                utc: true
            })
        );
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
    fn money_uses_the_currency_and_two_decimals() {
        assert_eq!(
            hint("total_amount", "95.40000000"),
            Some(Hint::Money {
                text: "$95.40".into()
            })
        );
        assert_eq!(money("-10", Some("EUR"), 2).unwrap(), "−€10.00");
        assert_eq!(money("5", Some("SEK"), 2).unwrap(), "5.00 SEK");
        assert_eq!(money("1999", Some("USD"), 2).unwrap(), "$1,999.00");
        assert_eq!(money("1234567.891", None, 2).unwrap(), "1,234,567.89");
    }

    #[test]
    fn money_edge_cases_keep_stored_text_or_round_exactly() {
        for (raw, currency, decimals, expected) in [
            ("inf", Some("USD"), 2, Some("inf")),
            ("NaN", Some("USD"), 2, Some("NaN")),
            ("", Some("USD"), 2, None),
            (
                "12345678901234567.89",
                None,
                2,
                Some("12,345,678,901,234,567.89"),
            ),
            ("-0.001", None, 2, Some("0.00")),
            ("1.2345", Some("USD"), 3, Some("$1.235")),
            ("-1.0005", None, 3, Some("−1.001")),
            ("1999.5", None, 0, Some("2,000")),
        ] {
            assert_eq!(
                money(raw, currency, decimals).as_deref(),
                expected,
                "{raw} {decimals}"
            );
        }
    }

    #[test]
    fn non_finite_amounts_get_no_money_hint() {
        assert_eq!(hint("total_amount", "inf"), None);
    }

    #[test]
    fn money_hint_uses_the_site_price_decimals() {
        let ctx = HintContext {
            price_decimals: 3,
            ..ctx()
        };
        assert_eq!(
            annotate("total_amount", Some("95.4"), &ctx).hint,
            Some(Hint::Money {
                text: "$95.400".into()
            })
        );
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
