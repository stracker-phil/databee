//! The window model Stage sees. DataBee has one native window; Settings and the command palette
//! are sheets inside it, exposed as the windows `settings` and `palette` and measured by the page.

use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

use super::page::{Choice, InfoReply, PageAnchor};
use super::protocol::{MAIN, PageRect, ScreenRect, StageError, to_screen_rect};

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

/// The windows the Stage hook exposes; settings and the palette are sheets in the main one.
/// Declared in describe order, which `Ord` follows.
#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, TS,
)]
#[ts(export, export_to = "src/stage/types.gen.ts")]
#[ts(rename = "StageWindow")]
#[serde(rename_all = "lowercase")]
pub enum Window {
    #[default]
    Main,
    Settings,
    Palette,
}

impl Window {
    /// In `describe` order.
    pub const ALL: [Window; 3] = [Window::Main, Window::Settings, Window::Palette];

    pub fn as_str(self) -> &'static str {
        match self {
            Window::Main => MAIN,
            Window::Settings => "settings",
            Window::Palette => "palette",
        }
    }

    /// The title while the page hasn't reported one.
    fn fallback_title(self) -> &'static str {
        match self {
            Window::Main => "DataBee",
            Window::Settings => "Settings",
            Window::Palette => "Command Palette",
        }
    }
}

impl FromStr for Window {
    type Err = StageError;

    fn from_str(id: &str) -> Result<Self, Self::Err> {
        Window::ALL
            .into_iter()
            .find(|w| w.as_str() == id)
            .ok_or_else(|| {
                let all: Vec<_> = Window::ALL.iter().map(|w| w.as_str()).collect();
                StageError::new(404, format!("“window” must be one of {}.", all.join(", ")))
            })
    }
}

/// On screen: the main window, or a sheet the page reports open in the shown main window.
pub fn is_shown(window: Window, frame: Option<&Frame>, info: Option<&InfoReply>) -> bool {
    let main = frame.is_some_and(|f| f.visible);
    if window == Window::Main {
        return main;
    }
    main && info
        .and_then(|i| i.surface(window))
        .is_some_and(|s| s.rect.is_some())
}

/// A page rectangle in screen points; None without one or without a window to place it in.
pub fn screen(rect: Option<PageRect>, frame: Option<&Frame>) -> Option<ScreenRect> {
    Some(to_screen_rect(rect?, frame?.content))
}

/// A sheet's pages; `section` is the one shown, None while the sheet is closed.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Sections {
    pub sections: Vec<Choice>,
    pub section: Option<String>,
}

/// One window in `describe` and the `window` verb's answer.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct WindowInfo {
    pub id: Window,
    pub title: String,
    pub visible: bool,
    pub focused: bool,
    pub movable: bool,
    pub bounds: Option<ScreenRect>,
    pub content: Option<ScreenRect>,
    pub view: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui: Option<Map<String, Value>>,
    #[serde(flatten)]
    pub sections: Option<Sections>,
}

fn main_info(frame: Option<&Frame>, info: Option<&InfoReply>) -> WindowInfo {
    let shown = frame.filter(|f| f.visible);
    // A sheet on top takes the keys.
    let sheet_open = [Window::Settings, Window::Palette]
        .into_iter()
        .any(|w| is_shown(w, frame, info));
    WindowInfo {
        id: Window::Main,
        title: frame
            .map_or(Window::Main.fallback_title(), |f| f.title.as_str())
            .into(),
        visible: shown.is_some(),
        focused: shown.is_some_and(|f| f.focused) && !sheet_open,
        movable: true,
        bounds: shown.map(|f| f.bounds),
        content: shown.map(|f| f.content),
        view: shown.and(info.and_then(|i| i.view.clone())),
        ui: info.map(|i| i.ui.clone()),
        sections: None,
    }
}

fn sheet_info(window: Window, frame: Option<&Frame>, info: Option<&InfoReply>) -> WindowInfo {
    let surface = info.and_then(|i| i.surface(window));
    let shown = is_shown(window, frame, info);
    let rect = surface
        .filter(|_| shown)
        .and_then(|s| screen(s.rect, frame));
    // The palette sits above Settings, so it takes the keys while both are open.
    let below_palette = window != Window::Palette && is_shown(Window::Palette, frame, info);
    WindowInfo {
        id: window,
        title: surface
            .map_or(window.fallback_title(), |s| s.title.as_str())
            .into(),
        visible: shown,
        focused: shown && frame.is_some_and(|f| f.focused) && !below_palette,
        // Sheets centre themselves in the main window.
        movable: false,
        bounds: rect,
        content: rect,
        view: None,
        ui: None,
        sections: surface.and_then(|s| {
            Some(Sections {
                sections: s.sections.clone()?,
                section: s.section.clone().filter(|_| shown),
            })
        }),
    }
}

pub fn window_info(window: Window, frame: Option<&Frame>, info: Option<&InfoReply>) -> WindowInfo {
    match window {
        Window::Main => main_info(frame, info),
        sheet => sheet_info(sheet, frame, info),
    }
}

/// Every window, in `describe` order.
pub fn windows(frame: Option<&Frame>, info: Option<&InfoReply>) -> Vec<WindowInfo> {
    Window::ALL
        .into_iter()
        .map(|w| window_info(w, frame, info))
        .collect()
}

/// One anchor in `describe`, in screen points.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AnchorInfo {
    pub id: String,
    pub label: String,
    pub window: Window,
    pub rect: ScreenRect,
    pub in_view: bool,
    pub visible: Option<ScreenRect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expanded: Option<bool>,
    /// Only `true` is reported; enabled is the default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

impl AnchorInfo {
    pub fn new(anchor: &PageAnchor, frame: &Frame) -> Self {
        Self {
            id: anchor.id.clone(),
            label: anchor.label.clone(),
            window: anchor.window,
            rect: to_screen_rect(anchor.rect, frame.content),
            in_view: anchor.in_view,
            visible: screen(anchor.visible, Some(frame)),
            role: anchor.role.clone(),
            list: anchor.list.clone(),
            selected: anchor.selected,
            checked: anchor.checked,
            expanded: anchor.expanded,
            disabled: anchor.disabled.filter(|d| *d),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_window_round_trips_through_its_name() {
        for window in Window::ALL {
            assert_eq!(window.as_str().parse::<Window>(), Ok(window), "{window:?}");
        }
        let names: Vec<_> = Window::ALL.iter().map(|w| w.as_str()).collect();
        assert_eq!(names, ["main", "settings", "palette"]);
    }

    #[test]
    fn unknown_window_name_is_a_404_listing_the_valid_ones() {
        for name in ["sidebar", "", "Main"] {
            assert_eq!(
                name.parse::<Window>(),
                Err(StageError::new(
                    404,
                    "“window” must be one of main, settings, palette."
                )),
                "{name:?}"
            );
        }
    }
}
