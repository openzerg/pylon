use std::sync::Arc;
use tonic::{Request, Response, Status};
use tokio_stream::wrappers::ReceiverStream;

use crate::proxy::{AppState, Claims, JWT_SECRET};
use crate::db::{self, Proxy, Permission, RequestLog};
use crate::grpc::generated::pylon::{
    self, 
    Empty, 
    ProxyInfo, 
    ProxyListResponse,
    CreateProxyRequest,
    UpdateProxyRequest,
    GetProxyRequest,
    DeleteProxyRequest,
    PermissionInfo,
    PermissionListResponse,
    AuthorizeAgentRequest,
    RevokeAgentRequest,
    ListPermissionsRequest,
    CheckPermissionRequest,
    CheckPermissionResponse,
    ChatCompletionRequest,
    ChatCompletionResponse,
    ChatCompletionChunk,
    ModelInfo,
    ModelListResponse,
    LogQueryRequest,
    LogInfo,
    LogListResponse,
    StatsResponse,
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};

pub struct PylonGrpcServer {
    state: Arc<AppState>,
}

impl PylonGrpcServer {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    fn validate_admin(&self, metadata: &tonic::metadata::MetadataMap) -> Result<Claims, Status> {
        let token = metadata
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| Status::unauthenticated("Missing or invalid token"))?;

        let claims = decode::<Claims>(
            token,
            &DecodingKey::from_secret(&JWT_SECRET),
            &Validation::new(Algorithm::HS256),
        )
        .map_err(|_| Status::unauthenticated("Invalid token"))?
        .claims;

        if claims.role != "admin" {
            return Err(Status::permission_denied("Admin access required"));
        }

        Ok(claims)
    }

    fn validate_token(&self, metadata: &tonic::metadata::MetadataMap) -> Result<Claims, Status> {
        let token = metadata
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| Status::unauthenticated("Missing or invalid token"))?;

        let claims = decode::<Claims>(
            token,
            &DecodingKey::from_secret(&JWT_SECRET),
            &Validation::new(Algorithm::HS256),
        )
        .map_err(|_| Status::unauthenticated("Invalid token"))?
        .claims;

        Ok(claims)
    }
}

fn proxy_to_info(proxy: &Proxy) -> ProxyInfo {
    ProxyInfo {
        id: proxy.id.clone(),
        source_model: proxy.source_model.clone(),
        target_model: proxy.target_model.clone(),
        upstream: proxy.upstream.clone(),
        support_streaming: proxy.support_streaming,
        support_tools: proxy.support_tools,
        support_vision: proxy.support_vision,
        default_max_tokens: proxy.default_max_tokens,
        default_temperature: proxy.default_temperature,
        default_top_p: proxy.default_top_p,
        default_top_k: proxy.default_top_k,
        created_at: proxy.created_at.clone(),
        updated_at: proxy.updated_at.clone(),
    }
}

fn permission_to_info(perm: &Permission) -> PermissionInfo {
    PermissionInfo {
        id: perm.id,
        proxy_id: perm.proxy_id.clone(),
        agent_name: perm.agent_name.clone(),
        permission_level: perm.permission_level.clone(),
        granted_by: perm.granted_by.clone(),
        granted_at: perm.granted_at.clone(),
    }
}

#[tonic::async_trait]
impl pylon::pylon_service_server::PylonService for PylonGrpcServer {
    async fn list_proxies(
        &self,
        request: Request<Empty>,
    ) -> Result<Response<ProxyListResponse>, Status> {
        self.validate_admin(request.metadata())?;

        let proxies = self.state.db.list_proxies().await
            .map_err(|e| Status::internal(e.to_string()))?;

        let response = ProxyListResponse {
            proxies: proxies.iter().map(proxy_to_info).collect(),
        };

        Ok(Response::new(response))
    }

