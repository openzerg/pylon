import { eq } from "drizzle-orm";
import type { DB } from "../db/index.js";
import { proxies } from "../db/schema.js";
import type {
  ProxyInfo,
  CreateProxyRequest,
  UpdateProxyRequest,
} from "@openzerg/common/gen/pylon/v1/pylon_pb";
import { randomId } from "./util.js";

function toProto(row: typeof proxies.$inferSelect): ProxyInfo {
  return {
    $typeName: "pylon.v1.ProxyInfo",
    id:               row.id,
    sourceModel:      row.sourceModel,
    targetModel:      row.targetModel,
    upstream:         row.upstream,
    supportStreaming: row.supportStreaming,
    supportTools:     row.supportTools,
    supportVision:    row.supportVision,
    supportReasoning: row.supportReasoning,
    defaultMaxTokens: row.defaultMaxTokens,
    contextLength:    row.contextLength,
    autoCompactLength: row.autoCompactLength,
    enabled:          row.enabled,
    createdAt:        BigInt(row.createdAt),
    updatedAt:        BigInt(row.updatedAt),
  } as unknown as ProxyInfo;
}

export function createProxyService(db: DB) {
  return {
    async list(enabledOnly: boolean): Promise<ProxyInfo[]> {
      const rows = enabledOnly
        ? await db.select().from(proxies).where(eq(proxies.enabled, true))
        : await db.select().from(proxies);
      return rows.map(toProto);
    },

    async get(id: string): Promise<ProxyInfo | null> {
      const rows = await db.select().from(proxies).where(eq(proxies.id, id));
      return rows[0] ? toProto(rows[0]) : null;
    },

    async getApiKey(id: string): Promise<string> {
      const rows = await db.select({ apiKey: proxies.apiKey }).from(proxies).where(eq(proxies.id, id));
      return rows[0]?.apiKey ?? "";
    },

    async create(req: CreateProxyRequest): Promise<ProxyInfo> {
      const id = randomId();
      const now = Math.floor(Date.now() / 1000);
      const contextLength = req.contextLength || 0;
      const autoCompactLength = req.autoCompactLength || Math.floor(contextLength * 0.9);
      await db.insert(proxies).values({
        id,
        sourceModel:      req.sourceModel,
        targetModel:      req.targetModel,
        upstream:         req.upstream,
        apiKey:           req.apiKey,
        supportStreaming: req.supportStreaming,
        supportTools:     req.supportTools,
        supportVision:    req.supportVision,
        supportReasoning: req.supportReasoning,
        defaultMaxTokens: req.defaultMaxTokens || 4096,
        contextLength,
        autoCompactLength,
        enabled:          true,
        createdAt:        now,
        updatedAt:        now,
      });
      return (await this.get(id))!;
    },

    async update(req: UpdateProxyRequest): Promise<ProxyInfo | null> {
      const now = Math.floor(Date.now() / 1000);
      const existing = await this.get(req.id);
      if (!existing) return null;

      const updates: Record<string, unknown> = { updatedAt: now };

      if (req.targetModel) updates.targetModel = req.targetModel;
      if (req.upstream) updates.upstream = req.upstream;
      if (req.apiKey) updates.apiKey = req.apiKey;
      if (req.supportStreaming !== existing.supportStreaming) updates.supportStreaming = req.supportStreaming;
      if (req.supportTools !== existing.supportTools) updates.supportTools = req.supportTools;
      if (req.supportVision !== existing.supportVision) updates.supportVision = req.supportVision;
      if (req.supportReasoning !== existing.supportReasoning) updates.supportReasoning = req.supportReasoning;
      if (req.defaultMaxTokens && req.defaultMaxTokens !== existing.defaultMaxTokens) updates.defaultMaxTokens = req.defaultMaxTokens;
      if (req.contextLength && req.contextLength !== existing.contextLength) {
        updates.contextLength = req.contextLength;
        updates.autoCompactLength = req.autoCompactLength || Math.floor(req.contextLength * 0.9);
      }
      if (req.autoCompactLength && req.autoCompactLength !== existing.autoCompactLength) updates.autoCompactLength = req.autoCompactLength;
      if (req.enabled !== existing.enabled) updates.enabled = req.enabled;

      await db.update(proxies).set(updates).where(eq(proxies.id, req.id));
      return await this.get(req.id);
    },

    async delete(id: string): Promise<void> {
      await db.delete(proxies).where(eq(proxies.id, id));
    },

    async listModels(): Promise<string[]> {
      const rows = await db.select({ model: proxies.sourceModel }).from(proxies).where(eq(proxies.enabled, true));
      return [...new Set(rows.map(r => r.model))];
    },
  };
}
