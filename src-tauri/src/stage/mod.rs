//! The Stage hook (docs/stage.md): a local HTTP API through which Stage, a separate presentation
//! app, reads DataBee's window, anchors and commands and makes it act. The user registers the
//! hook with Stage once (`stage register <url> --token <token>`); the token lives in `stage.json`
//! (`settings.rs`).

pub mod commands;
pub mod hooks;
pub mod http;
pub mod protocol;
pub mod settings;

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Emitter, Manager, RunEvent, Wry};
use ts_rs::TS;

use crate::error::AppResult;
use crate::lock::MutexExt;
use hooks::{Frame, Hub, StageHost, Timeouts};
use protocol::{MAIN, ScreenRect, register_command, stage_url};
use settings::{StageSettings, StageStore};

pub const NAME: &str = "DataBee";

fn logged<T>(call: &str, result: tauri::Result<T>) -> Option<T> {
    result
        .inspect_err(|e| log::warn!("Stage can't read the window ({call}): {e}"))
        .ok()
}

/// The main window through Tauri, in logical points (macOS points = CSS px).
struct TauriHost(AppHandle);

impl TauriHost {
    fn window(&self) -> Option<tauri::WebviewWindow> {
        self.0.get_webview_window(MAIN)
    }
}

fn logical(
    position: tauri::PhysicalPosition<i32>,
    size: tauri::PhysicalSize<u32>,
    scale: f64,
) -> ScreenRect {
    let p = position.to_logical::<f64>(scale);
    let s = size.to_logical::<f64>(scale);
    ScreenRect {
        x: p.x.round() as i64,
        y: p.y.round() as i64,
        width: s.width.round() as i64,
        height: s.height.round() as i64,
    }
}

impl StageHost for TauriHost {
    /// The hub reports a missing frame as "no window has focus", so the failing call is logged.
    fn frame(&self) -> Option<Frame> {
        let w = self.window()?;
        let scale = logged("scale_factor", w.scale_factor())?;
        let visible = w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false);
        let bounds = logical(
            logged("outer_position", w.outer_position())?,
            logged("outer_size", w.outer_size())?,
            scale,
        );
        let content = logical(
            logged("inner_position", w.inner_position())?,
            logged("inner_size", w.inner_size())?,
            scale,
        );
        Some(Frame {
            title: w.title().unwrap_or_else(|_| NAME.into()),
            visible,
            focused: w.is_focused().unwrap_or(false),
            bounds,
            content,
        })
    }

    fn show(&self) {
        if let Some(w) = self.window() {
            let _ = w.unminimize();
            let _ = w.show();
        }
    }

    fn hide(&self) {
        if let Some(w) = self.window() {
            let _ = w.hide();
        }
    }

    fn focus(&self) {
        if let Some(w) = self.window() {
            let _ = w.set_focus();
        }
    }

    fn place(&self, rect: ScreenRect) {
        if let Some(w) = self.window() {
            let _ = w.set_position(tauri::LogicalPosition::new(rect.x as f64, rect.y as f64));
            let _ = w.set_size(tauri::LogicalSize::new(
                rect.width as f64,
                rect.height as f64,
            ));
        }
    }

    fn emit(&self, event: &str, payload: serde_json::Value) {
        let _ = self.0.emit_to(MAIN, event, payload);
    }
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct StageStatus {
    pub enabled: bool,
    pub running: bool,
    pub error: Option<String>,
    pub url: String,
    /// `stage register …` with this hook's URL and token, for the user to paste into a terminal.
    pub register_command: String,
    /// When a presenter last used the hook (ISO 8601).
    pub last_request: Option<String>,
}

/// The running API; its settings are read from the store.
#[derive(Default)]
struct Server {
    task: Option<tauri::async_runtime::JoinHandle<()>>,
    error: Option<String>,
}

/// The API server, the hub its requests go to and the settings; the plugin's managed state.
pub struct Stage {
    pub hub: Arc<Hub>,
    store: StageStore,
    server: Mutex<Server>,
}

