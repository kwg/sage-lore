// SPDX-License-Identifier: MIT
//! Mock platform implementation for unit testing.

use async_trait::async_trait;

use super::r#trait::Platform;
use super::types::*;
use std::cell::RefCell;
use std::collections::HashMap;

/// Recorded platform operation for test verification.
#[derive(Debug, Clone, PartialEq)]
pub enum PlatformCall {
    CreateIssue(CreateIssueRequest),
    GetIssue(i64),
    CloseIssue(i64),
    ListIssues(IssueFilter),
    AddLabels {
        issue: i64,
        labels: Vec<String>,
    },
    RemoveLabels {
        issue: i64,
        labels: Vec<String>,
    },
    CreateComment {
        issue: i64,
        body: String,
    },
    GetComments(i64),
    CreateMilestone {
        title: String,
        desc: String,
    },
    GetMilestone(i64),
    CreatePr(CreatePrRequest),
    GetPr(i64),
    MergePr {
        number: i64,
        strategy: MergeStrategy,
    },
}

/// Response configuration key for MockPlatform.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MockResponseKey {
    CreateIssue,
    GetIssue(i64),
    CloseIssue(i64),
    ListIssues,
    AddLabels(i64),
    RemoveLabels(i64),
    CreateComment(i64),
    GetComments(i64),
    CreateMilestone,
    GetMilestone(i64),
    CreatePr,
    GetPr(i64),
    MergePr(i64),
}

/// Mock response that can be configured for testing.
#[derive(Debug, Clone)]
pub enum MockResponse {
    Issue(Issue),
    Issues(Vec<Issue>),
    Comment(Comment),
    Comments(Vec<Comment>),
    Milestone(Milestone),
    PullRequest(PullRequest),
    Unit,
    Error(PlatformError),
}

/// Mock platform implementation for unit testing.
///
/// Records all calls made and returns pre-configured responses.
/// Useful for testing scrolls and other code that interacts with the platform
/// without making actual HTTP requests.
///
/// # Example
///
/// ```ignore
/// use sage_method::primitives::platform::*;
///
/// let mock = MockPlatform::new()
///     .with_response(
///         MockResponseKey::GetIssue(42),
///         MockResponse::Issue(Issue { number: 42, .. })
///     );
///
/// let issue = mock.get_issue(42).unwrap();
/// assert_eq!(issue.number, 42);
/// assert!(mock.was_called(&PlatformCall::GetIssue(42)));
/// ```
pub struct MockPlatform {
    calls: RefCell<Vec<PlatformCall>>,
    responses: HashMap<MockResponseKey, MockResponse>,
    /// Default issue to return when no specific response is configured
    default_issue: Option<Issue>,
    /// Default milestone to return
    default_milestone: Option<Milestone>,
    /// Default pull request to return
    default_pr: Option<PullRequest>,
    /// Default comment to return
    default_comment: Option<Comment>,
}

impl std::fmt::Debug for MockPlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockPlatform")
            .field("calls", &self.calls)
            .field("responses", &format!("<{} configured>", self.responses.len()))
            .finish()
    }
}

impl Default for MockPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl MockPlatform {
    /// Create a new mock platform with no configured responses.
    pub fn new() -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            responses: HashMap::new(),
            default_issue: None,
            default_milestone: None,
            default_pr: None,
            default_comment: None,
        }
    }

    /// Configure a response for a specific operation.
    pub fn with_response(mut self, key: MockResponseKey, response: MockResponse) -> Self {
        self.responses.insert(key, response);
        self
    }

    /// Set a default issue to return for any get_issue/create_issue call without a specific response.
    pub fn with_default_issue(mut self, issue: Issue) -> Self {
        self.default_issue = Some(issue);
        self
    }

    /// Set a default milestone to return.
    pub fn with_default_milestone(mut self, milestone: Milestone) -> Self {
        self.default_milestone = Some(milestone);
        self
    }

    /// Set a default pull request to return.
    pub fn with_default_pr(mut self, pr: PullRequest) -> Self {
        self.default_pr = Some(pr);
        self
    }

    /// Set a default comment to return.
    pub fn with_default_comment(mut self, comment: Comment) -> Self {
        self.default_comment = Some(comment);
        self
    }

    /// Get all recorded calls.
    pub fn calls(&self) -> Vec<PlatformCall> {
        self.calls.borrow().clone()
    }

    /// Check if a specific call was made.
    pub fn was_called(&self, call: &PlatformCall) -> bool {
        self.calls.borrow().contains(call)
    }

    /// Check if any call matching a predicate was made.
    pub fn was_called_with<F>(&self, predicate: F) -> bool
    where
        F: Fn(&PlatformCall) -> bool,
    {
        self.calls.borrow().iter().any(predicate)
    }

    /// Count how many times a specific call was made.
    pub fn call_count(&self, call: &PlatformCall) -> usize {
        self.calls.borrow().iter().filter(|c| *c == call).count()
    }

    /// Get the total number of calls made.
    pub fn total_calls(&self) -> usize {
        self.calls.borrow().len()
    }

    /// Clear all recorded calls (but keep configured responses).
    pub fn clear_calls(&self) {
        self.calls.borrow_mut().clear();
    }

    /// Record a call.
    fn record(&self, call: PlatformCall) {
        self.calls.borrow_mut().push(call);
    }

    /// Get a configured response, returning an error if not found.
    fn get_response(&self, key: &MockResponseKey) -> Option<&MockResponse> {
        self.responses.get(key)
    }
}

