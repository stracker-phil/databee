pub mod cli;
mod commands;
pub mod connection;
pub mod error;
pub mod ipc;
mod launch;
pub mod secrets;
mod state;
pub mod store;

use std::path::PathBuf;

use tauri::{Emitter, Manager};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

use crate::cli::CliOutcome;
use crate::ipc::IpcMessage;
use crate::state::AppState;
use crate::store::settings::{LogLevel, SettingsStore};

const IDENTIFIER: &str = "com.philippstracker.databee";
const MAX_LOG_BYTES: u128 = 5 * 1024 * 1024;
pub const LAUNCH_EVENT: &str = "launch";

/// Same directory Tauri resolves as `app_config_dir`, but available before the app starts.
fn config_dir() -> PathBuf {
    std::env::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("Library/Application Support")
        .join(IDENTIFIER)
}

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
        .level_for("tauri_runtime_wry", log::LevelFilter::Info)
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config_dir = config_dir();
    let socket = config_dir.join("databee.sock");
    let default_port = SettingsStore::open(&config_dir).get().default_port;

    let (message, log_override) = match cli::parse(
        std::env::args_os(),
        default_port,
        &mut std::io::stdin().lock(),
        &|key| std::env::var(key).ok(),
    ) {
        Ok(CliOutcome::Exit { message }) => {
            print!("{message}");
            std::process::exit(0);
        }
        Ok(CliOutcome::Run { launch, log_level }) => (
            launch.map(IpcMessage::from).unwrap_or(IpcMessage::Activate),
            log_level,
        ),
        Err(message) => {
            eprintln!("{message}");
            (IpcMessage::Error { message }, None)
        }
    };
    let exit_code = if matches!(message, IpcMessage::Error { .. }) { 2 } else { 0 };

    match ipc::try_forward(&socket, &message) {
        Ok(true) => std::process::exit(exit_code),
        Ok(false) => {}
        Err(e) => eprintln!("Could not reach running databee instance: {e}"),
    }

    start_app(config_dir, socket, message, log_override);
}

fn start_app(config_dir: PathBuf, socket: PathBuf, initial: IpcMessage, log_override: Option<LogLevel>) {
    tauri::Builder::default()
        .plugin(log_plugin())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let state = AppState::open(&config_dir, Box::new(secrets::Keychain));
            let level = log_override.unwrap_or(state.settings.get().log_level);
            log::set_max_level(level.into());
            log::info!("databee {} started", env!("CARGO_PKG_VERSION"));
            state.launches.push(initial);
            app.manage(state);

            let listener = ipc::bind(&socket)?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(ipc::serve(listener, move |message| {
                log::info!("Received launch request from another process");
                if handle.state::<AppState>().launches.push(message) {
                    let _ = handle.emit(LAUNCH_EVENT, ());
                }
                if let Some(window) = handle.get_webview_window("main") {
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::update_settings,
            commands::open_log_folder,
            commands::take_launch_events,
            commands::list_recents,
            commands::remove_recent,
            commands::forget_password,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
