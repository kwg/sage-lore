# Primitive Reference

> **Note**: This document shows YAML syntax from v0.x. As of v1.0-beta, scrolls use
> **Scroll Assembly** syntax — a strongly-typed language with explicit types, blocks-as-expressions,
> and composable error handling. See `src/scroll/assembly/scroll_assembly.pest` for the formal grammar
> and `tests/scroll_corpus/` for examples. A full rewrite of this doc to Assembly syntax is tracked in #179.

sage-lore provides 20 orthogonal primitives for LLM orchestration. Each primitive has a single responsibility and composes cleanly with others.

## Design Principles

1. **Orthogonality**: Each primitive does one thing well
2. **Composability**: Primitives combine to build complex workflows
3. **Determinism**: Engine handles all state; LLMs only generate content
4. **Security-First**: All external operations are policy-enforced
5. **Domain-Agnostic**: Primitives never encode domain knowledge — that belongs in scrolls

## Scroll Structure

```yaml
scroll: name
description: "What this scroll does"

requires:                    # Optional — input variables
  var_name:
    type: string
    description: "What this variable is"
    default: "fallback value"

steps:
  - primitive_name:
      param: value
      param: ${var_ref}      # Variable interpolation
    output: result_name      # Binds result for later steps
    on_fail: halt            # Error handling (halt|continue|retry|fallback)
```

---

## Core Primitives (6)

Transform and manipulate information within the scroll context.

### elaborate

Expands high-level descriptions into detailed content. Includes deterministic output validation (length, format) with retry on failure.

```yaml
- elaborate:
    input: ${brief_spec}
    depth: thorough           # thorough | balanced | concise
    output_contract:
      length: paragraph       # sentence | paragraph | page
      format: prose           # prose | structured | list
    context:                  # Optional — arbitrary JSON passed to prompt
      role: "system architect"
      task: "expand into detailed design"
  output: detailed_design
```

