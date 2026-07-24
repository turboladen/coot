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
//
// `exists` = the active id is actually in the connection list (billz-a5y.1). The
// active connection is now mirrored from the active tab's own `connectionId`,
// which at cold start is set from a persisted tab BEFORE the list loads, and can
// also linger as a dangling id after its connection is deleted. Both cases must
// resolve to `null` (clear the store) rather than fire a premature `list_databases`
// against an absent connection. `locked` (a filtered `.find`) is null for BOTH an
// absent AND a present-ready connection, so presence is the only signal that tells
// them apart.
export function databaseLoadTarget(
  activeId: string | null,
  locked: boolean,
  exists: boolean,
): string | null {
  return locked || !exists ? null : activeId;
}
