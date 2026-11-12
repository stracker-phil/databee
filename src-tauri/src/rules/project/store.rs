//! `projects.json`: the projects of every connection and the global saved queries.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{Project, SavedQuery, WatchedValue, project_key};
use crate::error::{AppError, AppResult};
use crate::rules::types::Preset;
use crate::store::json_store::JsonStore;
use crate::store::settings::is_accent;

fn unknown_watch(id: &str) -> AppError {
    AppError::InvalidInput(format!("No watched value `{id}` in this project"))
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

    /// Adds `watched` unless the project already watches the same value; returns the stored watch.
    pub async fn watch(
        &self,
        connection: &str,
        database: &str,
        watched: WatchedValue,
    ) -> AppResult<WatchedValue> {
        let probe = watched.clone();
        let project = self
            .update(connection, database, move |project| {
                if !project.watched.iter().any(|w| w.same_value(&watched)) {
                    project.watched.push(watched);
                }
                Ok(())
            })
            .await?;
        Ok(project
            .watched
            .into_iter()
            .find(|w| w.same_value(&probe))
            .unwrap_or(probe))
    }

    pub async fn unwatch(&self, connection: &str, database: &str, id: &str) -> AppResult<()> {
        let id = id.to_string();
        self.update(connection, database, move |project| {
            let before = project.watched.len();
            project.watched.retain(|w| w.id != id);
            if project.watched.len() == before {
                return Err(unknown_watch(&id));
            }
            Ok(())
        })
        .await?;
        Ok(())
    }

    /// A blank label goes back to the default one.
    pub async fn label_watched(
        &self,
        connection: &str,
        database: &str,
        id: &str,
        label: Option<String>,
    ) -> AppResult<WatchedValue> {
        let id = id.to_string();
        let label = label
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty());
        let wanted = id.clone();
        let project = self
            .update(connection, database, move |project| {
                let watched = project
                    .watched
                    .iter_mut()
                    .find(|w| w.id == id)
                    .ok_or_else(|| unknown_watch(&id))?;
                watched.label = label;
                Ok(())
            })
            .await?;
        project
            .watched
            .into_iter()
            .find(|w| w.id == wanted)
            .ok_or_else(|| unknown_watch(&wanted))
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
