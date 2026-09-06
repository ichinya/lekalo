//! The typed changed-input handoff between the Git-aware adapter and the
//! pure impact core (issue #16).
//!
//! The adapter at the CLI/integration edge resolves Git output through the
//! accepted loader/IR source map and hands over only this validated, typed
//! value. The core never parses Git, never sees a raw path or patch line,
//! and never infers symbols from file names. Logical paths appear here only
//! as validated, canonical, project-relative references that never cross
//! back out into any result bytes.
//!
//! Canonical discipline: entries sort by change id, symbol ids, member
//! seeds, path, and change kind; exact duplicates collapse only after
//! normalization; distinct occurrences stay distinct. The set digest is the
//! SHA-256 of the compact canonical entry serialization.

use crate::diagnostics::DiagnosticSet;
use crate::versioning::plan::sha256_hex;

use super::diagnostic;
use super::version::MAX_ENTRIES;

/// How the changed-input set was derived. The CLI adapter emits
/// `committed` (two resolved revisions) and `worktree` (the current index
/// plus worktree against a base); `index` and `mixed` exist for typed
/// library callers that can distinguish staged from unstaged candidates.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ChangedMode {
    Committed,
    Index,
    Worktree,
    Mixed,
}

impl ChangedMode {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Committed => "committed",
            Self::Index => "index",
            Self::Worktree => "worktree",
            Self::Mixed => "mixed",
        }
    }
}

/// The closed file-level change vocabulary of one entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum FileChange {
    Added,
    Modified,
    Deleted,
    Renamed,
}

impl FileChange {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Deleted => "deleted",
            Self::Renamed => "renamed",
        }
    }
}

/// The closed evidence state of one entry.
///
/// `canonical` marks a change fully resolved against the current source
/// map; `unknown` marks a change whose symbols could not be resolved
/// (deleted, renamed-away, or outside the semantic surface); `stale` marks
/// a resolution against evidence older than the current revision. An entry
/// with unknown evidence degrades the whole result — it never silently
/// disappears.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum EntryEvidence {
    Canonical,
    Verified,
    Extracted,
    Stale,
    Unknown,
}

impl EntryEvidence {
    /// The exact wire key (the closed evidence vocabulary).
    pub const fn key(self) -> &'static str {
        match self {
            Self::Canonical => "canonical",
            Self::Verified => "verified",
            Self::Extracted => "extracted",
            Self::Stale => "stale",
            Self::Unknown => "unknown",
        }
    }
}

/// One typed field-member seed: the only way a field-level change enters
/// the core (never inferred from text). Field-level derivation is the #18
/// semantic comparison's owner decision; the v1 CLI adapter supplies no
/// member seeds, so field coverage stays explicitly unknown there.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct MemberSeed {
    entity: String,
    field: String,
    change: MemberChange,
}

/// The closed member-change vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum MemberChange {
    Removed,
    TypeNarrowed,
}

impl MemberChange {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Removed => "removed",
            Self::TypeNarrowed => "type-narrowed",
        }
    }
}

impl MemberSeed {
    /// Validate and construct one member seed.
    ///
    /// Both the entity and the field must be valid semantic-id/field-name
    /// grammar and bounded; anything else is a fatal changed-input fault.
    pub fn new(entity: &str, field: &str, change: MemberChange) -> Result<Self, DiagnosticSet> {
        if !is_symbol_grammar(entity) {
            return Err(diagnostic::changed_input_invalid_set(
                "member-entity-grammar",
            ));
        }
        if !is_field_grammar(field) {
            return Err(diagnostic::changed_input_invalid_set(
                "member-field-grammar",
            ));
        }
        Ok(Self {
            entity: entity.to_owned(),
            field: field.to_owned(),
            change,
        })
    }

    /// The entity semantic id.
    pub fn entity(&self) -> &str {
        &self.entity
    }

    /// The field name.
    pub fn field(&self) -> &str {
        &self.field
    }

    /// The member change.
    pub const fn change(&self) -> MemberChange {
        self.change
    }

    fn canonical_bytes(&self) -> String {
        format!(
            "member|{}|{}|{}",
            self.entity,
            self.field,
            self.change.key()
        )
    }
}

