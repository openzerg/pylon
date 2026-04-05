import { ConnectRouter } from "@connectrpc/connect";
import { PylonService } from "@openzerg/common/gen/pylon/v1/pylon_pb";
import type { DB } from "../db/index.js";
import { createProxyService } from "../service/proxy.js";
import { createPermissionService } from "../service/permission.js";
import { createChatService } from "../service/chat.js";
import { createLogsService } from "../service/logs.js";

export function createRouter(db: DB) {
  const proxySvc = createProxyService(db);
  const permSvc = createPermissionService(db);
  const chatSvc = createChatService(db);
  const logsSvc = createLogsService(db);

  return (router: ConnectRouter) => {
    router.service(PylonService, {
      // ── Proxies ────────────────────────────────────────────────────────
      async listProxies(req) {
        const list = await proxySvc.list(req.enabledOnly);
        return { proxies: list };
      },
      async getProxy(req) {
        const p = await proxySvc.get(req.id);
        if (!p) throw new Error("proxy not found");
        return p;
      },
      async createProxy(req) {
        return await proxySvc.create(req);
      },
      async updateProxy(req) {
        const p = await proxySvc.update(req);
        if (!p) throw new Error("proxy not found");
        return p;
      },
      async deleteProxy(req) {
        await proxySvc.delete(req.id);
        return {};
      },
      async listModels(_req) {
        const models = await proxySvc.listModels();
        return { models };
      },

      // ── Permissions ────────────────────────────────────────────────────
      async authorizeAgent(req) {
        return await permSvc.grant(req.proxyId, req.agentName);
      },
      async revokeAgent(req) {
        await permSvc.revoke(req.proxyId, req.agentName);
        return {};
      },
      async listPermissions(req) {
        const list = await permSvc.list(req.proxyId);
        return { permissions: list };
      },
      async checkPermission(req) {
        const allowed = await permSvc.check(req.proxyId, req.agentName);
        return { allowed };
      },

      // ── Chat ───────────────────────────────────────────────────────────
      async chat(req) {
        const model = req.model;
        const proxyList = await proxySvc.list(true);
        const proxy = proxyList.find(p => p.sourceModel === model);
        if (!proxy) throw new Error(`no proxy for model: ${model}`);

        const apiKey = await proxySvc.getApiKey(proxy.id);
        return await chatSvc.chat(req, proxy.id, proxy.upstream, apiKey, proxy.sourceModel, proxy.targetModel);
      },
      async *streamChat(req) {
        const model = req.model;
        const proxyList = await proxySvc.list(true);
        const proxy = proxyList.find(p => p.sourceModel === model);
        if (!proxy) throw new Error(`no proxy for model: ${model}`);

        const apiKey = await proxySvc.getApiKey(proxy.id);
        for await (const chunk of chatSvc.streamChat(req, proxy.id, proxy.upstream, apiKey, proxy.sourceModel, proxy.targetModel)) {
          yield chunk;
        }
      },

      // ── Logs ───────────────────────────────────────────────────────────
      async queryLogs(req) {
        const { logsList, total } = await logsSvc.query(req);
        return { logs: logsList, total };
      },
      async getTokenStats(req) {
        return await logsSvc.tokenStats(req.proxyId, req.fromTs, req.toTs);
      },
    });
  };
}
