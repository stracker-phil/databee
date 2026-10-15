/** "in 3 days" / "2 h ago" for a unix timestamp. */
export function relativeFromUnix(unix: number, now = Date.now()): string {
  const diff = Math.round(unix - now / 1000);
  const abs = Math.abs(diff);
  let text: string;
  if (abs < 45) text = "now";
  else if (abs < 3600) text = `${Math.round(abs / 60)} min`;
  else if (abs < 86400) text = `${Math.round(abs / 3600)} h`;
  else if (abs < 86400 * 45) text = `${Math.round(abs / 86400)} d`;
  else if (abs < 86400 * 365) text = `${Math.round(abs / (86400 * 30))} mo`;
  else text = `${(abs / (86400 * 365)).toFixed(1).replace(/\.0$/, "")} y`;
  if (text === "now") return "just now";
  return diff > 0 ? `in ${text}` : `${text} ago`;
}

export function absoluteFromUnix(unix: number): string {
  return new Date(unix * 1000).toLocaleString();
}

/** "AT" → 🇦🇹 */
export function flagEmoji(code: string): string {
  if (!/^[A-Z]{2}$/.test(code)) return "";
  return String.fromCodePoint(...[...code].map((c) => 0x1f1e6 + c.charCodeAt(0) - 65));
}

export function initials(name: string): string {
  const parts = name.trim().split(/[\s@._-]+/).filter(Boolean);
  return ((parts[0]?.[0] ?? "?") + (parts.length > 1 ? parts[parts.length - 1][0] : "")).toUpperCase();
}

/** Long single values collapse to "head…tail". */
export function shorten(text: string, head = 24, tail = 6): string {
  return text.length <= head + tail + 1 ? text : `${text.slice(0, head)}…${text.slice(-tail)}`;
}
