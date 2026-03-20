# Pylon 更新方案：SQLite 存储 + Web UI + 权限管理

## 一、架构概述

### 核心变更
1. **存储统一**: 从 JSON 文件迁移到 SQLite 数据库
2. **权限管理**: Pylon 独立管理 LLM 授权，不再依赖 Cerebrate
3. **Web UI**: 提供完整的管理界面
4. **请求日志**: 所有请求记录到 SQLite，支持灵活查询

### 系统架构

```
┌─────────────────────────────────────────────────────────────┐
│                         Pylon Gateway                        │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │  HTTP API   │  │   Web UI    │  │   Auth Middleware   │  │
│  │  /v1/*      │  │   /ui/*     │  │   (JWT 验证)        │  │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘  │
│         │                │                     │             │
│         └────────────────┼─────────────────────┘             │
│                          ▼                                   │
│  ┌───────────────────────────────────────────────────────┐  │
│  │                    SQLite Database                     │  │
│  │  - proxies (中转配置)                                   │  │
│  │  - permissions (权限配置)                               │  │
│  │  - request_logs (请求日志)                              │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

---

## 二、数据库 Schema

### 2.1 Proxy 配置表

```sql
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
```

### 2.2 权限配置表

```sql
CREATE TABLE IF NOT EXISTS permissions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    proxy_id TEXT NOT NULL REFERENCES proxies(id) ON DELETE CASCADE,
    agent_name TEXT NOT NULL,
    
    -- 权限级别
    permission_level TEXT DEFAULT 'use',  -- 'owner', 'use'
    
    -- 授权信息
    granted_by TEXT NOT NULL,             -- 谁授权的 (admin username)
    granted_at TEXT NOT NULL,
    
    -- 唯一约束
    UNIQUE(proxy_id, agent_name)
);

CREATE INDEX IF NOT EXISTS idx_permissions_proxy_id ON permissions(proxy_id);
CREATE INDEX IF NOT EXISTS idx_permissions_agent_name ON permissions(agent_name);
```

### 2.3 请求日志表

```sql
CREATE TABLE IF NOT EXISTS request_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    
    -- 关联
    proxy_id TEXT REFERENCES proxies(id) ON DELETE SET NULL,
    
    -- 用户信息 (从 JWT 解析)
    user_id TEXT NOT NULL,               -- JWT subject
    user_role TEXT NOT NULL,             -- JWT role (admin/agent/service)
    
    -- 模型信息
    source_model TEXT NOT NULL,
    target_model TEXT NOT NULL,
    upstream TEXT NOT NULL,
    
    -- 请求详情
    request_method TEXT NOT NULL DEFAULT 'POST',
    request_path TEXT NOT NULL DEFAULT '/v1/chat/completions',
    request_headers TEXT,                -- JSON
    request_body TEXT,                   -- JSON
    request_messages_count INTEGER,
    request_input_tokens INTEGER,
    
    -- 响应详情
    response_status INTEGER,
    response_headers TEXT,               -- JSON
    response_body TEXT,                  -- JSON
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

-- 索引 (支持灵活查询)
CREATE INDEX IF NOT EXISTS idx_logs_user_id ON request_logs(user_id);
CREATE INDEX IF NOT EXISTS idx_logs_proxy_id ON request_logs(proxy_id);
CREATE INDEX IF NOT EXISTS idx_logs_created_at ON request_logs(created_at);
CREATE INDEX IF NOT EXISTS idx_logs_source_model ON request_logs(source_model);
CREATE INDEX IF NOT EXISTS idx_logs_is_success ON request_logs(is_success);
CREATE INDEX IF NOT EXISTS idx_logs_user_created ON request_logs(user_id, created_at);
CREATE INDEX IF NOT EXISTS idx_logs_proxy_created ON request_logs(proxy_id, created_at);
```

---

## 三、认证与权限

### 3.1 JWT Claims 结构

```rust
pub struct Claims {
    pub iss: String,        // 签发者: "cerebrate.openzerg.local"
    pub sub: String,        // 主体: admin 或 agent 名称
    pub role: String,       // 角色: "admin", "agent", "service"
    pub forgejo_user: Option<String>,
    pub iat: i64,
    pub exp: i64,
}
```

### 3.2 权限验证逻辑

```rust
pub async fn check_permission(
    db: &SqlitePool,
    proxy_id: &str,
    claims: &Claims,
) -> Result<bool> {
    // 1. Admin 角色有所有权限
    if claims.role == "admin" {
        return Ok(true);
    }
    
    // 2. Agent 角色查询权限表
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM permissions 
         WHERE proxy_id = ?1 AND agent_name = ?2"
    )
    .bind(proxy_id)
    .bind(&claims.sub)
    .fetch_one(db)
    .await?;
    
    Ok(count > 0)
}
```

### 3.3 请求处理流程

```
请求到达 /v1/chat/completions
    │
    ▼
