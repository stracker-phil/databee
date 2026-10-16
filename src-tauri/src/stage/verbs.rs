//! The Stage hook's verbs (docs/stage.md): what a presenter may read and make DataBee do.
//! `http.rs` routes `/v1/stage/<verb>` here; status codes and messages are part of the protocol.

use std::str::FromStr;
use std::time::Duration;

use serde::Serialize;
use serde_json::{Map, Value};

use super::hub::{AppInfo, Hub};
use super::page::{ActReply, ActRequest, PageCommand, SheetAction};
use super::protocol::{
    FEATURES, PROTOCOL, Params, ScreenRect, StageError, StageResult, parse_keys, required,
    to_screen_rect,
};
use super::requests::{
    ActKind, WindowAction, WindowRequest, scroll_payload, trimmed, type_payload, wait_timeout,
};
use super::windows::{self, AnchorInfo, Window, WindowInfo, is_shown, screen};

const WAIT_POLL: Duration = Duration::from_millis(100);

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Describe {
    pub protocol: u32,
    pub features: [&'static str; 8],
    pub app: AppInfo,
    pub windows: Vec<WindowInfo>,
    pub anchors: Vec<AnchorInfo>,
    pub commands: Vec<PageCommand>,
    /// Always empty: DataBee has no global shortcuts; in-app ones work through `keys`.
    pub hotkeys: Vec<String>,
    pub state: Map<String, Value>,
}

#[derive(Serialize)]
struct StateResult {
    state: Map<String, Value>,
}

#[derive(Serialize)]
struct WindowResult {
    window: WindowInfo,
}

/// What `act` answers, by kind.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum ActResult {
    Anchor {
        anchor: String,
        window: Window,
        rect: Option<ScreenRect>,
        visible: Option<ScreenRect>,
    },
    Command {
        command: String,
    },
    /// Keys that reached an element of this window.
    Keys {
        window: Window,
    },
}

fn not_open(window: Window) -> StageError {
    StageError::new(
        409,
        format!("The {} window isn't open; show it first.", window.as_str()),
    )
}

