# sage-lore

**LLM Orchestration Engine** - Deterministic scroll execution for AI workflows.

sage-lore takes deterministic work away from LLMs. The engine manages context and flow; LLMs only do targeted single-task generation.

## Status

**v1.0.0-beta.1** - Core engine complete with 20 primitives, Scroll Assembly language, LSP, security ratchet, three-tier config hierarchy.

## Execution Model

**Primitives do not have a REPL or direct API.** All execution flows through scrolls.

Scrolls are the trust boundary. A scroll is a static file that defines a workflow. Before execution, scrolls are security-scanned and cached by content hash. Once scanned, the scroll is trusted for subsequent runs.

This architectural constraint means:
- **No runtime injection** - There is no input path to primitives outside of scrolls
- **Scan once at the door** - Security validation happens per-scroll, not per-primitive
- **Lock the doors, not the walls** - The scroll boundary is the security perimeter

Scrolls use the **Scroll Assembly** language — a strongly-typed, purpose-built syntax with explicit types, blocks-as-expressions, and composable error handling. See `src/scroll/assembly/scroll_assembly.pest` for the formal grammar.

## Installation

```bash
# From source
cargo install --path .

# Nix
nix build   # or nix run .#sage-lore
```

After installing, initialize your configuration:

```bash
sage-lore init              # creates ~/.config/sage-lore/ (user config)
sage-lore init --project    # creates .sage-lore/ (project config)
```

## Usage

```bash
sage-lore run <scroll-file> --project .
sage-lore run scroll.scroll --var issue_number=42 --output result -v
```

## Primitives

sage-lore provides 20 orthogonal primitives for deterministic LLM orchestration:

### Core Primitives (6)

Transform and manipulate information within the scroll context.

1. **elaborate** - Expand content with depth control (thorough/balanced/concise)
2. **distill** - Reduce content with intensity control (aggressive/balanced/minimal)
3. **split** - Break content by strategy (semantic/structural/count)
4. **merge** - Combine inputs with strategy (sequential/reconcile/union/intersection)
5. **validate** - Verify against criteria with mode (strict/majority/any)
6. **convert** - Transform between formats with schema support

### System Primitives (5)

Interact with the external environment.

7. **fs** - Filesystem operations (read, write, copy, move, delete, list, exists)
8. **vcs** - Version control operations (commit, status, diff, log, branch, checkout, add)
9. **test** - Test execution and coverage analysis (cargo, npm, jest, pytest, go, make, bats, vitest)
10. **platform** - Git forge operations (get_issue, create_issue, close_issue, list_issues, env, info). Forgejo backend included; adapter pattern supports GitHub/GitLab
11. **run** - Execute another scroll (scroll composition)

### Agent Operations (4)

Invoke and coordinate LLM agents.

12. **invoke** - Execute a single agent with a specific prompt
13. **parallel** - Fan out same prompt to multiple agents for diverse perspectives
14. **consensus** - Achieve agreement among agents on a decision
15. **concurrent** - Run different operations simultaneously (operation parallelism)

### Flow Control (4)

Control execution flow and data wiring within scrolls.

16. **branch** - Conditional execution based on runtime values
17. **loop** - Iterate over collections or repeat until condition
18. **aggregate** - Combine multiple results into a summary
19. **set** - Variable binding and data wiring (maps, arrays, values)

### Security (1)

Security scanning and validation.

20. **secure** - Run security scans (secret_detection, dependency_cve, static_analysis)

See `docs/reference/primitives.md` for detailed documentation and examples.

## Production Scrolls

The `scrolls/` directory contains production scrolls that ship with the package. When installed, these are available via the scroll search path — invoke by name, no file paths needed.

### Architecture

```
CONFIG LAYER → ADAPTER SCROLLS → EXECUTION SCROLLS
```

- **Execution scrolls** are platform-agnostic, pure workflow logic
- **Adapter scrolls** handle platform-specific integration (Forgejo, GitHub, etc.)
- **Config layer** provides environment-specific variables

### Execution Scrolls (Platform-Agnostic)

Located in `scrolls/`:

- `run-chunk.scroll` - Execute single implementation chunk
  - **Requires**: `chunk` (ChunkInput: title, number, complexity, files, spec, parent_story)
  - **Uses**: elaborate, invoke, validate, convert

- `run-story.scroll` - Execute story by running all chunks
  - **Requires**: `story` (StoryInput: number, title, body, epic_title, chunks)
  - **Uses**: loop → run-chunk.scroll, merge, validate, convert

- `run-epic.scroll` - Execute epic by running all stories
  - **Requires**: `epic` (EpicInput: number, title, milestone, stories)
  - **Uses**: loop → run-story.scroll, merge, validate, convert

- `dev-story.scroll` - Development wrapper with project context
  - **Requires**: `story` (StoryInput)
  - **Uses**: fs (load context), elaborate, run-story.scroll, test, validate

- `complete-story.scroll` - Finalize story (tests, commit, platform update)
  - **Requires**: `story`, `branch_name`
  - **Uses**: test, vcs, platform

- `complete-epic.scroll` - Finalize epic (merge, PR, platform update)
  - **Requires**: `epic`, `target_branch`, `epic_branch`
  - **Uses**: test, vcs, elaborate, platform, distill

- `code-review.scroll` - Adversarial review with depth levels
  - **Requires**: `depth` (quick/standard/thorough), `files`, `context`
  - **Uses**: branch, invoke, validate, vcs, test, secure

### Adapter Scrolls (Platform-Specific)

Located in `scrolls/adapters/`:

