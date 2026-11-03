use tauri::menu::{AboutMetadata, Menu, MenuItem, MenuItemBuilder, Submenu, SubmenuBuilder};
use tauri::{AppHandle, Manager, Wry};

use crate::bus;
use crate::state::AppState;
use crate::store::keybindings::{KeybindingsStore, accelerator, is_chord, key_label};

fn menu_label(id: &str) -> Option<&'static str> {
    match bus::find(id)?.menu {
        bus::MenuSlot::Item(label) => Some(label),
        _ => None,
    }
}

/// The command whose binding the item shows (a `fallback_for` command has none of its own).
fn binding_owner(id: &str) -> &str {
    bus::find(id)
        .and_then(|command| command.fallback_for)
        .unwrap_or(id)
}

/// A binding without ⌘ or ⌃ would take its key from text fields as a menu accelerator, so its keys go
/// into the label instead: `Copy Value (Y)`.
fn label_and_accelerator(label: &str, binding: Option<String>) -> (String, Option<String>) {
    let Some(binding) = binding else {
        return (label.to_string(), None);
    };
    if !is_chord(&binding) {
        return match key_label(&binding) {
            Some(keys) => (format!("{label} ({keys})"), None),
            None => (label.to_string(), None),
        };
    }
    let accel = accelerator(&binding);
    if accel.is_none() {
        log::warn!("No menu accelerator for {label} = {binding:?}");
    }
    (label.to_string(), accel)
}

fn build_item<M: Manager<Wry>>(
    app: &M,
    bindings: &KeybindingsStore,
    id: &str,
) -> tauri::Result<MenuItem<Wry>> {
    let label = menu_label(id).unwrap_or_else(|| {
        log::warn!("Menu item {id} has no menu label in the catalog");
        id
    });
    let (label, accel) = label_and_accelerator(label, bindings.menu_binding(binding_owner(id)));
    let mut builder = MenuItemBuilder::with_id(id, label);
    if let Some(accel) = accel {
        builder = builder.accelerator(accel);
    }
    builder.build(app)
}

fn submenu<M: Manager<Wry>>(
    app: &M,
    bindings: &KeybindingsStore,
    title: &str,
    groups: &[&[&str]],
) -> tauri::Result<Submenu<Wry>> {
    let mut builder = SubmenuBuilder::new(app, title);
    for (i, group) in groups.iter().enumerate() {
        if i > 0 {
            builder = builder.separator();
        }
        for id in *group {
            builder = builder.item(&build_item(app, bindings, id)?);
        }
    }
    builder.build()
}

pub fn build<M: Manager<Wry>>(app: &M, bindings: &KeybindingsStore) -> tauri::Result<Menu<Wry>> {
    let app_menu = SubmenuBuilder::new(app, "DataBee")
        .about(Some(AboutMetadata::default()))
        .separator()
        .item(&build_item(app, bindings, "settings.open")?)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;
    let file = submenu(
        app,
        bindings,
        "File",
        &[
            &["connection.new", "connection.switch", "connection.retry"],
            &["tabs.new", "palette.openTable", "column.openInTab"],
            &["query.save", "query.pin", "changes.apply", "changes.undo"],
            &["column.close", "tabs.close"],
        ],
    )?;
    let edit = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .separator()
        .item(&build_item(app, bindings, "value.copyRaw")?)
        .item(&build_item(app, bindings, "value.copyCondition")?)
        .item(&build_item(app, bindings, "value.edit")?)
        .build()?;
    let view = submenu(
        app,
        bindings,
        "View",
        &[
            &["view.sidebar"],
            &["object.viewDocument", "object.viewRows", "column.maximize"],
            &["readout.open", "project.open"],
            &["view.zoomIn", "view.zoomOut", "view.zoomReset"],
            &["view.refresh"],
        ],
    )?;
    let data = submenu(
        app,
        bindings,
        "Data",
        &[
            &["editor.run", "editor.runAll"],
            &[
                "table.focusFilter",
                "table.filterMode",
                "value.filter",
                "sidebar.focusFilter",
            ],
            &[
                "object.live",
                "object.snapshot",
                "object.sinceSnapshot",
                "object.dismissChanges",
                "object.compare",
            ],
            &[
                "value.addReference",
                "value.setKind",
                "value.setStyle",
                "value.watch",
            ],
        ],
    )?;
    let go = submenu(
        app,
        bindings,
        "Go",
        &[
            &["palette.open"],
            &["nav.back", "nav.forward"],
            &["tabs.next", "tabs.previous"],
            &["column.focusLeft", "column.focusRight"],
        ],
    )?;
    let window = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .fullscreen()
        .separator()
        .bring_all_to_front()
        .build()?;
    let help = submenu(app, bindings, "Help", &[&["help.shortcuts"]])?;
    Menu::with_items(
        app,
        &[&app_menu, &file, &edit, &view, &data, &go, &window, &help],
    )
}

/// Rebuilds the menu so changed key bindings show as accelerators.
pub fn rebuild(app: &AppHandle) {
    let state = app.state::<AppState>();
    let result = build(app, &state.keybindings).and_then(|menu| app.set_menu(menu));
    if let Err(e) = result {
        log::warn!("Couldn't rebuild the menu: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chord_binding_becomes_an_accelerator_with_the_plain_label() {
        assert_eq!(
            label_and_accelerator("Close Tab", Some("Mod+W".into())),
            ("Close Tab".to_string(), Some("CmdOrCtrl+W".to_string()))
        );
    }

    #[test]
    fn plain_key_binding_is_appended_to_the_label_without_accelerator() {
        assert_eq!(
            label_and_accelerator("Copy Value", Some("Y".into())),
            ("Copy Value (Y)".to_string(), None)
        );
        assert_eq!(
            label_and_accelerator("Copy Value as SQL Condition", Some("Shift+Y".into())),
            ("Copy Value as SQL Condition (⇧Y)".to_string(), None)
        );
    }

    #[test]
    fn shift_question_mark_is_shown_in_the_label() {
        assert_eq!(
            label_and_accelerator("Keyboard Shortcuts", Some("Shift+?".into())),
            ("Keyboard Shortcuts (⇧?)".to_string(), None)
        );
    }

    #[test]
    fn missing_binding_keeps_the_label_unchanged() {
        assert_eq!(
            label_and_accelerator("Readout", None),
            ("Readout".to_string(), None)
        );
    }

    #[test]
    fn items_show_their_own_binding_unless_they_borrow_one() {
        assert_eq!(binding_owner("query.save"), "changes.apply");
        for id in ["changes.apply", "tabs.new", "nope.unknown"] {
            assert_eq!(binding_owner(id), id);
        }
    }

    #[test]
    fn sidebar_toggle_gets_command_control_s_accelerator() {
        let def = bus::find("view.sidebar").unwrap();
        assert_eq!(
            label_and_accelerator(
                menu_label(def.id).unwrap(),
                def.default_binding.map(str::to_string)
            ),
            (
                "Toggle Sidebar".to_string(),
                Some("CmdOrCtrl+Ctrl+S".to_string())
            )
        );
    }
}
