//! What a snapshot stores of an address: a row's document, or the rows of a saved query keyed by
//! their primary key, both as a `FlatDocument`.

use std::time::Duration;

use super::diff::{FlatDocument, FlatField, FlatGroup, FlatRow};
use super::{Ctx, Document, GroupShape};
use crate::address::{Address, Target};
use crate::db::classify::is_read_only;
use crate::db::{Cell, ExecuteResponse, ResultSet};
use crate::error::{AppError, AppResult};
use crate::lens::annotate_execute;
use crate::readout::sql::{in_database, limited, single_statement};
use crate::rules::effective::RuleSources;
use crate::session::Session;
use crate::value::{KeyPart, Provenance, RowKey};

/// Rows of a saved query a snapshot keeps.
pub const QUERY_ROWS: u32 = 1000;
/// The one group of a saved query's snapshot.
pub const RESULT_GROUP: &str = "result";

/// The cells of `address` now, and the document when it is a row.
pub async fn capture(
    session: &Session,
    address: &Address,
    sources: RuleSources<'_>,
    timeout: Duration,
) -> AppResult<(FlatDocument, Option<Document>)> {
    match &address.target {
        Target::Row { .. } | Target::Table { .. } => {
            let document =
                super::open(session, &address.to_string(), None, sources, timeout).await?;
            Ok((FlatDocument::of(&document), Some(document)))
        }
        Target::Query { id } => {
            let ctx = Ctx::load(session, &address.database, sources, timeout).await?;
            let result = saved_query(&ctx, sources, id).await?;
            Ok((flat_result(&ctx, &result), None))
        }
        _ => Err(AppError::InvalidInput(format!(
            "Only a row or a saved query has snapshots: {address}"
        ))),
    }
}

/// The first `QUERY_ROWS` rows of saved query `id`, which must only read.
async fn saved_query(ctx: &Ctx<'_>, sources: RuleSources<'_>, id: &str) -> AppResult<ResultSet> {
    let project = sources.projects.get(&ctx.session.slug(), &ctx.database);
    let entry = crate::saved::list(&project, &sources.projects.global(), &ctx.rules)
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| AppError::InvalidInput(format!("No saved query {id}")))?;
    let statement = single_statement(&entry.sql).map_err(AppError::InvalidInput)?;
    is_read_only(statement).map_err(AppError::InvalidInput)?;
    let sql = in_database(ctx.dialect, &ctx.database, &limited(statement, QUERY_ROWS));
    let mut response = ExecuteResponse {
        results: vec![ctx.fetch(&sql).await?],
        elapsed_ms: 0,
        database: Some(ctx.database.clone()),
        failure: None,
    };
    annotate_execute(ctx.session, &mut response, Some(sources)).await;
    Ok(response.results.remove(0))
}

/// Where a result column was read: its table in this database and the positions of that table's
/// primary-key columns in the result.
struct Origin {
    table: String,
    column: String,
    key: Vec<(String, usize)>,
}

fn origins(ctx: &Ctx<'_>, result: &ResultSet) -> Vec<Option<Origin>> {
    let position = |table: &str, column: &str| {
        result.columns.iter().position(|c| {
            c.origin
                .as_ref()
                .is_some_and(|o| o.table == table && o.column.eq_ignore_ascii_case(column))
        })
    };
    result
        .columns
        .iter()
        .map(|c| {
            let origin = c.origin.as_ref()?;
            if origin
                .database
                .as_deref()
                .is_some_and(|d| d != ctx.database)
            {
                return None;
            }
            let key = ctx.primary_key(&origin.table);
            if key.is_empty() {
                return None;
            }
            let key = key
                .iter()
                .map(|k| Some((k.clone(), position(&origin.table, k)?)))
                .collect::<Option<Vec<_>>>()?;
            Some(Origin {
                table: origin.table.clone(),
                column: origin.column.clone(),
                key,
            })
        })
        .collect()
}

/// A saved query's rows as one group; each value is sourced when its table's key is in the
/// result, and a row is keyed by the first sourced value's row.
fn flat_result(ctx: &Ctx<'_>, result: &ResultSet) -> FlatDocument {
    let origins = origins(ctx, result);
    let rows = result
        .rows
        .iter()
        .map(|cells| {
            let fields: Vec<FlatField> = cells
                .iter()
                .zip(&result.columns)
                .zip(&origins)
                .map(|((cell, column), origin)| FlatField {
                    name: column.name.clone(),
                    source: origin.as_ref().and_then(|o| source(o, cells)),
                    cell: cell.clone(),
                })
                .collect();
            let key = fields
                .iter()
                .find_map(|f| f.source.as_ref())
                .map(|s| s.row.clone());
            FlatRow { key, fields }
        })
        .collect();
    let table = origins
        .iter()
        .flatten()
        .next()
        .map_or_else(String::new, |o| o.table.clone());
    FlatDocument {
        groups: vec![FlatGroup {
            id: RESULT_GROUP.into(),
            table,
            shape: GroupShape::Rows,
            capped: result.truncated || result.rows.len() >= QUERY_ROWS as usize,
            rows,
        }],
    }
}

