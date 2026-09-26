//! Issue #72: the closed, versioned client-SDK projection family.
//!
//! One deterministic projection joining the validated transport-http
//! attachment (#70), the compiled project IR (#8), the #62 error
//! registry, and the #64 query-model attachment into the typed client
//! contract every language backend (TypeScript, Go, PHP; Rust later)
//! renders from. The projection is the single shared source: multiple
//! language clients derive from one contract, never from three
//! independent interpretations.
//!
//! Boundaries: the projection is pure declaration data — client
//! generation never defines server behavior, never reads server
//! source, never infers safety from the HTTP method, never changes
//! server policy, and never synthesizes a field the contracts do not
//! declare. No base URL, credential, or environment value ever enters
//! the projection; transports are injected at construction time in
//! every generated language.
//!
//! Determinism and denial: canonical bytes are compact UTF-8 JSON with
//! byte-sorted keys, operations sort by effective operation id, types
//! by semantic id, and error variants by error id. Every semantic
//! contradiction rejects with an explicit registered `client.*`
//! diagnostic and no partial result. Retry authorization is
//! conservative by construction: the default is no automatic retry,
//! and a retry is emitted only when the declared #62 contract of the
//! specific error authorizes it.

pub mod diagnostic;
pub mod id;
pub mod impact;
pub mod project;
pub mod retry;
pub mod source;
pub mod types;
pub mod version;

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::SemanticId;

pub use id::{ConsumerId, Language, TargetIdent};
pub use impact::{
    affected_clients, AffectedClient, AffectedKind, ClientArtifactEntry, ClientArtifactIndex,
};
pub use project::project;
pub use retry::{authorize, plan_attempts, retry_permitted, AttemptPlan, RetryAuthorization};
pub use types::{
    BodyMode, ClientAuth, ClientBody, ClientConfig, ClientCorrelation, ClientField,
    ClientIdempotency, ClientOperation, ClientPagination, ClientParam, ClientType, ClientTypeError,
    ErrorVariant, ParamLocation, ParamStyle, ResultShape, ScalarMapping, TypeExpr, TypeKind,
};
pub use version::{
    COMPATIBILITY_IDENTITY, FAMILY, IDENTITY, INDEX_IDENTITY, IR_IDENTITY, MAX_AFFECTED,
    MAX_ATTEMPTS, MAX_CANONICAL_BYTES, MAX_CONSUMERS, MAX_ERRORS, MAX_IDENTIFIER, MAX_OPERATIONS,
    MAX_TYPES, MODEL_VERSION, MODE_CHECKED, MODE_GENERATED, SCHEMA_VERSION, VERSION,
};

pub use diagnostic::{io_failure, rule_set};

/// The bound source Model contract: exact accepted version plus digest.
///
/// The wire spelling mirrors the transport attachment's `modelRef`, so
/// one canonical digest rule covers both families.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelPin {
    /// The accepted Model contract version.
    pub(crate) version: crate::scenario::ModelPin,
    /// The digest of the exact source Model document.
    pub(crate) digest: Sha256Digest,
}

impl ModelPin {
    /// Assemble from validated parts (crate internal).
    pub(crate) fn new(version: crate::scenario::ModelPin, digest: Sha256Digest) -> Self {
        Self { version, digest }
    }

    /// The accepted Model contract version.
    pub const fn version(&self) -> &crate::scenario::ModelPin {
        &self.version
    }

    /// The digest of the exact source Model document.
    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }
}

/// One finished client-SDK projection: immutable, deterministically
/// ordered, and safe to share across threads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientContractWire {
    /// The exact contract family identity (`client-sdk@0.4.0`).
    identity: String,
    /// The exact schema discriminator.
    schema_version: String,
    /// The attachment revision this projection was derived from.
    attachment_revision: SemVer,
    /// The stable project identity.
    project_id: SemanticId,
    /// The bound source Model contract.
    model_ref: ModelPin,
    /// The digest of the exact canonical IR payload.
    ir_digest: Sha256Digest,
    /// The digest of the exact canonical transport attachment payload.
    transport_digest: Sha256Digest,
    /// The wire dialect the operations speak.
    wire_dialect: String,
    /// Every projected operation, ordered by effective operation id.
    operations: Vec<ClientOperation>,
    /// Every projected named type, ordered by semantic id.
    types: Vec<ClientType>,
}

