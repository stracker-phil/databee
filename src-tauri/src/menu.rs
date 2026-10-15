use tauri::menu::{AboutMetadata, Menu, MenuItemBuilder, SubmenuBuilder};
use tauri::{App, Emitter, Wry};

use crate::store::keybindings::{KeybindingsStore, MENU_DEFAULTS, accelerator};

/// Menu items emit `command` events with the command id; the UI's registry runs them.
pub const COMMAND_EVENT: &str = "command";

const LABELS: &[(&str, &str)] = &[
    ("connection.new", "New Connection…"),
    ("tabs.new", "New Query Tab"),
    ("tabs.close", "Close Tab"),
    ("favorites.add", "Add to Favorites"),
    ("settings.open", "Settings…"),
    ("changes.apply", "Apply Pending Changes"),
    ("nav.back", "Back"),
    ("nav.forward", "Forward"),
];

pub fn build(app: &App, bindings: &KeybindingsStore) -> tauri::Result<Menu<Wry>> {
    let item = |id: &str| {
        let (id, label) = LABELS.iter().find(|i| i.0 == id).unwrap();
        let mut builder = MenuItemBuilder::with_id(*id, *label);
        if let Some(binding) = bindings.menu_binding(id) {
            builder = builder.accelerator(accelerator(&binding));
        }
        builder.build(app)
    };
    let app_menu = SubmenuBuilder::new(app, "databee")
        .about(Some(AboutMetadata::default()))
        .separator()
        .item(&item("settings.open")?)
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
        .item(&item("connection.new")?)
        .item(&item("tabs.new")?)
        .separator()
        .item(&item("changes.apply")?)
        .separator()
        .item(&item("tabs.close")?)
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
        .item(&item("favorites.add")?)
        .build()?;
    let go = SubmenuBuilder::new(app, "Go")
        .item(&item("nav.back")?)
        .item(&item("nav.forward")?)
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
    if MENU_DEFAULTS.iter().any(|i| i.0 == id) {
        let _ = app.emit(COMMAND_EVENT, id);
    }
}
