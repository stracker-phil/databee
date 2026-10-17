//! Addresses: one URL-like name for every view (ADR 0021).
//!
//! ```text
//! databee://<connection>/<database>/                    home
//! databee://<connection>/<database>/<table>             rows of a table
//! databee://<connection>/<database>/<table>?col=val&…   rows with a structured filter (`?col` = IS NULL)
//! databee://<connection>/<database>/<table>/<key>       object (`?view=document|rows`, `?compare=<key2>`)
//! databee://<connection>/<database>/q/<id>              saved query
//! databee://<connection>/<database>/?find=<value>       find everywhere
//! databee://<connection>/<database>/?project            project page
//! ```
//!
//! Parsing and formatting are pure; `resolve` checks an address against a session.

mod percent;
pub mod resolve;

use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::db::ColumnEquals;
use crate::error::{AppError, AppResult};

pub const SCHEME: &str = "databee://";

/// The path segment of saved queries; a table literally named `q` is written `%71`.
const QUERY_SEGMENT: &str = "q";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct Address {
    /// The connection slug (`RecentConnection::slug`).
    pub connection: String,
    pub database: String,
    pub target: Target,
    /// Only on a `Row`; `None` is the default (document).
    pub view: Option<View>,
    /// Only on a `Row`: the key of a second row to compare with.
    pub compare: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Target {
    Home,
    /// Real table name with prefix; `filter` is the structured filter, in order.
    Table {
        table: String,
        filter: Vec<ColumnEquals>,
    },
    /// One row by its single-column primary key value. Composite keys use `Table` with a filter.
    Row {
        table: String,
        key: String,
    },
    Query {
        id: String,
    },
    Find {
        value: String,
    },
    Project,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub enum View {
    Document,
    Rows,
}

impl View {
    fn as_str(self) -> &'static str {
        match self {
            View::Document => "document",
            View::Rows => "rows",
        }
    }

    fn parse(text: &str) -> Option<View> {
        match text {
            "document" => Some(View::Document),
            "rows" => Some(View::Rows),
            _ => None,
        }
    }
}

/// Why a text is not an address; `Address::parse` adds the text itself.
type Reason = String;

impl Address {
    pub fn new(connection: impl Into<String>, database: impl Into<String>, target: Target) -> Self {
        Self {
            connection: connection.into(),
            database: database.into(),
            target,
            view: None,
            compare: None,
        }
    }

    pub fn parse(text: &str) -> AppResult<Address> {
        parse_address(text).map_err(|reason| {
            AppError::InvalidInput(format!("Not a DataBee address ({reason}): {text}"))
        })
    }
}

fn parse_address(text: &str) -> Result<Address, Reason> {
    let rest = text
        .strip_prefix(SCHEME)
        .ok_or("it does not start with databee://")?;
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let params = parse_query(query)?;
    let segments: Vec<&str> = path.split('/').collect();
    let [connection, database, tail @ ..] = segments.as_slice() else {
        return Err("it needs a connection and a database".into());
    };
    let connection = non_empty(connection, "connection")?;
    let database = non_empty(database, "database")?;
    let target = match tail {
        [] | [""] => home_target(params)?,
        [segment, id] if *segment == QUERY_SEGMENT => {
            if !params.is_empty() {
                return Err("a saved query takes no parameters".into());
            }
            Target::Query {
                id: non_empty(id, "query id")?,
            }
        }
        [table] => Target::Table {
            table: non_empty(table, "table")?,
            filter: filter(params),
        },
        [table, key] => {
            let target = Target::Row {
                table: non_empty(table, "table")?,
                key: non_empty(key, "key")?,
            };
            return with_row_params(Address::new(connection, database, target), params);
        }
        _ => return Err("it has too many path segments".into()),
    };
    Ok(Address::new(connection, database, target))
}

type Params = Vec<(String, Option<String>)>;

fn non_empty(raw: &str, part: &str) -> Result<String, Reason> {
    let decoded = percent::decode(raw)?;
    if decoded.is_empty() {
        return Err(format!("the {part} is empty"));
    }
    Ok(decoded)
}

/// `a=1&b` → `[("a", Some("1")), ("b", None)]`; a parameter without `=` has no value.
fn parse_query(query: &str) -> Result<Params, Reason> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    query
        .split('&')
        .map(|pair| {
            let (name, value) = match pair.split_once('=') {
                Some((name, value)) => (name, Some(percent::decode(value)?)),
                None => (pair, None),
            };
            Ok((non_empty(name, "parameter name")?, value))
        })
        .collect()
}

fn filter(params: Params) -> Vec<ColumnEquals> {
    params
        .into_iter()
        .map(|(column, value)| ColumnEquals { column, value })
        .collect()
}

fn home_target(params: Params) -> Result<Target, Reason> {
    match params.as_slice() {
        [] => Ok(Target::Home),
        [(name, Some(value))] if name == "find" => Ok(Target::Find {
            value: value.clone(),
        }),
        [(name, None)] if name == "project" => Ok(Target::Project),
        _ => Err("the database home takes only ?find=<value> or ?project".into()),
    }
}

