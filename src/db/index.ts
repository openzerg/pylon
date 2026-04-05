import { drizzle } from "drizzle-orm/bun-sqlite";
import { Database } from "bun:sqlite";
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";
import * as schema from "./schema.js";

export type DB = ReturnType<typeof openDB>;

export function openDB(path: string) {
  mkdirSync(dirname(path), { recursive: true });
  const sqlite = new Database(path);
  sqlite.exec("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;");

  // Create tables
  sqlite.exec(`
    CREATE TABLE IF NOT EXISTS proxies (
      id                TEXT PRIMARY KEY,
      source_model      TEXT NOT NULL,
      target_model      TEXT NOT NULL,
      upstream          TEXT NOT NULL,
      api_key           TEXT NOT NULL DEFAULT '',
      support_streaming INTEGER NOT NULL DEFAULT 1,
      support_tools     INTEGER NOT NULL DEFAULT 0,
      support_vision    INTEGER NOT NULL DEFAULT 0,
      support_reasoning INTEGER NOT NULL DEFAULT 0,
      default_max_tokens INTEGER NOT NULL DEFAULT 4096,
      enabled           INTEGER NOT NULL DEFAULT 1,
      created_at        INTEGER NOT NULL,
      updated_at        INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS permissions (
      id         INTEGER PRIMARY KEY AUTOINCREMENT,
      proxy_id   TEXT NOT NULL,
      agent_name TEXT NOT NULL,
      granted_at INTEGER NOT NULL,
      UNIQUE(proxy_id, agent_name)
    );

    CREATE TABLE IF NOT EXISTS logs (
      id                    TEXT PRIMARY KEY,
      proxy_id              TEXT NOT NULL DEFAULT '',
      source_model          TEXT NOT NULL DEFAULT '',
      target_model          TEXT NOT NULL DEFAULT '',
      upstream              TEXT NOT NULL DEFAULT '',
      input_tokens          INTEGER NOT NULL DEFAULT 0,
      output_tokens         INTEGER NOT NULL DEFAULT 0,
      total_tokens          INTEGER NOT NULL DEFAULT 0,
      duration_ms           INTEGER NOT NULL DEFAULT 0,
      time_to_first_token_ms INTEGER NOT NULL DEFAULT 0,
      is_stream             INTEGER NOT NULL DEFAULT 0,
      is_success            INTEGER NOT NULL DEFAULT 0,
      error_message         TEXT NOT NULL DEFAULT '',
      created_at            INTEGER NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_logs_proxy    ON logs(proxy_id);
    CREATE INDEX IF NOT EXISTS idx_logs_ts       ON logs(created_at);
    CREATE INDEX IF NOT EXISTS idx_perms_proxy   ON permissions(proxy_id);
    CREATE INDEX IF NOT EXISTS idx_perms_agent   ON permissions(agent_name);
  `);

  return drizzle(sqlite, { schema });
}
