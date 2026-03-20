mod integration_tests {
    use pylon::db::{Database, Proxy};
    
    async fn create_test_db() -> Database {
        let db_path = format!("/tmp/pylon_test_{}.db", std::process::id());
        let db = Database::new_with_path(&db_path).await.unwrap();
        db
    }

    fn create_test_proxy_with_id(id: &str) -> Proxy {
        let now = chrono::Utc::now().to_rfc3339();
        Proxy {
            id: id.to_string(),
            source_model: id.to_string(),
            target_model: format!("target-{}", id),
            upstream: "https://api.openai.com".to_string(),
            api_key: "test-key".to_string(),
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
        }
    }

    #[tokio::test]
    async fn test_create_proxy() {
        let db = create_test_db().await;
        let id = format!("create-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());
        let proxy = create_test_proxy_with_id(&id);
        
        db.create_proxy(&proxy).await.unwrap();
        
        let retrieved = db.get_proxy(&id).await.unwrap();
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_get_proxy() {
        let db = create_test_db().await;
        let id = format!("get-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());
        let proxy = create_test_proxy_with_id(&id);
        
        db.create_proxy(&proxy).await.unwrap();
        
        let retrieved = db.get_proxy(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.target_model, format!("target-{}", id));
    }

    #[tokio::test]
    async fn test_delete_proxy() {
        let db = create_test_db().await;
        let id = format!("del-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());
        let proxy = create_test_proxy_with_id(&id);
        
        db.create_proxy(&proxy).await.unwrap();
        db.delete_proxy(&id).await.unwrap();
        
        let retrieved = db.get_proxy(&id).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_list_proxies() {
        let db = create_test_db().await;
        let id1 = format!("list1-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());
        let id2 = format!("list2-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());
        
        db.create_proxy(&create_test_proxy_with_id(&id1)).await.unwrap();
        db.create_proxy(&create_test_proxy_with_id(&id2)).await.unwrap();
        
        let proxies = db.list_proxies().await.unwrap();
        assert!(proxies.len() >= 2);
    }

    #[tokio::test]
    async fn test_authorize_agent() {
        let db = create_test_db().await;
        let id = format!("auth-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());
        
        db.create_proxy(&create_test_proxy_with_id(&id)).await.unwrap();
        db.authorize(&id, "test-agent", "use", "admin").await.unwrap();
        
        let has_permission = db.check_permission(&id, "test-agent").await.unwrap();
        assert!(has_permission);
    }

    #[tokio::test]
    async fn test_revoke_agent() {
        let db = create_test_db().await;
        let id = format!("revoke-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());
        
        db.create_proxy(&create_test_proxy_with_id(&id)).await.unwrap();
        db.authorize(&id, "test-agent", "use", "admin").await.unwrap();
        db.revoke(&id, "test-agent").await.unwrap();
        
        let has_permission = db.check_permission(&id, "test-agent").await.unwrap();
        assert!(!has_permission);
    }
}