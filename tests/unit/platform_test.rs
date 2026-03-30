//! Unit tests for Platform trait and MockPlatform.

use chrono::Utc;
use sage_lore::primitives::platform::{
    Comment, CreateIssueRequest, CreatePrRequest, Issue, IssueFilter, MergeStrategy, Milestone,
    MockPlatform, MockResponse, MockResponseKey, Platform, PlatformCall, PlatformError,
    PrBranch, PullRequest,
};

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a sample issue for testing.
fn sample_issue(number: i64) -> Issue {
    Issue {
        number,
        title: format!("Test Issue #{}", number),
        body: "Test body".to_string(),
        state: "open".to_string(),
        labels: vec!["bug".to_string()],
        milestone_id: Some(1),
        assignees: vec!["testuser".to_string()],
        created_at: Utc::now(),
        updated_at: Utc::now(),
        closed_at: None,
        html_url: format!("http://forge.example.com/issues/{}", number),
    }
}

/// Create a sample comment for testing.
fn sample_comment(id: i64) -> Comment {
    Comment {
        id,
        body: format!("Comment #{}", id),
        user: "testuser".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

/// Create a sample milestone for testing.
fn sample_milestone(id: i64) -> Milestone {
    Milestone {
        id,
        title: format!("Milestone #{}", id),
        description: "Test milestone".to_string(),
        state: "open".to_string(),
        due_on: None,
        open_issues: 5,
        closed_issues: 3,
    }
}

/// Create a sample pull request for testing.
fn sample_pr(number: i64) -> PullRequest {
    PullRequest {
        number,
        title: format!("PR #{}", number),
        body: "Test PR body".to_string(),
        state: "open".to_string(),
        head: PrBranch {
            ref_name: "feature-branch".to_string(),
            sha: "abc123".to_string(),
        },
        base: PrBranch {
            ref_name: "main".to_string(),
            sha: "def456".to_string(),
        },
        mergeable: Some(true),
        merged: false,
        merged_at: None,
        html_url: format!("http://forge.example.com/pulls/{}", number),
        diff_url: format!("http://forge.example.com/pulls/{}.diff", number),
    }
}

// ============================================================================
// MockPlatform Construction Tests
// ============================================================================

#[tokio::test]
async fn test_mock_platform_new() {
    let mock = MockPlatform::new();
    assert_eq!(mock.total_calls(), 0);
    assert!(mock.calls().is_empty());
}

#[tokio::test]
async fn test_mock_platform_default() {
    let mock = MockPlatform::default();
    assert_eq!(mock.total_calls(), 0);
}

#[tokio::test]
async fn test_mock_platform_debug() {
    let mock = MockPlatform::new();
    let debug = format!("{:?}", mock);
    assert!(debug.contains("MockPlatform"));
    assert!(debug.contains("calls"));
}

// ============================================================================
// Issue Operation Tests
// ============================================================================

#[tokio::test]
async fn test_create_issue_with_configured_response() {
    let expected_issue = sample_issue(42);
    let mock = MockPlatform::new()
        .with_response(MockResponseKey::CreateIssue, MockResponse::Issue(expected_issue.clone()));

    let req = CreateIssueRequest {
        title: "New Issue".to_string(),
        body: "Issue body".to_string(),
        labels: Some(vec!["enhancement".to_string()]),
        milestone: Some(1),
        assignees: None,
    };

    let result = mock.create_issue(req.clone()).await.unwrap();
    assert_eq!(result.number, 42);
    assert_eq!(result.title, expected_issue.title);

    // Verify call was recorded
    assert!(mock.was_called(&PlatformCall::CreateIssue(req)));
    assert_eq!(mock.total_calls(), 1);
}

#[tokio::test]
async fn test_create_issue_with_default_issue() {
    let default_issue = sample_issue(99);
    let mock = MockPlatform::new().with_default_issue(default_issue.clone());

    let req = CreateIssueRequest {
        title: "Any Issue".to_string(),
        body: "Body".to_string(),
        labels: None,
        milestone: None,
        assignees: None,
    };

    let result = mock.create_issue(req).await.unwrap();
    assert_eq!(result.number, 99);
}

#[tokio::test]
async fn test_create_issue_error_response() {
    let mock = MockPlatform::new().with_response(
        MockResponseKey::CreateIssue,
        MockResponse::Error(PlatformError::AuthenticationFailed),
    );

    let req = CreateIssueRequest {
        title: "Test".to_string(),
        body: "Body".to_string(),
        labels: None,
        milestone: None,
        assignees: None,
    };

    let result = mock.create_issue(req).await;
    assert!(matches!(result, Err(PlatformError::AuthenticationFailed)));
}

#[tokio::test]
async fn test_create_issue_no_configured_response() {
    let mock = MockPlatform::new();

    let req = CreateIssueRequest {
        title: "Test".to_string(),
        body: "Body".to_string(),
        labels: None,
        milestone: None,
        assignees: None,
    };

    let result = mock.create_issue(req).await;
    assert!(matches!(result, Err(PlatformError::ApiError { status: 500, .. })));
}

#[tokio::test]
async fn test_get_issue_with_configured_response() {
    let expected_issue = sample_issue(42);
    let mock = MockPlatform::new()
        .with_response(MockResponseKey::GetIssue(42), MockResponse::Issue(expected_issue.clone()));

    let result = mock.get_issue(42).await.unwrap();
    assert_eq!(result.number, 42);
    assert_eq!(result.title, expected_issue.title);

    assert!(mock.was_called(&PlatformCall::GetIssue(42)));
}

#[tokio::test]
async fn test_get_issue_with_default_issue() {
    let default_issue = sample_issue(1);
    let mock = MockPlatform::new().with_default_issue(default_issue);

    // Request a different issue number - default should be used with number updated
    let result = mock.get_issue(99).await.unwrap();
    assert_eq!(result.number, 99); // Number should be updated
}

#[tokio::test]
async fn test_get_issue_not_found() {
    let mock = MockPlatform::new();

    let result = mock.get_issue(999).await;
    assert!(matches!(result, Err(PlatformError::IssueNotFound(999))));
}

#[tokio::test]
async fn test_close_issue_default_success() {
    let mock = MockPlatform::new();

    let result = mock.close_issue(42).await;
    assert!(result.is_ok());
    assert!(mock.was_called(&PlatformCall::CloseIssue(42)));
}

#[tokio::test]
async fn test_close_issue_with_explicit_success() {
    let mock = MockPlatform::new()
        .with_response(MockResponseKey::CloseIssue(42), MockResponse::Unit);

    let result = mock.close_issue(42).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_close_issue_error() {
    let mock = MockPlatform::new().with_response(
        MockResponseKey::CloseIssue(42),
        MockResponse::Error(PlatformError::IssueNotFound(42)),
    );

    let result = mock.close_issue(42).await;
    assert!(matches!(result, Err(PlatformError::IssueNotFound(42))));
}

#[tokio::test]
async fn test_list_issues_with_configured_response() {
    let issues = vec![sample_issue(1), sample_issue(2), sample_issue(3)];
    let mock = MockPlatform::new()
        .with_response(MockResponseKey::ListIssues, MockResponse::Issues(issues.clone()));

    let filter = IssueFilter {
        state: Some("open".to_string()),
        labels: Some(vec!["bug".to_string()]),
        ..Default::default()
    };

    let result = mock.list_issues(filter.clone()).await.unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].number, 1);

    assert!(mock.was_called(&PlatformCall::ListIssues(filter)));
}

#[tokio::test]
async fn test_list_issues_empty_default() {
    let mock = MockPlatform::new();

    let result = mock.list_issues(IssueFilter::default()).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_list_issues_error() {
    let mock = MockPlatform::new().with_response(
        MockResponseKey::ListIssues,
        MockResponse::Error(PlatformError::RateLimited(60)),
    );

    let result = mock.list_issues(IssueFilter::default()).await;
    assert!(matches!(result, Err(PlatformError::RateLimited(60))));
}

// ============================================================================
// Label Operation Tests
// ============================================================================

#[tokio::test]
async fn test_add_labels_default_success() {
    let mock = MockPlatform::new();

    let result = mock.add_labels(42, &["bug", "priority:high"]).await;
    assert!(result.is_ok());

    assert!(mock.was_called(&PlatformCall::AddLabels {
        issue: 42,
        labels: vec!["bug".to_string(), "priority:high".to_string()],
    }));
}

#[tokio::test]
async fn test_add_labels_error() {
    let mock = MockPlatform::new().with_response(
        MockResponseKey::AddLabels(42),
        MockResponse::Error(PlatformError::IssueNotFound(42)),
    );

    let result = mock.add_labels(42, &["bug"]).await;
    assert!(matches!(result, Err(PlatformError::IssueNotFound(42))));
}

#[tokio::test]
async fn test_remove_labels_default_success() {
    let mock = MockPlatform::new();

    let result = mock.remove_labels(42, &["wontfix"]).await;
    assert!(result.is_ok());

    assert!(mock.was_called(&PlatformCall::RemoveLabels {
        issue: 42,
        labels: vec!["wontfix".to_string()],
    }));
}

#[tokio::test]
async fn test_remove_labels_error() {
    let mock = MockPlatform::new().with_response(
        MockResponseKey::RemoveLabels(42),
        MockResponse::Error(PlatformError::ApiError {
            status: 404,
            message: "Label not found".to_string(),
        }),
    );

    let result = mock.remove_labels(42, &["nonexistent"]).await;
    assert!(matches!(result, Err(PlatformError::ApiError { status: 404, .. })));
}

// ============================================================================
// Comment Operation Tests
// ============================================================================

#[tokio::test]
async fn test_create_comment_with_configured_response() {
    let expected_comment = sample_comment(100);
    let mock = MockPlatform::new()
        .with_response(MockResponseKey::CreateComment(42), MockResponse::Comment(expected_comment.clone()));

    let result = mock.create_comment(42, "Test comment").await.unwrap();
    assert_eq!(result.id, 100);

    assert!(mock.was_called(&PlatformCall::CreateComment {
        issue: 42,
        body: "Test comment".to_string(),
    }));
}

#[tokio::test]
async fn test_create_comment_with_default_comment() {
    let default_comment = sample_comment(999);
    let mock = MockPlatform::new().with_default_comment(default_comment);

    let result = mock.create_comment(42, "Any comment").await.unwrap();
    assert_eq!(result.id, 999);
}

#[tokio::test]
async fn test_create_comment_error() {
    let mock = MockPlatform::new().with_response(
        MockResponseKey::CreateComment(42),
        MockResponse::Error(PlatformError::IssueNotFound(42)),
    );

    let result = mock.create_comment(42, "Comment").await;
    assert!(matches!(result, Err(PlatformError::IssueNotFound(42))));
}

#[tokio::test]
async fn test_get_comments_with_configured_response() {
    let comments = vec![sample_comment(1), sample_comment(2)];
    let mock = MockPlatform::new()
        .with_response(MockResponseKey::GetComments(42), MockResponse::Comments(comments.clone()));

    let result = mock.get_comments(42).await.unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].id, 1);

    assert!(mock.was_called(&PlatformCall::GetComments(42)));
}

