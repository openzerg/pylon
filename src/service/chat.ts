import type { DB } from "../db/index.js";
import type { ChatRequest, ChatResponse, ChatChunk } from "@openzerg/common/gen/pylon/v1/pylon_pb";
import { logs } from "../db/schema.js";
import { randomId, nowSec } from "./util.js";

interface OpenAIMessage {
  role: string;
  content: string;
  tool_calls?: unknown[];
  tool_call_id?: string;
}

interface OpenAIChunk {
  choices: Array<{
    delta: {
      content?: string;
      reasoning_content?: string;
      tool_calls?: Array<{ index: number; id?: string; type?: string; function?: { name?: string; arguments?: string } }>;
    };
    finish_reason?: string;
  }>;
  usage?: { prompt_tokens: number; completion_tokens: number };
}

async function callUpstream(
  upstream: string,
  apiKey: string,
  targetModel: string,
  messages: OpenAIMessage[],
  maxTokens?: number,
  temperature?: number,
  stream = false,
  tools?: unknown[],
  signal?: AbortSignal,
): Promise<Response> {
  const body: Record<string, unknown> = { model: targetModel, messages, stream };
  if (maxTokens) body.max_tokens = maxTokens;
  if (temperature != null) body.temperature = temperature;
  if (tools?.length) body.tools = tools;
  if (stream) body.stream_options = { include_usage: true };

  return fetch(`${upstream}/chat/completions`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Authorization": `Bearer ${apiKey}`,
    },
    body: JSON.stringify(body),
    signal,
  });
}

