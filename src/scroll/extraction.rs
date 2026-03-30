// SPDX-License-Identifier: MIT
//! LLM response extraction and prompt building utilities.
//!
//! This module handles the extraction of structured content from LLM responses
//! and builds prompts for each primitive type.

use crate::scroll::error::ExecutionError;

// ============================================================================
// LLM Response Extraction — strip markdown fences, prose prefixes, and extract structured data from raw LLM output (Bug #305)
// ============================================================================

/// Find the closing ``` fence that isn't part of a nested code block.
/// A closing fence is ``` at the start of a line (after a newline) that is NOT
/// immediately followed by a language identifier (which would make it an opening fence).
fn find_closing_fence(content: &str) -> Option<usize> {
    let mut search_from = 0;
    while let Some(pos) = content[search_from..].find("```") {
        let abs_pos = search_from + pos;

        // Check if this ``` is at the start of a line
        let at_line_start = abs_pos == 0 || content.as_bytes()[abs_pos - 1] == b'\n';

        if at_line_start {
            // Check what follows the ``` — if it's a letter, it's an opening fence (e.g., ```rust)
            let after = &content[abs_pos + 3..];
            let is_opening = after.starts_with(|c: char| c.is_ascii_alphabetic());

            if !is_opening {
                return Some(abs_pos);
            }
        }

        search_from = abs_pos + 3;
    }
    None
}

/// Extract structured content from LLM response.
///
/// LLMs often wrap output in markdown or prose. This function extracts the
/// actual structured content using a priority order:
/// 1. XML tags: `<yaml>...</yaml>` (most reliable)
/// 2. Markdown fences: ```yaml ... ```
/// 3. Raw content with prose prefix stripped
///
/// Backend-agnostic: works with Claude CLI, Ollama, or any LLM.
pub fn extract_structured_content(response: &str, format: &str) -> String {
    let response = response.trim();

    // 1. Try XML tags (e.g., <yaml>...</yaml>)
    let open_tag = format!("<{}>", format);
    let close_tag = format!("</{}>", format);
    if let Some(start) = response.find(&open_tag) {
        if let Some(end) = response.find(&close_tag) {
            let content_start = start + open_tag.len();
            if content_start < end {
                return response[content_start..end].trim().to_string();
            }
        }
    }

    // 2. Try markdown fences (```yaml ... ```)
    // Uses find_closing_fence to handle nested code fences in content
    let fence_start = format!("```{}", format);
    if let Some(start) = response.find(&fence_start) {
        let content_start = start + fence_start.len();
        // Skip optional newline after fence
        let content_start = if response[content_start..].starts_with('\n') {
            content_start + 1
        } else {
            content_start
        };
        if let Some(end) = find_closing_fence(&response[content_start..]) {
            return response[content_start..content_start + end].trim().to_string();
        }
    }

    // Also try bare ``` fences
    if let Some(start) = response.find("```\n") {
        let content_start = start + 4;
        if let Some(end) = response[content_start..].find("```") {
            let content = response[content_start..content_start + end].trim();
            // Only use if it looks like YAML (starts with - or key:)
            if content.starts_with('-') || content.contains(':') {
                return content.to_string();
            }
        }
    }

    // 3. Strip common prose prefixes and return
    let stripped = strip_prose_prefix(response);
    stripped.to_string()
}

/// Strip common LLM prose prefixes from response.
fn strip_prose_prefix(response: &str) -> &str {
    let lines: Vec<&str> = response.lines().collect();

    // Look for first line that starts YAML structure
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('-') || trimmed.starts_with('{') || trimmed.starts_with('[') {
            // Return from this line onward
            return &response[response.find(line).unwrap_or(0)..];
        }
        // If line contains ":" and doesn't look like prose, might be YAML mapping
        if trimmed.contains(':') && !trimmed.contains(' ') && i < 3 {
            return &response[response.find(line).unwrap_or(0)..];
        }
    }

    // No structure found, return as-is
    response.trim()
}

