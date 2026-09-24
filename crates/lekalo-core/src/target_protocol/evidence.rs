//! The deterministic confinement evidence of one adapter exchange
//! (issue #89).
//!
//! Every run records what the budget granted, what the adapter
//! described, what the budget check admitted, and what the platform
//! honestly enforced per dimension. The document carries names and
//! tokens only — never environment values, secret material, or
//! absolute host paths. Field order is the wire order; arrays are
//! sorted; there are no timestamps.

use serde::Serialize;

use super::confinement::ConfinementReport;
use crate::adapter_package::budget::{ChildPolicy, NetworkBudget, SessionBudget};

/// The confinement evidence member of one completed exchange.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ConfinementEvidence {
    /// The budget the session granted (manifest ceiling or the strict
    /// implicit default).
    pub budget: BudgetEvidence,
    /// The scopes the adapter described in this session.
    pub described: ScopeEvidence,
    /// The scopes the budget check admitted for the exchange.
    pub effective: ScopeEvidence,
    /// The write-plan versus actual-staged-changes audit, present only
    /// for operations that declare writes (issue #89): the passing case
    /// is an empty `outsideScopes` list; any path outside the effective
    /// write scopes is refused before publication.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub writes: Option<WriteAuditEvidence>,
    /// The normalized platform token (`<os>-<arch>`), never a host
    /// path or hostname.
    pub platform: &'static str,
}

/// The budget projection: granted scopes, environment names, and the
/// per-dimension network/children/resources posture.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BudgetEvidence {
    /// The read-scope ceiling (sorted; empty when the budget claims
    /// nothing independently — the implicit default).
    #[serde(rename = "readScopes")]
    pub read_scopes: Vec<String>,
    /// The write-scope ceiling (sorted; empty when the budget claims
    /// nothing independently).
    #[serde(rename = "writeScopes")]
    pub write_scopes: Vec<String>,
    /// Where the ceilings come from: `manifest` (the verified package
    /// manifest) or `described` (the budget claims nothing
    /// independently; the adapter's own describe bounds apply). Empty
    /// cap lists mean the latter unless this token says `manifest`.
    #[serde(rename = "scopeCeiling")]
    pub scope_ceiling: &'static str,
    /// The granted environment variable names (sorted). Values are
    /// never carried: they exist only inside the child environment
    /// block for the duration of one exchange.
    pub env: Vec<String>,
    /// The granted environment names dropped at spawn because they
    /// collide (case-insensitively) with the platform's fixed private
    /// environment block (Windows LPAC); sorted. The private value
    /// wins, never the host-sourced grant (issue #89, C-F5).
    #[serde(rename = "envDropped")]
    pub env_dropped: Vec<String>,
    pub network: NetworkEvidence,
    pub children: ChildrenEvidence,
    pub resources: ResourcesEvidence,
}

/// The network posture of the session and its honest enforcement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NetworkEvidence {
    /// `denied` or `allowlist` as declared.
    pub mode: &'static str,
    /// `enforced` (namespace-level denial) or `degraded-denied` (an
    /// allowlist was declared but no supported platform offers a
    /// namespace-level destination filter, so the run stays denied).
    pub enforcement: &'static str,
}

/// The children policy of the session and its honest enforcement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChildrenEvidence {
    /// `denied` or `declared` as declared.
    pub policy: &'static str,
    /// `denied-enforced` (a platform primitive holds the denial),
    /// `denied-bounded` (a platform bound caps the task count, not a
    /// fork primitive), `denied-unenforced` (no primitive exists — the
    /// namespace containment still applies), or `permitted`.
    pub enforcement: &'static str,
    /// The enforced process bound of the children policy (the Windows
    /// job cap or the Linux task bound), when one applies; `null` is an
    /// honest gap, never a guess. It rides the children dimension: the
    /// bound caps children, not a general resources budget (issue #89,
    /// C-F13).
    #[serde(rename = "processLimit")]
    pub process_limit: Option<u64>,
}

