use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{AppError, AppResult};

/// Creates `dir` (and its parents) readable only by the user, and tightens an existing one:
/// the config dir holds the IPC socket, the Stage token and query history.
pub fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    if fs::metadata(dir)?.permissions().mode() & 0o077 != 0 {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// A private JSON document on disk. Missing files load as `T::default()`; a file that cannot
/// be read or parsed is moved aside to `<name>.<timestamp>.corrupt` so user data is never
/// silently lost.
pub struct JsonFile {
    path: PathBuf,
    /// Set when a broken file could not be moved aside; saving would overwrite it.
    poisoned: AtomicBool,
}

impl JsonFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            poisoned: AtomicBool::new(false),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load<T: DeserializeOwned + Default>(&self) -> T {
        let reason = match fs::read(&self.path) {
            Ok(raw) => match serde_json::from_slice(&raw) {
                Ok(value) => return value,
                Err(e) => format!("invalid JSON ({e})"),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return T::default(),
            Err(e) => format!("unreadable ({e})"),
        };
        self.quarantine(&reason);
        T::default()
    }

    fn quarantine(&self, reason: &str) {
        let backup = self.backup_path();
        match fs::rename(&self.path, &backup) {
            Ok(()) => log::warn!(
                "{} is {reason}; moved to {}",
                self.path.display(),
                backup.display()
            ),
            Err(e) => {
                log::error!(
                    "{} is {reason} and cannot be moved aside ({e}); it will not be saved over",
                    self.path.display()
                );
                self.poisoned.store(true, Ordering::Relaxed);
            }
        }
    }

    /// `<name>.<UTC yyyymmddTHHMMSS>.corrupt`, with a counter if that name is taken, so a
    /// later corruption never replaces an earlier backup.
    fn backup_path(&self) -> PathBuf {
        let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S");
        let mut backup = self.sibling(&format!("{stamp}.corrupt"));
        let mut n = 1;
        while backup.exists() {
            backup = self.sibling(&format!("{stamp}-{n}.corrupt"));
            n += 1;
        }
        backup
    }

    fn sibling(&self, suffix: &str) -> PathBuf {
        let mut name = self.path.file_name().unwrap_or_default().to_os_string();
        name.push(".");
        name.push(suffix);
        self.path.with_file_name(name)
    }

    /// Writes a 0600 temp file, syncs it and renames it over the target, so a crash leaves
    /// either the old or the new file, never half of one.
    pub fn save<T: Serialize>(&self, value: &T) -> AppResult<()> {
        if self.poisoned.load(Ordering::Relaxed) {
            return Err(AppError::Io(format!(
                "{} could not be read or moved aside, so it is not overwritten",
                self.path.display()
            )));
        }
        if let Some(dir) = self.path.parent() {
            create_private_dir(dir)?;
        }
        let json = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
        let tmp = self.sibling("tmp");
        let written = write_synced(&tmp, &json).and_then(|()| fs::rename(&tmp, &self.path));
        if written.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        written?;
        Ok(())
    }
}

fn write_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
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

    fn backups(dir: &Path) -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                let name = p.file_name().unwrap().to_string_lossy();
                name.starts_with("doc.json.") && name.ends_with(".corrupt")
            })
            .collect();
        found.sort();
        found
    }

    fn chmod(path: &Path, mode: u32) {
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    fn mode_of(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn corrupt_file_loads_default_and_is_kept_as_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.json");
        fs::write(&path, "{ not json").unwrap();
        let file = JsonFile::new(&path);

        assert_eq!(file.load::<Doc>(), Doc::default());
        let found = backups(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(fs::read_to_string(&found[0]).unwrap(), "{ not json");
        assert!(!path.exists());
    }

    #[test]
    fn backup_name_carries_a_utc_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.json");
        fs::write(&path, "{ not json").unwrap();
        JsonFile::new(&path).load::<Doc>();

        let name = backups(dir.path())[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let stamp = name
            .strip_prefix("doc.json.")
            .and_then(|s| s.strip_suffix(".corrupt"))
            .unwrap();
        assert!(
            chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%dT%H%M%S").is_ok(),
            "{name}"
        );
    }

    #[test]
    fn non_utf8_file_loads_default_and_keeps_the_original_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.json");
        fs::write(&path, [0xff, 0xfe]).unwrap();

        assert_eq!(JsonFile::new(&path).load::<Doc>(), Doc::default());

        assert!(!path.exists());
        let found = backups(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(fs::read(&found[0]).unwrap(), vec![0xff, 0xfe]);
    }

    #[test]
    fn unreadable_file_is_moved_aside_and_the_next_save_writes_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.json");
        fs::write(&path, r#"{"items":["old"]}"#).unwrap();
        chmod(&path, 0o000);
        let file = JsonFile::new(&path);

        assert_eq!(file.load::<Doc>(), Doc::default());
        let doc = Doc {
            items: vec!["new".into()],
        };
        file.save(&doc).unwrap();

        assert_eq!(file.load::<Doc>(), doc);
        let found = backups(dir.path());
        assert_eq!(found.len(), 1);
        chmod(&found[0], 0o600);
        assert_eq!(
            fs::read_to_string(&found[0]).unwrap(),
            r#"{"items":["old"]}"#
        );
    }

    #[test]
    fn file_that_cannot_be_moved_aside_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.json");
        fs::write(&path, r#"{"items":["old"]}"#).unwrap();
        chmod(&path, 0o000);
        chmod(dir.path(), 0o500);
        let file = JsonFile::new(&path);

        let loaded = file.load::<Doc>();
        let saved = file.save(&Doc {
            items: vec!["new".into()],
        });

        chmod(dir.path(), 0o700);
        chmod(&path, 0o600);
        assert_eq!(loaded, Doc::default());
        assert!(saved.is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), r#"{"items":["old"]}"#);
    }

    #[test]
    fn second_corruption_keeps_the_first_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.json");
        let file = JsonFile::new(&path);

        fs::write(&path, "first").unwrap();
        file.load::<Doc>();
        fs::write(&path, "second").unwrap();
        file.load::<Doc>();

        let mut contents: Vec<String> = backups(dir.path())
            .iter()
            .map(|p| fs::read_to_string(p).unwrap())
            .collect();
        contents.sort();
        assert_eq!(contents, vec!["first", "second"]);
    }

    #[test]
    fn save_leaves_no_temp_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let file = JsonFile::new(dir.path().join("doc.json"));
        file.save(&Doc::default()).unwrap();

        assert!(dir.path().join("doc.json").exists());
        assert!(!dir.path().join("doc.json.tmp").exists());
    }

    #[test]
    fn saved_file_is_private_and_so_is_the_directory_it_creates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/doc.json");
        JsonFile::new(&path).save(&Doc::default()).unwrap();

        assert_eq!(mode_of(&path), 0o600);
        assert_eq!(mode_of(path.parent().unwrap()), 0o700);
    }

    #[test]
    fn create_private_dir_makes_nested_dirs_private() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a/b");
        create_private_dir(&nested).unwrap();

        assert_eq!(mode_of(&nested), 0o700);
        assert_eq!(mode_of(&dir.path().join("a")), 0o700);
    }

    #[test]
    fn create_private_dir_tightens_an_existing_open_dir() {
        let dir = tempfile::tempdir().unwrap();
        let open = dir.path().join("open");
        fs::create_dir(&open).unwrap();
        chmod(&open, 0o755);

        create_private_dir(&open).unwrap();

        assert_eq!(mode_of(&open), 0o700);
    }
}
