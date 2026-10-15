use std::collections::HashMap;
use std::sync::Mutex;

use crate::connection::{ConnectionSpec, Secret};
use crate::error::{AppError, AppResult};

pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> AppResult<Option<Secret>>;
    fn set(&self, account: &str, secret: &Secret) -> AppResult<()>;
    fn delete(&self, account: &str) -> AppResult<()>;
}

/// Generic passwords in the login Keychain, service "databee".
pub struct Keychain;

const SERVICE: &str = "databee";
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

impl SecretStore for Keychain {
    fn get(&self, account: &str) -> AppResult<Option<Secret>> {
        match security_framework::passwords::get_generic_password(SERVICE, account) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(|s| Some(Secret::new(s)))
                .map_err(|_| AppError::Keychain("stored password is not UTF-8".into())),
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(None),
            Err(e) => Err(AppError::Keychain(e.to_string())),
        }
    }

    fn set(&self, account: &str, secret: &Secret) -> AppResult<()> {
        security_framework::passwords::set_generic_password(
            SERVICE,
            account,
            secret.expose().as_bytes(),
        )
        .map_err(|e| AppError::Keychain(e.to_string()))
    }

    fn delete(&self, account: &str) -> AppResult<()> {
        match security_framework::passwords::delete_generic_password(SERVICE, account) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
            Err(e) => Err(AppError::Keychain(e.to_string())),
        }
    }
}

#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, Secret>>);

impl SecretStore for MemoryStore {
    fn get(&self, account: &str) -> AppResult<Option<Secret>> {
        Ok(self.0.lock().unwrap().get(account).cloned())
    }

    fn set(&self, account: &str, secret: &Secret) -> AppResult<()> {
        self.0
            .lock()
            .unwrap()
            .insert(account.to_string(), secret.clone());
        Ok(())
    }

    fn delete(&self, account: &str) -> AppResult<()> {
        self.0.lock().unwrap().remove(account);
        Ok(())
    }
}

/// Where a password came from; a stored password that fails auth should be re-prompted.
#[derive(Debug, Clone, PartialEq)]
pub enum PasswordSource {
    Provided(Secret),
    Stored(Secret),
    None,
}

impl PasswordSource {
    pub fn secret(&self) -> Option<&Secret> {
        match self {
            PasswordSource::Provided(s) | PasswordSource::Stored(s) => Some(s),
            PasswordSource::None => None,
        }
    }
}

/// Explicit password first, then the Keychain. A Keychain failure is logged and treated
/// as "no stored password" so the user can still type one in.
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
            PasswordSource::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
