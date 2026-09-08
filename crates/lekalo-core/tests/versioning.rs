//! Issue #9 versioning tests: strict parsing, exact-set support, registry
//! invariants, the step catalog binding, the representability byte
//! transform, and the compatibility preflight.
//!
//! All cases are hermetic: they construct synthetic registries from typed
//! records or use the embedded bytes, and touch no global state.

use lekalo_core::loader::error::{Span, SpanPos};
use lekalo_core::loader::{DocKind, ModelVersion};
use lekalo_core::versioning::compatibility::{
    AdapterCompatibilityManifest, CompatibilityPreflight, CompatibilityReport,
};
use lekalo_core::versioning::family::{
    IrContract, ModelContract, ProtocolContract, RegistryContract,
};
use lekalo_core::versioning::graph::{
    catalog, family_chain, model_chain, ChainError, NotMigratableDetail, StepFailure,
    MODEL_STEP_0_1_0_TO_1_0_0,
};
use lekalo_core::versioning::registry::{
    AliasToken, ChangeClassification, FamilyRegistry, MigrationEdge, RegenerationImpact,
    VersionRecord, VersionRegistry, VersionState,
};
use lekalo_core::versioning::support::{
    ensure_usable, ir_support, model_support, ModelTarget, TargetError,
};
use lekalo_core::versioning::version::ContractVersion;

fn model(text: &str) -> ContractVersion<ModelContract> {
    ContractVersion::parse_canonical(text).expect("canonical model version")
}

fn ir(text: &str) -> ContractVersion<IrContract> {
    ContractVersion::parse_canonical(text).expect("canonical ir version")
}

fn protocol(text: &str) -> ContractVersion<ProtocolContract> {
    ContractVersion::parse_canonical(text).expect("canonical protocol version")
}

fn registry_version(text: &str) -> ContractVersion<RegistryContract> {
    ContractVersion::parse_canonical(text).expect("canonical registry version")
}

mod canonical_parsing {
    use super::*;

    #[test]
    fn canonical_releases_round_trip_exactly() {
        for text in ["0.0.0", "0.1.0", "1.0.0", "2.0.0", "10.20.30"] {
            let version = model(text);
            assert_eq!(version.as_str(), text);
        }
    }

    #[test]
    fn prereleases_parse_but_are_distinct_values() {
        let version = model("2.0.0-rc.1");
        assert!(version.is_prerelease());
        assert_eq!(version.as_str(), "2.0.0-rc.1");
        assert!(version < model("2.0.0"));
    }

    #[test]
    fn every_hostile_spelling_is_rejected() {
        const CASES: &[&str] = &[
            "1",
            "1.",
            "1.0",
            "1.0.",
            "1.0.0.0",
            "v1.0.0",
            "vv1",
            " 1.0.0",
            "1.0.0 ",
            "01.0.0",
            "1.00.0",
            "1.0.00",
            "1.0.0+build",
            "1.0.0-01",
            "１.０.０",
            "18446744073709551616.0.0",
            "",
            "x.0.0",
            "1.x.0",
            "1.0.x",
            "1..0",
            "1.0.0-",
            "1.0.0 ",
        ];
        for text in CASES {
            assert!(
                ContractVersion::<ModelContract>::parse_canonical(text).is_err(),
                "{text} must not parse"
            );
        }
    }

    #[test]
    fn parse_error_details_are_closed_tokens() {
        assert_eq!(
            ContractVersion::<ModelContract>::parse_canonical("1.0.0+b")
                .unwrap_err()
                .as_str(),
            "build-metadata"
        );
        assert_eq!(
            ContractVersion::<ModelContract>::parse_canonical("１.0.0")
                .unwrap_err()
                .as_str(),
            "not-ascii"
        );
        assert_eq!(
            ContractVersion::<ModelContract>::parse_canonical("")
                .unwrap_err()
                .as_str(),
            "empty"
        );
        assert_eq!(
            ContractVersion::<ModelContract>::parse_canonical("v1.0.0")
                .unwrap_err()
                .as_str(),
            "malformed"
        );
    }
}

