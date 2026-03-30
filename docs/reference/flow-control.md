# Flow Control Reference

> **Note**: This document shows YAML syntax from v0.x. As of v1.0-beta, scrolls use
> **Scroll Assembly** syntax with native control flow: `if`/`else`, `for`, `while`, `match`,
> `concurrent`. Blocks are expressions — `set x: int = if cond { a } else { b };`.
> See `tests/scroll_corpus/test_control_flow.scroll` and `test_expressions.scroll` for examples.
> A full rewrite of this doc is tracked in #179.

Flow control primitives manage execution paths within scrolls. They enable conditional logic, iteration, and result aggregation.

## Primitives

### branch

Execute steps conditionally based on runtime values.

#### Syntax

```yaml
- branch:
    condition: <expression>
    if_true:
      - <step>
    if_false:          # Optional
      - <step>
  output: <variable>   # Optional
```

#### Parameters

- `condition`: Expression evaluated at runtime (supports variable interpolation and comparison operators)
- `if_true`: Steps to execute when condition is truthy
- `if_false`: Steps to execute when condition is falsy (optional)

#### Condition Operators

| Operator | Type | Example |
|----------|------|---------|
| `==` | Equality | `"${result} == pass"` |
| `!=` | Inequality | `"${status} != error"` |
| `>=` | Greater or equal (numeric) | `"${score} >= 0.8"` |
| `<=` | Less or equal (numeric) | `"${score} <= 0.5"` |
| `>` | Greater than (numeric) | `"${count} > 0"` |
| `<` | Less than (numeric) | `"${count} < 10"` |
| *(none)* | Truthiness | `"${has_cargo}"` |

Numeric operators (`>=`, `<=`, `>`, `<`) coerce both sides to `f64`. Strings that parse as numbers are coerced automatically (e.g., `"1.0" >= 0.8` works). Returns `false` if either side is not numeric.

#### Truthiness Rules

When no operator is present, the condition value is evaluated for truthiness:

- **Truthy**: `true`, non-empty string, non-zero number, non-empty array/object
- **Falsy**: `false`, `null`, empty string `""`, `0`, empty array `[]`, empty object `{}`

#### Examples

**Score threshold gate**:

```yaml
- validate:
    input: ${generated}
    criteria:
      - "Output is valid"
    mode: strict
  output: validation_result

- branch:
    condition: "${validation_result.score} >= 0.8"
    if_true:
      - fs:
          operation: write
          path: output.json
          content: ${generated}
    if_false:
      - set:
          values: ${validation_result}
        output: feedback
```

**File existence check**:

```yaml
- fs:
    operation: exists
    path: Cargo.toml
  output: has_cargo

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

**Environment-based execution**:

```yaml
- platform:
    operation: env
    var: CI
  output: is_ci

- branch:
    condition: "${is_ci}"
    if_true:
      - test:
          operation: coverage
          config:
            min_coverage: 90
    if_false:
      - test:
          operation: run
```

**Validation gate**:

```yaml
- validate:
    input: ${generated_code}
    criteria:
      - "Code is syntactically valid"
      - "All functions have return types"
    mode: strict
  output: check

- branch:
    condition: "${check}"
    if_true:
      - vcs:
          operation: commit
          message: "validated code"
    if_false:
      - elaborate:
          input: ${check}
          depth: concise
          context:
            task: "Explain what failed validation and suggest fixes"
        output: fix_suggestions
```

---

### loop

Iterate over collections or repeat operations.

#### Syntax

```yaml
- loop:
    items: <variable_or_list>
    item_var: <loop_variable>  # Default: "item"
    operation:
      - <step>
    while: <condition>         # Optional
    max: <number>              # Safety limit
  output: <variable>           # Optional
```

#### Parameters

- `items`: Variable containing a list/array to iterate over
- `item_var`: Name of the variable holding the current item (default: `item`)
- `operation`: Steps to execute for each iteration
- `while`: Optional condition to continue looping
- `max`: Maximum iterations (safety limit — always set this)

#### Examples

**Iterate over files**:

```yaml
- fs:
    operation: list
    path: .test-output/flow-loop
  output: test_files

- loop:
    items: "${test_files}"
    item_var: file
    max: 50
    operation:
      - fs:
          operation: exists
          path: "${file}"
        output: file_exists
  output: loop_results
```

**Process a work list**:

```yaml
- split:
    input: ${epic}
    by: semantic
    granularity: coarse
  output: story_list