/// Parse a serde_json::Value that might be a String containing structured data into an Array.
/// Used by decompose primitive which always expects a list output.
pub fn parse_as_sequence(value: &serde_json::Value) -> Result<serde_json::Value, ExecutionError> {
    match value {
        // Already an array - return as-is
        serde_json::Value::Array(_) => Ok(value.clone()),

        // String that might contain structured data - try to parse
        serde_json::Value::String(s) => {
            // Extract structured content first (handles XML tags, markdown fences, prose)
            let extracted = extract_structured_content(s, "yaml");

            // Try JSON first, fall back to YAML
            let parsed: serde_json::Value = if let Ok(v) = serde_json::from_str(&extracted) {
                v
            } else {
                serde_json::to_value(&serde_yaml::from_str::<serde_yaml::Value>(&extracted)
                    .map_err(|e| ExecutionError::ParseError(format!(
                        "Expected sequence, got unparseable string: {}. Content: {}",
                        e,
                        if extracted.len() > 100 { &extracted[..100] } else { &extracted }
                    )))?)
                .map_err(|e| ExecutionError::ParseError(format!(
                    "Failed to convert YAML to JSON: {}", e
                )))?
            };

            // Verify it's actually an array
            match parsed {
                serde_json::Value::Array(_) => Ok(parsed),
                other => Err(ExecutionError::TypeError(format!(
                    "Expected sequence, got {}",
                    value_type_name(&other)
                ))),
            }
        }

        // Any other type is an error
        other => Err(ExecutionError::TypeError(format!(
            "Expected sequence, got {}",
            value_type_name(other)
        ))),
    }
}

/// Parse a serde_json::Value that might be a String containing structured data into an Object.
/// Used when a primitive expects structured object output.
pub fn parse_as_mapping(value: &serde_json::Value) -> Result<serde_json::Value, ExecutionError> {
    match value {
        // Already an object - return as-is
        serde_json::Value::Object(_) => Ok(value.clone()),

        // String that might contain structured data - try to parse
        serde_json::Value::String(s) => {
            // Extract structured content first (handles XML tags, markdown fences, prose)
            let extracted = extract_structured_content(s, "yaml");

            // Try JSON first, fall back to YAML
            let parsed: serde_json::Value = if let Ok(v) = serde_json::from_str(&extracted) {
                v
            } else {
                serde_json::to_value(&serde_yaml::from_str::<serde_yaml::Value>(&extracted)
                    .map_err(|e| ExecutionError::ParseError(format!(
                        "Expected mapping, got unparseable string: {}. Content: {}",
                        e,
                        if extracted.len() > 100 { &extracted[..100] } else { &extracted }
                    )))?)
                .map_err(|e| ExecutionError::ParseError(format!(
                    "Failed to convert YAML to JSON: {}", e
                )))?
            };

            match parsed {
                serde_json::Value::Object(_) => Ok(parsed),
                other => Err(ExecutionError::TypeError(format!(
                    "Expected mapping, got {}",
                    value_type_name(&other)
                ))),
            }
        }

        // Any other type is an error
        other => Err(ExecutionError::TypeError(format!(
            "Expected mapping, got {}",
            value_type_name(other)
        ))),
    }
}

/// Check if string looks like YAML structure.
/// D15: Tight heuristic to avoid false positives from prose.
/// Detects:
/// - Sequences: starts with `-`
/// - JSON objects/arrays: starts with `{` or `[`
/// - YAML mappings: first line is `key:` or `key: value` pattern
///   where key is lowercase/snake_case (not capitalized prose)
pub fn is_likely_yaml_structure(s: &str) -> bool {
    let trimmed = s.trim();

    // Sequences and JSON
    if trimmed.starts_with('-') || trimmed.starts_with('{') || trimmed.starts_with('[') {
        return true;
    }

    // YAML mapping: first line should be "key:" or "key: value"
    // Key must look like a YAML key, not prose
    if let Some(first_line) = trimmed.lines().next() {
        let line = first_line.trim();
        if let Some(colon_pos) = line.find(':') {
            let key_part = &line[..colon_pos];
            // Key requirements:
            // 1. Not empty
            // 2. No spaces (not prose like "Here's the answer")
            // 3. Contains only valid identifier chars
            // 4. Starts with lowercase or underscore (not capitalized prose like "Note:")
            // 5. If all lowercase, likely YAML. If contains underscore/hyphen, likely YAML.
            if !key_part.is_empty()
                && !key_part.contains(' ')
                && key_part.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            {
                let first_char = key_part.chars().next().unwrap();
                // Reject if starts with uppercase (likely prose like "Note:", "Here:", etc.)
                // Accept if: starts lowercase, starts with underscore, or contains underscore/hyphen
                if first_char.is_lowercase()
                    || first_char == '_'
                    || key_part.contains('_')
                    || key_part.contains('-')
                {
                    return true;
                }
            }
        }
    }

    false
}

