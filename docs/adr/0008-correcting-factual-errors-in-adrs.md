# ADR-0008: An accepted ADR may be corrected for factual error

- **Status:** Accepted
- **Date:** 2026-08-24
- **Related:** [ADR-0001](0001-record-architecture-decisions.md) (which set the immutability rule
  this narrows); [ADR-0002](0002-connection-reuse-for-schema-introspection.md) (the ADR that
  produced the problem); bead `billz-nvt`

## Context

ADR-0001 makes an `Accepted` ADR immutable: a decision changes only by a new ADR superseding it, and
only the status line of the old file is ever edited. That rule protects the decision record from
being quietly rewritten, and it should stay.

It does not fit an ADR that is simply **wrong about the code**. ADR-0002 was distilled from a design
spec rather than from what shipped, and one bullet named three functions as promoted to `pub(crate)`
when only one of them was. Immutability left one repair available: leave the wrong bullet standing
and add a blockquote underneath saying the bullet above is wrong. A reader who skims the bullet — or
quotes it, which is what happened, misleading an implementer — never reaches the retraction. The
page is then actively worse than a page with no correction at all, because the error now carries the
authority of a record that survived review.

A decision and a claim of fact are different things. Superseding is the right instrument for
reversing the first and the wrong one for repairing the second: nothing was decided differently, so
a new ADR would have no decision to state.

## Decision

**An `Accepted` ADR's body may be edited to correct a factual error, provided the correction is
recorded in a dated `## Corrections` section at the end of that file.**

- The **wrong sentence is fixed in place**, so the page reads correctly top to bottom and cannot be
  quoted into an error.
- The correction entry is dated and says three things: what the text claimed, what is actually true,
  and how the error got in. The last of those is the part worth having — it names a trap.
- **Errors of fact only.** A change of mind, a relaxed constraint, or a reversed choice is a
  decision, and still requires a superseding ADR under ADR-0001. If editing the sentence would
  change what was decided rather than what was described, stop and write the new ADR.
- **Verify against the code, not against a design doc.** The claim being corrected and the claim
  replacing it are both checked against what shipped. A correction sourced from a spec repeats the
  mistake that made the correction necessary.

## Consequences

- **Positive:** an ADR that is trusted as current truth can be made true, rather than being made
  contradictory. A skimming reader gets the right answer, which is the case immutability could not
  serve.
- **Positive:** the `## Corrections` section keeps the history in the one place a reader looking for
  it will go, instead of scattering retractions through the argument.
- **Negative:** the boundary between "wrong description" and "changed decision" needs judgment, and
  a motivated editor could reclassify the second as the first. The mitigation is the dated entry:
  every in-place edit leaves a signed record of itself, so an unrecorded body change is visible as a
  rule violation rather than as a normal edit.
- **Negative:** immutability is now a rule with an exception, which is harder to state than the rule
  it replaces and easier to remember imprecisely. `docs/adr/README.md` carries the combined form.
