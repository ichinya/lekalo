//! `lekalo-adapter-sdk-rust` — the adapter SDK for the `lekalo.target/v1`
//! process protocol (issue #106).
//!
//! Generated protocol/capability/diagnostic data plus the hand-maintained
//! runtime: strict decode, semantic validate, honest builders with the
//! validate-before-write boundary, path/scope helpers, deterministic ids
//! and canonical bytes, bounded stderr logging, and the per-exchange
//! serve loop. No ambient I/O beyond the serve loop; no network; no
//! telemetry; no `lekalo-core` coupling beyond the wire contracts.

pub mod build;
pub mod canonical;
pub mod decode;
pub mod generated;
pub mod ids;
pub mod json;
pub mod logger;
pub mod paths;
pub mod serve;
pub mod validate;

pub use build::{
    build_adapter_identity, build_capabilities, build_diagnostic, build_error, build_finding,
    build_success, response_bytes, BuildError,
};
pub use canonical::{
    build_artifact_manifest, canonical_plan_id, normalize_scenario_result, sort_write_plan,
    write_entry_digest, write_plan_violation,
};
pub use decode::{
    admit_request, admit_response, decode_request, validate_request, validate_response_identity,
    validate_response_payload, Refusal,
};
pub use ids::{
    is_plan_id, is_request_id, is_sha256_digest, plan_id, request_id, sha256, sha256_hex,
};
pub use json::{canonical, read_single_object, strict_parse, ParseReason, Value};
pub use paths::{
    is_capability_id, is_contract_version, is_logical_path, is_scope, is_token, path_violation,
    plan_scope_violation, protected_home, scope_covers, scope_touches_protected_home,
};
pub use serve::{serve, Adapter, DeadlineToken, Output};
