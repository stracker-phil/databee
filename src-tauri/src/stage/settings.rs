//! Stage's own settings file, `<config>/stage.json` (ADR 0016).

use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::AppResult;
use crate::store::json_store::JsonStore;

/// One fixed port per presentable app: 27180–27183 belong to other apps; Stage itself listens on 27190.
pub const STAGE_PORT: u16 = 27184;

const FILE_NAME: &str = "stage.json";
/// Where the settings lived before Stage had its own file.
const LEGACY_FILE: &str = "settings.json";

/// The local HTTP API through which Stage presents DataBee (docs/stage.md).
#[derive(Serialize, Deserialize, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/stage/types.gen.ts")]
#[serde(default, rename_all = "camelCase")]
pub struct StageSettings {
    pub enabled: bool,
    pub port: u16,
    /// Registered with Stage by the user, never in the log.
    pub token: String,
}

impl Default for StageSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            port: STAGE_PORT,
            token: String::new(),
        }
    }
}

impl std::fmt::Debug for StageSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StageSettings")
            .field("enabled", &self.enabled)
            .field("port", &self.port)
            .field("token", &"<redacted>")
            .finish()
    }
}

impl StageSettings {
    /// Clamps the port and gives an empty token a fresh one, so Stage is never locked out.
    pub fn sanitized(mut self) -> Self {
        if self.port < 1024 {
            self.port = STAGE_PORT;
        }
        self.token = self.token.trim().to_string();
        if self.token.is_empty() {
            self.token = new_token();
        }
        self
    }
}

/// 256 random bits as 64 hex characters.
pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("macOS always provides system randomness");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub struct StageStore {
    store: JsonStore<StageSettings>,
}

impl StageStore {
    /// Opens `stage.json`; without one, takes the `stage` object of a legacy `settings.json`
    /// once, so an already registered token keeps working. `settings.json` is never written.
    pub fn open(config_dir: &Path) -> Self {
        let path = config_dir.join(FILE_NAME);
        let migrate = !path.exists();
        let store = Self {
            store: JsonStore::open_with(path, StageSettings::sanitized),
        };
        let legacy = if migrate {
            legacy_settings(config_dir)
        } else {
            None
        };
        let initial = legacy.unwrap_or_else(|| store.get());
        if migrate && let Err(e) = store.update(initial) {
            log::warn!("Cannot save the Stage settings: {e}");
        }
        store
    }

    pub fn get(&self) -> StageSettings {
        self.store.read(StageSettings::clone)
    }

    /// Returns `(previous, saved)`, both read under the same lock.
    pub fn update(&self, next: StageSettings) -> AppResult<(StageSettings, StageSettings)> {
        self.store.update(|current| {
            let previous = std::mem::replace(current, next.sanitized());
            (previous, current.clone())
        })
    }

    /// Replaces the token in one update, so a concurrent settings save cannot undo it.
    pub fn rotate_token(&self) -> AppResult<StageSettings> {
        self.store.update(|current| {
            current.token = new_token();
            current.clone()
        })
    }
}