mod finite_conversions {
    use super::*;

    #[test]
    fn model_versions_convert_exhaustively() {
        assert_eq!(
            ContractVersion::<ModelContract>::from(ModelVersion::V0_1_0).as_str(),
            "0.1.0"
        );
        assert_eq!(
            ContractVersion::<ModelContract>::from(ModelVersion::V1_0_0).as_str(),
            "1.0.0"
        );
    }

    #[test]
    fn ir_current_is_the_accepted_constant() {
        assert_eq!(ContractVersion::<IrContract>::current().as_str(), "0.1.0");
        assert_eq!(lekalo_core::ir::IDENTITY, "dev.lekalo.ir@0.1.0");
    }
}

mod alias_tokens {
    use super::*;

    #[test]
    fn only_v_major_tokens_are_aliases() {
        assert!(AliasToken::parse("v1").is_some());
        assert!(AliasToken::parse("v12").is_some());
        assert!(AliasToken::parse("v01").is_none());
        assert!(AliasToken::parse("vv1").is_none());
        assert!(AliasToken::parse("v1.0").is_none());
        assert!(AliasToken::parse("v").is_none());
        assert!(AliasToken::parse("1").is_none());
    }
}

mod embedded_registry {
    use super::*;

    #[test]
    fn embedded_bytes_parse_and_validate() {
        let registry = VersionRegistry::embedded().expect("shipped registry is valid");
        assert_eq!(registry.registry_version().as_str(), "1.0.0");
    }

    #[test]
    fn shipped_policy_is_exact() {
        let registry = VersionRegistry::embedded().expect("valid");
        let model_family = registry.model();
        assert_eq!(
            model_family.current().map(|v| v.to_string()),
            Some("1.0.0".to_owned())
        );
        assert_eq!(model_family.versions().len(), 2);
        assert_eq!(
            model_family.min().map(|v| v.to_string()),
            Some("0.1.0".to_owned())
        );
        assert_eq!(
            model_family.max().map(|v| v.to_string()),
            Some("1.0.0".to_owned())
        );
        let deprecated = &model_family.versions()[0];
        assert_eq!(deprecated.version.as_str(), "0.1.0");
        assert_eq!(deprecated.state.as_str(), "deprecated");
        match &deprecated.state {
            VersionState::Deprecated {
                deprecated_since,
                retirement_not_before,
            } => {
                assert_eq!(deprecated_since.as_str(), "1.0.0");
                assert_eq!(retirement_not_before.as_str(), "2.0.0");
            }
            other => panic!("unexpected state: {other:?}"),
        }
        assert_eq!(model_family.versions()[1].state.as_str(), "supported");
        assert_eq!(model_family.aliases()[0].0.as_str(), "v1");
        assert_eq!(model_family.aliases()[0].1.as_str(), "1.0.0");
        assert_eq!(model_family.edges().len(), 1);
        assert_eq!(model_family.edges()[0].id, MODEL_STEP_0_1_0_TO_1_0_0);

        let ir_family = registry.ir();
        assert_eq!(
            ir_family.current().map(|v| v.to_string()),
            Some("0.1.0".to_owned())
        );
        assert_eq!(ir_family.versions().len(), 1);
        assert!(ir_family.aliases().is_empty());
        assert!(ir_family.edges().is_empty());

        let protocol_family = registry.protocol();
        assert_eq!(
            protocol_family.current().map(|v| v.to_string()),
            Some("1.0.0".to_owned())
        );
        assert_eq!(protocol_family.versions().len(), 1);
        assert_eq!(protocol_family.aliases().len(), 1);
        assert_eq!(protocol_family.aliases()[0].0.as_str(), "v1");
        assert_eq!(protocol_family.aliases()[0].1.as_str(), "1.0.0");
        assert!(protocol_family.edges().is_empty());
    }

