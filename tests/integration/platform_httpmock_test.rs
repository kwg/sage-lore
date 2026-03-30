//! Integration tests for ForgejoBackend using httpmock.
//!
//! These tests verify HTTP request formatting, response parsing, error handling,
//! and pagination behavior against a mock HTTP server.

use httpmock::prelude::*;
use sage_lore::primitives::platform::{
    CreateIssueRequest, CreatePrRequest, ForgejoBackend, IssueFilter, MergeStrategy, Platform,
    PlatformError,
};
use serde_json::json;

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a ForgejoBackend pointed at the mock server.
fn create_backend(server: &MockServer) -> ForgejoBackend {
    ForgejoBackend::new(&server.url(""), "kai/test-repo", "test-token")
}

/// Standard issue JSON response from Forgejo API.
fn issue_json(number: i64) -> serde_json::Value {
    json!({
        "number": number,
        "title": format!("Test Issue #{}", number),
        "body": "Test body content",
        "state": "open",
        "labels": [{"name": "bug"}, {"name": "priority:high"}],
        "milestone": {"id": 1, "title": "v1.0", "description": "", "state": "open", "open_issues": 5, "closed_issues": 2},
        "assignees": [{"login": "testuser"}],
        "created_at": "2025-01-15T10:00:00Z",
        "updated_at": "2025-01-15T12:00:00Z",
        "closed_at": null,
        "html_url": format!("http://forge.example.com/kai/test-repo/issues/{}", number)
    })
}

/// Standard milestone JSON response from Forgejo API.
fn milestone_json(id: i64) -> serde_json::Value {
    json!({
        "id": id,
        "title": format!("Milestone #{}", id),
        "description": "Test milestone description",
        "state": "open",
        "due_on": null,
        "open_issues": 10,
        "closed_issues": 5
    })
}

/// Standard comment JSON response from Forgejo API.
fn comment_json(id: i64) -> serde_json::Value {
    json!({
        "id": id,
        "body": format!("Comment #{}", id),
        "user": {"login": "commenter"},
        "created_at": "2025-01-15T10:00:00Z",
        "updated_at": "2025-01-15T10:00:00Z"
    })
}

/// Standard pull request JSON response from Forgejo API.
fn pr_json(number: i64) -> serde_json::Value {
    json!({
        "number": number,
        "title": format!("PR #{}", number),
        "body": "Pull request description",
        "state": "open",
        "head": {
            "ref": "feature-branch",
            "sha": "abc123def456"
        },
        "base": {
            "ref": "main",
            "sha": "789xyz000111"
        },
        "mergeable": true,
        "merged": false,
        "merged_at": null,
        "html_url": format!("http://forge.example.com/kai/test-repo/pulls/{}", number),
        "diff_url": format!("http://forge.example.com/kai/test-repo/pulls/{}.diff", number)
    })
}

/// Standard label JSON response.
fn label_json(name: &str) -> serde_json::Value {
    json!({
        "id": 1,
        "name": name,
        "color": "ff0000"
    })
}

// ============================================================================
// Issue Operation Tests
// ============================================================================

#[tokio::test]
async fn test_create_issue_request_format() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/repos/kai/test-repo/issues")
            .header("Authorization", "token test-token")
            .header("Content-Type", "application/json")
            .json_body(json!({
                "title": "New Feature",
                "body": "Feature description"
            }));
        then.status(201).json_body(issue_json(42));
    });

    let backend = create_backend(&server);
    let req = CreateIssueRequest {
        title: "New Feature".to_string(),
        body: "Feature description".to_string(),
        labels: None,
        milestone: None,
        assignees: None,
    };

    let result = backend.create_issue(req).unwrap();
    assert_eq!(result.number, 42);
    assert_eq!(result.title, "Test Issue #42");

    mock.assert();
}

