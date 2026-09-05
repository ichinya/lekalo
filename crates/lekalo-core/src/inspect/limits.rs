//! The independently versioned inspect bound profile (issue #15).
//!
//! Every limit is a fixed owner-approved v1 constant (ADR-0014); no
//! caller may supply unlimited or larger bounds. Bounds never silently
//! drop mandatory sections: a bound crossing degrades the affected
//! section to `truncated` with returned/omitted counts, a reason, and
//! the deterministic frontier, and only a payload that cannot fit the
//! [`MAX_OUTPUT_BYTES`](super::version::MAX_OUTPUT_BYTES) bound fails
//! the whole invocation.

use super::version::{
    MAX_CANDIDATES, MAX_OUTPUT_BYTES, MAX_PROVENANCE_REFS, MAX_SECTION_ITEMS, MAX_SYMBOL_ID_BYTES,
    MAX_TEXT_BYTES,
};

/// The fixed inspect bound profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InspectLimits {
    /// Maximum items one section returns.
    pub section_items: usize,
    /// Maximum candidate ids in one ambiguity diagnostic.
    pub candidates: usize,
    /// Maximum bytes of one echoed semantic id.
    pub symbol_id_bytes: usize,
    /// Maximum bytes of one description or message value.
    pub text_bytes: usize,
    /// Maximum provenance references one item carries.
    pub provenance_refs: usize,
    /// Maximum bytes of one canonical payload.
    pub output_bytes: usize,
}

impl InspectLimits {
    /// The fixed v1 profile (ADR-0014).
    pub const fn v1() -> Self {
        Self {
            section_items: MAX_SECTION_ITEMS,
            candidates: MAX_CANDIDATES,
            symbol_id_bytes: MAX_SYMBOL_ID_BYTES,
            text_bytes: MAX_TEXT_BYTES,
            provenance_refs: MAX_PROVENANCE_REFS,
            output_bytes: MAX_OUTPUT_BYTES,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_profile_is_fixed() {
        let limits = InspectLimits::v1();
        assert_eq!(limits.section_items, 256);
        assert_eq!(limits.candidates, 32);
        assert_eq!(limits.symbol_id_bytes, 192);
        assert_eq!(limits.text_bytes, 4096);
        assert_eq!(limits.provenance_refs, 8);
        assert_eq!(limits.output_bytes, 1024 * 1024);
    }
}
