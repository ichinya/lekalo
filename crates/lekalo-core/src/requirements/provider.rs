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
//! never copied — only ids, opaque conflict subjects, and digests.
//!
//! Archiving is traceability-neutral by construction: archiving a change
//! moves its applied content into the accepted specs, and the projection
//! of the same content yields the same ids and the same body digests, so
//! a fresh reference stays fresh across the archive.

use std::collections::{BTreeMap, BTreeSet};

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
    /// The internal disputed title, used for matching only; never exported.
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
    /// Explicit active rename evidence, old id to new id.
    pub renames: BTreeMap<String, String>,
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
            renames: BTreeMap::new(),
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

/// A document separates named operations from requirement bodies. Rename
/// evidence is explicit; an equal digest alone never proves identity.
#[derive(Default)]
struct Document {
    blocks: Vec<(RawBlock, Section)>,
    renames: Vec<(RawBlock, RawBlock)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum Section {
    Renamed,
    Removed,
    Modified,
    Added,
}

impl Section {
    fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_uppercase().as_str() {
            "ADDED REQUIREMENTS" => Some(Self::Added),
            "MODIFIED REQUIREMENTS" => Some(Self::Modified),
            "REMOVED REQUIREMENTS" => Some(Self::Removed),
            "RENAMED REQUIREMENTS" => Some(Self::Renamed),
            _ => None,
        }
    }
}

type Key = (String, String);

#[derive(Default)]
struct State {
    effective: BTreeMap<Key, CatalogEntry>,
    conflicts: BTreeSet<ConflictRow>,
    // Custody survives removals and failed edits. Slug ownership includes
    // every encountered title, even one later removed or renamed.
    owners: BTreeMap<String, Key>,
    touched: BTreeMap<Key, String>,
    renames: BTreeMap<String, String>,
}

impl State {
    fn conflict(&mut self, key: &Key, detail: &'static str) {
        self.effective.remove(key);
        self.conflicts.insert(ConflictRow {
            capability: key.0.clone(),
            title: key.1.clone(),
            detail,
        });
    }

    fn disputed(&self, key: &Key) -> bool {
        self.conflicts
            .iter()
            .any(|c| c.capability == key.0 && c.title == key.1)
    }

    fn register(&mut self, block: &RawBlock) {
        let key = block.key();
        let id = block.id();
        if let Some(previous) = self.owners.get(&id).cloned() {
            if previous != key {
                self.conflict(&previous, "id-collision");
                self.conflict(&key, "id-collision");
            }
        } else {
            self.owners.insert(id, key);
        }
    }

    fn apply_document(&mut self, mut document: Document, change: &str, path: &str) {
        // Validate the complete operation history before applying anything.
        // Only one rename TO plus one MODIFIED of that new title may share
        // a title within a change. Every cross-change touch conflicts.
        let mut operations: BTreeMap<Key, Vec<&str>> = BTreeMap::new();
        for (block, section) in &document.blocks {
            self.register(block);
            operations
                .entry(block.key())
                .or_default()
                .push(match section {
                    Section::Added => "added",
                    Section::Modified => "modified",
                    Section::Removed => "removed",
                    Section::Renamed => unreachable!("rename pairs are separate"),
                });
        }
        for (from, to) in &document.renames {
            self.register(from);
            self.register(to);
            operations.entry(from.key()).or_default().push("from");
            operations.entry(to.key()).or_default().push("to");
        }
        for (key, mut ops) in operations {
            ops.sort_unstable();
            if self.touched.contains_key(&key) {
                self.conflict(&key, "multiple-changes");
            } else if ops.len() > 1 && ops != ["modified", "to"] {
                self.conflict(&key, "contradictory-operations");
            }
            self.touched.insert(key, change.to_owned());
        }
        for (from, to) in document.renames {
            let old = from.key();
            let new = to.key();
            let detail = if self.disputed(&old) || self.disputed(&new) {
                Some("rename-conflict")
            } else if !self.effective.contains_key(&old) {
                Some("renamed-missing")
            } else if self.effective.contains_key(&new) {
                Some("renamed-existing")
            } else {
                None
            };
            if let Some(detail) = detail {
                self.conflict(&old, detail);
                self.conflict(&new, detail);
                continue;
            }
            let mut entry = self
                .effective
                .remove(&old)
                .expect("validated rename source");
            entry.id = to.id();
            entry.origin = Origin::Change;
            entry.change = Some(change.to_owned());
            entry.path = path.to_owned();
            self.renames.insert(from.id(), to.id());
            self.effective.insert(new, entry);
        }
        document.blocks.sort_by_key(|(_, section)| *section);
        for (block, section) in document.blocks {
            let key = block.key();
            if self.disputed(&key) {
                continue;
            }
            let exists = self.effective.contains_key(&key);
            match (section, exists) {
                (Section::Added, true) => self.conflict(&key, "added-existing"),
                (Section::Modified, false) => self.conflict(&key, "modified-missing"),
                (Section::Removed, false) => self.conflict(&key, "removed-missing"),
                (Section::Removed, true) => {
                    self.effective.remove(&key);
                }
                (Section::Added | Section::Modified, _) => {
                    self.effective.insert(key, block.entry(path, Some(change)));
                }
                (Section::Renamed, _) => unreachable!("rename pairs are separate"),
            }
        }
        // A later operation that disputes a rename target also disputes
        // its source; never retain an apparently confirmed rename edge.
        let renames = self.renames.clone();
        for (from, to) in renames {
            let old = self.owners.get(&from).cloned().expect("registered old id");
            let new = self.owners.get(&to).cloned().expect("registered new id");
            if self.disputed(&old) || self.disputed(&new) {
                self.conflict(&old, "rename-conflict");
                self.conflict(&new, "rename-conflict");
                self.renames.remove(&from);
            }
        }
    }
}

