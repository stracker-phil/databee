//! The page's side of the Stage hook (src/stage/hooks.ts) and the Settings page's controls.

use tauri::State;

use crate::error::AppResult;
use crate::stage::protocol::{ActReply, AnchorsReply, InfoReply};
use crate::stage::{Stage, StageStatus, new_token};
use crate::state::AppState;
use crate::store::settings::Settings;

#[tauri::command]
pub fn stage_anchors(stage: State<'_, Stage>, reply: AnchorsReply) {
    stage.hub.receive_anchors(reply);
}

#[tauri::command]
pub fn stage_done(stage: State<'_, Stage>, reply: ActReply) {
    stage.hub.receive_act(reply);
}

#[tauri::command]
pub fn stage_info(stage: State<'_, Stage>, reply: InfoReply) {
    stage.hub.receive_info(reply);
}

#[tauri::command]
pub fn stage_status(stage: State<'_, Stage>) -> StageStatus {
    stage.status()
}

/// Replaces the API token; Stage needs the hook registered again with the new one.
#[tauri::command]
pub fn stage_new_token(state: State<'_, AppState>, stage: State<'_, Stage>) -> AppResult<Settings> {
    let mut settings = state.settings.get();
    settings.stage.token = new_token();
    let saved = state.settings.update(settings)?;
    stage.start(&saved.stage);
    log::info!("Stage API token replaced");
    Ok(saved)
}
