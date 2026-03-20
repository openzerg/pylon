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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub iss: String,
    pub sub: String,
    pub role: String,
    pub iat: i64,
    pub exp: i64,
}

pub fn extract_token_from_header(headers: &axum::http::HeaderMap) -> Option<String> {
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

pub fn default_max_tokens() -> u32 { 4096 }

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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue, header};

    #[test]
    fn test_jwt_secret_initialization() {
        let secret = JWT_SECRET.clone();
        assert!(!secret.is_empty());
    }

    #[test]
    fn test_claims_debug() {
        let claims = Claims {
            iss: "test".to_string(),
            sub: "user".to_string(),
            role: "admin".to_string(),
            iat: 0,
            exp: 0,
        };
        let debug_str = format!("{:?}", claims);
        assert!(debug_str.contains("Claims"));
    }

    #[test]
    fn test_extract_token_with_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer test-token-123"),
        );
        
        let token = extract_token_from_header(&headers);
        assert_eq!(token, Some("test-token-123".to_string()));
    }

    #[test]
    fn test_extract_token_without_auth_header() {
        let headers = HeaderMap::new();
        let token = extract_token_from_header(&headers);
        assert!(token.is_none());
    }

    #[test]
    fn test_extract_token_with_wrong_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic dXNlcjpwYXNz"),
        );
        
        let token = extract_token_from_header(&headers);
        assert!(token.is_none());
    }

    #[test]
    fn test_extract_token_with_empty_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer "),
        );
        
        let token = extract_token_from_header(&headers);
        assert_eq!(token, Some("".to_string()));
    }

    #[test]
    fn test_chat_request_default_max_tokens() {
        let json = r#"{"model":"test","messages":[]}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.max_tokens, 4096);
        assert_eq!(req.temperature, 0.0);
        assert!(!req.stream);
    }

    #[test]
    fn test_message_serde() {
        let msg = Message {
            role: "user".to_string(),
            content: "Hello, world!".to_string(),
        };
        
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json).unwrap();
        
        assert_eq!(parsed.role, "user");
        assert_eq!(parsed.content, "Hello, world!");
    }

    #[test]
    fn test_chat_request_serde() {
        let req = ChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![
                Message { role: "system".to_string(), content: "You are helpful".to_string() },
                Message { role: "user".to_string(), content: "Hi".to_string() },
            ],
            temperature: 0.7,
            max_tokens: 1000,
            stream: true,
        };
        
        let json = serde_json::to_string(&req).unwrap();
        let parsed: ChatRequest = serde_json::from_str(&json).unwrap();
        
        assert_eq!(parsed.model, "gpt-4");
        assert_eq!(parsed.messages.len(), 2);
        assert_eq!(parsed.temperature, 0.7);
        assert_eq!(parsed.max_tokens, 1000);
        assert!(parsed.stream);
    }

    #[test]
    fn test_chat_response_serde() {
        let resp = ChatResponse {
            id: "chat-123".to_string(),
            object: "chat.completion".to_string(),
            created: 1234567890,
            model: "gpt-4".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: "assistant".to_string(),
                    content: "Hello!".to_string(),
                },
                finish_reason: Some("stop".to_string()),
            }],
        };
        
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: ChatResponse = serde_json::from_str(&json).unwrap();
        
        assert_eq!(parsed.id, "chat-123");
        assert_eq!(parsed.choices.len(), 1);
    }

    #[test]
    fn test_model_info_serde() {
        let info = ModelInfo {
            id: "llama3.2".to_string(),
            object: "model".to_string(),
            created: 1700000000,
            owned_by: "openzerg".to_string(),
        };
        
        let json = serde_json::to_string(&info).unwrap();
        let parsed: ModelInfo = serde_json::from_str(&json).unwrap();
        
        assert_eq!(parsed.id, "llama3.2");
        assert_eq!(parsed.owned_by, "openzerg");
    }

    #[test]
    fn test_model_list_serde() {
        let list = ModelList {
            object: "list".to_string(),
            data: vec![
                ModelInfo {
                    id: "model-1".to_string(),
                    object: "model".to_string(),
                    created: 1700000000,
                    owned_by: "openzerg".to_string(),
                },
                ModelInfo {
                    id: "model-2".to_string(),
                    object: "model".to_string(),
                    created: 1700000000,
                    owned_by: "openzerg".to_string(),
                },
            ],
        };
        
        let json = serde_json::to_string(&list).unwrap();
        let parsed: ModelList = serde_json::from_str(&json).unwrap();
        
        assert_eq!(parsed.data.len(), 2);
    }

    #[tokio::test]
    async fn test_app_state() {
        let state = AppState {
            upstream: "http://localhost:11434".to_string(),
            client: Client::new(),
            models: RwLock::new(vec!["test".to_string()]),
        };
        
        let models = state.models.read().await;
        assert_eq!(models.len(), 1);
    }

    #[tokio::test]
    async fn test_app_state_models_modification() {
        let state = AppState {
            upstream: "http://localhost:11434".to_string(),
            client: Client::new(),
            models: RwLock::new(vec!["model-1".to_string()]),
        };
        
        {
            let mut models = state.models.write().await;
            models.push("model-2".to_string());
        }
        
        let models = state.models.read().await;
        assert_eq!(models.len(), 2);
    }

    #[test]
    fn test_choice_serde() {
        let choice = Choice {
            index: 0,
            message: Message {
                role: "assistant".to_string(),
                content: "Response".to_string(),
            },
            finish_reason: Some("stop".to_string()),
        };
        
        let json = serde_json::to_string(&choice).unwrap();
        let parsed: Choice = serde_json::from_str(&json).unwrap();
        
        assert_eq!(parsed.finish_reason, Some("stop".to_string()));
    }

    #[test]
    fn test_choice_null_finish_reason() {
        let json = r#"{"index":0,"message":{"role":"assistant","content":"test"},"finish_reason":null}"#;
        let choice: Choice = serde_json::from_str(json).unwrap();
        assert!(choice.finish_reason.is_none());
    }

    #[test]
    fn test_claims_serde() {
        let claims = Claims {
            iss: "test-issuer".to_string(),
            sub: "test-subject".to_string(),
            role: "admin".to_string(),
            iat: 1000,
            exp: 2000,
        };
        
        let json = serde_json::to_string(&claims).unwrap();
        let parsed: Claims = serde_json::from_str(&json).unwrap();
        
        assert_eq!(parsed.iss, "test-issuer");
        assert_eq!(parsed.sub, "test-subject");
        assert_eq!(parsed.role, "admin");
    }
}