impl RawBlock {
    fn key(&self) -> Key {
        (self.capability.clone(), self.title.clone())
    }
    fn id(&self) -> String {
        requirement_id(&self.capability, &self.slug)
    }
    fn entry(&self, path: &str, change: Option<&str>) -> CatalogEntry {
        CatalogEntry {
            id: self.id(),
            digest: sha256_digest(&self.body),
            origin: if change.is_some() {
                Origin::Change
            } else {
                Origin::Accepted
            },
            change: change.map(str::to_owned),
            path: path.to_owned(),
        }
    }
}

/// Load one complete provider, bounded across accepted and active inputs.
pub(crate) fn load_snapshot(fs: &crate::project_fs::Fs, decl: &ProviderDecl) -> Loaded {
    match load(fs, decl) {
        Ok(snapshot) => Loaded::Resolved(snapshot),
        Err(TreeError::Absent) => Loaded::Absent,
        Err(error) => error.into_loaded(),
    }
}

fn load(fs: &crate::project_fs::Fs, decl: &ProviderDecl) -> Result<Snapshot, TreeError> {
    if decl.kind != ProviderKind::Openspec {
        return Err(TreeError::Shape);
    }
    let specs_dir = format!("{}/specs", decl.root);
    let changes_dir = format!("{}/changes", decl.root);
    let specs = match read_capabilities(fs, &specs_dir) {
        Ok(specs) => specs,
        Err(TreeError::Missing) => {
            // New capabilities can exist only in active deltas. An absent
            // accepted specs directory is not an absent provider tree.
            match fs.entries(&decl.root) {
                Ok(_) => Vec::new(),
                Err(crate::project_fs::FsErrorKind::NotFound) => return Err(TreeError::Absent),
                Err(error) => return Err(TreeError::of(error)),
            }
        }
        Err(error) => return Err(error),
    };
    let changes = match read_change_ids(fs, &changes_dir) {
        Ok(changes) => changes,
        Err(TreeError::Missing) => Vec::new(),
        Err(error) => return Err(error),
    };
    if changes.len() > version::MAX_CHANGES {
        return Err(TreeError::Bound("changes-over-limit"));
    }
    let mut capabilities = BTreeSet::new();
    let mut state = State::default();
    for (capability, path) in &specs {
        capabilities.insert(capability.clone());
        let document = parse_document(fs, path, false)?;
        for (block, _) in document.blocks {
            state.register(&block);
            let key = block.key();
            if state.disputed(&key) {
                continue;
            }
            match state.effective.entry(key.clone()) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(block.entry(path, None));
                }
                std::collections::btree_map::Entry::Occupied(entry) => {
                    entry.remove();
                    state.conflict(&key, "duplicate-title");
                }
            }
        }
        check_bounds(&state, &capabilities)?;
    }
    for change in &changes {
        let deltas = match read_capabilities(fs, &format!("{changes_dir}/{change}/specs")) {
            Ok(deltas) => deltas,
            Err(TreeError::Missing) => continue,
            Err(error) => return Err(error),
        };
        for (capability, path) in deltas {
            capabilities.insert(capability);
            state.apply_document(parse_document(fs, &path, true)?, change, &path);
            check_bounds(&state, &capabilities)?;
        }
    }
    let mut entries: Vec<_> = state.effective.into_values().collect();
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(Snapshot {
        source: decl.source.clone(),
        root: decl.root.clone(),
        status: ProviderStatus::Ok,
        entries,
        conflicts: state.conflicts.into_iter().collect(),
        renames: state.renames,
    })
}

