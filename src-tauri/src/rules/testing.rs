//! Builders shared by the tests of the rules modules, and an invented "forum" preset that
//! exercises every rule type without naming any real application.

use std::sync::Arc;

use super::preset;
use super::project::Project;
use super::types::{ColumnRef, Conditions, Preset, Reference};
use crate::db::{ForeignKey, TableInfo, TableKeys};
use crate::schema::{DatabaseSchema, IncomingKey};

/// `(table, columns, primary key)`.
pub type TableSpec<'a> = (&'a str, &'a [&'a str], &'a [&'a str]);

pub fn schema(tables: &[TableSpec<'_>]) -> DatabaseSchema {
    let mut schema = DatabaseSchema {
        tables: Vec::new(),
        keys: Default::default(),
        columns: Default::default(),
        incoming: Default::default(),
    };
    for (name, columns, primary_key) in tables {
        schema.tables.push(TableInfo {
            name: name.to_string(),
            is_view: false,
            rows_estimate: None,
            size_bytes: None,
            comment: String::new(),
        });
        schema.keys.insert(
            name.to_string(),
            TableKeys {
                primary_key: primary_key.iter().map(|c| c.to_string()).collect(),
                ..TableKeys::default()
            },
        );
        schema.columns.insert(
            name.to_string(),
            columns.iter().map(|c| c.to_string()).collect(),
        );
    }
    schema
}

/// Tables with neither columns nor keys.
pub fn tables(names: &[&str]) -> DatabaseSchema {
    let specs: Vec<TableSpec<'_>> = names.iter().map(|n| (*n, &[][..], &[][..])).collect();
    schema(&specs)
}

pub fn with_foreign_key(mut schema: DatabaseSchema, table: &str, fk: ForeignKey) -> DatabaseSchema {
    schema
        .incoming
        .entry(fk.ref_table.clone())
        .or_default()
        .push(IncomingKey {
            table: table.to_string(),
            foreign_key: fk.clone(),
        });
    schema
        .keys
        .entry(table.to_string())
        .or_default()
        .foreign_keys
        .push(fk);
    schema
}

pub fn preset(src: &str) -> Arc<Preset> {
    Arc::new(preset::parse("test.toml", src).unwrap_or_else(|e| panic!("{e}")))
}

pub fn project() -> Project {
    Project::new("local", "wp")
}

pub fn column(text: &str) -> ColumnRef {
    ColumnRef::try_from(text.to_string()).unwrap()
}

pub fn reference(id: &str, from: &str, to: &str) -> Reference {
    Reference {
        id: id.into(),
        from: column(from),
        when: Conditions::new(),
        to: column(to),
        to_when: Conditions::new(),
        label: None,
        columns: Vec::new(),
    }
}

/// The tables the forum preset describes, under the prefix `f_`.
pub fn forum_schema() -> DatabaseSchema {
    schema(&[
        (
            "f_threads",
            &[
                "thread_id",
                "person_id",
                "author",
                "parent_id",
                "title",
                "status",
                "opened_gmt",
                "closed_local",
            ],
            &["thread_id"],
        ),
        (
            "f_people",
            &["person_id", "name", "secret_code", "joined_gmt"],
            &["person_id"],
        ),
        ("f_notes", &["note_id", "thread_id", "k", "v"], &["note_id"]),
        (
            "f_settings",
            &["setting_id", "name", "val"],
            &["setting_id"],
        ),
        ("f_replies", &["reply_id", "by", "thread_id"], &["reply_id"]),
        ("f_bots", &["bot_id", "label"], &["bot_id"]),
        (
            "f_invoices",
            &["invoice_id", "amount", "currency"],
            &["invoice_id"],
        ),
        ("f_sessions", &["session_id", "skey"], &["session_id"]),
    ])
}

pub fn forum() -> Arc<Preset> {
    preset(FORUM)
}

pub const FORUM: &str = r#"
[preset]
name = "forum"
marker_tables = ["threads", "people"]

[[meta]]
table = "notes"
owner = "threads"
fk = "thread_id"
key = "k"
value = "v"

[[meta]]
table = "settings"
key = "name"
value = "val"

[[reference]]
from = "threads.author"
to = "people.person_id"

[[reference]]
from = "notes.v"
when = { k = "_ref" }
to = "threads.thread_id"

[[reference]]
from = "notes.v"
when = { k = { glob = "_who*" } }
to = "people.person_id"

[[reference]]
from = "notes.v"
when = { k = "_ext" }
to = "notes.v"
to_when = { k = "_ext" }

[[reference]]
from = "replies.by"
to = "people.person_id"

[[reference]]
from = "replies.by"
to = "bots.bot_id"

[[reference]]
from = "sessions.skey"
when = { skey = { regexp = "^[0-9]+$" } }
to = "people.person_id"

[[entity]]
name = "thread"
table = "threads"

[[entity]]
name = "announcement"
table = "threads"
filter = "status = 'pinned'"

[[entity]]
name = "person"
table = "people"

[[entity]]
name = "ghost"
table = "ghosts"

[[kind]]
columns = ["people.secret_code"]
secret = true

[[kind]]
columns = ["*.*_gmt"]
timestamp = "utc"

[[kind]]
columns = ["threads.closed_local"]
timestamp = "local"

[[kind]]
columns = ["threads.status"]
enum = true

[[kind]]
columns = ["invoices.amount"]
money = { currency = ["currency"] }

[[kind]]
meta_keys = ["api_*"]
secret = true

[[kind]]
meta_keys = ["expires_*"]
in = ["settings"]
timestamp = "unix"

[[kind]]
meta_keys = ["tone"]
in = ["settings"]
enum = true

[[style]]
columns = ["threads.status"]
match = "="
value = "closed"
color = "red"

[[style]]
meta_keys = ["tone"]
in = ["settings"]
match = "LIKE"
value = "wa%"
color = "green"

[[style]]
columns = ["threads.title"]
match = "="
value = "draft"
color = "blue"
enabled = false
"#;