/// One changed input: the resolved semantic surface of one changed file.
///
/// `symbol_ids` are fully qualified semantic ids validated against the
/// source map; `logical_path` is the canonical project-relative path of
/// the change (the `to` side for renames). Zero symbols mean the change
/// could not be resolved to the semantic surface and carries `unknown`
/// evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangedInput {
    change_id: String,
    symbol_ids: Vec<String>,
    member_seeds: Vec<MemberSeed>,
    logical_path: (String, String),
    file_change: FileChange,
    evidence: EntryEvidence,
}

impl ChangedInput {
    /// Validate and construct one changed input.
    ///
    /// `paths` is `(to, from)`; the `from` side exists only for renames.
    /// Every symbol id must carry valid semantic-id grammar; symbol ids are
    /// deduplicated and sorted. An entry with no symbols and no member
    /// seeds is valid but carries unknown evidence.
    pub fn new(
        symbol_ids: Vec<String>,
        member_seeds: Vec<MemberSeed>,
        paths: Option<(&str, Option<&str>)>,
        file_change: FileChange,
        evidence: EntryEvidence,
    ) -> Result<Self, DiagnosticSet> {
        if symbol_ids.len() > MAX_ENTRIES {
            return Err(diagnostic::changed_input_invalid_set("entry-symbol-limit"));
        }
        let mut symbols = symbol_ids;
        for symbol in &symbols {
            if !is_symbol_grammar(symbol) {
                return Err(diagnostic::changed_input_invalid_set("symbol-grammar"));
            }
        }
        symbols.sort();
        symbols.dedup();
        let logical_path = match paths {
            Some((to, from)) => {
                let to = validate_logical_path(to)?;
                let from = match from {
                    Some(from) => Some(validate_logical_path(from)?),
                    None => None,
                };
                if file_change == FileChange::Renamed && from.is_none() {
                    return Err(diagnostic::changed_input_invalid_set("rename-without-from"));
                }
                if file_change != FileChange::Renamed && from.is_some() {
                    return Err(diagnostic::changed_input_invalid_set(
                        "unexpected-from-path",
                    ));
                }
                (to, from.unwrap_or_default())
            }
            None => return Err(diagnostic::changed_input_invalid_set("missing-path")),
        };
        let mut seeds = member_seeds;
        seeds.sort();
        seeds.dedup();
        if seeds.len() > MAX_ENTRIES {
            return Err(diagnostic::changed_input_invalid_set("member-seed-limit"));
        }
        let evidence = if symbols.is_empty() && seeds.is_empty() {
            EntryEvidence::Unknown
        } else {
            evidence
        };
        let mut normalized = Self {
            change_id: String::new(),
            symbol_ids: symbols,
            member_seeds: seeds,
            logical_path,
            file_change,
            evidence,
        };
        normalized.change_id = short_digest(&normalized.canonical_bytes());
        Ok(normalized)
    }

    /// The opaque bounded change identity.
    pub fn change_id(&self) -> &str {
        &self.change_id
    }

    /// The resolved semantic symbols, sorted.
    pub fn symbol_ids(&self) -> &[String] {
        &self.symbol_ids
    }

    /// The typed member seeds, sorted.
    pub fn member_seeds(&self) -> &[MemberSeed] {
        &self.member_seeds
    }

    /// The logical path pair `(to, from)`; `from` is empty for non-renames.
    pub fn logical_path(&self) -> (&str, &str) {
        (self.logical_path.0.as_str(), self.logical_path.1.as_str())
    }

    /// The file-level change.
    pub const fn file_change(&self) -> FileChange {
        self.file_change
    }

    /// The entry evidence state.
    pub const fn evidence(&self) -> EntryEvidence {
        self.evidence
    }

