# ADR-0008: Compile-check a query corpus instead of capturing execution plans

- **Status:** Accepted
- **Date:** 2026-09-11
- **Related:** beads `billz-xi6` (ditched), `billz-3xz` (the surviving path), `billz-bkm` (the test
  that proved the permission is enforced)

## Context

One of the two product goals is judging SQL that an LLM generates, using traces exported from
Langfuse. The design for it was `billz-xi6`: capture each query's estimated execution plan with
`SET SHOWPLAN_XML ON`, parse the ShowPlanXML, and reduce it to a verdict. Units 1-3 shipped — the
plan model, a parser validated against real captured fixtures, and the verdict engine.

Running a real 100-query corpus against a real server established two things.

**The permission is not available.** SQL Server checks SHOWPLAN against the databases holding the
objects a statement *names*, not the session's current database. A statement reading only `sys.*`
therefore captures a plan under a login that is denied SHOWPLAN, which is why the permission looked
present for a long time. Every query in the corpus names a user table, and all of them were refused
with error 262 on every reachable database. Granting SHOWPLAN means an ask across five teams on
another continent, which the maintainer has ruled out.

**The cheaper check found the defect.** `SET NOEXEC ON` compiles a batch and executes nothing,
requires no permission beyond the read access the statement already needs, and rejected 37 of the
100 queries: 35 for `LIMIT`, which belongs to another SQL dialect, and 2 for unbalanced parentheses.
Those queries never ran in production and never could have. One prompt change removes the category.

The plan analysis, had it been possible, would have graded the performance of queries that *do*
compile. That is real value, but second-order next to a third of the corpus not being valid T-SQL.

## Decision

**The corpus job compile-checks queries. It does not capture or analyze execution plans.**

`core::compile_check` sends `SET NOEXEC ON`, the query, then `SET NOEXEC OFF`, on its own connection,
issuing the `USE` as its own request immediately before the `ON` and never again after it — the two
hazards documented in `core/src/plan/capture.rs` apply to NOEXEC exactly as they do to SHOWPLAN,
because NOEXEC suppresses `USE` too. The `USE` stays a separate request rather than a line prepended
to the batch, for the reasons on `executor::run_batch`.

`core/src/plan/` keeps its model, parser and verdict. They are correct and tested, including against
a real captured plan; they are not extended and not deleted.

## Consequences

**Compile-checking validates syntax only.** SQL Server defers name resolution for an object that is
absent and raises it at execution, which NOEXEC prevents, so `SELECT * FROM dbo.no_such_table` is
accepted. `compile_check_rejects_bad_syntax_but_not_an_unresolved_table` in `core/tests/dev_box.rs`
pins both halves so that an `ok` result is never read as "the tables exist."

**Resolving names needs a database the corpus's queries were written against.**
`sys.sp_describe_first_result_set` binds without executing and would catch a hallucinated table,
which on generated SQL is likely the largest defect class after dialect errors (`billz-3xz.3`). It
is blocked on data, not on code: the corpus names per-tenant databases absent from every reachable
server, so runs substitute one that exists, and against a substitute an unresolved name cannot be
told apart from that database lagging the schema. A finding nobody can trust is worse than no
finding.

**Nothing executes, and that guarantee now rests on two different `SET` statements.** Both state the
dependency the same way: the query does not run *provided* the `SET` engaged, and what secures that
is the batch returning `Err` whenever it did not.

**If SHOWPLAN is ever granted, the plan path is a re-open, not a rewrite.** Capture, model, parser
and verdict are all in the tree and all tested.
