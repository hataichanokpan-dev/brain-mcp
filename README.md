# brain-mcp

Git-backed knowledge engine สำหรับ agent ที่คุยผ่าน MCP ได้ เช่น Codex, Claude Code, Claude Desktop และ IDE อื่นๆ

โปรเจคนี้เริ่มจาก `llm-wiki` engine และกำลังอัปเกรดตาม [BLUEPRINT.md](BLUEPRINT.md) ให้เป็นสมองกลางแบบ 3 stores:

- `profile` - กฎ, identity, style, stack, constraints ของผู้ใช้
- `semantic` - ความรู้แบบ declarative เช่น concept, entity, source, project, decision
- `procedure` - runbook ที่ execute ได้และต้องมี verification

สถานะปัจจุบัน: core engine พร้อมใช้งานเป็น MCP server แล้ว มี Markdown + Git เป็น source of truth, Tantivy BM25 search, typed schemas, graph, audit history และ read-tier tool aliases ตาม blueprint ชุดแรก. Vector/Qdrant/BGE-M3 ยังเป็น phase ถัดไป ยังไม่ได้เปิดใช้ใน binary นี้

## What It Does

`brain-mcp` ทำให้ agent มี long-term knowledge base ที่อ่านและเขียนได้ผ่าน MCP:

- เก็บความรู้เป็น Markdown ที่อ่านเองได้
- validate frontmatter ด้วย JSON Schema
- search ด้วย BM25
- สร้าง graph จาก wikilinks และ typed frontmatter edges
- commit เข้า Git เพื่อ audit และ rollback
- expose tools ผ่าน MCP stdio หรือ MCP HTTP
- ใช้กับ Codex และ Claude Code ได้โดยเพิ่ม MCP server config
- มี Hugo web UI สำหรับ browse wiki ใน browser โดย mount content จาก `wiki/` โดยตรง

ชื่อ binary ตอนนี้ยังเป็น:

```bash
llm-wiki
```

ชื่อ crate ยังเป็น:

```toml
llm-wiki-engine
```

## Current Tool Surface

Core wiki tools:

- `wiki_search`, `wiki_list`, `wiki_content_read`, `wiki_content_write`
- `wiki_content_new`, `wiki_content_commit`, `wiki_ingest`
- `wiki_graph`, `wiki_suggest`, `wiki_lint`, `wiki_stats`
- `wiki_history`, `wiki_export`, `wiki_schema`
- `wiki_spaces_create`, `wiki_spaces_register`, `wiki_spaces_list`, `wiki_spaces_remove`, `wiki_spaces_set_default`
- `wiki_config`, `wiki_index_rebuild`, `wiki_index_status`, `wiki_resolve`

Blueprint read-tier aliases:

- `profile_get`
- `semantic_search`
- `semantic_get`
- `procedural_find`
- `procedural_get`
- `graph_neighbors`
- `audit_history`

Important: `semantic_search` currently maps to the existing BM25 search. Hybrid vector search with Qdrant/BGE-M3 is not implemented yet

## Requirements

- Rust 1.95+
- Git
- Windows, macOS, or Linux
- Optional web UI: Hugo Extended 0.147+ (`install.sh` attempts to install it automatically)
- For integration tests: Python 3.11+ and `uv`

This repo includes `rust-toolchain.toml`, so `cargo` will use Rust 1.95 automatically when available.

## Install From Source

Clone and build:

```bash
git clone <repo-url> brain-mcp
cd brain-mcp
cargo build --release
```

Install into Cargo bin path:

```bash
cargo install --path .
```

Verify:

```bash
llm-wiki --version
```

If you do not install it globally, use the built binary directly:

Windows:

```powershell
.\target\release\llm-wiki.exe --version
```

macOS/Linux:

```bash
./target/release/llm-wiki --version
```

## Create Your Brain Wiki

Create a wiki repository. This is where the actual memory lives:

Windows PowerShell:

```powershell
llm-wiki spaces create "$HOME\wikis\brain" --name brain --set-default
```

macOS/Linux:

```bash
llm-wiki spaces create ~/wikis/brain --name brain --set-default
```

This creates:

```text
brain/
  wiki/              # Markdown content
  schemas/           # JSON schemas
  inbox/             # ingest staging
  raw/               # raw/archive material
  site/              # Hugo web UI scaffold
  wiki.toml
  .git/
```

Rebuild the index:

