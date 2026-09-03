//! Reference and type normalization: compact type sugar, short-reference
//! expansion, and direct-import visibility.

use std::collections::{BTreeMap, BTreeSet};

use super::error::{Diagnostic, Span};
use super::frontends::{MapEntry, Node, Scalar, Value};

/// Live-symbol lookup structures for one loaded project.
#[derive(Clone, Debug, Default)]
pub struct SymbolIndex {
    /// module ID -> (last segment -> full live symbol IDs).
    live_by_module: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    /// Every discovered module ID.
    declared_modules: BTreeSet<String>,
    /// module ID -> directly imported module IDs.
    imports: BTreeMap<String, BTreeSet<String>>,
}

/// Why a reference could not be normalized.
#[derive(Clone, Debug)]
pub enum RefError {
    /// Qualified reference to a module without a direct import edge.
    WithoutImport { module: String },
    /// No live candidate in the searched scope.
    Unresolved {
        reference: String,
        module: Option<String>,
    },
    /// More than one live candidate; carries the sorted full IDs.
    Ambiguous {
        reference: String,
        candidates: Vec<String>,
    },
}

impl SymbolIndex {
    /// Build from per-module live symbol IDs and direct import sets.
    ///
    /// `symbols_by_module` yields `(module ID, full symbol IDs)`; the last
    /// segment of each ID is its short name.
    pub fn build(
        symbols_by_module: impl IntoIterator<Item = (String, Vec<String>)>,
        declared_modules: impl IntoIterator<Item = String>,
        imports: impl IntoIterator<Item = (String, Vec<String>)>,
    ) -> Self {
        let mut index = Self::default();
        for (module_id, symbols) in symbols_by_module {
            let entry = index.live_by_module.entry(module_id).or_default();
            for symbol in symbols {
                let short = symbol.rsplit('.').next().unwrap_or(&symbol).to_owned();
                entry.entry(short).or_default().push(symbol);
            }
        }
        for symbols in index.live_by_module.values_mut() {
            for candidates in symbols.values_mut() {
                candidates.sort();
                candidates.dedup();
            }
        }
        index.declared_modules = declared_modules.into_iter().collect();
        index.imports = imports
            .into_iter()
            .map(|(module, list)| (module, list.into_iter().collect()))
            .collect();
        index
    }

    /// Normalize one reference string.
    ///
    /// Short references resolve only among live symbols of the current
    /// module. Dotted references are module-qualified: the prefix must be
    /// the current module or a directly imported module. Historical IDs are
    /// never aliases.
    pub fn normalize_reference(
        &self,
        current_module: &str,
        reference: &str,
    ) -> Result<String, RefError> {
        if !reference.contains('.') {
            let candidates = self
                .live_by_module
                .get(current_module)
                .and_then(|by_short| by_short.get(reference))
                .cloned()
                .unwrap_or_default();
            return match candidates.len() {
                0 => Err(RefError::Unresolved {
                    reference: reference.to_owned(),
                    module: Some(current_module.to_owned()),
                }),
                1 => Ok(candidates[0].clone()),
                _ => Err(RefError::Ambiguous {
                    reference: reference.to_owned(),
                    candidates,
                }),
            };
        }
        let prefix = reference.split('.').next().unwrap_or_default();
        if prefix == current_module {
            return Ok(reference.to_owned());
        }
        if !self.declared_modules.contains(prefix) {
            return Err(RefError::Unresolved {
                reference: reference.to_owned(),
                module: Some(prefix.to_owned()),
            });
        }
        let directly_imported = self
            .imports
            .get(current_module)
            .is_some_and(|set| set.contains(prefix));
        if directly_imported {
            Ok(reference.to_owned())
        } else {
            Err(RefError::WithoutImport {
                module: prefix.to_owned(),
            })
        }
    }
}

/// Compact type sugar: `Type := Ref | list<Type> | Type?`.
#[derive(Clone, Debug)]
enum Compact {
    Ref(String),
    List(Box<Compact>),
    Optional(Box<Compact>),
}

/// Maximum nesting levels including the leaf reference (three wrappers).
const MAX_TYPE_LEVELS: usize = 4;

fn parse_compact(text: &str) -> Result<Compact, ()> {
    parse_compact_at(text)
}

