//! The identity, bound, and grammar constants of the target profile
//! contract (issue #29).
//!
//! The profile contract (`dev.lekalo.target-profile@1.0.0`) is independent
//! of the product release, the Model/IR/protocol contract versions, the
//! diagnostic registry, and the adapter process protocol. The embedded
//! component definition registry (`dev.lekalo.target-components@1.0.0`)
//! is likewise independently versioned. Every bound here has a matching
//! JSON Schema constraint; the paired Node gate pins them together.

/// The identity of the published profile contract artifact.
pub const IDENTITY: &str = "dev.lekalo.target-profile@1.0.0";

/// The schema identity of the profile contract
/// (`lekalo/target-profile/v1.0.0`).
pub const SCHEMA_VERSION: &str = "lekalo/target-profile/v1.0.0";

/// The identity of the embedded component definition registry.
pub const COMPONENTS_IDENTITY: &str = "dev.lekalo.target-components@1.0.0";

/// The definition version every embedded component was written under.
pub const COMPONENTS_DEFINITION_VERSION: &str = "1.0.0";

/// Maximum number of profiles in one profile document.
pub const MAX_PROFILES: usize = 64;

/// Maximum number of provided capabilities on one component definition.
pub const MAX_PROVIDED: usize = 64;

/// Maximum number of component requirements on one component definition.
pub const MAX_REQUIRES_COMPONENTS: usize = 8;

/// Maximum number of capability requirements on one component definition.
pub const MAX_REQUIRES_CAPABILITIES: usize = 8;

/// Maximum number of conflicts on one component definition.
pub const MAX_CONFLICTS: usize = 8;

/// Maximum number of explicit override acknowledgments on one profile.
pub const MAX_OVERRIDES: usize = 64;

/// Maximum depth of an inheritance chain (self excluded).
pub const MAX_EXTEND_DEPTH: usize = 8;

/// Maximum byte length of one profile id (the shared component grammar).
pub const MAX_ID_BYTES: usize = 128;

/// Whether one string is a grammatical profile or component identifier:
/// the closed lowercase component grammar shared with the lock's
/// `ComponentId` — lowercase ASCII, starts with a letter or digit, then
/// letters/digits/hyphens; never a path, never private data.
pub fn is_profile_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_are_wellformed() {
        for identity in [IDENTITY, COMPONENTS_IDENTITY] {
            let (name, version) = identity.rsplit_once('@').expect("identity spelling");
            assert!(name.starts_with("dev.lekalo."), "{identity}");
            assert_eq!(version, "1.0.0");
        }
        assert_eq!(SCHEMA_VERSION, "lekalo/target-profile/v1.0.0");
        assert_eq!(COMPONENTS_DEFINITION_VERSION, "1.0.0");
    }

    #[test]
    fn profile_id_grammar_is_the_closed_component_grammar() {
        assert!(is_profile_id("node-postgres-http"));
        assert!(is_profile_id("laravel-postgres-http"));
        assert!(is_profile_id("0abc"));
        assert!(!is_profile_id(""));
        assert!(!is_profile_id("-leading"));
        assert!(!is_profile_id("Upper"));
        assert!(!is_profile_id("has.dot"));
        assert!(!is_profile_id("has/slash"));
        assert!(!is_profile_id("has..double"));
        assert!(!is_profile_id(&"a".repeat(MAX_ID_BYTES + 1)));
        assert!(is_profile_id(&"a".repeat(MAX_ID_BYTES)));
    }
}
