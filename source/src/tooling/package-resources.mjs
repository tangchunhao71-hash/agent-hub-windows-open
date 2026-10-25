import { spawnSync } from "node:child_process";
import {
  mkdirSync,
  copyFileSync,
  chmodSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import { join, resolve } from "node:path";
import {
  artifactRecord,
  mcpSourceFingerprint,
  sourceSnapshot,
  verifyMcpRelay,
} from "./build-provenance.mjs";

export const sourceRoot = fileURLToPath(new URL("../../", import.meta.url));

export function resourcePlan(
  env = process.env,
  host = process.platform,
  arch = process.arch,
) {
  const triple =
    env.TAURI_ENV_TARGET_TRIPLE ||
    {
      win32: `${arch === "arm64" ? "aarch64" : "x86_64"}-pc-windows-msvc`,
      darwin: `${arch === "arm64" ? "aarch64" : "x86_64"}-apple-darwin`,
      linux: `${arch === "arm64" ? "aarch64" : "x86_64"}-unknown-linux-gnu`,
    }[host];
  if (!triple) throw Error(`Unsupported resource target host: ${host}`);
  const platform = triple.includes("windows")
    ? "windows"
    : triple.includes("apple")
      ? "darwin"
      : triple.includes("linux")
        ? "linux"
        : null;
  const cpu = triple.startsWith("aarch64")
    ? "arm64"
    : triple.startsWith("x86_64")
      ? "x64"
      : null;
  if (!platform || !cpu) throw Error(`Unsupported resource target: ${triple}`);
  return {
    triple,
    platform,
    cpu,
    extension: platform === "windows" ? ".exe" : "",
    debug: env.TAURI_ENV_DEBUG === "true",
    reuse: Boolean(env.AGENTHUD_MCP_EXE || env.AGENTHUD_MCP_MANIFEST),
  };
}

export function packageResources({
  root = sourceRoot,
  output = join(root, "src/apps/desktop/src-tauri/resources"),
  env = process.env,
  run = spawnSync,
  host = process.platform,
  arch = process.arch,
} = {}) {
  const plan = resourcePlan(env, host, arch);
  // Refuse unverified reuse before starting a potentially expensive engine build.
  const relay = plan.reuse
    ? verifyMcpRelay({
        executable: env.AGENTHUD_MCP_EXE,
        manifest: env.AGENTHUD_MCP_MANIFEST,
        target: plan.triple,
        sourceRoot: root,
      })
    : null;
  const before = relay ? null : sourceSnapshot(root);
  const execute = (command, args) => {
    const result = run(command, args, {
      cwd: root,
      env,
      stdio: "inherit",
      windowsHide: true,
    });
    if (result.error || result.status !== 0)
      throw Error(
        `${command} failed (${result.status}): ${result.error?.message ?? ""}`,
      );
  };
  mkdirSync(output, { recursive: true });
  const targetDirectory = env.CARGO_TARGET_DIR
    ? resolve(root, env.CARGO_TARGET_DIR)
    : join(root, "target");
  execute("cargo", [
    "build",
    "--locked",
    "-p",
    "agent-hud-engine",
    ...(plan.debug ? [] : ["--release"]),
    ...(env.TAURI_ENV_TARGET_TRIPLE ? ["--target", plan.triple] : []),
  ]);
  copyFileSync(
    join(
      targetDirectory,
      ...(env.TAURI_ENV_TARGET_TRIPLE ? [plan.triple] : []),
      plan.debug ? "debug" : "release",
      `agent-hud-engine${plan.extension}`,
    ),
    join(output, `agent-hud-engine${plan.extension}`),
  );
  const relayOutput = join(output, `agent-hud-mcp${plan.extension}`);
  const inputs = mcpSourceFingerprint(root);
  if (relay) copyFileSync(env.AGENTHUD_MCP_EXE, relayOutput);
  else
    execute(env.BUN_EXECUTABLE || "bun", [
      "build",
      "--compile",
      `--target=bun-${plan.platform}-${plan.cpu}`,
      "src/apps/mcp/src/index.ts",
      "--outfile",
      relayOutput,
    ]);
  if (mcpSourceFingerprint(root).sha256 !== inputs.sha256)
    throw Error(
      "MCP build inputs changed while packaging; refusing an unbound resource",
    );
  const binary = artifactRecord(relayOutput, root);
  const after = relay ? null : sourceSnapshot(root);
  if (
    before &&
    (before.head !== after.head || before.inputSha256 !== after.inputSha256)
  )
    throw Error(
      "Source changed while building MCP; rebuild after the source tree settles",
    );
  const source = relay?.source ?? {
    kind: "source-build",
    repository: "local-git",
    commit: before.head,
    dirty: before.dirty,
    inputSha256: before.inputSha256,
  };
  const manifest = {
    schemaVersion: 1,
    kind: "mcp-relay",
    target: plan.triple,
    version:
      relay?.version ??
      JSON.parse(readFileSync(join(root, "src/apps/mcp/package.json"), "utf8"))
        .version,
    sha256: binary.sha256,
    source,
    compatibility: {
      mcpSourceSha256: inputs.sha256,
      packageVersion: JSON.parse(
        readFileSync(join(root, "src/apps/mcp/package.json"), "utf8"),
      ).version,
    },
  };
  writeFileSync(
    join(output, "agent-hud-mcp.build.json"),
    JSON.stringify(manifest, null, 2) + "\n",
  );
  if (plan.platform !== "windows") {
    chmodSync(join(output, "agent-hud-engine"), 0o755);
    chmodSync(relayOutput, 0o755);
  }
  return {
    ...plan,
    relay: { ...manifest, mode: relay ? "verified-reuse" : "source-build" },
  };
}

if (
  process.argv[1] &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url
) {
  try {
    const args = process.argv.slice(2);
    if (args.length && (args.length !== 2 || args[0] !== "--out"))
      throw Error("Usage: package-resources.mjs [--out directory]");
    packageResources({ output: args.length ? resolve(args[1]) : undefined });
  } catch (error) {
    process.stderr.write(`Resource packaging error: ${error.message}\n`);
    process.exitCode = 1;
  }
}