/// Try to parse as structured data, fall back to original value if not structured.
/// Used by transform which may output prose OR structure depending on the task.
pub fn try_parse_structured(value: &serde_json::Value) -> Result<serde_json::Value, ExecutionError> {
    match value {
        serde_json::Value::String(s) => {
            // Extract structured content first (handles XML tags, markdown fences, prose)
            let extracted = extract_structured_content(s, "yaml");

            // Only try parsing if it looks like YAML structure (D15)
            if is_likely_yaml_structure(&extracted) {
                // Try JSON first, fall back to YAML
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&extracted) {
                    if !matches!(parsed, serde_json::Value::String(_)) {
                        return Ok(parsed);
                    }
                } else if let Ok(yaml_val) = serde_yaml::from_str::<serde_yaml::Value>(&extracted) {
                    if let Ok(json_val) = serde_json::to_value(&yaml_val) {
                        if !matches!(json_val, serde_json::Value::String(_)) {
                            return Ok(json_val);
                        }
                    }
                }
            }

            // Not structured or parsing failed - return original
            Ok(value.clone())
        }
        // Non-string values pass through unchanged
        _ => Ok(value.clone()),
    }
}

/// Get human-readable type name for error messages
pub fn value_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "sequence",
        serde_json::Value::Object(_) => "mapping",
    }
}

// ============================================================================
// Prompt Builder Helpers
// ============================================================================

/// Build prompt for elaborate primitive (contract #42).
pub fn build_elaborate_prompt(
    input: &serde_json::Value,
    depth: &crate::scroll::schema::DepthLevel,
    output_contract: Option<&crate::scroll::schema::OutputContract>,
    context: Option<&serde_json::Value>,
) -> Result<String, ExecutionError> {
    use crate::scroll::schema::{DepthLevel, OutputFormat, OutputLength};

    let input_str = serde_json::to_string(input)
        .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))?;

    let depth_str = match depth {
        DepthLevel::Thorough => "thorough",
        DepthLevel::Balanced => "balanced",
        DepthLevel::Concise => "concise",
    };

    let (length_str, token_range) = match output_contract.map(|c| &c.length) {
        Some(OutputLength::Sentence) => ("sentence", "15-75 tokens"),
        Some(OutputLength::Page) => ("page", "400-2000 tokens"),
        _ => ("paragraph", "75-400 tokens"),
    };

    let format_str = match output_contract.map(|c| &c.format) {
        Some(OutputFormat::Structured) => "structured",
        Some(OutputFormat::List) => "list",
        _ => "prose",
    };

    let context_section = if let Some(ctx) = context {
        let ctx_str = serde_json::to_string(ctx)
            .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))?;
        format!("## Domain Context\n{}\n\n", ctx_str)
    } else {
        String::new()
    };

    Ok(format!(
        "You are performing the ELABORATE operation.\n\n\
         ## Rules (invariants - non-negotiable)\n\
         1. PRESERVE INTENT: Your output must not change the meaning of the input.\n\
            Expand and add detail, but do not alter, reframe, or reinterpret the core message.\n\
         2. ADD DETAIL: Your output must contain more specificity than the input.\n\
            If the input is vague, make it concrete. If it lacks examples, add them.\n\n\
         ## Operation Parameters\n\
         - Depth: {depth}\n\
         - Target Length: {length} ({token_range})\n\
         - Target Format: {format}\n\n\
         {context}\
         ## Input\n\
         {input}\n\n\
         ## Output Requirements\n\
         Return ONLY the elaborated content. No preamble, no explanation.",
        depth = depth_str,
        length = length_str,
        token_range = token_range,
        format = format_str,
        context = context_section,
        input = input_str
    ))
}