// Implement Send + Sync for MockPlatform
// SAFETY: RefCell is only accessed through &self methods, and we don't share across threads in tests
unsafe impl Send for MockPlatform {}
unsafe impl Sync for MockPlatform {}

#[async_trait]
impl Platform for MockPlatform {
    async fn create_issue(&self, req: CreateIssueRequest) -> PlatformResult<Issue> {
        self.record(PlatformCall::CreateIssue(req.clone()));

        match self.get_response(&MockResponseKey::CreateIssue) {
            Some(MockResponse::Issue(issue)) => Ok(issue.clone()),
            Some(MockResponse::Error(e)) => Err(e.clone()),
            None if self.default_issue.is_some() => Ok(self.default_issue.clone().unwrap()),
            _ => Err(PlatformError::ApiError {
                status: 500,
                message: "MockPlatform: no response configured for create_issue".to_string(),
            }),
        }
    }

    async fn get_issue(&self, number: i64) -> PlatformResult<Issue> {
        self.record(PlatformCall::GetIssue(number));

        match self.get_response(&MockResponseKey::GetIssue(number)) {
            Some(MockResponse::Issue(issue)) => Ok(issue.clone()),
            Some(MockResponse::Error(e)) => Err(e.clone()),
            None if self.default_issue.is_some() => {
                let mut issue = self.default_issue.clone().unwrap();
                issue.number = number;
                Ok(issue)
            }
            _ => Err(PlatformError::IssueNotFound(number)),
        }
    }

    async fn close_issue(&self, number: i64) -> PlatformResult<()> {
        self.record(PlatformCall::CloseIssue(number));

        match self.get_response(&MockResponseKey::CloseIssue(number)) {
            Some(MockResponse::Unit) => Ok(()),
            Some(MockResponse::Error(e)) => Err(e.clone()),
            None => Ok(()), // Default: succeed
            _ => Err(PlatformError::ApiError {
                status: 500,
                message: "MockPlatform: invalid response type for close_issue".to_string(),
            }),
        }
    }

    async fn list_issues(&self, filter: IssueFilter) -> PlatformResult<Vec<Issue>> {
        self.record(PlatformCall::ListIssues(filter));

        match self.get_response(&MockResponseKey::ListIssues) {
            Some(MockResponse::Issues(issues)) => Ok(issues.clone()),
            Some(MockResponse::Error(e)) => Err(e.clone()),
            None => Ok(Vec::new()), // Default: empty list
            _ => Err(PlatformError::ApiError {
                status: 500,
                message: "MockPlatform: invalid response type for list_issues".to_string(),
            }),
        }
    }

    async fn add_labels(&self, issue: i64, labels: &[&str]) -> PlatformResult<()> {
        self.record(PlatformCall::AddLabels {
            issue,
            labels: labels.iter().map(|s| s.to_string()).collect(),
        });

        match self.get_response(&MockResponseKey::AddLabels(issue)) {
            Some(MockResponse::Unit) => Ok(()),
            Some(MockResponse::Error(e)) => Err(e.clone()),
            None => Ok(()), // Default: succeed
            _ => Err(PlatformError::ApiError {
                status: 500,
                message: "MockPlatform: invalid response type for add_labels".to_string(),
            }),
        }
    }

    async fn remove_labels(&self, issue: i64, labels: &[&str]) -> PlatformResult<()> {
        self.record(PlatformCall::RemoveLabels {
            issue,
            labels: labels.iter().map(|s| s.to_string()).collect(),
        });

        match self.get_response(&MockResponseKey::RemoveLabels(issue)) {
            Some(MockResponse::Unit) => Ok(()),
            Some(MockResponse::Error(e)) => Err(e.clone()),
            None => Ok(()), // Default: succeed
            _ => Err(PlatformError::ApiError {
                status: 500,
                message: "MockPlatform: invalid response type for remove_labels".to_string(),
            }),
        }
    }

