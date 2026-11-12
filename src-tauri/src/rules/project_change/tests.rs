use std::collections::BTreeMap;

use super::*;
use crate::db::TableInfo;
use crate::error::ErrorKind;
use crate::rules::icons::TableIcon;
use crate::rules::kinds::{StyleColor, StyleMatch, ValueRule};
use crate::rules::preset;
use crate::rules::types::{ColumnGlob, ColumnRef, Conditions, Match};

fn schema(tables: &[(&str, &[&str])]) -> DatabaseSchema {
    DatabaseSchema {
        tables: tables
            .iter()
            .map(|(name, _)| TableInfo {
                name: name.to_string(),
                is_view: false,
                rows_estimate: None,
                size_bytes: None,
                comment: String::new(),
            })
            .collect(),
        keys: BTreeMap::new(),
        columns: tables
            .iter()
            .map(|(name, columns)| {
                (
                    name.to_string(),
                    columns.iter().map(|c| c.to_string()).collect(),
                )
            })
            .collect(),
        incoming: BTreeMap::new(),
    }
}

fn wp_schema() -> DatabaseSchema {
    schema(&[
        (
            "wp_posts",
            &["ID", "post_author", "post_status", "post_type"],
        ),
        ("wp_users", &["ID", "user_login"]),
    ])
}

fn multisite() -> DatabaseSchema {
    schema(&[
        ("wp_posts", &["ID", "post_author"]),
        ("wp_users", &["ID", "user_login"]),
        ("wp_2_posts", &["ID", "post_author"]),
    ])
}

fn preset(name: &str, markers: &[&str], network: &[&str]) -> Arc<Preset> {
    let names = |items: &[&str]| items.iter().map(|s| s.to_string()).collect();
    Arc::new(Preset {
        name: name.into(),
        marker_tables: names(markers),
        network_tables: names(network),
        ..Preset::default()
    })
}

fn ctx_of(schema: DatabaseSchema, presets: Vec<Arc<Preset>>) -> ChangeContext {
    ChangeContext {
        schema: Arc::new(schema),
        presets,
    }
}

fn ctx() -> ChangeContext {
    ctx_of(wp_schema(), vec![preset("core", &["posts", "users"], &[])])
}

fn column(text: &str) -> ColumnRef {
    ColumnRef::try_from(text.to_string()).unwrap()
}

fn glob(text: &str) -> ColumnGlob {
    ColumnGlob::try_from(text.to_string()).unwrap()
}

fn reference(from: &str, to: &str) -> Reference {
    Reference {
        id: String::new(),
        from: column(from),
        when: Conditions::new(),
        to: column(to),
        to_when: Conditions::new(),
        label: None,
        columns: Vec::new(),
    }
}

fn kind(columns: &[&str]) -> KindRule {
    KindRule {
        columns: columns.iter().map(|c| glob(c)).collect(),
        meta_keys: Vec::new(),
        in_meta: Vec::new(),
        value: ValueRule::Number,
    }
}

fn style(columns: &[&str], meta_keys: &[&str], in_meta: &[&str]) -> StyleRule {
    StyleRule {
        id: String::new(),
        columns: columns.iter().map(|c| glob(c)).collect(),
        meta_keys: meta_keys.iter().map(|k| k.to_string()).collect(),
        r#in: in_meta.iter().map(|t| t.to_string()).collect(),
        matching: StyleMatch::Eq,
        value: "x".into(),
        color: StyleColor::Red,
        enabled: true,
    }
}

fn add(reference: Reference) -> ProjectChange {
    ProjectChange::AddReference { reference }
}

fn author_to_user() -> ProjectChange {
    add(reference("posts.post_author", "users.ID"))
}

fn project() -> Project {
    Project::new("shop", "wp")
}

fn rejection(project: &mut Project, change: ProjectChange, ctx: &ChangeContext) -> String {
    let err = apply(project, change, ctx).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    err.to_string()
}

