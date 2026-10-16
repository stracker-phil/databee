//! The Stage hook (docs/stage.md): a local HTTP API through which Stage, a separate
//! presentation app, reads databee's window, anchors and commands and makes it act. While the
//! API listens, a discovery file tells Stage where it is.

pub mod discovery;
pub mod hooks;
pub mod http;
pub mod protocol;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::store::settings::StageSettings;
use hooks::{Frame, Hub, StageHost, Timeouts};
use protocol::{MAIN, ScreenRect, stage_url};

pub const NAME: &str = "databee";

/// 256 random bits as hex; generated on first start and on "New token".
pub fn new_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
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
    fn frame(&self) -> Option<Frame> {
        let w = self.window()?;
        let scale = w.scale_factor().ok()?;
        let visible = w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false);
        Some(Frame {
            title: w.title().unwrap_or_else(|_| NAME.into()),
            visible,
            focused: w.is_focused().unwrap_or(false),
            bounds: logical(w.outer_position().ok()?, w.outer_size().ok()?, scale),
            content: logical(w.inner_position().ok()?, w.inner_size().ok()?, scale),
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

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StageStatus {
    pub enabled: bool,
    pub running: bool,
    pub error: Option<String>,
    pub url: String,
    /// Set while the discovery file exists.
    pub discovery_file: Option<String>,
    /// When a presenter last used the hook (ISO 8601).
    pub last_request: Option<String>,
}

#[derive(Default)]
struct Server {
    task: Option<tauri::async_runtime::JoinHandle<()>>,
    enabled: bool,
    port: u16,
    error: Option<String>,
}

/// The API server and the hub its requests go to; managed Tauri state.
pub struct Stage {
    pub hub: Arc<Hub>,
    file: PathBuf,
    server: Mutex<Server>,
}

impl Stage {
    pub fn new(app: &AppHandle, id: &str) -> Self {
        Self {
            hub: Arc::new(Hub::new(
                Box::new(TauriHost(app.clone())),
                Timeouts::default(),
                NAME,
                id,
            )),
            file: discovery::path_in(&discovery::folder(), id),
            server: Mutex::default(),
        }
    }

    /// (Re)starts the API with these settings; off means stopped.
    pub fn start(&self, settings: &StageSettings) {
        self.stop();
        let mut server = self.server.lock().unwrap();
        server.enabled = settings.enabled;
        server.port = settings.port;
        if !settings.enabled {
            return;
        }
        let listener = std::net::TcpListener::bind(("127.0.0.1", settings.port))
            .and_then(|l| l.set_nonblocking(true).map(|()| l));
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
        if let Err(e) = discovery::publish(
            &self.file,
            NAME,
            &self.hub_id(),
            settings.port,
            &settings.token,
        ) {
            log::warn!("Couldn't write the Stage discovery file: {e}");
        }
    }

    fn hub_id(&self) -> String {
        self.file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    pub fn stop(&self) {
        let mut server = self.server.lock().unwrap();
        if let Some(task) = server.task.take() {
            task.abort();
        }
        server.error = None;
        discovery::remove(&self.file);
    }

    /// Removes the discovery file; the process is quitting.
    pub fn quit(&self) {
        discovery::remove(&self.file);
    }

    pub fn status(&self) -> StageStatus {
        let server = self.server.lock().unwrap();
        StageStatus {
            enabled: server.enabled,
            running: server.task.is_some(),
            error: server.error.clone(),
            url: stage_url(server.port),
            discovery_file: self.file.exists().then(|| self.file.display().to_string()),
            last_request: self
                .hub
                .last_request()
                .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
        }
    }

    pub fn discovery_path(&self) -> &std::path::Path {
        &self.file
    }
}