#[tokio::test]
async fn test_get_comments_empty_default() {
    let mock = MockPlatform::new();

    let result = mock.get_comments(42).await.unwrap();
    assert!(result.is_empty());
}

// ============================================================================
// Milestone Operation Tests
// ============================================================================

#[tokio::test]
async fn test_create_milestone_with_configured_response() {
    let expected_milestone = sample_milestone(10);
    let mock = MockPlatform::new()
        .with_response(MockResponseKey::CreateMilestone, MockResponse::Milestone(expected_milestone.clone()));

    let result = mock.create_milestone("v1.0", "First release").await.unwrap();
    assert_eq!(result.id, 10);

    assert!(mock.was_called(&PlatformCall::CreateMilestone {
        title: "v1.0".to_string(),
        desc: "First release".to_string(),
    }));
}

#[tokio::test]
async fn test_create_milestone_with_default_milestone() {
    let default_milestone = sample_milestone(50);
    let mock = MockPlatform::new().with_default_milestone(default_milestone);

    let result = mock.create_milestone("Any", "Any desc").await.unwrap();
    assert_eq!(result.id, 50);
}

#[tokio::test]
async fn test_create_milestone_error() {
    let mock = MockPlatform::new().with_response(
        MockResponseKey::CreateMilestone,
        MockResponse::Error(PlatformError::AuthenticationFailed),
    );

    let result = mock.create_milestone("v1.0", "Release").await;
    assert!(matches!(result, Err(PlatformError::AuthenticationFailed)));
}