impl ClientContractWire {
    /// Assemble from validated parts (crate internal); collections
    /// arrive in the caller's normalized order.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assemble(
        identity: String,
        schema_version: String,
        attachment_revision: SemVer,
        project_id: SemanticId,
        model_ref: ModelPin,
        ir_digest: Sha256Digest,
        transport_digest: Sha256Digest,
        wire_dialect: String,
        operations: Vec<ClientOperation>,
        types: Vec<ClientType>,
    ) -> Self {
        Self {
            identity,
            schema_version,
            attachment_revision,
            project_id,
            model_ref,
            ir_digest,
            transport_digest,
            wire_dialect,
            operations,
            types,
        }
    }

    /// The exact contract identity.
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// The exact wire discriminator.
    pub fn schema_version(&self) -> &str {
        &self.schema_version
    }

    /// The attachment revision the projection was derived from.
    pub fn attachment_revision(&self) -> &SemVer {
        &self.attachment_revision
    }

    /// The stable project identity.
    pub fn project_id(&self) -> &SemanticId {
        &self.project_id
    }

    /// The bound source Model contract.
    pub const fn model_ref(&self) -> &ModelPin {
        &self.model_ref
    }

    /// The digest of the exact canonical IR payload.
    pub fn ir_digest(&self) -> &Sha256Digest {
        &self.ir_digest
    }

    /// The digest of the exact canonical transport payload.
    pub fn transport_digest(&self) -> &Sha256Digest {
        &self.transport_digest
    }

    /// The wire dialect the operations speak.
    pub fn wire_dialect(&self) -> &str {
        &self.wire_dialect
    }

    /// Every projected operation, ordered by effective operation id.
    pub fn operations(&self) -> &[ClientOperation] {
        &self.operations
    }

    /// Every projected named type, ordered by semantic id.
    pub fn types(&self) -> &[ClientType] {
        &self.types
    }

    /// One operation by its stable effective operation id.
    pub fn operation(&self, id: &str) -> Option<&ClientOperation> {
        self.operations.iter().find(|op| op.operation_id == id)
    }

    /// The canonical payload bytes (compact JSON, byte-sorted keys,
    /// no trailing LF), or a typed refusal beyond the payload bound.
    pub fn canonical_bytes(&self) -> Result<String, DiagnosticSet> {
        project::canonical_bytes(self)
    }

    /// The digest of the canonical payload bytes.
    pub fn digest(&self) -> Result<Sha256Digest, DiagnosticSet> {
        let bytes = self.canonical_bytes()?;
        Ok(Sha256Digest::from_hex(&crate::digest::sha256_hex(
            bytes.as_bytes(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_accessors_round_trip() {
        let wire = ClientContractWire {
            identity: IDENTITY.to_owned(),
            schema_version: SCHEMA_VERSION.to_owned(),
            attachment_revision: SemVer::parse("0.4.0").expect("semver"),
            project_id: SemanticId::parse_root("planner").expect("id"),
            model_ref: ModelPin::new(
                crate::scenario::ModelPin::Current,
                Sha256Digest::from_hex(
                    "0000000000000000000000000000000000000000000000000000000000000000",
                ),
            ),
            ir_digest: Sha256Digest::from_hex(
                "1111111111111111111111111111111111111111111111111111111111111111",
            ),
            transport_digest: Sha256Digest::from_hex(
                "2222222222222222222222222222222222222222222222222222222222222222",
            ),
            wire_dialect: "lekalo-http-wire/v1".to_owned(),
            operations: Vec::new(),
            types: Vec::new(),
        };
        assert_eq!(wire.identity(), IDENTITY);
        assert_eq!(wire.schema_version(), SCHEMA_VERSION);
        assert_eq!(wire.project_id().as_str(), "planner");
        assert_eq!(wire.wire_dialect(), "lekalo-http-wire/v1");
        assert!(wire.operations().is_empty());
        assert!(wire.types().is_empty());
        assert!(wire.operation("plannerEndpointFocusTask").is_none());
        let bytes = wire.canonical_bytes().expect("canonical");
        assert!(!bytes.contains('\n'), "compact form");
        let digest = wire.digest().expect("digest");
        assert_eq!(
            digest.as_str(),
            format!("sha256:{}", crate::digest::sha256_hex(bytes.as_bytes()))
        );
    }
}
