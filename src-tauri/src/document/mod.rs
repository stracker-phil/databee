//! Documents: one row with everything the rules and the schema link to it (docs/architecture.md,
//! "Documents"). Built only from the effective rules and the schema, for any table with a
//! primary key: the own row, its meta rows, incoming references, pattern groups and `events`.
//!
//! The number of queries is bounded by the rules, never by the rows: one per group (two when it
//! is capped), one per pivoted group, one per referenced table for `exists`.

mod build;
pub mod commands;
mod context;
mod events;
mod exists;
mod incoming;
mod meta;
mod own;
mod patterns;
mod pivot;
#[cfg(test)]
mod server_tests;
mod shorthand;

use std::time::Duration;

use serde::Serialize;
use ts_rs::TS;

use crate::address::{Address, SCHEME};
use crate::error::{AppError, AppResult};
use crate::lens::ResolvedReference;
use crate::rules::effective::RuleSources;
use crate::session::Session;
use crate::value::{RowKey, Value};

/// Meta rows shown before the group is capped.
pub const META_CAP: usize = 50;
/// Rows of an incoming or pattern group shown before it is capped.
pub const ROWS_CAP: usize = 20;

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct Document {
    /// The row's canonical address.
    pub address: Address,
    /// The first entity of the table whose `filter` holds on this row.
    pub entity: Option<String>,
    /// The entity's `summary`: columns of the row or keys of its meta rows.
    pub header: Vec<(String, Value)>,
    /// Own, meta, incoming references, patterns, events; in that order.
    pub groups: Vec<Group>,
    /// Other rows an entity shorthand (`order 4711`) matched, in other tables.
    pub alternatives: Vec<Address>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct Group {
    /// Unique within the document.
    pub id: String,
    /// The real table the rows come from; empty for `events`, whose values name their own.
    pub table: String,
    pub via: GroupLink,
    pub shape: GroupShape,
    /// Column names, or meta keys pivoted in from the source's meta table.
    pub columns: Vec<String>,
    /// All matching rows, also beyond the cap.
    #[ts(type = "number")]
    pub total: u64,
    pub rows: Vec<GroupRow>,
    pub capped: bool,
    /// The group's rows as an address, when an `=` filter selects exactly them.
    pub rows_address: Option<Address>,
    /// The group's query failed (a timeout, broken rule SQL); the rest of the document stands.
    pub error: Option<String>,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub enum GroupShape {
    /// One row, column by column.
    Fields,
    /// Key/value rows.
    Meta,
    /// A table of rows.
    Rows,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GroupLink {
    Own,
    /// Rows of a `[[meta]]` table whose `fk` holds the row's key. Adding a key inserts
    /// `(fk, key, value)`.
    Meta {
        fk: String,
        key: String,
        value: String,
    },
    /// Rows whose reference points at this row.
    Incoming {
        reference: ResolvedReference,
        /// The rule's `label`.
        label: Option<String>,
    },
    /// Rows a `[[pattern]]` rule selects; `sql` is the condition as run.
    Pattern {
        rule: String,
        sql: String,
    },
    /// Every timestamp of the document, sorted; each row is one value with its provenance.
    Events,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct GroupRow {
    /// `None` for a table without a primary key.
    pub key: Option<RowKey>,
    pub fields: Vec<(String, Value)>,
}

/// The document of `text`: a row address (`databee://…/<table>/<key>`), or an entity shorthand
/// (`order 4711`) in `database`, the session's current one by default.
pub async fn open(
    session: &Session,
    text: &str,
    database: Option<String>,
    sources: RuleSources<'_>,
    timeout: Duration,
) -> AppResult<Document> {
    if text.starts_with(SCHEME) {
        let address = Address::parse(text)?;
        if address.connection != session.slug() {
            return Err(AppError::InvalidInput(format!(
                "Unknown connection `{}`; this connection is `{}`",
                address.connection,
                session.slug()
            )));
        }
        let ctx = context::Ctx::load(session, &address.database, sources, timeout).await?;
        return build::document(&ctx, &address).await;
    }
    let (entity, key) = shorthand::parse(text).ok_or_else(|| {
        AppError::InvalidInput(format!("Not an address or `<entity> <key>`: {text}"))
    })?;
    let database = database
        .or_else(|| session.db.current_database())
        .ok_or_else(|| AppError::InvalidInput("No database selected".into()))?;
    let ctx = context::Ctx::load(session, &database, sources, timeout).await?;
    let named = shorthand::resolve(&ctx, entity, key).await?;
    let mut document = build::document(&ctx, &named.address).await?;
    document.alternatives = named.alternatives;
    Ok(document)
}

impl Group {
    fn new(id: String, table: &str, via: GroupLink, shape: GroupShape) -> Self {
        Self {
            id,
            table: table.to_string(),
            via,
            shape,
            columns: Vec::new(),
            total: 0,
            rows: Vec::new(),
            capped: false,
            rows_address: None,
            error: None,
        }
    }

    fn failed(mut self, error: impl ToString) -> Self {
        self.error = Some(error.to_string());
        self
    }
}
