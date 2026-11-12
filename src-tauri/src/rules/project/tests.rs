use std::collections::BTreeMap;
use std::path::Path;

use super::*;
use crate::error::ErrorKind;
use crate::rules::icons::TableIcon;
use crate::rules::kinds::{Money, StyleColor, StyleMatch, ValueRule, Zone};
use crate::rules::types::{ColumnGlob, ColumnRef, Conditions, Match};

fn column(text: &str) -> ColumnRef {
    ColumnRef::try_from(text.to_string()).unwrap()
}

fn glob(text: &str) -> ColumnGlob {
    ColumnGlob::try_from(text.to_string()).unwrap()
}

fn full_project() -> Project {
    Project {
        connection: "shop".into(),
        database: "wp".into(),
        prefix: Some("wp_2_".into()),
        network_prefix: Some("wp_".into()),
        presets: Some(vec!["core".into(), "shop".into()]),
        references: vec![
            Reference {
                id: "postmeta.meta_value?meta_key=_customer_user".into(),
                from: column("postmeta.meta_value"),
                when: Conditions::from([(
                    "meta_key".to_string(),
                    Match::Eq("_customer_user".into()),
                )]),
                to: column("users.ID"),
                to_when: Conditions::new(),
                label: Some("Orders".into()),
                columns: vec!["post_status".into()],
            },
            Reference {
                id: "posts.post_author".into(),
                from: column("posts.post_author"),
                when: Conditions::new(),
                to: column("users.ID"),
                to_when: Conditions::from([(
                    "user_status".to_string(),
                    Match::In(vec!["0".into(), "1".into()]),
                )]),
                label: None,
                columns: Vec::new(),
            },
        ],
        disabled: vec!["posts.post_parent".into()],
        enabled: vec!["style:posts.post_status=trash".into()],
        kinds: vec![
            KindRule {
                columns: vec![glob("posts.*_total")],
                meta_keys: Vec::new(),
                in_meta: Vec::new(),
                value: ValueRule::Money(Money {
                    currency: vec!["currency".into()],
                }),
            },
            KindRule {
                columns: Vec::new(),
                meta_keys: vec!["_paid_date".into()],
                in_meta: vec!["postmeta".into()],
                value: ValueRule::Timestamp(Zone::Utc),
            },
        ],
        styles: vec![
            StyleRule {
                id: "style:posts.post_status=wc-failed".into(),
                columns: vec![glob("posts.post_status")],
                meta_keys: Vec::new(),
                r#in: Vec::new(),
                matching: StyleMatch::Eq,
                value: "wc-failed".into(),
                color: StyleColor::Red,
                enabled: true,
            },
            StyleRule {
                id: "ok".into(),
                columns: Vec::new(),
                meta_keys: vec!["_status".into()],
                r#in: vec!["postmeta".into()],
                matching: StyleMatch::Like,
                value: "ok%".into(),
                color: StyleColor::Green,
                enabled: false,
            },
        ],
        icons: vec![
            IconRule {
                table: "users".into(),
                icon: TableIcon::User,
            },
            IconRule {
                table: "*meta".into(),
                icon: TableIcon::Table,
            },
        ],
        saved: vec![
            SavedQuery {
                id: "open".into(),
                title: "Open orders".into(),
                sql: "SELECT 1".into(),
                pin: Some(Home::Rows { limit: Some(5) }),
            },
            SavedQuery {
                id: "count".into(),
                title: String::new(),
                sql: "SELECT COUNT(*) FROM {p}posts".into(),
                pin: Some(Home::Count),
            },
            SavedQuery {
                id: "scratch".into(),
                title: "Scratch".into(),
                sql: "SELECT 2".into(),
                pin: None,
            },
        ],
        accent: None,
        watched: Vec::new(),
    }
}

async fn store_with(dir: &Path, projects: Vec<Project>) -> ProjectsStore {
    let store = ProjectsStore::open(dir);
    for project in projects {
        let (connection, database) = (project.connection.clone(), project.database.clone());
        store
            .update(&connection, &database, move |stored| {
                *stored = project;
                Ok(())
            })
            .await
            .unwrap();
    }
    store
}

