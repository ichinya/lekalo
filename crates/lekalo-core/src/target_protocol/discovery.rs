//! Capability discovery over the safe describe handshake (issue #28).
//!
//! [`Discovery::run`] drives one adapter executable through the mandatory
//! describe exchange and translates the negotiated result into a typed,
//! deterministic [`DiscoveredAdapter`]. Discovery is safe by construction:
//! it sends `describe` only — no project IR path, no write operation, no
//! target or profile — so an incompatible adapter is characterized and
//! filtered before any project IR could ever be disclosed to it.
//!
//! Three distinctions the issue requires live here:
//!
//! - the adapter's **declared** digest (an identity claim in the describe
//!   response) versus the **verified executable digest** core computes
//!   over the launched program bytes;
//! - per-capability **provenance**: `declared` straight from the
//!   handshake, `probed` after a successful read-only operation through
//!   the same session, `verified` after a full planned-and-applied round;
//! - the discovery **cache**, keyed by every version and digest input, so
//!   a cached verdict is reused only while the adapter identity, its
//!   executable bytes, and every contract version are unchanged.

use serde::Serialize;
use std::sync::atomic::AtomicBool;

use super::capability;
use super::transport::AdapterCommand;
use super::{wire, TargetClient, TargetFailure};

/// How a capability's support state was established.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Provenance {
    /// Read from the describe response.
    Declared,
    /// Confirmed through a successful read-only operation.
    Probed,
    /// Confirmed through a full planned-and-applied exchange.
    Verified,
}

impl Provenance {
    /// The stable wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::Probed => "probed",
            Self::Verified => "verified",
        }
    }
}

/// One discovered capability with its versioned definition and provenance.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DiscoveredCapability {
    /// The stable capability id (`scan.symbols`).
    pub id: String,
    /// The support state the adapter declared.
    pub state: wire::SupportState,
    /// The definition version of the id's semantics.
    pub definition_version: &'static str,
    /// How the support state was established.
    pub provenance: Provenance,
}

/// One discovered adapter: the negotiated describe outcome, typed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DiscoveredAdapter {
    /// The adapter identity the describe response declared.
    pub adapter: wire::AdapterIdentity,
    /// The exact protocol version the session negotiated.
    pub negotiated_version: &'static str,
    /// Every protocol version the adapter declared.
    pub declared_protocols: Vec<String>,
    /// The IR contract versions the adapter declared (empty on a legacy
    /// 1.0.0 session: compatibility is then the #9 preflight's business).
    pub ir_versions: Vec<String>,
    /// The declared optional constraints, when present.
    pub constraints: Option<wire::AdapterConstraints>,
    /// Declared targets and profiles, in canonical order.
    pub targets: Vec<String>,
    pub profiles: Vec<String>,
    /// The digest over the canonical declared capability bytes.
    pub capability_digest: String,
    /// The digest over the launched program's bytes, when readable.
    pub executable_digest: Option<String>,
    /// The named capabilities, sorted by id.
    pub capabilities: Vec<DiscoveredCapability>,
}

impl DiscoveredAdapter {
    /// Whether the adapter declared the exact IR contract version. An
    /// empty declaration never matches: absence is not support.
    pub fn ir_compatible(&self, ir_version: &str) -> bool {
        self.ir_versions
            .iter()
            .any(|declared| declared == ir_version)
    }

    /// One discovered capability by id.
    pub fn capability(&self, id: &str) -> Option<&DiscoveredCapability> {
        self.capabilities.iter().find(|entry| entry.id == id)
    }

    /// The deterministic profile selection: the preferred token when the
    /// adapter declared it, else the lowest canonical declared profile.
    /// An adapter without profiles (profile-optional operations only)
    /// selects none.
    pub fn selected_profile(&self, preferred: Option<&str>) -> Option<String> {
        if let Some(preferred) = preferred {
            if self.profiles.iter().any(|offered| offered == preferred) {
                return Some(preferred.to_owned());
            }
        }
        self.profiles.iter().min().cloned()
    }

