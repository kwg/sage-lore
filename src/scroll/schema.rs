// SPDX-License-Identifier: MIT
//! Scroll schema types for SAGE Method scrolls.
//!
//! This module defines all types for parsing and representing scrolls,
//! including the Scroll root struct, all Step variants, and supporting types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Scroll Root
// ============================================================================

/// Type constraint for required/provided variables
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TypeConstraint {
    #[default]
    Any,
    String,
    Number,
    Bool,
    Sequence,
    Mapping,
}

/// Specification for a required input variable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequireSpec {
    /// Expected type (optional - defaults to Any)
    #[serde(default, rename = "type")]
    pub type_constraint: TypeConstraint,

    /// Human-readable description (shown in error messages)
    #[serde(default)]
    pub description: Option<String>,

    /// Default value if not provided by caller
    #[serde(default)]
    pub default: Option<serde_json::Value>,
}

/// Specification for a promised output variable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvideSpec {
    /// Expected type of the output
    #[serde(rename = "type")]
    pub type_constraint: TypeConstraint,

    /// Human-readable description
    #[serde(default)]
    pub description: Option<String>,
}

/// Top-level scroll structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scroll {
    /// Scroll identifier
    pub scroll: String,

    /// Human-readable description
    pub description: String,

    /// Required input variables (validated before execution)
    #[serde(default)]
    pub requires: Option<HashMap<String, RequireSpec>>,

    /// Promised output variables (validated after execution)
    #[serde(default)]
    pub provides: Option<HashMap<String, ProvideSpec>>,

    /// Ordered list of steps to execute
    pub steps: Vec<Step>,
}

// ============================================================================
// Step Enum (Untagged)
// ============================================================================

/// A single step in a scroll
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Step {
    // === Core Primitives ===
    Elaborate(ElaborateStep),
    Distill(DistillStep),
    Split(SplitStep),
    Merge(MergeStep),
    Validate(ValidateStep),
    Convert(ConvertStep),

    // === System Primitives ===
    Fs(FsStep),
    Vcs(VcsStep),
    Test(TestStep),
    Platform(PlatformStep),
    Run(RunStep),

    // === Agent Operations ===
    Invoke(InvokeStep),
    Parallel(ParallelStep),      // Phase 2: fan-out same prompt to multiple agents (agent parallelism)
    Consensus(ConsensusStep),
    Concurrent(ConcurrentStep),  // Phase 2: run multiple different operations simultaneously (operation parallelism)

    // === Flow Control ===
    Branch(BranchStep),
    Loop(LoopStep),
    Aggregate(AggregateStep),

    // === Data Wiring ===
    Set(SetStep),

    // === Security ===
    Secure(SecureStep),

}

