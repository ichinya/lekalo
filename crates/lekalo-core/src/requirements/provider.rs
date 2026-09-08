//! The on-disk OpenSpec requirement provider (issue #36).
//!
//! Reads the accepted/base specs (`<root>/specs/<capability>/spec.md`)
//! and the active change deltas (`<root>/changes/<change-id>/specs/
//! <capability>/spec.md`) through the confined no-follow filesystem
//! capability, derives stable requirement ids
//! (`<capability>.REQ-<title-slug>`), computes the exact body revision
//! digest of every requirement, and projects the effective requirement
//! set: accepted specs with active changes applied in ascending
//! change-directory order.
//!
//! Conflicts never resolve silently: two active changes touching one
//! requirement, an added requirement that already exists, a modified or
//! removed requirement that does not exist, or a duplicate title inside
//! one accepted capability each record an explicit conflict that gates
//! the integration instead of picking a winner. Requirement bodies are
//! never copied — only ids, bounded conflict titles, and digests.
//!
//! Archiving is traceability-neutral by construction: archiving a change
//! moves its applied content into the accepted specs, and the projection
//! of the same content yields the same ids and the same body digests, so
//! a fresh reference stays fresh across the archive.

use super::version;
use super::{is_change_id, requirement_id, sha256_digest, title_slug, ProviderDecl, ProviderKind};

/// The archive subtree of the changes directory; never projected as an
/// active change.
const ARCHIVE_DIR: &str = "archive";

/// The provider walk entry: a resolved snapshot, a tree references do
/// not depend on (absent), or a fatal tree violation.
pub(crate) enum Loaded {
    /// The provider resolved into a usable snapshot.
    Resolved(Snapshot),
    /// The provider root does not exist.
    Absent,
    /// The provider tree exists but violates the v1 contract.
    Invalid(&'static str),
}

/// One resolved requirement of the effective set.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) struct CatalogEntry {
    /// The requirement id (`<capability>.REQ-<slug>`).
    pub id: String,
    /// The exact `sha256:` digest of the canonical requirement body.
    pub digest: String,
    /// Where the effective content came from.
    pub origin: Origin,
    /// The owning change id when `origin` is [`Origin::Change`].
    pub change: Option<String>,
    /// The logical path of the file the content was read from.
    pub path: String,
}

/// The origin of one effective requirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) enum Origin {
    /// Content of the accepted/base specs.
    Accepted,
    /// Content introduced or modified by an active change.
    Change,
}

/// One explicit conflict that blocks resolution of one requirement.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) struct ConflictRow {
    /// The capability the disputed requirement belongs to.
    pub capability: String,
    /// The exact disputed requirement title (bounded identifier data).
    pub title: String,
    /// The fixed conflict classification token.
    pub detail: &'static str,
}

/// The closed provider availability vocabulary of the report wire.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) enum ProviderStatus {
    /// The tree resolved completely.
    Ok,
    /// The tree is absent; references must not depend on it.
    Absent,
}

/// One provider snapshot: the effective requirement set plus every
/// explicit conflict.
pub(crate) struct Snapshot {
    /// The provider namespace.
    pub source: String,
    /// The declared logical root.
    pub root: String,
    /// Whether the tree resolved or is absent.
    pub status: ProviderStatus,
    /// The effective requirements, canonically sorted by id.
    pub entries: Vec<CatalogEntry>,
    /// Every explicit conflict, canonically sorted.
    pub conflicts: Vec<ConflictRow>,
}

impl Snapshot {
    /// The absent snapshot of a provider references do not depend on.
    pub(crate) fn absent(decl: &ProviderDecl) -> Self {
        Self {
            source: decl.source.clone(),
            root: decl.root.clone(),
            status: ProviderStatus::Absent,
            entries: Vec::new(),
            conflicts: Vec::new(),
        }
    }
}

/// One raw requirement block parsed from a spec or delta document.
struct RawBlock {
    capability: String,
    title: String,
    slug: String,
    /// The canonical body bytes the revision digest is computed over.
    body: Vec<u8>,
}

/// The delta section a requirement block belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Section {
    Added,
    Modified,
    Removed,
}

impl Section {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "ADDED Requirements" => Some(Self::Added),
            "MODIFIED Requirements" => Some(Self::Modified),
            "REMOVED Requirements" => Some(Self::Removed),
            _ => None,
        }
    }
}

