//! Document decode: per-document shape, schema-version extraction, and the
//! exact dual Model-version dispatch (0.1.0 / 1.0.0).

use super::error::{Diagnostic, Span};
use super::frontends::{Node, Scalar, Value};

/// The two exact Model contract versions the loader recognizes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelVersion {
    V0_1_0,
    V1_0_0,
}

impl ModelVersion {
    /// The exact literal accepted in `schema_version`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V0_1_0 => "0.1.0",
            Self::V1_0_0 => "1.0.0",
        }
    }

    /// Exhaustive literal dispatch; no ranges, no prerelease forms.
    pub fn parse_exact(literal: &str) -> Option<Self> {
        match literal {
            "0.1.0" => Some(Self::V0_1_0),
            "1.0.0" => Some(Self::V1_0_0),
            _ => None,
        }
    }

    /// The module-ID grammar of the active Model version.
    ///
    /// Model 0.1.0 module names allow hyphens
    /// (`^[a-z][a-z0-9_-]{0,62}$`); Model 1.0.0 semantic module IDs do not
    /// (`^[a-z][a-z0-9_]{0,62}$`). Reserved words are the #6 validator's
    /// concern and are accepted here.
    pub fn module_id_valid(self, id: &str) -> bool {
        let bytes = id.as_bytes();
        let Some((&first, rest)) = bytes.split_first() else {
            return false;
        };
        if !first.is_ascii_lowercase() || bytes.len() > 63 {
            return false;
        }
        rest.iter().all(|byte| match self {
            Self::V0_1_0 => {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_' || *byte == b'-'
            }
            Self::V1_0_0 => byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_',
        })
    }
}

/// Which canonical home a document came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocKind {
    Project,
    Module,
    /// One of the seven optional kind documents.
    Symbol,
}

/// One decoded definition: the `id` plus the whole spanned node.
#[derive(Clone, Debug)]
pub struct Definition {
    pub id: String,
    pub id_span: Span,
    pub node: Node,
    /// Span of the definition mapping itself.
    pub span: Span,
}

/// One decoded source document.
#[derive(Clone, Debug)]
pub struct Document {
    /// Logical project-relative POSIX path.
    pub path: String,
    pub kind: DocKind,
    /// The literal `schema_version` string as written.
    pub version: String,
    pub version_span: Span,
    pub definitions: Vec<Definition>,
}

/// The single definition of a project or module document must be present
/// exactly once; kind documents require a non-empty array.
fn decode_document(path: &str, kind: DocKind, root: &Node) -> Result<Document, Vec<Diagnostic>> {
    let shape = |span: Span, detail: &str| {
        Diagnostic::new("loader.document-shape")
            .with_path(path)
            .with_span(span)
            .with_data(serde_json::json!({ "detail": detail }))
    };
    let _entries = match root.as_map() {
        Some(entries) => entries,
        None => {
            return Err(vec![shape(
                root.span,
                match root.value {
                    Value::Seq(_) => "root-array",
                    _ => "root-not-mapping",
                },
            )])
        }
    };
    let version_entry = root
        .get_entry("schema_version")
        .ok_or_else(|| vec![shape(root.span, "schema-version-missing")])?;
    let version = match &version_entry.value.value {
        Value::Scalar(Scalar::Str(text)) => text.clone(),
        other => {
            return Err(vec![shape(
                version_entry.value.span,
                match other {
                    Value::Scalar(_) => "schema-version-not-string",
                    _ => "schema-version-not-string",
                },
            )])
        }
    };
    let definitions_entry = root
        .get_entry("definitions")
        .ok_or_else(|| vec![shape(root.span, "definitions-missing")])?;
    let items = match definitions_entry.value.as_seq() {
        Some(items) => items,
        None => {
            return Err(vec![shape(
                definitions_entry.value.span,
                "definitions-not-array",
            )])
        }
    };
    if items.is_empty() {
        return Err(vec![shape(
            definitions_entry.value.span,
            "definitions-empty",
        )]);
    }
    if matches!(kind, DocKind::Project | DocKind::Module) && items.len() != 1 {
        return Err(vec![shape(
            definitions_entry.value.span,
            "definitions-count",
        )]);
    }
    let mut definitions = Vec::with_capacity(items.len());
    for item in items {
        let id_entry = item
            .get_entry("id")
            .ok_or_else(|| vec![shape(item.span, "definition-id-missing")])?;
        let id = match id_entry.value.as_str() {
            Some(text) => text.to_owned(),
            None => return Err(vec![shape(id_entry.value.span, "definition-id-not-string")]),
        };
        definitions.push(Definition {
            id,
            id_span: id_entry.value.span,
            node: item.clone(),
            span: item.span,
        });
    }
    Ok(Document {
        path: path.to_owned(),
        kind,
        version,
        version_span: version_entry.value.span,
        definitions,
    })
}

