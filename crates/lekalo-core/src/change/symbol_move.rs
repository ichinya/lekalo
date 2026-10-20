//! Same-identity semantic relocation. Only Model references are rewritten;
//! prose, enum literals, requirement IDs and target-native names are not refs.
//! Attachment homes that cannot yet be rewritten are explicitly guarded.

use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::loader::project_docs::DocKind;
use crate::project_fs::{Fs, FsErrorKind};

const FILE_GUARDS: &[&str] = &[
    "lekalo/authorization.yaml",
    "lekalo/transport.yaml",
    "lekalo/test-port.json",
    "lekalo/classification.json",
    "lekalo/classification-policy.json",
    "lekalo/query-model.yaml",
    "module-api.json",
    ".lekalo/import/observed/index.json",
];
const HOME_GUARDS: &[&str] = &[
    "lekalo/scenarios",
    "lekalo/types",
    "lekalo/operations",
    "lekalo/routes",
];

/// Absence/empty-home pins are part of the displayed input inventory and
/// plan identity, rechecked under the writer lock and before commit.
#[derive(Clone, Debug)]
pub(crate) struct Guards;

impl Guards {
    pub(crate) fn is_input(&self, path: &str) -> bool {
        FILE_GUARDS.contains(&path) || HOME_GUARDS.contains(&path)
    }

    fn check(&self, root: &Path) -> Result<(), ChangeError> {
        let fs = Fs::open(root).map_err(|_| ChangeError::WriteBoundaryDenied)?;
        for path in FILE_GUARDS {
            let read = if *path == binding::INDEX_PATH {
                binding::read_index(&fs)
            } else {
                fs.read_logical(path, crate::loader::source::MAX_DOCUMENT_BYTES)
            };
            match read {
                Ok(None) | Err(FsErrorKind::NotFound) => {}
                _ => return Err(unsupported("reference-attachment")),
            }
        }
        for path in HOME_GUARDS {
            match fs.entries_bounded(path, 1) {
                Ok(entries) if entries.is_empty() => {}
                Err(FsErrorKind::NotFound) => {}
                _ => return Err(unsupported("reference-attachment")),
            }
        }
        Ok(())
    }

    pub(crate) fn revalidate(&self, root: &Path) -> Result<(), ChangeError> {
        self.check(root)
            .map_err(|_| ChangeError::PreconditionFailed {
                subject: "symbol-attachments".to_owned(),
                detail: "input-inventory",
            })
    }

    fn pins(&self) -> Vec<InputPin> {
        FILE_GUARDS
            .iter()
            .chain(HOME_GUARDS)
            .map(|path| InputPin {
                path: (*path).to_owned(),
                digest: absent_digest(),
            })
            .collect()
    }
}

fn unsupported(member: &'static str) -> ChangeError {
    ChangeError::UnsupportedMember {
        operation: "symbol:move".to_owned(),
        member,
    }
}

fn conflict(subject: &str, detail: &'static str) -> ChangeError {
    ChangeError::SubjectConflict {
        subject: subject.to_owned(),
        detail,
    }
}

fn preservation(subject: &str) -> ChangeError {
    ChangeError::SourcePreservation {
        subject: subject.to_owned(),
        detail: "item-layout",
    }
}