/// Load one provider snapshot through the confined read-only capability.
pub(crate) fn load_snapshot(fs: &crate::project_fs::Fs, decl: &ProviderDecl) -> Loaded {
    if decl.kind != ProviderKind::Openspec {
        return Loaded::Invalid("provider-kind");
    }
    let specs_dir = format!("{}/specs", decl.root);
    let changes_dir = format!("{}/changes", decl.root);
    let specs = match read_capabilities(fs, &specs_dir) {
        Ok(capabilities) => capabilities,
        Err(TreeError::Missing) => return Loaded::Absent,
        Err(tree_error) => return tree_error.into_loaded(),
    };
    let changes = match read_change_ids(fs, &changes_dir) {
        Ok(changes) => changes,
        Err(TreeError::Missing) => Vec::new(),
        Err(tree_error) => return tree_error.into_loaded(),
    };
    if specs.len() > version::MAX_CAPABILITIES {
        return Loaded::Invalid("capabilities-over-limit");
    }
    if changes.len() > version::MAX_CHANGES {
        return Loaded::Invalid("changes-over-limit");
    }

    // Effective state keyed by (capability, title): the pair the delta
    // sections address requirements by.
    let mut effective: std::collections::BTreeMap<(String, String), CatalogEntry> =
        std::collections::BTreeMap::new();
    let mut conflicts: std::collections::BTreeSet<ConflictRow> = std::collections::BTreeSet::new();

    for (capability, path) in &specs {
        let blocks = match parse_document(fs, path, false) {
            Ok(blocks) => blocks,
            Err(tree_error) => return tree_error.into_loaded(),
        };
        if blocks.len() > version::MAX_REQUIREMENTS {
            return Loaded::Invalid("requirements-over-limit");
        }
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for (block, _) in blocks {
            if !seen.insert(block.title.clone()) {
                conflicts.insert(ConflictRow {
                    capability: block.capability.clone(),
                    title: block.title.clone(),
                    detail: "duplicate-title",
                });
                continue;
            }
            effective.insert(
                (block.capability.clone(), block.title.clone()),
                CatalogEntry {
                    id: requirement_id(capability, &block.slug),
                    digest: sha256_digest(&block.body),
                    origin: Origin::Accepted,
                    change: None,
                    path: path.clone(),
                },
            );
        }
    }

    for change in &changes {
        let change_specs = format!("{changes_dir}/{change}/specs");
        let deltas = match read_capabilities(fs, &change_specs) {
            Ok(capabilities) => capabilities,
            Err(TreeError::Missing) => continue,
            Err(tree_error) => return tree_error.into_loaded(),
        };
        for (capability, path) in &deltas {
            let blocks = match parse_document(fs, path, true) {
                Ok(blocks) => blocks,
                Err(tree_error) => return tree_error.into_loaded(),
            };
            for (block, section) in blocks {
                let key = (capability.clone(), block.title.clone());
                let _ = apply(
                    &mut effective,
                    &mut conflicts,
                    &key,
                    &block,
                    section,
                    change,
                    path,
                );
            }
        }
    }

    let mut entries: Vec<CatalogEntry> = effective.into_values().collect();
    entries.sort_by(|left, right| left.id.cmp(&right.id));
    let conflicts: Vec<ConflictRow> = conflicts.into_iter().collect();
    if entries.len() > version::MAX_REQUIREMENTS {
        return Loaded::Invalid("requirements-over-limit");
    }
    Loaded::Resolved(Snapshot {
        source: decl.source.clone(),
        root: decl.root.clone(),
        status: ProviderStatus::Ok,
        entries,
        conflicts,
    })
}

