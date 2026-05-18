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
pub use dep::{Dep, DepError, DepSource};
pub use kind::CaixaKind;
pub use layout::{LayoutError, LayoutInvariants, StandardLayout};
pub use limits::{LimitsError, LimitsSpec};
pub use manifest::{Caixa, ManifestError};
pub use render::{
    DNS_1123_LABEL_MAX_LEN, GATEWAY_API_HTTP_PATH_MAX_LEN, KUBE_KEY_API_VERSION, KUBE_KEY_KIND,
    KUBE_KEY_LABELS, KUBE_KEY_MATCH_LABELS, KUBE_KEY_METADATA, KUBE_KEY_NAME, KUBE_KEY_NAMESPACE,
    KindMismatch, LABEL_APLICACAO, LABEL_CONTRATO, LABEL_PROGRAM, M2_KEY_BEHAVIOR, M2_KEY_LIMITS,
    M2_KEY_UPGRADE_FROM, M3_KEY_PLACEMENT, NATS_SUBJECT_MAX_LEN, PLEME_LABEL_PREFIX, RenderError,
    WASI_KV_SLOT_MAX_LEN, WIT_IDENT_MAX_LEN, is_dns_1123_label, is_gateway_api_http_path,
    is_nats_subject, is_wasi_keyvalue_slot, is_wit_world_ref, kube_resource_skeleton,
    label_selector, pleme_program_in_aplicacao_selector, pleme_program_selector, require_kind,
    servico_m2_overlay, single_field_overlay, yaml_string_mapping,
};
pub use supervisor::{ChildSpec, RestartPolicy, RestartStrategy, SupervisorError, SupervisorSpec};
pub use upgrade::{UpgradeError, UpgradeFromEntry, UpgradeInstruction, validate_upgrade_from};
pub use version::{CaixaVersion, VersionError, parse_requirement};