    #[test]
    fn the_interval_hole_regression_holds() {
        // 0.2.0 lies numerically between registered entries and must stay
        // unsupported: support is exact-set membership, never an interval.
        let registry = VersionRegistry::embedded().expect("valid");
        assert!(registry.model().record(&model("0.2.0")).is_none());
    }

    #[test]
    fn retired_and_preregistered_support_states_behave() {
        // A retired entry is refused even while present in the exact set.
        let family = FamilyRegistry::new(
            Some(model("2.0.0")),
            vec![
                VersionRecord {
                    version: model("1.0.0"),
                    state: VersionState::Retired {
                        retired_since: model("2.0.0"),
                        replacement: Some(model("2.0.0")),
                    },
                    classification: ChangeClassification::Breaking,
                    reason: "synthetic".to_owned(),
                },
                VersionRecord {
                    version: model("2.0.0"),
                    state: VersionState::Supported,
                    classification: ChangeClassification::Breaking,
                    reason: "synthetic".to_owned(),
                },
            ],
            Vec::new(),
            Vec::new(),
        );
        let mut details = Vec::new();
        family.validate(&mut details);
        assert!(details.is_empty(), "{details:?}");
        let retired = &family.versions()[0];
        assert_eq!(retired.state.as_str(), "retired");
        assert!(!retired.state.is_usable());
    }

    #[test]
    fn invalid_registries_fail_closed() {
        // Family with entries but no current.
        let json = br#"{"registry":"dev.lekalo.version-registry","registryVersion":"1.0.0","families":{"model":{"current":null,"aliases":[],"versions":[{"version":"1.0.0","state":"supported","classification":"additive","reason":"r"}],"migrations":[]},"ir":{"current":null,"aliases":[],"versions":[],"migrations":[]},"protocol":{"current":null,"aliases":[],"versions":[],"migrations":[]}}}"#;
        let error = VersionRegistry::from_bytes(json).unwrap_err();
        assert!(
            error
                .details
                .iter()
                .any(|d| d.contains("has no current version")),
            "{error:?}"
        );

        // Duplicate alias.
        let json = br#"{"registry":"dev.lekalo.version-registry","registryVersion":"1.0.0","families":{"model":{"current":"1.0.0","aliases":[{"alias":"v1","version":"1.0.0"},{"alias":"v1","version":"0.1.0"}],"versions":[{"version":"0.1.0","state":"supported","classification":"additive","reason":"r"},{"version":"1.0.0","state":"supported","classification":"additive","reason":"r"}],"migrations":[]},"ir":{"current":null,"aliases":[],"versions":[],"migrations":[]},"protocol":{"current":null,"aliases":[],"versions":[],"migrations":[]}}}"#;
        let error = VersionRegistry::from_bytes(json).unwrap_err();
        assert!(error.details.iter().any(|d| d.contains("duplicate alias")));

        // Downgrade edge must strictly ascend.
        let json = br#"{"registry":"dev.lekalo.version-registry","registryVersion":"1.0.0","families":{"model":{"current":"1.0.0","aliases":[],"versions":[{"version":"0.1.0","state":"supported","classification":"additive","reason":"r"},{"version":"1.0.0","state":"supported","classification":"additive","reason":"r"}],"migrations":[{"id":"down@1","from":"1.0.0","to":"0.1.0","classification":"breaking","loss":[],"regenerationImpact":"none","reason":"r"}]},"ir":{"current":null,"aliases":[],"versions":[],"migrations":[]},"protocol":{"current":null,"aliases":[],"versions":[],"migrations":[]}}}"#;
        let error = VersionRegistry::from_bytes(json).unwrap_err();
        assert!(error
            .details
            .iter()
            .any(|d| d.contains("must strictly ascend")));

        // Alias targeting an unregistered version.
        let json = br#"{"registry":"dev.lekalo.version-registry","registryVersion":"1.0.0","families":{"model":{"current":"1.0.0","aliases":[{"alias":"v2","version":"2.0.0"}],"versions":[{"version":"1.0.0","state":"supported","classification":"additive","reason":"r"}],"migrations":[]},"ir":{"current":null,"aliases":[],"versions":[],"migrations":[]},"protocol":{"current":null,"aliases":[],"versions":[],"migrations":[]}}}"#;
        let error = VersionRegistry::from_bytes(json).unwrap_err();
        assert!(error
            .details
            .iter()
            .any(|d| d.contains("targets unregistered")));

        // Unknown field: the artifact is closed.
        let json = br#"{"registry":"dev.lekalo.version-registry","registryVersion":"1.0.0","surprise":1,"families":{"model":{"current":null,"aliases":[],"versions":[],"migrations":[]},"ir":{"current":null,"aliases":[],"versions":[],"migrations":[]},"protocol":{"current":null,"aliases":[],"versions":[],"migrations":[]}}}"#;
        assert!(VersionRegistry::from_bytes(json).is_err());

        // Build metadata in a version spelling: not canonical.
        let json = br#"{"registry":"dev.lekalo.version-registry","registryVersion":"1.0.0+nightly","families":{"model":{"current":null,"aliases":[],"versions":[],"migrations":[]},"ir":{"current":null,"aliases":[],"versions":[],"migrations":[]},"protocol":{"current":null,"aliases":[],"versions":[],"migrations":[]}}}"#;
        let error = VersionRegistry::from_bytes(json).unwrap_err();
        assert!(error.details.iter().any(|d| d.contains("registryVersion")));
    }
}

