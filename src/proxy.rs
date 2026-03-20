use axum::{
    extract::{State, Path, Extension},
    routing::{get, post},
    Json, Router,
    middleware,
    http::{Request, StatusCode, HeaderMap},
    response::{Response, IntoResponse},
    body::Body,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use once_cell::sync::Lazy;
use std::env;
use reqwest::Client;

use crate::db::{Database, Proxy, Permission, RequestLog};
use crate::error::{PylonError, Result};
use crate::stream::stream_response;

pub static JWT_SECRET: Lazy<Vec<u8>> = Lazy::new(|| {
    env::var("JWT_SECRET")
        .unwrap_or_else(|_| "openzerg-default-secret-change-in-production".to_string())
        .into_bytes()
});

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub iss: String,
    pub sub: String,
    pub role: String,
    pub iat: i64,
    pub exp: i64,
}

impl Claims {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

pub fn extract_token_from_header(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

pub async fn auth_middleware(
    mut request: Request<Body>,
    next: middleware::Next,
) -> std::result::Result<Response, StatusCode> {
    let path = request.uri().path();
    
    if path == "/health" {
        return Ok(next.run(request).await);
    }
    
    let token = extract_token_from_header(request.headers());
    match token {
        Some(t) => {
            match decode::<Claims>(
                &t,
                &DecodingKey::from_secret(&JWT_SECRET),
                &Validation::new(Algorithm::HS256),
            ) {
                Ok(token_data) => {
                    let claims = token_data.claims;
                    request.extensions_mut().insert(claims);
                    Ok(next.run(request).await)
                }
                Err(_) => Err(StatusCode::UNAUTHORIZED),
            }
        }
        None => Err(StatusCode::UNAUTHORIZED),
    }
}

pub async fn admin_middleware(
    request: Request<Body>,
    next: middleware::Next,
) -> std::result::Result<Response, StatusCode> {
    let claims = request.extensions().get::<Claims>().cloned();
    
    match claims {
        Some(c) if c.is_admin() => Ok(next.run(request).await),
        _ => Err(StatusCode::FORBIDDEN),
    }
}

pub struct AppState {
    pub client: Client,
    pub db: Arc<Database>,
}

impl AppState {
    pub fn new(db: Database) -> Self {
        Self {
            client: Client::new(),
            db: Arc::new(db),
        }
    }
}

pub async fn serve(port: u16, grpc_port: u16) -> anyhow::Result<()> {
    let db = Database::new().await?;
    let state = Arc::new(AppState::new(db));

    let api_routes = Router::new()
        .route("/health", get(health))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/models", get(list_models))
        .route("/v1/models/{model}", get(get_model))
        .route("/v1/proxies", get(list_proxies).post(create_proxy))
        .route("/v1/proxies/{id}", get(get_proxy).post(update_proxy).delete(delete_proxy))
        .route("/v1/proxies/{id}/authorize", post(authorize_proxy))
        .route("/v1/proxies/{id}/revoke", post(revoke_proxy))
        .route("/v1/proxies/{id}/permissions", get(list_permissions))
        .route("/v1/logs", get(query_logs))
        .route("/v1/logs/stats", get(get_stats));

    let app = Router::new()
        .merge(api_routes)
        .merge(crate::web::web_routes())
        .route_layer(middleware::from_fn(auth_middleware))
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    let http_addr = SocketAddr::from(([0, 0, 0, 0], port));
    let grpc_addr = SocketAddr::from(([0, 0, 0, 0], grpc_port));

    tracing::info!("Pylon HTTP gateway listening on {}", http_addr);
    tracing::info!("Pylon gRPC server listening on {}", grpc_addr);

    let http_listener = tokio::net::TcpListener::bind(http_addr).await?;
    let grpc_listener = tokio::net::TcpListener::bind(grpc_addr).await?;

    let grpc_server = crate::grpc::PylonGrpcServer::new(state);
    let grpc_service = crate::grpc::PylonServiceServer::new(grpc_server);

    let http_task = tokio::spawn(async move {
        axum::serve(http_listener, app).await
    });

    let grpc_task = tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(grpc_service)
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(grpc_listener))
            .await
    });

    let (http_result, grpc_result) = tokio::join!(http_task, grpc_task);

    http_result??;
    grpc_result??;

    Ok(())
}