#[test]
fn rejected_changes_name_the_field_and_the_reason() {
    let with_meta_key = Reference {
        when: Conditions::from([("meta_key".to_string(), Match::Eq("x".into()))]),
        ..reference("posts.post_author", "users.ID")
    };
    let cases = [
        (
            add(reference("orders.id", "users.ID")),
            "`reference.from`: no table `wp_orders` in this database",
        ),
        (
            add(reference("posts.nope", "users.ID")),
            "`reference.from`: `wp_posts` has no column `nope`",
        ),
        (
            add(reference("posts.post_author", "users.nope")),
            "`reference.to`: `wp_users` has no column `nope`",
        ),
        (
            add(with_meta_key),
            "`reference.when.meta_key`: `wp_posts` has no column `meta_key`",
        ),
        (
            ProjectChange::AddKind {
                rule: kind(&["posts.nope"]),
            },
            "`kind.columns[0]`: `wp_posts` has no column `nope`",
        ),
        (
            ProjectChange::AddStyle {
                rule: style(&[], &["_status"], &["nometa"]),
            },
            "`style.in[0]`: no table `wp_nometa` in this database",
        ),
        (
            ProjectChange::SetPresets {
                presets: Some(vec!["core".into(), "missing".into()]),
            },
            "`presets[1]`: no preset `missing` loads (see the presets list)",
        ),
        (
            ProjectChange::SetPresets {
                presets: Some(vec!["core".into(), "core".into()]),
            },
            "`presets[1]`: `core` is listed twice",
        ),
        (
            ProjectChange::RemoveKind { index: 0 },
            "`index`: 0 is out of range; the project has 0 such rules",
        ),
        (
            ProjectChange::UpdateStyle {
                id: "nope".into(),
                rule: style(&["posts.post_status"], &[], &[]),
            },
            "`id`: no value style `nope` in the project or its presets",
        ),
        (
            ProjectChange::RemoveStyle { id: "nope".into() },
            "`id`: the project has no value style `nope`",
        ),
        (
            ProjectChange::EnableStyle { id: "nope".into() },
            "`id`: no value style `nope` in the project or its presets",
        ),
        (
            ProjectChange::DisableStyle { id: "nope".into() },
            "`id`: no value style `nope` in the project or its presets",
        ),
        (
            ProjectChange::RemoveReference { id: "nope".into() },
            "`id`: the project has no reference `nope`",
        ),
        (
            ProjectChange::UpdateReference {
                id: "nope".into(),
                reference: reference("posts.post_author", "users.ID"),
            },
            "`id`: the project has no reference `nope`",
        ),
        (
            ProjectChange::Disable { id: String::new() },
            "`id`: is empty",
        ),
        (
            ProjectChange::Enable { id: String::new() },
            "`id`: is empty",
        ),
        (
            ProjectChange::SetPrefix {
                prefix: Some("zz_".into()),
            },
            "`prefix`: no table starts with `zz_`",
        ),
        (
            ProjectChange::SetNetworkPrefix {
                prefix: Some("zz_".into()),
            },
            "`network_prefix`: no table starts with `zz_`",
        ),
    ];

    for (change, message) in cases {
        let label = format!("{change:?}");
        let mut project = project();
        assert_eq!(rejection(&mut project, change, &ctx()), message, "{label}");
        assert_eq!(project, self::project(), "{label}");
    }
}

#[test]
fn a_rejected_change_leaves_a_populated_project_as_it_was() {
    let mut project = project();
    apply(&mut project, author_to_user(), &ctx()).unwrap();
    let before = project.clone();

    apply(
        &mut project,
        add(reference("posts.nope", "users.ID")),
        &ctx(),
    )
    .unwrap_err();

    assert_eq!(project, before);
}

#[test]
fn a_reference_without_an_id_gets_its_default_id() {
    let mut project = project();

    apply(&mut project, author_to_user(), &ctx()).unwrap();

    assert_eq!(project.references[0].id, "posts.post_author");
}

#[test]
fn a_colliding_default_id_gets_the_target_appended() {
    let mut project = project();
    apply(&mut project, author_to_user(), &ctx()).unwrap();

    apply(&mut project, author_to_user(), &ctx()).unwrap();

    let ids: Vec<&str> = project.references.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, ["posts.post_author", "posts.post_author->users.ID"]);
}

#[test]
fn a_third_identical_reference_asks_for_an_explicit_id() {
    let mut project = project();
    apply(&mut project, author_to_user(), &ctx()).unwrap();
    apply(&mut project, author_to_user(), &ctx()).unwrap();

    let message = rejection(&mut project, author_to_user(), &ctx());

    assert_eq!(
        message,
        "`reference.id`: `posts.post_author->users.ID` is already used; give this reference an `id`"
    );
}

