use std::path::Path;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

use super::json_store::JsonStore;
use crate::db::CellLimit;
use crate::error::AppResult;

/// Falls back to the default for a value the field doesn't know (a typo, a retired variant),
/// so one bad field doesn't quarantine the whole file and lose every other setting.
fn lenient<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    let raw = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(raw.clone()).unwrap_or_else(|_| {
        log::warn!("Ignoring unknown setting value {raw}; using the default");
        T::default()
    }))
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, clap::ValueEnum, TS,
)]
#[ts(export, export_to = "src/api/types.gen.ts")]
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
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "lowercase")]
pub enum Dock {
    #[default]
    Right,
    Bottom,
}

/// Accent hues; every color scheme defines each one. Values are uncolored by default, so no hue
/// is reserved for status.
pub const ACCENTS: &[&str] = &["orange", "gold", "blue", "purple", "graphite"];

/// Built-in color schemes (`src/styles/schemes/<id>.css`).
pub const SCHEMES: &[&str] = &["ayu-mirage", "ayu-light"];

const DEFAULT_LIGHT_SCHEME: &str = "ayu-light";
const DEFAULT_DARK_SCHEME: &str = "ayu-mirage";

/// Row padding steps (1 tightest, 4 loosest); the frontend maps each to px.
const ROW_PADDING_MIN: u8 = 1;
const ROW_PADDING_MAX: u8 = 4;
const DEFAULT_ROW_PADDING: u8 = 2;

/// Reads `rowPadding`, or the retired `density` key it replaced (`compact` is the tightest step).
/// Anything unreadable falls back to the default step instead of failing the file.
fn row_padding<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = serde_json::Value::deserialize(deserializer)?;
    Ok(match &raw {
        serde_json::Value::String(density) if density == "compact" => 1,
        serde_json::Value::Number(n) => n
            .as_u64()
            .and_then(|n| u8::try_from(n).ok())
            .unwrap_or(DEFAULT_ROW_PADDING),
        _ => DEFAULT_ROW_PADDING,
    })
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Which of the two schemes applies: light, dark, or the OS appearance.
    #[serde(deserialize_with = "lenient")]
    pub theme: Theme,
    #[serde(deserialize_with = "lenient")]
    pub light_scheme: String,
    #[serde(deserialize_with = "lenient")]
    pub dark_scheme: String,
    pub accent: String,
    /// Chrome: sidebar, tabs, labels, buttons (also scales rem).
    pub ui_font_size: u8,
    /// Data: grid cells, document values, trees, SQL editor. Reads the old `editorFontSize` key.
    #[serde(alias = "editorFontSize")]
    pub data_font_size: u8,
    #[serde(alias = "density", deserialize_with = "row_padding")]
    pub row_padding: u8,
    /// An installed font family; `None` uses the bundled Geist.
    #[serde(deserialize_with = "lenient")]
    pub font_ui: Option<String>,
    /// An installed font family; `None` uses the bundled JetBrains Mono.
    #[serde(deserialize_with = "lenient")]
    pub font_mono: Option<String>,
    pub row_limit: u32,
    pub query_timeout_secs: u32,
    pub confirm_destructive: bool,
    pub null_label: String,
    /// Longer text cells cross IPC as a preview; `fetch_cell` returns the full value.
    pub max_cell_chars: u32,
    pub connect_timeout_secs: u32,
    pub default_port: u16,
    pub show_system_schemas: bool,
    #[serde(deserialize_with = "lenient")]
    pub log_level: LogLevel,
    /// Loads avatars from Gravatar (sends hashed emails to gravatar.com).
    pub gravatar: bool,
    #[serde(deserialize_with = "lenient")]
    pub inspector_dock: Dock,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            light_scheme: DEFAULT_LIGHT_SCHEME.into(),
            dark_scheme: DEFAULT_DARK_SCHEME.into(),
            accent: "orange".into(),
            ui_font_size: 13,
            data_font_size: 12,
            row_padding: DEFAULT_ROW_PADDING,
            font_ui: None,
            font_mono: None,
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
        }
    }
}

