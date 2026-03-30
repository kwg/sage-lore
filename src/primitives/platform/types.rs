// SPDX-License-Identifier: MIT
//! Type definitions for the platform primitive.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Issue representation from the forge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub number: i64,
    pub title: String,
    pub body: String,
    /// State: "open" | "closed"
    pub state: String,
    pub labels: Vec<String>,
    pub milestone_id: Option<i64>,
    pub assignees: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub html_url: String,
}

/// Request to create an issue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateIssueRequest {
    pub title: String,
    pub body: String,
    pub labels: Option<Vec<String>>,
    pub milestone: Option<i64>,
    pub assignees: Option<Vec<String>>,
}

/// Comment on an issue or pull request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: i64,
    pub body: String,
    pub user: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Milestone representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    pub id: i64,
    pub title: String,
    pub description: String,
    /// State: "open" | "closed"
    pub state: String,
    pub due_on: Option<DateTime<Utc>>,
    pub open_issues: i64,
    pub closed_issues: i64,
}

/// Branch reference for pull requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrBranch {
    /// Branch name
    pub ref_name: String,
    /// Head commit SHA
    pub sha: String,
}

/// Pull request representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub number: i64,
    pub title: String,
    pub body: String,
    /// State: "open" | "closed" | "merged"
    pub state: String,
    pub head: PrBranch,
    pub base: PrBranch,
    pub mergeable: Option<bool>,
    pub merged: bool,
    pub merged_at: Option<DateTime<Utc>>,
    pub html_url: String,
    pub diff_url: String,
}

/// Request to create a pull request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreatePrRequest {
    pub title: String,
    pub body: String,
    /// Source branch
    pub head: String,
    /// Target branch
    pub base: String,
}

/// Merge strategies for pull requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergeStrategy {
    /// Create merge commit
    Merge,
    /// Squash and merge
    Squash,
    /// Rebase and merge
    Rebase,
}

/// Filter for listing issues.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IssueFilter {
    pub milestone: Option<i64>,
    pub labels: Option<Vec<String>>,
    /// State: "open" | "closed" | "all"
    pub state: Option<String>,
    pub assignee: Option<String>,
    pub page: Option<i64>,
    /// Default: 30, Max: 100
    pub per_page: Option<i64>,
}

/// Platform errors.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("Issue not found: {0}")]
    IssueNotFound(i64),

    #[error("PR not found: {0}")]
    PrNotFound(i64),

    #[error("Milestone not found: {0}")]
    MilestoneNotFound(i64),

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Rate limited, retry after {0} seconds")]
    RateLimited(u64),

    #[error("Merge conflict: {0}")]
    MergeConflict(String),

    #[error("PR not mergeable: {0}")]
    NotMergeable(String),

    #[error("API error ({status}): {message}")]
    ApiError { status: u16, message: String },

    #[error("Network error: {0}")]
    Network(String),

    /// JSON parse error — stores message as String for Clone support (SF5, #185).
    #[error("JSON parse error: {0}")]
    Parse(String),
}

impl Clone for PlatformError {
    fn clone(&self) -> Self {
        match self {
            PlatformError::IssueNotFound(n) => PlatformError::IssueNotFound(*n),
            PlatformError::PrNotFound(n) => PlatformError::PrNotFound(*n),
            PlatformError::MilestoneNotFound(n) => PlatformError::MilestoneNotFound(*n),
            PlatformError::AuthenticationFailed => PlatformError::AuthenticationFailed,
            PlatformError::RateLimited(n) => PlatformError::RateLimited(*n),
            PlatformError::MergeConflict(s) => PlatformError::MergeConflict(s.clone()),
            PlatformError::NotMergeable(s) => PlatformError::NotMergeable(s.clone()),
            PlatformError::ApiError { status, message } => PlatformError::ApiError {
                status: *status,
                message: message.clone(),
            },
            PlatformError::Network(s) => PlatformError::Network(s.clone()),
            PlatformError::Parse(s) => PlatformError::Parse(s.clone()),
        }
    }

}

impl From<serde_json::Error> for PlatformError {
    fn from(e: serde_json::Error) -> Self {
        PlatformError::Parse(e.to_string())
    }
}

/// Result type for platform operations.
pub type PlatformResult<T> = Result<T, PlatformError>;
