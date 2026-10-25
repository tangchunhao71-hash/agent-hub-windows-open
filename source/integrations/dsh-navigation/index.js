import { createServer } from "node:http";
import { randomBytes } from "node:crypto";
import { mkdir, readFile, writeFile, unlink } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";

export const validId = (id) =>
  typeof id === "string" && /^[A-Za-z0-9_-]{1,128}$/.test(id);
const reply = (res, code, value) => {
  res.writeHead(code, {
    "content-type": "application/json",
    "cache-control": "no-store",
  });
  res.end(JSON.stringify(value));
};
async function body(req) {
  let bytes = 0,
    chunks = [];
  for await (const chunk of req) {
    bytes += chunk.length;
    if (bytes > 1024) throw new Error("Request too large");
    chunks.push(chunk);
  }
  return JSON.parse(Buffer.concat(chunks));
}

// The private native endpoint accepts identifiers only. The DSH client must
// confirm its actual main view before the native caller receives success.
export async function createBridge({ directory, timeout = 6500 }) {
  const token = randomBytes(32).toString("hex");
  const path = join(directory, "agent-hub-navigation.json");
  let pending,
    selected = null,
    clientAt = 0;
  const settle = (ok) => {
    if (!pending) return;
    const current = pending;
    pending = undefined;
    clearTimeout(current.timer);
    current.finish(ok);
  };
  const server = createServer(async (req, res) => {
    try {
      if (
        req.headers.origin ||
        req.headers.authorization !== `Bearer ${token}` ||
        !/^127\.0\.0\.1:\d+$/.test(req.headers.host || "")
      )
        return reply(res, 403, { ok: false });
      if (req.url === "/status" && req.method === "GET") {
        return reply(res, 200, {
          version: 1,
          selected,
          clientReady: Date.now() - clientAt < 3000,
        });
      }
      if (req.url !== "/open" || req.method !== "POST")
        return reply(res, 404, { ok: false });
      const value = await body(req);
      if (
        !validId(value.sessionId) ||
        !validId(value.requestId) ||
        Object.keys(value).some(
          (key) => !["sessionId", "requestId"].includes(key),
        )
      )
        return reply(res, 400, { ok: false });
      if (pending) return reply(res, 409, { ok: false });
      if (Date.now() - clientAt >= 3000)
        return reply(res, 503, {
          ok: false,
          reason: "desktop-client-unavailable",
        });
      const ok = await new Promise((finish) => {
        pending = {
          ...value,
          finish,
          claimed: false,
          timer: setTimeout(() => settle(false), timeout),
        };
        res.once("close", () => {
          if (!res.writableEnded && pending?.requestId === value.requestId)
            settle(false);
        });
      });
      return reply(res, ok ? 200 : 504, {
        ok,
        requestId: value.requestId,
        sessionId: value.sessionId,
      });
    } catch {
      if (!res.headersSent) reply(res, 400, { ok: false });
    }
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const descriptor = { version: 1, port: server.address().port, token };
  try {
    await mkdir(directory, { recursive: true });
    await writeFile(path, JSON.stringify(descriptor), { mode: 0o600 });
  } catch (error) {
    server.close();
    throw error;
  }
  return {
    descriptor,
    // Served through DSH's own authenticated web server. No CORS, no token in
    // URLs, no chat content. Only the desktop renderer may consume requests.
    async client(req, res) {
      try {
        if (
          req.method !== "POST" ||
          req.headers["x-agent-hub-client"] !== "1" ||
          req.headers["sec-fetch-site"] === "cross-site"
        )
          return reply(res, 403, { ok: false });
        const origin = req.headers.origin;
        if (
          origin &&
          origin !== "dsh-app://app" &&
          origin !== `http://${req.headers.host}`
        )
          return reply(res, 403, { ok: false });
        const value = await body(req);
        if (
          value.desktop !== true ||
          (value.selected !== null && !validId(value.selected))
        )
          return reply(res, 400, { ok: false });
        selected = value.selected;
        clientAt = Date.now();
        if (pending && value.requestId === pending.requestId)
          settle(value.ok === true && selected === pending.sessionId);
        const next =
          pending && !pending.claimed
            ? { requestId: pending.requestId, sessionId: pending.sessionId }
            : null;
        if (next) pending.claimed = true;
        reply(res, 200, { pending: next });
      } catch {
        if (!res.headersSent) reply(res, 400, { ok: false });
      }
    },
    async close() {
      settle(false);
      await new Promise((resolve) => server.close(resolve));
      try {
        if (JSON.parse(await readFile(path, "utf8")).token === token)
          await unlink(path);
      } catch {
        /* already removed or superseded */
      }
    },
  };
}

export function apply(root) {
  root.inject(["webServer", "connection"], (ctx) => {
    let stopped = false,
      bridge,
      unregister;
    const ready = createBridge({
      directory: process.env.DSH_HOME || join(homedir(), ".dsh"),
    }).then(async (value) => {
      bridge = value;
      if (stopped) return bridge.close();
      unregister = ctx.webServer.register({
        kind: "exact",
        path: "/agent-hub-navigation/client",
        handler(req, res) {
          const rejected = ctx.connection.requestRejection(req);
          if (rejected)
            return reply(res, typeof rejected === "number" ? rejected : 403, {
              ok: false,
            });
          return bridge.client(req, res);
        },
      });
    });
    ready.catch(() =>
      console.error("[agent-hub-navigation] Local bridge could not start"),
    );
    ctx.effect(() => () => {
      stopped = true;
      unregister?.();
      void ready.then(() => bridge?.close()).catch(() => {});
    });
  });
}
