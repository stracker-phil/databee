//! The Stage hook's verbs (Stage protocol 1, revision 2026-10-16, §5): what a presenter may read
//! and make databee do. The page answers anchor, act and info requests (src/stage/hooks.ts);
//! `http.rs` routes `/v1/stage/<verb>` here.
//!
//! databee has one native window. Settings and the command palette are sheets inside it; the hook
//! exposes them as the windows `settings` and `palette`, measured by the page.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{Map, Value, json};
use tokio::sync::oneshot;

use super::protocol::{
    ActReply, AnchorsReply, Bounds, FEATURES, InfoReply, MAIN, PROTOCOL, PageAnchor, PageRect,
    Params, ScreenRect, StageError, StageKeys, StageResult, WINDOWS, parse_bounds, parse_keys,
    required, to_screen_rect,
};

pub const COLLECT_EVENT: &str = "stage:collect";
pub const ACT_EVENT: &str = "stage:act";
pub const INFO_EVENT: &str = "stage:info";

const WAIT_DEFAULT_MS: u64 = 2000;
const WAIT_MAX_MS: u64 = 5000;
const WAIT_POLL: Duration = Duration::from_millis(100);

/// The main window as the host sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub title: String,
    pub visible: bool,
    pub focused: bool,
    pub bounds: ScreenRect,
    /// The page's area; anchors are measured from its top-left.
    pub content: ScreenRect,
}

/// What the hub needs from the app: the window and a way to reach the page.
pub trait StageHost: Send + Sync {
    fn frame(&self) -> Option<Frame>;
    fn show(&self);
    fn hide(&self);
    fn focus(&self);
    fn place(&self, rect: ScreenRect);
    fn emit(&self, event: &str, payload: Value);
}

#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    /// The page answers at once; one that doesn't (still loading) is left out.
    pub collect: Duration,
    /// The page may first scroll the anchor into view, smoothly.
    pub act: Duration,
    /// A window needs a moment to appear or move before it is measured.
    pub settle: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            collect: Duration::from_millis(400),
            act: Duration::from_millis(2500),
            settle: Duration::from_millis(150),
        }
    }
}

enum Waiter {
    Anchors(oneshot::Sender<AnchorsReply>),
    Act(oneshot::Sender<ActReply>),
    Info(oneshot::Sender<InfoReply>),
}

pub struct Hub {
    host: Box<dyn StageHost>,
    timeouts: Timeouts,
    app: Value,
    waiting: Mutex<HashMap<String, Waiter>>,
    last_request: Mutex<Option<chrono::DateTime<chrono::Utc>>>,
}

fn rect_json(rect: ScreenRect) -> Value {
    serde_json::to_value(rect).unwrap_or(Value::Null)
}

fn screen(rect: Option<PageRect>, frame: Option<&Frame>) -> Value {
    match (rect, frame) {
        (Some(rect), Some(frame)) => rect_json(to_screen_rect(rect, frame.content)),
        _ => Value::Null,
    }
}

fn window_order(id: &str) -> usize {
    WINDOWS.iter().position(|w| *w == id).unwrap_or(0)
}

fn anchor_json(a: &PageAnchor, frame: &Frame) -> Value {
    let mut out = json!({
        "id": a.id,
        "label": a.label,
        "window": a.window,
        "rect": rect_json(to_screen_rect(a.rect, frame.content)),
        "inView": a.in_view,
        "visible": screen(a.visible, Some(frame)),
    });
    let extra = [
        ("role", a.role.clone().map(Value::from)),
        ("list", a.list.clone().map(Value::from)),
        ("selected", a.selected.map(Value::from)),
        ("checked", a.checked.map(Value::from)),
        ("expanded", a.expanded.map(Value::from)),
        ("disabled", a.disabled.filter(|d| *d).map(Value::from)),
    ];
    for (key, value) in extra {
        if let Some(value) = value {
            out[key] = value;
        }
    }
    out
}

impl Hub {
    pub fn new(host: Box<dyn StageHost>, timeouts: Timeouts, name: &str, id: &str) -> Self {
        Self {
            host,
            timeouts,
            app: json!({ "name": name, "id": id, "version": env!("CARGO_PKG_VERSION") }),
            waiting: Mutex::default(),
            last_request: Mutex::default(),
        }
    }

    /// When a presenter last used the hook (shown in Settings).
    pub fn last_request(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        *self.last_request.lock().unwrap()
    }

    // Page replies -------------------------------------------------------------------------

    fn take(&self, request: &str, page: &str) -> Option<Waiter> {
        if page != MAIN {
            return None;
        }
        self.waiting.lock().unwrap().remove(request)
    }

    pub fn receive_anchors(&self, reply: AnchorsReply) {
        if let Some(Waiter::Anchors(tx)) = self.take(&reply.request, &reply.page) {
            let _ = tx.send(reply.cleaned());
        }
    }

    pub fn receive_act(&self, reply: ActReply) {
        if let Some(Waiter::Act(tx)) = self.take(&reply.request, &reply.page) {
            let _ = tx.send(reply);
        }
    }

    pub fn receive_info(&self, reply: InfoReply) {
        if let Some(Waiter::Info(tx)) = self.take(&reply.request, &reply.page) {
            let _ = tx.send(reply);
        }
    }