pub(super) fn prepare(
    load: &crate::loader::ProjectLoad,
    request: &ChangeRequest,
    options: &PrepareOptions<'_>,
    base: planning::Base,
) -> Result<PreparedChange, ChangeError> {
    let Operation::SymbolMove {
        symbol,
        to_module,
        to_name,
    } = &request.operations[0]
    else {
        return Err(ChangeError::WriteBoundaryDenied);
    };
    let source = find_definition(&load.documents, symbol)?;
    let kind = definition_kind_of(source);
    if matches!(kind.as_str(), "module" | "project") {
        return Err(conflict(symbol, "wrong-kind"));
    }
    let from_module = symbol
        .split('.')
        .next()
        .ok_or(ChangeError::WriteBoundaryDenied)?;
    let destination = find_definition(&load.documents, to_module)?;
    if definition_kind_of(destination) != "module" {
        return Err(conflict(to_module, "wrong-kind"));
    }
    let mut segments: Vec<&str> = symbol.split('.').collect();
    segments[0] = to_module;
    if let Some(name) = to_name {
        *segments
            .last_mut()
            .ok_or(ChangeError::WriteBoundaryDenied)? = name;
    }
    let target = segments.join(".");
    if !symbol_id_valid(&target) || target.len() > 192 {
        return Err(conflict(&target, "symbol-grammar"));
    }
    if target == *symbol {
        return Err(conflict(symbol, "move-same-identity"));
    }
    if find_definition(&load.documents, &target).is_ok() {
        return Err(conflict(&target, "symbol-duplicate"));
    }
    let registry = load
        .model
        .project
        .as_ref()
        .and_then(|project| project.node.get("id_registry"));
    if registry.is_some_and(|registry| reserved(registry, &target)) {
        return Err(conflict(&target, "historical-id"));
    }
    // A local error registry is a separate owner. Until its relocation
    // protocol lands, never leave operation/payload/source metadata stale.
    if base.error_source.is_some() {
        return Err(unsupported("error-registry-relocation"));
    }
    let guards = Guards;
    guards.check(&base.root)?;
    let cross_module = from_module != to_module;
    if cross_module
        && (source.node.get("relations").is_some()
            || source.node.get("uniqueConstraints").is_some())
    {
        return Err(unsupported("nested-identity-relocation"));
    }
    let source_doc = load
        .documents
        .iter()
        .position(|doc| {
            doc.document
                .definitions
                .iter()
                .any(|definition| definition.id == *symbol)
        })
        .ok_or(ChangeError::WriteBoundaryDenied)?;
    if cross_module && load.documents[source_doc].document.definitions.len() == 1 {
        return Err(unsupported("last-definition-relocation"));
    }
    let destination_doc = load
        .documents
        .iter()
        .find(|doc| {
            doc.document
                .definitions
                .iter()
                .any(|definition| definition.id == *to_module)
        })
        .ok_or(ChangeError::WriteBoundaryDenied)?;
    let home = destination_doc
        .document
        .path
        .rsplit_once('/')
        .ok_or(ChangeError::WriteBoundaryDenied)?
        .0;
    let leaf = load.documents[source_doc]
        .document
        .path
        .rsplit_once('/')
        .ok_or(ChangeError::WriteBoundaryDenied)?
        .1;
    let target_path = format!("{home}/{leaf}");

    let mut documents = load.documents.clone();
    let mut prospective = BTreeMap::<String, Vec<u8>>::new();
    let mut module_refs = BTreeMap::<String, BTreeSet<String>>::new();
    let version = bumped(
        source
            .node
            .get("version")
            .ok_or(ChangeError::WriteBoundaryDenied)?,
    )?;
    for document in &mut documents {
        let mut edits = Vec::new();
        for definition in &mut document.document.definitions {
            if definition_kind_of(definition) == "module" {
                continue;
            }
            if definition_kind_of(definition) == "project" {
                // Tombstone replacement targets must remain exact live IDs;
                // rename_history edges remain historical and are not aliases.
                if let Some(tombstones) = definition
                    .node
                    .get_mut("id_registry")
                    .and_then(|node| node.get_mut("tombstones"))
                    .and_then(Node::as_seq_mut)
                {
                    for tombstone in tombstones {
                        if let Some(replacement) = tombstone.get_mut("replaced_by") {
                            if replacement.as_str() == Some(symbol) {
                                set_string(&document.bytes, replacement, &target, &mut edits)?;
                            }
                        }
                    }
                }
                let edge = serde_json::json!({"from":symbol,"to":target,"definition_version":version,"same_identity":true});
                add_history(&document.bytes, &mut definition.node, edge, &mut edits)?;
                bump(&document.bytes, &mut definition.node, &mut edits)?;
                continue;
            }
            let normalized = load
                .model
                .definitions
                .iter()
                .find(|entry| entry.id == definition.id)
                .ok_or(ChangeError::WriteBoundaryDenied)?;
            let moving = definition.id == *symbol;
            let owner = if moving {
                to_module.as_str()
            } else {
                definition.id.split('.').next().unwrap_or("")
            };
            let refs = module_refs.entry(owner.to_owned()).or_default();
            let before_edits = edits.len();
            rewrite_references(
                &document.bytes,
                &mut definition.node,
                &normalized.node,
                symbol,
                &target,
                moving && cross_module,
                refs,
                &mut edits,
            )?;
            if moving {
                let id = definition
                    .node
                    .get_mut("id")
                    .ok_or(ChangeError::WriteBoundaryDenied)?;
                set_string(&document.bytes, id, &target, &mut edits)?;
                definition.id = target.clone();
                set_member(
                    &document.bytes,
                    &mut definition.node,
                    "renamed_from",
                    serde_json::json!([symbol]),
                    &mut edits,
                )?;
            }
            if moving || before_edits != edits.len() {
                bump(&document.bytes, &mut definition.node, &mut edits)?;
            }
        }
        if !edits.is_empty() {
            prospective.insert(
                document.document.path.clone(),
                apply_checked(&document.bytes, &edits)?,
            );
        }
    }

    // Add only newly required imports. Existing declared boundaries are
    // preserved; a resulting dependency cycle is refused by the loader.
    for document in &mut documents {
        for definition in &mut document.document.definitions {
            if definition_kind_of(definition) != "module" {
                continue;
            }
            let mut imports: BTreeSet<String> = definition
                .node
                .get("imports")
                .and_then(Node::as_seq)
                .unwrap_or(&[])
                .iter()
                .filter_map(Node::as_str)
                .map(str::to_owned)
                .collect();
            let before = imports.clone();
            for reference in module_refs.get(&definition.id).into_iter().flatten() {
                if let Some(module) = reference.split('.').next() {
                    if module != definition.id {
                        imports.insert(module.to_owned());
                    }
                }
            }
            if imports != before {
                let mut edits = Vec::new();
                set_member(
                    &document.bytes,
                    &mut definition.node,
                    "imports",
                    serde_json::json!(imports),
                    &mut edits,
                )?;
                bump(&document.bytes, &mut definition.node, &mut edits)?;
                prospective.insert(
                    document.document.path.clone(),
                    apply_checked(&document.bytes, &edits)?,
                );
            }
        }
    }

    let mut creations = Vec::new();
    if cross_module {
        let source_path = documents[source_doc].document.path.clone();
        let bytes = prospective
            .get(&source_path)
            .ok_or(ChangeError::WriteBoundaryDenied)?
            .clone();
        let decoded = crate::loader::decode_document_bytes(&source_path, DocKind::Symbol, &bytes)
            .map_err(|_| preservation(symbol))?;
        let moved = decoded
            .definitions
            .iter()
            .find(|definition| definition.id == target)
            .ok_or(ChangeError::WriteBoundaryDenied)?;
        let (cut, block) = edit::plan_definition_cut(&bytes, &moved.node)?;
        prospective.insert(source_path, cut.apply(&bytes));
        let position = documents[source_doc]
            .document
            .definitions
            .iter()
            .position(|definition| definition.id == target)
            .ok_or(ChangeError::WriteBoundaryDenied)?;
        let moved = documents[source_doc].document.definitions.remove(position);
        if let Some(index) = documents
            .iter()
            .position(|doc| doc.document.path == target_path)
        {
            let bytes = prospective
                .get(&target_path)
                .unwrap_or(&documents[index].bytes);
            let decoded =
                crate::loader::decode_document_bytes(&target_path, DocKind::Symbol, bytes)
                    .map_err(|_| preservation(&target_path))?;
            let nodes: Vec<&Node> = decoded
                .definitions
                .iter()
                .map(|definition| &definition.node)
                .collect();
            // Reindent only declaration indentation; block scalar and nested
            // members retain their relative indentation and newline bytes.
            let indent = decoded
                .definitions
                .first()
                .map(|definition| {
                    definition
                        .node
                        .get_entry("id")
                        .map(|entry| entry.key_span)
                        .unwrap_or(definition.node.span)
                        .start
                        .column
                        .saturating_sub(3)
                })
                .unwrap_or(2);
            let block = reindent(&block, indent)?;
            let newline = newline(bytes);
            let patch =
                edit::plan_document_definition_append(bytes, &nodes, &format!("{newline}{block}"))?;
            prospective.insert(target_path, patch.apply(bytes));
            documents[index].document.definitions.push(moved);
        } else {
            let newline = newline(&bytes);
            let block = reindent(&block, 2)?;
            let rendered = format!(
                "schema_version: \"{}\"{newline}definitions:{newline}{block}{newline}",
                load.model.model_version.as_str()
            );
            let mut document = documents[source_doc].document.clone();
            document.path = target_path.clone();
            document.definitions = vec![moved];
            documents.push(LoadedDocument {
                document,
                bytes: Vec::new(),
            });
            prospective.insert(target_path.clone(), rendered.into_bytes());
            creations.push(target_path);
        }
    }
    let audit = ResolvedOperation {
        operation: "symbol:move".to_owned(),
        subject: symbol.clone(),
        subject_family: kind,
        status: "resolved".to_owned(),
        detail: None,
    };
    let mut prepared = planning::finish(
        load,
        request,
        options,
        base,
        planning::Sources {
            documents,
            prospective: prospective.into_iter().collect(),
            creations,
            resolved_rows: vec![audit],
            additional_pins: guards.pins(),
        },
    )?;
    prepared.symbol_move = Some(guards);
    Ok(prepared)
}

