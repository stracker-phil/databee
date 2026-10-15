//! Humanized WooCommerce log lines. PayPal Payments writes
//! `#7199.REST - POST https://api-m.sandbox.paypal.com/v2/checkout/orders/<id>/capture`
//! followed by `Response: Array ( [code] => 201 [message] => Created )`; this turns that into
//! `PayPal API · capture order → 201`. Unknown lines keep their first ~80 characters.

use serde::Serialize;

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LogSummary {
    /// Short human title shown in the timeline.
    pub title: String,
    pub method: Option<String>,
    pub endpoint: Option<String>,
    pub status: Option<u16>,
    /// The PayPal resource (order id, capture id…) the call was about.
    pub resource_id: Option<String>,
}

const MAX_TITLE: usize = 80;

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max - 1).collect();
        format!("{}…", cut.trim_end())
    }
}

/// Maps a PayPal REST path to a short name and extracts the resource id.
fn paypal_endpoint(path: &str) -> (String, Option<String>) {
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    let s = |i: usize| segments.get(i).copied().unwrap_or("");
    let resource = |i: usize| {
        segments
            .get(i)
            .map(|x| x.to_string())
            .filter(|x| !x.is_empty())
    };
    match (s(0), s(1), s(2)) {
        ("v1", "oauth2", "token") => ("access token".into(), None),
        ("v2", "checkout", "orders") => match (s(3), s(4)) {
            ("", _) => ("create order".into(), None),
            (_, "capture") => ("capture order".into(), resource(3)),
            (_, "authorize") => ("authorize order".into(), resource(3)),
            (_, "confirm-payment-source") => ("confirm payment source".into(), resource(3)),
            (_, "track" | "trackers") => ("add tracking".into(), resource(3)),
            (_, "") => ("order".into(), resource(3)),
            (_, other) => (format!("order {other}"), resource(3)),
        },
        ("v2", "payments", "captures") => match s(4) {
            "refund" => ("refund capture".into(), resource(3)),
            "" => ("capture".into(), resource(3)),
            other => (format!("capture {other}"), resource(3)),
        },
        ("v2", "payments", "authorizations") => match s(4) {
            "capture" => ("capture authorization".into(), resource(3)),
            "void" => ("void authorization".into(), resource(3)),
            "reauthorize" => ("reauthorize".into(), resource(3)),
            _ => ("authorization".into(), resource(3)),
        },
        ("v2", "payments", "refunds") => ("refund".into(), resource(3)),
        ("v1", "notifications", "webhooks") => ("webhooks".into(), resource(3)),
        ("v1", "notifications", "webhooks-event-types") => ("webhook event types".into(), None),
        ("v1", "notifications", "verify-webhook-signature") => ("verify webhook".into(), None),
        ("v3", "vault", "payment-tokens") => ("vault token".into(), resource(3)),
        ("v3", "vault", "setup-tokens") => ("vault setup token".into(), resource(3)),
        ("v1", "identity", "generate-token") => ("client token".into(), None),
        ("v1", "customer", "partners") => ("merchant integration".into(), None),
        ("v1", "billing", "subscriptions") => ("billing subscription".into(), resource(3)),
        ("v1", "catalogs", "products") => ("catalog product".into(), resource(3)),
        ("v1", "billing", "plans") => ("billing plan".into(), resource(3)),
        ("v1", "reporting", "transactions") => ("transaction search".into(), None),
        _ => (path.trim_start_matches('/').to_string(), None),
    }
}

