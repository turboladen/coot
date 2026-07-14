// Pure helpers for persisting result-grid column widths (billz-389). No Svelte /
// localStorage here — those live in columnWidths.svelte.ts; this file is bun:test-able.

// Stable key for a result SHAPE from its ordered column names. table-core's column
// id is the positional String(i) — unstable across queries — so persisted widths are
// keyed by name instead. JSON.stringify of the names array is an unambiguous,
// deterministic key (name contents can't collide with a delimiter) that is
// order-sensitive and preserves duplicate / empty-string names.
export function signatureOf(columnNames: string[]): string {
  return JSON.stringify(columnNames);
}

// Tolerant parse of the persisted width store: signature -> (columnName -> px).
// Mirrors parseStringMap (paramBarLogic.ts): a null / malformed / non-object /
// array blob degrades to {}. Per entry, non-object inner maps are dropped and each
// width must be a finite number > 0 (guards NaN / Infinity / <=0 / string / null).
export function parseWidthStore(raw: string | null): Record<string, Record<string, number>> {
  if (raw === null) return {};
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return {};
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return {};
  const out: Record<string, Record<string, number>> = {};
  for (const [sig, inner] of Object.entries(parsed)) {
    if (typeof inner !== "object" || inner === null || Array.isArray(inner)) continue;
    const widths: Record<string, number> = {};
    for (const [name, w] of Object.entries(inner)) {
      if (typeof w === "number" && Number.isFinite(w) && w > 0) widths[name] = w;
    }
    out[sig] = widths;
  }
  return out;
}
