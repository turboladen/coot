// Pure formatters for the result-set tabs + Messages pane (cwt.7). No Svelte,
// no Tauri — imports only `type QueryResult` (fully erased) so this is
// `bun test`-able without a DOM, mirroring the renderCell.ts/.test.ts pattern.
import type { QueryResult } from "./api";

// A line in the Messages pane. `error` renders red; `info` renders plain.
export type Message = { kind: "info" | "error"; text: string };

// Pluralize a count with its noun: (1,"row") → "1 row", (2,"row") → "2 rows".
function plural(n: number, noun: string): string {
  return `${n} ${noun}${n === 1 ? "" : "s"}`;
}

// The tab label for result set `i` (0-based) — the only place tab text is
// formatted. Row count comes from rows.length (rowsAffected is None — billz-38l).
export function tabLabel(r: QueryResult, i: number): string {
  return `Result ${i + 1} · ${plural(r.rows.length, "row")}`;
}

// The Messages-pane summary for a successful run.
// - 0 result sets → a DML/no-result batch (honest, not "0 rows affected").
// - else a header line + one line per set with its row/column counts.
export function summarize(results: QueryResult[]): Message[] {
  if (results.length === 0) {
    return [{ kind: "info", text: "Query ran. No result set returned." }];
  }
  const header: Message = {
    kind: "info",
    text: `Ran successfully — ${plural(results.length, "result set")}.`,
  };
  const perSet = results.map((r, i): Message => ({
    kind: "info",
    text: `Result ${i + 1}: ${plural(r.rows.length, "row")}, ${plural(r.columns.length, "column")}`,
  }));
  return [header, ...perSet];
}
