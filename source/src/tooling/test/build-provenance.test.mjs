// Agent Bot modifications, 2026-10-26; upstream portions retain their original licenses.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  executableTarget,
  mcpSourceFingerprint,
  pnpmCommand,
  sha256,
  sourceSnapshot,
  verifyMcpRelay,
} from "../build-provenance.mjs";
import { compareUpstream } from "../compare-upstream.mjs";
import { buildPlan, executeRecordedBuild, parseOptions } from "../build.mjs";
import { packageResources, resourcePlan } from "../package-resources.mjs";

const currentSource = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../../..",
);
const temporary = join(currentSource, ".build");
mkdirSync(temporary, { recursive: true });
function git(root, args) {
  const result = spawnSync("git", args, {
    cwd: root,
    encoding: "utf8",
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr);
}
function fixture() {
  const repo = mkdtempSync(join(temporary, "build tests 中文 - "));
  const root = join(repo, "source");
  const put = (path, text) => {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), text);
  };
  put(".gitignore", "target/\n");
  put("input.txt", "original\n");
  put("src/apps/mcp/src/index.ts", "console.log('relay');\n");
  put("src/apps/mcp/package.json", '{"version":"0.1.0"}\n');
  put("src/packages/engine-client/src/index.ts", "export const client = 1;\n");
  put("src/packages/engine-client/package.json", '{"version":"0.1.0"}\n');
  put("pnpm-lock.yaml", "lockfileVersion: 9\n");
  put("pnpm-workspace.yaml", "packages: []\n");
  git(repo, ["init", "--quiet"]);
  git(repo, ["add", "source"]);
  git(repo, [
    "-c",
    "user.name=Build fixture",
    "-c",
    "user.email=build@example.invalid",
    "commit",
    "--quiet",
    "-m",
    "fixture",
  ]);
  const exe = join(repo, "relay.exe");
  const bytes = Buffer.alloc(160);
  bytes.writeUInt16LE(0x5a4d, 0);
  bytes.writeUInt32LE(64, 0x3c);
  bytes.writeUInt32LE(0x4550, 64);
  bytes.writeUInt16LE(0x8664, 68);
  writeFileSync(exe, bytes);
  const manifest = {
    schemaVersion: 1,
    kind: "mcp-relay",
    target: "x86_64-pc-windows-msvc",
    version: "0.6.0-beta.3",
    sha256: sha256(bytes),
    source: {
      kind: "github-release",
      repository: "https://github.com/jazzenchen/agent-hud-open",
      commit: "a".repeat(40),
      release: "v0.6.0-beta.3",
      installerSha256: "b".repeat(64),
    },
    compatibility: {
      packageVersion: "0.1.0",
      mcpSourceSha256: mcpSourceFingerprint(root).sha256,
    },
  };
  const manifestFile = join(repo, "relay.json");
  writeFileSync(manifestFile, JSON.stringify(manifest));
  return { root, repo, put, exe, manifest, manifestFile, bytes };
}
function verify(f, overrides = {}) {
  return verifyMcpRelay({
    executable: f.exe,
    manifest: f.manifest,
    target: f.manifest.target,
    sourceRoot: f.root,
    ...overrides,
  });
}

test("verified relay refuses absent manifest, altered binary, wrong architecture and target", () => {
  const f = fixture();
  assert.equal(verify(f).mode, "verified-reuse");
  assert.throws(() => verify(f, { manifest: undefined }), /both/);
  assert.throws(
    () => verify(f, { target: "aarch64-pc-windows-msvc" }),
    /target mismatch/,
  );
  const arm = Buffer.from(f.bytes);
  arm.writeUInt16LE(0xaa64, 68);
  writeFileSync(f.exe, arm);
  assert.equal(executableTarget(f.exe), "aarch64-pc-windows-msvc");
  assert.throws(() => verify(f), /architecture/);
  const modified = Buffer.from(f.bytes);
  modified[159] = 1;
  writeFileSync(f.exe, modified);
  assert.throws(() => verify(f), /SHA-256/);
});

test("version text cannot override runtime-source or lockfile mismatch", () => {
  const f = fixture();
  f.put(
    "src/packages/engine-client/src/index.ts",
    "export const client = 2;\n",
  );
  assert.throws(() => verify(f), /compatibility fingerprint/);
  f.put(
    "src/packages/engine-client/src/index.ts",
    "export const client = 1;\n",
  );
  f.put("pnpm-lock.yaml", "lockfileVersion: 10\n");
  assert.throws(() => verify(f), /compatibility fingerprint/);
});

