# Installation

## Quick Install

macOS / Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/hataichanokpan-dev/brain-mcp/main/install.sh | bash
```

For a private repository, run the script from a local clone or export
`GITHUB_TOKEN`/`GH_TOKEN` before running it so release lookup and downloads can
authenticate.

Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/hataichanokpan-dev/brain-mcp/main/install.ps1 | iex
```

The scripts detect your platform, download the latest binary from
GitHub releases, install it, verify `git` is available, and try to install
Hugo Extended for the bundled web UI.

Custom install directory:

```bash
LLM_WIKI_INSTALL_DIR=~/.local/bin curl -fsSL https://raw.githubusercontent.com/hataichanokpan-dev/brain-mcp/main/install.sh | bash
```

Skip automatic Hugo installation:

```bash
LLM_WIKI_INSTALL_HUGO=0 curl -fsSL https://raw.githubusercontent.com/hataichanokpan-dev/brain-mcp/main/install.sh | bash
```

## From Source (cargo)

Requires [Rust](https://www.rust-lang.org/tools/install) 1.95+.

```bash
git clone https://github.com/hataichanokpan-dev/brain-mcp.git
cd brain-mcp
cargo install --path .
```

## Pre-built Binary

Use `install.sh`, `install.ps1`, or download from this repository's GitHub
releases manually. Do not use the upstream `llm-wiki` Homebrew/asdf channels
for this fork.

## Homebrew (macOS / Linux)

Not published for `brain-mcp` yet. Use `install.sh` or the source install above.

## asdf Version Manager

Not published for `brain-mcp` yet. Use `install.sh` or the source install above.

## Manual Download·

Download a binary from the
[GitHub releases](https://github.com/hataichanokpan-dev/brain-mcp/releases)
page. Available targets:

| Platform            | Archive                            |
| ------------------- | ---------------------------------- |
| Linux x86_64        | `x86_64-unknown-linux-gnu.tar.gz`  |
| Linux aarch64       | `aarch64-unknown-linux-gnu.tar.gz` |
| macOS Intel         | `x86_64-apple-darwin.tar.gz`       |
| macOS Apple Silicon | `aarch64-apple-darwin.tar.gz`      |
| Windows x86_64      | `x86_64-pc-windows-msvc.zip`       |

```bash
# Example: macOS Apple Silicon
curl -LO https://github.com/hataichanokpan-dev/brain-mcp/releases/latest/download/aarch64-apple-darwin.tar.gz
tar xzf aarch64-apple-darwin.tar.gz
sudo mv llm-wiki /usr/local/bin/
```

## Verify

```bash
llm-wiki --version
```

## Prerequisites

- `git` — required for wiki repositories (commit, diff, history)
- No required runtime dependencies — llm-wiki is a single static binary
- Optional web UI: [Hugo Extended](https://gohugo.io/installation/) 0.147+.
  `install.sh` attempts to install it automatically. The binary can still
  install the `site/` scaffold without Hugo, but `llm-wiki web serve` and
  `llm-wiki web build` need `hugo` on `PATH`.
