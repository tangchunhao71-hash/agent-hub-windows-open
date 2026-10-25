import test from "node:test";
import assert from "node:assert/strict";
import {
  existsSync,
  readFileSync,
  mkdirSync,
  mkdtempSync,
  writeFileSync,
} from "node:fs";
import { resolve, join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const source = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const template = readFileSync(
  join(source, "src/apps/desktop/src-tauri/windows/installer.nsi"),
  "utf8",
);
const makensis =
  process.env.AGENTBOT_MAKENSIS ||
  join(process.env.LOCALAPPDATA || "", "tauri/NSIS/makensis.exe");
const canRun = process.platform === "win32" && existsSync(makensis);
const functions = template.slice(
  template.indexOf("Function AgentBotPreflight"),
);
const preflight = functions.slice(
  0,
  functions.indexOf("Function AgentBotReadWebView"),
);
const policy = preflight.slice(preflight.indexOf("  ; VER_NT_WORKSTATION"));
const webview = functions.slice(
  functions.indexOf("Function AgentBotEnsureWebView"),
);
function runHarness(body, call) {
  const base = join(source, ".build");
  mkdirSync(base, { recursive: true });
  const dir = mkdtempSync(join(base, "installer-fixture-"));
  const exe = join(dir, "check.exe"),
    script = join(dir, "check.nsi");
  writeFileSync(
    script,
    `Unicode true\n!include "LogicLib.nsh"\nName "Agent Bot isolated test"\nOutFile "${exe}"\nSilentInstall silent\nRequestExecutionLevel user\nSection\nCall ${call}\nSetErrorLevel 0\nSectionEnd\n${body}`,
  );
  const compiled = spawnSync(
    makensis,
    ["/V2", "/INPUTCHARSET", "UTF8", script],
    { encoding: "utf8", windowsHide: true, timeout: 20000 },
  );
  assert.equal(compiled.status, 0, compiled.stdout + compiled.stderr);
  const ran = spawnSync(exe, ["/S"], { windowsHide: true, timeout: 10000 });
  assert.ifError(ran.error);
  return ran.status;
}
test("preflight precedes upgrade handling and WebView restoration precedes removal", () => {
  assert.match(template, /Function \.onInit\s+Call AgentBotPreflight/);
  assert.match(
    template,
    /Function PageLeaveReinstall\s+;[^\n]*\s+Call AgentBotEnsureWebView/,
  );
  assert.match(template, /Section WebView2\s+Call AgentBotEnsureWebView/);
  assert.doesNotMatch(webview, /\$UpdateMode/);
});
test(
  "compiled installer policy rejects old Windows, Server and incorrect architectures",
  { skip: !canRun },
  () => {
    // Exercise the production decision blocks; only the OS input collection is replaced.
    const architecture = preflight.slice(
      preflight.indexOf("  ; PROCESSOR_ARCHITECTURE_AMD64"),
      preflight.indexOf("  System::Alloc 284"),
    );
    for (const [arch, major, build, kind, expected] of [
      [9, 10, 22000, 1, 0],
      [9, 10, 26100, 1, 0],
      [9, 10, 19045, 1, 193],
      [9, 10, 26100, 3, 193],
      [12, 10, 26100, 1, 193],
      [0, 10, 26100, 1, 193],
    ]) {
      const body = `Function AgentBotPreflight\nStrCpy $1 ${arch}\nStrCpy $2 ${major}\nStrCpy $3 ${build}\nStrCpy $4 ${kind}\n${architecture}${policy}`;
      assert.equal(
        runHarness(body, "AgentBotPreflight"),
        expected,
        `${arch}/${major}/${build}/${kind}`,
      );
    }
  },
);
test(
  "real native preflight reads this Windows machine without installing anything",
  { skip: !canRun },
  () => {
    const probe = spawnSync(
      "powershell.exe",
      [
        "-NoProfile",
        "-Command",
        "$o=Get-CimInstance Win32_OperatingSystem; [pscustomobject]@{build=[int]$o.BuildNumber;kind=[int]$o.ProductType;arch=(Get-CimInstance Win32_Processor | Select-Object -First 1 -ExpandProperty Architecture)} | ConvertTo-Json -Compress",
      ],
      { encoding: "utf8", windowsHide: true, timeout: 20000 },
    );
    assert.equal(probe.status, 0, probe.stderr);
    const actual = JSON.parse(probe.stdout);
    const expected =
      actual.build >= 22000 && actual.kind === 1 && actual.arch === 9 ? 0 : 193;
    assert.equal(runHarness(preflight, "AgentBotPreflight"), expected);
  },
);
test(
  "compiled WebView control flow fails closed on download, install and registration errors",
  { skip: !canRun },
  () => {
    // No download, installer, browser or real registry is used in these fixtures.
    for (const [installed, download, exit, registered, expected] of [
      [true, "error", 1, false, 0],
      [false, "error", 0, false, 1],
      [false, "success", 5, false, 1],
      [false, "success", 0, false, 1],
      [false, "success", 0, true, 0],
      [false, "success", 3010, true, 3010],
    ]) {
      let body = webview
        .replace(/NSISdl::download[^\n]+/, `Push "${download}"`)
        .replace(/ExecWait[^\n]+/, `StrCpy $1 ${exit}`)
        .replace(/ExecShell[^\n]+/, "SetErrorLevel 99");
      body =
        `Var Calls\nFunction AgentBotReadWebView\nIntOp $Calls $Calls + 1\nStrCpy $4 \"\"\n` +
        `${installed ? 'StrCpy $4 "123.0.0.0"' : registered ? '${If} $Calls > 1\nStrCpy $4 "123.0.0.0"\n${EndIf}' : ""}\nFunctionEnd\n` +
        body;
      assert.equal(
        runHarness(body, "AgentBotEnsureWebView"),
        expected,
        JSON.stringify([installed, download, exit, registered]),
      );
    }
  },
);
