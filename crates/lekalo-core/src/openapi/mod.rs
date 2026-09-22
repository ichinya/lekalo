//! The deterministic OpenAPI projection of the HTTP/JSON transport
//! attachment (issue #46).
//!
//! The document is a **transport projection**: rendered from one
//! validated attachment joined with the compiled project IR through
//! [`crate::transport_http::ValidationContext`] — the same validated
//! join the route-surface projection consumes. It is never the
//! canonical domain model, the Model and IR gain no
//! transport/documentation construct, and anything the declared
//! sources cannot express surfaces as an `openapi.projection-partial`
//! warning naming the symbol — never a silent drop, never invented
//! prose, servers, contact, license, or URLs.
//!
//! Boundaries: rendering is pure and read-only — no filesystem, no
//! execution, no runtime server or middleware generation. Emission
//! bytes belong to the adapter: core produces canonical JSON (compact,
//! byte-sorted keys) plus the deterministic fragment/ownership model,
//! and the node-typescript adapter serializes that JSON to YAML.
//! Compatibility is classified once: the pointer-level view maps
//! `transport_http::DiffClass` paths onto the document locations they
//! touch; no second taxonomy exists.

mod diagnostic;
mod id;
mod render;
mod schema;
mod types;
mod version;

pub use diagnostic::{io_failure, rule_set};
pub use id::{component_name, escape_pointer, paths_pointer, schemas_pointer, COMPONENTS_SCHEMAS};
pub use render::{render, OpenApiDocument};
pub use schema::{component_body, SchemaMapper};
pub use types::{DocumentMode, DocumentVersion, Finding, RenderConfig};
pub use version::{
    GENERATOR_ID, GENERATOR_VERSION, MAX_COMPONENTS, MAX_DOCUMENT_BYTES, MAX_OPERATIONS,
    OWNERSHIP_CONTRACT,
};
