// SPDX-License-Identifier: MIT
//! Tests for production scroll parsing with the Scroll Assembly parser.
//!
//! These tests verify that the production scrolls (rewritten from YAML to
//! Scroll Assembly syntax) can be loaded and parsed by the assembly parser.

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::scroll::assembly::parser;

    fn assert_scroll_parses(path: &str) {
        let full_path = Path::new(path);
        let source = std::fs::read_to_string(full_path)
            .unwrap_or_else(|e| panic!("Failed to read {path}: {e}"));
        parser::parse(&source, path)
            .unwrap_or_else(|diags| {
                let msgs: Vec<_> = diags.iter().map(|d| d.to_string()).collect();
                panic!("Failed to parse {path}:\n{}", msgs.join("\n"));
            });
    }

    // ========================================================================
    // Adapter Scrolls
    // ========================================================================

    #[test]
    fn test_parse_story_from_forgejo() {
        assert_scroll_parses("scrolls/adapters/story-from-forgejo.scroll");
    }

    #[test]
    fn test_parse_chunk_from_forgejo() {
        assert_scroll_parses("scrolls/adapters/chunk-from-forgejo.scroll");
    }

    #[test]
    fn test_parse_epic_from_forgejo() {
        assert_scroll_parses("scrolls/adapters/epic-from-forgejo.scroll");
    }

    // ========================================================================
    // Core Workflow Scrolls
    // ========================================================================

    #[test]
    fn test_parse_run_epic() {
        assert_scroll_parses("scrolls/run-epic.scroll");
    }

    #[test]
    fn test_parse_run_story() {
        assert_scroll_parses("scrolls/run-story.scroll");
    }

    #[test]
    fn test_parse_run_milestone() {
        assert_scroll_parses("scrolls/run-milestone.scroll");
    }

    #[test]
    fn test_parse_create_epic() {
        assert_scroll_parses("scrolls/create-epic.scroll");
    }

    #[test]
    fn test_parse_create_chunks() {
        assert_scroll_parses("scrolls/create-chunks.scroll");
    }

    #[test]
    fn test_parse_dev_story() {
        assert_scroll_parses("scrolls/dev-story.scroll");
    }

    #[test]
    fn test_parse_implement_chunk() {
        assert_scroll_parses("scrolls/implement-chunk.scroll");
    }

    #[test]
    fn test_parse_implement_and_review() {
        assert_scroll_parses("scrolls/implement-and-review.scroll");
    }

    #[test]
    fn test_parse_code_review() {
        assert_scroll_parses("scrolls/code-review.scroll");
    }

    #[test]
    fn test_parse_complete_story() {
        assert_scroll_parses("scrolls/complete-story.scroll");
    }

    #[test]
    fn test_parse_complete_epic() {
        assert_scroll_parses("scrolls/complete-epic.scroll");
    }

    #[test]
    fn test_parse_consensus_vote() {
        assert_scroll_parses("scrolls/consensus-vote.scroll");
    }

    #[test]
    fn test_parse_build_from_forgejo() {
        assert_scroll_parses("scrolls/build-from-forgejo.scroll");
    }

    // ========================================================================
    // Utility/Test Scrolls
    // ========================================================================

    #[test]
    fn test_parse_test_system_primitives() {
        assert_scroll_parses("examples/scrolls/test-system-primitives.scroll");
    }

    #[test]
    fn test_parse_test_single_chunk() {
        assert_scroll_parses("examples/scrolls/test-single-chunk.scroll");
    }

    #[test]
    fn test_parse_test_backends() {
        assert_scroll_parses("examples/scrolls/test-backends.scroll");
    }

    #[test]
    fn test_parse_good_scroll_variable_refs() {
        assert_scroll_parses("examples/scrolls/good-scroll-variable-refs.scroll");
    }

    // Note: bad-scroll-hardcoded-secret.scroll intentionally contains secrets
    // for testing. It should still parse (secret detection is a separate concern).
    #[test]
    fn test_parse_bad_scroll_hardcoded_secret() {
        assert_scroll_parses("examples/scrolls/bad-scroll-hardcoded-secret.scroll");
    }
}
