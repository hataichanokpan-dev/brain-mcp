use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

use anyhow::Result;
use axum::extract::State;
use axum::response::Json;
use rmcp::ServiceExt;
use rmcp::transport::streamable_http_server::session::local::{LocalSessionManager, SessionConfig};
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use serde::Serialize;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::config;
use crate::engine::WikiEngine;
use crate::mcp::McpServer;

/// Configuration for the managed Hugo web UI spawned by `serve --web`.
#[derive(Debug, Clone)]
pub struct WebServeConfig {
    pub wiki: Option<String>,
    pub bind: String,
    pub port: u16,
    pub drafts: bool,
}

struct WebTarget {
    wiki_name: String,
    repo_root: PathBuf,
    wiki_root: String,
}

// ── serve_stdio ───────────────────────────────────────────────────────────────

async fn serve_stdio(server: McpServer, mut shutdown: watch::Receiver<bool>) -> Result<()> {
    let transport = rmcp::transport::io::stdio();
    let service = server
        .serve(transport)
        .await
        .map_err(|e| anyhow::anyhow!("failed to start MCP stdio server: {e}"))?;

    tokio::select! {
        result = service.waiting() => {
            result.map_err(|e| anyhow::anyhow!("MCP stdio server error: {e}"))?;
        }
        _ = shutdown.changed() => {
            tracing::info!("stdio: shutdown signal received");
        }
    }
    Ok(())
}

// ── serve_http ────────────────────────────────────────────────────────────────

fn optional_duration_from_secs(secs: u64) -> Option<Duration> {
    if secs == 0 {
        None
    } else {
        Some(Duration::from_secs(secs))
    }
}

pub(crate) fn mcp_session_config(serve_cfg: &config::ServeConfig) -> SessionConfig {
    let mut session_config = SessionConfig::default();
    session_config.keep_alive = optional_duration_from_secs(serve_cfg.mcp_session_keep_alive_secs);
    session_config.init_timeout = optional_duration_from_secs(serve_cfg.mcp_init_timeout_secs);
    session_config.completed_cache_ttl =
        Duration::from_secs(serve_cfg.mcp_completed_cache_ttl_secs);
    session_config
}

async fn serve_http(
    server: McpServer,
    port: u16,
    serve_cfg: &config::ServeConfig,
    cancel: CancellationToken,
    engine: Arc<WikiEngine>,
) -> Result<()> {
    let addr: SocketAddr = ([0, 0, 0, 0], port).into();

    let config = StreamableHttpServerConfig::default()
        .with_cancellation_token(cancel.child_token())
        .with_allowed_hosts(serve_cfg.http_allowed_hosts.clone())
        .with_stateful_mode(serve_cfg.mcp_stateful_mode)
        .with_json_response(serve_cfg.mcp_json_response);
    let session_config = mcp_session_config(serve_cfg);

    tracing::info!(
        stateful_mode = serve_cfg.mcp_stateful_mode,
        json_response = serve_cfg.mcp_json_response,
        keep_alive_secs = ?session_config.keep_alive.map(|d| d.as_secs()),
        init_timeout_secs = ?session_config.init_timeout.map(|d| d.as_secs()),
        completed_cache_ttl_secs = session_config.completed_cache_ttl.as_secs(),
        "MCP HTTP session config",
    );
    if let Some(keep_alive) = session_config.keep_alive
        && keep_alive < Duration::from_secs(600)
    {
        tracing::warn!(
            keep_alive_secs = keep_alive.as_secs(),
            "MCP HTTP session keep_alive is short; remote clients may need to reinitialize frequently",
        );
    }

    let mut session_manager = LocalSessionManager::default();
    session_manager.session_config = session_config;

    let router = if serve_cfg.mcp_stateful_mode {
        let service: StreamableHttpService<McpServer, LocalSessionManager> =
            StreamableHttpService::new(
                move || Ok(server.clone()),
                Arc::new(session_manager),
                config,
            );
        axum::Router::new()
            .nest_service("/mcp", service)
            .route("/health", axum::routing::get(health_handler))
            .with_state(engine.clone())
    } else {
        let service: StreamableHttpService<McpServer, NeverSessionManager> =
            StreamableHttpService::new(
                move || Ok(server.clone()),
                Arc::new(NeverSessionManager::default()),
                config,
            );
        axum::Router::new()
            .nest_service("/mcp", service)
            .route("/health", axum::routing::get(health_handler))
            .with_state(engine.clone())
    };

    let max_attempts = if serve_cfg.max_restarts == 0 {
        1
    } else {
        serve_cfg.max_restarts
    };
    let mut backoff = std::time::Duration::from_secs(serve_cfg.restart_backoff as u64);
    let max_backoff = std::time::Duration::from_secs(30);

    for attempt in 1..=max_attempts {
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                tracing::info!(%addr, "HTTP server listening");
                axum::serve(listener, router)
                    .with_graceful_shutdown(cancel.cancelled_owned())
                    .await
                    .map_err(|e| anyhow::anyhow!("HTTP server error: {e}"))?;
                return Ok(());
            }
            Err(e) => {
                if attempt == max_attempts {
                    return Err(anyhow::anyhow!(
                        "HTTP bind failed after {max_attempts} attempts: {e}"
                    ));
                }
                tracing::warn!(
                    %addr,
                    error = %e,
                    attempt,
                    max = max_attempts,
                    "HTTP bind failed, retrying",
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(max_backoff);
            }
        }
    }
    unreachable!()
}