    /// Upgrade one capability's provenance to `probed` after a successful
    /// read-only operation. A declared `unsupported` state is never
    /// upgraded: the state is the adapter's claim, provenance only records
    /// how core confirmed it.
    pub fn mark_probed(&mut self, id: &str) {
        self.upgrade_provenance(id, Provenance::Probed);
    }

    /// Upgrade one capability's provenance to `verified` after a full
    /// planned-and-applied exchange.
    pub fn mark_verified(&mut self, id: &str) {
        self.upgrade_provenance(id, Provenance::Verified);
    }

    fn upgrade_provenance(&mut self, id: &str, provenance: Provenance) {
        if let Some(entry) = self.capabilities.iter_mut().find(|entry| entry.id == id) {
            if entry.state != wire::SupportState::Unsupported && entry.provenance < provenance {
                entry.provenance = provenance;
            }
        }
    }

    /// The lock candidate capability entries this discovery contributes
    /// for one selected profile under the platform-neutral target
    /// (issue #28 → #10 resolution). The entry version is the capability
    /// definition version, so the committed snapshot is bound to the
    /// versioned semantics the support states were read under. A declared
    /// `unknown`/`unsupported` state is recorded as such — the snapshot
    /// preserves the declaration and the resolver refuses to satisfy a
    /// required capability with it.
    pub fn capability_candidates(
        &self,
        profile: &crate::lockfile::ComponentId,
    ) -> Result<Vec<crate::lockfile::resolution::CandidateCapability>, crate::lockfile::LockFailure>
    {
        use crate::lockfile::resolution::CandidateCapability;
        use crate::lockfile::types::{
            CapabilityId, Platform, ProviderKind, ProviderRef, SemVer, Support,
        };
        let target = Platform::parse("any")?;
        let provider_version = SemVer::parse(&self.adapter.version)?;
        let provider = ProviderRef::new(
            ProviderKind::Adapter,
            crate::lockfile::types::ComponentId::parse(&self.adapter.id)?,
            provider_version,
        );
        self.capabilities
            .iter()
            .map(|entry| {
                let support = match entry.state {
                    wire::SupportState::Full => Support::Full,
                    wire::SupportState::Partial => Support::Partial,
                    wire::SupportState::Unsupported => Support::Unsupported,
                    wire::SupportState::Unknown => Support::Unknown,
                };
                Ok(CandidateCapability::new(
                    target.clone(),
                    profile.clone(),
                    CapabilityId::parse(&entry.id)?,
                    SemVer::parse(entry.definition_version)?,
                    support,
                    provider.clone(),
                ))
            })
            .collect()
    }
}

/// The discovery entry point.
pub struct Discovery;

impl Discovery {
    /// Discover one adapter through the safe describe handshake and
    /// record the verified executable digest. On success the client holds
    /// the live session; the returned value is the typed snapshot.
    pub fn run(
        client: &mut TargetClient,
        command: &AdapterCommand,
        cwd: &std::path::Path,
    ) -> Result<DiscoveredAdapter, TargetFailure> {
        Self::run_with_cancel(client, command, cwd, None)
    }

    /// [`Discovery::run`] with an explicit caller cancel flag.
    pub fn run_with_cancel(
        client: &mut TargetClient,
        command: &AdapterCommand,
        cwd: &std::path::Path,
        cancel: Option<&AtomicBool>,
    ) -> Result<DiscoveredAdapter, TargetFailure> {
        let outcome = client.describe_with_cancel(command, cwd, cancel)?;
        let described = outcome.clone();
        let capabilities = described
            .capabilities
            .capabilities
            .iter()
            .map(|(id, state)| DiscoveredCapability {
                definition_version: capability::definition(id)
                    .map(|entry| entry.definition_version)
                    .expect("ids are registry-checked by the handshake"),
                id: id.clone(),
                provenance: Provenance::Declared,
                state: *state,
            })
            .collect();
        Ok(DiscoveredAdapter {
            adapter: described.capabilities.adapter,
            negotiated_version: described.negotiated_version,
            declared_protocols: described.capabilities.protocol_versions,
            ir_versions: described.capabilities.ir_versions,
            constraints: described.capabilities.constraints,
            targets: described.capabilities.targets,
            profiles: described.capabilities.profiles,
            capability_digest: described.capability_digest,
            executable_digest: executable_digest(command),
            capabilities,
        })
    }
}

