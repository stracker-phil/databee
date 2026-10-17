//! The lens the schema alone gives: primary keys, declared foreign keys and same-name inference.

use std::collections::HashMap;
use std::sync::Arc;

use super::{ColumnsRef, Lens, ResolvedReference};
use crate::address::{Address, Target};
use crate::db::{ColumnMeta, ForeignKey};
use crate::schema::DatabaseSchema;
use crate::value::{self, ColumnKind, ReferenceSource, ReferenceTarget};

pub struct SchemaLens {
    schema: Arc<DatabaseSchema>,
    connection: String,
    database: String,
    /// Tables with a single-column primary key, by the key column's lowercase name: `(table, key
    /// column)`. Column names compare ASCII case-insensitively throughout, as MySQL does; table
    /// names compare exactly.
    by_key: HashMap<String, Vec<(String, String)>>,
}

impl SchemaLens {
    /// `connection` is the session's slug, for the addresses the kinds carry.
    pub fn new(schema: Arc<DatabaseSchema>, connection: &str, database: &str) -> Self {
        let mut by_key = HashMap::<String, Vec<(String, String)>>::new();
        for (table, keys) in &schema.keys {
            if let [column] = keys.primary_key.as_slice() {
                by_key
                    .entry(column.to_ascii_lowercase())
                    .or_default()
                    .push((table.clone(), column.clone()));
            }
        }
        Self {
            schema,
            connection: connection.to_string(),
            database: database.to_string(),
            by_key,
        }
    }

    fn table_address(&self, table: &str) -> Address {
        let target = Target::Table {
            table: table.to_string(),
            filter: Vec::new(),
        };
        Address::new(self.connection.clone(), self.database.clone(), target)
    }

    fn primary_key(&self, table: &str) -> &[String] {
        self.schema
            .keys(table)
            .map_or(&[], |k| k.primary_key.as_slice())
    }

    fn single_key(&self, table: &str) -> Option<&str> {
        match self.primary_key(table) {
            [column] => Some(column),
            _ => None,
        }
    }

    fn foreign_keys(&self, table: &str) -> &[ForeignKey] {
        self.schema
            .keys(table)
            .map_or(&[], |k| k.foreign_keys.as_slice())
    }

    /// The tables (with their key column) that `table.column` points at by same-name inference.
    fn inferred_targets(&self, table: &str, column: &str) -> Vec<(&str, &str)> {
        if column.eq_ignore_ascii_case("id") {
            return Vec::new();
        }
        let own_key = self
            .single_key(table)
            .is_some_and(|key| key.eq_ignore_ascii_case(column));
        if own_key || self.has_declared_foreign_key(table, column) {
            return Vec::new();
        }
        let Some(tables) = self.by_key.get(&column.to_ascii_lowercase()) else {
            return Vec::new();
        };
        tables
            .iter()
            .filter(|(target, _)| target != table)
            .map(|(target, key)| (target.as_str(), key.as_str()))
            .collect()
    }

    fn has_declared_foreign_key(&self, table: &str, column: &str) -> bool {
        self.foreign_keys(table)
            .iter()
            .any(|fk| fk.columns.iter().any(|c| c.eq_ignore_ascii_case(column)))
    }

    /// Whether `table` has an index that starts with `column`, so following it is cheap.
    fn indexed(&self, table: &str, column: &str) -> bool {
        let Some(keys) = self.schema.keys(table) else {
            return false;
        };
        keys.indexes.iter().any(|i| {
            i.columns
                .first()
                .is_some_and(|c| c.eq_ignore_ascii_case(column))
        })
    }

    fn same_name(&self, table: &str, column: &str) -> ReferenceSource {
        ReferenceSource::SameName {
            column: column.to_string(),
            indexed: self.indexed(table, column),
        }
    }

    /// A reference kind from single-column foreign keys, else from inference. Composite foreign
    /// keys need the other columns' values, so they show in `references_from` only.
    fn reference_kind(&self, table: &str, column: &str) -> Option<ColumnKind> {
        let declared: Vec<&ForeignKey> = self
            .foreign_keys(table)
            .iter()
            .filter(|fk| matches!(fk.columns.as_slice(), [c] if c.eq_ignore_ascii_case(column)))
            .collect();
        if let Some(first) = declared.first() {
            let targets = declared
                .iter()
                .filter_map(|fk| Some(self.target(&fk.ref_table, fk.ref_columns.first()?)))
                .collect();
            let by = ReferenceSource::ForeignKey {
                constraint: first.name.clone(),
            };
            return Some(ColumnKind::Reference { targets, by });
        }
        let inferred = self.inferred_targets(table, column);
        if inferred.is_empty() {
            return None;
        }
        let targets = inferred
            .iter()
            .map(|(target, key)| self.target(target, key))
            .collect();
        let by = self.same_name(table, column);
        Some(ColumnKind::Reference { targets, by })
    }

