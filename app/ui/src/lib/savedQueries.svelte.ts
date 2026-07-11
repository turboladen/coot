// UI state for the saved-query library (Svelte 5 runes module) — d28.6. The single
// source of truth for the library list. Backend-persisted (like connections, NOT
// localStorage — that's the ephemeral scratch tabs). Mirrors connections.svelte.ts:
// mutate the exported `$state` object's fields in place — never reassign the export.
//
// Pure logic (filterQueries/promoteToSavedQuery) lives in the rune-free
// savedQueriesLogic.ts so it's `bun test`-able; this module is the live-state
// wrapper + the Tauri command adapter.
import { deleteQuery, listQueries, saveQuery, type SavedQuery } from "./api";

export const library = $state<{ list: SavedQuery[] }>({ list: [] });

export async function refresh() {
  library.list = await listQueries();
}

export async function save(q: SavedQuery) {
  await saveQuery(q);
  await refresh();
}

export async function remove(id: string) {
  await deleteQuery(id);
  await refresh();
}
