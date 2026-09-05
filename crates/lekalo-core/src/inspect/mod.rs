//! Issue #15: the deterministic single-symbol inspect projection.
//!
//! `lekalo inspect` answers one question — what exactly is this symbol
//! and what does it touch — for humans and AI consumers, without
//! reading the whole repository. The projection is read-only and
//! consumes exactly the accepted typed surfaces: the #8 IR (identity,
//! contracts, invariants, policies, scenarios, bindings), the #13
//! dependency graph (direct dependencies and dependents), and the #14
//! effect graph (declared and detected effects). It never parses
//! source bytes, never reads target files, never re-scans or rebuilds
//! indexes, and never writes anywhere.
//!
//! Resolution is safe by construction: the selector is grammar-validated
//! before any project discovery and is never handed to a filesystem
//! API; a full semantic id resolves case-sensitively through the
//! canonical definition index (a miss may consult the accepted #6
//! alias registry, never fuzzy matching); a one-segment token is a
//! safe short name whose zero-match and many-match outcomes are the
//! distinct stable diagnostics `inspect.short-name-unknown` and
//! `inspect.short-name-ambiguous`. Ambiguity always requires
//! disambiguation; there is no first-match selection.
//!
//! Every mandatory section is present in every result — explicit
//! `empty`/`unknown`/`unsupported`/`truncated` states with reasons and
//! returned/omitted counts replace silent omission. Human and JSON are
//! two projections of one normalized object. Outputs are
//! deterministic: fixed wire order, byte-sorted set-like arrays, no
//! locale/time/cwd dependence, byte-identical reruns.

pub mod canonical;
pub mod diagnostic;
pub mod limits;
pub mod result;
pub mod sections;
pub mod selector;
pub mod version;

use crate::diagnostics::DiagnosticSet;
use crate::ir::Compilation;

pub use limits::InspectLimits;
pub use result::InspectOutcome;
pub use selector::SelectorMode;

/// The closed `--include` selection of the optional projections.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Include {
    /// Project the declared target bindings of the symbol's module.
    pub bindings: bool,
    /// Project the Model scenarios that cover the symbol.
    pub scenarios: bool,
}

impl Include {
    /// Parse the comma-separated include list; the closed vocabulary is
    /// `bindings` and `scenarios`. `None` marks empty input and unknown
    /// or repeated tokens (the caller renders `cli.usage`).
    pub fn parse(text: &str) -> Option<Self> {
        if text.is_empty() {
            return None;
        }
        let mut include = Self::default();
        for token in text.split(',') {
            match token {
                "bindings" if !include.bindings => include.bindings = true,
                "scenarios" if !include.scenarios => include.scenarios = true,
                _ => return None,
            }
        }
        Some(include)
    }
}

/// One inspect invocation: the raw selector plus the include selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectRequest {
    /// The raw selector exactly as supplied by the caller.
    pub selector: String,
    /// The optional-projection selection.
    pub include: Include,
}

/// Validate one raw selector against the invocation grammar without
/// touching any project: CLI callers run this before project discovery
/// so a malformed selector is a pure usage failure.
pub fn validate_selector(raw: &str) -> Result<(), DiagnosticSet> {
    selector::classify(raw).map(|_| ())
}

/// Run one inspect invocation over the accepted typed surfaces.
///
/// The selector is classified and resolved first (all selector
/// failures happen before any projection work), the full mandatory
/// skeleton is projected, and the canonical bytes are emitted only when
/// they fit the recorded output bound. Failures are single normalized
/// `invalid` sets on the accepted #11 envelope.
pub fn run(
    compilation: &Compilation,
    graph: &crate::graph::DependencyGraph,
    effects: &crate::effects::EffectGraph,
    request: &InspectRequest,
) -> Result<InspectOutcome, DiagnosticSet> {
    let selection = selector::resolve(&compilation.project, &request.selector)?;
    let projection = sections::project(
        compilation,
        graph,
        effects,
        &selection,
        request.include.bindings,
        request.include.scenarios,
    );
    let json = canonical::payload_bytes(&projection);
    if json.len() > version::MAX_OUTPUT_BYTES {
        return Err(diagnostic::output_limit_set(json.len()));
    }
    Ok(InspectOutcome {
        human: projection.to_human(),
        json,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn include_parses_the_closed_vocabulary() {
        assert_eq!(
            Include::parse("bindings,scenarios").unwrap(),
            Include {
                bindings: true,
                scenarios: true
            }
        );
        assert_eq!(
            Include::parse("scenarios").unwrap(),
            Include {
                bindings: false,
                scenarios: true
            }
        );
        assert!(Include::parse("").is_none());
        assert!(Include::parse("ownership").is_none());
        assert!(Include::parse("bindings,bindings").is_none());
        assert!(Include::parse("Bindings").is_none());
    }

    #[test]
    fn selector_validation_rejects_malformed_spellings() {
        assert!(validate_selector("planner.focus_task").is_ok());
        assert!(validate_selector("focus_task").is_ok());
        assert!(validate_selector("../planner").is_err());
        assert!(validate_selector("planner.TASK").is_err());
        assert!(validate_selector("").is_err());
    }
}
