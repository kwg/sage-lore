// SPDX-License-Identifier: MIT
//! Consensus validation for primitive outputs.
//!
//! Spawns multiple cheap validators to check invariants, uses 2-of-3 voting.

use crate::primitives::invoke::LlmBackend;
use crate::scroll::error::ExecutionError;
use crate::scroll::schema::{ThresholdSpec, ThresholdType};
use std::collections::HashMap;
use std::sync::Arc;

/// Model tier for consensus validation.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelTier {
    Cheap,     // haiku, local models
    Standard,  // sonnet
    Premium,   // opus
}

/// Consensus validator spawns multiple validators to check invariants.
pub struct ConsensusValidator {
    backend: Arc<dyn LlmBackend>,
    tier: ModelTier,
}

/// Check specification for consensus validation.
#[derive(Debug, Clone)]
pub struct ConsensusCheck {
    pub primitive: String,
    pub invariants: Vec<String>,
    pub input: serde_json::Value,
    pub output: serde_json::Value,
    pub context: Option<serde_json::Value>,
}

/// Result of consensus validation.
#[derive(Debug, Clone)]
pub struct ConsensusResult {
    pub passed: bool,
    pub votes: Vec<ValidatorVote>,
    pub summary: String,
}

/// A single validator vote.
#[derive(Debug, Clone)]
pub struct ValidatorVote {
    pub validator_id: usize,
    pub invariant: String,
    pub passed: bool,
    pub explanation: String,
}

impl ConsensusValidator {
    /// Create new consensus validator with cheap tier backend.
    pub fn new_cheap(backend: Arc<dyn LlmBackend>) -> Self {
        Self {
            backend,
            tier: ModelTier::Cheap,
        }
    }

    /// Get the model tier.
    pub fn tier(&self) -> ModelTier {
        self.tier.clone()
    }

    /// Perform consensus validation.
    /// Uses parallel execution and retries once on tie (when votes don't meet 2-of-3 threshold).
    pub fn validate(&self, check: &ConsensusCheck) -> Result<ConsensusResult, ExecutionError> {
        // Try validation with retry_limit = 1 (one retry on tie)
        for attempt in 0..=1 {
            if attempt > 0 {
                tracing::info!("Retrying consensus validation due to tie");
            }

            let votes = self.validate_parallel(check)?;

            // Count votes per invariant (need 2-of-3 pass)
            let mut invariant_results: HashMap<String, usize> = HashMap::new();
            for vote in &votes {
                if vote.passed {
                    *invariant_results.entry(vote.invariant.clone()).or_insert(0) += 1;
                }
            }

            // Check if all invariants have 2-of-3 pass
            let passed = check.invariants.iter().all(|inv| {
                invariant_results
                    .get(inv)
                    .map(|&count| count >= 2)
                    .unwrap_or(false)
            });

            // Check if we have a tie (any invariant with exactly 1 or 2 votes but not reaching threshold)
            let has_tie = check.invariants.iter().any(|inv| {
                let count = invariant_results.get(inv).copied().unwrap_or(0);
                count > 0 && count < 2
            });

            // Log tally per invariant
            for inv in &check.invariants {
                let pass_count = invariant_results.get(inv).copied().unwrap_or(0);
                let total = votes.iter().filter(|v| v.invariant == *inv).count();
                tracing::debug!(invariant = %inv, passes = pass_count, total = total, "Invariant tally");
            }

            // If passed or no tie, return result
            if passed || !has_tie || attempt >= 1 {
                let summary = if passed {
                    format!(
                        "Consensus validation passed: {}/{} invariants passed 2-of-3 vote (attempt {})",
                        invariant_results.values().filter(|&&c| c >= 2).count(),
                        check.invariants.len(),
                        attempt + 1
                    )
                } else {
                    format!(
                        "Consensus validation failed: some invariants did not reach 2-of-3 threshold (attempt {})",
                        attempt + 1
                    )
                };

                return Ok(ConsensusResult {
                    passed,
                    votes,
                    summary,
                });
            }
            // Otherwise, continue to retry
        }

        // Should never reach here due to loop structure, but satisfy compiler
        Err(ExecutionError::InvocationError(
            "Consensus validation exhausted retries".to_string(),
        ))
    }

