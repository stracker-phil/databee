//! The messages between the hub and the page (src/stage/hooks.ts): what Rust asks it to do
//! (`ActEvent`) and its replies to `stage:collect`, `stage:act` and `stage:info`.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::protocol::{PageRect, StageKeys};
use super::windows::Window;

fn enabled() -> bool {
    true
}

/// Anchors in a window the hub doesn't know count as the main window's.
fn window_or_main<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Window, D::Error> {
    let id = Option::<String>::deserialize(deserializer)?;
    Ok(id.and_then(|id| id.parse().ok()).unwrap_or_default())
}

/// How `type` puts its text into the field.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/stage/types.gen.ts")]
#[ts(rename = "StageTypeMode")]
#[serde(rename_all = "lowercase")]
pub enum TypeMode {
    #[default]
    Replace,
    Append,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/stage/types.gen.ts")]
#[ts(rename = "StageScrollTo")]
#[serde(rename_all = "lowercase")]
pub enum ScrollTo {
    Top,
    Bottom,
}

/// What the page does with a sheet (settings, palette).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/stage/types.gen.ts")]
#[ts(rename = "StageSheetAction")]
#[serde(rename_all = "lowercase")]
pub enum SheetAction {
    Show,
    Hide,
}

/// One act for the page, already validated by the hub.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[ts(export, export_to = "src/stage/types.gen.ts")]
#[ts(rename = "StageAct")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ActRequest {
    Click {
        anchor: String,
    },
    Type {
        anchor: String,
        text: String,
        mode: TypeMode,
    },
    Reveal {
        anchor: String,
    },
    /// Either `by` (points, positive scrolls down) or `to`.
    Scroll {
        anchor: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        by: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        to: Option<ScrollTo>,
    },
    Keys {
        keys: StageKeys,
        /// The hub focused the window for these keys.
        focused: bool,
    },
    Command {
        command: String,
    },
    /// Opens or closes a sheet.
    Window {
        window: Window,
        action: SheetAction,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        section: Option<String>,
    },
}

/// The `stage:act` event: an act and the id its reply must carry.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[ts(export, export_to = "src/stage/types.gen.ts")]
#[ts(rename = "StageActRequest")]
pub struct ActEvent {
    pub request: String,
    #[serde(flatten)]
    pub act: ActRequest,
}

/// An element marked `data-stage="<id>"`, as the page measures it.
#[derive(Deserialize, Clone, Debug, PartialEq, TS)]
#[ts(export, export_to = "src/stage/types.gen.ts")]
#[ts(optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct PageAnchor {
    pub id: String,
    /// Already cut to a sane length by the page.
    pub label: String,
    pub rect: PageRect,
    pub in_view: bool,
    /// The part that shows through its scroll areas and the window; None when nothing does.
    #[serde(default)]
    #[ts(optional = false)]
    pub visible: Option<PageRect>,
    #[serde(default, deserialize_with = "window_or_main")]
    pub window: Window,
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
#[derive(Deserialize, Clone, Debug, TS)]
#[ts(export, export_to = "src/stage/types.gen.ts")]
pub struct AnchorsReply {
    pub request: String,
    pub page: String,
    pub anchors: Vec<PageAnchor>,
}

impl AnchorsReply {
    /// Anchors without an id or a size dropped.
    pub fn cleaned(mut self) -> Self {
        self.anchors
            .retain(|a| !a.id.trim().is_empty() && a.rect.width > 0.0 && a.rect.height > 0.0);
        self
    }
}

/// The page's answer to `stage:act`; `handled` is false when the anchor isn't on that page.
#[derive(Deserialize, Clone, Debug, PartialEq, Default, TS)]
#[ts(export, export_to = "src/stage/types.gen.ts")]
#[ts(rename = "StageActReply", optional_fields = nullable)]
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
    pub window: Option<Window>,
    /// The anchor's rectangle once in view, before acting.
    #[serde(default)]
    pub rect: Option<PageRect>,
    #[serde(default)]
    pub visible: Option<PageRect>,
    /// `keys` that ran a command (its shortcut) instead of reaching an element.
    #[serde(default)]
    pub command: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[ts(export, export_to = "src/stage/types.gen.ts")]
#[ts(rename = "StageChoice")]
pub struct Choice {
    pub id: String,
    pub title: String,
}

/// One command: the registry's, or a family (`arg` + `choices`) whose members append an argument.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[ts(export, export_to = "src/stage/types.gen.ts")]
#[ts(rename = "StageCommand")]
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
#[derive(Deserialize, Clone, Debug, PartialEq, TS)]
#[ts(export, export_to = "src/stage/types.gen.ts")]
#[ts(rename = "StageSurface", optional_fields = nullable)]
pub struct Surface {
    pub id: Window,
    pub title: String,
    #[serde(default)]
    pub rect: Option<PageRect>,
    #[serde(default)]
    pub sections: Option<Vec<Choice>>,
    #[serde(default)]
    pub section: Option<String>,
}

