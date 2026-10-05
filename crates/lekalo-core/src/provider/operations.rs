//! The closed operation vocabulary of the issue #34 workflow provider.
//!
//! One variant per AIFHub `/aif-*` provider operation. Detection is not
//! an operation: it is the manifest plus the installed-tool check the
//! consumer performs. Every operation carries its effect class, its
//! output schema identity, and its prerequisite flags, so a consumer
//! can negotiate and construct argv from `docs/provider-contract.md`
//! without ever executing provider-returned command text.
//!
//! Effect classes state the boundary-relevant truth:
//!
//! - `read-only` operations never write **when invoked through the
//!   prescribed provider argv** in `docs/provider-contract.md`. For
//!   `validate` that argv carries `--no-cache`, because the default
//!   cached pipeline materializes `.lekalo/cache/cache.sqlite`.
//! - `generated-artifacts` (only the mutating `generate` form) writes
//!   into the adapter-declared managed write scopes verified against
//!   the ownership plan, plus Lekalo's own `.lekalo/generated/**`
//!   metadata. The protected homes (`openspec/**`, `lekalo/**`,
//!   `lekalo.lock`, `.lekalo/{ir,cache,import,privacy,consumer}/**`)
//!   are refused as `target.protected-path` (exit 3). The `drift`
//!   operation is the read-only check variant of the same command:
//!   it never writes and never needs an adapter.

#![allow(non_snake_case)] // wire field names are the published contract

use serde::Serialize;

/// The closed effect classes. They describe the write surface of the
/// operation **as invoked through the prescribed provider argv**.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EffectClass {
    /// Never writes under the prescribed argv.
    ReadOnly,
    /// The mutating generation form: adapter-declared managed write
    /// scopes verified against the ownership plan, plus `.lekalo/**`
    /// metadata. Protected homes are refused.
    GeneratedArtifacts,
}

impl EffectClass {
    /// The stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::GeneratedArtifacts => "generated-artifacts",
        }
    }
}

/// One advertised workflow operation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Operation {
    /// The stable operation id the consumer selects.
    id: &'static str,
    /// The effect class of the underlying command under the prescribed
    /// argv.
    effect: EffectClass,
    /// The output schema identity of the success payload: either the
    /// exact `lekalo/.../v<version>` discriminator embedded on the
    /// wire, or — when the payload carries no discriminator — the
    /// published closed describing schema of this contract series.
    outputSchema: &'static str,
    /// Whether the command requires a validated project selection.
    requiresProject: bool,
    /// Whether the mutating form requires an explicit target adapter
    /// program. `generate`'s read-only `--check` form needs none (see
    /// the `drift` companion operation).
    requiresAdapter: bool,
}

impl Operation {
    /// The stable operation id.
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// The effect class.
    pub const fn effect(&self) -> EffectClass {
        self.effect
    }

    /// The output schema identity.
    pub const fn output_schema(&self) -> &'static str {
        self.outputSchema
    }

    /// Whether a validated project selection is required.
    pub const fn requires_project(&self) -> bool {
        self.requiresProject
    }

    /// Whether an explicit target adapter program is required.
    pub const fn requires_adapter(&self) -> bool {
        self.requiresAdapter
    }
}

