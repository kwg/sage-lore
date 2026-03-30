// SPDX-License-Identifier: MIT
use std::fs;
use std::path::PathBuf;

use chrono::Utc;

use super::types::{AuthConfig, AuthError, ForgeCredential, ForgeType};

pub struct AuthStore {
    path: PathBuf,
}

impl AuthStore {
    pub fn new() -> Result<Self, AuthError> {
        let path = dirs::config_dir()
            .ok_or_else(|| AuthError::ConfigNotFound("No config directory found".to_string()))?
            .join("sage")
            .join("auth.yaml");
        Ok(AuthStore { path })
    }

    /// Create an AuthStore with a custom path (useful for testing)
    pub fn with_path(path: PathBuf) -> Self {
        AuthStore { path }
    }

    fn load_config(&self) -> Result<AuthConfig, AuthError> {
        if !self.path.exists() {
            return Ok(AuthConfig {
                forges: Default::default(),
                default_forge: None,
            });
        }
        let content = fs::read_to_string(&self.path)?;
        serde_yaml::from_str(&content).map_err(AuthError::Parse)
    }

    fn save_config(&self, config: &AuthConfig) -> Result<(), AuthError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_yaml::to_string(&config).map_err(AuthError::Parse)?;
        fs::write(&self.path, content)?;
        Ok(())
    }

    pub fn login(
        &self,
        host: &str,
        forge_type: ForgeType,
        url: &str,
        username: &str,
        token: &str,
    ) -> Result<(), AuthError> {
        let mut config = self.load_config()?;
        let now = Utc::now();
        config.forges.insert(
            host.to_string(),
            ForgeCredential {
                forge_type,
                url: url.to_string(),
                username: username.to_string(),
                token: token.to_string(),
                added_at: now,
                last_used: now,
            },
        );
        self.save_config(&config)
    }

    pub fn logout(&self, host: &str) -> Result<(), AuthError> {
        let mut config = self.load_config()?;
        config.forges.remove(host);
        self.save_config(&config)
    }

    pub fn get_token(host: &str) -> Result<String, AuthError> {
        let store = AuthStore::new()?;
        let config = store.load_config()?;
        config
            .forges
            .get(host)
            .map(|cred| cred.token.clone())
            .ok_or_else(|| AuthError::NoCredentials(host.to_string()))
    }

    pub fn get_credential(&self, host: &str) -> Result<ForgeCredential, AuthError> {
        let config = self.load_config()?;
        config
            .forges
            .get(host)
            .cloned()
            .ok_or_else(|| AuthError::NoCredentials(host.to_string()))
    }
}
