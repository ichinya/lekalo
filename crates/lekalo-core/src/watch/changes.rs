//! The typed change batch between two captured snapshots (issue #95):
//! created/modified/deleted rows with retained old digests, plus
//! rename links only when they are reliable (an exact content-digest
//! match between one deleted and one created row of the same class).
//!
//! Pure data: `diff` never touches the filesystem and never trusts an
//! event sequence; the inventory comparison is the source of truth.

use std::collections::BTreeMap;

use crate::watch::snapshot::InputSnapshot;

/// The closed change kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ChangeKind {
    Created,
    Modified,
    Deleted,
}

impl ChangeKind {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Modified => "modified",
            Self::Deleted => "deleted",
        }
    }
}

/// One changed classified input row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeRow {
    /// Logical project-relative POSIX path.
    pub path: String,
    pub kind: ChangeKind,
    /// The previous revision's digest, retained for modified/deleted
    /// rows (old owners and spans stay attributable).
    pub old_sha256: Option<String>,
    /// The candidate revision's digest.
    pub new_sha256: Option<String>,
    /// The linked previous path when a rename is provable by exact
    /// content identity; a filesystem move alone is never inferred.
    pub rename_of: Option<String>,
    /// `semantic` or `generated` input class.
    pub class: &'static str,
}

/// The coalesced change set of one revision pair.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChangeBatch {
    pub rows: Vec<ChangeRow>,
}

impl ChangeBatch {
    /// Admit a revision PAIR against both public result and adapter
    /// selection bounds. An over-limit pair must not publish its rows.
    pub fn admit(&self) -> Result<(), super::snapshot::CaptureError> {
        if self.len() > super::request::limits::MAX_PENDING_PATHS {
            return Err(super::snapshot::CaptureError::Limit {
                limit: "changed-inputs",
                count: self.len() as u64,
            });
        }
        let paths = self.semantic_rows().count();
        if paths > crate::target_protocol::watch::MAX_CHANGED_PATHS {
            return Err(super::snapshot::CaptureError::Limit {
                limit: "target-changed-paths",
                count: paths as u64,
            });
        }
        Ok(())
    }

    /// Whether nothing changed.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The number of changed rows.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// The semantic (non-generated) rows.
    pub fn semantic_rows(&self) -> impl Iterator<Item = &ChangeRow> {
        self.rows.iter().filter(|row| row.class == "semantic")
    }

    /// The generated drift rows.
    pub fn generated_rows(&self) -> impl Iterator<Item = &ChangeRow> {
        self.rows.iter().filter(|row| row.class == "generated")
    }

    /// The distinct dirty paths (both classes).
    pub fn dirty_paths(&self) -> Vec<&str> {
        self.rows.iter().map(|row| row.path.as_str()).collect()
    }
}

fn diff_class(
    previous: &BTreeMap<String, crate::watch::snapshot::FileRecord>,
    current: &BTreeMap<String, crate::watch::snapshot::FileRecord>,
    class: &'static str,
) -> Vec<ChangeRow> {
    let mut rows = Vec::new();
    let mut deleted: Vec<(String, String)> = Vec::new();
    let mut created: Vec<(String, String)> = Vec::new();
    for (path, old) in previous {
        match current.get(path) {
            Some(new) => {
                if new.sha256 != old.sha256 {
                    rows.push(ChangeRow {
                        path: path.clone(),
                        kind: ChangeKind::Modified,
                        old_sha256: Some(old.sha256.clone()),
                        new_sha256: Some(new.sha256.clone()),
                        rename_of: None,
                        class,
                    });
                }
            }
            None => {
                deleted.push((path.clone(), old.sha256.clone()));
            }
        }
    }
    for (path, new) in current {
        if !previous.contains_key(path) {
            created.push((path.clone(), new.sha256.clone()));
        }
    }
    // Rename links: an exact digest identity between exactly one
    // deleted row and exactly one created row is reliable; anything
    // else stays an unlinkable delete+create pair.
    let mut links: BTreeMap<String, String> = BTreeMap::new();
    for (old_path, digest) in &deleted {
        let matches: Vec<&(String, String)> = created
            .iter()
            .filter(|(_, new_digest)| new_digest == digest)
            .collect();
        if matches.len() == 1
            && deleted
                .iter()
                .filter(|(_, old_digest)| old_digest == digest)
                .count()
                == 1
        {
            links.insert(matches[0].0.clone(), old_path.clone());
        }
    }
    for (path, digest) in deleted {
        rows.push(ChangeRow {
            path,
            kind: ChangeKind::Deleted,
            old_sha256: Some(digest),
            new_sha256: None,
            rename_of: None,
            class,
        });
    }
    for (path, digest) in created {
        let rename_of = links.get(&path).cloned();
        rows.push(ChangeRow {
            path,
            kind: ChangeKind::Created,
            old_sha256: None,
            new_sha256: Some(digest),
            rename_of,
            class,
        });
    }
    rows.sort_by(|left, right| left.path.cmp(&right.path));
    rows
}

