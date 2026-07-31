# ADR-0001: Record architecture decisions

- **Status:** Accepted
- **Date:** 2026-07-30

## Context

Design work on Coot runs through the brainstorming / writing-plans workflow, which produces a spec
and an implementation plan under `docs/superpowers/`. Those were previously committed to the repo,
which conflated two different kinds of document: **transient scaffolding** for a feature in flight,
and the **durable reasoning** behind a decision that still governs the code.

The cost of that conflation is concrete. The decision that the query runner connects fresh rather
than reusing a session — made on 2026-07-11 and recorded only inside `billz-lpb`'s design doc — was
independently re-derived from first principles on 2026-07-27 while designing plan capture, because
nobody knew it had already been decided. A reader of the repo had no way to distinguish "this is
current truth" from "this was one step in a conversation, parts of which we reversed."

## Decision

**Architecture Decision Records live in `docs/adr/`, in Nygard format, and are the durable decision
record for this project.** Superpowers specs and plans are transient working artifacts, are **not**
committed, and `docs/superpowers/` is gitignored.

- One file per decision: `NNNN-kebab-title.md`, numbered sequentially.
- Format: a status block plus **Context / Decision / Consequences**.
- **Immutable once `Accepted`.** To change a decision, add a new ADR that `Supersedes` it and flip
  the old one's status to `Superseded by ADR-N` — only the status line of a superseded ADR is
  edited, never its body.
- `Status` values: `Accepted`, `Superseded by ADR-N`, `Deprecated`, `Proposed`.
- Reference durable artifacts — code paths, `PLAN.md`, `CLAUDE.md`, beads — never transient design
  docs.

Write an ADR when a decision is **cross-cutting / architectural** OR **likely to be revisited or
reversed**. Not every feature needs one. Rule of thumb: if reversing it later would require a "why
did we do that?" explanation, it is an ADR.

`docs/adr` and Nygard were chosen to match the majority convention across the sibling projects
(`kammerz`, `selfie`, `healthie`), which also supplied the precedent for retiring a committed
`docs/superpowers` archive this way.

## Consequences

- **Positive:** the answer to "why is it this way?" is greppable, and a decision's current relevance
  is explicit in its `Status` rather than inferred from a document's date.
- **Positive:** specs regain their proper role as disposable scaffolding, so a design conversation
  can record dead ends and reversals without those becoming part of the permanent record.
- **Negative:** promoting a decision out of a spec is a manual step that can be forgotten. The
  mitigation is that retiring a spec is the moment to ask what should survive it.
- **Negative:** ADRs 0002–0007 were written retroactively from specs, so they record the decision
  and its rationale faithfully but not the discussion that produced them; that history remains in
  git for the artifacts removed in this change.
