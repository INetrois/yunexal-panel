use axum::{extract::{Form, Path, Query, State}, response::{IntoResponse, Redirect}, Extension, Json};
use axum_extra::extract::cookie::PrivateCookieJar;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::{Component, Path, PathBuf}};
use tokio::process::Command;

use crate::{auth, db, docker, state::AppState};
use super::{CspNonce, templates::{render, VersionControlTemplate}};

#[derive(Debug, Serialize)]
pub struct GitStatusResponse { pub connected: bool, pub github_connected: bool, pub repo_url: String, pub branch: String, pub auto_sync: bool, pub is_repo: bool, pub current_branch: String, pub last_commit: String, pub status: String }
#[derive(Debug, Serialize)]
pub struct GitActionResponse { pub ok: bool, pub message: String, pub output: String }
#[derive(Debug, Deserialize)]
pub struct GitConnectForm { pub repo_url: String, pub branch: String, pub auto_sync: Option<String> }
#[derive(Debug, Deserialize)]
pub struct GitBranchForm { pub branch: String }
#[derive(Debug, Deserialize)]
pub struct GitCommitForm { pub message: Option<String> }
#[derive(Debug, Deserialize)]
pub struct GitHubCallback { pub code: Option<String>, pub error: Option<String> }

fn key(server_id: i64, name: &str) -> String { format!("server.{server_id}.git.{name}") }
fn user_token_key(user_id: i64) -> String { format!("user.{user_id}.github.access_token") }
fn user_login_key(user_id: i64) -> String { format!("user.{user_id}.github.login") }

fn valid_branch(v: &str) -> bool { let s = v.trim(); !s.is_empty() && !s.starts_with('-') && !s.contains(' ') && !s.contains("..") && !s.contains('~') && !s.contains('^') && !s.contains(':') && !s.contains('\\') }
fn valid_repo_url(v: &str) -> bool { let s = v.trim(); s.starts_with("https://github.com/") && s.ends_with(".git") && !s.contains(' ') }

fn safe_join(root: &Path, rel: &str) -> anyhow::Result<PathBuf> {
    let mut out = root.to_path_buf();
    for part in rel.trim_start_matches('/').split('/') {
        if part.is_empty() || part == "." { continue; }
        if part == ".." { anyhow::bail!("path traversal is not allowed"); }
        if Path::new(part).components().any(|c| matches!(c, Component::RootDir | Component::Prefix(_))) { anyhow::bail!("absolute path is not allowed"); }
        out.push(part);
    }
    Ok(out)
}

async fn volume_root(state: &AppState, db_id: i64) -> Result<(String, String, PathBuf), String> {
    let (docker_id, name) = db::get_server_info_by_db_id(&state.db, db_id).await.map_err(|e| e.to_string())?.ok_or_else(|| "Server not found".to_string())?;
    let volume_dir = docker::get_volume_dir(&state.docker, &docker_id).await.map_err(|e| e.to_string())?;
    Ok((docker_id, name, docker::volume_dir_to_path(&volume_dir)))
}

async fn current_user_id(state: &AppState, jar: &PrivateCookieJar) -> Option<i64> { auth::session_user_id(state, jar).await }
async fn github_token(state: &AppState, jar: &PrivateCookieJar) -> Option<String> { let uid = current_user_id(state, jar).await?; let t = db::get_panel_setting(&state.db, &user_token_key(uid)).await; if t.trim().is_empty() { None } else { Some(t) } }

async fn git(dir: &Path, args: &[&str], token: Option<&str>) -> GitActionResponse {
    let mut cmd = Command::new("git");
    if let Some(t) = token.filter(|s| !s.trim().is_empty()) { cmd.args(["-c", &format!("http.extraHeader=AUTHORIZATION: bearer {t}")]); }
    let out = cmd.args(args).current_dir(dir).output().await;
    match out {
        Ok(o) => { let stdout = String::from_utf8_lossy(&o.stdout).trim().to_string(); let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string(); GitActionResponse { ok: o.status.success(), message: if o.status.success() { "ok".into() } else { "git command failed".into() }, output: [stdout, stderr].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n") } }
        Err(e) => GitActionResponse { ok: false, message: "failed to start git".into(), output: e.to_string() },
    }
}

pub async fn version_control_page(State(state): State<AppState>, jar: PrivateCookieJar, Path(db_id): Path<i64>, Extension(CspNonce(nonce)): Extension<CspNonce>) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "version_control", false).await { return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response(); }
    let can_members = auth::can_access_server_permission(&state, &jar, db_id, "members", false).await;
    let can_git_write = auth::can_access_server_permission(&state, &jar, db_id, "version_control", true).await;
    let (docker_id, name, _) = match volume_root(&state, db_id).await { Ok(v) => v, Err(e) => return e.into_response() };
    match docker::get_container(&state.docker, &docker_id).await { Ok(mut c) => { c.db_id = db_id; c.name = name; render(VersionControlTemplate { id: db_id, container: c, can_members, can_git_write, active_tab: "version_control", nonce }).into_response() }, Err(e) => format!("Error: {e}").into_response() }
}

