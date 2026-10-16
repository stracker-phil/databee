use super::SecretStore;
use crate::connection::Secret;
use crate::error::{AppError, AppResult};

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
