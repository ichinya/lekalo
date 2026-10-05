//! Closed file handoff into the existing typed ChangedInputSet; no Git parser.
use super::{
    diagnostic,
    wire::{self, State},
};
use crate::diagnostics::DiagnosticSet;
use crate::impact::{
    ChangedInput, ChangedInputSet, ChangedMode, EntryEvidence, FileChange, MemberChange, MemberSeed,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Member {
    pub entity: String,
    pub field: String,
    pub change: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub symbol_ids: Vec<String>,
    pub members: Vec<Member>,
    pub path: String,
    pub from_path: State<String>,
    pub change: String,
    pub evidence: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangeInput {
    pub schema_version: String,
    pub identity: String,
    pub mode: String,
    pub base_revision: State<String>,
    pub candidate_revision: State<String>,
    pub entries: Vec<Entry>,
}
impl ChangeInput {
    pub fn parse(bytes: &[u8]) -> Result<ChangedInputSet, DiagnosticSet> {
        let v: Self = wire::decode(bytes)?;
        if v.schema_version != format!("lekalo/coupling-change-input/v{}", wire::VERSION)
            || v.identity != format!("dev.lekalo.coupling-change-input@{}", wire::VERSION)
        {
            return Err(diagnostic::unsupported("change-input-version"));
        }
        if !matches!(v.base_revision, State::Known { .. } | State::Unknown)
            || !matches!(v.candidate_revision, State::Known { .. } | State::Unknown)
            || v.entries.len() > 2_000
        {
            return Err(diagnostic::invalid("change-input-state-or-limit"));
        }
        let mode = match v.mode.as_str() {
            "committed" => ChangedMode::Committed,
            "index" => ChangedMode::Index,
            "worktree" => ChangedMode::Worktree,
            "mixed" => ChangedMode::Mixed,
            _ => return Err(diagnostic::invalid("change-mode")),
        };
        let mut entries = vec![];
        for e in v.entries {
            let change = match e.change.as_str() {
                "added" => FileChange::Added,
                "modified" => FileChange::Modified,
                "deleted" => FileChange::Deleted,
                "renamed" => FileChange::Renamed,
                _ => return Err(diagnostic::invalid("file-change")),
            };
            let evidence = match e.evidence.as_str() {
                "canonical" => EntryEvidence::Canonical,
                "verified" => EntryEvidence::Verified,
                "extracted" => EntryEvidence::Extracted,
                "stale" => EntryEvidence::Stale,
                "unknown" => EntryEvidence::Unknown,
                _ => return Err(diagnostic::invalid("change-evidence")),
            };
            if !matches!(e.from_path, State::Known { .. } | State::Unknown) {
                return Err(diagnostic::invalid("from-path-state"));
            }
            let members = e
                .members
                .iter()
                .map(|m| {
                    let change = match m.change.as_str() {
                        "removed" => MemberChange::Removed,
                        "type-narrowed" => MemberChange::TypeNarrowed,
                        _ => return Err(diagnostic::invalid("member-change")),
                    };
                    MemberSeed::new(&m.entity, &m.field, change)
                        .map_err(|_| diagnostic::invalid("member-seed"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            entries.push(
                ChangedInput::new(
                    e.symbol_ids,
                    members,
                    Some((&e.path, e.from_path.value().map(String::as_str))),
                    change,
                    evidence,
                )
                .map_err(|_| diagnostic::invalid("change-entry"))?,
            );
        }
        ChangedInputSet::from_entries(
            mode,
            v.base_revision.value().cloned(),
            v.candidate_revision.value().cloned(),
            entries,
        )
        .map_err(|_| diagnostic::invalid("change-set"))
    }
}
