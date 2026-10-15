use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::AppResult;

/// A JSON document on disk. Missing files load as `T::default()`; unreadable
/// files are moved aside to `<name>.corrupt` so user data is never silently lost.
pub struct JsonFile {
    path: PathBuf,
}

impl JsonFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load<T: DeserializeOwned + Default>(&self) -> T {
        let raw = match fs::read_to_string(&self.path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return T::default(),
            Err(e) => {
                log::warn!("Cannot read {}: {e}", self.path.display());
                return T::default();
            }
        };
        match serde_json::from_str(&raw) {
            Ok(value) => value,
            Err(e) => {
                let backup = self.path.with_extension("corrupt");
                log::warn!(
                    "Invalid JSON in {} ({e}); moved to {}",
                    self.path.display(),
                    backup.display()
                );
                let _ = fs::rename(&self.path, backup);
                T::default()
            }
        }
    }

    /// Writes atomically (temp file + rename) so a crash never leaves half a file.
    pub fn save<T: Serialize>(&self, value: &T) -> AppResult<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("tmp");
        let json = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
    struct Doc {
        items: Vec<String>,
    }

    #[test]
    fn missing_file_loads_default() {
        let dir = tempfile::tempdir().unwrap();
        let file = JsonFile::new(dir.path().join("doc.json"));
        assert_eq!(file.load::<Doc>(), Doc::default());
    }

    #[test]
    fn saved_value_loads_back_and_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let file = JsonFile::new(dir.path().join("nested/doc.json"));
        let doc = Doc {
            items: vec!["a".into()],
        };
        file.save(&doc).unwrap();
        assert_eq!(file.load::<Doc>(), doc);
    }

    #[test]
    fn corrupt_file_loads_default_and_is_kept_as_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.json");
        fs::write(&path, "{ not json").unwrap();
        let file = JsonFile::new(&path);

        assert_eq!(file.load::<Doc>(), Doc::default());
        assert_eq!(
            fs::read_to_string(dir.path().join("doc.corrupt")).unwrap(),
            "{ not json"
        );
    }
}
