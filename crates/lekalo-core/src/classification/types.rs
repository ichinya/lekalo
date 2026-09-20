//! The closed type vocabulary of the classification family (issue #87).
//!
//! Every member is a closed, bounded, copyable token: the ten data
//! kinds with their lattice rank, the subject path grammar, the two
//! secure-default profiles, the declassification condition tokens, and
//! the bounded review reference. Nothing here carries a value: the
//! whole module is metadata-only by construction.

use serde::Serialize;

use super::version;

/// Why one textual member is not a valid closed-vocabulary value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VocabularyError {
    /// The text is empty or longer than the closed bound.
    Length,
    /// The text violates the closed spelling.
    Shape,
}

/// The closed data-kind vocabulary with its lattice rank (§2.3 of the
/// plan): higher rank is more restrictive; propagation widens to the
/// highest contributing rank and never silently lowers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DataKind {
    /// `public` — publishable everywhere; rank 0.
    Public,
    /// `internal` — project-internal use; rank 1.
    Internal,
    /// `derived` — derived or synthetic data; rank 2.
    Derived,
    /// `retention-limited` — bounded retention applies; rank 3.
    RetentionLimited,
    /// `tenant-scoped` — tenant-bound data; rank 4.
    TenantScoped,
    /// `confidential` — confidential business data; rank 5.
    Confidential,
    /// `financial` — financial data; rank 6.
    Financial,
    /// `personal` — personally identifiable information; rank 7.
    Personal,
    /// `health` — special-category health data; rank 8.
    Health,
    /// `credential` — a secret; rank 9, never declassifiable downward.
    Credential,
}

impl DataKind {
    /// Every kind in ascending rank order (the closed enumeration).
    pub const ALL: [DataKind; 10] = [
        DataKind::Public,
        DataKind::Internal,
        DataKind::Derived,
        DataKind::RetentionLimited,
        DataKind::TenantScoped,
        DataKind::Confidential,
        DataKind::Financial,
        DataKind::Personal,
        DataKind::Health,
        DataKind::Credential,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "public" => Self::Public,
            "internal" => Self::Internal,
            "derived" => Self::Derived,
            "retention-limited" => Self::RetentionLimited,
            "tenant-scoped" => Self::TenantScoped,
            "confidential" => Self::Confidential,
            "financial" => Self::Financial,
            "personal" => Self::Personal,
            "health" => Self::Health,
            "credential" => Self::Credential,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Derived => "derived",
            Self::RetentionLimited => "retention-limited",
            Self::TenantScoped => "tenant-scoped",
            Self::Confidential => "confidential",
            Self::Financial => "financial",
            Self::Personal => "personal",
            Self::Health => "health",
            Self::Credential => "credential",
        }
    }

    /// The lattice rank; higher is more restrictive.
    pub const fn rank(self) -> u8 {
        match self {
            Self::Public => 0,
            Self::Internal => 1,
            Self::Derived => 2,
            Self::RetentionLimited => 3,
            Self::TenantScoped => 4,
            Self::Confidential => 5,
            Self::Financial => 6,
            Self::Personal => 7,
            Self::Health => 8,
            Self::Credential => 9,
        }
    }

    /// Whether the kind is sensitive for the extended-effects gate:
    /// everything strictly above `internal` (§2.4 of the plan).
    pub const fn is_sensitive(self) -> bool {
        self.rank() > Self::Internal.rank()
    }

    /// Whether flows of this kind may cross tenant boundaries by
    /// default (the plan's `crossTenant` default rule).
    pub const fn cross_tenant_forbidden_by_default(self) -> bool {
        matches!(
            self,
            Self::Personal | Self::Credential | Self::Financial | Self::Health | Self::TenantScoped
        )
    }

    /// Whether the kind can ever lower itself by declassification.
    /// Only `credential` is sealed: secrets never declassify downward.
    pub const fn declassifiable(self) -> bool {
        !matches!(self, Self::Credential)
    }

    /// Whether `self` is a strict lowering of `other` on the lattice.
    pub const fn strict_lowering_of(self, other: Self) -> bool {
        self.rank() < other.rank()
    }
}

impl std::fmt::Display for DataKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for DataKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// The secure-defaults profile of one attachment.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Profile {
    /// `default` — findings warn; unclassified subjects on sensitive
    /// sinks are a warning, not a rejection.
    #[default]
    Default,
    /// `strict` — unclassified subjects on sensitive sinks reject and
    /// every error-severity finding invalidates the project.
    Strict,
}

impl Profile {
    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "default" => Self::Default,
            "strict" => Self::Strict,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Strict => "strict",
        }
    }
}