mod support_gate {
    use super::*;

    #[test]
    fn deprecated_stays_usable_supported_is_usable() {
        let registry = VersionRegistry::embedded().expect("valid");
        let deprecated = model_support(registry, ModelVersion::V0_1_0);
        assert_eq!(deprecated.state, "deprecated");
        ensure_usable(&deprecated).expect("deprecated is usable");
        let supported = model_support(registry, ModelVersion::V1_0_0);
        assert_eq!(supported.state, "supported");
        ensure_usable(&supported).expect("supported is usable");
        let ir_verdict = ir_support(registry, &ir("0.1.0"));
        ensure_usable(&ir_verdict).expect("ir current is usable");
    }

    #[test]
    fn unregistered_versions_refuse() {
        let registry = VersionRegistry::embedded().expect("valid");
        let verdict = ir_support(registry, &ir("9.9.9"));
        assert_eq!(verdict.state, "unregistered");
        let error = ensure_usable(&verdict).expect_err("unregistered refuses");
        assert_eq!(error.state, "unregistered");
        assert!(error.replacement.is_none());
    }

    #[test]
    fn targets_resolve_exactly_with_no_inference() {
        let registry = VersionRegistry::embedded().expect("valid");
        let canonical = ModelTarget::parse("1.0.0").expect("canonical target");
        assert_eq!(
            canonical.resolve(registry).expect("registered").as_str(),
            "1.0.0"
        );
        let alias = ModelTarget::parse("v1").expect("alias target");
        assert_eq!(alias.resolve(registry).expect("declared").as_str(), "1.0.0");

        // Well-formed but unregistered: the exit-5 refusal.
        for selector in ["2.0.0", "0.2.0", "1.0.1", "2.0.0-rc.1"] {
            let target = ModelTarget::parse(selector).expect("well-formed");
            match target.resolve(registry) {
                Err(TargetError::Unsupported(spelled)) => assert_eq!(spelled, selector),
                other => panic!("{selector}: {other:?}"),
            }
        }

        // Malformed: the exit-1 refusal, no inference.
        for selector in ["1", "v1.0.0", "vv1", "1.0", "01.0.0", "1.0.0+meta"] {
            assert!(ModelTarget::parse(selector).is_err(), "{selector}");
        }
    }
}