#[tokio::test]
async fn test_get_milestone_with_configured_response() {
    let expected_milestone = sample_milestone(5);
    let mock = MockPlatform::new()
        .with_response(MockResponseKey::GetMilestone(5), MockResponse::Milestone(expected_milestone.clone()));

    let result = mock.get_milestone(5).await.unwrap();
    assert_eq!(result.id, 5);

    assert!(mock.was_called(&PlatformCall::GetMilestone(5)));
}

#[tokio::test]
async fn test_get_milestone_with_default_milestone() {
    let default_milestone = sample_milestone(1);
    let mock = MockPlatform::new().with_default_milestone(default_milestone);

    let result = mock.get_milestone(99).await.unwrap();
    assert_eq!(result.id, 99); // ID should be updated
}

#[tokio::test]
async fn test_get_milestone_not_found() {
    let mock = MockPlatform::new();

    let result = mock.get_milestone(999).await;
    assert!(matches!(result, Err(PlatformError::MilestoneNotFound(999))));
}

// ============================================================================
// Pull Request Operation Tests
// ============================================================================

#[tokio::test]
async fn test_create_pr_with_configured_response() {
    let expected_pr = sample_pr(100);
    let mock = MockPlatform::new()
        .with_response(MockResponseKey::CreatePr, MockResponse::PullRequest(expected_pr.clone()));

    let req = CreatePrRequest {
        title: "New Feature".to_string(),
        body: "Adds a feature".to_string(),
        head: "feature-branch".to_string(),
        base: "main".to_string(),
    };

    let result = mock.create_pr(req.clone()).await.unwrap();
    assert_eq!(result.number, 100);

    assert!(mock.was_called(&PlatformCall::CreatePr(req)));
}

