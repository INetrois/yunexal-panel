use axum::{
    extract::{ConnectInfo, Form, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect},
    Json,
};
use axum_extra::extract::cookie::{Cookie, PrivateCookieJar, SameSite};
use time::Duration as TimeDuration;
use std::net::SocketAddr;
use std::time::Instant;
use tracing::warn;
use crate::{auth, db, password};
use crate::state::AppState;
use super::templates::{render, LoginForm, LoginTemplate};

pub async fn login_page() -> impl IntoResponse {
    render(LoginTemplate { error: None })
}

pub async fn login_submit(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);

    // ── Rate-limit check ────────────────────────────────────────────────
    if state.is_login_locked(&ip) {
        warn!("Login rate-limited for IP {}", ip);
        let _ = db::audit_log(&state.db, &form.username, "auth.login_locked", "", "", &ip, &auth::user_agent(&headers)).await;
        return (StatusCode::TOO_MANY_REQUESTS, render(LoginTemplate {
            error: Some("Too many login attempts. Please try again later.".to_string()),
        }))
        .into_response();
    }

    // Look up by username or uid and verify hashed password.
    let found_user = db::find_user_by_username_or_uid(&state.db, form.username.trim())
        .await
        .ok()
        .flatten();
    let ok = found_user
        .as_ref()
        .map(|user| password::verify(&form.password, &user.password_hash))
        .unwrap_or(false);

    if ok {
        let session_username = found_user
            .as_ref()
            .map(|u| u.username.clone())
            .unwrap_or_else(|| form.username.trim().to_string());
        state.clear_login_attempts(&ip);
        let _ = db::audit_log(&state.db, &session_username, "auth.login", "", "", &ip, &auth::user_agent(&headers)).await;
        let mut cookie = Cookie::new(auth::SESSION_COOKIE, session_username);
        cookie.set_http_only(true);
        cookie.set_same_site(SameSite::Strict);
        cookie.set_secure(true);
        cookie.set_path("/");
        cookie.set_max_age(TimeDuration::days(7));
        let updated_jar = jar.add(cookie);
        (updated_jar, Redirect::to("/")).into_response()
    } else {
        let locked = state.record_failed_login(&ip);
        let _ = db::audit_log(&state.db, &form.username, "auth.login_failed", "", "", &ip, &auth::user_agent(&headers)).await;
        if locked {
            warn!("IP {} locked out after repeated failed logins", ip);
        }
        // Async: check multi-IP brute force and maybe trigger CF UAM
        let state2 = state.clone();
        tokio::spawn(async move {
            check_and_maybe_trigger_uam(state2).await;
        });
        render(LoginTemplate {
            error: Some("Invalid username or password.".to_string()),
        })
        .into_response()
    }
}

/// Counts how many distinct IPs have recent failed logins;
/// if above threshold, enables Cloudflare Under Attack Mode.
async fn check_and_maybe_trigger_uam(state: AppState) {
    if !db::get_panel_setting_bool(&state.db, "cf_uam_enabled").await { return; }
    let token = db::get_panel_setting(&state.db, "cf_api_token").await;
    let zone_id = db::get_panel_setting(&state.db, "cf_zone_id").await;
    if token.is_empty() || zone_id.is_empty() { return; }

    let threshold: usize = db::get_panel_setting(&state.db, "cf_uam_threshold").await
        .parse().unwrap_or(5);

    let now = Instant::now();
    let distinct_ips = state.login_attempts.iter()
        .filter(|e| {
            let (count, last) = e.value();
            *count >= 1 && now.duration_since(*last).as_secs() <= crate::state::LOGIN_WINDOW_SECS
        })
        .count();

    if distinct_ips < threshold { return; }

    // Already auto-triggered?
    {
        let guard = state.cf_uam_triggered_at.lock().await;
        if guard.is_some() { return; }
    }

    match crate::cloudflare::enable_under_attack(&zone_id, &token).await {
        Ok(()) => {
            let mut guard = state.cf_uam_triggered_at.lock().await;
            *guard = Some(now);
            warn!("CF Under Attack Mode auto-enabled: {} distinct IPs failing login", distinct_ips);
            let _ = db::audit_log(&state.db, "system", "panel.cf_uam_enable", "auto",
                &format!("{} distinct IPs", distinct_ips), "127.0.0.1", "system").await;
        }
        Err(e) => {
            tracing::error!("Failed to auto-enable CF UAM: {}", e);
        }
    }
}

