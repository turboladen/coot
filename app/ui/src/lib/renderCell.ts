// Pure display logic for one grid cell — the substance of cwt.6's AC and the
// only genuinely unit-testable piece (see renderCell.test.ts). Switches on the
// `kind` STRING so it also handles core variants not in api.ts's narrowed union
// (Binary/Xml/DateTimeOffset arrive via the `{ kind: string; value?: unknown }`
// escape hatch). Decimal/money stay strings — never `Number()` them (precision).
import type { CellValue } from "./api";

export type Rendered = {
  text: string;
  align: "left" | "right";
  nullish: boolean;
  mono: boolean;
};

export function renderCell(cell: CellValue): Rendered {
  switch (cell.kind) {
    case "Null":
      return { text: "NULL", align: "left", nullish: true, mono: false };
    case "Int":
    case "BigInt":
    case "Float":
    case "Decimal":
      // Right-aligned + monospace so digits line up. Decimal and BigInt are
      // already strings from the wire — String() keeps them verbatim (no float
      // rounding, no 2^53 precision loss for bigint — billz-s7p).
      return { text: String((cell as { value: unknown }).value), align: "right", nullish: false, mono: true };
    case "Bool":
      return { text: (cell as { value: boolean }).value ? "true" : "false", align: "left", nullish: false, mono: false };
    case "Binary":
      // Core renders binary as a "0x…" hex string — show it verbatim, monospace.
      return { text: String((cell as { value: unknown }).value), align: "left", nullish: false, mono: true };
    case "Text":
    case "Uuid":
    case "Xml":
    case "Date":
    case "Time":
    case "DateTime":
    case "DateTimeOffset":
      return { text: String((cell as { value: unknown }).value), align: "left", nullish: false, mono: false };
    default: {
      // Any future core variant: show its value as JSON, or "" if valueless.
      const v = (cell as { value?: unknown }).value;
      return { text: v === undefined ? "" : JSON.stringify(v), align: "left", nullish: false, mono: false };
    }
  }
}