#[tokio::test]
async fn a_full_project_survives_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let project = full_project();
    store_with(dir.path(), vec![project.clone()]).await;

    let reopened = ProjectsStore::open(dir.path());

    assert_eq!(reopened.get("shop", "wp"), project);
}

#[test]
fn a_database_without_a_project_gets_an_empty_one_naming_its_key() {
    let dir = tempfile::tempdir().unwrap();

    let project = ProjectsStore::open(dir.path()).get("shop", "wp");

    assert_eq!(
        project,
        Project {
            connection: "shop".into(),
            database: "wp".into(),
            ..Project::default()
        }
    );
    assert_eq!(project.key(), "shop/wp");
}

#[tokio::test]
async fn update_returns_the_stored_project() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProjectsStore::open(dir.path());

    let updated = store
        .update("shop", "wp", |p| {
            p.prefix = Some("wp_".into());
            Ok(())
        })
        .await
        .unwrap();

    assert_eq!(updated.prefix.as_deref(), Some("wp_"));
    assert_eq!(store.get("shop", "wp"), updated);
}

#[tokio::test]
async fn a_failed_change_leaves_the_stored_project_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let project = full_project();
    let store = store_with(dir.path(), vec![project.clone()]).await;

    let err = store
        .update("shop", "wp", |p| {
            p.prefix = Some("changed_".into());
            p.references.clear();
            Err(AppError::InvalidInput("nope".into()))
        })
        .await
        .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(store.get("shop", "wp"), project);
    assert_eq!(ProjectsStore::open(dir.path()).get("shop", "wp"), project);
}

#[tokio::test]
async fn a_failed_change_does_not_create_a_project() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProjectsStore::open(dir.path());

    store
        .update("shop", "wp", |p| {
            p.prefix = Some("x_".into());
            Err(AppError::InvalidInput("nope".into()))
        })
        .await
        .unwrap_err();

    assert_eq!(store.get("shop", "wp"), Project::new("shop", "wp"));
}

#[test]
fn export_is_a_preset_with_the_projects_rules() {
    let project = full_project();

    let preset = preset::parse("project.toml", &project.export().unwrap()).unwrap();

    assert_eq!(preset.name, "shop-wp");
    assert_eq!(preset.requires, ["core", "shop"]);
    assert_eq!(preset.references, project.references);
    assert_eq!(preset.kinds, project.kinds);
    assert_eq!(preset.styles, project.styles);
    assert_eq!(preset.icons, project.icons);
    assert_eq!(
        preset.pinned,
        [
            Pinned {
                id: "open".into(),
                title: Some("Open orders".into()),
                sql: "SELECT 1".into(),
                home: Some(Home::Rows { limit: Some(5) }),
            },
            Pinned {
                id: "count".into(),
                title: None,
                sql: "SELECT COUNT(*) FROM {p}posts".into(),
                home: Some(Home::Count),
            },
            Pinned {
                id: "scratch".into(),
                title: Some("Scratch".into()),
                sql: "SELECT 2".into(),
                home: None,
            },
        ]
    );
}

#[test]
fn an_accent_survives_json_and_an_unset_one_is_omitted() {
    let project = Project {
        accent: Some("gold".into()),
        ..Project::new("shop", "wp")
    };

    let json = serde_json::to_value(&project).unwrap();
    assert_eq!(json["accent"], serde_json::json!("gold"));
    assert_eq!(serde_json::from_value::<Project>(json).unwrap(), project);

    let unset = serde_json::to_value(Project::new("shop", "wp")).unwrap();
    assert!(unset.get("accent").is_none());
}

#[test]
fn a_stored_accent_that_is_not_an_accent_loads_as_none() {
    for accent in [
        serde_json::json!("pink"),
        serde_json::json!("neon"),
        serde_json::json!(42),
        serde_json::json!(["gold"]),
        serde_json::Value::Null,
    ] {
        let project: Project = serde_json::from_value(serde_json::json!({
            "connection": "shop",
            "database": "wp",
            "accent": accent,
        }))
        .unwrap();

        assert_eq!(project.accent, None, "{accent}");
    }
}

