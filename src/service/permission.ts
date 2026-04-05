import { and, eq } from "drizzle-orm";
import type { DB } from "../db/index.js";
import { permissions } from "../db/schema.js";
import type { PermissionInfo } from "@openzerg/common/gen/pylon/v1/pylon_pb";
import { nowSec } from "./util.js";

function toProto(row: typeof permissions.$inferSelect): PermissionInfo {
  return {
    $typeName: "pylon.v1.PermissionInfo",
    proxyId:   row.proxyId,
    agentName: row.agentName,
    grantedAt: BigInt(row.grantedAt),
  } as unknown as PermissionInfo;
}

export function createPermissionService(db: DB) {
  return {
    async grant(proxyId: string, agentName: string): Promise<PermissionInfo> {
      const now = nowSec();
      await db.insert(permissions).values({ proxyId, agentName, grantedAt: now })
        .onConflictDoUpdate({ target: [permissions.proxyId, permissions.agentName], set: { grantedAt: now } });
      return toProto({ id: 0, proxyId, agentName, grantedAt: now });
    },

    async revoke(proxyId: string, agentName: string): Promise<void> {
      await db.delete(permissions).where(
        and(eq(permissions.proxyId, proxyId), eq(permissions.agentName, agentName))
      );
    },

    async list(proxyId: string): Promise<PermissionInfo[]> {
      const rows = await db.select().from(permissions).where(eq(permissions.proxyId, proxyId));
      return rows.map(toProto);
    },

    async check(proxyId: string, agentName: string): Promise<boolean> {
      const rows = await db.select({ id: permissions.id }).from(permissions).where(
        and(eq(permissions.proxyId, proxyId), eq(permissions.agentName, agentName))
      );
      return rows.length > 0;
    },
  };
}
