//! User shortcut overrides (`keybindings.json`): `{ "commandId": "Mod+Shift+X" | null }`.
//! The webview resolves most shortcuts; the native menu reads the same file for its
//! accelerators so the two never disagree.

use super::json_store::JsonStore;
use crate::error::AppResult;
use crate::menu::MENU_ITEMS;
use std::collections::BTreeMap;
use std::path::Path;

/// `None` means the user unbound the command.
pub type Keybindings = BTreeMap<String, Option<String>>;

pub struct KeybindingsStore {
    store: JsonStore<Keybindings>,
}

impl KeybindingsStore {
    pub fn open(dir: &Path) -> Self {
        Self {
            store: JsonStore::open_with(dir.join("keybindings.json"), sanitize),
        }
    }

    pub fn get(&self) -> Keybindings {
        self.store.read(Keybindings::clone)
    }

    /// Replaces all overrides; malformed entries are dropped (see `sanitize`).
    pub async fn update(&self, bindings: Keybindings) -> AppResult<Keybindings> {
        let bindings = sanitize(bindings);
        self.store
            .update_async(move |current| {
                *current = bindings.clone();
                bindings
            })
            .await
    }

    /// The binding a menu item should show: the user's override, else the default.
    pub fn menu_binding(&self, id: &str) -> Option<String> {
        if let Some(custom) = self.store.read(|current| current.get(id).cloned()) {
            return custom;
        }
        MENU_ITEMS
            .iter()
            .find(|item| item.id == id)
            .map(|item| item.default_binding.to_string())
            .filter(|b| !b.is_empty())
    }
}

const MODIFIERS: &[&str] = &["Mod", "Ctrl", "Alt", "Shift"];

/// Drops overrides whose command id or binding the webview could never match. Ids are not
/// checked against the registry: the frontend owns most commands and Rust does not know them.
pub fn sanitize(bindings: Keybindings) -> Keybindings {
    bindings
        .into_iter()
        .filter(|(id, binding)| {
            let valid = is_command_id(id) && binding.as_deref().is_none_or(|b| split(b).is_some());
            if !valid {
                log::warn!("Ignoring key binding {id:?} = {binding:?}");
            }
            valid
        })
        .collect()
}

fn is_command_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
}

/// `Mod+Shift+K` into its modifiers and key. A trailing `++` (or a bare `+`) is the `+` key.
fn split(binding: &str) -> Option<(Vec<&str>, &str)> {
    let (modifiers, key) = if binding == "+" {
        ("", "+")
    } else if let Some(modifiers) = binding.strip_suffix("++") {
        (modifiers, "+")
    } else {
        binding.rsplit_once('+').unwrap_or(("", binding))
    };
    let modifiers: Vec<&str> = match modifiers {
        "" => Vec::new(),
        some => some.split('+').collect(),
    };
    let modifiers_valid = modifiers.iter().all(|m| MODIFIERS.contains(m));
    let single_char = key.chars().count() == 1 && !key.trim().is_empty();
    let named_key = !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric());
    let key_valid = !MODIFIERS.contains(&key) && (single_char || named_key);
    (modifiers_valid && key_valid).then_some((modifiers, key))
}

/// `Mod+Shift+K` → the `CmdOrCtrl+Shift+K` form Tauri accelerators expect; `None` when the
/// binding is malformed or its key has no Tauri accelerator name.
pub fn accelerator(binding: &str) -> Option<String> {
    let (modifiers, key) = split(binding)?;
    let mut parts: Vec<&str> = modifiers
        .iter()
        .map(|m| if *m == "Mod" { "CmdOrCtrl" } else { m })
        .collect();
    // Tauri accelerators name physical keys and have none for `+`: Shift+Equal on US layouts.
    if key == "+" && !parts.contains(&"Shift") {
        parts.push("Shift");
    }
    let key = match key {
        "+" | "=" => "Equal",
        "-" => "Minus",
        single if single.chars().count() == 1 && !is_accelerator_char(single) => return None,
        other => other,
    };
    parts.push(key);
    Some(parts.join("+"))
}

