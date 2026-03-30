// SPDX-License-Identifier: MIT
//! Platform trait definition for forge API operations.

use async_trait::async_trait;

use super::types::*;

/// Platform trait for forge API operations.
///
/// Provides abstraction over different forge implementations (Forgejo, GitHub, GitLab).
/// All operations are async for non-blocking execution with tokio.
#[async_trait]
pub trait Platform: Send + Sync {
    // === Issues ===

    /// Create a new issue.
    async fn create_issue(&self, req: CreateIssueRequest) -> PlatformResult<Issue>;

    /// Get an issue by number.
    async fn get_issue(&self, number: i64) -> PlatformResult<Issue>;

    /// Close an issue.
    async fn close_issue(&self, number: i64) -> PlatformResult<()>;

    /// List issues with optional filtering.
    async fn list_issues(&self, filter: IssueFilter) -> PlatformResult<Vec<Issue>>;

    // === Labels ===

    /// Add labels to an issue.
    async fn add_labels(&self, issue: i64, labels: &[&str]) -> PlatformResult<()>;

    /// Remove labels from an issue.
    async fn remove_labels(&self, issue: i64, labels: &[&str]) -> PlatformResult<()>;

    // === Comments ===

    /// Create a comment on an issue or PR.
    async fn create_comment(&self, issue: i64, body: &str) -> PlatformResult<Comment>;

    /// Get all comments on an issue or PR.
    async fn get_comments(&self, issue: i64) -> PlatformResult<Vec<Comment>>;

    // === Milestones ===

    /// Create a new milestone.
    async fn create_milestone(&self, title: &str, desc: &str) -> PlatformResult<Milestone>;

    /// Get a milestone by ID.
    async fn get_milestone(&self, id: i64) -> PlatformResult<Milestone>;

    // === Pull Requests ===

    /// Create a new pull request.
    async fn create_pr(&self, req: CreatePrRequest) -> PlatformResult<PullRequest>;

    /// Get a pull request by number.
    async fn get_pr(&self, number: i64) -> PlatformResult<PullRequest>;

    /// Merge a pull request with the specified strategy.
    async fn merge_pr(&self, number: i64, strategy: MergeStrategy) -> PlatformResult<()>;
}
