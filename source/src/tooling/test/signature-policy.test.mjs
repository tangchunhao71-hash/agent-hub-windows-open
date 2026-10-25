// SPDX-License-Identifier: MIT
import test from "node:test";
import assert from "node:assert/strict";
import { signatureReport } from "../../../../scripts/signature-report.mjs";
test("ad hoc Mac signatures cannot be reported as trusted Developer ID", () => {
  const result = signatureReport("/tmp/app.app", "darwin", () => ({
    status: 0,
    stderr: "Signature=adhoc",
  }));
  assert.equal(result.codesignVerified, true);
  assert.equal(result.trusted, false);
  assert.equal(result.notarized, false);
});
test("Mac production trust requires Gatekeeper acceptance and notarization ticket", () => {
  const run = (tool, args) => ({
    status: args.includes("--assess") ? 1 : 0,
    stderr: "Authority=Developer ID Application: Example",
  });
  const result = signatureReport("/tmp/app.app", "darwin", run);
  assert.equal(result.trusted, false);
  assert.equal(result.notarized, false);
});
test("Windows missing, malformed and untrusted signatures stay untrusted", () => {
  for (const result of [
    { status: 1 },
    { status: 0, stdout: "not json" },
    { status: 0, stdout: '{"status":"NotSigned","trusted":false}' },
  ])
    assert.equal(
      signatureReport("fixture.exe", "win32", () => result).trusted,
      false,
    );
});

test("Mac identity command errors cannot establish trust from partial output", () => {
  const result = signatureReport("/tmp/app.app", "darwin", (tool, args) => ({
    status: args.includes("-dvv") ? 1 : 0,
    stderr: "Authority=Developer ID Application: Example",
  }));
  assert.equal(result.trusted, false);
  assert.equal(result.notarized, false);
});