#[tokio::test]
async fn test_create_pr_with_default_pr() {
    let default_pr = sample_pr(50);
    let mock = MockPlatform::new().with_default_pr(default_pr);

    let req = CreatePrRequest {
        title: "Any PR".to_string(),
        body: "Body".to_string(),
        head: "branch".to_string(),
        base: "main".to_string(),
    };

    let result = mock.create_pr(req).await.unwrap();
    assert_eq!(result.number, 50);
}

#[tokio::test]
async fn test_create_pr_error() {
    let mock = MockPlatform::new().with_response(
        MockResponseKey::CreatePr,
        MockResponse::Error(PlatformError::ApiError {
            status: 422,
            message: "Base branch does not exist".to_string(),
        }),
    );

    let req = CreatePrRequest {
        title: "PR".to_string(),
        body: "Body".to_string(),
        head: "branch".to_string(),
        base: "nonexistent".to_string(),
    };

    let result = mock.create_pr(req).await;
    assert!(matches!(result, Err(PlatformError::ApiError { status: 422, .. })));
}

#[tokio::test]
async fn test_get_pr_with_configured_response() {
    let expected_pr = sample_pr(42);
    let mock = MockPlatform::new()
        .with_response(MockResponseKey::GetPr(42), MockResponse::PullRequest(expected_pr.clone()));

    let result = mock.get_pr(42).await.unwrap();
    assert_eq!(result.number, 42);
    assert_eq!(result.head.ref_name, "feature-branch");

    assert!(mock.was_called(&PlatformCall::GetPr(42)));
}

#[tokio::test]
async fn test_get_pr_with_default_pr() {
    let default_pr = sample_pr(1);
    let mock = MockPlatform::new().with_default_pr(default_pr);

    let result = mock.get_pr(99).await.unwrap();
    assert_eq!(result.number, 99); // Number should be updated
}

