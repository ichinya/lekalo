//! Issue #94 canonical-source formatting: policy, trivia, rendering,
//! semantic verification, and the confined apply transaction.
//!
//! The formatter is a pure planning service over the accepted #7 loader
//! surface: it never serializes the compiled IR back into source, never
//! guesses semantics, and never writes outside the explicit apply
//! transaction. Every candidate is reparsed and compared through the real
//! loader/IR oracle before any disk mutation. Check and editor modes are
//! read-only on every outcome.

pub(crate) mod policy;
pub(crate) mod render;
pub(crate) mod semantic;
pub(crate) mod syntax;
pub(crate) mod transaction;
pub(crate) mod trivia;
pub mod wire;

pub use policy::Policy;

use crate::diagnostics::normalize::{build, source_location};
use crate::diagnostics::types::DataValue;
use crate::diagnostics::{DataObject, Diagnostic as Wire};
use crate::loader::frontends::Node;
use crate::loader::source::{LineIndex, Source};
use crate::loader::{diagnostic as loader_diagnostic, error::Diagnostic as RawDiagnostic};
use crate::loader::{DocKind, LoadSelection};
use crate::result::{DomainResult, Status, SuccessPayload};

use render::Syntax;

/// The exact embedded format-policy contract bytes.
pub const POLICY_BYTES: &[u8] = include_bytes!("../../../../contracts/format-policy.v0.8.0.json");

/// The formatter execution mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormatMode {
    /// Plan only; read-only on every outcome.
    Check,
    /// Plan and apply the verified candidates through the confined
    /// transaction.
    Apply,
    /// Editor mode: one in-memory buffer, full-document replacement text,
    /// no writes anywhere.
    Stdin,
}

/// The closed scope selector: the whole admitted Model source set, one
/// module directory, or one exact document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FormatScope {
    All,
    /// A module directory selector (`lekalo/modules/<dir>`).
    Module(String),
    /// An exact admitted document path.
    Document(String),
}

/// One formatter request: everything the pure service needs.
#[derive(Clone, Debug)]
pub struct FormatRequest {
    pub project: Option<String>,
    pub scope: FormatScope,
    pub mode: FormatMode,
    pub allow_lossy_comments: bool,
}

/// Run one format request to completion.
pub fn run(request: &FormatRequest, stdin_bytes: Option<&[u8]>) -> DomainResult {
    match plan(request, stdin_bytes) {
        Ok(planned) => dispatch(request, planned),
        Err(failure) => failure,
    }
}

/// A completed, verified plan: nothing has been written yet.
struct Planned {
    policy_digest: String,
    report: wire::Report,
    report_json: String,
    changed: Vec<wire::FileRow>,
    losses: Vec<wire::LossRow>,
    /// The verified replacement text in editor mode.
    stdin_replacement: Option<String>,
    /// Whether any enumerated loss rows exist (the stdin refusal data).
    stdin_losses: bool,
    /// The verified candidate bytes, parallel to `changed` rows.
    after_bytes: Vec<Vec<u8>>,
    /// The held ownership guard in apply mode (recovery + transaction).
    guard: std::cell::RefCell<Option<transaction::FormatGuard>>,
}

/// One rendered candidate pinned beside its source bytes.
struct RenderedFile {
    path: String,
    candidate: Vec<u8>,
    row: wire::FileRow,
    /// The loss rows with their exact source spans in the original
    /// buffer (finding r2 8): line/column and the byte range of the
    /// lost comment line, so every diagnostic coordinate agrees.
    losses: Vec<LossSpan>,
}

/// One enumerated comment loss with its exact span in the source text.
#[derive(Clone, Debug)]
struct LossSpan {
    line: usize,
    reason: String,
    start: crate::diagnostics::Position,
    end: crate::diagnostics::Position,
}

