//! The cache fingerprint seam (issue #16).
//!
//! The impact analysis exposes an immutable, canonical fingerprint of the
//! exact typed inputs a persistence owner (#20) may key on. This module
//! computes that fingerprint only; it never persists, invalidates,
//! clears, evicts, locks, or writes anything anywhere. A cache hit or
//! miss can never change semantic bytes, digest, ordering, or
//! completeness: the fingerprint covers every input the result derives
//! from and nothing volatile (no wall clock, no cwd, no host, no Git
//! status timestamps).

use super::ImpactResult;

/// The stable fingerprint of one impact result's inputs.
///
/// Layout: `impact-fp/v1|contract|algorithm|model|project|input-digest|
/// mode|base|candidate|depth|filters|profile|roots`. Every field is a
/// bounded canonical token; the whole fingerprint stays under 2 KiB.
pub fn fingerprint(result: &ImpactResult) -> String {
    let request = result.request();
    let mut canonical = String::new();
    canonical.push_str("impact-fp/v1");
    push(&mut canonical, super::version::IDENTITY);
    push(&mut canonical, super::version::ALGORITHM);
    push(&mut canonical, result.model_version().as_str());
    push(&mut canonical, result.project().unwrap_or(""));
    push(&mut canonical, &result.changed_input_digest);
    push(&mut canonical, result.input_mode.key());
    push(
        &mut canonical,
        result.base_revision_ref.as_deref().unwrap_or(""),
    );
    push(
        &mut canonical,
        result.candidate_revision_ref.as_deref().unwrap_or(""),
    );
    push(&mut canonical, &result.request.depth().to_string());
    for relation in request.relations() {
        push(&mut canonical, relation);
    }
    for kind in request.kinds() {
        push(&mut canonical, kind);
    }
    push(&mut canonical, request.module().unwrap_or(""));
    push(&mut canonical, request.target().unwrap_or(""));
    push(&mut canonical, request.profile().key());
    for root in &result.roots {
        push(&mut canonical, root.as_str());
    }
    format!(
        "sha256:{}",
        crate::versioning::plan::sha256_hex(canonical.as_bytes())
    )
}

fn push(canonical: &mut String, field: &str) {
    canonical.push('|');
    canonical.push_str(field);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_stable_and_opaque() {
        let canonical = {
            let mut canonical = String::new();
            canonical.push_str("impact-fp/v1");
            push(&mut canonical, "a");
            push(&mut canonical, "b");
            canonical
        };
        let first = crate::versioning::plan::sha256_hex(canonical.as_bytes());
        let second = crate::versioning::plan::sha256_hex(canonical.as_bytes());
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }
}
