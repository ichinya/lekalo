//! The five issue #33 resource families, their logical URIs, and the
//! bounded pagination cursors.
//!
//! Resource identities are logical: `lekalo://PROJECT/family[/qualifier]`
//! with bounded percent-encoding of semantic components. No `file://`
//! resource, arbitrary glob/read-file tool, recursive repository listing,
//! or discoverable private absolute root exists. Enumeration discloses
//! only authorized identities; cursors are opaque, bound to one snapshot
//! revision, and refused once stale.

use serde::Serialize;

use super::policy;

/// The five resource families, in disclosure order.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Family {
    ProjectSummary,
    ModuleList,
    SymbolMetadata,
    TargetStatus,
    DiagnosticsSummary,
}

/// The exact family keys, in catalog order.
pub const FAMILY_KEYS: [&str; 5] = [
    "project-summary",
    "module-list",
    "symbol-metadata",
    "target-status",
    "diagnostics",
];

impl Family {
    /// The exact family key used in URIs and the resource contract.
    pub const fn key(self) -> &'static str {
        match self {
            Self::ProjectSummary => "project-summary",
            Self::ModuleList => "modules",
            Self::SymbolMetadata => "symbol",
            Self::TargetStatus => "targets",
            Self::DiagnosticsSummary => "diagnostics",
        }
    }

    /// The URI template advertised through `resources/templates/list`.
    pub const fn uri_template(self) -> &'static str {
        match self {
            Self::ProjectSummary => "lekalo://{projectId}/summary",
            Self::ModuleList => "lekalo://{projectId}/modules",
            Self::SymbolMetadata => "lekalo://{projectId}/symbol/{symbol}",
            Self::TargetStatus => "lekalo://{projectId}/targets",
            Self::DiagnosticsSummary => "lekalo://{projectId}/diagnostics",
        }
    }

    /// The catalog family for one URI family key.
    pub fn parse_key(key: &str) -> Option<Self> {
        match key {
            "summary" | "project-summary" => Some(Self::ProjectSummary),
            "modules" | "module-list" => Some(Self::ModuleList),
            "symbol" | "symbol-metadata" => Some(Self::SymbolMetadata),
            "targets" | "target-status" => Some(Self::TargetStatus),
            "diagnostics" => Some(Self::DiagnosticsSummary),
            _ => None,
        }
    }

    /// Whether this family takes a semantic qualifier (a symbol id).
    pub const fn qualified(self) -> bool {
        matches!(self, Self::SymbolMetadata)
    }
}