JWT 认证中间件
    │
    ├── 验证 JWT 签名和过期时间
    │
    └── 解析 Claims → 注入到请求扩展
            │
            ▼
    解析请求体获取 model
            │
            ▼
    查询 proxy 配置 (source_model → proxy)
            │
            ├── 未找到 → 404 Proxy Not Found
            │
            └── 找到 → 权限检查
                    │
                    ├── Admin → 允许
                    │
                    ├── Agent → 查询 permissions 表
                    │           │
                    │           ├── 有权限 → 允许
                    │           │
                    │           └── 无权限 → 403 Forbidden
                    │
                    ▼
            转发请求到上游
                    │
                    ▼
            记录请求日志
                    │
                    ▼
            返回响应
```

---

## 四、API 设计

### 4.1 Proxy 管理 API (Admin Only)

| 方法 | 路径 | 功能 |
|------|------|------|
| GET | `/v1/proxies` | 列出所有 proxy |
| POST | `/v1/proxies` | 创建 proxy |
| GET | `/v1/proxies/{id}` | 获取 proxy 详情 |
| PUT | `/v1/proxies/{id}` | 更新 proxy |
| DELETE | `/v1/proxies/{id}` | 删除 proxy |

### 4.2 权限管理 API (Admin Only)

| 方法 | 路径 | 功能 |
|------|------|------|
| GET | `/v1/proxies/{id}/permissions` | 列出 proxy 的授权 |
| POST | `/v1/proxies/{id}/authorize` | 授权 agent |
| POST | `/v1/proxies/{id}/revoke` | 撤销授权 |

请求体：
```json
{
    "agent_name": "agent-1",
    "permission_level": "use"
}
```

### 4.3 模型查询 API (认证用户)

| 方法 | 路径 | 功能 |
|------|------|------|
| GET | `/v1/models` | 列出可用的模型 (根据权限过滤) |
| GET | `/v1/models/{model}` | 获取模型详情 |

### 4.4 聊天 API (认证用户)

| 方法 | 路径 | 功能 |
|------|------|------|
| POST | `/v1/chat/completions` | 聊天请求 |

### 4.5 日志查询 API (Admin Only)

| 方法 | 路径 | 功能 |
|------|------|------|
| GET | `/v1/logs` | 查询请求日志 |
| GET | `/v1/logs/stats` | 统计数据 |

查询参数：
- `start_date`: 开始日期 (YYYY-MM-DD)
- `end_date`: 结束日期 (YYYY-MM-DD)
- `user_id`: 用户筛选
- `proxy_id`: Proxy 筛选
- `source_model`: 模型筛选
- `is_success`: 成功/失败筛选
- `limit`: 返回数量限制
- `offset`: 分页偏移

---

## 五、Web UI 设计

### 5.1 页面路由

| 路径 | 功能 | 权限 |
|------|------|------|
| `/ui/login` | 登录页面 | 公开 |
| `/ui/` | 仪表盘 | 认证用户 |
| `/ui/proxies` | Proxy 列表 | 认证用户 |
| `/ui/proxies/new` | 创建 Proxy | Admin |
| `/ui/proxies/{id}` | 编辑 Proxy | Admin |
| `/ui/proxies/{id}/permissions` | 权限管理 | Admin |
| `/ui/models` | 模型列表 | 认证用户 |
| `/ui/chat` | API 测试控制台 | 认证用户 |
| `/ui/logs` | 请求日志查询 | Admin |

### 5.2 技术栈

- **模板引擎**: Askama (rinja)
- **CSS 框架**: Tailwind CSS (CDN)
- **前端交互**: HTMX
- **图表**: Chart.js (可选)

### 5.3 核心页面

#### 仪表盘
- 统计卡片: Proxy 数量、今日请求、成功率、平均耗时
- 用户使用排行
- 模型使用排行
- 最近请求列表

#### Proxy 管理
- 列表展示
- 创建/编辑表单
- 权限管理界面
- 测试连接功能

#### API 测试控制台
- 模型选择 (下拉)
- 消息输入 (JSON 编辑器)
- 参数设置
- 发送请求
- 响应展示

#### 日志查询
- 日期范围选择
- 多条件筛选
- 结果列表
- 详情查看

---

## 六、目录结构

```
pylon/
├── Cargo.toml
├── build.rs                        # Askama 模板编译
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── cli.rs
│   ├── error.rs
│   ├── db/
│   │   ├── mod.rs
│   │   ├── schema.sql
│   │   └── models.rs
│   ├── proxy/
│   │   ├── mod.rs
│   │   ├── handlers.rs
│   │   └── middleware.rs
│   ├── web/
│   │   ├── mod.rs
│   │   ├── routes.rs
│   │   ├── handlers.rs
│   │   ├── templates.rs
│   │   └── auth.rs
│   └── logging/
│       ├── mod.rs
│       └── recorder.rs
└── templates/
    ├── layout.html
    ├── login.html
    ├── index.html
    ├── proxies/
    │   ├── list.html
    │   ├── form.html
    │   └── permissions.html
    ├── models/
    │   └── list.html
    ├── chat/
    │   └── console.html
    └── logs/
        ├── list.html
        └── detail.html
