use tauri::menu::{AboutMetadata, Menu, MenuItemBuilder, SubmenuBuilder};
use tauri::{App, Emitter, Wry};

pub const MENU_EVENT: &str = "menu";

/// Custom items emit `menu` events with their id; the UI maps ids to actions.
const ITEMS: &[(&str, &str, &str)] = &[
    ("new_connection", "New Connection…", "CmdOrCtrl+N"),
    ("new_query", "New Query Tab", "CmdOrCtrl+T"),
    ("close_tab", "Close Tab", "CmdOrCtrl+W"),
    ("add_favorite", "Add to Favorites", "CmdOrCtrl+D"),
    ("settings", "Settings…", "CmdOrCtrl+,"),
    ("apply_changes", "Apply Pending Changes", "CmdOrCtrl+S"),
    ("go_back", "Back", "CmdOrCtrl+["),
    ("go_forward", "Forward", "CmdOrCtrl+]"),
];

pub fn build(app: &App) -> tauri::Result<Menu<Wry>> {
    let item = |id: &str| {
        let (id, label, accel) = ITEMS.iter().find(|i| i.0 == id).unwrap();
        MenuItemBuilder::with_id(*id, *label)
            .accelerator(*accel)
            .build(app)
    };
    let app_menu = SubmenuBuilder::new(app, "databee")
        .about(Some(AboutMetadata::default()))
        .separator()
        .item(&item("settings")?)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;
    let file = SubmenuBuilder::new(app, "File")
        .item(&item("new_connection")?)
        .item(&item("new_query")?)
        .separator()
        .item(&item("apply_changes")?)
        .separator()
        .item(&item("close_tab")?)
        .build()?;
    let edit = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .separator()
        .item(&item("add_favorite")?)
        .build()?;
    let go = SubmenuBuilder::new(app, "Go")
        .item(&item("go_back")?)
        .item(&item("go_forward")?)
        .build()?;
    let window = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .fullscreen()
        .separator()
        .bring_all_to_front()
        .build()?;
    Menu::with_items(app, &[&app_menu, &file, &edit, &go, &window])
}

pub fn forward(app: &tauri::AppHandle, id: &str) {
    if ITEMS.iter().any(|i| i.0 == id) {
        let _ = app.emit(MENU_EVENT, id);
    }
}
