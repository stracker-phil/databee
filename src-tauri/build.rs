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

/// WordPress mode's commands (`wp::COMMANDS`); granted to the main window through
/// `capabilities/wp.json`.
const WP_COMMANDS: &[&str] = &[
    "wp_detect",
    "wp_options",
    "wp_dashboard",
    "wp_browser",
    "wp_orders",
    "wp_users",
    "wp_products",
    "wp_actions",
    "wp_action_histogram",
    "wp_failed_by_hook",
    "wp_sessions",
    "wp_content",
    "wp_view",
    "wp_order_logs",
    "wp_search",
    "wp_peek",
    "wp_find_value",
    "wp_options_group",
    "wp_snapshot_take",
    "wp_snapshots",
    "wp_snapshot_diff",
    "wp_snapshot_delete",
    "wp_compare",
    "wp_plan_change",
];

fn inlined(commands: &'static [&'static str]) -> tauri_build::InlinedPlugin {
    tauri_build::InlinedPlugin::new()
        .commands(commands)
        .default_permission(tauri_build::DefaultPermissionRule::AllowAllCommands)
}

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .plugin("stage", inlined(STAGE_COMMANDS))
            .plugin("wp", inlined(WP_COMMANDS)),
    )
    .expect("tauri build");
}
