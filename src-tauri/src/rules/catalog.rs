//! The presets DataBee knows: the built-ins compiled in from `src-tauri/presets/*.toml` and the
//! user's `<config dir>/presets/*.toml`. A user preset replaces the built-in with the same name.
//! The directory is read again when a file's mtime, or the set of files, changes.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use serde::Serialize;
use ts_rs::TS;

use super::preset;
use super::types::Preset;
use crate::lock::MutexExt;

pub const BUILT_IN: &[(&str, &str)] = &[
    (
        "wordpress.toml",
        include_str!("../../presets/wordpress.toml"),
    ),
    (
        "woocommerce.toml",
        include_str!("../../presets/woocommerce.toml"),
    ),
    (
        "action-scheduler.toml",
        include_str!("../../presets/action-scheduler.toml"),
    ),
];

#[derive(Serialize, Debug, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PresetSource {
    BuiltIn,
    User { path: String },
}

/// One row of `presets_list`. A preset with an `error` does not apply.
#[derive(Serialize, Debug, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct PresetInfo {
    /// The preset's name; for a file that does not parse, its file stem.
    pub name: String,
    pub source: PresetSource,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PresetEntry {
    pub info: PresetInfo,
    /// `None` when `info.error` is set.
    pub preset: Option<Arc<Preset>>,
}

/// The files of the user directory and their mtimes; a missing directory is an empty version.
type Version = Vec<(PathBuf, Option<SystemTime>)>;

pub struct PresetCatalog {
    dir: PathBuf,
    built_in: &'static [(&'static str, &'static str)],
    cached: Mutex<Option<(Version, Arc<[PresetEntry]>)>>,
}

impl PresetCatalog {
    pub fn open(config_dir: &Path) -> Self {
        Self::with_built_in(config_dir.join("presets"), BUILT_IN)
    }

    pub fn with_built_in(dir: PathBuf, built_in: &'static [(&'static str, &'static str)]) -> Self {
        Self {
            dir,
            built_in,
            cached: Mutex::new(None),
        }
    }

    pub fn list(&self) -> Vec<PresetInfo> {
        self.entries().iter().map(|e| e.info.clone()).collect()
    }

    /// Every preset that loaded without error, built-ins first.
    pub fn presets(&self) -> Vec<Arc<Preset>> {
        loaded(&self.entries())
    }

    pub fn entries(&self) -> Arc<[PresetEntry]> {
        let version = self.version();
        let mut cached = self.cached.lock_unpoisoned();
        if let Some((v, entries)) = cached.as_ref()
            && *v == version
        {
            return entries.clone();
        }
        let entries: Arc<[PresetEntry]> = self.load(&version).into();
        *cached = Some((version, entries.clone()));
        entries
    }

    fn version(&self) -> Version {
        let Ok(dir) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut files: Version = dir
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|ext| ext == "toml"))
            .map(|p| {
                let modified = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
                (p, modified)
            })
            .collect();
        files.sort();
        files
    }

    fn load(&self, version: &Version) -> Vec<PresetEntry> {
        let mut entries: Vec<PresetEntry> = self
            .built_in
            .iter()
            .map(|(file, src)| {
                let stem = file.trim_end_matches(".toml");
                entry(
                    &format!("built-in {file}"),
                    stem,
                    src,
                    PresetSource::BuiltIn,
                )
            })
            .collect();
        for (path, _) in version {
            let label = path.display().to_string();
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            let source = PresetSource::User {
                path: label.clone(),
            };
            let user = match std::fs::read_to_string(path) {
                Ok(src) => entry(&label, &stem, &src, source),
                Err(e) => failed(&stem, source, format!("{label}: {e}")),
            };
            add_user(&mut entries, user);
        }
        check_requires(&mut entries);
        entries
    }
}

/// The presets of `entries` that loaded without error.
pub(super) fn loaded(entries: &[PresetEntry]) -> Vec<Arc<Preset>> {
    entries.iter().filter_map(|e| e.preset.clone()).collect()
}