    /// Sends `event` to the page and waits for its answer; None when it didn't answer in time.
    async fn ask<T>(
        &self,
        event: &str,
        mut payload: Map<String, Value>,
        wait: Duration,
        waiter: fn(oneshot::Sender<T>) -> Waiter,
    ) -> Option<T> {
        let request = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.waiting
            .lock()
            .unwrap()
            .insert(request.clone(), waiter(tx));
        payload.insert("request".into(), Value::String(request.clone()));
        self.host.emit(event, Value::Object(payload));
        let reply = tokio::time::timeout(wait, rx)
            .await
            .ok()
            .and_then(Result::ok);
        self.waiting.lock().unwrap().remove(&request);
        reply
    }

    async fn info(&self) -> Option<InfoReply> {
        self.ask(INFO_EVENT, Map::new(), self.timeouts.collect, Waiter::Info)
            .await
    }

    /// The page's anchors while the window is on screen, in window, then reading order.
    async fn page_anchors(&self, frame: Option<&Frame>) -> Vec<PageAnchor> {
        if !frame.is_some_and(|f| f.visible) {
            return Vec::new();
        }
        let Some(reply) = self
            .ask(
                COLLECT_EVENT,
                Map::new(),
                self.timeouts.collect,
                Waiter::Anchors,
            )
            .await
        else {
            return Vec::new();
        };
        let mut anchors = reply.anchors;
        anchors.sort_by(|a, b| {
            (window_order(&a.window), a.rect.y, a.rect.x)
                .partial_cmp(&(window_order(&b.window), b.rect.y, b.rect.x))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        anchors
    }

    async fn act_on_page(&self, payload: Value) -> Option<ActReply> {
        let Value::Object(payload) = payload else {
            return None;
        };
        self.ask(ACT_EVENT, payload, self.timeouts.act, Waiter::Act)
            .await
    }

    // Windows ------------------------------------------------------------------------------

    /// On screen: the main window, or a sheet the page reports open in the shown main window.
    fn is_shown(id: &str, frame: Option<&Frame>, info: Option<&InfoReply>) -> bool {
        let main = frame.is_some_and(|f| f.visible);
        if id == MAIN {
            return main;
        }
        main && info
            .and_then(|i| i.surface(id))
            .is_some_and(|s| s.rect.is_some())
    }

    fn windows_json(frame: Option<&Frame>, info: Option<&InfoReply>) -> Vec<Value> {
        let shown_main = frame.filter(|f| f.visible);
        let palette_open = Self::is_shown("palette", frame, info);
        let mut main = json!({
            "id": MAIN,
            "title": frame.map_or("databee", |f| f.title.as_str()),
            "visible": shown_main.is_some(),
            // A sheet on top takes the keys.
            "focused": shown_main.is_some_and(|f| f.focused)
                && !palette_open
                && !Self::is_shown("settings", frame, info),
            "movable": true,
            "bounds": shown_main.map(|f| rect_json(f.bounds)),
            "content": shown_main.map(|f| rect_json(f.content)),
            "view": shown_main.and(info.and_then(|i| i.view.clone())),
        });
        if let Some(info) = info {
            main["ui"] = Value::Object(info.ui.clone());
        }
        let sheet = |id: &str, fallback: &str| {
            let surface = info.and_then(|i| i.surface(id));
            let shown = Self::is_shown(id, frame, info);
            let rect = if shown {
                screen(surface.and_then(|s| s.rect), frame)
            } else {
                Value::Null
            };
            let mut out = json!({
                "id": id,
                "title": surface.map_or(fallback, |s| s.title.as_str()),
                "visible": shown,
                "focused": shown
                    && frame.is_some_and(|f| f.focused)
                    && (id == "palette" || !palette_open),
                // Sheets centre themselves in the main window.
                "movable": false,
                "bounds": rect.clone(),
                "content": rect,
                "view": Value::Null,
            });
            if let Some(sections) = surface.and_then(|s| s.sections.clone()) {
                out["sections"] = json!(sections);
                out["section"] = json!(surface.and_then(|s| s.section.clone()).filter(|_| shown));
            }
            out
        };
        vec![
            main,
            sheet("settings", "Settings"),
            sheet("palette", "Command Palette"),
        ]
    }

    fn window_named(windows: Vec<Value>, id: &str) -> Value {
        windows
            .into_iter()
            .find(|w| w["id"] == id)
            .unwrap_or(Value::Null)
    }

    async fn describe(&self) -> Map<String, Value> {
        let frame = self.host.frame();
        let (info, anchors) = tokio::join!(self.info(), self.page_anchors(frame.as_ref()));
        let anchors: Vec<Value> = match frame.as_ref() {
            Some(frame) => anchors.iter().map(|a| anchor_json(a, frame)).collect(),
            None => Vec::new(),
        };
        let mut out = Map::new();
        out.insert("protocol".into(), json!(PROTOCOL));
        out.insert("features".into(), json!(FEATURES));
        out.insert("app".into(), self.app.clone());
        out.insert(
            "windows".into(),
            json!(Self::windows_json(frame.as_ref(), info.as_ref())),
        );
        out.insert("anchors".into(), Value::Array(anchors));
        let (commands, state) = match info {
            Some(i) => (i.commands, i.state),
            None => (Vec::new(), Map::new()),
        };
        out.insert("commands".into(), json!(commands));
        // databee has no global shortcuts; in-app ones work through `keys`.
        out.insert("hotkeys".into(), json!([]));
        out.insert("state".into(), Value::Object(state));
        out
    }

    fn not_open(id: &str) -> StageError {
        StageError::new(409, format!("The {id} window isn't open; show it first."))
    }

    fn page_failed(reply: &ActReply) -> StageError {
        StageError::new(
            reply.status.unwrap_or(500),
            reply
                .error
                .clone()
                .filter(|e| !e.is_empty())
                .unwrap_or_else(|| "The page couldn't do that.".into()),
        )
    }

    /// Opens or closes a sheet through the page.
    async fn sheet_action(&self, id: &str, action: &str, section: Option<&str>) -> StageResult<()> {
        let mut payload = json!({ "kind": "window", "window": id, "action": action });
        if let Some(section) = section {
            payload["section"] = json!(section);
        }
        let reply = self
            .act_on_page(payload)
            .await
            .ok_or_else(|| StageError::new(500, "The page didn't answer."))?;
        if reply.ok {
            Ok(())
        } else {
            Err(Self::page_failed(&reply))
        }
    }

    async fn window(&self, params: &Params) -> StageResult<Map<String, Value>> {
        let id = params.get("window").map(|w| w.trim()).unwrap_or_default();
        if !WINDOWS.contains(&id) {
            return Err(StageError::new(
                404,
                format!("“window” must be one of {}.", WINDOWS.join(", ")),
            ));
        }
        let bounds: Bounds = parse_bounds(params)?;
        let action = match params.get("action").map(|a| a.trim()) {
            Some(a) if !a.is_empty() => a,
            _ if !bounds.is_empty() => "place",
            _ => "show",
        };
        let section = params
            .get("section")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        let frame = self.host.frame();
        match (id, action) {
            (MAIN, "show") => self.host.show(),
            (MAIN, "hide") => self.host.hide(),
            (_, "focus") => {
                let info = if id == MAIN { None } else { self.info().await };
                if !Self::is_shown(id, frame.as_ref(), info.as_ref()) {
                    return Err(Self::not_open(id));
                }
                self.host.focus();
            }
            (MAIN, "place") => {
                let frame = frame
                    .filter(|f| f.visible)
                    .ok_or_else(|| Self::not_open(MAIN))?;
                if bounds.is_empty() {
                    return Err(StageError::bad(
                        "Give x, y, width or height to place the window.",
                    ));
                }
                self.host.place(bounds.over(frame.bounds));
            }
            (_, "place") => {
                return Err(StageError::new(
                    409,
                    format!("The {id} window places itself (a sheet centred in databee's window)."),
                ));
            }
            (_, "show") => {
                if let Some(section) = section.filter(|_| id == "settings") {
                    let info = self.info().await;
                    let known = info
                        .as_ref()
                        .and_then(|i| i.surface(id))
                        .and_then(|s| s.sections.as_ref())
                        .is_some_and(|all| all.iter().any(|s| s.id == section));
                    if !known {
                        return Err(StageError::new(
                            404,
                            format!("Unknown section “{section}”; windows[].sections lists them."),
                        ));
                    }
                }
                if !frame.as_ref().is_some_and(|f| f.visible) {
                    self.host.show();
                }
                self.sheet_action(id, "show", section).await?;
            }
            (_, "hide") => self.sheet_action(id, "hide", None).await?,
            _ => {
                return Err(StageError::bad(
                    "“action” must be show, hide, focus or place.",
                ));
            }
        }
        tokio::time::sleep(self.timeouts.settle).await;
        let info = self.info().await;
        let mut out = Map::new();
        out.insert(
            "window".into(),
            Self::window_named(
                Self::windows_json(self.host.frame().as_ref(), info.as_ref()),
                id,
            ),
        );
        Ok(out)
    }

    // Acts ---------------------------------------------------------------------------------

    fn anchor_result(&self, anchor: &str, reply: &ActReply) -> Map<String, Value> {
        let frame = self.host.frame();
        let mut out = Map::new();
        out.insert("anchor".into(), json!(anchor));
        out.insert(
            "window".into(),
            json!(reply.window.as_deref().unwrap_or(MAIN)),
        );
        out.insert("rect".into(), screen(reply.rect, frame.as_ref()));
        if reply.visible.is_some() {
            out.insert("visible".into(), screen(reply.visible, frame.as_ref()));
        }
        out
    }

    async fn command(&self, params: &Params) -> StageResult<Map<String, Value>> {
        let command = required(params, "command")?;
        let commands = self.info().await.map(|i| i.commands).unwrap_or_default();
        let Some(found) = commands.iter().find(|c| c.runs(command)) else {
            return Err(StageError::new(
                404,
                format!("Unknown command “{command}”; describe lists them."),
            ));
        };
        if !found.enabled {
            return Err(StageError::new(
                409,
                format!("“{command}” can't run in the current view."),
            ));
        }
        let reply = self
            .act_on_page(json!({ "kind": "command", "command": command }))
            .await
            .ok_or_else(|| StageError::new(500, "The page didn't answer."))?;
        if !reply.ok {
            return Err(Self::page_failed(&reply));
        }
        let mut out = Map::new();
        out.insert("command".into(), json!(command));
        Ok(out)
    }

    async fn on_anchor(&self, kind: &str, params: &Params) -> StageResult<Map<String, Value>> {
        let anchor = required(params, "anchor")?;
        let not_shown = || {
            StageError::new(
                404,
                format!(
                    "No window on screen shows “{anchor}”; describe lists the anchors shown now."
                ),
            )
        };
        if !self.host.frame().is_some_and(|f| f.visible) {
            return Err(not_shown());
        }
        let mut payload = json!({ "kind": kind, "anchor": anchor });
        if kind == "type" {
            payload["text"] = json!(params.get("text").cloned().unwrap_or_default());
            payload["mode"] = json!(match params.get("mode").map(|m| m.trim()) {
                None | Some("") | Some("replace") => "replace",
                Some("append") => "append",
                Some(_) => return Err(StageError::bad("“mode” must be replace or append.")),
            });
        }
        if kind == "scroll" {
            match (params.get("by"), params.get("to").map(|t| t.trim())) {
                (_, Some(to @ ("top" | "bottom"))) => payload["to"] = json!(to),
                (_, Some(_)) => return Err(StageError::bad("“to” must be top or bottom.")),
                (Some(by), None) => {
                    let by: f64 = by
                        .trim()
                        .parse()
                        .ok()
                        .filter(|v: &f64| v.is_finite())
                        .ok_or_else(|| StageError::bad("“by” must be a number of points."))?;
                    payload["by"] = json!(by);
                }
                (None, None) => {
                    return Err(StageError::bad(
                        "Give “by” (points, positive scrolls down) or “to” (top or bottom).",
                    ));
                }
            }
        }
        let reply = self
            .act_on_page(payload)
            .await
            .filter(|r| r.handled)
            .ok_or_else(not_shown)?;
        if !reply.ok {
            return Err(Self::page_failed(&reply));
        }
        Ok(self.anchor_result(anchor, &reply))
    }

    async fn wait(&self, params: &Params) -> StageResult<Map<String, Value>> {
        let anchor = required(params, "anchor")?;
        let timeout = match params.get("timeout").map(|t| t.trim()) {
            None | Some("") => WAIT_DEFAULT_MS,
            Some(raw) => raw
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && *v >= 0.0)
                .ok_or_else(|| StageError::bad("“timeout” must be milliseconds."))?
                .min(WAIT_MAX_MS as f64) as u64,
        };
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout);
        loop {
            let frame = self.host.frame();
            if let Some(frame) = frame.as_ref()
                && let Some(found) = self
                    .page_anchors(Some(frame))
                    .await
                    .into_iter()
                    .find(|a| a.id == anchor)
            {
                let mut out = Map::new();
                out.insert("anchor".into(), json!(anchor));
                out.insert("window".into(), json!(found.window));
                out.insert(
                    "rect".into(),
                    rect_json(to_screen_rect(found.rect, frame.content)),
                );
                out.insert("visible".into(), screen(found.visible, Some(frame)));
                return Ok(out);
            }
            if tokio::time::Instant::now() + WAIT_POLL > deadline {
                return Err(StageError::new(
                    404,
                    format!("“{anchor}” didn't appear within {timeout} ms."),
                ));
            }
            tokio::time::sleep(WAIT_POLL).await;
        }
    }