/// Build prompt for distill primitive (contract #43).
pub fn build_distill_prompt(
    input: &serde_json::Value,
    intensity: &crate::scroll::schema::IntensityLevel,
    output_contract: Option<&crate::scroll::schema::DistillOutputContract>,
    context: Option<&serde_json::Value>,
) -> Result<String, ExecutionError> {
    use crate::scroll::schema::{IntensityLevel, DistillFormat, DistillLength};

    let input_str = serde_json::to_string(input)
        .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))?;

    let intensity_str = match intensity {
        IntensityLevel::Aggressive => "aggressive",
        IntensityLevel::Balanced => "balanced",
        IntensityLevel::Minimal => "minimal",
    };

    let (length_str, token_range) = match output_contract.map(|c| &c.length) {
        Some(DistillLength::Keywords) => ("keywords", "3-15 tokens"),
        Some(DistillLength::Phrase) => ("phrase", "10-30 tokens"),
        Some(DistillLength::Sentence) => ("sentence", "25-75 tokens"),
        Some(DistillLength::Paragraph) => ("paragraph", "75-300 tokens"),
        None => ("sentence", "25-75 tokens"),
    };

    let format_str = match output_contract.map(|c| &c.format) {
        Some(DistillFormat::Bullets) => "bullets",
        Some(DistillFormat::Keywords) => "keywords",
        _ => "prose",
    };

    let context_section = if let Some(ctx) = context {
        let ctx_str = serde_json::to_string(ctx)
            .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))?;
        format!("## Context\n{}\n\n", ctx_str)
    } else {
        String::new()
    };

    Ok(format!(
        "You are performing the DISTILL operation.\n\n\
         ## Rules (invariants - non-negotiable)\n\
         1. PRESERVE ESSENCE: Your output must retain the core meaning, claims, and assertions.\n\
            - Keep all quantities and measurements\n\
            - Maintain causal relationships\n\
            - Preserve contradictions (do not resolve them)\n\
         2. REMOVE REDUNDANCY: Eliminate repetition, filler words, and unnecessary elaboration.\n\
         3. NO HALLUCINATION: Do not add information that is not present in the input.\n\n\
         ## Operation Parameters\n\
         - Intensity: {intensity} (how aggressively to compress)\n\
         - Target Length: {length} ({token_range})\n\
         - Target Format: {format}\n\n\
         {context}\
         ## Input\n\
         {input}\n\n\
         ## Output Requirements\n\
         Return ONLY the distilled content. No preamble, no explanation.",
        intensity = intensity_str,
        length = length_str,
        token_range = token_range,
        format = format_str,
        context = context_section,
        input = input_str
    ))
}

