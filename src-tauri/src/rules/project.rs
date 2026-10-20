//! Projects: the rules a user adds per connection and database (`projects.json`), on top of the
//! presets. Rules are stored in their preset shape, so a project exports as a preset file.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

use super::icons::IconRule;
use super::kinds::{KindRule, StyleRule};
use super::preset;
use super::queries::{Home, Pinned};
use super::types::{Preset, Reference};
use crate::error::{AppError, AppResult};
use crate::store::json_store::JsonStore;
use crate::store::settings::is_accent;

/// One database of one connection. Fields use the snake_case of the rule types, so the stored
/// rules and the project around them read alike.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct Project {
    /// The connection slug (`databee://<slug>/…`).
    pub connection: String,
    pub database: String,
    /// `None`: suggested from the presets' marker tables.
    #[serde(default)]
    pub prefix: Option<String>,
    /// Where `network_tables` live (multisite). `None`: suggested, else the prefix.
    #[serde(default)]
    pub network_prefix: Option<String>,
    /// Enabled presets by name. `None`: every preset whose marker tables exist.
    #[serde(default)]
    pub presets: Option<Vec<String>>,
    /// Added rules, and rules that override the preset rule with the same id, or the same `from`
    /// and `when`.
    #[serde(default)]
    pub references: Vec<Reference>,
    /// Ids of project, preset or inferred references switched off, and of preset styles that ship
    /// on and are switched off.
    #[serde(default)]
    pub disabled: Vec<String>,
    /// Ids of preset styles that ship off and are switched on.
    #[serde(default)]
    pub enabled: Vec<String>,
    #[serde(default)]
    pub kinds: Vec<KindRule>,
    /// Added styles, and styles that override the preset style with the same `id`.
    #[serde(default)]
    pub styles: Vec<StyleRule>,
    /// Table icons; one per `table`, which beats a preset's icon for the tables it names.
    #[serde(default, deserialize_with = "known_icons")]
    pub icons: Vec<IconRule>,
    #[serde(default)]
    pub saved: Vec<SavedQuery>,
    /// The accent while this database is open, one of the settings' accents; `None`: the app's.
    /// Personalization, not a rule: exports leave it out.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "known_accent"
    )]
    #[ts(optional)]
    pub accent: Option<String>,
}

/// A stored accent that is no longer (or never was) an accent loads as `None`, the app's accent.
fn known_accent<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = serde_json::Value::deserialize(deserializer)?;
    Ok(match raw {
        serde_json::Value::String(name) if is_accent(&name) => Some(name),
        serde_json::Value::Null => None,
        other => {
            log::warn!("Ignoring the unknown project accent {other}");
            None
        }
    })
}

/// A stored icon rule that no longer parses (an icon the set dropped, such as `none`) is left out,
/// so the presets' icon or `table` applies again.
fn known_icons<'de, D>(deserializer: D) -> Result<Vec<IconRule>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = Vec::<serde_json::Value>::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .filter_map(|rule| match IconRule::deserialize(&rule) {
            Ok(rule) => Some(rule),
            Err(e) => {
                log::warn!("Ignoring the project icon rule {rule}: {e}");
                None
            }
        })
        .collect())
}

/// A query the user saved, optionally pinned to the readout home. `pin` shares the shape of a
/// preset's `[[pinned]] home`, so a project query exports as a pinned query.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct SavedQuery {
    pub id: String,
    pub title: String,
    pub sql: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<Home>,
}

impl Project {
    pub fn new(connection: &str, database: &str) -> Self {
        Self {
            connection: connection.to_string(),
            database: database.to_string(),
            ..Self::default()
        }
    }

    pub fn key(&self) -> String {
        project_key(&self.connection, &self.database)
    }

    /// The project's rules as a preset named `<connection>-<database>`. `disabled`, the prefixes
    /// and the auto preset choice have no preset form; explicit presets become `requires`.
    pub fn to_preset(&self) -> Preset {
        Preset {
            name: format!("{}-{}", self.connection, self.database),
            requires: self.presets.clone().unwrap_or_default(),
            references: self.references.clone(),
            kinds: self.kinds.clone(),
            styles: self.styles.clone(),
            icons: self.icons.clone(),
            pinned: self.saved.iter().map(SavedQuery::to_pinned).collect(),
            ..Preset::default()
        }
    }

