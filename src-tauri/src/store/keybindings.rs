//! User shortcut overrides (`keybindings.json`): `{ "commandId": "Mod+Shift+X" | null }`.
//! The webview resolves most shortcuts; the native menu reads the same file for its
//! accelerators so the two never disagree.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use super::json_file::JsonFile;
use crate::error::AppResult;

/// `None` means the user unbound the command.
pub type Keybindings = BTreeMap<String, Option<String>>;

/// Commands that live in the native menu, with their default bindings (mirrors `src/commands`);
/// an empty binding means none by default.
pub const MENU_DEFAULTS: &[(&str, &str)] = &[
    ("connection.new", "Mod+N"),
    ("tabs.new", "Mod+T"),
    ("tabs.close", "Mod+W"),
    ("favorites.add", "Mod+D"),
    ("settings.open", "Mod+,"),
    ("changes.apply", "Mod+S"),
    ("nav.back", "Mod+["),
    ("nav.forward", "Mod+]"),
    ("view.inspectorDock", ""),
];

pub struct KeybindingsStore {
    file: JsonFile,
    current: Mutex<Keybindings>,
}

impl KeybindingsStore {
    pub fn open(dir: &Path) -> Self {
        let file = JsonFile::new(dir.join("keybindings.json"));
        let current = file.load::<Keybindings>();
        Self {
            file,
            current: Mutex::new(current),
        }
    }

    pub fn get(&self) -> Keybindings {
        self.current.lock().unwrap().clone()
    }

    pub fn update(&self, bindings: Keybindings) -> AppResult<Keybindings> {
        self.file.save(&bindings)?;
        *self.current.lock().unwrap() = bindings.clone();
        Ok(bindings)
    }

    /// The binding a menu item should show: the user's override, else the default.
    pub fn menu_binding(&self, id: &str) -> Option<String> {
        let current = self.current.lock().unwrap();
        match current.get(id) {
            Some(custom) => custom.clone(),
            None => MENU_DEFAULTS
                .iter()
                .find(|(i, _)| *i == id)
                .map(|(_, b)| b.to_string())
                .filter(|b| !b.is_empty()),
        }
    }
}

/// `Mod+Shift+K` → the `CmdOrCtrl+Shift+K` form Tauri accelerators expect.
pub fn accelerator(binding: &str) -> String {
    binding
        .split('+')
        .map(|part| match part {
            "Mod" => "CmdOrCtrl".to_string(),
            "=" => "Equal".to_string(),
            "-" => "Minus".to_string(),
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_persist_and_fall_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let store = KeybindingsStore::open(dir.path());
        assert_eq!(store.menu_binding("tabs.new").as_deref(), Some("Mod+T"));
        let mut custom = Keybindings::new();
        custom.insert("tabs.new".into(), Some("Mod+Shift+T".into()));
        custom.insert("favorites.add".into(), None);
        store.update(custom).unwrap();

        let reopened = KeybindingsStore::open(dir.path());
        assert_eq!(
            reopened.menu_binding("tabs.new").as_deref(),
            Some("Mod+Shift+T")
        );
        assert_eq!(reopened.menu_binding("favorites.add"), None);
        assert_eq!(
            reopened.menu_binding("tabs.close").as_deref(),
            Some("Mod+W")
        );
        assert_eq!(reopened.menu_binding("unknown"), None);
        assert_eq!(reopened.menu_binding("view.inspectorDock"), None);
    }

    #[test]
    fn bindings_convert_to_tauri_accelerators() {
        assert_eq!(accelerator("Mod+Shift+K"), "CmdOrCtrl+Shift+K");
        assert_eq!(accelerator("Mod+["), "CmdOrCtrl+[");
        assert_eq!(accelerator("Mod+="), "CmdOrCtrl+Equal");
        assert_eq!(accelerator("Ctrl+Tab"), "Ctrl+Tab");
    }
}