/// Load, pin, render, verify, and build the bounded report. No writes.
/// In apply mode the returned plan carries the held ownership guard; the
/// caller must drop it only after the transaction outcome is decided.
fn plan(request: &FormatRequest, stdin_bytes: Option<&[u8]>) -> Result<Planned, DomainResult> {
    let policy = Policy::embedded().map_err(|_policy_error| policy_failure())?;
    let selection = LoadSelection {
        project: request.project.clone(),
    };

    // Fail-closed recovery state: check and editor modes refuse; the
    // explicit mutating mode acquires the real ownership lock and
    // recovers to verified original bytes before its own load.
    let mut guard = if request.mode == FormatMode::Apply {
        match transaction::recover_pending(&selection) {
            Ok(guard) => guard,
            Err(transaction::RecoveryFailure::LiveWriter) => {
                return Err(transaction::ApplyFailure::Locked.into_result());
            }
            Err(transaction::RecoveryFailure::Conflict {
                path,
                expected,
                current,
            }) => {
                return Err(transaction::ApplyFailure::RecoverConflict {
                    path,
                    expected,
                    current,
                }
                .into_result());
            }
            Err(transaction::RecoveryFailure::Recovery(error)) => {
                return Err(failure_recovery(&error.plan_id, error.reason));
            }
        }
    } else if let Some(plan_id) = transaction::pending_plan(&selection) {
        return Err(failure_recovery(&plan_id, "active-transaction"));
    } else {
        None
    };

    // The load. Editor mode overlays the unsaved buffer onto its declared
    // document BEFORE semantic loading (finding r4 9): the buffer is the
    // authority for exactly that document, so a malformed saved copy —
    // the very state an editor repair targets — must not fail the load
    // before the buffer is formed. Every other document, the whole
    // inventory, and every filesystem/version boundary validate exactly
    // as in any other mode.
    let stdin_document: Option<(&str, &[u8])> = match (&request.mode, &request.scope) {
        (FormatMode::Stdin, FormatScope::Document(selector)) => Some((
            selector.as_str(),
            stdin_bytes.ok_or_else(|| failure_selection("document", "stdin-empty"))?,
        )),
        _ => None,
    };
    let loaded = match stdin_document {
        Some((overlay_path, overlay_bytes)) => {
            crate::loader::load_with_snapshot_and_overlay(&selection, overlay_path, overlay_bytes)
        }
        None => crate::loader::load_with_snapshot(&selection),
    };
    let load = match loaded {
        Ok(load) => {
            // The validated admitted inventory completes the legacy-temp
            // ownership proof (finding r5 3): quarantined temps whose
            // emitted documents are admitted are deleted; every other
            // byte is restored to its exact original location. Settling
            // happens before any planning decision, so no later refusal
            // can leave custody behind.
            if let Some(held) = guard.as_mut() {
                let admitted: Vec<String> = load
                    .documents
                    .iter()
                    .map(|document| document.document.path.clone())
                    .collect();
                if let Err(error) = transaction::settle_quarantine(held, &admitted) {
                    return Err(failure_recovery(&error.plan_id, error.reason));
                }
            }
            load
        }
        Err(failure) => {
            // Every refusal restores the quarantine first (finding r5 3):
            // a run that cannot prove admission never keeps user bytes
            // in custody, and the loader's own refusal — not deletion —
            // answers the tree.
            if let Some(held) = guard.as_mut() {
                transaction::restore_quarantine(held);
            }
            return Err(failure);
        }
    };
    let model_version = load.model.model_version;

    // Resolve the scope against the closed inventory. Stdin mode is bound
    // to its declared document path, and the unsaved buffer is the
    // original state the whole overlay is built from (finding G). The
    // public core refuses the Stdin pairing with any non-Document scope
    // (finding r2 13): the editor replacement is defined as the one
    // declared document, never an arbitrary file of a wider overlay.
    if request.mode == FormatMode::Stdin && !matches!(request.scope, FormatScope::Document(_)) {
        return Err(failure_selection("scope", "stdin-requires-document"));
    }
    let mut rendered: Vec<RenderedFile> = Vec::new();
    let selected: Vec<(&crate::loader::LoadedDocument, Option<Vec<u8>>)> = match &request.scope {
        FormatScope::All => load.documents.iter().map(|d| (d, None)).collect(),
        FormatScope::Document(selector) => {
            // A well-formed selector clearly outside the admitted Model
            // source family (lockfiles, runtime trees, docs, invented
            // names) is the registered unsupported-surface refusal
            // (finding J). A selector inside the family shape that names
            // nothing admitted stays the selection-miss refusal.
            if !admitted_model_family(selector) {
                return Err(failure_unsupported_surface(
                    "model-source",
                    "not-admitted-family",
                ));
            }
            let document = load
                .documents
                .iter()
                .find(|document| &document.document.path == selector)
                .ok_or_else(|| failure_selection("document", "not-admitted"))?;
            let override_bytes = if request.mode == FormatMode::Stdin {
                Some(
                    stdin_bytes
                        .ok_or_else(|| failure_selection("document", "stdin-empty"))?
                        .to_vec(),
                )
            } else {
                None
            };
            vec![(document, override_bytes)]
        }
        FormatScope::Module(selector) => {
            let prefix = module_prefix(selector)
                .ok_or_else(|| failure_selection("module", "selector-not-module-directory"))?;
            let matched: Vec<&crate::loader::LoadedDocument> = load
                .documents
                .iter()
                .filter(|document| document.document.path.starts_with(&prefix))
                .collect();
            if matched.is_empty() {
                return Err(failure_selection("module", "no-admitted-documents"));
            }
            matched.into_iter().map(|d| (d, None)).collect()
        }
    };

    if selected.len() > policy.limits.max_files {
        return Err(failure_limit(
            "max-files",
            selected.len() as u64,
            policy.limits.max_files as u64,
        ));
    }

    // Render every selected candidate; collect losses per file.
    let mut total_candidate_bytes = 0usize;
    for (document, override_bytes) in &selected {
        let source: &[u8] = override_bytes.as_deref().unwrap_or(&document.bytes);
        let candidate = render_one(
            &policy,
            &document.document.path,
            document.document.kind,
            source,
        )?;
        total_candidate_bytes += candidate.text.len();
        if total_candidate_bytes > policy.limits.max_candidate_total_bytes {
            return Err(failure_limit(
                "max-candidate-total-bytes",
                total_candidate_bytes as u64,
                policy.limits.max_candidate_total_bytes as u64,
            ));
        }
        let action = if candidate.text.as_bytes() == source {
            "unchanged"
        } else {
            "changed"
        };
        // The exact loss spans are derived from the original text via
        // the same LineIndex the loader parses with (finding r2 8): the
        // diagnostic range and the report row can never disagree.
        let text = Source::validate_encoding(&document.document.path, source)
            .map_err(|diagnostic| loader_diagnostic::failure(Status::Invalid, vec![diagnostic]))?;
        let index = LineIndex::new(&text);
        let loss_spans: Vec<LossSpan> = candidate
            .losses
            .iter()
            .map(|loss| {
                let (start, end) = loss_line_span(&text, &index, loss.line);
                LossSpan {
                    line: loss.line,
                    reason: loss.reason.as_str().to_owned(),
                    start,
                    end,
                }
            })
            .collect();
        rendered.push(RenderedFile {
            path: document.document.path.clone(),
            candidate: candidate.text.clone().into_bytes(),
            row: wire::FileRow {
                path: document.document.path.clone(),
                syntax: candidate.syntax,
                action: action.to_owned(),
                bytes_before: source.len(),
                bytes_after: candidate.text.len(),
                sha256_before: crate::digest::sha256_hex(source),
                sha256_after: crate::digest::sha256_hex(candidate.text.as_bytes()),
                equal: true,
                loss: candidate
                    .losses
                    .iter()
                    .map(|loss| wire::FileLoss {
                        line: loss.line,
                        reason: loss.reason.as_str().to_owned(),
                    })
                    .collect(),
                replacement: None,
            },
            losses: loss_spans,
        });
    }
    rendered.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));

    // Verify the whole candidate overlay through the real loader/IR oracle
    // before any report or write exists. In editor mode the overlay is
    // built over the unsaved buffer, so the oracle compares against the
    // buffer's state — not the stale saved file (finding G).
    let replacements: Vec<(String, Vec<u8>)> = rendered
        .iter()
        .filter(|file| file.row.action == "changed")
        .map(|file| (file.path.clone(), file.candidate.clone()))
        .collect();
    let original_state: Vec<crate::loader::LoadedDocument> = load
        .documents
        .iter()
        .map(|document| {
            let bytes = selected
                .iter()
                .find(|(pinned, _)| pinned.document.path == document.document.path)
                .and_then(|(_, override_bytes)| override_bytes.as_ref())
                .cloned()
                .unwrap_or_else(|| document.bytes.clone());
            crate::loader::LoadedDocument {
                document: document.document.clone(),
                bytes,
            }
        })
        .collect();
    semantic::verify(&original_state, model_version, &replacements)?;

    let rows: Vec<wire::FileRow> = rendered.iter().map(|file| file.row.clone()).collect();
    let losses: Vec<wire::LossRow> = rendered
        .iter()
        .flat_map(|file| {
            file.losses
                .iter()
                .map(|loss| wire::LossRow {
                    path: file.path.clone(),
                    line: loss.line,
                    reason: loss.reason.clone(),
                    start: loss.start,
                    end: loss.end,
                })
                .collect::<Vec<_>>()
        })
        .collect();
    if losses.len() > policy.limits.max_loss_rows {
        return Err(failure_limit(
            "max-loss-rows",
            losses.len() as u64,
            policy.limits.max_loss_rows as u64,
        ));
    }

    let changed: Vec<wire::FileRow> = rows
        .iter()
        .filter(|row| row.action == "changed")
        .cloned()
        .collect();
    let after_bytes: Vec<Vec<u8>> = rendered
        .iter()
        .filter(|file| file.row.action == "changed")
        .map(|file| file.candidate.clone())
        .collect();
    let stdin_replacement = if request.mode == FormatMode::Stdin {
        Some(String::from_utf8_lossy(&rendered[0].candidate).into_owned())
    } else {
        None
    };
    let stdin_losses = request.mode == FormatMode::Stdin && !losses.is_empty();
    let unchanged = rows.iter().filter(|row| !row.is_changed()).count();
    let report = wire::Report {
        schema_version: wire::SCHEMA_VERSION,
        identity: wire::IDENTITY,
        mode: match request.mode {
            FormatMode::Check => "check",
            FormatMode::Apply => "apply",
            FormatMode::Stdin => "stdin",
        },
        complete: true,
        policy: wire::PolicyRef {
            version: policy::Policy::VERSION.to_owned(),
            digest: policy.digest.clone(),
        },
        model: wire::VersionRef {
            version: model_version.as_str().to_owned(),
        },
        ir: wire::VersionRef {
            version: crate::ir::VERSION.to_owned(),
        },
        scope: wire::Scope {
            kind: scope_kind(&request.scope).to_owned(),
            selector: scope_selector(&request.scope),
        },
        files: rows,
        summary: wire::Summary {
            selected: selected.len(),
            changed: changed.len(),
            unchanged,
            applied: 0,
            loss_count: losses.len(),
        },
    };
    let report_json = report.to_json();
    Ok(Planned {
        policy_digest: policy.digest.clone(),
        report,
        report_json,
        changed,
        losses,
        stdin_replacement,
        stdin_losses,
        after_bytes,
        guard: std::cell::RefCell::new(guard),
    })
}

