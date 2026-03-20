mod proxy_tests {
    use axum::http::{header, HeaderMap, HeaderValue};
    use pylon::db::Proxy;
    use pylon::error::PylonError;
    use pylon::proxy::{extract_token_from_header, Claims};

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
    fn test_proxy_serialization() {
        let now = chrono::Utc::now().to_rfc3339();
        let proxy = Proxy {
            id: "test-model".to_string(),
            source_model: "test-model".to_string(),
            target_model: "gpt-4".to_string(),
            upstream: "https://api.openai.com".to_string(),
            api_key: "sk-test".to_string(),
            default_max_tokens: None,
            default_temperature: None,
            default_top_p: None,
            default_top_k: None,
            support_streaming: true,
            support_tools: false,
            support_vision: false,
            extra_headers: None,
            extra_body: None,
            created_at: now.clone(),
            updated_at: now,
        };

        let json = serde_json::to_string(&proxy).unwrap();
        let parsed: Proxy = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.source_model, "test-model");
        assert_eq!(parsed.target_model, "gpt-4");
    }

    #[test]
    fn test_pylon_error_proxy_not_found() {
        let error = PylonError::ProxyNotFound("test-model".to_string());
        let message = format!("{:?}", error);
        assert!(message.contains("ProxyNotFound"));
    }
}
