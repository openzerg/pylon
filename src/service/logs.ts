import { and, eq, gte, lte, sql } from "drizzle-orm";
import type { DB } from "../db/index.js";
import { logs } from "../db/schema.js";
import type { LogEntry, QueryLogsRequest, TokenStatsResponse } from "@openzerg/common/gen/pylon/v1/pylon_pb";

function toProto(row: typeof logs.$inferSelect): LogEntry {
  return {
    $typeName: "pylon.v1.LogEntry",
    id:                   row.id,
    proxyId:              row.proxyId,
    sourceModel:          row.sourceModel,
    targetModel:          row.targetModel,
    upstream:             row.upstream,
    inputTokens:          BigInt(row.inputTokens),
    outputTokens:         BigInt(row.outputTokens),
    totalTokens:          BigInt(row.totalTokens),
    durationMs:           BigInt(row.durationMs),
    timeToFirstTokenMs:   BigInt(row.timeToFirstTokenMs),
    isStream:             row.isStream,
    isSuccess:            row.isSuccess,
    errorMessage:         row.errorMessage,
    createdAt:            BigInt(row.createdAt),
  } as unknown as LogEntry;
}

export function createLogsService(db: DB) {
  return {
    async query(req: QueryLogsRequest): Promise<{ logsList: LogEntry[]; total: number }> {
      const conditions = [];
      if (req.proxyId) conditions.push(eq(logs.proxyId, req.proxyId));
      if (req.fromTs)  conditions.push(gte(logs.createdAt, Number(req.fromTs)));
      if (req.toTs)    conditions.push(lte(logs.createdAt, Number(req.toTs)));

      const where = conditions.length > 0 ? and(...conditions) : undefined;

      const [{ count }] = await db.select({ count: sql<number>`count(*)` }).from(logs).where(where);
      const limit = req.limit || 50;
      const offset = req.offset || 0;
      const rows = await db.select().from(logs).where(where)
        .orderBy(sql`created_at DESC`)
        .limit(limit).offset(offset);

      return { logsList: rows.map(toProto), total: count };
    },

    async tokenStats(proxyId: string, fromTs: bigint, toTs: bigint): Promise<TokenStatsResponse> {
      const conditions = [];
      if (proxyId) conditions.push(eq(logs.proxyId, proxyId));
      if (fromTs)  conditions.push(gte(logs.createdAt, Number(fromTs)));
      if (toTs)    conditions.push(lte(logs.createdAt, Number(toTs)));

      const where = conditions.length > 0 ? and(...conditions) : undefined;

      const [row] = await db.select({
        totalInput:  sql<number>`sum(input_tokens)`,
        totalOutput: sql<number>`sum(output_tokens)`,
        totalTokens: sql<number>`sum(total_tokens)`,
        count:       sql<number>`count(*)`,
      }).from(logs).where(where);

      return {
        $typeName: "pylon.v1.TokenStatsResponse",
        totalInputTokens:  BigInt(row?.totalInput  ?? 0),
        totalOutputTokens: BigInt(row?.totalOutput ?? 0),
        totalTokens:       BigInt(row?.totalTokens ?? 0),
        requestCount:      BigInt(row?.count       ?? 0),
      } as unknown as TokenStatsResponse;
    },
  };
}
