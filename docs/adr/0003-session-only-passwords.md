# ADR-0003: Session-only passwords via a secret-store overlay

- **Status:** Accepted
- **Date:** 2026-07-12
- **Related:** `CLAUDE.md` ("secrets never touch disk in plaintext"); `PLAN.md` §2; bead `billz-85b`

## Context

The connection manager implemented remember-password = **true** only: the password went to the
macOS Keychain via `KeychainSecretStore`. The `rememberPassword` checkbox existed in the UI but was
UI-only — unchecking it passed `null` to `save_connection`, storing nothing, so every later
operation failed with `Config("no stored password…")`. There was no way to use a connection whose
password you did not want on disk.

The governing invariant is that secrets never touch disk in plaintext. Satisfying it must not
require choosing between "Keychain" and "unusable."

## Decision

**A connection may hold a session-only password: prompted at connect, held in memory for the
process lifetime, never written to the Keychain.**

- `ConnectionConfig` gains `remember_password: bool` — **metadata only**. No password field is
  added, so the disk invariant holds by construction. It defaults to `true` via
  `#[serde(default)]` for back-compatibility with configs written earlier, and does not enter
  `build_connection_string`.
- `SessionOverlaySecretStore<S>` decorates any `SecretStore` with an ephemeral, process-lifetime map:
  - `get_password` prefers the session value, else falls through to the durable inner store.
  - `set_session_password` writes to the session map **only** — it has no path to the inner store.
  - `set_password` (the remember-on path) writes through to the durable store unchanged.
  - `delete_password` clears both layers.
- `AppState.secrets` becomes `SessionOverlaySecretStore<CachingSecretStore<KeychainSecretStore>>`.
  Every core operation already receives `&state.secrets`, so the preference is applied
  transparently with **no per-operation change**.
- The UI tracks an `unlocked` set with the same lifetime as the backend session map — both empty on
  restart, so they cannot drift across runs. A locked connection shows a prompt on activation rather
  than failing an operation.

## Consequences

- **Positive:** the never-on-disk invariant is enforced structurally rather than by discipline. The
  overlay type has no code path that persists a session password, so violating it requires changing
  the type, not just forgetting a rule.
- **Positive:** the decorator keeps responsibilities clean — the overlay holds session passwords and
  prefers them; the inner caching Keychain store is untouched.
- **Negative:** session passwords vanish on restart by design, so a session-only connection
  re-prompts once per app run. That is the point, but it is friction.
- **Negative:** "unlocked" exists in two places (the backend map and the UI set) and they must be
  kept in sync — including when `save_connection` stashes a password, which must also seed the UI
  set or the user is immediately re-prompted.
