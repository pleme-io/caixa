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

pub mod aplicacao;
pub mod behavior;
pub mod dep;
pub mod kind;
pub mod layout;
pub mod limits;
pub mod manifest;
pub mod render;
pub mod supervisor;
pub mod upgrade;
pub mod version;

pub use aplicacao::{
    AplicacaoError, AplicacaoSpec, CircuitBreaker, Entrada, Membro, MeshPolicy, Placement,
    PlacementStrategy, RateLimit, WitContract, WitTarget,
};
pub use behavior::{BehaviorError, BehaviorSpec};
pub use dep::{Dep, DepSource};
pub use kind::CaixaKind;
pub use layout::{LayoutError, LayoutInvariants, StandardLayout};
pub use limits::{LimitsError, LimitsSpec};
pub use manifest::Caixa;
pub use render::{
    KUBE_KEY_API_VERSION, KUBE_KEY_KIND, KUBE_KEY_LABELS, KUBE_KEY_METADATA, KUBE_KEY_NAME,
    KUBE_KEY_NAMESPACE, LABEL_APLICACAO, LABEL_CONTRATO, LABEL_PROGRAM, M2_KEY_BEHAVIOR,
    M2_KEY_LIMITS, M2_KEY_UPGRADE_FROM, PLEME_LABEL_PREFIX, RenderError, kube_resource_skeleton,
    pleme_program_in_aplicacao_selector, pleme_program_selector, servico_m2_overlay,
    yaml_string_mapping,
};
pub use supervisor::{ChildSpec, RestartPolicy, RestartStrategy, SupervisorError, SupervisorSpec};
pub use upgrade::{UpgradeError, UpgradeFromEntry, UpgradeInstruction};
pub use version::{CaixaVersion, VersionError, parse_requirement};
