# Primitive Reference

sage-lore provides 20 orthogonal primitives for LLM orchestration. Each primitive has a single responsibility and composes cleanly with others.

All examples use **Scroll Assembly** syntax. See `src/scroll/assembly/scroll_assembly.pest` for the formal grammar.

## Design Principles

1. **Orthogonality**: Each primitive does one thing well
2. **Composability**: Primitives combine to build complex workflows
3. **Determinism**: Engine handles all state; LLMs only generate content
4. **Security-First**: All external operations are policy-enforced
5. **Domain-Agnostic**: Primitives never encode domain knowledge — that belongs in scrolls

## Scroll Structure

```
scroll "name" {
    description: "What this scroll does";

    require var_name: str;                  // Input variables
    require count: int = 5;                 // With defaults
    provide result: map;                    // Output variables

    // Body — ordered statements
    platform.get_issue(number: issue_num) -> raw_issue: map;
    invoke(agent: "dev", instructions: "...") { context: [raw_issue]; } -> impl: map;
    set result: map = { title: raw_issue.title, code: impl.code };
}
```

---

## Core Primitives (6)

Transform and manipulate information within the scroll context.

### elaborate

Expands high-level descriptions into detailed content. Deterministic output validation with retry on failure.

```
elaborate(input: brief_spec, depth: thorough) {
    output_contract: { length: "paragraph", format: "prose" },
    context: { role: "system architect", task: "expand into detailed design" },
} -> detailed_design: map;
```

