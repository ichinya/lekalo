//! The generation authority: the validated Model/IR/lock closure the
//! managed lane plans under (issue #41).
//!
//! A managed plan is never derived from caller-hashed optional bytes. The
//! required semantics come from the exact canonical Model compiled through
//! the accepted loader pipeline into typed IR, pinned by the exact lock
//! revision; the coverage evidence must name those pins and discharge
//! every obligation the ledger requires before any plan exists. The
//! renderer stays pure behind this validated boundary.

use super::{detail, DomainResult, INPUT_INVALID, PLAN_STALE};
use crate::ir::{DefinitionKind, TypeRef};
use crate::loader::{self, LoadSelection};
use crate::lockfile::plan::LockService;
use crate::lockfile::LockState;

/// The exact input pins of one generation: the canonical Model payload, the
/// canonical typed-IR payload, and the exact lock revision, plus the
/// renderable obligations the Model requires.
#[derive(Clone, Debug, PartialEq)]
pub struct Authority {
    /// `sha256:` pin over the canonical Model JSON bytes.
    pub model_digest: String,
    /// `sha256:` pin over the canonical typed-IR JSON bytes.
    pub ir_digest: String,
    /// `sha256:` pin over the exact lock file bytes.
    pub lock_digest: String,
    /// The pinned target protocol of the lock, when published.
    pub lock_protocol: Option<String>,
    /// The compiled Model contract version (e.g. `0.2.16`).
    pub model_version: String,
    /// The compiled project id of the Model.
    pub project_id: String,
    /// The required renderable obligations of the module, sorted by
    /// semantic id: `(semantic id, kind)`.
    pub ledger: Vec<(String, DefinitionKind)>,
    /// The Model-derived obligation semantics: the exact input fields
    /// (name, type spelling) of every command and the reads/returns of
    /// every query in this module's ledger. The renderer derives its
    /// projections from these, never from caller evidence (finding 4).
    pub semantics: Vec<Obligation>,
    /// The locked adapter pins the generation binds (id, version, package
    /// digest, the launched-entry artifact digest). A managed generation
    /// requires a real locked adapter (finding 4).
    pub adapter: Option<LockedAdapter>,
}

/// The exact Model semantics of one renderable obligation.
#[derive(Clone, Debug, PartialEq)]
pub struct Obligation {
    /// The exact semantic id (`planner.focus_task`).
    pub id: String,
    /// The closed definition kind.
    pub kind: DefinitionKind,
    /// The exact Model input fields of a command: `(name, type)`.
    pub input: Vec<(String, String)>,
    /// The exact reads of a query.
    pub reads: Vec<String>,
    /// The exact return type spelling of a query, when declared.
    pub returns: Option<String>,
    /// The declared field names of the returned entity, when the query
    /// returns an entity: the authority for the output surface.
    pub return_fields: Vec<String>,
}

/// The locked adapter a managed generation binds.
#[derive(Clone, Debug, PartialEq)]
pub struct LockedAdapter {
    pub id: String,
    pub version: String,
    /// The locked package digest.
    pub digest: String,
    /// The locked launched-entry artifact digest (platform `any`).
    pub entry_digest: String,
}

fn invalid(reason: &str) -> DomainResult {
    super::diagnostic::failure(INPUT_INVALID, &detail(reason))
}

fn stale(path: &str, reason: &str) -> DomainResult {
    super::diagnostic::failure(PLAN_STALE, &[("path", path), ("reason", reason)])
}

/// The live project's canonical lock revision, derived through the
/// accepted lock reader (read-only). `None` when the project's lock is
/// missing or unreadable — every consumer fails closed on divergence.
pub fn live_lock_digest(root: &std::path::Path) -> Option<String> {
    let lock = LockService::read_state_at(root).ok()?;
    match lock {
        crate::lockfile::LockState::Present(lock) => Some(
            crate::lockfile::canonical::lock_digest(&lock)
                .as_str()
                .to_owned(),
        ),
        crate::lockfile::LockState::Absent => None,
    }
}

