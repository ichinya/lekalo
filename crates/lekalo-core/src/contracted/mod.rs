//! Contracted mode for AI-written implementation (issue #40).
//!
//! The Lekalo Model is primary for the public contract, effects, and
//! invariants; the target source is maintained code written by a human
//! or an AI agent in the native language. Adapters check conformance and
//! may generate support artifacts only — Lekalo never rewrites command
//! or query handler bodies, custom SQL, application services, or
//! maintained tests outside the generated scenario layer.
//!
//! The conformed-binding registry (`.lekalo/import/contracted/
//! registry.json`, inside the accepted `lekalo.observed-model-draft`
//! authority home) records which canonical symbols the maintained code
//! implements, the typed signature and declared-effect claims captured
//! at declaration time, the ownership manifest of every generated
//! support artifact, and the attached native tests. [`check`] is the
//! read-only conformance gate: it re-fingerprints the sources,
//! recomputes the canonical signature and declared effects from the
//! typed IR, and re-digests every fingerprinted support artifact.

pub mod check;
pub mod diagnostic;
pub mod store;
pub mod types;
pub mod version;
mod wire;

use crate::diagnostics::DiagnosticSet;
use crate::ir::CompiledProject;
use crate::project_fs::Fs;

pub use check::CheckReceipt;
pub use diagnostic::{declaration_invalid_set, declaration_limit_set};
pub use types::{
    AdapterIdentity, ConformedRegistry, ConformedSymbol, DeclarationDocument, DeclaredEffect,
    DeclaredSymbol, SourceLocation, SupportArtifact, SupportKind, SupportLifecycle, SymbolKind,
};

/// The registry home and file name, re-exported for tests and tooling.
pub use version::{REGISTRY_DIR, REGISTRY_NAME};

/// Maximum accepted declaration document bytes.
pub const MAX_DECLARATION_BYTES: usize = version::MAX_REGISTRY_BYTES;

/// The exact `schema_version` literal of an adapter declaration
/// document.
pub const DECLARATION_SCHEMA_VERSION: &str = "lekalo/contracted-declaration/v1.0.0";

/// The loaded project context every contracted operation runs against.
pub struct Context {
    pub root: std::path::PathBuf,
    pub project: CompiledProject,
}

/// Load one project selection for the contracted seam: the validated
/// root plus the typed IR. Loader, structure, and IR failures surface
/// untouched as the terminal [`crate::result::DomainResult`].
pub fn context(
    selection: &crate::loader::LoadSelection,
) -> Result<Context, crate::result::DomainResult> {
    let loaded = crate::loader::load_with_snapshot(selection)?;
    let compilation = crate::ir::compile(&loaded.model).map_err(|failure| failure.into_result())?;
    Ok(Context {
        root: loaded.root,
        project: compilation.project,
    })
}

/// Read the persisted registry, if any; a present registry must be the
/// exact canonical rendering.
pub fn load_registry(ctx: &Context) -> Result<Option<ConformedRegistry>, DiagnosticSet> {
    let fs = Fs::open(&ctx.root).map_err(|_| diagnostic::registry_io_set("root-unreadable"))?;
    let bytes = match fs.read_file_opt(
        version::REGISTRY_DIR,
        version::REGISTRY_NAME,
        version::MAX_REGISTRY_BYTES,
    ) {
        Ok(Some(bytes)) => bytes,
        Ok(None) | Err(crate::project_fs::FsErrorKind::NotFound) => return Ok(None),
        Err(_) => return Err(diagnostic::registry_io_set("registry-unreadable")),
    };
    let registry = parse_registry(&bytes)?;
    if registry.mode != version::MODE || registry.identity != version::IDENTITY {
        return Err(diagnostic::registry_io_set("registry-identity"));
    }
    let declared_project = ctx
        .project
        .project
        .as_ref()
        .map(|project| project.id.as_str())
        .unwrap_or_default();
    if registry.project != declared_project {
        return Err(diagnostic::registry_io_set("project-mismatch"));
    }
    Ok(Some(registry))
}

