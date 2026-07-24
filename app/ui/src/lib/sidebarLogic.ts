// Pure, rune-free decisions for the connection-as-root sidebar (billz-a5y.3/.5).
// No Svelte / DOM → `bun test`-able. The reactive wrappers (sidebar.svelte.ts, the
// components) call these; keeping the logic here isolates the branching from the
// framework, matching databasesLogic.ts / tabsLogic.ts.
import type { DbStatus } from "./databases.svelte";

// billz-a5y.5: the honest passive status of one connection's sidebar dot. `locked`
// wins (a locked connection never fetches its store); otherwise it mirrors the
// per-connection databases store — no background probing, so "loaded"/"loading"/
// "error" only ever appear for a connection the user has touched this session.
export type ConnStatus = "locked" | "loaded" | "loading" | "error" | "neutral";

export function connectionStatus({
  locked,
  dbStatus,
}: {
  locked: boolean;
  dbStatus: DbStatus;
}): ConnStatus {
  if (locked) return "locked";
  switch (dbStatus) {
    case "loaded":
      return "loaded";
    case "error":
      return "error";
    case "loading":
      return "loading";
    default:
      return "neutral"; // idle — untouched this session
  }
}

// billz-a5y.3: which connection a NEW tab targets. The FOCUSED connection (last root
// browsed or retargeted) when it's live, else the active tab's connection when live,
// else null. Both candidates are validated against the live id set so a dangling id
// (a since-deleted connection) can never become a new tab's execution context.
export function defaultTabConnection(
  focusedId: string | null,
  activeId: string | null,
  liveIds: string[],
): string | null {
  if (focusedId !== null && liveIds.includes(focusedId)) return focusedId;
  if (activeId !== null && liveIds.includes(activeId)) return activeId;
  return null;
}