#[tokio::test]
async fn test_create_issue_with_all_fields() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/repos/kai/test-repo/issues")
            .json_body(json!({
                "title": "Bug Report",
                "body": "Description",
                "labels": ["bug", "critical"],
                "milestone": 5,
                "assignees": ["alice", "bob"]
            }));
        then.status(201).json_body(issue_json(100));
    });

    let backend = create_backend(&server);
    let req = CreateIssueRequest {
        title: "Bug Report".to_string(),
        body: "Description".to_string(),
        labels: Some(vec!["bug".to_string(), "critical".to_string()]),
        milestone: Some(5),
        assignees: Some(vec!["alice".to_string(), "bob".to_string()]),
    };

    let result = backend.create_issue(req).unwrap();
    assert_eq!(result.number, 100);

    mock.assert();
}

#[tokio::test]
async fn test_get_issue_request_format() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues/42")
            .header("Authorization", "token test-token")
            .header("Accept", "application/json");
        then.status(200).json_body(issue_json(42));
    });

    let backend = create_backend(&server);
    let result = backend.get_issue(42).unwrap();

    assert_eq!(result.number, 42);
    assert_eq!(result.state, "open");
    assert_eq!(result.labels, vec!["bug", "priority:high"]);
    assert_eq!(result.assignees, vec!["testuser"]);
    assert_eq!(result.milestone_id, Some(1));

    mock.assert();
}

#[tokio::test]
async fn test_get_issue_not_found() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues/999");
        then.status(404).json_body(json!({"message": "Issue not found"}));
    });

    let backend = create_backend(&server);
    let result = backend.get_issue(999);

    assert!(matches!(result, Err(PlatformError::IssueNotFound(999))));
    mock.assert();
}

#[tokio::test]
async fn test_close_issue_request_format() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(PATCH)
            .path("/api/v1/repos/kai/test-repo/issues/42")
            .header("Authorization", "token test-token")
            .json_body(json!({"state": "closed"}));
        then.status(200).json_body(issue_json(42));
    });

    let backend = create_backend(&server);
    backend.close_issue(42).unwrap();

    mock.assert();
}

#[tokio::test]
async fn test_close_issue_not_found() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(PATCH)
            .path("/api/v1/repos/kai/test-repo/issues/999");
        then.status(404).json_body(json!({"message": "Issue not found"}));
    });

    let backend = create_backend(&server);
    let result = backend.close_issue(999);

    assert!(matches!(result, Err(PlatformError::IssueNotFound(999))));
    mock.assert();
}

#[tokio::test]
async fn test_list_issues_basic() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues")
            .query_param("page", "1")
            .query_param("limit", "30");
        then.status(200)
            .json_body(json!([issue_json(1), issue_json(2), issue_json(3)]));
    });

    let backend = create_backend(&server);
    let result = backend.list_issues(IssueFilter::default()).unwrap();

    assert_eq!(result.len(), 3);
    assert_eq!(result[0].number, 1);
    assert_eq!(result[1].number, 2);
    assert_eq!(result[2].number, 3);

    mock.assert();
}

#[tokio::test]
async fn test_list_issues_with_filters() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues")
            .query_param("page", "1")
            .query_param("limit", "50")
            .query_param("state", "open")
            .query_param("milestone", "5")
            .query_param("labels", "bug,critical")
            .query_param("assignee", "alice");
        then.status(200).json_body(json!([issue_json(10)]));
    });

    let backend = create_backend(&server);
    let filter = IssueFilter {
        milestone: Some(5),
        labels: Some(vec!["bug".to_string(), "critical".to_string()]),
        state: Some("open".to_string()),
        assignee: Some("alice".to_string()),
        page: None,
        per_page: Some(50),
    };

    let result = backend.list_issues(filter).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].number, 10);

    mock.assert();
}

#[tokio::test]
async fn test_list_issues_empty_response() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues");
        then.status(200).json_body(json!([]));
    });

    let backend = create_backend(&server);
    let result = backend.list_issues(IssueFilter::default()).unwrap();

    assert!(result.is_empty());
    mock.assert();
}

// ============================================================================
// Pagination Tests
// ============================================================================

