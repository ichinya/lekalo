//! The target adapter process protocol (issue #27).
//!
//! Target adapters are separate executables in any language — never Rust
//! dynamic plugins and never internal core dependencies. This module owns
//! the client side of the published `lekalo.target/v1` process protocol:
//!
//! - the closed wire envelopes ([`wire`]) and their schema artifact
//!   `contracts/target-protocol.schema.v1.0.0.json`;
//! - the direct, shell-free process transport with deadline, cancellation,
//!   and output-size limits ([`transport`]);
//! - scope grammar, protected canonical homes, and coverage checks
//!   ([`scopes`]);
//! - declared output plans verified against the observed project state,
//!   with mandatory dry-run planning before any apply ([`plan`]);
//! - the typed failure taxonomy mapped onto the registered `target.*`
//!   diagnostics ([`TargetFailure`], [`diagnostic`]).
//!
//! The only coupling to an adapter is this process protocol: core never
//! loads a library, ABI, or plugin, and a protocol mismatch is refused as
//! `unsupported` before any generation can start.

pub mod diagnostic;
pub mod plan;
pub mod scopes;
pub mod transport;
pub mod version;
pub mod wire;

use std::sync::atomic::AtomicBool;

use crate::project_fs::Fs;
use transport::AdapterCommand;
use wire::{Operation, RequestEnvelope, ResponseEnvelope, ResponseInvalidity, ResponseStatus};

/// The negotiated, verified result of one `describe` handshake.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DescribeOutcome {
    /// The capabilities the adapter declared (identity, versions,
    /// operations, transports, scopes).
    pub capabilities: wire::Capabilities,
    /// The digest over the canonical capability bytes: the evidence anchor
    /// every later operation binds to.
    pub capability_digest: String,
}

/// One pending planned exchange, bound by a successful dry run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanBinding {
    /// The planning operation that produced the plan (`generate` dry-run
    /// or `plan-clean`).
    pub operation: Operation,
    /// The deterministic plan identifier.
    pub plan_id: String,
    /// The exact declared entries, in canonical order.
    pub entries: Vec<wire::WriteEntry>,
}

/// The caller's request for one adapter operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallRequest<'a> {
    pub operation: Operation,
    pub target: Option<&'a str>,
    pub profile: Option<&'a str>,
    pub ir_path: Option<&'a str>,
    /// Required (as a boolean) for `generate`, refused elsewhere.
    pub dry_run: Option<bool>,
    /// Required for apply operations; must equal the bound plan identity.
    pub plan_id: Option<&'a str>,
}

/// One completed, verified adapter operation.
#[derive(Clone, Debug, PartialEq)]
pub struct CallOutcome {
    /// The validated response envelope.
    pub response: ResponseEnvelope,
    /// The plan identifier the exchange bound (planning and apply).
    pub plan_id: Option<String>,
}

