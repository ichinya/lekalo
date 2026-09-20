//! Signature policy evaluation (issue #32).
//!
//! The policy is closed (`unsigned|optional|required`) and declared per
//! manifest. Digest verification is always mandatory and lives in
//! [`super::integrity`]; this module evaluates only the signature
//! *provenance* honestly:
//!
//! - `unsigned`: no signature block may exist; nothing to verify.
//! - `optional`: a declared signature is recorded evidence; without a
//!   shipped verifier it stays unverified but does not block.
//! - `required`: a signature must verify under a shipped verifier. v1
//!   ships **no cryptographic verifier**, so every scheme answers
//!   `adapter.signature-unverified` (unavailable) — an honest refusal,
//!   never a pass and never a weakening.
//!
//! The recommended first real scheme is minisign/ed25519 (the smallest
//! honest verifier); until one lands under review, `required` is a
//! refused posture, not a fake success.

use super::manifest::{ManifestDocument, SignaturePolicy};
use super::types::PackageFailure;

/// Evaluate the declared signature policy for one manifest.
pub fn evaluate(manifest: &ManifestDocument) -> Result<(), PackageFailure> {
    match manifest.signature_policy() {
        SignaturePolicy::Unsigned => {
            if manifest.signature().is_some() {
                return Err(PackageFailure::ManifestInvalid {
                    reason: "signature-policy".to_owned(),
                });
            }
            Ok(())
        }
        SignaturePolicy::Optional => {
            let _ = manifest.signature();
            Ok(())
        }
        SignaturePolicy::Required => {
            let scheme = manifest
                .signature()
                .map(|signature| signature.scheme())
                .ok_or_else(|| PackageFailure::ManifestInvalid {
                    reason: "signature-policy".to_owned(),
                })?;
            Err(PackageFailure::SignatureUnverified {
                scheme: scheme.as_str().to_owned(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(policy: &str, signature: Option<serde_json::Value>) -> ManifestDocument {
        manifest_result(policy, signature).expect("parses")
    }

    fn manifest_result(
        policy: &str,
        signature: Option<serde_json::Value>,
    ) -> Result<ManifestDocument, PackageFailure> {
        let mut integrity = serde_json::json!({
            "packageDigest": format!("sha256:{}", "77".repeat(32)),
            "files": [ { "path": "a.mjs", "digest": format!("sha256:{}", "88".repeat(32)), "bytes": 3 } ],
            "signaturePolicy": policy,
        });
        integrity["signature"] = signature.into();
        let json = serde_json::json!({
            "schemaVersion": crate::adapter_package::version::MANIFEST_SCHEMA_VERSION,
            "identity": crate::adapter_package::version::MANIFEST_IDENTITY,
            "adapter": { "id": "sig-adapter", "name": "Sig", "version": "1.0.0" },
            "source": { "kind": "path", "coordinate": "path:x", "digest": format!("sha256:{}", "99".repeat(32)) },
            "compatibility": {
                "protocolVersions": [crate::target_protocol::version::VERSION],
                "irVersions": [crate::ir::version::VERSION],
                "extensions": []
            },
            "executable": { "entry": "a.mjs" },
            "integrity": integrity,
            "status": "active",
            "revocation": null
        });
        ManifestDocument::from_value(json)
    }

    #[test]
    fn unsigned_without_a_signature_passes() {
        assert!(evaluate(&manifest("unsigned", None)).is_ok());
    }

    #[test]
    fn unsigned_with_a_signature_block_refuses_at_parse_time() {
        // The manifest cross-field rule refuses the combination before
        // the policy evaluator could ever see it.
        let signature = serde_json::json!({ "scheme": "minisign", "bundleDigest": format!("sha256:{}", "aa".repeat(32)) });
        let error = manifest_result("unsigned", Some(signature)).expect_err("unsigned + signature");
        assert!(matches!(error, PackageFailure::ManifestInvalid { .. }));
    }

    #[test]
    fn optional_passes_with_and_without_a_signature() {
        assert!(evaluate(&manifest("optional", None)).is_ok());
        let signature = serde_json::json!({ "scheme": "minisign", "bundleDigest": format!("sha256:{}", "aa".repeat(32)) });
        assert!(evaluate(&manifest("optional", Some(signature))).is_ok());
    }

    #[test]
    fn required_answers_honestly_unverified() {
        let signature = serde_json::json!({ "scheme": "minisign", "bundleDigest": format!("sha256:{}", "aa".repeat(32)) });
        let error =
            evaluate(&manifest("required", Some(signature))).expect_err("no shipped verifier");
        match error {
            PackageFailure::SignatureUnverified { scheme } => assert_eq!(scheme, "minisign"),
            other => panic!("unexpected: {other:?}"),
        }
    }
}
