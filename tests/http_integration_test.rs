mod http_integration_tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode, Method},
        Router,
    };
    use tower::ServiceExt;
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use reqwest::Client;
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{method, path};

    use pylon::proxy::{
        AppState, ChatRequest, Message, Claims,
        extract_token_from_header, default_max_tokens,
        serve, auth_middleware,
    };
    use axum::{
        routing::{get, post},
        Json,
        middleware,
    };
    use jsonwebtoken::{encode, Header, EncodingKey};

    fn create_test_app() -> Router {
        let state = Arc::new(AppState {
            upstream: "http://localhost:11434".to_string(),
            client: Client::new(),
            models: RwLock::new(vec!["llama3.2".to_string(), "gpt-4".to_string()]),
        });

        Router::new()
            .route("/health", get(|| async { "OK" }))
            .route("/v1/models", get(list_models_handler))
            .route("/v1/models/{model}", get(get_model_handler))
            .route("/v1/chat/completions", post(chat_handler))
            .layer(middleware::from_fn(auth_middleware))
            .with_state(state)
    }

    async fn list_models_handler(
        axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    ) -> Json<pylon::proxy::ModelList> {
        let models = state.models.read().await;
        let data: Vec<pylon::proxy::ModelInfo> = models.iter().map(|m| pylon::proxy::ModelInfo {
            id: m.clone(),
            object: "model".to_string(),
            created: 1700000000,
            owned_by: "openzerg".to_string(),
        }).collect();
        
        Json(pylon::proxy::ModelList {
            object: "list".to_string(),
            data,
        })
    }

    async fn get_model_handler(
        axum::extract::State(state): axum::extract::State<Arc<AppState>>,
        axum::extract::Path(model): axum::extract::Path<String>,
    ) -> Result<Json<pylon::proxy::ModelInfo>, StatusCode> {
        let models = state.models.read().await;
        if models.contains(&model) {
            Ok(Json(pylon::proxy::ModelInfo {
                id: model,
                object: "model".to_string(),
                created: 1700000000,
                owned_by: "openzerg".to_string(),
            }))
        } else {
            Err(StatusCode::NOT_FOUND)
        }
    }

    async fn chat_handler(
        axum::extract::State(_state): axum::extract::State<Arc<AppState>>,
        Json(_req): Json<ChatRequest>,
    ) -> Result<Json<pylon::proxy::ChatResponse>, StatusCode> {
        Ok(Json(pylon::proxy::ChatResponse {
            id: "test".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "test".to_string(),
            choices: vec![pylon::proxy::Choice {
                index: 0,
                message: Message {
                    role: "assistant".to_string(),
                    content: "Test response".to_string(),
                },
                finish_reason: Some("stop".to_string()),
            }],
        }))
    }

    fn create_test_jwt() -> String {
        let claims = Claims {
            iss: "cerebrate.openzerg.local".to_string(),
            sub: "admin".to_string(),
            role: "admin".to_string(),
            iat: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            exp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64 + 86400,
        };
        
        encode(&Header::default(), &claims, &EncodingKey::from_secret(&pylon::proxy::JWT_SECRET)).unwrap()
    }

    #[tokio::test]
    async fn test_health_no_auth_required() {
        let app = create_test_app();
        
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_models_requires_auth() {
        let app = create_test_app();
        
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/models")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_models_with_valid_jwt() {
        let app = create_test_app();
        let jwt = create_test_jwt();
        
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/models")
                    .header("Authorization", format!("Bearer {}", jwt))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
        
        assert_eq!(resp["object"], "list");
        assert!(resp["data"].as_array().unwrap().len() >= 2);
    }

    #[tokio::test]
    async fn test_get_model_found() {
        let app = create_test_app();
        let jwt = create_test_jwt();
        
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/models/llama3.2")
                    .header("Authorization", format!("Bearer {}", jwt))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
        
        assert_eq!(resp["id"], "llama3.2");
    }

    #[tokio::test]
    async fn test_get_model_not_found() {
        let app = create_test_app();
        let jwt = create_test_jwt();
        
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/models/nonexistent")
                    .header("Authorization", format!("Bearer {}", jwt))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_chat_completions_requires_auth() {
        let app = create_test_app();
        
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({
                        "model": "llama3.2",
                        "messages": [{"role": "user", "content": "Hi"}]
                    }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_chat_completions_with_jwt() {
        let app = create_test_app();
        let jwt = create_test_jwt();
        
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/chat/completions")
                    .header("Authorization", format!("Bearer {}", jwt))
                    .header("content-type", "application/json")
                    .body(Body::from(json!({
                        "model": "llama3.2",
                        "messages": [{"role": "user", "content": "Hi"}]
                    }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_invalid_jwt_rejected() {
        let app = create_test_app();
        
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/models")
                    .header("Authorization", "Bearer invalid-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_expired_jwt_rejected() {
        let app = create_test_app();
        
        let expired_jwt = {
            let claims = Claims {
                iss: "cerebrate.openzerg.local".to_string(),
                sub: "admin".to_string(),
                role: "admin".to_string(),
                iat: 0,
                exp: 1,
            };
            encode(&Header::default(), &claims, &EncodingKey::from_secret(&pylon::proxy::JWT_SECRET)).unwrap()
        };
        
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/models")
                    .header("Authorization", format!("Bearer {}", expired_jwt))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_app_state_concurrent_access() {
        let state = Arc::new(AppState {
            upstream: "http://localhost:11434".to_string(),
            client: Client::new(),
            models: RwLock::new(vec!["model-1".to_string()]),
        });
        
        let mut handles = vec![];
        
        for i in 0..10 {
            let state_clone = state.clone();
            let handle = tokio::spawn(async move {
                let mut models = state_clone.models.write().await;
                models.push(format!("model-{}", i));
            });
            handles.push(handle);
        }
        
        for handle in handles {
            handle.await.unwrap();
        }
        
        let models = state.models.read().await;
        assert_eq!(models.len(), 11);
    }
}