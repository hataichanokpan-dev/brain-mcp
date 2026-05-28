use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use agent_client_protocol::schema::{
    AgentCapabilities, CancelNotification, InitializeRequest, InitializeResponse,
    ListSessionsRequest, ListSessionsResponse, LoadSessionRequest, LoadSessionResponse,
    NewSessionRequest, NewSessionResponse, PromptCapabilities, PromptRequest, PromptResponse,
    SessionCapabilities, SessionId, SessionInfo, SessionListCapabilities, StopReason,
};
use agent_client_protocol::{
    Agent, ByteStreams, Client, ConnectionTo, Dispatch, on_receive_dispatch,
    on_receive_notification, on_receive_request,
};
use anyhow::Result;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::engine::WikiEngine;

use super::graph::run_graph;
use super::helpers::{clear_active_run, resolve_wiki_name, send_text, session_cwd, touch_session};
use super::ingest::run_ingest;
use super::lint::run_lint;
use super::research::run_research;
use super::{AcpSession, Sessions, dispatch_workflow, extract_prompt_text};

/// Start the ACP (Agent Client Protocol) server on stdio.
pub async fn serve_acp(
    manager: Arc<WikiEngine>,
    config: crate::config::ServeConfig,
    sessions: Sessions,
    mut push_rx: tokio::sync::mpsc::Receiver<(String, String)>,
) -> Result<()> {
    let (cx_tx, cx_rx) = tokio::sync::watch::channel::<Option<ConnectionTo<Client>>>(None);
    let cx_tx = Arc::new(cx_tx);

    // Idle session cleanup task
    {
        let sessions = sessions.clone();
        let ttl = std::time::Duration::from_secs(config.acp_session_ttl_secs);
        tokio::spawn(async move {
            if ttl.is_zero() {
                return; // cleanup disabled
            }
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let removed = {
                    let mut s = sessions.lock();
                    let before = s.len();
                    s.retain(|_, sess| sess.last_activity.elapsed() < ttl);
                    before - s.len()
                };
                if removed > 0 {
                    tracing::info!(removed = removed, "ACP: idle sessions cleaned up");
                }
            }
        });
    }

    // Push task: block until a cx is available, then forward watcher events to idle sessions
    {
        let sessions = sessions.clone();
        tokio::spawn(async move {
            // Wait for the first Prompt to establish the connection handle
            let mut cx_rx = cx_rx;
            let cx = loop {
                if cx_rx.changed().await.is_err() {
                    return; // channel closed, server shutting down
                }
                if let Some(cx) = cx_rx.borrow().clone() {
                    break cx;
                }
            };
            while let Some((wiki_name, msg)) = push_rx.recv().await {
                // Refresh cx in case it was updated
                let cx = cx_rx.borrow().clone().unwrap_or_else(|| cx.clone());
                if let Some(s) = sessions.try_lock() {
                    for (id, sess) in s.iter() {
                        if sess.wiki.as_deref() == Some(&wiki_name) && sess.active_run.is_none() {
                            let sid = SessionId::new(id.clone());
                            let _ = send_text(&cx, &sid, &msg);
                        }
                    }
                }
            }
        });
    }

    Agent
        .builder()
        .name("llm-wiki")
        // ── Initialize ───────────────────────────────────────────────
        .on_receive_request(
            async move |req: InitializeRequest, responder, _cx| {
                responder.respond(
                    InitializeResponse::new(req.protocol_version)
                        .agent_capabilities(
                            AgentCapabilities::new()
                                .prompt_capabilities(PromptCapabilities::new())
                                .session_capabilities(
                                    SessionCapabilities::new()
                                        .list(SessionListCapabilities::new()),
                                ),
                        )
                        .agent_info(agent_client_protocol::schema::Implementation::new(
                            "llm-wiki",
                            env!("CARGO_PKG_VERSION"),
                        )),
                )
            },
            on_receive_request!(),
        )
        // ── NewSession ───────────────────────────────────────────────
        .on_receive_request(
            {
                let sessions = sessions.clone();
                let config = config.clone();
                async move |req: NewSessionRequest, responder, _cx| {
                    {
                        let sessions = sessions.lock();
                        if sessions.len() >= config.acp_max_sessions {
                            return responder.respond_with_error(
                                agent_client_protocol::schema::Error::new(
                                    i32::from(agent_client_protocol::schema::ErrorCode::InvalidParams),
                                    format!("Session limit reached (max: {})", config.acp_max_sessions),
                                ),
                            );
                        }
                    }
                    let id = uuid::Uuid::new_v4().to_string();
                    let _span =
                        tracing::info_span!("acp_new_session", session = %id).entered();
                    let wiki = req
                        .meta
                        .as_ref()
                        .and_then(|m| m.get("wiki"))
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let session = AcpSession {
                        id: id.clone(),
                        label: None,
                        wiki,
                        created_at: chrono::Utc::now().timestamp() as u64,
                        active_run: None,
                        cancelled: Arc::new(AtomicBool::new(false)),
                        last_activity: std::time::Instant::now(),
                    };
                    sessions.lock().insert(id.clone(), session);
                    tracing::info!(session = %id, "session created");
                    responder.respond(NewSessionResponse::new(SessionId::new(id)))
                }
            },
            on_receive_request!(),
        )
        // ── LoadSession ──────────────────────────────────────────────
        .on_receive_request(
            {
                let sessions = sessions.clone();
                async move |req: LoadSessionRequest, responder, _cx| {
                    let sid = req.session_id.to_string();
                    let exists = sessions
                        .lock()
                        .contains_key(&sid);
                    if exists {
                        touch_session(&sessions, &sid);
                        responder.respond(LoadSessionResponse::new())
                    } else {
                        responder.respond_with_error(
                            agent_client_protocol::schema::Error::new(
                                i32::from(agent_client_protocol::schema::ErrorCode::InvalidParams),
                                format!("session {} not found", req.session_id),
                            ),
                        )
                    }
                }
            },
            on_receive_request!(),
        )
        // ── ListSessions ─────────────────────────────────────────────
        .on_receive_request(
            {
                let mgr = manager.clone();
                let sessions = sessions.clone();
                async move |_req: ListSessionsRequest, responder, _cx| {
                    let cwd = session_cwd(&mgr);
                    let infos: Vec<SessionInfo> = sessions
                        .lock()
                        .values()
                        .map(|sess| {
                            SessionInfo::new(
                                SessionId::new(sess.id.clone()),
                                cwd.clone(),
                            )
                            .title(if sess.active_run.is_some() {
                                Some(format!(
                                    "[active] {}",
                                    sess.label.clone().unwrap_or_default()
                                ))
                            } else {
                                sess.label.clone()
                            })
                        })
                        .collect();
                    responder.respond(ListSessionsResponse::new(infos))
                }
            },
            on_receive_request!(),
        )
        // ── Prompt ───────────────────────────────────────────────────
        .on_receive_request(
            {
                let mgr = manager.clone();
                let sessions = sessions.clone();
                let cx_tx = cx_tx.clone();
                async move |req: PromptRequest, responder, cx: ConnectionTo<Client>| {
                    let _ = cx_tx.send(Some(cx.clone()));
                    let text = extract_prompt_text(&req);
                    let (workflow, query) = dispatch_workflow(&text);
                    let session_id_str = req.session_id.to_string();
                    let _span = tracing::info_span!(
                        "acp_prompt",
                        session = %session_id_str,
                        workflow = %workflow,
                    )
                    .entered();

                    let wiki_name = resolve_wiki_name(&mgr, &sessions, &req.session_id);

                    // Update last activity
                    touch_session(&sessions, &session_id_str);

                    // Reset cancellation flag for new prompt
                    {
                        let mut s = sessions.lock();
                        if let Some(sess) = s.get_mut(&session_id_str) {
                            sess.cancelled.store(false, Ordering::Relaxed);
                        }
                    }

                    // Mark active run
                    {
                        let mut s = sessions.lock();
                        if let Some(sess) = s.get_mut(&session_id_str) {
                            sess.active_run =
                                Some(format!("run-{}", chrono::Utc::now().timestamp_millis()));
                        }
                    }

                    let query_text = if query.is_empty() { &text } else { query };

                    match workflow {
                        "research" => {
                            run_research(&cx, &mgr, &sessions, &req.session_id, query_text, &wiki_name)?;
                        }
                        "lint" => {
                            run_lint(&cx, &mgr, &sessions, &req.session_id, query_text, &wiki_name)?;
                        }
                        "graph" => {
                            run_graph(&cx, &mgr, &sessions, &req.session_id, query_text, &wiki_name)?;
                        }
                        "ingest" => {
                            run_ingest(&cx, &mgr, &sessions, &req.session_id, query_text, &wiki_name)?;
                        }
                        "use" => {
                            use super::research::step_read;
                            if query.is_empty() {
                                send_text(&cx, &req.session_id, "Usage: `llm-wiki:use <slug>`")?;
                            } else {
                                step_read(&cx, &mgr, &req.session_id, "use", query, &wiki_name, true)?;
                            }
                            clear_active_run(&sessions, &session_id_str);
                        }
                        _ => {
                            let msg = if workflow != "help" {
                                format!(
                                    "Unknown workflow \"{workflow}\". Available workflows:\n\
                                     • `llm-wiki:research <query>` — search + read top result\n\
                                     • `llm-wiki:lint [rules]`      — run lint rules (comma-separated or all)\n\
                                     • `llm-wiki:graph [root-slug]` — render concept graph\n\
                                     • `llm-wiki:ingest [path]`     — ingest path (default: cwd)\n\
                                     • `llm-wiki:use <slug>`        — read full page content\n\
                                     • `llm-wiki:help`              — this message\n\
                                     • (bare prompt)                — research workflow"
                                )
                            } else {
                                "Available workflows:\n\
                                 • `llm-wiki:research <query>` — search + read top result\n\
                                 • `llm-wiki:lint [rules]`      — run lint rules (comma-separated or all)\n\
                                 • `llm-wiki:graph [root-slug]` — render concept graph\n\
                                 • `llm-wiki:ingest [path]`     — ingest path (default: cwd)\n\
                                 • `llm-wiki:use <slug>`        — read full page content\n\
                                 • `llm-wiki:help`              — this message\n\
                                 • (bare prompt)                — research workflow"
                                    .to_string()
                            };
                            send_text(&cx, &req.session_id, &msg)?;
                            clear_active_run(&sessions, &session_id_str);
                        }
                    }
                    tracing::debug!(session = %session_id_str, workflow = %workflow, "prompt complete");
                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                }
            },
            on_receive_request!(),
        )
        // ── Cancel ───────────────────────────────────────────────────
        .on_receive_notification(
            {
                let sessions = sessions.clone();
                async move |notif: CancelNotification, _cx| {
                    let id = notif.session_id.to_string();
                    {
                        let sessions = sessions.lock();
                        if let Some(sess) = sessions.get(&id) {
                            sess.cancelled.store(true, Ordering::Relaxed);
                        }
                    }
                    clear_active_run(&sessions, &id);
                    Ok(())
                }
            },
            on_receive_notification!(),
        )
        // ── Catch-all ────────────────────────────────────────────────
        .on_receive_dispatch(
            async move |msg: Dispatch, cx: ConnectionTo<Client>| {
                msg.respond_with_error(
                    agent_client_protocol::util::internal_error("not supported"),
                    cx,
                )
            },
            on_receive_dispatch!(),
        )
        .connect_to(ByteStreams::new(
            tokio::io::stdout().compat_write(),
            tokio::io::stdin().compat(),
        ))
        .await
        .map_err(|e| anyhow::anyhow!("ACP error: {e}"))
}
