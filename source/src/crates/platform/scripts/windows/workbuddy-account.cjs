// Run ONLY with the installed WorkBuddy.exe and ELECTRON_RUN_AS_NODE=1.
// Official native loggerGet + official sym-v1 codec; never refreshes, signs in, claims or writes credentials.
const fs = require("fs"),
  path = require("path"),
  crypto = require("crypto");
// Output: {success:true,account:{uid},summary,freePackages,observedAt}; failures use fixed errorCategory.
// The stdout pipe is private to the caller: never log the account payload or native key.
function readBounded(file) {
  if (fs.statSync(file).size > 1048576) throw Error("input-too-large");
  return fs.readFileSync(file, "utf8");
}
function u32(n) {
  const b = Buffer.alloc(4);
  b.writeUInt32BE(n);
  return b;
}
function lp(s) {
  const b = Buffer.from(s);
  return Buffer.concat([u32(b.length), b]);
}
function decode(field, key) {
  if (typeof field === "string") return field;
  if (field?.$wbEncrypted !== 1 || field.scheme)
    throw Error("unsupported-envelope");
  const e = JSON.parse(Buffer.from(field.envelope, "base64"));
  const id = crypto.createHash("sha256").update(key).digest("hex").slice(0, 16);
  if (e.suite !== 1 || e.keyId !== id) throw Error("key-mismatch");
  const nonce = Buffer.from(e.nonce, "base64"),
    tag = Buffer.from(e.authTag, "base64");
  if (nonce.length !== 12 || tag.length !== 16) throw Error("invalid-envelope");
  const aad = Buffer.concat([
    Buffer.from("WB-AAD\0", "ascii"),
    Buffer.from([1]),
    lp("WBEV1"),
    lp("sym-v1"),
    u32(1),
    lp(id),
    Buffer.from([2, 0, 0]),
  ]);
  const d = crypto.createDecipheriv("aes-256-gcm", key, nonce, {
    authTagLength: 16,
  });
  d.setAAD(aad);
  d.setAuthTag(tag);
  const clear = Buffer.concat([
    d.update(Buffer.from(e.ciphertext, "base64")),
    d.final(),
  ]);
  try {
    return clear.toString("utf8");
  } finally {
    clear.fill(0);
  }
}
async function run() {
  const local = process.env.LOCALAPPDATA,
    p = path.join(
      local,
      "CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info",
    );
  if (fs.existsSync(p + ".logged-out")) throw Error("logged-out");
  const raw = readBounded(p),
    s = JSON.parse(raw),
    config = JSON.parse(
      readBounded(
        path.join(
          path.dirname(process.execPath),
          "resources/app.asar.unpacked/cli/product.json",
        ),
      ),
    );
  const endpoint = new URL(config.endpoint);
  if (
    endpoint.origin !== "https://www.workbuddy.cn" ||
    endpoint.pathname !== "/"
  )
    throw Error("endpoint-not-verified");
  if (
    !(config.authentication.attributes.internalDomain || []).includes(
      s.auth?.domain,
    )
  )
    throw Error("account-environment-not-verified");
  if (!s.account?.uid) throw Error("missing-account");
  if (s.account.enterpriseId)
    throw Error("enterprise-account-not-free-personal");
  let payload = JSON.parse(
    process._linkedBinding("electron_browser_workbuddy_storage").loggerGet(),
  );
  if (payload.version !== 1 || typeof payload.atRestSecretKey !== "string")
    throw Error("key-schema");
  const key = crypto
    .createHash("sha256")
    .update(payload.atRestSecretKey, "utf8")
    .digest();
  payload = null;
  let token;
  try {
    token = decode(s.auth.accessToken, key);
  } finally {
    key.fill(0);
  }
  const headers = {
    Authorization: "Bearer " + token,
    "X-User-Id": s.account.uid,
    "X-Domain": s.auth.domain,
    Accept: "application/json",
    "Content-Type": "application/json",
    "Accept-Language": "zh",
  };
  async function request(route, body) {
    const r = await fetch(new URL(route, endpoint), {
      method: "POST",
      headers,
      body: JSON.stringify(body),
      redirect: "error",
      signal: AbortSignal.timeout(12000),
    });
    if (!r.ok) throw Error("http-" + r.status);
    let size = 0;
    const chunks = [];
    for await (const chunk of r.body) {
      size += chunk.byteLength;
      if (size > 8388608) throw Error("response-too-large");
      chunks.push(Buffer.from(chunk));
    }
    const v = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    if (v.code !== 0 || !v.data) throw Error("api-unsuccessful");
    return v.data;
  }
  try {
    const summary = await request(
      "/billing/meter/get-user-resource-summary",
      {},
    );
    const start = new Date(),
      end = new Date();
    start.setHours(0, 0, 0, 0);
    end.setHours(23, 59, 59, 0);
    function date(d) {
      const pad = (n) => String(n).padStart(2, "0");
      return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
    }
    let rows = [];
    for (let page = 1; page <= 10; page++) {
      const v = await request(
        "/billing/meter/get-user-resource-free-packages",
        {
          PageNumber: page,
          PageSize: 200,
          PackageCodes: [
            "TCACA_code_001_PqouKr6QWV",
            "TCACA_code_008_cfWoLwvjU4",
            "TCACA_code_035_ArVxJcGDsm",
            "TCACA_code_006_DbXS0lrypC",
            "TCACA_code_039_KRcQj7wUat",
            "TCACA_code_040_mi9rCYg46x",
            "TCACA_code_007_nzdH5h4Nl0",
            "TCACA_code_028_NtpWi0jzXs",
            "TCACA_code_037_WxOD3MpI2o",
            "TCACA_code_029_6wCGEWquYy",
            "TCACA_code_030_BjSt89qTvr",
          ],
          Status: [0, 3],
          SlicePeriodStartTime: date(start),
          SlicePeriodEndTime: date(end),
        },
      );
      if (
        !Array.isArray(v.Accounts) ||
        !Number.isInteger(v.TotalCount) ||
        v.TotalCount < 0
      )
        throw Error("invalid-response");
      rows.push(...v.Accounts);
      if (rows.length >= v.TotalCount) break;
      if (!v.Accounts.length) throw Error("incomplete-response");
      if (page === 10) throw Error("page-limit");
    }
    if (readBounded(p) !== raw || fs.existsSync(p + ".logged-out"))
      throw Error("account-changed");
    const fields = [
      "ResourceId",
      "PackageCode",
      "PackageName",
      "CapacityType",
      "CapacityUnit",
      "Status",
      "CycleStartTime",
      "CycleEndTime",
      "DeductionStartTime",
      "DeductionEndTime",
      "ExpiredTime",
      "CycleCapacitySizePrecise",
      "CycleCapacityRemainPrecise",
      "CycleCapacityUsedPrecise",
      "CapacitySizePrecise",
      "CapacityRemainPrecise",
      "CapacityUsedPrecise",
      "SlicePeriodUsageDetails",
    ];
    return {
      success: true,
      account: {
        uid: s.account.uid,
        ...(s.account.enterpriseId
          ? { enterpriseId: s.account.enterpriseId }
          : {}),
      },
      summary,
      freePackages: rows.map((row) =>
        Object.fromEntries(
          fields.filter((k) => row[k] !== undefined).map((k) => [k, row[k]]),
        ),
      ),
      observedAt: new Date().toISOString(),
    };
  } finally {
    token = null;
    headers.Authorization = "";
  }
}
run()
  .then((v) => console.log(JSON.stringify(v)))
  .catch((e) => {
    const allowed =
      /^(http-\d+|api-unsuccessful|unsupported-envelope|key-mismatch|invalid-envelope|logged-out|endpoint-not-verified|account-environment-not-verified|missing-account|enterprise-account-not-free-personal|key-schema|page-limit|account-changed|input-too-large|response-too-large|invalid-response|incomplete-response)$/;
    console.log(
      JSON.stringify({
        success: false,
        errorCategory: allowed.test(e.message)
          ? e.message
          : e.name === "TimeoutError"
            ? "timeout"
            : "read-failed",
      }),
    );
    process.exitCode = 1;
  });