```bash
llm-wiki index rebuild --wiki brain
```

Check status:

```bash
llm-wiki index status --wiki brain
```

## Suggested Layout For Blueprint Stores

Inside the wiki content root (`~/wikis/brain/wiki`):

```text
profile/
  identity.md
  hard-rules.md
  soft-preferences.md
  style-guide.md
  stack.md
  constraints.md

concepts/
entities/
sources/
projects/
decisions/

procedural/
  deployment/
  development/
  troubleshooting/
```

Example profile page:

```markdown
---
title: "Hard Rules"
type: profile
section: rules
priority: hard
status: active
created: 2026-05-23
last_verified: 2026-05-23
---

- Before commit, run format, lint, and tests.
- Do not silently write memory. Propose first when changing user profile.
```

Example procedure page:

```markdown
---
title: "Deploy brain-mcp"
type: procedure
status: draft
verified_count: 0
failure_count: 0
verification:
  - "llm-wiki --version exits 0"
  - "MCP client can list tools"
risk_level: medium
tags: [deployment, mcp]
---

## Steps

1. Build release binary.
2. Register MCP server in client config.
3. Restart client.

## Verification

- Run `llm-wiki --version`
- Ask the MCP client to list tools

## Rollback

1. Remove MCP server entry from client config.
2. Restart client.
```

Ingest after editing:

```bash
llm-wiki ingest profile --wiki brain
llm-wiki ingest procedural --wiki brain
```

## Web UI

`spaces create` จะติดตั้ง Hugo scaffold ไว้ที่ `site/` ให้อัตโนมัติ. ถ้าเป็น wiki เก่าหรือ wiki ที่ register จาก repo เดิม ให้รัน:

```bash
llm-wiki web install --wiki brain
```

เปิด web preview:

```bash
llm-wiki web serve --wiki brain
```

แล้วเปิด:

```text
http://127.0.0.1:1313/
```

Build static site:

```bash
llm-wiki web build --wiki brain --minify
```

ตรวจสถานะ:

```bash
llm-wiki web status --wiki brain
```

Web UI ใช้ scaffold ที่ vendor มาจาก `geronimo-iia/llm-wiki-hugo-cms` และ mount content จาก `wiki/` โดยตรง จึงไม่มีการ copy Markdown ออกจาก source of truth.

## Run As MCP Server

Default mode is MCP over stdio:

```bash
llm-wiki serve
```

With live indexing:

```bash
llm-wiki serve --watch
```

MCP over HTTP:

```bash
llm-wiki serve --http :47778
```

HTTP endpoint:

```text
http://127.0.0.1:47778/mcp
```

ACP plus MCP HTTP:

```bash
llm-wiki serve --acp --http :47778
```

Why: ACP uses stdio. MCP stdio and ACP cannot share the same stdio stream, so when ACP is enabled, run MCP through HTTP.

Start MCP server and web UI together:

```bash
llm-wiki serve --watch --web
```

MCP stays on stdio by default, and the web UI is available at `http://127.0.0.1:1313/`.

## Configure Codex

Codex reads MCP servers from:

```text
~/.codex/config.toml
```

Add a stdio MCP server:

```toml
[mcp_servers.brain]
command = "llm-wiki"
args = ["serve"]
```

If `llm-wiki` is not on PATH, point to the binary directly.

Windows example:

```toml
[mcp_servers.brain]
command = 'C:\Programing\PersonalAI\mcp\brain-mcp\target\release\llm-wiki.exe'
args = ["serve"]
```

macOS/Linux example:

```toml
[mcp_servers.brain]
command = "/home/you/projects/brain-mcp/target/release/llm-wiki"
args = ["serve"]
```

Use a custom config path:

```toml
[mcp_servers.brain]
command = "llm-wiki"
args = ["--config", "C:\\Users\\you\\.llm-wiki\\config.toml", "serve"]
```

Or with environment:

```toml
[mcp_servers.brain]
command = "llm-wiki"
args = ["serve"]

[mcp_servers.brain.env]
LLM_WIKI_CONFIG = "C:\\Users\\you\\.llm-wiki\\config.toml"
```

Restart Codex after editing the config.

Quick verification prompt in Codex:

```text
List available MCP tools from brain.
```

Then:

```text
Call profile_get and summarize my active profile.
```

## Configure Claude Code

Claude Code can use a project `.mcp.json` file.