#[test]
fn an_explicit_id_that_is_taken_is_rejected() {
    let mut project = project();
    let named = || Reference {
        id: "mine".into(),
        ..reference("posts.post_author", "users.ID")
    };
    apply(&mut project, add(named()), &ctx()).unwrap();

    let message = rejection(&mut project, add(named()), &ctx());

    assert_eq!(
        message,
        "`reference.id`: `mine` is already used in this project"
    );
}

#[test]
fn updating_a_reference_without_an_id_keeps_the_old_one() {
    let mut project = project();
    let first = Reference {
        id: "mine".into(),
        ..reference("posts.post_author", "users.ID")
    };
    apply(&mut project, add(first), &ctx()).unwrap();

    apply(
        &mut project,
        ProjectChange::UpdateReference {
            id: "mine".into(),
            reference: reference("posts.post_author", "users.user_login"),
        },
        &ctx(),
    )
    .unwrap();

    assert_eq!(project.references.len(), 1);
    assert_eq!(project.references[0].id, "mine");
    assert_eq!(project.references[0].to, column("users.user_login"));
}

#[test]
fn removing_a_reference_drops_it_by_id() {
    let mut project = project();
    apply(&mut project, author_to_user(), &ctx()).unwrap();

    apply(
        &mut project,
        ProjectChange::RemoveReference {
            id: "posts.post_author".into(),
        },
        &ctx(),
    )
    .unwrap();

    assert!(project.references.is_empty());
}

#[test]
fn kinds_are_added_updated_and_removed_by_index() {
    let mut project = project();
    for column in ["posts.post_status", "posts.post_type"] {
        let rule = kind(&[column]);
        apply(&mut project, ProjectChange::AddKind { rule }, &ctx()).unwrap();
    }
    let replacement = kind(&["users.user_login"]);
    apply(
        &mut project,
        ProjectChange::UpdateKind {
            index: 1,
            rule: replacement.clone(),
        },
        &ctx(),
    )
    .unwrap();
    apply(&mut project, ProjectChange::RemoveKind { index: 0 }, &ctx()).unwrap();

    assert_eq!(project.kinds, [replacement]);
}

const STYLED: &str = r#"
[preset]
name = "styled"
marker_tables = ["posts"]

[[style]]
columns = ["posts.post_status"]
match = "="
value = "trash"
color = "red"
enabled = false

[[style]]
id = "drafts"
columns = ["posts.post_status"]
match = "="
value = "draft"
color = "blue"
"#;

fn styled_ctx() -> ChangeContext {
    let styled = preset::parse("styled.toml", STYLED).unwrap();
    ctx_of(wp_schema(), vec![Arc::new(styled)])
}

fn style_ids(project: &Project) -> Vec<&str> {
    project.styles.iter().map(|s| s.id.as_str()).collect()
}

fn valued(value: &str, color: StyleColor) -> StyleRule {
    StyleRule {
        value: value.into(),
        color,
        ..style(&["posts.post_status"], &[], &[])
    }
}

#[test]
fn styles_are_added_with_their_default_id_and_updated_and_removed_by_id() {
    let mut project = project();
    let add = |rule| ProjectChange::AddStyle { rule };
    apply(
        &mut project,
        add(style(&["posts.post_status"], &[], &[])),
        &ctx(),
    )
    .unwrap();
    apply(
        &mut project,
        add(style(&["posts.post_type"], &[], &[])),
        &ctx(),
    )
    .unwrap();
    assert_eq!(
        style_ids(&project),
        ["style:posts.post_status=x", "style:posts.post_type=x"]
    );

    let green = valued("x", StyleColor::Green);
    apply(
        &mut project,
        ProjectChange::UpdateStyle {
            id: "style:posts.post_status=x".into(),
            rule: green,
        },
        &ctx(),
    )
    .unwrap();
    apply(
        &mut project,
        ProjectChange::RemoveStyle {
            id: "style:posts.post_type=x".into(),
        },
        &ctx(),
    )
    .unwrap();

    assert_eq!(style_ids(&project), ["style:posts.post_status=x"]);
    assert_eq!(project.styles[0].color, StyleColor::Green);
}

#[test]
fn a_style_with_a_taken_default_id_gets_its_color_appended_and_an_exact_copy_is_rejected() {
    let mut project = project();
    let add = |rule| ProjectChange::AddStyle { rule };
    apply(&mut project, add(valued("x", StyleColor::Red)), &ctx()).unwrap();

    apply(&mut project, add(valued("x", StyleColor::Blue)), &ctx()).unwrap();
    let message = rejection(&mut project, add(valued("x", StyleColor::Red)), &ctx());

    assert_eq!(
        style_ids(&project),
        [
            "style:posts.post_status=x",
            "style:posts.post_status=x->blue"
        ]
    );
    assert_eq!(
        message,
        "`style.value`: the project already has this value style"
    );
}