#[tokio::test]
async fn test_get_pr_not_found() {
    let mock = MockPlatform::new();

    let result = mock.get_pr(999).await;
    assert!(matches!(result, Err(PlatformError::PrNotFound(999))));
}

#[tokio::test]
async fn test_merge_pr_default_success() {
    let mock = MockPlatform::new();

    let result = mock.merge_pr(42, MergeStrategy::Squash).await;
    assert!(result.is_ok());

    assert!(mock.was_called(&PlatformCall::MergePr {
        number: 42,
        strategy: MergeStrategy::Squash,
    }));
}

#[tokio::test]
async fn test_merge_pr_all_strategies() {
    let mock = MockPlatform::new();

    // Test all merge strategies
    mock.merge_pr(1, MergeStrategy::Merge).await.unwrap();
    mock.merge_pr(2, MergeStrategy::Squash).await.unwrap();
    mock.merge_pr(3, MergeStrategy::Rebase).await.unwrap();

    assert!(mock.was_called(&PlatformCall::MergePr {
        number: 1,
        strategy: MergeStrategy::Merge,
    }));
    assert!(mock.was_called(&PlatformCall::MergePr {
        number: 2,
        strategy: MergeStrategy::Squash,
    }));
    assert!(mock.was_called(&PlatformCall::MergePr {
        number: 3,
        strategy: MergeStrategy::Rebase,
    }));
}

#[tokio::test]
async fn test_merge_pr_conflict_error() {
    let mock = MockPlatform::new().with_response(
        MockResponseKey::MergePr(42),
        MockResponse::Error(PlatformError::MergeConflict("Conflicts in src/main.rs".to_string())),
    );

    let result = mock.merge_pr(42, MergeStrategy::Merge).await;
    assert!(matches!(result, Err(PlatformError::MergeConflict(_))));
}

#[tokio::test]
async fn test_merge_pr_not_mergeable_error() {
    let mock = MockPlatform::new().with_response(
        MockResponseKey::MergePr(42),
        MockResponse::Error(PlatformError::NotMergeable("PR has conflicts".to_string())),
    );

    let result = mock.merge_pr(42, MergeStrategy::Merge).await;
    assert!(matches!(result, Err(PlatformError::NotMergeable(_))));
}

// ============================================================================
// Call Recording Tests
// ============================================================================

#[tokio::test]
async fn test_call_recording_order() {
    let mock = MockPlatform::new()
        .with_default_issue(sample_issue(1))
        .with_default_comment(sample_comment(1));

    mock.get_issue(1).await.unwrap();
    mock.get_issue(2).await.unwrap();
    mock.create_comment(1, "Comment 1").await.unwrap();
    mock.close_issue(1).await.unwrap();

    let calls = mock.calls();
    assert_eq!(calls.len(), 4);
    assert_eq!(calls[0], PlatformCall::GetIssue(1));
    assert_eq!(calls[1], PlatformCall::GetIssue(2));
    assert!(matches!(calls[2], PlatformCall::CreateComment { issue: 1, .. }));
    assert_eq!(calls[3], PlatformCall::CloseIssue(1));
}

#[tokio::test]
async fn test_was_called_with_predicate() {
    let mock = MockPlatform::new();

    mock.close_issue(1).await.unwrap();
    mock.close_issue(5).await.unwrap();
    mock.close_issue(10).await.unwrap();

    // Find any close_issue call with number > 3
    let found = mock.was_called_with(|call| {
        matches!(call, PlatformCall::CloseIssue(n) if *n > 3)
    });
    assert!(found);

    // No close_issue with number > 100
    let not_found = mock.was_called_with(|call| {
        matches!(call, PlatformCall::CloseIssue(n) if *n > 100)
    });
    assert!(!not_found);
}