fn is_accelerator_char(key: &str) -> bool {
    key.chars()
        .all(|c| c.is_ascii_alphanumeric() || "`\\[],.';/".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overrides(entries: &[(&str, Option<&str>)]) -> Keybindings {
        entries
            .iter()
            .map(|(id, binding)| (id.to_string(), binding.map(String::from)))
            .collect()
    }

    #[test]
    fn menu_binding_without_override_is_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let store = KeybindingsStore::open(dir.path());
        assert_eq!(store.menu_binding("tabs.new").as_deref(), Some("Mod+T"));
    }

    #[tokio::test]
    async fn override_persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        KeybindingsStore::open(dir.path())
            .update(overrides(&[("tabs.new", Some("Mod+Shift+T"))]))
            .await
            .unwrap();
        let reopened = KeybindingsStore::open(dir.path());
        assert_eq!(
            reopened.menu_binding("tabs.new").as_deref(),
            Some("Mod+Shift+T")
        );
        assert_eq!(
            reopened.menu_binding("tabs.close").as_deref(),
            Some("Mod+W")
        );
    }

    #[tokio::test]
    async fn null_override_unbinds_a_default() {
        let dir = tempfile::tempdir().unwrap();
        let store = KeybindingsStore::open(dir.path());
        store
            .update(overrides(&[("favorites.add", None)]))
            .await
            .unwrap();
        assert_eq!(store.menu_binding("favorites.add"), None);
        assert_eq!(
            KeybindingsStore::open(dir.path()).menu_binding("favorites.add"),
            None
        );
    }

    #[test]
    fn unknown_id_has_no_menu_binding() {
        let dir = tempfile::tempdir().unwrap();
        let store = KeybindingsStore::open(dir.path());
        assert_eq!(store.menu_binding("unknown"), None);
    }

    #[test]
    fn command_without_default_has_no_menu_binding() {
        let dir = tempfile::tempdir().unwrap();
        let store = KeybindingsStore::open(dir.path());
        assert_eq!(store.menu_binding("help.shortcuts"), None);
    }

    #[test]
    fn bindings_convert_to_tauri_accelerators() {
        assert_eq!(
            accelerator("Mod+Shift+K").as_deref(),
            Some("CmdOrCtrl+Shift+K")
        );
        assert_eq!(accelerator("Mod+[").as_deref(), Some("CmdOrCtrl+["));
        assert_eq!(accelerator("Mod+=").as_deref(), Some("CmdOrCtrl+Equal"));
        assert_eq!(accelerator("Ctrl+Tab").as_deref(), Some("Ctrl+Tab"));
    }

    #[test]
    fn minus_and_plus_keys_convert_to_physical_key_names() {
        for (binding, expected) in [
            ("Mod+-", Some("CmdOrCtrl+Minus")),
            ("Mod++", Some("CmdOrCtrl+Shift+Equal")),
            ("Mod+Shift++", Some("CmdOrCtrl+Shift+Equal")),
            ("Mod+Ä", None),
        ] {
            assert_eq!(accelerator(binding).as_deref(), expected, "{binding}");
        }
    }

    #[test]
    fn sanitize_drops_bad_syntax_and_bad_ids() {
        let cleaned = sanitize(overrides(&[
            ("a.bad", Some("Mod+")),
            ("b.bad", Some("Hyper+K")),
            ("c.bad", Some("Mod+K+J")),
            ("bad id!", Some("Mod+K")),
            ("tabs.new", Some("Mod+Shift+T")),
            ("favorites.add", None),
            ("view.zoomIn", Some("Mod++")),
        ]));
        assert_eq!(
            cleaned,
            overrides(&[
                ("tabs.new", Some("Mod+Shift+T")),
                ("favorites.add", None),
                ("view.zoomIn", Some("Mod++")),
            ])
        );
    }

    #[tokio::test]
    async fn update_returns_the_sanitized_map_and_persists_it() {
        let dir = tempfile::tempdir().unwrap();
        let store = KeybindingsStore::open(dir.path());
        let saved = store
            .update(overrides(&[
                ("tabs.new", Some("Mod+Shift+T")),
                ("tabs.close", Some("Hyper+K")),
            ]))
            .await
            .unwrap();
        let expected = overrides(&[("tabs.new", Some("Mod+Shift+T"))]);
        assert_eq!(saved, expected);
        assert_eq!(KeybindingsStore::open(dir.path()).get(), expected);
    }

    #[test]
    fn hand_edited_file_with_a_malformed_entry_loads_without_it() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("keybindings.json"),
            r#"{"tabs.new": "Mod+", "tabs.close": "Mod+Q"}"#,
        )
        .unwrap();
        let store = KeybindingsStore::open(dir.path());
        assert_eq!(store.get(), overrides(&[("tabs.close", Some("Mod+Q"))]));
        assert_eq!(store.menu_binding("tabs.new").as_deref(), Some("Mod+T"));
    }
}