#[test]
fn switching_a_preset_style_on_or_off_twice_stores_its_id_once() {
    let ctx = styled_ctx();
    let mut project = project();
    let trash = || "style:posts.post_status=trash".to_string();
    let enable = |id: String| ProjectChange::EnableStyle { id };
    let disable = |id: String| ProjectChange::DisableStyle { id };

    apply(&mut project, enable(trash()), &ctx).unwrap();
    apply(&mut project, enable(trash()), &ctx).unwrap();
    apply(&mut project, disable("drafts".into()), &ctx).unwrap();
    apply(&mut project, disable("drafts".into()), &ctx).unwrap();
    assert_eq!(project.enabled, [trash()]);
    assert_eq!(project.disabled, ["drafts"]);
    assert!(project.styles.is_empty());

    apply(&mut project, disable(trash()), &ctx).unwrap();
    apply(&mut project, enable("drafts".into()), &ctx).unwrap();
    assert!(project.enabled.is_empty());
    assert!(project.disabled.is_empty());
}

#[test]
fn switching_a_project_style_sets_its_own_enabled_flag() {
    let mut project = project();
    let rule = valued("x", StyleColor::Red);
    let id = "style:posts.post_status=x";
    apply(&mut project, ProjectChange::AddStyle { rule }, &ctx()).unwrap();

    apply(
        &mut project,
        ProjectChange::DisableStyle { id: id.into() },
        &ctx(),
    )
    .unwrap();
    assert!(!project.styles[0].enabled);

    apply(
        &mut project,
        ProjectChange::EnableStyle { id: id.into() },
        &ctx(),
    )
    .unwrap();
    assert!(project.styles[0].enabled);
    assert!(project.enabled.is_empty() && project.disabled.is_empty());
}

#[test]
fn adding_a_reference_under_a_preset_references_id_stores_an_override() {
    let linked = preset::parse(
        "linked.toml",
        "[preset]\nname = \"linked\"\nmarker_tables = [\"posts\"]\n\n[[reference]]\nfrom = \"posts.post_author\"\nto = \"users.ID\"\n",
    )
    .unwrap();
    let ctx = ctx_of(wp_schema(), vec![Arc::new(linked)]);
    let mut project = project();
    let edited = Reference {
        id: "posts.post_author".into(),
        ..reference("posts.post_author", "users.user_login")
    };

    apply(&mut project, add(edited), &ctx).unwrap();

    assert_eq!(project.references.len(), 1);
    assert_eq!(project.references[0].id, "posts.post_author");
    assert_eq!(project.references[0].to, column("users.user_login"));
}

#[test]
fn editing_a_preset_style_stores_an_override_under_its_id() {
    let ctx = styled_ctx();
    let mut project = project();
    let edit = || ProjectChange::UpdateStyle {
        id: "drafts".into(),
        rule: valued("draft", StyleColor::Yellow),
    };

    apply(&mut project, edit(), &ctx).unwrap();
    apply(&mut project, edit(), &ctx).unwrap();

    assert_eq!(style_ids(&project), ["drafts"]);
    assert_eq!(project.styles[0].color, StyleColor::Yellow);
    assert!(project.enabled.is_empty() && project.disabled.is_empty());
}

#[test]
fn adding_a_style_under_a_preset_styles_id_overrides_it_and_switches_it_on() {
    let ctx = styled_ctx();
    let mut project = project();

    apply(
        &mut project,
        ProjectChange::AddStyle {
            rule: valued("trash", StyleColor::Purple),
        },
        &ctx,
    )
    .unwrap();

    assert_eq!(style_ids(&project), ["style:posts.post_status=trash"]);
    assert_eq!(project.enabled, ["style:posts.post_status=trash"]);
}

#[test]
fn globs_only_check_their_literal_parts() {
    let mut project = project();

    for rule in [
        kind(&["*.post_status"]),
        kind(&["posts.*_status"]),
        kind(&["*.*"]),
    ] {
        let label = format!("{rule:?}");
        apply(&mut project, ProjectChange::AddKind { rule }, &ctx())
            .unwrap_or_else(|e| panic!("{label}: {e}"));
    }
}