#[tokio::test]
async fn test_call_count() {
    let mock = MockPlatform::new().with_default_issue(sample_issue(1));

    mock.get_issue(42).await.unwrap();
    mock.get_issue(42).await.unwrap();
    mock.get_issue(42).await.unwrap();
    mock.get_issue(99).await.unwrap();

    assert_eq!(mock.call_count(&PlatformCall::GetIssue(42)), 3);
    assert_eq!(mock.call_count(&PlatformCall::GetIssue(99)), 1);
    assert_eq!(mock.call_count(&PlatformCall::GetIssue(1)), 0);
}

#[tokio::test]
async fn test_clear_calls() {
    let mock = MockPlatform::new();

    mock.close_issue(1).await.unwrap();
    mock.close_issue(2).await.unwrap();
    assert_eq!(mock.total_calls(), 2);

    mock.clear_calls();
    assert_eq!(mock.total_calls(), 0);
    assert!(mock.calls().is_empty());

    // Can continue making calls
    mock.close_issue(3).await.unwrap();
    assert_eq!(mock.total_calls(), 1);
}

#[tokio::test]
async fn test_total_calls() {
    let mock = MockPlatform::new()
        .with_default_issue(sample_issue(1))
        .with_default_milestone(sample_milestone(1));

    assert_eq!(mock.total_calls(), 0);

    mock.get_issue(1).await.unwrap();
    assert_eq!(mock.total_calls(), 1);

    mock.get_milestone(1).await.unwrap();
    assert_eq!(mock.total_calls(), 2);

    mock.close_issue(1).await.unwrap();
    assert_eq!(mock.total_calls(), 3);
}

// ============================================================================
// Error Type Tests
// ============================================================================

#[tokio::test]
async fn test_platform_error_clone() {
    let errors = vec![
        PlatformError::IssueNotFound(42),
        PlatformError::PrNotFound(10),
        PlatformError::MilestoneNotFound(5),
        PlatformError::AuthenticationFailed,
        PlatformError::RateLimited(60),
        PlatformError::MergeConflict("conflict".to_string()),
        PlatformError::NotMergeable("not mergeable".to_string()),
        PlatformError::ApiError {
            status: 500,
            message: "Server error".to_string(),
        },
        PlatformError::Network("Connection refused".to_string()),
    ];

    for err in errors {
        let cloned = err.clone();
        assert_eq!(err.to_string(), cloned.to_string());
    }
}

#[tokio::test]
async fn test_platform_error_display_all_variants() {
    assert!(PlatformError::IssueNotFound(42).to_string().contains("42"));
    assert!(PlatformError::PrNotFound(10).to_string().contains("10"));
    assert!(PlatformError::MilestoneNotFound(5).to_string().contains("5"));
    assert!(PlatformError::AuthenticationFailed.to_string().contains("Authentication"));
    assert!(PlatformError::RateLimited(60).to_string().contains("60"));
    assert!(PlatformError::MergeConflict("test".to_string()).to_string().contains("test"));
    assert!(PlatformError::NotMergeable("test".to_string()).to_string().contains("test"));
    assert!(PlatformError::Network("refused".to_string()).to_string().contains("refused"));

    let api_err = PlatformError::ApiError {
        status: 500,
        message: "Internal".to_string(),
    };
    assert!(api_err.to_string().contains("500"));
    assert!(api_err.to_string().contains("Internal"));
}

// ============================================================================
// Response Configuration Tests
// ============================================================================

#[tokio::test]
async fn test_multiple_responses_same_type() {
    // Configure different responses for different issue numbers
    let mock = MockPlatform::new()
        .with_response(MockResponseKey::GetIssue(1), MockResponse::Issue(sample_issue(1)))
        .with_response(MockResponseKey::GetIssue(2), MockResponse::Issue(sample_issue(2)))
        .with_response(
            MockResponseKey::GetIssue(3),
            MockResponse::Error(PlatformError::IssueNotFound(3)),
        );

    let issue1 = mock.get_issue(1).await.unwrap();
    assert_eq!(issue1.number, 1);

    let issue2 = mock.get_issue(2).await.unwrap();
    assert_eq!(issue2.number, 2);

    let err = mock.get_issue(3).await;
    assert!(matches!(err, Err(PlatformError::IssueNotFound(3))));
}

