//! Shared candidate validation and sealing for source-derived changes.

use super::*;

pub(super) struct Base {
    pub root: PathBuf,
    pub project_id: String,
    pub base_compiled: ir::Compilation,
    pub error_source: Option<Vec<u8>>,
    pub policy: Policy,
}

pub(super) struct Sources {
    pub documents: Vec<LoadedDocument>,
    pub prospective: Vec<(String, Vec<u8>)>,
    pub creations: Vec<String>,
    pub resolved_rows: Vec<ResolvedOperation>,
    pub additional_pins: Vec<InputPin>,
}

pub(super) fn finish(
    load: &crate::loader::ProjectLoad,
    request: &ChangeRequest,
    options: &PrepareOptions<'_>,
    base: Base,
    sources: Sources,
) -> Result<PreparedChange, ChangeError> {
    let Base {
        root,
        project_id,
        base_compiled,
        error_source,
        policy,
    } = base;
    let Sources {
        documents,
        mut prospective,
        creations,
        resolved_rows,
        additional_pins,
    } = sources;
    // Canonical path order for every derived projection.
    prospective.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));

    // Rebuild the candidate from the re-decoded prospective bytes and
    // assert compiled-candidate equality with the tree-edited candidate.
    let mut candidate_documents: Vec<LoadedDocument> = Vec::with_capacity(documents.len());
    for document in &documents {
        match prospective
            .iter()
            .find(|(path, _)| path == &document.document.path)
        {
            Some((path, bytes)) => {
                let decoded =
                    crate::loader::decode_document_bytes(path, document.document.kind, bytes)
                        .map_err(|_| ChangeError::SourcePreservation {
                            subject: path.clone(),
                            detail: "reparse-mismatch",
                        })?;
                candidate_documents.push(LoadedDocument {
                    document: decoded,
                    bytes: bytes.clone(),
                });
            }
            None => candidate_documents.push(document.clone()),
        }
    }
    let tree_candidate = build_candidate(load, &documents)?;
    let byte_documents: Vec<crate::loader::Document> = candidate_documents
        .iter()
        .map(|document| document.document.clone())
        .collect();
    // A candidate that does not compile is request-controlled data: the
    // failure is a typed diagnostic (invalid), never a panic (issue review
    // finding 2 — every outcome is a DomainResult with a registered rule).
    let tree_compiled = ir::compile(&tree_candidate)
        .map_err(|failure| ChangeError::LoadFailed(failure.into_result()))?
        .project;
    // The typed candidate is valid; a decode/compile failure in the byte
    // rendering is a source-layout refusal, never invalid user intent.
    let preservation = || ChangeError::SourcePreservation {
        subject: "candidate".to_owned(),
        detail: "reparse-mismatch",
    };
    let byte_candidate =
        crate::loader::build_normalized_model(load.model.model_version, byte_documents)
            .map_err(|_| preservation())?;
    let byte_compiled = ir::compile(&byte_candidate)
        .map_err(|_| preservation())?
        .project;
    if tree_compiled.to_canonical_json() != byte_compiled.to_canonical_json() {
        return Err(ChangeError::SourcePreservation {
            subject: "candidate".to_owned(),
            detail: "reparse-mismatch",
        });
    }

    // Compile and validate the candidate across project boundaries.
    let candidate_compiled = ir::compile(&byte_candidate)
        .map_err(|failure| ChangeError::LoadFailed(failure.into_result()))?
        .project;
    let candidate_compilation = ir::Compilation {
        project: candidate_compiled.clone(),
        source_map: ir::SourceMap::new(Vec::new()),
    };
    let profile = ValidationProfile::embedded_default()
        .map_err(|_| ChangeError::RequestInvalid { detail: "profile" })?;
    if let Err(set) = crate::validator::validate(&candidate_compilation, profile) {
        return Err(ChangeError::LoadFailed(DomainResult::invalid(set)));
    }
    operation_error::validate_source(error_source.as_deref(), &candidate_compiled)?;

    // Semantic diff and compatibility against the unchanged base.
    let base_compiled = base_compiled.project;
    let diff = diff::compare(
        &base_compiled,
        &candidate_compiled,
        &diff::DiffRequest::new(),
    )
    .map_err(|set| ChangeError::LoadFailed(DomainResult::invalid(set)))?;
    let semantic_diff = SemanticDiffSummary {
        equal: diff.equal(),
        classification: diff
            .classification()
            .iter()
            .map(|class| class.key().to_owned())
            .collect(),
        changes: diff
            .changes()
            .iter()
            .map(|change| plan::ChangeRecordWire {
                change_id: change.change_id().to_owned(),
                kind: change.kind().key().to_owned(),
                subject: SubjectWire {
                    family: change.subject().family().key().to_owned(),
                    id: change.subject().id().to_owned(),
                    member: change.subject().member().map(str::to_owned),
                },
                reasons: change.reasons().to_vec(),
            })
            .collect(),
        complete: diff.complete(),
        complete_reason: diff.complete_reason().map(str::to_owned),
    };

    // Compatibility gate: breaking classes need policy, explicit
    // acknowledgement, or (for evidence classes) reviewed evidence.
    // Invalid candidates never reach this line. The acknowledgement flag
    // never authorizes data-loss or storage classes (round 2, finding 15):
    // those need a reviewed policy with migration evidence.
    let mut denied_breaking_classes = Vec::new();
    for class in &semantic_diff.classification {
        if !policy::BREAKING_CLASSES.contains(&class.as_str()) {
            continue;
        }
        let flag_authorizes = options.allow_breaking && !policy::requires_evidence(class);
        if !policy.allows(class) && !flag_authorizes {
            denied_breaking_classes.push(class.clone());
        }
    }
    denied_breaking_classes.sort();

    // Impact projection from the typed diff seeds on the matching graphs.
    let impact = project_impact(&base_compiled, &candidate_compiled, &diff);

    // Generation projection: a real plan (round 2, finding 13) — every
    // declared target pinned by its exact document bytes, the project
    // lock pinned when present, and explicit unknown states. Round 3
    // (findings 7 and 8): the evidence is strictly validated (lockfile
    // owner parse, closed target identity, locked-target custody, cap
    // propagation) and the exact pins are sealed for the apply-time
    // revalidation. Generation itself is never executed here.
    let (generation, generation_pins) = build_generation_plan(&root)?;

    // Complete input inventory with exact byte pins. The history sidecar
    // distinguishes absent from empty (round 2, finding 8): a present but
    // empty or unreadable ledger refuses before any mutation.
    let mut inputs: Vec<InputPin> = load
        .documents
        .iter()
        .map(|document| InputPin {
            path: document.document.path.clone(),
            digest: format!("sha256:{}", crate::digest::sha256_hex(&document.bytes)),
        })
        .collect();
    let history_path = version::HISTORY_LOGICAL_PATH.to_owned();
    let history_digest = match read_history_bytes(&root)? {
        None => absent_digest(),
        Some(bytes) => format!("sha256:{}", crate::digest::sha256_hex(&bytes)),
    };
    inputs.push(InputPin {
        path: history_path,
        digest: history_digest,
    });
    inputs.push(operation_error::input_pin(error_source.as_deref()));
    inputs.extend(additional_pins);
    inputs.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    // (r6, finding omp F1): the export obeys its own published schema —
    // a plan whose input inventory exceeds the contract's 1000-entry pin
    // cap cannot seal (the pins are the apply-time CAS authority, so no
    // truncated plan may exist). The bounded-output refusal is explicit,
    // never a schema-invalid document presented as complete.
    if inputs.len() > version::MAX_PLAN_INPUTS {
        return Err(ChangeError::PlanExportLimit {
            subject: "inputs",
            measured: inputs.len() as u64,
            allowed: version::MAX_PLAN_INPUTS as u64,
        });
    }

    // Optional exact Model precondition.
    let model_digest = format!(
        "sha256:{}",
        crate::digest::sha256_hex(
            inputs
                .iter()
                .map(|pin| format!("{}\u{1f}{}\n", pin.path, pin.digest))
                .collect::<String>()
                .as_bytes()
        )
    );
    if let Some(expected) = &request.expect_model {
        if expected != &model_digest {
            return Err(ChangeError::PreconditionFailed {
                subject: "model".to_owned(),
                detail: "expect-model",
            });
        }
    }

    let file_edits: Vec<FileEdit> = prospective
        .iter()
        .map(|(path, after)| {
            // A creation target has no pinned original: its before digest
            // is the absence sentinel (round 3, finding 11).
            let before: &[u8] = match load
                .documents
                .iter()
                .find(|document| &document.document.path == path)
            {
                Some(document) => &document.bytes,
                None => b"",
            };
            FileEdit {
                path: path.clone(),
                before_digest: format!("sha256:{}", crate::digest::sha256_hex(before)),
                after_digest: format!("sha256:{}", crate::digest::sha256_hex(after)),
            }
        })
        .collect();
    let mut file_diffs = Vec::new();
    let mut diff_complete = true;
    for (path, after_bytes) in &prospective {
        let before: Vec<u8> = match load
            .documents
            .iter()
            .find(|document| &document.document.path == path)
        {
            Some(document) => document.bytes.clone(),
            // A creation diff starts from the empty document (round 3,
            // finding 11).
            None => Vec::new(),
        };
        let before = std::str::from_utf8(&before).map_err(|_| ChangeError::SourcePreservation {
            subject: path.clone(),
            detail: "encoding",
        })?;
        let after =
            std::str::from_utf8(after_bytes).map_err(|_| ChangeError::SourcePreservation {
                subject: path.clone(),
                detail: "encoding",
            })?;
        if before.len() > version::MAX_FILE_DIFF_BYTES || after.len() > version::MAX_FILE_DIFF_BYTES
        {
            diff_complete = false;
            continue;
        }
        let (patch, patch_complete) = plan::compute_patch(before, after);
        if !patch_complete {
            diff_complete = false;
        }
        file_diffs.push(FileDiff {
            path: path.clone(),
            before: before.to_owned(),
            after: after.to_owned(),
            patch,
        });
    }
    file_diffs.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    // (r6, finding omp F1): the changed-file arrays obey the contract's
    // 1000-entry caps the same way — the research contract's proposed
    // "1,000 changed files" request bound. A sealed plan that cannot
    // represent its own mutation set does not exist.
    if file_edits.len() > version::MAX_PLAN_FILES || file_diffs.len() > version::MAX_PLAN_FILES {
        return Err(ChangeError::PlanExportLimit {
            subject: "fileEdits",
            measured: file_edits.len().max(file_diffs.len()) as u64,
            allowed: version::MAX_PLAN_FILES as u64,
        });
    }

    let prepared = PreparedChange {
        ownership: None,
        symbol_move: None,
        binding: None,
        module_move: None,
        // The canonical re-serialization pins the request intent, not its
        // presentation: pretty file JSON and compact sugar bytes of one
        // request carry the same digest and the same plan identity.
        request_bytes: request.to_canonical_json_string().into_bytes(),
        policy,
        operations: resolved_rows,
        inputs,
        file_edits,
        file_diffs,
        semantic_diff,
        impact,
        generation,
        denied_breaking_classes,
        mode: "preview",
        project_id,
        edits: prospective,
        creations,
        generation_pins,
        root,
        diff_complete,
        allow_breaking: options.allow_breaking,
    };
    if !prepared.denied_breaking_classes.is_empty() {
        return Err(ChangeError::CompatibilityDenied {
            classes: prepared.denied_breaking_classes.clone(),
        });
    }
    Ok(prepared)
}