fn no_answer() -> StageError {
    StageError::new(500, "The page didn't answer.")
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

fn succeeded(reply: ActReply) -> StageResult<ActReply> {
    if reply.ok {
        Ok(reply)
    } else {
        Err(page_failed(&reply))
    }
}

fn object(result: impl Serialize) -> StageResult<Map<String, Value>> {
    match serde_json::to_value(result) {
        Ok(Value::Object(out)) => Ok(out),
        Ok(_) => Err(StageError::new(500, "The answer isn't an object.")),
        Err(e) => Err(StageError::new(500, e.to_string())),
    }
}

impl Hub {
    /// `/v1/stage/<verb>`; the result is sent as `{ ok: true, ...result }`.
    pub async fn run(&self, verb: &str, params: &Params) -> StageResult<Map<String, Value>> {
        self.touch();
        match verb {
            "describe" => object(self.describe().await),
            "state" => {
                let state = self.info().await.map(|i| i.state).unwrap_or_default();
                object(StateResult { state })
            }
            "window" => object(self.window(params).await?),
            "act" => object(self.act(params).await?),
            _ => Err(StageError::new(
                404,
                "Use /v1/stage/describe, window, act or state.",
            )),
        }
    }

    pub async fn describe(&self) -> Describe {
        let frame = self.host.frame();
        let (info, anchors) = tokio::join!(self.info(), self.page_anchors(frame.as_ref()));
        let anchors = match frame.as_ref() {
            Some(frame) => anchors.iter().map(|a| AnchorInfo::new(a, frame)).collect(),
            None => Vec::new(),
        };
        let windows = windows::windows(frame.as_ref(), info.as_ref());
        let (commands, state) = match info {
            Some(i) => (i.commands, i.state),
            None => (Vec::new(), Map::new()),
        };
        Describe {
            protocol: PROTOCOL,
            features: FEATURES,
            app: self.app.clone(),
            windows,
            anchors,
            commands,
            hotkeys: Vec::new(),
            state,
        }
    }

    async fn window(&self, params: &Params) -> StageResult<WindowResult> {
        let request = WindowRequest::parse(params)?;
        match request.window {
            Window::Main => self.main_window_action(&request).await?,
            sheet => self.sheet_action(sheet, &request).await?,
        }
        tokio::time::sleep(self.timeouts.settle).await;
        let info = self.info().await;
        let frame = self.host.frame();
        Ok(WindowResult {
            window: windows::window_info(request.window, frame.as_ref(), info.as_ref()),
        })
    }

    async fn main_window_action(&self, request: &WindowRequest) -> StageResult<()> {
        let frame = self.host.frame();
        match request.action {
            WindowAction::Show => self.host.show(),
            WindowAction::Hide => self.host.hide(),
            WindowAction::Focus => {
                if !is_shown(Window::Main, frame.as_ref(), None) {
                    return Err(not_open(Window::Main));
                }
                self.host.focus();
            }
            WindowAction::Place => {
                let frame = frame
                    .filter(|f| f.visible)
                    .ok_or_else(|| not_open(Window::Main))?;
                if request.bounds.is_empty() {
                    return Err(StageError::bad(
                        "Give x, y, width or height to place the window.",
                    ));
                }
                self.host.place(request.bounds.over(frame.bounds));
            }
        }
        Ok(())
    }

    async fn sheet_action(&self, sheet: Window, request: &WindowRequest) -> StageResult<()> {
        match request.action {
            WindowAction::Focus => {
                let info = self.info().await;
                if !is_shown(sheet, self.host.frame().as_ref(), info.as_ref()) {
                    return Err(not_open(sheet));
                }
                self.host.focus();
                Ok(())
            }
            WindowAction::Place => Err(StageError::new(
                409,
                format!(
                    "The {} window places itself (a sheet centred in DataBee's window).",
                    sheet.as_str()
                ),
            )),
            WindowAction::Show => {
                if sheet == Window::Settings
                    && let Some(section) = &request.section
                {
                    self.check_section(sheet, section).await?;
                }
                if !self.host.frame().is_some_and(|f| f.visible) {
                    self.host.show();
                }
                self.toggle_sheet(sheet, SheetAction::Show, request.section.clone())
                    .await
            }
            WindowAction::Hide => self.toggle_sheet(sheet, SheetAction::Hide, None).await,
        }
    }

    async fn check_section(&self, sheet: Window, section: &str) -> StageResult<()> {
        let info = self.info().await;
        let known = info
            .as_ref()
            .and_then(|i| i.surface(sheet))
            .and_then(|s| s.sections.as_ref())
            .is_some_and(|all| all.iter().any(|s| s.id == section));
        if known {
            return Ok(());
        }
        Err(StageError::new(
            404,
            format!("Unknown section “{section}”; windows[].sections lists them."),
        ))
    }

    /// Opens or closes a sheet through the page.
    async fn toggle_sheet(
        &self,
        window: Window,
        action: SheetAction,
        section: Option<String>,
    ) -> StageResult<()> {
        let act = ActRequest::Window {
            window,
            action,
            section,
        };
        let reply = self.act_on_page(act).await.ok_or_else(no_answer)?;
        succeeded(reply).map(|_| ())
    }

    async fn act(&self, params: &Params) -> StageResult<ActResult> {
        let kind: ActKind = params
            .get("kind")
            .map(|k| k.trim())
            .unwrap_or_default()
            .parse()?;
        match kind {
            ActKind::Command => self.command(params).await,
            ActKind::Wait => self.wait(params).await,
            ActKind::Keys => self.keys(params).await,
            ActKind::Click => {
                self.on_anchor(params, |anchor| {
                    Ok(ActRequest::Click {
                        anchor: anchor.into(),
                    })
                })
                .await
            }
            ActKind::Reveal => {
                self.on_anchor(params, |anchor| {
                    Ok(ActRequest::Reveal {
                        anchor: anchor.into(),
                    })
                })
                .await
            }
            ActKind::Type => {
                self.on_anchor(params, |anchor| type_payload(anchor, params))
                    .await
            }
            ActKind::Scroll => {
                self.on_anchor(params, |anchor| scroll_payload(anchor, params))
                    .await
            }
        }
    }

    async fn command(&self, params: &Params) -> StageResult<ActResult> {
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
        let act = ActRequest::Command {
            command: command.into(),
        };
        let reply = self.act_on_page(act).await.ok_or_else(no_answer)?;
        succeeded(reply)?;
        Ok(ActResult::Command {
            command: command.into(),
        })
    }

    /// Builds the request only once a window shows, so a hidden app answers 404 before any 400.
    async fn on_anchor(
        &self,
        params: &Params,
        request: impl FnOnce(&str) -> StageResult<ActRequest>,
    ) -> StageResult<ActResult> {
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
        let act = request(anchor)?;
        let reply = self
            .act_on_page(act)
            .await
            .filter(|r| r.handled)
            .ok_or_else(not_shown)?;
        let reply = succeeded(reply)?;
        let frame = self.host.frame();
        Ok(ActResult::Anchor {
            anchor: anchor.into(),
            window: reply.window.unwrap_or_default(),
            rect: screen(reply.rect, frame.as_ref()),
            visible: screen(reply.visible, frame.as_ref()),
        })
    }

    async fn wait(&self, params: &Params) -> StageResult<ActResult> {
        let anchor = required(params, "anchor")?;
        let timeout = wait_timeout(params)?;
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
                return Ok(ActResult::Anchor {
                    anchor: anchor.into(),
                    window: found.window,
                    rect: Some(to_screen_rect(found.rect, frame.content)),
                    visible: screen(found.visible, Some(frame)),
                });
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

    /// A command's shortcut runs the command (the page resolves it like a real key press, in the
    /// current view); otherwise the focused element of the focused window gets the keys.
    async fn keys(&self, params: &Params) -> StageResult<ActResult> {
        let keys = parse_keys(required(params, "keys")?).ok_or_else(|| {
            StageError::bad("“keys” isn't a key combination (e.g. ⌘K, cmd+shift+k, Enter).")
        })?;
        let window = trimmed(params, "window")
            .map(Window::from_str)
            .transpose()?;
        let frame = self.host.frame();
        if let Some(window) = window {
            let info = match window {
                Window::Main => None,
                _ => self.info().await,
            };
            if !is_shown(window, frame.as_ref(), info.as_ref()) {
                return Err(not_open(window));
            }
            self.host.focus();
        }
        let no_focus = || {
            StageError::new(
                409,
                "No DataBee window has keyboard focus; pass window=main (or settings, palette).",
            )
        };
        if frame.is_none() {
            return Err(no_focus());
        }
        let act = ActRequest::Keys {
            keys,
            focused: window.is_some(),
        };
        let reply = self
            .act_on_page(act)
            .await
            .filter(|r| r.handled)
            .ok_or_else(no_focus)?;
        let reply = succeeded(reply)?;
        Ok(match reply.command {
            Some(command) => ActResult::Command { command },
            None => ActResult::Keys {
                window: reply.window.unwrap_or_default(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::stage::hub::ACT_EVENT;
    use crate::stage::page::PageAnchor;
    use crate::stage::test_host::{FakePage, anchor, frame, hub_with, page_rect};

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
            ..anchor(
                "settings-nav:queries",
                Window::Settings,
                page_rect(210.0, 0.0),
            )
        });
        let d = run(&hub, "describe", &[]).await.unwrap();
        assert_eq!(d["protocol"], 1);
        assert_eq!(d["features"], json!(FEATURES));
        assert_eq!(d["app"]["name"], "DataBee");
        assert_eq!(
            d["windows"][0],
            json!({
                "id": "main", "title": "DataBee", "visible": true, "focused": true, "movable": true,
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
                    Window::Palette,
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
}