fn legacy_settings(config_dir: &Path) -> Option<StageSettings> {
    let raw = std::fs::read_to_string(config_dir.join(LEGACY_FILE)).ok()?;
    let mut json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let stage = json.get_mut("stage")?.take();
    serde_json::from_value::<StageSettings>(stage)
        .ok()
        .map(StageSettings::sanitized)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_lower_hex_64(token: &str) -> bool {
        token.len() == 64 && token.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
    }

    fn read_stage_file(dir: &Path) -> StageSettings {
        let raw = std::fs::read_to_string(dir.join(FILE_NAME)).unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    #[test]
    fn opening_an_empty_config_dir_creates_stage_json_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let settings = StageStore::open(dir.path()).get();
        assert!(settings.enabled);
        assert_eq!(settings.port, STAGE_PORT);
        assert!(is_lower_hex_64(&settings.token), "{}", settings.token.len());
        assert_eq!(read_stage_file(dir.path()), settings);
    }

    #[test]
    fn reopening_keeps_the_generated_token() {
        let dir = tempfile::tempdir().unwrap();
        let first = StageStore::open(dir.path()).get();
        assert_eq!(StageStore::open(dir.path()).get(), first);
    }

    #[test]
    fn legacy_settings_stage_object_is_migrated_and_sanitized() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = r#"{"theme":"dark","stage":{"port": 80, "token": " abc "}}"#;
        std::fs::write(dir.path().join(LEGACY_FILE), legacy).unwrap();

        let settings = StageStore::open(dir.path()).get();

        assert_eq!(settings.port, STAGE_PORT);
        assert_eq!(settings.token, "abc");
        assert_eq!(read_stage_file(dir.path()), settings);
        assert_eq!(
            std::fs::read_to_string(dir.path().join(LEGACY_FILE)).unwrap(),
            legacy
        );
    }

    #[test]
    fn legacy_settings_without_a_stage_object_yield_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(LEGACY_FILE), r#"{"theme":"dark"}"#).unwrap();
        let settings = StageStore::open(dir.path()).get();
        assert_eq!(settings.port, STAGE_PORT);
        assert!(is_lower_hex_64(&settings.token));
    }

    #[test]
    fn existing_stage_json_wins_over_legacy_settings() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(FILE_NAME),
            r#"{"enabled":false,"port":30000,"token":"mine"}"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join(LEGACY_FILE),
            r#"{"stage":{"port":31000,"token":"legacy"}}"#,
        )
        .unwrap();

        let settings = StageStore::open(dir.path()).get();

        assert!(!settings.enabled);
        assert_eq!(settings.port, 30000);
        assert_eq!(settings.token, "mine");
    }

    #[test]
    fn sanitizing_regenerates_empty_and_blank_tokens() {
        for token in ["", "   ", "\t\n"] {
            let settings = StageSettings {
                token: token.to_string(),
                ..StageSettings::default()
            }
            .sanitized();
            assert!(is_lower_hex_64(&settings.token), "{token:?}");
        }
    }

    #[test]
    fn sanitizing_trims_the_token() {
        let settings = StageSettings {
            token: "  abc\n".to_string(),
            ..StageSettings::default()
        }
        .sanitized();
        assert_eq!(settings.token, "abc");
    }

    #[test]
    fn sanitizing_clamps_ports_below_1024() {
        for (port, expected) in [
            (0, STAGE_PORT),
            (80, STAGE_PORT),
            (1023, STAGE_PORT),
            (1024, 1024),
            (40000, 40000),
        ] {
            let settings = StageSettings {
                port,
                token: "t".to_string(),
                ..StageSettings::default()
            }
            .sanitized();
            assert_eq!(settings.port, expected, "{port}");
        }
    }

    #[test]
    fn update_returns_previous_and_saved_settings() {
        let dir = tempfile::tempdir().unwrap();
        let store = StageStore::open(dir.path());
        let before = store.get();
        let next = StageSettings {
            enabled: false,
            port: 30001,
            token: " new ".to_string(),
        };

        let (previous, saved) = store.update(next).unwrap();

        assert_eq!(previous, before);
        assert!(!saved.enabled);
        assert_eq!(saved.port, 30001);
        assert_eq!(saved.token, "new");
        assert_eq!(read_stage_file(dir.path()), saved);
    }

    #[test]
    fn rotating_the_token_changes_it_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let store = StageStore::open(dir.path());
        let before = store.get();

        let rotated = store.rotate_token().unwrap();

        assert_ne!(rotated.token, before.token);
        assert!(is_lower_hex_64(&rotated.token));
        assert_eq!(rotated.port, before.port);
        assert_eq!(StageStore::open(dir.path()).get(), rotated);
    }

    #[test]
    fn new_tokens_are_64_lowercase_hex_chars_and_differ() {
        let (a, b) = (new_token(), new_token());
        assert!(is_lower_hex_64(&a), "{a}");
        assert!(is_lower_hex_64(&b), "{b}");
        assert_ne!(a, b);
    }

    #[test]
    fn debug_output_hides_the_token() {
        let settings = StageSettings {
            token: "super-secret-token".to_string(),
            ..StageSettings::default()
        };
        let debug = format!("{settings:?}");
        assert!(!debug.contains("super-secret-token"), "{debug}");
    }

    #[test]
    fn settings_serialize_with_camel_case_fields() {
        let settings = StageSettings {
            enabled: true,
            port: 27184,
            token: "t".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&settings).unwrap(),
            serde_json::json!({"enabled": true, "port": 27184, "token": "t"})
        );
    }
}
