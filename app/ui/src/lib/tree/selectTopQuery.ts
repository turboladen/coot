// Build the browse query for the tree's double-click-table action (rqb.6).
// Rune-free .ts (like columnLabel.ts) so `bun test` imports it with no webview.
// Takes plain strings — no api-type import needed.

// Bracket-quote one T-SQL identifier part: wrap in [ ] and double any ] inside.
// Mirrors core::context::use_statement's `]`→`]]` rule (the one tested quoting seam).
export function quoteIdent(part: string): string {
  return `[${part.replace(/]/g, "]]")}]`;
}

// Build the browse query for a table, 3-part named so it runs from any DB context.
export function selectTop1000(db: string, schema: string, table: string): string {
  return `SELECT TOP 1000 * FROM ${quoteIdent(db)}.${quoteIdent(schema)}.${quoteIdent(table)};`;
}