fn source(origin: &Origin, cells: &[Cell]) -> Option<Provenance> {
    let parts = origin
        .key
        .iter()
        .map(|(column, index)| {
            Some(KeyPart {
                column: column.clone(),
                value: cells.get(*index)?.as_text()?.to_string(),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Provenance {
        table: origin.table.clone(),
        column: origin.column.clone(),
        row: RowKey(parts),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ColumnMeta, ColumnOrigin};
    use crate::document::diff::fixtures::{key, number, prov, text};
    use crate::document::testing::{Rig, shop};
    use crate::error::ErrorKind;
    use crate::rules::project::SavedQuery;

    fn column(name: &str, table: &str, origin_column: &str) -> ColumnMeta {
        ColumnMeta {
            name: name.into(),
            type_name: "TEXT".into(),
            origin: Some(ColumnOrigin {
                database: None,
                table: table.into(),
                column: origin_column.into(),
            }),
        }
    }

    async fn rig_with_query(db: crate::db::FakeDatabase, sql: &str) -> (Rig, Address) {
        let rig = Rig::new(db, "");
        let sql = sql.to_string();
        rig.sources()
            .projects
            .update("local", "wp", move |p| {
                p.saved = vec![SavedQuery {
                    id: "q1".into(),
                    title: "q1".into(),
                    sql,
                    pin: None,
                }];
                Ok(())
            })
            .await
            .unwrap();
        let address = Address::new("local", "wp", Target::Query { id: "q1".into() });
        (rig, address)
    }

    #[tokio::test]
    async fn a_saved_query_is_captured_with_its_rows_keyed_by_primary_key() {
        let result = ResultSet {
            columns: vec![
                column("id", "f_orders", "id"),
                column("state", "f_orders", "status"),
                ColumnMeta {
                    name: "two".into(),
                    type_name: "BIGINT".into(),
                    origin: None,
                },
            ],
            rows: vec![
                vec![number("1"), text("paid"), number("2")],
                vec![number("2"), text("open"), number("2")],
            ],
            returns_rows: true,
            ..ResultSet::default()
        };
        let (rig, address) = rig_with_query(
            shop().on("SELECT id, status", result),
            "SELECT id, status AS state, 2 AS two FROM f_orders",
        )
        .await;

        let (document, row_document) = capture(
            &rig.session,
            &address,
            rig.sources(),
            Duration::from_secs(5),
        )
        .await
        .unwrap();

        assert!(row_document.is_none());
        assert_eq!(document.groups.len(), 1);
        let group = &document.groups[0];
        assert_eq!(
            (
                group.id.as_str(),
                group.table.as_str(),
                group.shape,
                group.capped
            ),
            (RESULT_GROUP, "f_orders", GroupShape::Rows, false)
        );
        assert_eq!(group.rows.len(), 2);
        assert_eq!(group.rows[0].key, Some(key("id", "1")));
        assert_eq!(group.rows[1].key, Some(key("id", "2")));
        let fields: Vec<(&str, Option<Provenance>, &Cell)> = group.rows[0]
            .fields
            .iter()
            .map(|f| (f.name.as_str(), f.source.clone(), &f.cell))
            .collect();
        assert_eq!(
            fields,
            [
                ("id", Some(prov("f_orders", "id", "id", "1")), &number("1")),
                (
                    "state",
                    Some(prov("f_orders", "status", "id", "1")),
                    &text("paid")
                ),
                ("two", None, &number("2")),
            ]
        );
    }

    #[tokio::test]
    async fn a_saved_query_that_writes_is_refused_without_running() {
        for sql in [
            "DELETE FROM f_orders",
            "UPDATE f_orders SET status = 'x'",
            "SELECT 1; SELECT 2",
        ] {
            let (rig, address) = rig_with_query(shop(), sql).await;

            let err = capture(
                &rig.session,
                &address,
                rig.sources(),
                Duration::from_secs(5),
            )
            .await
            .unwrap_err();

            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{sql}");
            assert!(
                rig.db.queries().iter().all(|q| !q.contains(sql)),
                "{sql}: {:?}",
                rig.db.queries()
            );
        }
    }

    #[tokio::test]
    async fn an_unknown_saved_query_is_invalid_input() {
        let (rig, _) = rig_with_query(shop(), "SELECT 1").await;
        let address = Address::new("local", "wp", Target::Query { id: "nope".into() });

        let err = capture(
            &rig.session,
            &address,
            rig.sources(),
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }

    #[tokio::test]
    async fn an_address_that_is_not_a_row_or_saved_query_has_no_snapshot() {
        let rig = Rig::new(shop(), "");

        for target in [
            Target::Home,
            Target::Project,
            Target::Find { value: "x".into() },
        ] {
            let address = Address::new("local", "wp", target);

            let err = capture(
                &rig.session,
                &address,
                rig.sources(),
                Duration::from_secs(5),
            )
            .await
            .unwrap_err();

            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{address}");
        }
    }
}
