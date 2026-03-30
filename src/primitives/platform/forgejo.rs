// SPDX-License-Identifier: MIT
//! Forgejo backend implementation for the Platform trait.

use super::forgejo_api;
use super::r#trait::Platform;
use super::types::*;
use async_trait::async_trait;
use reqwest::{Client, Response};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::time::Duration;

/// Forgejo backend implementation for Platform trait.
///
/// Makes actual HTTP calls to a Forgejo REST API instance.
///
/// # Example
///
/// ```ignore
/// use sage_method::primitives::platform::*;
///
/// let backend = ForgejoBackend::new(
///     "http://forgejo.example.com",
///     "owner/repo",
///     "api_token_here",
/// );
///
/// let issue = backend.get_issue(42)?;
/// println!("Issue #{}: {}", issue.number, issue.title);
/// ```
pub struct ForgejoBackend {
    client: Client,
    base_url: String,
    repo: String,
    token: String,
}

impl std::fmt::Debug for ForgejoBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForgejoBackend")
            .field("base_url", &self.base_url)
            .field("repo", &self.repo)
            .field("token", &"[REDACTED]")
            .finish()
    }
}

impl ForgejoBackend {
    /// Create a new Forgejo backend.
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL of the Forgejo instance (e.g., "https://forgejo.example.com")
    /// * `repo` - Repository in "owner/repo" format
    /// * `token` - API token for authentication
    pub fn new(base_url: &str, repo: &str, token: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            repo: repo.to_string(),
            token: token.to_string(),
        }
    }

    /// Create a new Forgejo backend with a custom client (useful for testing).
    pub fn with_client(client: Client, base_url: &str, repo: &str, token: &str) -> Self {
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            repo: repo.to_string(),
            token: token.to_string(),
        }
    }

    /// Build the API URL for a given path.
    fn api_url(&self, path: &str) -> String {
        format!("{}/api/v1/repos/{}{}", self.base_url, self.repo, path)
    }

    /// Perform a GET request.
    async fn get<T: DeserializeOwned>(&self, url: &str) -> PlatformResult<T> {
        let response = self
            .client
            .get(url)
            .header("Authorization", format!("token {}", self.token))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| PlatformError::Network(e.to_string()))?;

        self.handle_response(response).await
    }

    /// Perform a POST request with JSON body.
    async fn post<T: DeserializeOwned, B: Serialize + Send + Sync>(&self, url: &str, body: &B) -> PlatformResult<T> {
        let response = self
            .client
            .post(url)
            .header("Authorization", format!("token {}", self.token))
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| PlatformError::Network(e.to_string()))?;

        self.handle_response(response).await
    }

    /// Perform a PATCH request with JSON body.
    async fn patch<T: DeserializeOwned, B: Serialize + Send + Sync>(&self, url: &str, body: &B) -> PlatformResult<T> {
        let response = self
            .client
            .patch(url)
            .header("Authorization", format!("token {}", self.token))
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| PlatformError::Network(e.to_string()))?;

        self.handle_response(response).await
    }

    /// Perform a PUT request with JSON body.
    #[allow(dead_code)]
    async fn put<T: DeserializeOwned, B: Serialize + Send + Sync>(&self, url: &str, body: &B) -> PlatformResult<T> {
        let response = self
            .client
            .put(url)
            .header("Authorization", format!("token {}", self.token))
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| PlatformError::Network(e.to_string()))?;

        self.handle_response(response).await
    }

    /// Perform a DELETE request.
    async fn delete(&self, url: &str) -> PlatformResult<()> {
        let response = self
            .client
            .delete(url)
            .header("Authorization", format!("token {}", self.token))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| PlatformError::Network(e.to_string()))?;

        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            // Re-use handle_response for error handling
            let _: Value = self.handle_response(response).await?;
            Ok(())
        }
    }

    /// Handle API response, converting to appropriate error types.
    async fn handle_response<T: DeserializeOwned>(&self, response: Response) -> PlatformResult<T> {
        let status = response.status();

        match status.as_u16() {
            200..=299 => {
                let text = response
                    .text()
                    .await
                    .map_err(|e| PlatformError::Network(e.to_string()))?;
                serde_json::from_str(&text).map_err(|e| PlatformError::Parse(e.to_string()))
            }
            401 => Err(PlatformError::AuthenticationFailed),
            404 => {
                let body: Value = response.json().await.unwrap_or_default();
                let message = body["message"]
                    .as_str()
                    .unwrap_or("Not found")
                    .to_string();
                Err(PlatformError::ApiError { status: 404, message })
            }
            409 => {
                let body: Value = response.json().await.unwrap_or_default();
                let message = body["message"]
                    .as_str()
                    .unwrap_or("Conflict")
                    .to_string();
                // Check if it's a merge conflict
                if message.to_lowercase().contains("merge") {
                    Err(PlatformError::MergeConflict(message))
                } else {
                    Err(PlatformError::ApiError { status: 409, message })
                }
            }
            429 => {
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(60);
                Err(PlatformError::RateLimited(retry_after))
            }
            _ => {
                let body: Value = response.json().await.unwrap_or_default();
                let message = body["message"]
                    .as_str()
                    .unwrap_or("Unknown error")
                    .to_string();
                Err(PlatformError::ApiError {
                    status: status.as_u16(),
                    message,
                })
            }
        }
    }
}