// ── web supervisor ────────────────────────────────────────────────────────────

fn selected_web_target(manager: &WikiEngine, explicit_wiki: Option<&str>) -> Result<WebTarget> {
    let engine = manager.state.read();
    let wiki_name = engine.resolve_wiki_name(explicit_wiki).to_string();
    let space = engine.space(&wiki_name)?;
    let repo_root = space.repo_root.clone();
    let wiki_root = space
        .wiki_root
        .strip_prefix(&space.repo_root)
        .unwrap_or_else(|_| std::path::Path::new("wiki"))
        .to_string_lossy()
        .replace('\\', "/");
    Ok(WebTarget {
        wiki_name,
        repo_root,
        wiki_root,
    })
}

fn prepare_web_target(target: &WebTarget) -> Result<()> {
    if !crate::web::is_installed(&target.repo_root) {
        crate::web::install_hugo_site(
            &target.repo_root,
            &target.wiki_name,
            &target.wiki_root,
            false,
        )?;
    } else {
        crate::web::sync_hugo_content(&target.repo_root, &target.wiki_root)?;
    }
    Ok(())
}

fn restart_hugo_child(
    child: &mut std::process::Child,
    target: &WebTarget,
    cfg: &WebServeConfig,
) -> Result<()> {
    if let Err(e) = child.kill() {
        tracing::warn!(error = %e, "failed to stop Hugo before refresh restart");
    }
    let _ = child.wait();
    *child = crate::web::spawn_hugo_server(&target.repo_root, &cfg.bind, cfg.port, cfg.drafts)?;
    Ok(())
}

async fn drain_refresh_events(
    rx: &mut mpsc::Receiver<String>,
    selected_wiki: &str,
    debounce: Duration,
) -> bool {
    let mut refresh_selected = false;
    let deadline = tokio::time::sleep(debounce);
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            value = rx.recv() => match value {
                Some(wiki) => {
                    if wiki == selected_wiki {
                        refresh_selected = true;
                    }
                }
                None => break,
            },
            _ = &mut deadline => break,
        }
    }

    refresh_selected
}

async fn run_web_supervisor(
    target: WebTarget,
    cfg: WebServeConfig,
    mut child: std::process::Child,
    mut rx: mpsc::Receiver<String>,
    cancel: CancellationToken,
) {
    let mut health = tokio::time::interval(Duration::from_secs(5));
    let refresh_debounce = Duration::from_millis(750);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            maybe_wiki = rx.recv() => {
                let Some(wiki) = maybe_wiki else {
                    break;
                };
                let mut refresh_selected = wiki == target.wiki_name;
                refresh_selected |= drain_refresh_events(&mut rx, &target.wiki_name, refresh_debounce).await;
                if refresh_selected {
                    if let Err(e) = prepare_web_target(&target)
                        .and_then(|_| restart_hugo_child(&mut child, &target, &cfg))
                    {
                        tracing::error!(wiki = %target.wiki_name, error = %e, "managed Hugo refresh failed");
                    } else {
                        tracing::info!(wiki = %target.wiki_name, "managed Hugo refreshed");
                    }
                }
            }
            _ = health.tick() => {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        tracing::warn!(wiki = %target.wiki_name, %status, "Hugo exited, restarting");
                        match crate::web::spawn_hugo_server(&target.repo_root, &cfg.bind, cfg.port, cfg.drafts) {
                            Ok(next) => child = next,
                            Err(e) => tracing::error!(wiki = %target.wiki_name, error = %e, "failed to restart Hugo"),
                        }
                    }
                    Ok(None) => {}
                    Err(e) => tracing::warn!(wiki = %target.wiki_name, error = %e, "failed to check Hugo status"),
                }
            }
        }
    }

    if let Err(e) = child.kill() {
        tracing::warn!(error = %e, "failed to stop managed Hugo");
    }
    let _ = child.wait();
    tracing::info!(wiki = %target.wiki_name, "managed Hugo stopped");
}

