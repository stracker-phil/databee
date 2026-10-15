mod commands;
pub mod error;
mod state;
pub mod store;

use tauri::Manager;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

use crate::state::AppState;

const MAX_LOG_BYTES: u128 = 5 * 1024 * 1024;

fn log_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_log::Builder::new()
        .targets([
            Target::new(TargetKind::Stdout),
            Target::new(TargetKind::LogDir { file_name: None }),
        ])
        .rotation_strategy(RotationStrategy::KeepSome(3))
        .max_file_size(MAX_LOG_BYTES)
        .timezone_strategy(TimezoneStrategy::UseLocal)
        // Effective level is set from settings in `setup`.
        .level(log::LevelFilter::Trace)
        // sqlx logs statements at info; SQL text must not reach the log by default.
        .level_for("sqlx", log::LevelFilter::Warn)
        .level_for("tao", log::LevelFilter::Warn)
        .level_for("wry", log::LevelFilter::Warn)
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(log_plugin())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let state = AppState::open(&config_dir);
            log::set_max_level(state.settings.get().log_level.into());
            log::info!("databee {} started", env!("CARGO_PKG_VERSION"));
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::update_settings,
            commands::open_log_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
