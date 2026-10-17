//! `Rows`: an annotated result of one table, with each row's key and its values with provenance.

use crate::db::{Cell, ResultSet};
use crate::value::{ColumnKind, Provenance, ResultValues, RowKey, Value, ValueKind};

/// An annotated result of one table, with each row's key.
pub struct Rows {
    pub table: String,
    pub result: ResultSet,
    /// Empty when the table has no primary key.
    pub keys: Vec<RowKey>,
}

/// A currency candidate for a row by name: a column of the owner row or a meta key of the
/// document.
pub type CurrencyLookup<'a> = &'a dyn Fn(usize, &str) -> Option<Value>;

impl Rows {
    pub fn len(&self) -> usize {
        self.result.rows.len()
    }

    pub fn column(&self, name: &str) -> Option<usize> {
        self.result
            .columns
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn names(&self) -> Vec<String> {
        self.result.columns.iter().map(|c| c.name.clone()).collect()
    }

    pub fn cell(&self, row: usize, name: &str) -> Option<&Cell> {
        self.result.rows.get(row)?.get(self.column(name)?)
    }

    /// `None` when the table has no such column, `Some(None)` for NULL or binary data.
    pub fn text(&self, row: usize, name: &str) -> Option<Option<&str>> {
        self.cell(row, name).map(Cell::as_text)
    }

    pub fn key(&self, row: usize) -> Option<RowKey> {
        self.keys.get(row).cloned()
    }

    /// Every value of every row, sourced from this table. Money without a currency in its own
    /// row takes the first of its names that `currency` finds.
    pub fn values(&self, currency: CurrencyLookup<'_>) -> Vec<Vec<Value>> {
        let values = ResultValues::new(&self.result);
        (0..self.len())
            .map(|row| self.row_with(&values, row, currency))
            .collect()
    }

    /// The values of one row, as `values` gives them.
    pub fn row_values(&self, row: usize, currency: CurrencyLookup<'_>) -> Vec<Value> {
        self.row_with(&ResultValues::new(&self.result), row, currency)
    }

    fn row_with(
        &self,
        values: &ResultValues<'_>,
        row: usize,
        currency: CurrencyLookup<'_>,
    ) -> Vec<Value> {
        (0..self.result.columns.len())
            .map(|col| {
                let mut value = values.value(row, col, self.provenance(row, col));
                self.add_currency(&mut value, row, col, currency);
                value
            })
            .collect()
    }

    fn provenance(&self, row: usize, col: usize) -> Option<Provenance> {
        Some(Provenance {
            table: self.table.clone(),
            column: self.result.columns[col].name.clone(),
            row: self.keys.get(row)?.clone(),
        })
    }

    fn add_currency(
        &self,
        value: &mut Value,
        row: usize,
        col: usize,
        currency: CurrencyLookup<'_>,
    ) {
        let ValueKind::Money { currency: found } = &mut value.kind else {
            return;
        };
        if found.is_some() {
            return;
        }
        let names = match self.kind(row, col) {
            Some(ColumnKind::Money { currency }) => currency,
            _ => return,
        };
        *found = names
            .iter()
            .find_map(|name| currency(row, name))
            .map(Box::new);
    }

    fn kind(&self, row: usize, col: usize) -> Option<&ColumnKind> {
        let cell = self
            .result
            .cell_kinds
            .iter()
            .find(|c| c.row == row && c.col == col);
        cell.map(|c| &c.kind).or_else(|| self.result.kinds.get(col))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::testing::{result, text};

    fn eur() -> Value {
        Value {
            raw: text("EUR"),
            kind: ValueKind::Text,
            source: None,
            style: None,
            secondary: None,
        }
    }

    fn money_rows(amount: Cell, currency: Cell) -> Rows {
        let mut result = result(&["amount", "currency"], vec![vec![amount, currency]]);
        result.kinds = vec![
            ColumnKind::Money {
                currency: vec!["currency".into(), "_currency".into()],
            },
            ColumnKind::Text,
        ];
        Rows {
            table: "t".into(),
            result,
            keys: Vec::new(),
        }
    }

    #[test]
    fn rows_tell_a_missing_column_from_a_null_cell() {
        let rows = money_rows(Cell::Null, text("EUR"));

        assert_eq!(rows.text(0, "amount"), Some(None));
        assert_eq!(rows.text(0, "CURRENCY"), Some(Some("EUR")));
        assert_eq!(rows.text(0, "other"), None);
        assert_eq!(rows.names(), ["amount", "currency"]);
    }

    #[test]
    fn money_without_a_currency_in_its_row_takes_the_first_name_the_lookup_finds() {
        let rows = money_rows(text("10.00"), text(""));
        let asked = std::cell::RefCell::new(Vec::new());
        let lookup = |row: usize, name: &str| {
            asked.borrow_mut().push((row, name.to_string()));
            (name == "_currency").then(eur)
        };

        let values = rows.values(&lookup);

        assert_eq!(
            values[0][0].kind,
            ValueKind::Money {
                currency: Some(Box::new(eur()))
            }
        );
        assert_eq!(
            asked.into_inner(),
            [(0, "currency".to_string()), (0, "_currency".to_string())]
        );
    }

    #[test]
    fn money_with_a_currency_in_its_row_never_asks_the_lookup() {
        let mut rows = money_rows(text("10.00"), text("USD"));
        rows.keys = vec![RowKey(Vec::new())];
        let lookup = |_: usize, _: &str| -> Option<Value> { panic!("the row has a currency") };

        let values = rows.values(&lookup);

        let ValueKind::Money { currency } = &values[0][0].kind else {
            panic!("{:?}", values[0][0].kind);
        };
        let currency = currency.as_ref().unwrap();
        assert_eq!(currency.raw, text("USD"));
        assert_eq!(
            currency.source,
            Some(Provenance {
                table: "t".into(),
                column: "currency".into(),
                row: RowKey(Vec::new()),
            })
        );
    }

    #[test]
    fn money_whose_currency_nobody_knows_has_none() {
        let rows = money_rows(text("10.00"), Cell::Null);

        let values = rows.values(&|_, _| None);

        assert_eq!(values[0][0].kind, ValueKind::Money { currency: None });
        assert_eq!(values[0][1].kind, ValueKind::Null);
    }

    #[test]
    fn a_value_that_is_not_money_ignores_the_lookup() {
        let mut rows = money_rows(text("10.00"), text(""));
        rows.result.kinds[0] = ColumnKind::Text;

        let values = rows.values(&|_, _| Some(eur()));

        assert_eq!(values[0][0].kind, ValueKind::Text);
    }
}
