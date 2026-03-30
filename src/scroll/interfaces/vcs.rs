// SPDX-License-Identifier: MIT
//! VCS interface adapter.

use async_trait::async_trait;
use std::sync::Arc;

use crate::primitives::vcs::{GitBackend, DiffScope, ForceMode};
use crate::scroll::error::ExecutionError;
use super::InterfaceDispatch;

/// VCS interface for version control operations.
#[derive(Clone)]
pub struct VcsInterface {
    backend: Option<Arc<dyn GitBackend>>,
}

impl VcsInterface {
    /// Create a stub interface (for unit tests).
    pub fn new() -> Self {
        Self { backend: None }
    }

    /// Create an interface with a specific backend.
    pub fn with_backend(backend: Arc<dyn GitBackend>) -> Self {
        Self { backend: Some(backend) }
    }
}

impl Default for VcsInterface {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InterfaceDispatch for VcsInterface {
    async fn dispatch(
        &self,
        method: &str,
        params: &Option<serde_json::Value>,
    ) -> Result<serde_json::Value, ExecutionError> {
        let backend = match &self.backend {
            Some(b) => b.clone(),
            None => return Err(ExecutionError::NotImplemented(format!("git.{} (no backend)", method))),
        };

        match method {
            "ensure_branch" => {
                let name = params.as_ref()
                    .and_then(|p| p.get("name"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("name".to_string()))?;

                let result = backend.ensure_branch(name)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(result)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "commit" => {
                let message = params.as_ref()
                    .and_then(|p| p.get("message"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("message".to_string()))?;

                let result = backend.commit(message, None)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(result)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "push" => {
                let set_upstream = params.as_ref()
                    .and_then(|p| p.get("set_upstream"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                backend.push(set_upstream, ForceMode::None)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "merge" => {
                let branch = params.as_ref()
                    .and_then(|p| p.get("branch"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("branch".to_string()))?;

                let result = backend.merge(branch)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(result)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "squash" => {
                let branch = params.as_ref()
                    .and_then(|p| p.get("branch"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("branch".to_string()))?;

                let result = backend.squash(branch)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(result)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "diff" => {
                let scope = params.as_ref()
                    .and_then(|p| p.get("scope"))
                    .and_then(|v| v.as_str())
                    .map(|s| match s {
                        "staged" => DiffScope::Staged,
                        "unstaged" => DiffScope::Unstaged,
                        _ => DiffScope::Head,
                    })
                    .unwrap_or(DiffScope::Head);

                let result = backend.diff(scope)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(result)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "status" => {
                let result = backend.status()
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(result)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "stage_all" => {
                backend.stage_all()
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "current_branch" => {
                let branch = backend.current_branch()
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::String(branch))
            }
            "head" => {
                let sha = backend.head()
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::String(sha))
            }
            "head_short" => {
                let sha = backend.head_short()
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::String(sha))
            }
            "tag" => {
                let name = params.as_ref()
                    .and_then(|p| p.get("name"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("name".to_string()))?;

                let message = params.as_ref()
                    .and_then(|p| p.get("message"))
                    .and_then(|v| v.as_str());

                backend.tag(name, message)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "log" => {
                let count = params.as_ref()
                    .and_then(|p| p.get("count"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10) as usize;

                let result = backend.log(count)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(result)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "checkout" => {
                let branch = params.as_ref()
                    .and_then(|p| p.get("branch"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("branch".to_string()))?;

                backend.checkout(branch)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "stage" => {
                let files = params.as_ref()
                    .and_then(|p| p.get("files"))
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| ExecutionError::MissingParameter("files".to_string()))?;

                let file_strs: Vec<&str> = files
                    .iter()
                    .filter_map(|f| f.as_str())
                    .collect();

                backend.stage(&file_strs)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "unstage" => {
                let files = params.as_ref()
                    .and_then(|p| p.get("files"))
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| ExecutionError::MissingParameter("files".to_string()))?;

                let file_strs: Vec<&str> = files
                    .iter()
                    .filter_map(|f| f.as_str())
                    .collect();

                backend.unstage(&file_strs)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "branch_exists" => {
                let name = params.as_ref()
                    .and_then(|p| p.get("name"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("name".to_string()))?;

                let exists = backend.branch_exists(name);

                Ok(serde_json::Value::Bool(exists))
            }
            "delete_branch" => {
                let name = params.as_ref()
                    .and_then(|p| p.get("name"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("name".to_string()))?;

                let force = params.as_ref()
                    .and_then(|p| p.get("force"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                backend.delete_branch(name, force)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "fetch" => {
                let remote = params.as_ref()
                    .and_then(|p| p.get("remote"))
                    .and_then(|v| v.as_str());

                backend.fetch(remote)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "pull" => {
                let remote = params.as_ref()
                    .and_then(|p| p.get("remote"))
                    .and_then(|v| v.as_str());

                let branch = params.as_ref()
                    .and_then(|p| p.get("branch"))
                    .and_then(|v| v.as_str());

                backend.pull(remote, branch)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "pr_branch_ready" => {
                let branch = params.as_ref()
                    .and_then(|p| p.get("branch"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("branch".to_string()))?;

                let base = params.as_ref()
                    .and_then(|p| p.get("base"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("base".to_string()))?;

                let ready = backend.pr_branch_ready(branch, base)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Bool(ready))
            }
            "abort_merge" => {
                backend.abort_merge()
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "stash_push" => {
                let message = params.as_ref()
                    .and_then(|p| p.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("WIP");

                let result = backend.stash_push(message)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(result)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "stash_pop" => {
                let index = params.as_ref()
                    .and_then(|p| p.get("index"))
                    .and_then(|v| v.as_u64())
                    .map(|i| i as usize);

                backend.stash_pop(index)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "stash_list" => {
                let result = backend.stash_list()
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(result)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "reset_hard" => {
                let target = params.as_ref()
                    .and_then(|p| p.get("target"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("target".to_string()))?;

                backend.reset_hard(target)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "reset_soft" => {
                let target = params.as_ref()
                    .and_then(|p| p.get("target"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("target".to_string()))?;

                backend.reset_soft(target)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "resolve_ref" => {
                let ref_name = params.as_ref()
                    .and_then(|p| p.get("ref"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("ref".to_string()))?;

                let sha = backend.resolve_ref(ref_name)
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::String(sha))
            }
            "list_tags" => {
                let result = backend.list_tags()
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(result)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            _ => Err(ExecutionError::InterfaceError(format!(
                "unknown git method: {}",
                method
            ))),
        }
    }
}
