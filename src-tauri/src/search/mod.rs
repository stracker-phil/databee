//! The palette's row search over the rules and the schema (docs/architecture.md, "Palette search").

pub mod commands;
mod find;
mod hits;
mod ids;
mod text;

use std::collections::HashSet;
use std::time::Duration;

use serde::Serialize;
use ts_rs::TS;

use crate::address::Address;
use crate::document::Ctx;
use crate::error::AppResult;
use crate::rules::effective::RuleSources;
use crate::session::Session;
use crate::value::Value;

const SECTION_CAP: usize = 20;
const QUERY_TIMEOUT: Duration = Duration::from_secs(3);
/// Free text shorter than this matches only tables and commands.
const MIN_TEXT: usize = 6;
/// Without entities, `#` reads only tables with fewer estimated rows.
const PLAIN_TABLE_ROWS: u64 = 1_000_000;
/// Inputs only the page answers: tables, saved queries, commands and addresses.
const PAGE_PREFIXES: [&str; 4] = ["t:", "q:", ">", "databee:"];

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct PaletteHit {
    /// What the hits of a section are, e.g. `objects with id 4711`.
    pub section: String,
    /// What Enter opens: the row, or the owner row of a meta value.
    pub address: Address,
    /// The table, or `table.column` the row references or matched through.
    pub source: String,
    /// The key of the opened row.
    pub title: Vec<Value>,
    /// The entity's summary, else the first columns; for a meta value its key and value.
    pub detail: Vec<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Query<'a> {
    Id(&'a str),
    People(&'a str),
    Text(&'a str),
    Find(&'a str),
}

/// What the search does with the palette's input; `None` when only the page matches it.
fn parse(input: &str) -> Option<Query<'_>> {
    let input = input.trim();
    if let Some(id) = input.strip_prefix('#') {
        let id = id.trim();
        return (!id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())).then_some(Query::Id(id));
    }
    if let Some(text) = input.strip_prefix('@') {
        let text = text.trim();
        return (text.chars().count() >= 2).then_some(Query::People(text));
    }
    if let Some(value) = input.strip_prefix('=') {
        let value = value.trim();
        return (value.chars().count() >= 2).then_some(Query::Find(value));
    }
    let page_only = PAGE_PREFIXES.iter().any(|prefix| input.starts_with(prefix));
    (!page_only && input.chars().count() >= MIN_TEXT).then_some(Query::Text(input))
}

/// The hits for `input` in `database`, each row once (the first section that finds it keeps it).
pub async fn search(
    session: &Session,
    database: &str,
    input: &str,
    sources: RuleSources<'_>,
    timeout: Duration,
) -> AppResult<Vec<PaletteHit>> {
    let Some(query) = parse(input) else {
        return Ok(Vec::new());
    };
    let ctx = Ctx::load(session, database, sources, timeout.min(QUERY_TIMEOUT)).await?;
    let found = match query {
        Query::Id(id) => ids::search(&ctx, id).await,
        Query::People(text) => text::people(&ctx, text).await,
        Query::Text(text) => text::free(&ctx, text).await,
        Query::Find(value) => find::everywhere(&ctx, value).await,
    };
    Ok(unique(found))
}

fn unique(hits: Vec<PaletteHit>) -> Vec<PaletteHit> {
    let mut seen = HashSet::new();
    hits.into_iter()
        .filter(|hit| seen.insert(hit.address.to_string()))
        .collect()
}

#[cfg(test)]
mod server_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::Target;
    use crate::document::testing::{Rig, num, result, shop, text};
    use crate::error::AppError;

    const TIMEOUT: Duration = Duration::from_secs(30);

    const FIND_LOG_MESSAGES: &str = r#"
[[find]]
table = "orders"
columns = ["status"]
match = "contains"

[[find]]
table = "log"
columns = ["msg"]
match = "contains"
"#;

    const FIND_TWO_LOG_COLUMNS: &str = r#"
[[find]]
table = "log"
columns = ["msg", "order_id"]
match = "contains"
"#;

    fn log_rows(count: usize) -> crate::db::ResultSet {
        let rows = (1..=count)
            .map(|i| {
                vec![
                    num(&i.to_string()),
                    num("4711"),
                    text(""),
                    text("sandbox ok"),
                ]
            })
            .collect();
        result(&["log_id", "order_id", "at_gmt", "msg"], rows)
    }

    fn log_address(key: &str) -> Address {
        Address::new(
            "local",
            "wp",
            Target::Row {
                table: "f_log".into(),
                key: key.into(),
            },
        )
    }

    #[test]
    fn input_is_read_as_an_id_people_text_or_find_query() {
        for (input, expected) in [
            ("#4711", Some(Query::Id("4711"))),
            ("  #4711  ", Some(Query::Id("4711"))),
            ("# 4711", Some(Query::Id("4711"))),
            ("#abc", None),
            ("#", None),
            ("#47 11", None),
            ("#-5", None),
            ("@j", None),
            ("@ja", Some(Query::People("ja"))),
            ("@äö", Some(Query::People("äö"))),
            ("=s", None),
            ("=sa", Some(Query::Find("sa"))),
            ("= sandbox ", Some(Query::Find("sandbox"))),
            ("abcde", None),
            ("abcdef", Some(Query::Text("abcdef"))),
            ("äöüäöü", Some(Query::Text("äöüäöü"))),
            ("66M12696JX411763D", Some(Query::Text("66M12696JX411763D"))),
            ("t:orders", None),
            ("q:something_saved", None),
            (">open settings", None),
            ("databee://shop/db/wp_posts/42", None),
            ("", None),
        ] {
            assert_eq!(parse(input), expected, "{input:?}");
        }
    }

    #[tokio::test]
    async fn input_only_the_page_answers_reads_nothing() {
        let rig = Rig::new(shop(), "");

        for input in ["t:orders", "abcde", "#abc", "@j"] {
            let found = search(&rig.session, "wp", input, rig.sources(), TIMEOUT)
                .await
                .unwrap();

            assert!(found.is_empty(), "{input}: {found:?}");
        }
        assert_eq!(rig.db.queries(), Vec::<String>::new());
    }

    #[tokio::test]
    async fn a_failing_source_drops_only_its_section() {
        let db = shop()
            .failing("`f_orders`", AppError::InvalidInput("boom".into()))
            .on("`f_log`", log_rows(1));
        let rig = Rig::new(db, FIND_LOG_MESSAGES);

        let found = search(&rig.session, "wp", "=sandbox", rig.sources(), TIMEOUT)
            .await
            .unwrap();

        assert_eq!(found.len(), 1, "{found:?}");
        let hit = &found[0];
        assert_eq!(hit.section, "f_log.msg");
        assert_eq!(hit.source, "f_log.msg");
        assert_eq!(hit.address, log_address("1"));
        assert_eq!(hit.title[0].raw, num("1"));
        assert_eq!(hit.detail[0].raw, text("sandbox ok"));
    }

    #[tokio::test]
    async fn a_section_keeps_at_most_the_section_cap() {
        let db = shop().on("`f_log`", log_rows(SECTION_CAP + 5));
        let rig = Rig::new(db, FIND_LOG_MESSAGES);

        let found = search(&rig.session, "wp", "=sandbox", rig.sources(), TIMEOUT)
            .await
            .unwrap();

        assert_eq!(found.len(), SECTION_CAP);
        let keys: Vec<_> = found.iter().map(|hit| hit.title[0].raw.clone()).collect();
        let expected: Vec<_> = (1..=SECTION_CAP).map(|i| num(&i.to_string())).collect();
        assert_eq!(keys, expected);
    }

    #[tokio::test]
    async fn a_row_found_by_two_sections_is_listed_once_in_the_first() {
        let rig = Rig::new(shop().on("`f_log`", log_rows(1)), FIND_TWO_LOG_COLUMNS);

        let found = search(&rig.session, "wp", "=sandbox", rig.sources(), TIMEOUT)
            .await
            .unwrap();

        let sections: Vec<_> = found.iter().map(|hit| hit.section.as_str()).collect();
        assert_eq!(sections, ["f_log.msg"]);
    }

    #[tokio::test]
    async fn queries_run_with_the_settings_timeout_capped_at_the_search_limit() {
        for (settings, expected) in [
            (Duration::from_secs(30), QUERY_TIMEOUT),
            (Duration::from_secs(1), Duration::from_secs(1)),
        ] {
            let rig = Rig::new(shop(), "");

            search(&rig.session, "wp", "#5", rig.sources(), settings)
                .await
                .unwrap();

            let timeouts = rig.db.fetch_timeouts();
            assert!(!timeouts.is_empty(), "{settings:?}");
            assert!(
                timeouts.iter().all(|t| *t == expected),
                "{settings:?}: {timeouts:?}"
            );
        }
    }
}