impl Serialize for Profile {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// One classification subject: a semantic id alone (definition level)
/// or a bounded `/`-separated field path under it (field or payload
/// level). The exact byte text is preserved and canonical comparison
/// is byte order.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SubjectPath(String);

impl SubjectPath {
    /// Validate and keep the exact text of one subject path.
    ///
    /// One to three `/`-separated segments; the first segment is the
    /// inherited semantic-id grammar (`module.name` or
    /// `module.kind-namespace.name`), every later segment is one
    /// lowercase snake field token.
    pub fn parse(text: &str) -> Result<Self, VocabularyError> {
        if text.is_empty() || text.len() > 324 {
            return Err(VocabularyError::Length);
        }
        let mut segments = text.split('/');
        let head = segments.next().ok_or(VocabularyError::Shape)?;
        // The head is always a dotted semantic id; a single undotted
        // segment is not a project root in the subject grammar.
        if !head.contains('.') {
            return Err(VocabularyError::Shape);
        }
        crate::scenario::id::SemanticId::parse(head).map_err(|_| VocabularyError::Shape)?;
        let mut depth = 1usize;
        for segment in segments {
            depth += 1;
            if depth > version::MAX_SUBJECT_SEGMENTS {
                return Err(VocabularyError::Shape);
            }
            #[allow(clippy::incompatible_msrv)]
            let valid = field_token(segment);
            if !valid {
                return Err(VocabularyError::Shape);
            }
        }
        Ok(Self(text.to_owned()))
    }

    /// The semantic id of the addressed definition (the head segment).
    pub fn semantic_id(&self) -> &str {
        self.0.split('/').next().unwrap_or(&self.0)
    }

    /// The field path segments under the semantic id, in order.
    pub fn field_segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('/').skip(1)
    }

    /// Whether the subject addresses a definition wholesale.
    pub fn is_definition_level(&self) -> bool {
        !self.0.contains('/')
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SubjectPath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One closed lowercase snake field token of a subject path.
fn field_token(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
}

/// One closed declassification condition token.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Condition {
    /// `aggregated` — only aggregate statistics leave.
    Aggregated,
    /// `anonymized` — irreversibly anonymized.
    Anonymized,
    /// `consent-obtained` — explicit consent recorded.
    ConsentObtained,
    /// `pseudonymized` — reversible pseudonymization applied.
    Pseudonymized,
    /// `suppressed` — the value is suppressed at the sink.
    Suppressed,
}

impl Condition {
    /// Every condition in canonical byte order.
    pub const ALL: [Condition; 5] = [
        Condition::Aggregated,
        Condition::Anonymized,
        Condition::ConsentObtained,
        Condition::Pseudonymized,
        Condition::Suppressed,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "aggregated" => Self::Aggregated,
            "anonymized" => Self::Anonymized,
            "consent-obtained" => Self::ConsentObtained,
            "pseudonymized" => Self::Pseudonymized,
            "suppressed" => Self::Suppressed,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Aggregated => "aggregated",
            Self::Anonymized => "anonymized",
            Self::ConsentObtained => "consent-obtained",
            Self::Pseudonymized => "pseudonymized",
            Self::Suppressed => "suppressed",
        }
    }
}