/// Project a verified plan onto its terminal envelope and (for apply) the
/// confined transaction. The apply arm holds the ownership guard across
/// the transaction; the transaction itself re-acquires it, so the lock
/// ownership window is one unbroken mutating run.
fn dispatch(request: &FormatRequest, planned: Planned) -> DomainResult {
    match request.mode {
        FormatMode::Check => {
            let mut diagnostics: Vec<Wire> = Vec::new();
            if !planned.changed.is_empty() {
                if let Some(diagnostic) = diff_diagnostic(&planned) {
                    diagnostics.push(diagnostic);
                }
            }
            push_loss_diagnostic(&mut diagnostics, &planned.losses, "check");
            if !planned.changed.is_empty() {
                let set = DomainResult::from_wire_set(Status::Denied, diagnostics);
                return DomainResult::denied_json(
                    planned.report_json.clone(),
                    human_check(&planned),
                    set,
                );
            }
            DomainResult::Valid {
                payload: SuccessPayload::Model {
                    json: wire::envelope("valid", &planned.report_json),
                    human: human_valid(&planned.report),
                },
                diagnostics: wire_set(Status::Valid, diagnostics),
            }
        }
        FormatMode::Apply => {
            // Comment loss blocks by default; the explicit opt-in carries
            // the loss rows as non-blocking warnings instead.
            if !planned.losses.is_empty() && !request.allow_lossy_comments {
                let mut diagnostics: Vec<Wire> = Vec::new();
                push_loss_diagnostic(&mut diagnostics, &planned.losses, "blocked");
                let set = DomainResult::from_wire_set(Status::Denied, diagnostics);
                return DomainResult::denied_json(
                    planned.report_json.clone(),
                    human_lossy(&planned),
                    set,
                );
            }
            if planned.changed.is_empty() {
                let mut diagnostics: Vec<Wire> = Vec::new();
                push_loss_diagnostic(&mut diagnostics, &planned.losses, "lossy");
                let mut report = planned.report.clone();
                report.summary.applied = 0;
                return DomainResult::Valid {
                    payload: SuccessPayload::Model {
                        json: wire::envelope("valid", &report.to_json()),
                        human: human_valid(&report),
                    },
                    diagnostics: wire_set(Status::Valid, diagnostics),
                };
            }
            let plan_files: Vec<(String, String, String)> = planned
                .changed
                .iter()
                .map(|row| {
                    (
                        row.path.clone(),
                        row.sha256_before.clone(),
                        row.sha256_after.clone(),
                    )
                })
                .collect();
            let after_bytes = planned.after_bytes.clone();
            let guard = planned.guard.borrow_mut().take();
            match transaction::apply(
                &request.project,
                &planned.policy_digest,
                &plan_files,
                &after_bytes,
                guard,
            ) {
                Ok(()) => {
                    let mut report = planned.report.clone();
                    report.summary.applied = planned.changed.len();
                    let mut diagnostics: Vec<Wire> = Vec::new();
                    push_loss_diagnostic(&mut diagnostics, &planned.losses, "lossy");
                    DomainResult::Valid {
                        payload: SuccessPayload::Model {
                            json: wire::envelope("valid", &report.to_json()),
                            human: human_valid(&report),
                        },
                        diagnostics: wire_set(Status::Valid, diagnostics),
                    }
                }
                Err(error) => error.into_result(),
            }
        }
        FormatMode::Stdin => {
            // Comment loss refuses the editor return outright (finding F):
            // the buffer is the only copy of the text, so the loss must
            // gate the replacement exactly like the disk path, never
            // exit 0 with a comment-dropped buffer. The refusal carries
            // the registered editor-mode identity `format.stdin-loss`
            // (finding r2 9) so editor consumers can distinguish a
            // buffer-loss refusal from a disk-mode loss denial.
            if planned.stdin_losses {
                let mut diagnostics: Vec<Wire> = Vec::new();
                push_stdin_loss_diagnostic(&mut diagnostics, &planned.losses);
                let set = DomainResult::from_wire_set(Status::Denied, diagnostics);
                // The refusal carries no report payload (finding r4 7):
                // the stdin report schema requires the row's replacement
                // text, which a loss refusal must withhold — so the only
                // schema-valid stdin report is the success one. The
                // refusal is answered by the registered diagnostic (exact
                // loss span and count) and the human projection.
                return DomainResult::denied(set);
            }
            // The single row carries the authorized replacement text; the
            // normal envelope owns status/diagnostics, and nothing is
            // written anywhere.
            let mut report = planned.report.clone();
            let replacement = planned.stdin_replacement.clone().unwrap_or_else(|| {
                planned
                    .changed
                    .first()
                    .map(|_| String::new())
                    .unwrap_or_default()
            });
            for row in &mut report.files {
                row.replacement = Some(replacement.clone());
            }
            DomainResult::Valid {
                payload: SuccessPayload::Model {
                    json: wire::envelope("valid", &report.to_json()),
                    // The human projection of editor mode is the formatted
                    // buffer text itself (the subprocess recipe).
                    human: replacement,
                },
                diagnostics: Vec::new(),
            }
        }
    }
}