fn check_bounds(state: &State, capabilities: &BTreeSet<String>) -> Result<(), TreeError> {
    if capabilities.len() > version::MAX_CAPABILITIES {
        return Err(TreeError::Bound("capabilities-over-limit"));
    }
    if state.owners.len() > version::MAX_REQUIREMENTS
        || state.conflicts.len() > version::MAX_REQUIREMENTS
    {
        return Err(TreeError::Bound("requirements-over-limit"));
    }
    Ok(())
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

/// Parse Markdown structure only outside code fences. Body content,
/// including examples and non-requirement subheadings, stays in the digest.
fn parse_document(
    fs: &crate::project_fs::Fs,
    path: &str,
    delta: bool,
) -> Result<Document, TreeError> {
    let (dir, name) = path.rsplit_once('/').ok_or(TreeError::Shape)?;
    let bytes = fs
        .read_file_opt(dir, name, version::MAX_SPEC_BYTES)
        .map_err(TreeError::of)?
        .ok_or(TreeError::Missing)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| TreeError::Shape)?;
    parse_text(text, &capability_of(path)?, delta)
}

fn raw_block(capability: &str, title: &str, body: &[String]) -> Result<RawBlock, TreeError> {
    let slug = title_slug(title).ok_or(TreeError::Shape)?;
    if !super::is_requirement_id(&requirement_id(capability, &slug)) {
        return Err(TreeError::Bound("requirement-id-over-limit"));
    }
    Ok(RawBlock {
        capability: capability.to_owned(),
        title: title.to_owned(),
        slug,
        body: canonical_body(body),
    })
}

fn requirement_title(line: &str) -> Option<&str> {
    let heading = line.strip_prefix("###")?.trim_start();
    let prefix = heading.get(..12)?;
    prefix
        .eq_ignore_ascii_case("Requirement:")
        .then(|| heading[12..].trim())
        .filter(|s| !s.is_empty())
}

fn header_reference(text: &str) -> Option<&str> {
    requirement_title(text.trim().trim_matches('`').trim())
}

/// ECMAScript `\s`, as used by native OpenSpec's code-fence.ts. Rust's
/// Unicode whitespace differs (notably U+0085 and U+FEFF). This predicate
/// applies to fence boundaries only; it does not normalize body bytes.
fn native_fence_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'..='\u{000d}'
            | ' '
            | '\u{00a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    )
}