fn reserved(registry: &Node, target: &str) -> bool {
    ["rename_history", "tombstones"].iter().any(|member| {
        registry
            .get(member)
            .and_then(Node::as_seq)
            .unwrap_or(&[])
            .iter()
            .any(|entry| {
                ["from", "to", "id"]
                    .iter()
                    .any(|key| entry.get(key).and_then(Node::as_str) == Some(target))
            })
    })
}

fn bumped(node: &Node) -> Result<i128, ChangeError> {
    let Value::Scalar(Scalar::Int(value)) = &node.value else {
        return Err(ChangeError::WriteBoundaryDenied);
    };
    value
        .checked_add(1)
        .filter(|value| *value <= u64::MAX as i128)
        .ok_or_else(|| conflict("version", "version-overflow"))
}

fn bump(_bytes: &[u8], node: &mut Node, edits: &mut Vec<ByteEdit>) -> Result<(), ChangeError> {
    let node = node
        .get_mut("version")
        .ok_or(ChangeError::WriteBoundaryDenied)?;
    let value = bumped(node)?;
    edits.push(ByteEdit {
        start: node.span.start.byte,
        end: node.span.end.byte,
        replacement: value.to_string(),
    });
    node.value = Value::Scalar(Scalar::Int(value));
    Ok(())
}

