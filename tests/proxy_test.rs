mod proxy_tests {
    use axum::http::{header, HeaderMap, HeaderValue};
    use pylon::proxy::*;

    #[test]
    fn test_chat_request_serialization() {
        let req = ChatRequest {
            model: "llama3.2".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            temperature: 0.7,
            max_tokens: 100,
            stream: false,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("llama3.2"));
        assert!(json.contains("temperature"));

        let parsed: ChatRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.model, "llama3.2");
        assert_eq!(parsed.messages.len(), 1);
    }

    #[test]
    fn test_chat_request_defaults() {
        let json = r#"{"model":"gpt-4","messages":[]}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();

        assert_eq!(req.temperature, 0.0);
        assert_eq!(req.max_tokens, 4096);
        assert!(!req.stream);
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message {
            role: "assistant".to_string(),
            content: "Hello there!".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.role, "assistant");
        assert_eq!(parsed.content, "Hello there!");
    }

    #[test]
    fn test_chat_response() {
        let resp = ChatResponse {
            id: "chatcmpl-123".to_string(),
            object: "chat.completion".to_string(),
            created: 1234567890,
            model: "llama3.2".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: "assistant".to_string(),
                    content: "Response".to_string(),
                },
                finish_reason: Some("stop".to_string()),
            }],
        };

        let json = serde_json::to_string(&resp).unwrap();
        let parsed: ChatResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "chatcmpl-123");
        assert_eq!(parsed.choices.len(), 1);
    }

    #[test]
    fn test_choice_with_finish_reason() {
        let choice = Choice {
            index: 0,
            message: Message {
                role: "assistant".to_string(),
                content: "test".to_string(),
            },
            finish_reason: Some("length".to_string()),
        };

        let json = serde_json::to_string(&choice).unwrap();
        assert!(json.contains("length"));
    }

    #[test]
    fn test_choice_without_finish_reason() {
        let choice = Choice {
            index: 1,
            message: Message {
                role: "assistant".to_string(),
                content: "test".to_string(),
            },
            finish_reason: None,
        };

        let json = serde_json::to_string(&choice).unwrap();
        let parsed: Choice = serde_json::from_str(&json).unwrap();
        assert!(parsed.finish_reason.is_none());
    }

    #[test]
    fn test_model_info() {
        let info = ModelInfo {
            id: "llama3.2".to_string(),
            object: "model".to_string(),
            created: 1700000000,
            owned_by: "openzerg".to_string(),
        };

        let json = serde_json::to_string(&info).unwrap();
        let parsed: ModelInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "llama3.2");
        assert_eq!(parsed.owned_by, "openzerg");
    }

    #[test]
    fn test_model_list() {
        let list = ModelList {
            object: "list".to_string(),
            data: vec![
                ModelInfo {
                    id: "llama3.2".to_string(),
                    object: "model".to_string(),
                    created: 1700000000,
                    owned_by: "openzerg".to_string(),
                },
                ModelInfo {
                    id: "gpt-4".to_string(),
                    object: "model".to_string(),
                    created: 1700000000,
                    owned_by: "openzerg".to_string(),
                },
            ],
        };

        let json = serde_json::to_string(&list).unwrap();
        let parsed: ModelList = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.data.len(), 2);
    }

    #[test]
    fn test_empty_model_list() {
        let list = ModelList {
            object: "list".to_string(),
            data: vec![],
        };

        let json = serde_json::to_string(&list).unwrap();
        let parsed: ModelList = serde_json::from_str(&json).unwrap();

        assert!(parsed.data.is_empty());
    }

    #[test]
    fn test_claims_serialization() {
        let claims = Claims {
            iss: "cerebrate.openzerg.local".to_string(),
            sub: "admin".to_string(),
            role: "admin".to_string(),
            iat: 1234567890,
            exp: 1234654290,
        };

        let json = serde_json::to_string(&claims).unwrap();
        let parsed: Claims = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.iss, "cerebrate.openzerg.local");
        assert_eq!(parsed.sub, "admin");
        assert_eq!(parsed.role, "admin");
    }

    #[test]
    fn test_extract_token_valid() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer my-token"),
        );

        let token = extract_token_from_header(&headers);
        assert_eq!(token, Some("my-token".to_string()));
    }

    #[test]
    fn test_extract_token_missing() {
        let headers = HeaderMap::new();
        let token = extract_token_from_header(&headers);
        assert!(token.is_none());
    }

    #[test]
    fn test_extract_token_wrong_format() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic dXNlcjpwYXNz"),
        );

        let token = extract_token_from_header(&headers);
        assert!(token.is_none());
    }

    #[test]
    fn test_default_max_tokens() {
        assert_eq!(default_max_tokens(), 4096);
    }

    #[test]
    fn test_chat_request_multiple_messages() {
        let req = ChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: "You are helpful".to_string(),
                },
                Message {
                    role: "user".to_string(),
                    content: "Hi".to_string(),
                },
                Message {
                    role: "assistant".to_string(),
                    content: "Hello!".to_string(),
                },
                Message {
                    role: "user".to_string(),
                    content: "How are you?".to_string(),
                },
            ],
            temperature: 0.5,
            max_tokens: 200,
            stream: true,
        };

        let json = serde_json::to_string(&req).unwrap();
        let parsed: ChatRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.messages.len(), 4);
        assert!(parsed.stream);
    }
}
