//! The page's side of the Stage hook (src/stage/hooks.ts) and the Settings page's controls.

use tauri::State;

use super::page::{ActReply, AnchorsReply, InfoReply};
use super::settings::StageSettings;
use super::{Stage, StageStatus};
use crate::error::AppResult;

/// Registered in `build.rs` too, which grants them to the main window.
pub const COMMANDS: &[&str] = &[
    "stage_anchors",
    "stage_done",
    "stage_info",
    "stage_status",
    "stage_settings",
    "stage_update_settings",
    "stage_new_token",
];

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

#[tauri::command]
pub fn stage_settings(stage: State<'_, Stage>) -> StageSettings {
    stage.settings()
}

#[tauri::command]
pub fn stage_update_settings(
    stage: State<'_, Stage>,
    settings: StageSettings,
) -> AppResult<StageSettings> {
    stage.update_settings(settings)
}

/// Stage needs the hook registered again with the new token.
#[tauri::command]
pub fn stage_new_token(stage: State<'_, Stage>) -> AppResult<StageSettings> {
    stage.rotate_token()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUILD_RS: &str = include_str!("../../build.rs");

    fn quoted_stage_names(source: &str) -> Vec<&str> {
        source
            .split('"')
            .skip(1)
            .step_by(2)
            .filter(|s| s.starts_with("stage_"))
            .collect()
    }

    #[test]
    fn every_command_is_granted_in_build_rs() {
        let granted = quoted_stage_names(BUILD_RS);
        for name in COMMANDS {
            assert!(granted.contains(name), "{name} missing from build.rs");
        }
    }

    #[test]
    fn every_stage_command_in_build_rs_is_registered() {
        for name in quoted_stage_names(BUILD_RS) {
            assert!(COMMANDS.contains(&name), "{name} missing from COMMANDS");
        }
    }
}
