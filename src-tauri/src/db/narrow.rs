//! One more condition on a plain single-table `SELECT`: the statement keeps its text, and the
//! condition is added to its `WHERE` (or becomes it).

use std::ops::Range;

use super::browse::equals_sql;
use super::lexer::{Piece, pieces, statement_ranges};
use super::{ColumnEquals, Dialect};

/// Words that make a statement more than a plain read of one table.
const REFUSED: &[&str] = &[
    "UNION",
    "EXCEPT",
    "INTERSECT",
    "JOIN",
    "STRAIGHT_JOIN",
    "GROUP",
    "HAVING",
    "WINDOW",
    "INTO",
];

/// Clauses that end `FROM` or `WHERE`.
const CLAUSES: &[&str] = &["ORDER", "LIMIT", "FOR", "LOCK", "PROCEDURE"];

/// Where the condition goes in the script, in bytes of the script.
enum Spot {
    /// After the existing `WHERE` condition, which spans these bytes.
    Where(Range<usize>),
    /// After the table.
    NoWhere(usize),
}

/// Why `script` cannot be narrowed, or `None` when it can.
pub fn refusal(script: &str) -> Option<String> {
    insertion_point(script).err()
}

/// `script` with `condition` added to the `WHERE` of its only statement.
pub fn narrowed(script: &str, condition: &str) -> Result<String, String> {
    let out = match insertion_point(script)? {
        Spot::Where(body) => format!(
            "{}({}) AND {condition}{}",
            &script[..body.start],
            &script[body.clone()],
            &script[body.end..]
        ),
        Spot::NoWhere(at) => format!("{} WHERE {condition}{}", &script[..at], &script[at..]),
    };
    Ok(out)
}

/// `narrowed` with `column = value` (`IS NULL` for none) in `dialect`.
pub fn narrow_to_value(
    dialect: Dialect,
    script: &str,
    column: String,
    value: Option<String>,
) -> Result<String, String> {
    narrowed(
        script,
        &equals_sql(dialect, &[ColumnEquals { column, value }]),
    )
}

