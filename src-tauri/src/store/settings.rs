use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::json_file::JsonFile;
use crate::error::AppResult;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

impl From<LogLevel> for log::LevelFilter {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Error => log::LevelFilter::Error,
            LogLevel::Warn => log::LevelFilter::Warn,
            LogLevel::Info => log::LevelFilter::Info,
            LogLevel::Debug => log::LevelFilter::Debug,
            LogLevel::Trace => log::LevelFilter::Trace,
        }
    }
}

/// Where the value inspector and the WordPress detail pane sit next to the content.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Dock {
    #[default]
    Right,
    Bottom,
}

/// The local HTTP API through which Stage presents DataBee (docs: README "Presentations").
#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct StageSettings {
    pub enabled: bool,
    pub port: u16,
    /// Generated on first start; also in the discovery file, never in the log.
    pub token: String,
}

/// One fixed port per presentable app: 27180–27183 are taken by others, Stage uses 27190.
pub const STAGE_PORT: u16 = 27184;

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

/// Green and orange are status colors in the UI, so they are not offered as accents.
pub const ACCENTS: &[&str] = &["blue", "purple", "pink", "graphite"];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub theme: Theme,
    pub accent: String,
    /// Chrome: sidebar, tabs, labels, buttons (also scales rem).
    pub ui_font_size: u8,
    /// Data: grid cells, document values, trees, SQL editor. Reads the old `editorFontSize` key.
    #[serde(alias = "editorFontSize")]
    pub data_font_size: u8,
    pub density: Density,
    pub row_limit: u32,
    pub query_timeout_secs: u32,
    pub confirm_destructive: bool,
    pub null_label: String,
    pub max_cell_chars: u32,
    pub connect_timeout_secs: u32,
    pub default_port: u16,
    pub show_system_schemas: bool,
    pub log_level: LogLevel,
    /// Loads avatars from Gravatar (sends hashed emails to gravatar.com).
    pub gravatar: bool,
    pub inspector_dock: Dock,
    pub stage: StageSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            accent: "blue".into(),
            ui_font_size: 13,
            data_font_size: 12,
            density: Density::Comfortable,
            row_limit: 1000,
            query_timeout_secs: 30,
            confirm_destructive: true,
            null_label: "NULL".into(),
            max_cell_chars: 500,
            connect_timeout_secs: 10,
            default_port: 3306,
            show_system_schemas: false,
            log_level: LogLevel::Info,
            gravatar: false,
            inspector_dock: Dock::Right,
            stage: StageSettings::default(),
        }
    }
}

impl Settings {
    /// Clamps every value into a usable range, so a hand-edited file can't break the app.
    pub fn sanitized(mut self) -> Self {
        let d = Settings::default();
        if !ACCENTS.contains(&self.accent.as_str()) {
            self.accent = d.accent;
        }
        self.ui_font_size = self.ui_font_size.clamp(10, 24);
        self.data_font_size = self.data_font_size.clamp(9, 28);
        self.row_limit = self.row_limit.clamp(10, 100_000);
        self.query_timeout_secs = self.query_timeout_secs.clamp(1, 3600);
        self.max_cell_chars = self.max_cell_chars.clamp(20, 100_000);
        self.connect_timeout_secs = self.connect_timeout_secs.clamp(1, 120);
        if self.default_port == 0 {
            self.default_port = d.default_port;
        }
        if self.stage.port < 1024 {
            self.stage.port = STAGE_PORT;
        }
        self.stage.token = self.stage.token.trim().to_string();
        self
    }
}

pub struct SettingsStore {
    file: JsonFile,
    current: Mutex<Settings>,
}

impl SettingsStore {
    pub fn open(dir: &Path) -> Self {
        let file = JsonFile::new(dir.join("settings.json"));
        let current = file.load::<Settings>().sanitized();
        Self {
            file,
            current: Mutex::new(current),
        }
    }

    pub fn get(&self) -> Settings {
        self.current.lock().unwrap().clone()
    }

    pub fn update(&self, settings: Settings) -> AppResult<Settings> {
        let settings = settings.sanitized();
        self.file.save(&settings)?;
        *self.current.lock().unwrap() = settings.clone();
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_install_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(SettingsStore::open(dir.path()).get(), Settings::default());
    }

    #[test]
    fn updates_persist_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(dir.path());
        let changed = Settings {
            theme: Theme::Dark,
            row_limit: 250,
            inspector_dock: Dock::Bottom,
            ..Settings::default()
        };
        store.update(changed.clone()).unwrap();
        assert_eq!(SettingsStore::open(dir.path()).get(), changed);
    }

    #[test]
    fn partial_file_fills_missing_fields_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("settings.json"), r#"{"rowLimit": 42}"#).unwrap();
        let settings = SettingsStore::open(dir.path()).get();
        assert_eq!(settings.row_limit, 42);
        assert_eq!(settings.null_label, "NULL");
    }

    #[test]
    fn old_editor_font_size_and_retired_accents_migrate() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("settings.json"),
            r#"{"editorFontSize": 15, "accent": "green"}"#,
        )
        .unwrap();
        let settings = SettingsStore::open(dir.path()).get();
        assert_eq!(settings.data_font_size, 15);
        assert_eq!(settings.ui_font_size, 13);
        assert_eq!(settings.accent, "blue");
    }

    #[test]
    fn out_of_range_values_are_clamped_on_update() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(dir.path());
        let saved = store
            .update(Settings {
                row_limit: 0,
                data_font_size: 99,
                ui_font_size: 2,
                accent: "neon".into(),
                ..Settings::default()
            })
            .unwrap();
        assert_eq!(saved.row_limit, 10);
        assert_eq!(saved.data_font_size, 28);
        assert_eq!(saved.ui_font_size, 10);
        assert_eq!(saved.accent, "blue");
    }

    #[test]
    fn stage_api_defaults_on_with_a_valid_port_and_hides_the_token() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("settings.json"),
            r#"{"stage": {"port": 80, "token": " abc "}}"#,
        )
        .unwrap();
        let stage = SettingsStore::open(dir.path()).get().stage;
        assert!(stage.enabled);
        assert_eq!(stage.port, STAGE_PORT);
        assert_eq!(stage.token, "abc");
        assert!(!format!("{stage:?}").contains("abc"));
    }
}
