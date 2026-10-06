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
    /// The verified candidate bytes, parallel to `changed` rows.
    after_bytes: Vec<Vec<u8>>,
}

/// One rendered candidate pinned beside its source bytes.
struct RenderedFile {
    path: String,
    candidate: Vec<u8>,
    row: wire::FileRow,
}

/// Load, pin, render, verify, and build the bounded report. No writes.
fn plan(request: &FormatRequest, stdin_bytes: Option<&[u8]>) -> Result<Planned, DomainResult> {
    let policy = Policy::embedded().map_err(|_policy_error| policy_failure())?;
    let selection = LoadSelection {
        project: request.project.clone(),
    };

    // Fail-closed recovery state: check and editor modes refuse; the
    // explicit mutating mode recovers to verified original bytes first.
    if request.mode != FormatMode::Apply {
        if let Some(plan_id) = transaction::pending_plan(&selection) {
            return Err(failure_recovery(&plan_id, "active-transaction"));
        }
    } else if let Err(error) = transaction::recover_pending(&selection) {
        return Err(failure_recovery(&error.plan_id, error.reason));
    }

    let load = crate::loader::load_with_snapshot(&selection)?;
    let model_version = load.model.model_version;

    // Resolve the scope against the closed inventory. Stdin mode is bound
    // to its declared document path.
    let mut rendered: Vec<RenderedFile> = Vec::new();
    let selected: Vec<(&crate::loader::LoadedDocument, Option<Vec<u8>>)> = match &request.scope {
        FormatScope::All => load.documents.iter().map(|d| (d, None)).collect(),
        FormatScope::Document(selector) => {
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
                    .into_iter()
                    .map(|loss| wire::FileLoss {
                        line: loss.line,
                        reason: loss.reason.as_str().to_owned(),
                    })
                    .collect(),
                replacement: None,
            },
        });
    }
    rendered.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));

    // Verify the whole candidate overlay through the real loader/IR oracle
    // before any report or write exists.
    let replacements: Vec<(String, Vec<u8>)> = rendered
        .iter()
        .filter(|file| file.row.action == "changed")
        .map(|file| (file.path.clone(), file.candidate.clone()))
        .collect();
    semantic::verify(&load.documents, model_version, &replacements)?;

    let rows: Vec<wire::FileRow> = rendered.iter().map(|file| file.row.clone()).collect();
    let losses: Vec<wire::LossRow> = rows
        .iter()
        .flat_map(|row| {
            row.loss
                .iter()
                .map(|loss| wire::LossRow {
                    path: row.path.clone(),
                    line: loss.line,
                    reason: loss.reason.clone(),
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
    let report = wire::Report {
        mode: match request.mode {
            FormatMode::Check => "check",
            FormatMode::Apply => "apply",
            FormatMode::Stdin => "stdin",
        },
        policy_version: policy::Policy::VERSION.to_owned(),
        policy_digest: policy.digest.clone(),
        ir_version: crate::ir::VERSION.to_owned(),
        model_version: model_version.as_str().to_owned(),
        scope_kind: scope_kind(&request.scope).to_owned(),
        scope_selector: scope_selector(&request.scope),
        rows,
        selected: selected.len(),
        changed: changed.len(),
        applied: 0,
    };
    let report_json = report.to_json();
    Ok(Planned {
        policy_digest: policy.digest.clone(),
        report,
        report_json,
        changed,
        losses,
        stdin_replacement,
        after_bytes,
    })
}

/// Project a verified plan onto its terminal envelope and (for apply) the
/// confined transaction.
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
                report.applied = 0;
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
            match transaction::apply(
                &request.project,
                &planned.policy_digest,
                &plan_files,
                &after_bytes,
            ) {
                Ok(()) => {
                    let mut report = planned.report.clone();
                    report.applied = planned.changed.len();
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
            for row in &mut report.rows {
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
    // never enters the diagnostic.
    let position = crate::diagnostics::Position {
        byte: 0,
        line: first.line,
        column: 1,
    };
    let source = source_location(
        Some(first.path.clone()),
        Some(crate::diagnostics::Range {
            start: position,
            end: position,
        }),
    );
    if let Ok(diagnostic) = build("format.comments-loss", None, source, data) {
        diagnostics.push(diagnostic);
    }
}

fn human_valid(report: &wire::Report) -> String {
    let mut lines = vec![format!(
        "formatted {} files: {} changed, {} unchanged, {} applied",
        report.selected,
        report.changed,
        report.unchanged_count(),
        report.applied
    )];
    for row in report.rows.iter().filter(|row| row.action == "changed") {
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
