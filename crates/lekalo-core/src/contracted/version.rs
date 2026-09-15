//! Closed identity and bounds of contracted mode (issue #40).
//!
//! The conformed-binding registry is its own wire identity, independent
//! of the product release, of the Model/IR/protocol contract versions,
//! of the observed-index family, and of the diagnostic registry. The
//! limits below fail closed at every construction: bounded input never
//! scales the envelope.

/// The closed module family label used in receipts and diagnostics.
pub const FAMILY: &str = "contracted";

/// The conformed-binding registry wire version.
pub const VERSION: &str = "1.0.0";

/// The exact `schema_version` literal of a persisted registry.
pub const SCHEMA_VERSION: &str = "lekalo/conformed-binding-registry/v1.0.0";

/// The registry identity of the conformed-binding wire.
pub const IDENTITY: &str = "dev.lekalo.conformed-binding-registry@1.0.0";

/// The single mode this issue records; `observed` mode (issue #39) is a
/// separate surface and never appears here.
pub const MODE: &str = "contracted";

/// The runtime home of the conformed registry: the accepted
/// `lekalo.observed-model-draft` authority home (`.lekalo/import/**`).
pub const REGISTRY_DIR: &str = ".lekalo/import/contracted";

/// The registry file name inside [`REGISTRY_DIR`].
pub const REGISTRY_NAME: &str = "registry.json";

/// Maximum accepted registry document bytes.
pub const MAX_REGISTRY_BYTES: usize = 8 * 1024 * 1024;

/// Maximum symbols per registry.
pub const MAX_SYMBOLS: usize = 10_000;

/// Maximum support artifacts per registry.
pub const MAX_SUPPORT_ARTIFACTS: usize = 5_000;

/// Maximum native tests or gates attached to one symbol.
pub const MAX_ATTACHMENTS: usize = 64;

/// Maximum support artifacts owned by one symbol.
pub const MAX_SYMBOL_ARTIFACTS: usize = 32;

/// Maximum history entries on one binding.
pub const MAX_HISTORY: usize = 32;

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

/// The support-artifact path grammar: a POSIX logical path relative to
/// the project root, confined to the `.lekalo/generated/**` home so the
/// generated-artifact manifest can own it without orphaning the check.
pub fn is_support_path(text: &str) -> bool {
    let Some(rest) = text.strip_prefix(".lekalo/generated/") else {
        return false;
    };
    !rest.is_empty()
        && rest.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && segment.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'.'
                        || byte == b'_'
                        || byte == b'-'
                })
        })
        && text.len() <= 1024
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

    #[test]
    fn support_paths_confine_to_the_generated_home() {
        assert!(is_support_path(
            ".lekalo/generated/openapi/create-task.json"
        ));
        assert!(is_support_path(".lekalo/generated/types.ts"));
        assert!(!is_support_path("src/handler.ts"));
        assert!(!is_support_path("../escape.ts"));
        assert!(!is_support_path(".lekalo/generated/"));
        assert!(!is_support_path(".lekalo/generated/a//b.ts"));
        assert!(!is_support_path(".lekalo/generated/A.ts"));
    }
}
