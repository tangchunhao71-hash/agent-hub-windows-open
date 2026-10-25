import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import {
  existsSync,
  lstatSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
  mkdirSync,
} from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";

export const sha256 = (bytes) =>
  createHash("sha256").update(bytes).digest("hex");
const slash = (value) => value.split(sep).join("/");
const json = (file) =>
  JSON.parse(readFileSync(file, "utf8").replace(/^\uFEFF/, ""));
const canonicalText = (bytes) =>
  bytes
    .toString("utf8")
    .replace(/^\uFEFF/, "")
    .replace(/\r\n/g, "\n");

function runGit(root, args) {
  const result = spawnSync("git", ["-c", "core.quotepath=false", ...args], {
    cwd: root,
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
    windowsHide: true,
  });
  if (result.error || result.status !== 0)
    throw Error(
      `Git provenance unavailable: ${result.error?.message ?? result.stderr?.trim()}`,
    );
  return result.stdout;
}

function entries(root, rel) {
  const absolute = join(root, rel);
  if (lstatSync(absolute).isSymbolicLink())
    throw Error(`Build input must not be a symlink: ${rel}`);
  if (!statSync(absolute).isDirectory()) return [slash(rel)];
  return readdirSync(absolute)
    .sort()
    .flatMap((name) => entries(root, join(rel, name)));
}

/** Only runtime relay inputs: protocol imports in both packages are type-only. */
export const mcpInputs = [
  "src/apps/mcp/src",
  "src/apps/mcp/package.json",
  "src/packages/engine-client/src",
  "src/packages/engine-client/package.json",
  "pnpm-lock.yaml",
  "pnpm-workspace.yaml",
];

export function mcpSourceFingerprint(root) {
  const files = mcpInputs
    .flatMap((rel) => entries(root, rel))
    .sort()
    .map((path) => ({
      path,
      sha256: sha256(canonicalText(readFileSync(join(root, path)))),
    }));
  return { sha256: sha256(JSON.stringify(files)), files };
}

/** Captures actual working-tree bytes, including untracked build inputs. */
export function sourceSnapshot(sourceRoot) {
  const repoRoot = runGit(sourceRoot, ["rev-parse", "--show-toplevel"]).trim();
  const head = runGit(repoRoot, ["rev-parse", "HEAD"]).trim();
  const sourcePrefix = slash(relative(repoRoot, sourceRoot));
  if (sourcePrefix.startsWith(".."))
    throw Error("Source directory is outside its repository");
  const selected = [
    sourcePrefix || ".",
    "scripts/build-windows.ps1",
    "docs/upstream-baseline.json",
    "docs/mcp-relay-windows-x64.json",
  ];
  const files = [
    ...new Set(
      runGit(repoRoot, [
        "ls-files",
        "-z",
        "--cached",
        "--others",
        "--exclude-standard",
        "--",
        ...selected,
      ])
        .split("\0")
        .filter(Boolean),
    ),
  ].sort();
  const rows = files.map((path) => {
    const file = join(repoRoot, path);
    if (!existsSync(file)) return { path, missing: true };
    if (lstatSync(file).isSymbolicLink())
      throw Error(`Build input must not be a symlink: ${path}`);
    const before = statSync(file),
      bytes = readFileSync(file),
      after = statSync(file);
    if (before.size !== after.size || before.mtimeMs !== after.mtimeMs)
      throw Error(`Build input changed while reading: ${path}`);
    return { path, bytes: bytes.length, sha256: sha256(bytes) };
  });
  const status = runGit(repoRoot, [
    "status",
    "--porcelain=v1",
    "-z",
    "--untracked-files=all",
    "--",
    ...selected,
  ])
    .split("\0")
    .filter(Boolean);
  return {
    head,
    dirty: status.length > 0,
    changes: status,
    inputSha256: sha256(JSON.stringify(rows)),
    files: rows,
  };
}

export function executableTarget(file) {
  const bytes = readFileSync(file);
  if (bytes.length < 64 || bytes.readUInt16LE(0) !== 0x5a4d)
    throw Error("Reused MCP relay must be a Windows PE executable");
  const offset = bytes.readUInt32LE(0x3c);
  if (
    offset < 64 ||
    offset + 24 > bytes.length ||
    bytes.readUInt32LE(offset) !== 0x00004550
  )
    throw Error("Invalid Windows PE executable");
  const machine = bytes.readUInt16LE(offset + 4);
  if (machine === 0x8664) return "x86_64-pc-windows-msvc";
  if (machine === 0xaa64) return "aarch64-pc-windows-msvc";
  throw Error(`Unsupported Windows PE machine: ${machine}`);
}