#[tokio::test]
async fn test_list_issues_pagination_multiple_pages() {
    let server = MockServer::start();

    // First page - returns full page (30 items simulated with 3)
    let page1_mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues")
            .query_param("page", "1")
            .query_param("limit", "3");
        then.status(200)
            .json_body(json!([issue_json(1), issue_json(2), issue_json(3)]));
    });

    // Second page - returns partial page (indicating end)
    let page2_mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues")
            .query_param("page", "2")
            .query_param("limit", "3");
        then.status(200).json_body(json!([issue_json(4)]));
    });

    let backend = create_backend(&server);
    let filter = IssueFilter {
        per_page: Some(3),
        ..Default::default()
    };

    let result = backend.list_issues(filter).unwrap();

    assert_eq!(result.len(), 4);
    page1_mock.assert();
    page2_mock.assert();
}

#[tokio::test]
async fn test_list_issues_pagination_stops_on_empty() {
    let server = MockServer::start();

    let page1_mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues")
            .query_param("page", "1")
            .query_param("limit", "30");
        then.status(200)
            .json_body(json!([issue_json(1), issue_json(2)]));
    });

    let backend = create_backend(&server);
    let result = backend.list_issues(IssueFilter::default()).unwrap();

    // Should stop after first page since we got fewer than limit
    assert_eq!(result.len(), 2);
    page1_mock.assert();
}

#[tokio::test]
async fn test_list_issues_per_page_capped_at_100() {
    let server = MockServer::start();

    // Even if we request 200 per page, it should be capped to 100
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues")
            .query_param("limit", "100"); // Capped at 100
        then.status(200).json_body(json!([]));
    });

    let backend = create_backend(&server);
    let filter = IssueFilter {
        per_page: Some(200), // Request 200
        ..Default::default()
    };

    backend.list_issues(filter).unwrap();
    mock.assert();
}

// ============================================================================
// Label Operation Tests
// ============================================================================

#[tokio::test]
async fn test_add_labels_request_format() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/repos/kai/test-repo/issues/42/labels")
            .header("Authorization", "token test-token")
            .json_body(json!({"labels": ["bug", "enhancement"]}));
        then.status(200)
            .json_body(json!([label_json("bug"), label_json("enhancement")]));
    });

    let backend = create_backend(&server);
    backend.add_labels(42, &["bug", "enhancement"]).unwrap();

    mock.assert();
}

#[tokio::test]
async fn test_add_labels_issue_not_found() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/repos/kai/test-repo/issues/999/labels");
        then.status(404).json_body(json!({"message": "Issue not found"}));
    });

    let backend = create_backend(&server);
    let result = backend.add_labels(999, &["bug"]);

    assert!(matches!(result, Err(PlatformError::IssueNotFound(999))));
    mock.assert();
}

#[tokio::test]
async fn test_remove_labels_request_format() {
    let server = MockServer::start();

    // Forgejo removes labels one at a time
    let mock1 = server.mock(|when, then| {
        when.method(DELETE)
            .path("/api/v1/repos/kai/test-repo/issues/42/labels/bug");
        then.status(204);
    });

    let mock2 = server.mock(|when, then| {
        when.method(DELETE)
            .path("/api/v1/repos/kai/test-repo/issues/42/labels/wontfix");
        then.status(204);
    });

    let backend = create_backend(&server);
    backend.remove_labels(42, &["bug", "wontfix"]).unwrap();

    mock1.assert();
    mock2.assert();
}

#[tokio::test]
async fn test_remove_labels_issue_not_found() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(DELETE)
            .path("/api/v1/repos/kai/test-repo/issues/999/labels/bug");
        then.status(404).json_body(json!({"message": "Issue not found"}));
    });

    let backend = create_backend(&server);
    let result = backend.remove_labels(999, &["bug"]);

    assert!(matches!(result, Err(PlatformError::IssueNotFound(999))));
    mock.assert();
}

// ============================================================================
// Comment Operation Tests
// ============================================================================

