//! A fake host for the hub's tests: a page that answers like src/stage/hooks.ts would, and a
//! window that can be moved.

use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

use serde_json::{Value, json};

use super::hub::{COLLECT_EVENT, Hub, INFO_EVENT, StageHost, Timeouts};
use super::page::{ActReply, AnchorsReply, Choice, InfoReply, PageAnchor, PageCommand, Surface};
use super::protocol::{MAIN, PageRect, ScreenRect};
use super::windows::{Frame, Window};

pub struct FakePage {
    pub hub: OnceLock<Weak<Hub>>,
    pub frame: Mutex<Option<Frame>>,
    pub anchors: Mutex<Vec<PageAnchor>>,
    pub commands: Vec<PageCommand>,
    /// Which sheet is open, and on which section.
    pub sheet: Mutex<Option<(Window, Option<String>)>>,
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
        title: "DataBee".into(),
        visible,
        focused,
        bounds: rect,
        content: rect,
    }
}

pub fn anchor(id: &str, window: Window, rect: PageRect) -> PageAnchor {
    PageAnchor {
        id: id.into(),
        label: id.into(),
        rect,
        in_view: true,
        visible: Some(rect),
        window,
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
    let ok = |window: Window| ActReply {
        handled: true,
        ok: true,
        window: Some(window),
        rect: Some(page_rect(10.0, 20.0)),
        visible: Some(page_rect(10.0, 20.0)),
        ..ActReply::default()
    };
    match (p["kind"].as_str(), p["anchor"].as_str()) {
        (Some("keys"), _) if p["keys"]["key"] == "," => ActReply {
            command: Some("app:settings".into()),
            ..ok(Window::Main)
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
        (_, Some("settings:queries")) => ok(Window::Settings),
        (_, Some("nope")) => ActReply::default(),
        _ => ok(Window::Main),
    }
}

impl Default for FakePage {
    fn default() -> Self {
        Self {
            hub: OnceLock::new(),
            frame: Mutex::new(Some(frame(true, true))),
            anchors: Mutex::new(vec![
                anchor("statusbar:theme", Window::Main, page_rect(900.0, 780.0)),
                anchor("toolbar:settings", Window::Main, page_rect(1100.0, 8.4)),
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
        let surface = |id: Window, title: &str| Surface {
            id,
            title: title.into(),
            rect: sheet
                .as_ref()
                .filter(|(open, _)| *open == id)
                .map(|_| PageRect {
                    x: 200.0,
                    y: 100.0,
                    width: 700.0,
                    height: 600.0,
                }),
            sections: (id == Window::Settings).then(|| {
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
                surface(Window::Settings, "Settings"),
                surface(Window::Palette, "Command Palette"),
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
        let mut current = self.frame.lock().unwrap();
        *current = Some(Frame {
            visible: true,
            ..current.clone().unwrap_or_else(|| frame(true, false))
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
                let window: Window = payload["window"].as_str().unwrap().parse().unwrap();
                *self.sheet.lock().unwrap() = (payload["action"] == "show")
                    .then(|| (window, payload["section"].as_str().map(String::from)));
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
        "DataBee",
        "com.test",
    ));
    page.hub.set(Arc::downgrade(&hub)).unwrap();
    (hub, page)
}