    /// Validate using parallel execution of 3 validators.
    ///
    /// Uses a shared tokio runtime instead of spawning std::threads with
    /// per-thread runtimes (MF3, #185). This avoids starving the outer
    /// tokio runtime and reduces overhead.
    fn validate_parallel(&self, check: &ConsensusCheck) -> Result<Vec<ValidatorVote>, ExecutionError> {
        // Build all validation tasks
        let mut tasks = Vec::new();
        for validator_id in 0..3 {
            for invariant in &check.invariants {
                let backend = Arc::clone(&self.backend);
                let primitive = check.primitive.clone();
                let invariant = invariant.clone();
                let input = check.input.clone();
                let output = check.output.clone();
                let context = check.context.clone();

                tasks.push(async move {
                    check_invariant_async(
                        &backend,
                        validator_id,
                        &primitive,
                        &invariant,
                        &input,
                        &output,
                        context.as_ref(),
                    ).await
                });
            }
        }

        // Run tasks concurrently on a dedicated thread with its own runtime (MF3, #185).
        // Uses a single thread + single runtime instead of N threads with N runtimes.
        // The thread is needed because block_on() panics inside an existing tokio runtime.
        let results: Vec<Result<ValidatorVote, ExecutionError>> = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create consensus runtime");
            rt.block_on(futures::future::join_all(tasks))
        }).join().map_err(|_| ExecutionError::InvocationError(
            "Consensus validation thread panicked".to_string()
        ))?;

        let mut votes = Vec::new();
        let mut failures = 0;
        for result in results {
            match result {
                Ok(vote) => {
                    tracing::debug!(
                        validator = vote.validator_id,
                        invariant = %vote.invariant,
                        result = if vote.passed { "PASS" } else { "FAIL" },
                        "Validator vote"
                    );
                    votes.push(vote);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Validator invocation failed");
                    failures += 1;
                }
            }
        }

        tracing::info!(
            votes = votes.len(),
            failures = failures,
            primitive = %check.primitive,
            "Consensus validation votes collected"
        );

        Ok(votes)
    }

}

/// Async invariant check — used by validate_parallel to avoid per-thread runtimes.
async fn check_invariant_async(
    backend: &Arc<dyn LlmBackend>,
    validator_id: usize,
    primitive: &str,
    invariant: &str,
    input: &serde_json::Value,
    output: &serde_json::Value,
    context: Option<&serde_json::Value>,
) -> Result<ValidatorVote, ExecutionError> {
    let input_str = serde_json::to_string(input)
        .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))?;
    let output_str = serde_json::to_string(output)
        .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))?;

    let context_str = if let Some(ctx) = context {
        serde_json::to_string(ctx)
            .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))?
    } else {
        String::new()
    };

    let prompt = format!(
        "You are validating the output of the {primitive} primitive.\n\n\
         Check if this invariant holds:\n\
         INVARIANT: {invariant}\n\n\
         INPUT:\n{input_str}\n\n\
         OUTPUT:\n{output_str}\n\n\
         {context}\n\n\
         Respond with ONLY:\n\
         PASS or FAIL\n\
         Explanation: (one sentence why)\n",
        primitive = primitive,
        invariant = invariant,
        input_str = input_str,
        output_str = output_str,
        context = if !context_str.is_empty() {
            format!("CONTEXT:\n{}\n\n", context_str)
        } else {
            String::new()
        },
    );

    let request = crate::primitives::invoke::LlmRequest {
        prompt,
        system: None,
        max_tokens: Some(150),
        temperature: Some(0.0),
        timeout_secs: None,
        model_tier: Some(crate::primitives::invoke::ModelTier::Cheap),
        format_schema: None,
        model: None,
    };

    let response = backend.generate(request).await
        .map_err(|e| ExecutionError::InvocationError(e.to_string()))?;

    let (passed, explanation) = parse_validator_response(&response.text);

    Ok(ValidatorVote {
        validator_id,
        invariant: invariant.to_string(),
        passed,
        explanation,
    })
}

/// Parse validator response for PASS/FAIL and explanation.
fn parse_validator_response(response: &str) -> (bool, String) {
    let lines: Vec<&str> = response.lines().collect();

    let passed = lines
        .iter()
        .any(|line| line.trim().to_uppercase().starts_with("PASS"));

    let explanation = lines
        .iter()
        .find(|line| line.trim().to_lowercase().starts_with("explanation:"))
        .map(|line| {
            line.trim()
                .strip_prefix("Explanation:")
                .or_else(|| line.strip_prefix("explanation:"))
                .unwrap_or(line)
                .trim()
                .to_string()
        })
        .unwrap_or_else(|| "No explanation provided".to_string());

    (passed, explanation)
}