export function verifyMcpRelay({ executable, manifest, target, sourceRoot }) {
  if (!executable || !manifest)
    throw Error(
      "MCP reuse requires both AGENTHUD_MCP_EXE and AGENTHUD_MCP_MANIFEST; otherwise build from source with Bun",
    );
  const record = typeof manifest === "string" ? json(manifest) : manifest;
  if (record.schemaVersion !== 1 || record.kind !== "mcp-relay")
    throw Error("Unsupported MCP relay manifest");
  if (record.target !== target)
    throw Error(`MCP relay target mismatch: ${record.target} / ${target}`);
  if (executableTarget(executable) !== target)
    throw Error("MCP relay PE architecture differs from its manifest/target");
  const digest = sha256(readFileSync(executable));
  if (!/^[a-f0-9]{64}$/.test(record.sha256 ?? "") || record.sha256 !== digest)
    throw Error("MCP relay SHA-256 mismatch");
  if (
    !["https://github.com/jazzenchen/agent-hud-open", "local-git"].includes(
      record.source?.repository,
    ) ||
    !/^[a-f0-9]{40}$/.test(record.source?.commit ?? "")
  )
    throw Error("MCP relay needs its repository and source commit");
  if (
    !record.version ||
    !["github-release", "source-build"].includes(record.source.kind)
  )
    throw Error("MCP relay source/version missing");
  if (
    record.source.kind === "source-build" &&
    (typeof record.source.dirty !== "boolean" ||
      !/^[a-f0-9]{64}$/.test(record.source.inputSha256 ?? ""))
  )
    throw Error(
      "Source-built MCP relay needs HEAD, dirty state and input SHA-256",
    );
  if (
    record.source.kind === "github-release" &&
    (record.source.repository !==
      "https://github.com/jazzenchen/agent-hud-open" ||
      !/^v/.test(record.source.release ?? "") ||
      !/^[a-f0-9]{64}$/.test(record.source.installerSha256 ?? ""))
  )
    throw Error(
      "Released MCP relay needs its official source, release and verified installer SHA-256",
    );
  const inputs = mcpSourceFingerprint(sourceRoot);
  if (record.compatibility?.mcpSourceSha256 !== inputs.sha256)
    throw Error(
      "MCP relay source/lockfile compatibility fingerprint mismatch; rebuild relay from current source",
    );
  if (
    record.compatibility?.packageVersion !==
    json(join(sourceRoot, "src/apps/mcp/package.json")).version
  )
    throw Error("MCP relay package version mismatch");
  return {
    mode: "verified-reuse",
    target,
    version: record.version,
    sha256: digest,
    source: record.source,
    mcpSourceSha256: inputs.sha256,
  };
}

export function readUpstreamReference(sourceRoot) {
  const file = resolve(sourceRoot, "../docs/upstream-baseline.json");
  return existsSync(file)
    ? json(file)
    : { originalAncestor: null, status: "not-recorded" };
}

export function artifactRecord(file, sourceRoot) {
  if (!existsSync(file))
    throw Error(`Expected build artifact missing: ${file}`);
  return {
    path: slash(relative(sourceRoot, file)),
    bytes: statSync(file).size,
    sha256: sha256(readFileSync(file)),
  };
}

export function writeReceipt(file, value) {
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, JSON.stringify(value, null, 2) + "\n", { flag: "wx" });
}

export function pnpmCommand(env, host = process.platform) {
  if (env.AGENTHUD_PNPM_CLI)
    return {
      command: process.execPath,
      prefix: [resolve(env.AGENTHUD_PNPM_CLI)],
      shell: false,
    };
  if (host === "win32") {
    const locate = (name) => {
      const result = spawnSync("where.exe", [name], {
        env,
        encoding: "utf8",
        windowsHide: true,
      });
      return result.status === 0
        ? result.stdout.trim().split(/\r?\n/)[0]
        : null;
    };
    const shim = locate("pnpm.cmd");
    if (shim) {
      const cli = join(dirname(shim), "node_modules/pnpm/bin/pnpm.cjs");
      if (existsSync(cli))
        return { command: process.execPath, prefix: [cli], shell: false };
    }
    const executable = locate("pnpm.exe");
    if (executable) return { command: executable, prefix: [], shell: false };
    throw Error(
      "Set AGENTHUD_PNPM_CLI (or -PnpmCli) to pnpm.cjs; no standalone pnpm.exe or npm pnpm shim was found",
    );
  }
  return { command: "pnpm", prefix: [], shell: false };
}

