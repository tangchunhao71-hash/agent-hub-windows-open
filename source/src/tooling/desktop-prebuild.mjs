// Tauri runs this hook with the same explicit pnpm selection as its outer runner.
import { spawnSync } from "node:child_process";
import { packageResources, sourceRoot } from "./package-resources.mjs";
import { pnpmCommand } from "./build-provenance.mjs";

packageResources();
const pnpm = pnpmCommand(process.env);
const result = spawnSync(
  pnpm.command,
  [...pnpm.prefix, "--filter", "@agent-hud/desktop", "run", "build"],
  {
    cwd: sourceRoot,
    env: process.env,
    stdio: "inherit",
    shell: false,
    windowsHide: true,
  },
);
if (result.error || result.status !== 0) {
  process.stderr.write(
    `Frontend build failed: ${result.error?.message ?? result.status}\n`,
  );
  process.exitCode = result.status || 1;
}
