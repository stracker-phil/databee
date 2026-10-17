use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::json_store::JsonStore;
use super::slug;
use crate::connection::ConnectionSpec;
use crate::error::{AppError, AppResult};

const MAX_RECENTS: usize = 20;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct RecentConnection {
    pub identity: String,
    /// Derived from `spec` when listed; older files lack it.
    #[serde(default)]
    pub account: String,
    pub spec: ConnectionSpec,
    pub last_used: DateTime<Utc>,
    /// The connection part of an address, unique within the store (filled on open for older files).
    #[serde(default)]
    pub slug: String,
}

/// Gives every entry without a slug its default one, unique among all entries.
fn fill_slugs(mut recents: Vec<RecentConnection>) -> Vec<RecentConnection> {
    for i in 0..recents.len() {
        if !recents[i].slug.is_empty() {
            continue;
        }
        let taken: Vec<&str> = recents.iter().map(|r| r.slug.as_str()).collect();
        let slug = slug::unique(&slug::default_slug(&recents[i].spec), &taken);
        recents[i].slug = slug;
    }
    recents
}

fn taken_slugs<'a>(items: &'a [RecentConnection], except_identity: &str) -> Vec<&'a str> {
    items
        .iter()
        .filter(|r| r.identity != except_identity)
        .map(|r| r.slug.as_str())
        .collect()
}

/// The slug a known connection has, else its default, unique among the others.
fn resolve_slug(items: &[RecentConnection], spec: &ConnectionSpec, identity: &str) -> String {
    match items.iter().find(|r| r.identity == identity) {
        Some(known) => known.slug.clone(),
        None => slug::unique(&slug::default_slug(spec), &taken_slugs(items, identity)),
    }
}

pub struct RecentsStore {
    store: JsonStore<Vec<RecentConnection>>,
}

impl RecentsStore {
    pub fn open(dir: &Path) -> Self {
        Self {
            // The filled slugs are saved with the next change; until then they are derived the same way.
            store: JsonStore::open_with(dir.join("connections.json"), fill_slugs),
        }
    }

    /// Most recently used first.
    pub fn list(&self) -> Vec<RecentConnection> {
        let mut recents = self.store.read(Vec::clone);
        for recent in &mut recents {
            recent.account = recent.spec.account();
        }
        recents
    }

    /// Moves the connection to the top; returns its slug, which a known connection keeps.
    pub async fn touch(&self, spec: &ConnectionSpec) -> AppResult<String> {
        let spec = spec.clone();
        self.store
            .update_async(move |items| {
                let identity = spec.identity();
                let slug = resolve_slug(items, &spec, &identity);
                items.retain(|r| r.identity != identity);
                items.insert(
                    0,
                    RecentConnection {
                        identity,
                        account: spec.account(),
                        spec,
                        last_used: Utc::now(),
                        slug: slug.clone(),
                    },
                );
                items.truncate(MAX_RECENTS);
                slug
            })
            .await
    }

    /// Validates `slug` and stores it for the connection `identity`.
    pub async fn rename_slug(&self, identity: &str, slug: &str) -> AppResult<()> {
        slug::validate(slug)?;
        let identity = identity.to_string();
        let slug = slug.to_string();
        self.store
            .update_async(move |items| {
                if taken_slugs(items, &identity).contains(&slug.as_str()) {
                    return Err(AppError::InvalidInput(format!(
                        "Another connection is already named `{slug}`"
                    )));
                }
                let entry = items
                    .iter_mut()
                    .find(|r| r.identity == identity)
                    .ok_or_else(|| {
                        AppError::InvalidInput(format!("No saved connection {identity}"))
                    })?;
                entry.slug = slug;
                Ok(())
            })
            .await?
    }

    pub async fn remove(&self, identity: &str) -> AppResult<()> {
        let identity = identity.to_string();
        self.store
            .update_async(move |items| items.retain(|r| r.identity != identity))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn spec(db: &str) -> ConnectionSpec {
        ConnectionSpec {
            name: None,
            host: "127.0.0.1".into(),
            port: 3306,
            socket: None,
            user: "root".into(),
            database: Some(db.into()),
        }
    }

    fn dbs(store: &RecentsStore) -> Vec<String> {
        store
            .list()
            .into_iter()
            .map(|r| r.spec.database.unwrap())
            .collect()
    }

    #[tokio::test]
    async fn most_recent_comes_first_without_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentsStore::open(dir.path());
        store.touch(&spec("a")).await.unwrap();
        store.touch(&spec("b")).await.unwrap();
        store.touch(&spec("a")).await.unwrap();
        assert_eq!(dbs(&store), ["a", "b"]);
    }

