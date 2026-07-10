// DEV-ONLY sample so the grid renders (and virtualizes) before cwt.5 wires the
// Run button to live `run_sql`. cwt.5 DELETES this file and swaps in real data.
// Exercises every CellValue kind the grid must handle — incl. Binary/Null which
// have no other exercise until cwt.5. 44 rows so virtualization visibly kicks in.
import type { CellValue, ColumnMeta, QueryResult } from "./api";

const columns: ColumnMeta[] = [
  { name: "id", sqlType: "int", nullable: false, precision: null, scale: null },
  { name: "ratio", sqlType: "float", nullable: true, precision: null, scale: null },
  { name: "amount", sqlType: "decimal(19,4)", nullable: true, precision: 19, scale: 4 },
  { name: "name", sqlType: "nvarchar(50)", nullable: true, precision: null, scale: null },
  { name: "active", sqlType: "bit", nullable: false, precision: null, scale: null },
  { name: "guid", sqlType: "uniqueidentifier", nullable: false, precision: null, scale: null },
  { name: "created", sqlType: "datetime2", nullable: true, precision: null, scale: null },
  { name: "payload", sqlType: "varbinary(max)", nullable: true, precision: null, scale: null },
];

const NAMES = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel"];

function row(i: number): CellValue[] {
  const nullName = i % 7 === 0;
  const nullAmount = i % 5 === 0;
  const nullPayload = i % 3 === 0;
  return [
    { kind: "Int", value: i + 1 },
    { kind: "Float", value: Math.round((i / 7) * 1000) / 1000 },
    // Decimal stays a string on the wire — precision preserved through the grid.
    nullAmount ? { kind: "Null" } : { kind: "Decimal", value: `${(i + 1) * 100}.${String(i % 100).padStart(4, "0")}` },
    nullName ? { kind: "Null" } : { kind: "Text", value: `${NAMES[i % NAMES.length]}-${i + 1}` },
    { kind: "Bool", value: i % 2 === 0 },
    { kind: "Uuid", value: `00000000-0000-4000-8000-${String(i + 1).padStart(12, "0")}` },
    { kind: "DateTime", value: `2026-07-${String((i % 28) + 1).padStart(2, "0")}T09:${String(i % 60).padStart(2, "0")}:00` },
    nullPayload ? { kind: "Null" } : { kind: "Binary", value: `0x${(0xdeadbeef + i).toString(16)}` },
  ];
}

export const sampleResult: QueryResult = {
  columns,
  rows: Array.from({ length: 44 }, (_, i) => row(i)),
  rowsAffected: null,
};
