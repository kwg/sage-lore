// SPDX-License-Identifier: MIT
//! Filesystem interface adapter.

use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

use crate::primitives::fs::FsBackend;
use crate::scroll::error::ExecutionError;
use super::InterfaceDispatch;

/// Filesystem interface for file operations.
#[derive(Clone)]
pub struct FsInterface {
    backend: Option<Arc<dyn FsBackend>>,
}

impl FsInterface {
    /// Create a stub interface (for unit tests).
    pub fn new() -> Self {
        Self { backend: None }
    }

    /// Create an interface with a specific backend.
    pub fn with_backend(backend: Arc<dyn FsBackend>) -> Self {
        Self { backend: Some(backend) }
    }
}

impl Default for FsInterface {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InterfaceDispatch for FsInterface {
    async fn dispatch(
        &self,
        method: &str,
        params: &Option<serde_json::Value>,
    ) -> Result<serde_json::Value, ExecutionError> {
        let backend = match &self.backend {
            Some(b) => b.clone(),
            None => return Err(ExecutionError::NotImplemented(format!("fs.{} (no backend)", method))),
        };

        match method {
            "read" => {
                let path = params.as_ref()
                    .and_then(|p| p.get("path"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("path".to_string()))?;

                let content = backend.read(Path::new(path))
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::String(content))
            }
            "write" => {
                let path = params.as_ref()
                    .and_then(|p| p.get("path"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("path".to_string()))?;

                let content = params.as_ref()
                    .and_then(|p| p.get("content"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("content".to_string()))?;

                backend.write(Path::new(path), content)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "exists" => {
                let path = params.as_ref()
                    .and_then(|p| p.get("path"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("path".to_string()))?;

                let exists = backend.exists(Path::new(path));

                Ok(serde_json::Value::Bool(exists))
            }
            "list" => {
                let path = params.as_ref()
                    .and_then(|p| p.get("path"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("path".to_string()))?;

                let pattern = params.as_ref()
                    .and_then(|p| p.get("pattern"))
                    .and_then(|v| v.as_str());

                let entries = backend.list(Path::new(path), pattern)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                // Return filenames only (not absolute paths) so scrolls can
                // compose paths like "${project_root}/src/${src_file}".
                // The backend returns absolute paths after sandbox validation.
                let values: Vec<serde_json::Value> = entries
                    .iter()
                    .map(|p| {
                        let name = p.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| p.display().to_string());
                        serde_json::Value::String(name)
                    })
                    .collect();

                Ok(serde_json::Value::Array(values))
            }
            "mkdir" => {
                let path = params.as_ref()
                    .and_then(|p| p.get("path"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("path".to_string()))?;

                backend.mkdir(Path::new(path))
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "delete" => {
                let path = params.as_ref()
                    .and_then(|p| p.get("path"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("path".to_string()))?;

                backend.delete(Path::new(path))
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "copy" => {
                let src = params.as_ref()
                    .and_then(|p| p.get("path"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("path (source)".to_string()))?;

                let dest = params.as_ref()
                    .and_then(|p| p.get("dest"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("dest".to_string()))?;

                backend.copy(Path::new(src), Path::new(dest))
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "move" => {
                let src = params.as_ref()
                    .and_then(|p| p.get("path"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("path (source)".to_string()))?;

                let dest = params.as_ref()
                    .and_then(|p| p.get("dest"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("dest".to_string()))?;

                backend.rename(Path::new(src), Path::new(dest))
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "append" => {
                let path = params.as_ref()
                    .and_then(|p| p.get("path"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("path".to_string()))?;

                let content = params.as_ref()
                    .and_then(|p| p.get("content"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("content".to_string()))?;

                backend.append(Path::new(path), content)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "stat" => {
                let path = params.as_ref()
                    .and_then(|p| p.get("path"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("path".to_string()))?;

                let meta = backend.stat(Path::new(path))
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(meta)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            _ => Err(ExecutionError::InterfaceError(format!(
                "unknown fs method: {}",
                method
            ))),
        }
    }
}
