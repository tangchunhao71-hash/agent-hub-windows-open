import type { Quota, Snapshot } from "@agent-hud/protocol";
import { agentArtworkKey } from "./agent-artwork.ts";

/** An explicit account binding, never a blanket CodeBuddy/WorkBuddy alias. */
export function sharedQuotaAccount(
  snapshot: Snapshot,
  source: string,
): string | null {
  if (agentArtworkKey(source) !== "codebuddy") return null;
  const id = snapshot.settings.codebuddyWorkbuddyAccountId;
  if (
    !id ||
    !["codebuddy", "workbuddy"].every((provider) =>
      snapshot.providers.some((row) => row.id === provider && row.enabled),
    )
  )
    return null;
  return snapshot.accounts?.some(
    (account) => account.provider === "workbuddy" && account.id === id,
  )
    ? id
    : null;
}

/** Selects the original rows, so accounting/history keep a single quota sample. */
export function quotaMatchesSource(
  snapshot: Snapshot,
  quota: Quota,
  source: string,
): boolean {
  if (agentArtworkKey(quota.vendor) === agentArtworkKey(source)) return true;
  const account = sharedQuotaAccount(snapshot, source);
  return (
    account !== null &&
    quota.vendor === "workbuddy" &&
    quota.accountId === account
  );
}