/// The resource bounds of the session and their honest enforcement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResourcesEvidence {
    /// The enforced memory bound in bytes, when the platform enforces
    /// one; `null` is an honest gap, never a guess.
    #[serde(rename = "memoryLimit")]
    pub memory_limit: Option<u64>,
    /// `enforced` or `unenforced`.
    pub enforcement: &'static str,
}

/// One scope projection (sorted, canonical order).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ScopeEvidence {
    #[serde(rename = "readScopes")]
    pub read_scopes: Vec<String>,
    #[serde(rename = "writeScopes")]
    pub write_scopes: Vec<String>,
}

/// The write-plan versus actual audit of one write-carrying exchange.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WriteAuditEvidence {
    /// The number of declared write entries in the verified plan.
    pub declared: usize,
    /// The logical paths actually changed in the staged sandbox view,
    /// in canonical (sorted) order.
    pub changed: Vec<String>,
    /// Changed paths outside the effective write scopes. Always empty
    /// on a completed exchange: any such path refuses the run before
    /// publication.
    #[serde(rename = "outsideScopes")]
    pub outside_scopes: Vec<String>,
}

impl ConfinementEvidence {
    /// Build the evidence from the session budget, the described and
    /// effective scopes of this exchange, and the sandbox's honest
    /// enforcement record. In-crate: the run path builds this after the
    /// exchange; the document itself serializes into receipts.
    pub(super) fn build(
        budget: &SessionBudget,
        described_read: &[String],
        described_write: &[String],
        effective_read: &[String],
        effective_write: &[String],
        writes: Option<WriteAuditEvidence>,
        report: ConfinementReport,
    ) -> Self {
        let network_mode = match budget.network() {
            NetworkBudget::Denied => "denied",
            NetworkBudget::Allowlist(_) => "allowlist",
        };
        let network_enforcement = match report.network {
            super::confinement::Enforcement::Degraded => "degraded-denied",
            _ => "enforced",
        };
        let child_policy = match budget.children() {
            ChildPolicy::Denied => "denied",
            ChildPolicy::Declared => "declared",
        };
        let children_enforcement = match (report.children_denied, report.children) {
            (false, _) => "permitted",
            (true, super::confinement::Enforcement::Enforced) => "denied-enforced",
            (true, super::confinement::Enforcement::Degraded) => "denied-bounded",
            (true, super::confinement::Enforcement::Unenforced) => "denied-unenforced",
        };
        let mut env: Vec<String> = budget.environment().keys().cloned().collect();
        env.sort();
        let mut env_dropped: Vec<String> = budget
            .environment()
            .keys()
            .filter(|name| super::confinement::env_grant_dropped(name))
            .cloned()
            .collect();
        env_dropped.sort();
        let mut read_caps = budget.read_caps();
        read_caps.sort();
        let mut write_caps = budget.write_caps();
        write_caps.sort();
        Self {
            budget: BudgetEvidence {
                read_scopes: read_caps,
                write_scopes: write_caps,
                scope_ceiling: budget.scope_ceiling(),
                env,
                env_dropped,
                network: NetworkEvidence {
                    mode: network_mode,
                    enforcement: network_enforcement,
                },
                children: ChildrenEvidence {
                    policy: child_policy,
                    enforcement: children_enforcement,
                    process_limit: report.process_limit(),
                },
                resources: ResourcesEvidence {
                    memory_limit: report.memory_limit(),
                    enforcement: report.resources.as_str(),
                },
            },
            described: scope_evidence(described_read, described_write),
            effective: scope_evidence(effective_read, effective_write),
            writes,
            platform: platform_token(),
        }
    }
}