    #[tokio::test]
    async fn keeps_at_most_twenty_entries() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentsStore::open(dir.path());
        for i in 0..25 {
            store.touch(&spec(&format!("db{i}"))).await.unwrap();
        }
        assert_eq!(store.list().len(), 20);
        assert_eq!(dbs(&store)[0], "db24");
    }

    #[tokio::test]
    async fn list_fills_the_account_from_the_spec() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentsStore::open(dir.path());
        store.touch(&spec("a")).await.unwrap();

        assert_eq!(store.list()[0].account, "root@127.0.0.1:3306");
    }

    #[test]
    fn list_fills_the_account_for_entries_written_without_one() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("connections.json"),
            r#"[{
                "identity": "root@127.0.0.1:3306/a",
                "spec": {"host": "127.0.0.1", "port": 3306, "user": "root", "database": "a"},
                "lastUsed": "2024-01-01T00:00:00Z"
            }]"#,
        )
        .unwrap();

        let recents = RecentsStore::open(dir.path()).list();

        assert_eq!(recents.len(), 1);
        assert_eq!(recents[0].account, "root@127.0.0.1:3306");
    }

    fn named(name: &str, db: &str) -> ConnectionSpec {
        ConnectionSpec {
            name: Some(name.into()),
            ..spec(db)
        }
    }

    fn slugs(store: &RecentsStore) -> Vec<String> {
        store.list().into_iter().map(|r| r.slug).collect()
    }

    #[tokio::test]
    async fn touch_returns_the_default_slug_of_a_new_connection() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentsStore::open(dir.path());

        let slug = store.touch(&spec("wp")).await.unwrap();

        assert_eq!(slug, "127-0-0-1-3306-wp");
        assert_eq!(slugs(&store), [slug]);
    }

    #[tokio::test]
    async fn connections_on_different_ports_get_distinct_slugs() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentsStore::open(dir.path());
        let other_port = ConnectionSpec {
            port: 3307,
            ..spec("db")
        };

        let first = store.touch(&spec("db")).await.unwrap();
        let second = store.touch(&other_port).await.unwrap();

        assert_eq!(first, "127-0-0-1-3306-db");
        assert_eq!(second, "127-0-0-1-3307-db");
    }

    #[tokio::test]
    async fn connections_with_the_same_name_get_numbered_slugs() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentsStore::open(dir.path());

        let first = store.touch(&named("Shop", "a")).await.unwrap();
        let second = store.touch(&named("Shop", "b")).await.unwrap();
        let third = store.touch(&named("Shop", "c")).await.unwrap();

        assert_eq!([first, second, third], ["shop", "shop-2", "shop-3"]);
    }

    #[tokio::test]
    async fn touching_a_known_connection_keeps_its_renamed_slug() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentsStore::open(dir.path());
        store.touch(&spec("a")).await.unwrap();
        store
            .rename_slug(&spec("a").identity(), "mine")
            .await
            .unwrap();

        let slug = store.touch(&spec("a")).await.unwrap();

        assert_eq!(slug, "mine");
        assert_eq!(slugs(&store), ["mine"]);
    }

    #[test]
    fn list_fills_unique_slugs_for_entries_written_without_one() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("connections.json"),
            r#"[
                {
                    "identity": "root@127.0.0.1:3306/a",
                    "spec": {"name": "Shop", "host": "127.0.0.1", "port": 3306, "user": "root", "database": "a"},
                    "lastUsed": "2024-01-02T00:00:00Z"
                },
                {
                    "identity": "root@127.0.0.1:3306/b",
                    "spec": {"name": "Shop", "host": "127.0.0.1", "port": 3306, "user": "root", "database": "b"},
                    "lastUsed": "2024-01-01T00:00:00Z"
                },
                {
                    "identity": "root@127.0.0.1:3307/c",
                    "spec": {"host": "127.0.0.1", "port": 3307, "user": "root", "database": "c"},
                    "lastUsed": "2024-01-01T00:00:00Z",
                    "slug": "kept"
                }
            ]"#,
        )
        .unwrap();

        let store = RecentsStore::open(dir.path());

        assert_eq!(slugs(&store), ["shop", "shop-2", "kept"]);
    }

    #[tokio::test]
    async fn renaming_a_slug_stores_it_and_survives_a_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentsStore::open(dir.path());
        store.touch(&spec("a")).await.unwrap();

        store
            .rename_slug(&spec("a").identity(), "wc-pp")
            .await
            .unwrap();

        assert_eq!(slugs(&store), ["wc-pp"]);
        assert_eq!(slugs(&RecentsStore::open(dir.path())), ["wc-pp"]);
    }

    #[tokio::test]
    async fn renaming_a_slug_to_its_own_current_value_is_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentsStore::open(dir.path());
        let slug = store.touch(&spec("a")).await.unwrap();

        store
            .rename_slug(&spec("a").identity(), &slug)
            .await
            .unwrap();

        assert_eq!(slugs(&store), [slug]);
    }

    #[tokio::test]
    async fn renaming_to_an_invalid_or_taken_slug_or_for_an_unknown_identity_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentsStore::open(dir.path());
        store.touch(&named("Shop", "a")).await.unwrap();
        store.touch(&named("Blog", "b")).await.unwrap();
        let before = slugs(&store);
        let identity = named("Shop", "a").identity();

        for (identity, slug) in [
            (identity.as_str(), "Not A Slug"),
            (identity.as_str(), ""),
            (identity.as_str(), "blog"),
            ("root@nowhere:1/x", "free"),
        ] {
            let err = store.rename_slug(identity, slug).await.unwrap_err();
            assert_eq!(
                err.kind(),
                ErrorKind::InvalidInput,
                "{identity} -> {slug:?}"
            );
        }
        assert_eq!(slugs(&store), before);
    }

    #[tokio::test]
    async fn removal_persists() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentsStore::open(dir.path());
        store.touch(&spec("a")).await.unwrap();
        store.touch(&spec("b")).await.unwrap();
        store.remove(&spec("a").identity()).await.unwrap();
        assert_eq!(dbs(&RecentsStore::open(dir.path())), ["b"]);
    }
}
