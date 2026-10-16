//! The Stage hook protocol's pure parts (docs/stage.md §2): rectangles in screen
//! points, request bounds, key combinations, the discovery record and the pages' replies.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL: u32 = 1;
/// The protocol's name for databee's only window.
pub const MAIN: &str = "main";
const MAX_LABEL: usize = 200;

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
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
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
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
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

/// "⌘⇧K", "cmd+shift+k", "Enter" or "⌃⌥⌘J" as a key combination; None when it isn't one.
pub fn parse_keys(text: &str) -> Option<StageKeys> {
    let mut keys = StageKeys::default();
    let mut rest = text.trim();
    // Leading glyphs need no separator: ⌘⇧K.
    while rest.chars().count() > 1 {
        let first = rest.chars().next().unwrap();
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
    if parts.is_empty() {
        if !rest.contains('+') {
            return None;
        }
        parts.push("+");
    }
    let last = parts.pop().unwrap_or_default();
    for part in parts {
        keys.set(modifier_word(&part.to_lowercase()).or_else(|| modifier_glyph(part))?);
    }
    let lower = last.to_lowercase();
    if let Some((key, code)) = named_key(&lower) {
        return Some(StageKeys {
            key: key.into(),
            code: code.into(),
            ..keys
        });
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
        && (1..=20).contains(&n)
        && !lower[1..].starts_with('0')
    {
        return Some(StageKeys {
            key: format!("F{n}"),
            code: format!("F{n}"),
            ..keys
        });
    }
    let mut chars = last.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    let (key, code) = if c.is_ascii_alphabetic() {
        let lower = c.to_ascii_lowercase();
        let key = if keys.shift {
            c.to_ascii_uppercase()
        } else {
            lower
        };
        (key.to_string(), format!("Key{}", c.to_ascii_uppercase()))
    } else if c.is_ascii_digit() {
        (c.to_string(), format!("Digit{c}"))
    } else {
        (c.to_string(), String::new())
    };
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

/// `~/Library/Application Support/stage/apps/<id>.json`, present while the API runs.
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Discovery {
    pub protocol: u32,
    pub name: String,
    pub id: String,
    pub url: String,
    pub token: String,
    pub pid: u32,
    pub updated_at: String,
}

pub fn discovery_record(
    name: &str,
    id: &str,
    port: u16,
    token: &str,
    pid: u32,
    now: chrono::DateTime<chrono::Utc>,
) -> Discovery {
    Discovery {
        protocol: PROTOCOL,
        name: name.into(),
        id: id.into(),
        url: stage_url(port),
        token: token.into(),
        pid,
        updated_at: now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    }
}

// The pages' replies -------------------------------------------------------------------------

fn short(text: String) -> String {
    if text.chars().count() <= MAX_LABEL {
        text
    } else {
        text.chars().take(MAX_LABEL).collect()
    }
}

/// The windows databee exposes, in `describe` order. Settings and the palette are sheets inside
/// the main window; the page measures them.
pub const WINDOWS: [&str; 3] = [MAIN, "settings", "palette"];

/// Optional protocol capabilities (Stage protocol 1, revision 2026-10-16, §5.1).
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

fn main_window() -> String {
    MAIN.into()
}

fn enabled() -> bool {
    true
}

/// An element marked `data-stage="<id>"`, as the page measures it.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PageAnchor {
    pub id: String,
    pub label: String,
    pub rect: PageRect,
    pub in_view: bool,
    /// The part that shows through its scroll areas and the window; None when nothing does.
    #[serde(default)]
    pub visible: Option<PageRect>,
    #[serde(default = "main_window")]
    pub window: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub list: Option<String>,
    #[serde(default)]
    pub selected: Option<bool>,
    #[serde(default)]
    pub checked: Option<bool>,
    #[serde(default)]
    pub expanded: Option<bool>,
    #[serde(default)]
    pub disabled: Option<bool>,
}

/// The page's answer to `stage:collect`.
#[derive(Deserialize, Clone, Debug)]
pub struct AnchorsReply {
    pub request: String,
    pub page: String,
    pub anchors: Vec<PageAnchor>,
}

impl AnchorsReply {
    /// Ids and labels cut to a sane length; anchors without an id or a size dropped; unknown
    /// windows count as the main one.
    pub fn cleaned(mut self) -> Self {
        self.anchors = self
            .anchors
            .into_iter()
            .filter(|a| !a.id.trim().is_empty() && a.rect.width > 0.0 && a.rect.height > 0.0)
            .map(|a| PageAnchor {
                id: short(a.id),
                label: short(a.label),
                window: if WINDOWS.contains(&a.window.as_str()) {
                    a.window
                } else {
                    MAIN.into()
                },
                ..a
            })
            .collect();
        self
    }
}

/// The page's answer to `stage:act`; `handled` is false when the anchor isn't on that page.
#[derive(Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ActReply {
    pub request: String,
    pub page: String,
    pub handled: bool,
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    /// The HTTP status for a failure when it isn't 500 (e.g. 409: not scrollable).
    #[serde(default)]
    pub status: Option<u16>,
    /// The window the anchor is in, or the one that got the keys.
    #[serde(default)]
    pub window: Option<String>,
    /// The anchor's rectangle once in view, before acting.
    #[serde(default)]
    pub rect: Option<PageRect>,
    #[serde(default)]
    pub visible: Option<PageRect>,
    /// `keys` that ran a command (its shortcut) instead of reaching an element.
    #[serde(default)]
    pub command: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Choice {
    pub id: String,
    pub title: String,
}

/// One command: the registry's, or a family (`arg` + `choices`) whose members append an argument.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PageCommand {
    pub id: String,
    pub help: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<Choice>>,
}

impl PageCommand {
    /// The command id is this command, or a member of this family.
    pub fn runs(&self, id: &str) -> bool {
        if self.arg.is_some() {
            id.len() > self.id.len() && id.starts_with(&self.id)
        } else {
            id == self.id
        }
    }
}

/// A window the page draws inside the main one (settings, palette); `rect` None while closed.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Surface {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub rect: Option<PageRect>,
    #[serde(default)]
    pub sections: Option<Vec<Choice>>,
    #[serde(default)]
    pub section: Option<String>,
}