mod graph_catalog {
    use super::*;

    #[test]
    fn shipped_catalog_has_exactly_one_model_step() {
        assert_eq!(catalog().len(), 1);
        let step = catalog()[0];
        assert_eq!(step.id(), "model-0.1.0-to-1.0.0@1");
        assert_eq!(step.from(), ModelVersion::V0_1_0);
        assert_eq!(step.to(), ModelVersion::V1_0_0);
        assert_eq!(step.classification(), ChangeClassification::Breaking);
        assert!(
            step.loss().is_empty(),
            "byte-preserving step declares no loss"
        );
    }

    #[test]
    fn chains_are_explicit_and_unique() {
        let registry = VersionRegistry::embedded().expect("valid");
        let chain =
            model_chain(registry, ModelVersion::V0_1_0, ModelVersion::V1_0_0).expect("declared");
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].id, MODEL_STEP_0_1_0_TO_1_0_0);
        assert!(
            model_chain(registry, ModelVersion::V1_0_0, ModelVersion::V1_0_0)
                .expect("same version is an empty chain")
                .is_empty()
        );
        assert_eq!(
            model_chain(registry, ModelVersion::V1_0_0, ModelVersion::V0_1_0).unwrap_err(),
            ChainError::NoPath,
            "no automatic downgrades"
        );
    }

    #[test]
    fn registry_edges_and_catalog_bind_one_to_one() {
        // `embedded()` runs `validate_registry_binding`; a valid result is
        // the proof of the one-to-one binding (an edge without an
        // implementation or an implementation without an edge fails).
        VersionRegistry::embedded().expect("binding holds");
    }
}

mod synthetic_families {
    use super::*;

    /// A 1.0.0 -> 2.0.0 -> 3.0.0 synthetic family proving real one-step
    /// chain resolution and the unique-path invariant.
    fn synthetic_family() -> FamilyRegistry<ModelContract> {
        let records = [
            ("1.0.0", "additive"),
            ("2.0.0", "breaking"),
            ("3.0.0", "breaking"),
        ]
        .into_iter()
        .map(|(version, classification)| VersionRecord {
            version: model(version),
            state: VersionState::Supported,
            classification: ChangeClassification::parse(classification).unwrap(),
            reason: "synthetic".to_owned(),
        })
        .collect();
        let edges = vec![
            MigrationEdge {
                id: "a@1".to_owned(),
                from: model("1.0.0"),
                to: model("2.0.0"),
                classification: ChangeClassification::Breaking,
                loss: vec!["synthetic".to_owned()],
                regeneration_impact: RegenerationImpact::ChangedModules,
                reason: "synthetic".to_owned(),
            },
            MigrationEdge {
                id: "b@1".to_owned(),
                from: model("2.0.0"),
                to: model("3.0.0"),
                classification: ChangeClassification::Breaking,
                loss: Vec::new(),
                regeneration_impact: RegenerationImpact::All,
                reason: "synthetic".to_owned(),
            },
        ];
        FamilyRegistry::new(Some(model("3.0.0")), records, Vec::new(), edges)
    }

