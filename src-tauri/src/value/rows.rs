//! Row keys and the `Value` of one cell, derived from a result's column kinds.

use super::{
    ColumnKind, KeyPart, Provenance, ReferenceTarget, RowKey, SerializedCell, Value, ValueKind,
};
use crate::address::{Address, Target};
use crate::db::{BrowsePage, Cell, ColumnEquals, ResultSet};

/// The primary key of every row, or none at all when a key column is missing from the result
/// or holds a value that is not text (binary keys only arrive as a preview).
pub fn row_keys(result: &ResultSet, key: &[String]) -> Vec<RowKey> {
    if key.is_empty() {
        return Vec::new();
    }
    let Some(indexes) = column_indexes(result, key) else {
        return Vec::new();
    };
    let mut keys = Vec::with_capacity(result.rows.len());
    for cells in &result.rows {
        let mut parts = Vec::with_capacity(key.len());
        for (column, &index) in key.iter().zip(&indexes) {
            let Some(value) = cells.get(index).and_then(Cell::as_text) else {
                return Vec::new();
            };
            parts.push(KeyPart {
                column: column.clone(),
                value: value.to_string(),
            });
        }
        keys.push(RowKey(parts));
    }
    keys
}

fn column_indexes(result: &ResultSet, names: &[String]) -> Option<Vec<usize>> {
    names
        .iter()
        .map(|name| {
            result
                .columns
                .iter()
                .position(|c| c.name.eq_ignore_ascii_case(name))
        })
        .collect()
}

/// The value of one cell of a browse page, sourced from the page's table and the row's key.
/// Panics when `row` or `col` is out of range.
pub fn page_value_at(page: &BrowsePage, row: usize, col: usize) -> Value {
    let source = page.keys.get(row).map(|key| Provenance {
        table: page.table.clone(),
        column: page.result.columns[col].name.clone(),
        row: key.clone(),
    });
    value_at(&page.result, row, col, source)
}

/// The value of one cell, from the result's column kinds and serialized cells. The reference
/// implementation of the cell rule: `src/lib/values.ts` ports it for the page (payload size),
/// and Rust callers outside the page (the MCP server) use it directly. For many cells of one
/// result, use `ResultValues`. Panics when `row` or `col` is out of range.
pub fn value_at(result: &ResultSet, row: usize, col: usize, source: Option<Provenance>) -> Value {
    ResultValues::new(result).value(row, col, source)
}

/// A result with the key column positions of its `PrimaryKey` columns resolved once.
pub struct ResultValues<'a> {
    result: &'a ResultSet,
    /// Per column: the positions of its key columns, `None` when one is missing.
    key_indexes: Vec<Option<Vec<usize>>>,
}

impl<'a> ResultValues<'a> {
    pub fn new(result: &'a ResultSet) -> Self {
        let key_indexes = result
            .kinds
            .iter()
            .map(|kind| match kind {
                ColumnKind::PrimaryKey { columns, .. } => column_indexes(result, columns),
                _ => None,
            })
            .collect();
        Self {
            result,
            key_indexes,
        }
    }

    pub fn value(&self, row: usize, col: usize, source: Option<Provenance>) -> Value {
        let cells = &self.result.rows[row];
        let raw = cells[col].clone();
        let column = self.result.kinds.get(col).unwrap_or(&ColumnKind::Text);
        let kind = self.cell_kind(cells, row, col, column);
        Value {
            raw,
            kind,
            source,
            style: None,
            secondary: None,
        }
    }

