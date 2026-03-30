// SPDX-License-Identifier: MIT
//! Severity thresholds and finding action policies.

use serde::{Deserialize, Serialize};

/// Severity threshold for filtering findings.
///
/// Ordering: Low < Medium < High < Critical. Config merges take max so the security floor only rises (D28).
/// Default is Low to allow floor enforcement to work correctly in config merging.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum SeverityThreshold {
    #[default]
    Low,
    Medium,
    High,
    Critical,
}

impl PartialOrd for SeverityThreshold {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SeverityThreshold {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use SeverityThreshold::*;
        use std::cmp::Ordering;

        match (self, other) {
            (Low, Low) | (Medium, Medium) | (High, High) | (Critical, Critical) => Ordering::Equal,
            (Low, _) => Ordering::Less,
            (_, Low) => Ordering::Greater,
            (Medium, Critical) | (Medium, High) => Ordering::Less,
            (Critical, Medium) | (High, Medium) => Ordering::Greater,
            (High, Critical) => Ordering::Less,
            (Critical, High) => Ordering::Greater,
        }
    }
}

/// Action to take when secrets are detected.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OnFinding {
    /// Abort operation and reset state — no partial surgery, no salvage (D11)
    #[default]
    AbortAndReset,
    /// Log warning but continue
    Warn,
    /// Prompt user for decision
    PromptUser,
}

/// Action for existing secrets found during audit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OnExisting {
    /// Block until resolved
    #[default]
    Block,
    /// Warn but allow proceeding
    Warn,
    /// Ignore existing secrets
    Ignore,
}

/// What happens when a required tool is missing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FallbackPolicy {
    /// Refuse to run if required tools are missing (payment processor)
    #[default]
    HardStop,
    /// Log warning, continue with available tools (hobby project)
    Warn,
}

/// Helper: Choose stricter OnFinding policy.
/// AbortAndReset > PromptUser > Warn
pub(crate) fn max_on_finding(a: OnFinding, b: OnFinding) -> OnFinding {
    use OnFinding::*;
    match (a, b) {
        (AbortAndReset, _) | (_, AbortAndReset) => AbortAndReset,
        (PromptUser, _) | (_, PromptUser) => PromptUser,
        _ => Warn,
    }
}

/// Helper: Choose stricter OnExisting policy.
/// Block > Warn > Ignore
pub(crate) fn max_on_existing(a: OnExisting, b: OnExisting) -> OnExisting {
    use OnExisting::*;
    match (a, b) {
        (Block, _) | (_, Block) => Block,
        (Warn, _) | (_, Warn) => Warn,
        _ => Ignore,
    }
}

/// Helper: Choose stricter FallbackPolicy.
/// HardStop > Warn
pub(crate) fn max_fallback(a: FallbackPolicy, b: FallbackPolicy) -> FallbackPolicy {
    use FallbackPolicy::*;
    match (a, b) {
        (HardStop, _) | (_, HardStop) => HardStop,
        _ => Warn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_threshold_ordering() {
        // Verify severity ordering so config merges always raise the floor (D28)
        assert!(SeverityThreshold::Low < SeverityThreshold::Medium);
        assert!(SeverityThreshold::Medium < SeverityThreshold::High);
        assert!(SeverityThreshold::High < SeverityThreshold::Critical);

        // Transitivity checks
        assert!(SeverityThreshold::Low < SeverityThreshold::High);
        assert!(SeverityThreshold::Low < SeverityThreshold::Critical);
        assert!(SeverityThreshold::Medium < SeverityThreshold::Critical);
    }

    #[test]
    fn test_severity_threshold_max() {
        // Test that max() works correctly for floor enforcement
        assert_eq!(
            std::cmp::max(SeverityThreshold::Medium, SeverityThreshold::Low),
            SeverityThreshold::Medium
        );
        assert_eq!(
            std::cmp::max(SeverityThreshold::Low, SeverityThreshold::Medium),
            SeverityThreshold::Medium
        );
        assert_eq!(
            std::cmp::max(SeverityThreshold::High, SeverityThreshold::Critical),
            SeverityThreshold::Critical
        );
    }
}