/// `label` names the file in errors; a file that does not parse is listed under `stem`.
fn entry(label: &str, stem: &str, src: &str, source: PresetSource) -> PresetEntry {
    match preset::parse(label, src) {
        Ok(preset) => PresetEntry {
            info: PresetInfo {
                name: preset.name.clone(),
                source,
                error: None,
            },
            preset: Some(Arc::new(preset)),
        },
        Err(e) => failed(stem, source, e.to_string()),
    }
}

fn failed(name: &str, source: PresetSource, error: String) -> PresetEntry {
    PresetEntry {
        info: PresetInfo {
            name: name.to_string(),
            source,
            error: Some(error),
        },
        preset: None,
    }
}

/// A user preset takes the place of the built-in with its name; a second user file with the
/// same name is an error, since which one wins would depend on file order.
fn add_user(entries: &mut Vec<PresetEntry>, user: PresetEntry) {
    if user.preset.is_none() {
        entries.push(user);
        return;
    }
    let same_name = entries
        .iter()
        .position(|e| e.preset.is_some() && e.info.name == user.info.name);
    match same_name.map(|i| (i, &entries[i].info.source)) {
        Some((i, PresetSource::BuiltIn)) => entries[i] = user,
        Some((_, PresetSource::User { path })) => {
            let message = format!("`{}` is already defined in {path}", user.info.name);
            entries.push(failed(&user.info.name, user.info.source, message));
        }
        None => entries.push(user),
    }
}

/// Marks presets whose `requires` names a missing or failed preset, or forms a cycle, until no
/// more change: a failure makes everything that requires it fail too.
fn check_requires(entries: &mut [PresetEntry]) {
    loop {
        let usable: HashMap<String, Arc<Preset>> = entries
            .iter()
            .filter_map(|e| e.preset.clone().map(|p| (p.name.clone(), p)))
            .collect();
        let broken = entries.iter().enumerate().find_map(|(i, e)| {
            let p = e.preset.as_ref()?;
            requires_order(&usable, std::slice::from_ref(&p.name))
                .err()
                .map(|error| (i, error))
        });
        let Some((i, error)) = broken else { return };
        entries[i].info.error = Some(error);
        entries[i].preset = None;
    }
}

/// `wanted` and every preset they require, each after the presets it requires, in the order
/// `wanted` names them. Errors name a missing preset or a cycle.
pub fn requires_order(
    presets: &HashMap<String, Arc<Preset>>,
    wanted: &[String],
) -> Result<Vec<Arc<Preset>>, String> {
    let mut ordered = Vec::new();
    let mut done = HashSet::new();
    for name in wanted {
        visit(presets, name, &mut Vec::new(), &mut done, &mut ordered)?;
    }
    Ok(ordered)
}