    fn target(&self, table: &str, column: &str) -> ReferenceTarget {
        ReferenceTarget {
            table: self.table_address(table),
            column: column.to_string(),
            key: self
                .single_key(table)
                .is_some_and(|key| key.eq_ignore_ascii_case(column)),
        }
    }

    /// A single-column primary key is the row's identity; a member of a composite key that is
    /// also a reference (`wp_term_relationships.term_taxonomy_id`) is shown as the reference.
    fn column_kind(&self, table: &str, column: &ColumnMeta) -> ColumnKind {
        let is_single_key = self
            .single_key(table)
            .is_some_and(|key| key.eq_ignore_ascii_case(&column.name));
        if is_single_key {
            return self.primary_key_kind(table);
        }
        if let Some(reference) = self.reference_kind(table, &column.name) {
            return reference;
        }
        let in_key = self
            .primary_key(table)
            .iter()
            .any(|k| k.eq_ignore_ascii_case(&column.name));
        if in_key {
            return self.primary_key_kind(table);
        }
        value::type_kind(&column.type_name)
    }

    fn primary_key_kind(&self, table: &str) -> ColumnKind {
        ColumnKind::PrimaryKey {
            table: self.table_address(table),
            columns: self.primary_key(table).to_vec(),
        }
    }

    fn declared_reference(table: &str, fk: &ForeignKey) -> ResolvedReference {
        ResolvedReference {
            from: ColumnsRef::new(table, fk.columns.clone()),
            to: ColumnsRef::new(&fk.ref_table, fk.ref_columns.clone()),
            by: ReferenceSource::ForeignKey {
                constraint: fk.name.clone(),
            },
        }
    }

    fn inferred_from(&self, table: &str, column: &str) -> Vec<ResolvedReference> {
        self.inferred_targets(table, column)
            .into_iter()
            .map(|(target, key)| ResolvedReference {
                from: ColumnsRef::new(table, vec![column.to_string()]),
                to: ColumnsRef::new(target, vec![key.to_string()]),
                by: self.same_name(table, column),
            })
            .collect()
    }
}

impl Lens for SchemaLens {
    fn kinds(&self, table: &str, columns: &[ColumnMeta]) -> Vec<ColumnKind> {
        columns.iter().map(|c| self.column_kind(table, c)).collect()
    }

    fn references_from(&self, table: &str) -> Vec<ResolvedReference> {
        let declared = self
            .foreign_keys(table)
            .iter()
            .map(|fk| Self::declared_reference(table, fk));
        let inferred = self
            .schema
            .columns(table)
            .iter()
            .flat_map(|column| self.inferred_from(table, column));
        declared.chain(inferred).collect()
    }

    fn references_to(&self, table: &str) -> Vec<ResolvedReference> {
        let declared = self
            .schema
            .incoming(table)
            .iter()
            .map(|incoming| Self::declared_reference(&incoming.table, &incoming.foreign_key));
        let mut references: Vec<ResolvedReference> = declared.collect();
        let Some(key) = self.single_key(table) else {
            return references;
        };
        for (source, columns) in &self.schema.columns {
            for column in columns.iter().filter(|c| c.eq_ignore_ascii_case(key)) {
                let here = self.inferred_from(source, column);
                references.extend(here.into_iter().filter(|r| r.to.table == table));
            }
        }
        references
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::ConnectionSpec;
    use crate::db::{ColumnInfo, FakeDatabase, IndexInfo, TableStructure};
    use crate::schema;
    use crate::session::Session;

    fn column(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            column_type: "bigint".into(),
            nullable: false,
            default: None,
            key: String::new(),
            extra: String::new(),
            comment: String::new(),
            auto_increment: false,
            generated: false,
        }
    }

    fn table(columns: &[&str], primary_key: &[&str]) -> TableStructure {
        TableStructure {
            columns: columns.iter().map(|c| column(c)).collect(),
            indexes: Vec::new(),
            primary_key: primary_key.iter().map(ToString::to_string).collect(),
            foreign_keys: Vec::new(),
            ddl: None,
        }
    }

    fn index(name: &str, columns: &[&str]) -> IndexInfo {
        IndexInfo {
            name: name.into(),
            unique: false,
            columns: columns.iter().map(ToString::to_string).collect(),
        }
    }

    fn meta(name: &str) -> ColumnMeta {
        ColumnMeta {
            name: name.into(),
            type_name: "BIGINT".into(),
            origin: None,
        }
    }

