//! The read-only MCP surface (issue #33): pure contracts, policy, and
//! projections. The transport and the process lifecycle live in the CLI
//! crate; this module owns everything deterministic and side-effect-free.
//!
//! - [`capability`]: the embedded capabilities manifest (`lekalo mcp
//!   describe`).
//! - [`policy`]: effective limits, identifier/path admission, and the
//!   trusted catalog with group/manifest/git grants.
//! - [`resources`]: the five resource families and their logical URIs.
//! - [`tools`]: the exact seven-tool catalog and closed request grammar.
//! - [`schemas`]: the embedded 0.8.0 contract artifacts and their
//!   parse-once verification.
//! - [`diagnostic`]: the registered `mcp.*` transport diagnostics.

pub mod capability;
pub mod diagnostic;
pub mod policy;
pub mod resources;
pub mod schemas;
pub mod screen;
pub mod tools;
pub mod version;

pub use crate::classification::{parse_checked, ParseFailure};
#[cfg(test)]
mod tests {
    use crate::diagnostics::{DiagnosticRegistry, DiagnosticSet};
    use crate::result::Status;

    #[test]
    fn mcp_registry_is_the_grand_m8_union_of_the_0_6_5_successor() {
        let predecessor = DiagnosticRegistry::successor().expect("0.6.5");
        let union = DiagnosticRegistry::mcp_successor().expect("0.8.0");
        // Union-aware admission: no fixed predecessor+own total can
        // reject future landed families or hide dropped ones. Every
        // accessor parses the same merged bytes, all predecessor entries
        // remain identical, and each landed family must resolve below.
        assert!(union.len() > predecessor.len());
        assert_eq!(
            DiagnosticRegistry::format_successor().unwrap().len(),
            union.len()
        );
        assert_eq!(DiagnosticRegistry::adoption().unwrap().len(), union.len());
        assert_eq!(DiagnosticRegistry::m8().unwrap().len(), union.len());
        assert_eq!(DiagnosticRegistry::pilot().unwrap().len(), union.len());
        assert_eq!(
            DiagnosticRegistry::module_successor().unwrap().len(),
            union.len()
        );
        assert_eq!(DiagnosticRegistry::managed().unwrap().len(), union.len());
        assert_eq!(
            DiagnosticRegistry::tasklink_successor().unwrap().len(),
            union.len()
        );
        // A landed family that vanishes from the union is a stale-merge
        // failure (the check-registry-union.mjs live-line invariant).
        for family in [
            "format.",
            "adoption.",
            "portability.",
            "worklog-changelog.",
            "managed.",
            "module-api.",
            "tasklink.",
            "mcp.",
        ] {
            assert!(
                union.entries().any(|entry| entry.id().starts_with(family)),
                "family {family} missing from the union successor"
            );
        }
        for id in [
            "format.diff",
            "adoption.plan-stale",
            "portability.analysis-incomplete",
            "worklog-changelog.input-invalid",
            "managed.plan-stale",
            "module-api.input-invalid",
            "tasklink.evidence-invalid",
            "mcp.scope-denied",
        ] {
            assert!(union.entry(id).is_some(), "{id} must join the grand union");
        }
        for entry in predecessor.entries() {
            let mcp_entry = union
                .entry(entry.id())
                .unwrap_or_else(|| panic!("{} missing from union", entry.id()));
            assert_eq!(entry.code(), mcp_entry.code());
            assert_eq!(entry.category(), mcp_entry.category());
            assert_eq!(entry.default_severity(), mcp_entry.default_severity());
            assert_eq!(entry.default_message(), mcp_entry.default_message());
            assert_eq!(entry.lifecycle(), mcp_entry.lifecycle());
        }
    }

    #[test]
    fn every_mcp_set_carries_the_0_8_0_registry_pin() {
        let sets = [
            super::diagnostic::input_invalid_set("symbol", "required"),
            super::diagnostic::scope_denied_set("project", "unregistered"),
            super::diagnostic::sensitive_output_set("tool-result", "secret"),
            super::diagnostic::request_limit_set("frame"),
            super::diagnostic::output_limit_set(1, 2),
            super::diagnostic::timeout_set(30_000),
            super::diagnostic::cancelled_set(),
            super::diagnostic::contract_unsupported_set("pin"),
            super::diagnostic::snapshot_stale_set("model"),
            super::diagnostic::backend_unavailable_set("confinement"),
            super::diagnostic::result_invalid_set("detail"),
        ];
        for set in sets {
            assert!(set.reason_ids().iter().all(|id| id.starts_with("mcp.")));
        }
    }

    #[test]
    fn statuses_map_onto_the_domain_vocabulary() {
        // A scope denial is the same Status the CLI denied envelope uses.
        let denied: DiagnosticSet =
            DiagnosticSet::try_from_unsorted(Vec::new(), Status::Denied).expect("empty denied");
        assert!(denied.is_empty());
        assert_eq!(Status::Denied.exit_code(), 3);
        assert_eq!(Status::Unavailable.exit_code(), 4);
        assert_eq!(Status::UnsupportedVersion.exit_code(), 5);
    }
}
