//! Installed adapter custody and read-only collection through TargetClient.
//! The package gates, manifest consistency and confinement are shared with
//! generation; no direct application execution or shell bypass exists here.
use super::{diagnostic::failure, input, Evidence, Pins, State};
use crate::adapter_package::{Inventory, ManifestDocument};
use crate::observed::{BindingState, BindingStatus, ObservedIndex, SymbolKind};
use crate::orchestration::catalog::{self, AdapterSupply};
use crate::result::DomainResult;
use crate::target_protocol::{
    wire::{LintBinding, LintRequest, Operation},
    CallRequest, TargetClient,
};
use std::path::Path;
pub fn collect(
    root: &Path,
    target: &str,
    mut request: LintRequest,
) -> Result<Evidence, DomainResult> {
    let inventory =
        Inventory::load(root).map_err(|f| crate::adapter_package::diagnostic::domain_result(&f))?;
    let fs = crate::project_fs::Fs::open(root)
        .map_err(|_| failure("ai-lint.input-invalid", "project-root"))?;
    let mut matches = Vec::new();
    for row in inventory
        .rows()
        .iter()
        .filter(|r| r.selected && !r.quarantined)
    {
        let dir =
            crate::adapter_package::quarantine::package_path(&row.id, &row.version, &row.digest);
        // This is the package store's computed custody path, not an admitted
        // application source path (.lekalo is intentionally outside that grammar).
        let bytes = fs
            .read_file_opt(&dir, "adapter.manifest.json", input::MAX_BYTES)
            .map_err(|_| failure("ai-lint.input-invalid", "installed-manifest-read"))?
            .ok_or_else(|| failure("ai-lint.input-invalid", "installed-manifest-missing"))?;
        let manifest = ManifestDocument::from_bytes(&bytes)
            .map_err(|f| crate::adapter_package::diagnostic::domain_result(&f))?;
        let value: serde_json::Value =
            serde_json::from_slice(&manifest.canonical_bytes()).expect("manifest wire");
        if value["capabilities"]["targets"]
            .as_array()
            .is_some_and(|a| a.iter().any(|t| t.as_str() == Some(target)))
        {
            matches.push((dir, manifest, value));
        }
    }
    if matches.len() != 1 {
        return Err(failure(
            "ai-lint.input-invalid",
            "installed-target-missing-or-ambiguous",
        ));
    }
    let (dir, manifest, value) = matches.remove(0);
    let entry = root.join(&dir).join(manifest.entry());
    let runtime = value["executable"]["runtime"]["kind"]
        .as_str()
        .unwrap_or("");
    let (program, args) = match runtime {
        "node" | "php" => (
            runtime.to_owned(),
            vec![entry.to_string_lossy().into_owned()],
        ),
        "native" => (entry.to_string_lossy().into_owned(), Vec::new()),
        _ => return Err(failure("ai-lint.input-invalid", "runtime-unsupported")),
    };
    // AdapterSupply::new is for application-owned source paths; its #4
    // grammar deliberately excludes .lekalo. This supply comes from the
    // selected package inventory and still passes catalog's independent
    // integrity/trust/revocation gate before describe or lint can launch.
    let canonical_root = std::fs::canonicalize(root)
        .map_err(|_| failure("ai-lint.input-invalid", "installed-root"))?;
    let canonical_entry = std::fs::canonicalize(&entry)
        .map_err(|_| failure("ai-lint.input-invalid", "installed-entry"))?;
    if !canonical_entry.starts_with(canonical_root.join(&dir)) {
        return Err(failure("ai-lint.input-invalid", "installed-entry-escape"));
    }
    let supply = AdapterSupply {
        command: crate::target_protocol::transport::AdapterCommand {
            program: program.into(),
            args,
        },
        source_id: format!("{dir}/{}", manifest.entry()),
    };
    let mut client = TargetClient::default();
    let discovered = catalog::discover(&mut client, &supply, root, Default::default())
        .map_err(|f| DomainResult::from(&f))?;
    if discovered.negotiated_version != crate::target_protocol::version::LINT_VERSION
        || discovered
            .capability("lint.ai-readability")
            .map_or(true, |c| {
                !matches!(
                    c.state,
                    crate::target_protocol::wire::SupportState::Full
                        | crate::target_protocol::wire::SupportState::Partial
                )
            })
    {
        return Err(DomainResult::from(
            &crate::target_protocol::TargetFailure::CapabilityUnsupported {
                detail: "lint.ai-readability",
            },
        ));
    }
    request.pins.capabilities = State::Known(discovered.capability_digest.clone());
    let outcome = client
        .call(
            &supply.command,
            CallRequest {
                operation: Operation::Lint,
                target: Some(target),
                profile: None,
                profile_resolution: None,
                ir_path: None,
                dry_run: None,
                plan_id: None,
                native_request: None,
                lint_request: Some(&request),
            },
            root,
            &fs,
            None,
        )
        .map_err(|f| DomainResult::from(&f))?;
    let evidence = outcome
        .response
        .result
        .and_then(|r| r.lint_evidence)
        .ok_or_else(|| failure("ai-lint.input-invalid", "lint-evidence-missing"))?;
    // The package integrity gate verifies the complete entry bytes. Discovery's
    // optional small-entry digest is deliberately absent for the vendored TS
    // bundle; absence never turns the adapter's self-assertion into a pin.
    let entry_pin = manifest
        .files()
        .iter()
        .find(|file| file.path() == manifest.entry())
        .ok_or_else(|| failure("ai-lint.input-invalid", "entry-integrity-pin"))?;
    if evidence.target != target
        || evidence.producer.id != discovered.adapter.id
        || evidence.producer.artifact_digest != entry_pin.digest().as_str()
        || evidence.pins != request.pins
    {
        return Err(failure("ai-lint.input-invalid", "producer-custody"));
    }
    Ok(evidence)
}
pub fn request(
    observed: Option<&ObservedIndex>,
    fs: &crate::project_fs::Fs,
    model: &str,
    ir: &str,
    scope: Vec<String>,
    files: Vec<String>,
) -> Result<LintRequest, DomainResult> {
    let mut bindings = Vec::new();
    if let Some(o) = observed {
        for r in &o.symbols {
            let selected = r.kind == SymbolKind::Entity
                || scope
                    .iter()
                    .any(|selected| selected == &r.id || r.id.starts_with(&format!("{selected}.")));
            if !selected
                || r.status == BindingStatus::Inferred
                || r.state != BindingState::Current
                || !matches!(r.kind, SymbolKind::Command | SymbolKind::Entity)
            {
                continue;
            }
            if let (Some(native), Some(location), Some(fingerprint)) =
                (&r.stable_key, &r.location, &r.fingerprint)
            {
                if input::digest(&input::read_source(fs, &location.path)?) != *fingerprint {
                    continue;
                }
                bindings.push(LintBinding {
                    path: location.path.clone(),
                    native_id: native.clone(),
                    symbol: r.id.clone(),
                    kind: r.kind.key().into(),
                    fingerprint: fingerprint.clone(),
                });
            }
        }
    }
    bindings.sort_by(|a, b| {
        (&a.path, &a.native_id, &a.symbol).cmp(&(&b.path, &b.native_id, &b.symbol))
    });
    let files = files
        .into_iter()
        .chain(bindings.iter().map(|b| b.path.clone()))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(LintRequest {
        scope,
        pins: Pins {
            model: State::Known(model.into()),
            ir: State::Known(ir.into()),
            observed: observed.map_or(State::Unknown, |o| State::Known(input::hash(o))),
            revision: observed.map_or(State::Unknown, |o| State::Known(o.revision.clone())),
            profile: State::Unknown,
            capabilities: State::Unknown,
        },
        bindings,
        files,
    })
}
