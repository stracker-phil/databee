use tauri::State;

use super::PresetInfo;
use crate::state::AppState;

/// Every known preset, with the parse error of a user file that does not load.
#[tauri::command]
pub fn presets_list(state: State<'_, AppState>) -> Vec<PresetInfo> {
    state.presets.list()
}
