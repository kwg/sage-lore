// SPDX-License-Identifier: MIT
//! Coverage types for test coverage analysis.

use serde::{Deserialize, Serialize};

use super::framework::Framework;

// ============================================================================
// Coverage Types
// ============================================================================

/// Result of a coverage analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageResult {
    /// Line coverage percentage
    pub lines_percent: f64,
    /// Branch coverage percentage (if available)
    pub branches_percent: Option<f64>,
    /// Function coverage percentage (if available)
    pub functions_percent: Option<f64>,
    /// Number of lines covered
    pub lines_covered: u32,
    /// Total number of lines
    pub lines_total: u32,
    /// Per-file coverage details
    pub files: Vec<FileCoverage>,
    /// Framework used for coverage
    pub framework: Framework,
}

impl CoverageResult {
    /// Check if coverage meets minimum thresholds.
    pub fn meets_threshold(&self, min_lines: f64, min_branches: Option<f64>) -> bool {
        if self.lines_percent < min_lines {
            return false;
        }
        if let (Some(min_br), Some(actual_br)) = (min_branches, self.branches_percent) {
            if actual_br < min_br {
                return false;
            }
        }
        true
    }
}

impl Default for CoverageResult {
    fn default() -> Self {
        Self {
            lines_percent: 0.0,
            branches_percent: None,
            functions_percent: None,
            lines_covered: 0,
            lines_total: 0,
            files: Vec::new(),
            framework: Framework::Cargo,
        }
    }
}

/// Coverage information for a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCoverage {
    /// File path relative to project root
    pub path: String,
    /// Line coverage percentage
    pub lines_percent: f64,
    /// Lines that are covered
    pub covered_lines: Vec<u32>,
    /// Lines that are not covered
    pub uncovered_lines: Vec<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // CoverageResult tests
    // ========================================================================

    #[test]
    fn test_coverage_result_meets_threshold() {
        let coverage = CoverageResult {
            lines_percent: 85.0,
            branches_percent: Some(75.0),
            functions_percent: Some(90.0),
            lines_covered: 85,
            lines_total: 100,
            files: vec![],
            framework: Framework::Cargo,
        };

        // Meets both thresholds
        assert!(coverage.meets_threshold(80.0, Some(70.0)));

        // Meets line but not branch
        assert!(!coverage.meets_threshold(80.0, Some(80.0)));

        // Doesn't meet line threshold
        assert!(!coverage.meets_threshold(90.0, None));

        // Branch threshold ignored when None
        assert!(coverage.meets_threshold(80.0, None));
    }

    #[test]
    fn test_coverage_result_meets_threshold_no_branch_coverage() {
        let coverage = CoverageResult {
            lines_percent: 85.0,
            branches_percent: None,
            ..Default::default()
        };

        // Should pass even with branch threshold if no branch coverage available
        assert!(coverage.meets_threshold(80.0, Some(70.0)));
    }
}