/// Build prompt for split primitive (contract #44).
pub fn build_split_prompt(
    input: &serde_json::Value,
    by: &crate::scroll::schema::SplitStrategy,
    granularity: &crate::scroll::schema::Granularity,
    count: Option<usize>,
    markers: Option<&crate::scroll::schema::StructuralMarkers>,
    context: Option<&serde_json::Value>,
) -> Result<String, ExecutionError> {
    use crate::scroll::schema::{SplitStrategy, Granularity, StructuralMarkers};

    // For string values, use the raw string content (not JSON-escaped).
    // For structured values, serialize as JSON.
    let input_str = match input {
        serde_json::Value::String(s) => s.clone(),
        other => serde_json::to_string(other)
            .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))?,
    };

    let _strategy_str = match by {
        SplitStrategy::Semantic => "semantic",
        SplitStrategy::Structure => "structure",
        SplitStrategy::Count => "count",
    };

    let granularity_str = match granularity {
        Granularity::Coarse => "coarse",
        Granularity::Medium => "medium",
        Granularity::Fine => "fine",
    };

    let context_section = if let Some(ctx) = context {
        let ctx_str = serde_json::to_string(ctx)
            .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))?;
        format!("## Context\n{}\n\n", ctx_str)
    } else {
        String::new()
    };

    // Build strategy-specific instructions
    let strategy_instructions = match by {
        SplitStrategy::Semantic => {
            format!(
                "Split the content based on SEMANTIC COHERENCE - group related ideas together.\n\
                 Granularity: {} (coarse=major topics, medium=subtopics, fine=detailed points)",
                granularity_str
            )
        }
        SplitStrategy::Structure => {
            let markers_str = match markers {
                Some(StructuralMarkers::Headers) => "markdown headers (# ## ###)",
                Some(StructuralMarkers::Paragraphs) => "paragraph breaks (blank lines)",
                Some(StructuralMarkers::Sentences) => "sentence boundaries",
                Some(StructuralMarkers::Bullets) => "bullet points (- * •)",
                None => "structural markers (auto-detect)",
            };
            format!(
                "Split the content based on STRUCTURAL MARKERS: {}.\n\
                 Granularity: {} affects how to group sections.",
                markers_str, granularity_str
            )
        }
        SplitStrategy::Count => {
            let target_count = count.unwrap_or(5);
            format!(
                "Split the content into EXACTLY {} chunks of roughly equal size.\n\
                 If impossible (e.g., very short input), reduce count gracefully.\n\
                 Granularity: {} affects chunk boundaries (prefer natural breaks).",
                target_count, granularity_str
            )
        }
    };

    Ok(format!(
        "You are performing the SPLIT operation.\n\n\
         ## Rules (invariants - non-negotiable)\n\
         1. COMPLETE COVERAGE: Every character from the input must appear in exactly one chunk.\n\
            - No content should be lost or omitted\n\
            - No content should be duplicated (except structural markers like headers)\n\
            - Aim for 95%+ character coverage (whitespace normalization allowed)\n\
         2. ORDERED SEQUENCE: Chunks must preserve the original order of content.\n\
         3. NO OVERLAP: No sentence should appear verbatim in multiple chunks.\n\
            - Structural markers (headers) may repeat for labeling purposes\n\
         4. NO HALLUCINATION: Chunks must contain ONLY content from the input.\n\
            - Do not add explanations, summaries, or new content\n\n\
         ## Strategy\n\
         {strategy_instructions}\n\n\
         {context}\
         ## Input\n\
         {input}\n\n\
         ## Output Requirements\n\
         Return ONLY a YAML array of chunks. Each chunk must have:\n\
         - id: sequential number (1, 2, 3...)\n\
         - content: the actual text content from input\n\
         - label: optional descriptive label (e.g., section name)\n\n\
         Format:\n\
         ```yaml\n\
         - id: 1\n\
           content: \"First chunk content here...\"\n\
           label: \"Introduction\"\n\
         - id: 2\n\
           content: \"Second chunk content here...\"\n\
           label: \"Requirements\"\n\
         ```\n\n\
         Return ONLY the YAML array. No explanation, no preamble.",
        strategy_instructions = strategy_instructions,
        context = context_section,
        input = input_str
    ))
}

