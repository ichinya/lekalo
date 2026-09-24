//! The execution confinement budget (issue #89).
//!
//! The manifest's declared `permissions` block (projected by
//! [`super::permissions::declared_permissions`]) is the enforcement
//! ceiling of one adapter session: [`SessionBudget`] turns that ceiling
//! into the closed budget the run path checks before any sandbox is
//! built. The projection is fail-closed:
//!
//! - a described scope wider than the budget caps is an escalation
//!   refusal (`adapter.permission-escalated`) unless an explicit
//!   policy upgrade allows it;
//! - a synthesized/implicit descriptor claims nothing, so its budget is
//!   the strict default: no environment, no secrets, network denied,
//!   children denied, and filesystem bounds equal to whatever the
//!   adapter itself describes (the describe claim is the only claim it
//!   has, and describe is sandboxed inside those bounds);
//! - network `allowlist` has no namespace-level destination filter on
//!   any supported platform, so the run degrades to network-denied and
//!   the evidence records the honest capability gap (never a silent
//!   allowance);
//! - secret handles resolve to controlled environment variable names at
//!   spawn time only. Handle ids and variable names may enter
//!   diagnostics and evidence; **values never do** — a value exists
//!   solely inside the child's environment block.

use std::collections::BTreeMap;

use serde::Serialize;

use super::manifest::ManifestDocument;
use super::permissions::{declared_permissions, DeclaredPermissions};

/// The controlled name prefix a secret handle resolves to at spawn
/// time. The host value under `LEKALO_SECRET_<HANDLE>` (when present)
/// is injected into the child environment; the name itself is the only
/// thing core ever stores or reports.
pub const SECRET_ENV_PREFIX: &str = "LEKALO_SECRET_";

/// Where one granted environment variable's value comes from.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "source")]
pub enum EnvSource {
    /// The value is read from the host environment of the same name at
    /// spawn time.
    HostEnv,
    /// The variable is the controlled name of a declared secret handle;
    /// the value is read from the host environment at spawn time and
    /// never persisted anywhere.
    SecretHandle,
}

/// The network posture of one session.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkBudget {
    /// No network access. Enforced by the sandbox on every supported
    /// platform (namespace unsharing, LPAC, deny-default profile).
    #[default]
    Denied,
    /// An allowlist of destinations. No supported platform offers a
    /// namespace-level destination filter, so a session with this
    /// budget runs network-denied and records the degraded
    /// enforcement honestly in the confinement evidence.
    Allowlist(Vec<String>),
}

/// The child-process policy of one session.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChildPolicy {
    /// The adapter may not spawn children. Enforced where the platform
    /// has a primitive (Windows job active-process cap, Linux bounded
    /// process count); recorded as an honest gap where it does not.
    #[default]
    Denied,
    /// The adapter declared child processes; the current confinement
    /// applies unchanged (children stay inside the namespace or job).
    Declared,
}

/// How a described scope wider than the manifest ceiling is handled.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExpansionPolicy {
    /// Refuse with `adapter.permission-escalated` (the default).
    #[default]
    Refuse,
    /// Allow the widened scopes under an explicit operator policy
    /// (`--allow-permission-expansion`). The widening stays visible in
    /// the confinement evidence: `described` exceeds `budget`.
    AllowEscalated,
}

/// The filesystem scope ceiling of one session.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScopeCaps {
    /// The manifest ceiling: every described scope must fit inside
    /// these caps.
    Declared(Vec<String>),
    /// The implicit adapter has no independent claim; whatever it
    /// describes is the bound (describe itself runs with empty scopes,
    /// so this bounds nothing beyond the adapter's own claim).
    #[default]
    Described,
}

impl ScopeCaps {
    /// The sorted cap list for the evidence (`Described` publishes no
    /// independent cap — the budget claims nothing).
    pub fn caps(&self) -> Vec<String> {
        match self {
            Self::Declared(scopes) => scopes.clone(),
            Self::Described => Vec::new(),
        }
    }

    /// Whether one described scope list fits entirely inside the caps.
    fn covers(&self, described: &[String]) -> bool {
        match self {
            Self::Described => true,
            Self::Declared(caps) => described
                .iter()
                .all(|scope| caps.iter().any(|cap| scope_within(scope, cap))),
        }
    }
}

