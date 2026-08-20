//! Estimated execution plans — capture, parse, judge, and fingerprint.
//!
//! Every module here but [`capture`] is pure — functions over XML and structs —
//! so they are tested offline against the checked-in `.sqlplan` fixtures in
//! `core/tests/fixtures/plans/`, with no server, no VPN, and no driver. Those
//! fixtures are real, captured by `just dump-plans`; assertions come from the
//! files.
//!
//! [`capture`] is the one module that touches a server. It opens and closes its
//! OWN connection rather than using
//! [`SessionCache`](crate::session::SessionCache): a leaked
//! `SET SHOWPLAN_XML ON` on a reused client would make every later query return
//! plan XML instead of running. See that module's doc.

// The connection-reuse decision is ADR-0002,
// `docs/adr/0002-connection-reuse-for-schema-introspection.md`.

pub mod capture;
pub mod fingerprint;
pub mod model;
pub mod parse;
pub mod verdict;

pub use capture::capture_xml;
pub use fingerprint::shape;
pub use model::{
    DbPlanOutcome, Finding, FindingKind, MissingIndex, PlanCapture, PlanNode, PlanStatement,
    PlanVerdict, PlanWarning, QueryPlan, Severity,
};
pub use parse::parse_plan;
pub use verdict::judge;
