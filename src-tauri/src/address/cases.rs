//! Canonical addresses with their text, one per form of the spec plus the encoding edge cases.
//! The round-trip test walks them, and the fixture test writes them to
//! `src/test/fixtures/addresses.json` for the TypeScript port (`src/lib/address.ts`).

use super::{Address, Target, View};
use crate::db::ColumnEquals;

fn at(target: Target) -> Address {
    Address::new("my-shop", "db", target)
}

fn table(table: &str, filter: &[(&str, Option<&str>)]) -> Target {
    Target::Table {
        table: table.into(),
        filter: filter
            .iter()
            .map(|(column, value)| ColumnEquals {
                column: (*column).into(),
                value: value.map(Into::into),
            })
            .collect(),
    }
}

fn row(table: &str, key: &str) -> Target {
    Target::Row {
        table: table.into(),
        key: key.into(),
    }
}

/// `(canonical text, address)`; formatting the address gives the text and parsing the text gives the address.
pub fn canonical() -> Vec<(&'static str, Address)> {
    vec![
        ("databee://my-shop/db/", at(Target::Home)),
        (
            "databee://my-shop/db/wp_wc_orders",
            at(table("wp_wc_orders", &[])),
        ),
        (
            "databee://my-shop/db/wp_postmeta?post_id=12&meta_key=_price",
            at(table(
                "wp_postmeta",
                &[("post_id", Some("12")), ("meta_key", Some("_price"))],
            )),
        ),
        (
            "databee://my-shop/db/wp_posts?post_parent",
            at(table("wp_posts", &[("post_parent", None)])),
        ),
        (
            "databee://my-shop/db/wp_options?option_value=",
            at(table("wp_options", &[("option_value", Some(""))])),
        ),
        (
            "databee://my-shop/db/wp_wc_orders/3229",
            at(row("wp_wc_orders", "3229")),
        ),
        (
            "databee://my-shop/db/wp_wc_orders/3229?view=rows",
            Address {
                view: Some(View::Rows),
                ..at(row("wp_wc_orders", "3229"))
            },
        ),
        (
            "databee://my-shop/db/wp_wc_orders/3229?view=document",
            Address {
                view: Some(View::Document),
                ..at(row("wp_wc_orders", "3229"))
            },
        ),
        (
            "databee://my-shop/db/wp_wc_orders/3229?compare=3227",
            Address {
                compare: Some("3227".into()),
                ..at(row("wp_wc_orders", "3229"))
            },
        ),
        (
            "databee://my-shop/db/wp_wc_orders/3229?view=rows&compare=3227",
            Address {
                view: Some(View::Rows),
                compare: Some("3227".into()),
                ..at(row("wp_wc_orders", "3229"))
            },
        ),
        (
            "databee://my-shop/db/q/failed-orders",
            at(Target::Query {
                id: "failed-orders".into(),
            }),
        ),
        (
            "databee://my-shop/db/?find=PAYID-123",
            at(Target::Find {
                value: "PAYID-123".into(),
            }),
        ),
        ("databee://my-shop/db/?project", at(Target::Project)),
        ("databee://my-shop/db/%71", at(table("q", &[]))),
        ("databee://my-shop/db/%71/7", at(row("q", "7"))),
        (
            "databee://my-shop/db/wp_options/a%2Fb%3Fc%26d%3De%25f%23g",
            at(row("wp_options", "a/b?c&d=e%f#g")),
        ),
        (
            "databee://my-shop/db/wp_options?option_name=caf%C3%A9%20au%20lait",
            at(table(
                "wp_options",
                &[("option_name", Some("café au lait"))],
            )),
        ),
        (
            "databee://my-shop/db/?find=a%2Bb%20c",
            at(Target::Find {
                value: "a+b c".into(),
            }),
        ),
        (
            "databee://my%20site/wp-test.1/wp_users/1",
            Address::new("my site", "wp-test.1", row("wp_users", "1")),
        ),
        (
            "databee://ddev-db/db/wp_users/~u_1.x",
            Address::new("ddev-db", "db", row("wp_users", "~u_1.x")),
        ),
    ]
}
