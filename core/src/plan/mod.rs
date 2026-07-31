//! Estimated execution plans — capture, parse, judge, and fingerprint.
//!
//! Every module here but [`capture`] is pure — functions over XML and structs —
//! so once the parser lands they will all be tested offline against checked-in
//! `.sqlplan` fixtures, with no server, no VPN, and no driver. (No fixtures yet:
//! this is the scaffolding unit, and `parse`/`verdict`/`fingerprint` follow.)
//! [`capture`] is the one that touches a server, and it opens its
//! OWN connection and closes it rather than using
//! [`SessionCache`](crate::session::SessionCache) — see
//! `docs/adr/0002-connection-reuse-for-schema-introspection.md` and that
//! module's doc for why a leaked `SET SHOWPLAN_XML ON` would be so damaging.

pub mod capture;
pub mod model;

pub use capture::capture_xml;
pub use model::{
    DbPlanOutcome, Finding, FindingKind, MissingIndex, PlanCapture, PlanNode, PlanStatement,
    PlanVerdict, PlanWarning, QueryPlan, Severity,
};