pub async fn api_git_status(State(state): State<AppState>, jar: PrivateCookieJar, Path(db_id): Path<i64>) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "version_control", false).await { return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response(); }
    let (_, _, root) = match volume_root(&state, db_id).await { Ok(v) => v, Err(e) => return (axum::http::StatusCode::BAD_REQUEST, e).into_response() };
    let repo_url = db::get_panel_setting(&state.db, &key(db_id, "repo_url")).await;
    let branch = db::get_panel_setting(&state.db, &key(db_id, "branch")).await;
    let auto_sync = db::get_panel_setting(&state.db, &key(db_id, "auto_sync")).await == "1";
    let token = github_token(&state, &jar).await;
    let is_repo = root.join(".git").exists();
    let current_branch = if is_repo { git(&root, &["branch", "--show-current"], None).await.output } else { String::new() };
    let last_commit = if is_repo { git(&root, &["log", "-1", "--pretty=%h %s"], None).await.output } else { String::new() };
    let status = if is_repo { git(&root, &["status", "--short", "--branch"], None).await.output } else { "Repository is not initialized yet.".into() };
    Json(GitStatusResponse { connected: !repo_url.is_empty(), github_connected: token.is_some(), repo_url, branch, auto_sync, is_repo, current_branch, last_commit, status }).into_response()
}

pub async fn api_git_connect(State(state): State<AppState>, jar: PrivateCookieJar, Path(db_id): Path<i64>, Form(form): Form<GitConnectForm>) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "version_control", true).await { return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response(); }
    let repo = form.repo_url.trim(); let branch = if form.branch.trim().is_empty() { "main" } else { form.branch.trim() };
    if !valid_repo_url(repo) || !valid_branch(branch) { return Json(GitActionResponse { ok: false, message: "invalid GitHub repository URL or branch".into(), output: String::new() }).into_response(); }
    let (_, _, root) = match volume_root(&state, db_id).await { Ok(v) => v, Err(e) => return Json(GitActionResponse { ok: false, message: e, output: String::new() }).into_response() };
    if let Err(e) = tokio::fs::create_dir_all(&root).await { return Json(GitActionResponse { ok: false, message: "failed to create volume directory".into(), output: e.to_string() }).into_response(); }
    let token = github_token(&state, &jar).await;
    let res = if root.join(".git").exists() { let r = git(&root, &["remote", "set-url", "origin", repo], None).await; if r.ok { git(&root, &["fetch", "origin", branch], token.as_deref()).await } else { r } } else { git(&root, &["clone", "--branch", branch, repo, "."], token.as_deref()).await };
    if res.ok { let _ = db::set_panel_setting(&state.db, &key(db_id, "repo_url"), repo).await; let _ = db::set_panel_setting(&state.db, &key(db_id, "branch"), branch).await; let _ = db::set_panel_setting(&state.db, &key(db_id, "auto_sync"), if form.auto_sync.is_some() { "1" } else { "0" }).await; }
    Json(res).into_response()
}

pub async fn api_git_pull(State(state): State<AppState>, jar: PrivateCookieJar, Path(db_id): Path<i64>) -> impl IntoResponse { git_action(state, jar, db_id, "pull").await }
pub async fn api_git_push(State(state): State<AppState>, jar: PrivateCookieJar, Path(db_id): Path<i64>) -> impl IntoResponse { git_action(state, jar, db_id, "push").await }
pub async fn api_git_sync(State(state): State<AppState>, jar: PrivateCookieJar, Path(db_id): Path<i64>) -> impl IntoResponse { git_action(state, jar, db_id, "sync").await }

async fn git_action(state: AppState, jar: PrivateCookieJar, db_id: i64, action: &str) -> axum::response::Response {
    if !auth::can_access_server_permission(&state, &jar, db_id, "version_control", true).await { return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response(); }
    let (_, _, root) = match volume_root(&state, db_id).await { Ok(v) => v, Err(e) => return Json(GitActionResponse { ok: false, message: e, output: String::new() }).into_response() };
    let branch = db::get_panel_setting(&state.db, &key(db_id, "branch")).await; let branch = if branch.is_empty() { "main" } else { &branch };
    let token = github_token(&state, &jar).await;
    let res = match action { "pull" => git(&root, &["pull", "origin", branch], token.as_deref()).await, "push" => git(&root, &["push", "origin", branch], token.as_deref()).await, _ => { let p = git(&root, &["pull", "origin", branch], token.as_deref()).await; if p.ok { let q = git(&root, &["push", "origin", branch], token.as_deref()).await; GitActionResponse { ok: q.ok, message: if q.ok { "sync complete".into() } else { q.message }, output: format!("{}\n{}", p.output, q.output) } } else { p } } };
    Json(res).into_response()
}