/// Build prompt for merge primitive (contract-enforced replacement for synthesize).
///
/// Constructs a detailed prompt for merging 2-10 inputs with specified strategy.
/// Enforces coherent output, no hallucination, and strategy-specific structure.
pub fn build_merge_prompt(
    inputs: &[serde_json::Value],
    strategy: &crate::scroll::schema::MergeStrategy,
    output_contract: Option<&crate::scroll::schema::MergeOutputContract>,
    context: Option<&serde_json::Value>,
) -> Result<String, ExecutionError> {
    use crate::scroll::schema::MergeStrategy;

    // Serialize each input with index
    let mut inputs_str = Vec::new();
    for (i, v) in inputs.iter().enumerate() {
        let v_str = serde_json::to_string(v)
            .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))?;
        inputs_str.push(format!("Input {}:\n{}", i + 1, v_str));
    }

    // Strategy-specific instructions
    let strategy_name = match strategy {
        MergeStrategy::Sequential => "sequential",
        MergeStrategy::Reconcile => "reconcile",
        MergeStrategy::Union => "union",
        MergeStrategy::Intersection => "intersection",
    };

    let strategy_instructions = match strategy {
        MergeStrategy::Sequential => {
            "Combine inputs in order, maintaining the sequence and flow.\n\
             Synthesize them into a coherent unified narrative."
        },
        MergeStrategy::Reconcile => {
            "Identify and resolve conflicts between inputs.\n\
             Output structure MUST include:\n\
             - content: The reconciled unified output\n\
             - conflicts: Array of conflicts found (if any)\n\
               Each conflict has: topic, inputs (1-indexed array), resolution"
        },
        MergeStrategy::Union => {
            "Include all unique points from all inputs.\n\
             List alternatives where inputs differ."
        },
        MergeStrategy::Intersection => {
            "Include only points that appear across ALL inputs.\n\
             Exclude contradictory or input-specific points."
        },
    };

    // Context section
    let context_section = if let Some(ctx) = context {
        let ctx_str = serde_json::to_string(ctx)
            .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))?;
        format!("Context:\n{}\n\n", ctx_str)
    } else {
        String::new()
    };

    // Format requirements
    let format_instructions = if let Some(contract) = output_contract {
        match contract.format {
            crate::scroll::schema::OutputFormat::Prose => {
                "Output format: Prose (natural narrative text without structural markers)"
            },
            crate::scroll::schema::OutputFormat::Structured => {
                "Output format: Structured (YAML mapping with clear sections)"
            },
            crate::scroll::schema::OutputFormat::List => {
                "Output format: List (bullet points or numbered items)"
            },
        }
    } else {
        "Output format: Prose (default)"
    };

    // Coherence requirements
    let coherence_requirements = "\
CRITICAL REQUIREMENTS:
1. All inputs must be represented in the output
2. Produce a coherent unified whole, NOT a concatenation
3. Do NOT reference inputs by number (no 'Input 1 says...', 'According to input 2...', etc.)
4. Do NOT add information not present in the inputs (no hallucination)
5. Maintain a unified voice and perspective";

    Ok(format!(
        "Merge the following {} inputs using the '{}' strategy.\n\n\
         {}\n\n\
         {}\n\
         Strategy: {}\n\
         {}\n\n\
         {}\n\n\
         {}\n\n\
         Output ONLY valid YAML inside <yaml> tags. No preamble, no explanation.\n\n\
         <yaml>\n\
         your merged content here\n\
         </yaml>",
        inputs.len(),
        strategy_name,
        inputs_str.join("\n\n"),
        context_section,
        strategy_name,
        strategy_instructions,
        format_instructions,
        coherence_requirements
    ))
}