#[test]
fn the_accent_is_not_part_of_the_exported_preset() {
    let plain = full_project();
    let accented = Project {
        accent: Some("gold".into()),
        ..full_project()
    };

    assert_eq!(accented.to_preset(), plain.to_preset());
    assert_eq!(accented.export().unwrap(), plain.export().unwrap());
    assert!(!accented.export().unwrap().contains("gold"));
}

#[tokio::test]
async fn set_accent_stores_and_returns_the_accent_and_none_clears_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProjectsStore::open(dir.path());

    let set = store
        .set_accent("shop", "wp", Some("gold".into()))
        .await
        .unwrap();

    assert_eq!(set.as_deref(), Some("gold"));
    assert_eq!(store.get("shop", "wp").accent.as_deref(), Some("gold"));
    assert_eq!(
        ProjectsStore::open(dir.path())
            .get("shop", "wp")
            .accent
            .as_deref(),
        Some("gold")
    );

    let cleared = store.set_accent("shop", "wp", None).await.unwrap();

    assert_eq!(cleared, None);
    assert_eq!(store.get("shop", "wp").accent, None);
}

#[tokio::test]
async fn set_accent_keeps_the_projects_rules() {
    let dir = tempfile::tempdir().unwrap();
    let project = full_project();
    let store = store_with(dir.path(), vec![project.clone()]).await;

    store
        .set_accent("shop", "wp", Some("blue".into()))
        .await
        .unwrap();

    assert_eq!(
        store.get("shop", "wp"),
        Project {
            accent: Some("blue".into()),
            ..project
        }
    );
}

#[tokio::test]
async fn an_unknown_accent_is_refused_naming_the_field_and_stores_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(
        dir.path(),
        vec![Project {
            accent: Some("gold".into()),
            ..Project::new("shop", "wp")
        }],
    )
    .await;

    for name in ["pink", "Gold", ""] {
        let err = store
            .set_accent("shop", "wp", Some(name.into()))
            .await
            .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name:?}");
        assert!(
            matches!(&err, AppError::InvalidField { field, .. } if field == "accent"),
            "{name:?}: {err:?}"
        );
        assert_eq!(store.get("shop", "wp").accent.as_deref(), Some("gold"));
    }

    let other = store
        .set_accent("blog", "db", Some("pink".into()))
        .await
        .unwrap_err();
    assert_eq!(other.kind(), ErrorKind::InvalidInput);
    assert!(!store.accents().contains_key("blog/db"));
}

#[tokio::test]
async fn accents_lists_only_projects_with_one_keyed_by_slug_and_database() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(
        dir.path(),
        vec![
            Project {
                accent: Some("gold".into()),
                ..Project::new("shop", "wp")
            },
            Project {
                accent: Some("purple".into()),
                ..Project::new("shop", "staging")
            },
            Project::new("shop", "plain"),
        ],
    )
    .await;

    let expected = BTreeMap::from([
        ("shop/staging".to_string(), "purple".to_string()),
        ("shop/wp".to_string(), "gold".to_string()),
    ]);
    assert_eq!(store.accents(), expected);
    assert_eq!(ProjectsStore::open(dir.path()).accents(), expected);
    assert!(
        ProjectsStore::open(tempfile::tempdir().unwrap().path())
            .accents()
            .is_empty()
    );
}

#[tokio::test]
async fn renaming_a_connection_carries_the_accent() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(
        dir.path(),
        vec![Project {
            accent: Some("graphite".into()),
            ..Project::new("old", "db")
        }],
    )
    .await;

    store.rename_connection("old", "new").await.unwrap();

    assert_eq!(
        store.accents(),
        BTreeMap::from([("new/db".to_string(), "graphite".to_string())])
    );
}

#[test]
fn to_preset_carries_the_icons() {
    let project = full_project();

    assert_eq!(project.to_preset().icons, project.icons);
}

