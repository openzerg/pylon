use axum::{
    extract::{State, Path, Query, Form},
    routing::{get, post},
    Router,
    http::{StatusCode, header},
    response::{Html, Redirect, IntoResponse},
};
use askama::Template;
use std::sync::Arc;
use serde::Deserialize;
use jsonwebtoken::{encode, decode, Header, EncodingKey, DecodingKey, Validation, Algorithm};

use crate::proxy::{AppState, Claims, JWT_SECRET};
use crate::db::{self, Proxy};
use super::templates::{LoginTemplate, DashboardTemplate, ProxyListTemplate, ProxyFormTemplate, LogsTemplate};

#[derive(Deserialize)]
struct LoginForm {
    token: String,
}

#[derive(Deserialize)]
struct ProxyForm {
    id: String,
    source_model: String,
    target_model: String,
    upstream: String,
    api_key: String,
    default_max_tokens: Option<i32>,
    default_temperature: Option<f64>,
    default_top_p: Option<f64>,
    default_top_k: Option<i32>,
    support_streaming: Option<String>,
    support_tools: Option<String>,
    support_vision: Option<String>,
    extra_headers: Option<String>,
    extra_body: Option<String>,
}

#[derive(Deserialize)]
struct AuthorizeForm {
    agent_name: String,
    permission_level: Option<String>,
}

#[derive(Deserialize)]
struct RevokeForm {
    agent_name: String,
}

#[derive(Deserialize)]
struct LogsQuery {
    user_id: Option<String>,
    proxy_id: Option<String>,
    source_model: Option<String>,
    is_success: Option<String>,
}

pub fn web_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/ui/login", get(login_page).post(login_handler))
        .route("/ui/logout", post(logout_handler))
        .route("/ui/", get(dashboard))
        .route("/ui/proxies", get(proxy_list).post(proxy_create))
        .route("/ui/proxies/new", get(proxy_new))
        .route("/ui/proxies/{id}", get(proxy_edit).post(proxy_update))
        .route("/ui/proxies/{id}/delete", post(proxy_delete))
        .route("/ui/proxies/{id}/authorize", post(authorize_handler))
        .route("/ui/proxies/{id}/revoke", post(revoke_handler))
        .route("/ui/logs", get(logs_page))
}

async fn login_page() -> Html<String> {
    let template = LoginTemplate {
        user: String::new(),
        error: None,
    };
    Html(template.render().unwrap())
}

async fn login_handler(
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    let admin_token = std::env::var("PYLON_ADMIN_TOKEN")
        .unwrap_or_else(|_| "admin".to_string());
    
    if form.token != admin_token {
        let template = LoginTemplate {
            user: String::new(),
            error: Some("Invalid admin token".to_string()),
        };
        return (StatusCode::OK, Html(template.render().unwrap())).into_response();
    }
    
    let claims = Claims {
        iss: "pylon".to_string(),
        sub: "admin".to_string(),
        role: "admin".to_string(),
        iat: chrono::Utc::now().timestamp(),
        exp: chrono::Utc::now().timestamp() + 86400 * 7,
    };
    
    let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(&JWT_SECRET)).unwrap();
    
    (
        StatusCode::FOUND,
        [
            (header::LOCATION, "/ui/"),
            (header::SET_COOKIE, format!("pylon_token={}; Path=/; HttpOnly; Max-Age=604800", token).as_str()),
        ],
    ).into_response()
}

async fn logout_handler() -> impl IntoResponse {
    (
        StatusCode::FOUND,
        [
            (header::LOCATION, "/ui/login"),
            (header::SET_COOKIE, "pylon_token=; Path=/; HttpOnly; Max-Age=0"),
        ],
    )
}

fn get_claims_from_cookie(cookie: Option<&str>) -> Option<Claims> {
    let cookie = cookie?;
    let token = cookie
        .split(';')
        .find_map(|c| c.trim().strip_prefix("pylon_token="))?;
    
    decode::<Claims>(token, &DecodingKey::from_secret(&JWT_SECRET), &Validation::new(Algorithm::HS256))
        .ok()
        .map(|d| d.claims)
}

