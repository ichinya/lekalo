//! The owner-published PostgreSQL capability version matrix (issue
//! #69).
//!
//! One static table answering per-major support for the versioned
//! engine capabilities. The matrix is normative evidence, not
//! folklore: a profile pin must resolve to one published major row or
//! the profile is refused as unsupported — never clamped — and a
//! capability a version lacks is answered `unsupported`/`partial` by
//! the row, which is what keeps capability differences versioned.
//! Majors below the published floor (PostgreSQL 14 left upstream
//! support in November 2026) are absent by construction.

use crate::transaction_concurrency::SnapshotSupport;

/// One published major-version row of the matrix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MatrixRow {
    /// The exact major version.
    pub major: u32,
    /// The support answer per capability id (`storage.postgres.*`).
    pub answers: &'static [(&'static str, SnapshotSupport)],
}

/// The capability ids of the PostgreSQL matrix, byte-sorted.
pub const CAPABILITY_IDS: &[&str] = &[
    "storage.postgres.advisory_lock",
    "storage.postgres.covering_index",
    "storage.postgres.declarative_partition",
    "storage.postgres.explain",
    "storage.postgres.generated_column",
    "storage.postgres.identity_column",
    "storage.postgres.jsonb",
    "storage.postgres.merge",
    "storage.postgres.partial_index",
    "storage.postgres.rls",
    "storage.postgres.row_lock_modes",
    "storage.postgres.sequence",
    "storage.postgres.serializable_ssi",
    "storage.postgres.skip_locked",
    "storage.postgres.sql_json",
    "storage.postgres.temporal_constraint",
    "storage.postgres.unique_nulls_not_distinct",
];

const FULL: SnapshotSupport = SnapshotSupport::Full;

/// The published matrix rows (majors 15–18). Every row carries the
/// complete capability set in canonical id order; a row missing a
/// capability would be a developer fault refused by the paired test.
pub const ROWS: &[MatrixRow] = &[
    MatrixRow {
        major: 15,
        answers: &[
            ("storage.postgres.advisory_lock", FULL),
            ("storage.postgres.covering_index", FULL),
            ("storage.postgres.declarative_partition", FULL),
            ("storage.postgres.explain", FULL),
            ("storage.postgres.generated_column", FULL),
            ("storage.postgres.identity_column", FULL),
            ("storage.postgres.jsonb", FULL),
            ("storage.postgres.merge", FULL),
            ("storage.postgres.partial_index", FULL),
            ("storage.postgres.rls", FULL),
            ("storage.postgres.row_lock_modes", FULL),
            ("storage.postgres.sequence", FULL),
            ("storage.postgres.serializable_ssi", FULL),
            ("storage.postgres.skip_locked", FULL),
            ("storage.postgres.sql_json", SnapshotSupport::Partial),
            (
                "storage.postgres.temporal_constraint",
                SnapshotSupport::Unsupported,
            ),
            ("storage.postgres.unique_nulls_not_distinct", FULL),
        ],
    },
    MatrixRow {
        major: 16,
        answers: &[
            ("storage.postgres.advisory_lock", FULL),
            ("storage.postgres.covering_index", FULL),
            ("storage.postgres.declarative_partition", FULL),
            ("storage.postgres.explain", FULL),
            ("storage.postgres.generated_column", FULL),
            ("storage.postgres.identity_column", FULL),
            ("storage.postgres.jsonb", FULL),
            ("storage.postgres.merge", FULL),
            ("storage.postgres.partial_index", FULL),
            ("storage.postgres.rls", FULL),
            ("storage.postgres.row_lock_modes", FULL),
            ("storage.postgres.sequence", FULL),
            ("storage.postgres.serializable_ssi", FULL),
            ("storage.postgres.skip_locked", FULL),
            ("storage.postgres.sql_json", FULL),
            (
                "storage.postgres.temporal_constraint",
                SnapshotSupport::Unsupported,
            ),
            ("storage.postgres.unique_nulls_not_distinct", FULL),
        ],
    },
    MatrixRow {
        major: 17,
        answers: &[
            ("storage.postgres.advisory_lock", FULL),
            ("storage.postgres.covering_index", FULL),
            ("storage.postgres.declarative_partition", FULL),
            ("storage.postgres.explain", FULL),
            ("storage.postgres.generated_column", FULL),
            ("storage.postgres.identity_column", FULL),
            ("storage.postgres.jsonb", FULL),
            ("storage.postgres.merge", FULL),
            ("storage.postgres.partial_index", FULL),
            ("storage.postgres.rls", FULL),
            ("storage.postgres.row_lock_modes", FULL),
            ("storage.postgres.sequence", FULL),
            ("storage.postgres.serializable_ssi", FULL),
            ("storage.postgres.skip_locked", FULL),
            ("storage.postgres.sql_json", FULL),
            (
                "storage.postgres.temporal_constraint",
                SnapshotSupport::Unsupported,
            ),
            ("storage.postgres.unique_nulls_not_distinct", FULL),
        ],
    },
    MatrixRow {
        major: 18,
        answers: &[
            ("storage.postgres.advisory_lock", FULL),
            ("storage.postgres.covering_index", FULL),
            ("storage.postgres.declarative_partition", FULL),
            ("storage.postgres.explain", FULL),
            ("storage.postgres.generated_column", FULL),
            ("storage.postgres.identity_column", FULL),
            ("storage.postgres.jsonb", FULL),
            ("storage.postgres.merge", FULL),
            ("storage.postgres.partial_index", FULL),
            ("storage.postgres.rls", FULL),
            ("storage.postgres.row_lock_modes", FULL),
            ("storage.postgres.sequence", FULL),
            ("storage.postgres.serializable_ssi", FULL),
            ("storage.postgres.skip_locked", FULL),
            ("storage.postgres.sql_json", FULL),
            ("storage.postgres.temporal_constraint", FULL),
            ("storage.postgres.unique_nulls_not_distinct", FULL),
        ],
    },
];

/// The published matrix row for one major, or `None` when the major is
/// outside the owner-published support floor.
pub fn row_for(major: u32) -> Option<&'static MatrixRow> {
    ROWS.iter().find(|row| row.major == major)
}

/// The support answer of one capability on one major; `None` when
/// either the major or the capability id is outside the published
/// matrix.
pub fn answer(major: u32, capability: &str) -> Option<SnapshotSupport> {
    row_for(major)?
        .answers
        .iter()
        .find(|(id, _)| *id == capability)
        .map(|(_, support)| *support)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_complete_sorted_and_canonical() {
        assert_eq!(ROWS.len(), 4);
        for row in ROWS {
            let ids: Vec<&str> = row.answers.iter().map(|(id, _)| *id).collect();
            let mut sorted = ids.clone();
            sorted.sort_unstable();
            assert_eq!(ids, sorted, "row {} is id-sorted", row.major);
            assert_eq!(ids, CAPABILITY_IDS, "row {} is complete", row.major);
        }
    }

    #[test]
    fn the_per_version_differences_are_the_published_evidence() {
        assert_eq!(
            answer(15, "storage.postgres.sql_json"),
            Some(SnapshotSupport::Partial)
        );
        assert_eq!(answer(16, "storage.postgres.sql_json"), Some(FULL));
        assert_eq!(
            answer(17, "storage.postgres.temporal_constraint"),
            Some(SnapshotSupport::Unsupported)
        );
        assert_eq!(
            answer(18, "storage.postgres.temporal_constraint"),
            Some(FULL)
        );
        assert_eq!(answer(14, "storage.postgres.rls"), None, "below the floor");
        assert_eq!(answer(19, "storage.postgres.rls"), None, "unpublished");
    }
}