#[test]
fn a_stored_icon_rule_the_set_dropped_is_left_out_and_the_rest_loads() {
    let project: Project = serde_json::from_str(
        r#"{
            "connection": "shop",
            "database": "wp",
            "prefix": "wp_",
            "icons": [
                { "table": "users", "icon": "none" },
                { "table": "posts", "icon": "files" }
            ]
        }"#,
    )
    .unwrap();

    assert_eq!(
        project.icons,
        [IconRule {
            table: "posts".into(),
            icon: TableIcon::Files,
        }]
    );
    assert_eq!(
        (project.connection.as_str(), project.prefix.as_deref()),
        ("shop", Some("wp_"))
    );
}

#[test]
fn icons_that_are_not_a_list_stay_an_error() {
    let result = serde_json::from_str::<Project>(
        r#"{ "connection": "shop", "database": "wp", "icons": { "table": "users" } }"#,
    );

    assert!(result.is_err());
}

#[test]
fn a_project_stored_before_icons_existed_has_none() {
    let project: Project =
        serde_json::from_str(r#"{ "connection": "shop", "database": "wp" }"#).unwrap();

    assert!(project.icons.is_empty());
}

#[test]
fn export_leaves_out_prefixes_and_disabled_ids_and_an_automatic_preset_choice() {
    let project = Project {
        presets: None,
        ..full_project()
    };

    let preset = preset::parse("project.toml", &project.export().unwrap()).unwrap();

    assert!(preset.requires.is_empty());
    assert!(preset.marker_tables.is_empty());
    assert!(preset.network_tables.is_empty());
}

#[tokio::test]
async fn renaming_a_connection_moves_all_its_projects() {
    let dir = tempfile::tempdir().unwrap();
    let first = Project {
        prefix: Some("a_".into()),
        ..Project::new("old", "db1")
    };
    let second = Project {
        prefix: Some("b_".into()),
        ..Project::new("old", "db2")
    };
    let store = store_with(dir.path(), vec![first.clone(), second.clone()]).await;

    store.rename_connection("old", "new").await.unwrap();

    for store in [&store, &ProjectsStore::open(dir.path())] {
        assert_eq!(
            store.get("new", "db1"),
            Project {
                connection: "new".into(),
                ..first.clone()
            }
        );
        assert_eq!(
            store.get("new", "db2"),
            Project {
                connection: "new".into(),
                ..second.clone()
            }
        );
        assert_eq!(store.get("old", "db1"), Project::new("old", "db1"));
        assert_eq!(store.get("old", "db2"), Project::new("old", "db2"));
    }
}

#[tokio::test]
async fn renaming_a_connection_leaves_other_slugs_alone() {
    let dir = tempfile::tempdir().unwrap();
    let other = Project {
        prefix: Some("o_".into()),
        ..Project::new("older", "db")
    };
    let unrelated = Project {
        prefix: Some("u_".into()),
        ..Project::new("other", "db")
    };
    let projects = vec![Project::new("old", "db"), other.clone(), unrelated.clone()];
    let store = store_with(dir.path(), projects).await;

    store.rename_connection("old", "new").await.unwrap();

    assert_eq!(store.get("older", "db"), other);
    assert_eq!(store.get("other", "db"), unrelated);
}

#[tokio::test]
async fn renaming_a_connection_to_its_own_slug_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let project = full_project();
    let store = store_with(dir.path(), vec![project.clone()]).await;

    store.rename_connection("shop", "shop").await.unwrap();

    assert_eq!(store.get("shop", "wp"), project);
}

fn saved(id: &str) -> SavedQuery {
    SavedQuery {
        id: id.into(),
        title: id.to_uppercase(),
        sql: format!("SELECT '{id}'"),
        pin: None,
    }
}

#[test]
fn there_are_no_global_queries_before_any_is_stored() {
    let dir = tempfile::tempdir().unwrap();

    assert!(ProjectsStore::open(dir.path()).global().is_empty());
}