test("missing provenance and package mismatch are rejected", () => {
  const f = fixture();
  assert.throws(
    () => verify(f, { manifest: { ...f.manifest, source: {} } }),
    /source commit/,
  );
  assert.throws(
    () =>
      verify(f, {
        manifest: {
          ...f.manifest,
          compatibility: { ...f.manifest.compatibility, packageVersion: "9.0" },
        },
      }),
    /package version/,
  );
});

test("compatibility canonicalizes checkout line endings but notices source additions", () => {
  const f = fixture();
  f.put("src/apps/mcp/src/index.ts", "console.log('relay');\r\n");
  assert.equal(verify(f).mode, "verified-reuse");
  f.put("src/apps/mcp/src/extra.ts", "export {};\n");
  assert.throws(() => verify(f), /compatibility fingerprint/);
});

test("source snapshot binds clean, dirty, untracked and missing files to actual bytes", () => {
  const f = fixture();
  const clean = sourceSnapshot(f.root);
  assert.equal(clean.dirty, false);
  f.put("input.txt", "changed\n");
  f.put("added.txt", "new input\n");
  const changed = sourceSnapshot(f.root);
  assert.equal(changed.dirty, true);
  assert.notEqual(changed.inputSha256, clean.inputSha256);
  assert.equal(
    changed.files.find((row) => row.path === "source/input.txt").sha256,
    sha256("changed\n"),
  );
  assert(changed.files.some((row) => row.path === "source/added.txt"));
  unlinkSync(join(f.root, "input.txt"));
  assert.equal(
    sourceSnapshot(f.root).files.find((row) => row.path === "source/input.txt")
      .missing,
    true,
  );
});

test("pnpm CLI in a spaced path uses node argv rather than a shell command", () => {
  const cli = "C:/tool chain 中文/pnpm.cjs";
  const command = pnpmCommand({ AGENTHUD_PNPM_CLI: cli }, "win32");
  assert.equal(command.command, process.execPath);
  assert.equal(command.shell, false);
  const plan = buildPlan(
    parseOptions(["--platform", "windows", "--check"], "win32"),
    { AGENTHUD_PNPM_CLI: cli },
    "win32",
  );
  assert.equal(plan.commands[0].args[0], resolve(cli));
  assert.equal(plan.commands[0].shell, false);
});

test("Windows npm pnpm.cmd shim is resolved to its Node CLI and never executed", () => {
  const f = fixture(),
    commandRoot = join(f.repo, "pnpm tools 中文 path");
  const cli = join(commandRoot, "node_modules/pnpm/bin/pnpm.cjs");
  mkdirSync(dirname(cli), { recursive: true });
  writeFileSync(join(commandRoot, "pnpm.cmd"), "@exit /b 91\r\n");
  writeFileSync(cli, "console.log(JSON.stringify(process.argv.slice(2)))");
  const env = {
    ...process.env,
    PATH: commandRoot,
  };
  const selected = pnpmCommand(env, "win32");
  assert.equal(selected.command, process.execPath);
  assert.equal(selected.shell, false);
  assert.equal(selected.prefix[0], cli);
  const output = spawnSync(
    selected.command,
    [...selected.prefix, "--version"],
    { env, encoding: "utf8", shell: false, windowsHide: true },
  );
  assert.equal(output.status, 0);
  assert.deepEqual(JSON.parse(output.stdout), ["--version"]);
});

test("Windows npm .bin shims resolve Unicode paths without executing the shim", () => {
  const f = fixture();
  const directory = join(f.repo, "npm layout 中文", "node_modules", ".bin");
  const cli = resolve(directory, "../pnpm/bin/pnpm.cjs");
  mkdirSync(directory, { recursive: true });
  mkdirSync(dirname(cli), { recursive: true });
  writeFileSync(join(directory, "pnpm.cmd"), "@exit /b 91\r\n");
  writeFileSync(cli, "console.log('resolved-cli')");
  const selected = pnpmCommand({ Path: directory }, "win32");
  assert.equal(selected.command, process.execPath);
  const result = spawnSync(selected.command, selected.prefix, {
    encoding: "utf8",
  });
  assert.equal(result.status, 0);
  assert.equal(result.stdout.trim(), "resolved-cli");
});