#[tokio::test]
async fn test_response_priority_over_default() {
    let default_issue = sample_issue(999);
    let specific_issue = sample_issue(42);

    let mock = MockPlatform::new()
        .with_default_issue(default_issue)
        .with_response(MockResponseKey::GetIssue(42), MockResponse::Issue(specific_issue));

    // Specific response should be used for issue 42
    let result = mock.get_issue(42).await.unwrap();
    assert_eq!(result.number, 42);

    // Default should be used for other issues
    let result = mock.get_issue(100).await.unwrap();
    assert_eq!(result.number, 100); // Number updated from default
}

// ============================================================================
// Edge Cases
// ============================================================================

#[tokio::test]
async fn test_empty_labels_operations() {
    let mock = MockPlatform::new();

    // Empty labels should still succeed
    mock.add_labels(42, &[]).await.unwrap();
    mock.remove_labels(42, &[]).await.unwrap();

    assert!(mock.was_called(&PlatformCall::AddLabels {
        issue: 42,
        labels: vec![],
    }));
    assert!(mock.was_called(&PlatformCall::RemoveLabels {
        issue: 42,
        labels: vec![],
    }));
}

#[tokio::test]
async fn test_empty_comment_body() {
    let mock = MockPlatform::new().with_default_comment(sample_comment(1));

    mock.create_comment(42, "").await.unwrap();

    assert!(mock.was_called(&PlatformCall::CreateComment {
        issue: 42,
        body: String::new(),
    }));
}

#[tokio::test]
async fn test_issue_filter_all_fields() {
    let mock = MockPlatform::new()
        .with_response(MockResponseKey::ListIssues, MockResponse::Issues(vec![]));

    let filter = IssueFilter {
        milestone: Some(5),
        labels: Some(vec!["bug".to_string(), "critical".to_string()]),
        state: Some("open".to_string()),
        assignee: Some("testuser".to_string()),
        page: Some(2),
        per_page: Some(50),
    };

    mock.list_issues(filter.clone()).await.unwrap();

    assert!(mock.was_called(&PlatformCall::ListIssues(filter)));
}

#[tokio::test]
async fn test_pr_request_equality() {
    let req1 = CreatePrRequest {
        title: "Test".to_string(),
        body: "Body".to_string(),
        head: "feature".to_string(),
        base: "main".to_string(),
    };

    let req2 = req1.clone();
    assert_eq!(req1, req2);

    let req3 = CreatePrRequest {
        title: "Different".to_string(),
        ..req1.clone()
    };
    assert_ne!(req1, req3);
}

#[tokio::test]
async fn test_issue_request_equality() {
    let req1 = CreateIssueRequest {
        title: "Test".to_string(),
        body: "Body".to_string(),
        labels: Some(vec!["bug".to_string()]),
        milestone: Some(1),
        assignees: Some(vec!["user".to_string()]),
    };

    let req2 = req1.clone();
    assert_eq!(req1, req2);

    let req3 = CreateIssueRequest {
        labels: None,
        ..req1.clone()
    };
    assert_ne!(req1, req3);
}

#[tokio::test]
async fn test_merge_strategy_copy() {
    let s1 = MergeStrategy::Squash;
    let s2 = s1; // Copy
    assert_eq!(s1, s2);
}

// ============================================================================
// Builder Pattern Tests
// ============================================================================

#[tokio::test]
async fn test_builder_chain() {
    let mock = MockPlatform::new()
        .with_default_issue(sample_issue(1))
        .with_default_milestone(sample_milestone(1))
        .with_default_pr(sample_pr(1))
        .with_default_comment(sample_comment(1))
        .with_response(MockResponseKey::GetIssue(42), MockResponse::Issue(sample_issue(42)));

    // All defaults should work
    assert!(mock.get_issue(99).await.is_ok());
    assert!(mock.get_milestone(99).await.is_ok());
    assert!(mock.get_pr(99).await.is_ok());
    assert!(mock.create_comment(99, "test").await.is_ok());

    // Specific response should also work
    let issue = mock.get_issue(42).await.unwrap();
    assert_eq!(issue.number, 42);
}