/// Parse and fully validate registry bytes: canonical re-rendering
/// equality, identity, and the recorded mode.
pub fn parse_registry(bytes: &[u8]) -> Result<ConformedRegistry, DiagnosticSet> {
    if bytes.len() > version::MAX_REGISTRY_BYTES {
        return Err(diagnostic::declaration_limit_set(
            "registry-bytes",
            bytes.len(),
        ));
    }
    let registry: ConformedRegistry = serde_json::from_slice(bytes)
        .map_err(|_| diagnostic::registry_io_set("registry-malformed"))?;
    if registry.schema_version != version::SCHEMA_VERSION {
        return Err(diagnostic::registry_io_set("registry-identity"));
    }
    let canonical = canonical_bytes(&registry)?;
    if canonical.as_bytes() != bytes {
        return Err(diagnostic::registry_io_set("registry-noncanonical"));
    }
    Ok(registry)
}

/// The canonical JSON bytes of one registry (compact, serde field
/// order, sorted records).
pub fn canonical_bytes(registry: &ConformedRegistry) -> Result<String, DiagnosticSet> {
    let bytes = serde_json::to_string(registry)
        .map_err(|_| diagnostic::registry_io_set("registry-unserializable"))?;
    if bytes.len() > version::MAX_REGISTRY_BYTES {
        return Err(diagnostic::declaration_limit_set(
            "registry-bytes",
            bytes.len(),
        ));
    }
    Ok(bytes)
}

/// Persist the registry canonically and atomically.
pub(crate) fn save_registry(
    ctx: &Context,
    registry: &ConformedRegistry,
) -> Result<(), DiagnosticSet> {
    let bytes = canonical_bytes(registry)?;
    store::write_confined(
        &ctx.root,
        version::REGISTRY_DIR,
        version::REGISTRY_NAME,
        bytes.as_bytes(),
    )
}

/// Parse raw declaration bytes (public for the wire gate and tests).
pub fn parse_declaration_bytes(bytes: &[u8]) -> Result<DeclarationDocument, DiagnosticSet> {
    wire::parse_declaration(bytes)
}

