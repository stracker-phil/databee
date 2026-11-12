use std::str::FromStr;
use std::time::Duration;

use serde::Serialize;

use super::{no_answer, not_open, succeeded};
use crate::stage::hub::Hub;
use crate::stage::page::ActRequest;
use crate::stage::protocol::{
    Params, ScreenRect, StageError, StageResult, parse_keys, required, to_screen_rect,
};
use crate::stage::requests::{ActKind, scroll_payload, trimmed, type_payload, wait_timeout};
use crate::stage::windows::{Window, is_shown, screen};

const WAIT_POLL: Duration = Duration::from_millis(100);

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

impl Hub {
    pub(super) async fn act(&self, params: &Params) -> StageResult<ActResult> {
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
    use crate::stage::test_host::{FakePage, anchor, hub_with, page_rect};
    use crate::stage::verbs::tests::{last_act, run, status};

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
}
