# ADR-0005: A saved query is a stable template, not a living document

- **Status:** Accepted
- **Date:** 2026-07-12
- **Related:** `PLAN.md` §5 ("saved queries = intentional; tabs = scratch");
  [ADR-0004](0004-param-scope-tiers.md) (the tier routing this reuses); beads `billz-d28.8`,
  `billz-d28.10`

## Context

Opening a saved query into a tab makes its SQL editable, but `run()` persisted remembered parameter
values derived from the **edited** tab content against a stored `sql` that was never written back.
That produced two inconsistencies:

- **Orphan params** — typing `@b` into the tab and running persisted a value for `@b` onto the saved
  query, whose stored SQL never references `@b`.
- **Dropped values** — that remembered `@b` was then invisible on reopen, because parameters are
  derived by scanning the *stored* SQL, and purged on the next run.

Non-crashing cruft, and the common flow (open unchanged, fill, run) was unaffected — but the
semantics were undefined, and two reasonable models were available.

## Decision

**A saved query is a stable template, not a living document. Its persisted parameter set is defined
by its stored SQL, never by edited tab content.**

Chosen over the alternative "Run = save edits", which would silently rewrite a curated library entry
on every run. **Run is a read action and must not redefine the library.**

Concretely, on Run:

- **Declared params** — those present in the stored SQL — are remembered, routed to their tier per
  ADR-0004.
- **Edited-in params and SQL edits are scratch.** They are used for *this* run's execution and
  remembered nowhere.
- **Editing a declared param out of the tab is non-destructive.** Its stored remembered value
  survives; a throwaway edit must not wipe a saved value.

Because `sql` was already never written back, restricting persisted params to the declared set makes
stored `sql` and stored `params` consistent **by construction** rather than by reconciliation.

The deliberate complement — a way to change a definition **explicitly** — is the "Update saved
query" action (`billz-d28.10`), which exists precisely because the implicit path was rejected here.

## Consequences

- **Positive:** stored SQL and stored params cannot drift apart, so the orphan-param and
  dropped-value classes are eliminated rather than patched.
- **Positive:** the library stays curated. Nothing you do in a scratch tab can silently alter a
  saved entry.
- **Negative:** keeping an edit requires a deliberate action. There is no implicit save, which is
  correct but is the opposite of what a document-editor mental model predicts.
- **Negative:** the execution path and the persistence path deliberately diverge — edited content
  runs, only the declared set persists. That is surprising until you know the rule, which is why it
  is documented both here and on `persistDeclared`.