/// Diff two captured snapshots into one sorted change batch.
pub fn diff(previous: &InputSnapshot, current: &InputSnapshot) -> ChangeBatch {
    ChangeBatch {
        rows: {
            let mut rows = diff_class(&previous.files, &current.files, "semantic");
            rows.extend(diff_class(
                &previous.generated,
                &current.generated,
                "generated",
            ));
            rows.sort_by(|left, right| left.path.cmp(&right.path));
            rows
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watch::snapshot::FileRecord;

    fn record(digest: &str) -> FileRecord {
        FileRecord {
            sha256: format!("sha256:{digest}"),
            len: 1,
            bytes: None,
        }
    }

    #[test]
    fn diff_reports_created_modified_deleted_and_reliable_renames() {
        let mut previous = InputSnapshot::default();
        previous
            .files
            .insert("modules/planner/module.yaml".to_owned(), record("a"));
        previous
            .files
            .insert("modules/planner/entities.yaml".to_owned(), record("b"));
        previous
            .files
            .insert("modules/gone/module.yaml".to_owned(), record("c"));
        let mut current = InputSnapshot::default();
        current
            .files
            .insert("modules/planner/module.yaml".to_owned(), record("a2"));
        current
            .files
            .insert("modules/planner/entities.yaml".to_owned(), record("b"));
        current
            .files
            .insert("modules/moved/module.yaml".to_owned(), record("c"));
        let batch = diff(&previous, &current);
        assert_eq!(batch.len(), 3);
        let modified = batch
            .rows
            .iter()
            .find(|row| row.path == "modules/planner/module.yaml")
            .expect("modified row");
        assert_eq!(modified.kind, ChangeKind::Modified);
        let deleted = batch
            .rows
            .iter()
            .find(|row| row.path == "modules/gone/module.yaml")
            .expect("deleted row");
        assert_eq!(deleted.kind, ChangeKind::Deleted);
        let created = batch
            .rows
            .iter()
            .find(|row| row.path == "modules/moved/module.yaml")
            .expect("created row");
        assert_eq!(created.kind, ChangeKind::Created);
        assert_eq!(
            created.rename_of.as_deref(),
            Some("modules/gone/module.yaml")
        );
    }

    #[test]
    fn ambiguous_digest_matches_do_not_link_renames() {
        let mut previous = InputSnapshot::default();
        previous
            .files
            .insert("modules/a/module.yaml".to_owned(), record("same"));
        previous
            .files
            .insert("modules/b/module.yaml".to_owned(), record("same"));
        let mut current = InputSnapshot::default();
        current
            .files
            .insert("modules/c/module.yaml".to_owned(), record("same"));
        let batch = diff(&previous, &current);
        assert_eq!(
            batch
                .rows
                .iter()
                .filter(|row| row.rename_of.is_some())
                .count(),
            0
        );
    }

    #[test]
    fn generated_and_semantic_classes_stay_separate() {
        let mut previous = InputSnapshot::default();
        previous
            .files
            .insert("project.yaml".to_owned(), record("p"));
        let mut current = previous.clone();
        current
            .generated
            .insert(".lekalo/generated/x".to_owned(), record("g"));
        let batch = diff(&previous, &current);
        assert_eq!(batch.semantic_rows().count(), 0);
        assert_eq!(batch.generated_rows().count(), 1);
        assert_eq!(batch.dirty_paths().len(), 1);
    }
}
