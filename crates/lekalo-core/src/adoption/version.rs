//! Closed identity, bounds, and digest recipes of the #136 adoption
//! service.
//!
//! Every new wire family publishes at the 0.8.0 product generation. The
//! versions below are independent of every other contract family; the
//! frozen predecessor families are never relabeled here. Absence is a
//! typed `null`, never a fake zero digest.

/// The closed module family label used in receipts and diagnostics.
pub const FAMILY: &str = "adoption";

/// The product generation every new adoption family publishes at.
pub const VERSION: &str = "0.8.0";

/// The exact `schema_version` literal of an analysis document.
pub const ANALYSIS_SCHEMA_VERSION: &str = "lekalo/adoption-analysis/v0.8.0";
/// The exact `identity` literal of the analysis schema.
pub const ANALYSIS_IDENTITY: &str = "dev.lekalo.adoption-analysis@0.8.0";

/// The exact `schema_version` literal of a confirmation plan.
pub const CONFIRMATION_PLAN_SCHEMA_VERSION: &str = "lekalo/adoption-confirmation-plan/v0.8.0";
/// The exact `identity` literal of the confirmation-plan schema.
pub const CONFIRMATION_PLAN_IDENTITY: &str = "dev.lekalo.adoption-confirmation-plan@0.8.0";

/// The exact `schema_version` literal of a patch document.
pub const PATCH_SCHEMA_VERSION: &str = "lekalo/adoption-patch/v0.8.0";
/// The exact `identity` literal of the patch schema.
pub const PATCH_IDENTITY: &str = "dev.lekalo.adoption-patch@0.8.0";

/// The exact `schema_version` literal of a refactor plan.
pub const REFACTOR_PLAN_SCHEMA_VERSION: &str = "lekalo/adoption-refactor-plan/v0.8.0";
/// The exact `identity` literal of the refactor-plan schema.
pub const REFACTOR_PLAN_IDENTITY: &str = "dev.lekalo.adoption-refactor-plan@0.8.0";

/// The exact `schema_version` literal of a verification document.
pub const VERIFICATION_SCHEMA_VERSION: &str = "lekalo/adoption-verification/v0.8.0";
/// The exact `identity` literal of the verification schema.
pub const VERIFICATION_IDENTITY: &str = "dev.lekalo.adoption-verification@0.8.0";

/// The exact `schema_version` literal of an adoption receipt.
pub const RECEIPT_SCHEMA_VERSION: &str = "lekalo/adoption-receipt/v0.8.0";
/// The exact `identity` literal of the receipt schema.
pub const RECEIPT_IDENTITY: &str = "dev.lekalo.adoption-receipt@0.8.0";

/// The exact `schema_version` literal of a coverage document.
pub const COVERAGE_SCHEMA_VERSION: &str = "lekalo/adoption-coverage/v0.8.0";
/// The exact `identity` literal of the coverage schema.
pub const COVERAGE_IDENTITY: &str = "dev.lekalo.adoption-coverage@0.8.0";

/// The exact `schema_version` literal of a status projection.
pub const STATUS_REPORT_SCHEMA_VERSION: &str = "lekalo/adoption-status-report/v0.8.0";
/// The exact `identity` literal of the status-report schema.
pub const STATUS_REPORT_IDENTITY: &str = "dev.lekalo.adoption-status-report@0.8.0";

/// The exact `schema_version` literal of an impact projection.
pub const IMPACT_REPORT_SCHEMA_VERSION: &str = "lekalo/adoption-impact-report/v0.8.0";
/// The exact `identity` literal of the impact-report schema.
pub const IMPACT_REPORT_IDENTITY: &str = "dev.lekalo.adoption-impact-report@0.8.0";

/// The exact `schema_version` literal of a trace projection.
pub const TRACE_REPORT_SCHEMA_VERSION: &str = "lekalo/adoption-trace-report/v0.8.0";
/// The exact `identity` literal of the trace-report schema.
pub const TRACE_REPORT_IDENTITY: &str = "dev.lekalo.adoption-trace-report@0.8.0";

/// The runtime home of confirmed adoption receipts, inside the accepted
/// `.lekalo/import/**` authority area. Digest directory components use
/// the 64 lowercase hex bytes without the `sha256:` prefix.
pub const RECEIPTS_DIR: &str = ".lekalo/import/adoption/receipts";

/// The runtime home of recovery journals and exact local snapshots.
/// Runtime-only/private material: a filename never inherits permission
/// to publish, and no analysis command ever creates this home.
pub const RECOVERY_DIR: &str = ".lekalo/import/adoption/recovery";

/// The project-owned home of explicitly reviewed use-case scope records.
pub const USE_CASES_DIR: &str = ".lekalo/import/adoption/use-cases";

/// Maximum accepted input bytes for one closed adoption document.
pub const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;

/// Maximum definitions in one plan or analysis.
pub const MAX_DEFINITIONS: usize = 10_000;

