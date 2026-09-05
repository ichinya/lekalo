//! Trace manifest parsing: byte integrity, closed wire shape, and
//! semantic validation (issue #22).
//!
//! Three layers, in order, each terminal: the byte scan rejects a BOM,
//! invalid UTF-8, trailing bytes after the root value, and duplicate JSON
//! keys (the closed wire is canonical-payload data, and serde alone would
//! silently collapse duplicates); serde maps the document onto the closed
//! wire shape with `deny_unknown_fields`, so unknown keys, wrong shapes,
//! and type violations all fail without echoing attacker-controlled
//! names; the semantic validator enforces every closed rule the schema
//! cannot express — kind exclusivity, endpoint resolution and direction,
//! tuple uniqueness, gap coherence, and the completeness policy — before
//! any index exists. Every failure is one registered #11 diagnostic with
//! a fixed classification tag; raw input never enters the envelope.

use std::collections::{HashMap, HashSet};

use super::completeness::{Completeness, Gap, GapError};
use super::diagnostic::{self, PathViolation};
use super::node::{Node, NodeError};
use super::provenance::Status;
use super::relation::{Relation, RelationError};
use super::version;
use super::{ContractRef, Manifest, TraceManifest};

/// The maximum JSON nesting depth, matching serde_json's own limit.
const MAX_DEPTH: usize = 128;

/// Parse and validate one manifest document.
pub(super) fn parse_and_validate(
    bytes: &[u8],
) -> Result<TraceManifest, crate::diagnostics::DiagnosticSet> {
    if bytes.len() > version::MAX_MANIFEST_BYTES {
        return Err(diagnostic::input_invalid_detail("over-limit", None));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| diagnostic::input_invalid_detail("invalid-encoding", None))?;
    scan_document(text)?;
    let manifest: Manifest = serde_json::from_str(text)
        .map_err(|_| diagnostic::input_invalid_detail("schema-invalid", None))?;
    validate(manifest)
}

/// The byte-level scan: BOM, trailing bytes, structure, duplicate keys.
fn scan_document(text: &str) -> Result<(), crate::diagnostics::DiagnosticSet> {
    if text.starts_with('\u{feff}') {
        return Err(diagnostic::input_invalid_detail("bom-prefix", None));
    }
    let mut scanner = Scanner {
        bytes: text.as_bytes(),
        position: 0,
        member_start: 0,
        member_end: 0,
    };
    let mut stack: Vec<HashSet<&[u8]>> = Vec::new();
    scanner.skip_whitespace();
    scanner.value(&mut stack)?;
    scanner.skip_whitespace();
    if scanner.position != scanner.bytes.len() {
        return Err(diagnostic::input_invalid_detail("trailing-bytes", None));
    }
    Ok(())
}

struct Scanner<'a> {
    bytes: &'a [u8],
    position: usize,
    /// Bounds of the last scanned string, used for object key
    /// bookkeeping; only valid between [`Self::string`] and the next
    /// recursive scan.
    member_start: usize,
    member_end: usize,
}

