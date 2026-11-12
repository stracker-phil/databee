//! How a change line words a rule: references, scopes, value styles, kinds and preset lists.

use crate::lens::ColumnsRef;
use crate::rules::kinds::{KindRule, StyleMatch, StyleRule, ValueRule};
use crate::rules::overview::rows::ReferenceRow;

fn columns_text(columns: &ColumnsRef) -> String {
    format!("{}.{}", columns.table, columns.columns.join(", "))
}

pub(super) fn reference_text(
    from: &ColumnsRef,
    when: &str,
    to: &ColumnsRef,
    to_when: &str,
) -> String {
    let mut text = columns_text(from);
    if !when.is_empty() {
        text.push_str(&format!(" when {when}"));
    }
    text.push_str(&format!(" → {}", columns_text(to)));
    if !to_when.is_empty() {
        text.push_str(&format!(" where {to_when}"));
    }
    text
}

pub(super) fn row_text(row: &ReferenceRow) -> String {
    reference_text(&row.from, &row.when, &row.to, &row.to_when)
}

/// `wc_orders.status, meta keys _wc_* in wc_orders_meta`, as the rule stores it.
fn scope_text<T: ToString>(columns: &[T], meta_keys: &[String], in_meta: &[String]) -> String {
    let mut parts = Vec::new();
    if !columns.is_empty() {
        let columns: Vec<String> = columns.iter().map(ToString::to_string).collect();
        parts.push(columns.join(", "));
    }
    if !meta_keys.is_empty() {
        let mut keys = format!("meta keys {}", meta_keys.join(", "));
        if !in_meta.is_empty() {
            keys.push_str(&format!(" in {}", in_meta.join(", ")));
        }
        parts.push(keys);
    }
    parts.join(" · ")
}

pub(super) fn style_text(rule: &StyleRule) -> String {
    let matching = match rule.matching {
        StyleMatch::Eq => "=",
        StyleMatch::Like => "LIKE",
        StyleMatch::Regexp => "REGEXP",
    };
    let scope = scope_text(&rule.columns, &rule.meta_keys, &rule.r#in);
    format!(
        "{scope} {matching} '{}' · {}",
        rule.value,
        rule.color.name()
    )
}

pub(super) fn kind_text(rule: &KindRule) -> String {
    let kind = match &rule.value {
        ValueRule::Money(money) if money.currency.is_empty() => "money".to_string(),
        ValueRule::Money(money) => format!("money · currency from {}", money.currency.join(", ")),
        ValueRule::Timestamp(zone) => {
            let zone = serde_json::to_value(zone).ok();
            let zone = zone.as_ref().and_then(|z| z.as_str()).unwrap_or_default();
            format!("timestamp · {zone}")
        }
        ValueRule::Enum => "enum".into(),
        ValueRule::Secret => "secret".into(),
        ValueRule::Number => "number".into(),
    };
    let scope = scope_text(&rule.columns, &rule.meta_keys, &rule.in_meta);
    format!("{scope} · {kind}")
}

pub(super) fn list_text(names: &[String]) -> String {
    if names.is_empty() {
        return "none".into();
    }
    names.join(", ")
}
