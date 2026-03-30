// SPDX-License-Identifier: MIT
//! Test interface adapter.

use async_trait::async_trait;
use std::sync::Arc;

use crate::primitives::test::TestBackend;
use crate::scroll::error::ExecutionError;
use super::InterfaceDispatch;

/// Test interface for test execution.
#[derive(Clone)]
pub struct TestInterface {
    backend: Option<Arc<dyn TestBackend>>,
}

impl TestInterface {
    /// Create a stub interface (for unit tests).
    pub fn new() -> Self {
        Self { backend: None }
    }

    /// Create an interface with a specific backend.
    pub fn with_backend(backend: Arc<dyn TestBackend>) -> Self {
        Self { backend: Some(backend) }
    }
}

impl Default for TestInterface {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InterfaceDispatch for TestInterface {
    async fn dispatch(
        &self,
        method: &str,
        params: &Option<serde_json::Value>,
    ) -> Result<serde_json::Value, ExecutionError> {
        let backend = match &self.backend {
            Some(b) => b.clone(),
            None => return Err(ExecutionError::NotImplemented(format!("test.{} (no backend)", method))),
        };

        match method {
            "run" => {
                let filter = params.as_ref()
                    .and_then(|p| p.get("filter"))
                    .and_then(|v| v.as_str());

                let result = backend.run_suite(filter)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(result)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "coverage" => {
                let result = backend.coverage()
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(result)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "smoke" => {
                let result = backend.smoke()
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(result)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "run_filtered" => {
                let pattern = params.as_ref()
                    .and_then(|p| p.get("pattern"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("pattern".to_string()))?;

                let result = backend.run_filtered(pattern)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(result)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "run_files" => {
                let files = params.as_ref()
                    .and_then(|p| p.get("files"))
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| ExecutionError::MissingParameter("files".to_string()))?;

                let file_strs: Vec<&str> = files
                    .iter()
                    .filter_map(|f| f.as_str())
                    .collect();

                let result = backend.run_files(&file_strs)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(result)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "info" => {
                // Return test backend capabilities
                let mut info = serde_json::Map::new();
                info.insert(
                    "framework".to_string(),
                    serde_json::to_value(backend.framework())
                        .unwrap_or(serde_json::Value::Null),
                );
                info.insert(
                    "supports_coverage".to_string(),
                    serde_json::Value::Bool(backend.supports_coverage()),
                );
                info.insert(
                    "supports_watch".to_string(),
                    serde_json::Value::Bool(backend.supports_watch()),
                );
                Ok(serde_json::Value::Object(info))
            }
            _ => Err(ExecutionError::InterfaceError(format!(
                "unknown test method: {}",
                method
            ))),
        }
    }
}
