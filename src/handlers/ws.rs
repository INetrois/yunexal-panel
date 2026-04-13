use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, Path, State,
    },
    http::HeaderMap,
    response::IntoResponse,
};
use axum_extra::extract::cookie::PrivateCookieJar;
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use crate::{auth, db, docker};
use crate::state::AppState;

pub async fn console_ws(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
    Path(db_id): Path<i64>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "console", false).await {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let actor = auth::session_username(&jar).unwrap_or_default();
    let ip = auth::client_ip(&headers, addr);
    let (docker_id, db_name) = match db::get_server_info_by_db_id(&state.db, db_id).await.ok().flatten() {
        Some(v) => v,
        None => return axum::http::StatusCode::NOT_FOUND.into_response(),
    };
    let _ = db::audit_log(&state.db, &actor, "console.connect", &db_name, &format!("#{}", db_id), &ip, &auth::user_agent(&headers)).await;
    ws.on_upgrade(move |socket| handle_console_socket(socket, state, docker_id, actor, db_id, ip))
}

async fn handle_console_socket(socket: WebSocket, state: AppState, id: String, actor: String, db_id: i64, ip: String) {
    let (ws_sender, ws_receiver) = socket.split();
    let (msg_tx, msg_rx) = mpsc::unbounded_channel::<Message>();

    // ── Task 1: drain mpsc → WebSocket sender ─────────────────────────────────
    let mut drain_task = {
        let mut sender = ws_sender;
        let mut rx = msg_rx;
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if sender.send(msg).await.is_err() { break; }
            }
        })
    };

    // ── Tasks 2+3: docker attach — console output (Text) + stdin (Binary) ─────
    let (stdin_tx, stdin_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let mut attach_task = {
        let msg_tx      = msg_tx.clone();
        let docker      = state.docker.clone();
        let id_a        = id.clone();
        let mut stdin_rx = stdin_rx;
        tokio::spawn(async move {
            match docker::attach_container(&docker, &id_a).await {
                Ok((mut stream, mut sink)) => {
                    let tx = msg_tx.clone();
                    let mut out_task = tokio::spawn(async move {
                        while let Some(msg) = stream.next().await {
                            if let Ok(output) = msg {
                                if tx.send(Message::Text(output.to_string().into())).is_err() { break; }
                            }
                        }
                    });
                    let mut in_task = tokio::spawn(async move {
                        while let Some(bytes) = stdin_rx.recv().await {
                            if sink.write_all(&bytes).await.is_err() { break; }
                            let _ = sink.flush().await;
                        }
                    });
                    tokio::select! {
                        _ = (&mut out_task) => in_task.abort(),
                        _ = (&mut in_task)  => out_task.abort(),
                    }
                }
                Err(e) => {
                    let _ = msg_tx.send(Message::Text(format!("Failed to attach: {}", e).into()));
                }
            }
        })
    };

    // ── Task 4: push container stats every 1 s via Binary WebSocket frames ─────
    let mut stats_task = {
        let msg_tx  = msg_tx.clone();
        let docker  = state.docker.clone();
        let id_s    = id.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
            loop {
                interval.tick().await;
                let container = docker::get_container(&docker, &id_s).await;
                let state_str = container.as_ref()
                    .map(|c| c.state.clone())
                    .unwrap_or_else(|_| "unknown".to_string());
                let s = if state_str == "running" {
                    docker::get_container_stats_raw(&docker, &id_s).await.unwrap_or_default()
                } else {
                    docker::ContainerStatsRaw::default()
                };
                let json = serde_json::to_string(&serde_json::json!({
                    "state":     state_str,
                    "cpu":       s.cpu_usage,
                    "ram":       s.ram_usage,
                    "ram_limit": s.ram_limit,
                    "rx":        s.net_rx,
                    "tx":        s.net_tx,
                    "blk_read":  s.blk_read,
                    "blk_write": s.blk_write,
                }))
                .unwrap_or_default();
                if msg_tx.send(Message::Binary(json.into_bytes().into())).is_err() { break; }
            }
        })
    };

    // ── Task 5: receive client input → stdin_tx + audit log ───────────────────
    let mut recv_task = {
        let db           = state.db.clone();
        let mut receiver = ws_receiver;
        tokio::spawn(async move {
            while let Some(Ok(msg)) = receiver.next().await {
                match msg {
                    Message::Text(text) => {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            let short = if trimmed.len() > 200 { &trimmed[..200] } else { trimmed };
                            let _ = db::audit_log(&db, &actor, "console.command", short, &format!("#{}", db_id), &ip, "").await;
                        }
                        let _ = stdin_tx.send(text.as_bytes().to_vec());
                    }
                    Message::Binary(bytes) => {
                        let _ = stdin_tx.send(bytes.to_vec());
                    }
                    _ => {}
                }
            }
        })
    };

    tokio::select! {
        _ = (&mut drain_task)  => {}
        _ = (&mut attach_task) => {}
        _ = (&mut stats_task)  => {}
        _ = (&mut recv_task)   => {}
    }
    drain_task.abort();
    attach_task.abort();
    stats_task.abort();
    recv_task.abort();
}

// ── Stats WebSocket — streams all accessible containers' stats every 1 s ──────

pub async fn stats_ws(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let is_admin = auth::is_admin_session(&state, &jar).await;
    let uid = auth::session_user_id(&state, &jar).await;
    ws.on_upgrade(move |socket| handle_stats_socket(socket, state, is_admin, uid))
}

async fn collect_batch_stats(state: &AppState, is_admin: bool, uid: Option<i64>) -> String {
    let mut containers = match crate::docker::list_containers_fast(&state.docker).await {
        Ok(c) => c,
        Err(_) => return serde_json::json!({"ok": true, "stats": []}).to_string(),
    };
    let info_map = crate::db::get_server_info_map(&state.db).await.unwrap_or_default();
    for c in &mut containers {
        if let Some((id, name, _)) = info_map.get(&c.id) {
            c.db_id = *id;
            c.name  = name.clone();
        }
    }
    if !is_admin {
        if let Some(uid) = uid {
            let allowed = crate::db::list_accessible_container_ids(&state.db, uid).await.unwrap_or_default();
            containers.retain(|c| allowed.iter().any(|oid| oid.starts_with(&c.id) || c.id.starts_with(oid.as_str())));
        } else {
            containers.clear();
        }
    }
    let running: Vec<_> = containers.into_iter().filter(|c| c.state == "running").collect();
    let results = futures_util::future::join_all(running.iter().map(|c| {
        let docker   = state.docker.clone();
        let docker_id = c.id.clone();
        let db_id    = c.db_id;
        async move {
            match crate::docker::get_container_stats_raw(&docker, &docker_id).await {
                Ok(s) => Some(serde_json::json!({
                    "db_id":     db_id,
                    "cpu":       s.cpu_usage,
                    "ram":       s.ram_usage,
                    "ram_limit": s.ram_limit,
                })),
                Err(_) => None,
            }
        }
    })).await;
    let stats: Vec<_> = results.into_iter().flatten().collect();
    serde_json::json!({"ok": true, "stats": stats}).to_string()
}

async fn handle_stats_socket(socket: WebSocket, state: AppState, is_admin: bool, uid: Option<i64>) {
    let (mut sender, mut receiver) = socket.split();
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let json = collect_batch_stats(&state, is_admin, uid).await;
                if sender.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}
