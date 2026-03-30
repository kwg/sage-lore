//! Integration tests for primitive test scrolls.
//!
//! These tests execute the scrolls in tests/scrolls/ to verify
//! that all primitive operations are correctly wired and dispatchable.
//!
//! These tests use Executor::for_testing() which has mock backends for
//! fs, platform, test, and invoke. VCS backend is not mocked, so VCS
//! operations will return NotImplemented - scrolls should use on_fail: continue.

use std::path::Path;
use sage_lore::scroll::executor::Executor;
use sage_lore::scroll::parser::parse_scroll_file;

// ============================================================================
// FS Primitive Tests (MockFsBackend)
// ============================================================================

#[tokio::test]
async fn test_scroll_fs_basic() {
    let scroll_path = Path::new("tests/scrolls/01-fs-basic.scroll");
    let scroll = parse_scroll_file(scroll_path).expect("parse scroll");

    let mut executor = Executor::for_testing();
    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "fs-basic scroll failed: {:?}", result.err());

    // Verify outputs from mock backend
    assert!(executor.context().get_variable("write_result").is_some());
    assert!(executor.context().get_variable("read_content").is_some());
}

#[tokio::test]
async fn test_scroll_fs_extended() {
    let scroll_path = Path::new("tests/scrolls/01a-fs-extended.scroll");
    let scroll = parse_scroll_file(scroll_path).expect("parse scroll");

    let mut executor = Executor::for_testing();
    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "fs-extended scroll failed: {:?}", result.err());

    // Verify mkdir, append, stat, copy, move were exercised
    assert!(executor.context().get_variable("mkdir_result").is_some());
    assert!(executor.context().get_variable("append_result").is_some());
    assert!(executor.context().get_variable("stat_result").is_some());
    assert!(executor.context().get_variable("copy_result").is_some());
    assert!(executor.context().get_variable("move_result").is_some());
}

// ============================================================================
// VCS Primitive Tests (No backend in for_testing - uses on_fail: continue)
// ============================================================================

#[tokio::test]
async fn test_scroll_vcs_readonly_parses() {
    // VCS scrolls parse correctly - execution uses on_fail: continue
    let scroll_path = Path::new("tests/scrolls/02-vcs-readonly.scroll");
    let scroll = parse_scroll_file(scroll_path).expect("parse scroll");
    assert_eq!(scroll.scroll, "vcs-readonly-test");
    assert_eq!(scroll.steps.len(), 3);
}

#[tokio::test]
async fn test_scroll_vcs_branch_ops_parses() {
    let scroll_path = Path::new("tests/scrolls/02a-vcs-branch-ops.scroll");
    let scroll = parse_scroll_file(scroll_path).expect("parse scroll");
    assert_eq!(scroll.scroll, "vcs-branch-ops-test");
    assert_eq!(scroll.steps.len(), 7);
}

#[tokio::test]
async fn test_scroll_vcs_remote_ops_parses() {
    let scroll_path = Path::new("tests/scrolls/02b-vcs-remote-ops.scroll");
    let scroll = parse_scroll_file(scroll_path).expect("parse scroll");
    assert_eq!(scroll.scroll, "vcs-remote-ops-test");
    assert_eq!(scroll.steps.len(), 3);
}

// ============================================================================
// Platform Primitive Tests (MockPlatform)
// ============================================================================

#[tokio::test]
async fn test_scroll_platform_info() {
    let scroll_path = Path::new("tests/scrolls/03-platform-info.scroll");
    let scroll = parse_scroll_file(scroll_path).expect("parse scroll");

    let mut executor = Executor::for_testing();
    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "platform-info scroll failed: {:?}", result.err());
}

// ============================================================================
// Test Primitive Tests (NoopBackend)
// ============================================================================

#[tokio::test]
async fn test_scroll_test_runner() {
    let scroll_path = Path::new("tests/scrolls/04-test-runner.scroll");
    let scroll = parse_scroll_file(scroll_path).expect("parse scroll");

    let mut executor = Executor::for_testing();
    let result = executor.execute_scroll(&scroll).await;
    // NoopBackend returns success, scroll should complete
    assert!(result.is_ok(), "test-runner scroll failed: {:?}", result.err());
}

#[tokio::test]
async fn test_scroll_test_ops() {
    let scroll_path = Path::new("tests/scrolls/04a-test-ops.scroll");
    let scroll = parse_scroll_file(scroll_path).expect("parse scroll");

    let mut executor = Executor::for_testing();
    let result = executor.execute_scroll(&scroll).await;
    // NoopBackend + on_fail: continue, scroll should complete
    assert!(result.is_ok(), "test-ops scroll failed: {:?}", result.err());
}

// ============================================================================
// Flow Tests
// ============================================================================

#[tokio::test]
async fn test_scroll_flow_branch() {
    let scroll_path = Path::new("tests/scrolls/05-flow-branch.scroll");
    let scroll = parse_scroll_file(scroll_path).expect("parse scroll");

    let mut executor = Executor::for_testing();
    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "flow-branch scroll failed: {:?}", result.err());
}

#[tokio::test]
async fn test_scroll_flow_loop() {
    let scroll_path = Path::new("tests/scrolls/06-flow-loop.scroll");
    let scroll = parse_scroll_file(scroll_path).expect("parse scroll");

    let mut executor = Executor::for_testing();
    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "flow-loop scroll failed: {:?}", result.err());
}

// ============================================================================
// Example Scroll Parsing Tests
// Validates all production scrolls parse correctly
// ============================================================================

#[tokio::test]
async fn test_example_scrolls_parse() {
    // Core production scrolls must parse correctly
    // Note: create-epic.scroll, create-chunks.scroll, run-milestone.scroll need schema updates
    let example_scrolls = [
        "scrolls/run-epic.scroll",
        "scrolls/run-story.scroll",
        "scrolls/implement-chunk.scroll",
        "scrolls/complete-story.scroll",
        "scrolls/complete-epic.scroll",
        "scrolls/dev-story.scroll",
        "scrolls/code-review.scroll",
        "scrolls/consensus-vote.scroll",
        "examples/scrolls/test-backends.scroll",
        "examples/scrolls/test-system-primitives.scroll",
    ];

    for scroll_path in &example_scrolls {
        let source = std::fs::read_to_string(scroll_path)
            .unwrap_or_else(|e| panic!("Failed to read {scroll_path}: {e}"));
        let result = sage_lore::scroll::assembly::parser::parse(&source, scroll_path);
        assert!(result.is_ok(), "Failed to parse {}: {:?}", scroll_path, result.err());
    }
}

#[tokio::test]
async fn test_example_adapter_scrolls_parse() {
    // Forgejo adapter scrolls must parse correctly
    let adapter_scrolls = [
        "scrolls/adapters/epic-from-forgejo.scroll",
        "scrolls/adapters/story-from-forgejo.scroll",
        "scrolls/adapters/chunk-from-forgejo.scroll",
    ];

    for scroll_path in &adapter_scrolls {
        let source = std::fs::read_to_string(scroll_path)
            .unwrap_or_else(|e| panic!("Failed to read {scroll_path}: {e}"));
        let result = sage_lore::scroll::assembly::parser::parse(&source, scroll_path);
        assert!(result.is_ok(), "Failed to parse adapter {}: {:?}", scroll_path, result.err());
    }
}
