//! Diagnostic contract version identity (issue #11).
//!
//! The diagnostic schema version and the diagnostic registry version are
//! independent of the product release, the Model/IR/protocol contract
//! versions, the lock wire, and the resolver algorithm version. Reusing the
//! strict SemVer mechanics of issue #9 does not widen its registry.

use serde::Serialize;

/// The exact wire discriminator of the diagnostic item contract.
pub const SCHEMA_VERSION: &str = "lekalo/diagnostic/v0.2.16";

/// The exact wire discriminator of the diagnostic registry contract.
pub const REGISTRY_SCHEMA_VERSION: &str = "lekalo/diagnostic-registry/v0.6.4";

/// The embedded diagnostic registry identity.
pub const REGISTRY_IDENTITY: &str = "dev.lekalo.diagnostic-registry@0.6.4";

/// The current diagnostic registry version. The integrated chain is
/// additive end to end: 1.14.0 (issue #29) -> 1.16.0 (issue #39, the
/// reserved observed.* family on the integrated M2 line) -> 1.18.0
/// (issue #30, the reserved `implementation.*` family; 1.17.0 stays
/// reserved by its parallel owner and is not part of this line) ->
/// 1.19.0 (issue #40, the contracted.* family) -> 1.20.0 (issue #42,
/// the reserved `bindings.*` family: proposal-unknown, ambiguous, and
/// plan-mismatch) -> 1.21.0 (issue #64, the reserved `query.*` family
/// `storage.*` family LEK-STO-001..007) -> 1.24.0 (issue #97, the
/// reserved `init.bootstrap.*` family LEK-INIT-006..009; 1.23.0 stays
/// reserved by its parallel owner and is not part of this line) ->
/// 0.2.16 (issue #66, the reserved `expression.*` family
/// LEK-EXPR-001..009) -> 0.3.2 (issue #48, the reserved `native-gate.*`
/// family LEK-NGT-001..014) -> 0.4.0 (issues #85, #70, #45, #117, #69,
/// #87, and #46: the `nfr.*` family LEK-NFR-001..013, the reserved
/// `transport.*` family LEK-TRN-001..009, the additive
/// `storage.profile-*` family LEK-SEP-001..005, the
/// `storage.migration-*` planning family, the `classification.*`
/// family LEK-CLS-001..012 with the `dataflow.*` family
/// LEK-DFL-001..009, and the `openapi.*` family LEK-OAS-001..008,
/// all at the reserved 0.4.0 product generation;
/// issue #45 ships no new diagnostic families) -> issue #47
/// (the reserved `scenario.*` family LEK-SCN-001..011, additive at the
/// same 0.4.0 product generation) -> 0.6.3 (issue #75, the active
/// `context.*` family LEK-CONTEXT-001..008 of the context-budget
/// report) -> 0.6.4 (issues #35, #76 and #77: the `trace.bridge-*`
/// family LEK-TRACE-001..009, the `ai-lint.*` family
/// LEK-AILINT-001..006, the `ambiguity.*` family LEK-AMBIGUITY-001..003,
/// the `hidden.*` family LEK-HIDDEN-001..006/009, the `indirection.*`
/// family LEK-INDIRECTION-001, and the `coupling.*` family
/// LEK-COUPLING-001..015; the registry takes the product version of
/// its commit per docs/versioning.md). Issue #100 adds six independent
/// `evaluation.*` rules LEK-EVAL-001..006 without modifying earlier
/// entries; issue #102 adds eight `metrics-export.*` rules
/// LEK-MEXPORT-001..008 the same way.
pub const REGISTRY_VERSION: &str = "0.6.4";

/// Opt-in issue #84 union successor. Predecessor producers keep their exact pins.
pub const SUCCESSOR_REGISTRY_VERSION: &str = "0.6.5";

/// Issue #94 union successor: the 0.6.5 successor plus the closed
/// `format.*` family (LEK-FORMAT-001..009). Predecessor producers keep
/// their exact pins.
pub const FORMAT_REGISTRY_VERSION: &str = "0.8.0";

/// Opt-in issue #136 union successor: the brownfield adoption service's
/// nine `adoption.*` rules join the full 0.6.5 union. Predecessor
/// producer pins (the 0.6.4 default and the 0.6.5 architecture union)
/// are preserved verbatim; the diagnostic item wire stays 0.2.16.
///
/// After the milestone integration merges, this is the same merged
/// `format.*` + `adoption.*` + `portability.*` + `worklog-changelog.*`
/// + `managed.*` + `tasklink.*` union registry as the other 0.8.0
///   family constants.
pub const ADOPTION_REGISTRY_VERSION: &str = "0.8.0";

/// Issue #95 union successor: the merged `format.*` + `adoption.*`
/// union plus the closed `watch.*` family (LEK-WATCH-001..007) of the
/// incremental dev loop. The same merged registry file grows
/// additively; predecessor producer pins stay verbatim and the
/// diagnostic item wire stays 0.2.16.
pub const WATCH_REGISTRY_VERSION: &str = "0.8.0";

/// Issue #19 M8 union successor: the merged registry plus the nine
/// `portability.*` diagnostics (LEK-PORT-001..009). The new producer
/// explicitly selects this registry; frozen predecessor outputs retain
/// their pinned registries.
pub const M8_REGISTRY_VERSION: &str = "0.8.0";

/// Issue #51 union successor: the pilot composition family
/// (`worklog-changelog.*`, LEK-WC-001..007) on top of the merged union.
/// Predecessor producers keep their exact pins.
pub const PILOT_REGISTRY_VERSION: &str = "0.8.0";

/// Issue #113 union successor: the accepted M8 union (`format.*` +
/// `adoption.*` + `portability.*` + `worklog-changelog.*` + `managed.*`) plus the
/// fourteen `module-api.*` rules (`LEK-MODULE-001..014`). Predecessor
/// producers keep their exact pins; the union is additive end to end and
/// names the same merged registry bytes as the other 0.8.0 family
/// constants.
pub const MODULE_REGISTRY_VERSION: &str = "0.8.0";

/// Issue #41 managed union successor: the full integrated allocation union
/// plus the closed `managed.*` family. Predecessor producer pins are
/// unchanged; managed producers pin this registry.
pub const MANAGED_REGISTRY_VERSION: &str = "0.8.0";

/// The issue #52 union successor: the merged union plus the eight
/// `tasklink.*` declarations (LEK-TASKLINK-001..008) of the task-links
/// pilot. Earlier producers keep their exact registry pins end to end.
pub const TASKLINK_REGISTRY_VERSION: &str = "0.8.0";

/// Issue #93 union successor: the merged 0.8.0 grand union plus the
/// eight `search.*` rules LEK-SRCH-001..008. Predecessor producers keep
/// their exact pins.
pub const SEARCH_REGISTRY_VERSION: &str = "0.8.0";

/// The closed diagnostic schema version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticSchemaVersion {
    /// `lekalo/diagnostic/v0.2.16`.
    Current,
}

impl DiagnosticSchemaVersion {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        SCHEMA_VERSION
    }
}

impl Serialize for DiagnosticSchemaVersion {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