impl Settings {
    pub fn query_timeout(&self) -> Duration {
        Duration::from_secs(self.query_timeout_secs.into())
    }

    pub fn cell_limit(&self) -> CellLimit {
        CellLimit(self.max_cell_chars as usize)
    }

    pub fn connect_timeout(&self) -> Duration {
        Duration::from_secs(self.connect_timeout_secs.into())
    }

    /// Clamps every value into a usable range, so a hand-edited file can't break the app.
    pub fn sanitized(mut self) -> Self {
        let d = Settings::default();
        // Retired accents (`pink`, the old `green`) and typos all land on the default.
        if !ACCENTS.contains(&self.accent.as_str()) {
            self.accent = d.accent;
        }
        if !SCHEMES.contains(&self.light_scheme.as_str()) {
            self.light_scheme = d.light_scheme;
        }
        if !SCHEMES.contains(&self.dark_scheme.as_str()) {
            self.dark_scheme = d.dark_scheme;
        }
        self.row_padding = self.row_padding.clamp(ROW_PADDING_MIN, ROW_PADDING_MAX);
        self.font_ui = family_name(self.font_ui);
        self.font_mono = family_name(self.font_mono);
        self.ui_font_size = self.ui_font_size.clamp(10, 24);
        self.data_font_size = self.data_font_size.clamp(9, 28);
        self.row_limit = self.row_limit.clamp(10, 100_000);
        self.query_timeout_secs = self.query_timeout_secs.clamp(1, 3600);
        self.max_cell_chars = self.max_cell_chars.clamp(20, 100_000);
        self.connect_timeout_secs = self.connect_timeout_secs.clamp(1, 120);
        if self.default_port == 0 {
            self.default_port = d.default_port;
        }
        self
    }

    /// Reads the settings without the store's quarantine, so a short-lived second process
    /// never moves the running app's file aside.
    pub fn peek(dir: &Path) -> Settings {
        std::fs::read_to_string(dir.join(FILE_NAME))
            .ok()
            .and_then(|raw| serde_json::from_str::<Settings>(&raw).ok())
            .unwrap_or_default()
            .sanitized()
    }
}

/// A blank family name means "use the bundled font".
fn family_name(name: Option<String>) -> Option<String> {
    let trimmed = name?.trim().to_string();
    (!trimmed.is_empty()).then_some(trimmed)
}

const FILE_NAME: &str = "settings.json";

pub struct SettingsStore {
    store: JsonStore<Settings>,
}

impl SettingsStore {
    pub fn open(dir: &Path) -> Self {
        Self {
            store: JsonStore::open_with(dir.join(FILE_NAME), Settings::sanitized),
        }
    }

    pub fn get(&self) -> Settings {
        self.store.read(Settings::clone)
    }