async fn health() -> &'static str {
    "OK"
}

async fn chat_completions(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
    body: String,
) -> Result<Response> {
    let start = std::time::Instant::now();
    
    let req: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| PylonError::InvalidRequest(e.to_string()))?;
    
    let model = req.get("model")
        .and_then(|m| m.as_str())
        .ok_or_else(|| PylonError::InvalidRequest("Missing model field".to_string()))?;
    
    let proxy = state.db.get_proxy_by_source_model(model).await
        .map_err(|e| PylonError::InternalError(e.to_string()))?
        .ok_or_else(|| PylonError::ProxyNotFound(model.to_string()))?;
    
    // Permission check
    if !claims.is_admin() {
        let has_permission = state.db.check_permission(&proxy.id, &claims.sub).await
            .map_err(|e| PylonError::InternalError(e.to_string()))?;
        
        if !has_permission {
            return Err(PylonError::Forbidden);
        }
    }
    
    // Transform request
    let mut transformed = req.clone();
    if let Some(obj) = transformed.as_object_mut() {
        obj.insert("model".to_string(), serde_json::Value::String(proxy.target_model.clone()));
        
        if !obj.contains_key("max_tokens") {
            if let Some(tokens) = proxy.default_max_tokens {
                obj.insert("max_tokens".to_string(), serde_json::Value::Number(tokens.into()));
            }
        }
    }
    
    let upstream_url = format!("{}/v1/chat/completions", proxy.upstream);
    
    let is_stream = transformed.get("stream")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);
    
    let mut request = state.client
        .post(&upstream_url)
        .json(&transformed)
        .header("Authorization", format!("Bearer {}", proxy.api_key));
    
    if let Some(headers) = &proxy.extra_headers {
        if let Ok(headers_map) = serde_json::from_str::<std::collections::HashMap<String, String>>(headers) {
            for (key, value) in headers_map {
                request = request.header(&key, &value);
            }
        }
    }
    
    let response = request
        .send()
        .await
        .map_err(|e| PylonError::UpstreamError(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response.text().await.unwrap_or_default();
        
        // Log failed request
        let log = RequestLog {
            id: 0,
            proxy_id: Some(proxy.id.clone()),
            user_id: claims.sub.clone(),
            user_role: claims.role.clone(),
            source_model: proxy.source_model.clone(),
            target_model: proxy.target_model.clone(),
            upstream: proxy.upstream.clone(),
            request_method: "POST".to_string(),
            request_path: "/v1/chat/completions".to_string(),
            request_headers: None,
            request_body: Some(body.clone()),
            request_messages_count: None,
            request_input_tokens: None,
            response_status: Some(status.as_u16() as i32),
            response_headers: None,
            response_body: Some(error_body.clone()),
            response_output_tokens: None,
            response_reasoning_tokens: None,
            response_total_tokens: None,
            duration_ms: Some(start.elapsed().as_millis() as i32),
            time_to_first_token_ms: None,
            is_stream,
            is_success: false,
            error_type: Some("upstream_error".to_string()),
            error_message: Some(format!("{}: {}", status, error_body)),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let _ = state.db.create_log(&log).await;
        
        return Err(PylonError::UpstreamError(format!("{}: {}", status, error_body)));
    }

    if is_stream {
        Ok(stream_response(response).await.into_response())
    } else {
        let response_body = response.text().await
            .map_err(|e| PylonError::UpstreamError(e.to_string()))?;
        
        // Log successful request
        let log = RequestLog {
            id: 0,
            proxy_id: Some(proxy.id.clone()),
            user_id: claims.sub.clone(),
            user_role: claims.role.clone(),
            source_model: proxy.source_model.clone(),
            target_model: proxy.target_model.clone(),
            upstream: proxy.upstream.clone(),
            request_method: "POST".to_string(),
            request_path: "/v1/chat/completions".to_string(),
            request_headers: None,
            request_body: Some(body),
            request_messages_count: None,
            request_input_tokens: None,
            response_status: Some(200),
            response_headers: None,
            response_body: Some(response_body.clone()),
            response_output_tokens: None,
            response_reasoning_tokens: None,
            response_total_tokens: None,
            duration_ms: Some(start.elapsed().as_millis() as i32),
            time_to_first_token_ms: None,
            is_stream,
            is_success: true,
            error_type: None,
            error_message: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let _ = state.db.create_log(&log).await;
        
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .body(Body::from(response_body))
            .unwrap())
    }
}

async fn list_models(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let models = state.db.list_models().await.unwrap_or_default();
    let data: Vec<serde_json::Value> = models.iter().map(|m| {
        serde_json::json!({
            "id": m,
            "object": "model",
            "created": 1700000000,
            "owned_by": "pylon"
        })
    }).collect();
    
    Json(serde_json::json!({
        "object": "list",
        "data": data
    }))
}

async fn get_model(
    State(state): State<Arc<AppState>>,
    Path(model): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let proxy = state.db.get_proxy_by_source_model(&model).await
        .map_err(|e| PylonError::InternalError(e.to_string()))?
        .ok_or_else(|| PylonError::ProxyNotFound(model))?;
    
    Ok(Json(serde_json::json!({
        "id": proxy.source_model,
        "object": "model",
        "created": 1700000000,
        "owned_by": "pylon"
    })))
}

// Proxy CRUD
async fn list_proxies(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Proxy>>> {
    let proxies = state.db.list_proxies().await
        .map_err(|e| PylonError::InternalError(e.to_string()))?;
    Ok(Json(proxies))
}

async fn get_proxy(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Proxy>> {
    let proxy = state.db.get_proxy(&id).await
        .map_err(|e| PylonError::InternalError(e.to_string()))?
        .ok_or_else(|| PylonError::ProxyNotFound(id))?;
    Ok(Json(proxy))
}

#[derive(Debug, Deserialize)]
struct CreateProxyRequest {
    id: String,
    source_model: String,
    target_model: String,
    upstream: String,
    api_key: String,
    default_max_tokens: Option<i32>,
    default_temperature: Option<f64>,
    default_top_p: Option<f64>,
    default_top_k: Option<i32>,
    support_streaming: Option<bool>,
    support_tools: Option<bool>,
    support_vision: Option<bool>,
    extra_headers: Option<String>,
    extra_body: Option<String>,
}

async fn create_proxy(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateProxyRequest>,
) -> Result<StatusCode> {
    let now = chrono::Utc::now().to_rfc3339();
    let proxy = Proxy {
        id: req.id,
        source_model: req.source_model,
        target_model: req.target_model,
        upstream: req.upstream,
        api_key: req.api_key,
        default_max_tokens: req.default_max_tokens,
        default_temperature: req.default_temperature,
        default_top_p: req.default_top_p,
        default_top_k: req.default_top_k,
        support_streaming: req.support_streaming.unwrap_or(true),
        support_tools: req.support_tools.unwrap_or(false),
        support_vision: req.support_vision.unwrap_or(false),
        extra_headers: req.extra_headers,
        extra_body: req.extra_body,
        created_at: now.clone(),
        updated_at: now,
    };
    
    state.db.create_proxy(&proxy).await
        .map_err(|e| PylonError::InternalError(e.to_string()))?;
    
    Ok(StatusCode::CREATED)
}

async fn update_proxy(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<CreateProxyRequest>,
) -> Result<StatusCode> {
    let now = chrono::Utc::now().to_rfc3339();
    let proxy = Proxy {
        id,
        source_model: req.source_model,
        target_model: req.target_model,
        upstream: req.upstream,
        api_key: req.api_key,
        default_max_tokens: req.default_max_tokens,
        default_temperature: req.default_temperature,
        default_top_p: req.default_top_p,
        default_top_k: req.default_top_k,
        support_streaming: req.support_streaming.unwrap_or(true),
        support_tools: req.support_tools.unwrap_or(false),
        support_vision: req.support_vision.unwrap_or(false),
        extra_headers: req.extra_headers,
        extra_body: req.extra_body,
        created_at: now.clone(),
        updated_at: now,
    };
    
    state.db.update_proxy(&proxy).await
        .map_err(|e| PylonError::InternalError(e.to_string()))?;
    
    Ok(StatusCode::OK)
}

async fn delete_proxy(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    state.db.delete_proxy(&id).await
        .map_err(|e| PylonError::InternalError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

// Permission management
#[derive(Debug, Deserialize)]
struct AuthorizeRequest {
    agent_name: String,
    permission_level: Option<String>,
}

async fn authorize_proxy(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
    Path(proxy_id): Path<String>,
    Json(req): Json<AuthorizeRequest>,
) -> Result<StatusCode> {
    state.db.authorize(
        &proxy_id,
        &req.agent_name,
        &req.permission_level.unwrap_or_else(|| "use".to_string()),
        &claims.sub,
    ).await.map_err(|e| PylonError::InternalError(e.to_string()))?;
    
    Ok(StatusCode::OK)
}

async fn revoke_proxy(
    State(state): State<Arc<AppState>>,
    Path(proxy_id): Path<String>,
    Json(req): Json<AuthorizeRequest>,
) -> Result<StatusCode> {
    state.db.revoke(&proxy_id, &req.agent_name).await
        .map_err(|e| PylonError::InternalError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_permissions(
    State(state): State<Arc<AppState>>,
    Path(proxy_id): Path<String>,
) -> Result<Json<Vec<Permission>>> {
    let permissions = state.db.list_permissions(&proxy_id).await
        .map_err(|e| PylonError::InternalError(e.to_string()))?;
    Ok(Json(permissions))
}

// Log queries
#[derive(Debug, Deserialize)]
struct LogQuery {
    start_date: Option<String>,
    end_date: Option<String>,
    user_id: Option<String>,
    proxy_id: Option<String>,
    source_model: Option<String>,
    is_success: Option<bool>,
    limit: Option<i32>,
    offset: Option<i32>,
}

async fn query_logs(
    State(state): State<Arc<AppState>>,
    query: axum::extract::Query<LogQuery>,
) -> Result<Json<Vec<RequestLog>>> {
    let params = crate::db::LogQueryParams {
        start_date: query.start_date.clone(),
        end_date: query.end_date.clone(),
        user_id: query.user_id.clone(),
        proxy_id: query.proxy_id.clone(),
        source_model: query.source_model.clone(),
        is_success: query.is_success,
        limit: query.limit,
        offset: query.offset,
    };
    
    let logs = state.db.query_logs(&params).await
        .map_err(|e| PylonError::InternalError(e.to_string()))?;
    Ok(Json(logs))
}

async fn get_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<crate::db::DashboardStats>> {
    let stats = state.db.get_dashboard_stats().await
        .map_err(|e| PylonError::InternalError(e.to_string()))?;
    Ok(Json(stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claims_is_admin() {
        let admin = Claims {
            iss: "test".to_string(),
            sub: "user".to_string(),
            role: "admin".to_string(),
            iat: 0,
            exp: 0,
        };
        assert!(admin.is_admin());
        
        let user = Claims {
            iss: "test".to_string(),
            sub: "user".to_string(),
            role: "user".to_string(),
            iat: 0,
            exp: 0,
        };
        assert!(!user.is_admin());
    }

    #[test]
    fn test_extract_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("Bearer test-token"),
        );
        
        let token = extract_token_from_header(&headers);
        assert_eq!(token, Some("test-token".to_string()));
    }
}