#[tokio::test]
async fn test_create_comment_request_format() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/repos/kai/test-repo/issues/42/comments")
            .header("Authorization", "token test-token")
            .json_body(json!({"body": "This is a test comment"}));
        then.status(201).json_body(comment_json(100));
    });

    let backend = create_backend(&server);
    let result = backend.create_comment(42, "This is a test comment").unwrap();

    assert_eq!(result.id, 100);
    assert_eq!(result.user, "commenter");

    mock.assert();
}

#[tokio::test]
async fn test_create_comment_issue_not_found() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/repos/kai/test-repo/issues/999/comments");
        then.status(404).json_body(json!({"message": "Issue not found"}));
    });

    let backend = create_backend(&server);
    let result = backend.create_comment(999, "Comment");

    assert!(matches!(result, Err(PlatformError::IssueNotFound(999))));
    mock.assert();
}

#[tokio::test]
async fn test_get_comments_request_format() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues/42/comments")
            .header("Authorization", "token test-token");
        then.status(200)
            .json_body(json!([comment_json(1), comment_json(2)]));
    });

    let backend = create_backend(&server);
    let result = backend.get_comments(42).unwrap();

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].id, 1);
    assert_eq!(result[1].id, 2);

    mock.assert();
}

#[tokio::test]
async fn test_get_comments_empty() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues/42/comments");
        then.status(200).json_body(json!([]));
    });

    let backend = create_backend(&server);
    let result = backend.get_comments(42).unwrap();

    assert!(result.is_empty());
    mock.assert();
}

// ============================================================================
// Milestone Operation Tests
// ============================================================================

#[tokio::test]
async fn test_create_milestone_request_format() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/repos/kai/test-repo/milestones")
            .header("Authorization", "token test-token")
            .json_body(json!({"title": "v2.0", "description": "Major release"}));
        then.status(201).json_body(milestone_json(10));
    });

    let backend = create_backend(&server);
    let result = backend.create_milestone("v2.0", "Major release").unwrap();

    assert_eq!(result.id, 10);
    assert_eq!(result.state, "open");

    mock.assert();
}

#[tokio::test]
async fn test_get_milestone_request_format() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/milestones/5")
            .header("Authorization", "token test-token");
        then.status(200).json_body(milestone_json(5));
    });

    let backend = create_backend(&server);
    let result = backend.get_milestone(5).unwrap();

    assert_eq!(result.id, 5);
    assert_eq!(result.open_issues, 10);
    assert_eq!(result.closed_issues, 5);

    mock.assert();
}

#[tokio::test]
async fn test_get_milestone_not_found() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/milestones/999");
        then.status(404)
            .json_body(json!({"message": "Milestone not found"}));
    });

    let backend = create_backend(&server);
    let result = backend.get_milestone(999);

    assert!(matches!(result, Err(PlatformError::MilestoneNotFound(999))));
    mock.assert();
}

// ============================================================================
// Pull Request Operation Tests
// ============================================================================

#[tokio::test]
async fn test_create_pr_request_format() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/repos/kai/test-repo/pulls")
            .header("Authorization", "token test-token")
            .json_body(json!({
                "title": "Add new feature",
                "body": "This PR adds a great feature",
                "head": "feature-branch",
                "base": "main"
            }));
        then.status(201).json_body(pr_json(50));
    });

    let backend = create_backend(&server);
    let req = CreatePrRequest {
        title: "Add new feature".to_string(),
        body: "This PR adds a great feature".to_string(),
        head: "feature-branch".to_string(),
        base: "main".to_string(),
    };

    let result = backend.create_pr(req).unwrap();
    assert_eq!(result.number, 50);
    assert_eq!(result.head.ref_name, "feature-branch");
    assert_eq!(result.base.ref_name, "main");

    mock.assert();
}

#[tokio::test]
async fn test_get_pr_request_format() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/pulls/42")
            .header("Authorization", "token test-token");
        then.status(200).json_body(pr_json(42));
    });

    let backend = create_backend(&server);
    let result = backend.get_pr(42).unwrap();

    assert_eq!(result.number, 42);
    assert_eq!(result.state, "open");
    assert_eq!(result.mergeable, Some(true));
    assert!(!result.merged);

    mock.assert();
}

