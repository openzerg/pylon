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

// Combined HTTP server: ConnectRPC + OpenAI-compatible /v1/*
const server = createServer(async (req: IncomingMessage, res: ServerResponse) => {
  const url = req.url ?? "/";

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

// Register with Cerebrate
if (cfg.cerebrateURL && cfg.adminToken) {
  const cc = new CerebrateClient({ baseURL: cfg.cerebrateURL });
  cc.login(cfg.adminToken).then(async () => {
    const ip = "127.0.0.1";
    const publicURL = cfg.publicURL || `http://${ip}:${cfg.port}`;
    await cc.registerInstance({
      name: "pylon",
      instanceType: "pylon",
      ip,
      port: cfg.port,
      status: "running",
      labels: { public_url: publicURL },
    });
    console.log("[pylon] registered with cerebrate");
  }).catch(e => console.error("[pylon] cerebrate register failed:", e));
}

process.on("SIGINT", () => { server.close(); process.exit(0); });
process.on("SIGTERM", () => { server.close(); process.exit(0); });
