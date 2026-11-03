mod address;
mod cli;
mod commands;
pub mod connection;
pub mod db;
mod db_commands;
pub mod decode;
mod deep_link;
#[cfg(debug_assertions)]
mod dev;
mod document;
pub mod error;
mod health;
mod instance;
mod ipc;
mod launch;
mod lens;
mod lock;
mod mcp;
mod menu;
mod pending;
mod query;
mod readout;
mod remote;
mod rules;
mod saved;
mod schema;
mod search;
mod secrets;
mod session;
mod stage;
mod state;
#[cfg(test)]
mod statement_counts;
mod store;
mod table_check;
#[cfg(any(test, debug_assertions, feature = "test-util"))]
pub mod test_support;
mod value;
mod value_counts;
mod window;

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

use crate::cli::CliOutcome;
use crate::ipc::IpcMessage;
use crate::state::AppState;
use crate::store::json_file::create_private_dir;
use crate::store::settings::{LogLevel, Settings};

/// Each build is its own app with its own data (docs/adr/0028).
const IDENTIFIER: &str = if cfg!(debug_assertions) {
    "com.philippstracker.databee.debug"
} else {
    "com.philippstracker.databee"
};
pub const APP_NAME: &str = if cfg!(debug_assertions) {
    "DataBee (Debug)"
} else {
    "DataBee"
};
/// The URL scheme the build registers; `databee://` addresses follow it.
pub const LINK_SCHEME: &str = if cfg!(debug_assertions) {
    "databee-debug"
} else {
    "databee"
};
const MAX_LOG_BYTES: u128 = 5 * 1024 * 1024;

/// Same directory Tauri resolves as `app_config_dir`, but available before the app starts.
fn config_dir() -> PathBuf {
    let Some(home) = std::env::home_dir() else {
        fail("Cannot locate the home directory for DataBee's settings");
    };
    home.join("Library/Application Support").join(IDENTIFIER)
}

pub(crate) fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(2);
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
    let (Some((x, y, w, h)), Some(window)) =
        (dev::window_frame(), app.get_webview_window(window::MAIN))
    else {
        return;
    };
    let _ = window.set_position(tauri::LogicalPosition::new(x, y));
    let _ = window.set_size(tauri::LogicalSize::new(w, h));
}

/// The debug build starts with the demo connections (their passwords are in `secrets::app_store`).
#[cfg(debug_assertions)]
fn seed_demo(state: &AppState) {
    if state.config_dir.join("connections.json").exists() {
        return;
    }
    for (spec, _) in test_support::demo_connections() {
        if let Err(e) = tauri::async_runtime::block_on(state.recents.touch(&spec)) {
            log::warn!("Could not add the demo connection: {e}");
        }
    }
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
        launch::on_ipc_message(handle.clone(), message)
    }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config_dir = config_dir();
    if let Err(e) = create_private_dir(&config_dir) {
        eprintln!("Cannot create {}: {e}", config_dir.display());
    }
    let socket = config_dir.join("databee.sock");
    let stored = Settings::peek(&config_dir);

    let outcome = cli::parse(
        std::env::args_os(),
        stored.default_port,
        &mut std::io::stdin().lock(),
        &|key| std::env::var(key).ok(),
    );
    let (message, log_override) = match outcome {
        Ok(CliOutcome::Exit { message }) => {
            print!("{message}");
            std::process::exit(0);
        }
        Ok(CliOutcome::Run { launch, log_level }) => (
            launch.map(IpcMessage::from).unwrap_or(IpcMessage::Activate),
            log_level,
        ),
        Ok(CliOutcome::Mcp) => std::process::exit(mcp::run(&config_dir, socket)),
        Ok(CliOutcome::Quit) => {
            instance::forward(&socket, &IpcMessage::Quit, 0);
            fail("DataBee is not running");
        }
        Ok(CliOutcome::Remote { message }) => {
            instance::ask(&socket, &message);
            if !matches!(message, IpcMessage::Open { .. }) {
                fail("DataBee is not running");
            }
            (message, None)
        }
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

    let _instance_lock = instance::forward_or_lock(&config_dir, &socket, &message, exit_code);

    let mut state = AppState::open(&config_dir, secrets::app_store());
    #[cfg(debug_assertions)]
    seed_demo(&state);
    state.log_override = log_override;
    start_app(
        state,
        socket,
        message,
        log_override.unwrap_or(stored.log_level),
    );
}