    fn canonical_bytes(&self) -> String {
        let mut out = String::new();
        out.push_str(self.file_change.key());
        out.push('|');
        out.push_str(&self.logical_path.0);
        out.push('|');
        out.push_str(&self.logical_path.1);
        for symbol in &self.symbol_ids {
            out.push('|');
            out.push_str(symbol);
        }
        for seed in &self.member_seeds {
            out.push('|');
            out.push_str(&seed.canonical_bytes());
        }
        out
    }
}
/// One finished changed-input handoff: immutable, canonically ordered.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangedInputSet {
    mode: ChangedMode,
    base_revision_ref: Option<String>,
    candidate_revision_ref: Option<String>,
    entries: Vec<ChangedInput>,
    digest: String,
}

impl ChangedInputSet {
    /// The canonical digest of the empty set (the symbol-mode input).
    pub fn empty() -> Self {
        Self {
            mode: ChangedMode::Committed,
            base_revision_ref: None,
            candidate_revision_ref: None,
            entries: Vec::new(),
            digest: format!("sha256:{}", sha256_hex(b"changeset|empty")),
        }
    }

    /// Validate, normalize, and assemble one set.
    ///
    /// Entries sort canonically and exact duplicates collapse after
    /// normalization; distinct occurrences stay distinct. An empty entry
    /// list is a fatal fault: "nothing changed" is a caller decision, never
    /// an empty handoff the core would have to interpret. Revision
    /// references are opaque bounded hex digests resolved by the adapter.
    pub fn from_entries(
        mode: ChangedMode,
        base_revision_ref: Option<String>,
        candidate_revision_ref: Option<String>,
        entries: Vec<ChangedInput>,
    ) -> Result<Self, DiagnosticSet> {
        for revision in base_revision_ref
            .iter()
            .chain(candidate_revision_ref.iter())
        {
            if !is_revision_grammar(revision) {
                return Err(diagnostic::changed_input_invalid_set("revision-grammar"));
            }
        }
        if entries.is_empty() {
            return Err(diagnostic::changed_input_invalid_set("empty-change-set"));
        }
        if entries.len() > MAX_ENTRIES {
            return Err(diagnostic::changed_input_invalid_set("entry-limit"));
        }
        let mut entries = entries;
        entries.sort_by(|left, right| {
            let left = left.canonical_bytes();
            let right = right.canonical_bytes();
            left.cmp(&right)
        });
        entries.dedup();
        let mut canonical = String::from("changeset|");
        canonical.push_str(mode.key());
        for entry in &entries {
            canonical.push('\u{1f}');
            canonical.push_str(&entry.canonical_bytes());
        }
        Ok(Self {
            mode,
            base_revision_ref,
            candidate_revision_ref,
            entries,
            digest: format!("sha256:{}", sha256_hex(canonical.as_bytes())),
        })
    }

    /// The opaque base revision reference, when the mode carries one.
    pub fn base_revision_ref(&self) -> Option<&str> {
        self.base_revision_ref.as_deref()
    }

    /// The opaque candidate revision reference, when the mode carries one.
    pub fn candidate_revision_ref(&self) -> Option<&str> {
        self.candidate_revision_ref.as_deref()
    }

    /// The changed-input mode.
    pub const fn mode(&self) -> ChangedMode {
        self.mode
    }

    /// The canonically ordered entries.
    pub fn entries(&self) -> &[ChangedInput] {
        &self.entries
    }

    /// The opaque canonical digest of the whole set.
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

/// Whether `text` carries valid semantic-id grammar (bounded, first
/// character alphanumeric, then the closed semantic-id alphabet).
pub(crate) fn is_symbol_grammar(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 192
        && text
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric())
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | ':'))
}

/// Whether `text` carries valid field-name grammar.
pub(crate) fn is_field_grammar(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 128
        && text
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_lowercase())
        && text
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
}

/// Whether `text` is an opaque resolved revision reference: 40 or 64
/// lowercase hex characters.
pub fn is_revision_grammar(text: &str) -> bool {
    (text.len() == 40 || text.len() == 64)
        && text
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// Validate one logical project-relative path: forward slashes only, no
/// absolute, escape, drive, UNC, or alias spelling, bounded.
fn validate_logical_path(path: &str) -> Result<String, DiagnosticSet> {
    let fail = || diagnostic::changed_input_invalid_set("path-grammar");
    if path.is_empty() || path.len() > 512 {
        return Err(fail());
    }
    if path.starts_with('/') || path.starts_with('\\') || path.contains('\\') {
        return Err(fail());
    }
    if path.contains('\0') || path.chars().any(char::is_control) {
        return Err(fail());
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(fail());
        }
        if segment.ends_with('.') || segment.contains(':') {
            return Err(fail());
        }
    }
    Ok(path.to_owned())
}