/// Whether one declared scope reaches only paths another scope covers
/// (used to compare a described scope against the manifest ceiling).
/// Mirrors the plan-coverage grammar ([`scopes::scope_covers`]): a
/// recursive cap covers the contents below its base, never the bare
/// base itself — an exact described scope `a` under a cap `a/**`
/// refuses (issue #89 fix round 2, C-F6).
fn scope_within(scope: &str, cap: &str) -> bool {
    if scope == cap {
        return true;
    }
    let scope_recursive = scope.ends_with("/**");
    let cap_recursive = cap.ends_with("/**");
    let scope_base = scope.strip_suffix("/**").unwrap_or(scope);
    let cap_base = cap.strip_suffix("/**").unwrap_or(cap);
    if cap_recursive {
        // `a/**` covers `a/b`, `a/b/c`, and `a/b/**` — and an equal
        // recursive scope was handled above — but never the bare base
        // `a` itself: the cap covers contents, not the base.
        scope_base.starts_with(cap_base) && scope_base.as_bytes().get(cap_base.len()) == Some(&b'/')
    } else {
        // An exact cap covers only the exact scope.
        !scope_recursive && scope_base == cap_base
    }
}

/// The closed execution budget of one adapter session (issue #89).
///
/// Built from the verified manifest's declared permissions, or the
/// strict default for synthesized/implicit descriptors. The budget is
/// the enforcement ceiling: the run path refuses any describe or
/// request beyond it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SessionBudget {
    /// The read-scope ceiling.
    read_scope_caps: ScopeCaps,
    /// The write-scope ceiling.
    write_scope_caps: ScopeCaps,
    /// The granted environment variables by name and value source.
    /// Values never enter this map, diagnostics, or evidence.
    environment: BTreeMap<String, EnvSource>,
    /// The network posture.
    network: NetworkBudget,
    /// The child-process policy.
    children: ChildPolicy,
    /// How scope escalation beyond the ceiling is handled.
    policy: ExpansionPolicy,
}

/// Which described scope list exceeded the budget ceiling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionEscalation {
    /// `capabilities.readScopes` exceeds the read ceiling.
    ReadScopes,
    /// `capabilities.writeScopes` exceeds the write ceiling.
    WriteScopes,
}

impl PermissionEscalation {
    /// The wire token of the exceeded capability member, carried in the
    /// `adapter.permission-escalated` diagnostic data.
    pub const fn member(self) -> &'static str {
        match self {
            Self::ReadScopes => "capabilities.readScopes",
            Self::WriteScopes => "capabilities.writeScopes",
        }
    }
}

/// The scopes a session may actually use after the budget check.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EffectiveScopes {
    pub read: Vec<String>,
    pub write: Vec<String>,
}

impl SessionBudget {
    /// The strict default for synthesized/implicit descriptors: no
    /// environment, no secrets, network denied, children denied, and
    /// no independent filesystem claim (describe bounds).
    pub fn strict_implicit() -> Self {
        Self {
            read_scope_caps: ScopeCaps::Described,
            write_scope_caps: ScopeCaps::Described,
            environment: BTreeMap::new(),
            network: NetworkBudget::Denied,
            children: ChildPolicy::Denied,
            policy: ExpansionPolicy::Refuse,
        }
    }

    /// Build the budget from one declared-permission projection.
    /// Network `allowlist` keeps its destinations for the evidence; the
    /// sandbox still enforces denial (honest degradation, never a
    /// silent allowance).
    pub fn from_declared(declared: &DeclaredPermissions) -> Self {
        let mut environment = BTreeMap::new();
        for name in &declared.environment_allowlist {
            environment.insert(name.clone(), EnvSource::HostEnv);
        }
        for handle in &declared.secret_handles {
            environment.insert(
                format!("{SECRET_ENV_PREFIX}{}", handle.to_uppercase()),
                EnvSource::SecretHandle,
            );
        }
        Self {
            read_scope_caps: ScopeCaps::Declared(declared.read_scopes.clone()),
            write_scope_caps: ScopeCaps::Declared(declared.write_scopes.clone()),
            network: if declared.network_mode == "allowlist" {
                NetworkBudget::Allowlist(declared.network_destinations.clone())
            } else {
                NetworkBudget::Denied
            },
            children: if declared.child_processes == "declared" {
                ChildPolicy::Declared
            } else {
                ChildPolicy::Denied
            },
            environment,
            policy: ExpansionPolicy::Refuse,
        }
    }

    /// Build the budget for one resolved candidate: a real manifest
    /// projects its declared permissions; a synthesized/implicit
    /// descriptor gets the strict default.
    pub fn budget_for(
        manifest: Option<&ManifestDocument>,
    ) -> Result<Self, super::types::PackageFailure> {
        match manifest {
            Some(manifest) => declared_permissions(manifest)
                .map(|declared| Self::from_declared(&declared))
                .map_err(|reason| super::types::PackageFailure::ManifestInvalid { reason }),
            None => Ok(Self::strict_implicit()),
        }
    }

