//! What a snapshot stores of an address: a row's document, or the rows of a saved query keyed by
//! their primary key, both as a `FlatDocument`.

use std::time::Duration;

use serde::Serialize;
use ts_rs::TS;

use super::diff::{DocDelta, FlatDocument, FlatField, FlatGroup, FlatRow, diff_documents};
use super::{Ctx, Document, GroupShape, parse_address};
use crate::address::{Address, Target};
use crate::db::classify::is_read_only;
use crate::db::{Cell, ResultSet};
use crate::error::{AppError, AppResult};
use crate::readout::sql::{in_database, limited, single_statement};
use crate::rules::annotate::annotate_query;
use crate::rules::effective::RuleSources;
use crate::session::Session;
use crate::store::snapshots::{SnapshotInfo, SnapshotStore};
use crate::value::{KeyPart, Provenance, RowKey};

/// Rows of a saved query a snapshot keeps.
pub const QUERY_ROWS: u32 = 1000;
/// The one group of a saved query's snapshot.
pub const RESULT_GROUP: &str = "result";

/// What changed since snapshot `id`: its address read now against the stored cells.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDiff {
    pub snapshot: SnapshotInfo,
    pub delta: DocDelta,
    /// The row's document now; `None` for a saved query.
    pub document: Option<Document>,
}

/// Stores the row or saved query at `text` as it is now, under the trimmed `label`.
pub async fn take(
    session: &Session,
    store: &SnapshotStore,
    text: &str,
    label: &str,
    sources: RuleSources<'_>,
    timeout: Duration,
) -> AppResult<SnapshotInfo> {
    let label = label.trim();
    if label.is_empty() {
        return Err(AppError::InvalidInput("A snapshot needs a label".into()));
    }
    let address = address(session, text)?;
    let (cells, _) = capture(session, &address, sources, timeout).await?;
    let created = chrono::Utc::now().timestamp();
    store.add(label.to_string(), &address, created, cells).await
}

/// The snapshots of `database` in this connection, newest first; only those of `text` when given.
pub fn list(
    session: &Session,
    store: &SnapshotStore,
    database: &str,
    text: Option<&str>,
) -> AppResult<Vec<SnapshotInfo>> {
    let address = text
        .map(|text| address(session, text).map(|a| a.to_string()))
        .transpose()?;
    Ok(store.list(&session.slug(), database, address.as_deref()))
}

/// Snapshot `id` against its address read now.
pub async fn diff(
    session: &Session,
    store: &SnapshotStore,
    id: &str,
    sources: RuleSources<'_>,
    timeout: Duration,
) -> AppResult<SnapshotDiff> {
    let (snapshot, stored) = store.get(id).await?;
    let address = address(session, &snapshot.address)?;
    let (now, document) = capture(session, &address, sources, timeout).await?;
    let delta = diff_documents(&stored, &now);
    Ok(SnapshotDiff {
        snapshot,
        delta,
        document,
    })
}

/// A row or saved-query address of this session's connection, without view or compare.
fn address(session: &Session, text: &str) -> AppResult<Address> {
    let address = parse_address(session, text)?;
    Ok(Address {
        view: None,
        compare: None,
        ..address
    })
}

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
    let mut result = ctx.fetch(&sql).await?;
    annotate_query(
        ctx.session,
        &ctx.database,
        ctx.lens.clone(),
        &mut result,
        Some(sources),
    )
    .await;
    Ok(result)
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
    use crate::error::ErrorKind;
    use crate::query::testing::{Rig, result, shop};
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

    fn order_rig() -> Rig {
        let order = |id: &str, status: &str| {
            result(
                &["id", "status", "total", "currency", "created_gmt"],
                vec![vec![
                    number(id),
                    text(status),
                    text("10.00"),
                    text("EUR"),
                    Cell::Temporal("2023-11-14 22:13:20".into()),
                ]],
            )
        };
        Rig::new(
            shop()
                .on(
                    "`f_ordermeta`",
                    result(&["meta_id", "order_id", "k", "v"], Vec::new()),
                )
                .on("= '1' LIMIT", order("1", "paid"))
                .on("= '2' LIMIT", order("2", "open")),
            "",
        )
    }

    async fn take_order(
        rig: &Rig,
        store: &SnapshotStore,
        text: &str,
        label: &str,
    ) -> AppResult<SnapshotInfo> {
        take(
            &rig.session,
            store,
            text,
            label,
            rig.sources(),
            Duration::from_secs(5),
        )
        .await
    }

    #[tokio::test]
    async fn a_blank_label_is_invalid_input_and_stores_nothing() {
        let rig = order_rig();
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());

        for label in ["", "   "] {
            let err = take_order(&rig, &store, "databee://local/wp/f_orders/1", label)
                .await
                .unwrap_err();

            assert!(
                matches!(&err, AppError::InvalidInput(m) if m == "A snapshot needs a label"),
                "{err:?}"
            );
        }
        assert_eq!(list(&rig.session, &store, "wp", None).unwrap(), []);
        assert_eq!(rig.db.queries(), Vec::<String>::new());
    }

    #[tokio::test]
    async fn a_snapshot_is_stored_under_its_trimmed_label_at_the_plain_row_address() {
        let rig = order_rig();
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());

        let info = take_order(
            &rig,
            &store,
            "databee://local/wp/f_orders/1?view=document&compare=2",
            "  before  ",
        )
        .await
        .unwrap();

        assert_eq!(
            (
                info.label.as_str(),
                info.connection.as_str(),
                info.database.as_str()
            ),
            ("before", "local", "wp")
        );
        assert_eq!(info.address, "databee://local/wp/f_orders/1");
        assert_eq!(list(&rig.session, &store, "wp", None).unwrap(), [info]);
    }

    #[tokio::test]
    async fn listing_by_address_ignores_its_view_and_compare() {
        let rig = order_rig();
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());
        let first = take_order(&rig, &store, "databee://local/wp/f_orders/1", "one")
            .await
            .unwrap();
        take_order(&rig, &store, "databee://local/wp/f_orders/2", "two")
            .await
            .unwrap();

        let listed = list(
            &rig.session,
            &store,
            "wp",
            Some("databee://local/wp/f_orders/1?view=rows&compare=2"),
        )
        .unwrap();

        assert_eq!(listed, [first]);
        assert_eq!(list(&rig.session, &store, "other", None).unwrap(), []);
    }

    #[tokio::test]
    async fn listing_by_an_address_of_another_connection_is_invalid_input() {
        let rig = order_rig();
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());

        let err = list(
            &rig.session,
            &store,
            "wp",
            Some("databee://other/wp/f_orders/1"),
        )
        .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }
}
