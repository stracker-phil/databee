//! Edits to a project (`project_update`). Each change is checked against the database before it
//! is stored: the tables and columns a rule names must exist, and an error names the field.

use std::collections::HashSet;
use std::sync::Arc;

use serde::Deserialize;
use ts_rs::TS;

use super::applied::Applied;
use super::catalog::PresetCatalog;
use super::kinds::{KindRule, StyleRule};
use super::project::Project;
use super::types::{Check, ColumnGlob, Conditions, FieldError, Preset, Reference};
use crate::error::{AppError, AppResult};
use crate::schema::{self, DatabaseSchema};
use crate::session::Session;

/// Kind and style rules have no id, so they are addressed by their index in the project.
#[derive(Deserialize, Debug, Clone, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum ProjectChange {
    /// Without an `id`, the reference gets its default id (docs/presets.md).
    AddReference {
        reference: Reference,
    },
    /// An empty `id` in `reference` keeps the old one.
    UpdateReference {
        id: String,
        reference: Reference,
    },
    RemoveReference {
        id: String,
    },
    AddKind {
        rule: KindRule,
    },
    UpdateKind {
        index: usize,
        rule: KindRule,
    },
    RemoveKind {
        index: usize,
    },
    AddStyle {
        rule: StyleRule,
    },
    UpdateStyle {
        index: usize,
        rule: StyleRule,
    },
    RemoveStyle {
        index: usize,
    },
    /// Switches off a preset or inferred rule by its id.
    Disable {
        id: String,
    },
    Enable {
        id: String,
    },
    SetPrefix {
        prefix: Option<String>,
    },
    SetNetworkPrefix {
        prefix: Option<String>,
    },
    SetPresets {
        presets: Option<Vec<String>>,
    },
}

pub struct ChangeContext {
    pub schema: Arc<DatabaseSchema>,
    /// Every preset that loaded: `setPresets` names must be among them, and their marker and
    /// network tables suggest the prefixes when the project sets none.
    pub presets: Vec<Arc<Preset>>,
}

impl ChangeContext {
    pub async fn load(
        session: &Session,
        database: &str,
        presets: &PresetCatalog,
    ) -> AppResult<Self> {
        Ok(Self {
            schema: schema::database(session, database).await?,
            presets: presets.presets(),
        })
    }
}

fn invalid(error: FieldError) -> AppError {
    AppError::InvalidField {
        field: error.field,
        message: error.message,
    }
}

/// `field` prefixed to a rule's own field error (`reference.label`).
fn nested(section: &str, error: FieldError) -> FieldError {
    FieldError::new(format!("{section}.{}", error.field), error.message)
}

pub fn apply(project: &mut Project, change: ProjectChange, ctx: &ChangeContext) -> AppResult<()> {
    apply_checked(project, change, ctx).map_err(invalid)
}

/// Stores `reference` as `apply` does for `addReference`, or for `updateReference` of the
/// reference `replacing`, and returns the id it is stored under.
pub fn put_reference(
    project: &mut Project,
    reference: Reference,
    replacing: Option<&str>,
    ctx: &ChangeContext,
) -> AppResult<String> {
    put_checked(project, reference, replacing, ctx).map_err(invalid)
}

fn put_checked(
    project: &mut Project,
    mut reference: Reference,
    replacing: Option<&str>,
    ctx: &ChangeContext,
) -> Result<String, FieldError> {
    let index = replacing
        .map(|id| reference_index(&project.references, id))
        .transpose()?;
    Tables::new(project, ctx).check_reference(&reference)?;
    reference.id = new_reference_id(&project.references, &reference, index)?;
    let id = reference.id.clone();
    match index {
        Some(index) => project.references[index] = reference,
        None => project.references.push(reference),
    }
    Ok(id)
}