// ── serve (orchestration) ─────────────────────────────────────────────────────

/// Start the wiki server — spawns stdio, HTTP, ACP, and watcher transports as configured.
pub async fn serve(
    config_path: &std::path::Path,
    http_port: Option<u16>,
    acp: bool,
    watch: bool,
    web: Option<WebServeConfig>,
) -> Result<()> {
    // 1. Build WikiEngine
    let manager = Arc::new(WikiEngine::build(config_path)?);

    let (wiki_count, serve_cfg, http_enabled, resolved_port) = {
        let engine = manager.state.read();
        let count = engine.spaces.len();
        let cfg = engine.config.serve.clone();
        let http = http_port.is_some() || cfg.http;
        let port = http_port.unwrap_or(cfg.http_port);
        (count, cfg, http, port)
    };

    // 2. Log startup summary
    let mut transports = vec!["stdio".to_string()];
    if http_enabled {
        transports.push(format!("http :{resolved_port}"));
    }
    if acp {
        transports.push("acp".to_string());
    }
    if watch {
        transports.push("watch".to_string());
    }
    if web.is_some() {
        transports.push("web".to_string());
    }
    tracing::info!(
        wikis = wiki_count,
        transports = %transports.join("] ["),
        "server started",
    );

    // 3. Shutdown coordination
    let cancel = CancellationToken::new();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // ctrl_c handler
    let cancel_for_signal = cancel.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("shutdown signal received");
        cancel_for_signal.cancel();
        let _ = shutdown_tx.send(true);
    });

    // 4. Build MCP server and optional managed web-refresh channel.
    let (web_refresh_tx, web_refresh_rx) = if web.is_some() {
        let (tx, rx) = mpsc::channel::<String>(128);
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };
    let mcp_server = if let Some(tx) = web_refresh_tx.clone() {
        McpServer::with_web_refresh(manager.clone(), tx)
    } else {
        McpServer::new(manager.clone())
    };

    // 5. Heartbeat task
    if serve_cfg.heartbeat_secs > 0 {
        let interval_secs = serve_cfg.heartbeat_secs;
        let cancel_hb = cancel.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(interval_secs as u64));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        tracing::debug!("heartbeat");
                    }
                    _ = cancel_hb.cancelled() => {
                        break;
                    }
                }
            }
        });
    }

    // 6. Start watcher (if enabled)
    // Push channel: watcher sends (wiki_name, message) to ACP sessions.
    // The original sender is dropped after spawning the watcher; only the watcher clone remains.
    let (push_tx, push_rx) = tokio::sync::mpsc::channel::<(String, String)>(64);

    let watch_handle = if watch {
        let watch_manager = manager.clone();
        let cancel_watch = cancel.clone();
        let debounce = {
            let engine = manager.state.read();
            engine.config.watch.debounce_ms
        };
        let push_tx_watch = push_tx;
        let web_refresh_tx_watch = web_refresh_tx.clone();
        Some(tokio::spawn(async move {
            let max_retries: u32 = 5;
            let mut backoff = Duration::from_secs(1);
            let max_backoff = Duration::from_secs(30);
            let hung_timeout = Duration::from_secs(300);
            for attempt in 1..=max_retries {
                let result = tokio::time::timeout(
                    hung_timeout,
                    crate::watch::run_watcher(
                        watch_manager.clone(),
                        debounce,
                        cancel_watch.clone(),
                        push_tx_watch.clone(),
                        web_refresh_tx_watch.clone(),
                    ),
                )
                .await;
                match result {
                    Ok(Ok(())) => {
                        tracing::info!("watcher exited cleanly, restarting");
                    }
                    Ok(Err(e)) => {
                        tracing::error!(error = %e, attempt, "watcher error, restarting");
                    }
                    Err(_) => {
                        tracing::error!(attempt, "watcher hung (timeout), restarting");
                    }
                }
                if cancel_watch.is_cancelled() {
                    break;
                }
                if attempt < max_retries {
                    tracing::info!(
                        attempt,
                        backoff_secs = backoff.as_secs(),
                        "restarting watcher"
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(max_backoff);
                } else {
                    tracing::error!("watcher exhausted {max_retries} retries, giving up");
                }
            }
        }))
    } else {
        drop(push_tx);
        None
    };

    let web_handle = if let (Some(web_cfg), Some(rx)) = (web, web_refresh_rx) {
        let target = selected_web_target(&manager, web_cfg.wiki.as_deref())?;
        prepare_web_target(&target)?;
        tracing::info!(
            wiki = %target.wiki_name,
            bind = %web_cfg.bind,
            port = web_cfg.port,
            "starting managed Hugo web UI",
        );
        let child = crate::web::spawn_hugo_server(
            &target.repo_root,
            &web_cfg.bind,
            web_cfg.port,
            web_cfg.drafts,
        )?;
        let cancel_web = cancel.clone();
        Some(tokio::spawn(async move {
            run_web_supervisor(target, web_cfg, child, rx, cancel_web).await;
        }))
    } else {
        None
    };

    // 7. Start transports
    if acp {
        let acp_manager = manager.clone();
        let cancel_acp = cancel.clone();
        let acp_sessions: crate::acp::Sessions = Arc::new(Mutex::new(HashMap::new()));
        let acp_serve_cfg = serve_cfg.clone();

        let acp_handle = tokio::spawn(async move {
            tokio::select! {
                result = crate::acp::serve_acp(acp_manager, acp_serve_cfg, acp_sessions, push_rx) => {
                    if let Err(e) = result {
                        tracing::error!(transport = "acp", error = %e, "ACP transport error");
                    }
                }
                _ = cancel_acp.cancelled() => {
                    tracing::info!("ACP: shutdown signal received");
                }
            }
        });

        if http_enabled {
            serve_http(
                mcp_server,
                resolved_port,
                &serve_cfg,
                cancel,
                manager.clone(),
            )
            .await?;
        } else {
            serve_stdio(mcp_server, shutdown_rx).await?;
        }

        acp_handle.abort();
        let _ = acp_handle.await;
    } else if http_enabled {
        serve_http(
            mcp_server,
            resolved_port,
            &serve_cfg,
            cancel,
            manager.clone(),
        )
        .await?;
    } else {
        serve_stdio(mcp_server, shutdown_rx).await?;
    }

    if let Some(handle) = watch_handle {
        handle.abort();
        let _ = handle.await;
    }
    if let Some(handle) = web_handle {
        handle.abort();
        let _ = handle.await;
    }

    tracing::info!("server stopped");
    Ok(())
}

