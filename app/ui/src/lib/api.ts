// The one place the wire shape is declared. These TS types mirror `billz-core`'s
// serde types (camelCase). The UI sees ONLY these — never a driver type.
import { invoke } from "@tauri-apps/api/core";

export type ConnectionConfig = {
  id: string;
  name: string;
  server: string;
  username: string;
  defaultDatabase: string | null;
  encrypt: boolean;
  trustServerCertificate: boolean;
};

// Mirrors of core's result types, for run_sql's return. Minimal — the grid UI
// (cwt.5/cwt.6) fleshes these out; kept here so run_sql is typed end-to-end.
export type CellValue =
  | { kind: "Null" }
  | { kind: "Bool"; value: boolean }
  | { kind: "Int"; value: number }
  | { kind: "Float"; value: number }
  | { kind: "Decimal"; value: string }
  | { kind: "Text"; value: string }
  | { kind: "Uuid"; value: string }
  | { kind: "Date"; value: string }
  | { kind: "Time"; value: string }
  | { kind: "DateTime"; value: string }
  // #[non_exhaustive]-style escape hatch for variants added later in core.
  | { kind: string; value?: unknown };

export type ColumnMeta = {
  name: string;
  sqlType: string;
  nullable: boolean;
  precision: number | null;
  scale: number | null;
};

export type QueryResult = {
  columns: ColumnMeta[];
  rows: CellValue[][];
  rowsAffected: number | null;
};

// Object-tree schema types (rqb.2) — mirror core's `schema.rs` serde shapes.
export type DatabaseInfo = { name: string; databaseId: number; stateDesc: string };
export type TableInfo = { schema: string; name: string };
export type ViewInfo = { schema: string; name: string };
export type ColumnInfo = {
  name: string;
  dataType: string;
  nullable: boolean;
  isPrimaryKey: boolean;
  isForeignKey: boolean;
  ordinal: number;
};

// Saved-query library types (d28.6) — mirror core's `query.rs` serde shapes.
// The lowercase SqlType tags equal `friendly_type_name` output (query.rs SqlType
// serde) so d28.5 can map a catalog type straight to one.
export type SqlType =
  | "int"
  | "bigint"
  | "nvarchar"
  | "bit"
  | "date"
  | "datetime2"
  | "decimal"
  | "uniqueidentifier"
  | "money";

export type ParamScope = "global" | "session" | "local";

export type Param = {
  name: string;
  sqlType: SqlType | null; // null → raw-text fragment (unsafe); set → bind
  lastValue: string | null;
  scope: ParamScope;
};

export type SavedQuery = {
  id: string;
  name: string;
  sql: string;
  targetDatabase: string | null;
  params: Param[];
};

// The camelCase keys (`cfg`/`password`/`id`/`database`/`sql`) match the Rust
// command arg names — Tauri marshals JS→Rust args by name.
export const listConnections = () => invoke<ConnectionConfig[]>("list_connections");

export const saveConnection = (cfg: ConnectionConfig, password: string | null) =>
  invoke<void>("save_connection", { cfg, password });

export const deleteConnection = (id: string) => invoke<void>("delete_connection", { id });

export const testConnection = (id: string) => invoke<void>("test_connection", { id });

export const runSql = (
  id: string,
  database: string | null,
  sql: string,
  selection: string | null,
  line: number,
) => invoke<QueryResult[]>("run_sql", { id, database, sql, selection, line });

// Object-tree loaders (rqb.2). Keys (`id`/`db`/`schema`/`table`) match the Rust
// command arg names — Tauri marshals JS→Rust args by name.
export const listDatabases = (id: string) =>
  invoke<DatabaseInfo[]>("list_databases", { id });

export const listTables = (id: string, db: string) =>
  invoke<TableInfo[]>("list_tables", { id, db });

export const listViews = (id: string, db: string) =>
  invoke<ViewInfo[]>("list_views", { id, db });

export const listColumns = (id: string, db: string, schema: string, table: string) =>
  invoke<ColumnInfo[]>("list_columns", { id, db, schema, table });

// Refresh (rqb.5): drop the connection's cached schema so the next tree load re-queries sys.*.
export const refreshSchema = (id: string) => invoke<void>("refresh_schema", { id });

// Saved-query library (d28.6). Keys (`query`/`id`) match the Rust command arg
// names — Tauri marshals JS→Rust args by name.
export const listQueries = () => invoke<SavedQuery[]>("list_queries");

export const saveQuery = (query: SavedQuery) => invoke<void>("save_query", { query });

export const deleteQuery = (id: string) => invoke<void>("delete_query", { id });
