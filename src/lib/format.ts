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

export function formatCount(n: number): string {
  return n.toLocaleString("en-US");
}

/** Full text of a cell, as copied to the clipboard. NULL copies as an empty string. */
export function cellText(cell: Cell): string {
  switch (cell.t) {
    case "null":
      return "";
    case "bytes":
      return `0x${cell.v.hexPreview}${cell.v.len > cell.v.hexPreview.length / 2 ? "…" : ""}`;
    default:
      return cell.v;
  }
}

/** Single-line text for a grid cell, cut to `maxChars`. */
export function cellDisplay(cell: Cell, nullLabel: string, maxChars: number): string {
  if (cell.t === "null") return nullLabel;
  if (cell.t === "bytes") return `${cellText(cell)} (${formatBytes(cell.v.len)})`;
  const oneLine = cell.v.replace(/\s*\n\s*/g, " ⏎ ");
  return oneLine.length > maxChars ? `${oneLine.slice(0, maxChars)}…` : oneLine;
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

/** "3 hours ago"-style relative time for history and recents. */
export function relativeTime(iso: string, now = Date.now()): string {
  const seconds = Math.round((now - new Date(iso).getTime()) / 1000);
  if (seconds < 60) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  return `${Math.round(hours / 24)} d ago`;
}
