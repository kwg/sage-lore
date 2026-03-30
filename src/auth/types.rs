// SPDX-License-Identifier: MIT
use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub forges: HashMap<String, ForgeCredential>,
    pub default_forge: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeCredential {
    pub forge_type: ForgeType,
    pub url: String,
    pub username: String,
    pub token: String,
    pub added_at: DateTime<Utc>,
    pub last_used: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForgeType {
    Forgejo,
    GitHub,
    GitLab,
    Gitea,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("No credentials found for host: {0}")]
    NoCredentials(String),

    #[error("Token validation failed: {0}")]
    ValidationFailed(String),

    #[error("Auth config not found at {0}")]
    ConfigNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(#[from] serde_yaml::Error),
}
