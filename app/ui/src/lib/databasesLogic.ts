// What the App load effect should do with the object store for the ACTIVE
// connection, given the active id, whether that connection is locked (session-only
// and not yet unlocked this session), and whether it's actually present in the list.
//
// Since billz-a5y.2 the store is keyed PER CONNECTION, so the decision names the
// connection it targets rather than collapsing to a single nullable id:
//
//   present & unlocked → load  its own id  (ensureDatabases — load-once memo)
//   present & locked   → clear its own id  (clearDatabases  — billz-zmw)
//   absent  / null     → noop
//
// billz-zmw: a locked connection must never hit the DB, but must still CLEAR its own
// entry to empty — otherwise the tree/DB-picker keep rendering the PREVIOUS
// connection's schema under the locked one (a silent wrong-target risk). Clearing
// targets the connection id so any in-flight prior load for it is dropped.
//
// billz-a5y.1: `exists` = the active id is actually in the connection list. The
// active connection is mirrored from the active tab's own `connectionId`, which at
// cold start is set from a persisted tab BEFORE the list loads, and can also linger
// as a dangling id after its connection is deleted. Both must resolve to `noop`
// rather than fire a premature `list_databases` — and, unlike the old single shared
// store, there is nothing stale to clear (each reader reads its own connection's
// entry, which is idle-empty when absent). `locked` (a filtered `.find`) is null for
// BOTH an absent AND a present-ready connection, so presence is the only signal that
// tells them apart.
export type LoadAction =
  | { kind: "load"; connectionId: string }
  | { kind: "clear"; connectionId: string }
  | { kind: "noop" };

export function databaseLoadAction(
  activeId: string | null,
  locked: boolean,
  exists: boolean,
): LoadAction {
  if (activeId === null || !exists) return { kind: "noop" };
  return locked ? { kind: "clear", connectionId: activeId } : { kind: "load", connectionId: activeId };
}
