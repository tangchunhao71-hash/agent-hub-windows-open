#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { constants } from "node:os";
import { existsSync, readFileSync } from "node:fs";
import { randomUUID } from "node:crypto";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import {
  artifactRecord,
  pnpmCommand,
  preflight,
  readUpstreamReference,
  sourceSnapshot,
  writeReceipt,
} from "./build-provenance.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const platforms = { darwin: "macos", win32: "windows", linux: "linux" };

const help = `Usage: pnpm run build:platform -- [options]

  --platform host|macos|windows|linux  Build on the matching host OS (default: host)
  --component desktop|engine        Component to compile (default: desktop)
  --debug / --release                Profile (default: release; last flag wins)
  --bundle                          Let Tauri produce host-platform bundles
  --print-plan                      Print commands without starting build tools
  --check                           Check tools/resources without building or writing
  --receipt <file>                  Write a new, never-overwritten build receipt
  --help                            Show this help

This entry point does not cross-compile, install prerequisites, launch the app,
change client hooks, or publish a release. Desktop defaults to --no-bundle.`;

export function parseOptions(args, host = process.platform) {
  const options = {
    platform: "host",
    component: "desktop",
    debug: false,
    bundle: false,
    printPlan: false,
    check: false,
    receipt: null,
    help: false,
  };
  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (arg === "--") continue;
    if (arg === "--platform" || arg === "--component" || arg === "--receipt") {
      const value = args[++i];
      if (!value || value.startsWith("--"))
        throw new Error(`${arg} requires a value.`);
      options[arg.slice(2)] = value;
    } else if (arg === "--debug") options.debug = true;
    else if (arg === "--release") options.debug = false;
    else if (arg === "--bundle") options.bundle = true;
    else if (arg === "--print-plan") options.printPlan = true;
    else if (arg === "--check") options.check = true;
    else if (arg === "--help" || arg === "-h") options.help = true;
    else
      throw new Error(
        `Unknown option: ${arg}. Use --help for supported options.`,
      );
  }
  if (options.help) return options;
  if (!["host", "macos", "windows", "linux"].includes(options.platform))
    throw new Error(
      `Unsupported platform: ${options.platform}. Choose macos, windows, linux, or host.`,
    );
  if (!["desktop", "engine"].includes(options.component))
    throw new Error(
      `Unsupported component: ${options.component}. Choose desktop or engine.`,
    );
  if (options.bundle && options.component !== "desktop")
    throw new Error("--bundle applies only to the desktop component.");
  const native = platforms[host];
  if (!native)
    throw new Error(
      `Unsupported host OS: ${host}. Use a macOS, Windows, or Linux build host.`,
    );
  if (options.platform === "host") options.platform = native;
  if (options.platform !== native)
    throw new Error(
      `Cannot build ${options.platform} on ${native}. This entry point does not cross-compile; run it on a ${options.platform} host with that system's prerequisites.`,
    );
  return options;
}

export function buildPlan(options, env = process.env, host = process.platform) {
  let command;
  let args;
  let hookConfig;
  if (options.component === "engine") {
    command = "cargo";
    args = ["build", "--locked", "-p", "agent-hud-engine"];
    if (!options.debug) args.push("--release");
  } else {
    const pnpm = pnpmCommand(env, host);
    command = pnpm.command;
    args = [
      ...pnpm.prefix,
      "--filter",
      "@agent-hud/desktop",
      "exec",
      "tauri",
      "build",
    ];
    if (!options.bundle) args.push("--no-bundle");
    if (options.debug) args.push("--debug");
    // Override only the build hook; products/settings remain in their real config.
    hookConfig = {
      path: join(root, ".build", `tauri-${randomUUID()}.json`),
      value: {
        ...(options.platform === "windows" && options.bundle
          ? JSON.parse(
              readFileSync(
                join(
                  root,
                  "src/apps/desktop/src-tauri/windows-preview.conf.json",
                ),
                "utf8",
              ),
            )
          : {}),
        build: {
          beforeBuildCommand: {
            script: "node src/tooling/desktop-prebuild.mjs",
            cwd: root,
          },
        },
      },
    };
    args.push("--config", hookConfig.path);
  }
  return {
    platform: options.platform,
    component: options.component,
    profile: options.debug ? "debug" : "release",
    bundle: options.bundle,
    ...(hookConfig ? { hookConfig } : {}),
    commands: [
      {
        command,
        args,
        cwd: root,
        shell: false,
      },
    ],
  };
}

export function checkRustHost(platform, env, run) {
  const result = run("rustc", ["-vV"], { cwd: root, encoding: "utf8", env });
  if (result.error)
    throw new Error(
      `Cannot run rustc: ${result.error.message}. Install the host Rust toolchain first.`,
    );
  if (result.status !== 0)
    throw new Error(
      "rustc -vV failed. Repair the host Rust toolchain before building.",
    );
  const target = /^host: (\S+)$/m.exec(result.stdout)?.[1];
  const rustPlatform = target?.includes("apple-darwin")
    ? "macos"
    : target?.includes("windows")
      ? "windows"
      : target?.includes("linux")
        ? "linux"
        : undefined;
  if (rustPlatform !== platform)
    throw new Error(
      `Rust compiler host ${target ?? "(unknown)"} does not match ${platform}. Cross-compilation is not supported by this entry point.`,
    );
  const configured = env.CARGO_BUILD_TARGET?.trim();
  if (configured && configured !== target)
    throw new Error(
      `CARGO_BUILD_TARGET=${configured} differs from the compiler host ${target}. Remove the cross target or build on the matching host; this entry point does not cross-compile.`,
    );
  return target;
}