/// Decode a parsed document tree; `None` for the empty-document case.
pub fn decode(
    path: &str,
    kind: DocKind,
    parsed: &super::frontends::Parsed,
) -> Result<Document, Vec<Diagnostic>> {
    match parsed {
        super::frontends::Parsed::Empty => Err(vec![Diagnostic::new("loader.document-shape")
            .with_path(path)
            .with_data(serde_json::json!({ "detail": "empty-document" }))]),
        super::frontends::Parsed::Root(root) => decode_document(path, kind, root),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::frontends;
    use crate::loader::frontends::Parsed;
    use crate::loader::source::LineIndex;

    fn parse_json(text: &str) -> Parsed {
        let index = LineIndex::new(text);
        frontends::json::parse(text, &index).unwrap()
    }

    #[test]
    fn exact_version_dispatch_recognizes_only_the_two_literals() {
        assert_eq!(
            ModelVersion::parse_exact("0.1.0"),
            Some(ModelVersion::V0_1_0)
        );
        assert_eq!(
            ModelVersion::parse_exact("1.0.0"),
            Some(ModelVersion::V1_0_0)
        );
        for bogus in ["v1", "1", "1.x.0", "2.0.0", "0.1.0-rc1", "0.1", "01.0.0"] {
            assert_eq!(ModelVersion::parse_exact(bogus), None, "{bogus}");
        }
    }

    #[test]
    fn module_id_grammar_differs_by_model_version() {
        assert!(ModelVersion::V0_1_0.module_id_valid("work-items"));
        assert!(!ModelVersion::V1_0_0.module_id_valid("work-items"));
        assert!(ModelVersion::V1_0_0.module_id_valid("planner_v2"));
        assert!(!ModelVersion::V0_1_0.module_id_valid("Planner"));
        assert!(!ModelVersion::V1_0_0.module_id_valid(""));
        assert!(!ModelVersion::V1_0_0.module_id_valid("a.b"));
        let long = "a".repeat(64);
        assert!(!ModelVersion::V1_0_0.module_id_valid(&long));
        let max = "a".repeat(63);
        assert!(ModelVersion::V1_0_0.module_id_valid(&max));
    }

    #[test]
    fn document_decode_enforces_shape_rules() {
        let good = parse_json(
            "{\"schema_version\":\"1.0.0\",\"definitions\":[{\"id\":\"planner\",\"kind\":\"module\",\"version\":1}]}",
        );
        let document =
            decode("lekalo/modules/planner/module.yaml", DocKind::Module, &good).unwrap();
        assert_eq!(document.version, "1.0.0");
        assert_eq!(document.definitions.len(), 1);
        assert_eq!(document.definitions[0].id, "planner");

        let cases = [
            ("{\"definitions\":[]}", "schema-version-missing"),
            (
                "{\"schema_version\":1,\"definitions\":[]}",
                "schema-version-not-string",
            ),
            ("{\"schema_version\":\"1.0.0\"}", "definitions-missing"),
            (
                "{\"schema_version\":\"1.0.0\",\"definitions\":{}}",
                "definitions-not-array",
            ),
            (
                "{\"schema_version\":\"1.0.0\",\"definitions\":[]}",
                "definitions-empty",
            ),
            (
                "{\"schema_version\":\"1.0.0\",\"definitions\":[{\"kind\":\"module\"}]}",
                "definition-id-missing",
            ),
            (
                "{\"schema_version\":\"1.0.0\",\"definitions\":[{\"id\":3}]}",
                "definition-id-not-string",
            ),
        ];
        for (text, detail) in cases {
            let parsed = parse_json(text);
            let error = decode("d.yaml", DocKind::Module, &parsed).unwrap_err();
            assert_eq!(error[0].code, "loader.document-shape");
            assert_eq!(error[0].data.as_ref().unwrap()["detail"], detail);
        }
        // Two definitions in a module document are rejected.
        let two = parse_json(
            "{\"schema_version\":\"1.0.0\",\"definitions\":[{\"id\":\"a\"},{\"id\":\"b\"}]}",
        );
        let error = decode("d.yaml", DocKind::Module, &two).unwrap_err();
        assert_eq!(
            error[0].data.as_ref().unwrap()["detail"],
            "definitions-count"
        );
        // Kind documents accept many definitions.
        let many = decode("d.yaml", DocKind::Symbol, &two).unwrap();
        assert_eq!(many.definitions.len(), 2);
    }

    #[test]
    fn root_arrays_and_empty_documents_are_shape_errors() {
        let parsed = parse_json("[1,2]");
        let error = decode("d.yaml", DocKind::Project, &parsed).unwrap_err();
        assert_eq!(error[0].code, "loader.document-shape");
        assert_eq!(error[0].data.as_ref().unwrap()["detail"], "root-array");
        let error = decode("d.yaml", DocKind::Project, &Parsed::Empty).unwrap_err();
        assert_eq!(error[0].data.as_ref().unwrap()["detail"], "empty-document");
    }
}
