//! Typed semantic subjects and project references (issue #18).
//!
//! A subject is one stable semantic identity: the closed definition family,
//! the stable semantic ID (#6), and one optional typed member key. Subjects
//! carry no physical path, no span, and no display name; two compilations of
//! equivalent models produce identical subjects regardless of formatting,
//! document order, or module-directory layout.

use crate::ir::DefinitionKind;
use std::fmt;

/// The closed subject family: the 12 definition kinds plus `project` and
/// `module`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum SubjectFamily {
    Project,
    Module,
    Scalar,
    Enum,
    ValueObject,
    Entity,
    Command,
    Query,
    Policy,
    Event,
    Effect,
    Endpoint,
    Scenario,
    TargetBinding,
}

impl SubjectFamily {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Module => "module",
            Self::Scalar => "scalar",
            Self::Enum => "enum",
            Self::ValueObject => "value-object",
            Self::Entity => "entity",
            Self::Command => "command",
            Self::Query => "query",
            Self::Policy => "policy",
            Self::Event => "event",
            Self::Effect => "effect",
            Self::Endpoint => "endpoint",
            Self::Scenario => "scenario",
            Self::TargetBinding => "target-binding",
        }
    }

    /// The family of one definition kind.
    pub(crate) const fn of_kind(kind: DefinitionKind) -> Self {
        match kind {
            DefinitionKind::Scalar => Self::Scalar,
            DefinitionKind::Enum => Self::Enum,
            DefinitionKind::ValueObject => Self::ValueObject,
            DefinitionKind::Entity => Self::Entity,
            DefinitionKind::Command => Self::Command,
            DefinitionKind::Query => Self::Query,
            DefinitionKind::Policy => Self::Policy,
            DefinitionKind::Event => Self::Event,
            DefinitionKind::Effect => Self::Effect,
            DefinitionKind::Endpoint => Self::Endpoint,
            DefinitionKind::Scenario => Self::Scenario,
            DefinitionKind::TargetBinding => Self::TargetBinding,
        }
    }
}

/// One stable semantic subject: family, stable semantic ID, and one
/// optional typed member key (`field.total`, `identity`, `returns`, ...).
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct Subject {
    family: SubjectFamily,
    id: String,
    member: Option<String>,
}

impl Subject {
    /// One whole-definition subject.
    pub(crate) fn new(family: SubjectFamily, id: impl Into<String>) -> Self {
        Self {
            family,
            id: id.into(),
            member: None,
        }
    }

    /// One member subject of a definition (`field.x`, `input.y`,
    /// `identity`, `returns`, ...).
    pub(crate) fn with_member(
        family: SubjectFamily,
        id: impl Into<String>,
        member: impl Into<String>,
    ) -> Self {
        Self {
            family,
            id: id.into(),
            member: Some(member.into()),
        }
    }

    /// The subject family.
    pub const fn family(&self) -> SubjectFamily {
        self.family
    }

    /// The stable semantic ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The typed member key, when the change is member-scoped.
    pub fn member(&self) -> Option<&str> {
        self.member.as_deref()
    }

    /// The stable internal identity bytes used for ordering, digests, and
    /// change identity: `family<US>id<US>member`.
    pub(crate) fn identity(&self) -> String {
        format!(
            "{}\u{1f}{}\u{1f}{}",
            self.family.key(),
            self.id,
            self.member.as_deref().unwrap_or("")
        )
    }
}

impl fmt::Display for Subject {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.member.as_deref() {
            Some(member) => write!(formatter, "{}:{}#{}", self.family.key(), self.id, member),
            None => write!(formatter, "{}:{}", self.family.key(), self.id),
        }
    }
}

/// The typed reference of one compared compilation: the exact source Model
/// version, the IR contract identity, the canonical IR digest, and the
/// digest of the semantic projection (descriptions and registry excluded).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRef {
    pub(crate) model_version: String,
    pub(crate) ir_identity: String,
    pub(crate) ir_digest: String,
    pub(crate) semantic_digest: String,
}

impl ProjectRef {
    /// The exact source Model version (`0.1.0` or `1.0.0`).
    pub fn model_version(&self) -> &str {
        &self.model_version
    }

    /// The IR contract identity (`dev.lekalo.ir@0.1.0`).
    pub fn ir_identity(&self) -> &str {
        &self.ir_identity
    }

    /// The `sha256` digest of the canonical IR bytes.
    pub fn ir_digest(&self) -> &str {
        &self.ir_digest
    }

    /// The `sha256` digest of the semantic projection bytes.
    pub fn semantic_digest(&self) -> &str {
        &self.semantic_digest
    }
}
