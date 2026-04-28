//! `caixa-core` — manifest types, layout invariants, and version contract
//! for the caixa tatara-lisp package system.
//!
//! The `caixa.lisp` manifest is itself a [`tatara_lisp::domain::TataraDomain`]
//! — parsing is the derive macro, which makes ill-typed manifests impossible
//! at load time (the same discipline Cargo gets from `Cargo.toml`, but
//! enforced by Rust types rather than TOML schema).
//!
//! Layout invariants (lib/ presence, exe/ population, service entries) are
//! enforced by [`LayoutInvariants`], run by `caixa-feira` before any build
//! step.

extern crate self as caixa_core;

pub mod behavior;
pub mod dep;
pub mod kind;
pub mod layout;
pub mod limits;
pub mod manifest;
pub mod supervisor;
pub mod upgrade;
pub mod version;

pub use behavior::BehaviorSpec;
pub use dep::{Dep, DepSource};
pub use kind::CaixaKind;
pub use layout::{LayoutError, LayoutInvariants, StandardLayout};
pub use limits::{LimitsError, LimitsSpec};
pub use manifest::Caixa;
pub use supervisor::{
    ChildSpec, RestartPolicy, RestartStrategy, SupervisorError, SupervisorSpec,
};
pub use upgrade::{UpgradeError, UpgradeFromEntry, UpgradeInstruction};
pub use version::{CaixaVersion, VersionError, parse_requirement};
