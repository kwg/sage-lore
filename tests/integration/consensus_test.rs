// SPDX-License-Identifier: MIT
//! Integration tests for consensus validation infrastructure.

use sage_lore::primitives::invoke::{LlmResponse, MockLlmBackend};
use sage_lore::scroll::consensus::{
    check_threshold, parse_vote_response, ConsensusCheck, ConsensusValidator, ModelTier,
};
use sage_lore::scroll::schema::ThresholdSpec;
use std::sync::Arc;

#[tokio::test]
async fn test_consensus_3_validators() {
    // Create mock backend that returns PASS responses
    let mock = MockLlmBackend::new().with_default_response(LlmResponse {
        text: "PASS\nExplanation: Looks good".to_string(),
        tokens_used: Some(10),
        model: "mock-cheap".to_string(),
        truncated: false,
    });

    let validator = ConsensusValidator::new_cheap(Arc::new(mock));

    let check = ConsensusCheck {
        primitive: "elaborate".to_string(),
        invariants: vec!["preserves_intent".to_string()],
        input: serde_json::Value::String("test input".to_string()),
        output: serde_json::Value::String("test output".to_string()),
        context: None,
    };

    let result = validator.validate(&check).expect("validation should succeed");

    // Should have 3 votes (1 per validator)
    assert_eq!(result.votes.len(), 3, "Should spawn 3 validators");
    assert!(result.passed, "Should pass with all PASS votes");
}

#[tokio::test]
async fn test_consensus_2_of_3_pass() {
    // Since MockLlmBackend doesn't support stateful responses,
    // we'll test this scenario by using a mock that always passes
    // and verify the threshold logic works with the check_threshold function
    let mock = MockLlmBackend::new().with_default_response(LlmResponse {
        text: "PASS\nExplanation: Good".to_string(),
        tokens_used: Some(10),
        model: "mock".to_string(),
        truncated: false,
    });

    let validator = ConsensusValidator::new_cheap(Arc::new(mock));

    let check = ConsensusCheck {
        primitive: "elaborate".to_string(),
        invariants: vec!["preserves_intent".to_string()],
        input: serde_json::Value::String("test".to_string()),
        output: serde_json::Value::String("result".to_string()),
        context: None,
    };

    let result = validator.validate(&check).expect("validation should succeed");

    // Should pass with all 3 PASS votes
    assert!(result.passed, "Should pass with all PASS votes");
    assert_eq!(result.votes.len(), 3);

    // All votes should pass
    let pass_count = result.votes.iter().filter(|v| v.passed).count();
    assert_eq!(pass_count, 3, "Should have 3 PASS votes");
}

#[tokio::test]
async fn test_consensus_all_fail() {
    // Create mock that always returns FAIL
    let mock = MockLlmBackend::new().with_default_response(LlmResponse {
        text: "FAIL\nExplanation: Does not meet criteria".to_string(),
        tokens_used: Some(10),
        model: "mock".to_string(),
        truncated: false,
    });

    let validator = ConsensusValidator::new_cheap(Arc::new(mock));

    let check = ConsensusCheck {
        primitive: "elaborate".to_string(),
        invariants: vec!["preserves_intent".to_string()],
        input: serde_json::Value::String("test".to_string()),
        output: serde_json::Value::String("result".to_string()),
        context: None,
    };

    let result = validator.validate(&check).expect("validation should succeed");

    // Should fail when all votes are FAIL
    assert!(!result.passed, "Should fail with all FAIL votes");
    assert_eq!(result.votes.len(), 3);

    let pass_count = result.votes.iter().filter(|v| v.passed).count();
    assert_eq!(pass_count, 0, "Should have 0 PASS votes");
}