    async fn lens_of(tables: Vec<(&str, TableStructure)>) -> SchemaLens {
        let mut db = FakeDatabase::new().with_database("wp");
        for (name, structure) in tables {
            db = db.with_table("wp", name, structure);
        }
        let spec = ConnectionSpec {
            name: None,
            host: "127.0.0.1".into(),
            port: 3306,
            socket: None,
            user: "root".into(),
            database: Some("wp".into()),
        };
        let session = Session::new("s".into(), spec, "local".into(), Box::new(db));
        let schema = schema::database(&session, "wp").await.unwrap();
        SchemaLens::new(schema, "local", "wp")
    }

    fn address(table: &str) -> Address {
        let target = Target::Table {
            table: table.into(),
            filter: Vec::new(),
        };
        Address::new("local", "wp", target)
    }

    fn target(table: &str, column: &str, key: bool) -> ReferenceTarget {
        ReferenceTarget {
            table: address(table),
            column: column.into(),
            key,
        }
    }

    fn same_name(column: &str, indexed: bool) -> ReferenceSource {
        ReferenceSource::SameName {
            column: column.into(),
            indexed,
        }
    }

    fn reference(targets: Vec<ReferenceTarget>, by: ReferenceSource) -> ColumnKind {
        ColumnKind::Reference { targets, by }
    }

    fn resolved(from: (&str, &str), to: (&str, &str), by: ReferenceSource) -> ResolvedReference {
        ResolvedReference {
            from: ColumnsRef::new(from.0, vec![from.1.into()]),
            to: ColumnsRef::new(to.0, vec![to.1.into()]),
            by,
        }
    }

    fn fk(name: &str, column: &str, ref_table: &str) -> ForeignKey {
        ForeignKey {
            name: name.into(),
            columns: vec![column.into()],
            ref_table: ref_table.into(),
            ref_columns: vec!["id".into()],
        }
    }

    fn action_tables(logs_indexes: Vec<IndexInfo>) -> Vec<(&'static str, TableStructure)> {
        vec![
            (
                "wp_actionscheduler_actions",
                table(&["action_id", "hook"], &["action_id"]),
            ),
            (
                "wp_actionscheduler_logs",
                TableStructure {
                    indexes: logs_indexes,
                    ..table(&["log_id", "action_id", "message"], &["log_id"])
                },
            ),
        ]
    }

    #[tokio::test]
    async fn column_named_like_another_tables_primary_key_is_a_same_name_reference() {
        let indexes = vec![index("action_id", &["action_id"])];
        let lens = lens_of(action_tables(indexes)).await;

        let kinds = lens.kinds("wp_actionscheduler_logs", &[meta("action_id")]);

        assert_eq!(
            kinds,
            [reference(
                vec![target("wp_actionscheduler_actions", "action_id", true)],
                same_name("action_id", true)
            )]
        );
        assert_eq!(
            lens.references_from("wp_actionscheduler_logs"),
            [resolved(
                ("wp_actionscheduler_logs", "action_id"),
                ("wp_actionscheduler_actions", "action_id"),
                same_name("action_id", true)
            )]
        );
    }

    #[tokio::test]
    async fn columns_that_are_not_references_keep_their_kind() {
        let lens = lens_of(vec![
            ("wp_users", table(&["ID", "user_login"], &["ID"])),
            ("plain", table(&["plain_id", "id"], &["plain_id"])),
            ("upper", table(&["upper_id", "ID"], &["upper_id"])),
            ("actions", table(&["action_id"], &["action_id"])),
            ("archive", table(&["action_id"], &["action_id"])),
        ])
        .await;
        let own_key = ColumnKind::PrimaryKey {
            table: address("actions"),
            columns: vec!["action_id".into()],
        };

        for (table, name, expected) in [
            ("plain", "id", ColumnKind::Number),
            ("upper", "ID", ColumnKind::Number),
            ("actions", "action_id", own_key),
        ] {
            assert_eq!(
                lens.kinds(table, &[meta(name)]),
                [expected],
                "{table}.{name}"
            );
        }
        assert!(lens.references_from("actions").is_empty());
        assert!(lens.references_from("plain").is_empty());
    }

    #[tokio::test]
    async fn declared_foreign_key_wins_over_same_name_inference() {
        let books = TableStructure {
            foreign_keys: vec![fk("books_author", "author_id", "authors")],
            ..table(&["id", "author_id"], &["id"])
        };
        let lens = lens_of(vec![
            ("authors", table(&["id", "name"], &["id"])),
            ("writers", table(&["author_id"], &["author_id"])),
            ("books", books),
        ])
        .await;
        let by = ReferenceSource::ForeignKey {
            constraint: "books_author".into(),
        };

        let kinds = lens.kinds("books", &[meta("author_id")]);

        assert_eq!(
            kinds,
            [reference(vec![target("authors", "id", true)], by.clone())]
        );
        assert_eq!(
            lens.references_from("books"),
            [resolved(("books", "author_id"), ("authors", "id"), by)]
        );
    }

