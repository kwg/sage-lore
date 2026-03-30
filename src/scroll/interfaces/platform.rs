// SPDX-License-Identifier: MIT
//! Platform interface adapter for issue tracker operations.

use async_trait::async_trait;
use std::sync::Arc;

use crate::primitives::Platform;
use crate::primitives::platform::CreateIssueRequest;
use crate::scroll::error::ExecutionError;
use super::InterfaceDispatch;

/// Extract an i64 from a JSON value, handling both Number and String("42") (B2, #180).
/// YAML deserializes numbers into strings when the Rust type is Option<String>.
fn value_as_i64(v: &serde_json::Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

/// Platform interface for forge operations (issues, PRs, etc).
#[derive(Clone)]
pub struct PlatformInterface {
    backend: Option<Arc<dyn Platform>>,
}

impl PlatformInterface {
    /// Create a stub interface (for unit tests).
    pub fn new() -> Self {
        Self { backend: None }
    }

    /// Create an interface with a specific backend.
    pub fn with_backend(backend: Arc<dyn Platform>) -> Self {
        Self { backend: Some(backend) }
    }
}

impl Default for PlatformInterface {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InterfaceDispatch for PlatformInterface {
    async fn dispatch(
        &self,
        method: &str,
        params: &Option<serde_json::Value>,
    ) -> Result<serde_json::Value, ExecutionError> {
        let backend = match &self.backend {
            Some(b) => b.clone(),
            None => return Err(ExecutionError::NotImplemented(format!("platform.{} (no backend)", method))),
        };

        match method {
            "get_issue" => {
                let number = params.as_ref()
                    .and_then(|p| p.get("number"))
                    .and_then(value_as_i64)
                    .ok_or_else(|| ExecutionError::MissingParameter("number".to_string()))?;

                let issue = backend.get_issue(number).await
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(issue)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "create_issue" => {
                // Look for title/body at top level first, then inside "payload" object.
                // Scrolls pass `payload: ${var}` which step_dispatch nests as params["payload"].
                let effective = params.as_ref()
                    .and_then(|p| {
                        if p.get("title").is_some() {
                            Some(p)
                        } else {
                            p.get("payload").and_then(|v| v.as_object()).map(|_| p.get("payload").unwrap())
                        }
                    });

                let title = effective
                    .and_then(|p| p.get("title"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("title".to_string()))?;

                let body = effective
                    .and_then(|p| p.get("body"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let request = CreateIssueRequest {
                    title: title.to_string(),
                    body: body.to_string(),
                    labels: None,
                    milestone: None,
                    assignees: None,
                };

                let issue = backend.create_issue(request).await
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(issue)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "close_issue" => {
                let number = params.as_ref()
                    .and_then(|p| p.get("number"))
                    .and_then(value_as_i64)
                    .ok_or_else(|| ExecutionError::MissingParameter("number".to_string()))?;

                backend.close_issue(number).await
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "list_issues" => {
                let mut filter = crate::primitives::platform::IssueFilter::default();

                // Apply filter params from scroll step
                if let Some(p) = params.as_ref() {
                    if let Some(state) = p.get("state").and_then(|v| v.as_str()) {
                        filter.state = Some(state.to_string());
                    }
                    if let Some(labels) = p.get("labels").and_then(|v| v.as_array()) {
                        let label_strings: Vec<String> = labels.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect();
                        if !label_strings.is_empty() {
                            filter.labels = Some(label_strings);
                        }
                    }
                    if let Some(milestone) = p.get("milestone").and_then(value_as_i64) {
                        filter.milestone = Some(milestone);
                    }
                    if let Some(assignee) = p.get("assignee").and_then(|v| v.as_str()) {
                        filter.assignee = Some(assignee.to_string());
                    }
                }

                let issues = backend.list_issues(filter).await
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(issues)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "add_labels" => {
                let number = params.as_ref()
                    .and_then(|p| p.get("number"))
                    .and_then(value_as_i64)
                    .ok_or_else(|| ExecutionError::MissingParameter("number".to_string()))?;

                let labels: Vec<&str> = params.as_ref()
                    .and_then(|p| p.get("labels"))
                    .and_then(|v| v.as_array())
                    .map(|seq| seq.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();

                backend.add_labels(number, &labels).await
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "create_comment" => {
                let number = params.as_ref()
                    .and_then(|p| p.get("number"))
                    .and_then(value_as_i64)
                    .ok_or_else(|| ExecutionError::MissingParameter("number".to_string()))?;

                let body = params.as_ref()
                    .and_then(|p| p.get("body"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("body".to_string()))?;

                let comment = backend.create_comment(number, body).await
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(comment)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "remove_labels" => {
                let number = params.as_ref()
                    .and_then(|p| p.get("number"))
                    .and_then(value_as_i64)
                    .ok_or_else(|| ExecutionError::MissingParameter("number".to_string()))?;

                let labels: Vec<&str> = params.as_ref()
                    .and_then(|p| p.get("labels"))
                    .and_then(|v| v.as_array())
                    .map(|seq| seq.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();

                backend.remove_labels(number, &labels).await
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            "get_comments" => {
                let number = params.as_ref()
                    .and_then(|p| p.get("number"))
                    .and_then(value_as_i64)
                    .ok_or_else(|| ExecutionError::MissingParameter("number".to_string()))?;

                let comments = backend.get_comments(number).await
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(comments)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "create_milestone" => {
                let title = params.as_ref()
                    .and_then(|p| p.get("title"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("title".to_string()))?;

                let description = params.as_ref()
                    .and_then(|p| p.get("description"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let milestone = backend.create_milestone(title, description).await
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(milestone)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "get_milestone" => {
                let number = params.as_ref()
                    .and_then(|p| p.get("number"))
                    .and_then(value_as_i64)
                    .ok_or_else(|| ExecutionError::MissingParameter("number".to_string()))?;

                let milestone = backend.get_milestone(number).await
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(milestone)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "create_pr" => {
                let title = params.as_ref()
                    .and_then(|p| p.get("title"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("title".to_string()))?;

                let body = params.as_ref()
                    .and_then(|p| p.get("body"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let head = params.as_ref()
                    .and_then(|p| p.get("head"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("head".to_string()))?;

                let base = params.as_ref()
                    .and_then(|p| p.get("base"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ExecutionError::MissingParameter("base".to_string()))?;

                let request = crate::primitives::platform::CreatePrRequest {
                    title: title.to_string(),
                    body: body.to_string(),
                    head: head.to_string(),
                    base: base.to_string(),
                };

                let pr = backend.create_pr(request).await
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(pr)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "get_pr" => {
                let number = params.as_ref()
                    .and_then(|p| p.get("number"))
                    .and_then(value_as_i64)
                    .ok_or_else(|| ExecutionError::MissingParameter("number".to_string()))?;

                let pr = backend.get_pr(number).await
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                serde_json::to_value(pr)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))
            }
            "merge_pr" => {
                let number = params.as_ref()
                    .and_then(|p| p.get("number"))
                    .and_then(value_as_i64)
                    .ok_or_else(|| ExecutionError::MissingParameter("number".to_string()))?;

                let strategy_str = params.as_ref()
                    .and_then(|p| p.get("strategy"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("merge");

                let strategy = match strategy_str {
                    "squash" => crate::primitives::platform::MergeStrategy::Squash,
                    "rebase" => crate::primitives::platform::MergeStrategy::Rebase,
                    _ => crate::primitives::platform::MergeStrategy::Merge,
                };

                backend.merge_pr(number, strategy).await
                    .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

                Ok(serde_json::Value::Null)
            }
            _ => Err(ExecutionError::InterfaceError(format!(
                "unknown platform method: {}",
                method
            ))),
        }
    }
}
