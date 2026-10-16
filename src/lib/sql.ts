export interface Range {
  from: number;
  to: number;
}

/** Statement ranges (trimmed, non-empty), splitting on `;` outside strings and comments. */
export function statementRanges(sql: string): Range[] {
  const ranges: Range[] = [];
  let start = 0;
  let i = 0;
  const push = (end: number) => {
    const text = sql.slice(start, end);
    const lead = text.length - text.trimStart().length;
    const trimmed = text.trim();
    if (trimmed && !isOnlyComments(trimmed)) {
      ranges.push({ from: start + lead, to: start + lead + trimmed.length });
    }
  };
  while (i < sql.length) {
    const c = sql[i];
    if (c === "'" || c === '"' || c === "`") {
      i = skipQuoted(sql, i, c);
    } else if (c === "#" || (c === "-" && sql[i + 1] === "-" && startsDashComment(sql[i + 2]))) {
      const nl = sql.indexOf("\n", i);
      i = nl === -1 ? sql.length : nl + 1;
    } else if (c === "/" && sql[i + 1] === "*") {
      const end = sql.indexOf("*/", i + 2);
      i = end === -1 ? sql.length : end + 2;
    } else if (c === ";") {
      push(i);
      start = i + 1;
      i++;
    } else {
      i++;
    }
  }
  push(sql.length);
  return ranges;
}

/** MySQL reads `--` as a comment only before whitespace, a control character or the end of input. */
function startsDashComment(next: string | undefined): boolean {
  return next === undefined || next <= " ";
}

function skipQuoted(sql: string, start: number, quote: string): number {
  let i = start + 1;
  while (i < sql.length) {
    if (sql[i] === "\\" && quote !== "`") {
      i += 2;
      continue;
    }
    if (sql[i] === quote) {
      if (sql[i + 1] === quote) {
        i += 2;
        continue;
      }
      return i + 1;
    }
    i++;
  }
  return sql.length;
}

function isOnlyComments(text: string): boolean {
  const stripped = text
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/(^|\n)\s*(#|--(\s|$)).*/g, "")
    .trim();
  return stripped === "";
}

/**
 * What ⌘↵ runs: the selection if there is one, otherwise the statement under the cursor
 * (or the closest one before it when the cursor sits between statements).
 */
export function statementAt(sql: string, cursor: number, selection?: Range): Range | null {
  if (selection && selection.to > selection.from) {
    return selection;
  }
  const ranges = statementRanges(sql);
  const containing = ranges.find((r) => cursor >= r.from && cursor <= r.to + 1);
  if (containing) return containing;
  const before = ranges.filter((r) => r.to <= cursor).pop();
  return before ?? ranges[0] ?? null;
}

/** A MySQL string literal: backslashes and single quotes escaped. */
export function sqlString(value: string): string {
  return `'${value.replace(/\\/g, "\\\\").replace(/'/g, "\\'")}'`;
}

/** A MySQL identifier in backticks, inner backticks doubled. */
export function quoteIdent(name: string): string {
  return `\`${name.replace(/`/g, "``")}\``;
}

/** `` `col` = 'value' `` joined with AND, for WHERE filters that open rows in the Database view. */
export function whereEquals(pairs: [column: string, value: string][]): string {
  return pairs.map(([c, v]) => `${quoteIdent(c)} = ${/^(0|[1-9]\d*)$/.test(v) ? v : sqlString(v)}`).join(" AND ");
}
