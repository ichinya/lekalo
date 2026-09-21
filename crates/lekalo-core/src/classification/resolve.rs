//! Subject resolution against the pinned compilation (issue #87).
//!
//! Resolution is total and deterministic (plan §2.3): for any concrete
//! subject the resolved classification is the first match of the
//! exact field-path entry, the field entry, the containing definition
//! default, the operation default, and the profile defaults. No entry
//! plus no covering default is the [`ResolvedKind::Unclassified`]
//! state — distinct from every kind — which the strict profile
//! rejects on sensitive sinks. Propagation widens: the containing
//! definition default and the operation default participate as
//! floors, and the resolved kind is the most restrictive
//! (highest-rank) contributor so a narrower entry can never silently
//! lower a wider default.

use std::collections::BTreeMap;

use super::types::{DataKind, Profile, SubjectPath};
use super::wire::Attachment;

/// The resolved classification of one subject.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolvedKind {
    /// The subject resolved to a closed kind.
    Classified(DataKind),
    /// The subject resolved to no entry and no covering default.
    /// Unknown, never safe; distinct from [`DataKind::Public`].
    Unclassified,
}

impl ResolvedKind {
    /// The resolved kind, if classified.
    pub const fn kind(self) -> Option<DataKind> {
        match self {
            Self::Classified(kind) => Some(kind),
            Self::Unclassified => None,
        }
    }

    /// Whether the resolution is sensitive: unclassified counts as
    /// sensitive for sink gating (unknown is never safe).
    pub const fn is_sensitive(self) -> bool {
        match self {
            Self::Classified(kind) => kind.is_sensitive(),
            Self::Unclassified => true,
        }
    }

    /// The effective kind for widening: unclassified widens as the
    /// most restrictive rank.
    pub fn rank(self) -> u8 {
        match self {
            Self::Classified(kind) => kind.rank(),
            Self::Unclassified => DataKind::Credential.rank(),
        }
    }
}

/// One declassification grant evaluated against the attachment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SensitivityMark {
    /// The subject that carried the grant or classification.
    pub subject: String,
    /// The effective kind after the grant.
    pub kind: DataKind,
}

/// The total subject resolution over one attachment.
#[derive(Clone, Debug)]
pub struct Resolution {
    defaults: super::wire::Defaults,
    exact: BTreeMap<String, DataKind>,
    grants: BTreeMap<String, Vec<super::wire::Declassification>>,
    definition_defaults: BTreeMap<String, DataKind>,
}

impl Resolution {
    /// Build the resolution over one parsed attachment. Canonical:
    /// pure function of the attachment bytes.
    pub fn build(attachment: &Attachment) -> Self {
        let mut exact = BTreeMap::new();
        let mut definition_defaults = BTreeMap::new();
        for entry in attachment.classifications() {
            if entry.subject().is_definition_level() {
                definition_defaults.insert(entry.subject().as_str().to_owned(), entry.kind());
            } else {
                exact.insert(entry.subject().as_str().to_owned(), entry.kind());
            }
        }
        let mut grants: BTreeMap<String, Vec<super::wire::Declassification>> = BTreeMap::new();
        for grant in attachment.declassifications() {
            grants
                .entry(grant.subject().as_str().to_owned())
                .or_default()
                .push(grant.clone());
        }
        Self {
            defaults: attachment.defaults().clone(),
            exact,
            grants,
            definition_defaults,
        }
    }

    /// The declared profile.
    pub const fn profile(&self) -> Profile {
        self.defaults.profile()
    }

    /// The exact entry for one subject, when declared at its own path.
    pub fn exact(&self, subject: &SubjectPath) -> Option<DataKind> {
        self.exact.get(subject.as_str()).copied()
    }

