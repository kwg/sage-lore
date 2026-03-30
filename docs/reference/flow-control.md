# Flow Control Reference

Flow control in Scroll Assembly uses native language constructs — `if`/`else`, `for`, `while`, `match`. Blocks are expressions: they return values.

## if / else

```
if validation.result == "pass" {
    fs.write(path: "output.json", content: generated);
} else {
    set feedback: map = validation;
};

// Blocks are expressions — assign the result
set tier_choice: str = if chunk.complexity == "high" { "premium" } else { "standard" };
```

### Condition Operators

| Operator | Example |
|----------|---------|
| `==` | `result == "pass"` |
| `!=` | `status != "error"` |
| `>` `<` `>=` `<=` | `score >= 0.8` |
| `&&` `\|\|` `!` | `has_cargo && !is_ci` |
| *(bare value)* | `has_cargo` (truthiness) |

### Truthiness

- **Truthy**: `true`, non-empty string, non-zero number, non-empty array/object
- **Falsy**: `false`, `null`, `""`, `0`, `[]`, `{}`

---

## for

Iterate over collections. `for` returns an array (each iteration contributes one element).

```
// Collect results into an array
set results: map[] = for chunk_number in story.chunk_numbers {
    run("implement-chunk") { chunk_number: chunk_number; } -> result: map;
    result   // last expression = return value for this iteration
};

// Concurrent iteration
set results: map[] = concurrent for story in epic.stories {
    run("run-story") { story_number: story.number; } -> result: map;
    result
};
```

**Variables**: The iteration variable is named in the `for` clause. `loop_index` is available automatically.

---

## while

```
set attempts: int = 0;
while attempts < 3 {
    invoke(agent: "dev", instructions: "Fix the issues") {
        context: [code, review.issues],
    } -> code: map;

    validate(input: code, criteria: ["Tests pass"]) { mode: strict; } -> check: map;
    if check.result == "pass" { break; };

    attempts = attempts + 1;
};
```

---

## match

Multi-branch selection. Returns a value.

```
set tier_choice: str = match chunk.complexity {
    "low" => "cheap",
    "medium" => "standard",
    "high" => "premium",
};
```

---

## aggregate

Combine multiple results into a single output.

```
aggregate(results: [known_fields, llm_fields], strategy: merge) -> story: map;
aggregate(results: reviews, strategy: consensus_summary) -> final: map;
```

**Strategies**: `merge` (shallow object merge, right wins), `consensus_summary`

---

## Error Handling Chains

All statements support error handling with `|`:

```
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

## Common Patterns

### Validation Gate

```
validate(input: output, criteria: [...]) { mode: strict; } -> check: map;

if check.result == "pass" {
    vcs.commit(message: "validated");
} else {
    // halt or remediate
};
```

### Process-and-Collect

```
set processed: map[] = for item in work_list {
    elaborate(input: item, depth: thorough) -> expanded: map;
    expanded
};
```

### Conditional LLM Tier

```
set tier: str = match chunk.complexity {
    "low" => "cheap",
    "medium" => "standard",
    "high" => "premium",
};

invoke(agent: "dev", instructions: "Implement") {
    tier: tier,
} -> impl: map;
```

---

## Implementation

- Parser: `src/scroll/assembly/parser.rs` (if, for, while, match as grammar rules)
- Type checker: `src/scroll/assembly/type_checker.rs`
- Dispatcher: `src/scroll/assembly/dispatch.rs` (runtime execution)
- Grammar: `src/scroll/assembly/scroll_assembly.pest`