pub async fn api_git_checkout(State(state): State<AppState>, jar: PrivateCookieJar, Path(db_id): Path<i64>, Form(form): Form<GitBranchForm>) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "version_control", true).await { return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response(); }
    if !valid_branch(&form.branch) { return Json(GitActionResponse { ok: false, message: "invalid branch".into(), output: String::new() }).into_response(); }
    let (_, _, root) = match volume_root(&state, db_id).await { Ok(v) => v, Err(e) => return Json(GitActionResponse { ok: false, message: e, output: String::new() }).into_response() };
    let res = git(&root, &["checkout", form.branch.trim()], None).await; if res.ok { let _ = db::set_panel_setting(&state.db, &key(db_id, "branch"), form.branch.trim()).await; } Json(res).into_response()
}

pub async fn api_git_commit_all(State(state): State<AppState>, jar: PrivateCookieJar, Path(db_id): Path<i64>, Form(form): Form<GitCommitForm>) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "version_control", true).await { return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response(); }
    let (_, _, root) = match volume_root(&state, db_id).await { Ok(v) => v, Err(e) => return Json(GitActionResponse { ok: false, message: e, output: String::new() }).into_response() };
    let msg = form.message.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or("Update server files from Yunexal Panel");
    let add = git(&root, &["add", "."], None).await; if !add.ok { return Json(add).into_response(); } Json(git(&root, &["commit", "-m", msg], None).await).into_response()
}

pub async fn api_git_autosync(State(state): State<AppState>, jar: PrivateCookieJar, Path(db_id): Path<i64>) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "version_control", true).await { return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response(); }
    let new_val = if db::get_panel_setting(&state.db, &key(db_id, "auto_sync")).await == "1" { "0" } else { "1" };
    let _ = db::set_panel_setting(&state.db, &key(db_id, "auto_sync"), new_val).await;
    Json(GitActionResponse { ok: true, message: "auto-sync updated".into(), output: new_val.into() }).into_response()
}

pub async fn github_start(State(state): State<AppState>, jar: PrivateCookieJar) -> impl IntoResponse {
    if current_user_id(&state, &jar).await.is_none() { return Redirect::to("/login"); }
    let client_id = std::env::var("GITHUB_CLIENT_ID").unwrap_or_default(); let redirect = std::env::var("GITHUB_REDIRECT_URL").unwrap_or_default();
    if client_id.is_empty() || redirect.is_empty() { return Redirect::to("/"); }
    let url = format!("https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}&scope=repo%20read:user", urlencoding::encode(&client_id), urlencoding::encode(&redirect));
    Redirect::to(&url)
}

pub async fn github_callback(State(state): State<AppState>, jar: PrivateCookieJar, Query(q): Query<GitHubCallback>) -> impl IntoResponse {
    let uid = match current_user_id(&state, &jar).await { Some(v) => v, None => return Redirect::to("/login") };
    if q.error.is_some() || q.code.is_none() { return Redirect::to("/"); }
    let client_id = std::env::var("GITHUB_CLIENT_ID").unwrap_or_default(); let secret = std::env::var("GITHUB_CLIENT_SECRET").unwrap_or_default();
    let code = q.code.unwrap();
    let client = reqwest::Client::new();
    let token_res = client.post("https://github.com/login/oauth/access_token").header("Accept", "application/json").form(&HashMap::from([("client_id", client_id), ("client_secret", secret), ("code", code)])).send().await;
    if let Ok(resp) = token_res { if let Ok(v) = resp.json::<serde_json::Value>().await { if let Some(token) = v.get("access_token").and_then(|v| v.as_str()) { let _ = db::set_panel_setting(&state.db, &user_token_key(uid), token).await; let me = client.get("https://api.github.com/user").bearer_auth(token).header("User-Agent", "Yunexal-Panel").send().await; if let Ok(r) = me { if let Ok(u) = r.json::<serde_json::Value>().await { if let Some(login) = u.get("login").and_then(|v| v.as_str()) { let _ = db::set_panel_setting(&state.db, &user_login_key(uid), login).await; } } } } } }
    Redirect::to("/")
}

#[allow(dead_code)]
fn _keep_safe_join(root: &Path, rel: &str) -> anyhow::Result<PathBuf> { safe_join(root, rel) }
