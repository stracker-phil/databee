use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::json_file::JsonFile;
use crate::error::{AppError, AppResult};
use crate::lock::{MutexExt, RwLockExt};

/// A JSON file with its parsed value kept in memory. Changes are saved before they are
/// committed to memory, so a failed save never leaves a change that vanishes on restart.
pub struct JsonStore<T> {
    inner: Arc<Inner<T>>,
}

struct Inner<T> {
    file: JsonFile,
    value: RwLock<T>,
    /// Serializes writers. Readers only wait for the clone and the swap, never for the disk.
    gate: Mutex<()>,
}

impl<T> JsonStore<T>
where
    T: Serialize + DeserializeOwned + Clone + Default + Send + Sync + 'static,
{
    pub fn open(path: PathBuf) -> Self {
        Self::open_with(path, |value| value)
    }

    /// Like `open`, with `normalize` applied to the loaded value (not saved until the next update).
    pub fn open_with(path: PathBuf, normalize: impl FnOnce(T) -> T) -> Self {
        let file = JsonFile::new(path);
        let value = normalize(file.load());
        Self::from_loaded(file, value)
    }

    /// For callers that parse the file themselves, e.g. to migrate an older format.
    pub fn from_loaded(file: JsonFile, value: T) -> Self {
        Self {
            inner: Arc::new(Inner {
                file,
                value: RwLock::new(value),
                gate: Mutex::new(()),
            }),
        }
    }

    pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.inner.value.read_unpoisoned())
    }

    /// Blocks on the disk; for startup code and sync commands that run off the async workers.
    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> AppResult<R> {
        self.inner.update(f)
    }

    /// `update` on a blocking thread, so an async command never waits on the disk.
    pub async fn update_async<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut T) -> R + Send + 'static,
    ) -> AppResult<R> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.update(f))
            .await
            .map_err(|e| {
                AppError::Io(format!(
                    "Saving {} failed: {e}",
                    self.inner.file.path().display()
                ))
            })?
    }
}

impl<T: Serialize + Clone> Inner<T> {
    /// Applies `f` to a copy, saves the copy and only then swaps it in. A panic inside `f`
    /// happens before the swap, so the value behind a poisoned lock is intact.
    fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> AppResult<R> {
        let _gate = self.gate.lock_unpoisoned();
        let mut next = self.value.read_unpoisoned().clone();
        let result = f(&mut next);
        self.file.save(&next)?;
        *self.value.write_unpoisoned() = next;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn items(store: &JsonStore<Vec<String>>) -> Vec<String> {
        store.read(Clone::clone)
    }

    #[test]
    fn update_returns_the_closure_value_and_persists_the_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("list.json");
        let store = JsonStore::<Vec<String>>::open(path.clone());

        let len = store
            .update(|list| {
                list.push("a".into());
                list.len()
            })
            .unwrap();

        assert_eq!(len, 1);
        assert_eq!(items(&store), vec!["a"]);
        assert_eq!(items(&JsonStore::<Vec<String>>::open(path)), vec!["a"]);
    }

    #[test]
    fn failed_save_returns_an_error_and_keeps_the_old_value() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonStore::<Vec<String>>::open(dir.path().join("list.json"));
        store.update(|list| list.push("kept".into())).unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();

        let result = store.update(|list| list.push("lost".into()));

        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_err());
        assert_eq!(items(&store), vec!["kept"]);
    }

    #[tokio::test]
    async fn update_async_returns_the_closure_value_and_persists_the_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("list.json");
        let store = JsonStore::<Vec<String>>::open(path.clone());

        let len = store
            .update_async(|list| {
                list.push("a".into());
                list.len()
            })
            .await
            .unwrap();

        assert_eq!(len, 1);
        assert_eq!(items(&store), vec!["a"]);
        assert_eq!(items(&JsonStore::<Vec<String>>::open(path)), vec!["a"]);
    }

    #[tokio::test]
    async fn failed_async_save_returns_the_error_and_keeps_the_old_value() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonStore::<Vec<String>>::open(dir.path().join("list.json"));
        store
            .update_async(|list| list.push("kept".into()))
            .await
            .unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();

        let result = store.update_async(|list| list.push("lost".into())).await;

        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_err());
        assert_eq!(items(&store), vec!["kept"]);
    }

    #[tokio::test]
    async fn concurrent_async_updates_all_land() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("list.json");
        let store = Arc::new(JsonStore::<Vec<String>>::open(path.clone()));

        let tasks: Vec<_> = (0..20)
            .map(|i| {
                let store = Arc::clone(&store);
                tokio::spawn(async move {
                    store
                        .update_async(move |list| list.push(i.to_string()))
                        .await
                })
            })
            .collect();
        for task in tasks {
            task.await.unwrap().unwrap();
        }

        let mut expected: Vec<String> = (0..20).map(|i| i.to_string()).collect();
        expected.sort();
        let mut in_memory = items(&store);
        in_memory.sort();
        let mut on_disk = items(&JsonStore::<Vec<String>>::open(path));
        on_disk.sort();
        assert_eq!(in_memory, expected);
        assert_eq!(on_disk, expected);
    }

    #[test]
    fn open_with_normalizes_the_loaded_value() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("list.json");
        fs::write(&path, r#"["a","b"]"#).unwrap();

        let store = JsonStore::<Vec<String>>::open_with(path, |mut list| {
            list.reverse();
            list
        });

        assert_eq!(items(&store), vec!["b", "a"]);
    }
}
