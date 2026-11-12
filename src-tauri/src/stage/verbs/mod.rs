//! The Stage hook's verbs (docs/stage.md): what a presenter may read and make DataBee do.
//! `http.rs` routes `/v1/stage/<verb>` here; status codes and messages are part of the protocol.

mod act;
mod window;

use serde::Serialize;
use serde_json::{Map, Value};

use super::hub::{AppInfo, Hub};
use super::page::{ActReply, PageCommand};
use super::protocol::{FEATURES, PROTOCOL, Params, StageError, StageResult};
use super::windows::{self, AnchorInfo, Window, WindowInfo};

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

pub(super) fn not_open(window: Window) -> StageError {
    StageError::new(
        409,
        format!("The {} window isn't open; show it first.", window.as_str()),
    )
}

pub(super) fn no_answer() -> StageError {
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

pub(super) fn succeeded(reply: ActReply) -> StageResult<ActReply> {
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
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::stage::hub::ACT_EVENT;
    use crate::stage::page::PageAnchor;
    use crate::stage::test_host::{FakePage, anchor, frame, hub_with, page_rect};

    pub(super) fn params(pairs: &[(&str, &str)]) -> Params {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    pub(super) async fn run(hub: &Hub, verb: &str, pairs: &[(&str, &str)]) -> StageResult<Value> {
        hub.run(verb, &params(pairs)).await.map(Value::Object)
    }

    pub(super) async fn status(hub: &Hub, verb: &str, pairs: &[(&str, &str)]) -> u16 {
        run(hub, verb, pairs).await.err().map_or(200, |e| e.status)
    }

    pub(super) fn last_act(page: &FakePage) -> Value {
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
    async fn remembers_the_last_presenter_request() {
        let (hub, _) = hub_with(FakePage::default());
        assert!(hub.last_request().is_none());
        run(&hub, "state", &[]).await.unwrap();
        assert!(hub.last_request().is_some());
    }
}