/// Build prompt for validate primitive.
pub fn build_validate_prompt(
    input: &serde_json::Value,
    reference: Option<&serde_json::Value>,
    criteria: &serde_json::Value,
    mode: &crate::scroll::schema::ValidationMode,
) -> Result<String, ExecutionError> {
    use crate::scroll::schema::ValidationMode;

    let input_str = serde_json::to_string(input)
        .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))?;
    let criteria_str = serde_json::to_string(criteria)
        .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))?;
    let reference_section = if let Some(r) = reference {
        let r_str = serde_json::to_string(r)
            .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))?;
        format!("Reference:\n{}\n\n", r_str)
    } else {
        String::new()
    };

    let mode_description = match mode {
        ValidationMode::Strict => "All criteria must pass (100%)",
        ValidationMode::Majority => "More than 50% of criteria must pass (>50%, exactly 50% = FAIL)",
        ValidationMode::Any => "At least one criterion must pass (>0%)",
    };

    Ok(format!(
        "Validate the following input against the given criteria.\n\n\
         Input:\n{input_str}\n\n\
         {reference_section}\
         Criteria:\n{criteria_str}\n\n\
         Validation Mode: {mode_description}\n\n\
         IMPORTANT: Criterion Interpretation (D43):\n\
         - Bare statements default to 100%: \"functions have X\" = ALL functions\n\
         - \"Most\" = >50%, \"Some\" = >0%\n\
         - \"No X\" = zero occurrences\n\
         - When ambiguous, choose strictest interpretation\n\n\
         You MUST output ONLY valid YAML inside <yaml> tags with this exact structure:\n\n\
         <yaml>\n\
         result: pass  # or fail (based on mode and score)\n\
         score: 0.75  # passed_count / total_count (0.0-1.0, 2 decimal places)\n\
         criteria_results:\n\
           - criterion: \"criterion text\"\n\
             passed: true  # or false\n\
             explanation: \"detailed explanation\"\n\
           - criterion: \"next criterion\"\n\
             passed: false\n\
             explanation: \"why it failed\"\n\
         summary: \"Overall validation summary\"\n\
         </yaml>\n\n\
         Calculate score as: (number of criteria that passed) / (total number of criteria)\n\
         Determine result based on mode:\n\
         - strict: pass if score == 1.0 (all criteria passed)\n\
         - majority: pass if score > 0.5 (more than half, exactly 50% = FAIL)\n\
         - any: pass if score > 0.0 (at least one passed)",
        input_str = input_str,
        reference_section = reference_section,
        criteria_str = criteria_str,
        mode_description = mode_description
    ))
}

// ============================================================================
// Format Detection (D44)
// ============================================================================

/// Detect the format of input string.
/// Tries to parse in order: json, yaml, xml, markdown, prose.
/// Returns detected format and confidence level.
pub fn detect_format(input: &str) -> (String, String) {
    let trimmed = input.trim();

    // Try JSON first (most specific)
    if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
        return ("json".to_string(), "high".to_string());
    }

    // Try YAML (JSON is valid YAML, but we already ruled that out)
    if serde_yaml::from_str::<serde_yaml::Value>(trimmed).is_ok() {
        // Check if it looks like structured YAML (not just a string)
        if is_likely_yaml_structure(trimmed) {
            return ("yaml".to_string(), "high".to_string());
        }
    }

    // Check for XML
    if trimmed.starts_with("<?xml") || (trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.contains("</")) {
        return ("xml".to_string(), "medium".to_string());
    }

    // Check for Markdown
    if has_markdown_markers(trimmed) {
        return ("markdown".to_string(), "medium".to_string());
    }

    // Check for CSV
    if has_csv_structure(trimmed) {
        return ("csv".to_string(), "medium".to_string());
    }

    // Default to prose
    ("prose".to_string(), "high".to_string())
}

/// Check if text has markdown markers.
fn has_markdown_markers(text: &str) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return false;
    }

    // Check for headers
    let has_headers = lines.iter().any(|line| line.trim().starts_with('#'));

    // Check for lists
    let has_lists = lines.iter().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ")
    });

    // Check for code blocks
    let has_code_blocks = text.contains("```");

    // Check for links
    let has_links = text.contains("](") && text.contains("[");

    // Need at least 2 markdown features
    let feature_count = [has_headers, has_lists, has_code_blocks, has_links]
        .iter()
        .filter(|&&x| x)
        .count();

    feature_count >= 2
}

/// Check if text has CSV structure.
fn has_csv_structure(text: &str) -> bool {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() < 2 {
        return false;
    }

    // Check if lines have consistent comma count
    let first_comma_count = lines[0].matches(',').count();
    if first_comma_count == 0 {
        return false;
    }

    // At least 80% of lines should have same comma count
    let matching_lines = lines.iter()
        .filter(|line| line.matches(',').count() == first_comma_count)
        .count();

    matching_lines as f64 / lines.len() as f64 >= 0.8
}

