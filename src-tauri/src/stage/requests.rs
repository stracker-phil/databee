//! The verbs' requests, parsed and validated before anything reaches the page: the `act` kind,
//! the `window` request and the parameters of `type`, `scroll` and `wait`.

use std::str::FromStr;

use super::page::{ActRequest, ScrollTo, TypeMode};
use super::protocol::{Bounds, Params, StageError, StageResult, parse_bounds};
use super::windows::Window;

const WAIT_DEFAULT_MS: u64 = 2000;
const WAIT_MAX_MS: u64 = 5000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActKind {
    Click,
    Type,
    Reveal,
    Scroll,
    Wait,
    Keys,
    Command,
}

impl FromStr for ActKind {
    type Err = StageError;

    fn from_str(kind: &str) -> Result<Self, Self::Err> {
        Ok(match kind {
            "click" => ActKind::Click,
            "type" => ActKind::Type,
            "reveal" => ActKind::Reveal,
            "scroll" => ActKind::Scroll,
            "wait" => ActKind::Wait,
            "keys" => ActKind::Keys,
            "command" => ActKind::Command,
            _ => {
                return Err(StageError::bad(
                    "“kind” must be click, type, reveal, scroll, wait, keys or command.",
                ));
            }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction {
    Show,
    Hide,
    Focus,
    Place,
}

impl FromStr for WindowAction {
    type Err = StageError;

    fn from_str(action: &str) -> Result<Self, Self::Err> {
        Ok(match action {
            "show" => WindowAction::Show,
            "hide" => WindowAction::Hide,
            "focus" => WindowAction::Focus,
            "place" => WindowAction::Place,
            _ => {
                return Err(StageError::bad(
                    "“action” must be show, hide, focus or place.",
                ));
            }
        })
    }
}

/// A `window` request, validated: without `action`, bounds mean place and none mean show.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowRequest {
    pub window: Window,
    pub action: WindowAction,
    pub bounds: Bounds,
    pub section: Option<String>,
}

pub fn trimmed<'a>(params: &'a Params, key: &str) -> Option<&'a str> {
    params.get(key).map(|v| v.trim()).filter(|v| !v.is_empty())
}

impl WindowRequest {
    pub fn parse(params: &Params) -> StageResult<Self> {
        let window = params
            .get("window")
            .map(|w| w.trim())
            .unwrap_or_default()
            .parse()?;
        let bounds = parse_bounds(params)?;
        let action = match trimmed(params, "action") {
            Some(action) => action.parse()?,
            None if !bounds.is_empty() => WindowAction::Place,
            None => WindowAction::Show,
        };
        Ok(Self {
            window,
            action,
            bounds,
            section: trimmed(params, "section").map(String::from),
        })
    }
}

/// `type` on `anchor`: the text (empty when missing) and the mode, replace unless append.
pub fn type_payload(anchor: &str, params: &Params) -> StageResult<ActRequest> {
    let mode = match trimmed(params, "mode") {
        None | Some("replace") => TypeMode::Replace,
        Some("append") => TypeMode::Append,
        Some(_) => return Err(StageError::bad("“mode” must be replace or append.")),
    };
    Ok(ActRequest::Type {
        anchor: anchor.into(),
        text: params.get("text").cloned().unwrap_or_default(),
        mode,
    })
}

/// `scroll` on `anchor`: `to` (top or bottom) wins over `by` (points, positive scrolls down).
pub fn scroll_payload(anchor: &str, params: &Params) -> StageResult<ActRequest> {
    let scroll = |by, to| ActRequest::Scroll {
        anchor: anchor.into(),
        by,
        to,
    };
    let to = match params.get("to").map(|t| t.trim()) {
        Some("top") => Some(ScrollTo::Top),
        Some("bottom") => Some(ScrollTo::Bottom),
        Some(_) => return Err(StageError::bad("“to” must be top or bottom.")),
        None => None,
    };
    if to.is_some() {
        return Ok(scroll(None, to));
    }
    let Some(by) = params.get("by") else {
        return Err(StageError::bad(
            "Give “by” (points, positive scrolls down) or “to” (top or bottom).",
        ));
    };
    let by = by
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| StageError::bad("“by” must be a number of points."))?;
    Ok(scroll(Some(by), None))
}

/// How long `wait` waits, in milliseconds: 2 s unless given, at most 5 s.
pub fn wait_timeout(params: &Params) -> StageResult<u64> {
    let Some(raw) = trimmed(params, "timeout") else {
        return Ok(WAIT_DEFAULT_MS);
    };
    let ms = raw
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .ok_or_else(|| StageError::bad("“timeout” must be milliseconds."))?;
    Ok(ms.min(WAIT_MAX_MS as f64) as u64)
}