test("resource packaging validates reuse before invoking Cargo", () => {
  const f = fixture();
  let calls = 0;
  assert.throws(
    () =>
      packageResources({
        root: f.root,
        host: "win32",
        arch: "x64",
        output: join(f.repo, "out"),
        env: { AGENTHUD_MCP_EXE: f.exe },
        run: () => {
          calls++;
          return { status: 0 };
        },
      }),
    /both/,
  );
  assert.equal(calls, 0);
});

test("verified reuse stages identical bytes, a manifest, and locked engine build", () => {
  const f = fixture(),
    out = join(f.repo, "out"),
    target = join(f.root, "target", "release");
  mkdirSync(target, { recursive: true });
  writeFileSync(join(target, "agent-hud-engine.exe"), "fixture engine");
  const calls = [];
  const result = packageResources({
    root: f.root,
    host: "win32",
    arch: "x64",
    output: out,
    env: { AGENTHUD_MCP_EXE: f.exe, AGENTHUD_MCP_MANIFEST: f.manifestFile },
    run: (command, args) => {
      calls.push({ command, args });
      return { status: 0 };
    },
  });
  assert.equal(result.relay.mode, "verified-reuse");
  assert.equal(calls.length, 1);
  assert(calls[0].args.includes("--locked"));
  assert.equal(
    sha256(readFileSync(join(out, "agent-hud-mcp.exe"))),
    f.manifest.sha256,
  );
  assert.equal(
    JSON.parse(readFileSync(join(out, "agent-hud-mcp.build.json"))).sha256,
    f.manifest.sha256,
  );
});

test("default resource mode invokes the source compiler rather than a vendor fallback", () => {
  const f = fixture(),
    out = join(f.repo, "out"),
    target = join(f.root, "target", "release");
  mkdirSync(target, { recursive: true });
  writeFileSync(join(target, "agent-hud-engine.exe"), "fixture engine");
  const calls = [];
  const result = packageResources({
    root: f.root,
    host: "win32",
    arch: "x64",
    output: out,
    env: { BUN_EXECUTABLE: "explicit bun" },
    run: (command, args) => {
      calls.push({ command, args });
      if (command === "explicit bun") writeFileSync(args.at(-1), f.bytes);
      return { status: 0 };
    },
  });
  assert.equal(result.relay.mode, "source-build");
  assert.equal(calls[1].command, "explicit bun");
  assert(calls[1].args.includes("src/apps/mcp/src/index.ts"));
  const manifest = JSON.parse(
    readFileSync(join(out, "agent-hud-mcp.build.json"), "utf8"),
  );
  assert.equal(manifest.source.repository, "local-git");
  assert.equal(typeof manifest.source.dirty, "boolean");
  assert.match(manifest.source.inputSha256, /^[a-f0-9]{64}$/);
  assert.equal(
    verifyMcpRelay({
      executable: join(out, "agent-hud-mcp.exe"),
      manifest,
      target: manifest.target,
      sourceRoot: f.root,
    }).mode,
    "verified-reuse",
  );
});

function runFixture(f, body, suffix) {
  const receipt = join(f.repo, `receipt-${suffix}.json`);
  const plan = {
    platform: "windows",
    component: "engine",
    profile: "release",
    commands: [
      {
        command: process.execPath,
        args: ["-e", body],
        cwd: f.root,
        shell: false,
      },
    ],
  };
  const code = executeRecordedBuild(
    plan,
    { component: "engine", receipt },
    { target: "x86_64-pc-windows-msvc" },
    { sourceRoot: f.root, env: {} },
  );
  return { code, receipt: JSON.parse(readFileSync(receipt, "utf8")) };
}

test("Windows bundle receipt binds the exact version installer", () => {
  const f = fixture();
  const files = {
    "src/apps/desktop/src-tauri/tauri.conf.json": JSON.stringify({
      productName: "Agent Bot",
      version: "0.1.1",
    }),
    "src/apps/desktop/src-tauri/resources/agent-hud-engine.exe": "engine",
    "src/apps/desktop/src-tauri/resources/agent-hud-mcp.exe":
      "development relay",
    "target/release/agent-hud-next.exe": "app",
    "target/release/bundle/nsis/Agent Bot_0.1.1_x64-setup.exe":
      "exact installer",
  };
  for (const [path, text] of Object.entries(files)) {
    mkdirSync(dirname(join(f.root, path)), { recursive: true });
    writeFileSync(join(f.root, path), text);
  }
  const receipt = join(f.repo, "bundle.json");
  const code = executeRecordedBuild(
    { platform: "windows", profile: "release", commands: [] },
    { component: "desktop", bundle: true, receipt },
    { target: "x86_64-pc-windows-msvc" },
    { sourceRoot: f.root, env: {} },
  );
  assert.equal(code, 0);
  const saved = JSON.parse(readFileSync(receipt, "utf8"));
  assert.equal(
    saved.artifacts.at(-1).sha256,
    sha256(Buffer.from("exact installer")),
  );
  assert.match(
    saved.artifacts.at(-1).path,
    /Agent Bot_0\.1\.1_x64-setup\.exe$/,
  );
});