    #[test]
    fn unique_two_step_chain_plans_in_order() {
        let family = synthetic_family();
        let mut details = Vec::new();
        family.validate(&mut details);
        assert!(details.is_empty(), "{details:?}");
        let chain = family_chain(&family, &model("1.0.0"), &model("3.0.0")).expect("unique path");
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].id, "a@1");
        assert_eq!(chain[1].id, "b@1");
    }

    #[test]
    fn two_path_ambiguity_is_a_registry_fault() {
        let family = synthetic_family();
        let extra_versions = family.versions().to_vec();
        let mut edges = family.edges().to_vec();
        edges.push(MigrationEdge {
            id: "c@1".to_owned(),
            from: model("1.0.0"),
            to: model("2.0.0"),
            classification: ChangeClassification::Breaking,
            loss: Vec::new(),
            regeneration_impact: RegenerationImpact::None,
            reason: "synthetic".to_owned(),
        });
        let _ = extra_versions;
        // Rebuild with two parallel 1.0.0 -> 2.0.0 edges: two 1.0.0 ->
        // 3.0.0 paths make the registry invalid.
        let duplicate = FamilyRegistry::new(
            Some(model("3.0.0")),
            [
                ("1.0.0", "additive"),
                ("2.0.0", "breaking"),
                ("3.0.0", "breaking"),
            ]
            .into_iter()
            .map(|(version, classification)| VersionRecord {
                version: model(version),
                state: VersionState::Supported,
                classification: ChangeClassification::parse(classification).unwrap(),
                reason: "synthetic".to_owned(),
            })
            .collect(),
            Vec::new(),
            edges,
        );
        let mut details = Vec::new();
        duplicate.validate(&mut details);
        assert!(
            details
                .iter()
                .any(|detail| detail.contains("2 migration paths from 1.0.0 to 3.0.0")),
            "{details:?}"
        );
    }

    #[test]
    fn missing_chain_is_no_path() {
        let family = synthetic_family();
        assert_eq!(
            family_chain(&family, &model("3.0.0"), &model("1.0.0")).unwrap_err(),
            ChainError::NoPath,
            "no automatic downgrades"
        );
    }
}

mod compatibility {
    use super::*;

    fn manifest() -> AdapterCompatibilityManifest {
        AdapterCompatibilityManifest::new(
            registry_version("1.0.0"),
            "example-adapter",
            ir("0.1.0"),
            ir("0.1.0"),
            Some(lekalo_core::versioning::compatibility::ProtocolBounds {
                min: protocol("1.0.0"),
                max: protocol("1.0.0"),
            }),
            Vec::new(),
            Vec::new(),
        )
        .expect("valid manifest")
    }

    #[test]
    fn unpublished_protocol_refuses_before_anything_runs() {
        let registry = VersionRegistry::embedded().expect("valid");
        let verdict = CompatibilityPreflight::check(registry, &ir("0.1.0"), None, &manifest());
        assert!(!verdict.is_compatible());
        assert_eq!(verdict.reasons(), &["versioning.protocol-unpublished"]);
    }

    #[test]
    fn required_extensions_are_never_satisfied_by_the_accepted_ir() {
        let registry = VersionRegistry::embedded().expect("valid");
        let mut extended = manifest();
        extended.required_extensions = vec!["future-ext".to_owned()];
        let verdict = CompatibilityPreflight::check(
            registry,
            &ir("0.1.0"),
            Some(&protocol("1.0.0")),
            &extended,
        );
        assert!(verdict
            .reasons()
            .contains(&"versioning.extension-incompatible"));
    }

    #[test]
    fn unregistered_protocol_and_inverted_ranges_refuse() {
        let registry = VersionRegistry::embedded().expect("valid");
        // A protocol version outside the (empty) registry: unsupported.
        let verdict = CompatibilityPreflight::check(
            registry,
            &ir("0.1.0"),
            Some(&protocol("9.9.9")),
            &manifest(),
        );
        assert!(verdict
            .reasons()
            .contains(&"versioning.unsupported-version"));

        // Inverted IR range is a manifest fault.
        assert_eq!(
            AdapterCompatibilityManifest::new(
                registry_version("1.0.0"),
                "example-adapter",
                ir("0.2.0"),
                ir("0.1.0"),
                None,
                Vec::new(),
                Vec::new(),
            )
            .unwrap_err()
            .as_str(),
            "ir-range"
        );

        // A wrong manifest schema version is a manifest fault.
        assert_eq!(
            AdapterCompatibilityManifest::new(
                registry_version("0.9.0"),
                "example-adapter",
                ir("0.1.0"),
                ir("0.1.0"),
                None,
                Vec::new(),
                Vec::new(),
            )
            .unwrap_err()
            .as_str(),
            "manifest-version"
        );

        // Reserved adapter identity spelling stays invalid.
        assert_eq!(
            AdapterCompatibilityManifest::new(
                registry_version("1.0.0"),
                "bad id!",
                ir("0.1.0"),
                ir("0.1.0"),
                None,
                Vec::new(),
                Vec::new(),
            )
            .unwrap_err()
            .as_str(),
            "adapter-id"
        );
    }