```

---

## 七、实现步骤

### Phase 1: 数据库基础 (Day 1)

1. 添加依赖: `sqlx`, `askama`
2. 创建 `db/` 模块
3. 实现 SQLite 连接池
4. 创建 schema.sql
5. 实现 schema 初始化

### Phase 2: Proxy 配置迁移 (Day 2)

1. 实现 Proxy CRUD (SQLite)
2. 更新现有 API 使用 SQLite
3. 数据迁移工具 (JSON → SQLite)
4. 测试

### Phase 3: 权限系统 (Day 3)

1. 实现 permissions 表操作
2. 实现权限检查中间件
3. 实现授权/撤销 API
4. 测试

### Phase 4: 请求日志 (Day 4)

1. 实现日志记录中间件
2. 实现日志查询 API
3. 实现统计 API
4. 测试

### Phase 5: Web UI 基础 (Day 5-6)

1. 创建模板结构
2. 实现登录页面
3. 实现仪表盘
4. 实现 Proxy 管理页面
5. 实现 API 测试控制台

### Phase 6: Web UI 完善 (Day 7)

1. 实现权限管理界面
2. 实现日志查询界面
3. 实现模型列表页面
4. 样式优化

### Phase 7: 测试和文档 (Day 8)

1. 单元测试
2. 集成测试
3. 文档更新
4. 部署测试

---

## 八、依赖更新

```toml
[dependencies]
# 现有依赖...

# 新增
askama = "0.12"
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }
chrono = { version = "0.4", features = ["serde"] }

[build-dependencies]
askama = "0.12"
```

---

## 九、配置文件

数据库位置: `~/.pylon/pylon.db`

环境变量:
- `JWT_SECRET`: JWT 密钥 (与 Cerebrate 共享)
- `PYLON_DB_PATH`: 数据库路径 (可选，默认 `~/.pylon/pylon.db`)
- `PYLON_PORT`: 服务端口 (默认 8080)