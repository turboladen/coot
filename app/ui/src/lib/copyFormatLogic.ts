// Pure formatters that turn a result set into clipboard text, as CSV or as a
// GFM Markdown table. The module imports only types and renderCell, never Svelte
// or Tauri, so `bun test` can load it, as it does resultSummary.ts.
//
// Cell text comes from renderCell so a copy shows exactly what the grid shows,
// including the literal NULL and decimals kept as strings.
import type { QueryResult } from "./api";
import { renderCell } from "./renderCell";
import { plural } from "./resultSummary";

// Fields are quoted per RFC 4180. Leading or trailing whitespace is quoted too, so a reader
// that trims unquoted fields keeps it.
function csvField(text: string): string {
  return /[",\r\n]|^\s|\s$/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
}

// Escaping here guarantees only the table's structure. Inline Markdown such as
// `*x*` or `<b>` still renders, which an AI reading the raw text never sees.
// GFM splits cells after pairing each backslash with the next character, so any
// backslashes before a pipe are doubled; otherwise `C:\|x` would end the cell.
function mdCell(text: string): string {
  return text.replace(/(\\*)\|/g, "$1$1\\|").replace(/\r\n|\r|\n/g, "<br>");
}

/** Formats the result as CSV: a header row of column names, then one line per row, joined with `\n`. */
export function toCsv(result: QueryResult): string {
  // A lone empty field would leave a blank line, which many readers skip as no
  // record at all, so it is written as `""`.
  const line = (fields: string[]) => fields.map(csvField).join(",") || '""';
  const lines = [line(result.columns.map((c) => c.name))];
  for (const row of result.rows) lines.push(line(row.map((cell) => renderCell(cell).text)));
  return lines.join("\n");
}

/** Formats the result as a GFM table, right-aligning a column that has a value and whose non-NULL cells are all numeric. */
export function toMarkdown(result: QueryResult): string {
  const row = (cells: string[]) => `| ${cells.join(" | ")} |`;
  // One pass builds each row's text and tracks alignment per column, so no
  // rendered cell outlives its row on a large result.
  const sawValue = result.columns.map(() => false);
  const allRight = result.columns.map(() => true);
  const body = result.rows.map((r) =>
    row(
      r.map((cell, i) => {
        const c = renderCell(cell);
        if (!c.nullish) {
          sawValue[i] = true;
          if (c.align !== "right") allRight[i] = false;
        }
        return mdCell(c.text);
      }),
    ),
  );
  const separators = result.columns.map((_, i) => (sawValue[i] && allRight[i] ? "---:" : "---"));
  return [row(result.columns.map((c) => mdCell(c.name))), row(separators), ...body].join("\n");
}

const FORMATS = {
  csv: { label: "CSV", format: toCsv },
  markdown: { label: "Markdown", format: toMarkdown },
} satisfies Record<string, { label: string; format: (result: QueryResult) => string }>;

export type CopyFormat = keyof typeof FORMATS;

/** Every copy format, in the order the buttons show them. */
export const COPY_FORMATS = Object.keys(FORMATS) as CopyFormat[];

/** Returns the format's display name, such as "CSV". */
export function formatLabel(format: CopyFormat): string {
  return FORMATS[format].label;
}

export function formatResult(result: QueryResult, format: CopyFormat): string {
  return FORMATS[format].format(result);
}

/** Reports whether there is anything to copy, which needs a result with at least one column. */
export function canCopy(result: QueryResult | null): result is QueryResult {
  return result !== null && result.columns.length > 0;
}

/** Returns the toast text for a successful copy, such as "Copied 42 rows as CSV". */
export function copiedMessage(rowCount: number, format: CopyFormat): string {
  return `Copied ${plural(rowCount, "row")} as ${formatLabel(format)}`;
}