/// Parse a vote response to extract the vote and reasoning.
///
/// Looks for "VOTE: <option>" and "REASON: <text>" patterns.
/// If not found, tries to match any option word in the response.
pub fn parse_vote_response(response: &str, valid_options: &[String]) -> (Option<String>, String) {
    let mut vote = None;
    let mut reason = String::new();

    // Try JSON format first (from schema-constrained output)
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(response) {
        if let Some(v) = json.get("vote").and_then(|v| v.as_str()) {
            for option in valid_options {
                if v.eq_ignore_ascii_case(option) {
                    vote = Some(option.clone());
                    break;
                }
            }
        }
        if let Some(r) = json.get("reason").and_then(|v| v.as_str()) {
            reason = r.to_string();
        }
        if vote.is_some() {
            return (vote, reason);
        }
    }

    // Fall back to text format (VOTE: / REASON:)
    for line in response.lines() {
        let line = line.trim();

        if line.to_uppercase().starts_with("VOTE:") {
            let vote_text = line[5..].trim();
            // Find matching option (case-insensitive)
            for option in valid_options {
                if vote_text.eq_ignore_ascii_case(option) {
                    vote = Some(option.clone());
                    break;
                }
            }
        } else if line.to_uppercase().starts_with("REASON:") {
            reason = line[7..].trim().to_string();
        }
    }

    // If no structured vote found, try to find any valid option in the response
    if vote.is_none() {
        let response_lower = response.to_lowercase();
        for option in valid_options {
            if response_lower.contains(&option.to_lowercase()) {
                vote = Some(option.clone());
                break;
            }
        }
    }

    // If still no vote, use entire response as reason
    if vote.is_none() && reason.is_empty() {
        reason = response.to_string();
    }

    (vote, reason)
}

