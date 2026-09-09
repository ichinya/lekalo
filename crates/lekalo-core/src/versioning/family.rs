//! Sealed contract families (issue #9).
//!
//! A [`ContractFamily`] is a zero-cost marker that binds a
//! [`ContractVersion`](super::ContractVersion) to exactly one evolving
//! contract. The trait is sealed: library callers can match on families but
//! cannot invent new ones, so a Model version can never be smuggled into an
//! IR or protocol slot. The families are closed; adding one is a reviewed,
//! breaking registry change.

use std::fmt;

/// Marker trait for the closed set of Lekalo contract families.
///
/// Sealed via the private [`Sealed`] supertrait; the only implementors are
/// the four public marker types in this module.
pub trait ContractFamily: private::Sealed + Copy + Eq + fmt::Debug {
    /// The stable lowercase family wire name.
    const NAME: &'static str;
}

mod private {
    /// Seal [`super::ContractFamily`] to this module's marker types.
    pub trait Sealed {}
    impl Sealed for super::ModelContract {}
    impl Sealed for super::IrContract {}
    impl Sealed for super::ProtocolContract {}
    impl Sealed for super::RegistryContract {}
}

/// The user-facing Lekalo Model schema family (`model`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelContract;

/// The compiled, target-neutral Lekalo IR family (`ir`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IrContract;

/// The target/provider process protocol family (`protocol`).
///
/// Published by issue #27 as `lekalo.target/v1` (`dev.lekalo.protocol@1.0.0`,
/// selector alias `protocol/v1`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtocolContract;

/// The versioning machine-artifact family (`registry`): the version
/// registry itself, the adapter compatibility manifest schema, and the
/// migration result schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistryContract;

impl ContractFamily for ModelContract {
    const NAME: &'static str = "model";
}

impl ContractFamily for IrContract {
    const NAME: &'static str = "ir";
}

impl ContractFamily for ProtocolContract {
    const NAME: &'static str = "protocol";
}

impl ContractFamily for RegistryContract {
    const NAME: &'static str = "registry";
}

impl From<crate::loader::ModelVersion> for super::ContractVersion<ModelContract> {
    /// The exhaustive mapping from the accepted #7 finite Model versions.
    /// Adding a Model version is a breaking change that must extend this
    /// match and the registry data together.
    fn from(version: crate::loader::ModelVersion) -> Self {
        use crate::loader::ModelVersion;
        let text = match version {
            ModelVersion::V0_1_0 => "0.1.0",
            ModelVersion::V1_0_0 => "1.0.0",
        };
        // The literals are compile-time constants proven canonical by tests.
        Self::parse_canonical(text).expect("finite Model literals are canonical")
    }
}

impl super::ContractVersion<IrContract> {
    /// The one accepted #8 IR contract version (`dev.lekalo.ir@0.1.0`).
    pub fn current() -> Self {
        // `crate::ir::VERSION` is a repository constant; its canonical
        // spelling is asserted by a unit test in this module.
        Self::parse_canonical(crate::ir::VERSION).expect("IR constant is canonical")
    }
}
