export type Prefix = ">" | "t:" | "q:" | "c:" | "#" | "@" | "=" | "databee://" | null;

const PREFIXES = [">", "t:", "q:", "c:", "#", "@", "=", "databee://"] as const;

/** Free text shorter than this matches only tables and commands (Rust's `search::MIN_TEXT`). */
const MIN_TEXT = 6;

/** The prefix and the rest; an address keeps its whole text as the needle. */
export function parsePrefix(input: string): { prefix: Prefix; needle: string } {
  const q = input.trimStart();
  for (const prefix of PREFIXES) {
    if (!q.startsWith(prefix)) continue;
    if (prefix === "databee://") return { prefix, needle: q.trim() };
    return { prefix, needle: q.slice(prefix.length).trim() };
  }
  return { prefix: null, needle: q.trim() };
}

/** Whether Rust searches rows for this input (`search::parse`). */
export function searchesRows(prefix: Prefix, needle: string): boolean {
  if (prefix === "#") return /^\d+$/.test(needle);
  if (prefix === "@" || prefix === "=") return [...needle].length >= 2;
  return prefix === null && [...needle].length >= MIN_TEXT;
}

export function searchText(prefix: Prefix, needle: string): string {
  return prefix === null ? needle : `${prefix}${needle}`;
}

export function score(title: string, needle: string): number {
  if (!needle) return 1;
  const t = title.toLowerCase();
  const n = needle.toLowerCase();
  if (t === n) return 4;
  if (t.startsWith(n)) return 3;
  if (t.includes(n)) return 2;
  return 0;
}

function inOrder(text: string, needle: string): boolean {
  let at = 0;
  for (const ch of needle) {
    at = text.indexOf(ch, at) + 1;
    if (at === 0) return false;
  }
  return true;
}

/** The best `score` over `fields`, else 1 when one field holds the needle's characters in order. */
export function fuzzyScore(fields: string[], needle: string): number {
  const best = Math.max(0, ...fields.map((f) => score(f, needle)));
  if (best > 0) return best;
  const n = needle.toLowerCase();
  return fields.some((f) => inOrder(f.toLowerCase(), n)) ? 1 : 0;
}