/// Parse and validate JSON string.
pub fn parse_and_validate_json(text: &str) -> Result<serde_json::Value, ExecutionError> {
    // Extract structured content first
    let extracted = extract_structured_content(text, "json");

    // Parse as JSON directly — no conversion needed
    serde_json::from_str(&extracted)
        .map_err(|e| ExecutionError::ParseError(
            format!("CONVERT_PARSE_FAILED: Output could not be parsed as JSON: {}", e)
        ))
}

/// Parse and validate YAML string.
/// Tries JSON parsing first (JSON is valid YAML, but serde_yaml rejects
/// duplicate keys that serde_json allows). Falls back to YAML if JSON fails.
pub fn parse_and_validate_yaml(text: &str) -> Result<serde_json::Value, ExecutionError> {
    // Extract structured content first
    let extracted = extract_structured_content(text, "yaml");

    // Try JSON first — LLM output is almost always JSON, and serde_json is
    // more lenient (accepts duplicate keys, no type coercion surprises).
    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&extracted) {
        return Ok(json_val);
    }

    // Fall back to YAML parser for non-JSON content, convert to serde_json::Value
    let yaml_val = serde_yaml::from_str::<serde_yaml::Value>(&extracted)
        .map_err(|e| ExecutionError::ParseError(
            format!("CONVERT_PARSE_FAILED: Output could not be parsed as YAML: {}", e)
        ))?;

    serde_json::to_value(&yaml_val)
        .map_err(|e| ExecutionError::ParseError(
            format!("Failed to convert YAML to JSON: {}", e)
        ))
}

// ============================================================================
// Build prompt for convert primitive (Contract #47)
// ============================================================================

/// Build prompt for convert primitive.
pub fn build_convert_prompt(
    input: &str,
    from: Option<&str>,
    to: &crate::scroll::schema::ConvertTarget,
    context: Option<&serde_json::Value>,
) -> Result<String, ExecutionError> {
    use crate::scroll::schema::ConvertTarget;

    // Detect source format if not provided
    let (detected_format, confidence) = if let Some(f) = from {
        (f.to_string(), "explicit".to_string())
    } else {
        detect_format(input)
    };

    let from_str = from.unwrap_or(&detected_format);

    // Build target format description and schema section
    let (target_format, schema_section) = match to {
        ConvertTarget::Simple(format) => {
            (format.as_str(), String::new())
        }
        ConvertTarget::Detailed { format, schema } => {
            let schema_str = serde_json::to_string(schema)
                .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))?;
            let section = format!("\n## Target Schema\n{}\n", schema_str);
            (format.as_str(), section)
        }
    };

    // Build context section
    let context_section = if let Some(ctx) = context {
        let ctx_str = serde_json::to_string(ctx)
            .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))?;
        format!("\n## Additional Context\n{}\n", ctx_str)
    } else {
        String::new()
    };

    // Build detection metadata section
    let detection_section = if from.is_none() {
        format!("\n## Format Detection\n\
                 Detected source format: {} (confidence: {})\n",
                detected_format, confidence)
    } else {
        String::new()
    };

    Ok(format!(
        "You are performing the CONVERT operation.\n\n\
         ## Rules (invariants - non-negotiable)\n\
         1. SCHEMA COMPLIANT: Output must match the target format exactly.\n\
         2. CONTENT PRESERVED: All input information must be in the output.\n\
         3. NO HALLUCINATION: Do not add information not present in the input.\n\
         4. REVERSIBLE INTENT: Conversion should theoretically be reversible.\n\n\
         ## Operation Parameters\n\
         - Source Format: {from}\n\
         - Target Format: {target}\n\
         {schema_section}\
         {detection_section}\
         {context_section}\
         ## Input\n\
         {input}\n\n\
         ## Output Requirements\n\
         Return ONLY valid {target} content. No preamble, no explanation.\n\
         If target is JSON or YAML, output the raw structure.\n\
         If target is markdown, prose, csv, or xml, output the formatted text.",
        from = from_str,
        target = target_format,
        schema_section = schema_section,
        detection_section = detection_section,
        context_section = context_section,
        input = input
    ))
}
