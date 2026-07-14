// Persisted (localStorage) result-grid column widths (billz-389), keyed by a
// column-name signature then by column name. Presentation state, not secrets.
// Mirrors globalParams.svelte.ts: eager load on init, replace-per-signature +
// persist on write, degrade to {} on a corrupt / absent / quota blob.
import { parseWidthStore } from "./columnWidthsLogic";

const STORAGE_KEY = "billz.columnWidths.v1";

// signature -> (columnName -> width px)
type WidthStore = Record<string, Record<string, number>>;

function load(): WidthStore {
  try {
    return parseWidthStore(localStorage.getItem(STORAGE_KEY));
  } catch (e) {
    console.warn("billz: failed to load column widths from localStorage", e);
    return {};
  }
}

const widths = $state<WidthStore>(load());

// Persist the current store (degrade quietly on quota/serialize failure — these
// are UI widths, not critical data).
function persist(): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(widths));
  } catch (e) {
    console.warn("billz: failed to persist column widths to localStorage", e);
  }
}

// Stored widths (columnName -> px) for a result shape, or {} when none/corrupt.
export function loadColumnWidths(signature: string): Record<string, number> {
  return widths[signature] ?? {};
}

// Replace the stored widths for a signature and persist. `byName` is the full set
// of explicitly-sized columns for that result shape (columnName -> px) — the grid
// seeds prior widths into columnSizing at mount, so a drag-end snapshot already
// carries every column's width and this replace won't drop earlier ones.
export function saveColumnWidths(signature: string, byName: Record<string, number>): void {
  widths[signature] = byName;
  persist();
}
