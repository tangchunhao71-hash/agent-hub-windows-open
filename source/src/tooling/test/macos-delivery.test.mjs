import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import {
  machoArchitecture,
  packageMac,
  verifyBuildArtifact,
} from "../../../../scripts/package-macos.mjs";

test("delivery rejects a stale or modified app even if the version is identical", () => {
  const bytes = Buffer.from("current build");
  const path =
    "target/release/bundle/macos/Agent Bot.app/Contents/MacOS/agent-hud-next";
  const rows = [
    {
      path,
      bytes: bytes.length,
      sha256: createHash("sha256").update(bytes).digest("hex"),
    },
  ];
  verifyBuildArtifact(bytes, path, rows);
  assert.throws(
    () => verifyBuildArtifact(Buffer.from("stale build!!"), path, rows),
    /differs/,
  );
  assert.throws(() => verifyBuildArtifact(bytes, path, []), /differs/);
});

test("delivery refuses Windows, truncated and unrecognized executables", () => {
  for (const bytes of [
    Buffer.alloc(0),
    Buffer.from("MZ".repeat(64)),
    Buffer.alloc(64),
  ])
    assert.throws(() => machoArchitecture(bytes), /Mach-O/);
  const macho = Buffer.alloc(32);
  macho.writeUInt32LE(0xfeedfacf);
  macho.writeUInt32LE(7, 4);
  assert.throws(() => machoArchitecture(macho), /CPU/);
});
test("delivery distinguishes Apple Silicon and Intel executables", () => {
  for (const [cpu, arch] of [
    [0x0100000c, "arm64"],
    [0x01000007, "x64"],
  ]) {
    const bytes = Buffer.alloc(32);
    bytes.writeUInt32LE(0xfeedfacf);
    bytes.writeUInt32LE(cpu, 4);
    assert.equal(machoArchitecture(bytes), arch);
  }
});
test(
  "Mac packaging cannot produce a pretend app on a Windows host",
  { skip: process.platform === "darwin" },
  () => {
    assert.throws(() => packageMac("missing-receipt.json"), /requires macOS/);
  },
);
test("platform resources keep executables and experimental relay out of the Mac bundle", () => {
  const read = (name) =>
    JSON.parse(
      readFileSync(
        new URL("../../apps/desktop/src-tauri/" + name, import.meta.url),
        "utf8",
      ),
    );
  const base = read("tauri.conf.json"),
    mac = read("tauri.macos.conf.json"),
    win = read("tauri.windows.conf.json");
  const macFiles = Object.values({
    ...base.bundle.resources,
    ...mac.bundle.resources,
  });
  assert.equal(mac.identifier, "app.agentbot.macos");
  assert.deepEqual(mac.bundle.targets, ["app"]);
  assert.ok(macFiles.includes("agent-hud-engine"));
  assert.ok(macFiles.includes("LICENSE"));
  assert.ok(
    !macFiles.some((name) => name.endsWith(".exe") || name.includes("mcp")),
  );
  assert.equal(
    win.bundle.resources["resources/agent-hud-engine.exe"],
    "agent-hud-engine.exe",
  );
});
