use tauri::menu::{AboutMetadata, Menu, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::state::AppState;
use crate::store::keybindings::{KeybindingsStore, accelerator};

/// Menu items emit `command` events with the command id; the UI's registry runs them.
pub const COMMAND_EVENT: &str = "command";

pub struct MenuItemDef {
    pub id: &'static str,
    pub label: &'static str,
    /// Mirrors the command's `defaultBinding` in `src/commands`; empty means none.
    pub default_binding: &'static str,
}

const fn item(id: &'static str, label: &'static str, default_binding: &'static str) -> MenuItemDef {
    MenuItemDef {
        id,
        label,
        default_binding,
    }
}

pub const MENU_ITEMS: &[MenuItemDef] = &[
    item("connection.new", "New Connection…", "Mod+N"),
    item("connection.switch", "Switch Connection…", "Mod+Shift+P"),
    item("tabs.new", "New Query Tab", "Mod+T"),
    item("tabs.close", "Close Tab", "Mod+W"),
    item("settings.open", "Settings…", "Mod+,"),
    item("changes.apply", "Apply Pending Changes", "Mod+S"),
    item("nav.back", "Back", "Mod+["),
    item("nav.forward", "Forward", "Mod+]"),
    item("view.refresh", "Refresh", "Mod+R"),
];

pub fn is_menu_command(id: &str) -> bool {
    MENU_ITEMS.iter().any(|item| item.id == id)
}

pub fn build<M: Manager<Wry>>(app: &M, bindings: &KeybindingsStore) -> tauri::Result<Menu<Wry>> {
    let item = |id: &str| {
        let label = match MENU_ITEMS.iter().find(|def| def.id == id) {
            Some(def) => def.label,
            None => {
                log::warn!("Menu item {id} is missing from MENU_ITEMS");
                id
            }
        };
        let mut builder = MenuItemBuilder::with_id(id, label);
        if let Some(binding) = bindings.menu_binding(id) {
            match accelerator(&binding) {
                Some(accel) => builder = builder.accelerator(accel),
                None => log::warn!("No menu accelerator for {id} = {binding:?}"),
            }
        }
        builder.build(app)
    };
    let app_menu = SubmenuBuilder::new(app, "DataBee")
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
        .item(&item("connection.switch")?)
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
        .build()?;
    let view = SubmenuBuilder::new(app, "View")
        .item(&item("view.refresh")?)
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
    Menu::with_items(app, &[&app_menu, &file, &edit, &view, &go, &window])
}

/// Rebuilds the menu so changed key bindings show as accelerators.
pub fn rebuild(app: &AppHandle) {
    let state = app.state::<AppState>();
    let result = build(app, &state.keybindings).and_then(|menu| app.set_menu(menu));
    if let Err(e) = result {
        log::warn!("Couldn't rebuild the menu: {e}");
    }
}

pub fn forward(app: &AppHandle, id: &str) {
    if !is_menu_command(id) {
        return;
    }
    if let Err(e) = app.emit(COMMAND_EVENT, id) {
        log::warn!("Couldn't forward menu command {id}: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_item_ids_are_unique_and_labelled() {
        for (i, item) in MENU_ITEMS.iter().enumerate() {
            assert!(!item.label.is_empty(), "{} has no label", item.id);
            assert!(
                MENU_ITEMS[..i].iter().all(|other| other.id != item.id),
                "duplicate id {}",
                item.id
            );
        }
    }

    #[test]
    fn only_listed_ids_are_menu_commands() {
        for item in MENU_ITEMS {
            assert!(is_menu_command(item.id), "{}", item.id);
        }
        assert!(!is_menu_command("nope.unknown"));
        assert!(!is_menu_command(""));
    }

    #[test]
    fn default_bindings_convert_to_accelerators() {
        for item in MENU_ITEMS.iter().filter(|i| !i.default_binding.is_empty()) {
            assert!(
                accelerator(item.default_binding).is_some(),
                "{}: {}",
                item.id,
                item.default_binding
            );
        }
    }

    #[test]
    fn menu_defaults_match_the_frontend_command_registry() {
        let registry = include_str!("../../src/commands/index.ts");
        for item in MENU_ITEMS {
            let marker = format!("id: \"{}\"", item.id);
            let start = registry
                .find(&marker)
                .unwrap_or_else(|| panic!("{} is not in src/commands/index.ts", item.id));
            let rest = &registry[start + marker.len()..];
            let entry = &rest[..rest.find("id: \"").unwrap_or(rest.len())];
            assert!(
                entry.contains("menu: true"),
                "{} is not a menu command",
                item.id
            );
            if item.default_binding.is_empty() {
                assert!(
                    !entry.contains("defaultBinding"),
                    "{} has a frontend default but none in MENU_ITEMS",
                    item.id
                );
            } else {
                let expected = format!("defaultBinding: \"{}\"", item.default_binding);
                assert!(
                    entry.contains(&expected),
                    "{}: expected {expected}",
                    item.id
                );
            }
        }
    }
}
