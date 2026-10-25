/** The legacy drop switch controlled delivery too. Preserve the old disabled
 * choice until the person explicitly selects the independent notification switch. */
export function completionPreferences(value: {
  completionDrops?: boolean;
  completionNotifications?: boolean;
}) {
  return {
    completionNotifications:
      value.completionNotifications ?? value.completionDrops ?? true,
  };
}
