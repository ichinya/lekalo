//! Issue #18: the pure, read-only semantic diff of two accepted,
//! normalized, immutable `CompiledProject` values.
//!
//! The comparison never reads YAML text, line diffs, physical paths,
//! source bytes, Git state, clocks, or the filesystem: both sides are
//! already typed IR. It validates contract-family compatibility, projects
//! each side into a typed canonical semantic projection, compares by
//! stable semantic IDs and member keys, resolves renames and tombstones
//! only through the declared `renamed_from` claims and the #6 history
//! registry, classifies compatibility per explicit built-in profile, and
//! merges only validated namespaced adapter contributions. Ambiguous,
//! cyclic, dangling, conflicting, or version-invalid history classifies as
//! `unknown` — never a guessed alias.
//!
//! Determinism: canonical byte output, stable change identities, sorted
//! reasons, seeds, hints, and dominance-ordered classes. Bounds: every
//! recorded limit rejects with a typed `diff.*` diagnostic and no partial
//! result.

pub mod adapter;
pub mod canonical;
pub mod change;
pub mod compatibility;
pub mod diagnostic;
pub mod identity;
pub mod projection;
pub mod reason;
pub mod seed;
pub mod version;

use crate::diagnostics::DiagnosticSet;
use crate::ir::{CompiledProject, IdRegistry, Tombstone};
use std::collections::{BTreeMap, BTreeSet};

pub use adapter::{AdapterError, AdapterInput, AdapterTrust, ContributionEffect, ContributionSeed};
pub use change::{ChangeKind, ChangeRecord, Side, Summary};
pub use compatibility::{
    BlockedOn, CompatibilityClass, MigrationHint, ProfileDecision, ProfileId, ProfileOutcome,
    ProfileVerdict,
};
pub use identity::{ProjectRef, Subject, SubjectFamily};
pub use seed::AffectedSeed;

/// One comparison request: the selected built-in profiles and the typed
/// adapter contributions. Construction validates every term.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct DiffRequest {
    profiles: Vec<ProfileId>,
    adapters: Vec<AdapterInput>,
}

impl DiffRequest {
    /// An empty request: profile-independent change facts only.
    pub fn new() -> Self {
        Self::default()
    }

    /// Request one built-in profile; duplicates collapse.
    pub fn with_profile(mut self, profile: ProfileId) -> Self {
        if !self.profiles.contains(&profile) {
            self.profiles.push(profile);
        }
        self
    }

    /// Request the full built-in profile matrix.
    pub fn with_all_profiles(mut self) -> Self {
        for profile in [
            ProfileId::SourceConsumer,
            ProfileId::WireConsumer,
            ProfileId::StorageConsumer,
            ProfileId::TargetConsumer,
            ProfileId::Advisory,
        ] {
            self = self.with_profile(profile);
        }
        self
    }

    /// Attach one validated adapter contribution.
    pub fn with_adapter(mut self, adapter: AdapterInput) -> Self {
        self.adapters.push(adapter);
        self
    }

    /// The requested profiles in canonical order.
    pub fn profiles(&self) -> &[ProfileId] {
        &self.profiles
    }
}

/// One finished comparison: ordered facts plus per-profile decisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffResult {
    pub(crate) comparison_id: String,
    pub(crate) equal: bool,
    pub(crate) complete: bool,
    pub(crate) complete_reason: Option<&'static str>,
    pub(crate) base_ref: ProjectRef,
    pub(crate) candidate_ref: ProjectRef,
    pub(crate) classification: Vec<CompatibilityClass>,
    pub(crate) changes: Vec<ChangeRecord>,
    pub(crate) reasons: Vec<String>,
    pub(crate) seeds: Vec<AffectedSeed>,
    pub(crate) profiles: Vec<ProfileDecision>,
    pub(crate) adapters: Vec<adapter::RecordedAdapter>,
    pub(crate) migration_hints: Vec<MigrationHint>,
}

impl DiffResult {
    /// Whether the two projections are semantically identical.
    pub const fn equal(&self) -> bool {
        self.equal
    }

    /// Whether the comparison saw the complete input within every bound.
    pub const fn complete(&self) -> bool {
        self.complete
    }

    /// The recorded bound or invalidity, when not complete.
    pub const fn complete_reason(&self) -> Option<&'static str> {
        self.complete_reason
    }

    /// The stable comparison identity.
    pub fn comparison_id(&self) -> &str {
        &self.comparison_id
    }

    /// The base project reference.
    pub const fn base_ref(&self) -> &ProjectRef {
        &self.base_ref
    }

    /// The candidate project reference.
    pub const fn candidate_ref(&self) -> &ProjectRef {
        &self.candidate_ref
    }

    /// The profile-independent class union in dominance order.
    pub fn classification(&self) -> &[CompatibilityClass] {
        &self.classification
    }

    /// The ordered change records.
    pub fn changes(&self) -> &[ChangeRecord] {
        &self.changes
    }

    /// The ordered unique reason identifiers.
    pub fn reasons(&self) -> &[String] {
        &self.reasons
    }

    /// The ordered affected-symbol seeds.
    pub fn seeds(&self) -> &[AffectedSeed] {
        &self.seeds
    }

    /// The per-profile decisions in canonical profile order.
    pub fn profiles(&self) -> &[ProfileDecision] {
        &self.profiles
    }

    /// The recorded adapter contributions in canonical order.
    pub fn adapters(&self) -> &[adapter::RecordedAdapter] {
        &self.adapters
    }

    /// The ordered non-executable migration hints.
    pub fn migration_hints(&self) -> &[MigrationHint] {
        &self.migration_hints
    }

    /// The canonical result bytes (no trailing newline), or the typed
    /// export-limit rejection.
    pub fn to_canonical_json(&self) -> Result<String, DiagnosticSet> {
        canonical::result_bytes(self)
    }

    /// The stable human summary: one line, canonical facts only.
    pub fn to_human(&self) -> String {
        if self.equal {
            return format!(
                "diff: projects are semantically equal ({})",
                self.comparison_id
            );
        }
        let classes: Vec<String> = self
            .classification
            .iter()
            .map(|class| class.key().to_owned())
            .collect();
        format!(
            "diff: {} changes ({}) ({})",
            self.changes.len(),
            classes.join(", "),
            self.comparison_id
        )
    }
}

/// Compare two accepted, normalized compilations under one request.
///
/// Any fatal input violation rejects with a registered `diff.*` set and
/// no partial result.
/// Parse one comma-separated closed profile selector into the ordered
/// profile list; unknown or empty terms reject with the registered
/// `diff.profile-invalid` rule and a bounded echo.
pub fn parse_profile_terms(terms: &str) -> Result<Vec<ProfileId>, DiagnosticSet> {
    let mut profiles: Vec<ProfileId> = Vec::new();
    for term in terms.split(',') {
        let term = term.trim();
        let Some(profile) = ProfileId::from_key(term) else {
            return Err(diagnostic::profile_invalid_set(term));
        };
        if !profiles.contains(&profile) {
            profiles.push(profile);
        }
    }
    if profiles.is_empty() {
        return Err(diagnostic::profile_invalid_set(terms));
    }
    Ok(profiles)
}

