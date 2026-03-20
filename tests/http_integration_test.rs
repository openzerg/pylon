mod http_integration_tests {
    use axum::{
        http::StatusCode,
        response::IntoResponse,
    };
    use serde_json::json;
    use std::sync::Arc;
    use jsonwebtoken::{encode, Header, EncodingKey};

    use pylon::proxy::{AppState, Claims};
    use pylon::config::{ProxyConfig, ProxyOptions};
    use pylon::error::PylonError;

    fn create_test_state() -> Arc<AppState> {
        Arc::new(AppState::new())
    }

    fn create_test_proxy_with_id(id: &str) -> ProxyConfig {
        ProxyConfig {
            id: id.to_string(),
            source_model: id.to_string(),
            target_model: format!("target-{}", id),
            upstream: "https://api.openai.com/v1".to_string(),
            api_key: "test-key".to_string(),
            options: ProxyOptions::default(),
        }
    }

    fn create_test_jwt(role: &str) -> String {
        let claims = Claims {
            iss: "cerebrate.openzerg.local".to_string(),
            sub: "test-user".to_string(),
            role: role.to_string(),
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
    fn test_error_proxy_not_found() {
        let error = PylonError::ProxyNotFound("test-model".to_string());
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_error_unauthorized() {
        let error = PylonError::Unauthorized;
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_error_forbidden() {
        let error = PylonError::Forbidden;
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_add_proxy() {
        let state = create_test_state();
        let id = format!("test-add-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());
        let proxy = create_test_proxy_with_id(&id);
        
        state.config.add(proxy.clone()).await.unwrap();
        
        let retrieved = state.config.get(&id).await;
        assert!(retrieved.is_some());
        
        state.config.delete(&id).await.ok();
    }

    #[tokio::test]
    async fn test_get_proxy() {
        let state = create_test_state();
        let id = format!("test-get-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());
        let proxy = create_test_proxy_with_id(&id);
        
        state.config.add(proxy.clone()).await.unwrap();
        
        let retrieved = state.config.get(&id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().target_model, format!("target-{}", id));
        
        state.config.delete(&id).await.ok();
    }

    #[tokio::test]
    async fn test_delete_proxy() {
        let state = create_test_state();
        let id = format!("test-delete-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());
        let proxy = create_test_proxy_with_id(&id);
        
        state.config.add(proxy.clone()).await.unwrap();
        state.config.delete(&id).await.unwrap();
        
        let retrieved = state.config.get(&id).await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_proxy() {
        let state = create_test_state();
        let result = state.config.delete("nonexistent-proxy-xyz").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_proxy() {
        let state = create_test_state();
        let id = format!("test-update-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());
        let proxy = create_test_proxy_with_id(&id);
        
        state.config.add(proxy.clone()).await.unwrap();
        
        let mut updated = proxy.clone();
        updated.target_model = "gpt-4-turbo".to_string();
        state.config.update(&id, updated).await.unwrap();
        
        let retrieved = state.config.get(&id).await.unwrap();
        assert_eq!(retrieved.target_model, "gpt-4-turbo");
        
        state.config.delete(&id).await.ok();
    }

    #[test]
    fn test_proxy_config_serde() {
        let proxy = create_test_proxy_with_id("test-serde");
        
        let json = serde_json::to_string(&proxy).unwrap();
        let parsed: ProxyConfig = serde_json::from_str(&json).unwrap();
        
        assert_eq!(parsed.source_model, "test-serde");
        assert_eq!(parsed.target_model, "target-test-serde");
    }

    #[test]
    fn test_proxy_transform() {
        let proxy = create_test_proxy_with_id("test-transform");
        
        let body = json!({
            "model": "test-transform",
            "messages": [{"role": "user", "content": "Hello"}]
        });
        
        let transformed = proxy.transform_request(body);
        
        assert_eq!(transformed["model"], "target-test-transform");
    }

    #[test]
    fn test_proxy_transform_with_default_max_tokens() {
        let mut proxy = create_test_proxy_with_id("test-max-tokens");
        proxy.options.default_max_tokens = Some(8192);
        
        let body = json!({
            "model": "test-max-tokens",
            "messages": [{"role": "user", "content": "Hello"}]
        });
        
        let transformed = proxy.transform_request(body);
        
        assert_eq!(transformed["max_tokens"], 8192);
    }

    #[test]
    fn test_proxy_transform_does_not_override_existing_max_tokens() {
        let mut proxy = create_test_proxy_with_id("test-override");
        proxy.options.default_max_tokens = Some(8192);
        
        let body = json!({
            "model": "test-override",
            "messages": [{"role": "user", "content": "Hello"}],
            "max_tokens": 1000
        });
        
        let transformed = proxy.transform_request(body);
        
        assert_eq!(transformed["max_tokens"], 1000);
    }

    #[test]
    fn test_proxy_options_defaults() {
        let options = ProxyOptions::default();
        
        assert!(options.support_streaming);
        assert!(!options.support_tools);
        assert!(!options.support_vision);
        assert!(options.default_max_tokens.is_none());
        assert!(options.default_temperature.is_none());
    }
}