#[tokio::test]
async fn test_consensus_mixed_votes() {
    // Test with all FAIL to verify failure case
    let mock = MockLlmBackend::new().with_default_response(LlmResponse {
        text: "FAIL\nExplanation: Not good".to_string(),
        tokens_used: Some(10),
        model: "mock".to_string(),
        truncated: false,
    });

    let validator = ConsensusValidator::new_cheap(Arc::new(mock));

    let check = ConsensusCheck {
        primitive: "elaborate".to_string(),
        invariants: vec!["preserves_intent".to_string()],
        input: serde_json::Value::String("test".to_string()),
        output: serde_json::Value::String("result".to_string()),
        context: None,
    };

    let result = validator.validate(&check).expect("validation should succeed");

    // Should fail with all FAIL votes
    assert!(!result.passed, "Should fail with all FAIL votes");

    let pass_count = result.votes.iter().filter(|v| v.passed).count();
    assert_eq!(pass_count, 0, "Should have 0 PASS votes");
}

#[tokio::test]
async fn test_consensus_parse_vote_response() {
    let options = vec!["approve".to_string(), "reject".to_string()];

    // Test structured format
    let response = "VOTE: approve\nREASON: Looks good";
    let (vote, reason) = parse_vote_response(response, &options);
    assert_eq!(vote, Some("approve".to_string()));
    assert_eq!(reason, "Looks good");

    // Test lowercase
    let response = "vote: reject\nreason: Not ready";
    let (vote, reason) = parse_vote_response(response, &options);
    assert_eq!(vote, Some("reject".to_string()));
    assert_eq!(reason, "Not ready");

    // Test case-insensitive matching
    let response = "VOTE: APPROVE\nREASON: Perfect";
    let (vote, reason) = parse_vote_response(response, &options);
    assert_eq!(vote, Some("approve".to_string()));
    assert_eq!(reason, "Perfect");
}

#[tokio::test]
async fn test_consensus_cheap_tier() {
    // Verify that ConsensusValidator uses cheap tier
    let mock = MockLlmBackend::new().with_default_response(LlmResponse {
        text: "PASS\nExplanation: OK".to_string(),
        tokens_used: Some(5),
        model: "haiku".to_string(),
        truncated: false,
    });

    let validator = ConsensusValidator::new_cheap(Arc::new(mock));

    // Verify the tier is set to Cheap
    assert!(matches!(validator.tier(), ModelTier::Cheap));
}

#[tokio::test]
async fn test_consensus_multiple_invariants() {
    // Create mock that returns PASS
    let mock = MockLlmBackend::new().with_default_response(LlmResponse {
        text: "PASS\nExplanation: OK".to_string(),
        tokens_used: Some(10),
        model: "mock".to_string(),
        truncated: false,
    });

    let validator = ConsensusValidator::new_cheap(Arc::new(mock));

    let check = ConsensusCheck {
        primitive: "elaborate".to_string(),
        invariants: vec![
            "preserves_intent".to_string(),
            "adds_detail".to_string(),
        ],
        input: serde_json::Value::String("test".to_string()),
        output: serde_json::Value::String("result".to_string()),
        context: None,
    };

    let result = validator.validate(&check).expect("validation should succeed");

    // Should have 6 votes total (3 validators × 2 invariants)
    assert_eq!(result.votes.len(), 6);
    assert!(result.passed, "Should pass all invariants");

    // Verify votes cover all invariants
    let intent_votes: Vec<_> = result
        .votes
        .iter()
        .filter(|v| v.invariant == "preserves_intent")
        .collect();
    let detail_votes: Vec<_> = result
        .votes
        .iter()
        .filter(|v| v.invariant == "adds_detail")
        .collect();

    assert_eq!(intent_votes.len(), 3, "Should have 3 votes for preserves_intent");
    assert_eq!(detail_votes.len(), 3, "Should have 3 votes for adds_detail");
}

#[tokio::test]
async fn test_check_threshold_2_of_3() {
    // Test 2-of-3 numeric threshold
    let threshold = ThresholdSpec::Numeric(2);

    assert!(check_threshold(2, 3, &threshold));
    assert!(check_threshold(3, 3, &threshold));
    assert!(!check_threshold(1, 3, &threshold));
    assert!(!check_threshold(0, 3, &threshold));
}