    /// Returns `(previous, saved)`, both read under the same lock.
    pub async fn update(&self, settings: Settings) -> AppResult<(Settings, Settings)> {
        self.store
            .update_async(move |current| {
                let previous = std::mem::replace(current, settings.sanitized());
                (previous, current.clone())
            })
            .await
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

    #[tokio::test]
    async fn updates_persist_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(dir.path());
        let changed = Settings {
            theme: Theme::Dark,
            row_limit: 250,
            inspector_dock: Dock::Bottom,
            ..Settings::default()
        };
        store.update(changed.clone()).await.unwrap();
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
    fn peek_reads_valid_values_and_sanitizes_them() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("settings.json"),
            r#"{"defaultPort": 3307, "rowLimit": 0}"#,
        )
        .unwrap();
        let settings = Settings::peek(dir.path());
        assert_eq!(settings.default_port, 3307);
        assert_eq!(settings.row_limit, 10);
    }

    #[test]
    fn peek_on_corrupt_file_returns_defaults_and_leaves_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        std::fs::write(&file, "{ not json").unwrap();

        assert_eq!(Settings::peek(dir.path()), Settings::default());

        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ not json");
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("settings.json")]);
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
        assert_eq!(settings.accent, "orange");
    }

    #[tokio::test]
    async fn out_of_range_values_are_clamped_on_update() {
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
            .await
            .unwrap()
            .1;
        assert_eq!(saved.row_limit, 10);
        assert_eq!(saved.data_font_size, 28);
        assert_eq!(saved.ui_font_size, 10);
        assert_eq!(saved.accent, "orange");
    }

    fn open_with_json(json: &str) -> (tempfile::TempDir, SettingsStore) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("settings.json"), json).unwrap();
        let store = SettingsStore::open(dir.path());
        (dir, store)
    }

    #[test]
    fn zero_default_port_loads_as_3306() {
        let (_dir, store) = open_with_json(r#"{"defaultPort": 0}"#);
        assert_eq!(store.get().default_port, 3306);
    }

    #[test]
    fn timeouts_are_clamped_on_load() {
        for (json, query, connect) in [
            (r#"{"queryTimeoutSecs": 0, "connectTimeoutSecs": 0}"#, 1, 1),
            (
                r#"{"queryTimeoutSecs": 99999, "connectTimeoutSecs": 999}"#,
                3600,
                120,
            ),
        ] {
            let (_dir, store) = open_with_json(json);
            let settings = store.get();
            assert_eq!(settings.query_timeout_secs, query, "{json}");
            assert_eq!(settings.connect_timeout_secs, connect, "{json}");
        }
    }

    #[test]
    fn timeouts_are_the_clamped_seconds_as_durations() {
        for (settings, query, connect) in [
            (Settings::default(), 30, 10),
            (
                Settings {
                    query_timeout_secs: 0,
                    connect_timeout_secs: 0,
                    ..Settings::default()
                },
                1,
                1,
            ),
            (
                Settings {
                    query_timeout_secs: 99_999,
                    connect_timeout_secs: 999,
                    ..Settings::default()
                },
                3600,
                120,
            ),
        ] {
            let settings = settings.sanitized();
            assert_eq!(settings.query_timeout(), Duration::from_secs(query));
            assert_eq!(settings.connect_timeout(), Duration::from_secs(connect));
        }
    }

    #[test]
    fn unknown_enum_values_fall_back_without_quarantining_the_file() {
        let json = r#"{"theme": "neon", "rowLimit": 42, "density": "compact", "logLevel": "loud", "inspectorDock": 5}"#;
        let (dir, store) = open_with_json(json);

        let settings = store.get();
        assert_eq!(settings.theme, Theme::System);
        assert_eq!(settings.row_limit, 42);
        assert_eq!(settings.row_padding, 1);
        assert_eq!(settings.log_level, LogLevel::Info);
        assert_eq!(settings.inspector_dock, Dock::Right);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("settings.json")).unwrap(),
            json
        );
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("settings.json")]);
    }

    #[test]
    fn defaults_use_orange_accent_middle_padding_and_bundled_fonts() {
        let settings = Settings::default();
        assert_eq!(settings.accent, "orange");
        assert_eq!(settings.row_padding, 2);
        assert_eq!(settings.font_ui, None);
        assert_eq!(settings.font_mono, None);
        assert_eq!(settings.light_scheme, "ayu-light");
        assert_eq!(settings.dark_scheme, "ayu-mirage");
    }

    #[test]
    fn retired_and_unknown_accents_become_orange_and_known_ones_stay() {
        for (accent, expected) in [
            ("pink", "orange"),
            ("neon", "orange"),
            ("green", "orange"),
            ("gold", "gold"),
            ("graphite", "graphite"),
        ] {
            let json = format!(r#"{{"accent": "{accent}"}}"#);
            let (_dir, store) = open_with_json(&json);
            assert_eq!(store.get().accent, expected, "{accent}");
        }
    }

    #[test]
    fn row_padding_is_clamped_and_unreadable_values_use_the_default() {
        for (json, expected) in [
            (r#"{"rowPadding": 0}"#, 1),
            (r#"{"rowPadding": 1}"#, 1),
            (r#"{"rowPadding": 4}"#, 4),
            (r#"{"rowPadding": 9}"#, 4),
            (r#"{"rowPadding": 300}"#, 2),
            (r#"{"rowPadding": -1}"#, 2),
            (r#"{"rowPadding": "wide"}"#, 2),
            (r#"{}"#, 2),
        ] {
            let (_dir, store) = open_with_json(json);
            assert_eq!(store.get().row_padding, expected, "{json}");
        }
    }

    #[test]
    fn old_density_key_migrates_to_row_padding() {
        for (json, expected) in [
            (r#"{"density": "compact"}"#, 1),
            (r#"{"density": "comfortable"}"#, 2),
        ] {
            let (_dir, store) = open_with_json(json);
            assert_eq!(store.get().row_padding, expected, "{json}");
        }
    }

    #[test]
    fn missing_schemes_use_the_defaults() {
        let (_dir, store) = open_with_json(r#"{"rowLimit": 42}"#);
        let settings = store.get();
        assert_eq!(settings.light_scheme, "ayu-light");
        assert_eq!(settings.dark_scheme, "ayu-mirage");
    }

    #[test]
    fn known_schemes_are_kept() {
        let (_dir, store) =
            open_with_json(r#"{"lightScheme": "ayu-mirage", "darkScheme": "ayu-light"}"#);
        let settings = store.get();
        assert_eq!(settings.light_scheme, "ayu-mirage");
        assert_eq!(settings.dark_scheme, "ayu-light");
    }

    #[test]
    fn unknown_or_non_string_schemes_reset_without_quarantining_the_file() {
        let json = r#"{"lightScheme": "solarized", "darkScheme": 7, "rowLimit": 42}"#;
        let (dir, store) = open_with_json(json);

        let settings = store.get();
        assert_eq!(settings.light_scheme, "ayu-light");
        assert_eq!(settings.dark_scheme, "ayu-mirage");
        assert_eq!(settings.row_limit, 42);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("settings.json")).unwrap(),
            json
        );
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("settings.json")]);
    }

    #[test]
    fn font_families_are_trimmed_and_blank_means_bundled() {
        for (json, ui, mono) in [
            (r#"{"fontUi": ""}"#, None, None),
            (r#"{"fontUi": "   ", "fontMono": "\t"}"#, None, None),
            (r#"{"fontUi": null}"#, None, None),
            (r#"{"fontUi": 5}"#, None, None),
            (
                r#"{"fontUi": "  Inter ", "fontMono": " Fira Code "}"#,
                Some("Inter"),
                Some("Fira Code"),
            ),
        ] {
            let (_dir, store) = open_with_json(json);
            let settings = store.get();
            assert_eq!(settings.font_ui.as_deref(), ui, "{json}");
            assert_eq!(settings.font_mono.as_deref(), mono, "{json}");
        }
    }

    #[test]
    fn peek_applies_the_new_appearance_rules() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("settings.json"),
            r#"{"accent": "pink", "density": "compact", "darkScheme": "solarized", "fontMono": " "}"#,
        )
        .unwrap();
        let settings = Settings::peek(dir.path());
        assert_eq!(settings.accent, "orange");
        assert_eq!(settings.row_padding, 1);
        assert_eq!(settings.dark_scheme, "ayu-mirage");
        assert_eq!(settings.font_mono, None);
    }

    #[tokio::test]
    async fn update_returns_previous_and_sanitized_saved_settings() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(dir.path());
        let before = store.get();

        let (previous, saved) = store
            .update(Settings {
                theme: Theme::Dark,
                row_limit: 0,
                ..Settings::default()
            })
            .await
            .unwrap();

        assert_eq!(previous, before);
        assert_eq!(
            saved,
            Settings {
                theme: Theme::Dark,
                row_limit: 10,
                ..Settings::default()
            }
        );
        assert_eq!(store.get(), saved);
    }
}
