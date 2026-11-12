//! The readout's parts. Each reads with the readout's `Ctx`; a line whose query fails carries
//! the error instead of its result.

mod anomalies;
mod created;
mod pinned;

pub use anomalies::anomalies;
pub use created::recently_created;
pub use pinned::{pinned, status_values};

use futures_util::future::join_all;

use super::RecentLine;
use crate::address::{Address, Target};
use crate::document::{Summary, peek};
use crate::error::AppResult;
use crate::query::Ctx;
use crate::store::recent_objects::RecentObject;

pub async fn recent(ctx: &Ctx<'_>, recent: &[RecentObject]) -> AppResult<Vec<RecentLine>> {
    let addresses = recent.iter().filter_map(|r| {
        let address = Address::parse(&r.address).ok()?;
        let Target::Row { table, key } = address.target else {
            return None;
        };
        Some((ctx.row_address(&table, &key), r.at))
    });
    Ok(join_all(addresses.map(|(address, at)| recent_line(ctx, address, at))).await)
}

async fn recent_line(ctx: &Ctx<'_>, address: Address, at: i64) -> RecentLine {
    let (summary, error) = match peek::peek(ctx, &address).await {
        Ok(peek) => (peek.summary, None),
        Err(e) => (Summary::default(), Some(e.to_string())),
    };
    RecentLine {
        address,
        at,
        summary,
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Cell, ResultSet};
    use crate::query::testing::{result, shop};
    use crate::readout::testing::Rig;

    pub(super) fn one(cell: Cell) -> ResultSet {
        result(&["c"], vec![vec![cell]])
    }

    pub(super) fn table(ctx: &Ctx<'_>, name: &str) -> Address {
        ctx.address(Target::Table {
            table: name.into(),
            filter: Vec::new(),
        })
    }

    #[tokio::test]
    async fn recent_keeps_only_row_addresses_and_carries_the_error_of_an_unreadable_row() {
        let rig = Rig::new(shop(), "", &[]);
        let ctx = rig.ctx().await;
        let row = ctx.row_address("f_orders", "1");
        let recent_objects = [
            RecentObject {
                address: "not an address".into(),
                at: 5,
            },
            RecentObject {
                address: table(&ctx, "f_orders").to_string(),
                at: 4,
            },
            RecentObject {
                address: row.to_string(),
                at: 3,
            },
        ];

        let lines = recent(&ctx, &recent_objects).await.unwrap();

        assert_eq!(lines.len(), 1);
        assert_eq!((lines[0].address.clone(), lines[0].at), (row, 3));
        assert!(lines[0].summary.values.is_empty());
        assert!(lines[0].error.is_some());
    }
}
