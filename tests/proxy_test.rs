mod proxy_tests {
    use axum::http::{header, HeaderMap, HeaderValue};
    use pylon::config::{ProxyConfig, ProxyOptions};
    use pylon::error::PylonError;
    use pylon::proxy::{extract_token_from_header, Claims};
    use serde_json::json;

    #[test]
    fn test_claims_serialization() {
        let claims = Claims {
            iss: "cerebrate".to_string(),
            sub: "user-123".to_string(),
            role: "admin".to_string(),
            iat: 1000,
            exp: 2000,
        };

        let json = serde_json::to_string(&claims).unwrap();
        let parsed: Claims = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.iss, "cerebrate");
        assert_eq!(parsed.sub, "user-123");
        assert_eq!(parsed.role, "admin");
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
    fn test_extract_token_from_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer test-token-123"),
        );

        let token = extract_token_from_header(&headers);
        assert_eq!(token, Some("test-token-123".to_string()));
    }

    #[test]
    fn test_extract_token_missing_header() {
        let headers = HeaderMap::new();
        let token = extract_token_from_header(&headers);
        assert!(token.is_none());
    }

    #[test]
    fn test_extract_token_wrong_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic dXNlcjpwYXNz"),
        );

        let token = extract_token_from_header(&headers);
        assert!(token.is_none());
    }

    #[test]
    fn test_proxy_config_serialization() {
        let config = ProxyConfig {
            id: "test-model".to_string(),
            source_model: "test-model".to_string(),
            target_model: "gpt-4".to_string(),
            upstream: "https://api.openai.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            options: ProxyOptions::default(),
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: ProxyConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.source_model, "test-model");
        assert_eq!(parsed.target_model, "gpt-4");
    }

    #[test]
    fn test_proxy_transform_request() {
        let config = ProxyConfig {
            id: "test".to_string(),
            source_model: "test".to_string(),
            target_model: "gpt-4".to_string(),
            upstream: "https://api.openai.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            options: ProxyOptions::default(),
        };

        let body = json!({
            "model": "test",
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let transformed = config.transform_request(body);
        assert_eq!(transformed["model"], "gpt-4");
    }

    #[test]
    fn test_proxy_transform_with_default_max_tokens() {
        let config = ProxyConfig {
            id: "test".to_string(),
            source_model: "test".to_string(),
            target_model: "gpt-4".to_string(),
            upstream: "https://api.openai.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            options: ProxyOptions {
                default_max_tokens: Some(8192),
                ..Default::default()
            },
        };

        let body = json!({
            "model": "test",
            "messages": []
        });

        let transformed = config.transform_request(body);
        assert_eq!(transformed["max_tokens"], 8192);
    }

    #[test]
    fn test_proxy_transform_preserves_max_tokens() {
        let config = ProxyConfig {
            id: "test".to_string(),
            source_model: "test".to_string(),
            target_model: "gpt-4".to_string(),
            upstream: "https://api.openai.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            options: ProxyOptions {
                default_max_tokens: Some(8192),
                ..Default::default()
            },
        };

        let body = json!({
            "model": "test",
            "messages": [],
            "max_tokens": 1000
        });

        let transformed = config.transform_request(body);
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
        assert!(options.default_top_p.is_none());
        assert!(options.default_top_k.is_none());
    }

    #[test]
    fn test_pylon_error_proxy_not_found() {
        let error = PylonError::ProxyNotFound("test-model".to_string());
        let message = format!("{:?}", error);
        assert!(message.contains("ProxyNotFound"));
    }
}
