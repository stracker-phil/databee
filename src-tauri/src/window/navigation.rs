//! Keeps the webview on the app's own pages: a link in database content or a stray script can't load another
//! page inside the app. Web links open in the browser instead (`commands::open_url`).

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Runtime, Url};

pub fn guard<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("navigation-guard")
        .on_navigation(|_, url| {
            let allowed = is_app_page(url);
            if !allowed {
                log::warn!("Blocked a navigation away from the app to {}", url.scheme());
            }
            allowed
        })
        .build()
}

/// The bundled pages (`tauri://localhost`), and in debug builds the dev server.
pub fn is_app_page(url: &Url) -> bool {
    let host = url.host_str();
    match url.scheme() {
        "tauri" => host == Some("localhost"),
        "https" => host == Some("tauri.localhost"),
        "http" => {
            host == Some("tauri.localhost")
                || (cfg!(debug_assertions) && host == Some("localhost") && url.port() == Some(1420))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(url: &str) -> bool {
        is_app_page(&Url::parse(url).expect(url))
    }

    #[test]
    fn the_bundled_pages_are_app_pages() {
        for url in [
            "tauri://localhost",
            "tauri://localhost/",
            "tauri://localhost/index.html",
            "http://tauri.localhost/",
            "https://tauri.localhost/",
            "https://tauri.localhost/settings.html?tab=general#top",
        ] {
            assert!(allowed(url), "{url}");
        }
    }

    #[test]
    fn the_vite_dev_server_is_an_app_page_in_the_debug_build() {
        assert!(cfg!(debug_assertions));
        for url in ["http://localhost:1420", "http://localhost:1420/index.html"] {
            assert!(allowed(url), "{url}");
        }
    }

    #[test]
    fn other_pages_are_refused() {
        for url in [
            "https://example.com",
            "https://example.com/",
            "http://localhost:3000",
            "http://localhost",
            "http://localhost:1421/",
            "http://127.0.0.1:1420/",
            "tauri://example.com",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,<h1>hi</h1>",
            "about:blank",
        ] {
            assert!(!allowed(url), "{url}");
        }
    }

    #[test]
    fn lookalike_hosts_and_userinfo_tricks_are_refused() {
        for url in [
            "https://tauri.localhost.example.com/",
            "https://eviltauri.localhost/",
            "https://tauri.localhost@example.com/",
            "https://tauri.localhost:pass@example.com/",
            "http://localhost:1420@example.com/",
            "http://localhost.example.com:1420/",
            "tauri://localhost.example.com/",
        ] {
            assert!(!allowed(url), "{url}");
        }
    }
}