#[tokio::test]
async fn test_get_pr_not_found() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/pulls/999");
        then.status(404).json_body(json!({"message": "PR not found"}));
    });

    let backend = create_backend(&server);
    let result = backend.get_pr(999);

    assert!(matches!(result, Err(PlatformError::PrNotFound(999))));
    mock.assert();
}

#[tokio::test]
async fn test_get_pr_merged_state() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/pulls/42");
        then.status(200).json_body(json!({
            "number": 42,
            "title": "Merged PR",
            "body": "This was merged",
            "state": "closed",
            "head": {"ref": "feature", "sha": "abc123"},
            "base": {"ref": "main", "sha": "def456"},
            "mergeable": null,
            "merged": true,
            "merged_at": "2025-01-15T14:00:00Z",
            "html_url": "http://forge.example.com/pulls/42",
            "diff_url": "http://forge.example.com/pulls/42.diff"
        }));
    });

    let backend = create_backend(&server);
    let result = backend.get_pr(42).unwrap();

    assert_eq!(result.state, "merged"); // Should be converted to "merged"
    assert!(result.merged);
    assert!(result.merged_at.is_some());

    mock.assert();
}

#[tokio::test]
async fn test_merge_pr_merge_strategy() {
    let server = MockServer::start();

    // First, get_pr is called to check mergeability
    let get_mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/pulls/42");
        then.status(200).json_body(pr_json(42));
    });

    let merge_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/repos/kai/test-repo/pulls/42/merge")
            .json_body(json!({"Do": "merge"}));
        then.status(200).json_body(json!({}));
    });

    let backend = create_backend(&server);
    backend.merge_pr(42, MergeStrategy::Merge).unwrap();

    get_mock.assert();
    merge_mock.assert();
}

#[tokio::test]
async fn test_merge_pr_squash_strategy() {
    let server = MockServer::start();

    let get_mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/pulls/42");
        then.status(200).json_body(pr_json(42));
    });

    let merge_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/repos/kai/test-repo/pulls/42/merge")
            .json_body(json!({"Do": "squash"}));
        then.status(200).json_body(json!({}));
    });

    let backend = create_backend(&server);
    backend.merge_pr(42, MergeStrategy::Squash).unwrap();

    get_mock.assert();
    merge_mock.assert();
}

#[tokio::test]
async fn test_merge_pr_rebase_strategy() {
    let server = MockServer::start();

    let get_mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/pulls/42");
        then.status(200).json_body(pr_json(42));
    });

    let merge_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/repos/kai/test-repo/pulls/42/merge")
            .json_body(json!({"Do": "rebase"}));
        then.status(200).json_body(json!({}));
    });

    let backend = create_backend(&server);
    backend.merge_pr(42, MergeStrategy::Rebase).unwrap();

    get_mock.assert();
    merge_mock.assert();
}

#[tokio::test]
async fn test_merge_pr_not_mergeable() {
    let server = MockServer::start();

    let get_mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/pulls/42");
        then.status(200).json_body(json!({
            "number": 42,
            "title": "PR with conflicts",
            "body": "",
            "state": "open",
            "head": {"ref": "feature", "sha": "abc"},
            "base": {"ref": "main", "sha": "def"},
            "mergeable": false, // Not mergeable
            "merged": false,
            "merged_at": null,
            "html_url": "http://forge.example.com/pulls/42",
            "diff_url": "http://forge.example.com/pulls/42.diff"
        }));
    });

    let backend = create_backend(&server);
    let result = backend.merge_pr(42, MergeStrategy::Merge);

    assert!(matches!(result, Err(PlatformError::NotMergeable(_))));
    get_mock.assert();
}

#[tokio::test]
async fn test_merge_pr_conflict_error() {
    let server = MockServer::start();

    let get_mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/pulls/42");
        then.status(200).json_body(pr_json(42)); // mergeable is true
    });

    let merge_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/repos/kai/test-repo/pulls/42/merge");
        then.status(409)
            .json_body(json!({"message": "merge conflict"}));
    });

    let backend = create_backend(&server);
    let result = backend.merge_pr(42, MergeStrategy::Merge);

    assert!(matches!(result, Err(PlatformError::MergeConflict(_))));
    get_mock.assert();
    merge_mock.assert();
}