    /// Whether the preset style `rule` colors values here: one that ships on unless switched off,
    /// one that ships off when switched on.
    pub fn preset_style_on(&self, rule: &StyleRule) -> bool {
        if rule.enabled {
            !self.disabled.contains(&rule.id)
        } else {
            self.enabled.contains(&rule.id)
        }
    }

    /// Switches the preset style `rule` on or off; switching twice changes nothing.
    pub fn switch_preset_style(&mut self, rule: &StyleRule, on: bool) {
        let ids = if rule.enabled {
            &mut self.disabled
        } else {
            &mut self.enabled
        };
        let listed = on != rule.enabled;
        ids.retain(|id| *id != rule.id);
        if listed {
            ids.push(rule.id.clone());
        }
    }

    /// The project style that replaces the preset style `id`.
    pub fn style_override(&self, id: &str) -> Option<&StyleRule> {
        self.styles.iter().find(|s| s.id == id)
    }

    /// Gives styles stored before styles had ids their ids. A copy of a preset style (the old way
    /// to switch one on) becomes that style's switch, and an exact duplicate is dropped. Returns
    /// whether anything changed.
    pub fn migrate_styles(&mut self, presets: &[Arc<Preset>]) -> bool {
        if self.styles.iter().all(|s| !s.id.is_empty()) {
            return false;
        }
        let shipped: Vec<&StyleRule> = presets.iter().flat_map(|p| &p.styles).collect();
        let stored = std::mem::take(&mut self.styles);
        let mut taken: Vec<String> = stored
            .iter()
            .filter(|s| !s.id.is_empty())
            .map(|s| s.id.clone())
            .collect();
        for mut style in stored {
            if !style.id.is_empty() {
                self.styles.push(style);
                continue;
            }
            if let Some(copied) = shipped
                .iter()
                .find(|p| style.enabled && p.same_style(&style))
            {
                self.switch_preset_style(copied, true);
                continue;
            }
            if self.styles.iter().any(|s| s.same_style(&style)) {
                continue;
            }
            style.id = [style.default_id(), style.qualified_id()]
                .into_iter()
                .find(|id| !taken.contains(id))
                .unwrap_or_else(|| format!("{}#{}", style.qualified_id(), taken.len()));
            if let Some(overridden) = shipped.iter().find(|p| p.id == style.id) {
                self.switch_preset_style(overridden, true);
            }
            taken.push(style.id.clone());
            self.styles.push(style);
        }
        true
    }

    pub fn export(&self) -> AppResult<String> {
        preset::to_toml(&self.to_preset())
            .map_err(|e| AppError::Io(format!("Exporting {} failed: {e}", self.key())))
    }
}

impl SavedQuery {
    fn to_pinned(&self) -> Pinned {
        Pinned {
            id: self.id.clone(),
            title: (!self.title.is_empty()).then(|| self.title.clone()),
            sql: self.sql.clone(),
            home: self.pin.clone(),
        }
    }
}