    fn cell_kind(&self, cells: &[Cell], row: usize, col: usize, column: &ColumnKind) -> ValueKind {
        let cell = &cells[col];
        if matches!(column, ColumnKind::Reference { .. }) && is_no_reference(cell) {
            return ValueKind::NoReference;
        }
        match cell {
            Cell::Null => return ValueKind::Null,
            Cell::Text(text) if text.is_empty() => return ValueKind::Empty,
            Cell::Bytes { .. } => return ValueKind::Binary,
            _ => {}
        }
        if let Some(kind) = self.schema_kind(cells, col, column, cell) {
            return kind;
        }
        if let Some(found) = find_cell(&self.result.serialized, row, col) {
            return ValueKind::Serialized {
                format: found.format,
                preview: found.preview.clone(),
            };
        }
        match column {
            ColumnKind::Number => ValueKind::Number,
            ColumnKind::Timestamp => ValueKind::Timestamp { utc: None },
            _ => ValueKind::Text,
        }
    }

    /// `PrimaryKey` or `Reference`, when the cell's text can address a row.
    fn schema_kind(
        &self,
        cells: &[Cell],
        col: usize,
        column: &ColumnKind,
        cell: &Cell,
    ) -> Option<ValueKind> {
        match column {
            ColumnKind::PrimaryKey { table, columns } => {
                let indexes = self.key_indexes.get(col)?.as_deref()?;
                let address = row_address(cells, table, columns, indexes)?;
                Some(ValueKind::PrimaryKey { address })
            }
            ColumnKind::Reference { targets, by } => {
                let value = cell.as_text()?;
                Some(ValueKind::Reference {
                    targets: targets
                        .iter()
                        .filter_map(|t| target_address(t, value))
                        .collect(),
                    by: by.clone(),
                    exists: None,
                })
            }
            _ => None,
        }
    }
}

/// WordPress stores "no target" as `0`; NULL means the same in a reference column.
fn is_no_reference(cell: &Cell) -> bool {
    matches!(cell, Cell::Null) || cell.as_text() == Some("0")
}

fn find_cell(cells: &[SerializedCell], row: usize, col: usize) -> Option<&SerializedCell> {
    let at = cells
        .binary_search_by_key(&(row, col), |c| (c.row, c.col))
        .ok()?;
    cells.get(at)
}

/// The table of a kind's table address; kinds always carry `Target::Table`.
fn table_name(address: &Address) -> Option<&str> {
    match &address.target {
        Target::Table { table, .. } => Some(table),
        _ => None,
    }
}

fn with_target(base: &Address, target: Target) -> Address {
    Address::new(base.connection.clone(), base.database.clone(), target)
}