fn insertion_point(script: &str) -> Result<Spot, String> {
    let range = match statement_ranges(script).as_slice() {
        [only] => only.clone(),
        [] => return Err("There is no statement".into()),
        _ => return Err("Only a single statement can be narrowed".into()),
    };
    let offset = range.start;
    let code: Vec<Piece> = pieces(&script[range])
        .into_iter()
        .filter(|p| !p.is_comment())
        .collect();
    if !code.first().is_some_and(|p| p.is_keyword("SELECT")) {
        return Err("Only a plain SELECT can be narrowed".into());
    }
    let top: Vec<(usize, &Piece)> = code
        .iter()
        .enumerate()
        .filter(|(_, p)| p.depth == 0)
        .collect();
    if let Some((_, word)) = top
        .iter()
        .find(|(_, p)| REFUSED.iter().any(|k| p.is_keyword(k)))
    {
        return Err(format!(
            "Only a plain single-table SELECT can be narrowed, not one with {}",
            word.text.to_uppercase()
        ));
    }
    let from = top
        .iter()
        .position(|(_, p)| p.is_keyword("FROM"))
        .ok_or("The SELECT reads no table")?;
    let is_clause = |p: &Piece| p.is_keyword("WHERE") || CLAUSES.iter().any(|k| p.is_keyword(k));
    let clause_after = |at: usize| {
        top[at..]
            .iter()
            .position(|(_, p)| is_clause(p))
            .map(|i| at + i)
    };
    let table_end = clause_after(from + 1);
    let tables = &top[from + 1..table_end.unwrap_or(top.len())];
    if tables.is_empty() {
        return Err("The SELECT reads no table".into());
    }
    if code.get(top[from].0 + 1).is_some_and(|p| p.is_open()) {
        return Err("Only a SELECT from a table can be narrowed, not from a subquery".into());
    }
    if tables.iter().any(|(_, p)| p.is_symbol(",")) {
        return Err("Only a SELECT from a single table can be narrowed".into());
    }
    let end_of = |index: usize| offset + code[index].span().end;
    let Some(clause) = table_end else {
        return Ok(Spot::NoWhere(end_of(code.len() - 1)));
    };
    let (clause_index, keyword) = top[clause];
    if !keyword.is_keyword("WHERE") {
        return Ok(Spot::NoWhere(end_of(clause_index - 1)));
    }
    let body_end = clause_after(clause + 1).map_or(code.len(), |i| top[i].0);
    let first = clause_index + 1;
    if first >= body_end {
        return Err("The WHERE has no condition".into());
    }
    Ok(Spot::Where(
        offset + code[first].span().start..end_of(body_end - 1),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const COND: &str = "`status` = 'x'";

    #[test]
    fn a_select_without_where_gets_one_after_the_table() {
        for (script, expected) in [
            ("SELECT * FROM t", "SELECT * FROM t WHERE `status` = 'x'"),
            (
                "SELECT * FROM `db`.`t` AS a",
                "SELECT * FROM `db`.`t` AS a WHERE `status` = 'x'",
            ),
            ("select id from t", "select id from t WHERE `status` = 'x'"),
            (
                "SELECT 'JOIN' FROM t",
                "SELECT 'JOIN' FROM t WHERE `status` = 'x'",
            ),
        ] {
            assert_eq!(narrowed(script, COND).as_deref(), Ok(expected), "{script}");
        }
    }

    #[test]
    fn an_existing_where_is_wrapped_and_the_condition_is_anded() {
        for (script, expected) in [
            (
                "SELECT * FROM t WHERE a = 1",
                "SELECT * FROM t WHERE (a = 1) AND `status` = 'x'",
            ),
            (
                "SELECT * FROM t WHERE a = 1 OR b = 2",
                "SELECT * FROM t WHERE (a = 1 OR b = 2) AND `status` = 'x'",
            ),
            (
                "select * from t where a=1",
                "select * from t where (a=1) AND `status` = 'x'",
            ),
            (
                "SELECT * FROM t WHERE id IN (SELECT id FROM u JOIN v ON u.id = v.id)",
                "SELECT * FROM t WHERE (id IN (SELECT id FROM u JOIN v ON u.id = v.id)) AND `status` = 'x'",
            ),
        ] {
            assert_eq!(narrowed(script, COND).as_deref(), Ok(expected), "{script}");
        }
    }

    #[test]
    fn trailing_clauses_stay_after_the_condition() {
        for (script, expected) in [
            (
                "SELECT * FROM t ORDER BY id",
                "SELECT * FROM t WHERE `status` = 'x' ORDER BY id",
            ),
            (
                "SELECT * FROM t ORDER BY id DESC LIMIT 5",
                "SELECT * FROM t WHERE `status` = 'x' ORDER BY id DESC LIMIT 5",
            ),
            (
                "SELECT * FROM t LIMIT 5",
                "SELECT * FROM t WHERE `status` = 'x' LIMIT 5",
            ),
            (
                "SELECT * FROM t WHERE a = 1 ORDER BY id LIMIT 5",
                "SELECT * FROM t WHERE (a = 1) AND `status` = 'x' ORDER BY id LIMIT 5",
            ),
            (
                "SELECT * FROM t WHERE a = 1 FOR UPDATE",
                "SELECT * FROM t WHERE (a = 1) AND `status` = 'x' FOR UPDATE",
            ),
            (
                "SELECT * FROM t LOCK IN SHARE MODE",
                "SELECT * FROM t WHERE `status` = 'x' LOCK IN SHARE MODE",
            ),
        ] {
            assert_eq!(narrowed(script, COND).as_deref(), Ok(expected), "{script}");
        }
    }

    #[test]
    fn comments_and_a_trailing_semicolon_are_kept_in_place() {
        for (script, expected) in [
            ("SELECT * FROM t;", "SELECT * FROM t WHERE `status` = 'x';"),
            (
                "SELECT * FROM t WHERE a = 1;\n",
                "SELECT * FROM t WHERE (a = 1) AND `status` = 'x';\n",
            ),
            (
                "-- all rows\nSELECT * FROM t\n",
                "-- all rows\nSELECT * FROM t WHERE `status` = 'x'\n",
            ),
            (
                "SELECT * FROM t -- tail",
                "SELECT * FROM t WHERE `status` = 'x' -- tail",
            ),
            (
                "SELECT * FROM t /* order below */ ORDER BY id",
                "SELECT * FROM t WHERE `status` = 'x' /* order below */ ORDER BY id",
            ),
            (
                "SELECT * FROM t WHERE a /* one */ = 1 -- tail\nORDER BY id",
                "SELECT * FROM t WHERE (a /* one */ = 1) AND `status` = 'x' -- tail\nORDER BY id",
            ),
            (
                "SELECT * FROM t; -- done",
                "SELECT * FROM t WHERE `status` = 'x'; -- done",
            ),
        ] {
            assert_eq!(narrowed(script, COND).as_deref(), Ok(expected), "{script}");
        }
    }

    #[test]
    fn statements_that_are_not_a_plain_single_table_select_are_refused_with_a_reason() {
        for (script, reason) in [
            ("", "There is no statement"),
            ("-- nothing\n;", "There is no statement"),
            (
                "SELECT * FROM a; SELECT * FROM b",
                "Only a single statement can be narrowed",
            ),
            ("DELETE FROM t", "Only a plain SELECT can be narrowed"),
            (
                "WITH x AS (SELECT 1) SELECT * FROM x",
                "Only a plain SELECT can be narrowed",
            ),
            ("SELECT 1", "The SELECT reads no table"),
            ("SELECT * FROM WHERE a = 1", "The SELECT reads no table"),
            (
                "SELECT * FROM a JOIN b ON a.id = b.a_id",
                "Only a plain single-table SELECT can be narrowed, not one with JOIN",
            ),
            (
                "SELECT * FROM a LEFT JOIN b USING (id)",
                "Only a plain single-table SELECT can be narrowed, not one with JOIN",
            ),
            (
                "SELECT status, COUNT(*) FROM t GROUP BY status",
                "Only a plain single-table SELECT can be narrowed, not one with GROUP",
            ),
            (
                "SELECT COUNT(*) FROM t HAVING COUNT(*) > 1",
                "Only a plain single-table SELECT can be narrowed, not one with HAVING",
            ),
            (
                "SELECT a FROM t UNION SELECT a FROM u",
                "Only a plain single-table SELECT can be narrowed, not one with UNION",
            ),
            (
                "SELECT a INTO @v FROM t",
                "Only a plain single-table SELECT can be narrowed, not one with INTO",
            ),
            (
                "SELECT * FROM (SELECT 1) x",
                "Only a SELECT from a table can be narrowed, not from a subquery",
            ),
            (
                "SELECT * FROM a, b",
                "Only a SELECT from a single table can be narrowed",
            ),
            (
                "SELECT * FROM t WHERE ORDER BY id",
                "The WHERE has no condition",
            ),
        ] {
            assert_eq!(refusal(script).as_deref(), Some(reason), "{script}");
            assert_eq!(narrowed(script, COND), Err(reason.to_string()), "{script}");
        }
    }

    #[test]
    fn plain_selects_have_no_refusal() {
        for script in [
            "SELECT * FROM t",
            "SELECT * FROM t WHERE a = 1 ORDER BY id LIMIT 5;",
            "SELECT 'GROUP' AS `union` FROM t",
            "SELECT * FROM t WHERE id IN (SELECT id FROM u GROUP BY id)",
        ] {
            assert_eq!(refusal(script), None, "{script}");
        }
    }

    #[test]
    fn narrowing_to_a_value_filters_on_equality_or_null() {
        for (script, value, expected) in [
            (
                "SELECT * FROM t",
                Some("wc-paid"),
                "SELECT * FROM t WHERE `status` = 'wc-paid'",
            ),
            (
                "SELECT * FROM t",
                None,
                "SELECT * FROM t WHERE `status` IS NULL",
            ),
            (
                "SELECT * FROM t WHERE a = 1 LIMIT 3",
                None,
                "SELECT * FROM t WHERE (a = 1) AND `status` IS NULL LIMIT 3",
            ),
        ] {
            assert_eq!(
                narrow_to_value(
                    Dialect::MYSQL,
                    script,
                    "status".into(),
                    value.map(String::from)
                )
                .as_deref(),
                Ok(expected),
                "{script} {value:?}"
            );
        }
    }

    #[test]
    fn narrowing_to_a_value_refuses_what_narrowing_refuses() {
        let result = narrow_to_value(
            Dialect::MYSQL,
            "SELECT * FROM a JOIN b",
            "status".into(),
            None,
        );

        assert_eq!(
            result,
            Err("Only a plain single-table SELECT can be narrowed, not one with JOIN".to_string())
        );
    }
}