/// Compare two accepted, normalized compilations under one request.
pub fn compare(
    base: &CompiledProject,
    candidate: &CompiledProject,
    request: &DiffRequest,
) -> Result<DiffResult, DiagnosticSet> {
    if base.model_version != candidate.model_version {
        return Err(diagnostic::family_mismatch_set(
            "model-version-mismatch",
            candidate.model_version.as_str(),
        ));
    }

    let base_projection = projection::project(base)
        .map_err(|bound| diagnostic::limit_set(diagnostic::SUBJECT_LIMIT, bound_name(bound)))?;
    let candidate_projection = projection::project(candidate)
        .map_err(|bound| diagnostic::limit_set(diagnostic::SUBJECT_LIMIT, bound_name(bound)))?;

    let base_ref = project_ref(base, &base_projection);
    let candidate_ref = project_ref(candidate, &candidate_projection);
    let comparison_id = format!(
        "sha256:{}",
        digest_hex(
            format!(
                "{}\u{1f}{}\u{1f}{}",
                version::IDENTITY,
                base_ref.semantic_digest(),
                candidate_ref.semantic_digest()
            )
            .as_bytes()
        )
    );

    let mut changes: Vec<ChangeRecord> = Vec::new();
    let mut consumed: BTreeSet<String> = BTreeSet::new();

    // Rename pre-pass: validated same-identity renames resolve first so
    // that mechanical reference updates on the base side follow the new
    // id instead of masquerading as independent type changes. Renamed
    // subjects never re-enter removal resolution.
    let mut base_projection = base_projection;
    let mut renamed: BTreeSet<String> = BTreeSet::new();
    for RenameResolution { from, to, hops } in
        detect_renames(base, candidate, &base_projection, &candidate_projection)
    {
        projection::substitute_references(&mut base_projection, &from, &to);
        consumed.insert(to.clone());
        renamed.insert(from.clone());
        let mut reasons = vec![reason::RENAME_HISTORY.to_owned()];
        if hops > 1 {
            reasons.push(reason::RENAME_HISTORY_MULTIHOP.to_owned());
        }
        let subject = Subject::new(family_of(base, &from), to);
        changes.push(ChangeRecord::assemble(
            ChangeKind::SymbolRenamed,
            Side::Base,
            subject,
            Summary::Renamed {
                renamed_from: vec![from],
            },
            Summary::None,
            reasons,
        ));
    }

    // Union walk in canonical subject order: removals first so that
    // replacement successors are marked consumed before additions walk.
    let mut subjects: BTreeSet<&Subject> = BTreeSet::new();
    subjects.extend(base_projection.subjects.keys());
    subjects.extend(candidate_projection.subjects.keys());
    let base_only: Vec<&Subject> = subjects
        .iter()
        .copied()
        .filter(|subject| {
            base_projection.subjects.contains_key(subject)
                && !candidate_projection.subjects.contains_key(subject)
        })
        .collect();
    for subject in base_only {
        if renamed.contains(subject.id()) {
            continue;
        }
        resolve_removal(
            base,
            candidate,
            subject,
            &candidate_projection,
            &mut consumed,
            &mut changes,
        );
    }
    for subject in &subjects {
        let base_node = base_projection.subjects.get(subject);
        let candidate_node = candidate_projection.subjects.get(subject);
        if let (Some(base_node), Some(candidate_node)) = (base_node, candidate_node) {
            diff_node(subject, base_node, candidate_node, &mut changes);
        }
    }

    // Candidate-only additions run last, after consumption marks.
    for subject in &subjects {
        if base_projection.subjects.contains_key(subject) {
            continue;
        }
        resolve_addition(base, candidate, subject, &consumed, &mut changes);
    }

    // Canonical record order: subject, kind, change identity.
    changes.sort_by(|left, right| {
        (
            left.subject().identity(),
            left.kind().key(),
            left.change_id(),
        )
            .cmp(&(
                right.subject().identity(),
                right.kind().key(),
                right.change_id(),
            ))
    });

    // Seeds: one per changed subject, change ids aggregated.
    let mut seed_map: BTreeMap<Subject, AffectedSeed> = BTreeMap::new();
    for record in &changes {
        let entry = seed_map
            .entry(record.subject().clone())
            .or_insert_with(|| AffectedSeed {
                subject: record.subject().clone(),
                side: record.side(),
                change_ids: Vec::new(),
            });
        entry.change_ids.push(record.change_id().to_owned());
    }
    if seed_map.len() >= version::MAX_SEEDS {
        return Err(diagnostic::limit_set(diagnostic::SEED_LIMIT, "seed-limit"));
    }
    let mut seeds: Vec<AffectedSeed> = seed_map.into_values().collect();
    seeds.sort_by(|left, right| {
        (
            left.subject().family().key(),
            left.subject().id(),
            left.subject().member(),
        )
            .cmp(&(
                right.subject().family().key(),
                right.subject().id(),
                right.subject().member(),
            ))
    });

    // Baseline classes and ordered unique reasons.
    let mut classification: Vec<CompatibilityClass> = Vec::new();
    let mut reasons: BTreeSet<String> = BTreeSet::new();
    for record in &changes {
        classification.extend(compatibility::baseline(record));
        reasons.extend(record.reasons().iter().cloned());
    }
    let classification = compatibility::ordered_classes(classification);
    let reasons: Vec<String> = reasons.into_iter().collect();

    // Profiles over the recorded facts, with validated adapter merges.
    let (profiles, recorded) = evaluate_profiles(request, &changes, &candidate_ref)?;

    // Non-executable migration hints derived from direct facts.
    let migration_hints = derive_hints(&changes);

    let equal = changes.is_empty();
    Ok(DiffResult {
        comparison_id,
        equal,
        complete: true,
        complete_reason: None,
        base_ref,
        candidate_ref,
        classification,
        changes,
        reasons,
        seeds,
        profiles,
        adapters: recorded,
        migration_hints,
    })
}

/// The bound name for a limit rejection.
fn bound_name(bound: usize) -> &'static str {
    if bound == version::MAX_COMPARE_SUBJECTS {
        "subject-limit"
    } else {
        "bound"
    }
}

/// Build the typed reference of one side.
fn project_ref(compiled: &CompiledProject, projection: &projection::Projection) -> ProjectRef {
    ProjectRef {
        model_version: compiled.model_version.as_str().to_owned(),
        ir_identity: crate::ir::IDENTITY.to_owned(),
        ir_digest: format!(
            "sha256:{}",
            digest_hex(compiled.to_canonical_json().as_bytes())
        ),
        semantic_digest: format!(
            "sha256:{}",
            digest_hex(projection::canonical_bytes(projection).as_bytes())
        ),
    }
}

