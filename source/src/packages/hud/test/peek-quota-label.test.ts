import { test } from "node:test";
import assert from "node:assert/strict";
import { peekQuotaLabel } from "../src/scene/peek-quota-label.ts";

test("Buddy package labels from the overflowing card remain distinct", () => {
  for (const vendor of ["workbuddy", "codebuddy"]) {
    assert.equal(
      peekQuotaLabel(
        vendor,
        { name: "CodeBuddy个人版国内运营裂变包", shortLabel: "资源" },
        true,
      ),
      "活动积分",
    );
    assert.equal(
      peekQuotaLabel(
        vendor,
        { name: "CodeBuddy个人版拉新权益包", shortLabel: "资源" },
        true,
      ),
      "拉新积分",
    );
    assert.equal(
      peekQuotaLabel(
        vendor,
        { name: "每月免费积分", shortLabel: "月免费" },
        true,
      ),
      "月免费",
    );
  }
});

test("unknown packages and other providers keep their original labels", () => {
  assert.equal(peekQuotaLabel("workbuddy", { name: "新套餐" }, true), "新套餐");
  assert.equal(
    peekQuotaLabel("codex", { name: "运营裂变", shortLabel: "5h" }, true),
    "5h",
  );
  assert.equal(
    peekQuotaLabel("workbuddy", { name: "国内运营裂变包" }, false),
    "Activity",
  );
  assert.equal(
    peekQuotaLabel("codebuddy", { name: "拉新权益包" }, false),
    "Referral",
  );
});