fn apply_checked(
    project: &mut Project,
    change: ProjectChange,
    ctx: &ChangeContext,
) -> Result<(), FieldError> {
    match change {
        ProjectChange::AddReference { reference } => {
            put_checked(project, reference, None, ctx)?;
        }
        ProjectChange::UpdateReference { id, reference } => {
            put_checked(project, reference, Some(&id), ctx)?;
        }
        ProjectChange::RemoveReference { id } => {
            let index = reference_index(&project.references, &id)?;
            project.references.remove(index);
        }
        ProjectChange::AddKind { rule } => {
            Tables::new(project, ctx).check_globs("kind", &rule.columns, &rule.in_meta)?;
            project.kinds.push(rule);
        }
        ProjectChange::UpdateKind { index, rule } => {
            check_index(index, project.kinds.len())?;
            Tables::new(project, ctx).check_globs("kind", &rule.columns, &rule.in_meta)?;
            project.kinds[index] = rule;
        }
        ProjectChange::RemoveKind { index } => {
            check_index(index, project.kinds.len())?;
            project.kinds.remove(index);
        }
        ProjectChange::AddStyle { rule } => {
            Tables::new(project, ctx).check_style(&rule)?;
            project.styles.push(rule);
        }
        ProjectChange::UpdateStyle { index, rule } => {
            check_index(index, project.styles.len())?;
            Tables::new(project, ctx).check_style(&rule)?;
            project.styles[index] = rule;
        }
        ProjectChange::RemoveStyle { index } => {
            check_index(index, project.styles.len())?;
            project.styles.remove(index);
        }
        ProjectChange::Disable { id } => {
            check_rule_id(&id)?;
            if !project.disabled.contains(&id) {
                project.disabled.push(id);
            }
        }
        ProjectChange::Enable { id } => {
            check_rule_id(&id)?;
            project.disabled.retain(|d| *d != id);
        }
        ProjectChange::SetPrefix { prefix } => {
            check_prefix(&ctx.schema, "prefix", prefix.as_deref())?;
            project.prefix = prefix;
        }
        ProjectChange::SetNetworkPrefix { prefix } => {
            check_prefix(&ctx.schema, "network_prefix", prefix.as_deref())?;
            project.network_prefix = prefix;
        }
        ProjectChange::SetPresets { presets } => {
            check_presets(presets.as_deref(), &ctx.presets)?;
            project.presets = presets;
        }
    }
    Ok(())
}

fn check_rule_id(id: &str) -> Result<(), FieldError> {
    if id.is_empty() {
        return Err(FieldError::new("id", "is empty"));
    }
    Ok(())
}

/// A prefix no table starts with is a typo: no rule could resolve under it.
fn check_prefix(
    schema: &DatabaseSchema,
    field: &str,
    prefix: Option<&str>,
) -> Result<(), FieldError> {
    let Some(prefix) = prefix else { return Ok(()) };
    if schema.tables.iter().any(|t| t.name.starts_with(prefix)) {
        return Ok(());
    }
    let message = format!("no table starts with `{prefix}`");
    Err(FieldError::new(field, message))
}

fn check_index(index: usize, len: usize) -> Result<(), FieldError> {
    if index >= len {
        let message = format!("{index} is out of range; the project has {len} such rules");
        return Err(FieldError::new("index", message));
    }
    Ok(())
}

fn reference_index(references: &[Reference], id: &str) -> Result<usize, FieldError> {
    references
        .iter()
        .position(|r| r.id == id)
        .ok_or_else(|| FieldError::new("id", format!("the project has no reference `{id}`")))
}

/// The id `reference` is stored under; `replacing` is the index it takes over, whose id it
/// keeps when none is given. A default id that is taken gets `-><to>`, as in presets.
fn new_reference_id(
    references: &[Reference],
    reference: &Reference,
    replacing: Option<usize>,
) -> Result<String, FieldError> {
    let taken = |id: &str| {
        references
            .iter()
            .enumerate()
            .any(|(i, r)| Some(i) != replacing && r.id == id)
    };
    if !reference.id.is_empty() {
        if taken(&reference.id) {
            let message = format!("`{}` is already used in this project", reference.id);
            return Err(FieldError::new("reference.id", message));
        }
        return Ok(reference.id.clone());
    }
    if let Some(index) = replacing {
        return Ok(references[index].id.clone());
    }
    let default = reference.default_id();
    if !taken(&default) {
        return Ok(default);
    }
    let qualified = reference.qualified_id();
    if taken(&qualified) {
        let message = format!("`{qualified}` is already used; give this reference an `id`");
        return Err(FieldError::new("reference.id", message));
    }
    Ok(qualified)
}