fn icon(table: &str, icon: TableIcon) -> IconRule {
    IconRule {
        table: table.into(),
        icon,
    }
}

fn set_icon(table: &str, icon_: TableIcon) -> ProjectChange {
    ProjectChange::SetIcon {
        rule: icon(table, icon_),
    }
}

fn invalid_field(result: AppResult<()>) -> (String, String) {
    match result.unwrap_err() {
        AppError::InvalidField { field, message } => (field, message),
        other => panic!("expected InvalidField, got {other:?}"),
    }
}

#[test]
fn setting_an_icon_adds_it_and_replaces_the_one_for_the_same_table() {
    let mut project = project();

    apply(&mut project, set_icon("users", TableIcon::User), &ctx()).unwrap();
    apply(&mut project, set_icon("posts", TableIcon::Files), &ctx()).unwrap();
    apply(&mut project, set_icon("users", TableIcon::Table), &ctx()).unwrap();

    assert_eq!(
        project.icons,
        [
            icon("users", TableIcon::Table),
            icon("posts", TableIcon::Files)
        ]
    );
}

#[test]
fn an_icon_for_a_table_that_does_not_exist_is_rejected() {
    let mut project = project();

    let (field, message) = invalid_field(apply(
        &mut project,
        set_icon("orders", TableIcon::Package),
        &ctx(),
    ));

    assert_eq!(field, "icon.table");
    assert_eq!(message, "no table `wp_orders` in this database");
    assert!(project.icons.is_empty());
}

#[test]
fn an_icon_without_a_table_name_is_rejected() {
    let mut project = project();

    let (field, _) = invalid_field(apply(&mut project, set_icon("", TableIcon::User), &ctx()));

    assert_eq!(field, "icon.table");
}

#[test]
fn an_icon_glob_is_not_checked_against_the_tables() {
    let mut project = project();

    apply(&mut project, set_icon("nothing*", TableIcon::Menu), &ctx()).unwrap();

    assert_eq!(project.icons, [icon("nothing*", TableIcon::Menu)]);
}

#[test]
fn removing_an_icon_drops_it_by_table_and_a_missing_one_is_rejected() {
    let mut project = project();
    apply(&mut project, set_icon("users", TableIcon::User), &ctx()).unwrap();
    apply(&mut project, set_icon("*meta", TableIcon::Menu), &ctx()).unwrap();
    let remove = |table: &str| ProjectChange::RemoveIcon {
        table: table.into(),
    };

    apply(&mut project, remove("users"), &ctx()).unwrap();
    let (field, message) = invalid_field(apply(&mut project, remove("users"), &ctx()));

    assert_eq!(project.icons, [icon("*meta", TableIcon::Menu)]);
    assert_eq!(field, "table");
    assert_eq!(message, "the project has no icon for `users`");
}

#[test]
fn icon_changes_read_from_the_pages_camel_case_tags() {
    let set: ProjectChange = serde_json::from_value(serde_json::json!({
        "op": "setIcon",
        "rule": { "table": "users", "icon": "user" },
    }))
    .unwrap();
    let remove: ProjectChange = serde_json::from_value(serde_json::json!({
        "op": "removeIcon",
        "table": "users",
    }))
    .unwrap();

    assert_eq!(set, set_icon("users", TableIcon::User));
    assert_eq!(
        remove,
        ProjectChange::RemoveIcon {
            table: "users".into()
        }
    );
}

#[test]
fn disabling_an_id_stores_it_once_and_enabling_removes_it() {
    let mut project = project();
    let disable = || ProjectChange::Disable {
        id: "posts.post_parent".into(),
    };

    apply(&mut project, disable(), &ctx()).unwrap();
    apply(&mut project, disable(), &ctx()).unwrap();
    assert_eq!(project.disabled, ["posts.post_parent"]);

    apply(
        &mut project,
        ProjectChange::Enable {
            id: "posts.post_parent".into(),
        },
        &ctx(),
    )
    .unwrap();
    assert!(project.disabled.is_empty());
}

#[test]
fn prefixes_that_start_a_table_are_stored_and_can_be_cleared() {
    let mut project = project();
    let set = |prefix: Option<&str>| ProjectChange::SetPrefix {
        prefix: prefix.map(str::to_string),
    };

    apply(&mut project, set(Some("wp_")), &ctx()).unwrap();
    assert_eq!(project.prefix.as_deref(), Some("wp_"));

    apply(&mut project, set(None), &ctx()).unwrap();
    assert_eq!(project.prefix, None);
}

