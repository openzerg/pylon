pub mod models;

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::path::PathBuf;

pub use models::*;

const SCHEMA: &str = include_str!("schema.sql");

pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn new() -> Result<Self, sqlx::Error> {
        let db_path = Self::get_db_path();
        Self::new_with_path(&db_path.display().to_string()).await
    }

    pub async fn new_with_path(path: &str) -> Result<Self, sqlx::Error> {
        let db_path = PathBuf::from(path);
        
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        
        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
        
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await?;
        
        let db = Self { pool };
        db.init_schema().await?;
        
        Ok(db)
    }

    fn get_db_path() -> PathBuf {
        if let Ok(path) = std::env::var("PYLON_DB_PATH") {
            PathBuf::from(path)
        } else if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".pylon").join("pylon.db")
        } else {
            PathBuf::from("/root/.pylon/pylon.db")
        }
    }

    async fn init_schema(&self) -> Result<(), sqlx::Error> {
        for statement in SCHEMA.split(';') {
            let statement = statement.trim();
            if !statement.is_empty() {
                sqlx::query(statement)
                    .execute(&self.pool)
                    .await?;
            }
        }
        Ok(())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    // Proxy operations
    pub async fn list_proxies(&self) -> Result<Vec<Proxy>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM proxies ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await
    }

    pub async fn get_proxy(&self, id: &str) -> Result<Option<Proxy>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM proxies WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn get_proxy_by_source_model(&self, source_model: &str) -> Result<Option<Proxy>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM proxies WHERE source_model = ?")
            .bind(source_model)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn create_proxy(&self, proxy: &Proxy) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO proxies (
                id, source_model, target_model, upstream, api_key,
                default_max_tokens, default_temperature, default_top_p, default_top_k,
                support_streaming, support_tools, support_vision,
                extra_headers, extra_body, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&proxy.id)
        .bind(&proxy.source_model)
        .bind(&proxy.target_model)
        .bind(&proxy.upstream)
        .bind(&proxy.api_key)
        .bind(proxy.default_max_tokens)
        .bind(proxy.default_temperature)
        .bind(proxy.default_top_p)
        .bind(proxy.default_top_k)
        .bind(proxy.support_streaming)
        .bind(proxy.support_tools)
        .bind(proxy.support_vision)
        .bind(&proxy.extra_headers)
        .bind(&proxy.extra_body)
        .bind(&proxy.created_at)
        .bind(&proxy.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_proxy(&self, proxy: &Proxy) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE proxies SET
                source_model = ?, target_model = ?, upstream = ?, api_key = ?,
                default_max_tokens = ?, default_temperature = ?, default_top_p = ?, default_top_k = ?,
                support_streaming = ?, support_tools = ?, support_vision = ?,
                extra_headers = ?, extra_body = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&proxy.source_model)
        .bind(&proxy.target_model)
        .bind(&proxy.upstream)
        .bind(&proxy.api_key)
        .bind(proxy.default_max_tokens)
        .bind(proxy.default_temperature)
        .bind(proxy.default_top_p)
        .bind(proxy.default_top_k)
        .bind(proxy.support_streaming)
        .bind(proxy.support_tools)
        .bind(proxy.support_vision)
        .bind(&proxy.extra_headers)
        .bind(&proxy.extra_body)
        .bind(&proxy.updated_at)
        .bind(&proxy.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_proxy(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM proxies WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // Permission operations
    pub async fn check_permission(&self, proxy_id: &str, agent_name: &str) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM permissions WHERE proxy_id = ? AND agent_name = ?"
        )
        .bind(proxy_id)
        .bind(agent_name)
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }

    pub async fn list_permissions(&self, proxy_id: &str) -> Result<Vec<Permission>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM permissions WHERE proxy_id = ? ORDER BY granted_at DESC")
            .bind(proxy_id)
            .fetch_all(&self.pool)
            .await
    }

    pub async fn authorize(
        &self,
        proxy_id: &str,
        agent_name: &str,
        permission_level: &str,
        granted_by: &str,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO permissions (proxy_id, agent_name, permission_level, granted_by, granted_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(proxy_id, agent_name) DO UPDATE SET
                permission_level = excluded.permission_level,
                granted_by = excluded.granted_by,
                granted_at = excluded.granted_at
            "#,
        )
        .bind(proxy_id)
        .bind(agent_name)
        .bind(permission_level)
        .bind(granted_by)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn revoke(&self, proxy_id: &str, agent_name: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM permissions WHERE proxy_id = ? AND agent_name = ?")
            .bind(proxy_id)
            .bind(agent_name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // Log operations
    pub async fn create_log(&self, log: &RequestLog) -> Result<i64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            INSERT INTO request_logs (
                proxy_id, user_id, user_role, source_model, target_model, upstream,
                request_method, request_path, request_headers, request_body,
                request_messages_count, request_input_tokens,
                response_status, response_headers, response_body,
                response_output_tokens, response_reasoning_tokens, response_total_tokens,
                duration_ms, time_to_first_token_ms,
                is_stream, is_success, error_type, error_message, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&log.proxy_id)
        .bind(&log.user_id)
        .bind(&log.user_role)
        .bind(&log.source_model)
        .bind(&log.target_model)
        .bind(&log.upstream)
        .bind(&log.request_method)
        .bind(&log.request_path)
        .bind(&log.request_headers)
        .bind(&log.request_body)
        .bind(log.request_messages_count)
        .bind(log.request_input_tokens)
        .bind(log.response_status)
        .bind(&log.response_headers)
        .bind(&log.response_body)
        .bind(log.response_output_tokens)
        .bind(log.response_reasoning_tokens)
        .bind(log.response_total_tokens)
        .bind(log.duration_ms)
        .bind(log.time_to_first_token_ms)
        .bind(log.is_stream)
        .bind(log.is_success)
        .bind(&log.error_type)
        .bind(&log.error_message)
        .bind(&log.created_at)
        .execute(&self.pool)
        .await?;
        Ok(result.last_insert_rowid())
    }

    pub async fn query_logs(&self, params: &LogQueryParams) -> Result<Vec<RequestLog>, sqlx::Error> {
        let mut query = String::from("SELECT * FROM request_logs WHERE 1=1");

        if params.start_date.is_some() {
            query.push_str(" AND created_at >= ?");
        }
        if params.end_date.is_some() {
            query.push_str(" AND created_at <= ?");
        }
        if params.user_id.is_some() {
            query.push_str(" AND user_id = ?");
        }
        if params.proxy_id.is_some() {
            query.push_str(" AND proxy_id = ?");
        }
        if params.source_model.is_some() {
            query.push_str(" AND source_model = ?");
        }
        if params.is_success.is_some() {
            query.push_str(&format!(" AND is_success = {}", if params.is_success.unwrap() { 1 } else { 0 }));
        }

        query.push_str(" ORDER BY created_at DESC");

        let limit = params.limit.unwrap_or(100);
        query.push_str(&format!(" LIMIT {}", limit));

        if let Some(offset) = params.offset {
            query.push_str(&format!(" OFFSET {}", offset));
        }

        let mut q = sqlx::query_as::<_, RequestLog>(&query);
        
        if let Some(ref v) = params.start_date {
            q = q.bind(v);
        }
        if let Some(ref v) = params.end_date {
            q = q.bind(v);
        }
        if let Some(ref v) = params.user_id {
            q = q.bind(v);
        }
        if let Some(ref v) = params.proxy_id {
            q = q.bind(v);
        }
        if let Some(ref v) = params.source_model {
            q = q.bind(v);
        }

        q.fetch_all(&self.pool).await
    }

    pub async fn get_dashboard_stats(&self) -> Result<DashboardStats, sqlx::Error> {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

        let total_proxies: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM proxies")
            .fetch_one(&self.pool)
            .await?;

        let stats: (i64, i64, i64, i64, i64) = sqlx::query_as(
            r#"
            SELECT 
                COALESCE(COUNT(*), 0),
                COALESCE(SUM(CASE WHEN is_success = 1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(duration_ms), 0),
                COALESCE(SUM(request_input_tokens), 0),
                COALESCE(SUM(response_output_tokens), 0)
            FROM request_logs WHERE created_at LIKE ? || '%'
            "#
        )
        .bind(&today)
        .fetch_one(&self.pool)
        .await?;

        let (total_requests, successful_requests, total_duration, input_tokens, output_tokens) = stats;

        let success_rate = if total_requests > 0 {
            (successful_requests as f64 / total_requests as f64) * 100.0
        } else {
            0.0
        };

        let avg_duration = if successful_requests > 0 {
            total_duration as f64 / successful_requests as f64
        } else {
            0.0
        };

        Ok(DashboardStats {
            total_proxies,
            total_requests_today: total_requests,
            successful_requests_today: successful_requests,
            success_rate,
            avg_duration_ms: avg_duration,
            total_input_tokens: input_tokens,
            total_output_tokens: output_tokens,
        })
    }

    pub async fn list_models(&self) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT source_model FROM proxies ORDER BY source_model")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }
}