//! The hub between the HTTP API and the page: it sends the page an event with a fresh request id
//! and waits, under a timeout, for the reply that carries the same id. The verbs are in
//! `verbs.rs`; the page side is src/stage/hooks.ts.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use super::page::{ActEvent, ActReply, ActRequest, AnchorsReply, InfoReply, PageAnchor};
use super::protocol::{MAIN, ScreenRect};
use super::windows::Frame;
use crate::lock::MutexExt;
use crate::pending::PendingReplies;

pub const COLLECT_EVENT: &str = "stage:collect";
pub const ACT_EVENT: &str = "stage:act";
pub const INFO_EVENT: &str = "stage:info";

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

/// The app as `describe` names it.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct AppInfo {
    pub name: String,
    pub id: String,
    pub version: &'static str,
}

#[derive(Serialize)]
struct Request<'a> {
    request: &'a str,
}

pub struct Hub {
    pub(super) host: Box<dyn StageHost>,
    pub(super) timeouts: Timeouts,
    pub(super) app: AppInfo,
    anchors: PendingReplies<AnchorsReply>,
    acts: PendingReplies<ActReply>,
    infos: PendingReplies<InfoReply>,
    last_request: Mutex<Option<chrono::DateTime<chrono::Utc>>>,
}

impl Hub {
    pub fn new(host: Box<dyn StageHost>, timeouts: Timeouts, name: &str, id: &str) -> Self {
        Self {
            host,
            timeouts,
            app: AppInfo {
                name: name.into(),
                id: id.into(),
                version: env!("CARGO_PKG_VERSION"),
            },
            anchors: PendingReplies::default(),
            acts: PendingReplies::default(),
            infos: PendingReplies::default(),
            last_request: Mutex::default(),
        }
    }

    /// When a presenter last used the hook (shown in Settings).
    pub fn last_request(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        *self.last_request.lock_unpoisoned()
    }

    pub(super) fn touch(&self) {
        *self.last_request.lock_unpoisoned() = Some(chrono::Utc::now());
    }

    /// Replies from other pages are ignored, like those for requests that timed out.
    pub fn receive_anchors(&self, reply: AnchorsReply) {
        if reply.page == MAIN {
            self.anchors.answer(&reply.request.clone(), reply.cleaned());
        }
    }

    pub fn receive_act(&self, reply: ActReply) {
        if reply.page == MAIN {
            self.acts.answer(&reply.request.clone(), reply);
        }
    }

    pub fn receive_info(&self, reply: InfoReply) {
        if reply.page == MAIN {
            self.infos.answer(&reply.request.clone(), reply);
        }
    }

    /// Sends `event` to the page and waits for its answer; None when it didn't answer in time.
    async fn ask<T>(
        &self,
        pending: &PendingReplies<T>,
        event: &str,
        act: Option<ActRequest>,
        wait: Duration,
    ) -> Option<T> {
        let sent = pending.ask(wait, |request| {
            let payload = match act {
                Some(act) => serde_json::to_value(ActEvent {
                    request: request.to_string(),
                    act,
                }),
                None => serde_json::to_value(Request { request }),
            }
            .inspect_err(|e| log::warn!("Stage couldn't encode {event}: {e}"))?;
            self.host.emit(event, payload);
            Ok::<_, serde_json::Error>(())
        });
        sent.await.ok()
    }

    pub(super) async fn info(&self) -> Option<InfoReply> {
        self.ask(&self.infos, INFO_EVENT, None, self.timeouts.collect)
            .await
    }

    /// The page's anchors while the window is on screen, in window, then reading order.
    pub(super) async fn page_anchors(&self, frame: Option<&Frame>) -> Vec<PageAnchor> {
        if !frame.is_some_and(|f| f.visible) {
            return Vec::new();
        }
        let Some(reply) = self
            .ask(&self.anchors, COLLECT_EVENT, None, self.timeouts.collect)
            .await
        else {
            return Vec::new();
        };
        let mut anchors = reply.anchors;
        anchors.sort_by(|a, b| {
            (a.window, a.rect.y, a.rect.x)
                .partial_cmp(&(b.window, b.rect.y, b.rect.x))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        anchors
    }

    pub(super) async fn act_on_page(&self, act: ActRequest) -> Option<ActReply> {
        self.ask(&self.acts, ACT_EVENT, Some(act), self.timeouts.act)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::test_host::{FakePage, hub_with};

    #[tokio::test]
    async fn replies_for_unknown_requests_or_pages_are_ignored() {
        let (hub, _) = hub_with(FakePage::default());
        hub.receive_act(ActReply {
            request: "unknown".into(),
            page: "main".into(),
            handled: true,
            ok: true,
            ..ActReply::default()
        });
        assert!(hub.acts.is_empty());
    }

    #[tokio::test]
    async fn a_reply_after_the_timeout_is_dropped() {
        let (hub, page) = hub_with(FakePage {
            silent: true,
            ..FakePage::default()
        });
        let reply = hub
            .act_on_page(ActRequest::Click {
                anchor: "statusbar:theme".into(),
            })
            .await;
        assert!(reply.is_none());

        let request = page.events.lock().unwrap()[0].1["request"]
            .as_str()
            .unwrap()
            .to_string();
        hub.receive_act(ActReply {
            request: request.clone(),
            page: "main".into(),
            handled: true,
            ok: true,
            ..ActReply::default()
        });
        assert!(hub.acts.is_empty());
    }
}
