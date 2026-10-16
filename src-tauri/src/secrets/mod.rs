use crate::connection::{ConnectionSpec, Secret};
use crate::error::AppResult;

mod keychain;
#[cfg(any(test, feature = "test-util"))]
mod memory;

pub use keychain::Keychain;
#[cfg(any(test, feature = "test-util"))]
pub use memory::MemoryStore;

pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> AppResult<Option<Secret>>;
    fn set(&self, account: &str, secret: &Secret) -> AppResult<()>;
    fn delete(&self, account: &str) -> AppResult<()>;
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
        fn delete(&self, _: &str) -> AppResult<()> {
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

    #[test]
    fn deleted_password_is_no_longer_resolved() {
        let store = MemoryStore::default();
        store.set(&spec().account(), &Secret::new("x")).unwrap();
        store.delete(&spec().account()).unwrap();
        assert_eq!(
            resolve_password(None, &store, &spec()),
            PasswordSource::None
        );
    }
}