impl Serialize for Condition {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// One opaque review reference of the extended-effects security-gate
/// shape: it references a review; it never embeds identity claims,
/// actor names, or credentials.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ReviewRef(String);

impl ReviewRef {
    /// Validate and keep the exact text of one review reference.
    pub fn parse(text: &str) -> Result<Self, VocabularyError> {
        let bytes = text.as_bytes();
        if bytes.len() < 8 || bytes.len() > 128 {
            return Err(VocabularyError::Length);
        }
        let first_ok = bytes[0].is_ascii_alphanumeric();
        let rest_ok = bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'/' | b'-'));
        if !first_ok || !rest_ok {
            return Err(VocabularyError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ReviewRef {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One exact namespaced contract id with a SemVer tail (the
/// `is_provider_contract` shape): an NFR constraint family id, a
/// privacy-policy family id, a masking-policy document id, or the id
/// of a declassification grant.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ContractRef(String);

impl ContractRef {
    /// Validate and keep the exact text of one contract reference.
    pub fn parse(text: &str) -> Result<Self, VocabularyError> {
        let Some((namespace, tail)) = text.rsplit_once('@') else {
            return Err(VocabularyError::Shape);
        };
        let ok_namespace = !namespace.is_empty()
            && namespace.len() <= 128
            && namespace.split('.').all(super::super::effects::identity::is_lower_name);
        if !ok_namespace {
            return Err(VocabularyError::Shape);
        }
        if semver::Version::parse(tail).is_err() {
            return Err(VocabularyError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ContractRef {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One exact policy document id inside the privacy/export-policy
/// family of issue #120.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct PolicyRef(String);

impl PolicyRef {
    /// Validate and keep the exact text of one policy reference.
    pub fn parse(text: &str) -> Result<Self, VocabularyError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() || bytes.len() > 192 {
            return Err(VocabularyError::Length);
        }
        let first_ok = bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit();
        let rest_ok = bytes[1..].iter().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'/' | b'-' | b'#')
        });
        if !first_ok || !rest_ok {
            return Err(VocabularyError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PolicyRef {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One bounded retention class token (`^[a-z][a-z0-9_-]*$`).
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct RetentionClass(String);

impl RetentionClass {
    /// Validate and keep the exact text of one retention class.
    pub fn parse(text: &str) -> Result<Self, VocabularyError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() || bytes.len() > 64 || !bytes[0].is_ascii_lowercase() {
            return Err(if bytes.is_empty() || bytes.len() > 64 {
                VocabularyError::Length
            } else {
                VocabularyError::Shape
            });
        }
        if !bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-'))
        {
            return Err(VocabularyError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One bounded set-like label token (`^[a-zA-Z0-9][a-zA-Z0-9._:/-]*$`,
/// the bounded-token grammar).
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Label(String);

impl Label {
    /// Validate and keep the exact text of one label.
    pub fn parse(text: &str) -> Result<Self, VocabularyError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() || bytes.len() > 64 {
            return Err(VocabularyError::Length);
        }
        if !bytes[0].is_ascii_alphanumeric()
            || !bytes[1..]
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'/' | b'-'))
        {
            return Err(VocabularyError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One bounded open-question id (`^[a-zA-Z0-9][a-zA-Z0-9._:/-]*$`).
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct QuestionId(String);

impl QuestionId {
    /// Validate and keep the exact text of one open-question id.
    pub fn parse(text: &str) -> Result<Self, VocabularyError> {
        Label::parse(text).map(|label| Self(label.as_str().to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[allow(clippy::similar_names)]
/// One bounded declaration text (`description`-class prose):
/// 1-256 bytes of the closed printable subset, never source text,
/// paths, credentials, URLs, runtime values, or provider output.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct BoundedText(String);

impl BoundedText {
    /// Validate and keep the exact text.
    pub fn parse(text: &str) -> Result<Self, VocabularyError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() || bytes.len() > 256 {
            return Err(VocabularyError::Length);
        }
        let first_ok = bytes[0].is_ascii_alphanumeric();
        let rest_ok = bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b' ' | b'.' | b',' | b':' | b';' | b'(' | b')' | b'/' | b'_' | b'-'));
        if !first_ok || !rest_ok {
            return Err(VocabularyError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One exact ISO-8601 UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`).
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct IsoTimestamp(String);

impl IsoTimestamp {
    /// Validate and keep the exact text of one timestamp.
    pub fn parse(text: &str) -> Result<Self, VocabularyError> {
        let bytes = text.as_bytes();
        if bytes.len() != 20 || bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' || bytes[13] != b':' || bytes[16] != b':' || bytes[19] != b'Z' {
            return Err(VocabularyError::Shape);
        }
        if !bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 | 10 | 13 | 16 | 19 => true,
            _ => byte.is_ascii_digit(),
        }) {
            return Err(VocabularyError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text (lexicographic order is time order).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for IsoTimestamp {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_parse_the_closed_vocabulary_and_rank_totally() {
        for kind in DataKind::ALL {
            assert_eq!(DataKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(DataKind::parse("secret"), None);
        let mut kinds = DataKind::ALL;
        kinds.sort_by_key(|kind| kind.rank());
        assert_eq!(kinds, DataKind::ALL);
        assert!(DataKind::Credential.rank() > DataKind::Public.rank());
    }

    #[test]
    fn sensitive_and_default_rules_follow_the_plan() {
        assert!(!DataKind::Public.is_sensitive());
        assert!(!DataKind::Internal.is_sensitive());
        assert!(DataKind::Derived.is_sensitive());
        assert!(DataKind::Credential.is_sensitive());
        for kind in [DataKind::Personal, DataKind::Credential, DataKind::Financial, DataKind::Health, DataKind::TenantScoped] {
            assert!(kind.cross_tenant_forbidden_by_default());
        }
        assert!(!DataKind::Public.cross_tenant_forbidden_by_default());
        assert!(!DataKind::Credential.declassifiable());
        assert!(DataKind::Personal.declassifiable());
        assert!(DataKind::Derived.strict_lowering_of(DataKind::Personal));
        assert!(!DataKind::Public.strict_lowering_of(DataKind::Public));
    }

    #[test]
    fn subject_paths_accept_definition_field_and_payload_levels() {
        assert!(SubjectPath::parse("planner.user_task_planning").is_ok());
        assert!(SubjectPath::parse("core.entity.user/email").is_ok());
        assert!(SubjectPath::parse("core.command.create_user/payload/ssn").is_ok());
        assert!(SubjectPath::parse("core.command.create_user/payload/deep").is_ok());
        // Four segments exceed the closed bound.
        assert!(SubjectPath::parse("core.command.create_user/payload/x/y").is_err());
        assert!(SubjectPath::parse("").is_err());
        assert!(SubjectPath::parse("core.entity.user/Email").is_err());
        // A single undotted segment is not a semantic id.
        assert!(SubjectPath::parse("onlyonesegment").is_err());
        // A single-segment dotted semantic id is a definition subject.
        assert!(SubjectPath::parse("planner.user_task_planning").is_ok());
    }

    #[test]
    fn subject_paths_expose_head_and_field_segments() {
        let subject = SubjectPath::parse("core.command.create_user/payload/ssn").expect("valid");
        assert_eq!(subject.semantic_id(), "core.command.create_user");
        assert_eq!(subject.field_segments().collect::<Vec<_>>(), ["payload", "ssn"]);
        assert!(!subject.is_definition_level());
        let head = SubjectPath::parse("core.entity.user").expect("valid");
        assert!(head.is_definition_level());
        assert_eq!(head.field_segments().count(), 0);
    }

    #[test]
    fn conditions_parse_the_closed_vocabulary() {
        for condition in Condition::ALL {
            assert_eq!(Condition::parse(condition.as_str()), Some(condition));
        }
        assert_eq!(Condition::parse("deleted"), None);
    }

    #[test]
    fn review_refs_reject_short_and_hostile_text() {
        assert!(ReviewRef::parse("review-2024-09-001").is_ok());
        assert!(ReviewRef::parse("short").is_err());
        assert!(ReviewRef::parse("has space").is_err());
        assert!(ReviewRef::parse("").is_err());
    }

    #[test]
    fn contract_refs_demand_the_namespaced_semver_shape() {
        assert!(ContractRef::parse("dev.lekalo.nfr@0.4.0").is_ok());
        assert!(ContractRef::parse("grant.data-classification.export-user-derived@1.0.0").is_ok());
        assert!(ContractRef::parse("dev.lekalo.nfr").is_err());
        assert!(ContractRef::parse("dev.lekalo.nfr@not-a-version").is_err());
        assert!(ContractRef::parse("@0.4.0").is_err());
    }

    #[test]
    fn policy_refs_and_labels_follow_the_bounded_token_grammar() {
        assert!(PolicyRef::parse("privacy-policy.masking.personal#1").is_ok());
        assert!(PolicyRef::parse("-leading").is_err());
        assert!(Label::parse("gdpr.art-9").is_ok());
        assert!(Label::parse("_leading").is_err());
        assert!(QuestionId::parse("q1").is_ok());
    }

    #[test]
    fn timestamps_parse_only_the_exact_utc_shape() {
        assert!(IsoTimestamp::parse("2025-01-01T00:00:00Z").is_ok());
        assert!(IsoTimestamp::parse("2025-01-01T00:00:00z").is_err());
        assert!(IsoTimestamp::parse("2025-01-01 00:00:00").is_err());
        assert!(IsoTimestamp::parse("2025-01-01T00:00:00").is_err());
    }

    #[test]
    fn timestamps_order_lexicographically_as_time() {
        let early = IsoTimestamp::parse("2025-01-01T00:00:00Z").expect("valid");
        let late = IsoTimestamp::parse("2026-01-01T00:00:00Z").expect("valid");
        assert!(early < late);
    }

    #[test]
    fn bounded_text_rejects_source_shape() {
        assert!(BoundedText::parse("Approved aggregate-only export.").is_ok());
        assert!(BoundedText::parse("no").is_ok());
        assert!(BoundedText::parse("").is_err());
        assert!(BoundedText::parse("tabs\tinside").is_err());
        assert!(BoundedText::parse("quotes \"inside\"").is_err());
    }

    #[test]
    fn retention_classes_accept_kebab_tokens() {
        assert!(RetentionClass::parse("policy-30d").is_ok());
        assert!(RetentionClass::parse("Policy-30d").is_err());
        assert!(RetentionClass::parse("").is_err());
    }
}
