/// Stage's commands (`stage::commands::COMMANDS`, checked by a test there); granted to the main
/// window through `capabilities/stage.json`.
const STAGE_COMMANDS: &[&str] = &[
    "stage_anchors",
    "stage_done",
    "stage_info",
    "stage_status",
    "stage_settings",
    "stage_update_settings",
    "stage_new_token",
];

fn inlined(commands: &'static [&'static str]) -> tauri_build::InlinedPlugin {
    tauri_build::InlinedPlugin::new()
        .commands(commands)
        .default_permission(tauri_build::DefaultPermissionRule::AllowAllCommands)
}

fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().plugin("stage", inlined(STAGE_COMMANDS)))
        .expect("tauri build");
}
