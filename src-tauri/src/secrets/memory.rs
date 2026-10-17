use std::collections::HashMap;
use std::sync::Mutex;

use super::SecretStore;
use crate::connection::Secret;
use crate::error::AppResult;
use crate::lock::MutexExt;

/// In-memory store for tests.
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, Secret>>);

impl SecretStore for MemoryStore {
    fn get(&self, account: &str) -> AppResult<Option<Secret>> {
        Ok(self.0.lock_unpoisoned().get(account).cloned())
    }

    fn set(&self, account: &str, secret: &Secret) -> AppResult<()> {
        self.0
            .lock_unpoisoned()
            .insert(account.to_string(), secret.clone());
        Ok(())
    }
}
