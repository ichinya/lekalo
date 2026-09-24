//! Typed identifiers of the storage-engine attachment (issue #69).
//!
//! Every wire string becomes one closed typed value before it can
//! exist in a profile. The engine id is a closed enum; the version pin
//! is an exact canonical semver of the target server; the connection
//! name is one opaque token whose grammar makes credentials, hosts,
//! URLs, and parameters unrepresentable before anything serializes.

use std::fmt;

/// Why one textual engine-profile record is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShapeError {
    /// The record or one of its members is outside the closed grammar.
    Shape,
}

/// The closed storage-engine vocabulary of v1.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Engine {
    /// The PostgreSQL engine.
    Postgres,
}

impl Engine {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
        }
    }

    /// Parse one wire key.
    pub fn parse(text: &str) -> Result<Self, ShapeError> {
        match text {
            "postgres" => Ok(Self::Postgres),
            _ => Err(ShapeError::Shape),
        }
    }
}

/// The exact canonical version pin of one target server
/// (`16.4`). The pin must be an exact `major.minor.patch` spelling:
/// ranges, prefixes, and loose versions are refused so a profile can
/// never float between engine behaviors.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct VersionPin(String);

impl VersionPin {
    /// Validate and keep the exact pin text.
    pub fn parse(text: &str) -> Result<Self, ShapeError> {
        let parts: Vec<&str> = text.split('.').collect();
        if parts.len() != 3 {
            return Err(ShapeError::Shape);
        }
        let numeric = |part: &str| {
            !part.is_empty()
                && part.len() <= 4
                && part.starts_with(|b: char| b.is_ascii_digit())
                && part.bytes().all(|b| b.is_ascii_digit())
                && (part.len() == 1 || !part.starts_with('0'))
        };
        if parts.iter().all(|part| numeric(part)) {
            Ok(Self(text.to_owned()))
        } else {
            Err(ShapeError::Shape)
        }
    }

    /// The exact validated pin text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The major version number.
    pub fn major(&self) -> u32 {
        self.0
            .split('.')
            .next()
            .and_then(|major| major.parse().ok())
            .unwrap_or(0)
    }
}

impl fmt::Display for VersionPin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One opaque connection name (`ci-readonly`, `test-db`). The token is
/// resolved against a connection registry outside this contract; the
/// grammar refuses every credential-, host-, URL-, and
/// parameter-shaped spelling by construction.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ConnectionName(String);

impl ConnectionName {
    /// Validate and keep the exact connection name.
    pub fn parse(text: &str) -> Result<Self, ShapeError> {
        let bytes = text.as_bytes();
        if bytes.len() < 2 || bytes.len() > super::version::MAX_NAME_BYTES {
            return Err(ShapeError::Shape);
        }
        if !bytes[0].is_ascii_lowercase() {
            return Err(ShapeError::Shape);
        }
        if !bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        {
            return Err(ShapeError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ConnectionName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One bounded schema scope name (`public`).
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ScopeName(String);

impl ScopeName {
    /// Validate and keep the exact scope name.
    pub fn parse(text: &str) -> Result<Self, ShapeError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() || bytes.len() > super::version::MAX_NAME_BYTES {
            return Err(ShapeError::Shape);
        }
        if !bytes[0].is_ascii_lowercase() {
            return Err(ShapeError::Shape);
        }
        if !bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
        {
            return Err(ShapeError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ScopeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One bounded lowercase extension name (`pgcrypto`).
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ExtensionName(String);

impl ExtensionName {
    /// Validate and keep the exact extension name.
    pub fn parse(text: &str) -> Result<Self, ShapeError> {
        let bytes = text.as_bytes();
        if bytes.len() < 2 || bytes.len() > super::version::MAX_NAME_BYTES {
            return Err(ShapeError::Shape);
        }
        if !bytes[0].is_ascii_lowercase() {
            return Err(ShapeError::Shape);
        }
        if !bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
        {
            return Err(ShapeError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ExtensionName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One bounded custom GUC session-variable name (`app.tenant_id`).
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SessionVariable(String);

impl SessionVariable {
    /// Validate and keep the exact variable name.
    pub fn parse(text: &str) -> Result<Self, ShapeError> {
        let bytes = text.as_bytes();
        if bytes.len() < 3 || bytes.len() > super::version::MAX_NAME_BYTES {
            return Err(ShapeError::Shape);
        }
        if !bytes[0].is_ascii_lowercase() {
            return Err(ShapeError::Shape);
        }
        let mut previous_dot = false;
        for byte in &bytes[1..] {
            let ok = byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || *byte == b'_'
                || (*byte == b'.' && !previous_dot);
            if !ok {
                return Err(ShapeError::Shape);
            }
            previous_dot = *byte == b'.';
        }
        if previous_dot {
            return Err(ShapeError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionVariable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_pins_reject_loose_and_padded_spellings() {
        assert!(VersionPin::parse("16.4.0").is_ok());
        assert!(VersionPin::parse("17.2.1").is_ok());
        assert!(VersionPin::parse("16").is_err());
        assert!(VersionPin::parse("16.4").is_err());
        assert!(VersionPin::parse("16.04.0").is_err());
        assert!(VersionPin::parse("^16.0.0").is_err());
        assert!(VersionPin::parse("latest").is_err());
        assert!(VersionPin::parse("").is_err());
    }

    #[test]
    fn connection_names_reject_credential_shaped_text() {
        assert!(ConnectionName::parse("ci-readonly").is_ok());
        assert!(ConnectionName::parse("test-db").is_ok());
        assert!(ConnectionName::parse("postgres://u:p@host/db").is_err());
        assert!(ConnectionName::parse("Host=x").is_err());
        assert!(ConnectionName::parse("user=admin").is_err());
        assert!(ConnectionName::parse("-leading").is_err());
        assert!(ConnectionName::parse("").is_err());
        assert!(ConnectionName::parse(&"a".repeat(64)).is_err());
    }

    #[test]
    fn engine_vocabulary_is_closed() {
        assert_eq!(Engine::parse("postgres"), Ok(Engine::Postgres));
        assert!(Engine::parse("mysql").is_err());
    }

    #[test]
    fn scope_and_extension_names_are_closed_tokens() {
        assert!(ScopeName::parse("public").is_ok());
        assert!(ScopeName::parse("Public").is_err());
        assert!(ScopeName::parse("").is_err());
        assert!(ExtensionName::parse("pgcrypto").is_ok());
        assert!(ExtensionName::parse("postgis").is_ok());
        assert!(ExtensionName::parse("x").is_err());
        assert!(SessionVariable::parse("app.tenant_id").is_ok());
        assert!(SessionVariable::parse("app.").is_err());
        assert!(SessionVariable::parse(".app").is_err());
        assert!(SessionVariable::parse("ab").is_err());
    }
}