**Parameters**:
- `input` (required): Text or variable to elaborate
- `depth`: How much detail to add — `thorough`, `balanced` (default), `concise`
- `output_contract`: Constrain output length and format
- `context`: Additional JSON context included in the prompt
- `backend`, `model_tier`, `model`, `format_schema`: See [LLM Backends](#llm-backends)

**Use cases**: Requirements → design docs, outlines → full text, brief → detailed

---

### distill

Reduces verbose content to essential information. Inverse of elaborate.

```yaml
- distill:
    input: ${verbose_logs}
    intensity: aggressive     # aggressive | balanced | minimal
    output_contract:
      length: sentence        # keywords | phrase | sentence | paragraph
      format: prose           # prose | bullets | keywords
    context:
      preserve: "error codes and timestamps"
  output: summary
```

**Parameters**:
- `input` (required): Text to reduce
- `intensity`: How aggressively to compress — `aggressive`, `balanced` (default), `minimal`
- `output_contract`: Constrain output length and format
- `context`: Additional JSON context
- `backend`, `model_tier`, `model`, `format_schema`: See [LLM Backends](#llm-backends)

**Use cases**: Log analysis, documentation summarization, executive summaries

---

### split

Breaks content into constituent parts.

```yaml
- split:
    input: ${epic_description}
    by: semantic               # semantic | structure | count
    granularity: coarse        # coarse | medium | fine
  output: parts

# Count-based split
- split:
    input: ${document}
    by: count
    count: 5
  output: chunks

# Structure-based split
- split:
    input: ${markdown_doc}
    by: structure
    markers: headers           # headers | paragraphs | sentences | bullets
  output: sections
```

**Parameters**:
- `input` (required): Content to split
- `by`: Split strategy — `semantic` (default), `structure`, `count`
- `granularity`: For semantic splits — `coarse`, `medium` (default), `fine`
- `count`: For count splits — exact number of parts
- `markers`: For structure splits — `headers`, `paragraphs`, `sentences`, `bullets`
- `context`: Additional JSON context
- `backend`, `model_tier`, `model`, `format_schema`: See [LLM Backends](#llm-backends). Auto-gen schema enforces `[{id, content, label?}]`.

**Use cases**: Epic → stories, documents → sections, batch processing prep

---

### merge

Combines multiple inputs into a unified result.

```yaml
- merge:
    inputs:
      - ${frontend_design}
      - ${backend_api}
      - ${database_schema}
    strategy: sequential       # sequential | reconcile | union | intersection
    output_contract:
      format: structured       # prose | structured | list
  output: system_design
```

**Parameters**:
- `inputs` (required): Array of variable references (min 2, max 10)
- `strategy`: How to combine — `sequential` (default, in order), `reconcile` (resolve conflicts), `union` (all unique points), `intersection` (only common points)
- `output_contract`: Constrain output format
- `context`: Additional JSON context
- `backend`, `model_tier`, `model`, `format_schema`: See [LLM Backends](#llm-backends)

**Use cases**: Multi-source integration, assembling artifacts from parts

---

### validate

Verifies content against criteria. Returns structured pass/fail with per-criterion results.

```yaml
- validate:
    input: ${generated_code}
    criteria:
      - "Function has a return type"
      - "Function takes two parameters"
      - "Error handling is present"
    mode: strict               # strict | majority | any
    reference: ${specification} # Optional — reference to validate against
  output: validation_result
```

**Parameters**:
- `input` (required): Content to validate
- `criteria` (required): List of criteria strings or JSON array
- `mode`: How many criteria must pass — `strict` (all, default), `majority` (>50%), `any` (at least one)
- `reference`: Optional reference content to validate against
- `backend`, `model_tier`, `model`, `format_schema`: See [LLM Backends](#llm-backends). Auto-gen schema enforces `{result, score, criteria_results, summary}`.

**Output structure**:
```json
{
  "result": "pass",
  "score": 1.0,
  "criteria_results": [
    {"criterion": "...", "passed": true, "explanation": "..."}
  ],
  "summary": "All criteria passed"
}
```

**Return behavior**: Validate always returns the full result object (with `result`, `score`, `criteria_results`, `summary`), whether pass or fail. Use `${validation_result.score}` or `${validation_result.result}` in branch conditions to gate on outcomes.

**Use cases**: Quality gates, requirement verification, content review

---

### convert

Transforms content between formats, optionally constrained by a JSON schema. Includes fast paths for json→json and yaml→yaml (no LLM call when input parses locally). Retries up to 3 times on validation failures.

```yaml
# Simple format conversion
- convert:
    input: ${data}
    from: yaml
    to: json
  output: json_data

# Schema-constrained extraction
- convert:
    input: ${large_yaml}
    from: yaml
    to:
      format: json
      schema:
        type: object
        properties:
          name:
            type: string
          count:
            type: integer
        required:
          - name
    context:
      instructions: "Extract only the relevant fields"
  output: extracted_data
```

**Parameters**:
- `input` (required): Content to convert
- `to` (required): Target — simple string (`"json"`, `"yaml"`, `"markdown"`, `"prose"`, `"csv"`, `"xml"`) or object with `format` + `schema`
- `from`: Source format hint (auto-detected if omitted)
- `coercion`: Type coercion mode — `auto` (default, coerces obvious mismatches), `strict` (fail on mismatch)
- `context`: Additional JSON context included in the prompt
- `backend`, `model_tier`, `model`, `format_schema`: See [LLM Backends](#llm-backends)

**Fast paths**: When `to: json` and input starts with `{`/`[`, the engine parses locally (no LLM call). Same for `from: yaml` + `to: yaml`. Schema validation still applies. Falls through to LLM if local parse fails.

**Use cases**: Format conversion, structured extraction from documents, schema migration

---

## System Primitives (5)

Interact with the external environment.

### fs

Filesystem operations. All operations are policy-enforced (allowlist/denylist, path traversal prevention, file type restrictions).

```yaml
# Read a file
- fs:
    operation: read
    path: ./config.yaml
  output: config_content

# Write a file
- fs:
    operation: write
    path: ./output.json
    content: ${result}
  output: write_result

# Check existence
- fs:
    operation: exists
    path: ./config.yaml
  output: file_exists

# List directory
- fs:
    operation: list
    path: ./src
  output: source_files

# Create directory
- fs:
    operation: mkdir
    path: ./output
  output: mkdir_result

# Delete a file
- fs:
    operation: delete
    path: ./temp.txt
  output: delete_result
```

**Operations**: `read`, `write`, `exists`, `list`, `mkdir`, `delete`, `copy`, `move`

---

### vcs

Version control operations.

```yaml
- vcs:
    operation: status
  output: git_status

- vcs:
    operation: commit
    message: "feat: add new feature"
    files:
      - src/new_feature.rs

- vcs:
    operation: diff
  output: changes

- vcs:
    operation: log
    count: 5
  output: recent_commits
```

**Operations**: `status`, `commit`, `diff`, `log`, `branch`, `checkout`, `add`, `merge`, `tag`, `stash`, `remote`, `reset`, `refs`

---

### test

Test execution with auto-detection of test frameworks.

```yaml
- test:
    operation: run
    pattern: "tests/integration/*"
  output: test_results

- test:
    operation: coverage
    config:
      min_coverage: 80
  output: coverage_report
```

**Operations**: `run`, `coverage`

**Auto-detected backends**: cargo, npm, jest, pytest, go test, make, bats, vitest

---

### platform

Platform information and environment queries.

```yaml
- platform:
    operation: info
  output: system_info

- platform:
    operation: env
    var: API_KEY
  output: api_key

- platform:
    operation: check
    command: docker
  output: has_docker
```

**Operations**: `info`, `env`, `check`

---

### run

Execute another scroll (scroll composition).

```yaml
- run:
    scroll_path: scrolls/validate.scroll
  output: subscroll_result
```

**Parameters**:
- `scroll_path` (required): Path to the scroll to execute

**Use cases**: Modular workflows, reusable scroll libraries

---

## Agent Operations (4)

Invoke and coordinate LLM agents.

### invoke

Execute a single agent with a specific prompt.

```yaml
- invoke:
    agent: analyst
    prompt: "Review ${diff} for security issues"
    context:
      - ${coding_standards}
      - ${security_policy}
  output: review_comments
```

**Parameters**:
- `agent` (required): Agent identifier
- `prompt` (required): The prompt to send
- `context`: Optional list of additional context values
- `backend`, `model_tier`, `model`, `format_schema`: See [LLM Backends](#llm-backends)

**Multi-backend**: invoke uses the configured LLM backend — `ClaudeCliBackend` (production, shells out to `claude -p`), `OllamaBackend` (local LLMs via Ollama API), or `MockLlmBackend` (testing with canned responses).

---

### parallel

Fan out the same prompt to multiple agents for diverse perspectives.

```yaml
- parallel:
    agents:
      - claude_sonnet
      - claude_opus
      - local_llama
    prompt: "Analyze ${incident_report}"
    max_concurrent: 3
    on_fail: require_quorum
    quorum: 2
  output: analysis_results
```

**Parameters**:
- `agents` (required): List of agent identifiers
- `prompt` (required): Prompt sent to all agents
- `max_concurrent`: Maximum concurrent invocations
- `on_fail`: Error strategy — `require_quorum` allows partial failure
- `quorum`: Minimum successful responses required

**Failure modes**: `require_all` (default), `require_quorum`, `best_effort`

---

### consensus

Achieve agreement among agents on a decision via voting.

```yaml
- consensus:
    agents:
      - security_expert
      - performance_expert
      - maintainability_expert
    proposal: "Should we refactor ${module}?"
    mechanism: weighted_vote
    options:
      - refactor_now
      - refactor_later
      - keep_as_is
    threshold: majority        # majority | supermajority | unanimous | N (numeric)
  output: decision
```

**Parameters**:
- `agents` (required): Voting agents
- `proposal` (required): What to vote on
- `mechanism` (required): Voting mechanism (e.g., `weighted_vote`)
- `options` (required): Available choices
- `threshold`: Agreement threshold — `majority` (default), `supermajority`, `unanimous`, or numeric (e.g., `2`)

**No built-in consensus**: Core primitives do NOT include automatic consensus validation. If you want consensus on a step's output, add a `consensus` step explicitly. This keeps primitives fast and gives scroll designers control over when consensus is worth the cost.

---

### concurrent

Run different operations simultaneously (operation parallelism, not agent parallelism).

```yaml
- concurrent:
    operations:
      - test:
          operation: run
      - secure:
          scan_type: secret_detection
      - vcs:
          operation: status
    timeout: 300
  output: parallel_results
```

**Parameters**:
- `operations` (required): List of steps to run in parallel
- `timeout`: Optional timeout in seconds

**Use cases**: Independent operations (test + lint + scan), performance optimization

---

## Flow Control (3)

Control execution flow within scrolls.

### branch

Conditional execution based on runtime values.

```yaml
- branch:
    condition: "${has_cargo}"
    if_true:
      - fs:
          operation: read
          path: Cargo.toml
        output: cargo_contents
    if_false:
      - fs:
          operation: write
          path: .test-output/no-cargo.txt
          content: "No Cargo.toml found"
  output: branch_result
```

**Parameters**:
- `condition` (required): Expression evaluated at runtime (supports `${var}`, comparisons)
- `if_true` (required): Steps when condition is truthy
- `if_false`: Steps when condition is falsy (optional)

**Truthiness**: `true`, non-empty string, non-zero number, non-empty array/object are truthy. `false`, `null`, `""`, `0`, `[]`, `{}` are falsy.

---

### loop

Iterate over collections.

```yaml
- loop:
    items: "${file_list}"
    item_var: file
    max: 50
    operation:
      - fs:
          operation: exists
          path: "${file}"
        output: file_exists
  output: loop_results
```

**Parameters**:
- `items` (required): Variable containing list to iterate
- `item_var`: Name of current item variable (default: `item`)
- `operation` (required): Steps to execute per iteration
- `max`: Safety limit on iterations
- `while`: Optional condition to continue (for while-loop behavior)

---

### aggregate

Combine multiple results into a summary.

```yaml
- aggregate:
    results:
      - ${review_1}
      - ${review_2}
      - ${review_3}
    strategy: consensus_summary
  output: final_review
```

**Parameters**:
- `results` (required): List of variables to aggregate
- `strategy` (required): Aggregation strategy string

---

## Data Wiring (1)

### set

Bind arbitrary values into the scroll context for use in later steps.

```yaml
- set:
    values:
      project_name: "sage-polaris"
      max_retries: 3
      config:
        timeout: 30
        verbose: true
  output: settings
```

**Parameters**:
- `values` (required): Arbitrary JSON/YAML values to bind

**Use cases**: Constants, computed defaults, configuration injection

---

## Security (1)

### secure

Run security scans on content or files.

```yaml
- secure:
    scan_type: secret_detection   # secret_detection | dependency_cve | static_analysis
    policy: block                 # block | warn | audit
    input: ${generated_code}      # Optional — content to scan
  output: scan_report
```

**Parameters**:
- `scan_type` (required): `secret_detection`, `dependency_cve`, `static_analysis`
- `policy`: Action on findings — `block` (halt), `warn` (log and continue), `audit` (record only)
- `input`: Optional content to scan

---

## Error Handling

All primitives support `on_fail`:

```yaml
# Halt on failure (default)
- elaborate:
    input: ${data}
  on_fail: halt

# Continue past failure
- validate:
    input: ${data}
    criteria: [...]
  on_fail: continue

# Retry with limit
- convert:
    input: ${data}
    to: json
  on_fail:
    retry:
      max: 3

# Fallback to alternative steps
- invoke:
    agent: primary
    prompt: "..."
  on_fail:
    fallback:
      - invoke:
          agent: backup
          prompt: "..."
```

---

## Variable Interpolation

Steps reference previous outputs and input variables with `${name}`:

```yaml
requires:
  input_path:
    type: string
    default: "data.yaml"

steps:
  - fs:
      operation: read
      path: ${input_path}          # References input variable
    output: raw_data

  - convert:
      input: ${raw_data}           # References previous step output
      from: yaml
      to: json
    output: parsed

  - elaborate:
      input: ${parsed}             # Chain outputs through steps
      depth: thorough
    output: result
```

**Embedded interpolation**: `${var}` references work inside strings, including `input`, `context` sub-fields, `path`, `prompt`, and all platform operation string parameters (`body`, `title`, `description`, `labels`, etc.):

```yaml
- elaborate:
    input: ${concept_data}
    context:
      task: "Generate a problem for topic ${topic_id}, concept ${concept_id}"
    output: problem

- fs:
    operation: write
    path: "output/problem-${attempt}.json"
    content: ${problem}
```

**Path access**: Use dot notation to access nested fields: `${validation_result.score}`, `${problem.solution.code}`.

---

## LLM Backends

The `invoke` primitive (and core primitives that call the LLM internally) supports multiple backends:

| Backend | Use | Configuration |
|---------|-----|---------------|
| `ClaudeCliBackend` | Production — shells out to `claude -p` | Default |
| `OllamaBackend` | Local LLMs via Ollama API | Configure endpoint |
| `MockLlmBackend` | Testing with canned responses | Provide response map |

All core primitives (elaborate, distill, split, merge, convert, validate) and invoke support per-step backend override:

```yaml
# Use cheap model for routine work
- elaborate:
    input: ${data}
    depth: balanced
    backend: ollama
  output: draft

# Use expensive model for critical work
- elaborate:
    input: ${draft}
    depth: thorough
    backend: claude
  output: refined
```

If `backend` is not set, the step uses the default (SAGE_LLM_BACKEND env var or claude).

### Model Selection

All LLM-invoking primitives support per-step model selection:

```yaml
# Use cheap model for validation
- validate:
    input: ${problem}
    criteria: [...]
    model_tier: cheap          # cheap | standard | premium
  output: result

# Pin to specific model for benchmarking
- elaborate:
    input: ${topic}
    depth: detailed
    model: gpt-oss:20b         # Explicit model name (escape hatch)
  output: expanded

# Parameterized model via variable
- elaborate:
    input: ${topic}
    model: "${target_model}"   # Supports ${var} interpolation
  output: result
```

**Priority chain**: `model` > `model_tier` > env var default.

- `model_tier`: Routes to the tier's configured model (`cheap` → phi4-mini, `standard` → qwen3-coder:30b, `premium` → deepseek-r1:32b for Ollama). Portable across deployments.
- `model`: Explicit model name. Not portable — use for benchmarking, A/B testing, or pinning a known-good model to a critical step.

### Structured Output (format_schema)

All LLM-invoking primitives support JSON schema enforcement for structured output:

```yaml
# Enforce specific JSON structure on elaborate output
- elaborate:
    input: ${topic}
    depth: detailed
    format_schema:
      type: object
      properties:
        statement: { type: string }
        solution: { type: string }
        hints: { type: array, items: { type: string } }
      required: [statement, solution]
  output: problem

# Override auto-gen schema on validate
- validate:
    input: ${code}
    criteria: [...]
    format_schema:
      type: object
      properties:
        result: { type: string, enum: [pass, fail] }
        custom_metric: { type: number }
      required: [result, custom_metric]
  output: result
```

**Schema priority**: scroll `format_schema` > auto-generated > none.

**Auto-generated schemas** (applied when no scroll-level `format_schema` is set):
- **validate**: `{result: "pass"|"fail", score: number, criteria_results: [...], summary: string}` with `additionalProperties: true`
- **split**: `[{id: string, content: string, label?: string}]` with `additionalProperties: true`

**No auto-gen** (schema only applied if scroll specifies `format_schema`):
- elaborate, distill, merge, convert, invoke

**Backend support**:
- **Ollama**: Full enforcement via grammar-based token masking — model physically cannot emit tokens outside schema
- **Claude CLI**: Schema is ignored (no CLI flag for structured output) — falls back to prompt + post-hoc parsing
- **Future OpenAI-compatible backends**: Will support `response_format` parameter

---

## Implementation

All primitives are implemented in `src/`:

- **Core**: `scroll/step_dispatch.rs` (elaborate, distill, split, merge, validate, convert)
- **Prompts**: `scroll/extraction.rs` (prompt building for each core primitive)
- **System**: `primitives/fs`, `primitives/vcs`, `primitives/test`, `primitives/platform`
- **Agent**: `primitives/invoke` (ClaudeCliBackend, OllamaBackend, MockLlmBackend)
- **Consensus**: `scroll/consensus.rs`
- **Flow**: `scroll/step_dispatch.rs` (branch, loop, aggregate)
- **Data**: `scroll/step_dispatch.rs` (set)
- **Security**: `primitives/secure`
- **Schema**: `scroll/schema.rs` (all type definitions)