Create `.mcp.json` in the project where you want Claude Code to see the server:

```json
{
  "mcpServers": {
    "brain": {
      "command": "llm-wiki",
      "args": ["serve"]
    }
  }
}
```

Windows direct binary example:

```json
{
  "mcpServers": {
    "brain": {
      "command": "C:\\Programing\\PersonalAI\\mcp\\brain-mcp\\target\\release\\llm-wiki.exe",
      "args": ["serve"]
    }
  }
}
```

With custom config:

```json
{
  "mcpServers": {
    "brain": {
      "command": "llm-wiki",
      "args": ["--config", "C:\\Users\\you\\.llm-wiki\\config.toml", "serve"]
    }
  }
}
```

With live indexing:

```json
{
  "mcpServers": {
    "brain": {
      "command": "llm-wiki",
      "args": ["serve", "--watch"]
    }
  }
}
```

Alternative Claude Code CLI flow:

```bash
claude mcp add
```

or:

```bash
claude mcp add-json brain '{"command":"llm-wiki","args":["serve"]}'
```

Restart Claude Code after changing MCP config.

Quick verification prompt in Claude Code:

```text
Use the brain MCP server. Call wiki_spaces_list, then call profile_get.
```

## Configure Claude Desktop

Claude Desktop MCP config uses the same shape:

```json
{
  "mcpServers": {
    "brain": {
      "command": "llm-wiki",
      "args": ["serve"]
    }
  }
}
```

Restart Claude Desktop after editing the config.

## Remote MCP Server

For a remote host, run HTTP transport:

```bash
llm-wiki serve --http :47778
```

Use a private network layer such as Tailscale or Cloudflare Tunnel. Do not expose this server directly to the public internet. The wiki stores private memory and source material.

Remote endpoint:

```text
http://<host>:47778/mcp
```

### Allow Remote Host Headers

By default, the HTTP server only allows requests with `Host: localhost` (DNS rebinding protection). To accept connections from remote clients, add the server's public IP or hostname to the allowed hosts list:

```bash
llm-wiki config set --global serve.http_allowed_hosts "localhost,127.0.0.1,::1,<your-public-ip>"
```

Restart the server after changing this setting.

### Systemd Service (Linux)

To keep the server running across reboots:

**MCP server only:**

```bash
printf '[Unit]\nDescription=Brain MCP Server\nAfter=network.target\n\n[Service]\nType=simple\nUser=opc\nExecStart=/usr/local/bin/llm-wiki serve --http :47778 --watch\nRestart=on-failure\nRestartSec=5\n\n[Install]\nWantedBy=multi-user.target\n' | sudo tee /etc/systemd/system/brain-mcp.service

sudo systemctl daemon-reload
sudo systemctl enable --now brain-mcp
sudo systemctl status brain-mcp
```

**MCP server + Web UI (combined):**

```bash
printf '[Unit]\nDescription=Brain MCP Server + Web UI\nAfter=network.target\n\n[Service]\nType=simple\nUser=opc\nExecStart=/usr/local/bin/llm-wiki serve --http :47778 --watch --web --web-port 1414 --web-bind 0.0.0.0\nRestart=on-failure\nRestartSec=5\n\n[Install]\nWantedBy=multi-user.target\n' | sudo tee /etc/systemd/system/brain-mcp.service

sudo systemctl daemon-reload
sudo systemctl enable --now brain-mcp
sudo systemctl status brain-mcp
```

Prerequisites for the Web UI variant: Hugo Extended must be installed. Run `llm-wiki web install --wiki brain` first and verify with `llm-wiki web status --wiki brain`.

If you get `Permission denied` (status 203/EXEC), ensure the binary is executable:

```bash
sudo chmod +x /usr/local/bin/llm-wiki
sudo systemctl restart brain-mcp
```

### Firewall

On Oracle Linux / RHEL / CentOS with `firewalld`:

```bash
# MCP server
sudo firewall-cmd --add-port=47778/tcp --permanent
# Web UI (only needed for the combined service variant)
sudo firewall-cmd --add-port=1414/tcp --permanent
sudo firewall-cmd --reload
```

With raw `iptables`:

```bash
sudo iptables -I INPUT -p tcp --dport 47778 -j ACCEPT
# Web UI
sudo iptables -I INPUT -p tcp --dport 1414 -j ACCEPT
```

Also open the port in your cloud provider's security group / security list (e.g., OCI Security List Ingress Rule for TCP 47778).