export function executePlan(plan, run = spawnSync) {
  for (const item of plan.commands) {
    const result = run(item.command, item.args, {
      cwd: item.cwd,
      stdio: "inherit",
      shell: item.shell === true,
      windowsHide: true,
    });
    if (result.error)
      throw new Error(
        `Could not start ${item.command}: ${result.error.message}`,
      );
    if (result.signal) return 128 + (constants.signals[result.signal] ?? 1);
    if (result.status !== 0) return result.status ?? 1;
  }
  return 0;
}

/** The source capture and receipt are real I/O, including failed compilers. */
export function executeRecordedBuild(
  plan,
  options,
  checks,
  { sourceRoot = root, env = process.env, run = spawnSync } = {},
) {
  const receipt = options.receipt
    ? resolve(options.receipt)
    : join(sourceRoot, ".build", `build-${Date.now()}-${process.pid}.json`);
  if (existsSync(receipt))
    throw Error(`Receipt already exists; choose a new file: ${receipt}`);
  const before = sourceSnapshot(sourceRoot);
  const startedAt = new Date().toISOString();
  let status = 1,
    after = null,
    error = null;
  const outputs = [];
  try {
    if (plan.hookConfig)
      writeReceipt(plan.hookConfig.path, plan.hookConfig.value);
    status = executePlan(plan, run);
    if (status === 0) {
      const targetDirectory = env.CARGO_TARGET_DIR
        ? resolve(sourceRoot, env.CARGO_TARGET_DIR)
        : join(sourceRoot, "target");
      const extension = plan.platform === "windows" ? ".exe" : "";
      outputs.push(
        artifactRecord(
          join(
            targetDirectory,
            plan.profile,
            `${options.component === "engine" ? "agent-hud-engine" : "agent-hud-next"}${extension}`,
          ),
          sourceRoot,
        ),
      );
      if (options.component === "desktop") {
        const resources = join(
          sourceRoot,
          "src/apps/desktop/src-tauri/resources",
        );
        for (const name of ["agent-hud-engine", "agent-hud-mcp"])
          outputs.push(
            artifactRecord(join(resources, `${name}${extension}`), sourceRoot),
          );
        if (options.bundle && plan.platform === "macos") {
          const config = JSON.parse(
            readFileSync(
              join(sourceRoot, "src/apps/desktop/src-tauri/tauri.conf.json"),
              "utf8",
            ),
          );
          const app = join(
            targetDirectory,
            plan.profile,
            "bundle/macos",
            config.productName + ".app",
          );
          for (const path of [
            "Contents/Info.plist",
            "Contents/MacOS/" + (config.mainBinaryName || "agent-hud-next"),
            "Contents/Resources/agent-hud-engine",
          ]) {
            outputs.push(artifactRecord(join(app, path), sourceRoot));
          }
        }
        if (options.bundle && plan.platform === "windows") {
          const config = JSON.parse(
            readFileSync(
              join(sourceRoot, "src/apps/desktop/src-tauri/tauri.conf.json"),
              "utf8",
            ),
          );
          outputs.push(
            artifactRecord(
              join(
                targetDirectory,
                plan.profile,
                "bundle/nsis",
                `${config.productName}_${config.version}_x64-setup.exe`,
              ),
              sourceRoot,
            ),
          );
        }
      }
    }
  } catch (failure) {
    status = 2;
    error = failure.message;
  }
  try {
    after = sourceSnapshot(sourceRoot);
  } catch (failure) {
    error = [error, failure.message].filter(Boolean).join("; ");
    status ||= 2;
  }
  const stable =
    after !== null &&
    before.head === after.head &&
    before.inputSha256 === after.inputSha256;
  writeReceipt(receipt, {
    schemaVersion: 1,
    status:
      status !== 0
        ? "build-failed"
        : stable
          ? "built"
          : "source-changed-during-build",
    exitCode: status || (stable ? 0 : 3),
    error,
    startedAt,
    finishedAt: new Date().toISOString(),
    target: checks.target,
    plan,
    checks,
    source: before,
    sourceAfter: after
      ? { head: after.head, inputSha256: after.inputSha256, dirty: after.dirty }
      : null,
    sourceStable: stable,
    upstreamReference: readUpstreamReference(sourceRoot),
    artifacts: outputs,
  });
  process.stdout.write(`Build receipt: ${receipt}\n`);
  if (error) process.stderr.write(`Build failure: ${error}\n`);
  if (!stable)
    process.stderr.write(
      "Build inputs changed or could not be verified after compilation. Rebuild after the working tree settles.\n",
    );
  return status || (stable ? 0 : 3);
}

export function main(args = process.argv.slice(2)) {
  try {
    const options = parseOptions(args);
    if (options.help) {
      process.stdout.write(`${help}\n`);
      return 0;
    }
    const plan = buildPlan(options);
    if (options.printPlan) {
      process.stdout.write(`${JSON.stringify(plan, null, 2)}\n`);
      return 0;
    }
    const checks = preflight(root, { component: options.component });
    const target = checks.target;
    if (options.check) {
      const before = sourceSnapshot(root);
      process.stdout.write(
        `${JSON.stringify({ status: "ready", plan, checks, source: { head: before.head, dirty: before.dirty, inputSha256: before.inputSha256 }, upstreamReference: readUpstreamReference(root) }, null, 2)}\n`,
      );
      return 0;
    }
    process.stdout.write(
      `Building ${options.component} for ${options.platform} (${target}), ${plan.profile}${options.bundle ? ", bundles enabled" : ", executable only"}.\n`,
    );
    return executeRecordedBuild(plan, options, checks);
  } catch (error) {
    process.stderr.write(
      `Build error: ${error instanceof Error ? error.message : String(error)}\n`,
    );
    return 2;
  }
}

if (
  process.argv[1] &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url
)
  process.exitCode = main();
