// Pure display formatter for one `ColumnInfo` — the tree's single unit-testable
// seam (see columnLabel.test.ts). `import type` only, so api.ts's Tauri import is
// fully erased and this runs under `bun test` with no webview. Splits a column
// into its display pieces so `ColumnLeaf.svelte` renders them without inline
// logic: `name`, the muted `: dataType`, a `NULL`/`NOT NULL` marker, and the
// `PK`/`FK` badge flags (badges shown only when the flag is true).
import type { ColumnInfo } from "../api";

export type ColumnLabel = {
  name: string;
  dataType: string;
  nullText: "NULL" | "NOT NULL";
  isPrimaryKey: boolean;
  isForeignKey: boolean;
};

export function columnLabel(c: ColumnInfo): ColumnLabel {
  return {
    name: c.name,
    dataType: c.dataType,
    nullText: c.nullable ? "NULL" : "NOT NULL",
    isPrimaryKey: c.isPrimaryKey,
    isForeignKey: c.isForeignKey,
  };
}
