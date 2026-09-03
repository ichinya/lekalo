//! Typed IR diagnostics and the terminal failure (issue #8).
//!
//! The closed code registry, the diagnostic wire shape, the 100-diagnostic
//! phase bound, and the failure envelope are shared with the accepted #7
//! loader contract; IR codes live in the `ir.` namespace so no code is
//! duplicated across owners. Free-form operating-system messages, absolute
//! paths, and timestamps never appear in a diagnostic.

use crate::loader::error::{
    bounded_import_echo, finalize_diagnostics, Diagnostic, LoadStatus, MAX_PHASE_DIAGNOSTICS,
};
use crate::loader::{failure_envelope, LoadOutput};

/// A key was present that the closed schema for this node does not accept.
pub const UNKNOWN_FIELD: &str = "ir.unknown-field";
/// A schema-required key was absent.
pub const MISSING_FIELD: &str = "ir.missing-field";
/// A definition kind appeared in a document home that does not accept it.
pub const KIND_PLACEMENT: &str = "ir.kind-placement";
/// A `kind` value outside the closed 14-kind vocabulary.
pub const KIND_UNKNOWN: &str = "ir.kind-unknown";
/// A present value violated its grammar, closed enum, cardinality, depth, or
/// length rule; `data.detail` names the rule.
pub const VALUE_INVALID: &str = "ir.value-invalid";
/// A set-like array declared the same member twice.
pub const DUPLICATE_MEMBER: &str = "ir.duplicate-member";

/// The terminal result of compiling one normalized model into typed IR.
///
/// Any diagnostic makes the whole compilation fail: the IR is either fully
/// typed or absent. Diagnostics are finalized (sorted, bounded) exactly like
#[derive(Debug)]
pub struct IrFailure {
    pub diagnostics: Vec<Diagnostic>,
}

impl IrFailure {
    pub(crate) fn new(mut diagnostics: Vec<Diagnostic>) -> Self {
        diagnostics = finalize_diagnostics(diagnostics);
        Self { diagnostics }
    }

    /// True once the bound stopped further collection; callers stop decoding.
    pub(crate) fn is_full(diagnostics: &[Diagnostic]) -> bool {
        diagnostics.len() >= MAX_PHASE_DIAGNOSTICS
    }

    /// The CLI `load` outcome for an IR failure: `invalid`, exit 1, stderr,
    /// with the same pretty envelope the loader uses.
    pub fn load_output(&self) -> LoadOutput {
        let codes = self
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        LoadOutput {
            status: LoadStatus::Invalid,
            json: failure_envelope(LoadStatus::Invalid, &self.diagnostics),
            human: format!("{}: {}", LoadStatus::Invalid.as_str(), codes),
        }
    }
}

/// Echo a field key inside diagnostic `data.field`: grammar-legal keys are
/// short, but hostile keys arrive through parsed documents, so the echo is
/// bounded exactly like import echoes.
pub(crate) fn bounded_field_echo(field: &str) -> std::borrow::Cow<'_, str> {
    bounded_import_echo(field)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_envelope_matches_loader_shape_and_binds_to_exit_one() {
        let failure = IrFailure::new(vec![
            Diagnostic::new(VALUE_INVALID)
                .with_path("lekalo/modules/planner/entities.yaml")
                .with_data(serde_json::json!({ "detail": "version-integer", "field": "version" })),
            Diagnostic::new(UNKNOWN_FIELD)
                .with_data(serde_json::json!({ "field": bounded_field_echo("mystery") })),
        ]);
        let output = failure.load_output();
        assert_eq!(output.status, LoadStatus::Invalid);
        assert_eq!(output.status.exit_code(), 1);
        assert!(output.status.writes_stderr());
        assert!(output
            .json
            .starts_with("{\n  \"status\": \"invalid\",\n  \"reasonCodes\": ["));
        assert!(output.json.contains("\"code\":\"ir.unknown-field\""));
        assert!(output.json.contains("\"code\":\"ir.value-invalid\""));
        assert!(output.json.ends_with("]\n}\n"));
        assert_eq!(output.human, "invalid: ir.unknown-field, ir.value-invalid");
    }

    #[test]
    fn hostile_field_keys_are_bounded_in_echoes() {
        let hostile = "x".repeat(10_000);
        let echo = bounded_field_echo(&hostile);
        assert!(echo.chars().count() <= 65);
        assert!(echo.ends_with('…'));
    }
}