    #[tokio::test]
    async fn same_name_match_ignores_letter_case() {
        let lens = lens_of(vec![
            (
                "wp_comments",
                table(&["comment_ID", "comment_content"], &["comment_ID"]),
            ),
            (
                "wp_commentmeta",
                table(&["meta_id", "comment_id"], &["meta_id"]),
            ),
        ])
        .await;

        let kinds = lens.kinds("wp_commentmeta", &[meta("comment_id")]);

        assert_eq!(
            kinds,
            [reference(
                vec![target("wp_comments", "comment_ID", true)],
                same_name("comment_id", false)
            )]
        );
    }

    #[tokio::test]
    async fn column_matching_two_primary_keys_references_both_tables() {
        let lens = lens_of(vec![
            ("a", table(&["x"], &["x"])),
            ("b", table(&["x"], &["x"])),
            ("c", table(&["c_id", "x"], &["c_id"])),
        ])
        .await;

        let kinds = lens.kinds("c", &[meta("x")]);

        assert_eq!(
            kinds,
            [reference(
                vec![target("a", "x", true), target("b", "x", true)],
                same_name("x", false)
            )]
        );
        assert_eq!(
            lens.references_from("c"),
            [
                resolved(("c", "x"), ("a", "x"), same_name("x", false)),
                resolved(("c", "x"), ("b", "x"), same_name("x", false)),
            ]
        );
    }

    #[tokio::test]
    async fn same_name_reference_is_indexed_only_when_an_index_starts_with_the_column() {
        for (indexes, indexed) in [
            (vec![index("by_action", &["action_id"])], true),
            (vec![index("by_action", &["action_id", "log_id"])], true),
            (vec![index("by_log", &["log_id", "action_id"])], false),
            (Vec::new(), false),
        ] {
            let lens = lens_of(action_tables(indexes.clone())).await;

            let by = lens.references_from("wp_actionscheduler_logs").remove(0).by;

            assert_eq!(by, same_name("action_id", indexed), "{indexes:?}");
        }
    }

    #[tokio::test]
    async fn composite_primary_key_member_is_a_reference_when_it_matches_another_key() {
        let lens = lens_of(vec![
            ("wp_posts", table(&["ID"], &["ID"])),
            (
                "wp_term_taxonomy",
                table(&["term_taxonomy_id", "term_id"], &["term_taxonomy_id"]),
            ),
            (
                "wp_term_relationships",
                table(
                    &["object_id", "term_taxonomy_id", "term_order"],
                    &["object_id", "term_taxonomy_id"],
                ),
            ),
        ])
        .await;
        let columns = [
            meta("object_id"),
            meta("term_taxonomy_id"),
            meta("term_order"),
        ];

        let kinds = lens.kinds("wp_term_relationships", &columns);

        assert_eq!(
            kinds,
            [
                ColumnKind::PrimaryKey {
                    table: address("wp_term_relationships"),
                    columns: vec!["object_id".into(), "term_taxonomy_id".into()],
                },
                reference(
                    vec![target("wp_term_taxonomy", "term_taxonomy_id", true)],
                    same_name("term_taxonomy_id", false)
                ),
                ColumnKind::Number,
            ]
        );
    }

    #[tokio::test]
    async fn references_to_lists_declared_foreign_keys_pointing_at_the_table() {
        let books = TableStructure {
            foreign_keys: vec![fk("books_author", "author_id", "authors")],
            ..table(&["id", "author_id"], &["id"])
        };
        let lens = lens_of(vec![
            ("authors", table(&["id", "name"], &["id"])),
            ("books", books),
        ])
        .await;

        assert_eq!(
            lens.references_to("authors"),
            [resolved(
                ("books", "author_id"),
                ("authors", "id"),
                ReferenceSource::ForeignKey {
                    constraint: "books_author".into()
                }
            )]
        );
        assert!(lens.references_to("books").is_empty());
    }

    #[tokio::test]
    async fn references_to_includes_inferred_references_from_other_tables() {
        let lens = lens_of(vec![
            (
                "wp_woocommerce_sessions",
                table(&["session_id", "session_key"], &["session_id"]),
            ),
            ("logs", table(&["log_id", "session_id"], &["log_id"])),
        ])
        .await;

        assert_eq!(
            lens.references_to("wp_woocommerce_sessions"),
            [resolved(
                ("logs", "session_id"),
                ("wp_woocommerce_sessions", "session_id"),
                same_name("session_id", false)
            )]
        );
    }
}