/// `slug/database`. Slugs never hold a `/`, so the first one splits the key.
pub fn project_key(connection: &str, database: &str) -> String {
    format!("{connection}/{database}")
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
struct ProjectsFile {
    #[serde(default)]
    projects: BTreeMap<String, Project>,
    /// Saved queries offered in every project.
    #[serde(default)]
    global: Vec<SavedQuery>,
}

pub struct ProjectQueries {
    pub connection: String,
    pub database: String,
    pub queries: Vec<SavedQuery>,
}

pub struct ProjectsStore {
    store: JsonStore<ProjectsFile>,
}

impl ProjectsStore {
    pub fn open(dir: &Path) -> Self {
        Self {
            store: JsonStore::open(dir.join("projects.json")),
        }
    }

    /// The stored project, else an empty one: a database without a project uses presets only.
    pub fn get(&self, connection: &str, database: &str) -> Project {
        let key = project_key(connection, database);
        self.store
            .read(|file| file.projects.get(&key).cloned())
            .unwrap_or_else(|| Project::new(connection, database))
    }

    /// `read` of the stored project, `None` when there is none; nothing is cloned.
    pub fn with_project<R>(
        &self,
        connection: &str,
        database: &str,
        read: impl FnOnce(Option<&Project>) -> R,
    ) -> R {
        let key = project_key(connection, database);
        self.store.read(|file| read(file.projects.get(&key)))
    }

    /// Applies `change` to the project (an empty one if none is stored) and saves it. When
    /// `change` fails, the stored projects stay as they were (the file is rewritten unchanged).
    pub async fn update(
        &self,
        connection: &str,
        database: &str,
        change: impl FnOnce(&mut Project) -> AppResult<()> + Send + 'static,
    ) -> AppResult<Project> {
        let connection = connection.to_string();
        let database = database.to_string();
        // Inside the store's update, so the change sees the latest project.
        self.store
            .update_async(move |file| {
                let key = project_key(&connection, &database);
                let mut project = file
                    .projects
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| Project::new(&connection, &database));
                change(&mut project)?;
                file.projects.insert(key, project.clone());
                Ok(project)
            })
            .await?
    }

    /// The accent of every project that has one, by project key (`slug/database`).
    pub fn accents(&self) -> BTreeMap<String, String> {
        self.store.read(|file| {
            file.projects
                .iter()
                .filter_map(|(key, project)| Some((key.clone(), project.accent.clone()?)))
                .collect()
        })
    }

    /// Stores the database's accent; `None` goes back to the app's. An unknown accent is refused.
    pub async fn set_accent(
        &self,
        connection: &str,
        database: &str,
        accent: Option<String>,
    ) -> AppResult<Option<String>> {
        if let Some(name) = accent.as_deref().filter(|name| !is_accent(name)) {
            return Err(AppError::InvalidField {
                field: "accent".into(),
                message: format!("`{name}` is not an accent"),
            });
        }
        let project = self
            .update(connection, database, move |project| {
                project.accent = accent;
                Ok(())
            })
            .await?;
        Ok(project.accent)
    }

    pub fn global(&self) -> Vec<SavedQuery> {
        self.store.read(|file| file.global.clone())
    }

    /// Applies `change` to the global saved queries and saves them; a failed change saves nothing new.
    pub async fn update_global<R: Send + 'static>(
        &self,
        change: impl FnOnce(&mut Vec<SavedQuery>) -> AppResult<R> + Send + 'static,
    ) -> AppResult<R> {
        self.store
            .update_async(move |file| {
                let mut global = file.global.clone();
                let result = change(&mut global)?;
                file.global = global;
                Ok(result)
            })
            .await?
    }

    /// Appends saved queries to projects and to the global list in one save; blocks on the disk.
    pub fn import_saved(
        &self,
        projects: Vec<ProjectQueries>,
        global: Vec<SavedQuery>,
    ) -> AppResult<()> {
        self.store.update(move |file| {
            for entry in projects {
                let key = project_key(&entry.connection, &entry.database);
                file.projects
                    .entry(key)
                    .or_insert_with(|| Project::new(&entry.connection, &entry.database))
                    .saved
                    .extend(entry.queries);
            }
            file.global.extend(global);
        })
    }

    /// Runs `Project::migrate_styles` on every stored project; saves only when one changed.
    pub fn migrate_styles(&self, presets: &[Arc<Preset>]) -> AppResult<()> {
        let pending = self.store.read(|file| {
            file.projects
                .values()
                .any(|p| p.styles.iter().any(|s| s.id.is_empty()))
        });
        if !pending {
            return Ok(());
        }
        self.store.update(|file| {
            for project in file.projects.values_mut() {
                project.migrate_styles(presets);
            }
        })
    }

    /// Moves every project of the connection `old` to `new` after a slug rename. A project
    /// already stored under `new` belongs to no saved connection (slugs are unique) and gives way.
    pub async fn rename_connection(&self, old: &str, new: &str) -> AppResult<()> {
        if old == new {
            return Ok(());
        }
        let old_prefix = format!("{old}/");
        let new = new.to_string();
        self.store
            .update_async(move |file| {
                let keys: Vec<String> = file
                    .projects
                    .keys()
                    .filter(|key| key.starts_with(&old_prefix))
                    .cloned()
                    .collect();
                let moved: Vec<Project> = keys
                    .iter()
                    .filter_map(|key| file.projects.remove(key))
                    .collect();
                for mut project in moved {
                    project.connection = new.clone();
                    if let Some(orphan) = file.projects.insert(project.key(), project) {
                        log::warn!("Replaced the orphaned project {}", orphan.key());
                    }
                }
            })
            .await
    }
}

#[cfg(test)]
mod tests {
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
}
