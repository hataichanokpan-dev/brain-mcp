# MCP Session Timeout — Connection Dropped After Idle Period

- **Date:** 2026-05-24
- **Severity:** Medium (usability)
- **Status:** Fixed
- **Affected:** Remote MCP HTTP transport via `mcp-remote`

## Symptom

When using brain-mcp over HTTP through `mcp-remote`, the MCP connection drops after a period of inactivity. Claude Code reports `Connection closed` (MCP error -32000) on the next tool call. The user must run `/mcp` to reinitialize the connection before tools work again.

This happens consistently during normal usage — any gap longer than ~5 minutes between MCP tool calls triggers the issue.

## Root Cause

The server uses `rmcp`'s `StreamableHttpService` with **default** session timeout values. In `src/server.rs` (line 46–51):

```rust
let config = StreamableHttpServerConfig::default()
    .with_cancellation_token(cancel.child_token())
    .with_allowed_hosts(serve_cfg.http_allowed_hosts.clone());

let service: StreamableHttpService<McpServer, LocalSessionManager> =
    StreamableHttpService::new(move || Ok(server.clone()), Default::default(), config);
```

The `Default::default()` for `LocalSessionManager` → `SessionConfig` sets:

| Field | Default | Source (rmcp 1.7) |
|-------|---------|--------------------|
| `keep_alive` | `Some(300s)` | `local.rs:1151` — `DEFAULT_KEEP_ALIVE = Duration::from_secs(300)` |
| `init_timeout` | `Some(60s)` | `local.rs:1154` — `DEFAULT_INIT_TIMEOUT = Duration::from_secs(60)` |
| `completed_cache_ttl` | `60s` | `local.rs:1153` |

When no MCP request arrives within `keep_alive` (5 minutes), the session worker exits with `WorkerQuitReason::IdleTimeout`. The `mcp-remote` client receives a connection-closed event and cannot recover without reinitialization.

Additionally, `StreamableHttpServerConfig` defaults `sse_keep_alive` to `Duration::from_secs(15)` — while this sends SSE heartbeat comments every 15 seconds, it does **not** reset the session-level idle timer.

## Contributing Factor: Skills Don't Warn About Reconnection

Brain MCP skills (`ingest`, `crystallize`, `content`, etc.) document workflows as if the MCP connection is always available. There is no guidance for:
- Handling connection drops mid-workflow
- When to reinitialize the connection
- Retry strategies for transient failures

Users encounter `Connection closed` errors with no documented recovery path.

## Fix

### 1. Increase and expose MCP HTTP session keep_alive

The server now builds an explicit `rmcp` `SessionConfig` instead of using the
`LocalSessionManager` default. MCP HTTP sessions stay alive for 6 hours by
default, which covers normal IDE idle gaps.

```rust
let session_config = mcp_session_config(serve_cfg);

let mut session_manager = LocalSessionManager::default();
session_manager.session_config = session_config;
```

The setting is global-only because it controls the running server process, not
an individual wiki:

```bash
llm-wiki config set serve.mcp_session_keep_alive_secs 21600 --global
```

`0` disables idle cleanup and should only be used on private single-user
deployments.

Related controls:

```bash
llm-wiki config set serve.mcp_init_timeout_secs 60 --global
llm-wiki config set serve.mcp_completed_cache_ttl_secs 60 --global
```

### 2. Document remote-session behavior

The README and IDE/configuration guides now document that Streamable HTTP uses a
stateful `Mcp-Session-Id`, and that remote bridges should raise
`serve.mcp_session_keep_alive_secs` instead of relying on users to manually
reinitialize after normal idle periods.

### 3. Regression coverage

Added server/config tests for:

- default MCP HTTP keep_alive = 21600 seconds
- `0` disables optional session/init timeouts
- config set/get support for all new keys
- wiki-level config rejects these global-only keys

## Verification

1. Start server: `llm-wiki serve --http :47778 --watch`
2. Connect via `mcp-remote`
3. Make one MCP call, then wait > 5 minutes
4. Make another call → should **not** get `Connection closed`

Automated verification:

```bash
cargo test --lib server::tests
cargo test --test config
```

## Related

- rmcp `SessionConfig`: `local.rs:1119–1164`
- rmcp `StreamableHttpServerConfig`: `tower.rs:39–109`
- Previous bug: `2026-05-23-profile-get-empty-pages.md`
