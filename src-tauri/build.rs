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

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().plugin(
            "stage",
            tauri_build::InlinedPlugin::new()
                .commands(STAGE_COMMANDS)
                .default_permission(tauri_build::DefaultPermissionRule::AllowAllCommands),
        ),
    )
    .expect("tauri build");
}