async fn require_auth(cookie: Option<&str>) -> Result<Claims, Redirect> {
    match get_claims_from_cookie(cookie) {
        Some(claims) if claims.role == "admin" => Ok(claims),
        _ => Err(Redirect::to("/ui/login")),
    }
}

async fn dashboard(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let cookie = headers.get(axum::http::header::COOKIE).and_then(|v| v.to_str().ok());
    
    let claims = match require_auth(cookie).await {
        Ok(c) => c,
        Err(r) => return r.into_response(),
    };
    
    let stats = state.db.get_dashboard_stats().await.unwrap_or_else(|_| db::DashboardStats {
        total_proxies: 0,
        total_requests_today: 0,
        successful_requests_today: 0,
        success_rate: 0.0,
        avg_duration_ms: 0.0,
        total_input_tokens: 0,
        total_output_tokens: 0,
    });
    
    let proxies = state.db.list_proxies().await.unwrap_or_default();
    
    let success_rate = format!("{:.1}", stats.success_rate);
    let avg_duration = format!("{:.0}", stats.avg_duration_ms);
    
    let template = DashboardTemplate {
        user: claims.sub,
        stats,
        proxies: proxies.into_iter().take(10).collect(),
        success_rate,
        avg_duration,
    };
    
    Html(template.render().unwrap()).into_response()
}

async fn proxy_list(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let cookie = headers.get(axum::http::header::COOKIE).and_then(|v| v.to_str().ok());
    
    let claims = match require_auth(cookie).await {
        Ok(c) => c,
        Err(r) => return r.into_response(),
    };
    
    let proxies = state.db.list_proxies().await.unwrap_or_default();
    
    let template = ProxyListTemplate {
        user: claims.sub,
        proxies,
    };
    
    Html(template.render().unwrap()).into_response()
}

async fn proxy_new(
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let cookie = headers.get(axum::http::header::COOKIE).and_then(|v| v.to_str().ok());
    
    let claims = match require_auth(cookie).await {
        Ok(c) => c,
        Err(r) => return r.into_response(),
    };
    
    let template = ProxyFormTemplate {
        user: claims.sub,
        proxy: None,
        permissions: vec![],
    };
    
    Html(template.render().unwrap()).into_response()
}

async fn proxy_create(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Form(form): Form<ProxyForm>,
) -> impl IntoResponse {
    let cookie = headers.get(axum::http::header::COOKIE).and_then(|v| v.to_str().ok());
    
    let _claims = match require_auth(cookie).await {
        Ok(c) => c,
        Err(r) => return r.into_response(),
    };
    
    let now = chrono::Utc::now().to_rfc3339();
    let proxy = Proxy {
        id: form.id,
        source_model: form.source_model,
        target_model: form.target_model,
        upstream: form.upstream,
        api_key: form.api_key,
        default_max_tokens: form.default_max_tokens,
        default_temperature: form.default_temperature,
        default_top_p: form.default_top_p,
        default_top_k: form.default_top_k,
        support_streaming: form.support_streaming.is_some(),
        support_tools: form.support_tools.is_some(),
        support_vision: form.support_vision.is_some(),
        extra_headers: form.extra_headers,
        extra_body: form.extra_body,
        created_at: now.clone(),
        updated_at: now,
    };
    
    let _ = state.db.create_proxy(&proxy).await;
    
    Redirect::to("/ui/proxies").into_response()
}

async fn proxy_edit(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let cookie = headers.get(axum::http::header::COOKIE).and_then(|v| v.to_str().ok());
    
    let claims = match require_auth(cookie).await {
        Ok(c) => c,
        Err(r) => return r.into_response(),
    };
    
    let proxy = state.db.get_proxy(&id).await.unwrap_or_default();
    let permissions = state.db.list_permissions(&id).await.unwrap_or_default();
    
    let template = ProxyFormTemplate {
        user: claims.sub,
        proxy,
        permissions,
    };
    
    Html(template.render().unwrap()).into_response()
}

