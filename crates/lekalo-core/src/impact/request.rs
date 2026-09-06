//! The typed impact request (issue #16).
//!
//! A request is constructed only through validated constructors: roots are
//! bounded semantic ids, depth is bounded, filters are bounded, and the
//! profile is a closed vocabulary. Whether a relation or kind filter key is
//! registered is decided against the graph registry at analysis time, so a
//! request can never bypass the closed registries.

use crate::diagnostics::DiagnosticSet;

use super::diagnostic;
use super::version::{DEFAULT_DEPTH, MAX_DEPTH, MAX_FILTER_TERMS, MAX_ROOTS};

/// The closed impact profile vocabulary.
///
/// `strict` blocks targeted gate selection and public-safety verdicts on
/// required unknown/stale/incomplete evidence (bounded denial, exit 3);
/// `default` reports the same facts as degraded warnings and never denies.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ImpactProfile {
    Default,
    Strict,
}

impl ImpactProfile {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Strict => "strict",
        }
    }

    /// Parse the closed vocabulary.
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "default" => Some(Self::Default),
            "strict" => Some(Self::Strict),
            _ => None,
        }
    }
}

/// One validated impact request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImpactRequest {
    roots: Vec<String>,
    depth: u16,
    relations: Vec<String>,
    kinds: Vec<String>,
    module: Option<String>,
    target: Option<String>,
    profile: ImpactProfile,
}

impl ImpactRequest {
    /// Validate and construct one symbol-rooted request.
    pub fn for_symbol(root: &str) -> Result<Self, DiagnosticSet> {
        Self::for_symbols(&[root])
    }

    /// Validate and construct one request over several symbol roots.
    pub fn for_symbols(roots: &[&str]) -> Result<Self, DiagnosticSet> {
        if roots.is_empty() || roots.len() > MAX_ROOTS {
            return Err(diagnostic::selector_invalid_set("root-count"));
        }
        let mut checked = Vec::with_capacity(roots.len());
        for root in roots {
            if !super::input::is_symbol_grammar(root) {
                return Err(diagnostic::selector_invalid_set("root-grammar"));
            }
            checked.push((*root).to_owned());
        }
        checked.sort();
        checked.dedup();
        Ok(Self {
            roots: checked,
            depth: DEFAULT_DEPTH,
            relations: Vec::new(),
            kinds: Vec::new(),
            module: None,
            target: None,
            profile: ImpactProfile::Default,
        })
    }

    /// Cap the traversal depth (1..=256).
    pub fn with_depth(mut self, depth: u16) -> Result<Self, DiagnosticSet> {
        if depth == 0 || depth > MAX_DEPTH {
            return Err(diagnostic::selector_invalid_set("depth-bound"));
        }
        self.depth = depth;
        Ok(self)
    }

    /// Validate and construct one changed-input request; the roots come
    /// from the typed handoff, never from the command line.
    pub fn for_changed() -> Self {
        Self {
            roots: Vec::new(),
            depth: DEFAULT_DEPTH,
            relations: Vec::new(),
            kinds: Vec::new(),
            module: None,
            target: None,
            profile: ImpactProfile::Default,
        }
    }

    /// Narrow the admitted relations (closed registry keys, bounded count).
    pub fn with_relations(mut self, relations: &[String]) -> Result<Self, DiagnosticSet> {
        if relations.len() > MAX_FILTER_TERMS {
            return Err(diagnostic::selector_invalid_set("filter-term-limit"));
        }
        for relation in relations {
            if !self.relations.contains(relation) {
                self.relations.push(relation.clone());
            }
        }
        self.relations.sort();
        Ok(self)
    }

    /// Narrow the admitted discovered node kinds (closed registry keys).
    pub fn with_kinds(mut self, kinds: &[String]) -> Result<Self, DiagnosticSet> {
        if kinds.len() > MAX_FILTER_TERMS {
            return Err(diagnostic::selector_invalid_set("filter-term-limit"));
        }
        for kind in kinds {
            if kind.is_empty() || kind.len() > 64 {
                return Err(diagnostic::selector_invalid_set("kind-grammar"));
            }
            if !self.kinds.contains(kind) {
                self.kinds.push(kind.clone());
            }
        }
        self.kinds.sort();
        Ok(self)
    }

    /// Narrow the traversal to one module semantic id.
    pub fn with_module(mut self, module: &str) -> Result<Self, DiagnosticSet> {
        if module.is_empty() || module.len() > MAX_FILTER_TERMS {
            return Err(diagnostic::selector_invalid_set("module-bound"));
        }
        self.module = Some(module.to_owned());
        Ok(self)
    }

    /// Narrow the target projections to one target id.
    pub fn with_target(mut self, target: &str) -> Result<Self, DiagnosticSet> {
        if target.is_empty() || target.len() > MAX_FILTER_TERMS {
            return Err(diagnostic::selector_invalid_set("target-bound"));
        }
        self.target = Some(target.to_owned());
        Ok(self)
    }

    /// Select the closed profile.
    pub fn with_profile(mut self, profile: ImpactProfile) -> Self {
        self.profile = profile;
        self
    }

    /// The sorted unique semantic-id roots.
    pub fn roots(&self) -> &[String] {
        &self.roots
    }

    /// The traversal depth.
    pub const fn depth(&self) -> u16 {
        self.depth
    }

    /// The relation filter keys (empty admits everything).
    pub fn relations(&self) -> &[String] {
        &self.relations
    }

    /// The kind filter keys (empty admits everything).
    pub fn kinds(&self) -> &[String] {
        &self.kinds
    }

    /// The module filter, when set.
    pub fn module(&self) -> Option<&str> {
        self.module.as_deref()
    }

    /// The target filter, when set.
    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    /// The selected profile.
    pub const fn profile(&self) -> ImpactProfile {
        self.profile
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_issue_examples() {
        let request = ImpactRequest::for_symbol("planner.focus_task").expect("valid");
        assert_eq!(request.depth(), DEFAULT_DEPTH);
        assert_eq!(request.profile(), ImpactProfile::Default);
        assert_eq!(request.roots(), ["planner.focus_task"]);
    }

    #[test]
    fn bounds_are_enforced() {
        assert!(ImpactRequest::for_symbol("").is_err());
        assert!(ImpactRequest::for_symbols(&[]).is_err());
        assert!(ImpactRequest::for_symbol("planner.focus_task")
            .expect("valid")
            .with_depth(0)
            .is_err());
        assert!(ImpactRequest::for_symbol("planner.focus_task")
            .expect("valid")
            .with_depth(MAX_DEPTH)
            .is_ok());
        assert!(ImpactRequest::for_symbol("planner.focus_task")
            .expect("valid")
            .with_depth(MAX_DEPTH + 1)
            .is_err());
        let wide: Vec<String> = (0..(MAX_FILTER_TERMS + 1))
            .map(|i| format!("r{i}"))
            .collect();
        assert!(ImpactRequest::for_symbol("planner.focus_task")
            .expect("valid")
            .with_relations(&wide)
            .is_err());
    }

    #[test]
    fn roots_sort_and_deduplicate() {
        let request = ImpactRequest::for_symbols(&["b.sym", "a.sym", "b.sym"]).expect("valid");
        assert_eq!(request.roots(), ["a.sym", "b.sym"]);
    }
}
