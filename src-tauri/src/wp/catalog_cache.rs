use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use super::context::WpContext;
use super::entity::{self, Catalog, EntityDef};
use crate::error::{AppError, AppResult};
use crate::lock::MutexExt;

/// Entity definitions (built-in plus the user's `entities.toml`), parsed once per version of
/// the file. The version is the file's mtime; a missing file is a version too. A file that
/// fails to parse is reported on every call and never cached.
pub struct CatalogCache {
    path: PathBuf,
    cached: Mutex<Option<Cached>>,
}

struct Cached {
    modified: Option<SystemTime>,
    definitions: Arc<[EntityDef]>,
}

impl CatalogCache {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            cached: Mutex::new(None),
        }
    }

    /// The catalog depends on the database's tables, so only the definitions are cached.
    pub fn catalog(&self, ctx: &WpContext) -> AppResult<Catalog> {
        Ok(Catalog::new(&self.definitions()?, ctx))
    }

    fn definitions(&self) -> AppResult<Arc<[EntityDef]>> {
        let modified = std::fs::metadata(&self.path)
            .and_then(|m| m.modified())
            .ok();
        let mut cached = self.cached.lock_unpoisoned();
        if let Some(c) = cached.as_ref()
            && c.modified == modified
        {
            return Ok(c.definitions.clone());
        }
        let definitions: Arc<[EntityDef]> = self.load()?.into();
        *cached = Some(Cached {
            modified,
            definitions: definitions.clone(),
        });
        Ok(definitions)
    }

    fn load(&self) -> AppResult<Vec<EntityDef>> {
        let user = std::fs::read_to_string(&self.path).ok();
        entity::definitions(user.as_deref())
            .map_err(|e| AppError::InvalidInput(format!("{}: {e}", self.path.display())))
    }
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::path::Path;
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;
    use crate::error::ErrorKind;

    const VAULT: &str = r#"
        [[entity]]
        name = "vault_token"
        label = "PayPal vault token"
        table = "ppcp_vault"
        id = "id"
    "#;

    const BROKEN: &str = "[[entity]]\nname = = 1\n";

    fn write_at(path: &Path, content: &str, seconds: u64) {
        std::fs::write(path, content).unwrap();
        OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(seconds))
            .unwrap();
    }

    fn has(definitions: &[EntityDef], name: &str) -> bool {
        definitions.iter().any(|d| d.name == name)
    }

    #[test]
    fn definitions_are_shared_while_the_file_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("entities.toml");
        write_at(&path, VAULT, 1_000);
        let cache = CatalogCache::new(path);

        let first = cache.definitions().unwrap();
        let second = cache.definitions().unwrap();

        assert!(Arc::ptr_eq(&first, &second));
        assert!(has(&first, "vault_token"));
    }

    #[test]
    fn definitions_reload_when_the_file_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("entities.toml");
        write_at(&path, VAULT, 1_000);
        let cache = CatalogCache::new(path.clone());
        let before = cache.definitions().unwrap();

        write_at(&path, "", 2_000);
        let after = cache.definitions().unwrap();

        assert!(!Arc::ptr_eq(&before, &after));
        assert!(has(&before, "vault_token"));
        assert!(!has(&after, "vault_token"));
    }

    #[test]
    fn a_missing_file_yields_the_built_in_definitions_and_is_cached() {
        let dir = tempfile::tempdir().unwrap();
        let cache = CatalogCache::new(dir.path().join("entities.toml"));

        let first = cache.definitions().unwrap();
        let second = cache.definitions().unwrap();

        assert!(Arc::ptr_eq(&first, &second));
        assert!(has(&first, "user"));
        assert!(!has(&first, "vault_token"));
    }

    #[test]
    fn creating_the_file_after_a_missing_one_picks_it_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("entities.toml");
        let cache = CatalogCache::new(path.clone());
        let before = cache.definitions().unwrap();

        write_at(&path, VAULT, 1_000);
        let after = cache.definitions().unwrap();

        assert!(!has(&before, "vault_token"));
        assert!(has(&after, "vault_token"));
    }

    #[test]
    fn a_broken_file_is_invalid_input_naming_the_path_and_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("entities.toml");
        write_at(&path, BROKEN, 1_000);
        let cache = CatalogCache::new(path.clone());

        let err = cache.definitions().unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        let message = err.to_string();
        assert!(
            message.starts_with(&format!("{}: ", path.display())),
            "{message}"
        );
        assert!(message.contains("line 2"), "{message}");
    }

    #[test]
    fn a_broken_file_is_reported_on_every_call_until_it_is_fixed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("entities.toml");
        write_at(&path, BROKEN, 1_000);
        let cache = CatalogCache::new(path.clone());

        let first = cache.definitions().unwrap_err();
        let second = cache.definitions().unwrap_err();
        write_at(&path, VAULT, 2_000);
        let fixed = cache.definitions().unwrap();

        assert_eq!(first.to_string(), second.to_string());
        assert!(has(&fixed, "vault_token"));
    }

    #[test]
    fn catalog_applies_the_cached_definitions_to_the_context() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("entities.toml");
        write_at(&path, VAULT, 1_000);
        let cache = CatalogCache::new(path);
        let ctx = WpContext {
            prefix: "wp_".into(),
            version: None,
            sites: vec![],
            wc: None,
            site: Default::default(),
            tables: ["wp_users".to_string(), "wp_ppcp_vault".to_string()].into(),
        };

        let catalog = cache.catalog(&ctx).unwrap();

        assert_eq!(catalog.get("vault_token").unwrap().table, "ppcp_vault");
        assert_eq!(catalog.get("user").unwrap().table, "users");
    }
}