/// Parses `[code] => 201` from a PHP `print_r` response dump.
fn response_code(message: &str) -> Option<u16> {
    let idx = message.find("[code] =>")?;
    message[idx + 9..]
        .trim_start()
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

/// Humanizes one log message for the given source.
pub fn humanize(source: Option<&str>, message: &str) -> LogSummary {
    let first = message.lines().next().unwrap_or("").trim();
    let paypal = source.is_some_and(|s| s.starts_with("woocommerce-paypal"));
    if paypal {
        // "#7199.REST - POST https://host/path?query" or "#7199.REST -  https://host/path" (GET)
        let rest = first
            .split_once(" - ")
            .map(|(_, r)| r.trim())
            .unwrap_or(first);
        let (method, url) = match rest.split_once(' ') {
            Some((m, u)) if m.chars().all(|c| c.is_ascii_uppercase()) && !m.is_empty() => {
                (Some(m.to_string()), u.trim())
            }
            _ if rest.starts_with("http") => (Some("GET".to_string()), rest),
            _ => (None, ""),
        };
        if let Some(host_path) = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
        {
            let path = host_path.split_once('/').map(|(_, p)| p).unwrap_or("");
            let path = path.split('?').next().unwrap_or("");
            let (mut name, resource_id) = paypal_endpoint(path);
            // Plain resource calls say nothing without their verb: "PATCH order", "GET order".
            if let Some(m) = &method
                && !name.contains(' ')
            {
                name = format!("{m} {name}");
            }
            let status = response_code(message);
            let title = match status {
                Some(code) => format!("PayPal API · {name} → {code}"),
                None => format!("PayPal API · {name}"),
            };
            return LogSummary {
                title,
                method,
                endpoint: Some(format!("/{path}")),
                status,
                resource_id,
            };
        }
        if let Some(r) = first
            .strip_prefix('#')
            .and_then(|f| f.split_once(" - "))
            .map(|(_, r)| r.trim())
        {
            if let Some(req) = r.strip_prefix("[New Request]") {
                return LogSummary {
                    title: format!(
                        "PayPal request{}",
                        req.split('@')
                            .next()
                            .map(|s| format!(" ·{s}"))
                            .unwrap_or_default()
                    ),
                    method: None,
                    endpoint: None,
                    status: None,
                    resource_id: None,
                };
            }
            return LogSummary {
                title: truncate(r, MAX_TITLE),
                method: None,
                endpoint: None,
                status: None,
                resource_id: None,
            };
        }
    }
    LogSummary {
        title: truncate(first, MAX_TITLE),
        method: None,
        endpoint: None,
        status: None,
        resource_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: Option<&str> = Some("woocommerce-paypal-payments");

    #[test]
    fn paypal_api_calls_become_short_titles_with_status() {
        let msg = "#7199.REST - POST https://api-m.sandbox.paypal.com/v2/checkout/orders/3UT60541JW9211534/capture\nResponse Debug ID: f464302a597b4\nResponse: Array\n(\n    [code] => 201\n    [message] => Created\n)\n";
        let s = humanize(SRC, msg);
        assert_eq!(s.title, "PayPal API · capture order → 201");
        assert_eq!(s.method.as_deref(), Some("POST"));
        assert_eq!(
            s.endpoint.as_deref(),
            Some("/v2/checkout/orders/3UT60541JW9211534/capture")
        );
        assert_eq!(s.status, Some(201));
        assert_eq!(s.resource_id.as_deref(), Some("3UT60541JW9211534"));

        let get = humanize(
            SRC,
            "#7199.REST -  https://api-m.sandbox.paypal.com/v2/checkout/orders/3UT60541JW9211534\nResponse: Array\n(\n    [code] => 200\n)",
        );
        assert_eq!(get.title, "PayPal API · GET order → 200");
        assert_eq!(get.method.as_deref(), Some("GET"));

        let patch = humanize(
            SRC,
            "#7199.REST - PATCH https://api-m.sandbox.paypal.com/v2/checkout/orders/3UT60541JW9211534\nRequest Body: [{\"op\":\"replace\"}]",
        );
        assert_eq!(patch.title, "PayPal API · PATCH order");
        assert_eq!(patch.status, None);

        let token = humanize(
            SRC,
            "#1703.WC - POST https://api-m.sandbox.paypal.com/v1/oauth2/token?grant_type=client_credentials&response_type=client_token",
        );
        assert_eq!(token.title, "PayPal API · access token");
        assert_eq!(token.endpoint.as_deref(), Some("/v1/oauth2/token"));

        let refund = humanize(
            SRC,
            "#9.REST - POST https://api-m.paypal.com/v2/payments/captures/5XJ1234/refund\n[code] => 201",
        );
        assert_eq!(refund.title, "PayPal API · refund capture → 201");
        assert_eq!(refund.resource_id.as_deref(), Some("5XJ1234"));

        let webhook = humanize(
            SRC,
            "#1.WC - POST https://api-m.paypal.com/v1/notifications/verify-webhook-signature\n[code] => 200",
        );
        assert_eq!(webhook.title, "PayPal API · verify webhook → 200");
    }

    #[test]
    fn other_lines_keep_their_first_eighty_characters() {
        assert_eq!(
            humanize(SRC, "#1703.WC - [New Request] POST / @ 21:24:36.526").title,
            "PayPal request · POST / "
        );
        let long = "x".repeat(200);
        assert_eq!(
            humanize(Some("wcs-functions"), &long).title.chars().count(),
            80
        );
        assert_eq!(
            humanize(None, "Email \"new_order\" for order #4711 sent").title,
            "Email \"new_order\" for order #4711 sent"
        );
        let unknown = humanize(Some("other"), "Unrelated error #14711");
        assert_eq!(unknown.title, "Unrelated error #14711");
        assert_eq!(unknown.method, None);
    }
}