/// Maximum writes in one plan or receipt.
pub const MAX_WRITES: usize = 4_096;

/// Maximum exact content bytes carried per patched file.
pub const MAX_FILE_BYTES: usize = 1024 * 1024;

/// Maximum source files fingerprinted into one pin set.
pub const MAX_SOURCE_FILES: usize = 4_096;

/// Maximum candidates on one ambiguity row.
pub const MAX_CANDIDATES: usize = 64;

/// Maximum rows in one coverage join.
pub const MAX_ROWS: usize = 10_000;

/// Maximum executed checks in one verification document.
pub const MAX_CHECKS: usize = 256;

/// Per-command execution ceiling for one verification check.
pub const CHECK_TIMEOUT_MS: u64 = 60_000;

/// Captured output ceiling per stream, in bytes.
pub const CHECK_OUTPUT_BYTES: usize = 64 * 1024;

/// Total disposable-copy ceiling, in bytes.
pub const COPY_TOTAL_BYTES: usize = 64 * 1024 * 1024;

/// Total disposable-copy file ceiling.
pub const COPY_TOTAL_FILES: usize = 20_000;

/// The scope selector grammar: one reviewed identifier, never a
/// wildcard, never a path.
pub fn is_scope_id(text: &str) -> bool {
    (1..=128).contains(&text.len())
        && text.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || byte == b'_'
                || byte == b'.'
                || byte == b'-'
        })
        && text
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
}

/// The semantic id grammar shared with the Model (`taskboard.task`).
pub fn is_semantic_id(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 128
        && text.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_lowercase())
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        })
}

/// The project-relative POSIX path grammar of every adoption document.
/// Bounded, lowercase, never absolute, never a traversal; dotted
/// runtime homes (`.lekalo/...`) are admitted, `.` and `..` never are.
pub fn is_rel_path(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 1024
        && !text.starts_with('/')
        && !text.contains('\\')
        && text.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && segment.bytes().next().is_some_and(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'.'
                })
                && segment.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'.' | b'_' | b'-')
                })
        })
}

/// The external native id grammar: verbatim foreign ids, bounded,
/// printable, control-character free.
pub fn is_native_id(text: &str) -> bool {
    (1..=128).contains(&text.len())
        && text
            .chars()
            .all(|character| !character.is_control() && character != '\u{7f}')
        && !text.starts_with(' ')
        && !text.ends_with(' ')
}

/// The module id grammar shared with the Model (`taskboard`).
pub fn is_module_id(text: &str) -> bool {
    (1..=63).contains(&text.len())
        && text
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && text
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// The exact `sha256:<64 lowercase hex>` grammar.
pub fn is_sha256(text: &str) -> bool {
    let Some(hex) = text.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// The two bounded (32 + 32) hex directory components of a digest: the
/// project path policy caps one segment at 64 bytes, so the 64 hex bytes
/// split across two nested components.
pub fn digest_dir(digest: &str) -> Option<(String, String)> {
    let hex = digest.strip_prefix("sha256:")?;
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return None;
    }
    Some((hex[..32].to_owned(), hex[32..].to_owned()))
}

/// The bounded snapshot file name of one path: the first 32 hex bytes of
/// the path hash plus the extension (36 bytes, within the 64 cap).
pub fn snapshot_name(path: &str) -> String {
    format!("{}.bin", &crate::digest::sha256_hex(path.as_bytes())[..32])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_ids_are_bounded_tokens() {
        assert!(is_scope_id("taskboard.complete_case"));
        assert!(!is_scope_id("-leading"));
        assert!(!is_scope_id("*"));
        assert!(!is_scope_id("src/tasks.ts"));
    }

    #[test]
    fn semantic_ids_keep_the_model_grammar() {
        assert!(is_semantic_id("taskboard.complete_task"));
        assert!(is_semantic_id("taskboard"));
        assert!(!is_semantic_id("Taskboard.task"));
        assert!(!is_semantic_id("taskboard."));
    }

    #[test]
    fn rel_paths_reject_traversal_and_absolute_forms() {
        assert!(is_rel_path("src/tasks.ts"));
        assert!(is_rel_path(".lekalo/import/adoption/receipts/x.json"));
        assert!(!is_rel_path("/absolute"));
        assert!(!is_rel_path("../escape"));
        assert!(!is_rel_path("a//b"));
        assert!(!is_rel_path("C:/tmp"));
    }

    #[test]
    fn digest_dirs_strip_the_prefix_only_for_valid_digests() {
        let digest = format!("sha256:{}", "a".repeat(64));
        assert_eq!(digest_dir(&digest), Some(("a".repeat(32), "a".repeat(32))));
        assert!(digest_dir("sha256:abc").is_none());
        assert!(digest_dir("plaintext").is_none());
        let name = snapshot_name("src/tasks.mjs");
        assert_eq!(name.len(), 36);
        assert!(name.ends_with(".bin"));
    }
}
