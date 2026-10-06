//! The closed search contract identities (issue #93).
//!
//! Every search artifact takes the product version of its implementing
//! commit (`0.8.0`) per `docs/versioning.md`; unchanged upstream contracts
//! keep their exact accepted identities and bytes. The ranking/coverage
//! recipe is versioned separately from the schemas so a pure recipe change
//! never has to rewrite a wire contract.

/// The versioned ranking, normalization, coverage, and bounds recipe.
pub const RECIPE: &str = "semantic-search/1";

/// The closed wire discriminator of the search request contract.
pub const QUERY_SCHEMA_VERSION: &str = "lekalo/search-query/v0.8.0";
/// The closed identity of the search request contract.
pub const QUERY_IDENTITY: &str = "dev.lekalo.search-query@0.8.0";

/// The closed wire discriminator of the search result contract.
pub const RESULT_SCHEMA_VERSION: &str = "lekalo/search-result/v0.8.0";
/// The closed identity of the search result contract.
pub const RESULT_IDENTITY: &str = "dev.lekalo.search-result@0.8.0";

/// The closed wire discriminator of the durable index snapshot contract.
pub const INDEX_SCHEMA_VERSION: &str = "lekalo/search-index/v0.8.0";
/// The closed identity of the durable index snapshot contract.
pub const INDEX_IDENTITY: &str = "dev.lekalo.search-index@0.8.0";

/// The closed wire discriminator of the input-manifest contract.
pub const INPUTS_SCHEMA_VERSION: &str = "lekalo/search-inputs/v0.8.0";
/// The closed identity of the input-manifest contract.
pub const INPUTS_IDENTITY: &str = "dev.lekalo.search-inputs@0.8.0";

/// Maximum query length in Unicode scalar values (inclusive).
pub const MAX_QUERY_SCALARS: usize = 512;
/// Maximum number of normalized query terms.
pub const MAX_QUERY_TERMS: usize = 32;
/// Default page size.
pub const DEFAULT_LIMIT: usize = 20;
/// Maximum page size (inclusive).
pub const MAX_LIMIT: usize = 100;
/// Maximum cards in one index.
pub const MAX_CARDS: usize = 100_000;
/// Maximum posting/relation entries in one index.
pub const MAX_POSTINGS: usize = 1_000_000;
/// Maximum canonical index payload bytes.
pub const MAX_INDEX_BYTES: usize = 32 * 1024 * 1024;
/// Maximum serialized response bytes.
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
/// Maximum cursor token bytes (before/after encoding).
pub const MAX_CURSOR_BYTES: usize = 1024;
/// Maximum description text per card, in Unicode scalar values.
pub const MAX_DESCRIPTION: usize = 512;
/// Maximum refs per bounded preview list.
pub const MAX_PREVIEWS: usize = 8;
/// Maximum bounded byte length of one echoed input path/name string.
pub const MAX_PATH_BYTES: usize = 512;
/// Bounded edit distance for opt-in fuzzy mode (inclusive).
pub const MAX_FUZZY_EDITS: u32 = 2;
/// Maximum candidate keys fuzzy mode may expand before scoring.
pub const MAX_FUZZY_CANDIDATES: usize = 10_000;

/// The sha256 digest spelling used across every search pin.
pub(crate) fn is_sha256(text: &str) -> bool {
    let hex = match text.strip_prefix("sha256:") {
        Some(hex) => hex,
        None => return false,
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
