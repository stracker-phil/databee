use tauri::menu::{AboutMetadata, Menu, MenuItem, MenuItemBuilder, Submenu, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::state::AppState;
use crate::store::keybindings::{KeybindingsStore, accelerator, is_chord, key_label};

/// Menu items emit `command` events with the command id; the UI's registry runs them.
pub const COMMAND_EVENT: &str = "command";

pub struct MenuItemDef {
    pub id: &'static str,
    pub label: &'static str,
    /// Mirrors the command's `defaultBinding` in `src/commands`; empty means none.
    pub default_binding: &'static str,
    /// The command whose binding the item shows (a `fallbackFor` command has none of its own).
    pub binding_of: Option<&'static str>,
}

const fn item(id: &'static str, label: &'static str, default_binding: &'static str) -> MenuItemDef {
    MenuItemDef {
        id,
        label,
        default_binding,
        binding_of: None,
    }
}

pub const MENU_ITEMS: &[MenuItemDef] = &[
    item("settings.open", "Settings…", "Mod+,"),
    item("connection.new", "New Connection…", "Mod+N"),
    item("connection.switch", "Switch Connection…", "Mod+Shift+P"),
    item("connection.retry", "Retry Connection", ""),
    item("tabs.new", "New Query Tab", "Mod+T"),
    item("palette.openTable", "Open Table or Entity…", "Mod+P"),
    item("column.openInTab", "Open Pane in New Tab", "Mod+Alt+Enter"),
    MenuItemDef {
        binding_of: Some("changes.apply"),
        ..item("query.save", "Save Query", "")
    },
    item("query.pin", "Pin Query…", "P"),
    item("changes.apply", "Apply Pending Changes", "Mod+S"),
    item("changes.undo", "Undo Last Apply", ""),
    item("column.close", "Close Pane", "Mod+Shift+W"),
    item("tabs.close", "Close Tab", "Mod+W"),
    item("value.copyRaw", "Copy Value", "Y"),
    item(
        "value.copyCondition",
        "Copy Value as SQL Condition",
        "Shift+Y",
    ),
    item("value.edit", "Edit Value", "E"),
    item("view.sidebar", "Toggle Sidebar", "Mod+Ctrl+S"),
    item("object.viewDocument", "Show Document", "Mod+1"),
    item("object.viewRows", "Show Rows", "Mod+2"),
    item(
        "column.maximize",
        "Maximize or Restore Pane",
        "Mod+Shift+Enter",
    ),
    item("readout.open", "Readout", ""),
    item("project.open", "Project", ""),
    item("view.zoomIn", "Zoom In", "Mod+="),
    item("view.zoomOut", "Zoom Out", "Mod+-"),
    item("view.zoomReset", "Reset Zoom", "Mod+0"),
    item("view.refresh", "Refresh", "Mod+R"),
    item("editor.run", "Run Statement at Cursor", "Mod+Enter"),
    item("editor.runAll", "Run All Statements", "Mod+Shift+Enter"),
    item("table.focusFilter", "Value Filter (WHERE)", "Mod+F"),
    item(
        "table.filterMode",
        "Switch Filter Between Smart and SQL",
        "Ctrl+Alt+X",
    ),
    item("value.filter", "Filter Rows by Value", "F"),
    item("sidebar.focusFilter", "Filter Tables", "Mod+Shift+F"),
    item("object.live", "Toggle Live Mode", "W"),
    item("object.snapshot", "Take a Snapshot…", "S"),
    item("object.sinceSnapshot", "Changes Since Snapshot…", ""),
    item("object.dismissChanges", "Dismiss Change Marks", "X"),
    item("object.compare", "Compare with Another Row…", ""),
    item("value.addReference", "Add Reference from Value…", "R"),
    item("value.setKind", "Set Kind of Column or Meta Key…", ""),
    item("value.setStyle", "Add Value Style…", ""),
    item("value.watch", "Watch Value on Readout", ""),
    item("palette.open", "Command Palette…", "Mod+K"),
    item("nav.back", "Back", "Mod+["),
    item("nav.forward", "Forward", "Mod+]"),
    item("tabs.next", "Next Tab", "Ctrl+Tab"),
    item("tabs.previous", "Previous Tab", "Ctrl+Shift+Tab"),
    item(
        "column.focusLeft",
        "Focus Pane on the Left",
        "Mod+Alt+ArrowLeft",
    ),
    item(
        "column.focusRight",
        "Focus Pane on the Right",
        "Mod+Alt+ArrowRight",
    ),
    item("help.shortcuts", "Keyboard Shortcuts", "Shift+?"),
];

fn menu_item(id: &str) -> Option<&'static MenuItemDef> {
    MENU_ITEMS.iter().find(|def| def.id == id)
}

pub fn is_menu_command(id: &str) -> bool {
    menu_item(id).is_some()
}

pub fn binding_owner(id: &str) -> &str {
    menu_item(id).and_then(|def| def.binding_of).unwrap_or(id)
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
    let label = menu_item(id).map_or_else(
        || {
            log::warn!("Menu item {id} is missing from MENU_ITEMS");
            id
        },
        |def| def.label,
    );
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
    fn default_chord_bindings_convert_to_accelerators() {
        for item in MENU_ITEMS.iter().filter(|i| is_chord(i.default_binding)) {
            assert!(
                accelerator(item.default_binding).is_some(),
                "{}: {}",
                item.id,
                item.default_binding
            );
        }
    }

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
        let def = menu_item("view.sidebar").unwrap();
        assert_eq!(
            label_and_accelerator(def.label, Some(def.default_binding.into())),
            (
                "Toggle Sidebar".to_string(),
                Some("CmdOrCtrl+Ctrl+S".to_string())
            )
        );
    }
}