#[tokio::test]
async fn test_merge_pr_not_found() {
    let server = MockServer::start();

    let get_mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/pulls/999");
        then.status(404).json_body(json!({"message": "PR not found"}));
    });

    let backend = create_backend(&server);
    let result = backend.merge_pr(999, MergeStrategy::Merge);

    assert!(matches!(result, Err(PlatformError::PrNotFound(999))));
    get_mock.assert();
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_authentication_failed() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues/42");
        then.status(401).json_body(json!({"message": "Unauthorized"}));
    });

    let backend = create_backend(&server);
    let result = backend.get_issue(42);

    assert!(matches!(result, Err(PlatformError::AuthenticationFailed)));
    mock.assert();
}

#[tokio::test]
async fn test_rate_limited_with_retry_after() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues/42");
        then.status(429)
            .header("retry-after", "120")
            .json_body(json!({"message": "Rate limited"}));
    });

    let backend = create_backend(&server);
    let result = backend.get_issue(42);

    match result {
        Err(PlatformError::RateLimited(seconds)) => assert_eq!(seconds, 120),
        _ => panic!("Expected RateLimited error"),
    }
    mock.assert();
}

#[tokio::test]
async fn test_rate_limited_default_retry() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues/42");
        then.status(429).json_body(json!({"message": "Rate limited"}));
    });

    let backend = create_backend(&server);
    let result = backend.get_issue(42);

    match result {
        Err(PlatformError::RateLimited(seconds)) => assert_eq!(seconds, 60), // Default
        _ => panic!("Expected RateLimited error"),
    }
    mock.assert();
}

#[tokio::test]
async fn test_server_error() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues/42");
        then.status(500)
            .json_body(json!({"message": "Internal server error"}));
    });

    let backend = create_backend(&server);
    let result = backend.get_issue(42);

    match result {
        Err(PlatformError::ApiError { status, message }) => {
            assert_eq!(status, 500);
            assert_eq!(message, "Internal server error");
        }
        _ => panic!("Expected ApiError"),
    }
    mock.assert();
}

#[tokio::test]
async fn test_conflict_non_merge() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/repos/kai/test-repo/issues");
        then.status(409)
            .json_body(json!({"message": "Issue already exists"}));
    });

    let backend = create_backend(&server);
    let req = CreateIssueRequest {
        title: "Duplicate".to_string(),
        body: "Body".to_string(),
        labels: None,
        milestone: None,
        assignees: None,
    };
    let result = backend.create_issue(req);

    // Should be regular ApiError since it's not merge-related
    match result {
        Err(PlatformError::ApiError { status: 409, .. }) => {}
        _ => panic!("Expected ApiError with status 409"),
    }
    mock.assert();
}

#[tokio::test]
async fn test_json_parse_error() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues/42");
        then.status(200).body("not valid json");
    });

    let backend = create_backend(&server);
    let result = backend.get_issue(42);

    assert!(matches!(result, Err(PlatformError::Parse(_))));
    mock.assert();
}

#[tokio::test]
async fn test_missing_optional_fields() {
    let server = MockServer::start();

    // Response with many optional fields as null
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues/42");
        then.status(200).json_body(json!({
            "number": 42,
            "title": "Minimal Issue",
            "body": null,
            "state": "open",
            "labels": null,
            "milestone": null,
            "assignees": null,
            "created_at": "2025-01-15T10:00:00Z",
            "updated_at": "2025-01-15T10:00:00Z",
            "closed_at": null,
            "html_url": "http://forge.example.com/issues/42"
        }));
    });

    let backend = create_backend(&server);
    let result = backend.get_issue(42).unwrap();

    assert_eq!(result.number, 42);
    assert_eq!(result.body, ""); // null -> empty string
    assert!(result.labels.is_empty()); // null -> empty vec
    assert!(result.milestone_id.is_none()); // null -> None
    assert!(result.assignees.is_empty()); // null -> empty vec

    mock.assert();
}

