// The one place the wire shape is declared. These TS types mirror `coot-core`'s
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
  rememberPassword: boolean;
};

// Mirrors of core's result types, for run_sql's return. Minimal — the grid UI
// (cwt.5/cwt.6) fleshes these out; kept here so run_sql is typed end-to-end.
export type CellValue =
  | { kind: "Null" }
  | { kind: "Bool"; value: boolean }
  | { kind: "Int"; value: number }
  | { kind: "BigInt"; value: string } // bigint as a string (billz-s7p; no f64 precision loss)
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

// One database's slice of a cross-tenant fan-out (mirrors core's DbRunOutcome).
// `error` non-null ⇒ that DB failed and `results` is empty; the other DBs still
// ran. `elapsedMs` is that DB's own wall time.
export type DbRunOutcome = {
  database: string;
  results: QueryResult[];
  error: string | null;
  elapsedMs: number;
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

// The full SqlType set as a value, in the same order as the type union above — the
// single source for every bind-type <select>/validator (ParamBar, VariablesLibrary,
// variablesLogic's asSqlType) so the three don't drift out of sync with each other.
export const SQL_TYPES: readonly SqlType[] = [
  "int", "bigint", "nvarchar", "bit", "date", "datetime2", "decimal", "uniqueidentifier", "money",
];

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

// Execute-time param (mirrors core's ResolvedParam). sqlType null → raw-text.
export type ResolvedParam = { name: string; sqlType: SqlType | null; value: string };

// The camelCase keys (`cfg`/`password`/`id`/`database`/`sql`) match the Rust
// command arg names — Tauri marshals JS→Rust args by name.
export const listConnections = () => invoke<ConnectionConfig[]>("list_connections");

export const saveConnection = (cfg: ConnectionConfig, password: string | null) =>
  invoke<void>("save_connection", { cfg, password });

export const deleteConnection = (id: string) => invoke<void>("delete_connection", { id });

export const setSessionPassword = (id: string, password: string) =>
  invoke<void>("set_session_password", { id, password });

export const testConnection = (id: string) => invoke<void>("test_connection", { id });

export const runSql = (
  id: string,
  database: string | null,
  sql: string,
  selection: string | null,
  line: number,
) => invoke<QueryResult[]>("run_sql", { id, database, sql, selection, line });

// Cross-tenant fan-out: run the same query against many databases in parallel,
// one outcome per database. Keys match the Rust `run_fanout` param names.
export const runFanout = (
  id: string,
  databases: string[],
  sql: string,
  selection: string | null,
  line: number,
) => invoke<DbRunOutcome[]>("run_fanout", { id, databases, sql, selection, line });

export const runParams = (
  id: string,
  database: string | null,
  sql: string,
  params: ResolvedParam[],
) => invoke<QueryResult[]>("run_params", { id, database, sql, params });

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