#[tokio::test]
async fn updating_the_global_queries_returns_the_changes_result_and_survives_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProjectsStore::open(dir.path());

    let count = store
        .update_global(|global| {
            global.extend([saved("a"), saved("b")]);
            Ok(global.len())
        })
        .await
        .unwrap();

    assert_eq!(count, 2);
    assert_eq!(store.global(), [saved("a"), saved("b")]);
    assert_eq!(
        ProjectsStore::open(dir.path()).global(),
        [saved("a"), saved("b")]
    );
}

#[tokio::test]
async fn a_failed_global_change_leaves_the_list_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProjectsStore::open(dir.path());
    store
        .update_global(|global| {
            global.push(saved("a"));
            Ok(())
        })
        .await
        .unwrap();

    let err = store
        .update_global(|global| {
            global.clear();
            global.push(saved("changed"));
            Err::<(), _>(AppError::InvalidInput("nope".into()))
        })
        .await
        .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(store.global(), [saved("a")]);
    assert_eq!(ProjectsStore::open(dir.path()).global(), [saved("a")]);
}

#[tokio::test]
async fn global_queries_do_not_touch_the_projects() {
    let dir = tempfile::tempdir().unwrap();
    let project = full_project();
    let store = store_with(dir.path(), vec![project.clone()]).await;

    store
        .update_global(|global| {
            global.push(saved("a"));
            Ok(())
        })
        .await
        .unwrap();

    assert_eq!(ProjectsStore::open(dir.path()).get("shop", "wp"), project);
}

#[tokio::test]
async fn imported_queries_are_appended_to_projects_and_the_global_list_and_survive_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let project = full_project();
    let existing = project.saved.clone();
    let store = store_with(dir.path(), vec![project.clone()]).await;
    store
        .update_global(|global| {
            global.push(saved("old"));
            Ok(())
        })
        .await
        .unwrap();

    store
        .import_saved(
            vec![
                ProjectQueries {
                    connection: "shop".into(),
                    database: "wp".into(),
                    queries: vec![saved("a")],
                },
                ProjectQueries {
                    connection: "blog".into(),
                    database: "db".into(),
                    queries: vec![saved("b"), saved("c")],
                },
            ],
            vec![saved("g")],
        )
        .unwrap();

    for store in [&store, &ProjectsStore::open(dir.path())] {
        let mut expected = existing.clone();
        expected.push(saved("a"));
        assert_eq!(
            store.get("shop", "wp"),
            Project {
                saved: expected,
                ..project.clone()
            }
        );
        assert_eq!(
            store.get("blog", "db"),
            Project {
                saved: vec![saved("b"), saved("c")],
                ..Project::new("blog", "db")
            }
        );
        assert_eq!(store.global(), [saved("old"), saved("g")]);
    }
}

#[test]
fn importing_nothing_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProjectsStore::open(dir.path());

    store.import_saved(Vec::new(), Vec::new()).unwrap();

    assert_eq!(store.get("shop", "wp"), Project::new("shop", "wp"));
    assert!(store.global().is_empty());
}

const SHIPPED: &str = r#"
[preset]
name = "shop"

[[style]]
columns = ["posts.post_status"]
match = "="
value = "failed"
color = "red"
enabled = false

[[style]]
columns = ["posts.post_status"]
match = "="
value = "done"
color = "green"
"#;

fn status_style(value: &str, color: StyleColor) -> StyleRule {
    StyleRule {
        id: String::new(),
        columns: vec![glob("posts.post_status")],
        meta_keys: Vec::new(),
        r#in: Vec::new(),
        matching: StyleMatch::Eq,
        value: value.into(),
        color,
        enabled: true,
    }
}

fn shipped() -> Vec<Arc<Preset>> {
    vec![Arc::new(preset::parse("shop.toml", SHIPPED).unwrap())]
}