/// Percent-encode one semantic URI component with an explicit closed
/// safe set; everything else is `%XX` uppercase hex.
pub fn encode_component(component: &str) -> String {
    let mut encoded = String::with_capacity(component.len());
    for byte in component.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' => {
                encoded.push(*byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

/// Decode one percent-encoded component; invalid escapes are a refusal.
pub fn decode_component(component: &str) -> Option<String> {
    let bytes = component.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes.get(index + 1..index + 3)?;
            let value = u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
            decoded.push(value);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

/// Build one logical resource URI.
pub fn resource_uri(project: &str, family: Family, qualifier: Option<&str>) -> String {
    build_uri(
        &UriScope {
            project: project.to_owned(),
            group: None,
        },
        family,
        qualifier,
    )
}

/// Build one group-qualified logical resource URI.
pub fn group_resource_uri(
    project: &str,
    group: &str,
    family: Family,
    qualifier: Option<&str>,
) -> String {
    build_uri(
        &UriScope {
            project: project.to_owned(),
            group: Some(group.to_owned()),
        },
        family,
        qualifier,
    )
}

/// Assemble one URI from a parsed scope (also the resource dispatcher's
/// re-projection seam).
pub fn build_uri(scope: &UriScope, family: Family, qualifier: Option<&str>) -> String {
    let prefix = match &scope.group {
        Some(group) => format!(
            "lekalo://{}/g/{}",
            encode_component(&scope.project),
            encode_component(group)
        ),
        None => format!("lekalo://{}", encode_component(&scope.project)),
    };
    match qualifier {
        Some(qualifier) => format!("{prefix}/{}/{}", family.key(), encode_component(qualifier)),
        None => format!("{prefix}/{}", family.key()),
    }
}

/// The semantic scope of one resource URI: the project plus an optional
/// catalog-defined group qualifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UriScope {
    pub project: String,
    pub group: Option<String>,
}

/// Why one URI failed resource admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UriRefusal {
    /// Not a `lekalo://` resource URI.
    Scheme,
    /// The project id is outside the closed grammar.
    Project,
    /// The family key is unknown.
    Family,
    /// A qualified family lacks its qualifier, or vice versa.
    Qualifier,
}

/// Parse and admit one logical resource URI against the closed grammar.
///
/// Group-qualified identities use the reserved `g/<groupId>` segment:
/// `lekalo://P/g/G/family[/qualifier]`; the group id must satisfy the
/// closed grammar, and membership is checked against the catalog by the
/// caller before any read.
pub fn parse_uri(uri: &str) -> Result<(UriScope, Family, Option<String>), UriRefusal> {
    let rest = uri.strip_prefix("lekalo://").ok_or(UriRefusal::Scheme)?;
    let (project, remainder) = rest.split_once('/').ok_or(UriRefusal::Family)?;
    if policy::admit_id(project).is_err() {
        return Err(UriRefusal::Project);
    }
    let mut scope = UriScope {
        project: project.to_owned(),
        group: None,
    };
    let mut remainder = remainder;
    if let Some(after_g) = remainder.strip_prefix("g/") {
        let (group, rest) = after_g.split_once('/').ok_or(UriRefusal::Family)?;
        if policy::admit_id(group).is_err() {
            return Err(UriRefusal::Qualifier);
        }
        scope.group = Some(group.to_owned());
        remainder = rest;
    }
    let (family_key, qualifier) = match remainder.split_once('/') {
        Some((key, qualifier)) => (key, Some(qualifier)),
        None => (remainder, None),
    };
    let family = Family::parse_key(family_key).ok_or(UriRefusal::Family)?;
    match (family.qualified(), qualifier) {
        (true, Some(qualifier)) => {
            let qualifier = decode_component(qualifier).ok_or(UriRefusal::Qualifier)?;
            if qualifier.is_empty() || qualifier.len() > 192 {
                return Err(UriRefusal::Qualifier);
            }
            if qualifier.contains('/') || qualifier.contains('\\') || qualifier.contains("..") {
                return Err(UriRefusal::Qualifier);
            }
            Ok((scope, family, Some(qualifier)))
        }
        (true, None) => Err(UriRefusal::Qualifier),
        (false, Some(_)) => Err(UriRefusal::Qualifier),
        (false, None) => Ok((scope, family, None)),
    }
}

/// One opaque listing cursor, bound to one snapshot revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cursor {
    /// The snapshot revision this cursor is valid under.
    pub snapshot: String,
    /// The byte offset into the deterministic listing order.
    pub offset: usize,
}

impl Cursor {
    /// Encode the cursor to its opaque wire spelling: hex of the
    /// `snapshot:offset` UTF-8 bytes. Deterministic, reversible, and
    /// never a readable disclosure of the snapshot identity.
    pub fn encode(&self) -> String {
        let raw = format!("{}:{}", self.snapshot, self.offset);
        raw.bytes().map(|byte| format!("{byte:02x}")).collect()
    }

    /// Decode one opaque cursor; unrelated spellings are a stale refusal.
    pub fn decode(text: &str) -> Option<Self> {
        if text.is_empty() || text.len() > 1024 || text.len() % 2 != 0 {
            return None;
        }
        let mut raw = Vec::with_capacity(text.len() / 2);
        for pair in text.as_bytes().chunks(2) {
            let hex = std::str::from_utf8(pair).ok()?;
            raw.push(u8::from_str_radix(hex, 16).ok()?);
        }
        let text = String::from_utf8(raw).ok()?;
        let (snapshot, offset) = text.rsplit_once(':')?;
        let offset = offset.parse::<usize>().ok()?;
        Some(Self {
            snapshot: snapshot.to_owned(),
            offset,
        })
    }
}

/// One item of a `resources/list` page (MCP resource descriptor shape).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResourceDescriptor {
    pub uri: String,
    pub name: String,
    #[serde(rename = "mimeType")]
    pub mime_type: &'static str,
}

/// One template of `resources/templates/list`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResourceTemplate {
    #[serde(rename = "uriTemplate")]
    pub uri_template: String,
    pub name: String,
    #[serde(rename = "mimeType")]
    pub mime_type: &'static str,
}