impl<'a> Scanner<'a> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.position += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), crate::diagnostics::DiagnosticSet> {
        if self.peek() == Some(byte) {
            self.position += 1;
            Ok(())
        } else {
            Err(diagnostic::input_invalid_detail("malformed-json", None))
        }
    }

    fn value(
        &mut self,
        stack: &mut Vec<HashSet<&'a [u8]>>,
    ) -> Result<(), crate::diagnostics::DiagnosticSet> {
        if stack.len() > MAX_DEPTH {
            return Err(diagnostic::input_invalid_detail("over-limit", None));
        }
        match self.peek() {
            Some(b'{') => self.object(stack),
            Some(b'[') => self.array(stack),
            Some(b'"') => self.string(),
            Some(b't') => self.literal(b"true"),
            Some(b'f') => self.literal(b"false"),
            Some(b'n') => self.literal(b"null"),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(diagnostic::input_invalid_detail("malformed-json", None)),
        }
    }

    fn object(
        &mut self,
        stack: &mut Vec<HashSet<&'a [u8]>>,
    ) -> Result<(), crate::diagnostics::DiagnosticSet> {
        self.expect(b'{')?;
        stack.push(HashSet::new());
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.position += 1;
            stack.pop();
            return Ok(());
        }
        loop {
            self.skip_whitespace();
            self.string()?;
            let key: &'a [u8] = &self.bytes[self.member_start..self.member_end];
            self.skip_whitespace();
            self.expect(b':')?;
            self.skip_whitespace();
            self.value(stack)?;
            let keys = stack
                .last_mut()
                .ok_or_else(|| diagnostic::input_invalid_detail("malformed-json", None))?;
            if !keys.insert(key) {
                return Err(diagnostic::input_invalid_detail("duplicate-key", None));
            }
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.position += 1,
                Some(b'}') => {
                    self.position += 1;
                    stack.pop();
                    return Ok(());
                }
                _ => return Err(diagnostic::input_invalid_detail("malformed-json", None)),
            }
        }
    }

    fn array(
        &mut self,
        stack: &mut Vec<HashSet<&'a [u8]>>,
    ) -> Result<(), crate::diagnostics::DiagnosticSet> {
        self.expect(b'[')?;
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.position += 1;
            return Ok(());
        }
        loop {
            self.skip_whitespace();
            self.value(stack)?;
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.position += 1,
                Some(b']') => {
                    self.position += 1;
                    return Ok(());
                }
                _ => return Err(diagnostic::input_invalid_detail("malformed-json", None)),
            }
        }
    }

    fn string(&mut self) -> Result<(), crate::diagnostics::DiagnosticSet> {
        self.expect(b'"')?;
        self.member_start = self.position;
        loop {
            match self.peek() {
                None => return Err(diagnostic::input_invalid_detail("malformed-json", None)),
                Some(b'"') => {
                    self.member_end = self.position;
                    self.position += 1;
                    return Ok(());
                }
                Some(b'\\') => {
                    self.position += 1;
                    match self.peek() {
                        Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                            self.position += 1;
                        }
                        Some(b'u') => {
                            self.position += 1;
                            for _ in 0..4 {
                                match self.peek() {
                                    Some(byte) if byte.is_ascii_hexdigit() => self.position += 1,
                                    _ => {
                                        return Err(diagnostic::input_invalid_detail(
                                            "malformed-json",
                                            None,
                                        ))
                                    }
                                }
                            }
                        }
                        _ => return Err(diagnostic::input_invalid_detail("malformed-json", None)),
                    }
                }
                Some(_) => {
                    self.position += 1;
                }
            }
        }
    }

    fn literal(&mut self, expected: &[u8]) -> Result<(), crate::diagnostics::DiagnosticSet> {
        if self.bytes[self.position..].starts_with(expected) {
            self.position += expected.len();
            Ok(())
        } else {
            Err(diagnostic::input_invalid_detail("malformed-json", None))
        }
    }

    fn number(&mut self) -> Result<(), crate::diagnostics::DiagnosticSet> {
        // Permissive scan over the numeric alphabet; serde re-validates
        // the exact JSON number grammar afterwards.
        while matches!(
            self.peek(),
            Some(b'-' | b'+' | b'.' | b'0'..=b'9' | b'e' | b'E')
        ) {
            self.position += 1;
        }
        Ok(())
    }
}

/// Map one lexical path violation onto its stable `structure.*` code.
fn path_violation(path: &str) -> crate::diagnostics::DiagnosticSet {
    let code = crate::project_fs::path_violation(path).unwrap_or("structure.path-segment");
    diagnostic::path_violation_set(&PathViolation(code))
}