// ── Health endpoint ───────────────────────────────────────────────────────────

#[derive(Serialize)]
struct HealthResponse {
    uptime_secs: u64,
    wikis: Vec<WikiHealth>,
}

#[derive(Serialize)]
struct WikiHealth {
    name: String,
    index_open: bool,
    index_doc_count: u64,
}

static START_TIME: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

async fn health_handler(State(engine): State<Arc<WikiEngine>>) -> Json<HealthResponse> {
    let start = *START_TIME.get_or_init(std::time::Instant::now);
    let state = engine.state.read();
    let mut wikis = Vec::new();
    for (name, space) in &state.spaces {
        let searcher = space.index_manager.searcher().ok();
        let index_open = searcher.is_some();
        let index_doc_count = searcher.map(|s| s.num_docs()).unwrap_or(0);
        wikis.push(WikiHealth {
            name: name.clone(),
            index_open,
            index_doc_count,
        });
    }
    Json(HealthResponse {
        uptime_secs: start.elapsed().as_secs(),
        wikis,
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::config::ServeConfig;

    use super::mcp_session_config;

    #[test]
    fn mcp_session_config_uses_long_default_keep_alive() {
        let cfg = ServeConfig::default();
        let session = mcp_session_config(&cfg);

        assert_eq!(session.keep_alive, Some(Duration::from_secs(21_600)));
        assert_eq!(session.init_timeout, Some(Duration::from_secs(60)));
        assert_eq!(session.completed_cache_ttl, Duration::from_secs(60));
        assert!(!cfg.mcp_stateful_mode);
        assert!(cfg.mcp_json_response);
    }

    #[test]
    fn mcp_session_config_zero_disables_optional_timeouts() {
        let cfg = ServeConfig {
            mcp_session_keep_alive_secs: 0,
            mcp_init_timeout_secs: 0,
            mcp_completed_cache_ttl_secs: 5,
            ..Default::default()
        };
        let session = mcp_session_config(&cfg);

        assert_eq!(session.keep_alive, None);
        assert_eq!(session.init_timeout, None);
        assert_eq!(session.completed_cache_ttl, Duration::from_secs(5));
    }
}
