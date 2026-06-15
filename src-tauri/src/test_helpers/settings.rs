use std::sync::Mutex;

use crate::settings::{SecretStore, SettingsError};

#[derive(Default)]
pub struct MemorySecretStore {
    pub secret: Mutex<Option<String>>,
}

impl MemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_key(api_key: &str) -> Self {
        Self {
            secret: Mutex::new(Some(api_key.to_string())),
        }
    }
}

impl SecretStore for MemorySecretStore {
    fn save(&self, secret: &str) -> Result<(), SettingsError> {
        *self.secret.lock().unwrap() = Some(secret.to_string());
        Ok(())
    }

    fn get(&self) -> Result<Option<String>, SettingsError> {
        Ok(self.secret.lock().unwrap().clone())
    }

    fn clear(&self) -> Result<(), SettingsError> {
        *self.secret.lock().unwrap() = None;
        Ok(())
    }
}
