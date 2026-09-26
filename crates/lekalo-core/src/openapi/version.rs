//! Closed identity, bounds, and constants of the OpenAPI projection
//! (issue #46). The generator identity rides inside every rendered
//! document (`x-lekalo-provenance.generator`), so an emitted artifact
//! is self-pinning.

/// The generator identity carried in `x-lekalo-provenance`.
pub const GENERATOR_ID: &str = "lekalo-core/openapi";

/// The generator version: the product version of the commit that
/// changed the projection (the versioning-rule constant).
pub const GENERATOR_VERSION: &str = "0.4.0";

/// The generated-sidecar ownership-manifest contract of fragment
/// mode (the `lekalo/zod-map` sidecar precedent).
pub const OWNERSHIP_CONTRACT: &str = "lekalo/openapi-map/v0.4.0";

/// The canonical document byte bound. The rendered document is
/// strictly larger than the attachment it projects (it embeds every
/// referenced schema), so the bound is four times the attachment
/// bound.
pub const MAX_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;

/// The maximum number of reusable schema components one document may
/// declare.
pub const MAX_COMPONENTS: usize = 8192;

/// The maximum number of operations one document may declare.
pub const MAX_OPERATIONS: usize = 4096;