async fn proxy_update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
    Form(form): Form<ProxyForm>,
) -> impl IntoResponse {
    let cookie = headers.get(axum::http::header::COOKIE).and_then(|v| v.to_str().ok());
    
    let _claims = match require_auth(cookie).await {
        Ok(c) => c,
        Err(r) => return r.into_response(),
    };
    
    let now = chrono::Utc::now().to_rfc3339();
    
    let existing = state.db.get_proxy(&id).await.unwrap_or_default();
    let api_key = if form.api_key.is_empty() {
        existing.as_ref().map(|p| p.api_key.clone()).unwrap_or_default()
    } else {
        form.api_key
    };
    
    let created_at = existing.as_ref().map(|p| p.created_at.clone()).unwrap_or_else(|| now.clone());
    
    let proxy = Proxy {
        id: id.clone(),
        source_model: form.source_model,
        target_model: form.target_model,
        upstream: form.upstream,
        api_key,
        default_max_tokens: form.default_max_tokens,
        default_temperature: form.default_temperature,
        default_top_p: form.default_top_p,
        default_top_k: form.default_top_k,
        support_streaming: form.support_streaming.is_some(),
        support_tools: form.support_tools.is_some(),
        support_vision: form.support_vision.is_some(),
        extra_headers: form.extra_headers,
        extra_body: form.extra_body,
        created_at,
        updated_at: now,
    };
    
    if existing.is_some() {
        let _ = state.db.update_proxy(&proxy).await;
    } else {
        let _ = state.db.create_proxy(&proxy).await;
    }
    
    Redirect::to("/ui/proxies").into_response()
}

async fn proxy_delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let cookie = headers.get(axum::http::header::COOKIE).and_then(|v| v.to_str().ok());
    
    let _claims = match require_auth(cookie).await {
        Ok(c) => c,
        Err(r) => return r.into_response(),
    };
    
    let _ = state.db.delete_proxy(&id).await;
    
    Redirect::to("/ui/proxies").into_response()
}

async fn authorize_handler(
    State(state): State<Arc<AppState>>,
    Path(proxy_id): Path<String>,
    headers: axum::http::HeaderMap,
    Form(form): Form<AuthorizeForm>,
) -> impl IntoResponse {
    let cookie = headers.get(axum::http::header::COOKIE).and_then(|v| v.to_str().ok());
    
    let claims = match require_auth(cookie).await {
        Ok(c) => c,
        Err(r) => return r.into_response(),
    };
    
    let _ = state.db.authorize(
        &proxy_id,
        &form.agent_name,
        &form.permission_level.unwrap_or_else(|| "use".to_string()),
        &claims.sub,
    ).await;
    
    Redirect::to(&format!("/ui/proxies/{}", proxy_id)).into_response()
}

async fn revoke_handler(
    State(state): State<Arc<AppState>>,
    Path(proxy_id): Path<String>,
    headers: axum::http::HeaderMap,
    Form(form): Form<RevokeForm>,
) -> impl IntoResponse {
    let cookie = headers.get(axum::http::header::COOKIE).and_then(|v| v.to_str().ok());
    
    let _claims = match require_auth(cookie).await {
        Ok(c) => c,
        Err(r) => return r.into_response(),
    };
    
    let _ = state.db.revoke(&proxy_id, &form.agent_name).await;
    
    Redirect::to(&format!("/ui/proxies/{}", proxy_id)).into_response()
}

async fn logs_page(
    State(state): State<Arc<AppState>>,
    Query(query): Query<LogsQuery>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let cookie = headers.get(axum::http::header::COOKIE).and_then(|v| v.to_str().ok());
    
    let claims = match require_auth(cookie).await {
        Ok(c) => c,
        Err(r) => return r.into_response(),
    };
    
    let params = db::LogQueryParams {
        start_date: None,
        end_date: None,
        user_id: query.user_id.clone(),
        proxy_id: query.proxy_id.clone(),
        source_model: query.source_model.clone(),
        is_success: query.is_success.as_ref().and_then(|s| s.parse().ok()),
        limit: Some(100),
        offset: None,
    };
    
    let logs = state.db.query_logs(&params).await.unwrap_or_default();
    
    let template = LogsTemplate {
        user: claims.sub,
        logs,
        filter_user_id: query.user_id.unwrap_or_default(),
        filter_proxy_id: query.proxy_id.unwrap_or_default(),
        filter_source_model: query.source_model.unwrap_or_default(),
        filter_is_success: query.is_success.unwrap_or_default(),
    };
    
    Html(template.render().unwrap()).into_response()
}