fn parse_compact_at(text: &str) -> Result<Compact, ()> {
    if let Some(inner) = text.strip_suffix('?') {
        if inner.is_empty() {
            return Err(());
        }
        return Ok(Compact::Optional(Box::new(parse_compact_at(inner)?)));
    }
    if let Some(inner) = text.strip_prefix("list<") {
        if let Some(inner) = inner.strip_suffix('>') {
            if inner.is_empty() {
                return Err(());
            }
            return Ok(Compact::List(Box::new(parse_compact_at(inner)?)));
        }
    }
    if is_plausible_reference(text) {
        Ok(Compact::Ref(text.to_owned()))
    } else {
        Err(())
    }
}

fn levels(compact: &Compact) -> usize {
    match compact {
        Compact::Ref(_) => 1,
        Compact::List(inner) | Compact::Optional(inner) => 1 + levels(inner),
    }
}

fn is_plausible_reference(text: &str) -> bool {
    !text.is_empty()
        && !text.chars().any(|c| {
            c.is_whitespace() || matches!(c, '<' | '>' | '?' | '{' | '}' | '[' | ']' | ',' | ':')
        })
}

/// Diagnostic context threaded through normalization.
pub struct NormalizeContext<'index> {
    pub index: &'index SymbolIndex,
    pub current_module: String,
    pub diagnostics: Vec<Diagnostic>,
    /// `(pointer suffix relative to the definition, span)` source-map crumbs.
    pub source_entries: Vec<(String, Span)>,
}

impl<'index> NormalizeContext<'index> {
    pub fn new(index: &'index SymbolIndex, current_module: &str) -> Self {
        Self {
            index,
            current_module: current_module.to_owned(),
            diagnostics: Vec::new(),
            source_entries: Vec::new(),
        }
    }

    fn reference_diagnostic(&mut self, error: RefError, span: Span) {
        let (code, data) = match &error {
            RefError::WithoutImport { module } => (
                "loader.reference-without-import",
                serde_json::json!({ "module": module }),
            ),
            RefError::Unresolved { reference, module } => (
                "loader.short-reference-unresolved",
                serde_json::json!({ "reference": reference, "module": module }),
            ),
            RefError::Ambiguous {
                reference,
                candidates,
            } => (
                "loader.ambiguous-short-reference",
                serde_json::json!({ "reference": reference, "candidates": candidates }),
            ),
        };
        self.diagnostics
            .push(Diagnostic::new(code).with_span(span).with_data(data));
    }
}

/// Normalize every reference and type surface of one definition node.
///
/// The reference surfaces are exactly: `fields[].type` (also `input` and
/// `payload` field lists), `returns`, `effects`, `reads`, `applies_to`,
/// `entity`, `emits`, `invokes`, and `covers`. Everything else passes
/// through untouched; the Model validator owns closed fields and kind
/// placement.
pub fn normalize_definition(node: &mut Node, context: &mut NormalizeContext) {
    for key in ["fields", "input", "payload"] {
        if let Some(list) = node.get_mut(key) {
            normalize_field_list(list, key, context);
        }
    }
    if let Some(returns) = node.get_mut("returns") {
        normalize_type(returns, 0, "returns", context);
    }
    for key in ["effects", "reads", "applies_to", "emits", "covers"] {
        if let Some(list) = node.get_mut(key) {
            if let Value::Seq(items) = &mut list.value {
                for (position, item) in items.iter_mut().enumerate() {
                    normalize_reference_node(item, &format!("{key}/{position}"), context);
                }
            }
        }
    }
}

fn normalize_field_list(list: &mut Node, key: &str, context: &mut NormalizeContext) {
    let Some(items) = list.as_seq_mut() else {
        return;
    };
    for (position, field) in items.iter_mut().enumerate() {
        let pointer = format!("{key}/{position}/type");
        if let Some(type_node) = field.get_mut("type") {
            normalize_type(type_node, 0, &pointer, context);
        }
    }
}

fn normalize_reference_node(node: &mut Node, pointer: &str, context: &mut NormalizeContext) {
    let span = node.span;
    let Some(text) = node.as_str() else {
        // Non-string reference values are left for the Model validator.
        return;
    };
    let text = text.to_owned();
    match context
        .index
        .normalize_reference(&context.current_module, &text)
    {
        Ok(expanded) => {
            if expanded != text {
                *node = ref_node(&expanded, span);
            }
            context.source_entries.push((pointer.to_owned(), span));
        }
        Err(error) => context.reference_diagnostic(error, span),
    }
}

