import { connectNodeAdapter } from "@connectrpc/connect-node";
import { createServer } from "node:http";
import type { IncomingMessage, ServerResponse } from "node:http";
import { loadConfig } from "./config.js";
import { openDB } from "./db/index.js";
import { createRouter } from "./api/server.js";
import { createChatService } from "./service/chat.js";
import { CerebrateClient } from "@openzerg/common";

const cfg = loadConfig();
const db = openDB(cfg.dbPath);
const router = createRouter(db);
const chatSvc = createChatService(db);

const connectHandler = connectNodeAdapter({ routes: router });

// CORS helper
function setCORSHeaders(res: ServerResponse, origin: string) {
  res.setHeader("Access-Control-Allow-Origin", origin || "*");
  res.setHeader("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS");
  res.setHeader("Access-Control-Allow-Headers", "Content-Type, Authorization, Connect-Protocol-Version, Connect-Timeout-Ms, Grpc-Timeout, X-Grpc-Web, X-User-Agent");
  res.setHeader("Access-Control-Allow-Credentials", "true");
  res.setHeader("Access-Control-Expose-Headers", "Grpc-Status, Grpc-Message, Connect-Protocol-Version");
}

// Combined HTTP server: ConnectRPC + OpenAI-compatible /v1/*
const server = createServer(async (req: IncomingMessage, res: ServerResponse) => {
  const url = req.url ?? "/";
  const origin = req.headers.origin ?? "*";

  // Handle CORS preflight
  if (req.method === "OPTIONS") {
    setCORSHeaders(res, origin);
    res.writeHead(204);
    res.end();
    return;
  }

  // Add CORS headers to all responses
  setCORSHeaders(res, origin);

  // OpenAI-compatible passthrough endpoint
  if (url === "/v1/chat/completions" && req.method === "POST") {
    const chunks: Buffer[] = [];
    req.on("data", (c: Buffer) => chunks.push(c));
    req.on("end", async () => {
      const bunReq = new Request(`http://pylon${url}`, {
        method: "POST",
        headers: Object.fromEntries(
          Object.entries(req.headers)
            .filter(([, v]) => v != null)
            .map(([k, v]) => [k, Array.isArray(v) ? v[0] : v as string])
        ),
        body: Buffer.concat(chunks),
      });
      const response = await chatSvc.openaiPassthrough(bunReq, db);
      res.writeHead(response.status, Object.fromEntries(response.headers.entries()));
      if (response.body) {
        const reader = response.body.getReader();
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          res.write(value);
        }
      }
      res.end();
    });
    return;
  }

  // ConnectRPC handler
  return connectHandler(req, res);
});

server.listen(cfg.port, cfg.host, () => {
  console.log(`[pylon] listening on ${cfg.host}:${cfg.port}`);
  console.log(`[pylon] OpenAI-compatible endpoint: http://${cfg.host}:${cfg.port}/v1`);
});

// Register with Cerebrate and send periodic heartbeats
if (cfg.cerebrateURL && cfg.adminToken) {
  const cc = new CerebrateClient({ baseURL: cfg.cerebrateURL });
  let registeredInstanceId: string | null = null;

  const registerAndHeartbeat = async () => {
    await cc.login(cfg.adminToken);

    // Use host IP (192.168.200.1) so agent containers can reach pylon
    const hostIP = process.env.PYLON_HOST_IP ?? "192.168.200.1";
    const publicURL = cfg.publicURL || `http://${hostIP}:${cfg.port}`;

    // Check if already registered by name to avoid duplicates on restart
    const { instances } = await cc.listInstances({});
    const existing = instances.find(
      i => i.instanceType === "pylon" && i.name === "pylon"
    );
    if (existing) {
      console.log(`[pylon] already registered with cerebrate (${existing.instanceId})`);
      registeredInstanceId = existing.instanceId;
    } else {
      const inst = await cc.registerInstance({
        name: "pylon",
        instanceType: "pylon",
        ip: hostIP,
        port: cfg.port,
        status: "running",
        labels: { public_url: publicURL },
      });
      registeredInstanceId = inst.instanceId;
      console.log(`[pylon] registered with cerebrate (${registeredInstanceId})`);
    }

    // Send heartbeat every 30 seconds
    setInterval(async () => {
      if (!registeredInstanceId) return;
      try {
        await cc.heartbeat(registeredInstanceId);
      } catch (e) {
        console.error("[pylon] heartbeat failed:", e);
        // Try to re-login on next heartbeat
        try { await cc.login(cfg.adminToken); } catch {}
      }
    }, 30_000);
  };

  registerAndHeartbeat().catch(e => console.error("[pylon] cerebrate register failed:", e));
}

process.on("SIGINT", () => { server.close(); process.exit(0); });
process.on("SIGTERM", () => { server.close(); process.exit(0); });