/// The semantic equality oracle over one in-memory document set: decode
/// every document through the exact admission path, normalize through
/// phases 7-8, compile the IR, and reduce to the canonical Model bytes and
/// canonical IR bytes (the IR carries no source spans). Pure: no reads,
/// no writes, no network.
pub(crate) fn semantic_oracle(
    documents: &[crate::loader::LoadedDocument],
    model_version: crate::loader::ModelVersion,
) -> Result<(String, String), crate::result::DomainResult> {
    let mut decoded: Vec<crate::loader::Document> = Vec::with_capacity(documents.len());
    for document in documents {
        decoded.push(
            crate::loader::decode_document_bytes(
                &document.document.path,
                document.document.kind,
                &document.bytes,
            )
            .map_err(|diagnostics| loader_diagnostic::failure(Status::Invalid, diagnostics))?,
        );
    }
    let model = crate::loader::build_normalized_model(model_version, decoded)?;
    let model_bytes = crate::loader::canonical_model_bytes(&model);
    let compilation = crate::ir::compile(&model).map_err(|failure| failure.into_result())?;
    Ok((model_bytes, compilation.project.to_canonical_json()))
}

fn scope_kind(scope: &FormatScope) -> &'static str {
    match scope {
        FormatScope::All => "all",
        FormatScope::Module(_) => "module",
        FormatScope::Document(_) => "document",
    }
}

