//! Project admission and persistence of provider-defined scan facts.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::native_symbols::NativeSymbol;
use crate::project_fs::{Fs, FsErrorKind};
use crate::runtime;
use crate::target_protocol::scan_evidence::{self, Coverage, Fact};
use crate::target_protocol::{scopes, version, wire, LaunchPin};

use super::binding_rule::Rule;
use super::index::read_fingerprint;
use super::types::{
    BindingEvidence, BindingState, ObservedIndex, ScanSymbol, SymbolKind, SymbolRecord,
};

const CONFIG: &str = "lekalo.evidence.json";
pub(super) const RULE_KEY_PREFIX: &str = "binding:sha256:";
pub(super) const PINS_FLAG: &str = "--lekalo-evidence-pins-json";
type Result<T> = std::result::Result<T, &'static str>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema: String,
    scans: Vec<Selection>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    target: String,
    profile: Option<String>,
    producer: wire::AdapterIdentity,
    program_digest: String,
    entry_digest: String,
    families: BTreeMap<String, Pin>,
    http_projection: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pin {
    source: String,
    package_digest: String,
    schema_digest: String,
}

struct Provider {
    id: String,
    pin: Pin,
    validator: jsonschema::Validator,
    binding: Option<Rule>,
}

pub(super) struct Policy {
    producer: wire::AdapterIdentity,
    pub(super) launcher: LaunchPin,
    families: BTreeMap<String, Provider>,
    pub(super) http_projection: Option<String>,
}

fn valid_producer(producer: &wire::AdapterIdentity) -> bool {
    scopes::is_token(&producer.id)
        && wire::contract_version(&producer.version)
        && wire::is_sha256_digest(&producer.digest)
}

fn selections(root: &Path) -> Result<Vec<Selection>> {
    let fs = Fs::open(root).map_err(|_| "evidence-config-unreadable")?;
    let bytes = match fs.read_file_opt("", CONFIG, 64 * 1024) {
        Ok(Some(bytes)) => bytes,
        Ok(None) | Err(FsErrorKind::NotFound) => return Ok(Vec::new()),
        Err(_) => return Err("evidence-config-unreadable"),
    };
    let config: Config = serde_json::from_value(
        runtime::config::value(&bytes).map_err(|_| "evidence-config-invalid")?,
    )
    .map_err(|_| "evidence-config-invalid")?;
    let mut scopes = BTreeSet::new();
    if config.schema != "lekalo/evidence-config/v1"
        || config.scans.len() > 32
        || config.scans.iter().any(|s| {
            !scopes::is_token(&s.target)
                || s.profile.as_deref().is_some_and(|p| !scopes::is_token(p))
                || !valid_producer(&s.producer)
                || !wire::is_sha256_digest(&s.program_digest)
                || !wire::is_sha256_digest(&s.entry_digest)
                || !scopes.insert((&s.target, &s.profile))
                || s.http_projection
                    .as_ref()
                    .is_some_and(|f| !s.families.contains_key(f))
                || s.families.is_empty()
                || s.families.len() > scan_evidence::MAX_GROUPS
                || s.families.iter().any(|(family, pin)| {
                    !scopes::is_token(family)
                        || !wire::is_sha256_digest(&pin.package_digest)
                        || !wire::is_sha256_digest(&pin.schema_digest)
                })
        })
    {
        return Err("evidence-config-invalid");
    }
    Ok(config.scans)
}

/// Verify all configured provider schemas and their project-local pins offline.
pub fn verify_config(root: &Path) -> Result<()> {
    for selection in selections(root)? {
        Policy::from_selection(root, selection)?;
    }
    Ok(())
}

impl Policy {
    pub(super) fn load(root: &Path, target: &str, profile: Option<&str>) -> Result<Option<Self>> {
        selections(root)?
            .into_iter()
            .find(|s| s.target == target && s.profile.as_deref() == profile)
            .map(|selection| Self::from_selection(root, selection))
            .transpose()
    }

    fn from_selection(root: &Path, selection: Selection) -> Result<Self> {
        let mut families = BTreeMap::new();
        for (family, pin) in selection.families {
            let package = runtime::package::evidence_schema(
                root,
                &pin.source,
                &pin.package_digest,
                &pin.schema_digest,
            )
            .map_err(|_| "evidence-package-invalid")?;
            let validator =
                runtime::schema(&package.payload).map_err(|_| "evidence-schema-invalid")?;
            let binding = package
                .bindings
                .as_deref()
                .map(|bytes| Rule::parse(bytes, &package.payload))
                .transpose()?;
            families.insert(
                family,
                Provider {
                    id: package.id,
                    pin,
                    validator,
                    binding,
                },
            );
        }
        Ok(Self {
            producer: selection.producer,
            launcher: LaunchPin {
                program_digest: selection.program_digest,
                entry_digest: selection.entry_digest,
            },
            families,
            http_projection: selection.http_projection,
        })
    }

