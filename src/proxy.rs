use axum::{
    routing::{any, get},
    Router,
    middleware,
    http::{Request, StatusCode},
    response::Response,
    body::Body,
};
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::env;

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

pub async fn serve(port: u16, upstream: &str) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/{*path}", any(handler))
        .layer(middleware::from_fn(auth_middleware))
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Pylon gateway listening on {}", addr);
    tracing::info!("Upstream: {}", upstream);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn handler(
    axum::extract::Path(path): axum::extract::Path<String>,
) -> impl axum::response::IntoResponse {
    tracing::info!("Request for: {}", path);
    axum::http::StatusCode::NOT_IMPLEMENTED
}