/// The page's answer to `stage:info`: what it shows and can do.
#[derive(Deserialize, Clone, Debug, Default, TS)]
#[ts(export, export_to = "src/stage/types.gen.ts")]
#[ts(rename = "StageInfoReply")]
pub struct InfoReply {
    pub request: String,
    pub page: String,
    #[serde(default)]
    #[ts(type = "Record<string, string | number | boolean | null>")]
    pub state: serde_json::Map<String, Value>,
    /// What the main window shows.
    #[serde(default)]
    pub view: Option<String>,
    /// The main window's toggleable panels: `{ "inspector": "open", "sidebar": "closed" }`.
    #[serde(default)]
    #[ts(type = "Record<string, \"open\" | \"closed\">")]
    pub ui: serde_json::Map<String, Value>,
    #[serde(default)]
    pub surfaces: Vec<Surface>,
    #[serde(default)]
    pub commands: Vec<PageCommand>,
}

impl InfoReply {
    pub fn surface(&self, window: Window) -> Option<&Surface> {
        self.surfaces.iter().find(|s| s.id == window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn anchor_json(window: Option<Value>) -> Value {
        let mut anchor = json!({
            "id": "a", "label": "A",
            "rect": { "x": 0, "y": 0, "width": 10, "height": 10 },
            "inView": true
        });
        if let Some(window) = window {
            anchor["window"] = window;
        }
        anchor
    }

    #[test]
    fn act_requests_are_tagged_by_kind_with_camel_case_fields() {
        let keys = StageKeys {
            key: "k".into(),
            code: "KeyK".into(),
            meta: true,
            ..StageKeys::default()
        };
        let cases = [
            (
                ActRequest::Click { anchor: "a".into() },
                json!({ "kind": "click", "anchor": "a" }),
            ),
            (
                ActRequest::Type {
                    anchor: "a".into(),
                    text: "hi".into(),
                    mode: TypeMode::Replace,
                },
                json!({ "kind": "type", "anchor": "a", "text": "hi", "mode": "replace" }),
            ),
            (
                ActRequest::Type {
                    anchor: "a".into(),
                    text: "hi".into(),
                    mode: TypeMode::Append,
                },
                json!({ "kind": "type", "anchor": "a", "text": "hi", "mode": "append" }),
            ),
            (
                ActRequest::Reveal { anchor: "a".into() },
                json!({ "kind": "reveal", "anchor": "a" }),
            ),
            (
                ActRequest::Scroll {
                    anchor: "a".into(),
                    by: Some(120.0),
                    to: None,
                },
                json!({ "kind": "scroll", "anchor": "a", "by": 120.0 }),
            ),
            (
                ActRequest::Scroll {
                    anchor: "a".into(),
                    by: None,
                    to: Some(ScrollTo::Bottom),
                },
                json!({ "kind": "scroll", "anchor": "a", "to": "bottom" }),
            ),
            (
                ActRequest::Keys {
                    keys,
                    focused: true,
                },
                json!({
                    "kind": "keys",
                    "keys": {
                        "key": "k", "code": "KeyK",
                        "meta": true, "shift": false, "alt": false, "ctrl": false
                    },
                    "focused": true
                }),
            ),
            (
                ActRequest::Command {
                    command: "app:settings".into(),
                },
                json!({ "kind": "command", "command": "app:settings" }),
            ),
            (
                ActRequest::Window {
                    window: Window::Settings,
                    action: SheetAction::Show,
                    section: Some("queries".into()),
                },
                json!({
                    "kind": "window", "window": "settings",
                    "action": "show", "section": "queries"
                }),
            ),
            (
                ActRequest::Window {
                    window: Window::Palette,
                    action: SheetAction::Hide,
                    section: None,
                },
                json!({ "kind": "window", "window": "palette", "action": "hide" }),
            ),
        ];
        for (request, expected) in cases {
            assert_eq!(
                serde_json::to_value(&request).unwrap(),
                expected,
                "{request:?}"
            );
            assert_eq!(
                serde_json::from_value::<ActRequest>(expected.clone()).unwrap(),
                request,
                "{expected}"
            );
        }
    }

    #[test]
    fn act_events_flatten_the_act_next_to_the_request_id() {
        let event = ActEvent {
            request: "r1".into(),
            act: ActRequest::Click { anchor: "a".into() },
        };
        let expected = json!({ "request": "r1", "kind": "click", "anchor": "a" });
        assert_eq!(serde_json::to_value(&event).unwrap(), expected);
        assert_eq!(serde_json::from_value::<ActEvent>(expected).unwrap(), event);
    }

    #[test]
    fn anchors_in_an_unknown_or_missing_window_belong_to_main() {
        for window in [
            None,
            Some(json!("sidebar")),
            Some(json!(null)),
            Some(json!("")),
        ] {
            let anchor: PageAnchor = serde_json::from_value(anchor_json(window.clone())).unwrap();
            assert_eq!(anchor.window, Window::Main, "{window:?}");
        }
    }

    #[test]
    fn anchors_keep_the_window_they_name() {
        for window in Window::ALL {
            let anchor: PageAnchor =
                serde_json::from_value(anchor_json(Some(json!(window.as_str())))).unwrap();
            assert_eq!(anchor.window, window);
        }
    }

    #[test]
    fn act_replies_from_an_unknown_window_fail_to_parse() {
        let reply = |window: &str| {
            serde_json::from_value::<ActReply>(json!({
                "request": "r", "page": "main", "handled": true, "ok": true, "window": window
            }))
        };
        assert_eq!(reply("settings").unwrap().window, Some(Window::Settings));
        assert!(reply("sidebar").is_err());
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