fn normalize_type(node: &mut Node, wrappers: usize, pointer: &str, context: &mut NormalizeContext) {
    let span = node.span;
    context.source_entries.push((pointer.to_owned(), span));
    match &mut node.value {
        Value::Scalar(Scalar::Str(text)) => {
            let text = text.clone();
            match parse_compact(&text) {
                Ok(compact) => {
                    if let Some(canonical) = compact_to_node(compact, span, wrappers, context) {
                        *node = canonical;
                    }
                }
                Err(()) => {
                    context.diagnostics.push(
                        Diagnostic::new("loader.type-syntax")
                            .with_span(span)
                            .with_data(serde_json::json!({ "type": text })),
                    );
                }
            }
        }
        Value::Map(entries) => {
            if entries.len() != 1 {
                context.diagnostics.push(
                    Diagnostic::new("loader.type-syntax")
                        .with_span(span)
                        .with_data(serde_json::json!({ "detail": "exactly-one-key" })),
                );
                return;
            }
            let key = entries[0].key.clone();
            match key.as_str() {
                "ref" => {
                    let inner_span = entries[0].value.span;
                    let Some(text) = entries[0].value.as_str() else {
                        context.diagnostics.push(
                            Diagnostic::new("loader.type-syntax")
                                .with_span(inner_span)
                                .with_data(serde_json::json!({ "detail": "ref-not-string" })),
                        );
                        return;
                    };
                    let text = text.to_owned();
                    match context
                        .index
                        .normalize_reference(&context.current_module, &text)
                    {
                        Ok(expanded) => {
                            if expanded != text {
                                entries[0].value = Scalar::Str(expanded).into_node(inner_span);
                            }
                        }
                        Err(error) => context.reference_diagnostic(error, inner_span),
                    }
                }
                "list" | "optional" => {
                    if wrappers + 1 > MAX_TYPE_LEVELS - 1 {
                        context.diagnostics.push(
                            Diagnostic::new("loader.type-depth")
                                .with_span(span)
                                .with_data(serde_json::json!({ "maxLevels": MAX_TYPE_LEVELS })),
                        );
                        return;
                    }
                    normalize_type(&mut entries[0].value, wrappers + 1, pointer, context);
                }
                other => {
                    context.diagnostics.push(
                        Diagnostic::new("loader.type-syntax")
                            .with_span(span)
                            .with_data(serde_json::json!({ "wrapper": other })),
                    );
                }
            }
        }
        _ => {
            context.diagnostics.push(
                Diagnostic::new("loader.type-syntax")
                    .with_span(span)
                    .with_data(serde_json::json!({ "detail": "type-not-string-or-object" })),
            );
        }
    }
}

/// Materialize a compact form into canonical one-key objects, expanding the
/// leaf reference. Returns `None` once a diagnostic was recorded.
fn compact_to_node(
    compact: Compact,
    span: Span,
    wrappers: usize,
    context: &mut NormalizeContext,
) -> Option<Node> {
    let mut expanded_leaf = |reference: String, leaf_span: Span| -> Option<Node> {
        match context
            .index
            .normalize_reference(&context.current_module, &reference)
        {
            Ok(expanded) => Some(ref_node(&expanded, leaf_span)),
            Err(error) => {
                context.reference_diagnostic(error, leaf_span);
                None
            }
        }
    };
    // Enforce the level bound for the whole compact chain at once.
    if levels(&compact) + wrappers > MAX_TYPE_LEVELS {
        context.diagnostics.push(
            Diagnostic::new("loader.type-depth")
                .with_span(span)
                .with_data(serde_json::json!({ "maxLevels": MAX_TYPE_LEVELS })),
        );
        return None;
    }
    match compact {
        Compact::Ref(reference) => expanded_leaf(reference, span),
        Compact::Optional(inner) => compact_to_node(*inner, span, wrappers, context)
            .map(|inner| wrapper_node("optional", inner, span)),
        Compact::List(inner) => compact_to_node(*inner, span, wrappers, context)
            .map(|inner| wrapper_node("list", inner, span)),
    }
}

fn wrapper_node(key: &str, inner: Node, span: Span) -> Node {
    Node {
        span,
        value: Value::Map(vec![MapEntry {
            key: key.to_owned(),
            key_span: span,
            value: inner,
        }]),
    }
}

fn ref_node(reference: &str, span: Span) -> Node {
    Node {
        span,
        value: Value::Map(vec![MapEntry {
            key: "ref".to_owned(),
            key_span: span,
            value: Node::scalar(Scalar::Str(reference.to_owned()), span),
        }]),
    }
}

impl Scalar {
    fn into_node(self, span: Span) -> Node {
        Node::scalar(self, span)
    }
}