fn check_presets(names: Option<&[String]>, presets: &[Arc<Preset>]) -> Result<(), FieldError> {
    let Some(names) = names else { return Ok(()) };
    let mut seen = HashSet::new();
    for (i, name) in names.iter().enumerate() {
        let field = format!("presets[{i}]");
        if !presets.iter().any(|p| &p.name == name) {
            let message = format!("no preset `{name}` loads (see the presets list)");
            return Err(FieldError::new(field, message));
        }
        if !seen.insert(name) {
            return Err(FieldError::new(field, format!("`{name}` is listed twice")));
        }
    }
    Ok(())
}

/// Resolves rule table names (written without a prefix) against the schema as the effective
/// rules do (`Applied::existing_table`), so a rule that passes here checks the table it applies to.
struct Tables<'a> {
    schema: &'a DatabaseSchema,
    applied: Applied,
}

impl<'a> Tables<'a> {
    fn new(project: &Project, ctx: &'a ChangeContext) -> Self {
        let schema = ctx.schema.as_ref();
        Self {
            schema,
            applied: Applied::new(project, schema, &ctx.presets),
        }
    }

    /// The real table `name` stands for, under the prefix or the network prefix.
    fn resolve(&self, field: &str, name: &str) -> Result<String, FieldError> {
        if let Some(found) = self.applied.existing_table(name) {
            return Ok(found);
        }
        let prefixed = format!("{}{name}", self.applied.prefix);
        let networked = format!("{}{name}", self.applied.network_prefix);
        let message = if prefixed == networked {
            format!("no table `{prefixed}` in this database")
        } else {
            format!("no table `{prefixed}` or `{networked}` in this database")
        };
        Err(FieldError::new(field, message))
    }

    fn check_column(&self, field: &str, table: &str, column: &str) -> Result<(), FieldError> {
        let real = self.resolve(field, table)?;
        if self.schema.columns(&real).iter().any(|c| c == column) {
            return Ok(());
        }
        let message = format!("`{real}` has no column `{column}`");
        Err(FieldError::new(field, message))
    }

    fn check_conditions(
        &self,
        field: &str,
        table: &str,
        when: &Conditions,
    ) -> Result<(), FieldError> {
        for column in when.keys() {
            self.check_column(&format!("{field}.{column}"), table, column)?;
        }
        Ok(())
    }

    fn check_reference(&self, reference: &Reference) -> Result<(), FieldError> {
        reference.check().map_err(|e| nested("reference", e))?;
        let (from, to) = (&reference.from, &reference.to);
        self.check_column("reference.from", &from.table, &from.column)?;
        self.check_conditions("reference.when", &reference.from.table, &reference.when)?;
        self.check_column("reference.to", &to.table, &to.column)?;
        self.check_conditions("reference.to_when", &reference.to.table, &reference.to_when)
    }

    /// Globs match whatever exists, so only the literal parts are checked.
    fn check_globs(
        &self,
        section: &str,
        columns: &[ColumnGlob],
        in_meta: &[String],
    ) -> Result<(), FieldError> {
        for (i, glob) in columns.iter().enumerate() {
            let field = format!("{section}.columns[{i}]");
            if glob.table.contains('*') {
                continue;
            }
            if glob.column.contains('*') {
                self.resolve(&field, &glob.table)?;
            } else {
                self.check_column(&field, &glob.table, &glob.column)?;
            }
        }
        for (i, table) in in_meta.iter().enumerate() {
            self.resolve(&format!("{section}.in[{i}]"), table)?;
        }
        Ok(())
    }

    fn check_style(&self, rule: &StyleRule) -> Result<(), FieldError> {
        rule.check().map_err(|e| nested("style", e))?;
        self.check_globs("style", &rule.columns, &rule.r#in)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::db::TableInfo;
    use crate::error::ErrorKind;
    use crate::rules::kinds::{StyleColor, StyleMatch, ValueRule};
    use crate::rules::types::{ColumnRef, Match};

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
                    index: 2,
                    rule: style(&["posts.post_status"], &[], &[]),
                },
                "`index`: 2 is out of range; the project has 0 such rules",
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
    fn kinds_and_styles_are_added_updated_and_removed_by_index() {
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

        let first = style(&["posts.post_status"], &[], &[]);
        let second = style(&["posts.post_type"], &[], &[]);
        for rule in [first, second.clone()] {
            apply(&mut project, ProjectChange::AddStyle { rule }, &ctx()).unwrap();
        }
        apply(
            &mut project,
            ProjectChange::RemoveStyle { index: 0 },
            &ctx(),
        )
        .unwrap();
        assert_eq!(project.styles, [second]);
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
}
