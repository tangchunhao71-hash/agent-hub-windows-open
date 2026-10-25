// Read-only Grok Bot account usage. No token renewal, login, app launch or writes.
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const { spawnSync } = require("node:child_process");
function bounded(file) {
  const stat = fs.lstatSync(file);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 1048576)
    throw Error("invalid-file");
  return fs.readFileSync(file, "utf8");
}
function b32(text) {
  let bits = 0,
    buffer = 0,
    result = "";
  const alphabet = "abcdefghijklmnopqrstuvwxyz234567";
  for (const b of Buffer.from(text)) {
    buffer = (buffer << 8) | b;
    bits += 8;
    while (bits >= 5) {
      bits -= 5;
      result += alphabet[(buffer >> bits) & 31];
    }
    buffer &= (1 << bits) - 1;
  }
  if (bits) result += alphabet[(buffer << (5 - bits)) & 31];
  return result;
}
function cached(dir, key) {
  const name = b32(key) + ".blob";
  const raw = bounded(
    path.join(
      dir,
      name.length <= 240
        ? name
        : b32(crypto.createHash("sha256").update(key).digest()) + ".kblob",
    ),
  );
  return JSON.parse(
    raw.startsWith("{") ? raw : raw.slice(raw.indexOf("\n") + 1),
  ).value;
}
function decrypt(value, key) {
  if (typeof value !== "string") throw Error("missing-login");
  if (value.startsWith("scoped:v1:"))
    value = value.slice(value.indexOf(":", 10) + 1);
  const blob = Buffer.from(value, "base64");
  if (blob.length < 31 || blob.subarray(0, 3).toString() !== "v10")
    throw Error("unsupported-storage");
  const decipher = crypto.createDecipheriv(
    "aes-256-gcm",
    key,
    blob.subarray(3, 15),
  );
  decipher.setAuthTag(blob.subarray(-16));
  const clear = Buffer.concat([
    decipher.update(blob.subarray(15, -16)),
    decipher.final(),
  ]);
  try {
    return clear.toString("utf8");
  } finally {
    clear.fill(0);
  }
}
async function request(route, body, token) {
  if (
    !["GetMe", "GetDailySpendByCategory", "ListGrokBotAgentStatuses"].includes(
      route,
    )
  )
    throw Error("invalid-response");
  const service =
    route === "ListGrokBotAgentStatuses"
      ? "GrokBotService"
      : "DashboardService";
  const response = await fetch(
    "https://api2.cursor.sh/aiserver.v1." + service + "/" + route,
    {
      method: "POST",
      redirect: "error",
      signal: AbortSignal.timeout(12000),
      headers: {
        Authorization: "Bearer " + token,
        "Content-Type": "application/json",
        "Connect-Protocol-Version": "1",
      },
      body: JSON.stringify(body),
    },
  );
  if (!response.ok) throw Error("http-" + response.status);
  let size = 0;
  const chunks = [];
  for await (const chunk of response.body) {
    size += chunk.byteLength;
    if (size > 8388608) throw Error("response-too-large");
    chunks.push(Buffer.from(chunk));
  }
  return JSON.parse(Buffer.concat(chunks).toString("utf8"));
}
async function readActivity(cache, slot, token, checkAccount) {
  const roster = cached(
    cache,
    "sand.client.slice.account." +
      encodeURIComponent(slot) +
      ".roster.last-roster",
  );
  if (!Array.isArray(roster?.rows) || roster.rows.length > 500)
    throw Error("invalid-response");
  const ids = roster.rows.map((row) => row.id);
  if (
    ids.some(
      (id) => typeof id !== "string" || !/^[a-zA-Z0-9_-]{1,128}$/.test(id),
    ) ||
    new Set(ids).size !== ids.length
  )
    throw Error("invalid-response");
  if (ids.length === 0) {
    checkAccount();
    return {
      success: true,
      accountSlot: slot,
      observedAt: new Date().toISOString(),
      statuses: [],
    };
  }
  const response = await request(
    "ListGrokBotAgentStatuses",
    { agentIds: ids },
    token,
  );
  if (
    !Array.isArray(response.statuses) ||
    response.statuses.length > ids.length
  )
    throw Error("invalid-response");
  checkAccount();
  return {
    success: true,
    accountSlot: slot,
    observedAt: new Date().toISOString(),
    statuses: response.statuses.map((row) => {
      const live = row.live;
      return {
        agentId: row.agentId,
        isRunning: row.isRunning ?? false,
        ...(live == null
          ? {}
          : {
              live: {
                agentId: live.agentId,
                isRunning: live.isRunning ?? false,
                hasRunningSubagents: live.hasRunningSubagents ?? false,
                isRetrying: live.isRetrying ?? false,
                updatedAtMs: live.updatedAtMs,
                staleAfterMs: live.staleAfterMs,
                waiting: live.awaiting != null,
                ...(live.activity == null
                  ? {}
                  : {
                      activity: {
                        kind: live.activity.kind,
                      },
                    }),
                ...(live.lastTurnSettlement == null
                  ? {}
                  : {
                      lastTurnSettlement: {
                        ...(typeof live.lastTurnSettlement.turnId ===
                          "string" &&
                        live.lastTurnSettlement.turnId.length > 0 &&
                        live.lastTurnSettlement.turnId.length <= 512 &&
                        !/[\u0000-\u001f\u007f]/.test(
                          live.lastTurnSettlement.turnId,
                        )
                          ? { turnId: live.lastTurnSettlement.turnId }
                          : {}),
                        outcome: [
                          "success",
                          "cancelled",
                          "error",
                          "failed",
                          "interrupted",
                        ].includes(live.lastTurnSettlement.outcome)
                          ? live.lastTurnSettlement.outcome
                          : undefined,
                        settledAtMs:
                          (typeof live.lastTurnSettlement.settledAtMs ===
                            "string" &&
                            /^[0-9]{1,19}$/.test(
                              live.lastTurnSettlement.settledAtMs,
                            )) ||
                          (typeof live.lastTurnSettlement.settledAtMs ===
                            "number" &&
                            Number.isSafeInteger(
                              live.lastTurnSettlement.settledAtMs,
                            ))
                            ? live.lastTurnSettlement.settledAtMs
                            : undefined,
                      },
                    }),
              },
            }),
      };
    }),
  };
}
async function run(mode = "usage") {
  if (mode !== "usage" && mode !== "activity") throw Error("invalid-response");
  const dir = path.join(process.env.APPDATA, "Grok Bot");
  const secretsFile = path.join(dir, "sand-secrets.json");
  const raw = bounded(secretsFile),
    secrets = JSON.parse(raw);
  if (secrets.version !== 1) throw Error("unsupported-storage");
  const accounts = JSON.parse(secrets["cursor-accounts"] || "{}");
  if (!accounts.active || !accounts.accounts?.[accounts.active])
    throw Error("missing-login");
  const active = accounts.accounts[accounts.active];
  const cache = path.join(dir, "sand-client-persistence");
  const slot = cached(cache, "sand.client.slice.client-meta.account-slot");
  if (typeof slot !== "string" || !slot) throw Error("missing-account");
  const local = JSON.parse(bounded(path.join(dir, "Local State")));
  const wrapped = Buffer.from(local.os_crypt?.encrypted_key || "", "base64");
  if (wrapped.subarray(0, 5).toString() !== "DPAPI")
    throw Error("unsupported-storage");
  const ps = path.join(
    process.env.SystemRoot,
    "System32/WindowsPowerShell/v1.0/powershell.exe",
  );
  const script =
    '$ErrorActionPreference="Stop"; Add-Type -AssemblyName System.Security; $b=[Convert]::FromBase64String([Console]::In.ReadToEnd()); $k=[Security.Cryptography.ProtectedData]::Unprotect($b,$null,[Security.Cryptography.DataProtectionScope]::CurrentUser); try{[Console]::Out.Write([Convert]::ToBase64String($k))}finally{[Array]::Clear($k,0,$k.Length)}';
  const result = spawnSync(
    ps,
    ["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script],
    {
      input: wrapped.subarray(5).toString("base64"),
      encoding: "utf8",
      windowsHide: true,
      timeout: 5000,
      maxBuffer: 65536,
    },
  );
  if (result.status !== 0) throw Error("secure-storage-unavailable");
  const key = Buffer.from(result.stdout.trim(), "base64");
  result.stdout = "";
  if (key.length !== 32) throw Error("secure-storage-unavailable");
  let token, team;
  try {
    token = decrypt(active["cursor-access-token"], key);
    const selection =
      active["cursor-selected-team-id"] || secrets["cursor-selected-team-id"];
    if (selection) {
      if (
        selection.startsWith("scoped:v1:") &&
        selection.split(":")[2] !== accounts.active
      )
        throw Error("account-mismatch");
      team = Number(decrypt(selection, key));
    }
  } finally {
    key.fill(0);
  }
  try {
    const parts = token.split(".");
    if (parts.length !== 3) throw Error("invalid-login");
    const claims = JSON.parse(Buffer.from(parts[1], "base64url"));
    if (typeof claims.exp !== "number" || claims.exp * 1000 <= Date.now())
      throw Error("expired-login");
    if (
      typeof claims.sub !== "string" ||
      crypto.createHash("sha256").update(claims.sub).digest("hex") !==
        accounts.active
    )
      throw Error("account-mismatch");
    // Installed main-app.cjs Jp checks (parseJwtPayload(token).sub ?? email)
    // against accountSlot before authenticated Bot dispatch. This login has sub.
    if (slot !== claims.sub) throw Error("account-mismatch");
    if (team !== undefined && (!Number.isSafeInteger(team) || team <= 0))
      throw Error("invalid-team");
    if (mode === "activity")
      return await readActivity(cache, slot, token, () => {
        if (
          bounded(secretsFile) !== raw ||
          cached(cache, "sand.client.slice.client-meta.account-slot") !== slot
        )
          throw Error("account-changed");
      });
    const me = await request("GetMe", {}, token);
    if (!me.userId) throw Error("invalid-response");
    const end = Date.now(),
      start = new Date(end);
    // DailySpend.day is a UTC-midnight millisecond bucket. Request whole UTC
    // days so the first bucket cannot precede the reported period start.
    start.setUTCHours(0, 0, 0, 0);
    start.setUTCDate(start.getUTCDate() - 7);
    const reply = await request(
      "GetDailySpendByCategory",
      {
        userId: String(me.userId),
        ...(team ? { teamId: team } : {}),
        periodStartMs: String(start.getTime()),
        periodEndMs: String(end),
        // Official protobuf enums: SpendGroupByCategory.GROK_BOT = 4,
        // SpendType.ALL = 3. Short strings silently fall back to MODEL.
        groupBy: 4,
        spendType: 3,
        clientType: "grok-bot",
        products: ["sand"],
        includeCreditUsage: true,
      },
      token,
    );
    if (!Array.isArray(reply.dailySpend)) throw Error("invalid-response");
    if (
      bounded(secretsFile) !== raw ||
      cached(cache, "sand.client.slice.client-meta.account-slot") !== slot
    )
      throw Error("account-changed");
    return {
      success: true,
      accountSlot: slot,
      observedAt: new Date().toISOString(),
      periodStart: start.toISOString(),
      periodEnd: new Date(end).toISOString(),
      dailySpend: reply.dailySpend.map((row) => ({
        category: row.category,
        day: row.day,
        totalTokens: row.totalTokens,
      })),
    };
  } finally {
    token = null;
  }
}
run(process.argv[1] === "activity" ? "activity" : "usage")
  .then((value) => console.log(JSON.stringify(value)))
  .catch((error) => {
    const allowed =
      /^(invalid-file|missing-login|missing-account|unsupported-storage|secure-storage-unavailable|invalid-login|expired-login|account-mismatch|invalid-team|invalid-response|account-changed|response-too-large|http-\d+)$/;
    console.log(
      JSON.stringify({
        success: false,
        errorCategory: allowed.test(error.message)
          ? error.message
          : "read-failed",
      }),
    );
    process.exitCode = 1;
  });
