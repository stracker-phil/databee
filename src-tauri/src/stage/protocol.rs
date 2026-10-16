//! The Stage hook protocol's pure parts (docs/stage.md): rectangles in screen points, request
//! bounds, key combinations and the register command. The page's messages are in `page.rs`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const PROTOCOL: u32 = 1;
/// The Tauri label of DataBee's only native window, and the id its page answers with.
pub const MAIN: &str = "main";

/// Flat request parameters: query string, JSON primitives or a form body.
pub type Params = HashMap<String, String>;

/// A rejected request; `status` is the HTTP status code.
#[derive(Debug, Clone, PartialEq)]
pub struct StageError {
    pub status: u16,
    pub message: String,
}

impl StageError {
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub fn bad(message: impl Into<String>) -> Self {
        Self::new(400, message)
    }
}

pub type StageResult<T> = Result<T, StageError>;

/// The value of `key`, trimmed; 400 when it is missing or empty.
pub fn required<'a>(params: &'a Params, key: &str) -> StageResult<&'a str> {
    match params.get(key).map(|v| v.trim()) {
        Some(v) if !v.is_empty() => Ok(v),
        _ => Err(StageError::bad(format!("“{key}” is missing."))),
    }
}

/// CSS pixels from the page's top-left, as a page measures them.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, TS)]
#[ts(export, export_to = "src/stage/types.gen.ts")]
#[ts(rename = "StageRect")]
pub struct PageRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Whole screen points, origin at the main display's top-left.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenRect {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

/// A page rectangle in screen points: the window's content origin is added, then rounded.
pub fn to_screen_rect(rect: PageRect, content: ScreenRect) -> ScreenRect {
    ScreenRect {
        x: (content.x as f64 + rect.x).round() as i64,
        y: (content.y as f64 + rect.y).round() as i64,
        width: rect.width.round() as i64,
        height: rect.height.round() as i64,
    }
}

/// `x`, `y`, `width`, `height` from the request: only those given, whole numbers.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub x: Option<i64>,
    pub y: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
}

impl Bounds {
    pub fn is_empty(&self) -> bool {
        *self == Bounds::default()
    }

    /// The given values over `current`.
    pub fn over(&self, current: ScreenRect) -> ScreenRect {
        ScreenRect {
            x: self.x.unwrap_or(current.x),
            y: self.y.unwrap_or(current.y),
            width: self.width.unwrap_or(current.width),
            height: self.height.unwrap_or(current.height),
        }
    }
}

pub fn parse_bounds(params: &Params) -> StageResult<Bounds> {
    let mut bounds = Bounds::default();
    for key in ["x", "y", "width", "height"] {
        let Some(raw) = params.get(key).map(|v| v.trim()).filter(|v| !v.is_empty()) else {
            continue;
        };
        let value = raw
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && v.fract() == 0.0)
            .ok_or_else(|| StageError::bad(format!("“{key}” must be a whole number.")))?
            as i64;
        if (key == "width" || key == "height") && value <= 0 {
            return Err(StageError::bad(format!("“{key}” must be positive.")));
        }
        let slot = match key {
            "x" => &mut bounds.x,
            "y" => &mut bounds.y,
            "width" => &mut bounds.width,
            _ => &mut bounds.height,
        };
        *slot = Some(value);
    }
    Ok(bounds)
}

