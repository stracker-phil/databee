//! Stage finds presentable apps in one shared folder: one file per app saying where its hook
//! answers, present only while the API runs.

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use super::protocol::discovery_record;

pub fn folder() -> PathBuf {
    std::env::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("Library/Application Support/stage/apps")
}

pub fn path_in(folder: &Path, id: &str) -> PathBuf {
    folder.join(format!("{id}.json"))
}

/// Writes the record; readable by this user only, since the token is in it.
pub fn publish(file: &Path, name: &str, id: &str, port: u16, token: &str) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let record = discovery_record(
        name,
        id,
        port,
        token,
        std::process::id(),
        chrono::Utc::now(),
    );
    let json = serde_json::to_string_pretty(&record).map_err(std::io::Error::other)?;
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(file)?;
    out.write_all(format!("{json}\n").as_bytes())?;
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600))?;
    log::info!("Discoverable by Stage: {}", file.display());
    Ok(())
}

/// Synchronous, so it also runs while the app quits.
pub fn remove(file: &Path) {
    match std::fs::remove_file(file) {
        Ok(()) => log::info!("Removed the Stage discovery file"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::warn!("Couldn't remove the Stage discovery file: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishes_a_private_record_and_removes_it() {
        let dir = tempfile::tempdir().unwrap();
        let file = path_in(&dir.path().join("apps"), "com.example");
        publish(&file, "databee", "com.example", 27184, "token").unwrap();
        let record: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(record["url"], "http://127.0.0.1:27184/v1/stage");
        assert_eq!(record["pid"], std::process::id());
        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        remove(&file);
        assert!(!file.exists());
        remove(&file);
    }
}
