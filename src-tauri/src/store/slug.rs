//! Connection slugs: the stable, user-editable name an address starts with (`databee://<slug>/…`).

use std::path::Path;

use crate::connection::ConnectionSpec;
use crate::error::{AppError, AppResult};

const MAX_LEN: usize = 64;
/// For a spec whose parts all reduce to nothing (a name made only of symbols).
const FALLBACK: &str = "connection";

/// Lowercase `[a-z0-9-]`, runs of anything else collapsed into one `-`, no `-` at either end.
fn slugify(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let cut = &slug[..slug.len().min(MAX_LEN)];
    cut.trim_end_matches('-').to_string()
}

/// The `--name` given on launch, else `<host>-<port>-<database>` (a socket: `<file stem>-<database>`).
/// DDEV names every database `db`, so the endpoint is always part of the default.
pub fn default_slug(spec: &ConnectionSpec) -> String {
    let named = spec.name.as_deref().map(slugify).filter(|s| !s.is_empty());
    let slug = named.unwrap_or_else(|| {
        let endpoint = match &spec.socket {
            Some(socket) => Path::new(socket)
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| socket.clone()),
            None => format!("{}-{}", spec.host, spec.port),
        };
        let database = spec.database.as_deref().unwrap_or("");
        slugify(&format!("{endpoint}-{database}"))
    });
    if slug.is_empty() {
        FALLBACK.to_string()
    } else {
        slug
    }
}

/// `base`, or `base-2`, `base-3`, … when taken.
pub fn unique(base: &str, taken: &[&str]) -> String {
    if !taken.contains(&base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|candidate| !taken.contains(&candidate.as_str()))
        .expect("an unbounded range always has a free suffix")
}

/// A slug the user typed: already in slug form, not empty, at most 64 characters.
pub fn validate(slug: &str) -> AppResult<()> {
    if slug.is_empty() {
        return Err(AppError::InvalidInput(
            "The connection slug is empty".into(),
        ));
    }
    if slug.len() > MAX_LEN {
        return Err(AppError::InvalidInput(format!(
            "The connection slug is longer than {MAX_LEN} characters"
        )));
    }
    if slugify(slug) != slug {
        return Err(AppError::InvalidInput(format!(
            "`{slug}` is not a slug: use a-z, 0-9 and single hyphens between them"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn spec(
        name: Option<&str>,
        host: &str,
        socket: Option<&str>,
        db: Option<&str>,
    ) -> ConnectionSpec {
        ConnectionSpec {
            name: name.map(Into::into),
            host: host.into(),
            port: 3306,
            socket: socket.map(Into::into),
            user: "root".into(),
            database: db.map(Into::into),
        }
    }

    #[test]
    fn slugify_lowercases_replaces_symbols_collapses_runs_and_trims() {
        for (text, slug) in [
            ("My Site", "my-site"),
            ("WC_PP!!", "wc-pp"),
            ("a---b", "a-b"),
            ("a. /_b", "a-b"),
            ("  -x- ", "x"),
            ("Café au lait", "caf-au-lait"),
            ("127.0.0.1:3306/wp", "127-0-0-1-3306-wp"),
            ("!!!", ""),
            ("", ""),
        ] {
            assert_eq!(slugify(text), slug, "{text:?}");
        }
    }

    #[test]
    fn slugify_cuts_at_64_characters_without_a_trailing_hyphen() {
        assert_eq!(slugify(&"a".repeat(100)), "a".repeat(64));
        assert_eq!(slugify(&format!("{}-b", "a".repeat(63))), "a".repeat(63));
    }

    #[test]
    fn default_slug_prefers_the_slugified_name() {
        let named = spec(Some("My Site"), "127.0.0.1", None, Some("wp"));

        assert_eq!(default_slug(&named), "my-site");
    }

    #[test]
    fn default_slug_without_a_name_joins_host_port_and_database() {
        let plain = spec(None, "127.0.0.1", None, Some("wp"));
        let no_database = spec(None, "db.local", None, None);

        assert_eq!(default_slug(&plain), "127-0-0-1-3306-wp");
        assert_eq!(default_slug(&no_database), "db-local-3306");
    }

    #[test]
    fn default_slug_of_a_socket_uses_the_file_stem_and_the_database() {
        let socket = spec(None, "localhost", Some("/x/mysql.sock"), Some("db"));

        assert_eq!(default_slug(&socket), "mysql-db");
    }

    #[test]
    fn default_slug_of_a_symbol_only_name_falls_back_to_the_endpoint() {
        let symbols = spec(Some("!!!"), "127.0.0.1", None, Some("wp"));

        assert_eq!(default_slug(&symbols), "127-0-0-1-3306-wp");
    }

    #[test]
    fn default_slug_of_a_spec_reducing_to_nothing_is_connection() {
        let nothing = spec(None, "", Some("/"), None);

        assert_eq!(default_slug(&nothing), "connection");
    }

    #[test]
    fn unique_appends_the_first_free_number() {
        for (taken, expected) in [
            (vec![], "shop"),
            (vec!["other"], "shop"),
            (vec!["shop"], "shop-2"),
            (vec!["shop", "shop-2"], "shop-3"),
            (vec!["shop", "shop-3"], "shop-2"),
        ] {
            assert_eq!(unique("shop", &taken), expected, "{taken:?}");
        }
    }

    #[test]
    fn validate_accepts_slugs() {
        let longest = "a".repeat(64);
        for slug in ["a", "wc-pp", "127-0-0-1", longest.as_str()] {
            assert!(validate(slug).is_ok(), "{slug}");
        }
    }

    #[test]
    fn validate_rejects_empty_too_long_and_malformed_slugs() {
        let too_long = "a".repeat(65);
        for slug in [
            "",
            too_long.as_str(),
            "Upper",
            "a--b",
            "-a",
            "a-",
            "a b",
            "a_b",
        ] {
            let err = validate(slug).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{slug}");
        }
    }
}