/// Installs Stage: its commands, its settings file in `config_dir` and the API server.
pub fn init(config_dir: &std::path::Path) -> TauriPlugin<Wry> {
    let config_dir = config_dir.to_path_buf();
    tauri::plugin::Builder::new("stage")
        .invoke_handler(tauri::generate_handler![
            commands::stage_anchors,
            commands::stage_done,
            commands::stage_info,
            commands::stage_status,
            commands::stage_settings,
            commands::stage_update_settings,
            commands::stage_new_token,
        ])
        .setup(move |app, _| {
            let id = app.config().identifier.clone();
            let stage = Stage::new(app, &id, StageStore::open(&config_dir));
            stage.start();
            app.manage(stage);
            Ok(())
        })
        .on_event(|app, event| {
            if let RunEvent::Exit = event {
                app.state::<Stage>().stop();
            }
        })
        .build()
}

/// An aborted server task releases its socket on the async runtime, a moment after `stop`,
/// so a restart retries briefly while the port is still taken.
fn bind(port: u16, restarting: bool) -> std::io::Result<std::net::TcpListener> {
    const RETRIES: u32 = 20;
    let mut attempt = 0;
    loop {
        let bound = std::net::TcpListener::bind(("127.0.0.1", port))
            .and_then(|l| l.set_nonblocking(true).map(|()| l));
        match bound {
            Err(e)
                if restarting && e.kind() == std::io::ErrorKind::AddrInUse && attempt < RETRIES =>
            {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            other => return other,
        }
    }
}

impl Stage {
    pub fn new(app: &AppHandle, id: &str, store: StageStore) -> Self {
        Self {
            hub: Arc::new(Hub::new(
                Box::new(TauriHost(app.clone())),
                Timeouts::default(),
                NAME,
                id,
            )),
            store,
            server: Mutex::default(),
        }
    }

    pub fn settings(&self) -> StageSettings {
        self.store.get()
    }

    /// Saves the settings and restarts the API when they changed.
    pub fn update_settings(&self, next: StageSettings) -> AppResult<StageSettings> {
        let (previous, saved) = self.store.update(next)?;
        if saved != previous {
            self.start();
        }
        Ok(saved)
    }

    pub fn rotate_token(&self) -> AppResult<StageSettings> {
        let saved = self.store.rotate_token()?;
        self.start();
        log::info!("Stage API token replaced");
        Ok(saved)
    }

    /// (Re)starts the API with the stored settings; off means stopped.
    pub fn start(&self) {
        let restarting = self.stop();
        let settings = self.store.get();
        let mut server = self.server.lock_unpoisoned();
        if !settings.enabled {
            return;
        }
        let listener = bind(settings.port, restarting);
        let listener = match listener {
            Ok(l) => l,
            Err(e) => {
                let message = if e.kind() == std::io::ErrorKind::AddrInUse {
                    format!("Port {} is already used by another app.", settings.port)
                } else {
                    e.to_string()
                };
                log::warn!("The Stage API couldn't start: {message}");
                server.error = Some(message);
                return;
            }
        };
        let hub = self.hub.clone();
        let token: Arc<str> = Arc::from(settings.token.as_str());
        server.task = Some(tauri::async_runtime::spawn(async move {
            match tokio::net::TcpListener::from_std(listener) {
                Ok(listener) => http::serve(listener, token, hub).await,
                Err(e) => log::warn!("The Stage API couldn't listen: {e}"),
            }
        }));
        log::info!("Stage API listening on {}", stage_url(settings.port));
    }

    /// Returns whether a server was running.
    pub fn stop(&self) -> bool {
        let mut server = self.server.lock_unpoisoned();
        server.error = None;
        let Some(task) = server.task.take() else {
            return false;
        };
        task.abort();
        true
    }

    pub fn status(&self) -> StageStatus {
        let settings = self.store.get();
        let server = self.server.lock_unpoisoned();
        StageStatus {
            enabled: settings.enabled,
            running: server.task.is_some(),
            error: server.error.clone(),
            url: stage_url(settings.port),
            register_command: register_command(settings.port, &settings.token),
            last_request: self
                .hub
                .last_request()
                .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
        }
    }
}