- loop:
    items: ${story_list}
    item_var: story
    max: 20
    operation:
      - elaborate:
          input: ${story}
          depth: thorough
          context:
            task: "Generate implementation plan"
        output: plan
      - validate:
          input: ${plan}
          criteria:
            - "Plan has clear acceptance criteria"
          mode: strict
        output: plan_check
```

**Batch file processing**:

```yaml
- fs:
    operation: list
    path: ./pending_reviews
  output: review_files

- loop:
    items: ${review_files}
    item_var: file
    max: 100
    operation:
      - fs:
          operation: read
          path: ${file}
        output: content
      - elaborate:
          input: ${content}
          depth: balanced
          context:
            task: "Review this code for quality issues"
        output: review
      - fs:
          operation: write
          path: "./reviews/${file}.review"
          content: ${review}
```

---

### aggregate

Combine multiple results into a unified output.

#### Syntax

```yaml
- aggregate:
    results:
      - ${variable1}
      - ${variable2}
    strategy: <aggregation_strategy>
  output: <variable>
```

#### Parameters

- `results`: List of variables to aggregate
- `strategy`: How to combine results (e.g., `consensus_summary`, `merge`)

#### Examples

**Multi-agent synthesis**:

```yaml
- parallel:
    agents:
      - reviewer_1
      - reviewer_2
      - reviewer_3
    prompt: "Review ${pull_request}"
  output: reviews

- aggregate:
    results: ${reviews}
    strategy: consensus_summary
  output: final_review
```

**Combining test results**:

```yaml
- concurrent:
    operations:
      - test:
          operation: run
          pattern: "unit/*"
      - test:
          operation: run
          pattern: "integration/*"
  output: all_tests

- aggregate:
    results: ${all_tests}
    strategy: merge
  output: overall_result
```

---

## Best Practices

### 1. Always Set Maximum Iterations

Prevent runaway loops:

```yaml
# Good — bounded
- loop:
    items: ${items}
    operation: [...]
    max: 100

# Dangerous — could loop forever
- loop:
    items: ${items}
    operation: [...]
```

### 2. Use Meaningful Variable Names

```yaml
# Clear
- loop:
    items: ${stories}
    item_var: story

# Ambiguous
- loop:
    items: ${stories}
    item_var: item
```

### 3. Use the Right Condition Style

Use truthiness for simple pass/fail, operators for thresholds:

```yaml
# Truthiness — good for validate pass/fail
- branch:
    condition: "${is_ready}"
    if_true: [...]

# String equality — good for status checks
- branch:
    condition: "${validation_result.result} == pass"
    if_true: [...]

# Numeric threshold — good for scores
- branch:
    condition: "${validation_result.score} >= 0.8"
    if_true: [...]
```

### 4. Handle Both Branches

Always provide `if_false` for important decisions:

```yaml
- branch:
    condition: "${scan.clean}"
    if_true:
      - vcs:
          operation: commit
    if_false:
      - elaborate:
          input: ${scan}
          depth: concise
          context:
            task: "Summarize security findings"
        output: alert
```

---

## Common Patterns

### Validation Gate

```yaml
- validate:
    input: ${output}
    criteria: [...]
    mode: strict
  output: check

- branch:
    condition: "${check}"
    if_true:
      # proceed
    if_false:
      # diagnose and halt
      on_fail: halt
```

### Process-and-Collect

```yaml
- loop:
    items: ${work_list}
    item_var: item
    max: 50
    operation:
      - elaborate:
          input: ${item}
          depth: thorough
        output: result
  output: all_results

- merge:
    inputs: ${all_results}
    strategy: sequential
  output: combined
```

### Parallel Processing with Aggregation

```yaml
- parallel:
    agents: [agent_1, agent_2, agent_3]
    prompt: "Analyze ${data}"
  output: analyses

- aggregate:
    results: ${analyses}
    strategy: consensus_summary
  output: final_analysis
```

---

## Integration with Other Primitives

Flow control composes with everything:

- **Core primitives**: Loop over split results, branch on validate outcomes, aggregate merged outputs
- **System primitives**: Conditional file operations, environment-based VCS actions
- **Agent operations**: Iterative generation, conditional invocation
- **Security**: Scan → branch on findings → remediate or block

## Implementation

Flow control is implemented in:

- Schema: `src/scroll/schema.rs`
- Execution: `src/scroll/step_dispatch.rs`

See [primitives.md](./primitives.md) for the complete primitive reference.