fn parse_text(text: &str, capability: &str, delta: bool) -> Result<Document, TreeError> {
    let text = text
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut document = Document::default();
    let mut section = None;
    let mut delta_sections = BTreeSet::new();
    let mut accepted_section_seen = false;
    let mut current: Option<(String, Section, Vec<String>)> = None;
    let mut rename_from: Option<RawBlock> = None;
    let mut fence: Option<(u8, usize)> = None;
    for line in text.split('\n') {
        // Native OpenSpec accepts arbitrary leading whitespace and opener
        // info, including backticks. A close needs the same marker, at least
        // the opener length, and only native whitespace after the run.
        let trimmed = line.trim_start_matches(native_fence_whitespace);
        let marker = trimmed.as_bytes().first().copied();
        let run = trimmed.bytes().take_while(|b| Some(*b) == marker).count();
        let fence_line = matches!(marker, Some(b'`' | b'~')) && run >= 3;
        let masked = if let Some((open, length)) = fence {
            if fence_line
                && marker == Some(open)
                && run >= length
                && trimmed[run..]
                    .trim_matches(native_fence_whitespace)
                    .is_empty()
            {
                fence = None;
            }
            true
        } else if fence_line {
            fence = Some((marker.expect("fence marker"), run));
            true
        } else {
            false
        };
        let title = if masked {
            None
        } else {
            requirement_title(line)
        };
        let section_heading = !masked
            && line
                .strip_prefix("##")
                .is_some_and(|rest| rest.starts_with(char::is_whitespace));
        let removed_bullet = if !masked && section == Some(Section::Removed) {
            line.trim_start()
                .strip_prefix('-')
                .and_then(header_reference)
        } else {
            None
        };
        if title.is_some() || section_heading || removed_bullet.is_some() {
            if let Some((name, operation, body)) = current.take() {
                document
                    .blocks
                    .push((raw_block(capability, &name, &body)?, operation));
            }
        }
        if section_heading {
            if rename_from.is_some() {
                return Err(TreeError::Shape);
            }
            section = if delta {
                let operation = Section::parse(line[2..].trim());
                if let Some(operation) = operation {
                    // Native selects one body per operation (exact-title
                    // repeats overwrite; case variants select differently).
                    // This reader supports one section per operation only:
                    // never certify a union that native archive will discard.
                    if !delta_sections.insert(operation) {
                        return Err(TreeError::DuplicateSection);
                    }
                }
                operation
            } else if !accepted_section_seen
                && line[2..].trim().eq_ignore_ascii_case("Requirements")
            {
                // Native accepted specs expose only the first Requirements
                // section. The next unfenced H2 closes it permanently.
                accepted_section_seen = true;
                Some(Section::Added)
            } else {
                None
            };
            continue;
        }
        if let Some(title) = title.or(removed_bullet) {
            let operation = if delta {
                section.ok_or(TreeError::Shape)?
            } else if let Some(operation) = section {
                operation
            } else {
                // Examples outside the native section are not requirements.
                continue;
            };
            if operation == Section::Renamed {
                return Err(TreeError::Shape);
            }
            current = Some((title.to_owned(), operation, Vec::new()));
            continue;
        }
        if !masked && section == Some(Section::Renamed) {
            let text = line.trim().strip_prefix('-').unwrap_or(line.trim()).trim();
            if let Some(rest) = text.strip_prefix("FROM:") {
                if rename_from.is_some() {
                    return Err(TreeError::Shape);
                }
                rename_from = Some(raw_block(
                    capability,
                    header_reference(rest).ok_or(TreeError::Shape)?,
                    &[],
                )?);
            } else if let Some(rest) = text.strip_prefix("TO:") {
                let from = rename_from.take().ok_or(TreeError::Shape)?;
                let to = raw_block(
                    capability,
                    header_reference(rest).ok_or(TreeError::Shape)?,
                    &[],
                )?;
                document.renames.push((from, to));
            } else if !text.is_empty() {
                return Err(TreeError::Shape);
            }
            continue;
        }
        if let Some((_, _, body)) = &mut current {
            body.push(line.trim_end().to_owned());
        }
    }
    if rename_from.is_some() {
        return Err(TreeError::Shape);
    }
    if let Some((name, operation, body)) = current {
        document
            .blocks
            .push((raw_block(capability, &name, &body)?, operation));
    }
    if document.blocks.len() + document.renames.len() > version::MAX_REQUIREMENTS {
        return Err(TreeError::Bound("requirements-over-limit"));
    }
    if delta && document.blocks.is_empty() && document.renames.is_empty() {
        return Err(TreeError::Shape);
    }
    Ok(document)
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
    Absent,
    Bound(&'static str),
    /// The root or a capability directory does not exist.
    Missing,
    /// Any other filesystem failure.
    Io,
    /// A document exceeded its byte bound.
    Limit,
    /// The tree violates the v1 layout or text contract.
    Shape,
    /// Repeated delta operation sections are outside the supported subset.
    DuplicateSection,
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
            Self::Absent => Loaded::Absent,
            Self::Bound(detail) => Loaded::Invalid(detail),
            Self::Missing => Loaded::Invalid("tree-missing"),
            Self::Io => Loaded::Invalid("tree-unreadable"),
            Self::Limit => Loaded::Invalid("tree-over-limit"),
            Self::Shape => Loaded::Invalid("tree-shape"),
            Self::DuplicateSection => Loaded::Invalid("duplicate-operation-section"),
        }
    }
}