/// A key combination as a page dispatches it (`KeyboardEvent` names).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/stage/types.gen.ts")]
pub struct StageKeys {
    pub key: String,
    /// "KeyK", "Digit2", "Enter", "F2"; empty when unknown.
    pub code: String,
    pub meta: bool,
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

#[derive(Clone, Copy)]
enum Modifier {
    Meta,
    Shift,
    Alt,
    Ctrl,
}

impl StageKeys {
    fn set(&mut self, modifier: Modifier) {
        match modifier {
            Modifier::Meta => self.meta = true,
            Modifier::Shift => self.shift = true,
            Modifier::Alt => self.alt = true,
            Modifier::Ctrl => self.ctrl = true,
        }
    }
}

fn modifier_glyph(c: &str) -> Option<Modifier> {
    Some(match c {
        "⌘" => Modifier::Meta,
        "⇧" => Modifier::Shift,
        "⌥" => Modifier::Alt,
        "⌃" | "^" => Modifier::Ctrl,
        _ => return None,
    })
}

fn modifier_word(word: &str) -> Option<Modifier> {
    Some(match word {
        "cmd" | "command" | "meta" => Modifier::Meta,
        "shift" => Modifier::Shift,
        "alt" | "opt" | "option" => Modifier::Alt,
        "ctrl" | "control" => Modifier::Ctrl,
        _ => return None,
    })
}

fn named_key(name: &str) -> Option<(&'static str, &'static str)> {
    Some(match name {
        "enter" | "return" | "↵" | "↩" | "⏎" => ("Enter", "Enter"),
        "esc" | "escape" | "⎋" => ("Escape", "Escape"),
        "space" => (" ", "Space"),
        "tab" | "⇥" => ("Tab", "Tab"),
        "backspace" | "⌫" => ("Backspace", "Backspace"),
        "delete" | "⌦" => ("Delete", "Delete"),
        "up" | "arrowup" | "↑" => ("ArrowUp", "ArrowUp"),
        "down" | "arrowdown" | "↓" => ("ArrowDown", "ArrowDown"),
        "left" | "arrowleft" | "←" => ("ArrowLeft", "ArrowLeft"),
        "right" | "arrowright" | "→" => ("ArrowRight", "ArrowRight"),
        "home" => ("Home", "Home"),
        "end" => ("End", "End"),
        "pageup" => ("PageUp", "PageUp"),
        "pagedown" => ("PageDown", "PageDown"),
        _ => return None,
    })
}

/// The modifiers before the key and the key's text: leading glyphs need no separator (⌘⇧K),
/// words are joined with `+` (cmd+shift+k). None when a modifier is unknown or no key is left.
fn split_modifiers(text: &str) -> Option<(StageKeys, &str)> {
    let mut keys = StageKeys::default();
    let mut rest = text.trim();
    while rest.chars().count() > 1 {
        let first = rest.chars().next()?;
        let Some(modifier) = modifier_glyph(first.encode_utf8(&mut [0; 4])) else {
            break;
        };
        keys.set(modifier);
        rest = &rest[first.len_utf8()..];
    }
    let mut parts: Vec<&str> = rest
        .split('+')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    // A lone "+" is the plus key, not a separator.
    let last = match parts.pop() {
        Some(last) => last,
        None if rest.contains('+') => "+",
        None => return None,
    };
    for part in parts {
        keys.set(modifier_word(&part.to_lowercase()).or_else(|| modifier_glyph(part))?);
    }
    Some((keys, last))
}

fn function_key(lower: &str) -> Option<u8> {
    let digits = lower.strip_prefix('f')?;
    if digits.starts_with('0') {
        return None;
    }
    digits.parse().ok().filter(|n| (1..=20).contains(n))
}

/// The key's `KeyboardEvent` `key` and `code`; letters are upper case with Shift.
fn parse_last(last: &str, shift: bool) -> Option<(String, String)> {
    let lower = last.to_lowercase();
    if let Some((key, code)) = named_key(&lower) {
        return Some((key.into(), code.into()));
    }
    if let Some(n) = function_key(&lower) {
        return Some((format!("F{n}"), format!("F{n}")));
    }
    let mut chars = last.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    if c.is_ascii_alphabetic() {
        let key = if shift {
            c.to_ascii_uppercase()
        } else {
            c.to_ascii_lowercase()
        };
        return Some((key.to_string(), format!("Key{}", c.to_ascii_uppercase())));
    }
    if c.is_ascii_digit() {
        return Some((c.to_string(), format!("Digit{c}")));
    }
    Some((c.to_string(), String::new()))
}

/// "⌘⇧K", "cmd+shift+k", "Enter" or "⌃⌥⌘J" as a key combination; None when it isn't one.
pub fn parse_keys(text: &str) -> Option<StageKeys> {
    let (keys, last) = split_modifiers(text)?;
    let (key, code) = parse_last(last, keys.shift)?;
    Some(StageKeys { key, code, ..keys })
}

/// One spelling per combination, so "⌘F2" and "cmd+f2" compare equal.
pub fn canonical_keys(keys: &StageKeys) -> String {
    let name = if let Some(letter) = keys.code.strip_prefix("Key") {
        letter.to_lowercase()
    } else if let Some(digit) = keys.code.strip_prefix("Digit") {
        digit.to_string()
    } else if keys.key == " " {
        "space".into()
    } else {
        keys.key.to_lowercase()
    };
    [
        keys.ctrl.then_some("ctrl"),
        keys.alt.then_some("alt"),
        keys.shift.then_some("shift"),
        keys.meta.then_some("cmd"),
        Some(name.as_str()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("+")
}

pub fn stage_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/v1/stage")
}

/// The terminal command that registers this hook with Stage; Stage reads the app's name and id
/// from `describe`. Tokens are hex, so nothing needs quoting.
pub fn register_command(port: u16, token: &str) -> String {
    format!("stage register {} --token {token}", stage_url(port))
}

/// Optional protocol capabilities that DataBee implements, listed by `describe`.
pub const FEATURES: [&str; 8] = [
    "reveal",
    "visible",
    "sections",
    "ui",
    "type.append",
    "keys.window",
    "scroll",
    "wait",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(text: &str) -> StageKeys {
        parse_keys(text).unwrap_or_else(|| panic!("{text} should parse"))
    }

    fn params(pairs: &[(&str, &str)]) -> Params {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn moves_page_rectangles_to_the_content_origin_rounded() {
        let rect = PageRect {
            x: 16.4,
            y: 100.6,
            width: 208.2,
            height: 32.0,
        };
        let content = ScreenRect {
            x: 120,
            y: 88,
            width: 1240,
            height: 692,
        };
        assert_eq!(
            to_screen_rect(rect, content),
            ScreenRect {
                x: 136,
                y: 189,
                width: 208,
                height: 32
            }
        );
    }

    #[test]
    fn reads_only_the_bounds_given_as_whole_numbers() {
        assert_eq!(
            parse_bounds(&params(&[("x", " 100 "), ("width", "1200"), ("y", "")])).unwrap(),
            Bounds {
                x: Some(100),
                width: Some(1200),
                ..Bounds::default()
            }
        );
        assert_eq!(parse_bounds(&params(&[("x", "-40")])).unwrap().x, Some(-40));
        assert_eq!(
            parse_bounds(&params(&[("x", "1.5")])).unwrap_err().message,
            "“x” must be a whole number."
        );
        assert_eq!(
            parse_bounds(&params(&[("height", "0")])).unwrap_err(),
            StageError::bad("“height” must be positive.")
        );
        assert!(
            parse_bounds(&params(&[("action", "show")]))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn bounds_override_only_what_they_name() {
        let current = ScreenRect {
            x: 1,
            y: 2,
            width: 3,
            height: 4,
        };
        let bounds = Bounds {
            y: Some(20),
            height: Some(40),
            ..Bounds::default()
        };
        assert_eq!(
            bounds.over(current),
            ScreenRect {
                x: 1,
                y: 20,
                width: 3,
                height: 40
            }
        );
    }

    #[test]
    fn parses_glyphs_words_and_named_keys() {
        assert_eq!(
            keys("⌘⇧K"),
            StageKeys {
                key: "K".into(),
                code: "KeyK".into(),
                meta: true,
                shift: true,
                ..StageKeys::default()
            }
        );
        assert_eq!(keys("cmd+shift+k"), keys("⌘⇧K"));
        assert_eq!(keys("⌃⌥⌘J"), keys("ctrl+alt+cmd+j"));
        assert_eq!(keys("Return").key, "Enter");
        assert_eq!(keys("esc").code, "Escape");
        assert_eq!(keys("Space").key, " ");
        assert_eq!(keys("Up").key, "ArrowUp");
        assert_eq!(keys("⌘↵").code, "Enter");
        assert_eq!(keys("cmd+f2").code, "F2");
        assert_eq!(keys("F20").key, "F20");
        assert_eq!(keys("⌘2").code, "Digit2");
        assert_eq!(keys("cmd+=").key, "=");
        assert_eq!(keys("+").key, "+");
        assert!(!keys("k").shift);
        assert_eq!(keys("k").key, "k");
    }

    #[test]
    fn splits_known_modifiers_from_the_key_text() {
        let mods = |meta, shift, alt, ctrl| StageKeys {
            meta,
            shift,
            alt,
            ctrl,
            ..StageKeys::default()
        };
        for (text, expected, last) in [
            ("⌘⇧K", mods(true, true, false, false), "K"),
            ("cmd+shift+k", mods(true, true, false, false), "k"),
            ("Ctrl+Alt+J", mods(false, false, true, true), "J"),
            ("⌃⌥⌘J", mods(true, false, true, true), "J"),
            ("option+Enter", mods(false, false, true, false), "Enter"),
            ("Enter", mods(false, false, false, false), "Enter"),
            ("+", mods(false, false, false, false), "+"),
        ] {
            assert_eq!(split_modifiers(text), Some((expected, last)), "{text}");
        }
    }

    #[test]
    fn unknown_modifiers_or_no_key_split_to_nothing() {
        for text in ["", "  ", "hyper+k", "cmd+hyper+k"] {
            assert_eq!(split_modifiers(text), None, "{text}");
        }
    }

    #[test]
    fn last_key_names_letters_digits_named_and_function_keys() {
        for (last, shift, expected) in [
            ("k", false, Some(("k", "KeyK"))),
            ("K", false, Some(("k", "KeyK"))),
            ("k", true, Some(("K", "KeyK"))),
            ("2", false, Some(("2", "Digit2"))),
            ("2", true, Some(("2", "Digit2"))),
            ("Return", false, Some(("Enter", "Enter"))),
            ("space", false, Some((" ", "Space"))),
            ("Left", true, Some(("ArrowLeft", "ArrowLeft"))),
            ("f2", false, Some(("F2", "F2"))),
            ("F12", true, Some(("F12", "F12"))),
            ("F20", false, Some(("F20", "F20"))),
            ("=", false, Some(("=", ""))),
            ("F21", false, None),
            ("F0", false, None),
            ("ab", false, None),
            ("", false, None),
        ] {
            assert_eq!(
                parse_last(last, shift),
                expected.map(|(k, c)| (k.to_string(), c.to_string())),
                "{last} shift={shift}"
            );
        }
    }

    #[test]
    fn rejects_what_isnt_a_combination() {
        for text in ["", "  ", "hyper+k", "F21", "F0", "cmd+ab", "cmd+"] {
            assert_eq!(parse_keys(text), None, "{text}");
        }
    }

    #[test]
    fn spells_equal_combinations_the_same() {
        assert_eq!(canonical_keys(&keys("⌘F2")), "cmd+f2");
        assert_eq!(canonical_keys(&keys("⌃⌥⌘J")), "ctrl+alt+cmd+j");
        assert_eq!(canonical_keys(&keys("shift+space")), "shift+space");
        assert_eq!(
            canonical_keys(&keys("⌘⇧K")),
            canonical_keys(&keys("cmd+shift+k"))
        );
    }

    #[test]
    fn register_command_names_the_hook_url_and_token() {
        assert_eq!(
            register_command(27184, "abc123"),
            "stage register http://127.0.0.1:27184/v1/stage --token abc123"
        );
    }
}