/// Apply one delta block to the effective state. Every contradiction
/// records an explicit conflict and leaves the disputed requirement out
/// of the effective set; nothing is ever silently overwritten.
fn apply(
    effective: &mut std::collections::BTreeMap<(String, String), CatalogEntry>,
    conflicts: &mut std::collections::BTreeSet<ConflictRow>,
    key: &(String, String),
    block: &RawBlock,
    section: Section,
    change: &str,
    path: &str,
) -> Result<(), &'static str> {
    if is_disputed(conflicts, key) {
        return Err("disputed");
    }
    let existing = effective.remove(key);
    match (section, existing) {
        (Section::Added, Some(previous)) => {
            let detail = match previous.origin {
                Origin::Accepted => "added-existing",
                Origin::Change => "duplicate-added",
            };
            record_conflict(conflicts, key, detail);
            Err(detail)
        }
        (Section::Added, None) => {
            effective.insert(
                key.clone(),
                CatalogEntry {
                    id: requirement_id(&key.0, &block.slug),
                    digest: sha256_digest(&block.body),
                    origin: Origin::Change,
                    change: Some(change.to_owned()),
                    path: path.to_owned(),
                },
            );
            Ok(())
        }
        (Section::Modified, None) => {
            record_conflict(conflicts, key, "modified-missing");
            Err("modified-missing")
        }
        (Section::Modified, Some(previous)) => {
            if let Origin::Change = previous.origin {
                if previous.change.as_deref() != Some(change) {
                    record_conflict(conflicts, key, "multiple-changes");
                    return Err("multiple-changes");
                }
            }
            effective.insert(
                key.clone(),
                CatalogEntry {
                    id: previous.id,
                    digest: sha256_digest(&block.body),
                    origin: Origin::Change,
                    change: Some(change.to_owned()),
                    path: path.to_owned(),
                },
            );
            Ok(())
        }
        (Section::Removed, None) => {
            record_conflict(conflicts, key, "removed-missing");
            Err("removed-missing")
        }
        (Section::Removed, Some(previous)) => {
            if let Origin::Change = previous.origin {
                if previous.change.as_deref() != Some(change) {
                    record_conflict(conflicts, key, "multiple-changes");
                    return Err("multiple-changes");
                }
            }
            // Removed stays removed; the key remains out of the
            // effective set.
            Ok(())
        }
    }
}

/// Record one explicit conflict for a disputed (capability, title).
fn record_conflict(
    conflicts: &mut std::collections::BTreeSet<ConflictRow>,
    key: &(String, String),
    detail: &'static str,
) {
    conflicts.insert(ConflictRow {
        capability: key.0.clone(),
        title: key.1.clone(),
        detail,
    });
}

/// Whether the (capability, title) pair is already disputed.
fn is_disputed(
    conflicts: &std::collections::BTreeSet<ConflictRow>,
    key: &(String, String),
) -> bool {
    conflicts
        .iter()
        .any(|row| row.capability == key.0 && row.title == key.1)
}

/// The capabilities of one specs directory: `(capability, logical spec
/// path)` pairs in canonical byte order.
fn read_capabilities(
    fs: &crate::project_fs::Fs,
    specs_dir: &str,
) -> Result<Vec<(String, String)>, TreeError> {
    let mut names = Vec::new();
    for (name, entry_type) in fs.entries(specs_dir).map_err(TreeError::of)? {
        if entry_type != crate::project_fs::EntryType::Directory {
            return Err(TreeError::Shape);
        }
        if !is_capability(&name) {
            return Err(TreeError::Shape);
        }
        names.push(name);
    }
    names.sort();
    Ok(names
        .into_iter()
        .map(|capability| {
            let path = format!("{specs_dir}/{capability}/spec.md");
            (capability, path)
        })
        .collect())
}

/// The active change ids of one changes directory, canonically sorted;
/// the archive subtree and non-directories are not active changes.
fn read_change_ids(
    fs: &crate::project_fs::Fs,
    changes_dir: &str,
) -> Result<Vec<String>, TreeError> {
    let mut changes = Vec::new();
    for (name, entry_type) in fs.entries(changes_dir).map_err(TreeError::of)? {
        if name == ARCHIVE_DIR {
            continue;
        }
        if entry_type != crate::project_fs::EntryType::Directory {
            return Err(TreeError::Shape);
        }
        if !is_change_id(&name) {
            return Err(TreeError::Shape);
        }
        changes.push(name);
    }
    changes.sort();
    Ok(changes)
}

