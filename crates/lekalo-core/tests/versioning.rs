//! Exact contract support and compatibility regressions.
use lekalo_core::versioning::compatibility::{
    AdapterCompatibilityManifest, CompatibilityPreflight, CompatibilityReport,
};
use lekalo_core::versioning::family::{
    IrContract, ModelContract, ProtocolContract, RegistryContract,
};
use lekalo_core::versioning::{ContractVersion, VersionRegistry};
fn model(s: &str) -> ContractVersion<ModelContract> {
    ContractVersion::parse_canonical(s).unwrap()
}
fn ir(s: &str) -> ContractVersion<IrContract> {
    ContractVersion::parse_canonical(s).unwrap()
}
fn protocol(s: &str) -> ContractVersion<ProtocolContract> {
    ContractVersion::parse_canonical(s).unwrap()
}
fn registry_version(s: &str) -> ContractVersion<RegistryContract> {
    ContractVersion::parse_canonical(s).unwrap()
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

mod compatibility {
    use super::*;

    fn manifest() -> AdapterCompatibilityManifest {
        AdapterCompatibilityManifest::new(
            registry_version("0.2.16"),
            "example-adapter",
            ir("0.2.16"),
            ir("0.2.16"),
            Some(lekalo_core::versioning::compatibility::ProtocolBounds {
                min: protocol("0.3.2"),
                max: protocol("0.3.2"),
            }),
            Vec::new(),
            Vec::new(),
        )
        .expect("valid manifest")
    }

    #[test]
    fn unpublished_protocol_refuses_before_anything_runs() {
        let registry = VersionRegistry::embedded().expect("valid");
        let verdict = CompatibilityPreflight::check(registry, &ir("0.2.16"), None, &manifest());
        assert!(!verdict.is_compatible());
        assert_eq!(verdict.reasons(), &["versioning.protocol-unpublished"]);
    }

    #[test]
    fn historical_protocol_snapshot_does_not_inherit_publication() {
        let mut snapshot: serde_json::Value =
            serde_json::from_slice(lekalo_core::versioning::REGISTRY_BYTES).unwrap();
        snapshot["families"]["protocol"] = serde_json::from_str(include_str!(
            "../../../tests/fixtures/versioning/protocol-unpublished.family.json"
        ))
        .unwrap();
        let historical =
            VersionRegistry::from_bytes(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
        let published = VersionRegistry::embedded().unwrap();
        assert!(historical.protocol().current().is_none());
        assert!(historical.protocol().resolve_alias("v1").is_none());
        assert_eq!(published.protocol().resolve_alias("v1"), None);
        let selected = protocol("0.3.2");
        for (registry, version, expected) in [
            (&historical, None, &["versioning.protocol-unpublished"][..]),
            (
                &historical,
                Some(&selected),
                &["versioning.unsupported-version"][..],
            ),
            (published, None, &["versioning.protocol-unpublished"][..]),
            (published, Some(&selected), &[][..]),
        ] {
            let verdict =
                CompatibilityPreflight::check(registry, &ir("0.2.16"), version, &manifest());
            assert_eq!(verdict.reasons(), expected);
            assert_eq!(verdict.is_compatible(), expected.is_empty());
        }
    }

    #[test]
    fn required_extensions_are_never_satisfied_by_the_accepted_ir() {
        let registry = VersionRegistry::embedded().expect("valid");
        let mut extended = manifest();
        extended.required_extensions = vec!["future-ext".to_owned()];
        let verdict = CompatibilityPreflight::check(
            registry,
            &ir("0.2.16"),
            Some(&protocol("0.2.16")),
            &extended,
        );
        assert!(verdict
            .reasons()
            .contains(&"versioning.extension-incompatible"));
    }

    #[test]
    fn unregistered_protocol_and_inverted_ranges_refuse() {
        let registry = VersionRegistry::embedded().expect("valid");
        // A protocol version outside the published exact set: unsupported.
        let verdict = CompatibilityPreflight::check(
            registry,
            &ir("0.2.16"),
            Some(&protocol("9.9.9")),
            &manifest(),
        );
        assert!(verdict
            .reasons()
            .contains(&"versioning.unsupported-version"));

        // Inverted IR range is a manifest fault.
        assert_eq!(
            AdapterCompatibilityManifest::new(
                registry_version("0.2.16"),
                "example-adapter",
                ir("0.3.0"),
                ir("0.2.16"),
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
                ir("0.2.16"),
                ir("0.2.16"),
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
                registry_version("0.2.16"),
                "bad id!",
                ir("0.2.16"),
                ir("0.2.16"),
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
        assert_eq!(report.registry_version, "0.2.16");
        let names: Vec<&str> = report.families.iter().map(|f| f.family).collect();
        assert_eq!(names, ["model", "ir", "protocol"]);
        let model_family: &CompatibilityReportFamilyAlias = &report.families[0];
        assert_eq!(model_family.current.as_deref(), Some("0.2.16"));
    }

    type CompatibilityReportFamilyAlias = lekalo_core::versioning::compatibility::FamilySummary;
}

mod digests {
    use lekalo_core::digest::sha256_hex;

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

#[test]
fn baseline_rejects_old_model_and_undeclared_migration() {
    let registry = VersionRegistry::embedded().unwrap();
    assert_eq!(registry.model().versions().len(), 1);
    assert_eq!(registry.model().current().unwrap().as_str(), "0.2.16");
    for version in ["0.1.0", "1.0.0", "0.2.15"] {
        assert!(registry.model().record(&model(version)).is_none());
    }
    let mut raw: serde_json::Value =
        serde_json::from_slice(lekalo_core::versioning::REGISTRY_BYTES).unwrap();
    raw["families"]["model"]["migrations"] = serde_json::json!([{"id":"invented"}]);
    assert!(VersionRegistry::from_bytes(&serde_json::to_vec(&raw).unwrap()).is_err());
}
