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

use crate::config::{ConfigManager, ProxyConfig};
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
    pub config: ConfigManager,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            config: ConfigManager::new(),
        }
    }
}

pub async fn serve(port: u16) -> anyhow::Result<()> {
    let state = Arc::new(AppState::new());

    let admin_routes = Router::new()
        .route("/proxies", get(list_proxies).post(create_proxy))
        .route("/proxies/{id}", get(get_proxy).post(update_proxy).delete(delete_proxy))
        .layer(middleware::from_fn(admin_middleware));

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/models", get(list_models))
        .route("/v1/models/{model}", get(get_model))
        .merge(admin_routes)
        .layer(middleware::from_fn(auth_middleware))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Pylon gateway listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> &'static str {
    "OK"
}

async fn chat_completions(
    State(state): State<Arc<AppState>>,
    Extension(_claims): Extension<Claims>,
    body: String,
) -> Result<Response> {
    let req: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| PylonError::InvalidRequest(e.to_string()))?;
    
    let model = req.get("model")
        .and_then(|m| m.as_str())
        .ok_or_else(|| PylonError::InvalidRequest("Missing model field".to_string()))?;
    
    let proxy = state.config.get(model).await
        .ok_or_else(|| PylonError::ProxyNotFound(model.to_string()))?;
    
    let transformed = proxy.transform_request(req);
    
    let upstream_url = format!("{}/v1/chat/completions", proxy.upstream);
    
    let is_stream = transformed.get("stream")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);
    
    let mut request = state.client
        .post(&upstream_url)
        .json(&transformed)
        .header("Authorization", format!("Bearer {}", proxy.api_key));
    
    for (key, value) in &proxy.options.extra_headers {
        request = request.header(key, value);
    }
    
    let response = request
        .send()
        .await
        .map_err(|e| PylonError::UpstreamError(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(PylonError::UpstreamError(format!("{}: {}", status, body)));
    }

    if is_stream {
        Ok(stream_response(response).await.into_response())
    } else {
        let body = response.text().await
            .map_err(|e| PylonError::UpstreamError(e.to_string()))?;
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap())
    }
}

async fn list_models(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let models = state.config.list_models().await;
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
    let proxy = state.config.get(&model).await
        .ok_or_else(|| PylonError::ProxyNotFound(model))?;
    
    Ok(Json(serde_json::json!({
        "id": proxy.source_model,
        "object": "model",
        "created": 1700000000,
        "owned_by": "pylon"
    })))
}

async fn list_proxies(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<ProxyConfig>> {
    let proxies = state.config.list().await;
    Json(proxies)
}

async fn get_proxy(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ProxyConfig>> {
    let proxy = state.config.get(&id).await
        .ok_or_else(|| PylonError::ProxyNotFound(id))?;
    Ok(Json(proxy))
}

async fn create_proxy(
    State(state): State<Arc<AppState>>,
    Json(proxy): Json<ProxyConfig>,
) -> Result<StatusCode> {
    state.config.add(proxy).await?;
    Ok(StatusCode::CREATED)
}

async fn update_proxy(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(proxy): Json<ProxyConfig>,
) -> Result<StatusCode> {
    state.config.update(&id, proxy).await?;
    Ok(StatusCode::OK)
}

async fn delete_proxy(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    state.config.delete(&id).await?;
    Ok(StatusCode::NO_CONTENT)
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