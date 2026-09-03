//! The one real Model migration step: `0.1.0` -> `1.0.0` (issues #6 + #9).
//!
//! Automatic migration is legal exactly when every existing ID already
//! satisfies the Model 1.0.0 grammar, reservation, and qualification rules
//! (the `docs/model-migration-0.1.0-to-1.0.0.md` preconditions). The step
//! never guesses: it case-folds nothing, translates nothing, truncates
//! nothing, and never invents rename history. When the preconditions fail,
//! planning aborts with `versioning.migration-precondition` and zero bytes
//! have changed; the owner authors the semantic edits.
//!
//! When the preconditions hold, the transformation is the schema-version
//! token rewrite in every present Model document: only the parsed
//! `schema_version` value bytes change, preserving comments, quoting style,
//! whitespace, CRLF/LF breaks, multibyte content, and the final-newline
//! state of every document.

use crate::ir::grammar::is_segment;
use crate::loader::{ModelVersion, NormalizedModel};

use super::graph::MigrationStep;
use super::graph::{
    NotMigratableDetail, PreconditionRule, PreconditionViolation, StepFailure,
    MODEL_STEP_0_1_0_TO_1_0_0,
};
use super::plan::DocumentSnapshot;
use super::registry::ChangeClassification;

/// The exact source token this step rewrites.
const FROM_TOKEN: &str = "0.1.0";
/// The exact target token written in its place.
const TO_TOKEN: &str = "1.0.0";

/// Words reserved in both the project and module ID classes.
const RESERVED: [&str; 2] = ["lekalo", "dev"];

/// The compiled `model-0.1.0-to-1.0.0@1` step.
pub(crate) struct ModelV0_1_0ToV1_0_0;

