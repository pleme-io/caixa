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
    AplicacaoError, AplicacaoSpec, CircuitBreaker, DEFAULT_SERVICO_PORT, Entrada, Membro,
    MeshPolicy, POLICY_BREAKER_MAX_FAILURES_MAX, POLICY_BREAKER_WINDOW_MAX, POLICY_RATE_LIMIT_MAX,
    POLICY_RETRIES_MAX, POLICY_TIMEOUT_MAX, Placement, PlacementStrategy, RateLimit, WitContract,
    WitTarget,
};
pub use behavior::{BehaviorError, BehaviorSpec};
pub use dep::{Dep, DepError, DepSource};
pub use kind::CaixaKind;
pub use layout::{LayoutError, LayoutInvariants, StandardLayout};
pub use limits::{
    LIMITS_CPU_MILLICORES_MAX, LIMITS_FUEL_MAX, LIMITS_MEMORY_WASM32_MAX_BYTES,
    LIMITS_MEMORY_WASM32_PAGE_BYTES, LIMITS_WALL_CLOCK_MAX, LimitsError, LimitsSpec,
};
pub use manifest::{Caixa, ManifestError};
pub use render::{
    CARGO_FEATURE_NAME_MAX_LEN, CILIUM_API_VERSION, CILIUM_KEY_AUTHENTICATION,
    CILIUM_KEY_ENDPOINT_SELECTOR, CILIUM_KEY_FROM_ENDPOINTS, CILIUM_KEY_INGRESS, CILIUM_KEY_PORTS,
    CILIUM_KEY_TO_PORTS, CILIUM_KIND_NETWORK_POLICY, COMPUTEUNIT_YAML_SUFFIX,
    DEFAULT_FLUX_SYSTEM_NAMESPACE, DEFAULT_LIBRARY_NAME, DEFAULT_NAMESPACE, DNS_1123_LABEL_MAX_LEN,
    FLEET_PROGRAMS_KEY_PROGRAMS, FLUX_GITREPOSITORY_API_VERSION, FLUX_HELMRELEASE_API_VERSION,
    FLUX_KIND_GIT_REPOSITORY, FLUX_KIND_HELM_RELEASE, FLUX_KIND_KUSTOMIZATION,
    FLUX_KUSTOMIZATION_API_VERSION, GATEWAY_API_API_VERSION, GATEWAY_API_HTTP_PATH_MAX_LEN,
    GATEWAY_API_KEY_BACKEND_REFS, GATEWAY_API_KEY_HOSTNAME, GATEWAY_API_KEY_HOSTNAMES,
    GATEWAY_API_KEY_LISTENERS, GATEWAY_API_KEY_PARENT_REFS, GATEWAY_API_KEY_TIMEOUTS,
    GATEWAY_API_KIND_GATEWAY, GATEWAY_API_KIND_HTTP_ROUTE, GIT_OID_SHA1_LEN, GIT_OID_SHA256_LEN,
    GIT_REF_NAME_MAX_LEN, GIT_REPO_URL_MAX_LEN, HELM_CHART_API_VERSION, KUBE_KEY_API_VERSION,
    KUBE_KEY_KIND, KUBE_KEY_LABELS, KUBE_KEY_MATCH_LABELS, KUBE_KEY_METADATA, KUBE_KEY_NAME,
    KUBE_KEY_NAMESPACE, KUBE_KEY_PORT, KUBE_KEY_PROTOCOL, KUBE_KEY_RULES, KUBE_KEY_SPEC,
    KindMismatch, LABEL_APLICACAO, LABEL_CONTRATO, LABEL_PROGRAM, LAREIRA_CHART_NAME_NOME_MAX_LEN,
    LAREIRA_CHART_NAME_PREFIX, LISP_SOURCE_EXTENSION, M2_KEY_BEHAVIOR, M2_KEY_LIMITS,
    M2_KEY_UPGRADE_FROM, M3_KEY_PLACEMENT, NATS_SUBJECT_MAX_LEN, PLEME_LABEL_PREFIX,
    PathShapeViolation, RenderError, ServicoCountMismatch, WASI_KV_SLOT_MAX_LEN, WIT_IDENT_MAX_LEN,
    find_ascii_whitespace_byte, find_non_ascii_whitespace_char, is_cargo_feature_name,
    is_computeunit_yaml_extension, is_digit_only_magnitude, is_dns_1123_label,
    is_gateway_api_http_path, is_git_oid, is_git_ref_name, is_git_repo_url,
    is_lareira_chart_name_shape, is_leading_zero_padded_magnitude, is_lisp_extension,
    is_nats_subject, is_sandboxed_relative_path, is_wasi_keyvalue_slot, is_wit_world_ref,
    kube_resource_skeleton, label_selector, lareira_chart_name,
    pleme_program_in_aplicacao_selector, pleme_program_selector, require_kind,
    require_single_servico, servico_m2_overlay, single_field_overlay, yaml_string_mapping,
};
pub use supervisor::{
    ChildSpec, RestartPolicy, RestartStrategy, SUPERVISOR_MAX_RESTARTS_MAX,
    SUPERVISOR_RESTART_WINDOW_MAX, SupervisorError, SupervisorSpec,
};
pub use upgrade::{
    UpgradeError, UpgradeFromEntry, UpgradeInstruction, validate_upgrade_from,
    validate_upgrade_from_against_behavior, validate_upgrade_from_against_versao,
};
pub use version::{
    CaixaVersion, DEFAULT_GIT_REMOTE, DEFAULT_PUBLISH_TAG_PREFIX, VersionError, parse_requirement,
};