### Connect Claude Desktop to Remote Server

Claude Desktop uses stdio MCP by default and does not support the `url` field for remote servers. Use `mcp-remote` as a stdio-to-HTTP bridge:

```json
{
  "mcpServers": {
    "brain": {
      "command": "npx",
      "args": ["-y", "mcp-remote", "http://<host>:47778/mcp", "--allow-http"]
    }
  }
}
```

The `--allow-http` flag is required for non-HTTPS endpoints. For production use, place the server behind HTTPS (Tailscale, Cloudflare Tunnel, or a reverse proxy with TLS).

## Common Workflows

Search:

```bash
llm-wiki search "hybrid retrieval" --wiki brain
```

Create a page:

```bash
llm-wiki content new concepts/hybrid-retrieval --type concept --name "Hybrid Retrieval" --wiki brain
```

Read a page:

```bash
llm-wiki content read concepts/hybrid-retrieval --wiki brain
```

Ingest and commit:

```bash
llm-wiki ingest concepts/hybrid-retrieval --wiki brain
```

Show graph:

```bash
llm-wiki graph --root concepts/hybrid-retrieval --depth 2 --wiki brain
```

Show audit history:

```bash
llm-wiki history concepts/hybrid-retrieval --wiki brain
```

## Verification Checklist

After setup:

```bash
llm-wiki spaces list
llm-wiki schema list --wiki brain
llm-wiki index rebuild --wiki brain
llm-wiki search "test" --wiki brain
```

Expected:

- `spaces list` shows `brain`
- `schema list` includes `profile`, `entity`, `source`, `project`, `decision`, `procedure`
- `index rebuild` succeeds
- MCP client can list tools

## Development

Run Rust tests:

```bash
cargo test
```

On Windows, this repo currently works cleanly with MSVC Rust:

```powershell
rustup toolchain install 1.95-x86_64-pc-windows-msvc --component rustfmt --component clippy
cargo +1.95-x86_64-pc-windows-msvc test
```

Format and lint:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Integration tests:

```bash
cd tests-integration
uv sync --group dev
```

Windows:

```powershell
New-Item -ItemType Directory -Force -Path C:\tmp | Out-Null
$env:PYTHONUTF8 = "1"
$env:LLM_WIKI_BIN = "C:\path\to\llm-wiki.exe"
uv run pytest engine/ -v
uv run pytest acp/ -v
uv run pytest mcp/ -v
```

macOS/Linux:

```bash
export LLM_WIKI_BIN=/path/to/llm-wiki
uv run pytest engine/ -v
uv run pytest acp/ -v
uv run pytest mcp/ -v
```

Run integration suites separately. The test files intentionally reuse names across `engine/`, `acp/`, and `mcp/`, so collecting all folders in one pytest process can cause import mismatch.

## Troubleshooting

### Codex or Claude Code cannot find the server

Use an absolute `command` path in the MCP config.

Windows JSON paths must escape backslashes:

```json
"command": "C:\\path\\to\\llm-wiki.exe"
```

TOML can use single-quoted literal strings:

```toml
command = 'C:\path\to\llm-wiki.exe'
```

### Server exits immediately

Run it manually:

```bash
llm-wiki serve
```

If config is missing, create a wiki first:

```bash
llm-wiki spaces create ~/wikis/brain --name brain --set-default
```

### Tool list is empty

Rebuild index and restart the MCP client:

```bash
llm-wiki index rebuild --wiki brain
```

### HTTP client cannot connect

Make sure you use `/mcp`:

```text
http://127.0.0.1:47778/mcp
```

### ACP conflicts with MCP stdio

Use:

```bash
llm-wiki serve --acp --http :47778
```

Do not run `serve --acp` as a normal MCP stdio server.

## Roadmap

Implemented now:

- Markdown + Git canonical storage
- schemas for profile, semantic, and procedure memory
- BM25 search
- graph and backlinks
- Git audit history
- MCP stdio and HTTP transport
- ACP transport
- read-tier blueprint aliases

Next major phases from [BLUEPRINT.md](BLUEPRINT.md):

- Qdrant vector index
- BGE-M3 dense + sparse embeddings
- BGE reranker
- true hybrid `semantic_search`
- propose/commit gate for no silent writes
- procedural promotion workflow with verification evidence
- consolidation and stale/contradiction review workflow

## License

MIT OR Apache-2.0