/// Check if the winning vote count meets the specified threshold.
pub fn check_threshold(
    winning_count: usize,
    total_votes: usize,
    threshold: &ThresholdSpec,
) -> bool {
    if total_votes == 0 {
        return false;
    }

    match threshold {
        ThresholdSpec::Named(threshold_type) => match threshold_type {
            ThresholdType::Majority => {
                // More than 50%
                winning_count > total_votes / 2
            }
            ThresholdType::Supermajority => {
                // More than 66%
                winning_count * 3 > total_votes * 2
            }
            ThresholdType::Unanimous => {
                // 100%
                winning_count == total_votes
            }
        },
        ThresholdSpec::Numeric(required_count) => {
            // At least N votes
            winning_count >= *required_count
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::invoke::{LlmResponse, MockLlmBackend};

    #[test]
    fn test_consensus_validation_passes_with_2_of_3() {
        // Create mock backend that returns PASS for 2 validators, FAIL for 1
        let mock = MockLlmBackend::new()
            .with_default_response(LlmResponse {
                text: "PASS\nExplanation: Output meets invariant".to_string(),
                tokens_used: Some(10),
                model: "mock".to_string(),
                truncated: false,
            });

        let validator = ConsensusValidator::new_cheap(Arc::new(mock));

        let check = ConsensusCheck {
            primitive: "elaborate".to_string(),
            invariants: vec!["Output should be longer than input".to_string()],
            input: serde_json::Value::String("short".to_string()),
            output: serde_json::Value::String("much longer expanded output".to_string()),
            context: None,
        };

        let result = validator.validate(&check).unwrap();
        assert!(result.passed, "Expected validation to pass with 2-of-3 threshold");
    }

    #[test]
    fn test_consensus_validation_fails_without_threshold() {
        // Create mock backend that returns FAIL for all validators
        let mock = MockLlmBackend::new()
            .with_default_response(LlmResponse {
                text: "FAIL\nExplanation: Output does not meet invariant".to_string(),
                tokens_used: Some(10),
                model: "mock".to_string(),
                truncated: false,
            });

        let validator = ConsensusValidator::new_cheap(Arc::new(mock));

        let check = ConsensusCheck {
            primitive: "elaborate".to_string(),
            invariants: vec!["Output should be longer than input".to_string()],
            input: serde_json::Value::String("short".to_string()),
            output: serde_json::Value::String("out".to_string()),
            context: None,
        };

        let result = validator.validate(&check).unwrap();
        assert!(!result.passed, "Expected validation to fail without 2-of-3 threshold");
    }

    #[test]
    fn test_consensus_validation_multiple_invariants() {
        // Test with multiple invariants
        let mock = MockLlmBackend::new()
            .with_default_response(LlmResponse {
                text: "PASS\nExplanation: All invariants met".to_string(),
                tokens_used: Some(10),
                model: "mock".to_string(),
                truncated: false,
            });

        let validator = ConsensusValidator::new_cheap(Arc::new(mock));

        let check = ConsensusCheck {
            primitive: "distill".to_string(),
            invariants: vec![
                "Output should be shorter than input".to_string(),
                "Output should preserve key information".to_string(),
            ],
            input: serde_json::Value::String("long text here".to_string()),
            output: serde_json::Value::String("short".to_string()),
            context: None,
        };

        let result = validator.validate(&check).unwrap();
        assert!(result.passed);
        assert_eq!(result.votes.len(), 6); // 3 validators * 2 invariants
    }

    #[test]
    fn test_parse_vote_structured() {
        let response = "VOTE: approve\nREASON: Looks good to me";
        let options = vec!["approve".to_string(), "reject".to_string()];
        let (vote, reason) = parse_vote_response(response, &options);
        assert_eq!(vote, Some("approve".to_string()));
        assert_eq!(reason, "Looks good to me");
    }

    #[test]
    fn test_parse_vote_unstructured() {
        let response = "I think we should approve this change";
        let options = vec!["approve".to_string(), "reject".to_string()];
        let (vote, reason) = parse_vote_response(response, &options);
        assert_eq!(vote, Some("approve".to_string()));
    }

    #[test]
    fn test_check_threshold_majority() {
        let threshold = ThresholdSpec::Named(ThresholdType::Majority);
        assert!(check_threshold(3, 5, &threshold)); // 60%
        assert!(!check_threshold(2, 4, &threshold)); // 50% - not majority
        assert!(check_threshold(3, 4, &threshold)); // 75%
    }

    #[test]
    fn test_check_threshold_supermajority() {
        let threshold = ThresholdSpec::Named(ThresholdType::Supermajority);
        assert!(check_threshold(7, 10, &threshold)); // 70%
        assert!(!check_threshold(6, 10, &threshold)); // 60% - not supermajority
    }

    #[test]
    fn test_check_threshold_unanimous() {
        let threshold = ThresholdSpec::Named(ThresholdType::Unanimous);
        assert!(check_threshold(5, 5, &threshold));
        assert!(!check_threshold(4, 5, &threshold));
    }

    #[test]
    fn test_check_threshold_numeric() {
        let threshold = ThresholdSpec::Numeric(3);
        assert!(check_threshold(3, 10, &threshold));
        assert!(check_threshold(5, 10, &threshold));
        assert!(!check_threshold(2, 10, &threshold));
    }

    #[test]
    fn test_parse_validator_response_pass() {
        let response = "PASS\nExplanation: Looks good";
        let (passed, explanation) = parse_validator_response(response);
        assert!(passed);
        assert_eq!(explanation, "Looks good");
    }

    #[test]
    fn test_parse_validator_response_fail() {
        let response = "FAIL\nExplanation: Does not meet criteria";
        let (passed, explanation) = parse_validator_response(response);
        assert!(!passed);
        assert_eq!(explanation, "Does not meet criteria");
    }

    #[test]
    fn test_parse_validator_response_lowercase() {
        let response = "pass\nexplanation: all good";
        let (passed, explanation) = parse_validator_response(response);
        assert!(passed);
        assert_eq!(explanation, "all good");
    }

    #[test]
    fn test_parse_validator_response_no_explanation() {
        let response = "PASS";
        let (passed, explanation) = parse_validator_response(response);
        assert!(passed);
        assert_eq!(explanation, "No explanation provided");
    }

    #[test]
    fn test_parse_validator_response_multiline() {
        let response = "The output is valid.\nPASS\nExplanation: Preserves all key information";
        let (passed, explanation) = parse_validator_response(response);
        assert!(passed);
        assert_eq!(explanation, "Preserves all key information");
    }
}
