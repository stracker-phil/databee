pub mod cli;
mod commands;
pub mod connection;
pub mod db;
mod db_commands;
pub mod decode;
#[cfg(debug_assertions)]
mod dev;
pub mod error;
pub mod ipc;
mod launch;
mod lock;
mod menu;
pub mod secrets;
mod session;
pub mod stage;
mod stage_commands;
mod state;
pub mod store;
mod window;
pub mod wp;
mod wp_commands;

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

use crate::cli::CliOutcome;
use crate::ipc::{Forward, IpcMessage};
use crate::state::AppState;
use crate::store::json_file::create_private_dir;
use crate::store::settings::{LogLevel, Settings};

const IDENTIFIER: &str = "com.philippstracker.databee";
const MAX_LOG_BYTES: u128 = 5 * 1024 * 1024;
const LOCK_POLL: Duration = Duration::from_millis(200);
const LOCK_POLLS: u32 = 25;

/// Same directory Tauri resolves as `app_config_dir`, but available before the app starts.
fn config_dir() -> PathBuf {
    let Some(home) = std::env::home_dir() else {
        fail("Cannot locate the home directory for DataBee's settings");
    };
    home.join("Library/Application Support").join(IDENTIFIER)
}

/// Held for the app's lifetime, so two launches that both find no listener can't both start.
/// `None` when another process holds it.
fn lock_instance(dir: &Path) -> std::io::Result<Option<File>> {
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("databee.lock"))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(e)) => Err(e),
    }
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(2);
}

/// Exits when a running instance answers or refuses the message; returns when none is listening.
fn forward(socket: &Path, message: &IpcMessage, exit_code: i32) {
    match ipc::try_forward(socket, message) {
        Ok(Forward::Delivered) => std::process::exit(exit_code),
        Ok(Forward::NoInstance) => {}
        Ok(Forward::Rejected) => fail("The running DataBee instance did not accept the request"),
        Err(e) => fail(&format!(
            "Could not reach the running DataBee instance: {e}"
        )),
    }
}

/// The lock holder may still be starting up and not listen yet.
fn forward_to_starting_instance(socket: &Path, message: &IpcMessage, exit_code: i32) -> ! {
    for _ in 0..LOCK_POLLS {
        std::thread::sleep(LOCK_POLL);
        forward(socket, message, exit_code);
    }
    fail("Another DataBee instance is starting but does not answer")
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
        // The dispatch caps every later `log::set_max_level`, so it lets everything through;
        // `init_logging` sets the effective level.
        .level(log::LevelFilter::Trace)
        // sqlx logs statements at info; SQL text must not reach the log by default.
        .level_for("sqlx", log::LevelFilter::Warn)
        .level_for("tao", log::LevelFilter::Warn)
        .level_for("wry", log::LevelFilter::Warn)
        .level_for("tauri_runtime_wry", log::LevelFilter::Info)
        .build()
}

/// Runs after `log_plugin`'s setup, which raises the max level to `Trace`, and applies the effective level.
fn init_logging(level: LogLevel) -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("log-level")
        .setup(move |_, _| {
            log::set_max_level(level.into());
            Ok(())
        })
        .build()
}

#[cfg(debug_assertions)]
fn apply_dev_frame(app: &tauri::App) {
    let (Some((x, y, w, h)), Some(window)) = (dev::window_frame(), app.get_webview_window("main"))
    else {
        return;
    };
    let _ = window.set_position(tauri::LogicalPosition::new(x, y));
    let _ = window.set_size(tauri::LogicalSize::new(w, h));
}

/// Without the socket the app still runs, only without single-instance handoff.
fn start_ipc(handle: AppHandle, socket: &Path) {
    let listener = match ipc::bind(socket) {
        Ok(listener) => listener,
        Err(e) => {
            log::warn!("Cannot listen on {}: {e}", socket.display());
            return;
        }
    };
    tauri::async_runtime::spawn(ipc::serve(listener, move |message| {
        launch::on_ipc_message(&handle, message)
    }));
}

