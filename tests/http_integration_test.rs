mod http_integration_tests {
    use axum::{
        http::StatusCode,
        response::IntoResponse,
    };
    use std::sync::Arc;
    use jsonwebtoken::{encode, Header, EncodingKey};

    use pylon::proxy::{AppState, Claims};
    use pylon::error::PylonError;
    use pylon::db::Database;

    async fn create_test_state() -> Arc<AppState> {
        let db_path = format!("/tmp/pylon_http_test_{}.db", std::process::id());
        let db = Database::new_with_path(&db_path).await.unwrap();
        Arc::new(AppState::new(db))
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

    #[test]
    fn test_jwt_generation() {
        let token = create_test_jwt("admin");
        assert!(!token.is_empty());
    }
}