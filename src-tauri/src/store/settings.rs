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

pub const ACCENTS: &[&str] = &["blue", "purple", "pink", "orange", "green", "graphite"];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub theme: Theme,
    pub accent: String,
    pub editor_font_size: u8,
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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            accent: "blue".into(),
            editor_font_size: 13,
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
        self.editor_font_size = self.editor_font_size.clamp(9, 28);
        self.row_limit = self.row_limit.clamp(10, 100_000);
        self.query_timeout_secs = self.query_timeout_secs.clamp(1, 3600);
        self.max_cell_chars = self.max_cell_chars.clamp(20, 100_000);
        self.connect_timeout_secs = self.connect_timeout_secs.clamp(1, 120);
        if self.default_port == 0 {
            self.default_port = d.default_port;
        }
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
    fn out_of_range_values_are_clamped_on_update() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(dir.path());
        let saved = store
            .update(Settings {
                row_limit: 0,
                editor_font_size: 99,
                accent: "neon".into(),
                ..Settings::default()
            })
            .unwrap();
        assert_eq!(saved.row_limit, 10);
        assert_eq!(saved.editor_font_size, 28);
        assert_eq!(saved.accent, "blue");
    }
}
