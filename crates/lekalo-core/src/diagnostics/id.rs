//! Validated diagnostic identity newtypes (issue #11).
//!
//! Every identifier is grammar-checked at construction; the wire types keep
//! private fields so no caller can smuggle an unvalidated string into a
//! diagnostic.

use std::fmt;

use serde::Serialize;

/// A validated dotted rule id such as `loader.json-parse`.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct DiagnosticId(String);

/// A validated immutable `LEK-SUBSYSTEM-NNN` code.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct DiagnosticCode(String);

/// A validated message catalog key; v1 always equals the rule id.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct MessageId(String);

/// A validated registered fix id such as `loader.fix-rename-import`.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct FixId(String);

/// A validated provider namespace: reverse-DNS labels plus exact SemVer.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ProviderNamespace(String);

/// A validated bounded provider original code (ASCII graphic only).
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct OriginalCode(String);

/// Shared grammar of one dotted, lowercase rule segment chain.
pub(crate) fn is_rule_id(text: &str) -> bool {
    let segments: Vec<&str> = text.split('.').collect();
    text.len() <= 128
        && segments.len() >= 2
        && segments.iter().all(|segment| {
            let bytes = segment.as_bytes();
            !segment.is_empty()
                && bytes[0].is_ascii_lowercase()
                && bytes
                    .iter()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        })
}

/// `^LEK-[A-Z][A-Z0-9]{1,11}-[0-9]{3}$`
pub(crate) fn is_diagnostic_code(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() < 10 || bytes.len() > 20 || !text.starts_with("LEK-") {
        return false;
    }
    let rest = &text[4..];
    let Some(dash) = rest.rfind('-') else {
        return false;
    };
    let (subsystem, number) = (&rest[..dash], &rest[dash + 1..]);
    let subsystem = subsystem.as_bytes();
    subsystem.len() >= 2
        && subsystem.len() <= 12
        && subsystem[0].is_ascii_uppercase()
        && subsystem[1..]
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        && number.len() == 3
        && number.bytes().all(|byte| byte.is_ascii_digit())
}

/// Reverse-DNS namespace with exact `major.minor.patch` SemVer tail.
pub(crate) fn is_provider_namespace(text: &str) -> bool {
    let Some((labels, version)) = text.rsplit_once(':') else {
        return false;
    };
    let labels: Vec<&str> = labels.split('.').collect();
    labels.len() >= 2
        && labels.iter().enumerate().all(|(index, label)| {
            let bytes = label.as_bytes();
            !label.is_empty()
                && bytes[0].is_ascii_lowercase()
                && bytes
                    .iter()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
                && !(index == labels.len() - 1 && label.parse::<u64>().is_ok())
        })
        && is_semver(version)
}

/// One canonical `major.minor.patch` spelling without build metadata.
pub(crate) fn is_semver(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 19
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && (part.len() == 1 || !part.starts_with('0'))
        })
}

/// ASCII graphic bytes only (no space, no control, no non-ASCII).
pub(crate) fn is_original_code(text: &str) -> bool {
    text.len() <= 128 && text.bytes().all(|byte| (0x21..=0x7E).contains(&byte))
}

macro_rules! validated_newtype {
    ($name:ident, $check:path, $doc:literal) => {
        impl $name {
            /// Construct from text that must already satisfy the grammar.
            #[cfg_attr(not(test), allow(dead_code))]
            pub(crate) fn new(text: impl Into<String>) -> Option<Self> {
                let text = text.into();
                $check(&text).then_some(Self(text))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

validated_newtype!(DiagnosticId, is_rule_id, "dotted rule id");
validated_newtype!(DiagnosticCode, is_diagnostic_code, "LEK code");
validated_newtype!(MessageId, is_rule_id, "message catalog key");
validated_newtype!(FixId, is_rule_id, "registered fix id");
validated_newtype!(
    ProviderNamespace,
    is_provider_namespace,
    "provider namespace"
);
validated_newtype!(OriginalCode, is_original_code, "provider original code");