export function createChatService(db: DB) {
  return {
    async chat(req: ChatRequest, proxyId: string, upstream: string, apiKey: string, sourceModel: string, targetModel: string): Promise<ChatResponse> {
      const start = Date.now();
      const messages = req.messages.map(m => ({ role: m.role, content: m.content }));

      let inputTokens = 0n;
      let outputTokens = 0n;
      let content = "";
      let errMsg = "";
      let success = false;

      try {
        const resp = await callUpstream(upstream, apiKey, targetModel, messages, req.maxTokens, req.temperature);
        if (!resp.ok) throw new Error(`upstream ${resp.status}: ${await resp.text()}`);
        const json = await resp.json() as { choices: Array<{ message: { content: string } }>; usage?: { prompt_tokens: number; completion_tokens: number } };
        content = json.choices[0]?.message?.content ?? "";
        inputTokens = BigInt(json.usage?.prompt_tokens ?? 0);
        outputTokens = BigInt(json.usage?.completion_tokens ?? 0);
        success = true;
      } catch (e) {
        errMsg = String(e);
      }

      const durationMs = Date.now() - start;
      const total = inputTokens + outputTokens;

      await db.insert(logs).values({
        id: randomId(), proxyId, sourceModel, targetModel, upstream,
        inputTokens: Number(inputTokens), outputTokens: Number(outputTokens),
        totalTokens: Number(total), durationMs, timeToFirstTokenMs: 0,
        isStream: false, isSuccess: success, errorMessage: errMsg, createdAt: nowSec(),
      });

      return {
        $typeName: "pylon.v1.ChatResponse",
        content, inputTokens, outputTokens, totalTokens: total,
      } as unknown as ChatResponse;
    },

    async *streamChat(req: ChatRequest, proxyId: string, upstream: string, apiKey: string, sourceModel: string, targetModel: string): AsyncIterable<ChatChunk> {
      const start = Date.now();
      const messages = req.messages.map(m => ({ role: m.role, content: m.content }));

      let inputTokens = 0;
      let outputTokens = 0;
      let firstTokenMs = 0;
      let errMsg = "";
      let success = false;
      let first = true;

      try {
        const resp = await callUpstream(upstream, apiKey, targetModel, messages, req.maxTokens, req.temperature, true);
        if (!resp.ok) throw new Error(`upstream ${resp.status}: ${await resp.text()}`);

        const reader = resp.body!.getReader();
        const decoder = new TextDecoder();
        let buf = "";

        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          buf += decoder.decode(value, { stream: true });

          const lines = buf.split("\n");
          buf = lines.pop() ?? "";

          for (const line of lines) {
            if (!line.startsWith("data: ")) continue;
            const raw = line.slice(6).trim();
            if (raw === "[DONE]") continue;
            try {
              const chunk = JSON.parse(raw) as OpenAIChunk;
              const delta = chunk.choices[0]?.delta;
              if (delta?.content) {
                if (first) { firstTokenMs = Date.now() - start; first = false; }
                yield { $typeName: "pylon.v1.ChatChunk", delta: delta.content, done: false, inputTokens: 0n, outputTokens: 0n } as unknown as ChatChunk;
              }
              if (chunk.usage) {
                inputTokens = chunk.usage.prompt_tokens;
                outputTokens = chunk.usage.completion_tokens;
              }
            } catch { /* skip bad lines */ }
          }
        }
        success = true;
        yield { $typeName: "pylon.v1.ChatChunk", delta: "", done: true, inputTokens: BigInt(inputTokens), outputTokens: BigInt(outputTokens) } as unknown as ChatChunk;
      } catch (e) {
        errMsg = String(e);
        yield { $typeName: "pylon.v1.ChatChunk", delta: "", done: true, inputTokens: 0n, outputTokens: 0n } as unknown as ChatChunk;
      }

      const durationMs = Date.now() - start;
      await db.insert(logs).values({
        id: randomId(), proxyId, sourceModel, targetModel, upstream,
        inputTokens, outputTokens, totalTokens: inputTokens + outputTokens,
        durationMs, timeToFirstTokenMs: firstTokenMs,
        isStream: true, isSuccess: success, errorMessage: errMsg, createdAt: nowSec(),
      });
    },

    // OpenAI-compatible passthrough for mutalisk
    async openaiPassthrough(req: Request, db: DB): Promise<Response> {
      let body: Record<string, unknown>;
      try { body = await req.json() as Record<string, unknown>; }
      catch { return new Response(JSON.stringify({ error: "invalid json" }), { status: 400 }); }

      const model = body.model as string;
      if (!model) return new Response(JSON.stringify({ error: "model required" }), { status: 400 });

      const authHeader = req.headers.get("Authorization") ?? "";
      const agentName = authHeader.startsWith("Bearer ") ? authHeader.slice(7) : "";
      if (!agentName) return new Response(JSON.stringify({ error: { message: "authorization required", type: "authentication_error" } }), { status: 401, headers: { "Content-Type": "application/json" } });

      // Find proxy by source_model
      const { proxies } = await import("../db/schema.js");
      const { eq, and } = await import("drizzle-orm");
      const rows = await db.select().from(proxies).where(and(eq(proxies.sourceModel, model), eq(proxies.enabled, true)));
      const proxy = rows[0];
      if (!proxy) return new Response(JSON.stringify({ error: { message: `no proxy for model: ${model}`, type: "invalid_request_error" } }), { status: 404, headers: { "Content-Type": "application/json" } });

      // Check permission (always required)
      const { permissions } = await import("../db/schema.js");
      const perm = await db.select().from(permissions).where(and(eq(permissions.proxyId, proxy.id), eq(permissions.agentName, agentName)));
      if (perm.length === 0) return new Response(JSON.stringify({ error: { message: "permission denied", type: "permission_error" } }), { status: 403, headers: { "Content-Type": "application/json" } });

      // Forward to upstream with target model
      const forwarded = { ...body, model: proxy.targetModel };
      const start = Date.now();
      const isStream = !!body.stream;

      try {
        const upstreamResp = await fetch(`${proxy.upstream}/chat/completions`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Authorization": `Bearer ${proxy.apiKey}` },
          body: JSON.stringify(forwarded),
        });

        const durationMs = Date.now() - start;

        if (isStream) {
          // Stream: forward body directly, log after
          const [forLog, forClient] = upstreamResp.body!.tee();
          // async log
          void logStream(forLog, { db, proxyId: proxy.id, sourceModel: model, targetModel: proxy.targetModel, upstream: proxy.upstream, durationMs, agentName });

          return new Response(forClient, {
            status: upstreamResp.status,
            headers: {
              "Content-Type": "text/event-stream",
              "Cache-Control": "no-cache",
              "Transfer-Encoding": "chunked",
            },
          });
        } else {
          const json = await upstreamResp.json() as { usage?: { prompt_tokens: number; completion_tokens: number } };
          const inputTokens = json.usage?.prompt_tokens ?? 0;
          const outputTokens = json.usage?.completion_tokens ?? 0;
          await db.insert(logs).values({
            id: randomId(), proxyId: proxy.id, sourceModel: model, targetModel: proxy.targetModel, upstream: proxy.upstream,
            inputTokens, outputTokens, totalTokens: inputTokens + outputTokens,
            durationMs, timeToFirstTokenMs: 0, isStream: false, isSuccess: upstreamResp.ok,
            errorMessage: upstreamResp.ok ? "" : "upstream error", createdAt: nowSec(),
          });
          return new Response(JSON.stringify(json), { status: upstreamResp.status, headers: { "Content-Type": "application/json" } });
        }
      } catch (e) {
        return new Response(JSON.stringify({ error: { message: String(e), type: "server_error" } }), { status: 502, headers: { "Content-Type": "application/json" } });
      }
    },
  };
}

async function logStream(stream: ReadableStream, opts: { db: DB; proxyId: string; sourceModel: string; targetModel: string; upstream: string; durationMs: number; agentName: string }) {
  try {
    const reader = stream.getReader();
    const decoder = new TextDecoder();
    let buf = "";
    let inputTokens = 0, outputTokens = 0;
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buf += decoder.decode(value, { stream: true });
      const lines = buf.split("\n"); buf = lines.pop() ?? "";
      for (const line of lines) {
        if (!line.startsWith("data: ")) continue;
        const raw = line.slice(6).trim();
        if (raw === "[DONE]") continue;
        try {
          const chunk = JSON.parse(raw) as OpenAIChunk;
          if (chunk.usage) { inputTokens = chunk.usage.prompt_tokens; outputTokens = chunk.usage.completion_tokens; }
        } catch { /* ignore */ }
      }
    }
    await opts.db.insert(logs).values({
      id: randomId(), proxyId: opts.proxyId, sourceModel: opts.sourceModel, targetModel: opts.targetModel, upstream: opts.upstream,
      inputTokens, outputTokens, totalTokens: inputTokens + outputTokens,
      durationMs: opts.durationMs, timeToFirstTokenMs: 0, isStream: true, isSuccess: true,
      errorMessage: "", createdAt: nowSec(),
    });
  } catch { /* best-effort */ }
}