/// Parse one spec or delta document into requirement blocks (with their
/// delta sections). In accepted specs the section is meaningless and
/// carries a placeholder; in deltas an unclassified or unknown section
/// fails closed.
fn parse_document(
    fs: &crate::project_fs::Fs,
    path: &str,
    delta: bool,
) -> Result<Vec<(RawBlock, Section)>, TreeError> {
    let Some((dir, name)) = path.rsplit_once('/') else {
        return Err(TreeError::Shape);
    };
    let bytes = match fs.read_file_opt(dir, name, version::MAX_SPEC_BYTES) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return Err(TreeError::Missing),
        Err(crate::project_fs::FsErrorKind::Limit { .. }) => return Err(TreeError::Limit),
        Err(_) => return Err(TreeError::Io),
    };
    let text = std::str::from_utf8(&bytes).map_err(|_| TreeError::Shape)?;
    let capability = capability_of(path)?;
    let mut blocks = Vec::new();
    let mut section: Option<Section> = None;
    // The open block: its title, slug, delta section, and body lines.
    let mut current: Option<(String, String, Option<Section>, Vec<String>)> = None;
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let heading_level = line
            .chars()
            .take_while(|character| *character == '#')
            .count();
        let is_heading =
            heading_level > 0 && heading_level <= 3 && line[heading_level..].starts_with(' ');
        if is_heading {
            // A level <= 3 heading closes any open requirement block;
            // deeper headings (scenario details) stay inside the body.
            if let Some((title, slug, block_section, body)) = current.take() {
                blocks.push((
                    RawBlock {
                        capability: capability.clone(),
                        title,
                        slug,
                        body: canonical_body(&body),
                    },
                    block_section.unwrap_or(Section::Added),
                ));
            }
            let heading = line[heading_level..].trim_start();
            if heading_level == 3 {
                let Some(title) = heading.strip_prefix("Requirement:") else {
                    continue;
                };
                let title = title.trim();
                let Some(slug) = title_slug(title) else {
                    return Err(TreeError::Shape);
                };
                current = Some((title.to_owned(), slug, section, Vec::new()));
                continue;
            }
            if heading_level == 2 && delta {
                section = match Section::parse(heading.trim()) {
                    Some(section) => Some(section),
                    None => return Err(TreeError::Shape),
                };
            }
            continue;
        }
        if let Some((_, _, _, body)) = current.as_mut() {
            body.push(line.trim_end().to_owned());
        }
    }
    if let Some((title, slug, block_section, body)) = current.take() {
        blocks.push((
            RawBlock {
                capability,
                title,
                slug,
                body: canonical_body(&body),
            },
            block_section.unwrap_or(Section::Added),
        ));
    }
    Ok(blocks)
}

/// The canonical body bytes a revision digest is computed over: exact
/// line content with trailing whitespace dropped, trailing blank lines
/// dropped, one trailing newline. The title line is excluded so a pure
/// rename (title change, unchanged body) is detectable as an id move
/// with an identical digest.
fn canonical_body(body: &[String]) -> Vec<u8> {
    let mut start = 0;
    while start < body.len() && body[start].is_empty() {
        start += 1;
    }
    let mut end = body.len();
    while end > start && body[end - 1].is_empty() {
        end -= 1;
    }
    let body = &body[start..end];
    let mut out = Vec::new();
    for line in body {
        out.extend_from_slice(line.as_bytes());
        out.push(b'\n');
    }
    out
}

/// The capability name of a spec path: the directory segment before
/// `spec.md` (`<root>/specs/<capability>/spec.md` or
/// `<root>/changes/<id>/specs/<capability>/spec.md`).
fn capability_of(path: &str) -> Result<String, TreeError> {
    let without_file = path.strip_suffix("/spec.md").ok_or(TreeError::Shape)?;
    let capability = without_file
        .rsplit_once('/')
        .map(|(_, capability)| capability)
        .ok_or(TreeError::Shape)?;
    Ok(capability.to_owned())
}

/// The requirement capability grammar: one kebab-case segment of at
/// most 63 characters.
fn is_capability(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some((&first, rest)) = bytes.split_first() else {
        return false;
    };
    (first.is_ascii_lowercase() || first.is_ascii_digit())
        && rest.len() <= 62
        && rest
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

/// Why one provider tree could not be read.
enum TreeError {
    /// The root or a capability directory does not exist.
    Missing,
    /// Any other filesystem failure.
    Io,
    /// A document exceeded its byte bound.
    Limit,
    /// The tree violates the v1 layout or text contract.
    Shape,
}

impl TreeError {
    fn of(error: crate::project_fs::FsErrorKind) -> Self {
        match error {
            crate::project_fs::FsErrorKind::NotFound => Self::Missing,
            crate::project_fs::FsErrorKind::Limit { .. } => Self::Limit,
            crate::project_fs::FsErrorKind::Io => Self::Io,
        }
    }

    fn into_loaded(self) -> Loaded {
        match self {
            Self::Missing => Loaded::Invalid("tree-missing"),
            Self::Io => Loaded::Invalid("tree-unreadable"),
            Self::Limit => Loaded::Invalid("tree-over-limit"),
            Self::Shape => Loaded::Invalid("tree-shape"),
        }
    }
}