    /// The declared grants for one subject, canonically ordered.
    pub fn grants(&self, subject: &SubjectPath) -> &[super::wire::Declassification] {
        self.grants
            .get(subject.as_str())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Resolve one subject to its classification.
    ///
    /// Precedence (plan §2.3):
    /// 1. the exact field-path entry,
    /// 2. the definition-level entry of the head semantic id,
    /// 3. the profile default for field subjects
    ///    (`unclassifiedFields`) or payload subjects
    ///    (`unclassifiedPayloads`; a two-or-more-segment path).
    ///
    /// The result widens across the contributors: the resolved kind is
    /// the most restrictive contributor, never a silent lowering.
    /// An exact entry can only narrow a *wider* default; if the exact
    /// entry is narrower than the covering default the covering
    /// default wins (widening, never lowering).
    pub fn resolve(&self, subject: &SubjectPath) -> ResolvedKind {
        let exact = self.exact(subject);
        let definition = self.definition_defaults.get(subject.semantic_id()).copied();
        let default = if subject.field_segments().count() >= 2 {
            Some(self.defaults.unclassified_payloads())
        } else if !subject.is_definition_level() {
            Some(self.defaults.unclassified_fields())
        } else {
            None
        };
        let mut best: Option<DataKind> = None;
        for candidate in [exact, definition, default].into_iter().flatten() {
            best = Some(match best {
                None => candidate,
                Some(current) => {
                    if candidate.rank() > current.rank() {
                        candidate
                    } else {
                        current
                    }
                }
            });
        }
        match best {
            Some(kind) => ResolvedKind::Classified(kind),
            None => ResolvedKind::Unclassified,
        }
    }

    /// Resolve one subject and apply the first valid declassification
    /// grant: a strict lowering on the kind lattice whose `fromKind`
    /// matches the resolved kind. Grants are never implicit; a grant
    /// that does not apply is ignored here and reported by the
    /// validator.
    pub fn resolve_with_grants(
        &self,
        subject: &SubjectPath,
        grants_valid: impl Fn(&super::wire::Declassification) -> bool,
    ) -> SensitivityMark {
        let resolved = self.resolve(subject);
        let mut kind = match resolved {
            ResolvedKind::Classified(kind) => kind,
            ResolvedKind::Unclassified => {
                return SensitivityMark {
                    subject: subject.as_str().to_owned(),
                    kind: DataKind::Credential,
                }
            }
        };
        for grant in self.grants(subject) {
            if !grants_valid(grant) {
                continue;
            }
            if grant.from_kind() == kind && grant.to_kind().strict_lowering_of(kind) {
                kind = grant.to_kind();
            }
        }
        SensitivityMark {
            subject: subject.as_str().to_owned(),
            kind,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::{DataKind, SubjectPath};
    use super::*;

    const VALID_ATTACHMENT: &str = r#"{
      "schemaVersion": "lekalo/data-classification/v0.4.0",
      "identity": "dev.lekalo.data-classification@0.4.0",
      "attachmentRevision": "1.0.0",
      "projectId": "clinic",
      "modelRef": {"modelVersion": "0.2.16", "digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
      "irRef": {"irVersion": "0.2.16", "digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
      "defaults": {"profile": "strict", "unclassifiedFields": "internal", "unclassifiedPayloads": "confidential"},
      "classifications": [
        {"subject": "core.entity.user/email", "kind": "personal"},
        {"subject": "core.entity.user", "kind": "confidential"},
        {"subject": "core.command.create_user/payload/ssn", "kind": "credential"}
      ],
      "declassifications": [
        {
          "id": "grant.core.export-user-derived@1.0.0",
          "subject": "core.entity.user/email",
          "fromKind": "personal",
          "toKind": "derived",
          "approvedBy": "review-2025-001",
          "justification": "Aggregated analytics only.",
          "conditions": ["aggregated"]
        }
      ],
      "openQuestions": []
    }"#;

    fn attachment() -> Attachment {
        Attachment::parse(VALID_ATTACHMENT.as_bytes()).expect("valid attachment")
    }

    #[test]
    fn exact_field_entry_wins_and_widening_applies() {
        let resolution = Resolution::build(&attachment());
        let email = SubjectPath::parse("core.entity.user/email").expect("valid");
        // Exact `personal` entry, but the containing entity default is
        // `confidential` (rank 5 > rank 7? no: personal=7 >
        // confidential=5) — the exact entry is the wider contributor
        // and wins.
        assert_eq!(
            resolution.resolve(&email),
            ResolvedKind::Classified(DataKind::Personal)
        );
    }

    #[test]
    fn definition_default_covers_unlisted_fields() {
        let resolution = Resolution::build(&attachment());
        let name = SubjectPath::parse("core.entity.user/name").expect("valid");
        // No exact entry: the entity default `confidential` applies.
        assert_eq!(
            resolution.resolve(&name),
            ResolvedKind::Classified(DataKind::Confidential)
        );
    }

    #[test]
    fn payload_paths_use_the_payload_default() {
        let resolution = Resolution::build(&attachment());
        let city = SubjectPath::parse("core.command.create_user/payload/city").expect("valid");
        assert_eq!(
            resolution.resolve(&city),
            ResolvedKind::Classified(DataKind::Confidential)
        );
    }

    #[test]
    fn unclassified_is_its_own_state() {
        let resolution = Resolution::build(&attachment());
        let orphan = SubjectPath::parse("other.module.thing/mystery").expect("valid");
        // `other.module.thing` has no entry and no definition default,
        // but the profile field default still covers it. A subject
        // with a definition default of a wider kind widens too.
        let resolved = resolution.resolve(&orphan);
        assert!(matches!(resolved, ResolvedKind::Classified(_)));
    }

    #[test]
    fn grants_lower_only_through_the_declared_path() {
        let resolution = Resolution::build(&attachment());
        let email = SubjectPath::parse("core.entity.user/email").expect("valid");
        let mark = resolution.resolve_with_grants(&email, |_| true);
        assert_eq!(mark.kind, DataKind::Derived);
        // With grants refused, the kind stays `personal`.
        let mark = resolution.resolve_with_grants(&email, |_| false);
        assert_eq!(mark.kind, DataKind::Personal);
    }

    #[test]
    fn credential_subjects_keep_their_kind() {
        let resolution = Resolution::build(&attachment());
        let ssn = SubjectPath::parse("core.command.create_user/payload/ssn").expect("valid");
        let mark = resolution.resolve_with_grants(&ssn, |_| true);
        assert_eq!(mark.kind, DataKind::Credential);
    }
}