function version(command, args, root, env, run, shell = false) {
  const result = run(command, args, {
    cwd: root,
    encoding: "utf8",
    env,
    shell,
    windowsHide: true,
  });
  if (result.error || result.status !== 0)
    throw Error(
      `Cannot run ${command}: ${result.error?.message ?? result.stderr?.trim() ?? result.status}`,
    );
  return result.stdout.trim();
}

export function preflight(
  sourceRoot,
  {
    component = "desktop",
    env = process.env,
    run = spawnSync,
    host = process.platform,
  } = {},
) {
  const tools = { node: process.version };
  if (Number(process.versions.node.split(".")[0]) < 24)
    throw Error("Node.js 24 or later is required");
  tools.rustc = version("rustc", ["-vV"], sourceRoot, env, run);
  const target = /^host: (\S+)$/m.exec(tools.rustc)?.[1];
  const os = { win32: "windows", darwin: "apple-darwin", linux: "linux" }[host];
  if (!target?.includes(os))
    throw Error(`Rust host ${target ?? "unknown"} does not match ${host}`);
  if (host === "win32" && !target.endsWith("windows-msvc"))
    throw Error("Windows builds require the MSVC Rust toolchain");
  if (env.CARGO_BUILD_TARGET && env.CARGO_BUILD_TARGET !== target)
    throw Error(
      "CARGO_BUILD_TARGET must match the Rust host; cross compilation is not supported",
    );
  tools.cargo = version("cargo", ["--version"], sourceRoot, env, run);
  if (!existsSync(join(sourceRoot, "vendor/tao/Cargo.toml")))
    throw Error("Vendored tao source is missing");
  if (host === "win32") {
    for (const tool of ["cl.exe", "link.exe", "rc.exe"])
      version("where.exe", [tool], sourceRoot, env, run);
    tools.msvc = "cl.exe, link.exe, rc.exe available in process PATH";
  }
  let relay = null;
  if (component === "desktop") {
    const pnpm = pnpmCommand(env, host);
    tools.pnpm = version(
      pnpm.command,
      [...pnpm.prefix, "--version"],
      sourceRoot,
      env,
      run,
      pnpm.shell,
    );
    const expected = json(join(sourceRoot, "package.json")).packageManager;
    if (`pnpm@${tools.pnpm}` !== expected)
      throw Error(`Expected ${expected}, found pnpm@${tools.pnpm}`);
    if (
      !existsSync(join(sourceRoot, "node_modules/.bin/tauri")) &&
      !existsSync(join(sourceRoot, "node_modules/.bin/tauri.cmd")) &&
      !existsSync(
        join(sourceRoot, "src/apps/desktop/node_modules/.bin/tauri.cmd"),
      ) &&
      !existsSync(join(sourceRoot, "src/apps/desktop/node_modules/.bin/tauri"))
    )
      throw Error(
        "Desktop dependencies missing; explicitly run pnpm install --frozen-lockfile first",
      );
    if (env.AGENTHUD_MCP_EXE || env.AGENTHUD_MCP_MANIFEST)
      relay = verifyMcpRelay({
        executable: env.AGENTHUD_MCP_EXE,
        manifest: env.AGENTHUD_MCP_MANIFEST,
        target,
        sourceRoot,
      });
    else {
      tools.bun = version(
        env.BUN_EXECUTABLE || "bun",
        ["--version"],
        sourceRoot,
        env,
        run,
      );
      relay = {
        mode: "source-build",
        mcpSourceSha256: mcpSourceFingerprint(sourceRoot).sha256,
      };
    }
  }
  return {
    target,
    tools,
    relay,
    lockfiles: ["Cargo.lock", "pnpm-lock.yaml"].map((path) => ({
      path,
      sha256: sha256(readFileSync(join(sourceRoot, path))),
    })),
  };
}
