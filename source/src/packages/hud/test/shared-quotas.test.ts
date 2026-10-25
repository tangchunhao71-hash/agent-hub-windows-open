import { test } from "node:test";
import assert from "node:assert/strict";
import type { Snapshot } from "@agent-hud/protocol";
import {
  quotaMatchesSource,
  sharedQuotaAccount,
} from "../src/shared-quotas.ts";

function fixture() {
  return {
    settings: {},
    accounts: [{ id: "workbuddy:account-a", provider: "workbuddy" }],
    providers: [
      { id: "workbuddy", enabled: true },
      { id: "codebuddy", enabled: true },
    ],
    quotas: [
      {
        id: "original-window",
        vendor: "workbuddy",
        accountId: "workbuddy:account-a",
        usedPercent: 60,
      },
    ],
  } as unknown as Snapshot;
}
test("same product names never automatically share an account", () => {
  const snapshot = fixture();
  assert.equal(sharedQuotaAccount(snapshot, "codebuddy"), null);
  assert.equal(
    quotaMatchesSource(snapshot, snapshot.quotas[0], "codebuddy"),
    false,
  );
});
test("explicit binding reuses original rows and updated readings without duplicating ledger samples", () => {
  const snapshot = fixture();
  snapshot.settings.codebuddyWorkbuddyAccountId = "workbuddy:account-a";
  const rows = snapshot.quotas.filter((q) =>
    quotaMatchesSource(snapshot, q, "CodeBuddy"),
  );
  assert.equal(rows.length, 1);
  assert.equal(rows[0], snapshot.quotas[0]);
  assert.equal(snapshot.quotas.length, 1);
  snapshot.quotas[0].usedPercent = 73;
  assert.equal(rows[0].usedPercent, 73);
  assert.equal(quotaMatchesSource(snapshot, rows[0], "workbuddy"), true);
});
test("switching or removing the active account immediately stops sharing", () => {
  const snapshot = fixture();
  snapshot.settings.codebuddyWorkbuddyAccountId = "workbuddy:account-a";
  snapshot.accounts![0].id = "workbuddy:account-b";
  assert.equal(
    quotaMatchesSource(snapshot, snapshot.quotas[0], "codebuddy"),
    false,
  );
  snapshot.accounts = [];
  assert.equal(sharedQuotaAccount(snapshot, "codebuddy"), null);
});
test("a link selects only the confirmed account and respects source switches", () => {
  const snapshot = fixture();
  snapshot.settings.codebuddyWorkbuddyAccountId = "workbuddy:account-a";
  const other = { ...snapshot.quotas[0], accountId: "workbuddy:account-b" };
  assert.equal(quotaMatchesSource(snapshot, other, "codebuddy"), false);
  for (const source of snapshot.providers) {
    source.enabled = false;
    assert.equal(sharedQuotaAccount(snapshot, "codebuddy"), null);
    source.enabled = true;
  }
  snapshot.quotas = [];
  assert.deepEqual(
    snapshot.quotas.filter((q) => quotaMatchesSource(snapshot, q, "codebuddy")),
    [],
  );
});