/// The advertised operations in canonical (sorted) order. The exact
/// order is part of the closed manifest contract.
pub const OPERATIONS: [Operation; crate::provider::version::OPERATION_COUNT] = [
    Operation {
        id: "context",
        effect: EffectClass::ReadOnly,
        outputSchema: crate::context::version::SCHEMA_VERSION,
        requiresProject: true,
        requiresAdapter: false,
    },
    Operation {
        id: "doctor",
        effect: EffectClass::ReadOnly,
        outputSchema: crate::doctor::version::SCHEMA_VERSION,
        requiresProject: true,
        requiresAdapter: false,
    },
    Operation {
        id: "drift",
        effect: EffectClass::ReadOnly,
        outputSchema: crate::provider::version::GENERATE_CHECK_SCHEMA,
        requiresProject: true,
        requiresAdapter: false,
    },
    Operation {
        id: "generate",
        effect: EffectClass::GeneratedArtifacts,
        outputSchema: crate::orchestration::SCHEMA_VERSION,
        requiresProject: true,
        requiresAdapter: true,
    },
    Operation {
        id: "impact",
        effect: EffectClass::ReadOnly,
        outputSchema: crate::impact::SCHEMA_VERSION,
        requiresProject: true,
        requiresAdapter: false,
    },
    Operation {
        id: "readiness",
        effect: EffectClass::ReadOnly,
        outputSchema: crate::doctor::version::SCHEMA_VERSION,
        requiresProject: true,
        requiresAdapter: false,
    },
    Operation {
        id: "status",
        effect: EffectClass::ReadOnly,
        outputSchema: crate::doctor::version::SCHEMA_VERSION,
        requiresProject: true,
        requiresAdapter: false,
    },
    Operation {
        id: "trace.assess",
        effect: EffectClass::ReadOnly,
        outputSchema: crate::trace::assessment::REPORT_SCHEMA,
        requiresProject: false,
        requiresAdapter: false,
    },
    Operation {
        id: "trace.export",
        effect: EffectClass::ReadOnly,
        outputSchema: crate::trace::version::SCHEMA_VERSION,
        requiresProject: false,
        requiresAdapter: false,
    },
    Operation {
        id: "validate",
        effect: EffectClass::ReadOnly,
        outputSchema: crate::provider::version::VALIDATION_REPORT_SCHEMA,
        requiresProject: true,
        requiresAdapter: false,
    },
    Operation {
        id: "verify",
        effect: EffectClass::ReadOnly,
        outputSchema: crate::orchestration::SCHEMA_VERSION,
        requiresProject: true,
        requiresAdapter: false,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocabulary_is_closed_sorted_and_exactly_ten() {
        let ids: Vec<&str> = OPERATIONS.iter().map(|operation| operation.id()).collect();
        assert_eq!(
            ids,
            vec![
                "context",
                "doctor",
                "drift",
                "generate",
                "impact",
                "readiness",
                "status",
                "trace.assess",
                "trace.export",
                "validate",
                "verify"
            ]
        );
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted, "canonical order is sorted");
        assert_eq!(OPERATIONS.len(), crate::provider::version::OPERATION_COUNT);
    }

    #[test]
    fn no_hidden_lifecycle_operation_is_advertised() {
        for operation in &OPERATIONS {
            let id = operation.id();
            assert!(
                !matches!(
                    id,
                    "init" | "install" | "update" | "sync" | "cleanup" | "migrate"
                ),
                "operation {id} must never be advertised"
            );
        }
    }

    #[test]
    fn generation_is_the_only_mutating_operation() {
        for operation in &OPERATIONS {
            if operation.id() == "generate" {
                assert_eq!(operation.effect(), EffectClass::GeneratedArtifacts);
                assert!(operation.requires_adapter());
            } else {
                assert_eq!(
                    operation.effect(),
                    EffectClass::ReadOnly,
                    "operation {} must be read-only",
                    operation.id()
                );
                assert!(
                    !operation.requires_adapter(),
                    "read-only operation {} needs no adapter",
                    operation.id()
                );
            }
        }
    }

    #[test]
    fn schema_identities_are_pinned_per_operation() {
        let schema_of = |id: &str| {
            OPERATIONS
                .iter()
                .find(|operation| operation.id() == id)
                .expect("operation")
                .output_schema()
        };
        assert_eq!(schema_of("context"), "lekalo/context/v0.2.16");
        assert_eq!(schema_of("impact"), "lekalo/impact/v0.2.16");
        assert_eq!(schema_of("doctor"), "lekalo/doctor/v0.3.2");
        assert_eq!(schema_of("status"), "lekalo/doctor/v0.3.2");
        assert_eq!(schema_of("readiness"), "lekalo/doctor/v0.3.2");
        assert_eq!(schema_of("verify"), "lekalo/orchestration/v0.2.16");
        assert_eq!(schema_of("generate"), "lekalo/orchestration/v0.2.16");
        assert_eq!(schema_of("trace.export"), "lekalo/trace-manifest/v0.2.16");
        assert_eq!(
            schema_of("validate"),
            crate::provider::version::VALIDATION_REPORT_SCHEMA
        );
        assert_eq!(
            schema_of("drift"),
            crate::provider::version::GENERATE_CHECK_SCHEMA
        );
        for operation in &OPERATIONS {
            assert!(
                operation.output_schema().starts_with("lekalo/"),
                "schema identity must use the native discriminator form"
            );
        }
    }
}
