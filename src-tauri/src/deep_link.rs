//! `databee://` links from other apps, queued like a launch so they wait for the page.

use tauri::{AppHandle, Manager};
use tauri_plugin_deep_link::DeepLinkExt;

use crate::address::Address;
use crate::connection::ConnectionSpec;
use crate::ipc::IpcMessage;
use crate::launch::queue_launches;
use crate::state::AppState;

pub fn listen(handle: &AppHandle) {
    let app = handle.clone();
    handle.deep_link().on_open_url(move |event| {
        for url in event.urls() {
            log::info!("Received a deep link");
            open(&app, url.as_str());
        }
    });
}

fn open(handle: &AppHandle, url: &str) {
    let state = handle.state::<AppState>();
    let saved_spec = |slug: &str| {
        state
            .recents
            .list()
            .into_iter()
            .find(|r| r.slug == slug)
            .map(|r| r.spec)
    };
    let messages = link_messages(url, &|slug| state.sessions.has_slug(slug), &saved_spec);
    queue_launches(handle, messages);
}

/// What opening `url` takes: the open, after a connect when only a saved connection has the
/// address's slug (`saved_spec`), or an error for the page to show.
fn link_messages(
    url: &str,
    is_open: &dyn Fn(&str) -> bool,
    saved_spec: &dyn Fn(&str) -> Option<ConnectionSpec>,
) -> Vec<IpcMessage> {
    let address = Address::parse(url);
    let Ok(address) = address else {
        return vec![IpcMessage::Error {
            message: address.unwrap_err().to_string(),
        }];
    };
    let open = IpcMessage::Open {
        address: address.to_string(),
        connection: None,
        focus: true,
    };
    if is_open(&address.connection) {
        return vec![open];
    }
    match saved_spec(&address.connection) {
        Some(spec) => vec![
            IpcMessage::Launch {
                spec,
                password: None,
                save_password: false,
            },
            open,
        ],
        None => vec![IpcMessage::Error {
            message: format!(
                "No open or saved connection is named {}",
                address.connection
            ),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ConnectionSpec {
        ConnectionSpec {
            name: Some("Shop".into()),
            host: "127.0.0.1".into(),
            port: 3306,
            socket: None,
            user: "root".into(),
            database: None,
        }
    }

    fn open_message(url: &str) -> IpcMessage {
        IpcMessage::Open {
            address: url.into(),
            connection: None,
            focus: true,
        }
    }

    #[test]
    fn a_url_that_is_not_an_address_is_an_error() {
        let messages = link_messages("https://example.com/", &|_| true, &|_| Some(spec()));

        assert_eq!(messages.len(), 1);
        assert!(matches!(messages[0], IpcMessage::Error { .. }));
    }

    #[test]
    fn an_open_slug_opens_the_url_unchanged_and_focused() {
        let url = "databee://shop/wp/wp_posts?post_status=publish";

        let messages = link_messages(url, &|slug| slug == "shop", &|_| None);

        assert_eq!(messages, vec![open_message(url)]);
    }

    #[test]
    fn a_saved_only_slug_connects_without_a_password_and_then_opens() {
        let url = "databee://shop/wp/";

        let messages = link_messages(url, &|_| false, &|slug| (slug == "shop").then(spec));

        assert_eq!(
            messages,
            vec![
                IpcMessage::Launch {
                    spec: spec(),
                    password: None,
                    save_password: false,
                },
                open_message(url),
            ]
        );
    }

    #[test]
    fn an_open_slug_wins_over_a_saved_one() {
        let url = "databee://shop/wp/";

        let messages = link_messages(url, &|_| true, &|_| Some(spec()));

        assert_eq!(messages, vec![open_message(url)]);
    }

    #[test]
    fn an_unknown_slug_is_an_error_naming_it() {
        let messages = link_messages("databee://nowhere/wp/", &|_| false, &|_| None);

        assert_eq!(
            messages,
            vec![IpcMessage::Error {
                message: "No open or saved connection is named nowhere".into(),
            }]
        );
    }

    #[test]
    fn a_percent_encoded_slug_is_compared_decoded() {
        let url = "databee://my%20shop/wp/";

        let messages = link_messages(url, &|slug| slug == "my shop", &|_| None);

        assert_eq!(messages, vec![open_message(url)]);
    }
}