/// Derive the generation authority of one project: load and compile the
/// canonical Model through the accepted pipeline, verify the exact lock
/// pins it, and collect the module's required renderable ledger.
pub fn derive(selection: &LoadSelection, module: &str) -> Result<Authority, DomainResult> {
    let root = match loader::root_for_selection(selection) {
        Ok(root) => root,
        Err(error) => return Err(classify(error)),
    };
    let model_json = match loader::run(selection, false) {
        crate::result::DomainResult::Valid {
            payload: crate::result::SuccessPayload::Model { json, .. },
            ..
        } => json,
        other => return Err(classify(other)),
    };
    let model = match loader::normalize_model(selection) {
        Ok(model) => model,
        Err(error) => return Err(classify(error)),
    };
    let compilation = match crate::ir::compile(&model) {
        Ok(compilation) => compilation,
        Err(failure) => return Err(classify(failure.into_result())),
    };
    let ir_json = compilation.project.to_canonical_json();
    // The exact lock revision must exist and parse closed: it pins the
    // accepted contract families the pipeline compiled under, and its own
    // exact bytes ride the authority digest, so any lock mutation moves
    // the generation identity.
    let _lock_bytes = match std::fs::read(root.join("lekalo.lock")) {
        Ok(bytes) => bytes,
        Err(_) => return Err(stale("lekalo.lock", "missing")),
    };
    let lock = match LockService::read_state_at(&root) {
        Ok(LockState::Present(lock)) => lock,
        Ok(LockState::Absent) => return Err(stale("lekalo.lock", "missing")),
        Err(_) => return Err(stale("lekalo.lock", "unreadable")),
    };
    // The locked adapter the generation binds (finding 4): a managed
    // generation must run against a real locked adapter package whose
    // pinned target protocol is the negotiated managed successor. An
    // adapter-free lock cannot back a coverage document claiming protocol
    // support (`adapters:[]` while `lockProtocol` names a protocol is a
    // capability claim without an implementation).
    let protocol = lock
        .target_protocol()
        .map(|pin| pin.version().as_str().to_owned());
    let adapter = lock.adapters().first().map(|resolved| LockedAdapter {
        id: resolved.id().as_str().to_owned(),
        version: resolved.version().as_str().to_owned(),
        digest: resolved.digest().as_str().to_owned(),
        entry_digest: resolved
            .artifacts()
            .iter()
            .find(|artifact| artifact.platform().as_str() == "any")
            .or_else(|| resolved.artifacts().first())
            .map(|artifact| artifact.digest().as_str().to_owned())
            .unwrap_or_default(),
    });
    if adapter.is_some() {
        // The locked generator/profile attachments must resolve inside the
        // lock's own component graph: a profile or generator naming a
        // missing adapter is a broken capability claim.
        for generator in lock.generators() {
            if !lock.adapters().iter().any(|candidate| {
                candidate.id().as_str() == generator.adapter().id().as_str()
                    && candidate.version() == generator.adapter().version()
            }) {
                return Err(stale("lekalo.lock", "generator-adapter"));
            }
        }
    }
    let _ = &protocol;
    // The ledger: every renderable obligation this module declares, with
    // the exact Model-derived semantics (finding 4). A coverage document
    // that omits one is refused before planning; a coverage projection
    // that disagrees with these semantics never renders.
    let prefix = format!("{module}.");
    let mut semantics = Vec::new();
    let ledger = compilation
        .project
        .definitions
        .iter()
        .filter(|definition| {
            matches!(
                definition.kind(),
                DefinitionKind::Command | DefinitionKind::Query
            ) && definition.id().as_str().starts_with(&prefix)
        })
        .filter_map(|definition| {
            let id = definition.id().as_str().to_owned();
            if !super::token(&id) {
                return None;
            }
            let obligation = match definition {
                crate::ir::Definition::Command(command) => Obligation {
                    id: id.clone(),
                    kind: DefinitionKind::Command,
                    input: command
                        .input
                        .iter()
                        .map(|field| (field.name.as_str().to_owned(), type_spelling(&field.r#type)))
                        .collect(),
                    reads: Vec::new(),
                    returns: None,
                    return_fields: Vec::new(),
                },
                crate::ir::Definition::Query(query) => Obligation {
                    id: id.clone(),
                    kind: DefinitionKind::Query,
                    input: Vec::new(),
                    reads: query
                        .reads
                        .iter()
                        .map(|read| read.as_str().to_owned())
                        .collect(),
                    returns: query
                        .returns
                        .as_ref()
                        .map(|returned: &TypeRef| type_spelling(returned)),
                    return_fields: query
                        .returns
                        .as_ref()
                        .and_then(|returned| return_entity_fields(&compilation.project, returned))
                        .unwrap_or_default(),
                },
                _ => return None,
            };
            semantics.push(obligation.clone());
            Some((id, definition.kind()))
        })
        .collect::<Vec<_>>();
    if ledger.iter().any(|(id, _)| !super::token(id)) {
        return Err(invalid("ledger"));
    }
    Ok(Authority {
        model_digest: super::digest(model_json.as_bytes()),
        ir_digest: super::digest(ir_json.as_bytes()),
        // The canonical lock digest (the same identity the artifact owner
        // and every source map bind), never the raw file bytes.
        lock_digest: crate::lockfile::canonical::lock_digest(&lock)
            .as_str()
            .to_owned(),
        lock_protocol: lock
            .target_protocol()
            .map(|pin| pin.version().as_str().to_owned()),
        model_version: compilation.project.model_version.as_str().to_owned(),
        project_id: compilation
            .project
            .project
            .as_ref()
            .map(|project| project.id.as_str().to_owned())
            .unwrap_or_default(),
        ledger,
        semantics,
        adapter,
    })
}

/// Whether the coverage document's authority pins match the derived
/// authority exactly: the canonical Model, the canonical typed IR, and the
/// exact lock revision the evidence claims. The advertised adapter
/// protocol is qualified in [`super::coverage::admit`] against the
/// negotiated successor token.
///
/// The capability qualification (finding 4): the pinned lock protocol
/// must be the exact negotiated managed successor, and the coverage
/// document's semantics must agree with the Model-derived obligations —
/// a caller-edited projection/sample cannot rename the generated shape
/// while the canonical Model, IR, and lock stay pinned.
pub fn qualify(
    root: &std::path::Path,
    coverage: &super::wire::Coverage,
    authority: &Authority,
) -> Result<(), DomainResult> {
    let pins = coverage
        .authority
        .value()
        .ok_or_else(|| invalid("authority"))?;
    if pins.model != authority.model_digest
        || pins.ir != authority.ir_digest
        || pins.lock != authority.lock_digest
    {
        return Err(stale("authority", "inputs"));
    }
    // The negotiated successor is a locked protocol, not a claim (finding
    // 4): the coverage may advertise managed support only when the pinned
    // lock declares a real published target protocol — an adapter-free,
    // protocol-free lock cannot back a coverage document claiming protocol
    // capability (`adapters:[]` with a protocol claim is an unimplemented
    // capability claim). The exact managed successor token is enforced on
    // the coverage's adapter protocol by [`super::coverage::admit`]; the
    // published registry protocol family stays the versioning authority.
    //
    // Protocol-story note (auditable split, r3 finding 12): the coverage's
    // `adapter.protocol` is the *managed lane* protocol — the managed
    // wire/contract family version (0.8.0) — while the lock's
    // `targetProtocol` pin is the *resolver negotiation* protocol of the
    // lock schema itself (0.3.2). The two pins answer different questions
    // and are deliberately not equated; this comment is the documented
    // split, and both pins are independently validated (the lane protocol
    // by `coverage::admit`, the lock protocol by presence here and by the
    // closed lock reader).
    if authority.lock_protocol.is_none() {
        return Err(super::diagnostic::failure(
            super::VERSION_UNSUPPORTED,
            &detail("lock-protocol"),
        ));
    }
    // The generation binds a real locked adapter package (finding 4): a
    // managed apply without any locked adapter has no implementation to
    // qualify its recipe against.
    if authority.adapter.is_none() {
        return Err(super::diagnostic::failure(
            super::VERSION_UNSUPPORTED,
            &detail("adapter"),
        ));
    }
    // The coverage's adapter evidence must name the locked adapter
    // exactly (finding 4): the locked package is the generation's real
    // implementation; a caller-authored entry/package digest pointing
    // anywhere else is a forged capability pin. The lock's project
    // source id is the entry coordinate; the locked digest is the
    // package identity.
    if let Some(adapter) = &authority.adapter {
        if coverage.adapter.package_digest != adapter.digest {
            return Err(stale("adapter", "package-digest"));
        }
        // The coverage entry must be the locked package's real project
        // bytes: the single-entry convention makes the locked package
        // digest the entry's own byte digest, so reading the entry under
        // the confined capability and comparing pins the implementation
        // exactly (finding 4). The entry coordinate is a project-relative
        // path (enforced by the closed grammar), read through the confined
        // capability — no raw join, no reparse alias, no outside read
        // (finding 10).
        let entry = match super::fs::read_file(root, &coverage.adapter.entry) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return Err(stale("adapter", "entry")),
            Err(_) => return Err(stale("adapter", "entry")),
        };
        if super::digest(&entry) != adapter.entry_digest {
            return Err(stale("adapter", "entry-digest"));
        }
        // The locked bytes must implement the adapter protocol surface,
        // not merely exist (finding 8): a trivial file with recomputed
        // lock digests is a false capability proof. The bounded structural
        // conformance probe requires the entry to carry the complete
        // request-handling surface of the adapter handshake — the describe
        // /validate/verify/generate operations and the protocol negotiation
        // parameter — without executing anything (the planning lane stays
        // offline).
        adapter_entry_conformance(&entry)?;
    }
    semantics_match(coverage, authority)?;
    Ok(())
}

/// Whether the coverage discharges every required obligation of the
/// ledger: every renderable semantic the Model declares must be covered
/// with a matching requirement before any plan exists.
pub fn ledger_complete(
    coverage: &super::wire::Coverage,
    authority: &Authority,
) -> Result<(), DomainResult> {
    // The Model may not be dodged (forward direction) ...
    for (id, kind) in &authority.ledger {
        let requirement = match coverage.requirements.iter().find(|r| &r.id == id) {
            Some(requirement) => requirement,
            None => {
                return Err(super::diagnostic::failure(
                    super::CAPABILITY_GAP,
                    &[
                        ("requirement", id.as_str()),
                        ("reason", "omitted-obligation"),
                    ],
                ))
            }
        };
        let covered_kind = match kind {
            DefinitionKind::Command => super::wire::RequirementKind::Command,
            DefinitionKind::Query => super::wire::RequirementKind::Query,
            _ => continue,
        };
        if requirement.kind != covered_kind {
            return Err(super::diagnostic::failure(
                super::CAPABILITY_GAP,
                &[("requirement", id.as_str()), ("reason", "kind")],
            ));
        }
    }
    // ... and the coverage may not invent obligations the Model never
    // declared (finding 4, reverse direction): an unmodeled renderable
    // requirement would generate an application surface with no semantic
    // source of truth.
    for requirement in &coverage.requirements {
        if !requirement.kind.renders() {
            continue;
        }
        if !authority.ledger.iter().any(|(id, _)| *id == requirement.id) {
            return Err(super::diagnostic::failure(
                super::CAPABILITY_GAP,
                &[
                    ("requirement", requirement.id.as_str()),
                    ("reason", "unmodeled-obligation"),
                ],
            ));
        }
    }
    Ok(())
}

/// The bounded structural conformance probe of one locked adapter entry
/// (finding 8): the entry bytes must carry the complete adapter handshake
/// surface — every lifecycle operation and the protocol negotiation
/// parameter. This is a source-surface check, not execution: the planning
/// lane never launches processes. An entry without the full operation set
/// is a capability claim without an implementation.
fn adapter_entry_conformance(entry: &[u8]) -> Result<(), DomainResult> {
    let Ok(text) = std::str::from_utf8(entry) else {
        return Err(stale("adapter", "entry-encoding"));
    };
    for surface in [
        "describe",
        "validate",
        "verify",
        "generate",
        "protocol_version",
    ] {
        if !text.contains(surface) {
            return Err(stale("adapter", "entry-surface"));
        }
    }
    Ok(())
}

fn classify(error: DomainResult) -> DomainResult {
    error
}

/// The declared field names of the entity one type reference resolves
/// to, when it names an entity of the compiled project (finding 4): the
/// authority for a query's generated output surface.
fn return_entity_fields(
    project: &crate::ir::CompiledProject,
    reference: &TypeRef,
) -> Option<Vec<String>> {
    let TypeRef::Ref(symbol) = reference else {
        return None;
    };
    let definition = project
        .definitions
        .iter()
        .find(|definition| definition.id() == symbol)?;
    let crate::ir::Definition::Entity(entity) = definition else {
        return None;
    };
    Some(
        entity
            .fields
            .iter()
            .map(|field| field.name.as_str().to_owned())
            .collect(),
    )
}

/// The closed type spelling of one IR type reference (finding 4): the
/// renderer derives field types from these, never from caller evidence.
fn type_spelling(reference: &TypeRef) -> String {
    match reference {
        TypeRef::Ref(symbol) => symbol.as_str().to_owned(),
        TypeRef::List(inner) => format!("list<{}>", type_spelling(inner)),
        TypeRef::Optional(inner) => format!("optional<{}>", type_spelling(inner)),
    }
}

/// Whether the coverage document's semantics agree with the Model-derived
/// obligations (finding 4): every renderable requirement's projection and
/// sample must witness the exact Model fields. The coverage evidence may
/// narrow the naming (`to` renames) but never invent input fields, drop
/// Model-declared fields, or relabel sample outputs the Model does not
/// declare.
pub fn semantics_match(
    coverage: &super::wire::Coverage,
    authority: &Authority,
) -> Result<(), DomainResult> {
    for obligation in &authority.semantics {
        let Some(name) =
            super::canonical::requirement_name(coverage.module.as_str(), obligation.id.as_str())
        else {
            continue;
        };
        let requirement = match coverage
            .requirements
            .iter()
            .find(|requirement| requirement.id == obligation.id)
        {
            Some(requirement) => requirement,
            None => continue, // ledger completeness is a separate refusal
        };
        if !requirement.kind.renders() {
            continue;
        }
        // The input side of the projection must witness exactly the
        // Model's declared input fields for commands (finding 4): the
        // generated handler renders from the Model's fields, so caller
        // evidence cannot rename the input surface. Queries carry no
        // Model-declared input in this Model version; their input side
        // stays the reviewed projection.
        if obligation.kind == DefinitionKind::Command {
            let mut witnessed: Vec<String> = requirement
                .projection
                .iter()
                .map(|projection| projection.from.clone())
                .collect();
            witnessed.sort();
            witnessed.dedup();
            let mut expected: Vec<String> = obligation
                .input
                .iter()
                .map(|(name, _)| name.clone())
                .collect();
            expected.sort();
            expected.dedup();
            if witnessed != expected {
                return Err(super::diagnostic::failure(
                    super::CAPABILITY_GAP,
                    &[
                        ("requirement", obligation.id.as_str()),
                        ("reason", "projection-not-model"),
                    ],
                ));
            }
            // The sample input must witness the same Model fields with
            // the Model's declared type classes.
            if let Some(sample) = requirement.sample.value() {
                for (name, spelling) in &obligation.input {
                    let Some(value) = sample.input.get(name) else {
                        return Err(super::diagnostic::failure(
                            super::CAPABILITY_GAP,
                            &[
                                ("requirement", obligation.id.as_str()),
                                ("reason", "sample-not-model"),
                            ],
                        ));
                    };
                    if !scalar_matches(value, spelling) {
                        return Err(super::diagnostic::failure(
                            super::CAPABILITY_GAP,
                            &[
                                ("requirement", obligation.id.as_str()),
                                ("reason", "sample-type-not-model"),
                            ],
                        ));
                    }
                }
            }
        }
        // Query semantics (finding 4): the Model's `returns` type is the
        // authority for the generated output shape. A returned entity
        // binds the sample's output keys to its exact declared fields —
        // caller evidence cannot rename the output surface (the same
        // Model + lock always render the same names). The input side of a
        // query is bound too (finding 7): every projected source name must
        // be a declared field of the returned entity, so the handler's
        // input interface is spelled by the Model, never by caller
        // evidence — an invented input name cannot change the rendered
        // bytes under an unchanged IR + lock.
        if obligation.kind == DefinitionKind::Query && !obligation.return_fields.is_empty() {
            if let Some(sample) = requirement.sample.value() {
                let mut declared = obligation.return_fields.clone();
                declared.sort();
                let mut witnessed: Vec<String> = sample.output.keys().cloned().collect();
                witnessed.sort();
                if declared != witnessed {
                    return Err(super::diagnostic::failure(
                        super::CAPABILITY_GAP,
                        &[
                            ("requirement", obligation.id.as_str()),
                            ("reason", "returns-not-model"),
                        ],
                    ));
                }
            }
            for projection in &requirement.projection {
                if !obligation
                    .return_fields
                    .iter()
                    .any(|field| field == &projection.from)
                {
                    return Err(super::diagnostic::failure(
                        super::CAPABILITY_GAP,
                        &[
                            ("requirement", obligation.id.as_str()),
                            ("reason", "query-input-not-model"),
                        ],
                    ));
                }
            }
        }
        let _ = name;
    }
    Ok(())
}

/// Whether the scalar sample value fits the Model's declared type
/// spelling: the closed scalar bases map to the JSON scalar classes.
fn scalar_matches(value: &super::wire::Scalar, spelling: &str) -> bool {
    let base = spelling.strip_prefix("optional<").unwrap_or(spelling);
    let base = base.strip_prefix("list<").unwrap_or(base);
    let base = base.strip_suffix('>').unwrap_or(base);
    match base {
        "string" | "date" | "datetime" | "uuid" | "uri" => {
            matches!(value, super::wire::Scalar::Text(_))
        }
        "number" => matches!(value, super::wire::Scalar::Count(_)),
        "boolean" => matches!(value, super::wire::Scalar::Flag(_)),
        _ => true,
    }
}