fn visit(
    presets: &HashMap<String, Arc<Preset>>,
    name: &str,
    path: &mut Vec<String>,
    done: &mut HashSet<String>,
    ordered: &mut Vec<Arc<Preset>>,
) -> Result<(), String> {
    if done.contains(name) {
        return Ok(());
    }
    if path.iter().any(|p| p == name) {
        return Err(format!(
            "`requires` forms a cycle: {} -> {name}",
            path.join(" -> ")
        ));
    }
    let Some(preset) = presets.get(name) else {
        return Err(match path.last() {
            Some(by) => format!("`{by}` requires `{name}`, which is not available"),
            None => format!("no preset `{name}`"),
        });
    };
    path.push(name.to_string());
    for required in &preset.requires {
        visit(presets, required, path, done, ordered)?;
    }
    path.pop();
    done.insert(name.to_string());
    ordered.push(preset.clone());
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Duration;

    use tempfile::{TempDir, tempdir};

    use super::*;

    const BLOG: &str = "[preset]\nname = \"blog\"\nmarker_tables = [\"posts\"]\n";
    static BLOG_ONLY: &[(&str, &str)] = &[("blog.toml", BLOG)];

    fn source(name: &str, requires: &[&str]) -> String {
        let requires: Vec<String> = requires.iter().map(|r| format!("\"{r}\"")).collect();
        format!(
            "[preset]\nname = \"{name}\"\nrequires = [{}]\n",
            requires.join(", ")
        )
    }

    /// A catalog over `<tempdir>/presets`, which does not exist until a test writes into it.
    fn catalog(dir: &TempDir, built_in: &'static [(&'static str, &'static str)]) -> PresetCatalog {
        PresetCatalog::with_built_in(dir.path().join("presets"), built_in)
    }

    /// Writes `presets/<file>` and pins its mtime, so a rewrite is seen regardless of the
    /// filesystem's timestamp resolution.
    fn write(dir: &TempDir, file: &str, content: &str, mtime_secs: u64) -> PathBuf {
        let presets = dir.path().join("presets");
        fs::create_dir_all(&presets).unwrap();
        let path = presets.join(file);
        fs::write(&path, content).unwrap();
        let handle = fs::OpenOptions::new().write(true).open(&path).unwrap();
        handle
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(mtime_secs))
            .unwrap();
        path
    }

    fn user(path: &Path) -> PresetSource {
        PresetSource::User {
            path: path.display().to_string(),
        }
    }

    fn names(catalog: &PresetCatalog) -> Vec<String> {
        catalog.list().into_iter().map(|i| i.name).collect()
    }

    fn by_name(presets: &[(&str, &str)]) -> HashMap<String, Arc<Preset>> {
        presets
            .iter()
            .map(|(name, requires)| {
                let requires: Vec<&str> = requires.split_whitespace().collect();
                let parsed = preset::parse(name, &source(name, &requires)).unwrap();
                (name.to_string(), Arc::new(parsed))
            })
            .collect()
    }

    fn order(map: &HashMap<String, Arc<Preset>>, wanted: &[&str]) -> Result<Vec<String>, String> {
        let wanted: Vec<String> = wanted.iter().map(|w| w.to_string()).collect();
        requires_order(map, &wanted).map(|presets| presets.iter().map(|p| p.name.clone()).collect())
    }

    #[test]
    fn missing_directory_lists_the_built_ins_only() {
        let dir = tempdir().unwrap();
        assert_eq!(
            catalog(&dir, BLOG_ONLY).list(),
            [PresetInfo {
                name: "blog".into(),
                source: PresetSource::BuiltIn,
                error: None
            }]
        );
    }

    #[test]
    fn without_built_ins_or_directory_nothing_is_listed() {
        let dir = tempdir().unwrap();
        assert!(catalog(&dir, &[]).list().is_empty());
    }

    #[test]
    fn user_preset_replaces_the_built_in_of_the_same_name() {
        let dir = tempdir().unwrap();
        let path = write(
            &dir,
            "mine.toml",
            "[preset]\nname = \"blog\"\nmarker_tables = [\"entries\"]\n",
            100,
        );
        let catalog = catalog(&dir, BLOG_ONLY);
        assert_eq!(
            catalog.list(),
            [PresetInfo {
                name: "blog".into(),
                source: user(&path),
                error: None
            }]
        );
        let presets = catalog.presets();
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].marker_tables, ["entries"]);
    }

    #[test]
    fn built_in_presets_come_before_user_presets() {
        let dir = tempdir().unwrap();
        write(&dir, "a.toml", &source("alpha", &[]), 100);
        assert_eq!(names(&catalog(&dir, BLOG_ONLY)), ["blog", "alpha"]);
    }

    #[test]
    fn second_user_file_with_the_same_name_errors() {
        let dir = tempdir().unwrap();
        let first = write(&dir, "a.toml", &source("shop", &[]), 100);
        let second = write(&dir, "b.toml", &source("shop", &[]), 100);
        let catalog = catalog(&dir, &[]);
        assert_eq!(
            catalog.list(),
            [
                PresetInfo {
                    name: "shop".into(),
                    source: user(&first),
                    error: None
                },
                PresetInfo {
                    name: "shop".into(),
                    source: user(&second),
                    error: Some(format!("`shop` is already defined in {}", first.display())),
                },
            ]
        );
        assert_eq!(catalog.presets().len(), 1);
    }

    #[test]
    fn broken_file_is_listed_by_its_stem_and_the_built_in_stays() {
        let dir = tempdir().unwrap();
        let path = write(&dir, "blog.toml", "[preset\nname = ", 100);
        let catalog = catalog(&dir, BLOG_ONLY);
        let list = catalog.list();
        assert_eq!(list.len(), 2);
        assert_eq!(
            list[0],
            PresetInfo {
                name: "blog".into(),
                source: PresetSource::BuiltIn,
                error: None
            }
        );
        assert_eq!(list[1].name, "blog");
        assert_eq!(list[1].source, user(&path));
        let error = list[1]
            .error
            .as_deref()
            .expect("the broken file has an error");
        assert!(error.starts_with(&path.display().to_string()), "{error}");
        let presets = catalog.presets();
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].marker_tables, ["posts"]);
    }

    #[test]
    fn broken_built_in_is_listed_by_its_file_stem() {
        static BROKEN: &[(&str, &str)] = &[("shop.toml", "[preset\n")];
        let dir = tempdir().unwrap();
        let catalog = catalog(&dir, BROKEN);
        let list = catalog.list();
        assert_eq!(
            (list[0].name.as_str(), &list[0].source),
            ("shop", &PresetSource::BuiltIn)
        );
        assert!(
            list[0]
                .error
                .as_deref()
                .unwrap()
                .starts_with("built-in shop.toml: ")
        );
        assert!(catalog.presets().is_empty());
    }

    #[test]
    fn requiring_a_missing_preset_errors() {
        let dir = tempdir().unwrap();
        write(&dir, "shop.toml", &source("shop", &["ghost"]), 100);
        let catalog = catalog(&dir, &[]);
        assert_eq!(
            catalog.list()[0].error.as_deref(),
            Some("`shop` requires `ghost`, which is not available")
        );
        assert!(catalog.presets().is_empty());
    }

    #[test]
    fn requiring_a_present_preset_is_fine() {
        let dir = tempdir().unwrap();
        write(&dir, "shop.toml", &source("shop", &["blog"]), 100);
        let catalog = catalog(&dir, BLOG_ONLY);
        assert!(catalog.list().iter().all(|i| i.error.is_none()));
        assert_eq!(catalog.presets().len(), 2);
    }

    #[test]
    fn requires_cycle_errors() {
        let dir = tempdir().unwrap();
        write(&dir, "a.toml", &source("a", &["b"]), 100);
        write(&dir, "b.toml", &source("b", &["a"]), 100);
        let catalog = catalog(&dir, &[]);
        let list = catalog.list();
        assert_eq!(
            list[0].error.as_deref(),
            Some("`requires` forms a cycle: a -> b -> a")
        );
        assert!(list[1].error.is_some());
        assert!(catalog.presets().is_empty());
    }

    #[test]
    fn requiring_a_broken_preset_errors() {
        let dir = tempdir().unwrap();
        write(&dir, "bad.toml", "[preset\n", 100);
        write(&dir, "shop.toml", &source("shop", &["bad"]), 100);
        let catalog = catalog(&dir, &[]);
        let list = catalog.list();
        assert_eq!(list[0].name, "bad");
        assert!(list[0].error.is_some());
        assert_eq!(list[1].name, "shop");
        assert_eq!(
            list[1].error.as_deref(),
            Some("`shop` requires `bad`, which is not available")
        );
    }

    #[test]
    fn a_failure_propagates_to_everything_that_requires_it() {
        let dir = tempdir().unwrap();
        write(&dir, "a.toml", &source("a", &["ghost"]), 100);
        write(&dir, "b.toml", &source("b", &["a"]), 100);
        write(&dir, "c.toml", &source("c", &[]), 100);
        let catalog = catalog(&dir, &[]);
        let errors: Vec<bool> = catalog.list().iter().map(|i| i.error.is_some()).collect();
        assert_eq!(errors, [true, true, false]);
    }

    #[test]
    fn added_file_is_picked_up() {
        let dir = tempdir().unwrap();
        write(&dir, "a.toml", &source("alpha", &[]), 100);
        let catalog = catalog(&dir, &[]);
        assert_eq!(names(&catalog), ["alpha"]);
        write(&dir, "b.toml", &source("beta", &[]), 100);
        assert_eq!(names(&catalog), ["alpha", "beta"]);
    }

    #[test]
    fn edited_file_is_picked_up_by_its_new_mtime() {
        let dir = tempdir().unwrap();
        let path = write(&dir, "a.toml", &source("alpha", &[]), 100);
        let catalog = catalog(&dir, &[]);
        assert_eq!(names(&catalog), ["alpha"]);
        write(&dir, "a.toml", &source("renamed", &[]), 200);
        assert_eq!(
            catalog.list(),
            [PresetInfo {
                name: "renamed".into(),
                source: user(&path),
                error: None
            }]
        );
    }

    #[test]
    fn removed_file_disappears() {
        let dir = tempdir().unwrap();
        let path = write(&dir, "a.toml", &source("alpha", &[]), 100);
        write(&dir, "b.toml", &source("beta", &[]), 100);
        let catalog = catalog(&dir, &[]);
        assert_eq!(names(&catalog), ["alpha", "beta"]);
        fs::remove_file(path).unwrap();
        assert_eq!(names(&catalog), ["beta"]);
    }

    #[test]
    fn unchanged_directory_is_served_from_the_cache() {
        let dir = tempdir().unwrap();
        write(&dir, "a.toml", &source("alpha", &[]), 100);
        let catalog = catalog(&dir, &[]);
        assert!(Arc::ptr_eq(&catalog.entries(), &catalog.entries()));
    }

    #[test]
    fn files_without_the_toml_extension_are_ignored() {
        let dir = tempdir().unwrap();
        write(&dir, "notes.txt", "not a preset", 100);
        write(&dir, "a.toml", &source("alpha", &[]), 100);
        assert_eq!(names(&catalog(&dir, &[])), ["alpha"]);
    }

    #[test]
    fn open_reads_presets_from_the_config_dir() {
        let dir = tempdir().unwrap();
        write(&dir, "a.toml", &source("alpha", &[]), 100);
        assert!(names(&PresetCatalog::open(dir.path())).contains(&"alpha".to_string()));
    }

    #[test]
    fn preset_source_and_info_serialize_camel_case() {
        let info = PresetInfo {
            name: "shop".into(),
            source: PresetSource::User {
                path: "/p/shop.toml".into(),
            },
            error: None,
        };
        assert_eq!(
            serde_json::to_value(&info).unwrap(),
            serde_json::json!({
                "name": "shop",
                "source": { "kind": "user", "path": "/p/shop.toml" },
                "error": null,
            })
        );
        assert_eq!(
            serde_json::to_value(PresetSource::BuiltIn).unwrap(),
            serde_json::json!({ "kind": "builtIn" })
        );
    }

    #[test]
    fn requires_order_puts_requirements_first() {
        let map = by_name(&[("shop", "blog"), ("blog", "base"), ("base", "")]);
        assert_eq!(
            order(&map, &["shop"]),
            Ok(vec!["base".into(), "blog".into(), "shop".into()])
        );
    }

    #[test]
    fn requires_order_lists_a_shared_requirement_once() {
        let map = by_name(&[("d", "b c"), ("b", "a"), ("c", "a"), ("a", "")]);
        assert_eq!(
            order(&map, &["d", "a"]),
            Ok(vec!["a".into(), "b".into(), "c".into(), "d".into()])
        );
    }

    #[test]
    fn requires_order_keeps_the_order_of_the_wanted_names() {
        let map = by_name(&[("x", ""), ("y", ""), ("z", "")]);
        assert_eq!(order(&map, &["z", "x"]), Ok(vec!["z".into(), "x".into()]));
    }

    #[test]
    fn requires_order_names_an_unknown_preset() {
        let map = by_name(&[("a", "")]);
        assert_eq!(
            order(&map, &["ghost"]),
            Err("no preset `ghost`".to_string())
        );
    }
}