/// The semantic validator: every closed rule the JSON Schema cannot
/// express, checked before the index is built.
fn validate(manifest: Manifest) -> Result<TraceManifest, crate::diagnostics::DiagnosticSet> {
    if manifest.schema_version != version::SCHEMA_VERSION || manifest.identity != version::IDENTITY
    {
        return Err(diagnostic::input_invalid_detail("schema-invalid", None));
    }
    if !super::id::is_manifest_id(&manifest.manifest_id)
        || !super::id::is_semantic_id(&manifest.project_ref)
        || !super::id::is_revision(&manifest.source_revision)
    {
        return Err(diagnostic::input_invalid_detail("identity-invalid", None));
    }
    validate_contract_ref(&manifest.model_ref, &["0.1.0", "1.0.0"])?;
    if let Some(ir) = &manifest.ir_ref {
        validate_contract_ref(ir, &["0.1.0"])?;
    }
    if let Some(graph) = &manifest.graph_ref {
        validate_contract_ref(graph, &["1.0.0"])?;
    }
    if let Some(artifact) = &manifest.artifact_manifest_ref {
        validate_contract_ref(artifact, &[])?;
    }
    if manifest.nodes.len() > version::MAX_NODES
        || manifest.relations.len() > version::MAX_RELATIONS
        || manifest.gaps.len() > version::MAX_GAPS
    {
        return Err(diagnostic::input_invalid_detail("over-limit", None));
    }

    // Nodes: grammar, kind exclusivity, unique ids, unique per-kind
    // identities.
    let mut node_positions: HashMap<&str, usize> = HashMap::with_capacity(manifest.nodes.len());
    let mut identities: HashSet<(&str, &str)> = HashSet::new();
    for (position, node) in manifest.nodes.iter().enumerate() {
        match node.validate() {
            Ok(()) => {}
            Err(NodeError::Path) => {
                return Err(path_violation(node.path.as_deref().unwrap_or_default()))
            }
            Err(NodeError::ExternalOverLimit) => {
                return Err(diagnostic::input_invalid_detail("over-limit", None))
            }
            Err(_) => return Err(diagnostic::input_invalid_detail("node-invalid", None)),
        }
        if node_positions
            .insert(node.node_id.as_str(), position)
            .is_some()
        {
            return Err(diagnostic::input_invalid_detail("duplicate-node-id", None));
        }
        let identity = node.identity().unwrap_or_default();
        if !identities.insert((node.node_kind.as_str(), identity)) {
            return Err(diagnostic::input_invalid_detail("duplicate-identity", None));
        }
    }

    // Relations: grammar, tuple digest, endpoint resolution and
    // direction, evidence resolution, unique tuples, confirmed policy.
    let mut relation_tuples: HashSet<(&str, &str, &str, &str)> = HashSet::new();
    for relation in &manifest.relations {
        match relation.validate(&manifest.source_revision) {
            Ok(()) => {}
            Err(RelationError::ConfirmedPolicy) => {
                return Err(diagnostic::input_invalid_detail("confirmed-policy", None))
            }
            Err(RelationError::Provenance) => {
                return match relation.provenance.source_path.as_deref() {
                    Some(path) => Err(path_violation(path)),
                    None => Err(diagnostic::input_invalid_detail("provenance-invalid", None)),
                };
            }
            Err(_) => return Err(diagnostic::input_invalid_detail("relation-invalid", None)),
        }
        if !relation_tuples.insert((
            relation.relation_kind.as_str(),
            relation.from_node.as_str(),
            relation.to_node.as_str(),
            relation.occurrence.as_str(),
        )) {
            return Err(diagnostic::input_invalid_detail("duplicate-relation", None));
        }
        let Some(from) = node_positions.get(relation.from_node.as_str()) else {
            return Err(diagnostic::input_invalid_detail("dangling-endpoint", None));
        };
        let Some(to) = node_positions.get(relation.to_node.as_str()) else {
            return Err(diagnostic::input_invalid_detail("dangling-endpoint", None));
        };
        let from_kind = manifest.nodes[*from].node_kind;
        let to_kind = manifest.nodes[*to].node_kind;
        if !relation.relation_kind.endpoints_legal(from_kind, to_kind) {
            return Err(diagnostic::input_invalid_detail("endpoints-illegal", None));
        }
        for evidence in &relation.evidence_refs {
            if !node_positions.contains_key(evidence.as_str()) {
                return Err(diagnostic::input_invalid_detail("dangling-evidence", None));
            }
        }
    }

    // Gaps: closed kinds/statuses, legal anchors, safe paths.
    for gap in &manifest.gaps {
        match gap.validate() {
            Ok(()) => {}
            Err(GapError::Path) => {
                return Err(path_violation(
                    gap.source_path.as_deref().unwrap_or_default(),
                ))
            }
            Err(_) => return Err(diagnostic::input_invalid_detail("gap-invalid", None)),
        }
        if let Some(anchor) = &gap.anchor_node {
            if !node_positions.contains_key(anchor.as_str()) {
                return Err(diagnostic::input_invalid_detail(
                    "dangling-gap-anchor",
                    None,
                ));
            }
        }
    }

    // Completeness policy.
    let has_gaps = !manifest.gaps.is_empty();
    match manifest.completeness {
        Completeness::Full if has_gaps => {
            return Err(diagnostic::input_invalid_detail("full-with-gaps", None));
        }
        Completeness::Full => {
            if manifest
                .relations
                .iter()
                .any(|relation| relation.status != Status::Confirmed)
            {
                return Err(diagnostic::input_invalid_detail(
                    "full-with-unconfirmed",
                    None,
                ));
            }
        }
        Completeness::Partial if !has_gaps => {
            return Err(diagnostic::input_invalid_detail(
                "partial-without-gap",
                None,
            ));
        }
        Completeness::Partial => {}
    }

    // Canonical ordering and the derived indexes.
    let mut manifest = manifest;
    manifest.nodes.sort_by(canonical_node_order);
    for node in &mut manifest.nodes {
        super::node::canonicalize_external_refs(&mut node.external_refs);
    }
    manifest.relations.sort_by(canonical_relation_order);
    manifest.gaps.sort_by(canonical_gap_order);

    let mut node_positions: HashMap<String, usize> = HashMap::with_capacity(manifest.nodes.len());
    for (position, node) in manifest.nodes.iter().enumerate() {
        node_positions.insert(node.node_id.clone(), position);
    }
    let mut outgoing = vec![Vec::new(); manifest.nodes.len()];
    let mut incoming = vec![Vec::new(); manifest.nodes.len()];
    for (relation_position, relation) in manifest.relations.iter().enumerate() {
        let from = node_positions[&relation.from_node];
        let to = node_positions[&relation.to_node];
        outgoing[from].push(relation_position);
        incoming[to].push(relation_position);
    }

    let built = TraceManifest {
        manifest,
        node_positions,
        outgoing,
        incoming,
    };

    // Sink coverage is checked on the built manifest (it needs the
    // adjacency). A full manifest with an uncovered sink is incoherent:
    // the data is complete only when every claimed chain closes.
    if built.manifest().completeness == Completeness::Full && !built.uncovered_sinks().is_empty() {
        return Err(diagnostic::input_invalid_detail(
            "full-with-uncovered-sink",
            None,
        ));
    }
    Ok(built)
}

