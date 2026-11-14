// Pure formatters that turn a result set into clipboard text, as CSV or as a
// GFM Markdown table. No Svelte, no Tauri: this module imports only types and
// renderCell, so it is `bun test`-able, mirroring resultSummary.ts.
//
// Cell text comes from renderCell so a copy shows exactly what the grid shows,
// including the literal NULL and decimals kept as strings.
import type { QueryResult } from "./api";
import { renderCell } from "./renderCell";

export type CopyFormat = "csv" | "markdown";

const FORMAT_LABEL: Record<CopyFormat, string> = { csv: "CSV", markdown: "Markdown" };

// RFC 4180 quoting. Leading or trailing whitespace is quoted too, so a reader
// that trims unquoted fields keeps it.
function csvField(text: string): string {
  return /[",\r\n]|^\s|\s$/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
}

function mdCell(text: string): string {
  return text.replace(/\|/g, "\\|").replace(/\r\n|\r|\n/g, "<br>");
}

/** The result as CSV: a header row of column names, then one line per row, joined with `\n`. */
export function toCsv(result: QueryResult): string {
  const lines = [result.columns.map((c) => csvField(c.name)).join(",")];
  for (const row of result.rows) lines.push(row.map((cell) => csvField(renderCell(cell).text)).join(","));
  return lines.join("\n");
}

/** The result as a GFM table; columns whose non-NULL cells are all numeric are right-aligned. */
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

/** Whether there is anything to copy: a result with at least one column. */
export function canCopy(result: QueryResult | null): result is QueryResult {
  return result !== null && result.columns.length > 0;
}

/** Toast text for a successful copy, e.g. "Copied 42 rows as CSV". */
export function copiedMessage(rowCount: number, format: CopyFormat): string {
  return `Copied ${rowCount} row${rowCount === 1 ? "" : "s"} as ${FORMAT_LABEL[format]}`;
}
