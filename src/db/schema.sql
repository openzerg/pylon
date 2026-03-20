-- ============================================
-- Proxy 配置表
-- ============================================
CREATE TABLE IF NOT EXISTS proxies (
    id TEXT PRIMARY KEY,
    source_model TEXT NOT NULL UNIQUE,
    target_model TEXT NOT NULL,
    upstream TEXT NOT NULL,
    api_key TEXT NOT NULL,
    
    -- 可选参数
    default_max_tokens INTEGER,
    default_temperature REAL,
    default_top_p REAL,
    default_top_k INTEGER,
    
    -- 功能开关
    support_streaming INTEGER DEFAULT 1,
    support_tools INTEGER DEFAULT 0,
    support_vision INTEGER DEFAULT 0,
    
    -- 扩展配置
    extra_headers TEXT,
    extra_body TEXT,
    
    -- 元数据
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_proxies_source_model ON proxies(source_model);

-- ============================================
-- 权限配置表
-- ============================================
CREATE TABLE IF NOT EXISTS permissions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    proxy_id TEXT NOT NULL REFERENCES proxies(id) ON DELETE CASCADE,
    agent_name TEXT NOT NULL,
    
    -- 权限级别: 'owner', 'use'
    permission_level TEXT DEFAULT 'use',
    
    -- 授权信息
    granted_by TEXT NOT NULL,
    granted_at TEXT NOT NULL,
    
    -- 唯一约束
    UNIQUE(proxy_id, agent_name)
);

CREATE INDEX IF NOT EXISTS idx_permissions_proxy_id ON permissions(proxy_id);
CREATE INDEX IF NOT EXISTS idx_permissions_agent_name ON permissions(agent_name);

-- ============================================
-- 请求日志表
-- ============================================
CREATE TABLE IF NOT EXISTS request_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    
    -- 关联
    proxy_id TEXT REFERENCES proxies(id) ON DELETE SET NULL,
    
    -- 用户信息 (从 JWT 解析)
    user_id TEXT NOT NULL,
    user_role TEXT NOT NULL,
    
    -- 模型信息
    source_model TEXT NOT NULL,
    target_model TEXT NOT NULL,
    upstream TEXT NOT NULL,
    
    -- 请求详情
    request_method TEXT NOT NULL DEFAULT 'POST',
    request_path TEXT NOT NULL DEFAULT '/v1/chat/completions',
    request_headers TEXT,
    request_body TEXT,
    request_messages_count INTEGER,
    request_input_tokens INTEGER,
    
    -- 响应详情
    response_status INTEGER,
    response_headers TEXT,
    response_body TEXT,
    response_output_tokens INTEGER,
    response_reasoning_tokens INTEGER,
    response_total_tokens INTEGER,
    
    -- 性能指标
    duration_ms INTEGER,
    time_to_first_token_ms INTEGER,
    
    -- 标记
    is_stream INTEGER DEFAULT 0,
    is_success INTEGER DEFAULT 1,
    error_type TEXT,
    error_message TEXT,
    
    -- 时间戳
    created_at TEXT NOT NULL
);

-- 索引
CREATE INDEX IF NOT EXISTS idx_logs_user_id ON request_logs(user_id);
CREATE INDEX IF NOT EXISTS idx_logs_proxy_id ON request_logs(proxy_id);
CREATE INDEX IF NOT EXISTS idx_logs_created_at ON request_logs(created_at);
CREATE INDEX IF NOT EXISTS idx_logs_source_model ON request_logs(source_model);
CREATE INDEX IF NOT EXISTS idx_logs_is_success ON request_logs(is_success);
CREATE INDEX IF NOT EXISTS idx_logs_user_created ON request_logs(user_id, created_at);
CREATE INDEX IF NOT EXISTS idx_logs_proxy_created ON request_logs(proxy_id, created_at);