fn scope_selector(scope: &FormatScope) -> Option<String> {
    match scope {
        FormatScope::All => None,
        FormatScope::Module(selector) | FormatScope::Document(selector) => Some(selector.clone()),
    }
}

/// Whether one selector is inside the admitted Model source family's
/// shape: the project document, a module directory or manifest, or a
/// definition-kind document (finding J). A selector of this shape that
/// names nothing admitted is a selection miss; anything else is outside
/// the formatter surface entirely.
fn admitted_model_family(selector: &str) -> bool {
    // Uppercase, NFC, and other Unicode spellings are legal file names
    // outside the governed tree — they are simply not Model sources. The
    // selector-grammar gate stays authoritative only for actual escape
    // attempts (traversal, absolute, device, percent). Traversal etc.
    // refuse at the loader before this check, so they never reach here
    // in a real run; everything well-formed is judged by family.
    if crate::project_fs::selection_violation(selector).is_some() {
        // The loader owns the structure.* denial for escape attempts;
        // keep this seam's answer neutral (true) for them so the loader's
        // diagnostic wins.
        return true;
    }
    if selector == "lekalo/project.yaml" {
        return true;
    }
    // Other canonical `lekalo/` declaration homes (targets, transport,
    // attachments, types, ...) are governed project files of other
    // owners: a formatter request naming one is a selection miss inside
    // the governed tree, not a foreign surface.
    if let Some(rest) = selector.strip_prefix("lekalo/") {
        if crate::project_fs::CANONICAL_ROOT_ENTRIES
            .iter()
            .any(|entry| rest == *entry || rest.starts_with(&format!("{entry}/")))
        {
            return true;
        }
        // Any other path under the governed tree (invented `lekalo/...`)
        // also stays inside the project surface; a module-family path is
        // judged by its exact file-name grammar.
        if let Some(rest) = rest.strip_prefix("modules/") {
            let mut parts = rest.split('/');
            let directory = parts.next().unwrap_or_default();
            if directory.is_empty() {
                return false;
            }
            return match parts.next() {
                None => true,
                Some(file) => {
                    !file.is_empty()
                        && parts.next().is_none()
                        && (file == "module.yaml"
                            || crate::project_fs::KIND_STEMS
                                .iter()
                                .any(|stem| file == format!("{stem}.yaml")))
                }
            };
        }
        if !rest.is_empty() {
            return true;
        }
    }
    false
}

