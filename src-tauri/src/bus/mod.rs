//! Runs commands by id for every caller: native menu items, the page, the CLI and MCP. Rust commands run
//! here, also while the window is hidden; page commands go to the page (docs/adr/0036).

pub mod catalog;

pub use catalog::{MenuSlot, Runs, find};

use tauri::{AppHandle, Emitter, Manager};

use crate::commands::apply_settings;
use crate::error::{AppError, AppResult};
use crate::ipc::HandlerResult;
use crate::remote::{Bridge, RemoteRequest};
use crate::state::AppState;
use crate::window;

/// Sent to the page with the id of a page command whose native menu item was clicked.
pub const PAGE_COMMAND_EVENT: &str = "page_command";

#[derive(Debug, Clone, Copy)]
enum RustAction {
    OpenSettings,
    /// One text size step up or down; `None` resets.
    Zoom(Option<i8>),
}

fn rust_action(id: &str) -> Option<RustAction> {
    match id {
        "settings.open" => Some(RustAction::OpenSettings),
        "view.zoomIn" => Some(RustAction::Zoom(Some(1))),
        "view.zoomOut" => Some(RustAction::Zoom(Some(-1))),
        "view.zoomReset" => Some(RustAction::Zoom(None)),
        _ => None,
    }
}

async fn zoom(app: &AppHandle, step: Option<i8>) -> AppResult<()> {
    let state = app.state::<AppState>();
    let zoomed = state.settings.get().zoomed(step);
    let (previous, saved) = state.settings.update(zoomed).await?;
    apply_settings(app, Some(&previous), &saved);
    window::announce_settings(app);
    Ok(())
}

async fn run_in_rust(app: &AppHandle, id: &str) -> AppResult<()> {
    match rust_action(id) {
        Some(RustAction::OpenSettings) => window::settings::open(app, None),
        Some(RustAction::Zoom(step)) => zoom(app, step).await?,
        None => {
            return Err(AppError::InvalidInput(format!(
                "{id} is not a command that runs in Rust"
            )));
        }
    }
    Ok(())
}

/// A native menu item was clicked; items that aren't commands (About, Quit) are handled by macOS.
pub fn run_from_menu(app: &AppHandle, id: &str) {
    let Some(command) = find(id) else {
        return;
    };
    let id = command.id;
    match command.runs {
        Runs::Rust => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = run_in_rust(&app, id).await {
                    log::warn!("Menu command {id} failed: {e}");
                }
            });
        }
        Runs::Page => {
            if let Err(e) = app.emit(PAGE_COMMAND_EVENT, id) {
                log::warn!("Couldn't forward menu command {id}: {e}");
            }
        }
    }
}

/// Runs a command for another process (`databee command`, MCP). Page commands wait for the page's answer.
pub async fn run(app: &AppHandle, id: &str) -> HandlerResult {
    let command = find(id).ok_or_else(|| format!("No command {id}"))?;
    match command.runs {
        Runs::Rust => run_in_rust(app, id)
            .await
            .map(|()| serde_json::Value::Null)
            .map_err(|e| e.to_string()),
        Runs::Page => {
            let request = RemoteRequest::PageCommand { id: id.to_string() };
            app.state::<Bridge>().ask(app, request).await
        }
    }
}

/// The page runs Rust commands (from a key or the palette) through this.
#[tauri::command]
pub async fn run_command(app: AppHandle, id: String) -> AppResult<()> {
    run_in_rust(&app, &id).await
}

#[cfg(test)]
mod tests {
    use super::catalog::CATALOG;
    use super::*;

    #[test]
    fn every_rust_command_has_an_action_and_no_page_command_does() {
        for command in CATALOG {
            let has_action = rust_action(command.id).is_some();
            assert_eq!(has_action, command.runs == Runs::Rust, "{}", command.id);
        }
    }
}