/// SHA-256 of arbitrary bytes as lowercase hex.
pub(crate) fn digest_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// Compare two projected nodes of the same subject: common members, then
/// the typed payload. A family mismatch is impossible here (subjects key
/// includes the family), so kind changes only fire for the id-stable
/// definition kinds the Model allows to differ in declared kind — which
/// is none; the arm exists to keep the walk total.
fn diff_node(
    subject: &Subject,
    base_node: &projection::SubjectNode,
    candidate_node: &projection::SubjectNode,
    changes: &mut Vec<ChangeRecord>,
) {
    if base_node.version != candidate_node.version {
        changes.push(ChangeRecord::assemble(
            ChangeKind::SymbolVersionChanged,
            Side::Both,
            subject.clone(),
            Summary::Count(base_node.version),
            Summary::Count(candidate_node.version),
            vec![reason::kind_reason(ChangeKind::SymbolVersionChanged.key())],
        ));
    }
    if base_node.derived_from != candidate_node.derived_from {
        changes.push(ChangeRecord::assemble(
            ChangeKind::SymbolDerivedFromChanged,
            Side::Both,
            subject.clone(),
            Summary::Ids(base_node.derived_from.clone()),
            Summary::Ids(candidate_node.derived_from.clone()),
            vec![reason::kind_reason(
                ChangeKind::SymbolDerivedFromChanged.key(),
            )],
        ));
    }
    if base_node.visibility != candidate_node.visibility {
        changes.push(ChangeRecord::assemble(
            ChangeKind::SymbolVisibilityChanged,
            Side::Both,
            subject.clone(),
            Summary::Text(
                base_node
                    .visibility
                    .clone()
                    .unwrap_or_else(|| "none".to_owned()),
            ),
            Summary::Text(
                candidate_node
                    .visibility
                    .clone()
                    .unwrap_or_else(|| "none".to_owned()),
            ),
            vec![reason::kind_reason(
                ChangeKind::SymbolVisibilityChanged.key(),
            )],
        ));
    }
    if base_node.portability != candidate_node.portability {
        changes.push(ChangeRecord::assemble(
            ChangeKind::SymbolPortabilityChanged,
            Side::Both,
            subject.clone(),
            Summary::Text(
                base_node
                    .portability
                    .clone()
                    .unwrap_or_else(|| "none".to_owned()),
            ),
            Summary::Text(
                candidate_node
                    .portability
                    .clone()
                    .unwrap_or_else(|| "none".to_owned()),
            ),
            vec![reason::kind_reason(
                ChangeKind::SymbolPortabilityChanged.key(),
            )],
        ));
    }
    diff_payload(
        subject,
        &base_node.payload,
        &candidate_node.payload,
        changes,
    );
}