impl MigrationStep for ModelV0_1_0ToV1_0_0 {
    fn id(&self) -> &'static str {
        MODEL_STEP_0_1_0_TO_1_0_0
    }

    fn from(&self) -> ModelVersion {
        ModelVersion::V0_1_0
    }

    fn to(&self) -> ModelVersion {
        ModelVersion::V1_0_0
    }

    fn classification(&self) -> ChangeClassification {
        ChangeClassification::Breaking
    }

    fn loss(&self) -> &'static [&'static str] {
        // The rewrite touches only the version token bytes; comments,
        // quoting, and formatting are preserved exactly, so the honest loss
        // list is empty.
        &[]
    }

    fn preflight(&self, model: &NormalizedModel) -> Result<(), StepFailure> {
        let mut violations: Vec<PreconditionViolation> = Vec::new();

        // 1. Project ID: one legal 1.0.0 segment, not reserved.
        if let Some(project) = &model.project {
            check_segment(
                &project.id,
                PreconditionRule::ProjectGrammar,
                PreconditionRule::Reserved,
                &mut violations,
            );
        }

        // 2. Module IDs: one legal 1.0.0 segment, not reserved.
        // 5. Imports: legal 1.0.0 module IDs.
        for module in &model.modules {
            check_segment(
                &module.id,
                PreconditionRule::ModuleGrammar,
                PreconditionRule::Reserved,
                &mut violations,
            );
            if let Some(imports) = module.node.get("imports").and_then(|node| node.as_seq()) {
                for import in imports {
                    match import.as_str() {
                        Some(text) if is_segment(text) && !RESERVED.contains(&text) => {}
                        Some(text) => violations.push(PreconditionViolation {
                            id: text.to_owned(),
                            rule: PreconditionRule::ImportGrammar,
                        }),
                        None => violations.push(PreconditionViolation {
                            id: "<non-string-import>".to_owned(),
                            rule: PreconditionRule::ImportGrammar,
                        }),
                    }
                }
            }
        }

        // 3. Symbol IDs: exactly two legal 1.0.0 segments whose first
        //    segment equals the declared module ID. 0.1.0 IDs cannot carry
        //    the optional kind-namespace segment, so three parts already
        //    fail here. Reference liveness was enforced by the loader.
        let declared_modules: std::collections::BTreeSet<&str> = model
            .modules
            .iter()
            .map(|module| module.id.as_str())
            .collect();
        for definition in &model.definitions {
            let id = definition.id.as_str();
            let parts: Vec<&str> = id.split('.').collect();
            let mut ok = parts.len() == 2;
            if ok {
                ok = parts.iter().all(|part| is_segment(part));
            }
            if ok {
                ok = declared_modules.contains(parts[0]);
            }
            if !ok {
                violations.push(PreconditionViolation {
                    id: id.to_owned(),
                    rule: if parts.len() != 2 {
                        PreconditionRule::SymbolGrammar
                    } else if parts.iter().all(|part| is_segment(part)) {
                        PreconditionRule::Qualification
                    } else {
                        PreconditionRule::SymbolGrammar
                    },
                });
            }
        }

        if violations.is_empty() {
            Ok(())
        } else {
            violations.sort_by(|left, right| {
                left.id
                    .as_bytes()
                    .cmp(right.id.as_bytes())
                    .then_with(|| left.rule.as_str().cmp(right.rule.as_str()))
            });
            violations.dedup();
            Err(StepFailure::Precondition { violations })
        }
    }

    fn transform(&self, document: &DocumentSnapshot) -> Result<Vec<u8>, StepFailure> {
        if document.version != FROM_TOKEN {
            return Err(StepFailure::NotFileMigratable {
                path: document.path.clone(),
                detail: NotMigratableDetail::UnexpectedSourceVersion,
            });
        }
        let start = document.version_span.start.byte;
        let end = document.version_span.end.byte;
        if start > end || end > document.bytes.len() {
            return Err(StepFailure::NotFileMigratable {
                path: document.path.clone(),
                detail: NotMigratableDetail::TokenNotUnique,
            });
        }
        let window = &document.bytes[start..end];
        let token = FROM_TOKEN.as_bytes();
        let positions: Vec<usize> = window
            .windows(token.len())
            .enumerate()
            .filter(|(_, slice)| *slice == token)
            .map(|(position, _)| position)
            .collect();
        if positions.len() != 1 {
            return Err(StepFailure::NotFileMigratable {
                path: document.path.clone(),
                detail: NotMigratableDetail::TokenNotUnique,
            });
        }
        let position = start + positions[0];
        let mut after = Vec::with_capacity(document.bytes.len() - token.len() + TO_TOKEN.len());
        after.extend_from_slice(&document.bytes[..position]);
        after.extend_from_slice(TO_TOKEN.as_bytes());
        after.extend_from_slice(&document.bytes[position + token.len()..]);

        // The rewritten document must re-parse with the target version.
        let decoded = crate::loader::decode_document_bytes(&document.path, document.kind, &after);
        match decoded {
            Ok(parsed) if parsed.version == TO_TOKEN => Ok(after),
            Ok(_) => Err(StepFailure::NotFileMigratable {
                path: document.path.clone(),
                detail: NotMigratableDetail::ReparseFailed,
            }),
            Err(_) => Err(StepFailure::NotFileMigratable {
                path: document.path.clone(),
                detail: NotMigratableDetail::ReparseFailed,
            }),
        }
    }
}

/// Check one ID against the 1.0.0 segment grammar and the reserved set,
/// recording the most specific violation.
fn check_segment(
    id: &str,
    grammar_rule: PreconditionRule,
    reserved_rule: PreconditionRule,
    violations: &mut Vec<PreconditionViolation>,
) {
    if RESERVED.contains(&id) {
        violations.push(PreconditionViolation {
            id: id.to_owned(),
            rule: reserved_rule,
        });
        return;
    }
    if !is_segment(id) {
        violations.push(PreconditionViolation {
            id: id.to_owned(),
            rule: grammar_rule,
        });
    }
}
