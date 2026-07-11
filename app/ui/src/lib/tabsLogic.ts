// Pure, rune-free tab logic — the unit-testable substance of the editor tab bar
// (cwt.8). Lives in a plain `.ts` (NOT `.svelte.ts`) so `bun test` can import it
// without a Svelte compiler; `tabs.svelte.ts` is the runes wrapper that holds the
// live `$state` and delegates here. Mirrors the repo's renderCell.ts/resultSummary.ts
// pure-helper pattern.

// `database`: the tab's target DB for the runner (billz-cwt.9). null = the
// connection's default DB. Each tab carries its own, so one tab can sit on
// ESP_Arnotts_Group_DEV while another targets ESP_Suntory_DEV (PLAN §4/§5).
export type QueryTab = { id: string; title: string; content: string; database: string | null };

export type TabsState = { tabs: QueryTab[]; activeId: string };

// Derive a tab's display title from its content: the first non-empty (trimmed)
// line, truncated to ~24 chars; "Untitled" when the content is empty/whitespace.
// Titles are derived, not free-typed, this wave (no rename polish — out of scope).
const TITLE_MAX = 24;
export function deriveTitle(content: string): string {
  for (const line of content.split("\n")) {
    const trimmed = line.trim();
    if (trimmed.length > 0) {
      return trimmed.length > TITLE_MAX ? trimmed.slice(0, TITLE_MAX - 1) + "…" : trimmed;
    }
  }
  return "Untitled";
}

// Which tab should become active after `closingId` is removed. Picks the previous
// neighbour (clamped to index 0), so closing a middle/last tab lands on its left
// neighbour and closing the first lands on the new first. Returns null when the
// closing tab is the only one (caller reseeds a fresh empty tab). If `closingId`
// isn't found, returns the current first tab's id (or null if empty).
export function pickNeighbourId(tabs: QueryTab[], closingId: string): string | null {
  const idx = tabs.findIndex((t) => t.id === closingId);
  if (idx === -1) return tabs.length > 0 ? tabs[0].id : null;
  if (tabs.length <= 1) return null;
  const neighbourIdx = idx > 0 ? idx - 1 : 1; // left neighbour, or the new-first
  return tabs[neighbourIdx].id;
}

// Serialize the tab set for persistence. Plain JSON of the whole state shape.
export function serialize(state: TabsState): string {
  return JSON.stringify(state);
}

// Parse a persisted blob back into a valid TabsState, or null when it's
// malformed/empty (caller then seeds the default tab). Repairs a dangling
// activeId (one matching no tab) by falling back to the first tab's id, so a
// partially-valid blob stays usable rather than being discarded.
export function deserialize(json: string | null): TabsState | null {
  if (json === null) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(json);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const obj = parsed as Record<string, unknown>;
  if (!Array.isArray(obj.tabs)) return null;
  const tabs: QueryTab[] = [];
  for (const t of obj.tabs) {
    if (
      typeof t === "object" && t !== null
      && typeof (t as Record<string, unknown>).id === "string"
      && typeof (t as Record<string, unknown>).title === "string"
      && typeof (t as Record<string, unknown>).content === "string"
    ) {
      const raw = t as Record<string, unknown>;
      // `database` is read-tolerant: a pre-cwt.9 blob has no key, and a garbled
      // value shouldn't poison the whole set — anything non-string becomes null
      // (= connection default), matching the field's "unset" meaning.
      const database = typeof raw.database === "string" ? raw.database : null;
      tabs.push({ id: raw.id as string, title: raw.title as string, content: raw.content as string, database });
    } else {
      return null; // any malformed tab poisons the blob → reseed default
    }
  }
  if (tabs.length === 0) return null;
  const activeId = typeof obj.activeId === "string" && tabs.some((t) => t.id === obj.activeId)
    ? obj.activeId
    : tabs[0].id; // repair a dangling/absent activeId
  return { tabs, activeId };
}