- `chunk-from-forgejo.scroll` - Fetch chunk from Forgejo issue
  - **Requires**: `chunk_number`, `repo`, `parent_story`
  - **Provides**: `chunk` (ChunkInput contract)
  - **Uses**: platform (fetch), invoke (interpret)

- `story-from-forgejo.scroll` - Fetch story with linked chunks
  - **Requires**: `story_number`, `repo`, `epic_title`
  - **Provides**: `story` (StoryInput contract)
  - **Uses**: platform (fetch), loop → chunk-from-forgejo.scroll, invoke

- `epic-from-forgejo.scroll` - Fetch epic with linked stories
  - **Requires**: `epic_number`, `repo`, `milestone`
  - **Provides**: `epic` (EpicInput contract)
  - **Uses**: platform (fetch), loop → story-from-forgejo.scroll, invoke

### Usage Example

```bash
# Fetch epic from Forgejo and build the project
sage-lore run build-from-forgejo --project . \
  --var epic_number=1 \
  --var milestone_name="Epic #1" \
  --var project_root=. -v

# Or run a single story directly
sage-lore run run-story --project . --var story=@story-data.json
```

### Contract Guarantees

All adapters guarantee their output matches the corresponding execution scroll's `requires:` contract. This enables:

- **Composability**: Adapters can be swapped (Forgejo → GitHub) without changing execution logic
- **Testability**: Execution scrolls can be tested with synthetic data
- **Platform independence**: Same workflow runs on any platform with an adapter

## Git Workflow

### Archiving Merged Branches

When a branch is merged, archive it before deletion to preserve the reference:

```bash
# Before deleting merged branch:
git tag archive/branch-name branch-name
git push origin archive/branch-name
git branch -d branch-name
```

This workflow is currently **manual** in v1.0. Automated archive-on-merge is planned for v1.1.

**Why archive?** Preserves branch context for historical investigation without cluttering active branch lists.

## Directory Structure

```
sage-lore/
├── src/
│   ├── lib.rs              # Library root
│   ├── main.rs             # CLI entry point
│   ├── primitives/         # Core operations (fs, git, invoke, etc.)
│   ├── scroll/             # Scroll parsing, execution, and Assembly language
│   │   └── assembly/       # Scroll Assembly parser, type checker, dispatch
│   ├── config/             # Configuration loading and merging
│   └── cli/                # CLI commands (run, lsp, auth)
├── examples/
│   └── scrolls/            # Production .scroll files (Scroll Assembly syntax)
├── editors/
│   ├── vscode/             # VS Code extension (syntax + LSP client)
│   ├── vim/                # Vim/Neovim syntax highlighting
│   ├── kate/               # KDE Kate/KSyntaxHighlighting
│   └── neovim/             # Tree-sitter highlight queries
├── tests/
│   ├── unit/               # Unit tests
│   ├── integration/        # Integration tests
│   ├── support/            # Shared test infrastructure
│   └── fixtures/           # Test fixture files
└── .sage-lore/             # Project configuration (template)
    ├── config.yaml         # Project settings
    ├── scrolls/            # Project-specific scrolls
    ├── interfaces/         # Backend configs (no secrets)
    ├── security/           # Security policy overrides
    └── state/              # Execution state (git-tracked)
```

## Configuration

sage-lore uses a layered configuration hierarchy. Settings are merged with project values taking precedence:

| Layer | Location | Purpose |
|-------|----------|---------|
| Corp | `/etc/sage-lore/` | Organization-wide defaults, security floors |
| User | `~/.config/sage-lore/` | Personal preferences, API keys |
| Project | `.sage-lore/` | Project-specific overrides |

**Resolution order**: Project → User → Corp (most specific wins)

### Secrets

Scrolls **never contain secrets**. They reference environment variables:

```
invoke(agent: "dev", instructions: "...") {
    tier: premium,   // resolved by executor from config
}
```

Secrets are resolved from: Environment → `.env` → `~/.config/sage-lore/secrets.yaml`

## Security

### Principles

- **Ratchet model**: Security floors can only be raised, never lowered
- **Reference-only secrets**: Scrolls reference `${VAR}`, never contain values
- **Validation**: Engine warns/blocks if scroll contains secret patterns

### Best Practices

- **Do not run as root**. sage-lore does not require elevated privileges.
- **Scope CI/CD permissions**. Use minimal required access for automation.
- **Review scrolls before execution**. Scrolls can invoke external commands.
- **Keep secrets in environment**. Never commit `.env` or `secrets.yaml`.

## Platform Support

- **Linux**: Supported
- **macOS**: Supported
- **Windows**: Not supported in v0.1.0

## Known Limitations (v1.0)

- **Manual branch archiving**: The archive-on-merge workflow (tagging deleted branches) is manual. Automated archiving via git hooks or forge integration is planned for v1.1.
- **Windows support**: Not available in v1.0. WSL2 is recommended for Windows users.

## Legal Notice

sage-lore is a framework for LLM orchestration. Scrolls that invoke LLMs
constitute AI system deployments. Users are responsible for ensuring their
scroll-based workflows comply with applicable regulations (including the
EU AI Act and relevant jurisdictional laws). This is not legal advice;
consult legal counsel for compliance guidance.

## License

MIT License. See [LICENSE](LICENSE) for details.

The SAGE Method was originally inspired by concepts from the [BMAD Method](https://github.com/bmadcode/BMAD-METHOD). sage-lore is an independent implementation.

## References

- sage-method — Framework of agents and workflows (SAGE Method repository)
- Scroll Assembly grammar — `src/scroll/assembly/scroll_assembly.pest`
- Editor support — `editors/README.md`
- LSP server — `sage-lore lsp` (stdio, reuses parser + type checker)
