// SPDX-License-Identifier: MIT
//! Platform primitive for forge API operations.
//!
//! Provides abstraction over Forgejo/GitHub/GitLab APIs for issue tracking,
//! pull requests, labels, milestones, and comments.

mod forgejo;
mod forgejo_api;
mod mock;
mod r#trait;
mod types;

// Re-export all public types
pub use forgejo::ForgejoBackend;
pub use mock::{MockPlatform, MockResponse, MockResponseKey, PlatformCall};
pub use r#trait::Platform;
pub use types::{
    Comment, CreateIssueRequest, CreatePrRequest, Issue, IssueFilter, MergeStrategy, Milestone,
    PlatformError, PlatformResult, PrBranch, PullRequest,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_issue_filter_default() {
        let filter = IssueFilter::default();
        assert!(filter.milestone.is_none());
        assert!(filter.labels.is_none());
        assert!(filter.state.is_none());
        assert!(filter.assignee.is_none());
        assert!(filter.page.is_none());
        assert!(filter.per_page.is_none());
    }

    #[test]
    fn test_merge_strategy_equality() {
        assert_eq!(MergeStrategy::Merge, MergeStrategy::Merge);
        assert_ne!(MergeStrategy::Merge, MergeStrategy::Squash);
        assert_ne!(MergeStrategy::Squash, MergeStrategy::Rebase);
    }

    #[test]
    fn test_platform_error_display() {
        let err = PlatformError::IssueNotFound(42);
        assert_eq!(err.to_string(), "Issue not found: 42");

        let err = PlatformError::AuthenticationFailed;
        assert_eq!(err.to_string(), "Authentication failed");

        let err = PlatformError::RateLimited(60);
        assert_eq!(err.to_string(), "Rate limited, retry after 60 seconds");

        let err = PlatformError::ApiError {
            status: 500,
            message: "Internal server error".to_string(),
        };
        assert_eq!(err.to_string(), "API error (500): Internal server error");
    }

    #[test]
    fn test_create_issue_request_serialization() {
        let req = CreateIssueRequest {
            title: "Test issue".to_string(),
            body: "Test body".to_string(),
            labels: Some(vec!["bug".to_string()]),
            milestone: Some(1),
            assignees: None,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("Test issue"));
        assert!(json.contains("bug"));
    }
}
