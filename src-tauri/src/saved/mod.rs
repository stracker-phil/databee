//! Saved queries: the project's, the global ones and the `[[pinned]]` queries of the applied
//! presets, as the palette's `q:` lists them (docs/presets.md, "Pinned queries").

pub mod commands;
pub mod migrate;

use serde::Serialize;
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use crate::rules::ProjectsStore;
use crate::rules::effective::EffectiveRules;
use crate::rules::placeholder::with_prefixes;
use crate::rules::project::{Project, SavedQuery};
use crate::rules::queries::{Home, Pinned};

#[derive(Serialize, Debug, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SavedOrigin {
    Project,
    /// Offered in every project.
    Global,
    Preset {
        preset: String,
    },
}

/// A saved query as the palette offers it, with `{p}`/`{np}` replaced. Preset queries are
/// read-only: copying one to the project makes it editable.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct SavedEntry {
    pub id: String,
    pub title: String,
    pub sql: String,
    pub pin: Option<Home>,
    pub origin: SavedOrigin,
    pub read_only: bool,
}

struct Prefixes<'a> {
    prefix: &'a str,
    network_prefix: &'a str,
}

impl<'a> Prefixes<'a> {
    fn of(rules: &'a EffectiveRules) -> Self {
        Self {
            prefix: &rules.applied.prefix,
            network_prefix: &rules.applied.network_prefix,
        }
    }

    fn apply(&self, text: &str) -> String {
        with_prefixes(text, self.prefix, self.network_prefix)
    }

    fn saved(&self, query: &SavedQuery, origin: SavedOrigin) -> SavedEntry {
        SavedEntry {
            id: query.id.clone(),
            title: self.apply(&query.title),
            sql: self.apply(&query.sql),
            pin: query.pin.clone(),
            origin,
            read_only: false,
        }
    }

    fn pinned_title(&self, pinned: &Pinned) -> String {
        self.apply(pinned.title.as_deref().unwrap_or(&pinned.id))
    }

    fn pinned(&self, pinned: &Pinned, preset: &str) -> SavedEntry {
        SavedEntry {
            id: pinned.id.clone(),
            title: self.pinned_title(pinned),
            sql: self.apply(&pinned.sql),
            pin: pinned.home.clone(),
            origin: SavedOrigin::Preset {
                preset: preset.to_string(),
            },
            read_only: true,
        }
    }
}

/// The project's queries, then the global ones, then each applied preset's pinned queries in
/// preset order.
pub fn list(project: &Project, global: &[SavedQuery], rules: &EffectiveRules) -> Vec<SavedEntry> {
    let prefixes = Prefixes::of(rules);
    let own = project
        .saved
        .iter()
        .map(|q| prefixes.saved(q, SavedOrigin::Project));
    let shared = global
        .iter()
        .map(|q| prefixes.saved(q, SavedOrigin::Global));
    let presets = rules.applied.presets.iter().flat_map(|preset| {
        preset
            .pinned
            .iter()
            .map(|pinned| prefixes.pinned(pinned, &preset.name))
            .collect::<Vec<_>>()
    });
    own.chain(shared).chain(presets).collect()
}

