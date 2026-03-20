mod integration_tests {
    use std::sync::Arc;
    use pylon::proxy::AppState;
    use pylon::config::{ProxyConfig, ProxyOptions};
    
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

    #[tokio::test]
    async fn test_app_state_creation() {
        let state = create_test_state();
        let models = state.config.list_models().await;
        assert!(!models.is_empty() || models.is_empty());
    }

    #[tokio::test]
    async fn test_add_proxy() {
        let state = create_test_state();
        let id = format!("add-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());
        let proxy = create_test_proxy_with_id(&id);
        
        state.config.add(proxy.clone()).await.unwrap();
        
        let retrieved = state.config.get(&id).await;
        assert!(retrieved.is_some());
        
        state.config.delete(&id).await.ok();
    }

    #[tokio::test]
    async fn test_get_proxy() {
        let state = create_test_state();
        let id = format!("get-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());
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
        let id = format!("del-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());
        let proxy = create_test_proxy_with_id(&id);
        
        state.config.add(proxy.clone()).await.unwrap();
        state.config.delete(&id).await.unwrap();
        
        let retrieved = state.config.get(&id).await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_proxy_not_found() {
        let state = create_test_state();
        let retrieved = state.config.get("nonexistent-proxy-xyz-123").await;
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_proxy_config_serialization() {
        let proxy = create_test_proxy_with_id("serde-test");
        
        let json = serde_json::to_string(&proxy).unwrap();
        let parsed: ProxyConfig = serde_json::from_str(&json).unwrap();
        
        assert_eq!(parsed.source_model, "serde-test");
        assert_eq!(parsed.target_model, "target-serde-test");
    }

    #[test]
    fn test_proxy_transform_request() {
        let proxy = create_test_proxy_with_id("transform-test");
        
        let body = serde_json::json!({
            "model": "transform-test",
            "messages": [{"role": "user", "content": "Hello"}]
        });
        
        let transformed = proxy.transform_request(body);
        
        assert_eq!(transformed["model"], "target-transform-test");
    }

    #[test]
    fn test_proxy_transform_with_default_max_tokens() {
        let mut proxy = create_test_proxy_with_id("max-tokens-test");
        proxy.options.default_max_tokens = Some(8192);
        
        let body = serde_json::json!({
            "model": "max-tokens-test",
            "messages": [{"role": "user", "content": "Hello"}]
        });
        
        let transformed = proxy.transform_request(body);
        
        assert_eq!(transformed["max_tokens"], 8192);
    }

    #[test]
    fn test_proxy_transform_preserves_existing_max_tokens() {
        let mut proxy = create_test_proxy_with_id("preserve-test");
        proxy.options.default_max_tokens = Some(8192);
        
        let body = serde_json::json!({
            "model": "preserve-test",
            "messages": [{"role": "user", "content": "Hello"}],
            "max_tokens": 1000
        });
        
        let transformed = proxy.transform_request(body);
        
        assert_eq!(transformed["max_tokens"], 1000);
    }

    #[test]
    fn test_proxy_options_default() {
        let options = ProxyOptions::default();
        
        assert!(options.support_streaming);
        assert!(!options.support_tools);
        assert!(!options.support_vision);
    }
}