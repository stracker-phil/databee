import type { Cell } from "../api";

export function formatBytes(bytes: number | null): string {
  if (bytes == null) return "";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${unit === 0 ? value : value.toFixed(1)} ${units[unit]}`;
}

/** `12,345`: one grouping style everywhere, whatever the system locale. */
export function formatCount(n: number): string {
  return n.toLocaleString("en-US");
}

export const plural = (n: number, one: string, many: string) => `${formatCount(n)} ${n === 1 ? one : many}`;

/** `950`, `1.2k`, `12k`, `3.4M` for tight spots (table sizes, chart labels). */
export function compactCount(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${(n / 1000).toFixed(n < 10_000 ? 1 : 0)}k`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}

/** Text of a cell for the clipboard. NULL is empty; bytes and long text give only their preview. */
export function cellText(cell: Cell): string {
  switch (cell.t) {
    case "null":
      return "";
    case "bytes":
      return `0x${cell.v.hexPreview}${cell.v.len > cell.v.hexPreview.length / 2 ? "…" : ""}`;
    case "longText":
      return `${cell.v.preview}…`;
    default:
      return cell.v;
  }
}

/** A grid column's width in characters: its name or its longest text, capped at `max`, plus room for padding. */
export function charWidth(name: string, texts: Iterable<string>, max = 40): string {
  let longest = name.length;
  for (const text of texts) longest = Math.max(longest, text.length);
  return `${Math.min(max, longest) + 2}ch`;
}

export function cellDisplay(cell: Cell, nullLabel: string, maxChars: number): string {
  if (cell.t === "null") return nullLabel;
  if (cell.t === "bytes") return `${cellText(cell)} (${formatBytes(cell.v.len)})`;
  if (cell.t === "longText") return `${oneLine(cellText(cell))} (${formatCount(cell.v.len)} chars)`;
  const line = oneLine(cell.v);
  return line.length > maxChars ? `${line.slice(0, maxChars)}…` : line;
}

function oneLine(text: string): string {
  return text.replace(/\s*\n\s*/g, " ⏎ ");
}

export function rowsToTsv(columns: string[], rows: Cell[][]): string {
  const escape = (s: string) => s.replace(/[\t\n\r]/g, " ");
  return [columns.map(escape).join("\t"), ...rows.map((r) => r.map((c) => escape(cellText(c))).join("\t"))].join("\n");
}

export function rowsToJson(columns: string[], rows: Cell[][]): string {
  const objects = rows.map((r) =>
    Object.fromEntries(r.map((c, i) => [columns[i], c.t === "null" ? null : cellText(c)])),
  );
  return JSON.stringify(objects, null, 2);
}

const MINUTE = 60;
const HOUR = 3600;
const DAY = 86400;

/** "2 h ago" or "in 3 d" for an ISO date or a unix timestamp in seconds. */
export function relativeTime(when: string | number, now = Date.now()): string {
  const ms = typeof when === "number" ? when * 1000 : new Date(when).getTime();
  const diff = Math.round((ms - now) / 1000);
  const abs = Math.abs(diff);
  if (abs < 45) return "just now";
  let text: string;
  if (abs < HOUR) text = `${Math.round(abs / MINUTE)} min`;
  else if (abs < DAY) text = `${Math.round(abs / HOUR)} h`;
  else if (abs < DAY * 45) text = `${Math.round(abs / DAY)} d`;
  else if (abs < DAY * 365) text = `${Math.round(abs / (DAY * 30))} mo`;
  else text = `${(abs / (DAY * 365)).toFixed(1).replace(/\.0$/, "")} y`;
  return diff > 0 ? `in ${text}` : `${text} ago`;
}
