//! Checks an address against an open session: the slug names the session's connection, and the
//! database, table, filter columns and key exist.

use std::time::Duration;

use serde::Serialize;
use ts_rs::TS;

use super::{Address, Target, View};
use crate::db::{BrowseRequest, CellLimit, ColumnEquals};
use crate::error::{AppError, AppResult};
use crate::session::Session;

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct Resolved {
    /// The canonical form: table and column names as the server spells them, the default view dropped.
    pub address: Address,
    /// The database, table, filter columns and row (key and compare key) all exist.
    pub exists: bool,
}

/// An address of another connection (an unknown slug) is an error.
pub async fn resolve(
    session: &Session,
    mut address: Address,
    timeout: Duration,
) -> AppResult<Resolved> {
    let slug = session.slug();
    if address.connection != slug {
        return Err(AppError::InvalidInput(format!(
            "Unknown connection `{}`; this connection is `{slug}`",
            address.connection
        )));
    }
    if address.view == Some(View::Document) {
        address.view = None;
    }
    let exists = database_exists(session, &address.database).await?
        && check_and_canonicalize_target(session, &mut address, timeout).await?;
    Ok(Resolved { address, exists })
}

async fn database_exists(session: &Session, database: &str) -> AppResult<bool> {
    Ok(session
        .db
        .databases()
        .await?
        .iter()
        .any(|d| d.name == database))
}

/// Whether the target exists; replaces its table and column names with the server's spelling on the way.
async fn check_and_canonicalize_target(
    session: &Session,
    address: &mut Address,
    timeout: Duration,
) -> AppResult<bool> {
    let database = address.database.clone();
    match &mut address.target {
        Target::Home | Target::Query { .. } | Target::Find { .. } | Target::Project => Ok(true),
        Target::Table { table, filter } => {
            let Some(real) = real_table(session, &database, table).await? else {
                return Ok(false);
            };
            *table = real;
            check_and_canonicalize_columns(session, &database, table, filter).await
        }
        Target::Row { table, key } => {
            let Some(real) = real_table(session, &database, table).await? else {
                return Ok(false);
            };
            *table = real;
            let primary_key = session.db.structure(&database, table).await?.primary_key;
            let [column] = primary_key.as_slice() else {
                return Ok(false);
            };
            let keys = std::iter::once(key.as_str()).chain(address.compare.as_deref());
            for row_key in keys {
                if !row_exists(session, &database, table, column, row_key, timeout).await? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
    }
}

/// The exact name, else the single case-insensitive match (table names may be case-insensitive).
async fn real_table(session: &Session, database: &str, table: &str) -> AppResult<Option<String>> {
    let tables = session.db.tables(database).await?;
    if tables.iter().any(|t| t.name == table) {
        return Ok(Some(table.to_string()));
    }
    let mut similar = tables
        .into_iter()
        .filter(|t| t.name.eq_ignore_ascii_case(table));
    Ok(match (similar.next(), similar.next()) {
        (Some(only), None) => Some(only.name),
        _ => None,
    })
}

/// Whether every filter column exists. MySQL column names are case-insensitive; the filter takes the
/// schema's spelling.
async fn check_and_canonicalize_columns(
    session: &Session,
    database: &str,
    table: &str,
    filter: &mut [ColumnEquals],
) -> AppResult<bool> {
    if filter.is_empty() {
        return Ok(true);
    }
    let columns = session.db.structure(database, table).await?.columns;
    for condition in filter {
        let Some(real) = columns
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&condition.column))
        else {
            return Ok(false);
        };
        condition.column = real.name.clone();
    }
    Ok(true)
}

async fn row_exists(
    session: &Session,
    database: &str,
    table: &str,
    column: &str,
    key: &str,
    timeout: Duration,
) -> AppResult<bool> {
    let request = BrowseRequest {
        database: database.to_string(),
        table: table.to_string(),
        limit: 1,
        r#where: vec![ColumnEquals {
            column: column.to_string(),
            value: Some(key.to_string()),
        }],
        ..BrowseRequest::default()
    };
    // Only the row count matters, so cells are cut short.
    let page = session.db.browse(&request, CellLimit(1), timeout).await?;
    Ok(!page.result.rows.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::{ConnectionSpec, Secret};
    use crate::db::mysql::MySqlSession;
    use crate::error::ErrorKind;

    const TIMEOUT: Duration = Duration::from_secs(5);
    const PORTS: [u16; 2] = [33306, 33307];

    async fn session(port: u16) -> Session {
        let spec = ConnectionSpec {
            name: None,
            host: "127.0.0.1".into(),
            port,
            socket: None,
            user: "root".into(),
            database: Some("wp_test".into()),
        };
        let db = MySqlSession::connect(&spec, Some(&Secret::new("root")), TIMEOUT, TIMEOUT)
            .await
            .unwrap();
        Session::new("it".into(), spec, format!("it-{port}"), Box::new(db))
    }

    async fn resolved(session: &Session, rest: &str) -> AppResult<Resolved> {
        let text = format!("databee://{}/{rest}", session.slug());
        resolve(session, Address::parse(&text).unwrap(), TIMEOUT).await
    }

    #[tokio::test]
    #[ignore]
    async fn existing_targets_resolve_to_their_canonical_form_on_the_fixture() {
        for port in PORTS {
            let session = session(port).await;
            let slug = session.slug();
            for (given, canonical) in [
                ("wp_test/", "wp_test/"),
                ("wp_test/wp_users", "wp_test/wp_users"),
                ("wp_test/WP_USERS", "wp_test/wp_users"),
                ("wp_test/wp_users?id=1", "wp_test/wp_users?ID=1"),
                ("wp_test/wp_users/12", "wp_test/wp_users/12"),
                (
                    "wp_test/wp_users/1?compare=12",
                    "wp_test/wp_users/1?compare=12",
                ),
                (
                    "wp_test/wp_users/1?view=rows",
                    "wp_test/wp_users/1?view=rows",
                ),
                ("wp_test/wp_users/1?view=document", "wp_test/wp_users/1"),
                ("wp_test/q/anything", "wp_test/q/anything"),
                ("wp_test/?find=x", "wp_test/?find=x"),
                ("wp_test/?project", "wp_test/?project"),
            ] {
                let result = resolved(&session, given).await.unwrap();
                assert!(result.exists, "{port}: {given}");
                assert_eq!(
                    result.address.to_string(),
                    format!("databee://{slug}/{canonical}"),
                    "{port}: {given}"
                );
            }
        }
    }

    #[tokio::test]
    #[ignore]
    async fn missing_database_table_column_or_row_does_not_exist_on_the_fixture() {
        for port in PORTS {
            let session = session(port).await;
            for given in [
                "no_such_db/",
                "wp_test/no_such_table",
                "wp_test/no_such_table/1",
                "wp_test/wp_users?no_such_column=1",
                "wp_test/wp_users/999",
                "wp_test/wp_users/1?compare=999",
                "wp_test/wp_term_relationships/1",
            ] {
                let result = resolved(&session, given).await.unwrap();
                assert!(!result.exists, "{port}: {given}");
            }
        }
    }

    #[tokio::test]
    #[ignore]
    async fn address_of_another_connection_is_an_invalid_input_error_on_the_fixture() {
        for port in PORTS {
            let session = session(port).await;
            let address = Address::parse("databee://other/wp_test/wp_users").unwrap();

            let err = resolve(&session, address, TIMEOUT).await.unwrap_err();

            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{port}");
        }
    }
}