fn with_row_params(mut address: Address, params: Params) -> Result<Address, Reason> {
    for (name, value) in params {
        match (name.as_str(), value) {
            ("view", Some(view)) if address.view.is_none() => {
                address.view = Some(View::parse(&view).ok_or("view is document or rows")?);
            }
            ("compare", Some(key)) if address.compare.is_none() && !key.is_empty() => {
                address.compare = Some(key)
            }
            _ => return Err(format!("`{name}` is not a row parameter or is repeated")),
        }
    }
    Ok(address)
}

fn table_segment(table: &str) -> String {
    if table == QUERY_SEGMENT {
        return "%71".into();
    }
    percent::encode(table)
}

fn param(name: &str, value: Option<&str>) -> String {
    match value {
        Some(value) => format!("{}={}", percent::encode(name), percent::encode(value)),
        None => percent::encode(name),
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let base = format!(
            "{SCHEME}{}/{}/",
            percent::encode(&self.connection),
            percent::encode(&self.database)
        );
        let (path, params): (String, Vec<String>) = match &self.target {
            Target::Home => (String::new(), Vec::new()),
            Target::Table { table, filter } => (
                table_segment(table),
                filter
                    .iter()
                    .map(|c| param(&c.column, c.value.as_deref()))
                    .collect(),
            ),
            Target::Row { table, key } => {
                let view = self.view.map(|v| param("view", Some(v.as_str())));
                let compare = self
                    .compare
                    .as_deref()
                    .map(|key| param("compare", Some(key)));
                (
                    format!("{}/{}", table_segment(table), percent::encode(key)),
                    view.into_iter().chain(compare).collect(),
                )
            }
            Target::Query { id } => (
                format!("{QUERY_SEGMENT}/{}", percent::encode(id)),
                Vec::new(),
            ),
            Target::Find { value } => (String::new(), vec![param("find", Some(value))]),
            Target::Project => (String::new(), vec![param("project", None)]),
        };
        let query = if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params.join("&"))
        };
        write!(f, "{base}{path}{query}")
    }
}

#[cfg(test)]
pub mod cases;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn table(name: &str) -> Target {
        Target::Table {
            table: name.into(),
            filter: Vec::new(),
        }
    }

    #[test]
    fn canonical_text_parses_to_its_address_and_formats_back() {
        for (text, address) in cases::canonical() {
            assert_eq!(Address::parse(text).unwrap(), address, "{text}");
            assert_eq!(address.to_string(), text);
        }
    }

    #[test]
    fn non_canonical_spellings_parse_to_the_same_address_as_their_canonical_one() {
        for (text, expected) in [
            (
                "databee://wc-pp/db",
                Address::new("wc-pp", "db", Target::Home),
            ),
            (
                "databee://wc-pp/db/q",
                Address::new("wc-pp", "db", table("q")),
            ),
        ] {
            assert_eq!(Address::parse(text).unwrap(), expected, "{text}");
        }
    }

    #[test]
    fn malformed_addresses_are_invalid_input() {
        for text in [
            "wc-pp/db/",
            "http://wc-pp/db/",
            "databee://",
            "databee://wc-pp",
            "databee:///db/",
            "databee://wc-pp//",
            "databee://wc-pp/",
            "databee://wc-pp/db//5",
            "databee://wc-pp/db/wp_posts/",
            "databee://wc-pp/db/q/",
            "databee://wc-pp/db/%zz",
            "databee://wc-pp/db/wp_posts?a=%zz",
            "databee://wc-pp/db/wp_posts/%4",
            "databee://wc-pp/db/%FF",
            "databee://wc-pp/db/wp_posts/1?view=x",
            "databee://wc-pp/db/wp_posts/1?view=rows&view=rows",
            "databee://wc-pp/db/wp_posts/1?view=rows&view=document",
            "databee://wc-pp/db/wp_posts/1?foo=1",
            "databee://wc-pp/db/wp_posts/1?view",
            "databee://wc-pp/db/wp_posts/1?compare=",
            "databee://wc-pp/db/wp_posts/1?compare=2&compare=3",
            "databee://wc-pp/db/wp_posts?=1",
            "databee://wc-pp/db/q/id?x",
            "databee://wc-pp/db/wp_posts/1/2",
            "databee://wc-pp/db/?foo=1",
            "databee://wc-pp/db/?find",
            "databee://wc-pp/db/?project=1",
            "databee://wc-pp/db/?find=a&project",
        ] {
            let err = Address::parse(text).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{text}");
        }
    }

    #[test]
    fn canonical_addresses_serialize_to_the_fixture_the_typescript_port_checks() {
        let cases: Vec<serde_json::Value> = cases::canonical()
            .into_iter()
            .map(|(text, address)| serde_json::json!({"text": text, "address": address}))
            .collect();
        let expected = serde_json::Value::Array(cases);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../src/test/fixtures/addresses.json");
        let current = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());

        if current.as_ref() != Some(&expected) {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let json = serde_json::to_string_pretty(&expected).unwrap();
            std::fs::write(&path, format!("{json}\n")).unwrap();
            panic!("fixture updated, rerun/commit: {}", path.display());
        }
    }
}