test("real failed subprocess leaves a failure receipt with source identity", () => {
  const f = fixture();
  const result = runFixture(f, "process.exit(7)", "compiler-failed");
  assert.equal(result.code, 7);
  assert.equal(result.receipt.status, "build-failed");
  assert.match(result.receipt.source.head, /^[a-f0-9]{40}$/);
});

test("successful command without expected artifact is a recorded failure", () => {
  const f = fixture();
  const result = runFixture(f, "process.exit(0)", "missing-artifact");
  assert.equal(result.code, 2);
  assert.equal(result.receipt.status, "build-failed");
  assert.match(result.receipt.error, /artifact missing/);
});

test("source edits made by a real build process cannot produce a clean-bound receipt", () => {
  const f = fixture();
  const result = runFixture(
    f,
    "const fs=require('node:fs');fs.mkdirSync('target/release',{recursive:true});fs.writeFileSync('target/release/agent-hud-engine.exe','engine');fs.writeFileSync('input.txt','during build')",
    "source-changed",
  );
  assert.equal(result.code, 3);
  assert.equal(result.receipt.sourceStable, false);
  assert.equal(result.receipt.status, "source-changed-during-build");
  assert.notEqual(
    result.receipt.source.inputSha256,
    result.receipt.sourceAfter.inputSha256,
  );
});

test("stable dirty source gets explicit dirty identity and output digest", () => {
  const f = fixture();
  f.put("input.txt", "dirty before build\n");
  const result = runFixture(
    f,
    "const fs=require('node:fs');fs.mkdirSync('target/release',{recursive:true});fs.writeFileSync('target/release/agent-hud-engine.exe','engine')",
    "dirty-built",
  );
  assert.equal(result.code, 0);
  assert.equal(result.receipt.status, "built");
  assert.equal(result.receipt.source.dirty, true);
  assert.equal(result.receipt.sourceStable, true);
  assert.equal(result.receipt.artifacts[0].sha256, sha256("engine"));
  assert.equal(result.receipt.upstreamReference.originalAncestor, null);
});

test("no matching-host shortcut permits cross compilation or incomplete argument", () => {
  assert.throws(
    () => parseOptions(["--platform", "linux"], "win32"),
    /does not cross-compile/,
  );
  assert.throws(() => parseOptions(["--receipt"], "win32"), /requires a value/);
  assert.throws(
    () =>
      resourcePlan({ TAURI_ENV_TARGET_TRIPLE: "riscv64-unknown-linux-gnu" }),
    /Unsupported resource target/,
  );
});

test("Windows bundle uses user-level NSIS with explicit WebView2 bootstrapper", () => {
  const options = parseOptions(["--platform", "windows", "--bundle"], "win32");
  const plan = buildPlan(
    options,
    { AGENTHUD_PNPM_CLI: "C:/tools/pnpm.mjs" },
    "win32",
  );
  assert.deepEqual(plan.hookConfig.value.bundle.targets, ["nsis"]);
  assert.equal(
    plan.hookConfig.value.bundle.windows.nsis.installMode,
    "currentUser",
  );
  assert.equal(
    plan.hookConfig.value.bundle.windows.webviewInstallMode.type,
    "downloadBootstrapper",
  );
});

test("reference comparison separates edits, new and absent files without inventing ancestry", () => {
  const f = fixture(),
    reference = join(f.repo, "reference");
  mkdirSync(reference);
  writeFileSync(join(reference, "input.txt"), "original\r\n");
  writeFileSync(join(reference, "only-upstream.txt"), "upstream\n");
  const report = compareUpstream(f.root, reference);
  assert.deepEqual(report.newlineOnly, ["input.txt"]);
  assert.deepEqual(report.upstreamOnly, ["only-upstream.txt"]);
  assert(report.localOnly.includes("src/apps/mcp/src/index.ts"));
  assert.match(report.scope, /no proof of Git ancestry/);
});