/// The pinned query `id` of the applied preset `preset`, as a project query with a fresh id.
pub fn preset_copy(rules: &EffectiveRules, preset: &str, id: &str) -> AppResult<SavedQuery> {
    let pinned = rules
        .applied
        .presets
        .iter()
        .filter(|p| p.name == preset)
        .flat_map(|p| p.pinned.iter())
        .find(|p| p.id == id)
        .ok_or_else(|| {
            AppError::InvalidInput(format!("The preset {preset} has no pinned query {id}"))
        })?;
    let prefixes = Prefixes::of(rules);
    Ok(SavedQuery {
        id: new_id(),
        title: prefixes.pinned_title(pinned),
        sql: prefixes.apply(&pinned.sql),
        pin: pinned.home.clone(),
    })
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// The title without surrounding whitespace; a blank one is refused.
pub fn checked_title(title: &str) -> AppResult<String> {
    let title = title.trim();
    if title.is_empty() {
        return Err(AppError::InvalidInput("A saved query needs a title".into()));
    }
    Ok(title.to_string())
}

pub fn is_project_query(project: &Project, id: &str) -> bool {
    project.saved.iter().any(|q| q.id == id)
}

pub fn replace(queries: &mut [SavedQuery], query: &SavedQuery) -> AppResult<()> {
    let stored = queries
        .iter_mut()
        .find(|q| q.id == query.id)
        .ok_or_else(|| {
            AppError::InvalidInput(format!(
                "No saved query {} (preset queries are read-only)",
                query.id
            ))
        })?;
    *stored = query.clone();
    Ok(())
}

fn in_project(projects: &ProjectsStore, connection: &str, database: &str, id: &str) -> bool {
    projects.with_project(connection, database, |project| {
        project.is_some_and(|p| is_project_query(p, id))
    })
}

pub async fn add_to_project(
    projects: &ProjectsStore,
    connection: &str,
    database: &str,
    query: SavedQuery,
) -> AppResult<SavedQuery> {
    let stored = query.clone();
    projects
        .update(connection, database, move |project| {
            project.saved.push(stored);
            Ok(())
        })
        .await?;
    Ok(query)
}

/// Replaces the project query with `query.id`, else the global one; the title is trimmed.
pub async fn update(
    projects: &ProjectsStore,
    connection: &str,
    database: &str,
    mut query: SavedQuery,
) -> AppResult<SavedQuery> {
    query.title = checked_title(&query.title)?;
    let stored = query.clone();
    if in_project(projects, connection, database, &query.id) {
        projects
            .update(connection, database, move |project| {
                replace(&mut project.saved, &stored)
            })
            .await?;
    } else {
        projects
            .update_global(move |global| replace(global, &stored))
            .await?;
    }
    Ok(query)
}

/// Removes the project query `id`, else the global one.
pub async fn remove(
    projects: &ProjectsStore,
    connection: &str,
    database: &str,
    id: String,
) -> AppResult<()> {
    if in_project(projects, connection, database, &id) {
        projects
            .update(connection, database, move |project| {
                project.saved.retain(|q| q.id != id);
                Ok(())
            })
            .await?;
        return Ok(());
    }
    projects
        .update_global(move |global| {
            global.retain(|q| q.id != id);
            Ok(())
        })
        .await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::error::ErrorKind;
    use crate::rules::Preset;
    use crate::rules::preset;
    use crate::schema::DatabaseSchema;

    const ALPHA: &str = r#"
[preset]
name = "alpha"

[[pinned]]
id = "open"
title = "Open {p}orders"
sql = "SELECT * FROM {p}orders"
home = { show = "rows", limit = 5 }

[[pinned]]
id = "users"
sql = "SELECT COUNT(*) FROM {np}users"
"#;

    const BETA: &str = r#"
[preset]
name = "beta"

[[pinned]]
id = "late"
title = "Late"
sql = "SELECT 1"
"#;

    fn query(id: &str, title: &str, sql: &str, pin: Option<Home>) -> SavedQuery {
        SavedQuery {
            id: id.into(),
            title: title.into(),
            sql: sql.into(),
            pin,
        }
    }

    fn mine() -> SavedQuery {
        query(
            "mine",
            "Orders of {p}",
            "SELECT * FROM {p}orders WHERE id = 1",
            Some(Home::Count),
        )
    }

    fn shared() -> SavedQuery {
        query("shared", "Users", "SELECT * FROM {np}users", None)
    }

    fn project() -> Project {
        Project {
            prefix: Some("wp_".into()),
            network_prefix: Some("wp_net_".into()),
            presets: Some(vec!["beta".into(), "alpha".into()]),
            saved: vec![mine()],
            ..Project::new("local", "wp")
        }
    }

    fn rules(project: &Project) -> EffectiveRules {
        let loaded: Vec<Arc<Preset>> = [ALPHA, BETA]
            .into_iter()
            .map(|src| Arc::new(preset::parse("test.toml", src).unwrap_or_else(|e| panic!("{e}"))))
            .collect();
        let schema = DatabaseSchema {
            tables: Vec::new(),
            keys: Default::default(),
            columns: Default::default(),
            incoming: Default::default(),
        };
        EffectiveRules::build(project, &schema, &loaded)
    }

    fn entry(
        id: &str,
        title: &str,
        sql: &str,
        pin: Option<Home>,
        origin: SavedOrigin,
    ) -> SavedEntry {
        SavedEntry {
            id: id.into(),
            title: title.into(),
            sql: sql.into(),
            pin,
            read_only: matches!(origin, SavedOrigin::Preset { .. }),
            origin,
        }
    }

    fn preset_origin(name: &str) -> SavedOrigin {
        SavedOrigin::Preset {
            preset: name.into(),
        }
    }

    #[test]
    fn the_list_has_project_queries_then_global_then_each_presets_in_preset_order() {
        let project = project();

        let entries = list(&project, &[shared()], &rules(&project));

        assert_eq!(
            entries,
            [
                entry(
                    "mine",
                    "Orders of wp_",
                    "SELECT * FROM wp_orders WHERE id = 1",
                    Some(Home::Count),
                    SavedOrigin::Project,
                ),
                entry(
                    "shared",
                    "Users",
                    "SELECT * FROM wp_net_users",
                    None,
                    SavedOrigin::Global,
                ),
                entry("late", "Late", "SELECT 1", None, preset_origin("beta")),
                entry(
                    "open",
                    "Open wp_orders",
                    "SELECT * FROM wp_orders",
                    Some(Home::Rows { limit: Some(5) }),
                    preset_origin("alpha"),
                ),
                entry(
                    "users",
                    "users",
                    "SELECT COUNT(*) FROM wp_net_users",
                    None,
                    preset_origin("alpha"),
                ),
            ]
        );
    }

    #[test]
    fn project_and_global_entries_are_editable_and_preset_entries_are_read_only() {
        let project = project();

        let entries = list(&project, &[shared()], &rules(&project));

        let read_only: Vec<(&str, bool)> = entries
            .iter()
            .map(|e| (e.id.as_str(), e.read_only))
            .collect();
        assert_eq!(
            read_only,
            [
                ("mine", false),
                ("shared", false),
                ("late", true),
                ("open", true),
                ("users", true)
            ]
        );
    }

    #[test]
    fn a_project_without_queries_or_presets_lists_only_the_global_ones() {
        let project = Project {
            presets: Some(Vec::new()),
            saved: Vec::new(),
            ..project()
        };

        let entries = list(&project, &[shared()], &rules(&project));

        assert_eq!(
            entries,
            [entry(
                "shared",
                "Users",
                "SELECT * FROM wp_net_users",
                None,
                SavedOrigin::Global,
            )]
        );
    }

    #[test]
    fn entries_serialize_with_camel_case_fields_and_a_tagged_origin() {
        let preset = entry(
            "a",
            "A",
            "SELECT 1",
            Some(Home::Count),
            preset_origin("alpha"),
        );
        let own = entry("b", "B", "SELECT 2", None, SavedOrigin::Project);
        let global = entry("c", "C", "SELECT 3", None, SavedOrigin::Global);

        assert_eq!(
            serde_json::to_value(&preset).unwrap(),
            serde_json::json!({
                "id": "a",
                "title": "A",
                "sql": "SELECT 1",
                "pin": {"show": "count"},
                "origin": {"kind": "preset", "preset": "alpha"},
                "readOnly": true,
            })
        );
        assert_eq!(
            serde_json::to_value(&own).unwrap()["origin"],
            serde_json::json!({"kind": "project"})
        );
        assert_eq!(
            serde_json::to_value(&global).unwrap()["origin"],
            serde_json::json!({"kind": "global"})
        );
    }

    #[test]
    fn copying_a_preset_query_gives_an_editable_query_with_a_fresh_id_and_substituted_sql() {
        let project = project();
        let rules = rules(&project);

        let copy = preset_copy(&rules, "alpha", "open").unwrap();
        let again = preset_copy(&rules, "alpha", "open").unwrap();

        assert_eq!(
            SavedQuery {
                id: String::new(),
                ..copy.clone()
            },
            query(
                "",
                "Open wp_orders",
                "SELECT * FROM wp_orders",
                Some(Home::Rows { limit: Some(5) }),
            )
        );
        assert!(uuid::Uuid::parse_str(&copy.id).is_ok(), "{}", copy.id);
        assert_ne!(copy.id, again.id);
    }

    #[test]
    fn copying_an_untitled_preset_query_titles_it_with_its_id() {
        let project = project();

        let copy = preset_copy(&rules(&project), "alpha", "users").unwrap();

        assert_eq!(copy.title, "users");
        assert_eq!(copy.sql, "SELECT COUNT(*) FROM wp_net_users");
        assert_eq!(copy.pin, None);
    }

    #[test]
    fn copying_from_an_unknown_preset_or_with_an_unknown_id_is_rejected() {
        let project = project();
        let rules = rules(&project);

        for (preset, id) in [("nope", "open"), ("alpha", "late"), ("alpha", "missing")] {
            let err = preset_copy(&rules, preset, id).unwrap_err();

            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{preset}/{id}");
            assert_eq!(
                err.to_string(),
                format!("The preset {preset} has no pinned query {id}"),
                "{preset}/{id}"
            );
        }
    }

    #[test]
    fn a_title_is_trimmed_and_must_have_text() {
        for title in ["", "   ", "\t\n"] {
            let err = checked_title(title).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{title:?}");
        }
        assert_eq!(checked_title(" Orders ").unwrap(), "Orders");
    }

    #[test]
    fn replacing_swaps_the_query_with_the_same_id_only() {
        let mut queries = vec![mine(), shared()];
        let edited = query("shared", "Renamed", "SELECT 2", None);

        replace(&mut queries, &edited).unwrap();

        assert_eq!(queries, [mine(), edited]);
    }

    #[test]
    fn replacing_an_unknown_id_is_an_error_and_changes_nothing() {
        let mut queries = vec![mine()];

        let err = replace(&mut queries, &query("other", "x", "SELECT 3", None)).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(
            err.to_string(),
            "No saved query other (preset queries are read-only)"
        );
        assert_eq!(queries, [mine()]);
    }

    #[tokio::test]
    async fn an_added_query_lands_in_the_project_and_survives_a_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProjectsStore::open(dir.path());

        let added = add_to_project(&store, "local", "wp", mine()).await.unwrap();

        assert_eq!(added, mine());
        assert_eq!(
            ProjectsStore::open(dir.path()).get("local", "wp").saved,
            [mine()]
        );
        assert!(store.global().is_empty());
    }

    #[tokio::test]
    async fn updating_replaces_the_project_query_and_trims_its_title() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProjectsStore::open(dir.path());
        add_to_project(&store, "local", "wp", mine()).await.unwrap();
        store
            .update_global(|global| {
                global.push(query("mine", "Global twin", "SELECT 9", None));
                Ok(())
            })
            .await
            .unwrap();

        let updated = update(
            &store,
            "local",
            "wp",
            query("mine", "  Renamed  ", "SELECT 2", None),
        )
        .await
        .unwrap();

        let expected = query("mine", "Renamed", "SELECT 2", None);
        assert_eq!(updated, expected);
        assert_eq!(store.get("local", "wp").saved, [expected]);
        assert_eq!(
            store.global(),
            [query("mine", "Global twin", "SELECT 9", None)]
        );
    }

    #[tokio::test]
    async fn updating_a_query_that_is_not_in_the_project_replaces_the_global_one() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProjectsStore::open(dir.path());
        store
            .update_global(|global| {
                global.push(shared());
                Ok(())
            })
            .await
            .unwrap();

        update(
            &store,
            "local",
            "wp",
            query("shared", "Users", "SELECT 2", None),
        )
        .await
        .unwrap();

        assert_eq!(
            ProjectsStore::open(dir.path()).global(),
            [query("shared", "Users", "SELECT 2", None)]
        );
    }

    #[tokio::test]
    async fn updating_an_unknown_query_or_with_a_blank_title_is_rejected_and_stores_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProjectsStore::open(dir.path());
        add_to_project(&store, "local", "wp", mine()).await.unwrap();

        let unknown = update(
            &store,
            "local",
            "wp",
            query("preset-one", "T", "SELECT 1", None),
        )
        .await
        .unwrap_err();
        let blank = update(&store, "local", "wp", query("mine", "  ", "SELECT 1", None))
            .await
            .unwrap_err();

        assert_eq!(unknown.kind(), ErrorKind::InvalidInput);
        assert_eq!(blank.kind(), ErrorKind::InvalidInput);
        assert_eq!(store.get("local", "wp").saved, [mine()]);
        assert!(store.global().is_empty());
    }

    #[tokio::test]
    async fn removing_takes_the_project_query_before_the_global_one() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProjectsStore::open(dir.path());
        add_to_project(&store, "local", "wp", mine()).await.unwrap();
        store
            .update_global(|global| {
                global.extend([shared(), query("mine", "Global twin", "SELECT 9", None)]);
                Ok(())
            })
            .await
            .unwrap();

        remove(&store, "local", "wp", "mine".into()).await.unwrap();
        remove(&store, "local", "wp", "shared".into())
            .await
            .unwrap();

        assert!(store.get("local", "wp").saved.is_empty());
        assert_eq!(
            ProjectsStore::open(dir.path()).global(),
            [query("mine", "Global twin", "SELECT 9", None)]
        );
    }

    #[tokio::test]
    async fn removing_an_unknown_id_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProjectsStore::open(dir.path());
        add_to_project(&store, "local", "wp", mine()).await.unwrap();

        remove(&store, "local", "wp", "late".into()).await.unwrap();

        assert_eq!(store.get("local", "wp").saved, [mine()]);
    }
}
