import { test } from "node:test";
import assert from "node:assert/strict";
import { completionPreferences } from "../src/lib/completion-preferences.ts";
test("a legacy disabled combined switch keeps notifications disabled on upgrade", () => {
  assert.equal(
    completionPreferences({ completionDrops: false }).completionNotifications,
    false,
  );
  assert.equal(
    completionPreferences({ completionDrops: true }).completionNotifications,
    true,
  );
});
test("delivery and animation can be selected independently after migration", () => {
  for (const notifications of [true, false])
    for (const drops of [true, false]) {
      assert.equal(
        completionPreferences({
          completionNotifications: notifications,
          completionDrops: drops,
        }).completionNotifications,
        notifications,
      );
    }
  assert.equal(completionPreferences({}).completionNotifications, true);
});