/// The deterministic template list; identical for every catalog.
pub fn templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate {
            uri_template: Family::ProjectSummary.uri_template().to_owned(),
            name: "project-summary".to_owned(),
            mime_type: "application/json",
        },
        ResourceTemplate {
            uri_template: Family::ModuleList.uri_template().to_owned(),
            name: "module-list".to_owned(),
            mime_type: "application/json",
        },
        ResourceTemplate {
            uri_template: Family::SymbolMetadata.uri_template().to_owned(),
            name: "symbol-metadata".to_owned(),
            mime_type: "application/json",
        },
        ResourceTemplate {
            uri_template: Family::TargetStatus.uri_template().to_owned(),
            name: "target-status".to_owned(),
            mime_type: "application/json",
        },
        ResourceTemplate {
            uri_template: Family::DiagnosticsSummary.uri_template().to_owned(),
            name: "diagnostics".to_owned(),
            mime_type: "application/json",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_are_the_exact_five() {
        assert_eq!(FAMILY_KEYS.len(), 5);
        assert_eq!(
            FAMILY_KEYS,
            [
                "project-summary",
                "module-list",
                "symbol-metadata",
                "target-status",
                "diagnostics"
            ]
        );
        assert_eq!(templates().len(), 5);
    }

    #[test]
    fn uris_round_trip_with_encoding() {
        let uri = resource_uri(
            "planner",
            Family::SymbolMetadata,
            Some("planner.focus_task"),
        );
        assert_eq!(uri, "lekalo://planner/symbol/planner.focus_task");
        assert_eq!(
            parse_uri(&uri),
            Ok((
                UriScope {
                    project: "planner".to_owned(),
                    group: None
                },
                Family::SymbolMetadata,
                Some("planner.focus_task".to_owned())
            ))
        );
        let encoded = resource_uri("planner", Family::SymbolMetadata, Some("mod:weird id"));
        assert!(encoded.contains("%3A"), "{encoded}");
        assert!(parse_uri(&encoded).is_ok());
    }

    #[test]
    fn group_qualified_uris_round_trip() {
        let uri = group_resource_uri("planner", "planner-review", Family::ProjectSummary, None);
        assert_eq!(uri, "lekalo://planner/g/planner-review/project-summary");
        let parsed = parse_uri("lekalo://planner/g/planner-review/summary").expect("group uri");
        assert_eq!(parsed.0.project, "planner");
        assert_eq!(parsed.0.group.as_deref(), Some("planner-review"));
        assert_eq!(parsed.1, Family::ProjectSummary);
        // The reserved segment stays closed: an invalid group id refuses.
        assert_eq!(
            parse_uri("lekalo://p/g/Not_Valid/summary"),
            Err(UriRefusal::Qualifier)
        );
        assert_eq!(parse_uri("lekalo://p/g"), Err(UriRefusal::Family));
    }

    #[test]
    fn uris_refuse_foreign_schemes_and_traversal() {
        assert_eq!(parse_uri("file:///etc/passwd"), Err(UriRefusal::Scheme));
        assert_eq!(parse_uri("http://x/y"), Err(UriRefusal::Scheme));
        assert_eq!(parse_uri("lekalo://../summary"), Err(UriRefusal::Project));
        assert_eq!(parse_uri("lekalo://p/unknown"), Err(UriRefusal::Family));
        assert_eq!(
            parse_uri("lekalo://p/summary/extra"),
            Err(UriRefusal::Qualifier)
        );
        assert_eq!(parse_uri("lekalo://p/symbol"), Err(UriRefusal::Qualifier));
        assert_eq!(
            parse_uri("lekalo://p/symbol/..%2F..%2Fetc"),
            Err(UriRefusal::Qualifier)
        );
    }

    #[test]
    fn cursors_are_opaque_and_snapshot_bound() {
        let cursor = Cursor {
            snapshot: "abc".to_owned(),
            offset: 42,
        };
        let encoded = cursor.encode();
        assert!(!encoded.contains("abc"), "{encoded}");
        assert_eq!(Cursor::decode(&encoded), Some(cursor));
        assert_eq!(Cursor::decode("not-a-cursor"), None);
        assert_eq!(Cursor::decode(""), None);
    }
}
