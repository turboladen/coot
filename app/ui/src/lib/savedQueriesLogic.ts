// Pure, rune-free saved-query logic — the unit-testable substance of the library
// panel (d28.6). Lives in a plain `.ts` (NOT `.svelte.ts`) so `bun test` can
// import it without a Svelte compiler; `savedQueries.svelte.ts` is the runes
// wrapper that holds the live `$state` and delegates the persistence. Mirrors the
// repo's tabsLogic.ts pure-helper pattern.
import type { SavedQuery } from "./api";
import { deriveParams } from "./paramBarLogic";
import { deriveTitle } from "./tabsLogic";

// Case-insensitive filter by name OR sql substring. Empty/whitespace search → all.
export function filterQueries(list: SavedQuery[], search: string): SavedQuery[] {
  const q = search.trim().toLowerCase();
  if (q === "") return list;
  return list.filter(
    (sq) => sq.name.toLowerCase().includes(q) || sq.sql.toLowerCase().includes(q),
  );
}

// Build a SavedQuery from the current tab (id minted by the caller so it's
// injectable/testable).
//
// billz-he0: params are DERIVED, not left empty. This used to hardcode `params:
// []` while the other two save paths (openScopedQuery, App.updateSavedQuery) both
// ran deriveParams — so a promoted query containing @params came back with an
// empty param list and no param bar until something else happened to fix it up.
// `stored` is [] because a brand-new saved query has no prior param config to
// preserve; deriveParams is lexer-aware (billz-7c9), so @words inside string
// literals and comments correctly aren't params.
export function promoteToSavedQuery(
  id: string,
  name: string,
  sql: string,
  targetDatabase: string | null,
): SavedQuery {
  return { id, name: name.trim(), sql, targetDatabase, params: deriveParams(sql, []) };
}

// Apply a persisted write to the in-memory list, mirroring what the backend just
// did to the file (billz-sjn). `core`'s QueryStore::upsert replaces by id IN PLACE
// and otherwise pushes (core/src/query_store.rs), storing the row verbatim — so
// this returns exactly what a subsequent `list_queries` would.
//
// That equivalence is the whole point: it lets `save()` treat its read-back as
// optional rather than authoritative, which is what stops a failed re-read from
// being reported as a failed WRITE. Keep the two in step — if the Rust side ever
// sorts or normalizes, this drifts silently and the tests below are the tripwire.
//
// Takes ownership of `q` (it is stored by reference, not copied): every call site
// passes a freshly-built or spread object, so a later mutation of a caller's local
// can't reach into the store.
export function upsertQuery(list: SavedQuery[], q: SavedQuery): SavedQuery[] {
  const at = list.findIndex((existing) => existing.id === q.id);
  if (at === -1) return [...list, q];
  const next = [...list];
  next[at] = q;
  return next;
}

// The delete counterpart. Hands the input list straight back when nothing matched,
// so the store's `library.list = removeQueryById(...)` doesn't invalidate the
// `$state` field for a removal that did nothing (same no-op discipline as
// toastLogic's dismissToast).
export function removeQueryById(list: SavedQuery[], id: string): SavedQuery[] {
  const next = list.filter((q) => q.id !== id);
  return next.length === list.length ? list : next;
}

// A rename is a field-preserving rewrite: only `name` changes, and `id` / `sql` /
// `targetDatabase` / `params` ride through by spread (billz-1kn). Written this way
// on purpose — "renaming can't lose the SQL or break a tab's savedQueryId linkage"
// becomes a property of the code rather than of the caller remembering to copy
// every field. Does NOT guard an empty name; the caller does (see planRename).
export function renameSavedQuery(q: SavedQuery, name: string): SavedQuery {
  return { ...q, name: name.trim() };
}

// deriveTitle truncates with a literal "…", which is right for a live, disposable
// tab title and wrong for a name we're about to PERSIST — the whole point of
// seeding a suggestion is that Enter-alone yields something clean. Strip the
// ellipsis plus any trailing comma/space the cut left behind.
function tidy(title: string): string {
  const stripped = title.replace(/[…,\s]+$/u, "");
  // Degenerate input only (SQL whose first non-empty line is nothing but
  // ellipsis/comma/space), but it keeps this total: the seed is never "", so the
  // dialog is never born with a disabled submit button. Covered by a test.
  return stripped === "" ? "Untitled" : stripped;
}

// What the rename dialog pre-fills. Normally the CURRENT name — this is a rename,
// not a re-derive. The exception is saved queries created before billz-he0, whose
// "name" is really the query, in one of TWO shapes:
//
//   1. The common one. The old promote flow (0032f67) seeded its input with
//      `deriveTitle(activeContent())`, so an untouched name is a ≤24-char SQL
//      fragment ENDING IN "…" — e.g. "SELECT TOP 100 * FROM o…".
//   2. The pasted one. That input was editable, so a name can also be a whole
//      multi-line statement someone typed or pasted over the suggestion.
//
// Shape 1 is why the ellipsis is stripped BEFORE the prefix test: the SQL contains
// "SELECT TOP 100 * FROM or…", never the truncated form, so comparing the raw name
// against it always misses and the seed would carry "…" into a PERSISTED name.
//
// "Unusable" is deliberately NOT "long" — a 61-character name someone actually
// typed is a real name, and throwing it away on a typo-fix rename would be the
// same bug as always re-deriving. The signals below all mean "this isn't a name,
// it's the query": blank, multi-line, or a string the SQL starts with.
//
// Note the asymmetry in the return: a KEPT name comes back verbatim, ellipsis and
// all, so an intentional "Work in progress…" survives. Only the derived branch is
// guaranteed ellipsis-free, because only it is a machine-made suggestion.
//
// Accepted false positive: a 1-2 character name that happens to prefix the SQL
// ("S" with "SELECT 1") derives instead of being kept. Guarding it would take a
// minimum-length constant, which is the arbitrary threshold this function exists
// without; and real names can't prefix SQL anyway, since SQL opens with
// SELECT/WITH/EXEC/DECLARE. Locked by a test so it reads as a decision.
export function renameSeed(q: SavedQuery): string {
  const name = q.name.trim();
  const sql = q.sql.trim();
  const bare = name.replace(/…$/u, "");
  const unusable = bare === "" || bare.includes("\n") || sql.startsWith(bare);
  return unusable ? tidy(deriveTitle(q.sql)) : name;
}

// The rename DECISION, extracted from the component so the branches that can
// actually be wrong are the ones `bun test` can reach — the component keeps only
// the await and the toasts.
export type RenamePlan =
  | { kind: "noop" } // nothing to write
  | { kind: "missing" } // the row is gone (deleted while the dialog was open)
  | { kind: "write"; query: SavedQuery }; // persist this

// Takes the CURRENT list rather than a snapshot captured when the dialog opened:
// a rename writes the whole row back, so `sql`/`params` must be read fresh or a
// concurrent edit would be silently reverted by the rename. Also why a deleted row
// reports `missing` instead of being resurrected by the upsert.
export function planRename(list: SavedQuery[], id: string, rawName: string): RenamePlan {
  const name = rawName.trim();
  // Empty-name check BEFORE the lookup, deliberately: an empty submit on a row
  // that has also been deleted is a silent no-op, not a "no longer exists" error.
  if (name === "") return { kind: "noop" };
  const current = list.find((q) => q.id === id);
  if (!current) return { kind: "missing" };
  if (current.name === name) return { kind: "noop" };
  return { kind: "write", query: renameSavedQuery(current, name) };
}