/// Compare two payloads and emit one record per observable difference.
#[allow(clippy::too_many_lines)]
fn diff_payload(
    subject: &Subject,
    base_payload: &projection::Payload,
    candidate_payload: &projection::Payload,
    changes: &mut Vec<ChangeRecord>,
) {
    use projection::Payload;
    match (base_payload, candidate_payload) {
        (Payload::Project, Payload::Project) => {}
        (
            Payload::Struct {
                fields: base_fields,
                identity: base_identity,
            },
            Payload::Struct {
                fields: candidate_fields,
                identity: candidate_identity,
            },
        ) => {
            diff_fields(subject, base_fields, candidate_fields, changes);
            if base_identity != candidate_identity {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::InvariantIdentityChanged,
                    Side::Both,
                    Subject::with_member(subject.family(), subject.id(), "identity"),
                    Summary::Ids(base_identity.clone()),
                    Summary::Ids(candidate_identity.clone()),
                    vec![reason::kind_reason(
                        ChangeKind::InvariantIdentityChanged.key(),
                    )],
                ));
            }
        }
        (
            Payload::Module {
                imports: base_imports,
            },
            Payload::Module {
                imports: candidate_imports,
            },
        ) => {
            if base_imports != candidate_imports {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::ModuleImportsChanged,
                    Side::Both,
                    Subject::with_member(subject.family(), subject.id(), "imports"),
                    Summary::Ids(base_imports.clone()),
                    Summary::Ids(candidate_imports.clone()),
                    vec![reason::kind_reason(ChangeKind::ModuleImportsChanged.key())],
                ));
            }
        }
        (
            Payload::Scalar { base: base_base },
            Payload::Scalar {
                base: candidate_base,
            },
        ) => {
            if base_base != candidate_base {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::TypeShapeChanged,
                    Side::Both,
                    Subject::with_member(subject.family(), subject.id(), "base"),
                    Summary::Text(base_base.clone()),
                    Summary::Text(candidate_base.clone()),
                    vec![reason::kind_reason(ChangeKind::TypeShapeChanged.key())],
                ));
            }
        }
        (
            Payload::Enum {
                values: base_values,
            },
            Payload::Enum {
                values: candidate_values,
            },
        ) => {
            for value in sorted_difference(base_values, candidate_values) {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::TypeMemberRemoved,
                    Side::Both,
                    Subject::with_member(subject.family(), subject.id(), format!("member.{value}")),
                    Summary::Text(value.clone()),
                    Summary::None,
                    vec![reason::kind_reason(ChangeKind::TypeMemberRemoved.key())],
                ));
            }
            for value in sorted_difference(candidate_values, base_values) {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::TypeMemberAdded,
                    Side::Both,
                    Subject::with_member(subject.family(), subject.id(), format!("member.{value}")),
                    Summary::None,
                    Summary::Text(value.clone()),
                    vec![reason::kind_reason(ChangeKind::TypeMemberAdded.key())],
                ));
            }
        }
        (
            Payload::Command {
                input: base_input,
                effects: base_effects,
            },
            Payload::Command {
                input: candidate_input,
                effects: candidate_effects,
            },
        ) => {
            diff_input(subject, base_input, candidate_input, changes);
            if base_effects != candidate_effects {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::SignatureEffectsChanged,
                    Side::Both,
                    Subject::with_member(subject.family(), subject.id(), "effects"),
                    Summary::Ids(base_effects.clone()),
                    Summary::Ids(candidate_effects.clone()),
                    vec![reason::kind_reason(
                        ChangeKind::SignatureEffectsChanged.key(),
                    )],
                ));
            }
        }
        (
            Payload::Query {
                reads: base_reads,
                returns: base_returns,
            },
            Payload::Query {
                reads: candidate_reads,
                returns: candidate_returns,
            },
        ) => {
            if base_reads != candidate_reads {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::SignatureReadsChanged,
                    Side::Both,
                    Subject::with_member(subject.family(), subject.id(), "reads"),
                    Summary::Ids(base_reads.clone()),
                    Summary::Ids(candidate_reads.clone()),
                    vec![reason::kind_reason(ChangeKind::SignatureReadsChanged.key())],
                ));
            }
            if base_returns != candidate_returns {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::SignatureOutputChanged,
                    Side::Both,
                    Subject::with_member(subject.family(), subject.id(), "returns"),
                    match base_returns {
                        Some(expression) => Summary::Text(expression.clone()),
                        None => Summary::None,
                    },
                    match candidate_returns {
                        Some(expression) => Summary::Text(expression.clone()),
                        None => Summary::None,
                    },
                    vec![reason::kind_reason(
                        ChangeKind::SignatureOutputChanged.key(),
                    )],
                ));
            }
        }
        (
            Payload::Policy {
                applies_to: base_applies,
                decision: base_decision,
            },
            Payload::Policy {
                applies_to: candidate_applies,
                decision: candidate_decision,
            },
        ) => {
            if base_applies != candidate_applies {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::PolicyScopeChanged,
                    Side::Both,
                    Subject::with_member(subject.family(), subject.id(), "applies-to"),
                    Summary::Ids(base_applies.clone()),
                    Summary::Ids(candidate_applies.clone()),
                    vec![reason::kind_reason(ChangeKind::PolicyScopeChanged.key())],
                ));
            }
            if base_decision != candidate_decision {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::PolicyDecisionChanged,
                    Side::Both,
                    Subject::with_member(subject.family(), subject.id(), "decision"),
                    Summary::Text(base_decision.clone()),
                    Summary::Text(candidate_decision.clone()),
                    vec![reason::kind_reason(ChangeKind::PolicyDecisionChanged.key())],
                ));
            }
        }
        (
            Payload::Event {
                payload: base_payload,
            },
            Payload::Event {
                payload: candidate_payload,
            },
        ) => {
            diff_fields(subject, base_payload, candidate_payload, changes);
        }
        (
            Payload::Effect {
                operation: base_operation,
                entity: base_entity,
                emits: base_emits,
            },
            Payload::Effect {
                operation: candidate_operation,
                entity: candidate_entity,
                emits: candidate_emits,
            },
        ) => {
            if base_operation != candidate_operation {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::EffectOperationChanged,
                    Side::Both,
                    Subject::with_member(subject.family(), subject.id(), "operation"),
                    Summary::Text(base_operation.clone()),
                    Summary::Text(candidate_operation.clone()),
                    vec![reason::kind_reason(
                        ChangeKind::EffectOperationChanged.key(),
                    )],
                ));
            }
            if base_entity != candidate_entity {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::EffectEntityChanged,
                    Side::Both,
                    Subject::with_member(subject.family(), subject.id(), "entity"),
                    Summary::Text(base_entity.clone()),
                    Summary::Text(candidate_entity.clone()),
                    vec![reason::kind_reason(ChangeKind::EffectEntityChanged.key())],
                ));
            }
            if base_emits != candidate_emits {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::EffectEmitsChanged,
                    Side::Both,
                    Subject::with_member(subject.family(), subject.id(), "emits"),
                    Summary::Ids(base_emits.clone()),
                    Summary::Ids(candidate_emits.clone()),
                    vec![reason::kind_reason(ChangeKind::EffectEmitsChanged.key())],
                ));
            }
        }
        (
            Payload::Endpoint {
                invokes: base_invokes,
                method: base_method,
                path: base_path,
            },
            Payload::Endpoint {
                invokes: candidate_invokes,
                method: candidate_method,
                path: candidate_path,
            },
        ) => {
            if base_method != candidate_method {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::EndpointMethodChanged,
                    Side::Both,
                    Subject::with_member(subject.family(), subject.id(), "method"),
                    Summary::Text(base_method.clone()),
                    Summary::Text(candidate_method.clone()),
                    vec![reason::kind_reason(ChangeKind::EndpointMethodChanged.key())],
                ));
            }
            if base_path != candidate_path {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::EndpointPathChanged,
                    Side::Both,
                    Subject::with_member(subject.family(), subject.id(), "path"),
                    Summary::Text(base_path.clone()),
                    Summary::Text(candidate_path.clone()),
                    vec![reason::kind_reason(ChangeKind::EndpointPathChanged.key())],
                ));
            }
            if base_invokes != candidate_invokes {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::EndpointInvokesChanged,
                    Side::Both,
                    Subject::with_member(subject.family(), subject.id(), "invokes"),
                    Summary::Text(base_invokes.clone()),
                    Summary::Text(candidate_invokes.clone()),
                    vec![reason::kind_reason(
                        ChangeKind::EndpointInvokesChanged.key(),
                    )],
                ));
            }
        }
        (
            Payload::Scenario {
                summary: base_summary,
                covers: base_covers,
            },
            Payload::Scenario {
                summary: candidate_summary,
                covers: candidate_covers,
            },
        ) => {
            if base_summary != candidate_summary {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::ScenarioSummaryChanged,
                    Side::Both,
                    Subject::with_member(subject.family(), subject.id(), "summary"),
                    Summary::Text(base_summary.clone()),
                    Summary::Text(candidate_summary.clone()),
                    vec![reason::kind_reason(
                        ChangeKind::ScenarioSummaryChanged.key(),
                    )],
                ));
            }
            if base_covers != candidate_covers {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::ScenarioCoversChanged,
                    Side::Both,
                    Subject::with_member(subject.family(), subject.id(), "covers"),
                    Summary::Ids(base_covers.clone()),
                    Summary::Ids(candidate_covers.clone()),
                    vec![reason::kind_reason(ChangeKind::ScenarioCoversChanged.key())],
                ));
            }
        }
        (
            Payload::TargetBinding {
                target: base_target,
            },
            Payload::TargetBinding {
                target: candidate_target,
            },
        ) => {
            if base_target != candidate_target {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::BindingTargetChanged,
                    Side::Both,
                    Subject::with_member(subject.family(), subject.id(), "target"),
                    Summary::Text(base_target.clone()),
                    Summary::Text(candidate_target.clone()),
                    vec![reason::kind_reason(ChangeKind::BindingTargetChanged.key())],
                ));
            }
        }
        _ => {
            // Same subject id, different payload families: the declared
            // kind changed under one stable id.
            changes.push(ChangeRecord::assemble(
                ChangeKind::SymbolKindChanged,
                Side::Both,
                subject.clone(),
                Summary::Text(payload_kind(base_payload).to_owned()),
                Summary::Text(payload_kind(candidate_payload).to_owned()),
                vec![reason::kind_reason(ChangeKind::SymbolKindChanged.key())],
            ));
        }
    }
}