    async fn create_comment(&self, issue: i64, body: &str) -> PlatformResult<Comment> {
        self.record(PlatformCall::CreateComment {
            issue,
            body: body.to_string(),
        });

        match self.get_response(&MockResponseKey::CreateComment(issue)) {
            Some(MockResponse::Comment(comment)) => Ok(comment.clone()),
            Some(MockResponse::Error(e)) => Err(e.clone()),
            None if self.default_comment.is_some() => Ok(self.default_comment.clone().unwrap()),
            _ => Err(PlatformError::ApiError {
                status: 500,
                message: "MockPlatform: no response configured for create_comment".to_string(),
            }),
        }
    }

    async fn get_comments(&self, issue: i64) -> PlatformResult<Vec<Comment>> {
        self.record(PlatformCall::GetComments(issue));

        match self.get_response(&MockResponseKey::GetComments(issue)) {
            Some(MockResponse::Comments(comments)) => Ok(comments.clone()),
            Some(MockResponse::Error(e)) => Err(e.clone()),
            None => Ok(Vec::new()), // Default: empty list
            _ => Err(PlatformError::ApiError {
                status: 500,
                message: "MockPlatform: invalid response type for get_comments".to_string(),
            }),
        }
    }

    async fn create_milestone(&self, title: &str, desc: &str) -> PlatformResult<Milestone> {
        self.record(PlatformCall::CreateMilestone {
            title: title.to_string(),
            desc: desc.to_string(),
        });

        match self.get_response(&MockResponseKey::CreateMilestone) {
            Some(MockResponse::Milestone(milestone)) => Ok(milestone.clone()),
            Some(MockResponse::Error(e)) => Err(e.clone()),
            None if self.default_milestone.is_some() => {
                Ok(self.default_milestone.clone().unwrap())
            }
            _ => Err(PlatformError::ApiError {
                status: 500,
                message: "MockPlatform: no response configured for create_milestone".to_string(),
            }),
        }
    }

    async fn get_milestone(&self, id: i64) -> PlatformResult<Milestone> {
        self.record(PlatformCall::GetMilestone(id));

        match self.get_response(&MockResponseKey::GetMilestone(id)) {
            Some(MockResponse::Milestone(milestone)) => Ok(milestone.clone()),
            Some(MockResponse::Error(e)) => Err(e.clone()),
            None if self.default_milestone.is_some() => {
                let mut milestone = self.default_milestone.clone().unwrap();
                milestone.id = id;
                Ok(milestone)
            }
            _ => Err(PlatformError::MilestoneNotFound(id)),
        }
    }

    async fn create_pr(&self, req: CreatePrRequest) -> PlatformResult<PullRequest> {
        self.record(PlatformCall::CreatePr(req.clone()));

        match self.get_response(&MockResponseKey::CreatePr) {
            Some(MockResponse::PullRequest(pr)) => Ok(pr.clone()),
            Some(MockResponse::Error(e)) => Err(e.clone()),
            None if self.default_pr.is_some() => Ok(self.default_pr.clone().unwrap()),
            _ => Err(PlatformError::ApiError {
                status: 500,
                message: "MockPlatform: no response configured for create_pr".to_string(),
            }),
        }
    }

    async fn get_pr(&self, number: i64) -> PlatformResult<PullRequest> {
        self.record(PlatformCall::GetPr(number));

        match self.get_response(&MockResponseKey::GetPr(number)) {
            Some(MockResponse::PullRequest(pr)) => Ok(pr.clone()),
            Some(MockResponse::Error(e)) => Err(e.clone()),
            None if self.default_pr.is_some() => {
                let mut pr = self.default_pr.clone().unwrap();
                pr.number = number;
                Ok(pr)
            }
            _ => Err(PlatformError::PrNotFound(number)),
        }
    }

    async fn merge_pr(&self, number: i64, strategy: MergeStrategy) -> PlatformResult<()> {
        self.record(PlatformCall::MergePr { number, strategy });

        match self.get_response(&MockResponseKey::MergePr(number)) {
            Some(MockResponse::Unit) => Ok(()),
            Some(MockResponse::Error(e)) => Err(e.clone()),
            None => Ok(()), // Default: succeed
            _ => Err(PlatformError::ApiError {
                status: 500,
                message: "MockPlatform: invalid response type for merge_pr".to_string(),
            }),
        }
    }
}