/// The page's answer to `stage:info`: what it shows and can do.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct InfoReply {
    pub request: String,
    pub page: String,
    #[serde(default)]
    pub state: serde_json::Map<String, Value>,
    /// What the main window shows.
    #[serde(default)]
    pub view: Option<String>,
    /// The main window's toggleable panels: `{ "inspector": "open" }`.
    #[serde(default)]
    pub ui: serde_json::Map<String, Value>,
    #[serde(default)]
    pub surfaces: Vec<Surface>,
    #[serde(default)]
    pub commands: Vec<PageCommand>,
}

impl InfoReply {
    pub fn surface(&self, id: &str) -> Option<&Surface> {
        self.surfaces.iter().find(|s| s.id == id)
    }
}

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
    fn discovery_record_points_at_the_stage_url() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-15T12:00:00Z")
            .unwrap()
            .to_utc();
        let record = discovery_record("databee", "com.example", 27184, "t", 42, now);
        assert_eq!(
            serde_json::to_value(&record).unwrap(),
            serde_json::json!({
                "protocol": 1,
                "name": "databee",
                "id": "com.example",
                "url": "http://127.0.0.1:27184/v1/stage",
                "token": "t",
                "pid": 42,
                "updatedAt": "2026-10-15T12:00:00.000Z"
            })
        );
    }

    #[test]
    fn anchor_replies_drop_empty_ids_and_sizes() {
        let reply: AnchorsReply = serde_json::from_value(serde_json::json!({
            "request": "r", "page": "main",
            "anchors": [
                { "id": "toolbar", "label": "Toolbar", "rect": { "x": 0, "y": 0, "width": 10, "height": 10 }, "inView": true },
                { "id": " ", "label": "", "rect": { "x": 0, "y": 0, "width": 10, "height": 10 }, "inView": true },
                { "id": "gone", "label": "", "rect": { "x": 0, "y": 0, "width": 0, "height": 10 }, "inView": false }
            ]
        }))
        .unwrap();
        let ids: Vec<_> = reply.cleaned().anchors.into_iter().map(|a| a.id).collect();
        assert_eq!(ids, ["toolbar"]);
    }
}
