//! The Stage hook's verbs (docs/stage.md §2.3–2.6): what a presentation layer may read
//! and make databee do. The page answers anchor, act and info requests (src/stage/hooks.ts);
//! `http.rs` routes `/v1/stage/<verb>` here.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{Map, Value, json};
use tokio::sync::oneshot;

use super::protocol::{
    ActReply, AnchorsReply, Bounds, InfoReply, MAIN, PROTOCOL, Params, ScreenRect, StageError,
    StageKeys, StageResult, parse_bounds, parse_keys, required, to_screen_rect,
};

pub const COLLECT_EVENT: &str = "stage:collect";
pub const ACT_EVENT: &str = "stage:act";
pub const INFO_EVENT: &str = "stage:info";

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
}

fn rect_json(rect: ScreenRect) -> Value {
    serde_json::to_value(rect).unwrap_or(Value::Null)
}

impl Hub {
    pub fn new(host: Box<dyn StageHost>, timeouts: Timeouts, name: &str, id: &str) -> Self {
        Self {
            host,
            timeouts,
            app: json!({ "name": name, "id": id, "version": env!("CARGO_PKG_VERSION") }),
            waiting: Mutex::default(),
        }
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

    async fn anchors(&self, frame: Option<&Frame>) -> Vec<Value> {
        let Some(frame) = frame.filter(|f| f.visible) else {
            return Vec::new();
        };
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
        let mut anchors: Vec<_> = reply
            .anchors
            .into_iter()
            .map(|a| (to_screen_rect(a.rect, frame.content), a))
            .collect();
        // Top to bottom, then left to right, so a script reads in order.
        anchors.sort_by_key(|(rect, _)| (rect.y, rect.x));
        anchors
            .into_iter()
            .map(|(rect, a)| {
                json!({ "id": a.id, "label": a.label, "window": MAIN, "rect": rect, "inView": a.in_view })
            })
            .collect()
    }

    async fn act_on_page(&self, payload: Value) -> Option<ActReply> {
        let Value::Object(payload) = payload else {
            return None;
        };
        self.ask(ACT_EVENT, payload, self.timeouts.act, Waiter::Act)
            .await
    }

    // Verbs --------------------------------------------------------------------------------

    fn window_json(frame: Option<&Frame>, view: Option<String>) -> Value {
        let shown = frame.filter(|f| f.visible);
        json!({
            "id": MAIN,
            "title": frame.map_or("databee", |f| f.title.as_str()),
            "visible": shown.is_some(),
            "focused": shown.is_some_and(|f| f.focused),
            "movable": true,
            "bounds": shown.map(|f| rect_json(f.bounds)),
            "content": shown.map(|f| rect_json(f.content)),
            "view": shown.and(view),
        })
    }

    async fn describe(&self) -> Map<String, Value> {
        let frame = self.host.frame();
        let (info, anchors) = tokio::join!(self.info(), self.anchors(frame.as_ref()));
        let (state, view, commands) = match info {
            Some(i) => (i.state, i.view, i.commands),
            None => (Map::new(), None, Vec::new()),
        };
        let mut out = Map::new();
        out.insert("protocol".into(), json!(PROTOCOL));
        out.insert("app".into(), self.app.clone());
        out.insert(
            "windows".into(),
            json!([Self::window_json(frame.as_ref(), view)]),
        );
        out.insert("anchors".into(), Value::Array(anchors));
        out.insert("commands".into(), json!(commands));
        // databee has no global shortcuts; in-app ones reach the page through `keys`.
        out.insert("hotkeys".into(), json!([]));
        out.insert("state".into(), Value::Object(state));
        out
    }

    fn open_frame(&self) -> StageResult<Frame> {
        self.host
            .frame()
            .filter(|f| f.visible)
            .ok_or_else(|| StageError::new(409, "The main window isn't open; show it first."))
    }

    async fn window(&self, params: &Params) -> StageResult<Map<String, Value>> {
        let window = params.get("window").map(|w| w.trim()).unwrap_or_default();
        if window != MAIN {
            return Err(StageError::new(
                404,
                "“window” must be main (databee has one window; settings is a dialog in it).",
            ));
        }
        let bounds: Bounds = parse_bounds(params)?;
        let action = match params.get("action").map(|a| a.trim()) {
            Some(a) if !a.is_empty() => a,
            _ if !bounds.is_empty() => "place",
            _ => "show",
        };
        match action {
            "show" => self.host.show(),
            "hide" => self.host.hide(),
            "focus" => {
                self.open_frame()?;
                self.host.focus();
            }
            "place" => {
                let frame = self.open_frame()?;
                if bounds.is_empty() {
                    return Err(StageError::bad(
                        "Give x, y, width or height to place the window.",
                    ));
                }
                self.host.place(bounds.over(frame.bounds));
            }
            _ => {
                return Err(StageError::bad(
                    "“action” must be show, hide, focus or place.",
                ));
            }
        }
        tokio::time::sleep(self.timeouts.settle).await;
        let view = self.info().await.and_then(|i| i.view);
        let mut out = Map::new();
        out.insert(
            "window".into(),
            Self::window_json(self.host.frame().as_ref(), view),
        );
        Ok(out)
    }

    fn failed(reply: &ActReply) -> StageError {
        StageError::new(
            500,
            reply
                .error
                .clone()
                .filter(|e| !e.is_empty())
                .unwrap_or_else(|| "The page couldn't do that.".into()),
        )
    }

    async fn act(&self, params: &Params) -> StageResult<Map<String, Value>> {
        let kind = params.get("kind").map(|k| k.trim()).unwrap_or_default();
        let mut out = Map::new();
        match kind {
            "command" => {
                let command = required(params, "command")?;
                let commands = self.info().await.map(|i| i.commands).unwrap_or_default();
                let Some(found) = commands.iter().find(|c| c.id == command) else {
                    return Err(StageError::new(
                        404,
                        format!("Unknown command “{command}”; describe lists them."),
                    ));
                };
                if !found.available {
                    return Err(StageError::new(
                        409,
                        format!("“{command}” isn't available in the current view."),
                    ));
                }
                let reply = self
                    .act_on_page(json!({ "kind": "command", "command": command }))
                    .await
                    .ok_or_else(|| StageError::new(500, "The page didn't answer."))?;
                if !reply.ok {
                    return Err(Self::failed(&reply));
                }
                out.insert("command".into(), json!(command));
            }
            "click" | "type" | "reveal" => {
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
                }
                let reply = self
                    .act_on_page(payload)
                    .await
                    .filter(|r| r.handled)
                    .ok_or_else(not_shown)?;
                if !reply.ok {
                    return Err(Self::failed(&reply));
                }
                let rect = match (reply.rect, self.host.frame()) {
                    (Some(rect), Some(frame)) => rect_json(to_screen_rect(rect, frame.content)),
                    _ => Value::Null,
                };
                out.insert("anchor".into(), json!(anchor));
                out.insert("window".into(), json!(MAIN));
                out.insert("rect".into(), rect);
            }
            "keys" => {
                let keys: StageKeys = parse_keys(required(params, "keys")?).ok_or_else(|| {
                    StageError::bad("“keys” isn't a key combination (e.g. ⌘K, cmd+shift+k, Enter).")
                })?;
                let no_focus = || {
                    StageError::new(
                        409,
                        "No databee window has keyboard focus; focus it first (window action=focus).",
                    )
                };
                if !self.host.frame().is_some_and(|f| f.visible && f.focused) {
                    return Err(no_focus());
                }
                let reply = self
                    .act_on_page(json!({ "kind": "keys", "keys": keys }))
                    .await
                    .filter(|r| r.handled)
                    .ok_or_else(no_focus)?;
                if !reply.ok {
                    return Err(Self::failed(&reply));
                }
                out.insert("window".into(), json!(MAIN));
            }
            _ => {
                return Err(StageError::bad(
                    "“kind” must be click, type, reveal, keys or command.",
                ));
            }
        }
        Ok(out)
    }

