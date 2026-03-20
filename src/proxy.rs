use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
    middleware,
    http::{Request, StatusCode},
    response::Response,
    body::Body,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use once_cell::sync::Lazy;
use std::env;
use reqwest::Client;

pub static JWT_SECRET: Lazy<Vec<u8>> = Lazy::new(|| {
    env::var("JWT_SECRET")
        .unwrap_or_else(|_| "openzerg-default-secret-change-in-production".to_string())
        .into_bytes()
});

#[derive(Debug, Clone, Deserialize)]
pub struct Claims {
    pub iss: String,
    pub sub: String,
    pub role: String,
    pub iat: i64,
    pub exp: i64,
}

fn extract_token_from_header(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

pub async fn auth_middleware(
    request: Request<Body>,
    next: middleware::Next,
) -> Result<Response, StatusCode> {
    if request.uri().path() == "/health" {
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
                Ok(_) => Ok(next.run(request).await),
                Err(_) => Err(StatusCode::UNAUTHORIZED),
            }
        }
        None => Err(StatusCode::UNAUTHORIZED),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default)]
    pub stream: bool,
}

fn default_max_tokens() -> u32 { 4096 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: Message,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub owned_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelList {
    pub object: String,
    pub data: Vec<ModelInfo>,
}

pub struct AppState {
    pub upstream: String,
    pub client: Client,
    pub models: RwLock<Vec<String>>,
}

pub async fn serve(port: u16, upstream: &str) -> anyhow::Result<()> {
    let state = Arc::new(AppState {
        upstream: upstream.to_string(),
        client: Client::new(),
        models: RwLock::new(vec!["llama3.2".to_string()]),
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/models", get(list_models))
        .route("/v1/models/{model}", get(get_model))
        .route("/{*path}", post(proxy_request))
        .layer(middleware::from_fn(auth_middleware))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Pylon gateway listening on {}", addr);
    tracing::info!("Upstream: {}", upstream);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> &'static str {
    "OK"
}

async fn chat_completions(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, StatusCode> {
    tracing::info!("Chat request for model: {}", req.model);
    
    let upstream_url = format!("{}/v1/chat/completions", state.upstream);
    
    let response = state.client
        .post(&upstream_url)
        .json(&req)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Upstream error: {}", e);
            StatusCode::BAD_GATEWAY
        })?;

    if !response.status().is_success() {
        tracing::error!("Upstream returned: {}", response.status());
        return Err(StatusCode::BAD_GATEWAY);
    }

    let chat_response: ChatResponse = response.json().await.map_err(|e| {
        tracing::error!("Failed to parse response: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(chat_response))
}

async fn list_models(
    State(state): State<Arc<AppState>>,
) -> Json<ModelList> {
    let models = state.models.read().await;
    let data: Vec<ModelInfo> = models.iter().map(|m| ModelInfo {
        id: m.clone(),
        object: "model".to_string(),
        created: 1700000000,
        owned_by: "openzerg".to_string(),
    }).collect();
    
    Json(ModelList {
        object: "list".to_string(),
        data,
    })
}

async fn get_model(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(model): axum::extract::Path<String>,
) -> Result<Json<ModelInfo>, StatusCode> {
    let models = state.models.read().await;
    if models.contains(&model) {
        Ok(Json(ModelInfo {
            id: model,
            object: "model".to_string(),
            created: 1700000000,
            owned_by: "openzerg".to_string(),
        }))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn proxy_request(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(path): axum::extract::Path<String>,
    body: String,
) -> Result<String, StatusCode> {
    tracing::info!("Proxying request to: {}", path);
    
    let upstream_url = format!("{}/{}", state.upstream, path);
    
    let response = state.client
        .post(&upstream_url)
        .body(body)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let text = response.text().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(text)
}