/// Sort a module definition's `imports` array by module ID (set-like
/// output ordering). Non-string entries are left in place; they can only
/// reach this point when import decoding already passed.
pub fn sort_imports(module_node: &mut Node) {
    if let Some(imports) = module_node.get_mut("imports") {
        if let Value::Seq(items) = &mut imports.value {
            items.sort_by(|left, right| {
                let left_key = left.as_str().map(|text| text.as_bytes().to_vec());
                let right_key = right.as_str().map(|text| text.as_bytes().to_vec());
                left_key.cmp(&right_key)
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> SymbolIndex {
        SymbolIndex::build(
            [
                (
                    "planner".to_owned(),
                    vec![
                        "planner.task".to_owned(),
                        "planner.event.focus_changed".to_owned(),
                        "planner.focus_changed".to_owned(),
                        "planner.event.focus_lost".to_owned(),
                    ],
                ),
                ("scheduling".to_owned(), vec!["scheduling.slot".to_owned()]),
            ],
            ["planner".to_owned(), "scheduling".to_owned()],
            [("scheduling".to_owned(), vec!["planner".to_owned()])],
        )
    }

    #[test]
    fn short_references_resolve_only_in_the_current_module() {
        let index = index();
        assert_eq!(
            index.normalize_reference("planner", "task").unwrap(),
            "planner.task"
        );
        // Never searches other modules for short names.
        assert!(matches!(
            index.normalize_reference("planner", "slot"),
            Err(RefError::Unresolved { .. })
        ));
    }

    #[test]
    fn ambiguous_short_references_list_sorted_candidates() {
        let index = index();
        let error = index.normalize_reference("planner", "focus").unwrap_err();
        // "focus" does not match; "focus_changed" would.
        assert!(matches!(error, RefError::Unresolved { .. }));
        let error = index
            .normalize_reference("planner", "focus_changed")
            .unwrap_err();
        match error {
            RefError::Ambiguous { candidates, .. } => {
                assert_eq!(
                    candidates,
                    ["planner.event.focus_changed", "planner.focus_changed"]
                );
            }
            _ => panic!("expected ambiguity"),
        }
    }

    #[test]
    fn qualified_references_require_direct_imports() {
        let index = index();
        // Own module needs no import.
        assert_eq!(
            index
                .normalize_reference("planner", "planner.task")
                .unwrap(),
            "planner.task"
        );
        // Direct import present.
        assert_eq!(
            index
                .normalize_reference("scheduling", "planner.task")
                .unwrap(),
            "planner.task"
        );
        // No import edge from planner to scheduling.
        assert!(matches!(
            index.normalize_reference("planner", "scheduling.slot"),
            Err(RefError::WithoutImport { module }) if module == "scheduling"
        ));
        // Unknown module prefix.
        assert!(matches!(
            index.normalize_reference("planner", "ghost.task"),
            Err(RefError::Unresolved { .. })
        ));
    }

    #[test]
    fn compact_types_parse_exactly_and_reject_garbage() {
        let ok = [
            ("task", Compact::Ref("task".to_owned())),
            (
                "list<task>",
                Compact::List(Box::new(Compact::Ref("task".to_owned()))),
            ),
            (
                "task?",
                Compact::Optional(Box::new(Compact::Ref("task".to_owned()))),
            ),
            (
                "list<task?>",
                Compact::List(Box::new(Compact::Optional(Box::new(Compact::Ref(
                    "task".to_owned(),
                ))))),
            ),
            (
                "list<list<task>>",
                Compact::List(Box::new(Compact::List(Box::new(Compact::Ref(
                    "task".to_owned(),
                ))))),
            ),
        ];
        for (text, expected) in ok {
            assert_eq!(
                format!("{:?}", parse_compact(text).unwrap()),
                format!("{:?}", expected),
                "{text}"
            );
        }
        for bogus in [
            "list< task",
            "list<task",
            "list<",
            "task ??",
            "list<x>y",
            "ta sk",
            "list<>",
            "?",
            "list<list<list<list<x>>>",
        ] {
            assert!(parse_compact(bogus).is_err(), "{bogus}");
        }
    }

    #[test]
    fn depth_bound_counts_levels_including_the_leaf() {
        // Three wrappers + leaf = four levels: legal.
        assert!(parse_compact("list<list<list<task>>>").is_ok());
        // Four wrappers + leaf = five levels: grammar still parses, the
        // depth check rejects during normalization.
        let deep = parse_compact("list<list<list<task?>>>").unwrap();
        assert_eq!(
            format!("{deep:?}"),
            "List(List(List(Optional(Ref(\"task\")))))"
        );
    }
}