fn validate_contract_ref(
    reference: &ContractRef,
    allowed_versions: &[&str],
) -> Result<(), crate::diagnostics::DiagnosticSet> {
    if !super::id::is_contract_version(&reference.schema_version)
        || !super::id::is_digest(&reference.digest)
    {
        return Err(diagnostic::input_invalid_detail(
            "contract-ref-invalid",
            None,
        ));
    }
    if !allowed_versions.is_empty()
        && !allowed_versions.contains(&reference.schema_version.as_str())
    {
        return Err(diagnostic::input_invalid_detail(
            "contract-ref-invalid",
            None,
        ));
    }
    Ok(())
}

/// Canonical node order: (kind rank, node id) by unsigned UTF-8.
pub(crate) fn canonical_node_order(left: &Node, right: &Node) -> std::cmp::Ordering {
    (
        super::node::NodeKind::KEYS
            .iter()
            .position(|kind| *kind == left.node_kind),
        left.node_id.as_bytes(),
    )
        .cmp(&(
            super::node::NodeKind::KEYS
                .iter()
                .position(|kind| *kind == right.node_kind),
            right.node_id.as_bytes(),
        ))
}

/// Canonical relation order: (kind rank, from, to, occurrence).
pub(crate) fn canonical_relation_order(left: &Relation, right: &Relation) -> std::cmp::Ordering {
    (
        super::relation::RelationKind::KEYS
            .iter()
            .position(|kind| *kind == left.relation_kind),
        left.from_node.as_bytes(),
        left.to_node.as_bytes(),
        left.occurrence.as_bytes(),
    )
        .cmp(&(
            super::relation::RelationKind::KEYS
                .iter()
                .position(|kind| *kind == right.relation_kind),
            right.from_node.as_bytes(),
            right.to_node.as_bytes(),
            right.occurrence.as_bytes(),
        ))
}

/// Canonical gap order: (kind rank, anchor, expected).
pub(crate) fn canonical_gap_order(left: &Gap, right: &Gap) -> std::cmp::Ordering {
    (
        super::completeness::GapKind::KEYS
            .iter()
            .position(|kind| *kind == left.gap_kind),
        left.anchor_node.as_deref().map(str::as_bytes),
        left.expected.as_deref().map(str::as_bytes),
    )
        .cmp(&(
            super::completeness::GapKind::KEYS
                .iter()
                .position(|kind| *kind == right.gap_kind),
            right.anchor_node.as_deref().map(str::as_bytes),
            right.expected.as_deref().map(str::as_bytes),
        ))
}