impl WriteAuditEvidence {
    /// Build the audit from the declared plan size, the changed staged
    /// paths, and the paths outside the effective write scopes. Lists
    /// are sorted into canonical order; the passing case is an empty
    /// `outsideScopes`.
    pub fn new(declared: usize, changed: Vec<String>, outside_scopes: Vec<String>) -> Self {
        let mut changed = changed;
        changed.sort();
        let mut outside_scopes = outside_scopes;
        outside_scopes.sort();
        Self {
            declared,
            changed,
            outside_scopes,
        }
    }
}

/// Sorted scope projection.
fn scope_evidence(read: &[String], write: &[String]) -> ScopeEvidence {
    let mut read: Vec<String> = read.to_vec();
    read.sort();
    let mut write: Vec<String> = write.to_vec();
    write.sort();
    ScopeEvidence {
        read_scopes: read,
        write_scopes: write,
    }
}

/// The normalized `<os>-<arch>` token of the host platform. Fixed
/// spellings keep the evidence byte-stable across machines of one
/// platform.
fn platform_token() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "windows-x86_64",
        ("windows", "aarch64") => "windows-aarch64",
        ("macos", "x86_64") => "macos-x86_64",
        ("macos", "aarch64") => "macos-aarch64",
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
        _ => "other-unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter_package::budget::SessionBudget;

    #[test]
    fn the_evidence_carries_names_and_tokens_only() {
        let budget = SessionBudget::strict_implicit();
        let policy = super::super::confinement::SandboxPolicy::from_budget(&budget);
        let report = ConfinementReport::compute(policy);
        let evidence = ConfinementEvidence::build(
            &budget,
            &["b/**".to_owned(), "a/**".to_owned()],
            &[],
            &["a/**".to_owned(), "b/**".to_owned()],
            &[],
            None,
            report,
        );
        assert_eq!(
            evidence.described.read_scopes,
            ["a/**", "b/**"],
            "sorted canonical order"
        );
        assert!(evidence.budget.env.is_empty());
        assert_eq!(evidence.budget.env_dropped, Vec::<String>::new());
        assert_eq!(evidence.budget.network.mode, "denied");
        assert_eq!(evidence.budget.network.enforcement, "enforced");
        assert_eq!(evidence.budget.children.policy, "denied");
        assert_eq!(evidence.platform, platform_token());
        // The strict implicit budget caps nothing independently.
        assert!(evidence.budget.read_scopes.is_empty());
        assert_eq!(evidence.budget.scope_ceiling, "described");
        let manifested = SessionBudget::from_declared(
            &crate::adapter_package::permissions::DeclaredPermissions {
                read_scopes: vec!["src/**".to_owned()],
                write_scopes: vec!["gen/**".to_owned()],
                network_mode: "denied".to_owned(),
                network_destinations: Vec::new(),
                environment_allowlist: Vec::new(),
                child_processes: "denied".to_owned(),
                secret_handles: Vec::new(),
            },
        );
        let policy = super::super::confinement::SandboxPolicy::from_budget(&manifested);
        let report = ConfinementReport::compute(policy);
        let evidence = ConfinementEvidence::build(&manifested, &[], &[], &[], &[], None, report);
        assert_eq!(evidence.budget.scope_ceiling, "manifest");
    }

    #[test]
    fn serialization_is_deterministic_camel_case() {
        let budget = SessionBudget::strict_implicit();
        let policy = super::super::confinement::SandboxPolicy::from_budget(&budget);
        let report = ConfinementReport::compute(policy);
        let evidence = ConfinementEvidence::build(&budget, &[], &[], &[], &[], None, report);
        let bytes = serde_json::to_string(&evidence).expect("serializes");
        assert!(bytes.contains("\"readScopes\""), "camelCase wire: {bytes}");
        assert!(
            bytes.contains("\"scopeCeiling\":\"described\""),
            "the ceiling token rides the budget evidence: {bytes}"
        );
        assert!(!bytes.contains('\\'), "no escapes, no host paths: {bytes}");
    }
}