/// Merge one adapter declaration into the registry (or create the
/// registry): every declared symbol must exist canonically with the
/// same kind and module; claims are validated against the closed kind
/// rules; artifacts upsert by path. New symbols get a `recorded`
/// history entry; refreshes are silent, like adapter scans.
pub fn update_registry(
    ctx: &Context,
    declaration_bytes: &[u8],
) -> Result<UpdateReceipt, DiagnosticSet> {
    let declaration = wire::parse_declaration(declaration_bytes)?;
    let declared_project = ctx
        .project
        .project
        .as_ref()
        .map(|project| project.id.as_str())
        .unwrap_or_default();
    if declaration.project != declared_project {
        return Err(diagnostic::declaration_invalid_set(
            "project-mismatch",
            None,
        ));
    }
    let declared_modules: Vec<&str> = ctx
        .project
        .modules
        .iter()
        .map(|module| module.id.as_str())
        .collect();
    for symbol in &declaration.symbols {
        let module = ConformedRegistry::module_of(&symbol.id)
            .unwrap_or_default()
            .to_owned();
        if !declared_modules.contains(&module.as_str()) {
            return Err(diagnostic::unknown_module_set(&module));
        }
        let Some(definition) = ctx
            .project
            .definitions
            .iter()
            .find(|definition| definition.id().as_str() == symbol.id)
        else {
            return Err(diagnostic::unknown_symbol_set(&symbol.id));
        };
        if check::kind_word(definition) != symbol.kind.key() {
            return Err(diagnostic::declaration_invalid_set(
                "kind-mismatch",
                Some(&symbol.id),
            ));
        }
        if check::signature_required(symbol.kind) && symbol.signature.is_none() {
            return Err(diagnostic::declaration_invalid_set(
                "signature-required",
                Some(&symbol.id),
            ));
        }
        if !check::signature_required(symbol.kind) && symbol.signature.is_some() {
            return Err(diagnostic::declaration_invalid_set(
                "signature-unsupported",
                Some(&symbol.id),
            ));
        }
        if !check::effects_allowed(symbol.kind) && !symbol.effects.is_empty() {
            return Err(diagnostic::declaration_invalid_set(
                "effects-unsupported",
                Some(&symbol.id),
            ));
        }
    }
    for artifact in &declaration.artifacts {
        if ctx
            .project
            .definitions
            .iter()
            .all(|definition| definition.id().as_str() != artifact.symbol)
        {
            return Err(diagnostic::unknown_symbol_set(&artifact.symbol));
        }
    }

    let mut registry = load_registry(ctx)?.unwrap_or_else(|| {
        ConformedRegistry::empty(
            &declaration.project,
            declaration.adapter.clone(),
            &declaration.revision,
        )
    });
    registry.adapter = declaration.adapter.clone();
    registry.revision = declaration.revision.clone();

    let mut recorded: Vec<String> = Vec::new();
    for declared in &declaration.symbols {
        if let Some(record) = registry
            .symbols
            .iter_mut()
            .find(|record| record.id == declared.id)
        {
            record.kind = declared.kind;
            record.source = declared.source.clone();
            record.fingerprint = declared.fingerprint.clone();
            record.signature = declared.signature.clone();
            record.effects = declared.effects.clone();
            record.provenance.adapter = declaration.adapter.id.clone();
            record.provenance.revision = declaration.revision.clone();
            continue;
        }
        recorded.push(declared.id.clone());
        registry.symbols.push(ConformedSymbol {
            id: declared.id.clone(),
            kind: declared.kind,
            source: declared.source.clone(),
            fingerprint: declared.fingerprint.clone(),
            state: types::DriftState::Conformant,
            signature: declared.signature.clone(),
            effects: declared.effects.clone(),
            native_tests: Vec::new(),
            gates: Vec::new(),
            provenance: types::Provenance {
                adapter: declaration.adapter.id.clone(),
                revision: declaration.revision.clone(),
            },
            history: vec![types::HistoryEntry {
                event: types::HistoryEvent::Recorded,
                revision: declaration.revision.clone(),
            }],
        });
        if registry.symbols.len() > version::MAX_SYMBOLS {
            return Err(diagnostic::declaration_limit_set(
                "symbols",
                registry.symbols.len(),
            ));
        }
    }
    registry
        .symbols
        .sort_by(|left, right| left.id.cmp(&right.id));

    for declared in &declaration.artifacts {
        if let Some(record) = registry
            .artifacts
            .iter_mut()
            .find(|record| record.path == declared.path)
        {
            *record = declared.clone();
            continue;
        }
        registry.artifacts.push(declared.clone());
        if registry.artifacts.len() > version::MAX_SUPPORT_ARTIFACTS {
            return Err(diagnostic::declaration_limit_set(
                "artifacts",
                registry.artifacts.len(),
            ));
        }
    }
    registry
        .artifacts
        .sort_by(|left, right| left.path.cmp(&right.path));

    save_registry(ctx, &registry)?;
    recorded.sort();
    Ok(UpdateReceipt {
        status: "valid",
        operation: "conform",
        mode: "update",
        symbols: registry.symbols.len(),
        artifacts: registry.artifacts.len(),
        recorded,
    })
}

/// The receipt of `contract update`.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct UpdateReceipt {
    pub status: &'static str,
    pub operation: &'static str,
    pub mode: &'static str,
    pub symbols: usize,
    pub artifacts: usize,
    /// The newly recorded symbol ids, sorted.
    pub recorded: Vec<String>,
}

/// Run the conformance gate over the registry (or one module slice).
pub fn check(ctx: &Context, module: Option<&str>) -> Result<CheckReceipt, DiagnosticSet> {
    let registry =
        load_registry(ctx)?.ok_or_else(|| diagnostic::registry_io_set("registry-missing"))?;
    if let Some(module) = module {
        if !ctx
            .project
            .modules
            .iter()
            .any(|declared| declared.id.as_str() == module)
        {
            return Err(diagnostic::unknown_module_set(module));
        }
    }
    check::run_check(ctx, &registry, module)
}

