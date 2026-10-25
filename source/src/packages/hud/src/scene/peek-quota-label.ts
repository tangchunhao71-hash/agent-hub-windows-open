/** Compact labels for the Buddy packages shown in the hover card.
 * Keep the original quota name for the tooltip and account details. */
export function peekQuotaLabel(
  vendor: string,
  quota: { name: string; shortLabel?: string },
  zh: boolean,
): string {
  if (/^(workbuddy|codebuddy)$/i.test(vendor)) {
    if (quota.name.includes("运营裂变")) return zh ? "活动积分" : "Activity";
    if (quota.name.includes("拉新权益")) return zh ? "拉新积分" : "Referral";
  }
  return quota.shortLabel || quota.name;
}
