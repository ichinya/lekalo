//! Typed, injectable inputs for the task-links attachment join (issue #52).
//!
//! Digest pins are facts, so they are recomputed, never trusted. This
//! module carries the verified environment facts into the pure core:
//! the canonical IR digest of the live compilation, the accepted
//! owner-contract pin digests (computed by the CLI adapter from the
//! exact checkout contract files), and the helpers to recompute the
//! attachment revision from the canonical attachment bytes. Everything
//! here is deterministic; no filesystem, network, clock, or provider
//! access lives in the core itself — the CLI adapter owns every read.

use super::{diagnostic, wire};

/// The canonical attachment members the revision digest covers: every
/// member of the closed attachment except `attachmentRevision` itself,
/// in canonical document order.
pub const REVISION_MEMBERS: [&str; 16] = [
    "schemaVersion",
    "identity",
    "projectId",
    "modelRef",
    "irRef",
    "ownerContractPins",
    "entities",
    "relations",
    "uniqueConstraints",
    "providerContracts",
    "linkCapabilityRequirements",
    "fieldPolicies",
    "actionPolicies",
    "operationBindings",
    "deliveryRules",
    "contributions",
];

/// The canonical IR digest of one compilation: lowercase SHA-256 over
/// the canonical IR bytes — the exact bytes `load --ir` renders and
/// the cache fragments pin.
pub fn ir_digest(compilation: &crate::ir::Compilation) -> String {
    format!(
        "sha256:{}",
        crate::digest::sha256_hex(compilation.project.to_canonical_json().as_bytes())
    )
}

/// Compute the canonical attachment revision: the `sha256:` digest of
/// the compact serialization of every revision-covered member with
/// byte-sorted member names (the `serde_json` map's canonical order —
/// no dependency on input key order). Returns `None` for a document
/// that is not an object or that lacks any required member.
pub fn compute_revision(canonical_bytes: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(canonical_bytes).ok()?;
    let object = value.as_object()?;
    let mut revision = serde_json::Map::new();
    for member in REVISION_MEMBERS {
        revision.insert(member.to_owned(), object.get(member)?.clone());
    }
    let bytes = serde_json::to_vec(&revision).ok()?;
    Some(format!("sha256:{}", crate::digest::sha256_hex(&bytes)))
}

/// Verify the Model/IR pin pair against the live compilation.
///
/// `irRef.digest` and `modelRef.irDigest` must equal the recomputed
/// canonical IR digest; divergence anywhere is one
/// `ir-digest-stale`/`ir-digest-mismatch` failure before any
/// validation, view, or projection runs.
pub fn verify_ir_digest(
    attachment: &wire::Attachment,
    compilation: &crate::ir::Compilation,
) -> Result<(), diagnostic::Failure> {
    let computed = wire::Digest::parse(&ir_digest(compilation))
        .ok_or_else(|| diagnostic::input_invalid("ir-digest-stale"))?;
    if *attachment.ir_digest() != computed {
        return Err(diagnostic::input_invalid("ir-digest-stale"));
    }
    if attachment.model_ref().ir_digest != *attachment.ir_digest() {
        return Err(diagnostic::input_invalid("ir-digest-mismatch"));
    }
    Ok(())
}

/// Verify the declared attachment revision against the recomputed one.
///
/// `canonical_bytes` are the exact attachment input bytes; the closed
/// parser has already normalized every member, so the recomputation is
/// the same document every honest producer derives.
pub fn verify_revision(
    attachment: &wire::Attachment,
    canonical_bytes: &[u8],
) -> Result<(), diagnostic::Failure> {
    let computed = compute_revision(canonical_bytes)
        .ok_or_else(|| diagnostic::input_invalid("revision-digest"))?;
    let computed = wire::Digest::parse(&computed)
        .ok_or_else(|| diagnostic::input_invalid("revision-digest"))?;
    if *attachment.attachment_revision() != computed {
        return Err(diagnostic::input_invalid("revision-digest-stale"));
    }
    Ok(())
}

/// Verify every owner-contract pin against the accepted pin set.
///
/// `accepted` carries the real checkout digests (the CLI adapter hashes
/// the exact owner contract files). Every pinned family must be
/// present with the exact digest; a copied placeholder digest fails
/// closed with `owner-pin-stale` before anything else runs.
pub fn verify_owner_pins(
    attachment: &wire::Attachment,
    accepted: &[wire::OwnerPin],
) -> Result<(), diagnostic::Failure> {
    for pin in attachment.owner_pins() {
        let Some(verified) = accepted
            .iter()
            .find(|candidate| candidate.family == pin.family)
        else {
            return Err(diagnostic::input_invalid("owner-pin-family"));
        };
        if pin.identity != verified.identity || pin.digest != verified.digest {
            return Err(diagnostic::input_invalid("owner-pin-stale"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_covers_every_member_and_moves_with_any_member() {
        let mut object = serde_json::Map::new();
        for member in REVISION_MEMBERS {
            object.insert(member.to_owned(), serde_json::json!("a"));
        }
        let bytes = serde_json::to_vec(&object).unwrap();
        let first = compute_revision(&bytes).expect("revision");
        object.insert("projectId".to_owned(), serde_json::json!("b"));
        let bytes = serde_json::to_vec(&object).unwrap();
        let second = compute_revision(&bytes).expect("revision");
        assert_ne!(first, second, "a covered member change moves the revision");
        // A document missing a covered member never yields a revision.
        object.remove("deliveryRules");
        let bytes = serde_json::to_vec(&object).unwrap();
        assert!(compute_revision(&bytes).is_none());
    }

    #[test]
    fn owner_pin_verification_is_exact_per_family() {
        let accepted = vec![wire::OwnerPin {
            family: "authorization".to_owned(),
            identity: "dev.lekalo.authorization@0.2.16".to_owned(),
            digest: wire::Digest::parse(&format!("sha256:{}", "1".repeat(64))).unwrap(),
        }];
        let attachment_pin = wire::OwnerPin {
            family: "authorization".to_owned(),
            identity: "dev.lekalo.authorization@0.2.16".to_owned(),
            digest: wire::Digest::parse(&format!("sha256:{}", "2".repeat(64))).unwrap(),
        };
        assert!(verify_owner_pins_stub(&attachment_pin, &accepted).is_err());
        let matching = wire::OwnerPin {
            family: "authorization".to_owned(),
            identity: "dev.lekalo.authorization@0.2.16".to_owned(),
            digest: wire::Digest::parse(&format!("sha256:{}", "1".repeat(64))).unwrap(),
        };
        assert!(verify_owner_pins_stub(&matching, &accepted).is_ok());
    }

    // The stub keeps the unit test focused on the pin comparison; the
    // real entry point takes the full attachment.
    fn verify_owner_pins_stub(
        pin: &wire::OwnerPin,
        accepted: &[wire::OwnerPin],
    ) -> Result<(), diagnostic::Failure> {
        let Some(verified) = accepted.iter().find(|c| c.family == pin.family) else {
            return Err(diagnostic::input_invalid("owner-pin-family"));
        };
        if pin.identity != verified.identity || pin.digest != verified.digest {
            return Err(diagnostic::input_invalid("owner-pin-stale"));
        }
        Ok(())
    }
}