/// The payload family tag used by kind-change records.
fn payload_kind(payload: &projection::Payload) -> &'static str {
    use projection::Payload;
    match payload {
        Payload::Project => "project",
        Payload::Module { .. } => "module",
        Payload::Scalar { .. } => "scalar",
        Payload::Enum { .. } => "enum",
        Payload::Struct { .. } => "struct",
        Payload::Command { .. } => "command",
        Payload::Query { .. } => "query",
        Payload::Policy { .. } => "policy",
        Payload::Event { .. } => "event",
        Payload::Effect { .. } => "effect",
        Payload::Endpoint { .. } => "endpoint",
        Payload::Scenario { .. } => "scenario",
        Payload::TargetBinding { .. } => "target-binding",
    }
}

/// Compare field arrays by name: additions, removals, type, nullability,
/// and presence records. Names are stable member keys; order is not.
fn diff_fields(
    subject: &Subject,
    base_fields: &[projection::FieldProjection],
    candidate_fields: &[projection::FieldProjection],
    changes: &mut Vec<ChangeRecord>,
) {
    let base_map: BTreeMap<&str, &projection::FieldProjection> = base_fields
        .iter()
        .map(|field| (field.name.as_str(), field))
        .collect();
    let candidate_map: BTreeMap<&str, &projection::FieldProjection> = candidate_fields
        .iter()
        .map(|field| (field.name.as_str(), field))
        .collect();
    for (name, base_field) in &base_map {
        match candidate_map.get(name) {
            None => {
                let member = field_member(subject, name);
                changes.push(ChangeRecord::assemble(
                    ChangeKind::FieldRemoved,
                    Side::Base,
                    member,
                    Summary::Text(base_field.field_type.clone()),
                    Summary::None,
                    vec![reason::kind_reason(ChangeKind::FieldRemoved.key())],
                ));
            }
            Some(candidate_field) => {
                let member = field_member(subject, name);
                if base_field.field_type != candidate_field.field_type {
                    let base_optional = is_optional(&base_field.field_type);
                    let candidate_optional = is_optional(&candidate_field.field_type);
                    let kind = if base_optional != candidate_optional {
                        ChangeKind::FieldNullabilityChanged
                    } else {
                        ChangeKind::FieldTypeChanged
                    };
                    let mut reasons = vec![reason::kind_reason(kind.key())];
                    if kind == ChangeKind::FieldNullabilityChanged {
                        if candidate_optional {
                            reasons.push("field.nullability-relaxed".to_owned());
                        } else {
                            reasons.push("field.nullability-tightened".to_owned());
                        }
                    }
                    changes.push(ChangeRecord::assemble(
                        kind,
                        Side::Both,
                        member,
                        Summary::Flag(base_optional),
                        Summary::Flag(candidate_optional),
                        reasons,
                    ));
                }
                if base_field.required != candidate_field.required {
                    changes.push(ChangeRecord::assemble(
                        ChangeKind::FieldPresenceChanged,
                        Side::Both,
                        field_member(subject, name),
                        Summary::Flag(base_field.required),
                        Summary::Flag(candidate_field.required),
                        vec![reason::kind_reason(ChangeKind::FieldPresenceChanged.key())],
                    ));
                }
            }
        }
    }
    for (name, candidate_field) in &candidate_map {
        if !base_map.contains_key(name) {
            changes.push(ChangeRecord::assemble(
                ChangeKind::FieldAdded,
                Side::Candidate,
                field_member(subject, name),
                Summary::None,
                Summary::Flag(candidate_field.required),
                vec![reason::kind_reason(ChangeKind::FieldAdded.key())],
            ));
        }
    }
}

/// Whether one canonical type expression is optional at the top level.
fn is_optional(expression: &str) -> bool {
    expression.starts_with("optional<")
}

/// The typed member key of one field within its owning subject.
fn field_member(subject: &Subject, name: &str) -> Subject {
    let prefix = match subject.family() {
        SubjectFamily::Command => "input.",
        SubjectFamily::Event => "payload.",
        _ => "field.",
    };
    Subject::with_member(subject.family(), subject.id(), format!("{prefix}{name}"))
}

/// Compare command input fields: input additions, removals, type,
/// nullability, and presence.
fn diff_input(
    subject: &Subject,
    base_input: &[projection::FieldProjection],
    candidate_input: &[projection::FieldProjection],
    changes: &mut Vec<ChangeRecord>,
) {
    let base_map: BTreeMap<&str, &projection::FieldProjection> = base_input
        .iter()
        .map(|field| (field.name.as_str(), field))
        .collect();
    let candidate_map: BTreeMap<&str, &projection::FieldProjection> = candidate_input
        .iter()
        .map(|field| (field.name.as_str(), field))
        .collect();
    for (name, base_field) in &base_map {
        match candidate_map.get(name) {
            None => {
                changes.push(ChangeRecord::assemble(
                    ChangeKind::SignatureInputRemoved,
                    Side::Base,
                    field_member(subject, name),
                    Summary::Text(base_field.field_type.clone()),
                    Summary::None,
                    vec![reason::kind_reason(ChangeKind::SignatureInputRemoved.key())],
                ));
            }
            Some(candidate_field) => {
                if base_field.field_type != candidate_field.field_type {
                    changes.push(ChangeRecord::assemble(
                        ChangeKind::SignatureInputTypeChanged,
                        Side::Both,
                        field_member(subject, name),
                        Summary::Text(base_field.field_type.clone()),
                        Summary::Text(candidate_field.field_type.clone()),
                        vec![reason::kind_reason(
                            ChangeKind::SignatureInputTypeChanged.key(),
                        )],
                    ));
                }
                if base_field.required != candidate_field.required {
                    changes.push(ChangeRecord::assemble(
                        ChangeKind::SignatureInputPresenceChanged,
                        Side::Both,
                        field_member(subject, name),
                        Summary::Flag(base_field.required),
                        Summary::Flag(candidate_field.required),
                        vec![reason::kind_reason(
                            ChangeKind::SignatureInputPresenceChanged.key(),
                        )],
                    ));
                }
            }
        }
    }
    for (name, candidate_field) in &candidate_map {
        if !base_map.contains_key(name) {
            changes.push(ChangeRecord::assemble(
                ChangeKind::SignatureInputAdded,
                Side::Candidate,
                field_member(subject, name),
                Summary::None,
                Summary::Flag(candidate_field.required),
                vec![reason::kind_reason(ChangeKind::SignatureInputAdded.key())],
            ));
        }
    }
}

/// One resolved rename: the base id, the candidate id, and the hop count
/// of the declared history walk (1 = direct edge).
struct RenameResolution {
    from: String,
    to: String,
    hops: usize,
}

