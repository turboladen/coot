// Pure, rune-free saved-query logic — the unit-testable substance of the library
// panel (d28.6). Lives in a plain `.ts` (NOT `.svelte.ts`) so `bun test` can
// import it without a Svelte compiler; `savedQueries.svelte.ts` is the runes
// wrapper that holds the live `$state` and delegates the persistence. Mirrors the
// repo's tabsLogic.ts pure-helper pattern.
import type { SavedQuery } from "./api";

// Case-insensitive filter by name OR sql substring. Empty/whitespace search → all.
export function filterQueries(list: SavedQuery[], search: string): SavedQuery[] {
  const q = search.trim().toLowerCase();
  if (q === "") return list;
  return list.filter(
    (sq) => sq.name.toLowerCase().includes(q) || sq.sql.toLowerCase().includes(q),
  );
}

// Build a SavedQuery from the current tab (id minted by the caller so it's
// injectable/testable). MVP: params = [] — detecting `@name` tokens is the d28.5
// `detectParams()` seam, intentionally NOT built here (CLAUDE.md scope discipline).
export function promoteToSavedQuery(
  id: string,
  name: string,
  sql: string,
  targetDatabase: string | null,
): SavedQuery {
  return { id, name: name.trim(), sql, targetDatabase, params: [] };
}