// ============================================================================
// Response Parsing Tests
// ============================================================================

#[tokio::test]
async fn test_issue_labels_parsing() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues/42");
        then.status(200).json_body(json!({
            "number": 42,
            "title": "Issue with labels",
            "body": "",
            "state": "open",
            "labels": [
                {"name": "bug", "color": "d73a4a"},
                {"name": "enhancement", "color": "a2eeef"},
                {"name": "priority:critical", "color": "ff0000"}
            ],
            "milestone": null,
            "assignees": [],
            "created_at": "2025-01-15T10:00:00Z",
            "updated_at": "2025-01-15T10:00:00Z",
            "closed_at": null,
            "html_url": "http://forge.example.com/issues/42"
        }));
    });

    let backend = create_backend(&server);
    let result = backend.get_issue(42).unwrap();

    assert_eq!(result.labels.len(), 3);
    assert!(result.labels.contains(&"bug".to_string()));
    assert!(result.labels.contains(&"enhancement".to_string()));
    assert!(result.labels.contains(&"priority:critical".to_string()));

    mock.assert();
}

#[tokio::test]
async fn test_pr_branch_parsing() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/pulls/42");
        then.status(200).json_body(json!({
            "number": 42,
            "title": "Test PR",
            "body": "Description",
            "state": "open",
            "head": {
                "ref": "feature/new-stuff",
                "sha": "abc123456789"
            },
            "base": {
                "ref": "develop",
                "sha": "xyz987654321"
            },
            "mergeable": true,
            "merged": false,
            "merged_at": null,
            "html_url": "http://forge.example.com/pulls/42",
            "diff_url": "http://forge.example.com/pulls/42.diff"
        }));
    });

    let backend = create_backend(&server);
    let result = backend.get_pr(42).unwrap();

    assert_eq!(result.head.ref_name, "feature/new-stuff");
    assert_eq!(result.head.sha, "abc123456789");
    assert_eq!(result.base.ref_name, "develop");
    assert_eq!(result.base.sha, "xyz987654321");

    mock.assert();
}

#[tokio::test]
async fn test_datetime_parsing() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues/42");
        then.status(200).json_body(json!({
            "number": 42,
            "title": "Issue",
            "body": "",
            "state": "closed",
            "labels": [],
            "milestone": null,
            "assignees": [],
            "created_at": "2025-01-10T08:30:00Z",
            "updated_at": "2025-01-15T14:45:30Z",
            "closed_at": "2025-01-15T14:45:30Z",
            "html_url": "http://forge.example.com/issues/42"
        }));
    });

    let backend = create_backend(&server);
    let result = backend.get_issue(42).unwrap();

    assert!(result.closed_at.is_some());
    // Verify dates parsed correctly (not checking exact values since they're UTC)
    assert!(result.created_at < result.updated_at);

    mock.assert();
}

// ============================================================================
// Backend Configuration Tests
// ============================================================================

#[tokio::test]
async fn test_trailing_slash_handling() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues/42");
        then.status(200).json_body(issue_json(42));
    });

    // Create backend with trailing slash - should be handled
    let backend = ForgejoBackend::new(&format!("{}/", server.url("")), "kai/test-repo", "token");
    let result = backend.get_issue(42);

    assert!(result.is_ok());
    mock.assert();
}

#[tokio::test]
async fn test_authorization_header_format() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/repos/kai/test-repo/issues/42")
            .header("Authorization", "token my-secret-token");
        then.status(200).json_body(issue_json(42));
    });

    let backend = ForgejoBackend::new(&server.url(""), "kai/test-repo", "my-secret-token");
    backend.get_issue(42).unwrap();

    mock.assert();
}

#[tokio::test]
async fn test_debug_redacts_token() {
    let backend = ForgejoBackend::new("http://example.com", "kai/repo", "secret-token");
    let debug = format!("{:?}", backend);

    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains("secret-token"));
    assert!(debug.contains("http://example.com"));
    assert!(debug.contains("kai/repo"));
}