    /// §5.5: a command's shortcut runs the command (the page resolves it like a real key press,
    /// in the current view); otherwise the focused element of the focused window gets the keys.
    async fn keys(&self, params: &Params) -> StageResult<Map<String, Value>> {
        let keys: StageKeys = parse_keys(required(params, "keys")?).ok_or_else(|| {
            StageError::bad("“keys” isn't a key combination (e.g. ⌘K, cmd+shift+k, Enter).")
        })?;
        let window = params
            .get("window")
            .map(|w| w.trim())
            .filter(|w| !w.is_empty());
        let frame = self.host.frame();
        let mut focused = false;
        if let Some(id) = window {
            if !WINDOWS.contains(&id) {
                return Err(StageError::new(
                    404,
                    format!("“window” must be one of {}.", WINDOWS.join(", ")),
                ));
            }
            let info = if id == MAIN { None } else { self.info().await };
            if !Self::is_shown(id, frame.as_ref(), info.as_ref()) {
                return Err(Self::not_open(id));
            }
            self.host.focus();
            focused = true;
        }
        let no_focus = || {
            StageError::new(
                409,
                "No databee window has keyboard focus; pass window=main (or settings, palette).",
            )
        };
        if frame.is_none() {
            return Err(no_focus());
        }
        let reply = self
            .act_on_page(json!({ "kind": "keys", "keys": keys, "focused": focused }))
            .await
            .filter(|r| r.handled)
            .ok_or_else(no_focus)?;
        if !reply.ok {
            return Err(Self::page_failed(&reply));
        }
        let mut out = Map::new();
        match reply.command {
            Some(command) => out.insert("command".into(), json!(command)),
            None => out.insert(
                "window".into(),
                json!(reply.window.as_deref().unwrap_or(MAIN)),
            ),
        };
        Ok(out)
    }

