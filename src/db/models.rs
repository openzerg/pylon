use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Proxy {
    pub id: String,
    pub source_model: String,
    pub target_model: String,
    pub upstream: String,
    pub api_key: String,
    pub default_max_tokens: Option<i32>,
    pub default_temperature: Option<f64>,
    pub default_top_p: Option<f64>,
    pub default_top_k: Option<i32>,
    pub support_streaming: bool,
    pub support_tools: bool,
    pub support_vision: bool,
    pub extra_headers: Option<String>,
    pub extra_body: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyOptions {
    pub default_max_tokens: Option<i32>,
    pub default_temperature: Option<f64>,
    pub default_top_p: Option<f64>,
    pub default_top_k: Option<i32>,
    pub support_streaming: bool,
    pub support_tools: bool,
    pub support_vision: bool,
    pub extra_headers: Option<String>,
    pub extra_body: Option<String>,
}

impl Default for ProxyOptions {
    fn default() -> Self {
        Self {
            default_max_tokens: None,
            default_temperature: None,
            default_top_p: None,
            default_top_k: None,
            support_streaming: true,
            support_tools: false,
            support_vision: false,
            extra_headers: None,
            extra_body: None,
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Permission {
    pub id: i64,
    pub proxy_id: String,
    pub agent_name: String,
    pub permission_level: String,
    pub granted_by: String,
    pub granted_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizeRequest {
    pub agent_name: String,
    pub permission_level: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct RequestLog {
    pub id: i64,
    pub proxy_id: Option<String>,
    pub user_id: String,
    pub user_role: String,
    pub source_model: String,
    pub target_model: String,
    pub upstream: String,
    pub request_method: String,
    pub request_path: String,
    pub request_headers: Option<String>,
    pub request_body: Option<String>,
    pub request_messages_count: Option<i32>,
    pub request_input_tokens: Option<i32>,
    pub response_status: Option<i32>,
    pub response_headers: Option<String>,
    pub response_body: Option<String>,
    pub response_output_tokens: Option<i32>,
    pub response_reasoning_tokens: Option<i32>,
    pub response_total_tokens: Option<i32>,
    pub duration_ms: Option<i32>,
    pub time_to_first_token_ms: Option<i32>,
    pub is_stream: bool,
    pub is_success: bool,
    pub error_type: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogQueryParams {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub user_id: Option<String>,
    pub proxy_id: Option<String>,
    pub source_model: Option<String>,
    pub is_success: Option<bool>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardStats {
    pub total_proxies: i64,
    pub total_requests_today: i64,
    pub successful_requests_today: i64,
    pub success_rate: f64,
    pub avg_duration_ms: f64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
}
