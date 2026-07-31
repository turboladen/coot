# Architecture Decision Records

This directory holds **ADRs** — short, immutable records of a significant decision: the context that
forced it, the choice made, and the consequences. They answer "why is it this way?" for the next
person (usually future-you). ADRs are the **durable decision record** for this project.

## Why ADRs

A decision's current relevance is explicit in its **Status**: an `Accepted` decision is live; a
reversed one becomes `Superseded by ADR-N`, and the replacement points back with `Supersedes ADR-M`.
You never edit a decision's history — you supersede it. Grep a topic and the status tells you
immediately whether you are looking at current truth or a retired call.

## Relationship to `docs/superpowers/`

Design and planning still happen through the brainstorming / writing-plans workflow, which produces
a spec and an implementation plan. Those are **transient working artifacts** — scaffolding useful
while a feature is in flight — **not** a permanent archive. `docs/superpowers/` is **gitignored**.

The durable *decision* inside a design gets promoted to an ADR here; the spec and plan are retired
once the feature ships. The earlier committed `docs/superpowers/specs`/`plans` archive was retired
this way — its durable decisions were distilled into ADRs 0002–0007, and the artifacts themselves
remain in git history if ever needed.

## When to write one

Write an ADR when a decision is **cross-cutting / architectural** OR **likely to be revisited or
reversed** — a data-model convention, a concurrency invariant, a security policy, a design-system
rule. Not every feature needs one: a UI detail whose conventions already live in code and `PLAN.md`
does not.

Rule of thumb: if reversing it later would need a "why did we do that?" explanation, it's an ADR.

## Conventions

- One file per decision: `NNNN-kebab-title.md`, numbered sequentially.
- Format: a status block + **Context / Decision / Consequences** (Nygard style).
- **Immutable once `Accepted`.** To change a decision, add a new ADR that `Supersedes` it and flip
  the old one's status to `Superseded by` — only the status line of a superseded ADR is edited,
  never its body.
- `Status` values: `Accepted`, `Superseded by ADR-N`, `Deprecated`, `Proposed`.
- Reference durable artifacts (code paths, `PLAN.md`, `CLAUDE.md`, beads), not transient design docs.

## Index

| ADR | Status | Decision |
| --- | --- | --- |
| [0001](0001-record-architecture-decisions.md) | Accepted | Adopt ADRs in `docs/adr`; superpowers specs/plans are transient and gitignored |
| [0002](0002-connection-reuse-for-schema-introspection.md) | Accepted | Schema introspection reuses one locked client per connection; the query runner connects fresh on purpose |
| [0003](0003-session-only-passwords.md) | Accepted | Session-only passwords via a `SecretStore` overlay; never written to the Keychain |
| [0004](0004-param-scope-tiers.md) | Accepted | Parameter values resolve `Local ?? Session ?? Global`; scope selects the write target |
| [0005](0005-saved-query-is-a-stable-template.md) | Accepted | A saved query is a stable template; Run never rewrites the library |
| [0006](0006-schema-cache-generation-guard.md) | Accepted | Generation counter checked under the insert lock; return but don't cache across a clear |
| [0007](0007-design-tokens-and-colorblind-safe-theme.md) | Accepted | Single token layer; colourblind-safety as a hard constraint (hue is never the only cue) |
