use super::own::Own;
use crate::db::Cell;
use crate::query::Ctx;
use crate::query::testing::{num, result, text};

pub fn order(ctx: &Ctx<'_>, id: &str, status: Cell, currency: Cell) -> Own {
    let row = vec![
        num(id),
        status,
        text("10.00"),
        currency,
        Cell::Temporal("2023-11-14 22:13:20".into()),
    ];
    let columns = ["id", "status", "total", "currency", "created_gmt"];
    Own {
        rows: ctx.annotate("f_orders", result(&columns, vec![row])),
        entity: None,
    }
}