/// `lekalo/modules/<dir>` -> `lekalo/modules/<dir>/`; anything else is
/// not a module-directory selector.
fn module_prefix(selector: &str) -> Option<String> {
    let mut parts = selector.split('/');
    if parts.next() != Some("lekalo") || parts.next() != Some("modules") {
        return None;
    }
    let directory = parts.next()?;
    if parts.next().is_some() || directory.is_empty() {
        return None;
    }
    if crate::project_fs::path_violation(selector).is_some() {
        return None;
    }
    Some(format!("lekalo/modules/{directory}/"))
}

/// The exact span of one 1-based source line, clamped to the text
/// (finding r2 8): start = the line's first byte, end = one past its
/// last content byte (EOL/CR excluded), both mapped through the loader's
/// `LineIndex` so byte, line, and column always agree.
fn loss_line_span(
    text: &str,
    index: &LineIndex,
    line: usize,
) -> (crate::diagnostics::Position, crate::diagnostics::Position) {
    let total_lines =
        text.matches('\n').count() + usize::from(!text.is_empty() && !text.ends_with('\n'));
    let line = line.max(1);
    let line_starts: Vec<usize> = std::iter::once(0usize)
        .chain(
            text.as_bytes()
                .iter()
                .enumerate()
                .filter(|(_, byte)| **byte == b'\n')
                .map(|(position, _)| position + 1),
        )
        .collect();
    let start_byte = line_starts.get(line - 1).copied().unwrap_or(text.len());
    let end_byte = if line < total_lines {
        line_starts.get(line).copied().unwrap_or(text.len())
    } else {
        text.len()
    };
    // Exclude the line break itself from the span end.
    let mut end_byte = end_byte.min(text.len());
    if end_byte > start_byte && text.as_bytes()[end_byte - 1] == b'\n' {
        end_byte -= 1;
        if end_byte > start_byte && text.as_bytes()[end_byte - 1] == b'\r' {
            end_byte -= 1;
        }
    }
    let start = index.position(text, start_byte);
    let end = index.position(text, end_byte);
    (
        crate::diagnostics::Position {
            byte: start.byte,
            line: start.line,
            column: start.column,
        },
        crate::diagnostics::Position {
            byte: end.byte,
            line: end.line,
            column: end.column,
        },
    )
}

