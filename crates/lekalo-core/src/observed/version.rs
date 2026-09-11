//! Closed identity and bounds of the observed mode (issues #39 and #42).
//!
//! The observed index and the adapter scan document are two independent
//! wire identities. Versions evolve independently of the product release
//! and of every other contract family.

/// The closed module family label used in receipts and diagnostics.
pub const FAMILY: &str = "observed";

/// The observed index wire version.
pub const VERSION: &str = "1.1.0";

/// The exact `schema_version` literal of a persisted observed index.
pub const SCHEMA_VERSION: &str = "lekalo/observed-index/v1.1.0";

/// The exact `schema_version` literals of an adapter scan document: the
/// frozen issue #39 base plus the additive issue #42 binding-registry
/// extension. A scan in either spelling decodes; the additive members
/// (target, profile, candidates, native test bindings) exist only on a
/// 1.1.0 document.
pub const SCAN_SCHEMA_VERSIONS: [&str; 2] =
    ["lekalo/observed-scan/v1.0.0", "lekalo/observed-scan/v1.1.0"];

/// The current scan spelling new documents advertise.
pub const SCAN_SCHEMA_VERSION: &str = SCAN_SCHEMA_VERSIONS[1];

/// The registry identity of the observed index wire.
pub const INDEX_IDENTITY: &str = "dev.lekalo.observed-index@1.1.0";

/// The single mode this issue records; `contracted` mode (issue #40) is a
/// separate surface and never appears here.
pub const MODE: &str = "observed";

/// The runtime home of the observed index: the accepted
/// `lekalo.observed-model-draft` authority home (`.lekalo/import/**`).
pub const INDEX_DIR: &str = ".lekalo/import/observed";

/// The observed index file name inside [`INDEX_DIR`].
pub const INDEX_NAME: &str = "index.json";

/// Maximum accepted scan document bytes.
pub const MAX_SCAN_BYTES: usize = 4 * 1024 * 1024;

/// Maximum persisted or projected index bytes.
pub const MAX_INDEX_BYTES: usize = 8 * 1024 * 1024;

/// Maximum symbols per scan or index.
pub const MAX_SYMBOLS: usize = 10_000;

/// Maximum endpoints per scan or index.
pub const MAX_ENDPOINTS: usize = 2_000;

/// Maximum schema records per scan or index.
pub const MAX_SCHEMAS: usize = 5_000;

/// Maximum evidence references or fields on one symbol.
pub const MAX_REFERENCES: usize = 256;

/// Maximum enum values on one symbol.
pub const MAX_VALUES: usize = 256;

/// Maximum history entries on one binding.
pub const MAX_HISTORY: usize = 32;

/// Maximum attached native tests or gates on one symbol.
pub const MAX_ATTACHMENTS: usize = 64;

/// Maximum promoted symbols in one plan.
pub const MAX_PLAN_SYMBOLS: usize = 1_000;

/// Maximum candidates recorded on one binding (issue #42): an ambiguous
/// adapter names every plausible native symbol instead of picking one.
pub const MAX_CANDIDATES: usize = 64;

/// Maximum native test bindings per scan or index (issue #42).
pub const MAX_TEST_BINDINGS: usize = 2_000;

/// The prefix of every deterministic proposal identifier (issue #42).
pub const PROPOSAL_ID_PREFIX: &str = "prop-";

/// The closed binding relation vocabulary (issue #42). The relation of a
/// binding row follows from its section: symbol bindings implement,
/// endpoint bindings expose, and native test bindings verify.
pub const RELATIONS: [&str; 3] = ["implements", "verifies", "exposes"];

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

/// The adapter identity grammar: 1..=128 bytes of lowercase ASCII
/// letters, digits, `.`, `_`, `-`; it never starts with a separator.
pub fn is_adapter_id(text: &str) -> bool {
    (1..=128).contains(&text.len())
        && text.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || byte == b'.'
                || byte == b'_'
                || byte == b'-'
        })
        && text
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

/// The external native-test or gate id grammar: verbatim foreign ids,
/// bounded, printable, and control-character free.
pub fn is_external_id(text: &str) -> bool {
    (1..=128).contains(&text.len())
        && text
            .chars()
            .all(|character| !character.is_control() && character != '\u{7f}')
        && !text.starts_with(' ')
        && !text.ends_with(' ')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_grammar_is_exact() {
        assert!(is_sha256(&format!("sha256:{}", "a".repeat(64))));
        assert!(!is_sha256(&format!("sha256:{}", "A".repeat(64))));
        assert!(!is_sha256(&format!("sha256:{}", "a".repeat(63))));
        assert!(!is_sha256("a".repeat(64).as_str()));
    }

    #[test]
    fn adapter_ids_are_bounded_and_lowercase() {
        assert!(is_adapter_id("lekalo-target-node-typescript"));
        assert!(!is_adapter_id("-leading"));
        assert!(!is_adapter_id("UPPER"));
        assert!(!is_adapter_id(&"a".repeat(129)));
    }

    #[test]
    fn external_ids_reject_controls_and_padding() {
        assert!(is_external_id("npm test -- suite"));
        assert!(!is_external_id(" line"));
        assert!(!is_external_id("line\n"));
        assert!(!is_external_id(&"x".repeat(129)));
    }
}
