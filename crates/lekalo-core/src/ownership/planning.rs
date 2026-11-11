use super::*;
use crate::{contracted, managed as mg};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(crate) const REGISTRY: &str = ".lekalo/import/contracted/registry.json";

fn selected(request: &Request, id: &str) -> bool {
    request.symbol.as_ref().map_or_else(
        || {
            id.strip_prefix(&request.module)
                .is_some_and(|suffix| suffix.starts_with('.'))
        },
        |symbol| id == symbol,
    )
}

fn add(root: &Path, paths: &mut BTreeMap<String, Option<Vec<u8>>>, path: &str) -> Outcome<()> {
    paths.insert(path.to_owned(), read(root, path)?);
    if paths.len() > MAX_FILES
        || paths.values().flatten().map(Vec::len).sum::<usize>() > MAX_BYTES * 4
    {
        return Err(fail("ownership-closure-bound"));
    }
    Ok(())
}

/// Prepare a scoped handoff. Every native byte consumed by the staged runner
/// comes from the explicit bounded closure, never a whole-project conversion.
pub fn plan(root: &Path, request: Request) -> Outcome<TransitionPlan> {
    if request.schema_version != format!("lekalo/ownership-transition-request/v{VERSION}")
        || !mg::token(&request.module)
        || request.module.contains('.')
        || request.symbol.as_ref().is_some_and(|id| {
            !selected(
                &Request {
                    symbol: None,
                    ..request.clone()
                },
                id,
            )
        })
        || request.inputs.len() > MAX_FILES
        || !request.unknown_behavior.is_empty()
        || request.inputs.iter().any(|path| {
            !path_ok(path)
                || path.starts_with(".lekalo/")
                || path.starts_with("scratch/")
                || path == "ownership-bindings.json"
        })
        || request.runner.expected.is_empty()
        || request.runner.expected.len() > 128
    {
        return Err(fail("ownership-request-scope"));
    }
    let load = crate::loader::load_validated_root(root.to_path_buf())?;
    let compiled = crate::ir::compile(&load.model).map_err(|error| error.into_result())?;
    let policy_bytes = required(root, mg::source::PROJECT_PATH)?;
    let mut policy = mg::parse_ownership(&policy_bytes)?;
    mg::source::validate_module(&policy, &compiled.project, &request.module)?;
    let mut state = state(root)?;
    if request.symbol.is_none()
        && state
            .assignments
            .iter()
            .any(|row| row.module == request.module && row.symbol.is_some())
    {
        // A module-wide policy word cannot silently absorb split native custody.
        return Err(fail("ownership-mixed-module-scope"));
    }
    let module_policy = policy
        .modules
        .iter_mut()
        .find(|row| row.module == request.module)
        .ok_or_else(|| fail("ownership-module"))?;
    let mut previous = state
        .assignments
        .iter()
        .find(|row| row.module == request.module && row.symbol == request.symbol)
        .or_else(|| {
            state
                .assignments
                .iter()
                .find(|row| row.module == request.module && row.symbol.is_none())
        })
        .cloned();
    let from = previous.as_ref().map_or(module_policy.mode, |row| row.mode);
    if !matches!(
        (from, request.to),
        (mg::Mode::Observed, mg::Mode::Contracted)
            | (mg::Mode::Contracted, mg::Mode::Managed)
            | (mg::Mode::Managed, mg::Mode::Contracted)
            | (mg::Mode::Managed, mg::Mode::Managed)
    ) {
        return Err(fail("ownership-transition"));
    }
    let mut paths = BTreeMap::new();
    for document in &load.documents {
        add(root, &mut paths, &document.document.path)?;
    }
    for path in request.inputs.iter().map(String::as_str).chain([
        "lekalo.lock",
        mg::source::PROJECT_PATH,
        STATE,
        REGISTRY,
        mg::custody::MANIFEST_LOGICAL,
    ]) {
        add(root, &mut paths, path)?;
    }
    for path in &policy.inputs {
        add(root, &mut paths, path)?;
    }
    for path in [&request.coverage, &request.declaration]
        .into_iter()
        .flatten()
    {
        add(root, &mut paths, path)?;
    }
    if from == mg::Mode::Observed {
        let observed = crate::observed::ObservedContext {
            model: load.model.clone(),
            root: root.to_path_buf(),
        };
        let index_path = format!(
            "{}/{}",
            crate::observed::INDEX_DIR,
            crate::observed::INDEX_NAME
        );
        add(root, &mut paths, &index_path)?;
        if let Some(index) =
            crate::observed::load_index(&observed).map_err(DomainResult::invalid)?
        {
            for row in index
                .symbols
                .iter()
                .filter(|row| selected(&request, &row.id))
            {
                if row.status == crate::observed::BindingStatus::Inferred
                    || row.state != crate::observed::BindingState::Current
                    || !row.candidates.is_empty()
                {
                    return Err(fail("ownership-binding-unconfirmed"));
                }
                if let Some(location) = &row.location {
                    let actual = required(root, &location.path)?;
                    if row.fingerprint.as_deref() != Some(mg::digest(&actual).as_str()) {
                        return Err(fail("ownership-observation-stale"));
                    }
                    add(root, &mut paths, &location.path)?;
                }
            }
        } else if request.declaration.is_none() {
            return Err(fail("ownership-explicit-contract-required"));
        }
    }
    let ctx = contracted::Context {
        root: root.to_path_buf(),
        project: compiled.project,
    };
    let existing = contracted::load_registry(&ctx).map_err(DomainResult::invalid)?;
    let mut all_sources = BTreeMap::<String, BTreeSet<String>>::new();
    let observed = crate::observed::ObservedContext {
        model: load.model.clone(),
        root: root.to_path_buf(),
    };
    let observed_path = format!(
        "{}/{}",
        crate::observed::INDEX_DIR,
        crate::observed::INDEX_NAME
    );
    add(root, &mut paths, &observed_path)?;
    if let Some(index) = crate::observed::load_index(&observed).map_err(DomainResult::invalid)? {
        for row in &index.symbols {
            if let Some(location) = &row.location {
                all_sources
                    .entry(location.path.clone())
                    .or_default()
                    .insert(row.id.clone());
            }
        }
    }
    if let Some(registry) = &existing {
        for row in &registry.symbols {
            all_sources
                .entry(row.source.path.clone())
                .or_default()
                .insert(row.id.clone());
            if selected(&request, &row.id) {
                add(root, &mut paths, &row.source.path)?;
            }
        }
    }
    if let Some(path) = &request.declaration {
        let declaration = contracted::parse_declaration_bytes(&required(root, path)?)
            .map_err(DomainResult::invalid)?;
        if (from != mg::Mode::Observed
            && !(from == mg::Mode::Managed && request.to == mg::Mode::Contracted))
            || declaration.symbols.is_empty()
            || declaration
                .symbols
                .iter()
                .any(|row| !selected(&request, &row.id))
        {
            return Err(fail("ownership-declaration-scope"));
        }
        for row in &declaration.symbols {
            all_sources
                .entry(row.source.path.clone())
                .or_default()
                .insert(row.id.clone());
            add(root, &mut paths, &row.source.path)?;
        }
    }
    // One physical file cannot be transferred by selecting only one of its owners.
    for ids in all_sources.values() {
        if ids.iter().any(|id| selected(&request, id))
            && ids.iter().any(|id| !selected(&request, id))
        {
            return Err(fail("ownership-shared-file"));
        }
    }
    let staged = verification::snapshot(&paths)?;
    let stage = staged.path();
    let staged_load = crate::loader::load_validated_root(stage.to_path_buf())?;
    let staged_ir = crate::ir::compile(&staged_load.model).map_err(|error| error.into_result())?;
    let stage_ctx = contracted::Context {
        root: stage.to_path_buf(),
        project: staged_ir.project,
    };
    if let Some(path) = &request.declaration {
        contracted::update_registry(&stage_ctx, &required(root, path)?)
            .map_err(DomainResult::invalid)?;
    }
    let mut registry = contracted::load_registry(&stage_ctx)
        .map_err(DomainResult::invalid)?
        .ok_or_else(|| fail("ownership-contract-missing"))?;
    let ids: BTreeSet<String> = registry
        .symbols
        .iter()
        .filter(|row| selected(&request, &row.id))
        .map(|row| row.id.clone())
        .collect();
    if ids.is_empty() {
        return Err(fail("ownership-selection-empty"));
    }
    // Attachment is explicit in the reviewed oracle, not inferred from a file name.
    if from == mg::Mode::Observed || request.declaration.is_some() {
        for row in registry
            .symbols
            .iter_mut()
            .filter(|row| ids.contains(&row.id))
        {
            row.native_tests = request
                .runner
                .expected
                .iter()
                .map(|row| row.scenario.clone())
                .collect();
            row.native_tests.sort();
            row.native_tests.dedup();
        }
    }
    // Scoped conformance must not require unrelated observed neighbors to migrate.
    let full_registry = registry.clone();
    registry.symbols.retain(|row| ids.contains(&row.id));
    registry.artifacts.retain(|row| ids.contains(&row.symbol));
    mg::fs::write_file(
        stage,
        REGISTRY,
        contracted::canonical_bytes(&registry)
            .map_err(DomainResult::invalid)?
            .as_bytes(),
    )?;
    let module_scope = request.symbol.is_none().then_some(request.module.as_str());
    let checked = contracted::check(&stage_ctx, module_scope).map_err(DomainResult::invalid)?;
    if checked.status != "valid" {
        return Err(fail("ownership-contract-incomplete"));
    }
    registry = full_registry;
    let baseline: Vec<Binding> = registry
        .symbols
        .iter()
        .filter(|row| ids.contains(&row.id))
        .map(|row| Binding {
            symbol: row.id.clone(),
            path: row.source.path.clone(),
            native_symbol: mg::canonical::camel(row.id.rsplit('.').next().unwrap_or_default()),
        })
        .collect();
    if baseline.iter().any(|row| {
        row.path.starts_with(".lekalo/")
            || row.path.starts_with("scratch/")
            || row.path == "ownership-bindings.json"
    }) {
        return Err(fail("ownership-runtime-source-scope"));
    }
    let mut candidate = baseline.clone();
    if from == mg::Mode::Managed && previous.is_none() {
        let coverage_path = request
            .coverage
            .as_deref()
            .ok_or_else(|| fail("ownership-legacy-coverage-required"))?;
        let coverage = mg::parse_coverage(&required(root, coverage_path)?)?;
        let record = mg::history::current_record(root)?;
        let generation_plan = mg::history::plan(root, &record.plan_digest)?;
        if generation_plan.coverage_digest != mg::hash(&coverage) {
            return Err(fail("ownership-legacy-coverage-stale"));
        }
        let target = policy
            .modules
            .iter()
            .find(|row| row.module == request.module)
            .and_then(|row| {
                row.targets
                    .iter()
                    .find(|target| target.target == coverage.target)
            })
            .ok_or_else(|| fail("ownership-legacy-target"))?;
        let mut claims = Vec::new();
        for output in &record.outputs {
            if output.owner_module != request.module || output.owner_target != coverage.target {
                continue;
            }
            let id = coverage
                .requirements
                .iter()
                .find(|row| {
                    mg::canonical::output_paths(&target.generated_root, &coverage.module, row)
                        .is_some_and(|(implementation, test)| {
                            implementation == output.path || test == output.path
                        })
                })
                .map(|row| row.id.clone())
                .ok_or_else(|| fail("ownership-legacy-artifact"))?;
            if !ids.contains(&id) {
                continue;
            }
            if output.role == mg::ArtifactRole::Implementation
                && !baseline
                    .iter()
                    .any(|binding| binding.symbol == id && binding.path == output.path)
            {
                return Err(fail("ownership-legacy-binding"));
            }
            claims.push(Claim {
                binding: Binding {
                    symbol: id.clone(),
                    path: output.path.clone(),
                    native_symbol: mg::canonical::camel(id.rsplit('.').next().unwrap_or_default()),
                },
                digest: output.digest.clone(),
                lifecycle: "generated".to_owned(),
            });
        }
        for path in [
            mg::history::custody_logical("current"),
            mg::custody::MANIFEST_LOGICAL.to_owned(),
        ] {
            add(root, &mut paths, &path)?;
        }
        for object in mg::history::objects_for_probe(root) {
            add(
                root,
                &mut paths,
                &mg::history::custody_logical(&format!("objects/{object}")),
            )?;
        }
        previous = Some(Assignment {
            module: request.module.clone(),
            symbol: request.symbol.clone(),
            mode: from,
            transition: record.generation_digest,
            artifacts: claims,
        });
    }
    let mut artifacts = Vec::new();
    let mut generated_coverage = None;
    let mut edits = BTreeMap::<String, Option<Vec<u8>>>::new();
    if request.to == mg::Mode::Managed {
        let coverage_path = request
            .coverage
            .as_deref()
            .ok_or_else(|| fail("ownership-coverage-required"))?;
        let coverage = mg::parse_coverage(&required(root, coverage_path)?)?;
        generated_coverage = Some(coverage.clone());
        let authority = mg::authority::derive_loaded(&load, &request.module)?;
        mg::authority::qualify(root, &coverage, &authority)?;
        mg::authority::ledger_complete(&coverage, &authority)?;
        let assignment = policy
            .modules
            .iter()
            .find(|row| row.module == request.module)
            .expect("validated module");
        if assignment.targets.len() != 1 {
            return Err(fail("ownership-one-target-required"));
        }
        let target = &assignment.targets[0];
        if target.generated_root.starts_with(".lekalo/")
            || target.generated_root.starts_with("scratch")
        {
            return Err(fail("ownership-runtime-generated-scope"));
        }
        let mut managed_assignment = assignment.clone();
        managed_assignment.mode = mg::Mode::Managed;
        mg::coverage::admit(&managed_assignment, target, &coverage)?;
        mg::authority::resolve_target_provenance(target, &authority)?;
        mg::bindings::resolve(root, target, &coverage)?;
        add(root, &mut paths, &coverage.adapter.entry)?;
        for extension in &coverage.extensions {
            let path = mg::bindings::binding_file(&extension.binding)
                .ok_or_else(|| fail("ownership-extension"))?;
            add(root, &mut paths, path)?;
        }
        let mut scoped_coverage = coverage.clone();
        scoped_coverage
            .requirements
            .retain(|row| ids.contains(&row.id));
        let rendered = mg::canonical::render_inventory(
            &scoped_coverage,
            &target.generated_root,
            &authority.semantics,
        )?;
        if rendered.is_empty() {
            return Err(fail("ownership-generator-empty"));
        }
        candidate.clear();
        for output in rendered {
            let id = scoped_coverage
                .requirements
                .iter()
                .find(|row| {
                    mg::canonical::output_paths(&target.generated_root, &coverage.module, row)
                        .is_some_and(|(implementation, test)| {
                            implementation == output.path || test == output.path
                        })
                })
                .map(|row| row.id.clone())
                .ok_or_else(|| fail("ownership-artifact-owner"))?;
            let binding = Binding {
                symbol: id.clone(),
                path: output.path.clone(),
                native_symbol: mg::canonical::camel(id.rsplit('.').next().unwrap_or_default()),
            };
            let before = read(root, &output.path)?;
            let claimed = previous.as_ref().is_some_and(|row| {
                row.artifacts.iter().any(|claim| {
                    claim.binding.path == output.path
                        && before.as_deref().map(mg::digest).as_ref() == Some(&claim.digest)
                })
            });
            if before.is_some() && !claimed && !baseline.iter().any(|row| row.path == output.path) {
                return Err(fail("ownership-unclaimed-output"));
            }
            if output.role == mg::ArtifactRole::Implementation {
                candidate.push(binding.clone());
            }
            artifacts.push(Claim {
                binding,
                digest: mg::digest(&output.bytes),
                lifecycle: "generated".to_owned(),
            });
            add(root, &mut paths, &output.path)?;
            edits.insert(output.path, Some(output.bytes));
        }
        if ids
            .iter()
            .any(|id| !candidate.iter().any(|binding| &binding.symbol == id))
        {
            return Err(fail("ownership-generator-scope-incomplete"));
        }
    } else if from == mg::Mode::Managed {
        let previous = previous
            .as_ref()
            .ok_or_else(|| fail("ownership-legacy-managed-import-required"))?;
        for claim in previous
            .artifacts
            .iter()
            .filter(|claim| ids.contains(&claim.binding.symbol))
        {
            if required(root, &claim.binding.path).map(|bytes| mg::digest(&bytes))? != claim.digest
            {
                return Err(fail("ownership-generated-drift"));
            }
            add(root, &mut paths, &claim.binding.path)?;
            let mut maintained = claim.clone();
            maintained.lifecycle = "scaffolded".to_owned();
            artifacts.push(maintained);
        }
        if artifacts.is_empty() {
            return Err(fail("ownership-downgrade-empty"));
        }
    }
    for binding in &candidate {
        let row = registry
            .symbols
            .iter_mut()
            .find(|row| row.id == binding.symbol)
            .expect("selected binding");
        row.source.path = binding.path.clone();
        row.source.line = None;
        let source = match edits.get(&binding.path).and_then(|bytes| bytes.clone()) {
            Some(bytes) => bytes,
            None => required(root, &binding.path)?,
        };
        row.fingerprint = Some(mg::digest(&source));
    }
    candidate.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    artifacts.sort_by(|a, b| a.binding.path.cmp(&b.binding.path));
    if !artifacts.is_empty() {
        let authority = mg::authority::derive_loaded(&load, &request.module)?;
        edits.insert(
            mg::custody::MANIFEST_LOGICAL.to_owned(),
            Some(super::manifest::prepare(
                root,
                &artifacts,
                generated_coverage.as_ref(),
                &authority,
            )?),
        );
    }
    if request.symbol.is_none() {
        policy
            .modules
            .iter_mut()
            .find(|row| row.module == request.module)
            .expect("selected module")
            .mode = request.to;
        edits.insert(mg::source::PROJECT_PATH.to_owned(), Some(bytes(&policy)));
    }
    edits.insert(
        REGISTRY.to_owned(),
        Some(
            contracted::canonical_bytes(&registry)
                .map_err(DomainResult::invalid)?
                .into_bytes(),
        ),
    );
    // The canonical intent binds every consumed preimage, including absent
    // destinations. A rewritten cache cannot invent different rollback bytes
    // while preserving this source-side authority.
    for path in edits.keys() {
        add(root, &mut paths, path)?;
    }
    let project = load
        .model
        .project
        .as_ref()
        .ok_or_else(|| fail("ownership-project"))?
        .id
        .clone();
    let before_pins: Vec<_> = paths
        .iter()
        .map(|(path, value)| Pin {
            path: path.clone(),
            digest: value.as_deref().map(mg::digest),
        })
        .collect();
    let intent = mg::hash(&(
        "ownership-intent/v1",
        &project,
        &request,
        &from,
        &before_pins,
        &candidate,
        &artifacts,
    ));
    state.assignments.retain(|row| {
        !(row.module == request.module
            && (row.symbol == request.symbol || request.symbol.is_none()))
    });
    state.assignments.push(Assignment {
        module: request.module.clone(),
        symbol: request.symbol.clone(),
        mode: request.to,
        transition: intent,
        artifacts: artifacts.clone(),
    });
    state
        .assignments
        .sort_by(|a, b| (&a.module, &a.symbol).cmp(&(&b.module, &b.symbol)));
    edits.insert(STATE.to_owned(), Some(bytes(&state)));
    let mut file_edits = Vec::new();
    for (path, content) in edits {
        add(root, &mut paths, &path)?;
        let before = paths.get(&path).and_then(|bytes| bytes.as_deref());
        if before == content.as_deref() {
            continue;
        }
        file_edits.push(Edit {
            path,
            before: before.map(mg::digest),
            after: content.as_deref().map(mg::digest),
            before_content: before
                .map(|bytes| {
                    std::str::from_utf8(bytes)
                        .map(str::to_owned)
                        .map_err(|_| fail("ownership-before-utf8"))
                })
                .transpose()?,
            content: content
                .map(|bytes| String::from_utf8(bytes).map_err(|_| fail("ownership-output-utf8")))
                .transpose()?,
        });
    }
    let semantic_diff = vec![SemanticChange {
        kind: "ownership.mode-changed".to_owned(),
        subject: request
            .symbol
            .clone()
            .unwrap_or_else(|| request.module.clone()),
        from,
        to: request.to,
    }];
    file_edits.sort_by(|a, b| a.path.cmp(&b.path));
    let mut plan = TransitionPlan {
        schema_version: format!("lekalo/ownership-transition-plan/v{VERSION}"),
        plan_digest: String::new(),
        project,
        request,
        from,
        inputs: paths
            .into_iter()
            .map(|(path, value)| Pin {
                path,
                digest: value.as_deref().map(mg::digest),
            })
            .collect(),
        baseline,
        candidate,
        edits: file_edits,
        claims: artifacts,
        semantic_diff,
    };
    plan.plan_digest = mg::plan_digest(&plan);
    if bytes(&plan).len() > MAX_BYTES / 2 {
        return Err(fail("ownership-plan-bound"));
    }
    Ok(plan)
}
