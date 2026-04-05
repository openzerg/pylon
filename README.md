# pylon

LLM 网关。统一代理多个上游大模型提供商，提供权限控制、用量追踪和模型路由。

- **端口**: 15316
- **语言**: TypeScript / Bun
- **数据库**: SQLite (`~/.openzerg/pylon.db`)

## 职责

Pylon 作为 AI Agent 与 LLM 之间的代理层，解决以下问题：

1. **多模型统一接入**：将不同提供商（OpenAI、Anthropic、Google、本地模型）的 API 差异屏蔽，对外提供统一的 `Chat` / `StreamChat` 接口
2. **模型路由**：通过 Proxy 配置定义 `source_model`（对外名称）→ `target_model`（实际模型）的映射，agent 按逻辑名称请求，pylon 路由到实际端点
3. **权限控制**：每个 Proxy 可授权特定 agent（按 `agent_name`）使用，未授权的 agent 请求被拒绝
4. **用量统计**：记录每次 LLM 调用的 token 消耗、首 token 时间（TTFT）、总耗时，支持按时间/模型/用户分组查询

## Proxy 配置

```
Proxy = {
    id: "anthropic-claude",
    source_model: "claude-3.5-sonnet",      // agent 使用的逻辑名
    target_model: "claude-3-5-sonnet-20241022", // 实际发往 API 的模型名
    upstream: "https://api.anthropic.com",
    api_key: "sk-ant-...",
    support_streaming: true,
    support_tools: true,
}
```

Agent 配置 `model: "claude-3.5-sonnet"` → Pylon 查找对应 Proxy → 调实际 API。

## API（ConnectRPC）

```protobuf
service PylonService {
    // Proxy 管理
    rpc ListProxies / GetProxy / CreateProxy / UpdateProxy / DeleteProxy

    // 权限管理
    rpc AuthorizeAgent / RevokeAgent / ListPermissions / CheckPermission

    // LLM 调用
    rpc Chat(ChatRequest) returns (ChatResponse);
    rpc StreamChat(ChatRequest) returns (stream ChatChunk);
    rpc ListModels(Empty) returns (ModelListResponse);

    // 日志与统计
    rpc QueryLogs / ClearLogs
    rpc GetTokenStats(TokenStatsRequest) returns (TokenStatsResponse);
}
```

## 与 Mutalisk 的关系

Mutalisk 使用 Pylon 的方式是可选的：

- **通过 Pylon**：Mutalisk 配置 `provider.base_url = http://pylon:15316`，模型名通过 Pylon 路由
- **直连 LLM**：Mutalisk 配置 `provider.base_url = https://api.openai.com`，绕过 Pylon

推荐使用 Pylon 的场景：集中管理多个 agent 的 LLM 用量、需要审计日志、需要按团队/项目控制模型使用权限。

Pylon 的 `agent_name` 权限系统与 Mutalisk 的 `provider_name` 配置是两套独立机制。当 mutalisk 通过 pylon 发起请求时，pylon 通过 JWT token 中的用户信息识别调用方。

## 代码结构

```
pylon/
├── src/
│   ├── main.ts
│   ├── config.ts         # 配置加载（~/.openzerg/config.yaml 中的 pylon 配置段）
│   ├── db/               # SQLite 数据访问层（Drizzle ORM）
│   ├── llm/
│   │   ├── client.ts     # ILLMClient 接口 + AIKitLLMClient 实现（基于 ai SDK）
│   │   ├── provider.ts   # 各提供商适配器
│   │   └── tokens.ts     # Token 用量提取（处理各提供商差异）
│   ├── service/
│   │   ├── proxy.ts      # Proxy CRUD
│   │   ├── permission.ts # 权限检查
│   │   ├── chat.ts       # Chat/StreamChat 主逻辑 + 日志
│   │   ├── logs.ts       # 日志查询
│   │   └── stats.ts      # Token 统计聚合
│   └── api/
│       └── connect.ts    # ConnectRPC server 装配
└── proto/
    └── pylon.proto
```

## 环境变量

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `PYLON_PORT` | 监听端口 | `15316` |
| `PYLON_DB_PATH` | SQLite 路径 | `~/.openzerg/pylon.db` |
| `CEREBRATE_URL` | Cerebrate 地址 | — |
| `CEREBRATE_ADMIN_TOKEN` | Cerebrate API Key | — |
| `PYLON_PUBLIC_URL` | 本服务公开 URL | — |

> **与 ZergRepos 的区别：** 数据库路径从 `~/.pylon/pylon.db` 改为 `~/.openzerg/pylon.db`，与其他服务统一目录。共享 `@openzerg/cerebrate-client` 包，不再复制文件。
