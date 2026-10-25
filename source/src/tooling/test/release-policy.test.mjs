import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import assert from "node:assert/strict";

const source = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const read = (path) => readFileSync(resolve(source, path), "utf8");
const config = JSON.parse(read("src/apps/desktop/src-tauri/tauri.conf.json"));

test("release identity and installer ownership are independent of the preview", () => {
  assert.equal(config.productName, "Agent Bot");
  assert.equal(config.identifier, "app.agentbot.windows");
  assert.match(config.app.windows[0].title, /^Agent Bot/);
  const windows = JSON.parse(
    read("src/apps/desktop/src-tauri/windows-preview.conf.json"),
  );
  assert.equal(windows.bundle.windows.nsis.installMode, "currentUser");
  assert.equal(
    windows.bundle.windows.webviewInstallMode.type,
    "downloadBootstrapper",
  );
});

test("installer resources cannot sweep up local credentials or experiment files", () => {
  const windows = JSON.parse(
    read("src/apps/desktop/src-tauri/tauri.windows.conf.json"),
  );
  const resources = { ...config.bundle.resources, ...windows.bundle.resources };
  assert.deepEqual(
    Object.values(resources).sort(),
    [
      "LICENSE",
      "DEPENDENCY_NOTICES.txt",
      "RELEASE_LICENSE_SUPPLEMENT.txt",
      "LobeIcons-LICENSE.txt",
      "THIRD_PARTY.md",
      "THIRD_PARTY_NOTICES.txt",
      "agent-hud-engine.exe",
    ].sort(),
  );
  for (const path of Object.keys(resources)) {
    assert.ok(!path.endsWith("/") && !path.includes("*"), path);
    if (!path.endsWith(".exe"))
      assert.ok(
        readFileSync(resolve(source, "src/apps/desktop/src-tauri", path))
          .length > 0,
      );
  }
});

test("desktop release embeds the frontend and does not delegate legacy hooks", () => {
  const cargo = read("src/apps/desktop/src-tauri/Cargo.toml");
  assert.match(cargo, /default\s*=\s*\["custom-protocol"\]/);
  assert.match(cargo, /custom-protocol\s*=\s*\["tauri\/custom-protocol"\]/);
  assert.match(
    read("src/apps/desktop/src-tauri/build.rs"),
    /CARGO_FEATURE_CUSTOM_PROTOCOL/,
  );
  assert.match(
    read("src/apps/desktop/src-tauri/src/main.rs"),
    /let canonical_executable = None;/,
  );
});
