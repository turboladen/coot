// UI state for the saved-query library (Svelte 5 runes module) — d28.6. The single
// source of truth for the library list. Backend-persisted (like connections, NOT
// localStorage — that's the ephemeral scratch tabs). Mirrors connections.svelte.ts:
// mutate the exported `$state` object's fields in place — never reassign the export.
//
// Pure logic (filterQueries/promoteToSavedQuery) lives in the rune-free
// savedQueriesLogic.ts so it's `bun test`-able; this module is the live-state
// wrapper + the Tauri command adapter.
import { deleteQuery, listQueries, saveQuery, type SavedQuery } from "./api";
import { removeQueryById, upsertQuery } from "./savedQueriesLogic";

export const library = $state<{ list: SavedQuery[] }>({ list: [] });

export async function refresh() {
  library.list = await listQueries();
}

/**
 * Persist `q`, then make `library.list` reflect it.
 *
 * **A rejection means the write did not happen** (billz-sjn). That is the whole
 * contract, and it is why the list is updated from the WRITE rather than from the
 * read-back: `upsertQuery` mirrors the backend's upsert exactly, so a list that was
 * in sync before the write is still in sync after it, and the `refresh()` below is
 * self-healing reconciliation, not the source of truth.
 *
 * What makes swallowing its failure safe is that "write succeeded, read-back
 * failed" is a TRANSIENT window and never a steady state: `QueryStore::upsert`
 * opens with `self.list()?` (core/src/query_store.rs), so the write and the
 * read-back go through the same file read, and anything that breaks `list_queries`
 * breaks `save_query` FIRST. A save therefore cannot succeed on top of a list that
 * was never loaded — the case where a mirror-only update would render a library
 * missing every pre-existing row. Nothing is lost and nothing is actionable, so it
 * must NOT surface as a rejection. Don't "fix" that catch.
 *
 * (`console.warn` rather than a toast because the store must not reach into
 * app-level toast policy. The other `console.warn` stores are localStorage — this
 * is the first backend/IPC failure to take that route, on the reasoning above
 * rather than by precedent.)
 *
 * The bug that earned this comment: `await saveQuery` then `await refresh` made a
 * failed re-read indistinguishable from a failed write, so a caller's catch fired
 * on a row that WAS persisted. App.saveToLibrary then skipped setTabSavedQuery,
 * the tab stayed scratch, and the user's retry minted a duplicate row under a
 * fresh UUID — exactly the duplicate-entry bug billz-he0 removed, via the error
 * path. Callers distinguish the two outcomes by construction: catch = write failed.
 */
export async function save(q: SavedQuery) {
  await saveQuery(q);
  library.list = upsertQuery(library.list, q);
  try {
    await refresh();
  } catch (e) {
    console.warn("coot: saved query written, but the library re-read failed", e);
  }
}

/** Delete `id`. Same contract as `save`: a rejection means nothing was deleted. */
export async function remove(id: string) {
  await deleteQuery(id);
  library.list = removeQueryById(library.list, id);
  try {
    await refresh();
  } catch (e) {
    console.warn("coot: saved query deleted, but the library re-read failed", e);
  }
}
