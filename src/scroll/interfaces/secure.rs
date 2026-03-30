// SPDX-License-Identifier: MIT
//! Secure interface adapter for security scanning.

use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

use crate::primitives::SecureBackend;
use crate::scroll::error::ExecutionError;
use super::InterfaceDispatch;

/// Secure interface for security scanning operations.
#[derive(Clone)]
pub struct SecureInterface {
    backend: Option<Arc<dyn SecureBackend>>,
}

impl SecureInterface {
    /// Create a stub interface (for unit tests).
    pub fn new() -> Self {
        Self { backend: None }
    }

    /// Create an interface with a specific backend.
    pub fn with_backend(backend: Arc<dyn SecureBackend>) -> Self {
        Self { backend: Some(backend) }
    }
}

impl Default for SecureInterface {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InterfaceDispatch for SecureInterface {
    async fn dispatch(
        &self,
        method: &str,
        params: &Option<serde_json::Value>,
    ) -> Result<serde_json::Value, ExecutionError> {
        let backend = match &self.backend {
            Some(b) => b.clone(),
            None => return Err(ExecutionError::NotImplemented(format!("secure.{} (no backend)", method))),
        };

        match method {
            "secret_detection" => {
                let content = params.as_ref()
                    .and_then(|p| p.get("content"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("content".to_string()))?;

                let result = backend.secret_detection(content)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(result)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "dependency_scan" => {
                let manifest = params.as_ref()
                    .and_then(|p| p.get("manifest"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("manifest".to_string()))?;

                let result = backend.dependency_scan(Path::new(manifest))
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(result)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "static_analysis" => {
                let path = params.as_ref()
                    .and_then(|p| p.get("path"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("path".to_string()))?;

                let result = backend.static_analysis(Path::new(path))
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(result)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "audit" => {
                let root = params.as_ref()
                    .and_then(|p| p.get("root"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("root".to_string()))?;

                let result = backend.audit(Path::new(root))
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(result)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "available_tools" => {
                let tools = backend.available_tools();

                serde_json::to_value(tools)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            _ => Err(ExecutionError::InterfaceError(format!(
                "unknown secure method: {}",
                method
            ))),
        }
    }
}
