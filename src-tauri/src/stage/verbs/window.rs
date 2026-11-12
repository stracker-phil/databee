use serde::Serialize;

use super::{no_answer, not_open, succeeded};
use crate::stage::hub::Hub;
use crate::stage::page::{ActRequest, SheetAction};
use crate::stage::protocol::{Params, StageError, StageResult};
use crate::stage::requests::{WindowAction, WindowRequest};
use crate::stage::windows::{self, Window, WindowInfo, is_shown};

#[derive(Serialize)]
pub(super) struct WindowResult {
    window: WindowInfo,
}

impl Hub {
    pub(super) async fn window(&self, params: &Params) -> StageResult<WindowResult> {
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
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::stage::test_host::{FakePage, hub_with};
    use crate::stage::verbs::tests::{last_act, run, status};

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
}