#[test]
fn copies_of_a_preset_style_become_its_switch_and_other_styles_get_ids() {
    let failed = || status_style("failed", StyleColor::Red);
    let mut project = Project {
        disabled: vec!["style:posts.post_status=done".into()],
        styles: vec![
            failed(),
            failed(),
            status_style("open", StyleColor::Blue),
            failed(),
            status_style("open", StyleColor::Blue),
            status_style("open", StyleColor::Yellow),
            status_style("done", StyleColor::Green),
        ],
        ..Project::new("shop", "wp")
    };

    assert!(project.migrate_styles(&shipped()));

    assert_eq!(project.enabled, ["style:posts.post_status=failed"]);
    assert!(project.disabled.is_empty());
    let ids: Vec<&str> = project.styles.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "style:posts.post_status=open",
            "style:posts.post_status=open->yellow"
        ]
    );
    assert!(!project.migrate_styles(&shipped()));
}

#[test]
fn a_style_stored_without_id_under_a_preset_styles_id_overrides_it_and_switches_it_on() {
    let mut project = Project {
        styles: vec![status_style("failed", StyleColor::Purple)],
        ..Project::new("shop", "wp")
    };

    project.migrate_styles(&shipped());

    assert_eq!(project.styles[0].id, "style:posts.post_status=failed");
    assert_eq!(project.enabled, ["style:posts.post_status=failed"]);
}

#[tokio::test]
async fn the_store_migrates_and_saves_styles_stored_without_ids() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = Project {
        styles: vec![status_style("failed", StyleColor::Red)],
        ..Project::new("shop", "wp")
    };
    let store = store_with(dir.path(), vec![legacy]).await;

    store.migrate_styles(&shipped()).unwrap();

    let reopened = ProjectsStore::open(dir.path()).get("shop", "wp");
    assert!(reopened.styles.is_empty());
    assert_eq!(reopened.enabled, ["style:posts.post_status=failed"]);
}

#[tokio::test]
async fn with_project_sees_the_stored_project_or_none() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(dir.path(), vec![full_project()]).await;

    let stored = store.with_project("shop", "wp", |p| p.map(|p| p.database.clone()));
    let missing = store.with_project("shop", "other", |p| p.is_none());

    assert_eq!(stored.as_deref(), Some("wp"));
    assert!(missing);
}

fn watched_value(id: &str) -> WatchedValue {
    WatchedValue {
        id: id.into(),
        label: None,
        table: "wp_options".into(),
        column: "option_value".into(),
        row: RowKey(vec![crate::value::KeyPart {
            column: "option_name".into(),
            value: "siteurl".into(),
        }]),
        path: vec!["a".into()],
    }
}

fn watched_ids(store: &ProjectsStore) -> Vec<String> {
    store
        .get("shop", "wp")
        .watched
        .into_iter()
        .map(|w| w.id)
        .collect()
}

#[tokio::test]
async fn watching_the_same_value_again_keeps_the_first_watch() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProjectsStore::open(dir.path());

    let first = store.watch("shop", "wp", watched_value("a")).await.unwrap();
    let again = store.watch("shop", "wp", watched_value("b")).await.unwrap();

    assert_eq!((first.id.as_str(), again.id.as_str()), ("a", "a"));
    assert_eq!(watched_ids(&store), ["a"]);
}

#[tokio::test]
async fn a_watch_differing_in_table_column_row_or_path_is_another_value() {
    let variants: [(&str, fn(&mut WatchedValue)); 4] = [
        ("table", |w| w.table = "wp_usermeta".into()),
        ("column", |w| w.column = "option_name".into()),
        ("row", |w| w.row.0[0].value = "home".into()),
        ("path", |w| w.path = Vec::new()),
    ];

    for (what, change) in variants {
        let dir = tempfile::tempdir().unwrap();
        let store = ProjectsStore::open(dir.path());
        store.watch("shop", "wp", watched_value("a")).await.unwrap();
        let mut other = watched_value("b");
        change(&mut other);

        let added = store.watch("shop", "wp", other).await.unwrap();

        assert_eq!(added.id, "b", "{what}");
        assert_eq!(watched_ids(&store), ["a", "b"], "{what}");
    }
}

#[tokio::test]
async fn watches_keep_the_order_they_were_added_and_survive_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProjectsStore::open(dir.path());
    for (id, column) in [("c", "x"), ("a", "y"), ("b", "z")] {
        let mut watch = watched_value(id);
        watch.column = column.into();
        store.watch("shop", "wp", watch).await.unwrap();
    }

    assert_eq!(watched_ids(&store), ["c", "a", "b"]);
    assert_eq!(
        watched_ids(&ProjectsStore::open(dir.path())),
        ["c", "a", "b"]
    );
}