#[async_trait]
impl Platform for ForgejoBackend {
    async fn create_issue(&self, req: CreateIssueRequest) -> PlatformResult<Issue> {
        let url = self.api_url("/issues");

        #[derive(Serialize)]
        struct CreateIssueBody {
            title: String,
            body: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            labels: Option<Vec<String>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            milestone: Option<i64>,
            #[serde(skip_serializing_if = "Option::is_none")]
            assignees: Option<Vec<String>>,
        }

        let body = CreateIssueBody {
            title: req.title,
            body: req.body,
            labels: req.labels,
            milestone: req.milestone,
            assignees: req.assignees,
        };

        let response: forgejo_api::ForgejoIssue = self.post(&url, &body).await?;
        Ok(response.into())
    }

    async fn get_issue(&self, number: i64) -> PlatformResult<Issue> {
        let url = self.api_url(&format!("/issues/{}", number));
        let response: forgejo_api::ForgejoIssue = self.get(&url).await.map_err(|e| {
            if matches!(e, PlatformError::ApiError { status: 404, .. }) {
                PlatformError::IssueNotFound(number)
            } else {
                e
            }
        })?;
        Ok(response.into())
    }

    async fn close_issue(&self, number: i64) -> PlatformResult<()> {
        let url = self.api_url(&format!("/issues/{}", number));

        #[derive(Serialize)]
        struct CloseIssueBody {
            state: String,
        }

        let body = CloseIssueBody {
            state: "closed".to_string(),
        };

        let _: forgejo_api::ForgejoIssue = self.patch(&url, &body).await.map_err(|e| {
            if matches!(e, PlatformError::ApiError { status: 404, .. }) {
                PlatformError::IssueNotFound(number)
            } else {
                e
            }
        })?;
        Ok(())
    }

    async fn list_issues(&self, filter: IssueFilter) -> PlatformResult<Vec<Issue>> {
        let mut all_issues = Vec::new();
        let mut page = filter.page.unwrap_or(1);
        let per_page = filter.per_page.unwrap_or(30).min(100);

        loop {
            let mut url = format!(
                "{}?page={}&limit={}",
                self.api_url("/issues"),
                page,
                per_page
            );

            if let Some(ref state) = filter.state {
                url.push_str(&format!("&state={}", state));
            }
            if let Some(milestone) = filter.milestone {
                url.push_str(&format!("&milestone={}", milestone));
            }
            if let Some(ref labels) = filter.labels {
                url.push_str(&format!("&labels={}", labels.join(",")));
            }
            if let Some(ref assignee) = filter.assignee {
                url.push_str(&format!("&assignee={}", assignee));
            }

            let response: Vec<forgejo_api::ForgejoIssue> = self.get(&url).await?;

            if response.is_empty() {
                break;
            }

            let count = response.len();
            all_issues.extend(response.into_iter().map(Issue::from));

            // Stop if we got fewer than requested (last page) or hit safety limit
            if count < per_page as usize || all_issues.len() >= 1000 {
                break;
            }

            page += 1;
        }

        Ok(all_issues)
    }

    async fn add_labels(&self, issue: i64, labels: &[&str]) -> PlatformResult<()> {
        let url = self.api_url(&format!("/issues/{}/labels", issue));

        #[derive(Serialize)]
        struct AddLabelsBody {
            labels: Vec<String>,
        }

        let body = AddLabelsBody {
            labels: labels.iter().map(|s| s.to_string()).collect(),
        };

        // Forgejo returns the updated labels, but we don't need them
        let _: Vec<forgejo_api::ForgejoLabel> = self.post(&url, &body).await.map_err(|e| {
            if matches!(e, PlatformError::ApiError { status: 404, .. }) {
                PlatformError::IssueNotFound(issue)
            } else {
                e
            }
        })?;
        Ok(())
    }

