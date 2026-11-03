use crate::connection::{ConnectionSpec, Secret};
use crate::error::AppResult;

#[cfg(not(debug_assertions))]
mod keychain;
#[cfg(any(test, debug_assertions))]
mod memory;

#[cfg(not(debug_assertions))]
pub use keychain::Keychain;
#[cfg(any(test, debug_assertions))]
pub use memory::MemoryStore;

/// Debug builds never touch the Keychain: their passwords live in memory, starting with the demo
/// connections' (docs/adr/0028).
#[cfg(debug_assertions)]
pub fn app_store() -> std::sync::Arc<dyn SecretStore> {
    let store = MemoryStore::default();
    for (spec, password) in crate::test_support::demo_connections() {
        let _ = store.set(&spec.account(), &password);
    }
    std::sync::Arc::new(store)
}

#[cfg(not(debug_assertions))]
pub fn app_store() -> std::sync::Arc<dyn SecretStore> {
    std::sync::Arc::new(Keychain)
}

pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> AppResult<Option<Secret>>;
    fn set(&self, account: &str, secret: &Secret) -> AppResult<()>;
}

/// Where a password came from.
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub enum PasswordSource {
    Provided(Secret),
    Stored(Secret),
    None,
    /// The Keychain lookup failed, with its reason.
    Unavailable(String),
}

impl PasswordSource {
    pub fn secret(&self) -> Option<&Secret> {
        match self {
            PasswordSource::Provided(s) | PasswordSource::Stored(s) => Some(s),
            PasswordSource::None | PasswordSource::Unavailable(_) => None,
        }
    }
}

/// Explicit password first, then the Keychain. A Keychain failure still lets the
/// connection be tried without a password, so the user can type one in.
pub fn resolve_password(
    explicit: Option<Secret>,
    store: &dyn SecretStore,
    spec: &ConnectionSpec,
) -> PasswordSource {
    if let Some(secret) = explicit {
        return PasswordSource::Provided(secret);
    }
    match store.get(&spec.account()) {
        Ok(Some(secret)) => PasswordSource::Stored(secret),
        Ok(None) => PasswordSource::None,
        Err(e) => {
            log::warn!("Keychain lookup for {} failed: {e}", spec.account());
            PasswordSource::Unavailable(e.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;

    fn spec() -> ConnectionSpec {
        ConnectionSpec {
            name: None,
            host: "127.0.0.1".into(),
            port: 3306,
            socket: None,
            user: "wp".into(),
            database: None,
        }
    }

    #[cfg(debug_assertions)]
    #[test]
    fn the_debug_app_store_knows_each_demo_connections_password() {
        let store = app_store();

        for (spec, password) in crate::test_support::demo_connections() {
            assert_eq!(store.get(&spec.account()).unwrap(), Some(password));
        }
    }

    #[cfg(debug_assertions)]
    #[test]
    fn the_debug_app_store_is_not_shared_between_calls() {
        app_store()
            .set(&spec().account(), &Secret::new("typed"))
            .unwrap();

        assert_eq!(app_store().get(&spec().account()).unwrap(), None);
    }

    #[test]
    fn explicit_password_wins_over_stored_one() {
        let store = MemoryStore::default();
        store
            .set(&spec().account(), &Secret::new("stored"))
            .unwrap();
        assert_eq!(
            resolve_password(Some(Secret::new("given")), &store, &spec()),
            PasswordSource::Provided(Secret::new("given"))
        );
    }

    #[test]
    fn falls_back_to_stored_password() {
        let store = MemoryStore::default();
        store
            .set(&spec().account(), &Secret::new("stored"))
            .unwrap();
        assert_eq!(
            resolve_password(None, &store, &spec()),
            PasswordSource::Stored(Secret::new("stored"))
        );
    }

    #[test]
    fn nothing_found_means_no_password() {
        assert_eq!(
            resolve_password(None, &MemoryStore::default(), &spec()),
            PasswordSource::None
        );
    }

    struct FailingStore;

    impl SecretStore for FailingStore {
        fn get(&self, _: &str) -> AppResult<Option<Secret>> {
            Err(AppError::Keychain("locked".into()))
        }
        fn set(&self, _: &str, _: &Secret) -> AppResult<()> {
            Err(AppError::Keychain("locked".into()))
        }
    }

    #[test]
    fn keychain_failure_is_unavailable_without_a_secret() {
        let source = resolve_password(None, &FailingStore, &spec());
        assert!(matches!(source, PasswordSource::Unavailable(_)));
        assert_eq!(source.secret(), None);
    }

    #[test]
    fn only_provided_and_stored_sources_carry_a_secret() {
        assert_eq!(
            PasswordSource::Provided(Secret::new("a")).secret(),
            Some(&Secret::new("a"))
        );
        assert_eq!(
            PasswordSource::Stored(Secret::new("b")).secret(),
            Some(&Secret::new("b"))
        );
        assert_eq!(PasswordSource::None.secret(), None);
        assert_eq!(PasswordSource::Unavailable("x".into()).secret(), None);
    }
}