/// One admitted document -> canonical candidate text plus its loss rows.
/// The exact loader admission path runs first: encoding gate, strict
/// parse, document shape, and version extraction. A parse error stays a
/// parse error (invalid, exit 1) — never a format diff.
fn render_one(
    policy: &Policy,
    path: &str,
    kind: DocKind,
    bytes: &[u8],
) -> Result<render::Rendered, DomainResult> {
    if bytes.len() > policy.limits.max_document_bytes {
        return Err(failure_limit(
            "max-document-bytes",
            bytes.len() as u64,
            policy.limits.max_document_bytes as u64,
        ));
    }
    crate::loader::decode_document_bytes(path, kind, bytes)
        .map_err(|diagnostics| loader_diagnostic::failure(Status::Invalid, diagnostics))?;
    let text = Source::validate_encoding(path, bytes)
        .map_err(|diagnostic| loader_diagnostic::failure(Status::Invalid, vec![diagnostic]))?;
    let index = LineIndex::new(&text);
    let parsed = crate::loader::frontends::parse_document(&text, &index)
        .map_err(|diagnostics| loader_diagnostic::failure(Status::Invalid, diagnostics))?;
    let root: &Node = match &parsed {
        crate::loader::frontends::Parsed::Root(root) => root,
        crate::loader::frontends::Parsed::Empty => {
            return Err(loader_diagnostic::failure(
                Status::Invalid,
                vec![RawDiagnostic::new("loader.document-shape")
                    .with_path(path)
                    .with_data(serde_json::json!({ "detail": "empty-document" }))],
            ))
        }
    };
    let syntax = Syntax::of(&text, root);
    render::render(policy, root, syntax, &text)
}

fn policy_failure() -> DomainResult {
    let diagnostics = build(
        crate::result::REGISTRY_INVALID,
        None,
        None,
        DataObject::new(),
    )
    .map(|diagnostic| vec![diagnostic])
    .unwrap_or_default();
    let set = DomainResult::from_wire_set(Status::Invalid, diagnostics);
    DomainResult::invalid(set)
}

fn wire_set(status: Status, diagnostics: Vec<Wire>) -> Vec<Wire> {
    DomainResult::from_wire_set(status, diagnostics)
        .as_slice()
        .to_vec()
}

fn failure_selection(selector_kind: &str, detail: &str) -> DomainResult {
    let mut data = DataObject::new();
    data.insert(
        "selectorKind".to_owned(),
        DataValue::Token(selector_kind.to_owned()),
    );
    data.insert("detail".to_owned(), DataValue::Token(detail.to_owned()));
    let diagnostics = build("format.selection-invalid", None, None, data)
        .map(|diagnostic| vec![diagnostic])
        .unwrap_or_default();
    let set = DomainResult::from_wire_set(Status::Invalid, diagnostics);
    DomainResult::invalid(set)
}

/// The registered `format.unsupported-surface` refusal (exit 4): the
/// declared document is a well-formed path outside the admitted Model
/// source family — never a formatter input, in any mode.
fn failure_unsupported_surface(family: &str, reason: &str) -> DomainResult {
    let mut data = DataObject::new();
    data.insert("family".to_owned(), DataValue::Token(family.to_owned()));
    data.insert("reason".to_owned(), DataValue::Token(reason.to_owned()));
    let diagnostics = build("format.unsupported-surface", None, None, data)
        .map(|diagnostic| vec![diagnostic])
        .unwrap_or_default();
    let set = DomainResult::from_wire_set(Status::Unsupported, diagnostics);
    DomainResult::UnsupportedOperation { diagnostics: set }
}

fn failure_limit(limit: &str, measured: u64, allowed: u64) -> DomainResult {
    let mut data = DataObject::new();
    data.insert("limit".to_owned(), DataValue::Token(limit.to_owned()));
    data.insert("measured".to_owned(), DataValue::Count(measured));
    data.insert("allowed".to_owned(), DataValue::Count(allowed));
    let diagnostics = build("format.limit-exceeded", None, None, data)
        .map(|diagnostic| vec![diagnostic])
        .unwrap_or_default();
    let set = DomainResult::from_wire_set(Status::Invalid, diagnostics);
    DomainResult::invalid(set)
}

fn failure_semantic(stage: &str, reason: &str) -> DomainResult {
    let mut data = DataObject::new();
    data.insert("stage".to_owned(), DataValue::Token(stage.to_owned()));
    data.insert("reason".to_owned(), DataValue::Token(reason.to_owned()));
    let diagnostics = build("format.semantic-change", None, None, data)
        .map(|diagnostic| vec![diagnostic])
        .unwrap_or_default();
    let set = DomainResult::from_wire_set(Status::Invalid, diagnostics);
    DomainResult::invalid(set)
}

