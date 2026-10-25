/** Pin only an existing mark. All other marks keep their exact relative order. */
export function mainAgentFirst<T extends { key: string }>(
  items: T[],
  key: string,
): T[] {
  const index = items.findIndex((item) => item.key === key);
  return index > 0
    ? [items[index], ...items.slice(0, index), ...items.slice(index + 1)]
    : items;
}
