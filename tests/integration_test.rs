mod integration_tests {
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use reqwest::Client;
    use pylon::proxy::{AppState, ChatRequest, Message, ModelList};
    
    fn create_test_state() -> Arc<AppState> {
        Arc::new(AppState {
            upstream: "http://localhost:11434".to_string(),
            client: Client::new(),
            models: RwLock::new(vec!["llama3.2".to_string(), "gpt-4".to_string()]),
        })
    }

    #[tokio::test]
    async fn test_app_state_creation() {
        let state = create_test_state();
        assert_eq!(state.upstream, "http://localhost:11434");
    }

    #[tokio::test]
    async fn test_models_rwlock() {
        let state = create_test_state();
        
        {
            let models = state.models.read().await;
            assert_eq!(models.len(), 2);
            assert!(models.contains(&"llama3.2".to_string()));
        }
        
        {
            let mut models = state.models.write().await;
            models.push("claude-3".to_string());
        }
        
        let models = state.models.read().await;
        assert_eq!(models.len(), 3);
    }

    #[test]
    fn test_chat_request_builder() {
        let req = ChatRequest {
            model: "test-model".to_string(),
            messages: vec![
                Message { role: "system".to_string(), content: "Be helpful".to_string() },
                Message { role: "user".to_string(), content: "Hi".to_string() },
            ],
            temperature: 0.0,
            max_tokens: 2048,
            stream: false,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "test-model");
        assert_eq!(json["temperature"], 0.0);
    }

    #[test]
    fn test_message_roles() {
        let roles = vec!["system", "user", "assistant", "tool"];
        
        for role in roles {
            let msg = Message {
                role: role.to_string(),
                content: "test".to_string(),
            };
            let json = serde_json::to_string(&msg).unwrap();
            assert!(json.contains(role));
        }
    }

    #[test]
    fn test_large_content() {
        let large_content = "x".repeat(10000);
        let msg = Message {
            role: "user".to_string(),
            content: large_content.clone(),
        };
        
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json).unwrap();
        
        assert_eq!(parsed.content.len(), 10000);
    }

    #[test]
    fn test_special_characters_in_content() {
        let content = r#"{"key": "value", "nested": {"a": 1}}"#;
        let msg = Message {
            role: "user".to_string(),
            content: content.to_string(),
        };
        
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json).unwrap();
        
        assert_eq!(parsed.content, content);
    }

    #[test]
    fn test_unicode_content() {
        let content = "你好世界 🌍 مرحبا";
        let msg = Message {
            role: "user".to_string(),
            content: content.to_string(),
        };
        
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json).unwrap();
        
        assert_eq!(parsed.content, content);
    }

    #[test]
    fn test_model_list_serialization() {
        let list = ModelList {
            object: "list".to_string(),
            data: vec![],
        };
        
        let json = serde_json::to_string(&list).unwrap();
        assert!(json.contains("list"));
        assert!(json.contains("data"));
    }

    #[tokio::test]
    async fn test_concurrent_model_access() {
        let state = create_test_state();
        let mut handles = vec![];
        
        for i in 0..10 {
            let state_clone = state.clone();
            let handle = tokio::spawn(async move {
                let models = state_clone.models.read().await;
                models.len() + i
            });
            handles.push(handle);
        }
        
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result >= 2);
        }
    }

    #[test]
    fn test_empty_messages() {
        let req = ChatRequest {
            model: "test".to_string(),
            messages: vec![],
            temperature: 0.5,
            max_tokens: 100,
            stream: false,
        };
        
        let json = serde_json::to_string(&req).unwrap();
        let parsed: ChatRequest = serde_json::from_str(&json).unwrap();
        
        assert!(parsed.messages.is_empty());
    }

    #[test]
    fn test_max_values() {
        let req = ChatRequest {
            model: "test".to_string(),
            messages: vec![],
            temperature: f32::MAX,
            max_tokens: u32::MAX,
            stream: true,
        };
        
        let json = serde_json::to_string(&req).unwrap();
        let parsed: ChatRequest = serde_json::from_str(&json).unwrap();
        
        assert_eq!(parsed.temperature, f32::MAX);
        assert_eq!(parsed.max_tokens, u32::MAX);
    }

    #[test]
    fn test_model_name_variations() {
        let model_names = vec![
            "gpt-4",
            "gpt-4-turbo",
            "claude-3-opus-20240229",
            "llama-3.2-1b-instruct",
            "model_with_underscores",
            "model-with-dashes",
            "ModelWithCamelCase",
        ];
        
        for name in model_names {
            let req = ChatRequest {
                model: name.to_string(),
                messages: vec![],
                temperature: 0.0,
                max_tokens: 100,
                stream: false,
            };
            
            let json = serde_json::to_string(&req).unwrap();
            let parsed: ChatRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.model, name);
        }
    }
}