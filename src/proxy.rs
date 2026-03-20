use axum::{
    routing::{any, get},
    Router,
};
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

pub async fn serve(port: u16, upstream: &str) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/*path", any(handler))
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
    method: axum::http::Method,
    headers: axum::http::HeaderMap,
    body: axum::body::Body,
) -> impl axum::response::IntoResponse {
    tracing::debug!("Proxying {} /{}", method, path);
    
    axum::http::StatusCode::NOT_IMPLEMENTED
}