#[tokio::test]
async fn unwatch_removes_one_watch_and_refuses_an_unknown_id() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProjectsStore::open(dir.path());
    for (id, column) in [("a", "x"), ("b", "y")] {
        let mut watch = watched_value(id);
        watch.column = column.into();
        store.watch("shop", "wp", watch).await.unwrap();
    }

    store.unwatch("shop", "wp", "a").await.unwrap();
    let err = store.unwatch("shop", "wp", "a").await.unwrap_err();

    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(err.to_string(), "No watched value `a` in this project");
    assert_eq!(watched_ids(&store), ["b"]);
}

#[tokio::test]
async fn label_watched_stores_a_trimmed_label_and_a_blank_one_resets_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProjectsStore::open(dir.path());
    store.watch("shop", "wp", watched_value("a")).await.unwrap();

    let named = store
        .label_watched("shop", "wp", "a", Some("  Site URL ".into()))
        .await
        .unwrap();
    assert_eq!(named.label.as_deref(), Some("Site URL"));
    assert_eq!(
        store.get("shop", "wp").watched[0].label.as_deref(),
        Some("Site URL")
    );

    for blank in [Some("   ".to_string()), Some(String::new()), None] {
        store
            .label_watched("shop", "wp", "a", Some("Site URL".into()))
            .await
            .unwrap();

        let reset = store
            .label_watched("shop", "wp", "a", blank.clone())
            .await
            .unwrap();

        assert_eq!(reset.label, None, "{blank:?}");
        assert_eq!(store.get("shop", "wp").watched[0].label, None, "{blank:?}");
    }
}

#[tokio::test]
async fn label_watched_refuses_an_unknown_id_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProjectsStore::open(dir.path());
    store.watch("shop", "wp", watched_value("a")).await.unwrap();

    let err = store
        .label_watched("shop", "wp", "zzz", Some("x".into()))
        .await
        .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(err.to_string(), "No watched value `zzz` in this project");
    assert_eq!(store.get("shop", "wp").watched, [watched_value("a")]);
}

#[test]
fn a_label_defaults_to_table_column_and_path() {
    let mut watch = watched_value("a");
    assert_eq!(watch.label(), "wp_options.option_value › a");

    watch.path = Vec::new();
    assert_eq!(watch.label(), "wp_options.option_value");

    watch.path = vec!["a".into(), "b".into()];
    watch.label = Some("Site".into());
    assert_eq!(watch.label(), "Site");
}

#[test]
fn watched_values_are_left_out_of_the_export() {
    let mut watching = full_project();
    watching.watched = vec![watched_value("a")];

    assert_eq!(watching.export().unwrap(), full_project().export().unwrap());
}

#[test]
fn json_omits_an_empty_watched_list_and_names_the_fields_of_a_watch() {
    let empty = serde_json::to_value(Project::new("shop", "wp")).unwrap();
    let mut watching = Project::new("shop", "wp");
    watching.watched = vec![watched_value("a")];

    let json = serde_json::to_value(&watching).unwrap();

    assert!(empty.get("watched").is_none(), "{empty}");
    assert_eq!(
        json["watched"],
        serde_json::json!([{
            "id": "a",
            "table": "wp_options",
            "column": "option_value",
            "row": [{ "column": "option_name", "value": "siteurl" }],
            "path": ["a"],
        }])
    );
    let loaded: Project = serde_json::from_value(empty).unwrap();
    assert!(loaded.watched.is_empty());
}

#[tokio::test]
async fn a_slug_rename_carries_the_watches() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProjectsStore::open(dir.path());
    store.watch("old", "wp", watched_value("a")).await.unwrap();

    store.rename_connection("old", "new").await.unwrap();

    assert_eq!(store.get("new", "wp").watched, [watched_value("a")]);
    assert!(store.get("old", "wp").watched.is_empty());
}
