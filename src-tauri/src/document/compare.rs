//! Compare: two rows of one table side by side, from a row address with `?compare=<key>`.

use std::time::Duration;

use serde::Serialize;
use ts_rs::TS;

use super::diff::{DocDelta, FlatDocument, compare_documents};
use super::{Document, parse_address};
use crate::address::{Address, Target};
use crate::error::{AppError, AppResult};
use crate::rules::effective::RuleSources;
use crate::session::Session;

/// Two rows of one table side by side: the row of `address` and the one its `compare` names.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub left: Document,
    pub right: Document,
    /// `left` as the old side, `right` as the new.
    pub delta: DocDelta,
}

/// The documents of the row address `text` with `?compare=<key>` and their differences.
pub async fn compare(
    session: &Session,
    text: &str,
    sources: RuleSources<'_>,
    timeout: Duration,
) -> AppResult<Comparison> {
    let address = parse_address(session, text)?;
    let (Target::Row { table, .. }, Some(other)) = (&address.target, &address.compare) else {
        return Err(AppError::InvalidInput(format!(
            "Not a row address with `?compare=`: {address}"
        )));
    };
    let right_address = Address::new(
        address.connection.clone(),
        address.database.clone(),
        Target::Row {
            table: table.clone(),
            key: other.clone(),
        },
    );
    let left_address = Address {
        view: None,
        compare: None,
        ..address
    };
    let (left, right) = futures_util::future::try_join(
        super::open(session, &left_address.to_string(), None, sources, timeout),
        super::open(session, &right_address.to_string(), None, sources, timeout),
    )
    .await?;
    let delta = compare_documents(&FlatDocument::of(&left), &FlatDocument::of(&right));
    Ok(Comparison { left, right, delta })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Cell;
    use crate::document::testing::{Rig, TIMEOUT, num, result, shop, text};

    const ORDER_COLUMNS: [&str; 5] = ["id", "status", "total", "currency", "created_gmt"];

    fn order_row(id: &str, status: &str) -> Vec<Cell> {
        vec![
            num(id),
            text(status),
            text("10.00"),
            text("EUR"),
            Cell::Temporal("2023-11-14 22:13:20".into()),
        ]
    }

    async fn compare_err(rig: &Rig, text: &str) -> AppError {
        compare(&rig.session, text, rig.sources(), TIMEOUT)
            .await
            .unwrap_err()
    }

    #[tokio::test]
    async fn an_address_that_is_not_a_row_with_compare_is_invalid_input() {
        let rig = Rig::new(shop(), "");

        for text in [
            "databee://local/wp/f_orders/1",
            "databee://local/wp/f_orders?compare=2",
            "databee://local/wp",
        ] {
            let err = compare_err(&rig, text).await;

            assert!(
                matches!(&err, AppError::InvalidInput(m) if m.starts_with("Not a row address with `?compare=`")),
                "{text}: {err:?}"
            );
        }
        assert_eq!(rig.db.queries(), Vec::<String>::new());
    }

    #[tokio::test]
    async fn an_address_of_another_connection_is_invalid_input() {
        let rig = Rig::new(shop(), "");

        let err = compare_err(&rig, "databee://other/wp/f_orders/1?compare=2").await;

        assert!(
            matches!(&err, AppError::InvalidInput(m) if m.starts_with("Unknown connection `other`")),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn the_row_is_the_old_side_and_the_compared_row_the_new() {
        let rig = Rig::new(
            shop()
                .on(
                    "`f_ordermeta`",
                    result(&["meta_id", "order_id", "k", "v"], Vec::new()),
                )
                .on(
                    "= '1' LIMIT",
                    result(&ORDER_COLUMNS, vec![order_row("1", "paid")]),
                )
                .on(
                    "= '2' LIMIT",
                    result(&ORDER_COLUMNS, vec![order_row("2", "open")]),
                ),
            "",
        );

        let comparison = compare(
            &rig.session,
            "databee://local/wp/f_orders/1?view=document&compare=2",
            rig.sources(),
            TIMEOUT,
        )
        .await
        .unwrap();

        assert_eq!(
            comparison.left.address.to_string(),
            "databee://local/wp/f_orders/1"
        );
        assert_eq!(
            comparison.right.address.to_string(),
            "databee://local/wp/f_orders/2"
        );
        let changes: Vec<(&str, &Cell, &Cell)> = comparison
            .delta
            .changed
            .iter()
            .map(|c| (c.label.as_str(), &c.old, &c.new))
            .collect();
        assert_eq!(
            changes,
            [
                ("id", &num("1"), &num("2")),
                ("status", &text("paid"), &text("open"))
            ]
        );
    }
}
