use lekalo_openspec::*;
use std::collections::BTreeMap;

#[derive(Default)]
struct Memory(BTreeMap<String, Vec<u8>>);
impl Memory {
    fn spec(mut self, path: &str, text: &str) -> Self {
        self.0.insert(path.into(), text.as_bytes().to_vec());
        self
    }
    fn resolve(&self) -> Loaded {
        load_snapshot(
            self,
            &ProviderDecl {
                source: "spec".into(),
                root: "openspec".into(),
            },
        )
    }
}
impl Source for Memory {
    fn entries(&self, path: &str) -> Result<Vec<(String, EntryType)>, ReadError> {
        let mut rows = BTreeMap::new();
        for file in self.0.keys() {
            if let Some(rest) = file.strip_prefix(&format!("{path}/")) {
                let (name, kind) = match rest.split_once('/') {
                    Some((dir, _)) => (dir, EntryType::Directory),
                    None => (rest, EntryType::Other),
                };
                rows.insert(name.to_owned(), kind);
            }
        }
        if rows.is_empty() {
            Err(ReadError::NotFound)
        } else {
            Ok(rows.into_iter().collect())
        }
    }
    fn read_file_opt(
        &self,
        dir: &str,
        name: &str,
        limit: usize,
    ) -> Result<Option<Vec<u8>>, ReadError> {
        match self.0.get(&format!("{dir}/{name}")) {
            Some(bytes) if bytes.len() > limit => Err(ReadError::Limit),
            bytes => Ok(bytes.cloned()),
        }
    }
}
const BASE: &str =
    "# Planner\n\n## Requirements\n\n### Requirement: Focus task\nThe system SHALL focus a task.\n";

#[test]
fn archive_preserves_ids_and_digests() {
    let active = Memory::default().spec("openspec/specs/planner/spec.md", BASE)
        .spec("openspec/changes/add/specs/planner/spec.md", "## ADDED Requirements\n### Requirement: Finish task\nThe system SHALL finish a task.\n");
    let archived = Memory::default().spec(
        "openspec/specs/planner/spec.md",
        &format!("{BASE}\n### Requirement: Finish task\nThe system SHALL finish a task.\n"),
    );
    let Loaded::Resolved(a) = active.resolve() else {
        panic!("active resolution")
    };
    let Loaded::Resolved(b) = archived.resolve() else {
        panic!("archived resolution")
    };
    assert!(a.conflicts.is_empty());
    assert_eq!(
        a.entries
            .iter()
            .map(|e| (&e.id, &e.digest))
            .collect::<Vec<_>>(),
        b.entries
            .iter()
            .map(|e| (&e.id, &e.digest))
            .collect::<Vec<_>>()
    );
    assert_eq!(a.entries.len(), 2);
}

#[test]
fn conflicting_changes_do_not_choose_a_winner() {
    let delta = "## MODIFIED Requirements\n### Requirement: Focus task\nThe system SHALL focus another task.\n";
    let tree = Memory::default()
        .spec("openspec/specs/planner/spec.md", BASE)
        .spec("openspec/changes/a/specs/planner/spec.md", delta)
        .spec("openspec/changes/b/specs/planner/spec.md", delta);
    let Loaded::Resolved(result) = tree.resolve() else {
        panic!("resolution")
    };
    assert!(!result.conflicts.is_empty());
    assert!(result.entries.is_empty());
}

#[test]
fn missing_and_over_limit_are_explicit() {
    assert!(matches!(Memory::default().resolve(), Loaded::Absent));
    let tree = Memory::default().spec(
        "openspec/specs/planner/spec.md",
        &"x".repeat(limits::MAX_SPEC_BYTES + 1),
    );
    assert!(matches!(tree.resolve(), Loaded::Invalid("tree-over-limit")));
}

#[test]
fn lexical_compatibility_rejects_ambiguous_grammar() {
    let tree = Memory::default().spec("openspec/specs/planner/spec.md", &format!("\u{feff}{BASE}"));
    assert!(matches!(
        tree.resolve(),
        Loaded::Invalid("unsupported-native-grammar")
    ));
    assert_eq!(ids::title_slug("Focus  task!"), Some("focus-task".into()));
    assert!(ids::is_requirement_id("planner.REQ-focus-task"));
}