/// Detect validated renames: exactly one live candidate claims the
/// base id through `renamed_from`, proven by a direct same-identity
/// edge or a bounded multi-hop walk through the declared chain.
/// Ambiguous, cyclic, dangling, or conflicting claims stay with the
/// removal resolver, which classifies them instead of guessing.
fn detect_renames(
    base: &CompiledProject,
    candidate: &CompiledProject,
    base_projection: &projection::Projection,
    candidate_projection: &projection::Projection,
) -> Vec<RenameResolution> {
    let registry = project_registry(candidate);
    let mut resolved = Vec::new();
    for subject in base_projection.subjects.keys() {
        if candidate_projection.subjects.contains_key(subject) {
            continue;
        }
        // Walk the declared chain from this id; the rename is valid when
        // the chain ends at a live candidate definition whose own
        // `renamed_from` names its direct predecessor, every edge is
        // same-identity with the definition version, and no cycle or
        // hop-bound stops the walk.
        let mut chain: Vec<String> = vec![subject.id().to_owned()];
        let mut valid = true;
        let mut visited: BTreeSet<String> = BTreeSet::new();
        visited.insert(subject.id().to_owned());
        while chain.len() <= change::HISTORY_HOPS {
            let Some(entry) = single_outgoing_edge(registry, chain.last().expect("nonempty"))
            else {
                break;
            };
            let node = candidate_projection
                .subjects
                .get(&Subject::new(subject.family(), entry.to.as_str()))
                .or_else(|| {
                    base_projection
                        .subjects
                        .get(&Subject::new(subject.family(), entry.to.as_str()))
                });
            let version_ok = match node {
                Some(node) => entry.definition_version == node.version,
                None => true,
            };
            if !entry.same_identity || !version_ok {
                valid = false;
                break;
            }
            if !visited.insert(entry.to.as_str().to_owned()) {
                valid = false;
                break;
            }
            chain.push(entry.to.as_str().to_owned());
            // A live candidate definition claiming its predecessor ends
            // the chain successfully.
            let last = chain.last().expect("nonempty");
            let predecessor = chain[chain.len() - 2].clone();
            if let Some(node) = candidate_projection
                .subjects
                .get(&Subject::new(subject.family(), last))
            {
                if node.renamed_from.iter().any(|claim| claim == &predecessor) {
                    resolved.push(RenameResolution {
                        from: subject.id().to_owned(),
                        to: last.clone(),
                        hops: chain.len() - 1,
                    });
                    valid = false; // handled; do not continue the chain
                    break;
                }
            }
        }
        let _ = valid;
    }
    let _ = base;
    resolved
}

/// The one outgoing rename-history edge of one id, when declared.
fn single_outgoing_edge<'a>(
    registry: Option<&'a IdRegistry>,
    from: &str,
) -> Option<&'a crate::ir::RenameHistoryEntry> {
    registry?
        .rename_history
        .iter()
        .find(|entry| entry.from.as_str() == from)
}

/// The subject family of one semantic id on the candidate side, or the
/// base side, falling back to `Project` when neither declares it.
fn family_of(base: &CompiledProject, id: &str) -> SubjectFamily {
    for definition in &base.definitions {
        if definition.id().as_str() == id {
            return SubjectFamily::of_kind(definition.kind());
        }
    }
    if base.project.as_ref().map(|project| project.id.as_str()) == Some(id) {
        return SubjectFamily::Project;
    }
    if base.modules.iter().any(|module| module.id.as_str() == id) {
        return SubjectFamily::Module;
    }
    SubjectFamily::Project
}

/// Resolve one base-only subject: rename, replacement, tombstoned
/// deletion, or plain removal — strictly from declared evidence.
#[allow(clippy::too_many_arguments)]
fn resolve_removal(
    _base: &CompiledProject,
    candidate: &CompiledProject,
    subject: &Subject,
    candidate_projection: &projection::Projection,
    consumed: &mut BTreeSet<String>,
    changes: &mut Vec<ChangeRecord>,
) {
    let id = subject.id();
    // 1. Semantic rename: a live candidate claims this id through
    //    `renamed_from`, proven by a matching same-identity history edge.
    let claimants: Vec<&Subject> = candidate_projection
        .subjects
        .iter()
        .filter(|(candidate_subject, node)| {
            candidate_subject.family() == subject.family()
                && node.renamed_from.iter().any(|claim| claim == id)
        })
        .map(|(candidate_subject, _)| candidate_subject)
        .collect();
    match claimants.len() {
        // No claim: fall through to replacement, tombstone, or plain
        // removal below.
        0 => {}
        // Exactly one claim. The pre-pass already consumed the valid
        // renames, so every remaining shape is a history conflict —
        // classified, never guessed into a rename.
        1 => {
            let target = claimants[0].id();
            let registry = project_registry(candidate);
            let node = candidate_projection
                .subjects
                .get(claimants[0])
                .expect("claimant projected");
            consumed.insert(target.to_owned());
            let mut reasons = vec![
                reason::kind_reason(ChangeKind::SymbolRemoved.key()),
                reason::HISTORY_CONFLICTING.to_owned(),
            ];
            match direct_edge(registry, id, target) {
                // An edge exists but denies the same identity or names a
                // different definition version.
                Some(entry) if !entry.same_identity || entry.definition_version != node.version => {
                    reasons.push(reason::HISTORY_VERSION_INVALID.to_owned());
                }
                _ => {}
            }
            if history_cycle(registry, id) {
                reasons.push(reason::HISTORY_CYCLIC.to_owned());
            }
            emit_removed_with_reasons(subject, reasons, changes);
            changes.push(ChangeRecord::assemble(
                ChangeKind::SymbolAdded,
                Side::Candidate,
                claimants[0].clone(),
                Summary::None,
                Summary::None,
                vec![
                    reason::kind_reason(ChangeKind::SymbolAdded.key()),
                    reason::HISTORY_CONFLICTING.to_owned(),
                ],
            ));
            return;
        }
        // Two or more live candidates claim the same predecessor:
        // ambiguous, never a guessed alias.
        _ => {
            emit_removed_with_reasons(
                subject,
                vec![
                    reason::kind_reason(ChangeKind::SymbolRemoved.key()),
                    reason::HISTORY_AMBIGUOUS.to_owned(),
                ],
                changes,
            );
            for claimant in claimants {
                consumed.insert(claimant.id().to_owned());
                changes.push(ChangeRecord::assemble(
                    ChangeKind::SymbolAdded,
                    Side::Candidate,
                    claimant.clone(),
                    Summary::None,
                    Summary::None,
                    vec![
                        reason::kind_reason(ChangeKind::SymbolAdded.key()),
                        reason::HISTORY_AMBIGUOUS.to_owned(),
                    ],
                ));
            }
            return;
        }
    }

    // 2. Replacement: a `replaced` tombstone names a live successor.
    let registry = project_registry(candidate);
    if let Some(replacement) = replacement_target(registry, id) {
        let successor = Subject::new(subject.family(), replacement.clone());
        if candidate_projection.subjects.contains_key(&successor) {
            consumed.insert(replacement.clone());
            changes.push(ChangeRecord::assemble(
                ChangeKind::SymbolReplaced,
                Side::Base,
                subject.clone(),
                Summary::None,
                Summary::Replaced {
                    replaced_by: replacement.clone(),
                },
                vec![reason::REPLACEMENT_TOMBSTONE.to_owned()],
            ));
            return;
        }
        // The named successor does not exist: dangling evidence.
        emit_removed_with_reasons(
            subject,
            vec![
                reason::kind_reason(ChangeKind::SymbolRemoved.key()),
                reason::HISTORY_DANGLING.to_owned(),
            ],
            changes,
        );
        return;
    }

    // 3. Tombstoned deletion without replacement.
    if let Some(tombstone) = deleted_tombstone(registry, id) {
        changes.push(ChangeRecord::assemble(
            ChangeKind::SymbolTombstoned,
            Side::Base,
            subject.clone(),
            Summary::None,
            Summary::Tombstoned { since: tombstone },
            vec![reason::kind_reason(ChangeKind::SymbolTombstoned.key())],
        ));
        return;
    }

    // 4. Plain removal.
    emit_removed_with_reasons(
        subject,
        vec![reason::kind_reason(ChangeKind::SymbolRemoved.key())],
        changes,
    );
}

