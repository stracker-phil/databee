/** Case and accents don't count. */
export const fold = (text: string) => text.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();

const WORD_BREAK = /[\s:·/_›-]+/u;

/** The characters of `needle` appear in order in `hay` ("tsb" → "Toggle Sidebar"). */
function subsequence(hay: string, needle: string): boolean {
  let at = 0;
  for (const c of hay) if (c === needle[at]) at += 1;
  return at === needle.length;
}

const withoutSpaces = (text: string) => text.replace(/\s+/g, "");

/** How well an entry matches a folded query (the palette's commands and the Settings search): the label starting with it, then a
 *  word of the label, then every query word somewhere in label or detail, then the letters in order. Lower is better; null doesn't
 *  match. */
export function rankMatch(label: string, detail: string, query: string): number | null {
  if (!query) return 0;
  const name = fold(label);
  if (name.startsWith(query)) return 0;
  if (name.split(WORD_BREAK).some((word) => word.startsWith(query))) return 1;
  const hay = `${name} ${fold(detail)}`;
  if (query.split(/\s+/).every((word) => hay.includes(word))) return 2;
  return subsequence(withoutSpaces(name), withoutSpaces(query)) ? 3 : null;
}

/** The matching items, best first; ties keep their order. */
export function ranked<T>(items: T[], query: string, text: (item: T) => [label: string, detail: string]): T[] {
  const needle = fold(query.trim());
  return items
    .map((item, index) => ({ item, index, rank: rankMatch(...text(item), needle) }))
    .filter((entry): entry is { item: T; index: number; rank: number } => entry.rank !== null)
    .sort((a, b) => a.rank - b.rank || a.index - b.index)
    .map((entry) => entry.item);
}
