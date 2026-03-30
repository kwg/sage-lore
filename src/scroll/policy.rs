// SPDX-License-Identifier: MIT
//! Policy enforcement for security gates with permissive/enforcing modes.

use crate::scroll::error::{PolicyError, PolicyViolation};
use crate::scroll::schema::{Scroll, ScanType, Step};

#[derive(Debug, Clone, Copy)]
pub enum EnforcementMode {
    /// Phase 1: Log warnings but don't block
    Permissive,
    /// Future: Block on policy violations
    Enforcing,
}

#[derive(Clone)]
pub struct PolicyEnforcer {
    mode: EnforcementMode,
}

impl PolicyEnforcer {
    pub fn new(mode: EnforcementMode) -> Self {
        PolicyEnforcer { mode }
    }

    pub fn permissive() -> Self {
        PolicyEnforcer {
            mode: EnforcementMode::Permissive,
        }
    }

    pub fn enforcing() -> Self {
        PolicyEnforcer {
            mode: EnforcementMode::Enforcing,
        }
    }

    pub fn mode(&self) -> EnforcementMode {
        self.mode
    }

    /// Check if a secure step precedes the given step index
    fn has_secure_before(&self, scroll: &Scroll, step_idx: usize) -> bool {
        for i in 0..step_idx {
            if let Step::Secure(_) = &scroll.steps[i] {
                return true;
            }
        }
        false
    }

    /// Check if secure(secret_detection) precedes the given step index
    fn _has_secret_detection_before(&self, scroll: &Scroll, step_idx: usize) -> bool {
        for i in 0..step_idx {
            if let Step::Secure(secure_step) = &scroll.steps[i] {
                match &secure_step.secure.scan_type {
                    ScanType::SecretDetection => return true,
                    ScanType::Multiple(scans) => {
                        if scans.iter().any(|s| s == "secret_detection") {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
        }
        false
    }

    /// Check scroll for required security gates before sensitive operations.
    pub fn check_security_gates(&self, scroll: &Scroll) -> Vec<PolicyViolation> {
        let mut violations = Vec::new();

        for (idx, step) in scroll.steps.iter().enumerate() {
            if let Step::Invoke(_invoke_step) = step {
                // Check: invoke(agent=*) should be preceded by secure()
                // Note: agent is now required, so all invoke steps require secure gate
                if !self.has_secure_before(scroll, idx) {
                    violations.push(PolicyViolation {
                        step: idx,
                        rule: "security_gate_before_agent".to_string(),
                        message: "invoke(agent=*) should be preceded by secure()".to_string(),
                    });
                }
            }
        }

        violations
    }

    /// Apply enforcement policy to violations.
    pub fn enforce(&self, violations: &[PolicyViolation]) -> Result<(), PolicyError> {
        if violations.is_empty() {
            return Ok(());
        }

        match self.mode {
            EnforcementMode::Permissive => {
                for v in violations {
                    tracing::warn!(
                        step = v.step,
                        rule = %v.rule,
                        "Policy violation (permissive mode): {}",
                        v.message
                    );
                }
                Ok(())
            }
            EnforcementMode::Enforcing => Err(PolicyError::Violations(violations.to_vec())),
        }
    }
}

impl Default for PolicyEnforcer {
    fn default() -> Self {
        Self::permissive()
    }
}