/// The base-only removal record with extra history reasons.
fn emit_removed_with_reasons(
    subject: &Subject,
    reasons: Vec<String>,
    changes: &mut Vec<ChangeRecord>,
) {
    changes.push(ChangeRecord::assemble(
        ChangeKind::SymbolRemoved,
        Side::Base,
        subject.clone(),
        Summary::None,
        Summary::None,
        reasons,
    ));
}

/// Set difference of two sorted string arrays: items only in the first.
fn sorted_difference(left: &[String], right: &[String]) -> Vec<String> {
    left.iter()
        .filter(|value| !right.contains(value))
        .cloned()
        .collect()
}

/// The recorded candidate-only addition.
fn resolve_addition(
    base: &CompiledProject,
    _candidate: &CompiledProject,
    subject: &Subject,
    consumed: &BTreeSet<String>,
    changes: &mut Vec<ChangeRecord>,
) {
    if consumed.contains(subject.id()) {
        // Already covered by a rename or replacement record.
        return;
    }
    let mut reasons = vec![reason::kind_reason(ChangeKind::SymbolAdded.key())];
    let registry = project_registry(base);
    if tombstoned_id(registry, subject.id()) {
        reasons.push(reason::TOMBSTONE_REUSE.to_owned());
    }
    changes.push(ChangeRecord::assemble(
        ChangeKind::SymbolAdded,
        Side::Candidate,
        subject.clone(),
        Summary::None,
        Summary::None,
        reasons,
    ));
}

/// The project registry of one side, when declared.
fn project_registry(compiled: &CompiledProject) -> Option<&IdRegistry> {
    compiled
        .project
        .as_ref()
        .and_then(|project| project.id_registry.as_ref())
}

/// The direct same-identity history edge from `from` to `to`.
fn direct_edge<'a>(
    registry: Option<&'a IdRegistry>,
    from: &str,
    to: &str,
) -> Option<&'a crate::ir::RenameHistoryEntry> {
    let registry = registry?;
    registry
        .rename_history
        .iter()
        .find(|entry| entry.from.as_str() == from && entry.to.as_str() == to)
}
/// Whether the declared rename-history chain from one id revisits a
/// node within the hop bound: a cycle can never order renames.
fn history_cycle(registry: Option<&IdRegistry>, from: &str) -> bool {
    let Some(registry) = registry else {
        return false;
    };
    let mut visited: BTreeSet<&str> = BTreeSet::new();
    let mut current = from;
    for _ in 0..change::HISTORY_HOPS {
        if !visited.insert(current) {
            return true;
        }
        match registry
            .rename_history
            .iter()
            .find(|entry| entry.from.as_str() == current)
        {
            Some(entry) => current = entry.to.as_str(),
            None => return false,
        }
    }
    false
}

/// The replacement target of one `replaced` tombstone.
fn replacement_target(registry: Option<&IdRegistry>, id: &str) -> Option<String> {
    let registry = registry?;
    registry
        .tombstones
        .iter()
        .find_map(|tombstone| match tombstone {
            Tombstone::Replaced {
                id: retired,
                replaced_by,
                ..
            } if retired.as_str() == id => Some(replaced_by.as_str().to_owned()),
            _ => None,
        })
}

/// The `since` value of one `deleted` tombstone.
fn deleted_tombstone(registry: Option<&IdRegistry>, id: &str) -> Option<u64> {
    let registry = registry?;
    registry
        .tombstones
        .iter()
        .find_map(|tombstone| match tombstone {
            Tombstone::Deleted { id: retired, since } if retired.as_str() == id => Some(*since),
            _ => None,
        })
}

/// Whether any tombstone retires one id (either shape).
fn tombstoned_id(registry: Option<&IdRegistry>, id: &str) -> bool {
    let Some(registry) = registry else {
        return false;
    };
    registry.tombstones.iter().any(|tombstone| match tombstone {
        Tombstone::Replaced { id: retired, .. } | Tombstone::Deleted { id: retired, .. } => {
            retired.as_str() == id
        }
    })
}

/// Evaluate the requested profiles over the recorded facts and record
/// every adapter contribution with its validated state.
fn evaluate_profiles(
    request: &DiffRequest,
    changes: &[ChangeRecord],
    candidate_ref: &ProjectRef,
) -> Result<(Vec<ProfileDecision>, Vec<adapter::RecordedAdapter>), DiagnosticSet> {
    if request.profiles.len() > version::MAX_PROFILE_TERMS {
        return Err(diagnostic::profile_invalid_set("over-profile-bound"));
    }
    let mut profile_ids: Vec<ProfileId> = request.profiles.clone();
    profile_ids.sort_by_key(|profile| profile.key());

    // Adapter validation and dedup first: contributions feed decisions.
    let mut recorded: Vec<adapter::RecordedAdapter> = Vec::new();
    for input in &request.adapters {
        if !adapter::can_accept(recorded.len()) {
            return Err(diagnostic::adapter_invalid_set("over-adapter-bound"));
        }
        if input.profile_ref() != ProfileId::Advisory && !profile_ids.contains(&input.profile_ref())
        {
            return Err(diagnostic::adapter_invalid_set("profile-not-requested"));
        }
        let mut reasons: Vec<String> = Vec::new();
        let mut trust = input.trust();
        if input.input_ir_digest() != candidate_ref.ir_digest() {
            trust = AdapterTrust::Stale;
            reasons.push(reason::ADAPTER_STALE.to_owned());
        }
        match trust {
            AdapterTrust::Verified => {}
            AdapterTrust::Stale => reasons.push(reason::ADAPTER_STALE.to_owned()),
            AdapterTrust::Conflicting => reasons.push(reason::ADAPTER_CONFLICTING.to_owned()),
            AdapterTrust::Unsupported => reasons.push(reason::ADAPTER_UNSUPPORTED.to_owned()),
            AdapterTrust::Unknown | AdapterTrust::Rejected => {
                reasons.push(reason::ADAPTER_UNKNOWN.to_owned())
            }
        }
        // Conflicting duplicates: same identity, different content.
        let duplicate = recorded
            .iter()
            .any(|existing| existing.conflicts_with(input));
        if duplicate {
            reasons.push(reason::ADAPTER_CONFLICTING.to_owned());
        }
        let change_ids: BTreeSet<&str> = changes.iter().map(|record| record.change_id()).collect();
        let effects: Vec<adapter::RecordedEffect> = input
            .effects()
            .iter()
            .filter(|effect| {
                let known = change_ids.contains(effect.change_id());
                if !known && trust.merges() {
                    return false;
                }
                known
            })
            .map(|effect| adapter::RecordedEffect {
                change_id: effect.change_id().to_owned(),
                classes: effect.classes().to_vec(),
            })
            .collect();
        if effects.len() != input.effects().len() {
            reasons.push(reason::ADAPTER_UNKNOWN.to_owned());
        }
        recorded.push(adapter::RecordedAdapter::from_input(
            input, effects, reasons, duplicate, trust,
        ));
    }

    // Exact duplicates collapse; only the first survives.
    let mut deduped: Vec<adapter::RecordedAdapter> = Vec::new();
    for entry in recorded {
        let is_exact_duplicate = deduped.contains(&entry);
        if !is_exact_duplicate {
            deduped.push(entry);
        }
    }

    let mut decisions: Vec<ProfileDecision> = Vec::new();
    for profile in profile_ids {
        decisions.push(evaluate_profile(profile, changes, &deduped));
    }
    Ok((decisions, deduped))
}