    /// `/v1/stage/<verb>`; the result is sent as `{ ok: true, ...result }`.
    pub async fn run(&self, verb: &str, params: &Params) -> StageResult<Map<String, Value>> {
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
    use crate::stage::protocol::{PageAnchor, PageCommand, PageRect};

    /// A page that answers like src/stage/hooks.ts would, and a window that can be moved.
    pub struct FakePage {
        pub hub: OnceLock<Weak<Hub>>,
        pub frame: Mutex<Option<Frame>>,
        pub anchors: Vec<PageAnchor>,
        pub commands: Vec<PageCommand>,
        /// What the page answers to an act: (handled, ok, error).
        pub act: fn(&Value) -> (bool, bool, Option<String>),
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
        Frame {
            title: "databee".into(),
            visible,
            focused,
            bounds: ScreenRect {
                x: 100,
                y: 50,
                width: 1200,
                height: 800,
            },
            content: ScreenRect {
                x: 100,
                y: 50,
                width: 1200,
                height: 800,
            },
        }
    }

    impl Default for FakePage {
        fn default() -> Self {
            Self {
                hub: OnceLock::new(),
                frame: Mutex::new(Some(frame(true, true))),
                anchors: vec![
                    PageAnchor {
                        id: "statusbar:theme".into(),
                        label: "Theme".into(),
                        rect: page_rect(900.0, 780.0),
                        in_view: true,
                    },
                    PageAnchor {
                        id: "toolbar:settings".into(),
                        label: "Settings".into(),
                        rect: page_rect(1100.0, 8.4),
                        in_view: true,
                    },
                ],
                commands: vec![
                    PageCommand {
                        id: "settings.open".into(),
                        help: "Settings".into(),
                        available: true,
                    },
                    PageCommand {
                        id: "editor.run".into(),
                        help: "Run statement".into(),
                        available: false,
                    },
                ],
                act: |p| match p["anchor"].as_str() {
                    Some("toolbar:settings") | None => (true, true, None),
                    Some("disabled") => (true, false, Some("“disabled” is disabled".into())),
                    _ => (false, false, None),
                },
                silent: false,
                events: Mutex::default(),
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
            let page = MAIN.to_string();
            match event {
                COLLECT_EVENT => hub.receive_anchors(AnchorsReply {
                    request,
                    page,
                    anchors: self.anchors.clone(),
                }),
                INFO_EVENT => hub.receive_info(InfoReply {
                    request,
                    page,
                    state: json!({ "mode": "db" }).as_object().unwrap().clone(),
                    view: Some("db".into()),
                    commands: self.commands.clone(),
                }),
                _ => {
                    let (handled, ok, error) = (self.act)(&payload);
                    hub.receive_act(ActReply {
                        request,
                        page,
                        handled,
                        ok,
                        error,
                        rect: handled.then(|| page_rect(10.0, 20.0)),
                    })
                }
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

    #[tokio::test]
    async fn describe_lists_the_window_anchors_in_reading_order_and_commands() {
        let (hub, _) = hub_with(FakePage::default());
        let d = run(&hub, "describe", &[]).await.unwrap();
        assert_eq!(d["protocol"], 1);
        assert_eq!(d["app"]["name"], "databee");
        assert_eq!(
            d["windows"],
            json!([{
                "id": "main", "title": "databee", "visible": true, "focused": true, "movable": true,
                "bounds": { "x": 100, "y": 50, "width": 1200, "height": 800 },
                "content": { "x": 100, "y": 50, "width": 1200, "height": 800 },
                "view": "db"
            }])
        );
        assert_eq!(
            d["anchors"][0],
            json!({ "id": "toolbar:settings", "label": "Settings", "window": "main",
                    "rect": { "x": 1200, "y": 58, "width": 100, "height": 20 }, "inView": true })
        );
        assert_eq!(d["anchors"][1]["id"], "statusbar:theme");
        assert_eq!(d["commands"][0]["id"], "settings.open");
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
    async fn windows_show_hide_focus_and_place() {
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
    async fn window_requests_are_validated() {
        let (hub, _) = hub_with(FakePage::default());
        assert_eq!(status(&hub, "window", &[("window", "settings")]).await, 404);
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
                    "rect": { "x": 110, "y": 70, "width": 100, "height": 20 } })
        );
        run(
            &hub,
            "act",
            &[
                ("kind", "type"),
                ("anchor", "toolbar:settings"),
                ("text", "Hi"),
            ],
        )
        .await
        .unwrap();
        let events = page.events.lock().unwrap();
        let (event, payload) = events.last().unwrap();
        assert_eq!(event, ACT_EVENT);
        assert_eq!(payload["text"], "Hi");
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
        assert_eq!(status(&hub, "act", &[("kind", "click")]).await, 400);
        assert_eq!(status(&hub, "act", &[("kind", "dance")]).await, 400);
        assert_eq!(
            status(&hub, "act", &[("kind", "keys"), ("keys", "hyper+q")]).await,
            400
        );
        assert_eq!(status(&hub, "nope", &[]).await, 404);

        page.frame.lock().unwrap().as_mut().unwrap().focused = false;
        assert_eq!(
            status(&hub, "act", &[("kind", "keys"), ("keys", "Esc")]).await,
            409
        );
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
    async fn keys_reach_the_focused_page_parsed() {
        let (hub, page) = hub_with(FakePage::default());
        let r = run(&hub, "act", &[("kind", "keys"), ("keys", "⌘K")])
            .await
            .unwrap();
        assert_eq!(r, json!({ "window": "main" }));
        let events = page.events.lock().unwrap();
        assert_eq!(events.last().unwrap().1["keys"]["code"], "KeyK");
        assert_eq!(events.last().unwrap().1["keys"]["meta"], true);
    }

    #[tokio::test]
    async fn commands_run_through_the_page_when_available() {
        let (hub, _) = hub_with(FakePage::default());
        let r = run(
            &hub,
            "act",
            &[("kind", "command"), ("command", "settings.open")],
        )
        .await
        .unwrap();
        assert_eq!(r, json!({ "command": "settings.open" }));
        assert_eq!(
            status(&hub, "act", &[("kind", "command"), ("command", "nope")]).await,
            404
        );
        assert_eq!(
            status(
                &hub,
                "act",
                &[("kind", "command"), ("command", "editor.run")]
            )
            .await,
            409
        );
    }

    #[tokio::test]
    async fn replies_for_unknown_requests_or_pages_are_ignored() {
        let (hub, _) = hub_with(FakePage::default());
        hub.receive_act(ActReply {
            request: "unknown".into(),
            page: "main".into(),
            handled: true,
            ok: true,
            error: None,
            rect: None,
        });
        assert!(hub.take("unknown", "main").is_none());
        assert!(hub.take("x", "settings").is_none());
    }
}