**Parameters**:
- `input` (required): Text or variable to elaborate
- `depth`: `thorough`, `balanced` (default), `concise`
- `output_contract`: Constrain output length and format
- `context`: Additional JSON context included in the prompt
- `tier`, `model`: See [LLM Backends](#llm-backends)

---

### distill

Reduces verbose content to essential information. Inverse of elaborate.

```
distill(input: verbose_logs, intensity: aggressive) {
    output_contract: { length: "sentence", format: "prose" },
    context: { preserve: "error codes and timestamps" },
} -> summary: str;
```

**Parameters**:
- `input` (required): Text to reduce
- `intensity`: `aggressive`, `balanced` (default), `minimal`
- `output_contract`: Constrain output length and format
- `context`: Additional JSON context
- `tier`, `model`: See [LLM Backends](#llm-backends)

---

### split

Breaks content into constituent parts.

```
// Semantic split
split(input: epic_description, by: "semantic", granularity: "coarse") -> parts: map[];

// Count-based split
split(input: document, by: "count", count: 5) -> chunks: map[];

// Structure-based split
split(input: markdown_doc, by: "structure", markers: "headers") -> sections: map[];
```

**Parameters**:
- `input` (required): Content to split
- `by`: `semantic` (default), `structure`, `count`
- `granularity`: For semantic — `coarse`, `medium` (default), `fine`
- `count`: For count splits — exact number of parts
- `markers`: For structure — `headers`, `paragraphs`, `sentences`, `bullets`

---

### merge

Combines multiple inputs into a unified result.

```
merge(inputs: [frontend_design, backend_api, database_schema], strategy: "reconcile") {
    output_contract: { format: "structured" },
} -> system_design: map;
```

**Parameters**:
- `inputs` (required): Array of variables (min 2, max 10)
- `strategy`: `sequential` (default), `reconcile`, `union`, `intersection`
- `output_contract`: Constrain output format

---

### validate

Verifies content against criteria. Returns structured pass/fail.

```
validate(input: generated_code, reference: specification) {
    criteria: [
        "Function has a return type",
        "Error handling is present",
    ],
    mode: strict,
} -> validation: map;

// Use result in branch
if validation.result == "pass" {
    // proceed
};
```

**Parameters**:
- `input` (required): Content to validate
- `criteria` (required): List of criteria strings
- `mode`: `strict` (all, default), `majority`, `any`
- `reference`: Optional reference to validate against

**Output**: `{ result: "pass"|"fail", score: float, criteria_results: [...], summary: str }`

---

### convert

Transforms content between formats, optionally constrained by a JSON schema.

```
// Simple conversion
convert(input: data, to: "json") -> json_data: map;

// Schema-constrained extraction
convert(input: raw_text, to: "json", schema: {
    type: "object",
    properties: { name: { type: "string" }, count: { type: "integer" } },
    required: ["name"],
}) -> extracted: map;
```

**Parameters**:
- `input` (required): Content to convert
- `to` (required): Target format — `"json"`, `"yaml"`, `"markdown"`, `"prose"`, `"csv"`, `"xml"`
- `from`: Source format hint (auto-detected if omitted)
- `schema`: JSON Schema to validate output against

**Fast paths**: When `to: "json"` and input starts with `{`/`[`, the engine parses locally (no LLM call). Same for `from: "yaml"` + `to: "yaml"`. Falls through to LLM if local parse fails.

---

## System Primitives (5)

Interact with the external environment.

### fs

Filesystem operations. All operations are policy-enforced (allowlist/denylist, path traversal prevention, extension restrictions).

```
fs.read(path: "src/main.rs") -> content: str;
fs.write(path: "output.json", content: result);
fs.exists(path: "config.yaml") -> has_config: bool;
fs.list(path: "src") -> entries: str[];
fs.mkdir(path: "output");
fs.delete(path: "temp.txt");
```

**Operations**: `read`, `write`, `exists`, `list`, `mkdir`, `delete`, `copy`, `move`

---

### vcs

Version control operations.

```
vcs.status() -> status: map;
vcs.commit(message: "feat: add new feature");
vcs.diff() -> changes: str;
vcs.log(count: 5) -> recent: map[];
vcs.branch(name: "feature-branch");
vcs.add(files: ["src/main.rs", "src/lib.rs"]);
```

**Operations**: `status`, `commit`, `diff`, `log`, `branch`, `checkout`, `add`, `merge`, `tag`, `stash`, `remote`, `reset`, `refs`

---

### test

Test execution with auto-detection of test frameworks.

```
test.run() -> results: map;
test.run(filter: "test_parser") -> filtered: map;
```

**Auto-detected backends**: cargo, npm, jest, pytest, go test, make, bats, vitest

---

### platform

Git forge operations (Forgejo, with adapter pattern for GitHub/GitLab).

```
platform.get_issue(number: 42) -> issue: map;
platform.create_issue(title: "Bug fix", body: spec, labels: ["type:bug"]) -> new_issue: map;
platform.list_issues(state: "open", labels: ["type:story"]) -> stories: map[];
platform.close_issue(number: 42);
platform.env(var: "API_KEY") -> key: str;
platform.info() -> system: map;
```

**Operations**: `get_issue`, `create_issue`, `update_issue`, `close_issue`, `list_issues`, `env`, `info`

---

### run

Execute another scroll (scroll composition). Resolves via three-tier search path: project -> user -> global.

```
// By name (search path resolves it)
run("adapters/story-from-forgejo") {
    story_number: story.number,
    epic_title: epic.title,
} -> story: map;

// Direct path
run("./local-scroll.scroll") -> result: map;
```

**Parameters**:
- First argument: scroll name or path
- Block: variables to pass as inputs to the subscroll

**Resolution**: Bare names search `.sage-lore/scrolls/` -> `~/.config/sage-lore/scrolls/` -> `$SAGE_LORE_DATADIR/scrolls/`. Paths starting with `./` or `/` resolve directly.

---

## Agent Operations (4)

Invoke and coordinate LLM agents.

### invoke

Execute a single agent call. The agent runs via `claude -p` with read-only tools and returns structured output.

```
invoke(agent: "dev", instructions: "Implement chunk {chunk.number}") {
    context: [chunk, project_root],
    schema: Implementation,
    tier: premium,
    timeout: 600,
} -> raw_impl: map;
```

**Parameters**:
- `agent` (required): Agent name (resolved from agents/ directory)
- `instructions` (required): Task prompt with `{var}` interpolation
- `context`: Array of variables passed as context
- `schema`: Expected output type
- `tier`: `cheap`, `standard`, `premium` (maps to configured models)
- `model`: Explicit model name (overrides tier)
- `timeout`: Seconds before timeout

**Backends**: `ClaudeCliBackend` (production, `claude -p`), `OllamaBackend` (local LLMs), `MockLlmBackend` (testing)

---

### parallel

Fan out the same prompt to multiple agents for diverse perspectives.

```
parallel(agents: ["analyst-1", "analyst-2", "analyst-3"], prompt: "Analyze {report}") {
    max_concurrent: 3,
    quorum: 2,
} -> analyses: map[];
```

**Parameters**:
- `agents` (required): List of agent identifiers
- `prompt` (required): Prompt sent to all agents
- `max_concurrent`: Maximum concurrent invocations
- `quorum`: Minimum successful responses required

---

### consensus

Achieve agreement among agents via voting.

```
consensus(mechanism: "vote", threshold: majority) {
    agents: ["reviewer-1", "reviewer-2", "reviewer-3"],
    proposal: "Is this implementation correct? {raw_impl}",
    options: ["approve", "reject"],
} -> vote: map;
```

**Parameters**:
- `mechanism` (required): `"vote"`
- `agents` (required): Voting agents
- `proposal` (required): What to vote on
- `options` (required): Available choices
- `threshold`: `majority` (default), `supermajority`, `unanimous`, or numeric

---

### concurrent

Run different operations simultaneously (operation parallelism, not agent parallelism).

```
concurrent {
    test.run() -> test_results: map;
    vcs.status() -> git_status: map;
};
```

---

## Flow Control (3)

### if / else

Conditional execution based on runtime values.

```
if story.chunk_numbers {
    set chunks: int[] = story.chunk_numbers;
} else {
    run("create-chunks") { story_number: story.number; } -> chunks: int[];
};
```

**Truthiness**: `true`, non-empty string, non-zero number, non-empty array/object are truthy. `false`, `null`, `""`, `0`, `[]`, `{}` are falsy.

---

### for

Iterate over collections. Blocks are expressions — `for` returns an array.

```
set results: map[] = for chunk_number in story.chunk_numbers {
    run("implement-chunk") { chunk_number: chunk_number; } -> result: map;
    result
};
```

---

### aggregate

Combine multiple results.

```
aggregate(results: [known_fields, llm_fields], strategy: merge) -> story: map;
```

**Strategies**: `merge` (shallow object merge), `consensus_summary`, custom

---

## Data Wiring (1)

### set

Bind values into the scroll context.

```
set count: int = 5;
set name: str = "sage-polaris";
set config: map = { timeout: 30, verbose: true };
set full: map = base + { extra_field: "value" };  // map merge
set all: str[] = first ++ second;                  // array concat
```

---

## Security (1)

### secure

Run security scans on content or files.

```
secure(scan_type: "secret_detection") {
    policy: "block",
    input: generated_code,
} -> scan: map;
```

**Scan types**: `secret_detection`, `dependency_cve`, `static_analysis`
**Policies**: `block` (halt), `warn` (log and continue), `audit` (record only)

---

## Error Handling

All statements support error handling chains with `|`:

```
// Halt on failure (default)
platform.get_issue(number: n) -> issue: map;

// Continue past failure (null on error)
test.run() -> results: map | continue;

// Retry then halt
invoke(agent: "dev", instructions: "...") {} -> impl: map | retry(3);

// Retry then fallback
invoke(agent: "primary", instructions: "...") {} -> result: map
    | retry(3)
    | fallback {
        invoke(agent: "backup", instructions: "...") {} -> result: map;
    };
```

Chains read left-to-right. Last handler is terminal.

---

## String Interpolation

Strings use `{expr}` for interpolation. Raw strings use backticks.

```
set msg: str = "Issue {issue_number}: {raw_issue.title}";
set raw: str = `No {interpolation} here`;
```

Single-pass resolution. Undefined variables are compile errors.

---

## LLM Backends

| Backend | Use | Configuration |
|---------|-----|---------------|
| `ClaudeCliBackend` | Production — `claude -p` | Default |
| `OllamaBackend` | Local LLMs via Ollama API | `SAGE_LLM_BACKEND=ollama` |
| `MockLlmBackend` | Testing with canned responses | Test-only |

Per-step override via `tier` (portable) or `model` (explicit):

```
// Cheap model for routine work
elaborate(input: data, depth: balanced) { tier: cheap; } -> draft: str;

// Premium for critical work
invoke(agent: "dev", instructions: "...") { tier: premium; } -> impl: map;
```

**Tier mapping** (configurable in config.yaml):
- `cheap`: haiku (Claude) / phi4-mini (Ollama)
- `standard`: sonnet (Claude) / qwen2.5-coder:32b (Ollama)
- `premium`: opus (Claude) / deepseek-r1:32b (Ollama)

---

## Implementation

All primitives are implemented in `src/`:

- **Core**: `scroll/step_dispatch.rs` (elaborate, distill, split, merge, validate, convert)
- **Prompts**: `scroll/extraction.rs` (prompt building for each core primitive)
- **Assembly**: `scroll/assembly/` (parser, type checker, dispatcher)
- **System**: `primitives/fs`, `primitives/vcs`, `primitives/test`, `primitives/platform`
- **Agent**: `primitives/invoke` (ClaudeCliBackend, OllamaBackend)
- **Consensus**: `scroll/consensus.rs`
- **Security**: `primitives/secure`
- **Schema**: `scroll/schema.rs` (all type definitions)