/// Evaluate one built-in profile over the change facts.
fn evaluate_profile(
    profile: ProfileId,
    changes: &[ChangeRecord],
    adapters: &[adapter::RecordedAdapter],
) -> ProfileDecision {
    let policy_digest = format!(
        "sha256:{}",
        digest_hex(canonical::policy_bytes(profile).as_bytes())
    );
    let applicable: Vec<&adapter::RecordedAdapter> = adapters
        .iter()
        .filter(|entry| entry.profile_ref == profile && entry.trust.merges())
        .collect();
    let degraded_adapters = adapters
        .iter()
        .any(|entry| entry.profile_ref == profile && !entry.trust.merges());

    let mut outcomes: Vec<ProfileOutcome> = Vec::new();
    for record in changes {
        let base = compatibility::baseline(record);
        let (mut classes, mut reasons) = compatibility::evaluate_dimension(profile, record, &base);
        // Verified contributions append classes; they never remove any.
        for entry in &applicable {
            for effect in &entry.effects {
                if effect.change_id == record.change_id() {
                    classes.extend(effect.classes.iter().copied());
                }
            }
        }
        classes = compatibility::ordered_classes(classes);
        if classes.is_empty() {
            classes.push(CompatibilityClass::Additive);
        }
        // Every outcome names at least the change's own kind reason: a
        // direct fact, never a locale string or inferred cause.
        reasons.push(reason::kind_reason(record.kind().key()));
        reasons.sort();
        reasons.dedup();
        outcomes.push(ProfileOutcome {
            change_id: record.change_id().to_owned(),
            classes,
            reasons,
        });
    }
    outcomes.sort_by(|left, right| left.change_id.cmp(&right.change_id));

    let mut union: Vec<CompatibilityClass> = Vec::new();
    for outcome in &outcomes {
        union.extend(outcome.classes.iter().copied());
    }
    let classes = compatibility::ordered_classes(union);

    // Strict profiles block on absent evidence and degraded adapters.
    let blocked_on = if profile.is_strict() {
        let evidence_absent = matches!(
            profile.dimension(),
            Some(compatibility::Dimension::Storage) | Some(compatibility::Dimension::Target)
        );
        let blocked: Vec<String> = outcomes
            .iter()
            .filter(|outcome| outcome.classes.contains(&CompatibilityClass::Unknown))
            .map(|outcome| outcome.change_id.clone())
            .collect();
        let reason = match profile.dimension() {
            Some(compatibility::Dimension::Storage) => {
                Some(reason::STORAGE_EVIDENCE_ABSENT.to_owned())
            }
            Some(compatibility::Dimension::Target) => {
                Some(reason::TARGET_EVIDENCE_ABSENT.to_owned())
            }
            _ => None,
        };
        if !blocked.is_empty() && evidence_absent {
            Some(BlockedOn {
                reason: reason.expect("dimension reason"),
                change_ids: blocked,
            })
        } else if degraded_adapters {
            Some(BlockedOn {
                reason: reason::ADAPTER_STALE.to_owned(),
                change_ids: changes
                    .iter()
                    .map(|record| record.change_id().to_owned())
                    .collect(),
            })
        } else {
            None
        }
    } else {
        None
    };

    let verdict = if blocked_on.is_some() {
        ProfileVerdict::Blocked
    } else if classes.iter().any(|class| class.is_breaking()) {
        ProfileVerdict::Breaking
    } else if profile == ProfileId::Advisory && degraded_adapters {
        ProfileVerdict::Degraded
    } else {
        ProfileVerdict::Compatible
    };

    ProfileDecision {
        profile_id: profile,
        policy_digest,
        verdict,
        classes,
        outcomes,
        blocked_on,
    }
}

/// Derive non-executable migration hints from direct change facts.
fn derive_hints(changes: &[ChangeRecord]) -> Vec<MigrationHint> {
    let mut hints: Vec<MigrationHint> = Vec::new();
    for record in changes {
        let hint = match record.kind() {
            ChangeKind::SymbolReplaced => match record.after() {
                Summary::Replaced { replaced_by } => Some((
                    compatibility::hints::REPLACEMENT_AVAILABLE,
                    replaced_by.clone(),
                )),
                _ => None,
            },
            ChangeKind::SymbolTombstoned
            | ChangeKind::SymbolRemoved
            | ChangeKind::FieldRemoved
            | ChangeKind::SignatureInputRemoved
            | ChangeKind::TypeMemberRemoved
            | ChangeKind::FieldTypeChanged
            | ChangeKind::TypeShapeChanged
            | ChangeKind::InvariantIdentityChanged => Some((
                compatibility::hints::STORAGE_REVIEW,
                record.subject().id().to_owned(),
            )),
            ChangeKind::EndpointMethodChanged | ChangeKind::EndpointPathChanged => Some((
                compatibility::hints::WIRE_REVIEW,
                record.subject().id().to_owned(),
            )),
            _ => None,
        };
        if let Some((vocabulary, detail)) = hint {
            hints.push(MigrationHint {
                change_id: record.change_id().to_owned(),
                hint: vocabulary,
                detail: diagnostic::bounded(&detail),
            });
        }
        if hints.len() >= version::MAX_CHANGES {
            break;
        }
    }
    hints.sort();
    hints
}