fn failure_recovery(plan_id: &str, reason: &str) -> DomainResult {
    let mut data = DataObject::new();
    let bounded_plan: String = plan_id.chars().take(64).collect();
    data.insert("planId".to_owned(), DataValue::Token(bounded_plan));
    data.insert("reason".to_owned(), DataValue::Token(reason.to_owned()));
    let diagnostics = build("format.recovery-required", None, None, data)
        .map(|diagnostic| vec![diagnostic])
        .unwrap_or_default();
    let set = DomainResult::from_wire_set(Status::Invalid, diagnostics);
    DomainResult::invalid(set)
}

fn diff_diagnostic(planned: &Planned) -> Option<Wire> {
    let mut data = DataObject::new();
    data.insert(
        "policyDigest".to_owned(),
        DataValue::Token(planned.policy_digest.clone()),
    );
    data.insert(
        "changedFiles".to_owned(),
        DataValue::Count(planned.changed.len() as u64),
    );
    build("format.diff", None, None, data).ok()
}

fn push_loss_diagnostic(diagnostics: &mut Vec<Wire>, losses: &[wire::LossRow], disposition: &str) {
    let Some(first) = losses.first() else {
        return;
    };
    let mut data = DataObject::new();
    data.insert("losses".to_owned(), DataValue::Count(losses.len() as u64));
    data.insert(
        "disposition".to_owned(),
        DataValue::Token(disposition.to_owned()),
    );
    // One bounded location (the first loss row); the full inventory lives
    // in the report, never in unbounded diagnostics. Raw comment text
    // never enters the diagnostic. The range is the loss row's exact
    // span (finding r2 8): byte offsets and line/column are derived from
    // the same LineIndex, so every client projection agrees.
    let source = source_location(
        Some(first.path.clone()),
        Some(crate::diagnostics::Range {
            start: first.start,
            end: first.end,
        }),
    );
    if let Ok(diagnostic) = build("format.comments-loss", None, source, data) {
        diagnostics.push(diagnostic);
    }
}

/// The registered `format.stdin-loss` producer (LEK-FORMAT-011, finding
/// r2 9): editor mode's refusal of a comment-losing buffer. The buffer
/// is the only copy of the text, so the loss is reported with its exact
/// span and the replacement is withheld — distinct from the disk-path
/// `format.comments-loss` so editor consumers can tell the two apart.
fn push_stdin_loss_diagnostic(diagnostics: &mut Vec<Wire>, losses: &[wire::LossRow]) {
    let Some(first) = losses.first() else {
        return;
    };
    let mut data = DataObject::new();
    data.insert("losses".to_owned(), DataValue::Count(losses.len() as u64));
    data.insert(
        "disposition".to_owned(),
        DataValue::Token("blocked".to_owned()),
    );
    // The registered `span` requirement: the exact first-loss span from
    // the unsaved buffer (finding r2 8).
    let source = source_location(
        Some(first.path.clone()),
        Some(crate::diagnostics::Range {
            start: first.start,
            end: first.end,
        }),
    );
    if let Ok(diagnostic) = build("format.stdin-loss", None, source, data) {
        diagnostics.push(diagnostic);
    }
}

fn human_valid(report: &wire::Report) -> String {
    let mut lines = vec![format!(
        "formatted {} files: {} changed, {} unchanged, {} applied",
        report.summary.selected,
        report.summary.changed,
        report.summary.unchanged,
        report.summary.applied
    )];
    for row in report.files.iter().filter(|row| row.is_changed()) {
        lines.push(format!("changed {}", row.path));
    }
    lines.join("\n")
}

fn human_check(planned: &Planned) -> String {
    let mut lines = vec![format!(
        "{} file(s) require canonical formatting",
        planned.changed.len()
    )];
    for row in &planned.changed {
        lines.push(format!("changed {}", row.path));
    }
    for loss in &planned.losses {
        lines.push(format!(
            "comment loss {}:{} ({})",
            loss.path, loss.line, loss.reason
        ));
    }
    lines.join("\n")
}

fn human_lossy(planned: &Planned) -> String {
    let mut lines = vec![format!(
        "comment loss blocks formatting: {} loss row(s); re-run with --allow-lossy-comments to accept",
        planned.losses.len()
    )];
    for loss in &planned.losses {
        lines.push(format!(
            "comment loss {}:{} ({})",
            loss.path, loss.line, loss.reason
        ));
    }
    lines.join("\n")
}