/// Attach verbatim native-test or gate ids to one recorded symbol.
pub fn attach(
    ctx: &Context,
    symbol_id: &str,
    native_tests: &[String],
    gates: &[String],
) -> Result<AttachReceipt, DiagnosticSet> {
    let mut registry =
        load_registry(ctx)?.ok_or_else(|| diagnostic::registry_io_set("registry-missing"))?;
    let position = registry
        .symbols
        .iter()
        .position(|record| record.id == symbol_id)
        .ok_or_else(|| diagnostic::selector_unknown_set(symbol_id))?;
    for id in native_tests.iter().chain(gates.iter()) {
        if !version::is_external_id(id) {
            return Err(diagnostic::declaration_invalid_set(
                "attachment-id",
                Some(id),
            ));
        }
    }
    let record = &mut registry.symbols[position];
    let mut tests = record.native_tests.clone();
    for test in native_tests {
        if !tests.contains(test) {
            tests.push(test.clone());
        }
    }
    tests.sort();
    if tests.len() > version::MAX_ATTACHMENTS {
        return Err(diagnostic::declaration_limit_set(
            "native-tests",
            tests.len(),
        ));
    }
    let mut attached_gates = record.gates.clone();
    for gate in gates {
        if !attached_gates.contains(gate) {
            attached_gates.push(gate.clone());
        }
    }
    attached_gates.sort();
    if attached_gates.len() > version::MAX_ATTACHMENTS {
        return Err(diagnostic::declaration_limit_set(
            "gates",
            attached_gates.len(),
        ));
    }
    if tests != record.native_tests || attached_gates != record.gates {
        record.native_tests = tests.clone();
        record.gates = attached_gates.clone();
        record.history.push(types::HistoryEntry {
            event: types::HistoryEvent::Attached,
            revision: registry.revision.clone(),
        });
        if record.history.len() > version::MAX_HISTORY {
            let excess = record.history.len() - version::MAX_HISTORY;
            record.history.drain(..excess);
        }
        save_registry(ctx, &registry)?;
    }
    Ok(AttachReceipt {
        status: "valid",
        operation: "conform",
        mode: "attach",
        symbol: symbol_id.to_owned(),
        native_tests: tests,
        gates: attached_gates,
    })
}

/// The receipt of `contract attach`.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct AttachReceipt {
    pub status: &'static str,
    pub operation: &'static str,
    pub mode: &'static str,
    pub symbol: String,
    pub native_tests: Vec<String>,
    pub gates: Vec<String>,
}

/// The receipt of `contract support` (ownership registration).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct SupportReceipt {
    pub status: &'static str,
    pub operation: &'static str,
    pub mode: &'static str,
    pub symbol: String,
    pub path: String,
    pub kind: SupportKind,
    pub lifecycle: SupportLifecycle,
}

/// Register one support artifact in the ownership manifest section.
///
/// The path must stay inside the generated home
/// (`.lekalo/generated/**`): support artifacts are the only writes
/// contracted mode performs, and maintained implementation paths refuse
/// by construction. The artifact content itself is written by its
/// owner (the adapter or the maintainer); Lekalo records ownership,
/// lifecycle, and the exact content digest for the staleness gate.
pub fn support(
    ctx: &Context,
    symbol_id: &str,
    kind: SupportKind,
    path: &str,
    lifecycle: SupportLifecycle,
    digest: Option<&str>,
) -> Result<SupportReceipt, DiagnosticSet> {
    if !version::is_support_path(path) {
        return Err(diagnostic::declaration_invalid_set(
            "support-path-refused",
            Some(path),
        ));
    }
    if let Some(digest) = digest {
        if !version::is_sha256(digest) {
            return Err(diagnostic::declaration_invalid_set(
                "artifact-digest",
                Some(path),
            ));
        }
    }
    let mut registry =
        load_registry(ctx)?.ok_or_else(|| diagnostic::registry_io_set("registry-missing"))?;
    if registry.symbols.iter().all(|record| record.id != symbol_id) {
        return Err(diagnostic::selector_unknown_set(symbol_id));
    }
    let artifact = SupportArtifact {
        symbol: symbol_id.to_owned(),
        kind,
        path: path.to_owned(),
        lifecycle,
        digest: digest.map(|digest| digest.to_owned()),
    };
    if let Some(record) = registry
        .artifacts
        .iter_mut()
        .find(|record| record.path == path)
    {
        *record = artifact;
    } else {
        registry.artifacts.push(artifact);
        if registry.artifacts.len() > version::MAX_SUPPORT_ARTIFACTS {
            return Err(diagnostic::declaration_limit_set(
                "artifacts",
                registry.artifacts.len(),
            ));
        }
        registry
            .artifacts
            .sort_by(|left, right| left.path.cmp(&right.path));
    }
    save_registry(ctx, &registry)?;
    Ok(SupportReceipt {
        status: "valid",
        operation: "conform",
        mode: "support",
        symbol: symbol_id.to_owned(),
        path: path.to_owned(),
        kind,
        lifecycle,
    })
}
