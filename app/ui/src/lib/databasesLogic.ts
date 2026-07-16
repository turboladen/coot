// Which connection's databases the shared `dbStore` should load, given the active
// connection id and whether that connection is currently locked (session-only and
// not yet unlocked this session).
//
// A locked connection resolves to `null` — which `load(null)` uses to CLEAR the
// store (and bump its token, dropping any in-flight prior load). This is the
// billz-zmw fix: the App effect must not early-return on a locked connection, or
// the tree/DB-picker keep rendering the PREVIOUS connection's schema under the
// locked one — a silent wrong-target risk in a multi-connection tool. Only an
// unlocked, present connection loads its own id.
export function databaseLoadTarget(activeId: string | null, locked: boolean): string | null {
  return locked ? null : activeId;
}
