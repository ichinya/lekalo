//! Closed change-policy decoding (issue #73).
//!
//! A policy may authorize otherwise-denied breaking compatibility classes
//! for a reviewed batch. It can never disable structural validity, semantic
//! validation, path safety, ownership proofs, or required evidence. Absent
//! policy means the restrictive default: no breaking class is allowed.

use serde_json::Value as Json;

use super::ChangeError;

/// The closed breaking compatibility classes a policy may allow.
pub const BREAKING_CLASSES: [&str; 4] = [
    "data-loss-risk",
    "storage-migration-required",
    "wire-breaking",
    "source-breaking",
];

/// Maximum accepted policy bytes before any decode runs.
const MAX_POLICY_BYTES: usize = 64 * 1024;

/// The effective, pinned change policy of one preparation run.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Policy {
    /// The allowed breaking classes in canonical order (subset order of
    /// [`BREAKING_CLASSES`]).
    pub allow_breaking_classes: Vec<String>,
    /// The exact `sha256:` digest of the policy bytes, when one was loaded.
    pub digest: Option<String>,
}

impl Policy {
    /// The restrictive default: nothing breaking is allowed.
    pub fn restrictive() -> Self {
        Self::default()
    }

    /// Decode and pin policy bytes. Duplicate JSON keys are refused (the
    /// repository `Unique` convention), the input is byte-bounded like the
    /// request decoder, and every member is required except the
    /// restrictive-default `allowBreakingClasses` — matching the closed
    /// schema (review finding 9).
    pub fn decode(bytes: &[u8]) -> Result<Self, ChangeError> {
        if bytes.len() > MAX_POLICY_BYTES || bytes.is_empty() {
            return Err(ChangeError::PolicyInvalid {
                detail: "policy-bounds",
            });
        }
        let value: Json =
            super::request::unique_json(bytes).map_err(|_| ChangeError::PolicyInvalid {
                detail: "json-shape",
            })?;
        let map = value.as_object().ok_or(ChangeError::PolicyInvalid {
            detail: "root-not-object",
        })?;
        for key in map.keys() {
            if !matches!(
                key.as_str(),
                "schemaVersion" | "identity" | "allowBreakingClasses"
            ) {
                return Err(ChangeError::PolicyInvalid {
                    detail: "unknown-member",
                });
            }
        }
        if map.get("schemaVersion").and_then(Json::as_str)
            != Some(super::version::POLICY_SCHEMA_VERSION)
        {
            return Err(ChangeError::PolicyInvalid {
                detail: "schema-version",
            });
        }
        if map.get("identity").and_then(Json::as_str) != Some(super::version::POLICY_IDENTITY) {
            return Err(ChangeError::PolicyInvalid { detail: "identity" });
        }
        let mut allow_breaking_classes = Vec::new();
        if let Some(classes) = map.get("allowBreakingClasses") {
            let items = classes.as_array().ok_or(ChangeError::PolicyInvalid {
                detail: "classes-not-array",
            })?;
            if items.len() > BREAKING_CLASSES.len() {
                return Err(ChangeError::PolicyInvalid {
                    detail: "classes-over-bound",
                });
            }
            for item in items {
                let class = item.as_str().ok_or(ChangeError::PolicyInvalid {
                    detail: "class-not-string",
                })?;
                if !BREAKING_CLASSES.contains(&class) {
                    return Err(ChangeError::PolicyInvalid {
                        detail: "unknown-class",
                    });
                }
                if !allow_breaking_classes
                    .iter()
                    .any(|allowed| allowed == class)
                {
                    allow_breaking_classes.push(class.to_owned());
                }
            }
        }
        allow_breaking_classes.sort_by(|left, right| {
            BREAKING_CLASSES
                .iter()
                .position(|candidate| candidate == left)
                .cmp(
                    &BREAKING_CLASSES
                        .iter()
                        .position(|candidate| candidate == right),
                )
        });
        Ok(Self {
            allow_breaking_classes,
            digest: Some(format!("sha256:{}", crate::digest::sha256_hex(bytes))),
        })
    }

    /// Whether this policy allows the given breaking class.
    pub fn allows(&self, class: &str) -> bool {
        self.allow_breaking_classes
            .iter()
            .any(|allowed| allowed == class)
    }

    /// Whether every reported breaking class is allowed.
    pub fn allows_all(&self, classes: &[String]) -> bool {
        classes.iter().all(|class| self.allows(class))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "{\"schemaVersion\":\"lekalo/change-policy/v0.8.0\",\"identity\":\"dev.lekalo.change-policy@0.8.0\"";

    #[test]
    fn restrictive_default_allows_nothing() {
        let policy = Policy::restrictive();
        assert!(policy.digest.is_none());
        assert!(!policy.allows("wire-breaking"));
    }

    #[test]
    fn classes_decode_and_pin_the_policy_bytes() {
        let policy = Policy::decode(
            format!("{HEADER},\"allowBreakingClasses\":[\"wire-breaking\"]}}").as_bytes(),
        )
        .expect("policy decodes");
        assert!(policy.allows("wire-breaking"));
        assert!(!policy.allows("additive"));
        assert!(policy.digest.expect("pinned").starts_with("sha256:"));
    }

    #[test]
    fn unknown_class_and_member_fail_closed() {
        let invalid = Policy::decode(
            format!("{HEADER},\"allowBreakingClasses\":[\"behavioral\"]}}").as_bytes(),
        );
        assert!(matches!(
            invalid,
            Err(ChangeError::PolicyInvalid {
                detail: "unknown-class"
            })
        ));
        let invalid = Policy::decode(format!("{HEADER},\"force\":true}}").as_bytes());
        assert!(matches!(
            invalid,
            Err(ChangeError::PolicyInvalid {
                detail: "unknown-member"
            })
        ));
    }

    #[test]
    fn duplicate_members_are_refused_not_last_wins() {
        // A reviewed policy showing `[]` must never silently carry a
        // hidden authorizing duplicate (review finding 9).
        let invalid = Policy::decode(
            format!(
                "{HEADER},\"allowBreakingClasses\":[],\"allowBreakingClasses\":[\"wire-breaking\"]}}"
            )
            .as_bytes(),
        );
        assert!(matches!(
            invalid,
            Err(ChangeError::PolicyInvalid {
                detail: "json-shape"
            })
        ));
    }

    #[test]
    fn absent_and_over_limit_policy_bytes_fail_closed() {
        assert!(matches!(
            Policy::decode(b""),
            Err(ChangeError::PolicyInvalid {
                detail: "policy-bounds"
            })
        ));
        let oversized = vec![b' '; MAX_POLICY_BYTES + 1];
        assert!(matches!(
            Policy::decode(&oversized),
            Err(ChangeError::PolicyInvalid {
                detail: "policy-bounds"
            })
        ));
    }

    #[test]
    fn restrictive_header_without_classes_is_valid() {
        let policy = Policy::decode(format!("{HEADER}}}").as_bytes()).expect("decodes");
        assert!(policy.allow_breaking_classes.is_empty());
        assert!(!policy.allows("wire-breaking"));
    }
}
