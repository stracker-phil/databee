use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::json_file::JsonFile;
use crate::error::AppResult;

/// A JSON file with its parsed value kept in memory. Changes are saved before they are
/// committed to memory, so a failed save never leaves a change that vanishes on restart.
pub struct JsonStore<T> {
    file: JsonFile,
    value: Mutex<T>,
}

impl<T: Serialize + DeserializeOwned + Clone + Default> JsonStore<T> {
    pub fn open(path: PathBuf) -> Self {
        Self::open_with(path, |value| value)
    }

    /// Like `open`, with `normalize` applied to the loaded value (not saved until the next update).
    pub fn open_with(path: PathBuf, normalize: impl FnOnce(T) -> T) -> Self {
        let file = JsonFile::new(path);
        let value = Mutex::new(normalize(file.load()));
        Self { file, value }
    }

    pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.lock())
    }

    /// Applies `f` to a copy, saves the copy and only then replaces the value. The lock is
    /// held throughout, so concurrent updates cannot interleave.
    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> AppResult<R> {
        let mut value = self.lock();
        let mut next = value.clone();
        let result = f(&mut next);
        self.file.save(&next)?;
        *value = next;
        Ok(result)
    }

    /// A panic inside `f` happens before the swap, so the value behind a poisoned lock is intact.
    fn lock(&self) -> MutexGuard<'_, T> {
        self.value.lock().unwrap_or_else(PoisonError::into_inner)
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