    async fn remove_labels(&self, issue: i64, labels: &[&str]) -> PlatformResult<()> {
        // Forgejo requires removing labels one at a time
        for label in labels {
            let url = self.api_url(&format!("/issues/{}/labels/{}", issue, label));
            self.delete(&url).await.map_err(|e| {
                if matches!(e, PlatformError::ApiError { status: 404, .. }) {
                    PlatformError::IssueNotFound(issue)
                } else {
                    e
                }
            })?;
        }
        Ok(())
    }

    async fn create_comment(&self, issue: i64, body: &str) -> PlatformResult<Comment> {
        let url = self.api_url(&format!("/issues/{}/comments", issue));

        #[derive(Serialize)]
        struct CreateCommentBody {
            body: String,
        }

        let req_body = CreateCommentBody {
            body: body.to_string(),
        };

        let response: forgejo_api::ForgejoComment = self.post(&url, &req_body).await.map_err(|e| {
            if matches!(e, PlatformError::ApiError { status: 404, .. }) {
                PlatformError::IssueNotFound(issue)
            } else {
                e
            }
        })?;
        Ok(response.into())
    }

    async fn get_comments(&self, issue: i64) -> PlatformResult<Vec<Comment>> {
        let url = self.api_url(&format!("/issues/{}/comments", issue));
        let response: Vec<forgejo_api::ForgejoComment> = self.get(&url).await.map_err(|e| {
            if matches!(e, PlatformError::ApiError { status: 404, .. }) {
                PlatformError::IssueNotFound(issue)
            } else {
                e
            }
        })?;
        Ok(response.into_iter().map(Comment::from).collect())
    }

    async fn create_milestone(&self, title: &str, desc: &str) -> PlatformResult<Milestone> {
        let url = self.api_url("/milestones");

        #[derive(Serialize)]
        struct CreateMilestoneBody {
            title: String,
            description: String,
        }

        let body = CreateMilestoneBody {
            title: title.to_string(),
            description: desc.to_string(),
        };

        let response: forgejo_api::ForgejoMilestone = self.post(&url, &body).await?;
        Ok(response.into())
    }

    async fn get_milestone(&self, id: i64) -> PlatformResult<Milestone> {
        let url = self.api_url(&format!("/milestones/{}", id));
        let response: forgejo_api::ForgejoMilestone = self.get(&url).await.map_err(|e| {
            if matches!(e, PlatformError::ApiError { status: 404, .. }) {
                PlatformError::MilestoneNotFound(id)
            } else {
                e
            }
        })?;
        Ok(response.into())
    }

    async fn create_pr(&self, req: CreatePrRequest) -> PlatformResult<PullRequest> {
        let url = self.api_url("/pulls");

        #[derive(Serialize)]
        struct CreatePrBody {
            title: String,
            body: String,
            head: String,
            base: String,
        }

        let body = CreatePrBody {
            title: req.title,
            body: req.body,
            head: req.head,
            base: req.base,
        };

        let response: forgejo_api::ForgejoPullRequest = self.post(&url, &body).await?;
        Ok(response.into())
    }

    async fn get_pr(&self, number: i64) -> PlatformResult<PullRequest> {
        let url = self.api_url(&format!("/pulls/{}", number));
        let response: forgejo_api::ForgejoPullRequest = self.get(&url).await.map_err(|e| {
            if matches!(e, PlatformError::ApiError { status: 404, .. }) {
                PlatformError::PrNotFound(number)
            } else {
                e
            }
        })?;
        Ok(response.into())
    }

    async fn merge_pr(&self, number: i64, strategy: MergeStrategy) -> PlatformResult<()> {
        let url = self.api_url(&format!("/pulls/{}/merge", number));

        #[derive(Serialize)]
        struct MergePrBody {
            #[serde(rename = "Do")]
            do_action: String,
        }

        let do_action = match strategy {
            MergeStrategy::Merge => "merge",
            MergeStrategy::Squash => "squash",
            MergeStrategy::Rebase => "rebase",
        };

        let body = MergePrBody {
            do_action: do_action.to_string(),
        };

        // Check if PR is mergeable first
        let pr = self.get_pr(number).await?;
        if let Some(false) = pr.mergeable {
            return Err(PlatformError::NotMergeable(format!(
                "PR #{} has conflicts or is not mergeable",
                number
            )));
        }

        let _: Value = self.post(&url, &body).await.map_err(|e| {
            if matches!(e, PlatformError::ApiError { status: 404, .. }) {
                PlatformError::PrNotFound(number)
            } else if matches!(e, PlatformError::ApiError { status: 409, .. }) {
                PlatformError::MergeConflict(format!("PR #{} has merge conflicts", number))
            } else {
                e
            }
        })?;
        Ok(())
    }
}
