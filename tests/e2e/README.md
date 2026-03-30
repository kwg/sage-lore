# E2E Test: sage-dice NLP Dice Roller

Self-contained end-to-end test for sage-lore. Builds a complete NLP dice roller
from fixture data — no Forgejo or external API required.

## What it tests

The full scroll pipeline: epic -> stories -> chunks -> implement -> write -> test -> review -> consensus.

- **Epic #1**: NLP Dice Roller (3 stories, 12 chunks)
- **Story 1**: Dice Engine Core (types, parser, engine, output)
- **Story 2**: Game System Modifiers (exploding dice, Fate dice)
- **Story 3**: NLP Parser + CLI Polish (clap, ollama integration)

## Prerequisites

1. `sage-lore` binary built (`cargo build --release`)
2. An LLM backend:
   - **Claude** (default): `claude` CLI installed or `ANTHROPIC_API_KEY` set
   - **Ollama**: local server at `http://localhost:11434`

## Running

The test creates a **sibling project directory** next to sage-lore — it does NOT run inside the sage-lore tree.

```bash
# From sage-lore repo root:
./tests/e2e/run-e2e.sh                         # creates ../sage-dice-e2e/

# Custom location:
./tests/e2e/run-e2e.sh /tmp/sage-dice-e2e

# With Ollama:
SAGE_LLM_BACKEND=ollama ./tests/e2e/run-e2e.sh

# Custom binary:
SAGE_LORE_BIN=./target/debug/sage-lore ./tests/e2e/run-e2e.sh
```

## What the script does

1. Creates a clean project directory (default: `../sage-dice-e2e/`)
2. Copies fixtures, scrolls, config, and Cargo.toml into it
3. Initializes a git repo (executor requires one)
4. Runs `sage-lore run e2e-sage-dice.scroll` from that directory

## How it works

Uses the **adapter pattern** to swap Forgejo API calls with local fixture files:

```
Production:  epic-from-forgejo.scroll  -> platform.get_issue() -> Forgejo API
E2E test:    epic-from-fixtures.scroll -> fs.read()            -> fixture files
```

The execution scrolls (implement-chunk) are unchanged — only the data source is swapped.

## After running

The test generates a complete Rust project. You can build it:

```bash
cd ../sage-dice-e2e   # or wherever you pointed it
cargo build
./target/debug/sage-dice "roll 2d6+3"
```

## Fixture data

`fixtures/` contains JSON snapshots from the sage-dice Forgejo repo:
- `issue-{N}.json` — full issue data (number, title, body, labels)
- `epic-{N}-meta.json` — pre-extracted story numbers
- `story-{N}-meta.json` — pre-extracted chunk numbers
- `chunk-{N}-meta.json` — pre-extracted complexity and file list
