// SPDX-License-Identifier: MIT
//! Primitive types for the SAGE Method engine.

pub mod fs;
pub mod invoke;
pub mod platform;
pub mod secure;
pub mod test;
pub mod vcs;
// Note: test.rs has been decomposed into test/ module directory

pub use fs::{
    DeletePolicy, FileMeta, FsBackend, FsCall, FsError, FsPolicy, MockFsBackend, SecureFsBackend,
    DEFAULT_ALLOWED_EXTENSIONS, DEFAULT_PROTECTED,
};

pub use vcs::{
    BranchResult, CommitResult, DiffHunk, DiffResult, DiffScope, FileDiff, FileStatus,
    FileStatusType, ForceMode, Git2Backend, GitBackend, GitCall, GitError, LogEntry, MergeResult,
    PrResult, StashEntry, StashRef, Status, SubmodulePolicy,
};
pub use secure::{
    compute_content_hash, AuditReport, CveEntry, CveReport, Finding, FindingType, Location,
    PolicyDrivenBackend, SastReport, ScanResult, ScanType, SecureBackend, Severity, ToolStatus,
};
pub use platform::{
    Comment, CreateIssueRequest, CreatePrRequest, ForgejoBackend, Issue, IssueFilter,
    MergeStrategy, Milestone, MockPlatform, MockResponse, MockResponseKey, Platform, PlatformCall,
    PlatformError, PlatformResult, PrBranch, PullRequest,
};
pub use invoke::{
    ClaudeCliBackend, LlmBackend, LlmError, LlmRequest, LlmResponse, LlmResult, MockLlmBackend,
    MockResponseKey as LlmMockResponseKey, OllamaBackend,
};
pub use test::{
    AutoDetectBackend, BatsBackend, CargoBackend, CoverageConfig, CoverageResult, FileCoverage,
    FlakyConfig, Framework, GoBackend, JestBackend, MakeBackend, NoopBackend, NpmBackend,
    PytestBackend, RetryRecord, SmokeConfig, TestBackend, TestConfig, TestError, TestFailure,
    TestResult, TestRunResult, TestSummary, VitestBackend, WatchHandle,
};