fn set_string(
    bytes: &[u8],
    node: &mut Node,
    value: &str,
    edits: &mut Vec<ByteEdit>,
) -> Result<(), ChangeError> {
    if node.as_str() == Some(value) {
        return Ok(());
    }
    let slice = bytes
        .get(node.span.start.byte..node.span.end.byte)
        .ok_or(ChangeError::WriteBoundaryDenied)?;
    let replacement = if slice.first() == Some(&b'\'') {
        format!("'{}'", value.replace('\'', "''"))
    } else if slice.first() == Some(&b'\"') {
        serde_json::to_string(value).map_err(|_| ChangeError::WriteBoundaryDenied)?
    } else {
        value.to_owned()
    };
    edits.push(ByteEdit {
        start: node.span.start.byte,
        end: node.span.end.byte,
        replacement,
    });
    node.value = Value::Scalar(Scalar::Str(value.to_owned()));
    Ok(())
}

fn normalized_ref(node: &Node) -> Option<&str> {
    node.as_str()
        .or_else(|| node.get("ref").and_then(Node::as_str))
}

#[allow(clippy::too_many_arguments)]
fn rewrite_references(
    bytes: &[u8],
    raw: &mut Node,
    normalized: &Node,
    old: &str,
    new: &str,
    qualify: bool,
    refs: &mut BTreeSet<String>,
    edits: &mut Vec<ByteEdit>,
) -> Result<(), ChangeError> {
    for key in ["fields", "input", "payload"] {
        if let (Some(raw_items), Some(normalized_items)) = (
            raw.get_mut(key).and_then(Node::as_seq_mut),
            normalized.get(key).and_then(Node::as_seq),
        ) {
            for (field, normalized_field) in raw_items.iter_mut().zip(normalized_items) {
                if let (Some(raw_type), Some(normalized_type)) =
                    (field.get_mut("type"), normalized_field.get("type"))
                {
                    rewrite_type(
                        bytes,
                        raw_type,
                        normalized_type,
                        old,
                        new,
                        qualify,
                        refs,
                        edits,
                    )?;
                }
            }
        }
    }
    if let (Some(raw), Some(normalized)) = (raw.get_mut("returns"), normalized.get("returns")) {
        rewrite_type(bytes, raw, normalized, old, new, qualify, refs, edits)?;
    }
    for key in ["effects", "reads", "applies_to", "emits", "covers"] {
        if let (Some(raw), Some(normalized)) = (
            raw.get_mut(key).and_then(Node::as_seq_mut),
            normalized.get(key).and_then(Node::as_seq),
        ) {
            for (raw, normalized) in raw.iter_mut().zip(normalized) {
                rewrite_ref(bytes, raw, normalized, old, new, qualify, refs, edits)?;
            }
        }
    }
    for key in ["entity", "invokes"] {
        if let (Some(raw), Some(normalized)) = (raw.get_mut(key), normalized.get(key)) {
            rewrite_ref(bytes, raw, normalized, old, new, qualify, refs, edits)?;
        }
    }
    if let (Some(raw), Some(normalized)) = (
        raw.get_mut("relations").and_then(Node::as_seq_mut),
        normalized.get("relations").and_then(Node::as_seq),
    ) {
        for (raw, normalized) in raw.iter_mut().zip(normalized) {
            if let (Some(raw), Some(normalized)) = (raw.get_mut("target"), normalized.get("target"))
            {
                rewrite_ref(bytes, raw, normalized, old, new, qualify, refs, edits)?;
            }
        }
    }
    if let (Some(raw), Some(normalized)) = (
        raw.get_mut("uniqueConstraints").and_then(Node::as_seq_mut),
        normalized.get("uniqueConstraints").and_then(Node::as_seq),
    ) {
        for (raw, normalized) in raw.iter_mut().zip(normalized) {
            if let (Some(raw), Some(normalized)) = (
                raw.get_mut("normalizationRef"),
                normalized.get("normalizationRef"),
            ) {
                rewrite_ref(bytes, raw, normalized, old, new, qualify, refs, edits)?;
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn rewrite_ref(
    bytes: &[u8],
    raw: &mut Node,
    normalized: &Node,
    old: &str,
    new: &str,
    qualify: bool,
    refs: &mut BTreeSet<String>,
    edits: &mut Vec<ByteEdit>,
) -> Result<(), ChangeError> {
    let reference = normalized_ref(normalized).ok_or(ChangeError::WriteBoundaryDenied)?;
    let result = if reference == old { new } else { reference };
    refs.insert(result.to_owned());
    if reference == old || qualify {
        set_string(bytes, raw, result, edits)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn rewrite_type(
    bytes: &[u8],
    raw: &mut Node,
    normalized: &Node,
    old: &str,
    new: &str,
    qualify: bool,
    refs: &mut BTreeSet<String>,
    edits: &mut Vec<ByteEdit>,
) -> Result<(), ChangeError> {
    if raw.as_str().is_some() {
        let (spelling, changed) = compact_type(normalized, old, new, refs)?;
        if changed || qualify {
            set_string(bytes, raw, &spelling, edits)?;
        }
    } else if let Value::Map(entries) = &mut raw.value {
        for entry in entries {
            let normalized = normalized
                .get(&entry.key)
                .ok_or(ChangeError::WriteBoundaryDenied)?;
            if entry.key == "ref" {
                rewrite_ref(
                    bytes,
                    &mut entry.value,
                    normalized,
                    old,
                    new,
                    qualify,
                    refs,
                    edits,
                )?;
            } else {
                rewrite_type(
                    bytes,
                    &mut entry.value,
                    normalized,
                    old,
                    new,
                    qualify,
                    refs,
                    edits,
                )?;
            }
        }
    } else {
        return Err(ChangeError::WriteBoundaryDenied);
    }
    Ok(())
}

fn compact_type(
    node: &Node,
    old: &str,
    new: &str,
    refs: &mut BTreeSet<String>,
) -> Result<(String, bool), ChangeError> {
    if let Some(reference) = node.get("ref").and_then(Node::as_str) {
        let result = if reference == old { new } else { reference };
        refs.insert(result.to_owned());
        return Ok((result.to_owned(), reference == old));
    }
    for key in ["list", "optional"] {
        if let Some(inner) = node.get(key) {
            let (inner, changed) = compact_type(inner, old, new, refs)?;
            return Ok((
                if key == "list" {
                    format!("list<{inner}>")
                } else {
                    format!("{inner}?")
                },
                changed,
            ));
        }
    }
    Err(ChangeError::WriteBoundaryDenied)
}

fn newline(bytes: &[u8]) -> &str {
    if bytes.windows(2).any(|pair| pair == b"\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn json_node(value: &Json) -> Node {
    let span = placeholder_span();
    Node {
        span,
        value: match value {
            Json::Object(map) => Value::Map(
                map.iter()
                    .map(|(key, value)| MapEntry {
                        key: key.clone(),
                        key_span: span,
                        value: json_node(value),
                    })
                    .collect(),
            ),
            Json::Array(items) => Value::Seq(items.iter().map(json_node).collect()),
            Json::String(text) => Value::Scalar(Scalar::Str(text.clone())),
            Json::Bool(value) => Value::Scalar(Scalar::Bool(*value)),
            Json::Number(number) => {
                Value::Scalar(Scalar::Int(number.as_u64().unwrap_or_default() as i128))
            }
            Json::Null => Value::Scalar(Scalar::Null),
        },
    }
}

fn yaml_lines(value: &Json, indent: usize) -> Vec<String> {
    let prefix = " ".repeat(indent);
    match value {
        Json::Object(map) => map
            .iter()
            .flat_map(|(key, value)| {
                if value.is_object() || value.is_array() {
                    let mut lines = vec![format!("{prefix}{key}:")];
                    lines.extend(yaml_lines(value, indent + 2));
                    lines
                } else {
                    vec![format!("{prefix}{key}: {value}")]
                }
            })
            .collect(),
        Json::Array(items) => items
            .iter()
            .flat_map(|value| {
                let mut lines = yaml_lines(value, indent + 2);
                if let Some(first) = lines.first_mut() {
                    *first = format!("{prefix}- {}", first.trim_start());
                }
                lines
            })
            .collect(),
        _ => vec![format!("{prefix}{value}")],
    }
}

fn is_json_map(bytes: &[u8], node: &Node) -> bool {
    bytes.get(node.span.start.byte) == Some(&b'{')
}

fn set_member(
    bytes: &[u8],
    parent: &mut Node,
    member: &str,
    value: Json,
    edits: &mut Vec<ByteEdit>,
) -> Result<(), ChangeError> {
    let existing = parent.get(member);
    if is_json_map(bytes, parent) {
        if let Some(existing) = existing {
            edits.push(ByteEdit {
                start: existing.span.start.byte,
                end: existing.span.end.byte,
                replacement: value.to_string(),
            });
        } else {
            let end = parent
                .span
                .end
                .byte
                .checked_sub(1)
                .ok_or(ChangeError::WriteBoundaryDenied)?;
            if bytes.get(end) != Some(&b'}') {
                return Err(preservation(member));
            }
            edits.push(ByteEdit {
                start: end,
                end,
                replacement: format!(
                    ", {}: {}",
                    serde_json::to_string(member).map_err(|_| ChangeError::WriteBoundaryDenied)?,
                    value
                ),
            });
        }
    } else {
        let Value::Map(entries) = &parent.value else {
            return Err(preservation(member));
        };
        let indent = entries
            .last()
            .ok_or(ChangeError::WriteBoundaryDenied)?
            .key_span
            .start
            .column
            - 1;
        let suffix = if value.is_object() || value.is_array() {
            format!(
                "{}{}",
                newline(bytes),
                yaml_lines(&value, indent + 2).join(newline(bytes))
            )
        } else {
            format!(" {value}")
        };
        edits.push(if existing.is_some() {
            edit::plan_raw_member_replace(bytes, parent, member, &suffix)?
        } else {
            edit::plan_raw_member_append(bytes, parent, member, &suffix)?
        });
    }
    let Value::Map(entries) = &mut parent.value else {
        return Err(preservation(member));
    };
    if let Some(entry) = entries.iter_mut().find(|entry| entry.key == member) {
        entry.value = json_node(&value);
    } else {
        entries.push(MapEntry {
            key: member.to_owned(),
            key_span: placeholder_span(),
            value: json_node(&value),
        });
    }
    Ok(())
}

fn add_history(
    bytes: &[u8],
    project: &mut Node,
    edge: Json,
    edits: &mut Vec<ByteEdit>,
) -> Result<(), ChangeError> {
    let Some(registry) = project.get_mut("id_registry") else {
        return set_member(
            bytes,
            project,
            "id_registry",
            serde_json::json!({"rename_history":[edge]}),
            edits,
        );
    };
    let Some(history) = registry.get("rename_history") else {
        return set_member(
            bytes,
            registry,
            "rename_history",
            serde_json::json!([edge]),
            edits,
        );
    };
    if bytes.get(history.span.start.byte) == Some(&b'[') {
        let end = history
            .span
            .end
            .byte
            .checked_sub(1)
            .ok_or(ChangeError::WriteBoundaryDenied)?;
        if bytes.get(end) != Some(&b']') {
            return Err(preservation("rename_history"));
        }
        edits.push(ByteEdit {
            start: end,
            end,
            replacement: format!(", {edge}"),
        });
    } else {
        let nodes = history.as_seq().ok_or(ChangeError::WriteBoundaryDenied)?;
        let indent = nodes
            .first()
            .and_then(|node| {
                let Value::Map(entries) = &node.value else {
                    return None;
                };
                entries
                    .first()
                    .map(|entry| entry.key_span.start.column.saturating_sub(3))
            })
            .ok_or(ChangeError::WriteBoundaryDenied)?;
        let rendered = format!(
            "{}{}",
            newline(bytes),
            yaml_lines(&serde_json::json!([edge]), indent).join(newline(bytes))
        );
        edits.push(edit::plan_seq_insert(
            bytes,
            registry,
            "rename_history",
            &rendered,
        )?);
    }
    registry
        .get_mut("rename_history")
        .ok_or(ChangeError::WriteBoundaryDenied)?
        .as_seq_mut()
        .ok_or(ChangeError::WriteBoundaryDenied)?
        .push(json_node(&edge));
    Ok(())
}

fn apply_checked(bytes: &[u8], edits: &[ByteEdit]) -> Result<Vec<u8>, ChangeError> {
    let mut sorted: Vec<&ByteEdit> = edits.iter().collect();
    sorted.sort_by_key(|edit| (edit.start, edit.end));
    if sorted
        .iter()
        .any(|edit| edit.start > edit.end || edit.end > bytes.len())
        || sorted
            .windows(2)
            .any(|pair| pair[0].end > pair[1].start || pair[0].start == pair[1].start)
    {
        return Err(preservation("overlapping-edits"));
    }
    Ok(ByteEdit::apply_all(bytes, edits))
}

fn reindent(block: &str, indent: usize) -> Result<String, ChangeError> {
    let original = block.bytes().take_while(|byte| *byte == b' ').count();
    let mut output = String::new();
    for line in block.split_inclusive('\n') {
        if line.trim().is_empty() {
            output.push_str(line);
            continue;
        }
        if !line
            .as_bytes()
            .get(..original)
            .is_some_and(|prefix| prefix.iter().all(|byte| *byte == b' '))
        {
            return Err(preservation("declaration-indent"));
        }
        output.push_str(&" ".repeat(indent));
        output.push_str(&line[original..]);
    }
    Ok(output)
}