/// A single-column key addresses the row itself; a composite key is a filter on all of it.
fn row_address(
    cells: &[Cell],
    table: &Address,
    key: &[String],
    indexes: &[usize],
) -> Option<Address> {
    let name = table_name(table)?.to_string();
    let text = |index: usize| Some(cells.get(index)?.as_text()?.to_string());
    if let [index] = indexes {
        let key = text(*index)?;
        return Some(with_target(table, Target::Row { table: name, key }));
    }
    let filter = key
        .iter()
        .zip(indexes)
        .map(|(column, &index)| {
            Some(ColumnEquals {
                column: column.clone(),
                value: Some(text(index)?),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let target = Target::Table {
        table: name,
        filter,
    };
    Some(with_target(table, target))
}

/// `None` for a target whose address is not a table, which the lens never builds.
fn target_address(target: &ReferenceTarget, value: &str) -> Option<Address> {
    let table = table_name(&target.table)?.to_string();
    let value = value.to_string();
    if target.key {
        return Some(with_target(
            &target.table,
            Target::Row { table, key: value },
        ));
    }
    let filter = vec![ColumnEquals {
        column: target.column.clone(),
        value: Some(value),
    }];
    Some(with_target(&target.table, Target::Table { table, filter }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ColumnMeta, FullText};
    use crate::decode::value::Format;
    use crate::value::{KeyPart, ReferenceSource, find_serialized};

    fn table_address(table: &str) -> Address {
        let target = Target::Table {
            table: table.into(),
            filter: Vec::new(),
        };
        Address::new("local", "wp", target)
    }

    fn row_address_of(table: &str, key: &str) -> Address {
        let target = Target::Row {
            table: table.into(),
            key: key.into(),
        };
        Address::new("local", "wp", target)
    }

    fn filtered(table: &str, filter: &[(&str, &str)]) -> Address {
        let filter = filter
            .iter()
            .map(|(column, value)| ColumnEquals {
                column: column.to_string(),
                value: Some(value.to_string()),
            })
            .collect();
        let target = Target::Table {
            table: table.into(),
            filter,
        };
        Address::new("local", "wp", target)
    }

    fn by() -> ReferenceSource {
        ReferenceSource::SameName {
            column: "user_id".into(),
            indexed: false,
        }
    }

    fn reference(key: bool) -> ColumnKind {
        ColumnKind::Reference {
            targets: vec![ReferenceTarget {
                table: table_address("wp_users"),
                column: "ID".into(),
                key,
            }],
            by: by(),
        }
    }

    fn primary_key(table: &str, columns: &[&str]) -> ColumnKind {
        ColumnKind::PrimaryKey {
            table: table_address(table),
            columns: columns.iter().map(ToString::to_string).collect(),
        }
    }

    fn text(value: &str) -> Cell {
        Cell::Text(value.into())
    }

    fn number(value: &str) -> Cell {
        Cell::Number(value.into())
    }

    fn bytes() -> Cell {
        Cell::Bytes {
            len: 2,
            hex_preview: "00ff".into(),
        }
    }

    fn result(names: &[&str], kinds: Vec<ColumnKind>, rows: Vec<Vec<Cell>>) -> ResultSet {
        ResultSet {
            columns: names
                .iter()
                .map(|name| ColumnMeta {
                    name: name.to_string(),
                    type_name: "TEXT".into(),
                    origin: None,
                })
                .collect(),
            rows,
            kinds,
            ..ResultSet::default()
        }
    }

    fn kind_of(column: ColumnKind, cell: Cell) -> ValueKind {
        let mut set = result(&["c"], vec![column], vec![vec![cell]]);
        find_serialized(&mut set);
        value_at(&set, 0, 0, None).kind
    }

    fn key_of(parts: &[(&str, &str)]) -> RowKey {
        RowKey(
            parts
                .iter()
                .map(|(column, value)| KeyPart {
                    column: column.to_string(),
                    value: value.to_string(),
                })
                .collect(),
        )
    }

    #[test]
    fn plain_cells_take_their_kind_from_the_cell_and_the_column() {
        let cases = [
            (ColumnKind::Text, Cell::Null, ValueKind::Null),
            (ColumnKind::Text, text(""), ValueKind::Empty),
            (ColumnKind::Text, bytes(), ValueKind::Binary),
            (ColumnKind::Number, bytes(), ValueKind::Binary),
            (ColumnKind::Text, text("hello"), ValueKind::Text),
            (ColumnKind::Number, number("0"), ValueKind::Number),
            (ColumnKind::Number, number("12"), ValueKind::Number),
            (
                ColumnKind::Timestamp,
                Cell::Temporal("2026-01-01 10:00:00".into()),
                ValueKind::Timestamp { utc: None },
            ),
        ];
        for (column, cell, expected) in cases {
            let label = format!("{column:?} {cell:?}");
            assert_eq!(kind_of(column, cell), expected, "{label}");
        }
    }

    #[test]
    fn result_values_agree_with_value_at_for_every_cell() {
        let key = primary_key("wp_term_relationships", &["object_id", "term_taxonomy_id"]);
        let set = result(
            &["object_id", "term_taxonomy_id", "owner"],
            vec![key.clone(), key, reference(true)],
            vec![
                vec![number("1"), number("5"), number("3")],
                vec![number("2"), number("6"), Cell::Null],
            ],
        );
        let values = ResultValues::new(&set);

        for row in 0..2 {
            for col in 0..3 {
                assert_eq!(
                    values.value(row, col, None),
                    value_at(&set, row, col, None),
                    "{row},{col}"
                );
            }
        }
    }

    #[test]
    fn reference_to_a_non_table_target_gets_no_address() {
        let column = ColumnKind::Reference {
            targets: vec![ReferenceTarget {
                table: Address::new("local", "wp", Target::Home),
                column: "ID".into(),
                key: true,
            }],
            by: by(),
        };

        assert_eq!(
            kind_of(column, number("5")),
            ValueKind::Reference {
                targets: Vec::new(),
                by: by(),
                exists: None,
            }
        );
    }

    #[test]
    fn missing_column_kind_reads_as_text() {
        let set = result(&["c"], Vec::new(), vec![vec![text("x")]]);

        assert_eq!(value_at(&set, 0, 0, None).kind, ValueKind::Text);
    }

    #[test]
    fn null_and_zero_in_a_reference_column_mean_no_reference() {
        for cell in [Cell::Null, number("0"), text("0")] {
            assert_eq!(
                kind_of(reference(true), cell.clone()),
                ValueKind::NoReference,
                "{cell:?}"
            );
        }
    }

    #[test]
    fn empty_text_in_a_reference_column_is_empty() {
        assert_eq!(kind_of(reference(true), text("")), ValueKind::Empty);
    }

    #[test]
    fn reference_to_a_primary_key_opens_the_target_row() {
        assert_eq!(
            kind_of(reference(true), number("5")),
            ValueKind::Reference {
                targets: vec![row_address_of("wp_users", "5")],
                by: by(),
                exists: None,
            }
        );
    }

    #[test]
    fn reference_to_another_column_opens_the_target_table_filtered() {
        assert_eq!(
            kind_of(reference(false), number("5")),
            ValueKind::Reference {
                targets: vec![filtered("wp_users", &[("ID", "5")])],
                by: by(),
                exists: None,
            }
        );
    }

    #[test]
    fn single_column_primary_key_cell_addresses_its_row() {
        let set = result(
            &["ID", "title"],
            vec![primary_key("wp_posts", &["ID"]), ColumnKind::Text],
            vec![vec![number("7"), text("Hello")]],
        );

        assert_eq!(
            value_at(&set, 0, 0, None).kind,
            ValueKind::PrimaryKey {
                address: row_address_of("wp_posts", "7")
            }
        );
    }

    #[test]
    fn composite_primary_key_cell_addresses_a_filter_on_all_key_columns() {
        let key = primary_key("wp_term_relationships", &["object_id", "term_taxonomy_id"]);
        let set = result(
            &["object_id", "term_taxonomy_id"],
            vec![key.clone(), key],
            vec![vec![number("1"), number("5")]],
        );

        assert_eq!(
            value_at(&set, 0, 1, None).kind,
            ValueKind::PrimaryKey {
                address: filtered(
                    "wp_term_relationships",
                    &[("object_id", "1"), ("term_taxonomy_id", "5")]
                )
            }
        );
    }

    #[test]
    fn primary_key_column_missing_from_the_result_reads_as_text() {
        let set = result(
            &["title"],
            vec![primary_key("wp_posts", &["ID"])],
            vec![vec![text("Hello")]],
        );

        assert_eq!(value_at(&set, 0, 0, None).kind, ValueKind::Text);
    }

    #[test]
    fn serialized_text_carries_its_format_and_preview() {
        let php = ValueKind::Serialized {
            format: Format::Php,
            preview: "a:1:{…}".into(),
        };
        let object = ValueKind::Serialized {
            format: Format::Json,
            preview: "{…}".into(),
        };
        let list = ValueKind::Serialized {
            format: Format::Json,
            preview: "[…]".into(),
        };
        let cases = [
            (text("a:1:{i:0;i:1;}"), php),
            (text(r#"{"a":1}"#), object),
            (Cell::Json("[1,2]".into()), list),
        ];
        for (cell, expected) in cases {
            let label = format!("{cell:?}");
            assert_eq!(kind_of(ColumnKind::Text, cell), expected, "{label}");
        }
    }

    #[test]
    fn long_text_is_serialized_when_its_full_text_is() {
        let mut set = result(
            &["meta_value"],
            vec![ColumnKind::Text],
            vec![vec![Cell::LongText {
                len: 14,
                preview: "a:1:".into(),
            }]],
        );
        set.full_texts = vec![FullText {
            row: 0,
            col: 0,
            text: "a:1:{i:0;i:1;}".into(),
        }];
        find_serialized(&mut set);

        assert_eq!(
            value_at(&set, 0, 0, None).kind,
            ValueKind::Serialized {
                format: Format::Php,
                preview: "a:1:{…}".into(),
            }
        );
    }

    #[test]
    fn long_text_that_is_not_serialized_stays_text() {
        let mut set = result(
            &["c"],
            vec![ColumnKind::Text],
            vec![vec![Cell::LongText {
                len: 9,
                preview: "abc".into(),
            }]],
        );
        set.full_texts = vec![FullText {
            row: 0,
            col: 0,
            text: "abcdefghi".into(),
        }];
        find_serialized(&mut set);

        assert_eq!(value_at(&set, 0, 0, None).kind, ValueKind::Text);
    }

    #[test]
    fn value_keeps_the_raw_cell_and_the_given_source() {
        let set = result(&["c"], vec![ColumnKind::Text], vec![vec![text("x")]]);
        let source = Provenance {
            table: "t".into(),
            column: "c".into(),
            row: key_of(&[("id", "1")]),
        };

        let value = value_at(&set, 0, 0, Some(source.clone()));

        assert_eq!(value.raw, text("x"));
        assert_eq!(value.source, Some(source));
    }

    #[test]
    fn row_keys_lists_the_key_of_every_row_in_key_order() {
        let set = result(
            &["name", "b", "a"],
            Vec::new(),
            vec![
                vec![text("x"), number("2"), number("1")],
                vec![text("y"), number("4"), number("3")],
            ],
        );

        let found = row_keys(&set, &["a".into(), "B".into()]);

        assert_eq!(
            found,
            [
                key_of(&[("a", "1"), ("B", "2")]),
                key_of(&[("a", "3"), ("B", "4")])
            ]
        );
    }

    #[test]
    fn row_keys_is_empty_for_a_table_without_a_primary_key() {
        let set = result(&["a"], Vec::new(), vec![vec![number("1")]]);

        assert!(row_keys(&set, &[]).is_empty());
    }

    #[test]
    fn row_keys_is_empty_when_a_key_column_is_not_in_the_result() {
        let set = result(&["a"], Vec::new(), vec![vec![number("1")]]);

        assert!(row_keys(&set, &["a".into(), "b".into()]).is_empty());
    }

    #[test]
    fn row_keys_is_empty_when_a_key_cell_is_binary() {
        let set = result(&["a"], Vec::new(), vec![vec![number("1")], vec![bytes()]]);

        assert!(row_keys(&set, &["a".into()]).is_empty());
    }

    fn page(keys: Vec<RowKey>) -> BrowsePage {
        BrowsePage {
            table: "wp_posts".into(),
            result: result(
                &["ID", "title"],
                vec![ColumnKind::Number, ColumnKind::Text],
                vec![vec![number("7"), text("Hello")]],
            ),
            keys,
            has_more: false,
            limit: 10,
            filter: None,
            columns_filter: None,
        }
    }

    #[test]
    fn page_value_is_sourced_from_the_pages_table_and_the_rows_key() {
        let page = page(vec![key_of(&[("ID", "7")])]);

        let value = page_value_at(&page, 0, 1);

        assert_eq!(
            value.source,
            Some(Provenance {
                table: "wp_posts".into(),
                column: "title".into(),
                row: key_of(&[("ID", "7")]),
            })
        );
    }

    #[test]
    fn page_value_without_row_keys_has_no_source() {
        assert_eq!(page_value_at(&page(Vec::new()), 0, 0).source, None);
    }
}
