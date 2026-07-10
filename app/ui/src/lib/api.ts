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