/// The closed failure taxonomy of the target protocol client.
///
/// Every variant maps onto one registered rule via [`diagnostic`]; the
/// status — and therefore the exit class — is owned by the mapping, never
/// by severity or category.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TargetFailure {
    /// The version registry publishes no protocol; nothing can run.
    ProtocolUnpublished,
    /// The embedded registry itself is invalid (developer fault).
    RegistryInvalid,
    /// The request could not be assembled.
    RequestInvalid { detail: &'static str },
    /// An operation ran without a successful handshake for this adapter.
    HandshakeRequired { operation: Operation },
    /// The adapter speaks a different protocol line or version.
    ProtocolMismatch { detail: wire::ProtocolMismatch },
    /// The adapter does not offer the requested operation/target/profile.
    CapabilityUnsupported { detail: &'static str },
    /// A declared scope violates grammar, protection, or verifiability.
    ScopeViolation {
        path: Option<String>,
        detail: &'static str,
    },
    /// A plan path touches a canonical home adapters may never write.
    ProtectedPath { path: String, home: &'static str },
    /// A dry-run exchange changed the project.
    DryRunMutation { path: Option<String> },
    /// Observed project state disagrees with the declared plan.
    PlanMismatch {
        path: Option<String>,
        detail: &'static str,
    },
    /// The adapter reported an operation error in a well-formed envelope.
    OperationFailed {
        class: wire::ErrorClass,
        code: String,
        partial: bool,
    },
    /// The response is malformed (unparseable or semantically invalid).
    ResponseInvalid { detail: ResponseInvalidity },
    /// The process could not be started or the request not delivered.
    TransportFailed { detail: &'static str },
    /// The deadline elapsed; the child was killed.
    Timeout,
    /// The child exited abnormally without a usable error envelope.
    Crash { detail: String },
    /// A response stream exceeded its cap.
    OutputLimit { stream: transport::Stream },
    /// The caller cancelled the exchange.
    Cancelled,
}

impl TargetFailure {
    pub(crate) fn plan_mismatch(path: Option<String>, detail: &'static str) -> Self {
        Self::PlanMismatch { path, detail }
    }

    pub(crate) fn protected_path(path: &str, home: &'static str) -> Self {
        Self::ProtectedPath {
            path: path.to_owned(),
            home,
        }
    }

    pub(crate) fn scope_violation(path: Option<String>, detail: &'static str) -> Self {
        Self::ScopeViolation { path, detail }
    }

    /// A non-zero exit with an unparseable or malformed envelope is a
    /// crash; a zero exit with one is a malformed response.
    fn crash_or_invalid(exit_code: i32, invalid: ResponseInvalidity) -> Self {
        if exit_code != 0 {
            Self::Crash {
                detail: format!("exit-{exit_code}"),
            }
        } else {
            Self::ResponseInvalid { detail: invalid }
        }
    }

    /// The registered rule id and status this failure projects.
    pub fn rule(&self) -> (&'static str, crate::result::Status) {
        diagnostic::rule_for(self)
    }
}

/// The target protocol client: one instance drives one adapter executable.
///
/// The handshake is explicit and mandatory before any operation:
/// [`Self::describe`] negotiates the exact protocol version, validates the
/// declared scopes, and caches the capability verdict. Apply operations
/// additionally require a prior successful planning exchange whose plan
/// identifier the caller echoes.
pub struct TargetClient {
    limits: transport::TransportLimits,
    described: Option<(AdapterCommand, DescribeOutcome)>,
    binding: Option<PlanBinding>,
}

impl Default for TargetClient {
    fn default() -> Self {
        Self::new(transport::TransportLimits::default())
    }
}

impl TargetClient {
    /// Build one client with explicit transport limits.
    pub fn new(limits: transport::TransportLimits) -> Self {
        Self {
            limits,
            described: None,
            binding: None,
        }
    }

    /// The cached handshake outcome, if any.
    pub fn describe_outcome(&self) -> Option<&DescribeOutcome> {
        self.described.as_ref().map(|(_, outcome)| outcome)
    }

    /// The pending plan binding, if any.
    pub fn plan_binding(&self) -> Option<&PlanBinding> {
        self.binding.as_ref()
    }

    /// Run the `describe` handshake: negotiate the exact protocol version,
    /// validate the declared scopes, and cache the capability verdict.
    /// The handshake always travels over stdin; the file transport applies
    /// only to later operations when the adapter declared no stdin support.
    pub fn describe(
        &mut self,
        command: &AdapterCommand,
        cwd: &std::path::Path,
    ) -> Result<&DescribeOutcome, TargetFailure> {
        self.describe_with_cancel(command, cwd, None)
    }

    /// [`Self::describe`] with an explicit caller cancel flag.
    pub fn describe_with_cancel(
        &mut self,
        command: &AdapterCommand,
        cwd: &std::path::Path,
        cancel: Option<&AtomicBool>,
    ) -> Result<&DescribeOutcome, TargetFailure> {
        let protocol_version = published_version()?;
        let request_id = wire::request_id(&base_envelope(
            Operation::Describe,
            protocol_version.clone(),
            self.wire_limits(),
        ));
        let mut envelope = base_envelope(Operation::Describe, protocol_version, self.wire_limits());
        envelope.request_id = request_id;
        let serialized = serialize(&envelope)?;
        let exchange = transport::run(command, &serialized, &self.limits, cwd, false, cancel);
        let response = self.interpret(exchange, &envelope)?;
        let invalid = |detail| Err(TargetFailure::ResponseInvalid { detail });
        if response.writes.is_some() {
            return invalid(ResponseInvalidity::UnexpectedMember);
        }
        let Some(capabilities) = response.capabilities.clone() else {
            return invalid(ResponseInvalidity::Shape);
        };
        if response.evidence.adapter != capabilities.adapter {
            return invalid(ResponseInvalidity::Evidence);
        }
        if !capabilities.operations.contains(&Operation::Describe) {
            return Err(TargetFailure::CapabilityUnsupported { detail: "describe" });
        }
        if !capabilities
            .protocol_versions
            .iter()
            .any(|v| v == version::VERSION)
        {
            return Err(TargetFailure::ProtocolMismatch {
                detail: wire::ProtocolMismatch::Negotiation,
            });
        }
        if capabilities.transports.is_empty() {
            return invalid(ResponseInvalidity::Shape);
        }
        self.validate_scopes(&capabilities.read_scopes, true)?;
        self.validate_scopes(&capabilities.write_scopes, false)?;
        if !scopes::is_token(&capabilities.adapter.id)
            || !wire::is_sha256_digest(&capabilities.adapter.digest)
        {
            return invalid(ResponseInvalidity::Evidence);
        }
        let outcome = DescribeOutcome {
            capability_digest: format!(
                "sha256:{}",
                plan::sha256_hex(&canonical_bytes(&capabilities))
            ),
            capabilities,
        };
        self.described = Some((command.clone(), outcome));
        Ok(&self.described.as_ref().expect("just stored").1)
    }

    /// Run one adapter operation end to end: request validation against the
    /// cached capabilities, bounded transport, envelope interpretation, and
    /// — for write-carrying operations — plan verification against the
    /// observed project state.
    pub fn call(
        &mut self,
        command: &AdapterCommand,
        request: CallRequest<'_>,
        cwd: &std::path::Path,
        fs: &Fs,
        cancel: Option<&AtomicBool>,
    ) -> Result<CallOutcome, TargetFailure> {
        let Some((described_command, described)) = self.described.as_ref() else {
            return Err(TargetFailure::HandshakeRequired {
                operation: request.operation,
            });
        };
        if described_command != command {
            return Err(TargetFailure::HandshakeRequired {
                operation: request.operation,
            });
        }
        let capabilities = described.capabilities.clone();
        self.validate_call_request(&request, &capabilities)?;
        let protocol_version = published_version()?;
        let dry_run = match request.operation {
            Operation::Generate => request.dry_run,
            _ => None,
        };
        let plan_request = if request.operation.is_destructive_apply()
            || (request.operation == Operation::Generate && request.dry_run == Some(false))
        {
            request.plan_id.map(str::to_owned)
        } else {
            None
        };
        let mut envelope = base_envelope(request.operation, protocol_version, self.wire_limits());
        envelope.ir_path = request.ir_path.map(str::to_owned);
        envelope.target = request.target.map(str::to_owned);
        envelope.profile = request.profile.map(str::to_owned);
        envelope.dry_run = dry_run;
        envelope.plan_id = plan_request;
        envelope.request_id = wire::request_id(&envelope);
        let serialized = serialize(&envelope)?;
        let use_file = !capabilities.transports.contains(&wire::Transport::Stdin);

        // Write-carrying exchanges snapshot their scopes first: the apply is
        // verified against the before state, and a dry run must not change
        // anything at all.
        let before = if request.operation.declares_writes() {
            Some(snapshot_scopes(fs, &capabilities.write_scopes)?)
        } else {
            None
        };

        let exchange = transport::run(command, &serialized, &self.limits, cwd, use_file, cancel);
        let response = self.interpret(exchange, &envelope)?;
        self.validate_response_payload(&request, &response, &capabilities)?;
        let writes = response.writes.clone().unwrap_or_default();

        let mut outcome_plan_id = None;
        if request.operation.declares_writes() {
            let after = snapshot_scopes(fs, &capabilities.write_scopes)?;
            let is_planning = request.operation == Operation::PlanClean
                || (request.operation == Operation::Generate && request.dry_run == Some(true));
            if is_planning {
                if let Some(path) = plan::changed_paths(&before.expect("snapshotted"), &after)
                    .into_iter()
                    .next()
                {
                    return Err(TargetFailure::DryRunMutation { path: Some(path) });
                }
                let identity = wire::plan_id(&writes);
                self.binding = Some(PlanBinding {
                    operation: request.operation,
                    plan_id: identity.clone(),
                    entries: writes,
                });
                outcome_plan_id = Some(identity);
            } else {
                let Some(binding) = self.binding.take() else {
                    return Err(TargetFailure::RequestInvalid { detail: "plan-id" });
                };
                if binding.plan_id != envelope.plan_id.as_deref().unwrap_or("") {
                    return Err(TargetFailure::plan_mismatch(None, "plan-id"));
                }
                if response
                    .evidence
                    .plan_id
                    .as_deref()
                    .is_some_and(|id| id != binding.plan_id)
                {
                    return Err(TargetFailure::plan_mismatch(None, "plan-id"));
                }
                if !plan::plans_equal(&binding.entries, &writes) {
                    return Err(TargetFailure::plan_mismatch(None, "plan-drift"));
                }
                plan::verify_applied(fs, &writes, &before.expect("snapshotted"), &after)?;
                outcome_plan_id = Some(binding.plan_id);
            }
        }
        Ok(CallOutcome {
            response,
            plan_id: outcome_plan_id,
        })
    }

    /// Map one transport exchange onto a validated response envelope.
    fn interpret(
        &self,
        exchange: Result<transport::TransportSuccess, transport::TransportFailure>,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, TargetFailure> {
        let success = match exchange {
            Ok(success) => success,
            Err(error) => {
                return Err(match error {
                    transport::TransportFailure::Spawn => {
                        TargetFailure::TransportFailed { detail: "spawn" }
                    }
                    transport::TransportFailure::RequestWrite => TargetFailure::TransportFailed {
                        detail: "request-write",
                    },
                    transport::TransportFailure::Timeout => TargetFailure::Timeout,
                    transport::TransportFailure::Cancelled => TargetFailure::Cancelled,
                    transport::TransportFailure::OutputLimit { stream } => {
                        TargetFailure::OutputLimit { stream }
                    }
                });
            }
        };
        // The envelope is the protocol: unparseable output is an
        // infrastructure refusal; a non-zero exit with a well-formed error
        // envelope stays an operation error, anything else is a crash.
        let value: serde_json::Value = match serde_json::from_slice(&success.stdout) {
            Ok(value) => value,
            Err(_) => {
                return Err(TargetFailure::crash_or_invalid(
                    success.exit_code,
                    ResponseInvalidity::NotJson,
                ));
            }
        };
        let response: ResponseEnvelope = match serde_json::from_value(value) {
            Ok(response) => response,
            Err(_) => {
                return Err(TargetFailure::crash_or_invalid(
                    success.exit_code,
                    ResponseInvalidity::Shape,
                ));
            }
        };
        if response.status == ResponseStatus::Ok && success.exit_code != 0 {
            return Err(TargetFailure::Crash {
                detail: format!("exit-{}", success.exit_code),
            });
        }
        match wire::validate_response_identity(&response, &request.request_id, request.operation) {
            Ok(()) => {}
            Err(wire::ResponseRejection::Protocol(detail)) => {
                return Err(TargetFailure::ProtocolMismatch { detail });
            }
            Err(wire::ResponseRejection::Invalid(detail)) => {
                return Err(TargetFailure::ResponseInvalid { detail });
            }
        }
        // A well-formed error envelope is an operation failure for every
        // operation, describe included; the pairing is part of the shape.
        let pairing_ok = match (&response.status, &response.error) {
            (ResponseStatus::Ok, None) => true,
            (ResponseStatus::Error, Some(error)) => {
                !error.code.is_empty() && error.code.len() <= 128 && !error.message.is_empty()
            }
            _ => false,
        };
        if !pairing_ok {
            return Err(TargetFailure::ResponseInvalid {
                detail: ResponseInvalidity::ErrorPairing,
            });
        }
        if response.status == ResponseStatus::Error {
            let error = response.error.as_ref().expect("pairing checked");
            let mut code = error.code.clone();
            code.truncate(128);
            return Err(TargetFailure::OperationFailed {
                class: error.class,
                code,
                partial: error.partial.unwrap_or(false),
            });
        }
        if let Some((_, described)) = self.described.as_ref() {
            if response.evidence.adapter != described.capabilities.adapter {
                return Err(TargetFailure::ResponseInvalid {
                    detail: ResponseInvalidity::Evidence,
                });
            }
        }
        Ok(response)
    }

    /// Validate the caller's request against the cached capabilities and
    /// the closed per-operation member table.
    fn validate_call_request(
        &self,
        request: &CallRequest<'_>,
        capabilities: &wire::Capabilities,
    ) -> Result<(), TargetFailure> {
        let invalid = |detail| Err(TargetFailure::RequestInvalid { detail });
        if request.operation == Operation::Describe {
            // `describe` travels through `TargetClient::describe` only.
            return invalid("operation");
        }
        if !capabilities.operations.contains(&request.operation) {
            return Err(TargetFailure::CapabilityUnsupported {
                detail: "operation",
            });
        }
        if request.operation.requires_ir() && request.ir_path.is_none() {
            return invalid("ir-path");
        }
        if request.operation == Operation::Generate && request.dry_run.is_none() {
            return invalid("dry-run");
        }
        if request.operation != Operation::Generate && request.dry_run.is_some() {
            return invalid("dry-run");
        }
        let apply = request.operation.is_destructive_apply()
            || (request.operation == Operation::Generate && request.dry_run == Some(false));
        if apply != request.plan_id.is_some() {
            // Apply exchanges must echo a plan; planning exchanges must not.
            return invalid("plan-id");
        }
        if request.operation == Operation::Generate && request.target.is_none() {
            return invalid("target");
        }
        if request.operation == Operation::Bind {
            if request.target.is_none() {
                return invalid("target");
            }
            if request.profile.is_none() {
                return invalid("profile");
            }
        }
        if let Some(target) = request.target {
            if !scopes::is_token(target) {
                return invalid("grammar");
            }
            if !capabilities.targets.is_empty()
                && !capabilities.targets.iter().any(|offered| offered == target)
            {
                return Err(TargetFailure::CapabilityUnsupported { detail: "target" });
            }
        }
        if let Some(profile) = request.profile {
            if !scopes::is_token(profile) {
                return invalid("grammar");
            }
            if !capabilities.profiles.is_empty()
                && !capabilities
                    .profiles
                    .iter()
                    .any(|offered| offered == profile)
            {
                return Err(TargetFailure::CapabilityUnsupported { detail: "profile" });
            }
        }
        if let Some(ir_path) = request.ir_path {
            if !scopes::is_logical_path(ir_path) {
                return invalid("grammar");
            }
            if !capabilities.read_scopes.is_empty()
                && !plan::covered_by(ir_path, &capabilities.read_scopes)
            {
                return Err(TargetFailure::scope_violation(
                    Some(ir_path.to_owned()),
                    "ir-uncovered",
                ));
            }
        }
        Ok(())
    }

    /// Validate the grammar and protection rules of one declared scope list.
    fn validate_scopes(&self, scope_list: &[String], read: bool) -> Result<(), TargetFailure> {
        if scope_list.len() > version::MAX_SCOPES {
            return Err(TargetFailure::scope_violation(None, "scope-limit"));
        }
        for scope in scope_list {
            if !scopes::is_scope(scope) {
                return Err(TargetFailure::scope_violation(
                    Some(scope.clone()),
                    "grammar",
                ));
            }
            if !read && scopes::scope_touches_protected_home(scope).is_some() {
                return Err(TargetFailure::scope_violation(
                    Some(scope.clone()),
                    "protected",
                ));
            }
        }
        Ok(())
    }

    /// Validate the operation-specific payload members of a response.
    fn validate_response_payload(
        &self,
        request: &CallRequest<'_>,
        response: &ResponseEnvelope,
        capabilities: &wire::Capabilities,
    ) -> Result<(), TargetFailure> {
        let invalid = |detail| Err(TargetFailure::ResponseInvalid { detail });
        // Progress is legal only from an adapter that declared it.
        if response.progress.is_some() && !capabilities.progress {
            return invalid(ResponseInvalidity::UnexpectedMember);
        }
        if let Some(result) = response.result.as_ref() {
            if result.entries.is_some() && request.operation != Operation::Scan {
                return invalid(ResponseInvalidity::UnexpectedMember);
            }
            if result.bindings.is_some() && request.operation != Operation::Bind {
                return invalid(ResponseInvalidity::UnexpectedMember);
            }
            if result.ok.is_some()
                && !matches!(request.operation, Operation::Validate | Operation::Verify)
            {
                return invalid(ResponseInvalidity::UnexpectedMember);
            }
        }
        if request.operation.declares_writes() {
            let Some(writes) = &response.writes else {
                return invalid(ResponseInvalidity::WritesMissing);
            };
            wire::validate_writes(writes, &capabilities.write_scopes)?;
        } else if response.writes.is_some() {
            return invalid(ResponseInvalidity::UnexpectedMember);
        }
        Ok(())
    }

    /// The wire projection of the client's transport limits.
    fn wire_limits(&self) -> wire::Limits {
        wire::Limits {
            timeout_ms: Some(self.limits.timeout_ms),
            max_output_bytes: Some(self.limits.max_output_bytes as u64),
        }
    }
}

/// The published protocol version of the embedded registry.
fn published_version() -> Result<String, TargetFailure> {
    let registry = crate::versioning::VersionRegistry::embedded()
        .map_err(|_| TargetFailure::RegistryInvalid)?;
    match registry.protocol().current() {
        Some(current) => Ok(current.to_string()),
        None => Err(TargetFailure::ProtocolUnpublished),
    }
}

/// The shared envelope prefix every request carries.
fn base_envelope(
    operation: Operation,
    protocol_version: String,
    limits: wire::Limits,
) -> RequestEnvelope {
    RequestEnvelope {
        protocol: version::PROTOCOL_TOKEN.to_owned(),
        protocol_version,
        operation,
        request_id: String::new(),
        project_root: ".".to_owned(),
        ir_path: None,
        target: None,
        profile: None,
        dry_run: None,
        limits: Some(limits),
        plan_id: None,
    }
}

/// Serialize one request to compact canonical JSON, bounded.
fn serialize(envelope: &RequestEnvelope) -> Result<Vec<u8>, TargetFailure> {
    let bytes = serde_json::to_vec(envelope).map_err(|_| TargetFailure::RequestInvalid {
        detail: "serialize",
    })?;
    if bytes.len() > version::MAX_REQUEST_BYTES {
        return Err(TargetFailure::RequestInvalid {
            detail: "request-size",
        });
    }
    Ok(bytes)
}

/// Canonical JSON bytes of any serializable capability payload.
fn canonical_bytes(value: &impl serde::Serialize) -> Vec<u8> {
    serde_json::to_vec(value).expect("capability payload serializes")
}

/// Map a scope-snapshot refusal onto its closed target failure. Snapshot
/// I/O during verification means the plan cannot be proven, so it fails
/// closed as a plan mismatch.
fn snapshot_rejection(rejection: plan::SnapshotRejection) -> TargetFailure {
    match rejection {
        plan::SnapshotRejection::Io => TargetFailure::plan_mismatch(None, "verification-io"),
        other => TargetFailure::scope_violation(None, other.detail()),
    }
}

/// Snapshot every declared write scope, mapping refusals onto failures.
fn snapshot_scopes(fs: &Fs, write_scopes: &[String]) -> Result<plan::Snapshot, TargetFailure> {
    plan::snapshot_scopes(fs, write_scopes).map_err(snapshot_rejection)
}
