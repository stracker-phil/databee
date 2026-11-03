//! Every command of the app, in one list: the native menu, the keybindings file, the CLI, MCP and the page
//! (through `src/commands/catalog.gen.json`) read it, so they can't disagree on ids, titles or keys.

use serde::Serialize;
use ts_rs::TS;

/// Where a command runs. Page commands need the page's state (the active tab, the selection); Rust commands
/// run without it, also while the window is hidden.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub enum Runs {
    Page,
    Rust,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuSlot {
    None,
    /// A native menu item with this label.
    Item(&'static str),
    /// Runs through another command's menu item, while that one's scope is not active.
    Shares(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct CommandDef {
    pub id: &'static str,
    pub title: &'static str,
    /// One of `SCOPES`: where its key fires.
    pub scope: &'static str,
    /// In the page's notation (`Mod+Shift+K`).
    pub default_binding: Option<&'static str>,
    pub menu: MenuSlot,
    /// Has no key of its own: it takes this command's binding and runs while that one is disabled.
    pub fallback_for: Option<&'static str>,
    /// Leaves its key to text fields and the editor even with a modifier.
    pub skip_in_text: bool,
    pub runs: Runs,
}

/// The page's scopes (`src/commands/index.ts` `registerCoreScopes`).
#[cfg(test)]
pub const SCOPES: &[&str] = &["global", "db", "table", "editor", "object"];

const fn page(id: &'static str, title: &'static str, scope: &'static str) -> CommandDef {
    CommandDef {
        id,
        title,
        scope,
        default_binding: None,
        menu: MenuSlot::None,
        fallback_for: None,
        skip_in_text: false,
        runs: Runs::Page,
    }
}

impl CommandDef {
    const fn key(self, binding: &'static str) -> Self {
        Self {
            default_binding: Some(binding),
            ..self
        }
    }

    const fn menu(self, label: &'static str) -> Self {
        Self {
            menu: MenuSlot::Item(label),
            ..self
        }
    }

    const fn shares(self, id: &'static str) -> Self {
        Self {
            menu: MenuSlot::Shares(id),
            ..self
        }
    }

    const fn fallback_for(self, id: &'static str) -> Self {
        Self {
            fallback_for: Some(id),
            ..self
        }
    }

    const fn skip_in_text(self) -> Self {
        Self {
            skip_in_text: true,
            ..self
        }
    }

    const fn in_rust(self) -> Self {
        Self {
            runs: Runs::Rust,
            ..self
        }
    }
}

pub const CATALOG: &[CommandDef] = &[
    page("palette.open", "Command palette", "global")
        .key("Mod+K")
        .menu("Command Palette…"),
    page("palette.openTable", "Open table or entity", "global")
        .key("Mod+P")
        .menu("Open Table or Entity…"),
    page("nav.back", "Back", "global").key("Mod+[").menu("Back"),
    page("nav.forward", "Forward", "global")
        .key("Mod+]")
        .menu("Forward"),
    page("view.zoomIn", "Zoom in", "global")
        .key("Mod+=")
        .menu("Zoom In")
        .in_rust(),
    page("view.zoomOut", "Zoom out", "global")
        .key("Mod+-")
        .menu("Zoom Out")
        .in_rust(),
    page("view.zoomReset", "Reset zoom", "global")
        .key("Mod+0")
        .menu("Reset Zoom")
        .in_rust(),
    page("view.sidebar", "Toggle sidebar", "global")
        .key("Mod+Ctrl+S")
        .menu("Toggle Sidebar"),
    page("view.refresh", "Refresh current view", "global")
        .key("Mod+R")
        .menu("Refresh"),
    page("help.shortcuts", "Keyboard shortcuts", "global")
        .key("Shift+?")
        .menu("Keyboard Shortcuts"),
    page("table.focusFilter", "Value filter (WHERE)", "table")
        .key("Mod+F")
        .menu("Value Filter (WHERE)"),
    page(
        "table.filterMode",
        "Switch the WHERE filter between smart and SQL",
        "table",
    )
    .key("Ctrl+Alt+X")
    .menu("Switch Filter Between Smart and SQL"),
    page("sidebar.focusFilter", "Filter tables", "db")
        .key("Mod+Shift+F")
        .menu("Filter Tables"),
    page("tabs.new", "New query tab", "global")
        .key("Mod+T")
        .menu("New Query Tab"),
    page("tabs.close", "Close tab", "global")
        .key("Mod+W")
        .menu("Close Tab"),
    page("tabs.next", "Next tab", "db")
        .key("Ctrl+Tab")
        .menu("Next Tab"),
    page("tabs.previous", "Previous tab", "db")
        .key("Ctrl+Shift+Tab")
        .menu("Previous Tab"),
    page("editor.run", "Run statement at cursor", "editor")
        .key("Mod+Enter")
        .menu("Run Statement at Cursor"),
    page("editor.runAll", "Run all statements", "editor")
        .key("Mod+Shift+Enter")
        .menu("Run All Statements"),
    page("object.viewDocument", "Show Document", "object")
        .key("Mod+1")
        .menu("Show Document"),
    page("object.viewRows", "Show Rows", "object")
        .key("Mod+2")
        .menu("Show Rows"),
    page("readout.open", "Readout", "global").menu("Readout"),
    page("project.open", "Project", "global").menu("Project"),
    page("connection.new", "New connection…", "global")
        .key("Mod+N")
        .menu("New Connection…"),
    page("connection.switch", "Switch connection…", "global")
        .key("Mod+Shift+P")
        .menu("Switch Connection…"),
    page("connection.retry", "Retry connection", "global").menu("Retry Connection"),
    page("settings.open", "Settings…", "global")
        .key("Mod+,")
        .menu("Settings…")
        .in_rust(),
    page("query.save", "Save query", "editor")
        .fallback_for("changes.apply")
        .menu("Save Query"),
    page("query.pin", "Pin query…", "editor")
        .key("P")
        .menu("Pin Query…"),
    page("changes.apply", "Apply pending changes", "global")
        .key("Mod+S")
        .menu("Apply Pending Changes"),
    page("changes.undo", "Undo last apply", "global").menu("Undo Last Apply"),
    // Values: the table-scope commands own the menu items; the object-scope ones share them.
    page("value.copyRaw", "Copy value", "table")
        .key("Y")
        .menu("Copy Value"),
    page(
        "value.copyCondition",
        "Copy value as SQL condition",
        "table",
    )
    .key("Shift+Y")
    .menu("Copy Value as SQL Condition"),
    page("value.edit", "Edit value", "table")
        .key("E")
        .menu("Edit Value"),
    page("value.filter", "Filter rows by value", "table")
        .key("F")
        .menu("Filter Rows by Value"),
    page(
        "value.addReference",
        "Add reference from this value…",
        "table",
    )
    .key("R")
    .menu("Add Reference from Value…"),
    page(
        "value.setKind",
        "Set the kind of this column or meta key…",
        "table",
    )
    .menu("Set Kind of Column or Meta Key…"),
    page(
        "value.setStyle",
        "Add a value style for this value…",
        "table",
    )
    .menu("Add Value Style…"),
    page("value.watch", "Watch value on the readout", "table").menu("Watch Value on Readout"),
    page("object.copyRaw", "Copy value", "object")
        .key("Y")
        .shares("value.copyRaw"),
    page(
        "object.copyCondition",
        "Copy value as SQL condition",
        "object",
    )
    .key("Shift+Y")
    .shares("value.copyCondition"),
    page("object.edit", "Edit value", "object")
        .key("E")
        .shares("value.edit"),
    page("object.filter", "Filter rows by value", "object")
        .key("F")
        .shares("value.filter"),
    page(
        "object.addReference",
        "Add reference from this value…",
        "object",
    )
    .key("R")
    .shares("value.addReference"),
    page(
        "object.setKind",
        "Set the kind of this column or meta key…",
        "object",
    )
    .shares("value.setKind"),
    page(
        "object.setStyle",
        "Add a value style for this value…",
        "object",
    )
    .shares("value.setStyle"),
    page("object.watch", "Watch value on the readout", "object").shares("value.watch"),
    // Rows of the selected grid cell.
    page("row.delete", "Delete row", "table")
        .key("Mod+Backspace")
        .skip_in_text(),
    page("row.duplicate", "Duplicate row", "table")
        .key("Mod+D")
        .skip_in_text(),
    page("value.setNull", "Set value to NULL", "table")
        .key("Mod+Shift+Backspace")
        .skip_in_text(),
    page("row.openObject", "Open row as object", "table"),
    page("row.copyTsv", "Copy row as TSV", "db"),
    page("row.copyJson", "Copy row as JSON", "db"),
    page("grid.copyAllTsv", "Copy all rows as TSV", "db"),
    page("grid.copyAllJson", "Copy all rows as JSON", "db"),
    // Object tabs: values, columns and panes.
    page("object.down", "Focus the next value", "object").key("J"),
    page("object.downArrow", "Focus the next value (arrow)", "object").key("ArrowDown"),
    page("object.up", "Focus the previous value", "object").key("K"),
    page(
        "object.upArrow",
        "Focus the previous value (arrow)",
        "object",
    )
    .key("ArrowUp"),
    page("object.left", "Move to the column on the left", "object").key("H"),
    page(
        "object.leftArrow",
        "Move to the column on the left (arrow)",
        "object",
    )
    .key("ArrowLeft"),
    page("object.right", "Move to the column on the right", "object").key("L"),
    page(
        "object.rightArrow",
        "Move to the column on the right (arrow)",
        "object",
    )
    .key("ArrowRight"),
    page(
        "object.drill",
        "Open the focused reference in a new column",
        "object",
    )
    .key("Enter"),
    page(
        "column.maximize",
        "Maximize or restore the active pane",
        "object",
    )
    .key("Mod+Shift+Enter")
    .menu("Maximize or Restore Pane"),
    page("column.focusLeft", "Focus the pane on the left", "object")
        .key("Mod+Alt+ArrowLeft")
        .menu("Focus Pane on the Left"),
    page("column.focusRight", "Focus the pane on the right", "object")
        .key("Mod+Alt+ArrowRight")
        .menu("Focus Pane on the Right"),
    page(
        "column.close",
        "Close the active pane and the panes opened from it",
        "object",
    )
    .key("Mod+Shift+W")
    .menu("Close Pane"),
    page(
        "column.openInTab",
        "Open the active pane's row in a new tab",
        "object",
    )
    .key("Mod+Alt+Enter")
    .menu("Open Pane in New Tab"),
    page(
        "columns.close",
        "Close the rightmost column (restores a maximized pane first)",
        "global",
    )
    .key("Escape"),
    // Live mode and snapshots.
    page("object.live", "Toggle live mode", "object")
        .key("W")
        .menu("Toggle Live Mode"),
    page("object.dismissChanges", "Dismiss change marks", "object")
        .key("X")
        .menu("Dismiss Change Marks"),
    page("object.snapshot", "Take a snapshot…", "object")
        .key("S")
        .menu("Take a Snapshot…"),
    page("object.sinceSnapshot", "Changes since snapshot…", "object")
        .menu("Changes Since Snapshot…"),
    page("object.compare", "Compare with another row…", "object").menu("Compare with Another Row…"),
    page("query.snapshot", "Snapshot saved query results…", "editor").shares("object.snapshot"),
    page("query.sinceSnapshot", "Changes since snapshot…", "editor").shares("object.sinceSnapshot"),
    page("query.dismissChanges", "Dismiss change marks", "editor").shares("object.dismissChanges"),
];

pub fn find(id: &str) -> Option<&'static CommandDef> {
    CATALOG.iter().find(|command| command.id == id)
}

/// A catalog entry as the page and `databee commands --json` read it.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct CommandInfo {
    pub id: String,
    pub title: String,
    pub scope: String,
    pub default_binding: Option<String>,
    /// The label of its own native menu item.
    pub menu_label: Option<String>,
    /// The command whose native menu item it runs through.
    pub menu_shares: Option<String>,
    pub fallback_for: Option<String>,
    pub skip_in_text: bool,
    pub runs: Runs,
}

impl From<&CommandDef> for CommandInfo {
    fn from(def: &CommandDef) -> Self {
        let (menu_label, menu_shares) = match def.menu {
            MenuSlot::None => (None, None),
            MenuSlot::Item(label) => (Some(label.to_string()), None),
            MenuSlot::Shares(id) => (None, Some(id.to_string())),
        };
        Self {
            id: def.id.to_string(),
            title: def.title.to_string(),
            scope: def.scope.to_string(),
            default_binding: def.default_binding.map(str::to_string),
            menu_label,
            menu_shares,
            fallback_for: def.fallback_for.map(str::to_string),
            skip_in_text: def.skip_in_text,
            runs: def.runs,
        }
    }
}

pub fn infos() -> Vec<CommandInfo> {
    CATALOG.iter().map(CommandInfo::from).collect()
}

/// `databee commands`: one `id, title, default key` line per command, tab-separated.
pub fn catalog_text() -> String {
    CATALOG
        .iter()
        .map(|command| {
            let key = command.default_binding.unwrap_or("");
            format!("{}\t{}\t{key}\n", command.id, command.title)
        })
        .collect()
}

pub fn catalog_json() -> String {
    let mut json = serde_json::to_string_pretty(&infos()).expect("the catalog always serializes");
    json.push('\n');
    json
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::keybindings::{accelerator, is_chord};
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique() {
        let mut seen = HashSet::new();
        for command in CATALOG {
            assert!(seen.insert(command.id), "duplicate id {}", command.id);
        }
    }

    #[test]
    fn every_scope_is_a_known_scope() {
        for command in CATALOG {
            assert!(
                SCOPES.contains(&command.scope),
                "{}: {}",
                command.id,
                command.scope
            );
        }
    }

    #[test]
    fn fallback_targets_exist() {
        for command in CATALOG {
            if let Some(target) = command.fallback_for {
                assert!(
                    find(target).is_some(),
                    "{} falls back to {target}",
                    command.id
                );
            }
        }
    }

    #[test]
    fn shared_menu_items_belong_to_another_command_with_its_own_item() {
        for command in CATALOG {
            if let MenuSlot::Shares(target) = command.menu {
                let owner = find(target)
                    .unwrap_or_else(|| panic!("{} shares missing {target}", command.id));
                assert!(
                    matches!(owner.menu, MenuSlot::Item(_)),
                    "{} shares {target}, which has no menu item of its own",
                    command.id
                );
            }
        }
    }

    #[test]
    fn chord_default_bindings_convert_to_menu_accelerators() {
        for command in CATALOG {
            if let Some(binding) = command.default_binding
                && is_chord(binding)
            {
                assert!(accelerator(binding).is_some(), "{}: {binding}", command.id);
            }
        }
    }

    #[test]
    fn info_maps_an_own_menu_item_to_its_label() {
        let info = CommandInfo::from(find("tabs.close").unwrap());
        assert_eq!(
            serde_json::to_value(&info).unwrap(),
            serde_json::json!({
                "id": "tabs.close",
                "title": "Close tab",
                "scope": "global",
                "defaultBinding": "Mod+W",
                "menuLabel": "Close Tab",
                "menuShares": null,
                "fallbackFor": null,
                "skipInText": false,
                "runs": "page",
            })
        );
    }

    #[test]
    fn info_maps_a_shared_menu_item_to_the_owning_command() {
        let info = CommandInfo::from(find("object.copyRaw").unwrap());
        assert_eq!(info.menu_label, None);
        assert_eq!(info.menu_shares.as_deref(), Some("value.copyRaw"));
    }

    #[test]
    fn info_of_a_command_without_menu_item_has_neither() {
        let info = CommandInfo::from(find("row.copyTsv").unwrap());
        assert_eq!((info.menu_label, info.menu_shares), (None, None));
    }

    #[test]
    fn info_carries_fallback_text_and_runs_flags() {
        let save = CommandInfo::from(find("query.save").unwrap());
        assert_eq!(save.fallback_for.as_deref(), Some("changes.apply"));
        assert_eq!(save.default_binding, None);
        assert!(CommandInfo::from(find("row.delete").unwrap()).skip_in_text);
        assert_eq!(
            CommandInfo::from(find("view.zoomIn").unwrap()).runs,
            Runs::Rust
        );
    }

    #[test]
    fn catalog_json_has_one_entry_per_command_in_order() {
        let json = catalog_json();
        assert!(json.ends_with('\n'));
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let ids: Vec<&str> = parsed
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["id"].as_str().unwrap())
            .collect();
        let expected: Vec<&str> = CATALOG.iter().map(|command| command.id).collect();
        assert_eq!(ids, expected);
    }

    #[test]
    fn find_returns_none_for_an_unknown_id() {
        assert!(find("nope.unknown").is_none());
    }
}

#[cfg(test)]
mod export {
    use super::*;

    /// The page imports this file; `bun run types` writes it (the name starts with `export_bindings`) and
    /// the lint drift check compares it.
    #[test]
    fn export_bindings_command_catalog() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../src/commands/catalog.gen.json");
        std::fs::write(path, catalog_json()).unwrap();
    }
}