/// SHA-256 over the adapter entry's exact bytes, bounded like every file
/// read. Following the #27 runtime convention, the entry is the launched
/// executable, or its first argument when that names an existing regular
/// file (the interpreter-script convention). `None` records an unreadable
/// entry: verification evidence is missing, never forged.
fn executable_digest(command: &AdapterCommand) -> Option<String> {
    let script = command
        .args
        .first()
        .map(std::path::Path::new)
        .filter(|path| path.is_file());
    let entry = script.unwrap_or(command.program.as_path());
    let bytes = std::fs::read(entry).ok()?;
    Some(format!("sha256:{}", super::plan::sha256_hex(&bytes)))
}

/// The cache key of one discovery verdict: every version and digest input
/// that can invalidate it. Two keys are equal only when every component
/// matches exactly, so a changed adapter version, changed executable
/// bytes, changed negotiated protocol, changed IR version, changed
/// capability-definition registry, or changed capability digest misses.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize)]
pub struct CacheKey {
    /// The declared adapter identity (id, version, digest).
    pub adapter: wire::AdapterIdentity,
    /// The verified executable digest, when one was computed.
    pub executable_digest: Option<String>,
    /// The negotiated protocol version.
    pub negotiated_version: &'static str,
    /// The capability definition registry generation.
    pub capability_registry: &'static str,
    /// The core IR contract version the verdict answers for.
    pub ir_version: &'static str,
    /// The digest over the declared capability bytes.
    pub capability_digest: String,
}

/// The discovery cache: exact-key version/digest invalidation only. A
/// cached verdict never outlives its key; there is no partial matching
/// and no expiry-by-time, because determinism is the contract.
#[derive(Clone, Debug, Default)]
pub struct Cache {
    entries: Vec<(CacheKey, DiscoveredAdapter)>,
}