    pub(super) fn launch_pins(&self) -> String {
        let pins: BTreeMap<_, _> = self
            .families
            .iter()
            .map(|(family, p)| {
                (
                    family,
                    serde_json::json!({
                        "package_digest": p.pin.package_digest,
                        "schema_digest": p.pin.schema_digest,
                    }),
                )
            })
            .collect();
        serde_json::to_string(&pins).expect("evidence digests serialize")
    }

    pub(super) fn authorize(&self, protocol: &str, producer: &wire::AdapterIdentity) -> Result<()> {
        if protocol != version::EVIDENCE_SCAN_VERSION {
            return Err("evidence-protocol-required");
        }
        if &self.producer != producer {
            return Err("evidence-producer");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Provenance {
    producer: wire::AdapterIdentity,
    program_digest: String,
    entry_digest: String,
    package_digest: String,
    schema_digest: String,
    scan_profile_digest: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum State {
    Current,
    Unassessed,
    Stale,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    id: String,
    state: State,
    provider: String,
    provenance: Provenance,
    fact: Fact,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupRecord {
    family: String,
    coverage: Coverage,
    facts: Vec<Record>,
}

/// Only admission bound to a live adapter exchange can construct this value.
pub(super) struct Admitted {
    groups: Vec<GroupRecord>,
    pub(super) http_projection: Option<String>,
    bindings: BTreeMap<String, (String, SymbolKind)>,
}

impl Admitted {
    pub(super) fn resolve_bindings(
        &mut self,
        policy: &Policy,
        model: &crate::loader::NormalizedModel,
        roots: &BTreeSet<&str>,
    ) -> Result<()> {
        for group in &self.groups {
            let Some(rule) = policy
                .families
                .get(&group.family)
                .and_then(|family| family.binding.as_ref())
            else {
                continue;
            };
            let targets = rule.model_targets(&model.definitions);
            for record in &group.facts {
                let Some(symbol) = rule.matching_target(&targets, &record.fact)? else {
                    continue;
                };
                let subject = &record.fact.subject;
                if !roots.contains(subject.as_str()) {
                    return Err("evidence-binding-subject");
                }
                let binding = (symbol.to_owned(), rule.kind());
                if self
                    .bindings
                    .get(subject)
                    .is_some_and(|other| other != &binding)
                {
                    return Err("evidence-binding-ambiguous-subject");
                }
                self.bindings.insert(subject.clone(), binding);
            }
        }
        Ok(())
    }

    pub(super) fn binding(&self, native: &str) -> Option<(&str, SymbolKind)> {
        self.bindings
            .get(native)
            .map(|(id, kind)| (id.as_str(), *kind))
    }

    /// Existing confirmation refers to exact evidence. A rule-driven
    /// rescan can offer new candidates, but cannot reapprove them.
    pub(super) fn owned_binding_current(
        &self,
        previous: &ObservedIndex,
        old: &SymbolRecord,
        current: &ScanSymbol,
        bindings: &[BindingEvidence],
    ) -> Option<bool> {
        let native = old.stable_key.as_deref();
        let prior = native
            .map(|id| rule_facts(previous.evidence_groups.as_deref().unwrap_or_default(), id));
        let projected = current
            .candidates
            .iter()
            .any(|candidate| self.binding(&candidate.native).is_some());
        if !projected && prior.as_ref().is_none_or(Vec::is_empty) {
            return None;
        }
        let unchanged = native.is_some_and(|id| {
            old.state == BindingState::Current
                && current.kind == old.kind
                && self.binding(id).is_some_and(|(symbol, _)| symbol == old.id)
                && current.candidates.iter().any(|candidate| {
                    candidate.native == id
                        && old.location.as_ref().is_some_and(|location| {
                            candidate.path == location.path && candidate.line == location.line
                        })
                        && candidate.fingerprint == old.fingerprint
                })
                && bindings
                    .binary_search_by(|candidate| {
                        (candidate.symbol.as_str(), candidate.native.as_str())
                            .cmp(&(old.id.as_str(), id))
                    })
                    .is_ok_and(|position| bindings[position].evidence == old.evidence)
                && prior
                    .as_ref()
                    .is_some_and(|prior| *prior == rule_facts(&self.groups, id))
        });
        Some(unchanged)
    }

    pub(super) fn http_coverage(&self) -> Option<Coverage> {
        let family = self.http_projection.as_ref()?;
        self.groups
            .iter()
            .find(|g| &g.family == family)
            .map(|g| g.coverage)
    }

    /// Project the admitted HTTP family into the legacy endpoint view.
    pub(super) fn http_routes(
        &self,
    ) -> Result<BTreeMap<String, Vec<crate::http_routes::HttpRouteEvidence>>> {
        let Some(family) = self.http_projection.as_ref() else {
            return Ok(BTreeMap::new());
        };
        let group = self
            .groups
            .iter()
            .find(|g| &g.family == family)
            .filter(|g| g.coverage != Coverage::Unassessed)
            .ok_or("http-projection-unassessed")?;
        let mut routes = BTreeMap::<String, Vec<_>>::new();
        for record in &group.facts {
            let fact = &record.fact;
            let route: crate::http_routes::HttpRouteEvidence =
                serde_json::from_value(fact.payload.clone())
                    .map_err(|_| "http-projection-payload")?;
            if !route.valid()
                || route.handler_native != fact.subject
                || fact.references.len() != 1
                || fact.references.get("application") != Some(&route.application_native)
                || !route
                    .sources
                    .iter()
                    .map(|s| (&s.path, &s.source_digest))
                    .eq(fact.sources.iter().map(|s| (&s.path, &s.source_digest)))
            {
                return Err("http-projection-custody");
            }
            routes.entry(fact.subject.clone()).or_default().push(route);
        }
        Ok(routes)
    }
}

fn is_current_rule_fact(record: &Record) -> bool {
    record.state == State::Current && record.fact.key.starts_with(RULE_KEY_PREFIX)
}

pub(super) fn rule_subjects(index: &ObservedIndex) -> BTreeSet<&str> {
    index
        .evidence_groups
        .as_deref()
        .unwrap_or_default()
        .iter()
        .flat_map(|group| &group.facts)
        .filter(|record| is_current_rule_fact(record))
        .map(|record| record.fact.subject.as_str())
        .collect()
}

fn rule_facts<'a>(groups: &'a [GroupRecord], native: &str) -> Vec<(&'a str, &'a Record)> {
    groups
        .iter()
        .flat_map(|group| {
            group
                .facts
                .iter()
                .filter(|record| record.fact.subject == native && is_current_rule_fact(record))
                .map(|record| (group.family.as_str(), record))
        })
        .collect()
}

fn native_paths(nodes: &[NativeSymbol]) -> BTreeMap<&str, &str> {
    nodes
        .iter()
        .map(|n| (n.native.as_str(), n.path.as_str()))
        .collect()
}

fn fact_id(scope: (&str, &str, Option<&str>), family: &str, provider: &str, fact: &Fact) -> String {
    runtime::digest(
        &serde_json::to_vec(&(scope, family, provider, &fact.subject, &fact.key))
            .expect("fact identity serializes"),
    )
}

struct FactCustody<'a> {
    nodes: BTreeMap<&'a str, &'a str>,
    sources: BTreeSet<(&'a str, &'a str)>,
}

impl<'a> FactCustody<'a> {
    fn new(
        entries: &'a [wire::ScanEntry],
        nodes: &'a [NativeSymbol],
        read_scopes: &[String],
    ) -> Self {
        let nodes = native_paths(nodes);
        // Each entry digest has already been checked against the real source.
        let sources: BTreeSet<_> = entries
            .iter()
            .filter_map(|entry| {
                if !read_scopes
                    .iter()
                    .any(|scope| scopes::scope_covers(scope, &entry.path))
                {
                    return None;
                }
                let digest = entry.evidence.as_ref()?.source_digest.as_deref()?;
                Some((entry.path.as_str(), digest))
            })
            .collect();
        Self { nodes, sources }
    }

    fn check(&self, fact: &Fact) -> Result<()> {
        if !fact.anchors_match(&self.nodes)
            || fact.sources.iter().any(|source| {
                !self
                    .sources
                    .contains(&(source.path.as_str(), source.source_digest.as_str()))
            })
        {
            return Err("evidence-source-custody");
        }
        Ok(())
    }
}

pub(super) fn verify_envelope(
    response: &wire::ResponseEnvelope,
    entries: &[wire::ScanEntry],
    nodes: &[NativeSymbol],
    read_scopes: &[String],
) -> Result<()> {
    let groups = response
        .result
        .as_ref()
        .and_then(|result| result.evidence_groups.as_ref())
        .ok_or("evidence-groups-missing")?;
    let custody = FactCustody::new(entries, nodes, read_scopes);
    for group in groups {
        let mut identities = BTreeSet::new();
        for fact in &group.facts {
            custody.check(fact)?;
            if !identities.insert((&fact.subject, &fact.key)) {
                return Err("evidence-duplicate-fact");
            }
        }
    }
    Ok(())
}

pub(super) fn admit(
    policy: Option<&Policy>,
    response: &wire::ResponseEnvelope,
    scope: (&str, &str, Option<&str>),
    entries: &[wire::ScanEntry],
    nodes: &[NativeSymbol],
    read_scopes: &[String],
) -> Result<Option<Admitted>> {
    if response.protocol_version != version::EVIDENCE_SCAN_VERSION {
        return if policy.is_some() {
            Err("evidence-protocol-required")
        } else {
            Ok(None)
        };
    }
    let groups = response
        .result
        .as_ref()
        .and_then(|r| r.evidence_groups.as_ref())
        .ok_or("evidence-groups-missing")?;
    let profile_digest = response
        .evidence
        .scan_profile_digest
        .as_ref()
        .ok_or("evidence-scan-profile-missing")?;
    let custody = FactCustody::new(entries, nodes, read_scopes);
    let mut admitted = BTreeMap::new();
    for group in groups {
        let policy = policy.ok_or("evidence-family-disabled")?;
        let provider = policy
            .families
            .get(&group.family)
            .ok_or("evidence-family-disabled")?;
        if group.package_digest != provider.pin.package_digest
            || group.schema_digest != provider.pin.schema_digest
        {
            return Err("evidence-package-pin");
        }
        let mut facts = Vec::new();
        let mut ids = BTreeSet::new();
        for fact in &group.facts {
            custody.check(fact)?;
            if !provider.validator.is_valid(&fact.payload) {
                return Err("evidence-payload");
            }
            let mut fact = fact.clone();
            if let Some(binding) = &provider.binding {
                binding.verify_fact(&fact)?;
                fact.key = binding.identity_key(&fact);
            } else if fact.key.starts_with(RULE_KEY_PREFIX) {
                return Err("evidence-binding-key-reserved");
            }
            let id = fact_id(scope, &group.family, &provider.id, &fact);
            if !ids.insert(id.clone()) {
                return Err("evidence-duplicate-fact");
            }
            fact.sources.sort_by(|a, b| a.path.cmp(&b.path));
            facts.push(Record {
                id,
                state: State::Current,
                provider: provider.id.clone(),
                provenance: Provenance {
                    producer: policy.producer.clone(),
                    program_digest: policy.launcher.program_digest.clone(),
                    entry_digest: policy.launcher.entry_digest.clone(),
                    package_digest: group.package_digest.clone(),
                    schema_digest: group.schema_digest.clone(),
                    scan_profile_digest: profile_digest.clone(),
                },
                fact,
            });
        }
        facts.sort_by(|a, b| a.id.cmp(&b.id));
        admitted.insert(
            group.family.clone(),
            GroupRecord {
                family: group.family.clone(),
                coverage: group.coverage,
                facts,
            },
        );
    }
    if let Some(policy) = policy {
        for family in policy.families.keys() {
            admitted
                .entry(family.clone())
                .or_insert_with(|| unassessed(family));
        }
    }
    Ok(Some(Admitted {
        groups: admitted.into_values().collect(),
        http_projection: policy.and_then(|p| p.http_projection.clone()),
        bindings: BTreeMap::new(),
    }))
}

fn unassessed(family: &str) -> GroupRecord {
    GroupRecord {
        family: family.to_owned(),
        coverage: Coverage::Unassessed,
        facts: Vec::new(),
    }
}

fn sources_current(fact: &Fact, fs: &Fs, cache: &mut BTreeMap<String, Option<String>>) -> bool {
    fact.sources.iter().all(|source| {
        cache
            .entry(source.path.clone())
            .or_insert_with(|| read_fingerprint(fs, &source.path).ok().flatten())
            .as_deref()
            == Some(source.source_digest.as_str())
    })
}

impl Admitted {
    pub(super) fn revision_digest(&self) -> String {
        let bytes = if self.bindings.is_empty() {
            serde_json::to_vec(&(&self.groups, &self.http_projection))
        } else {
            serde_json::to_vec(&(&self.groups, &self.http_projection, &self.bindings))
        };
        runtime::digest(&bytes.expect("admitted evidence serializes"))
    }

    pub(super) fn merge(self, root: &Path, previous: &[GroupRecord]) -> Result<Vec<GroupRecord>> {
        let fs = Fs::open(root).map_err(|_| "evidence-root-unreadable")?;
        let mut cache = BTreeMap::new();
        let mut groups: BTreeMap<_, _> = self
            .groups
            .into_iter()
            .map(|g| (g.family.clone(), g))
            .collect();
        for old in previous {
            let group = groups
                .entry(old.family.clone())
                .or_insert_with(|| unassessed(&old.family));
            if group.coverage == Coverage::Complete {
                continue;
            }
            let newest: BTreeSet<_> = group.facts.iter().map(|r| r.id.clone()).collect();
            for mut retained in old
                .facts
                .iter()
                .filter(|r| !newest.contains(&r.id))
                .cloned()
            {
                retained.state = if sources_current(&retained.fact, &fs, &mut cache) {
                    State::Unassessed
                } else {
                    State::Stale
                };
                group.facts.push(retained);
            }
        }
        let mut groups: Vec<_> = groups.into_values().collect();
        for group in &mut groups {
            group.facts.sort_by(|a, b| a.id.cmp(&b.id));
            for record in group.facts.iter().filter(|r| r.state == State::Current) {
                if !sources_current(&record.fact, &fs, &mut cache) {
                    return Err("evidence-source-custody");
                }
            }
        }
        Ok(groups)
    }
}

fn current_facts(index: &ObservedIndex) -> impl Iterator<Item = &Fact> {
    index
        .evidence_groups
        .iter()
        .flatten()
        .flat_map(|group| &group.facts)
        .filter(|record| record.state == State::Current)
        .map(|record| &record.fact)
}

pub(super) fn check_sources(index: &ObservedIndex, fs: &Fs) -> bool {
    let mut cache = BTreeMap::new();
    current_facts(index).all(|fact| sources_current(fact, fs, &mut cache))
}

/// Bind confirmation to the candidate's own admitted facts. Other native
/// subjects can have stale facts without invalidating this candidate.
pub(super) fn candidate_sources_current(index: &ObservedIndex, fs: &Fs, native: &str) -> bool {
    let mut cache = BTreeMap::new();
    current_facts(index)
        .filter(|fact| fact.subject == native)
        .all(|fact| sources_current(fact, fs, &mut cache))
}

pub(super) fn stale_subjects<'a>(index: &'a ObservedIndex, fs: &Fs) -> BTreeSet<&'a str> {
    let mut cache = BTreeMap::new();
    current_facts(index)
        .filter(|fact| !sources_current(fact, fs, &mut cache))
        .map(|fact| fact.subject.as_str())
        .collect()
}

pub(super) fn valid_index(index: &ObservedIndex) -> bool {
    let (Some(groups), Some(target)) = (&index.evidence_groups, &index.target) else {
        return false;
    };
    let scope = (
        index.project.as_str(),
        target.as_str(),
        index.profile.as_deref(),
    );
    let nodes = native_paths(&index.native_symbols);
    let projection_valid = index.http_projection.as_ref().is_none_or(|family| {
        groups
            .iter()
            .any(|g| &g.family == family && g.coverage != Coverage::Unassessed)
    });
    projection_valid
        && groups.len() <= scan_evidence::MAX_GROUPS
        && groups.iter().map(|g| g.facts.len()).sum::<usize>() <= scan_evidence::MAX_FACTS
        && groups
            .windows(2)
            .all(|pair| pair[0].family < pair[1].family)
        && groups.iter().all(|g| {
            scopes::is_token(&g.family)
                && g.facts.windows(2).all(|pair| pair[0].id < pair[1].id)
                && g.facts.iter().all(|r| {
                    runtime::config::token(&r.provider)
                        && valid_producer(&r.provenance.producer)
                        && [
                            &r.provenance.program_digest,
                            &r.provenance.entry_digest,
                            &r.provenance.package_digest,
                            &r.provenance.schema_digest,
                            &r.provenance.scan_profile_digest,
                        ]
                        .iter()
                        .all(|digest| wire::is_sha256_digest(digest))
                        && r.fact.valid()
                        && r.fact.sources.windows(2).all(|p| p[0].path < p[1].path)
                        && r.id == fact_id(scope, &g.family, &r.provider, &r.fact)
                        && match r.state {
                            State::Current => {
                                g.coverage != Coverage::Unassessed
                                    && r.fact.anchors_match(&nodes)
                                    && r.provenance.producer.id == index.adapter.id
                                    && r.provenance.producer.version == index.adapter.version
                                    && Some(&r.provenance.producer.digest)
                                        == index.adapter.digest.as_ref()
                            }
                            _ => g.coverage != Coverage::Complete,
                        }
                })
        })
}