    async fn get_proxy(
        &self,
        request: Request<GetProxyRequest>,
    ) -> Result<Response<ProxyInfo>, Status> {
        self.validate_admin(request.metadata())?;

        let proxy = self.state.db.get_proxy(&request.get_ref().id).await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("Proxy not found"))?;

        Ok(Response::new(proxy_to_info(&proxy)))
    }

    async fn create_proxy(
        &self,
        request: Request<CreateProxyRequest>,
    ) -> Result<Response<ProxyInfo>, Status> {
        self.validate_admin(request.metadata())?;

        let req = request.get_ref();
        let now = chrono::Utc::now().to_rfc3339();

        let proxy = Proxy {
            id: req.id.clone(),
            source_model: req.source_model.clone(),
            target_model: req.target_model.clone(),
            upstream: req.upstream.clone(),
            api_key: req.api_key.clone(),
            default_max_tokens: req.default_max_tokens,
            default_temperature: req.default_temperature,
            default_top_p: req.default_top_p,
            default_top_k: req.default_top_k,
            support_streaming: req.support_streaming.unwrap_or(true),
            support_tools: req.support_tools.unwrap_or(false),
            support_vision: req.support_vision.unwrap_or(false),
            extra_headers: req.extra_headers.clone(),
            extra_body: req.extra_body.clone(),
            created_at: now.clone(),
            updated_at: now,
        };

        self.state.db.create_proxy(&proxy).await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(proxy_to_info(&proxy)))
    }

    async fn update_proxy(
        &self,
        request: Request<UpdateProxyRequest>,
    ) -> Result<Response<ProxyInfo>, Status> {
        let claims = self.validate_admin(request.metadata())?;
        let req = request.get_ref();

        let existing = self.state.db.get_proxy(&req.id).await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("Proxy not found"))?;

        let now = chrono::Utc::now().to_rfc3339();

        let proxy = Proxy {
            id: req.id.clone(),
            source_model: req.source_model.clone(),
            target_model: req.target_model.clone(),
            upstream: req.upstream.clone(),
            api_key: req.api_key.clone().unwrap_or(existing.api_key),
            default_max_tokens: req.default_max_tokens,
            default_temperature: req.default_temperature,
            default_top_p: req.default_top_p,
            default_top_k: req.default_top_k,
            support_streaming: req.support_streaming.unwrap_or(existing.support_streaming),
            support_tools: req.support_tools.unwrap_or(existing.support_tools),
            support_vision: req.support_vision.unwrap_or(existing.support_vision),
            extra_headers: req.extra_headers.clone(),
            extra_body: req.extra_body.clone(),
            created_at: existing.created_at,
            updated_at: now,
        };

        self.state.db.update_proxy(&proxy).await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(proxy_to_info(&proxy)))
    }

    async fn delete_proxy(
        &self,
        request: Request<DeleteProxyRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.validate_admin(request.metadata())?;

        self.state.db.delete_proxy(&request.get_ref().id).await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(Empty {}))
    }

    async fn authorize_agent(
        &self,
        request: Request<AuthorizeAgentRequest>,
    ) -> Result<Response<Empty>, Status> {
        let claims = self.validate_admin(request.metadata())?;
        let req = request.get_ref();

        self.state.db.authorize(
            &req.proxy_id,
            &req.agent_name,
            &req.permission_level.clone().unwrap_or_else(|| "use".to_string()),
            &claims.sub,
        ).await.map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(Empty {}))
    }

    async fn revoke_agent(
        &self,
        request: Request<RevokeAgentRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.validate_admin(request.metadata())?;

        let req = request.get_ref();
        self.state.db.revoke(&req.proxy_id, &req.agent_name).await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(Empty {}))
    }

    async fn list_permissions(
        &self,
        request: Request<ListPermissionsRequest>,
    ) -> Result<Response<PermissionListResponse>, Status> {
        self.validate_admin(request.metadata())?;

        let permissions = self.state.db.list_permissions(&request.get_ref().proxy_id).await
            .map_err(|e| Status::internal(e.to_string()))?;

        let response = PermissionListResponse {
            permissions: permissions.iter().map(permission_to_info).collect(),
        };

        Ok(Response::new(response))
    }

    async fn check_permission(
        &self,
        request: Request<CheckPermissionRequest>,
    ) -> Result<Response<CheckPermissionResponse>, Status> {
        let claims = self.validate_token(request.metadata())?;
        let req = request.get_ref();

        let has_permission = if claims.role == "admin" {
            true
        } else {
            self.state.db.check_permission(&req.proxy_id, &req.agent_name).await
                .map_err(|e| Status::internal(e.to_string()))?
        };

        Ok(Response::new(CheckPermissionResponse { has_permission }))
    }

    async fn chat_completion(
        &self,
        request: Request<ChatCompletionRequest>,
    ) -> Result<Response<ChatCompletionResponse>, Status> {
        let claims = self.validate_token(request.metadata())?;
        let req = request.get_ref();

        let body: serde_json::Value = serde_json::from_str(&req.request_json)
            .map_err(|e| Status::invalid_argument(format!("Invalid JSON: {}", e)))?;

        let model = body.get("model")
            .and_then(|m| m.as_str())
            .ok_or_else(|| Status::invalid_argument("Missing model field"))?;

        let proxy = self.state.db.get_proxy_by_source_model(model).await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found(format!("Proxy not found for model: {}", model)))?;

        if claims.role != "admin" {
            let has_permission = self.state.db.check_permission(&proxy.id, &claims.sub).await
                .map_err(|e| Status::internal(e.to_string()))?;

            if !has_permission {
                return Err(Status::permission_denied("No permission to use this proxy"));
            }
        }

        let mut transformed = body.clone();
        if let Some(obj) = transformed.as_object_mut() {
            obj.insert("model".to_string(), serde_json::Value::String(proxy.target_model.clone()));

            if !obj.contains_key("max_tokens") {
                if let Some(tokens) = proxy.default_max_tokens {
                    obj.insert("max_tokens".to_string(), serde_json::Value::Number(tokens.into()));
                }
            }
        }

        let upstream_url = format!("{}/v1/chat/completions", proxy.upstream);

        let response = self.state.client
            .post(&upstream_url)
            .json(&transformed)
            .header("Authorization", format!("Bearer {}", proxy.api_key))
            .send()
            .await
            .map_err(|e| Status::unavailable(format!("Upstream error: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            return Ok(Response::new(ChatCompletionResponse {
                response_json: String::new(),
                success: false,
                error: Some(format!("{}: {}", status, error_body)),
            }));
        }

        let response_body = response.text().await
            .map_err(|e| Status::internal(format!("Failed to read response: {}", e)))?;

        Ok(Response::new(ChatCompletionResponse {
            response_json: response_body,
            success: true,
            error: None,
        }))
    }

    type StreamChatCompletionStream = ReceiverStream<Result<ChatCompletionChunk, Status>>;

    async fn stream_chat_completion(
        &self,
        request: Request<ChatCompletionRequest>,
    ) -> Result<Response<Self::StreamChatCompletionStream>, Status> {
        let claims = self.validate_token(request.metadata())?;
        let req = request.get_ref();

        let body: serde_json::Value = serde_json::from_str(&req.request_json)
            .map_err(|e| Status::invalid_argument(format!("Invalid JSON: {}", e)))?;

        let model = body.get("model")
            .and_then(|m| m.as_str())
            .ok_or_else(|| Status::invalid_argument("Missing model field"))?;

        let proxy = self.state.db.get_proxy_by_source_model(model).await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found(format!("Proxy not found for model: {}", model)))?;

        if claims.role != "admin" {
            let has_permission = self.state.db.check_permission(&proxy.id, &claims.sub).await
                .map_err(|e| Status::internal(e.to_string()))?;

            if !has_permission {
                return Err(Status::permission_denied("No permission to use this proxy"));
            }
        }

        let mut transformed = body.clone();
        if let Some(obj) = transformed.as_object_mut() {
            obj.insert("model".to_string(), serde_json::Value::String(proxy.target_model.clone()));
            obj.insert("stream".to_string(), serde_json::Value::Bool(true));
        }

        let upstream_url = format!("{}/v1/chat/completions", proxy.upstream);

        let response = self.state.client
            .post(&upstream_url)
            .json(&transformed)
            .header("Authorization", format!("Bearer {}", proxy.api_key))
            .send()
            .await
            .map_err(|e| Status::unavailable(format!("Upstream error: {}", e)))?;

        let (tx, rx) = tokio::sync::mpsc::channel(100);

        tokio::spawn(async move {
            use futures::StreamExt;

            let mut stream = response.bytes_stream();
            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        let chunk_str = String::from_utf8_lossy(&bytes).to_string();
                        if tx.send(Ok(ChatCompletionChunk {
                            chunk_json: chunk_str,
                            is_done: false,
                        })).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(Status::internal(e.to_string()))).await;
                        break;
                    }
                }
            }
            let _ = tx.send(Ok(ChatCompletionChunk {
                chunk_json: String::new(),
                is_done: true,
            })).await;
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn list_models(
        &self,
        request: Request<Empty>,
    ) -> Result<Response<ModelListResponse>, Status> {
        self.validate_token(request.metadata())?;

        let proxies = self.state.db.list_proxies().await
            .map_err(|e| Status::internal(e.to_string()))?;

        let models: Vec<ModelInfo> = proxies.iter().map(|p| ModelInfo {
            id: p.id.clone(),
            source_model: p.source_model.clone(),
            target_model: p.target_model.clone(),
            upstream: p.upstream.clone(),
        }).collect();

        Ok(Response::new(ModelListResponse { models }))
    }

    async fn query_logs(
        &self,
        request: Request<LogQueryRequest>,
    ) -> Result<Response<LogListResponse>, Status> {
        self.validate_admin(request.metadata())?;

        let req = request.get_ref();
        let params = db::LogQueryParams {
            start_date: req.start_date.clone(),
            end_date: req.end_date.clone(),
            user_id: req.user_id.clone(),
            proxy_id: req.proxy_id.clone(),
            source_model: req.source_model.clone(),
            is_success: req.is_success,
            limit: req.limit,
            offset: req.offset,
        };

        let logs = self.state.db.query_logs(&params).await
            .map_err(|e| Status::internal(e.to_string()))?;

        let log_infos: Vec<LogInfo> = logs.iter().map(|l| LogInfo {
            id: l.id,
            proxy_id: l.proxy_id.clone(),
            user_id: l.user_id.clone(),
            user_role: l.user_role.clone(),
            source_model: l.source_model.clone(),
            target_model: l.target_model.clone(),
            upstream: l.upstream.clone(),
            request_method: l.request_method.clone(),
            request_path: l.request_path.clone(),
            request_body: l.request_body.clone(),
            response_status: l.response_status,
            response_body: l.response_body.clone(),
            duration_ms: l.duration_ms,
            is_stream: l.is_stream,
            is_success: l.is_success,
            error_type: l.error_type.clone(),
            error_message: l.error_message.clone(),
            created_at: l.created_at.clone(),
        }).collect();

        let total = log_infos.len() as i64;

        Ok(Response::new(LogListResponse { logs: log_infos, total }))
    }

    async fn get_stats(
        &self,
        request: Request<Empty>,
    ) -> Result<Response<StatsResponse>, Status> {
        self.validate_admin(request.metadata())?;

        let stats = self.state.db.get_dashboard_stats().await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(StatsResponse {
            total_proxies: stats.total_proxies,
            total_requests_today: stats.total_requests_today,
            successful_requests_today: stats.successful_requests_today,
            success_rate: stats.success_rate,
            avg_duration_ms: stats.avg_duration_ms,
            total_input_tokens: stats.total_input_tokens,
            total_output_tokens: stats.total_output_tokens,
        }))
    }
}