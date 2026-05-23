# CI/CD Integration

llm-wiki is a single binary with no runtime dependencies. It runs in
any CI environment that has `git`.

## Install in CI

```yaml
# GitHub Actions
- name: Install llm-wiki
  run: curl -fsSL https://raw.githubusercontent.com/hataichanokpan-dev/brain-mcp/main/install.sh | bash

# Or from source (slower)
- name: Install llm-wiki
  run: git clone https://github.com/hataichanokpan-dev/brain-mcp /tmp/brain-mcp && cargo install --path /tmp/brain-mcp
```

## Schema Validation on PR

Validate that all pages pass frontmatter validation and all schemas
are well-formed. Fails the build if a page has invalid frontmatter
(in strict mode) or a schema file is broken.

```yaml
name: Wiki Lint

on:
  pull_request:
    paths:
      - 'wiki/**'
      - 'schemas/**'
      - 'wiki.toml'

jobs:
  validate:
    runs-on: ubuntu-latest
    env:
      LLM_WIKI_CONFIG: ${{ runner.temp }}/llm-wiki.toml
    steps:
      - uses: actions/checkout@v6

      - name: Install llm-wiki
        run: curl -fsSL https://raw.githubusercontent.com/hataichanokpan-dev/brain-mcp/main/install.sh | bash

      - name: Register wiki
        run: llm-wiki spaces create . --name ci

      - name: Validate schemas
        run: llm-wiki schema validate --wiki ci

      - name: Ingest (dry run)
        run: llm-wiki ingest wiki/ --dry-run --wiki ci
```

## Index Rebuild on Merge

Rebuild the search index after content changes land on main. Useful
if the index is stored as a CI artifact or deployed alongside a
static site.

```yaml
name: Rebuild Index

on:
  push:
    branches: [main]
    paths:
      - 'wiki/**'
      - 'schemas/**'

jobs:
  rebuild:
    runs-on: ubuntu-latest
    env:
      LLM_WIKI_CONFIG: ${{ runner.temp }}/llm-wiki.toml
    steps:
      - uses: actions/checkout@v6

      - name: Install llm-wiki
        run: curl -fsSL https://raw.githubusercontent.com/hataichanokpan-dev/brain-mcp/main/install.sh | bash

      - name: Register wiki
        run: llm-wiki spaces create . --name ci

      - name: Rebuild index
        run: llm-wiki index rebuild --wiki ci

      - name: Index status
        run: llm-wiki index status --wiki ci --format json
```

## Ingest on PR Merge

Automatically validate and commit after content is merged. Useful
for wikis where an LLM writes pages via PR and the engine validates
on merge.

```yaml
name: Auto Ingest

on:
  push:
    branches: [main]
    paths:
      - 'wiki/**'

jobs:
  ingest:
    runs-on: ubuntu-latest
    env:
      LLM_WIKI_CONFIG: ${{ runner.temp }}/llm-wiki.toml
    steps:
      - uses: actions/checkout@v6

      - name: Install llm-wiki
        run: curl -fsSL https://raw.githubusercontent.com/hataichanokpan-dev/brain-mcp/main/install.sh | bash

      - name: Register wiki
        run: llm-wiki spaces create . --name ci

      - name: Ingest all
        run: llm-wiki ingest wiki/ --wiki ci
```

## Pre-commit Hook

Validate frontmatter locally before committing. Add to
`.pre-commit-config.yaml`:

```yaml
repos:
  - repo: local
    hooks:
      - id: wiki-validate
        name: Validate wiki pages
        entry: bash -c 'llm-wiki spaces create . --name local 2>/dev/null; llm-wiki ingest wiki/ --dry-run --wiki local'
        language: system
        files: '^wiki/.*\.md$'
        pass_filenames: false
```

Or as a git hook in `.git/hooks/pre-commit`:

```bash
#!/bin/bash
set -e
llm-wiki spaces create . --name local 2>/dev/null || true
llm-wiki ingest wiki/ --dry-run --wiki local
```

## Graph Generation in CI

Generate a concept graph as a build artifact or commit it to the repo:

```yaml
      - name: Generate graph
        run: |
          llm-wiki graph --format mermaid --output wiki/graph.md --wiki ci
          llm-wiki graph --format dot --output wiki/graph.dot --wiki ci
```

## Integration Test Workflow

The integration suite lives in `tests-integration/` — a pytest project managed
by `uv`. Three suites cover all transports:

| Suite | Transport | Target |
|---|---|---|
| `engine/` | CLI subprocess | `make validate-py-engine` |
| `mcp/` | MCP stdio (official `mcp` SDK) | `make validate-py-mcp` |
| `acp/` | ACP NDJSON stdio (`asyncio`) | `make validate-py-acp` |

Run all suites locally:

```bash
make validate-py
```

Run a specific suite:

```bash
make validate-py-engine
make validate-py-mcp
make validate-py-acp
```

The GitHub Actions workflow at `.github/workflows/integration.yml` triggers
automatically on pushes to `main` that touch `src/**` or `tests-integration/**`,
and can also be run manually from the Actions tab → **Integration Tests** →
**Run workflow** with a `suite` input (`all`, `engine`, `mcp`, `acp`).

The workflow:
1. Builds the debug binary (`cargo build --locked`)
2. Installs Python deps via `uv sync`
3. Runs the selected pytest suite(s)

No external tools required (`jq`, `mcptools`, etc.). Dependencies are declared
in `tests-integration/pyproject.toml`.

Use this after merging features that touch MCP handlers, ACP workflows, graph
rendering, or ingest logic — areas not covered by unit tests alone.

## Environment Notes

- llm-wiki writes its space registry to `~/.llm-wiki/config.toml` by default
- In CI, set `LLM_WIKI_CONFIG` to a temp path to avoid touching `~/.llm-wiki/`:
  ```yaml
  env:
    LLM_WIKI_CONFIG: ${{ runner.temp }}/llm-wiki.toml
  ```
  Or pass `--config` to individual commands when env vars are not practical.
- `spaces create` is idempotent — safe to run on every build
- `--dry-run` on ingest validates without committing
- The wiki repo must be a git repository (`actions/checkout` handles this)
