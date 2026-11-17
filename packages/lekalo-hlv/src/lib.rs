//! HLV-specific revision mapping policy. The host owns generic trace assessment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MappingFailure {
    Missing,
    Conflicting,
    Stale,
}

/// An HLV test/gate mapping must have exactly one alias at the source revision.
/// Missing revisions are stale evidence; multiple aliases never pick a winner.
pub fn assess_mapping<'a>(
    revisions: impl IntoIterator<Item = Option<&'a str>>,
    source_revision: &str,
) -> Result<(), MappingFailure> {
    let mut revisions = revisions.into_iter();
    let Some(revision) = revisions.next() else {
        return Err(MappingFailure::Missing);
    };
    if revisions.next().is_some() {
        return Err(MappingFailure::Conflicting);
    }
    if revision != Some(source_revision) {
        return Err(MappingFailure::Stale);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requires_unique_current_evidence() {
        assert_eq!(assess_mapping([], "r1"), Err(MappingFailure::Missing));
        assert_eq!(
            assess_mapping([Some("r1"), Some("r1")], "r1"),
            Err(MappingFailure::Conflicting)
        );
        assert_eq!(assess_mapping([None], "r1"), Err(MappingFailure::Stale));
        assert_eq!(
            assess_mapping([Some("r0")], "r1"),
            Err(MappingFailure::Stale)
        );
        assert_eq!(assess_mapping([Some("r1")], "r1"), Ok(()));
    }
}