/// A token that can't be saved still works until the app quits.
fn ensure_stage_token(state: &AppState) -> Settings {
    let mut settings = state.settings.get();
    if !settings.stage.token.is_empty() {
        return settings;
    }
    settings.stage.token = stage::new_token();
    match state.settings.update(settings.clone()) {
        Ok((_, saved)) => saved,
        Err(e) => {
            log::warn!("Cannot save the Stage token: {e}");
            settings
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config_dir = config_dir();
    if let Err(e) = create_private_dir(&config_dir) {
        eprintln!("Cannot create {}: {e}", config_dir.display());
    }
    let socket = config_dir.join("databee.sock");
    let stored = Settings::peek(&config_dir);

    let (message, log_override) = match cli::parse(
        std::env::args_os(),
        stored.default_port,
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

    forward(&socket, &message, exit_code);
    let _instance_lock = match lock_instance(&config_dir) {
        Ok(Some(lock)) => Some(lock),
        Ok(None) => forward_to_starting_instance(&socket, &message, exit_code),
        Err(e) => {
            eprintln!("Cannot lock the DataBee instance: {e}");
            None
        }
    };

    let mut state = AppState::open(&config_dir, Arc::new(secrets::Keychain));
    state.log_override = log_override;
    start_app(
        state,
        socket,
        message,
        log_override.unwrap_or(stored.log_level),
    );
}

fn start_app(state: AppState, socket: PathBuf, initial: IpcMessage, log_level: LogLevel) {
    tauri::Builder::default()
        .manage(state)
        .plugin(log_plugin())
        .plugin(init_logging(log_level))
        .plugin(tauri_plugin_opener::init())
        .on_menu_event(|app, event| menu::forward(app, event.id().as_ref()))
        .setup(move |app| {
            log::info!("DataBee {} started", env!("CARGO_PKG_VERSION"));
            let state = app.state::<AppState>();
            app.set_menu(menu::build(app, &state.keybindings)?)?;
            state.launches.push(initial);
            let settings = ensure_stage_token(&state);
            app.manage(stage::Stage::new(app.handle(), IDENTIFIER));
            commands::apply_settings(app.handle(), None, &settings);
            #[cfg(debug_assertions)]
            apply_dev_frame(app);
            start_ipc(app.handle().clone(), &socket);
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
            commands::list_favorites,
            commands::create_favorite,
            commands::update_favorite,
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
            wp_commands::wp_dashboard,
            wp_commands::wp_browser,
            wp_commands::wp_orders,
            wp_commands::wp_users,
            wp_commands::wp_products,
            wp_commands::wp_actions,
            wp_commands::wp_action_histogram,
            wp_commands::wp_failed_by_hook,
            wp_commands::wp_sessions,
            wp_commands::wp_content,
            wp_commands::wp_view,
            wp_commands::wp_order_logs,
            wp_commands::wp_search,
            wp_commands::wp_peek,
            wp_commands::wp_find_value,
            wp_commands::wp_options_group,
            wp_commands::wp_snapshot_take,
            wp_commands::wp_snapshots,
            wp_commands::wp_snapshot_diff,
            wp_commands::wp_snapshot_delete,
            wp_commands::wp_compare,
            wp_commands::plan_change,
            wp_commands::wp_plan_change,
            db_commands::plan_row_change,
            db_commands::discard_change,
            db_commands::apply_planned,
            db_commands::undo_batch,
            wp_commands::apply_changes,
            decode::commands::decode_value,
            decode::commands::edit_serialized,
            stage_commands::stage_anchors,
            stage_commands::stage_done,
            stage_commands::stage_info,
            stage_commands::stage_status,
            stage_commands::stage_new_token,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_matches_the_tauri_config() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(conf["identifier"], serde_json::json!(IDENTIFIER));
    }
}