    /// Opt this budget into explicit escalation tolerance (the
    /// `--allow-permission-expansion` policy). Refuses nothing; the
    /// widening stays visible in the evidence.
    pub fn with_expansion_allowed(mut self) -> Self {
        self.policy = ExpansionPolicy::AllowEscalated;
        self
    }

    /// The read-scope ceiling (sorted; empty when the budget makes no
    /// independent claim).
    pub fn read_caps(&self) -> Vec<String> {
        self.read_scope_caps.caps()
    }

    /// The write-scope ceiling (sorted; empty when the budget makes no
    /// independent claim).
    pub fn write_caps(&self) -> Vec<String> {
        self.write_scope_caps.caps()
    }

    /// Where the scope ceilings come from: `manifest` names the verified
    /// package manifest as the ceiling; `described` means the budget
    /// claims nothing independently (the strict implicit default) and
    /// the adapter's own describe bounds apply. The evidence publishes
    /// the token so an auditor reads empty cap lists correctly
    /// (issue #89 fix round 2, C-F4).
    pub fn scope_ceiling(&self) -> &'static str {
        match self.read_scope_caps {
            ScopeCaps::Declared(_) => "manifest",
            ScopeCaps::Described => "described",
        }
    }

    /// The granted environment variable names and their value sources,
    /// in canonical (sorted) name order. Values are never carried.
    pub fn environment(&self) -> &BTreeMap<String, EnvSource> {
        &self.environment
    }

    /// The network posture.
    pub fn network(&self) -> &NetworkBudget {
        &self.network
    }

    /// The child-process policy.
    pub fn children(&self) -> ChildPolicy {
        self.children
    }

    /// The escalation policy.
    pub fn policy(&self) -> ExpansionPolicy {
        self.policy
    }

    /// Resolve the spawn-time environment pairs from the budget: host
    /// values are read at spawn, secret handles resolve to their
    /// controlled names. A variable with no host value grants nothing
    /// (core never fabricates values). Sorted by name; values are for
    /// the child environment block only.
    pub fn resolve_environment(&self) -> Vec<(String, String)> {
        let mut names: Vec<(String, String)> = Vec::new();
        for name in self.environment.keys() {
            if let Ok(value) = std::env::var(name) {
                names.push((name.clone(), value));
            }
        }
        names
    }

    /// Check the described scopes against the ceiling. A described list
    /// beyond the caps is an escalation refusal unless the explicit
    /// policy upgrade applies; within the caps, the effective scopes
    /// are the described ones (the intersection equals them because a
    /// refusal fires on any excess).
    pub fn check_scopes(
        &self,
        described_read: &[String],
        described_write: &[String],
    ) -> Result<EffectiveScopes, PermissionEscalation> {
        let within = self.read_scope_caps.covers(described_read)
            && self.write_scope_caps.covers(described_write);
        if !within && self.policy != ExpansionPolicy::AllowEscalated {
            return Err(if !self.read_scope_caps.covers(described_read) {
                PermissionEscalation::ReadScopes
            } else {
                PermissionEscalation::WriteScopes
            });
        }
        Ok(EffectiveScopes {
            read: described_read.to_vec(),
            write: described_write.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(read: &[&str], write: &[&str]) -> DeclaredPermissions {
        DeclaredPermissions {
            read_scopes: read.iter().map(|s| s.to_string()).collect(),
            write_scopes: write.iter().map(|s| s.to_string()).collect(),
            network_mode: "denied".to_owned(),
            network_destinations: Vec::new(),
            environment_allowlist: vec!["LEKALO_TEST_VAR".to_owned()],
            child_processes: "denied".to_owned(),
            secret_handles: vec!["kyber".to_owned()],
        }
    }

    #[test]
    fn the_budget_projects_the_declared_permissions() {
        let budget = SessionBudget::from_declared(&declared(&["src/**"], &["gen/**"]));
        assert_eq!(budget.read_caps(), ["src/**"]);
        assert_eq!(budget.write_caps(), ["gen/**"]);
        assert_eq!(budget.children(), ChildPolicy::Denied);
        assert_eq!(budget.network(), &NetworkBudget::Denied);
        assert_eq!(budget.policy(), ExpansionPolicy::Refuse);
        let names = budget.environment().keys().cloned().collect::<Vec<_>>();
        assert_eq!(
            names,
            ["LEKALO_SECRET_KYBER", "LEKALO_TEST_VAR"],
            "secret handles resolve to controlled names"
        );
    }

    #[test]
    fn allowlist_network_is_recorded_but_denial_stays_the_enforcement() {
        let mut declared = declared(&[], &[]);
        declared.network_mode = "allowlist".to_owned();
        declared.network_destinations = vec!["api.example.com:443".to_owned()];
        let budget = SessionBudget::from_declared(&declared);
        assert_eq!(
            budget.network(),
            &NetworkBudget::Allowlist(vec!["api.example.com:443".to_owned()])
        );
        // The budget never grants network access: the sandbox denies
        // and the evidence records the degradation.
    }

    #[test]
    fn the_strict_default_claims_nothing() {
        let budget = SessionBudget::strict_implicit();
        assert!(budget.read_caps().is_empty() && budget.write_caps().is_empty());
        assert!(budget.environment().is_empty());
        assert_eq!(budget.children(), ChildPolicy::Denied);
        assert_eq!(budget.network(), &NetworkBudget::Denied);
        // Whatever describe claims bounds the implicit adapter.
        let effective = budget
            .check_scopes(&["src/**".to_owned()], &["gen/**".to_owned()])
            .expect("described bounds");
        assert_eq!(effective.read, ["src/**"]);
        assert_eq!(effective.write, ["gen/**"]);
    }

    #[test]
    fn described_scopes_beyond_the_ceiling_refuse() {
        let budget = SessionBudget::from_declared(&declared(&["src/**"], &["gen/**"]));
        assert_eq!(
            budget.check_scopes(&["other/**".to_owned()], &[]),
            Err(PermissionEscalation::ReadScopes)
        );
        assert_eq!(
            budget.check_scopes(&[], &["other/**".to_owned()]),
            Err(PermissionEscalation::WriteScopes)
        );
        // Within the ceiling the described scopes pass unchanged.
        let effective = budget
            .check_scopes(&["src/**".to_owned()], &["gen/**".to_owned()])
            .expect("within ceiling");
        assert_eq!(effective.read, ["src/**"]);
        // An exact-file cap covers exact paths only.
        let strict = SessionBudget::from_declared(&declared(&["src/main.ts"], &[]));
        assert_eq!(
            strict.check_scopes(&["src/**".to_owned()], &[]),
            Err(PermissionEscalation::ReadScopes)
        );
        assert!(strict
            .check_scopes(&["src/main.ts".to_owned()], &[])
            .is_ok());
        // A bare base under a recursive cap refuses: `src/**` covers the
        // contents of `src`, never `src` itself (issue #89, C-F6).
        assert_eq!(
            budget.check_scopes(&["src".to_owned()], &[]),
            Err(PermissionEscalation::ReadScopes)
        );
    }

    #[test]
    fn explicit_policy_allows_and_records_the_widening() {
        let budget = SessionBudget::from_declared(&declared(&[], &[])).with_expansion_allowed();
        let effective = budget
            .check_scopes(&["src/**".to_owned()], &[])
            .expect("policy upgrade");
        assert_eq!(effective.read, ["src/**"]);
        // The widening stays visible: the budget caps remain empty.
        assert!(budget.read_caps().is_empty());
    }

    #[test]
    fn scope_containment_is_exact_or_recursive() {
        assert!(scope_within("src/**", "src/**"));
        assert!(scope_within("src/a/**", "src/**"));
        assert!(scope_within("src/a", "src/**"));
        assert!(!scope_within("src/**", "src/a/**"));
        assert!(!scope_within("srcx/**", "src/**"));
        assert!(scope_within("src/main.ts", "src/main.ts"));
        assert!(!scope_within("src/**", "src/main.ts"));
        assert!(!scope_within("src/other.ts", "src/main.ts"));
    }

    /// A recursive cap covers the contents below its base, never the
    /// bare base itself — mirroring the plan-coverage rule
    /// (`scopes::scope_covers`), issue #89 fix round 2, C-F6.
    #[test]
    fn a_recursive_cap_does_not_cover_its_bare_base() {
        assert!(!scope_within("src", "src/**"));
        assert!(!scope_within("src", "src/a/**"));
        // Equal scopes still admit each other, recursive or exact.
        assert!(scope_within("src/**", "src/**"));
        assert!(scope_within("src", "src"));
        // Contents below the base keep passing.
        assert!(scope_within("src/b", "src/**"));
        assert!(scope_within("src/b/c", "src/**"));
        assert!(scope_within("src/b/**", "src/**"));
        // Deep recursive caps cover strictly below their base too.
        assert!(!scope_within("src/a", "src/a/b/**"));
        assert!(scope_within("src/a/b/c", "src/a/b/**"));
    }
}