/// Counts distinct IPs with abnormally-high request rates (L7 HTTP flood);
/// if the number of flooding IPs meets the configured minimum, enables Cloudflare UAM.
pub async fn check_l7_and_maybe_trigger_uam(state: AppState) {
    if !db::get_panel_setting_bool(&state.db, "cf_uam_enabled").await { return; }
    if !db::get_panel_setting_bool(&state.db, "cf_l7_enabled").await { return; }
    let token = db::get_panel_setting(&state.db, "cf_api_token").await;
    let zone_id = db::get_panel_setting(&state.db, "cf_zone_id").await;
    if token.is_empty() || zone_id.is_empty() { return; }

    let threshold: u32 = db::get_panel_setting(&state.db, "cf_l7_threshold")
        .await.parse().unwrap_or(200);
    let min_ips: usize = db::get_panel_setting(&state.db, "cf_l7_ips_min")
        .await.parse().unwrap_or(2);

    let attacking = state.l7_attacking_ips(threshold);
    if attacking < min_ips { return; }

    {
        let guard = state.cf_uam_triggered_at.lock().await;
        if guard.is_some() { return; }
    }

    let now = Instant::now();
    match crate::cloudflare::enable_under_attack(&zone_id, &token).await {
        Ok(()) => {
            *state.cf_uam_triggered_at.lock().await = Some(now);
            warn!("CF Under Attack Mode auto-enabled: {} IPs flooding (L7 >{}/min)", attacking, threshold);
            let _ = db::audit_log(&state.db, "system", "panel.cf_uam_enable", "auto-l7",
                &format!("{} IPs flooding >{}/min", attacking, threshold), "127.0.0.1", "system").await;
        }
        Err(e) => tracing::error!("Failed to auto-enable CF UAM (L7): {}", e),
    }
}

pub async fn logout(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let actor = auth::session_username(&jar).unwrap_or_default();
    let _ = db::audit_log(&state.db, &actor, "auth.logout", "", "", &ip, &auth::user_agent(&headers)).await;
    let updated_jar = jar.remove(Cookie::from(auth::SESSION_COOKIE));
    (updated_jar, Redirect::to("/login")).into_response()
}

#[derive(serde::Deserialize)]
pub struct ServiceLoginBody {
    pub username: Option<String>,
}

/// POST /api/auth/service-login
///
/// Issues a regular panel session cookie for service clients authenticated
/// with API key headers:
/// - Authorization: Bearer <token>
/// - X-API-Key: <token>
pub async fn api_service_login(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<ServiceLoginBody>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);

    if !auth::is_service_api_request_authorized(&state, &headers).await {
        let _ = db::audit_log(&state.db, "service", "auth.service_login_denied", "", "invalid_api_key", &ip, &auth::user_agent(&headers)).await;
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Invalid API key"})),
        )
            .into_response();
    }

    let username = body
        .username
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("root")
        .to_string();

    let user = match db::find_user_by_username_or_uid(&state.db, &username).await {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "User not found"})),
            )
                .into_response();
        }
    };

    if !auth::role_has_permission(&state, &user.role, "admin.access").await {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "User has no API access"})),
        )
            .into_response();
    }

    let mut cookie = Cookie::new(auth::SESSION_COOKIE, username.clone());
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Strict);
    cookie.set_secure(true);
    cookie.set_path("/");
    cookie.set_max_age(TimeDuration::days(7));

    let updated_jar = jar.add(cookie);
    let _ = db::audit_log(&state.db, &username, "auth.service_login", "", "third_party_api", &ip, &auth::user_agent(&headers)).await;

    (
        updated_jar,
        Json(serde_json::json!({
            "ok": true,
            "username": username,
            "expires_days": 7,
        })),
    )
        .into_response()
}
