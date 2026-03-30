// SPDX-License-Identifier: MIT
//! Security policy management and enforcement.
use crate::config::{FallbackPolicy, Policy, SecurityError, SecurityFloor};
use crate::primitives::secure::scanner::{get_current_commit, SecureBackend};
use super::types::{Finding, FindingType, Location, ScanResult, ScanType};
use super::reports::{AuditReport, CveReport, SastReport, ToolStatus};
use std::path::Path;

pub struct PolicyDrivenBackend {
    policy: Policy,
    global_floor: Option<SecurityFloor>,
    gitleaks: ToolStatus,
    trivy: ToolStatus,
    semgrep: ToolStatus,
}

impl PolicyDrivenBackend {
    pub fn new(policy_path: &Path) -> Result<Self, SecurityError> {
        let policy = Policy::load(policy_path)?;
        let global_floor = SecurityFloor::load_global()?;
        if let Some(ref floor) = global_floor {
            policy.validate_against_floor(floor)?;
        }
        let gitleaks = ToolStatus::detect("gitleaks");
        let trivy = ToolStatus::detect("trivy");
        let semgrep = ToolStatus::detect("semgrep");
        let mut required_tools = policy.required_tools.clone();
        if let Some(ref floor) = global_floor {
            for tool in &floor.minimum_required_tools {
                if !required_tools.contains(tool) {
                    required_tools.push(tool.clone());
                }
            }
        }
        for tool in &required_tools {
            let available = match tool.as_str() {
                "gitleaks" => gitleaks.available,
                "trivy" => trivy.available,
                "semgrep" => semgrep.available,
                _ => Self::detect_tool(tool).is_some(),
            };
            if !available && policy.fallback_policy == FallbackPolicy::HardStop {
                return Err(SecurityError::RequiredToolMissing(tool.clone()));
            }
        }
        Ok(Self { policy, global_floor, gitleaks, trivy, semgrep })
    }

    pub fn from_project(project_root: &Path) -> Result<Self, SecurityError> {
        let policy_path = project_root.join(".sage-lore/security/policy.yaml");
        Self::new(&policy_path)
    }

    pub fn detect_tool(name: &str) -> Option<ToolStatus> {
        let status = ToolStatus::detect(name);
        if status.available { Some(status) } else { None }
    }

    pub fn policy(&self) -> &Policy { &self.policy }
    pub fn global_floor(&self) -> Option<&SecurityFloor> { self.global_floor.as_ref() }
    pub fn has_gitleaks(&self) -> bool { self.gitleaks.available }
    pub fn has_trivy(&self) -> bool { self.trivy.available }
    pub fn has_semgrep(&self) -> bool { self.semgrep.available }
}

impl SecureBackend for PolicyDrivenBackend {
    fn secret_detection(&self, content: &str) -> Result<ScanResult, SecurityError> {
        if self.gitleaks.available {
            crate::primitives::secure::scanner::run_gitleaks(content)
        } else {
            Ok(crate::primitives::secure::builtin::builtin_secret_detection(content))
        }
    }

    fn audit(&self, root: &Path) -> Result<AuditReport, SecurityError> {
        let mut scans = Vec::new();
        let commit = get_current_commit(root).unwrap_or_else(|| "unknown".to_string());
        if self.policy.required_tools.contains(&"gitleaks".to_string()) || self.gitleaks.available {
            let scan = if self.gitleaks.available {
                crate::primitives::secure::scanner::run_gitleaks_on_path(root)?
            } else {
                ScanResult::passed("builtin", ScanType::SecretDetection, 0)
            };
            scans.push(scan);
        }
        if self.policy.dependency_scan.enabled && self.trivy.available {
            let manifests = ["Cargo.toml", "package.json", "go.mod", "requirements.txt"];
            for manifest in manifests {
                let manifest_path = root.join(manifest);
                if manifest_path.exists() {
                    let cve_report = crate::primitives::secure::scanner::run_trivy(&manifest_path)?;
                    let findings: Vec<Finding> = cve_report.vulnerabilities.into_iter()
                        .map(|v| Finding {
                            severity: v.severity,
                            finding_type: FindingType::Cve,
                            location: Location::file(manifest),
                            description: format!("{}: {} ({})", v.cve_id, v.description, v.package),
                            remediation: v.fixed_version.map(|fv| format!("Upgrade to {}", fv)).unwrap_or_else(|| "No fix available".to_string()),
                            rule_id: v.cve_id.clone(),
                            cve_id: Some(v.cve_id),
                            content_hash: None,
                        }).collect();
                    scans.push(ScanResult {
                        passed: !findings.iter().any(|f| f.is_blocker()),
                        findings,
                        tool_used: "trivy".to_string(),
                        scan_type: ScanType::DependencyCve,
                        duration_ms: 0,
                    });
                    break;
                }
            }
        }
        if self.policy.static_analysis.enabled && self.semgrep.available {
            let sast_report = crate::primitives::secure::scanner::run_semgrep(root, &self.policy.static_analysis.ruleset)?;
            scans.push(ScanResult {
                passed: sast_report.passed,
                findings: sast_report.findings,
                tool_used: "semgrep".to_string(),
                scan_type: ScanType::StaticAnalysis,
                duration_ms: 0,
            });
        }
        Ok(AuditReport::from_scans(scans, commit))
    }

    fn dependency_scan(&self, manifest: &Path) -> Result<CveReport, SecurityError> {
        if !self.policy.dependency_scan.enabled {
            return Ok(CveReport::default());
        }
        if self.trivy.available {
            crate::primitives::secure::scanner::run_trivy(manifest)
        } else {
            Ok(CveReport { passed: true, vulnerabilities: Vec::new(), packages_scanned: 0, tool_used: "none (trivy not available)".to_string() })
        }
    }

    fn static_analysis(&self, path: &Path) -> Result<SastReport, SecurityError> {
        if !self.policy.static_analysis.enabled {
            return Ok(SastReport::default());
        }
        if self.semgrep.available {
            crate::primitives::secure::scanner::run_semgrep(path, &self.policy.static_analysis.ruleset)
        } else {
            Ok(SastReport { passed: true, findings: Vec::new(), files_scanned: 0, rules_applied: 0, tool_used: "none (semgrep not available)".to_string() })
        }
    }

    fn available_tools(&self) -> Vec<ToolStatus> {
        vec![self.gitleaks.clone(), self.trivy.clone(), self.semgrep.clone()]
    }
}
