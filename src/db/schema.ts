import { sqliteTable, text, integer, real } from "drizzle-orm/sqlite-core";

export const proxies = sqliteTable("proxies", {
  id:              text("id").primaryKey(),
  sourceModel:     text("source_model").notNull(),
  targetModel:     text("target_model").notNull(),
  upstream:        text("upstream").notNull(),
  apiKey:          text("api_key").notNull().default(""),
  supportStreaming: integer("support_streaming", { mode: "boolean" }).notNull().default(true),
  supportTools:    integer("support_tools",    { mode: "boolean" }).notNull().default(false),
  supportVision:   integer("support_vision",   { mode: "boolean" }).notNull().default(false),
  supportReasoning: integer("support_reasoning", { mode: "boolean" }).notNull().default(false),
  defaultMaxTokens: integer("default_max_tokens").notNull().default(4096),
  contextLength:    integer("context_length").notNull().default(0),
  autoCompactLength: integer("auto_compact_length").notNull().default(0),
  enabled:         integer("enabled", { mode: "boolean" }).notNull().default(true),
  createdAt:       integer("created_at").notNull(),
  updatedAt:       integer("updated_at").notNull(),
});

export const permissions = sqliteTable("permissions", {
  id:        integer("id").primaryKey({ autoIncrement: true }),
  proxyId:   text("proxy_id").notNull(),
  agentName: text("agent_name").notNull(),
  grantedAt: integer("granted_at").notNull(),
});

export const logs = sqliteTable("logs", {
  id:                text("id").primaryKey(),
  proxyId:           text("proxy_id").notNull().default(""),
  sourceModel:       text("source_model").notNull().default(""),
  targetModel:       text("target_model").notNull().default(""),
  upstream:          text("upstream").notNull().default(""),
  inputTokens:       integer("input_tokens").notNull().default(0),
  outputTokens:      integer("output_tokens").notNull().default(0),
  totalTokens:       integer("total_tokens").notNull().default(0),
  durationMs:        integer("duration_ms").notNull().default(0),
  timeToFirstTokenMs: integer("time_to_first_token_ms").notNull().default(0),
  isStream:          integer("is_stream", { mode: "boolean" }).notNull().default(false),
  isSuccess:         integer("is_success", { mode: "boolean" }).notNull().default(false),
  errorMessage:      text("error_message").notNull().default(""),
  createdAt:         integer("created_at").notNull(),
});
