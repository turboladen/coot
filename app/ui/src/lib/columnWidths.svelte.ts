// Persisted (localStorage) result-grid column widths (billz-389), keyed by a
// column-name signature then by column name. Presentation state, not secrets.
// Mirrors globalParams.svelte.ts: eager load on init, replace-per-signature +
// persist on write, degrade to {} on a corrupt / absent / quota blob.
import { MAX_WIDTH_SIGNATURES, evictSignatures, parseWidthStore } from "./columnWidthsLogic";

const STORAGE_KEY = "coot.columnWidths.v1";

// signature -> (columnName -> width px)
type WidthStore = Record<string, Record<string, number>>;

// Set when load() had to trim an over-cap blob (billz-10s) so we can reclaim the
// on-disk bytes once the $state proxy exists (persist() can't run during load()).
let trimmedOnLoad = false;

function load(): WidthStore {
  try {
    const parsed = parseWidthStore(localStorage.getItem(STORAGE_KEY));
    // Self-heal a blob already bloated by pre-cap sessions: keep only the most-recent
    // MAX_WIDTH_SIGNATURES shapes (billz-10s).
    const evict = evictSignatures(Object.keys(parsed), MAX_WIDTH_SIGNATURES);
    for (const k of evict) delete parsed[k];
    trimmedOnLoad = evict.length > 0;
    return parsed;
  } catch (e) {
    console.warn("coot: failed to load column widths from localStorage", e);
    return {};
  }
}

const widths = $state<WidthStore>(load());

// Rewrite the trimmed store so an already-bloated blob shrinks on disk even if the
// user never resizes again this session (billz-10s).
if (trimmedOnLoad) persist();

// Persist the current store (degrade quietly on quota/serialize failure — these
// are UI widths, not critical data).
function persist(): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(widths));
  } catch (e) {
    console.warn("coot: failed to persist column widths to localStorage", e);
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
//
// Recency is refreshed on write only, not on read (loadColumnWidths at grid mount):
// last-resized is a good-enough LRU signal for a single user, and persisting on every
// mount would be needless churn. delete-then-reinsert moves this signature to the
// most-recent tail before evicting the oldest past the cap (billz-10s).
export function saveColumnWidths(signature: string, byName: Record<string, number>): void {
  delete widths[signature];
  widths[signature] = byName;
  for (const k of evictSignatures(Object.keys(widths), MAX_WIDTH_SIGNATURES)) delete widths[k];
  persist();
}