/// Validate one logical project-relative path on behalf of the CLI
/// adapter; the same closed grammar as the internal constructor.
pub fn validate_path(path: &str) -> Result<String, DiagnosticSet> {
    validate_logical_path(path)
}

/// The short opaque digest used for change identities.
fn short_digest(canonical: &str) -> String {
    let hex = sha256_hex(canonical.as_bytes());
    hex[..16].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(symbol: &str, path: &str) -> ChangedInput {
        ChangedInput::new(
            vec![symbol.to_owned()],
            Vec::new(),
            Some((path, None)),
            FileChange::Modified,
            EntryEvidence::Canonical,
        )
        .expect("valid entry")
    }

    #[test]
    fn entries_normalize_sort_and_collapse() {
        let set = ChangedInputSet::from_entries(
            ChangedMode::Worktree,
            None,
            Some("0".repeat(40)),
            vec![
                entry("b.sym", "lekalo/modules/b/b.yaml"),
                entry("a.sym", "lekalo/modules/a/a.yaml"),
                entry("b.sym", "lekalo/modules/b/b.yaml"),
            ],
        )
        .expect("valid set");
        let symbols: Vec<&str> = set
            .entries()
            .iter()
            .flat_map(|entry| entry.symbol_ids())
            .map(String::as_str)
            .collect();
        assert_eq!(symbols, vec!["a.sym", "b.sym"]);
        assert!(set.digest().starts_with("sha256:"));
    }

    #[test]
    fn empty_sets_and_bad_paths_are_fatal() {
        assert!(
            ChangedInputSet::from_entries(ChangedMode::Worktree, None, None, Vec::new()).is_err()
        );
        let bad = ChangedInput::new(
            vec!["a.sym".to_owned()],
            Vec::new(),
            Some(("/etc/passwd", None)),
            FileChange::Modified,
            EntryEvidence::Canonical,
        );
        assert!(bad.is_err());
        let escape = ChangedInput::new(
            Vec::new(),
            Vec::new(),
            Some(("../outside.yaml", None)),
            FileChange::Deleted,
            EntryEvidence::Canonical,
        );
        assert!(escape.is_err());
    }

    #[test]
    fn unresolvable_entries_degrade_to_unknown_evidence() {
        let resolved = ChangedInput::new(
            Vec::new(),
            Vec::new(),
            Some(("lekalo/modules/a/a.yaml", None)),
            FileChange::Deleted,
            EntryEvidence::Canonical,
        )
        .expect("valid entry");
        assert_eq!(resolved.evidence(), EntryEvidence::Unknown);
        assert_eq!(resolved.symbol_ids(), Vec::<String>::new());
    }

    #[test]
    fn renames_carry_both_paths() {
        let rename = ChangedInput::new(
            vec!["m.sym".to_owned()],
            Vec::new(),
            Some(("lekalo/new.yaml", Some("lekalo/old.yaml"))),
            FileChange::Renamed,
            EntryEvidence::Canonical,
        )
        .expect("valid rename");
        assert_eq!(rename.logical_path().0, "lekalo/new.yaml");
        assert_eq!(rename.logical_path().1, "lekalo/old.yaml");
        let plain = ChangedInput::new(
            vec!["m.sym".to_owned()],
            Vec::new(),
            Some(("lekalo/new.yaml", Some("lekalo/old.yaml"))),
            FileChange::Modified,
            EntryEvidence::Canonical,
        );
        assert!(plain.is_err());
    }

    #[test]
    fn member_seeds_validate_their_grammar() {
        assert!(MemberSeed::new("mod.entity", "field_name", MemberChange::Removed).is_ok());
        assert!(MemberSeed::new("Bad Entity", "field", MemberChange::Removed).is_err());
        assert!(MemberSeed::new("mod.entity", "Bad-Field", MemberChange::Removed).is_err());
    }
}