// ============================================================================
// Elaborate Primitive — expand terse input into detailed prose (Contract #42)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElaborateStep {
    pub elaborate: ElaborateParams,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub on_fail: OnFail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElaborateParams {
    pub input: String,
    #[serde(default)]
    pub depth: DepthLevel,
    #[serde(default)]
    pub output_contract: Option<OutputContract>,
    #[serde(default)]
    pub context: Option<serde_json::Value>,
    /// Override the LLM backend for this step. "claude" or "ollama".
    #[serde(default)]
    pub backend: Option<String>,
    /// Model tier for routing: cheap, standard, or premium.
    #[serde(default)]
    pub model_tier: Option<String>,
    /// Explicit model name override. Takes priority over model_tier and defaults.
    #[serde(default)]
    pub model: Option<String>,
    /// JSON schema for structured output enforcement. Overrides auto-gen schemas.
    /// Passed to Ollama for grammar-based token masking. Claude ignores (prompt-only).
    #[serde(default)]
    pub format_schema: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepthLevel {
    Thorough,
    #[default]
    Balanced,
    Concise,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputContract {
    #[serde(default)]
    pub length: OutputLength,
    #[serde(default)]
    pub format: OutputFormat,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputLength {
    Sentence,
    #[default]
    Paragraph,
    Page,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    #[default]
    Prose,
    Structured,
    List,
}

// ============================================================================
// Distill Primitive — compress verbose content into concise form (Contract #43)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillStep {
    pub distill: DistillParams,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub on_fail: OnFail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistillParams {
    pub input: String,
    #[serde(default)]
    pub intensity: IntensityLevel,
    #[serde(default)]
    pub output_contract: Option<DistillOutputContract>,
    #[serde(default)]
    pub context: Option<serde_json::Value>,
    /// Override the LLM backend for this step. "claude" or "ollama".
    #[serde(default)]
    pub backend: Option<String>,
    /// Model tier for routing: cheap, standard, or premium.
    #[serde(default)]
    pub model_tier: Option<String>,
    /// Explicit model name override. Takes priority over model_tier and defaults.
    #[serde(default)]
    pub model: Option<String>,
    /// JSON schema for structured output enforcement. Overrides auto-gen schemas.
    /// Passed to Ollama for grammar-based token masking. Claude ignores (prompt-only).
    #[serde(default)]
    pub format_schema: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntensityLevel {
    Aggressive,
    #[default]
    Balanced,
    Minimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillOutputContract {
    #[serde(default)]
    pub length: DistillLength,
    #[serde(default)]
    pub format: DistillFormat,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistillLength {
    Keywords,   // 3-15 tokens
    Phrase,     // 10-30 tokens
    #[default]
    Sentence,   // 25-75 tokens
    Paragraph,  // 75-300 tokens
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistillFormat {
    #[default]
    Prose,
    Bullets,
    Keywords,
}

// ============================================================================
// Split Primitive — decompose content into structured parts (Contract #44)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitStep {
    pub split: SplitParams,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub on_fail: OnFail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitParams {
    pub input: String,
    #[serde(default)]
    pub by: SplitStrategy,
    #[serde(default)]
    pub granularity: Granularity,
    /// For by=count strategy
    #[serde(default)]
    pub count: Option<usize>,
    /// For by=structure strategy
    #[serde(default)]
    pub markers: Option<StructuralMarkers>,
    #[serde(default)]
    pub context: Option<serde_json::Value>,
    /// Override the LLM backend for this step. "claude" or "ollama".
    #[serde(default)]
    pub backend: Option<String>,
    /// Model tier for routing: cheap, standard, or premium.
    #[serde(default)]
    pub model_tier: Option<String>,
    /// Explicit model name override. Takes priority over model_tier and defaults.
    #[serde(default)]
    pub model: Option<String>,
    /// JSON schema for structured output enforcement. Overrides auto-gen schemas.
    /// Passed to Ollama for grammar-based token masking. Claude ignores (prompt-only).
    #[serde(default)]
    pub format_schema: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitStrategy {
    #[default]
    Semantic,
    Structure,
    Count,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Granularity {
    Coarse,
    #[default]
    Medium,
    Fine,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuralMarkers {
    Headers,
    Paragraphs,
    Sentences,
    Bullets,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitChunk {
    pub id: usize,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

// ============================================================================
// Merge Primitive — combine multiple inputs into unified output (Contract #45)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeStep {
    pub merge: MergeParams,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub on_fail: OnFail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeParams {
    pub inputs: Vec<String>,  // Array of variable refs, min 2, max 10
    #[serde(default)]
    pub strategy: MergeStrategy,
    #[serde(default)]
    pub output_contract: Option<MergeOutputContract>,
    #[serde(default)]
    pub context: Option<serde_json::Value>,
    /// Override the LLM backend for this step. "claude" or "ollama".
    #[serde(default)]
    pub backend: Option<String>,
    /// Model tier for routing: cheap, standard, or premium.
    #[serde(default)]
    pub model_tier: Option<String>,
    /// Explicit model name override. Takes priority over model_tier and defaults.
    #[serde(default)]
    pub model: Option<String>,
    /// JSON schema for structured output enforcement. Overrides auto-gen schemas.
    /// Passed to Ollama for grammar-based token masking. Claude ignores (prompt-only).
    #[serde(default)]
    pub format_schema: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    #[default]
    Sequential,    // Combine in order
    Reconcile,     // Identify conflicts, resolve
    Union,         // All unique points
    Intersection,  // Only common points
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeOutputContract {
    #[serde(default)]
    pub format: OutputFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeConflict {
    pub topic: String,
    pub inputs: Vec<usize>,  // 1-indexed
    pub resolution: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateStep {
    pub validate: ValidateParams,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub on_fail: OnFail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidateParams {
    pub input: String,
    #[serde(default)]
    pub reference: Option<String>,
    pub criteria: serde_json::Value,
    #[serde(default)]
    pub mode: ValidationMode,
    /// Override the LLM backend for this step. "claude" or "ollama".
    #[serde(default)]
    pub backend: Option<String>,
    /// Model tier for routing: cheap, standard, or premium.
    #[serde(default)]
    pub model_tier: Option<String>,
    /// Explicit model name override. Takes priority over model_tier and defaults.
    #[serde(default)]
    pub model: Option<String>,
    /// JSON schema for structured output enforcement. Overrides auto-gen schemas.
    /// Passed to Ollama for grammar-based token masking. Claude ignores (prompt-only).
    #[serde(default)]
    pub format_schema: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationMode {
    #[default]
    Strict,      // All criteria must pass (score == 1.0)
    Majority,    // >50% must pass (score > 0.5)
    Any,         // At least one must pass (score > 0.0)
}

// Expected output structure for validate primitive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub result: ValidationOutcome,
    pub score: f64,
    pub criteria_results: Vec<CriterionResult>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidationOutcome {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriterionResult {
    pub criterion: String,
    pub passed: bool,
    pub explanation: String,
}

// ============================================================================
// Convert Primitive — parse and transform between data formats with optional schema validation (Contract #47)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertStep {
    pub convert: ConvertParams,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub on_fail: OnFail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConvertParams {
    pub input: String,
    pub to: ConvertTarget,  // Target format (can be string or object with schema)
    #[serde(default)]
    pub from: Option<String>,  // Optional source format hint
    #[serde(default)]
    pub coercion: CoercionMode,  // Type coercion mode (auto/strict)
    #[serde(default)]
    pub context: Option<serde_json::Value>,
    /// Override the LLM backend for this step. "claude" or "ollama".
    #[serde(default)]
    pub backend: Option<String>,
    /// Model tier for routing: cheap, standard, or premium.
    #[serde(default)]
    pub model_tier: Option<String>,
    /// Explicit model name override. Takes priority over model_tier and defaults.
    #[serde(default)]
    pub model: Option<String>,
    /// JSON schema for structured output enforcement. Overrides auto-gen schemas.
    /// Passed to Ollama for grammar-based token masking. Claude ignores (prompt-only).
    #[serde(default)]
    pub format_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConvertTarget {
    Simple(String),  // "json", "yaml", "markdown", etc.
    Detailed {
        format: String,
        schema: serde_json::Value,  // JSON Schema
    },
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CoercionMode {
    #[default]
    Auto,    // Engine coerces obvious type mismatches
    Strict,  // No coercion, fail on type mismatch
}

// ============================================================================
// System Primitive Steps
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsStep {
    pub fs: FsParams,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub on_fail: OnFail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FsParams {
    pub operation: FsOperation,
    pub path: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub dest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FsOperation {
    Read,
    Write,
    Append,
    Delete,
    Copy,
    Move,
    List,
    Exists,
    Mkdir,
    Stat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcsStep {
    pub vcs: VcsParams,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub on_fail: OnFail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VcsParams {
    pub operation: VcsOperation,
    /// For Commit/StashPush/Tag: commit/stash/tag message
    #[serde(default)]
    pub message: Option<String>,
    /// For Add/Unstage: list of file paths
    #[serde(default)]
    pub files: Option<Vec<String>>,
    /// For Checkout/Merge/Pull: branch name
    #[serde(default)]
    pub branch: Option<String>,
    /// For Branch/DeleteBranch/Tag/BranchExists: name
    #[serde(default)]
    pub name: Option<String>,
    /// For Push: set upstream tracking
    #[serde(default)]
    pub set_upstream: Option<bool>,
    /// For Diff/Log: scope/range
    #[serde(default)]
    pub scope: Option<String>,
    /// For Fetch/Pull/Push: remote name (default: origin)
    #[serde(default)]
    pub remote: Option<String>,
    /// For ResetHard/ResetSoft/ResolveRef: git ref (commit/branch/tag)
    #[serde(default)]
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VcsOperation {
    // Core operations
    Commit,
    Status,
    Diff,
    Log,
    // Branch operations
    Branch,
    Checkout,
    EnsureBranch,
    CurrentBranch,
    BranchExists,
    DeleteBranch,
    // Staging operations
    Add,
    Unstage,
    // Remote operations
    Push,
    Fetch,
    Pull,
    PrBranchReady,
    // Merge operations
    Merge,
    Squash,
    AbortMerge,
    // Stash operations
    StashPush,
    StashPop,
    StashList,
    // Reset operations
    ResetHard,
    ResetSoft,
    // Reference operations
    Head,
    HeadShort,
    ResolveRef,
    // Tag operations
    Tag,
    ListTags,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestStep {
    pub test: TestParams,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub on_fail: OnFail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestParams {
    pub operation: TestOperation,
    /// For Run/RunFiltered: pattern to filter tests
    #[serde(default)]
    pub pattern: Option<String>,
    /// For RunFiles: specific test files to run
    #[serde(default)]
    pub files: Option<Vec<String>>,
    /// Additional test configuration
    #[serde(default)]
    pub config: Option<serde_json::Value>,
    /// For Verify: which verification tool to use
    #[serde(default)]
    pub tool: Option<String>,
    /// For Verify: input data for the verification tool
    #[serde(default)]
    pub input: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestOperation {
    Run,
    Coverage,
    Smoke,
    RunFiltered,
    RunFiles,
    Info,
    Verify,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformStep {
    pub platform: PlatformParams,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub on_fail: OnFail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformParams {
    pub operation: PlatformOperation,
    /// For Env operation: environment variable name to get
    #[serde(default)]
    pub var: Option<String>,
    /// For Check operation: command/tool name to check for
    #[serde(default)]
    pub command: Option<String>,
    /// For issue/PR/milestone operations: the number/id
    #[serde(default)]
    pub number: Option<String>,
    /// For CreateIssue/CreatePr/CreateMilestone: structured payload
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
    /// For AddLabels/RemoveLabels: list of label names
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    /// For CreateComment: comment body text
    #[serde(default)]
    pub body: Option<String>,
    /// For CreatePr: source branch name
    #[serde(default)]
    pub head: Option<String>,
    /// For CreatePr: target branch name
    #[serde(default)]
    pub base: Option<String>,
    /// For CreateIssue/CreatePr/CreateMilestone: title
    #[serde(default)]
    pub title: Option<String>,
    /// For CreateMilestone: description
    #[serde(default)]
    pub description: Option<String>,
    /// For MergePr: merge strategy (merge/squash/rebase)
    #[serde(default)]
    pub strategy: Option<String>,
    /// For ListIssues: filter by state (open/closed/all)
    #[serde(default)]
    pub state: Option<String>,
    /// For ListIssues: filter by milestone id (string to support ${var} interpolation)
    #[serde(default)]
    pub milestone: Option<String>,
    /// For ListIssues: filter by assignee
    #[serde(default)]
    pub assignee: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformOperation {
    // Environment/system operations
    Env,
    Info,
    Check,
    // Issue operations
    CreateIssue,
    GetIssue,
    CloseIssue,
    ListIssues,
    // Label operations
    AddLabels,
    RemoveLabels,
    // Comment operations
    CreateComment,
    GetComments,
    // Milestone operations
    CreateMilestone,
    GetMilestone,
    // Pull request operations
    CreatePr,
    GetPr,
    MergePr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStep {
    pub run: RunParams,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub on_fail: OnFail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunParams {
    pub scroll_path: String,
    #[serde(default)]
    pub args: Option<HashMap<String, serde_json::Value>>,
}

// ============================================================================
// Agent Operation Steps
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeStep {
    pub invoke: InvokeParams,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub on_fail: OnFail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvokeParams {
    pub agent: String,
    /// Short, directive task-level instruction (one sentence preferred).
    /// Domain expertise belongs in agent definition files, not here.
    pub instructions: String,
    #[serde(default)]
    pub context: Option<Vec<String>>,
    /// Timeout in seconds for the LLM call. Default: 120, max: 1200.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// Override the LLM backend for this call. "claude" or "ollama".
    /// If not set, uses the default backend (SAGE_LLM_BACKEND env var).
    #[serde(default)]
    pub backend: Option<String>,
    /// Model tier for routing: cheap, standard, or premium.
    #[serde(default)]
    pub model_tier: Option<String>,
    /// Explicit model name override. Takes priority over model_tier and defaults.
    #[serde(default)]
    pub model: Option<String>,
    /// Output schema for post-invocation structured extraction.
    /// When present, the engine parses the LLM's raw text response into
    /// a structured JSON value matching this schema. Uses local parsing
    /// (no extra LLM call): strip markdown fences → serde parse → schema validate.
    /// The parsed+validated value is bound to `output:` instead of raw text.
    #[serde(default)]
    pub output_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelStep {
    pub parallel: ParallelParams,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub on_fail: OnFail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParallelParams {
    pub agents: Vec<String>,
    pub prompt: String,
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
    #[serde(default)]
    pub timeout_per_agent: Option<u64>,
    #[serde(default)]
    pub on_fail: ParallelFailMode,
    #[serde(default)]
    pub quorum: Option<usize>,
}

fn default_max_concurrent() -> usize {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ParallelFailMode {
    #[default]
    RequireAll,
    RequireQuorum,
    BestEffort,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusStep {
    pub consensus: ConsensusParams,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub on_fail: OnFail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsensusParams {
    pub agents: Vec<String>,
    pub proposal: String,
    pub mechanism: String,
    pub options: Vec<String>,
    #[serde(default = "default_threshold")]
    pub threshold: ThresholdSpec,
}

fn default_threshold() -> ThresholdSpec {
    ThresholdSpec::Named(ThresholdType::Majority)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ThresholdSpec {
    Named(ThresholdType),
    Numeric(usize),
}

impl Default for ThresholdSpec {
    fn default() -> Self {
        ThresholdSpec::Named(ThresholdType::Majority)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdType {
    Majority,
    Supermajority,
    Unanimous,
}

/// Run multiple different operations simultaneously.
/// Unlike `parallel` (which fans out the same prompt to multiple agents),
/// `concurrent` runs different operations in parallel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrentStep {
    pub concurrent: ConcurrentParams,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub on_fail: OnFail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConcurrentParams {
    pub operations: Vec<Step>,
    #[serde(default)]
    pub timeout: Option<u64>, // timeout in seconds
}

// ============================================================================
// Flow Control Steps
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchStep {
    pub branch: BranchParams,
    #[serde(default)]
    pub output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BranchParams {
    pub condition: String,
    pub if_true: Vec<Step>,
    #[serde(default)]
    pub if_false: Option<Vec<Step>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopStep {
    #[serde(rename = "loop")]
    pub loop_params: LoopParams,
    #[serde(default)]
    pub output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopParams {
    pub items: String,
    #[serde(default = "default_item_var")]
    pub item_var: String,
    pub operation: Vec<Step>,
    #[serde(rename = "while")]
    #[serde(default)]
    pub while_cond: Option<String>,
    #[serde(default)]
    pub max: Option<u32>,
}

fn default_item_var() -> String {
    "item".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateStep {
    pub aggregate: AggregateParams,
    #[serde(default)]
    pub output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AggregateParams {
    pub results: Vec<String>,
    pub strategy: String,
}

// ============================================================================
// Data Wiring Steps
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetStep {
    pub set: SetParams,
    #[serde(default)]
    pub output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetParams {
    pub values: serde_json::Value,
}

// ============================================================================
// Security Steps and Enums
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureStep {
    pub secure: SecureParams,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub on_fail: OnFail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecureParams {
    #[serde(default)]
    pub input: Option<String>,
    pub scan_type: ScanType,
    #[serde(default)]
    pub policy: SecurityPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanType {
    DependencyCve,
    SecretDetection,
    StaticAnalysis,
    #[serde(untagged)]
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SecurityPolicy {
    Block,
    #[default]
    Warn,
    Audit,
}

// ============================================================================
// Error Handling
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OnFail {
    #[default]
    Halt,
    Continue,
    CollectErrors,
    #[serde(rename = "retry")]
    Retry(RetryConfig),
    #[serde(rename = "fallback")]
    Fallback(Vec<Step>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max: u32,
}
