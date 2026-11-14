// Pure formatters that turn a result set into clipboard text, as CSV or as a
// GFM Markdown table. The module imports only types and renderCell, never Svelte
// or Tauri, so `bun test` can load it, as it does resultSummary.ts.
//
// Cell text comes from renderCell so a copy shows exactly what the grid shows,
// including the literal NULL and decimals kept as strings.
import type { QueryResult } from "./api";
import { renderCell } from "./renderCell";

export type CopyFormat = "csv" | "markdown";

const FORMAT_LABEL: Record<CopyFormat, string> = { csv: "CSV", markdown: "Markdown" };

// Fields are quoted per RFC 4180. Leading or trailing whitespace is quoted too, so a reader
// that trims unquoted fields keeps it.
function csvField(text: string): string {
  return /[",\r\n]|^\s|\s$/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
}

// Escaping here guarantees only the table's structure. Inline Markdown such as
// `*x*` or `<b>` still renders, which an AI reading the raw text never sees.
function mdCell(text: string): string {
  return text.replace(/\|/g, "\\|").replace(/\r\n|\r|\n/g, "<br>");
}

/** Formats the result as CSV: a header row of column names, then one line per row, joined with `\n`. */
export function toCsv(result: QueryResult): string {
  const lines = [result.columns.map((c) => csvField(c.name)).join(",")];
  for (const row of result.rows) lines.push(row.map((cell) => csvField(renderCell(cell).text)).join(","));
  return lines.join("\n");
}

/** Formats the result as a GFM table, right-aligning a column when all its non-NULL cells are numeric. */
export function toMarkdown(result: QueryResult): string {
  const row = (cells: string[]) => `| ${cells.join(" | ")} |`;
  const rendered = result.rows.map((r) => r.map(renderCell));
  const separators = result.columns.map((_, i) => {
    const values = rendered.map((r) => r[i]).filter((c) => c && !c.nullish);
    return values.length > 0 && values.every((c) => c.align === "right") ? "---:" : "---";
  });
  const lines = [row(result.columns.map((c) => mdCell(c.name))), row(separators)];
  for (const r of rendered) lines.push(row(r.map((c) => mdCell(c.text))));
  return lines.join("\n");
}

export function formatResult(result: QueryResult, format: CopyFormat): string {
  return format === "csv" ? toCsv(result) : toMarkdown(result);
}

/** Reports whether there is anything to copy, which needs a result with at least one column. */
export function canCopy(result: QueryResult | null): result is QueryResult {
  return result !== null && result.columns.length > 0;
}

/** Returns the toast text for a successful copy, such as "Copied 42 rows as CSV". */
export function copiedMessage(rowCount: number, format: CopyFormat): string {
  return `Copied ${rowCount} row${rowCount === 1 ? "" : "s"} as ${FORMAT_LABEL[format]}`;
}