    #[test]
    fn report_projects_families_in_fixed_order() {
        let registry = VersionRegistry::embedded().expect("valid");
        let report = CompatibilityReport::from_registry(registry);
        assert_eq!(report.status, "valid");
        assert_eq!(report.registry_version, "1.0.0");
        let names: Vec<&str> = report.families.iter().map(|f| f.family).collect();
        assert_eq!(names, ["model", "ir", "protocol"]);
        let model_family: &CompatibilityReportFamilyAlias = &report.families[0];
        assert_eq!(model_family.current.as_deref(), Some("1.0.0"));
    }

    type CompatibilityReportFamilyAlias = lekalo_core::versioning::compatibility::FamilySummary;
}

mod planner_step {
    use super::*;
    use lekalo_core::versioning::plan::DocumentSnapshot;

    const SOURCE_DOC: &str = "# keep me\r\nschema_version: \"0.1.0\" # keep\r\nother: \"not-a-version\"\r\ndefinitions:\r\n  - id: planner\r\n    kind: project\r\n    version: 1\r\n";

    fn snapshot(bytes: &[u8]) -> DocumentSnapshot {
        let text = String::from_utf8(bytes.to_vec()).expect("utf8");
        let index = text.find("\"0.1.0\"").expect("quoted token present");
        let pos = |byte: usize| SpanPos {
            byte,
            line: 1,
            column: 1,
        };
        DocumentSnapshot {
            path: "lekalo/project.yaml".to_owned(),
            version: "0.1.0".to_owned(),
            version_span: Span::new(pos(index + 1), pos(index + 6)),
            kind: DocKind::Project,
            bytes: bytes.to_vec(),
        }
    }

    #[test]
    fn transform_rewrites_only_the_version_token() {
        let step = catalog()[0];
        let document = snapshot(SOURCE_DOC.as_bytes());
        let after = step.transform(&document).expect("rewrites cleanly");
        let text = String::from_utf8(after).unwrap();
        assert!(text.contains("schema_version: \"1.0.0\" # keep"));
        assert!(text.contains("# keep me\r\n"));
        assert!(text.contains("other: \"not-a-version\""));
        assert!(text.ends_with("version: 1\r\n"), "final CRLF preserved");
        assert_eq!(
            text.matches("1.0.0").count(),
            1,
            "exactly the schema token changed"
        );
    }
    #[test]
    fn transform_refuses_foreign_versions_and_missing_tokens() {
        let step = catalog()[0];
        let mut foreign = snapshot(SOURCE_DOC.as_bytes());
        foreign.version = "1.0.0".to_owned();
        match step.transform(&foreign) {
            Err(StepFailure::NotFileMigratable {
                detail: NotMigratableDetail::UnexpectedSourceVersion,
                ..
            }) => {}
            other => panic!("unexpected: {other:?}"),
        }
        let mut truncated = snapshot(SOURCE_DOC.as_bytes());
        truncated.version_span.end.byte = truncated.version_span.start.byte + 2;
        match step.transform(&truncated) {
            Err(StepFailure::NotFileMigratable {
                detail: NotMigratableDetail::TokenNotUnique,
                ..
            }) => {}
            other => panic!("unexpected: {other:?}"),
        }
    }
}

mod digests {
    use lekalo_core::versioning::plan::sha256_hex;

    #[test]
    fn sha256_matches_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