#[test]
fn presets_that_load_are_stored() {
    let mut project = project();

    apply(
        &mut project,
        ProjectChange::SetPresets {
            presets: Some(vec!["core".into()]),
        },
        &ctx(),
    )
    .unwrap();

    assert_eq!(project.presets, Some(vec!["core".to_string()]));
}

#[test]
fn the_suggested_prefix_applies_when_the_project_sets_none() {
    let mut project = project();

    apply(&mut project, author_to_user(), &ctx()).unwrap();

    assert_eq!(project.prefix, None);
    assert_eq!(project.references.len(), 1);
}

#[test]
fn rules_do_not_resolve_when_no_prefix_is_suggested() {
    let no_presets = ctx_of(wp_schema(), Vec::new());
    let mut disabled = Project {
        presets: Some(Vec::new()),
        ..project()
    };

    let first = rejection(&mut project(), author_to_user(), &no_presets);
    let second = rejection(&mut disabled, author_to_user(), &ctx());

    for message in [first, second] {
        assert_eq!(
            message,
            "`reference.from`: no table `posts` in this database"
        );
    }
}

#[test]
fn network_tables_resolve_under_an_explicit_network_prefix() {
    let ctx = ctx_of(multisite(), Vec::new());
    let mut project = Project {
        prefix: Some("wp_2_".into()),
        network_prefix: Some("wp_".into()),
        ..project()
    };

    apply(&mut project, author_to_user(), &ctx).unwrap();

    assert_eq!(project.references[0].to, column("users.ID"));
}

#[test]
fn the_network_prefix_is_suggested_from_the_presets_network_tables() {
    let ctx = ctx_of(multisite(), vec![preset("core", &["posts"], &["users"])]);
    let mut project = Project {
        prefix: Some("wp_2_".into()),
        ..project()
    };

    apply(&mut project, author_to_user(), &ctx).unwrap();

    assert_eq!(project.network_prefix, None);
    assert_eq!(project.references.len(), 1);
}

#[test]
fn a_table_missing_under_the_site_prefix_is_rejected_without_a_network_prefix() {
    let ctx = ctx_of(multisite(), Vec::new());
    let mut project = Project {
        prefix: Some("wp_2_".into()),
        ..project()
    };

    let message = rejection(&mut project, author_to_user(), &ctx);

    assert_eq!(
        message,
        "`reference.to`: no table `wp_2_users` in this database"
    );
}

#[test]
fn errors_name_both_candidates_when_the_prefixes_differ() {
    let ctx = ctx_of(multisite(), Vec::new());
    let mut project = Project {
        prefix: Some("wp_2_".into()),
        network_prefix: Some("wp_".into()),
        ..project()
    };

    let message = rejection(&mut project, add(reference("orders.id", "users.ID")), &ctx);

    assert_eq!(
        message,
        "`reference.from`: no table `wp_2_orders` or `wp_orders` in this database"
    );
}

#[test]
fn a_preset_that_does_not_apply_declares_no_network_tables() {
    let ctx = ctx_of(multisite(), vec![preset("core", &["posts"], &["users"])]);
    let mut project = Project {
        prefix: Some("wp_2_".into()),
        presets: Some(Vec::new()),
        ..project()
    };

    let message = rejection(&mut project, author_to_user(), &ctx);

    assert_eq!(
        message,
        "`reference.to`: no table `wp_2_users` in this database"
    );
}

#[test]
fn a_network_table_that_also_exists_under_the_site_prefix_is_checked_where_it_applies() {
    let both = schema(&[
        ("wp_2_posts", &["ID", "post_author"]),
        ("wp_2_users", &["ID"]),
        ("wp_users", &["ID", "user_login"]),
    ]);
    let ctx = ctx_of(both, vec![preset("core", &["posts"], &["users"])]);
    let mut project = Project {
        prefix: Some("wp_2_".into()),
        network_prefix: Some("wp_".into()),
        ..project()
    };

    apply(
        &mut project,
        add(reference("posts.post_author", "users.user_login")),
        &ctx,
    )
    .unwrap();
    let message = rejection(
        &mut project,
        add(reference("posts.ID", "users.missing")),
        &ctx,
    );

    assert_eq!(project.references.len(), 1);
    assert_eq!(
        message,
        "`reference.to`: `wp_users` has no column `missing`"
    );
}