fn start_app(state: AppState, socket: PathBuf, initial: IpcMessage, log_level: LogLevel) {
    let config_dir = state.config_dir.clone();
    tauri::Builder::default()
        .manage(state)
        .manage(remote::Bridge::default())
        .plugin(log_plugin())
        .plugin(init_logging(log_level))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(window::navigation::guard())
        .plugin(stage::init(&config_dir))
        .on_menu_event(|app, event| menu::forward(app, event.id().as_ref()))
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && window::hide_instead_of_close(window.app_handle(), window.label())
            {
                api.prevent_close();
            }
        })
        .setup(move |app| {
            log::info!("DataBee {} started", env!("CARGO_PKG_VERSION"));
            let state = app.state::<AppState>();
            app.set_menu(menu::build(app, &state.keybindings)?)?;
            state.launches.push(initial);
            state.history.start_writer();
            commands::apply_settings(app.handle(), None, &state.settings.get());
            #[cfg(debug_assertions)]
            apply_dev_frame(app);
            start_ipc(app.handle().clone(), &socket);
            deep_link::listen(app.handle());
            health::watch::start(app.handle());
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
            commands::open_settings,
            commands::hide_settings,
            commands::choose_save_path,
            commands::take_launch_events,
            commands::list_recents,
            commands::remove_recent,
            commands::rename_connection_slug,
            commands::list_history,
            commands::clear_history,
            commands::log_frontend_error,
            remote::remote_reply,
            db_commands::connect,
            db_commands::disconnect,
            db_commands::list_databases,
            db_commands::list_tables,
            db_commands::describe_table,
            db_commands::refresh_schema,
            db_commands::completion_schema,
            db_commands::use_database,
            db_commands::execute,
            db_commands::cancel_query,
            db_commands::browse_table,
            db_commands::resolve_address,
            db_commands::table_references,
            db_commands::value_condition,
            db_commands::narrow_refusal,
            db_commands::narrow_query,
            db_commands::fetch_cell,
            db_commands::table_select_sql,
            db_commands::plan_change,
            db_commands::discard_change,
            db_commands::apply_changes,
            db_commands::undo_batch,
            health::commands::reconnect_session,
            decode::commands::decode_value,
            decode::commands::edit_serialized,
            rules::commands::presets_list,
            rules::commands::rules_info,
            rules::commands::project_get,
            rules::commands::project_update,
            rules::commands::project_accents,
            rules::commands::project_set_accent,
            rules::commands::project_export,
            rules::commands::project_export_to,
            rules::commands::project_rules,
            rules::commands::table_rules,
            rules::commands::rule_draft,
            rules::commands::coverage_preview,
            rules::commands::reference_coverage,
            document::commands::document,
            document::commands::peek,
            document::commands::table_actions,
            document::commands::plan_action,
            document::commands::preview_action,
            document::commands::queue_action_preview,
            document::commands::watch_start,
            document::commands::watch_poll,
            document::commands::watch_stop,
            document::commands::compare,
            document::commands::snapshot_take,
            document::commands::snapshot_list,
            document::commands::snapshot_diff,
            document::commands::snapshot_delete,
            search::commands::palette_search,
            saved::commands::saved_queries,
            saved::commands::save_query,
            saved::commands::update_saved_query,
            saved::commands::remove_saved_query,
            saved::commands::copy_preset_query_to_project,
            value_counts::value_counts,
            table_check::table_check,
            table_check::table_check_rows,
            readout::commands::readout,
            readout::commands::remember_object,
            readout::commands::readout_watch,
            readout::commands::readout_unwatch,
            readout::commands::readout_watch_label,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event
                && let Err(e) = app.state::<AppState>().history.flush()
            {
                log::warn!("Could not save the query history: {e}");
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configs() -> (serde_json::Value, serde_json::Value) {
        let release = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let debug = serde_json::from_str(include_str!("../tauri.debug.conf.json")).unwrap();
        (release, debug)
    }

    fn this_build() -> serde_json::Value {
        let (release, debug) = configs();
        if cfg!(debug_assertions) {
            debug
        } else {
            release
        }
    }

    #[test]
    fn identifier_matches_the_tauri_config() {
        assert_eq!(this_build()["identifier"], serde_json::json!(IDENTIFIER));
    }

    #[test]
    fn app_name_matches_the_product_name_in_the_tauri_config() {
        assert_eq!(this_build()["productName"], serde_json::json!(APP_NAME));
    }

    #[test]
    fn link_scheme_matches_the_deep_link_scheme_in_the_tauri_config() {
        assert_eq!(
            this_build()["plugins"]["deep-link"]["desktop"]["schemes"],
            serde_json::json!([LINK_SCHEME])
        );
    }

    #[test]
    fn the_debug_build_differs_from_the_release_build_in_name_identifier_and_scheme() {
        let (release, debug) = configs();
        assert_eq!(release["productName"], "DataBee");
        assert_eq!(release["identifier"], "com.philippstracker.databee");
        assert_eq!(debug["productName"], "DataBee (Debug)");
        assert_eq!(
            debug["identifier"],
            serde_json::json!(format!("{}.debug", release["identifier"].as_str().unwrap()))
        );
        let schemes = |config: &serde_json::Value| {
            config["plugins"]["deep-link"]["desktop"]["schemes"].clone()
        };
        assert_eq!(schemes(&release), serde_json::json!(["databee"]));
        assert_eq!(schemes(&debug), serde_json::json!(["databee-debug"]));
    }
}
