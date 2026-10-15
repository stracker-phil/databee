pub mod cli;
mod commands;
pub mod connection;
pub mod db;
mod db_commands;
mod dev;
pub mod error;
pub mod ipc;
mod launch;
mod menu;
pub mod secrets;
mod session;
mod state;
pub mod store;
pub mod wp;
mod wp_commands;

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
    let exit_code = if matches!(message, IpcMessage::Error { .. }) {
        2
    } else {
        0
    };

    match ipc::try_forward(&socket, &message) {
        Ok(true) => std::process::exit(exit_code),
        Ok(false) => {}
        Err(e) => eprintln!("Could not reach running databee instance: {e}"),
    }

    start_app(config_dir, socket, message, log_override);
}

fn start_app(
    config_dir: PathBuf,
    socket: PathBuf,
    initial: IpcMessage,
    log_override: Option<LogLevel>,
) {
    tauri::Builder::default()
        .plugin(log_plugin())
        .plugin(tauri_plugin_opener::init())
        .on_menu_event(|app, event| menu::forward(app, event.id().as_ref()))
        .setup(move |app| {
            let state = AppState::open(&config_dir, Box::new(secrets::Keychain));
            app.set_menu(menu::build(app, &state.keybindings)?)?;
            let level = log_override.unwrap_or(state.settings.get().log_level);
            log::set_max_level(level.into());
            log::info!("databee {} started", env!("CARGO_PKG_VERSION"));
            state.launches.push(initial);
            app.manage(state);

            if let (Some((x, y, w, h)), Some(window)) =
                (dev::window_frame(), app.get_webview_window("main"))
            {
                let _ = window.set_position(tauri::LogicalPosition::new(x, y));
                let _ = window.set_size(tauri::LogicalSize::new(w, h));
            }

            let listener = ipc::bind(&socket)?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(ipc::serve(listener, move |message| {
                let window = handle.get_webview_window("main");
                if let IpcMessage::Eval { js } = &message {
                    if cfg!(debug_assertions)
                        && let Some(w) = &window
                    {
                        let _ = w.eval(js);
                    }
                    return;
                }
                log::info!("Received launch request from another process");
                if handle.state::<AppState>().launches.push(message) {
                    let _ = handle.emit(LAUNCH_EVENT, ());
                }
                if let Some(window) = window
                    && !dev::background()
                {
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::update_settings,
            commands::get_keybindings,
            commands::set_keybindings,
            commands::get_ui_state,
            commands::set_ui_state,
            commands::open_log_folder,
            commands::open_url,
            commands::take_launch_events,
            commands::list_recents,
            commands::remove_recent,
            commands::forget_password,
            commands::list_favorites,
            commands::save_favorite,
            commands::remove_favorite,
            commands::reorder_favorites,
            commands::list_history,
            commands::clear_history,
            commands::log_frontend_error,
            db_commands::connect,
            db_commands::disconnect,
            db_commands::list_databases,
            db_commands::list_tables,
            db_commands::describe_table,
            db_commands::completion_schema,
            db_commands::use_database,
            db_commands::execute,
            db_commands::cancel_query,
            db_commands::browse_table,
            wp_commands::wp_detect,
            wp_commands::wp_load,
            wp_commands::wp_expand,
            wp_commands::wp_options,
            wp_commands::wp_transient_cleanup_sql,
            wp_commands::decode_value,
            wp_commands::wp_dashboard,
            wp_commands::wp_browser,
            wp_commands::wp_orders,
            wp_commands::wp_customers,
            wp_commands::wp_products,
            wp_commands::wp_actions,
            wp_commands::wp_action_histogram,
            wp_commands::wp_failed_by_hook,
            wp_commands::wp_sessions,
            wp_commands::wp_content,
            wp_commands::wp_view,
            wp_commands::wp_order_logs,
            wp_commands::wp_search,
            wp_commands::wp_options_group,
            wp_commands::wp_snapshot_take,
            wp_commands::wp_snapshots,
            wp_commands::wp_snapshot_diff,
            wp_commands::wp_snapshot_delete,
            wp_commands::wp_compare,
            wp_commands::plan_change,
            wp_commands::apply_changes,
            wp_commands::edit_serialized,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