    async fn act(&self, params: &Params) -> StageResult<Map<String, Value>> {
        match params.get("kind").map(|k| k.trim()).unwrap_or_default() {
            "command" => self.command(params).await,
            kind @ ("click" | "type" | "reveal" | "scroll") => self.on_anchor(kind, params).await,
            "wait" => self.wait(params).await,
            "keys" => self.keys(params).await,
            _ => Err(StageError::bad(
                "“kind” must be click, type, reveal, scroll, wait, keys or command.",
            )),
        }
    }

    /// `/v1/stage/<verb>`; the result is sent as `{ ok: true, ...result }`.
    pub async fn run(&self, verb: &str, params: &Params) -> StageResult<Map<String, Value>> {
        *self.last_request.lock().unwrap() = Some(chrono::Utc::now());
        match verb {
            "describe" => Ok(self.describe().await),
            "state" => {
                let state = self.info().await.map(|i| i.state).unwrap_or_default();
                let mut out = Map::new();
                out.insert("state".into(), Value::Object(state));
                Ok(out)
            }
            "window" => self.window(params).await,
            "act" => self.act(params).await,
            _ => Err(StageError::new(
                404,
                "Use /v1/stage/describe, window, act or state.",
            )),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::{Arc, OnceLock, Weak};

    use super::*;
    use crate::stage::protocol::{Choice, PageCommand, Surface};

    /// A page that answers like src/stage/hooks.ts would, and a window that can be moved.
    pub struct FakePage {
        pub hub: OnceLock<Weak<Hub>>,
        pub frame: Mutex<Option<Frame>>,
        pub anchors: Mutex<Vec<PageAnchor>>,
        pub commands: Vec<PageCommand>,
        /// Which sheet is open, and on which section.
        pub sheet: Mutex<Option<(String, Option<String>)>>,
        /// What the page answers to an act.
        pub act: fn(&Value) -> ActReply,
        pub silent: bool,
        pub events: Mutex<Vec<(String, Value)>>,
    }

    pub fn page_rect(x: f64, y: f64) -> PageRect {
        PageRect {
            x,
            y,
            width: 100.0,
            height: 20.0,
        }
    }

    pub fn frame(visible: bool, focused: bool) -> Frame {
        let rect = ScreenRect {
            x: 100,
            y: 50,
            width: 1200,
            height: 800,
        };
        Frame {
            title: "databee".into(),
            visible,
            focused,
            bounds: rect,
            content: rect,
        }
    }

    pub fn anchor(id: &str, window: &str, rect: PageRect) -> PageAnchor {
        PageAnchor {
            id: id.into(),
            label: id.into(),
            rect,
            in_view: true,
            visible: Some(rect),
            window: window.into(),
            role: None,
            list: None,
            selected: None,
            checked: None,
            expanded: None,
            disabled: None,
        }
    }

    fn command(id: &str, enabled: bool) -> PageCommand {
        PageCommand {
            id: id.into(),
            help: id.into(),
            shortcut: None,
            enabled,
            arg: None,
            choices: None,
        }
    }

    /// The default page: an anchor exists → handled; "disabled" fails; scrolling "flat" is 409.
    fn default_act(p: &Value) -> ActReply {
        let ok = |window: &str| ActReply {
            handled: true,
            ok: true,
            window: Some(window.into()),
            rect: Some(page_rect(10.0, 20.0)),
            visible: Some(page_rect(10.0, 20.0)),
            ..ActReply::default()
        };
        match (p["kind"].as_str(), p["anchor"].as_str()) {
            (Some("keys"), _) if p["keys"]["key"] == "," => ActReply {
                command: Some("app:settings".into()),
                ..ok(MAIN)
            },
            (Some("keys"), _) if p["focused"] == false => ActReply::default(),
            (Some("scroll"), Some("flat")) => ActReply {
                handled: true,
                status: Some(409),
                error: Some("“flat” isn't scrollable".into()),
                ..ActReply::default()
            },
            (_, Some("disabled")) => ActReply {
                handled: true,
                error: Some("“disabled” is disabled".into()),
                ..ActReply::default()
            },
            (_, Some("settings:queries")) => ok("settings"),
            (_, Some("nope")) => ActReply::default(),
            _ => ok(MAIN),
        }
    }

    impl Default for FakePage {
        fn default() -> Self {
            Self {
                hub: OnceLock::new(),
                frame: Mutex::new(Some(frame(true, true))),
                anchors: Mutex::new(vec![
                    anchor("statusbar:theme", MAIN, page_rect(900.0, 780.0)),
                    anchor("toolbar:settings", MAIN, page_rect(1100.0, 8.4)),
                ]),
                commands: vec![
                    command("app:settings", true),
                    command("editor.run", false),
                    PageCommand {
                        arg: Some("<section>".into()),
                        choices: Some(vec![Choice {
                            id: "settings:open:queries".into(),
                            title: "Queries".into(),
                        }]),
                        ..command("settings:open:", true)
                    },
                ],
                sheet: Mutex::default(),
                act: default_act,
                silent: false,
                events: Mutex::default(),
            }
        }
    }

    impl FakePage {
        fn info(&self, request: String) -> InfoReply {
            let sheet = self.sheet.lock().unwrap().clone();
            let surface = |id: &str, title: &str| Surface {
                id: id.into(),
                title: title.into(),
                rect: sheet
                    .as_ref()
                    .filter(|(open, _)| open == id)
                    .map(|_| PageRect {
                        x: 200.0,
                        y: 100.0,
                        width: 700.0,
                        height: 600.0,
                    }),
                sections: (id == "settings").then(|| {
                    vec![Choice {
                        id: "queries".into(),
                        title: "Queries".into(),
                    }]
                }),
                section: sheet.as_ref().and_then(|(_, s)| s.clone()),
            };
            InfoReply {
                request,
                page: MAIN.into(),
                state: json!({ "mode": "db" }).as_object().unwrap().clone(),
                view: Some("db".into()),
                ui: json!({ "inspector": "closed" })
                    .as_object()
                    .unwrap()
                    .clone(),
                surfaces: vec![
                    surface("settings", "Settings"),
                    surface("palette", "Command Palette"),
                ],
                commands: self.commands.clone(),
            }
        }
    }

    impl StageHost for Arc<FakePage> {
        fn frame(&self) -> Option<Frame> {
            self.frame.lock().unwrap().clone()
        }
        fn show(&self) {
            let mut frame = self.frame.lock().unwrap();
            *frame = Some(Frame {
                visible: true,
                ..frame
                    .clone()
                    .unwrap_or_else(|| super::tests::frame(true, false))
            });
        }
        fn hide(&self) {
            if let Some(f) = self.frame.lock().unwrap().as_mut() {
                f.visible = false;
                f.focused = false;
            }
        }
        fn focus(&self) {
            if let Some(f) = self.frame.lock().unwrap().as_mut() {
                f.focused = true;
            }
        }
        fn place(&self, rect: ScreenRect) {
            if let Some(f) = self.frame.lock().unwrap().as_mut() {
                f.bounds = rect;
                f.content = rect;
            }
        }
        fn emit(&self, event: &str, payload: Value) {
            self.events
                .lock()
                .unwrap()
                .push((event.into(), payload.clone()));
            if self.silent {
                return;
            }
            let hub = self.hub.get().and_then(Weak::upgrade).unwrap();
            let request = payload["request"].as_str().unwrap().to_string();
            match event {
                COLLECT_EVENT => hub.receive_anchors(AnchorsReply {
                    request,
                    page: MAIN.into(),
                    anchors: self.anchors.lock().unwrap().clone(),
                }),
                INFO_EVENT => hub.receive_info(self.info(request)),
                _ if payload["kind"] == "window" => {
                    let id = payload["window"].as_str().unwrap().to_string();
                    *self.sheet.lock().unwrap() = (payload["action"] == "show")
                        .then(|| (id, payload["section"].as_str().map(String::from)));
                    hub.receive_act(ActReply {
                        request,
                        page: MAIN.into(),
                        handled: true,
                        ok: true,
                        ..ActReply::default()
                    })
                }
                _ => hub.receive_act(ActReply {
                    request,
                    page: MAIN.into(),
                    ..(self.act)(&payload)
                }),
            }
        }
    }

    pub fn hub_with(page: FakePage) -> (Arc<Hub>, Arc<FakePage>) {
        let page = Arc::new(page);
        let timeouts = Timeouts {
            collect: Duration::from_millis(50),
            act: Duration::from_millis(50),
            settle: Duration::ZERO,
        };
        let hub = Arc::new(Hub::new(
            Box::new(page.clone()),
            timeouts,
            "databee",
            "com.test",
        ));
        page.hub.set(Arc::downgrade(&hub)).unwrap();
        (hub, page)
    }

    fn params(pairs: &[(&str, &str)]) -> Params {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    async fn run(hub: &Hub, verb: &str, pairs: &[(&str, &str)]) -> StageResult<Value> {
        hub.run(verb, &params(pairs)).await.map(Value::Object)
    }

    async fn status(hub: &Hub, verb: &str, pairs: &[(&str, &str)]) -> u16 {
        run(hub, verb, pairs).await.err().map_or(200, |e| e.status)
    }

    fn last_act(page: &FakePage) -> Value {
        page.events
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(e, _)| e == ACT_EVENT)
            .unwrap()
            .1
            .clone()
    }

    #[tokio::test]
    async fn describe_lists_features_windows_anchors_and_commands() {
        let (hub, page) = hub_with(FakePage::default());
        page.anchors.lock().unwrap().push(PageAnchor {
            role: Some("item".into()),
            list: Some("settings-nav".into()),
            selected: Some(true),
            disabled: Some(false),
            ..anchor("settings-nav:queries", "settings", page_rect(210.0, 0.0))
        });
        let d = run(&hub, "describe", &[]).await.unwrap();
        assert_eq!(d["protocol"], 1);
        assert_eq!(d["features"], json!(FEATURES));
        assert_eq!(d["app"]["name"], "databee");
        assert_eq!(
            d["windows"][0],
            json!({
                "id": "main", "title": "databee", "visible": true, "focused": true, "movable": true,
                "bounds": { "x": 100, "y": 50, "width": 1200, "height": 800 },
                "content": { "x": 100, "y": 50, "width": 1200, "height": 800 },
                "view": "db", "ui": { "inspector": "closed" }
            })
        );
        assert_eq!(
            d["windows"][1],
            json!({
                "id": "settings", "title": "Settings", "visible": false, "focused": false,
                "movable": false, "bounds": null, "content": null, "view": null,
                "sections": [{ "id": "queries", "title": "Queries" }], "section": null
            })
        );
        assert_eq!(d["windows"][2]["id"], "palette");
        let ids: Vec<_> = d["anchors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["id"].as_str().unwrap())
            .collect();
        // Window order first, then top to bottom.
        assert_eq!(
            ids,
            [
                "toolbar:settings",
                "statusbar:theme",
                "settings-nav:queries"
            ]
        );
        assert_eq!(
            d["anchors"][0],
            json!({ "id": "toolbar:settings", "label": "toolbar:settings", "window": "main",
                    "rect": { "x": 1200, "y": 58, "width": 100, "height": 20 }, "inView": true,
                    "visible": { "x": 1200, "y": 58, "width": 100, "height": 20 } })
        );
        assert_eq!(d["anchors"][2]["list"], "settings-nav");
        assert_eq!(d["anchors"][2]["selected"], true);
        assert!(d["anchors"][2].get("disabled").is_none());
        assert_eq!(
            d["commands"][1],
            json!({ "id": "editor.run", "help": "editor.run", "enabled": false })
        );
        assert_eq!(d["commands"][2]["arg"], "<section>");
        assert_eq!(d["hotkeys"], json!([]));
        assert_eq!(d["state"], json!({ "mode": "db" }));
    }

    #[tokio::test]
    async fn a_hidden_window_has_no_bounds_and_no_anchors() {
        let page = FakePage::default();
        *page.frame.lock().unwrap() = Some(frame(false, false));
        let (hub, _) = hub_with(page);
        let d = run(&hub, "describe", &[]).await.unwrap();
        assert_eq!(d["windows"][0]["visible"], false);
        assert_eq!(d["windows"][0]["bounds"], Value::Null);
        assert_eq!(d["anchors"], json!([]));
    }

    #[tokio::test]
    async fn a_silent_page_still_gets_a_description() {
        let (hub, _) = hub_with(FakePage {
            silent: true,
            ..FakePage::default()
        });
        let d = run(&hub, "describe", &[]).await.unwrap();
        assert_eq!(d["anchors"], json!([]));
        assert_eq!(d["commands"], json!([]));
        assert_eq!(
            run(&hub, "state", &[]).await.unwrap(),
            json!({ "state": {} })
        );
    }

    #[tokio::test]
    async fn the_main_window_shows_hides_focuses_and_places() {
        let (hub, page) = hub_with(FakePage::default());
        let w = run(&hub, "window", &[("window", "main"), ("action", "hide")])
            .await
            .unwrap();
        assert_eq!(w["window"]["visible"], false);
        assert_eq!(
            status(&hub, "window", &[("window", "main"), ("action", "focus")]).await,
            409
        );
        assert_eq!(
            status(&hub, "window", &[("window", "main"), ("x", "5")]).await,
            409
        );
        let w = run(&hub, "window", &[("window", "main")]).await.unwrap();
        assert_eq!(w["window"]["visible"], true);
        let w = run(
            &hub,
            "window",
            &[("window", "main"), ("x", "5"), ("width", "900")],
        )
        .await
        .unwrap();
        assert_eq!(
            w["window"]["bounds"],
            json!({ "x": 5, "y": 50, "width": 900, "height": 800 })
        );
        run(&hub, "window", &[("window", "main"), ("action", "focus")])
            .await
            .unwrap();
        assert!(page.frame.lock().unwrap().as_ref().unwrap().focused);
    }

    #[tokio::test]
    async fn settings_opens_on_a_section_as_its_own_window() {
        let (hub, page) = hub_with(FakePage::default());
        let w = run(
            &hub,
            "window",
            &[("window", "settings"), ("section", "queries")],
        )
        .await
        .unwrap();
        assert_eq!(w["window"]["visible"], true);
        assert_eq!(w["window"]["section"], "queries");
        assert_eq!(
            w["window"]["bounds"],
            json!({ "x": 300, "y": 150, "width": 700, "height": 600 })
        );
        assert_eq!(last_act(&page)["section"], "queries");
        let d = run(&hub, "describe", &[]).await.unwrap();
        assert_eq!(d["windows"][0]["focused"], false);
        assert_eq!(d["windows"][1]["focused"], true);

        assert_eq!(
            status(&hub, "window", &[("window", "settings"), ("x", "1")]).await,
            409
        );
        assert_eq!(
            status(
                &hub,
                "window",
                &[("window", "settings"), ("section", "nope")]
            )
            .await,
            404
        );
        let w = run(
            &hub,
            "window",
            &[("window", "settings"), ("action", "hide")],
        )
        .await
        .unwrap();
        assert_eq!(w["window"]["visible"], false);
        assert_eq!(
            status(
                &hub,
                "window",
                &[("window", "palette"), ("action", "focus")]
            )
            .await,
            409
        );
    }

    #[tokio::test]
    async fn window_requests_are_validated() {
        let (hub, _) = hub_with(FakePage::default());
        assert_eq!(
            status(&hub, "window", &[("window", "inspector")]).await,
            404
        );
        assert_eq!(
            status(&hub, "window", &[("window", "main"), ("width", "-1")]).await,
            400
        );
        assert_eq!(
            status(&hub, "window", &[("window", "main"), ("action", "fly")]).await,
            400
        );
        assert_eq!(
            status(&hub, "window", &[("window", "main"), ("action", "place")]).await,
            400
        );
    }

    #[tokio::test]
    async fn acts_on_anchors_and_reports_their_screen_rect() {
        let (hub, page) = hub_with(FakePage::default());
        let r = run(
            &hub,
            "act",
            &[("kind", "click"), ("anchor", "toolbar:settings")],
        )
        .await
        .unwrap();
        assert_eq!(
            r,
            json!({ "anchor": "toolbar:settings", "window": "main",
                    "rect": { "x": 110, "y": 70, "width": 100, "height": 20 },
                    "visible": { "x": 110, "y": 70, "width": 100, "height": 20 } })
        );
        let r = run(
            &hub,
            "act",
            &[("kind", "reveal"), ("anchor", "settings:queries")],
        )
        .await
        .unwrap();
        assert_eq!(r["window"], "settings");
        run(
            &hub,
            "act",
            &[
                ("kind", "type"),
                ("anchor", "palette:search"),
                ("text", "Hi"),
                ("mode", "append"),
            ],
        )
        .await
        .unwrap();
        assert_eq!(last_act(&page)["text"], "Hi");
        assert_eq!(last_act(&page)["mode"], "append");
        run(
            &hub,
            "act",
            &[("kind", "type"), ("anchor", "editor"), ("text", "x")],
        )
        .await
        .unwrap();
        assert_eq!(last_act(&page)["mode"], "replace");
        run(
            &hub,
            "act",
            &[("kind", "scroll"), ("anchor", "results"), ("by", "-120")],
        )
        .await
        .unwrap();
        assert_eq!(last_act(&page)["by"], -120.0);
        run(
            &hub,
            "act",
            &[("kind", "scroll"), ("anchor", "results"), ("to", "bottom")],
        )
        .await
        .unwrap();
        assert_eq!(last_act(&page)["to"], "bottom");
    }

    #[tokio::test]
    async fn act_failures_map_to_status_codes() {
        let (hub, page) = hub_with(FakePage::default());
        let missing = run(&hub, "act", &[("kind", "click"), ("anchor", "nope")])
            .await
            .unwrap_err();
        assert_eq!(missing.status, 404);
        assert!(missing.message.contains("describe"));
        let disabled = run(&hub, "act", &[("kind", "click"), ("anchor", "disabled")])
            .await
            .unwrap_err();
        assert_eq!(disabled, StageError::new(500, "“disabled” is disabled"));
        assert_eq!(
            status(
                &hub,
                "act",
                &[("kind", "scroll"), ("anchor", "flat"), ("by", "10")]
            )
            .await,
            409
        );
        for bad in [
            &[("kind", "click")][..],
            &[("kind", "dance")],
            &[("kind", "keys"), ("keys", "hyper+q")],
            &[("kind", "type"), ("anchor", "editor"), ("mode", "shout")],
            &[("kind", "scroll"), ("anchor", "results")],
            &[("kind", "scroll"), ("anchor", "results"), ("by", "far")],
            &[("kind", "scroll"), ("anchor", "results"), ("to", "middle")],
            &[("kind", "wait"), ("anchor", "x"), ("timeout", "soon")],
        ] {
            assert_eq!(status(&hub, "act", bad).await, 400, "{bad:?}");
        }
        assert_eq!(status(&hub, "nope", &[]).await, 404);

        page.frame.lock().unwrap().as_mut().unwrap().visible = false;
        assert_eq!(
            status(
                &hub,
                "act",
                &[("kind", "reveal"), ("anchor", "toolbar:settings")]
            )
            .await,
            404
        );
    }

    #[tokio::test]
    async fn keys_run_shortcut_commands_or_reach_the_focused_window() {
        let (hub, page) = hub_with(FakePage::default());
        page.frame.lock().unwrap().as_mut().unwrap().focused = false;
        // A command's shortcut works without focus, like a menu accelerator.
        let r = run(&hub, "act", &[("kind", "keys"), ("keys", "⌘,")])
            .await
            .unwrap();
        assert_eq!(r, json!({ "command": "app:settings" }));
        // Other keys need a focused window; window= focuses it first.
        assert_eq!(
            status(&hub, "act", &[("kind", "keys"), ("keys", "Esc")]).await,
            409
        );
        let r = run(
            &hub,
            "act",
            &[("kind", "keys"), ("keys", "⌘K"), ("window", "main")],
        )
        .await
        .unwrap();
        assert_eq!(r, json!({ "window": "main" }));
        assert!(page.frame.lock().unwrap().as_ref().unwrap().focused);
        assert_eq!(last_act(&page)["keys"]["code"], "KeyK");
        assert_eq!(last_act(&page)["focused"], true);
        assert_eq!(
            status(
                &hub,
                "act",
                &[("kind", "keys"), ("keys", "Enter"), ("window", "palette")]
            )
            .await,
            409
        );
        assert_eq!(
            status(
                &hub,
                "act",
                &[("kind", "keys"), ("keys", "Enter"), ("window", "dock")]
            )
            .await,
            404
        );
    }

    #[tokio::test]
    async fn commands_and_families_run_through_the_page_when_enabled() {
        let (hub, page) = hub_with(FakePage::default());
        let r = run(
            &hub,
            "act",
            &[("kind", "command"), ("command", "app:settings")],
        )
        .await
        .unwrap();
        assert_eq!(r, json!({ "command": "app:settings" }));
        run(
            &hub,
            "act",
            &[("kind", "command"), ("command", "settings:open:queries")],
        )
        .await
        .unwrap();
        assert_eq!(last_act(&page)["command"], "settings:open:queries");
        for (command, code) in [("nope", 404), ("settings:open:", 404), ("editor.run", 409)] {
            assert_eq!(
                status(&hub, "act", &[("kind", "command"), ("command", command)]).await,
                code,
                "{command}"
            );
        }
    }

    #[tokio::test]
    async fn wait_answers_once_the_anchor_appears() {
        let (hub, page) = hub_with(FakePage::default());
        let late = {
            let page = page.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(150)).await;
                page.anchors.lock().unwrap().push(anchor(
                    "palette:search",
                    "palette",
                    page_rect(300.0, 120.0),
                ));
            })
        };
        let r = run(
            &hub,
            "act",
            &[("kind", "wait"), ("anchor", "palette:search")],
        )
        .await
        .unwrap();
        late.await.unwrap();
        assert_eq!(r["window"], "palette");
        assert_eq!(
            r["rect"],
            json!({ "x": 400, "y": 170, "width": 100, "height": 20 })
        );
        let err = run(
            &hub,
            "act",
            &[("kind", "wait"), ("anchor", "never"), ("timeout", "200")],
        )
        .await
        .unwrap_err();
        assert_eq!(
            err,
            StageError::new(404, "“never” didn't appear within 200 ms.")
        );
    }

    #[tokio::test]
    async fn remembers_the_last_presenter_request() {
        let (hub, _) = hub_with(FakePage::default());
        assert!(hub.last_request().is_none());
        run(&hub, "state", &[]).await.unwrap();
        assert!(hub.last_request().is_some());
    }

    #[tokio::test]
    async fn replies_for_unknown_requests_or_pages_are_ignored() {
        let (hub, _) = hub_with(FakePage::default());
        hub.receive_act(ActReply {
            request: "unknown".into(),
            page: "main".into(),
            handled: true,
            ok: true,
            ..ActReply::default()
        });
        assert!(hub.take("unknown", "main").is_none());
        assert!(hub.take("x", "settings").is_none());
    }
}