impl Cache {
    /// The empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// The cached verdict for an exact key.
    pub fn get(&self, key: &CacheKey) -> Option<&DiscoveredAdapter> {
        self.entries
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, value)| value)
    }

    /// Record one verdict under its exact key.
    pub fn insert(&mut self, key: CacheKey, adapter: DiscoveredAdapter) {
        self.entries.push((key, adapter));
    }

    /// The recorded verdict's cache key, or `None`.
    pub fn key(adapter: &DiscoveredAdapter, ir_version: &'static str) -> CacheKey {
        CacheKey {
            adapter: adapter.adapter.clone(),
            executable_digest: adapter.executable_digest.clone(),
            negotiated_version: adapter.negotiated_version,
            capability_registry: capability::REGISTRY_IDENTITY,
            ir_version,
            capability_digest: adapter.capability_digest.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_adapter(digest: &str, ir: &[&str]) -> DiscoveredAdapter {
        DiscoveredAdapter {
            adapter: wire::AdapterIdentity {
                id: "node-typescript".to_owned(),
                version: "0.1.0".to_owned(),
                digest: "sha256:46332b8c3649ad6fd9b4be16a212113dee1a7d084bd2dc910c9a1c82ceaa7778"
                    .to_owned(),
            },
            negotiated_version: super::super::version::VERSION,
            declared_protocols: vec!["1.0.0".to_owned(), "1.1.0".to_owned()],
            ir_versions: ir.iter().map(|value| value.to_string()).collect(),
            constraints: None,
            targets: vec!["node-typescript".to_owned()],
            profiles: vec!["default".to_owned()],
            capability_digest: digest.to_owned(),
            executable_digest: None,
            capabilities: vec![DiscoveredCapability {
                id: "scan.symbols".to_owned(),
                state: wire::SupportState::Full,
                definition_version: "1.0.0",
                provenance: Provenance::Declared,
            }],
        }
    }

    #[test]
    fn ir_compatibility_requires_exact_declared_membership() {
        let adapter = sample_adapter("sha256:aa", &["0.1.0"]);
        assert!(adapter.ir_compatible("0.1.0"));
        assert!(!adapter.ir_compatible("0.2.0"));
        let undeclared = sample_adapter("sha256:aa", &[]);
        assert!(
            !undeclared.ir_compatible("0.1.0"),
            "an undeclared IR version is never optimistic yes"
        );
    }

    #[test]
    fn profile_selection_is_deterministic() {
        let mut adapter = sample_adapter("sha256:aa", &["0.1.0"]);
        adapter.profiles = vec!["zeta".to_owned(), "alpha".to_owned()];
        assert_eq!(
            adapter.selected_profile(None).as_deref(),
            Some("alpha"),
            "no preference selects the lowest canonical profile"
        );
        assert_eq!(
            adapter.selected_profile(Some("zeta")).as_deref(),
            Some("zeta")
        );
        assert_eq!(
            adapter.selected_profile(Some("missing")).as_deref(),
            Some("alpha"),
            "a refused preference falls back deterministically"
        );
        adapter.profiles.clear();
        assert_eq!(adapter.selected_profile(None), None);
    }

    #[test]
    fn provenance_upgrades_monotonically_and_never_revives_unsupported() {
        let mut adapter = sample_adapter("sha256:aa", &["0.1.0"]);
        adapter.mark_probed("scan.symbols");
        assert_eq!(
            adapter.capability("scan.symbols").unwrap().provenance,
            Provenance::Probed
        );
        adapter.mark_probed("scan.symbols");
        assert_eq!(
            adapter.capability("scan.symbols").unwrap().provenance,
            Provenance::Probed,
            "an upgrade never downgrades"
        );
        adapter.mark_verified("scan.symbols");
        assert_eq!(
            adapter.capability("scan.symbols").unwrap().provenance,
            Provenance::Verified
        );
        adapter.capabilities.push(DiscoveredCapability {
            id: "generate.ui".to_owned(),
            state: wire::SupportState::Unsupported,
            definition_version: "1.0.0",
            provenance: Provenance::Declared,
        });
        adapter.mark_verified("generate.ui");
        assert_eq!(
            adapter.capability("generate.ui").unwrap().provenance,
            Provenance::Declared,
            "an unsupported declaration stays unsupported"
        );
        adapter.mark_verified("absent.capability");
    }

    #[test]
    fn cache_hits_only_on_the_exact_version_and_digest_key() {
        let mut cache = Cache::new();
        let adapter = sample_adapter("sha256:aa", &["0.1.0"]);
        let key = Cache::key(&adapter, crate::ir::version::VERSION);
        cache.insert(key.clone(), adapter);
        assert!(cache.get(&key).is_some(), "identical inputs hit");

        let mut different_digest = sample_adapter("sha256:bb", &["0.1.0"]);
        different_digest.adapter.digest =
            "sha256:46332b8c3649ad6fd9b4be16a212113dee1a7d084bd2dc910c9a1c82ceaa7777".to_owned();
        let key2 = Cache::key(&different_digest, crate::ir::version::VERSION);
        assert!(
            cache.get(&key2).is_none(),
            "a changed adapter digest misses"
        );

        let key3 = Cache::key(&sample_adapter("sha256:aa", &["0.1.0"]), "0.0.9");
        assert!(cache.get(&key3).is_none(), "a changed IR version misses");

        let mut changed_executable = sample_adapter("sha256:aa", &["0.1.0"]);
        changed_executable.executable_digest = Some("sha256:cc".to_owned());
        let key4 = Cache::key(&changed_executable, crate::ir::version::VERSION);
        assert!(cache.get(&key4).is_none(), "changed executable bytes miss");
    }
}
