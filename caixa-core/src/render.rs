//! Render-side helpers shared by every per-Servico renderer
//! ([`caixa-helm`], [`caixa-flux`]) — the canonical place for "if the
//! M2 typed slot is non-empty, emit its camelCase YAML fragment under
//! the agreed key" patterns to live exactly once.
//!
//! Until this module landed both renderers carried an inline ~20-line
//! block per render entry-point that:
//!
//! 1. Checked `caixa.limits.is_some() && !limits.is_empty()`.
//! 2. Called `serde_yaml::to_value(limits).unwrap_or(Value::Null)` —
//!    silently swallowing every serialization error as a `null`-shaped
//!    fragment that would render as `limits: null` in the values block,
//!    indistinguishable from "the author omitted the slot" downstream.
//! 3. Inserted under the camelCase key `"limits"` with `or_insert`
//!    semantics so explicit `spec.*` fields from the ComputeUnit YAML
//!    take precedence over the manifest-derived overlay.
//! 4. Repeated the same shape for `:behavior` → `"behavior"` and
//!    `:upgrade-from` → `"upgradeFrom"`.
//!
//! That's the duplication budget violated three ways: same emptiness
//! check, same camelCase key, same precedence rule, written twice
//! verbatim. THEORY.md §I.3.5 ("Generation first, composition second,
//! hand-authoring last; the duplication budget is zero") promotes that
//! to a build-time concern: every recurring shape lives in a typed
//! helper before its third occurrence — and PRIME DIRECTIVE work is
//! exactly that lift.
//!
//! [`servico_m2_overlay`] is that helper. Renderers iterate the map it
//! returns and merge each `(key, value)` pair into their target with
//! their own map type's `entry().or_insert()` (so `spec.*` precedence
//! is preserved by construction).

use std::collections::BTreeMap;
use std::path::{Component, Path};
use thiserror::Error;

use crate::{Caixa, CaixaKind};

/// Errors the render helpers can raise.
#[derive(Debug, Error)]
pub enum RenderError {
    /// `serde_yaml::to_value` failed for one of the M2 typed slots —
    /// theoretically impossible for the canonical
    /// [`crate::LimitsSpec`] / [`crate::BehaviorSpec`] /
    /// [`crate::UpgradeFromEntry`] types (all derive Serialize without
    /// fallible custom impls), but surfaced rather than swallowed so a
    /// future slot whose Serialize impl gains a fallible branch
    /// surfaces the failure to the caller instead of silently rendering
    /// as `null` (the prior inline block's behavior).
    #[error("yaml serialization of M2 slot {slot}: {source}")]
    Yaml {
        slot: &'static str,
        #[source]
        source: serde_yaml::Error,
    },
}

/// Typed kind-mismatch view: the canonical surface every per-kind
/// `caixa-<target>` renderer raises when it's handed a [`Caixa`] whose
/// `:kind` doesn't match the kind that renderer is targeting. Carries
/// the offending caixa's `:nome` alongside the expected/actual kinds,
/// so the diagnostic reads `caixa "<nome>": expected :kind <expected>,
/// got <actual>` — naming which caixa needs author attention, not just
/// which kind the renderer rejected.
///
/// Lifted from three identical-shape per-renderer arms in
/// `caixa-helm` ([`Error::NotAServico`][helm-err]), `caixa-flux`
/// ([`Error::NotAServico`][flux-err]) and `caixa-mesh`
/// ([`Error::NotAnAplicacao`][mesh-err]). The prior arms each carried
/// only the actual [`CaixaKind`], leaving the user to grep for which
/// `caixa.lisp` triggered the mismatch — exactly the
/// "feira verb whose error path doesn't name the offending caixa"
/// punch-list item the compounding-mandate protocol calls out.
///
/// Renderers wrap this view in their own [`thiserror`] `Error` enum
/// via `#[from]`; the `?` operator at every kind-checking call site
/// turns the [`require_kind`] result into the renderer's local error
/// type with no manual conversion.
///
/// [helm-err]: https://docs.rs/caixa-helm
/// [flux-err]: https://docs.rs/caixa-flux
/// [mesh-err]: https://docs.rs/caixa-mesh
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("caixa {nome:?}: expected :kind {expected:?}, got {actual:?}")]
pub struct KindMismatch {
    /// The offending caixa's `:nome` — names which `caixa.lisp` the
    /// renderer was handed, so the diagnostic doesn't require the
    /// user to grep for it.
    pub nome: String,
    /// The `:kind` this renderer targets.
    pub expected: CaixaKind,
    /// The `:kind` the offending caixa actually carries.
    pub actual: CaixaKind,
}

/// Predicate: assert that `caixa.kind == expected`, returning a typed
/// [`KindMismatch`] view (carrying [`Caixa::nome`]) on rejection. The
/// canonical entry-point every per-kind renderer wraps in its own
/// [`thiserror`] `Error` variant via `#[from]` — the call site
/// becomes a single `caixa_core::require_kind(caixa, CaixaKind::X)?;`
/// in place of the prior inline `if caixa.kind != CaixaKind::X {
/// return Err(Error::NotAnX(caixa.kind)); }` block.
///
/// Lifted to a single helper so a future per-kind renderer
/// (`caixa-otel`, the future per-Aplicacao CR materializer the M3.x
/// roadmap acknowledges, the future per-Supervisor reconciler
/// renderer) gets the same naming-the-offending-caixa diagnostic for
/// free, and a future change to the diagnostic format (e.g. adding
/// a [`Caixa::versao`] suffix once multi-version-skew authoring lands)
/// is one edit here, not a coordinated rewrite of every renderer.
///
/// # Errors
///
/// Returns [`KindMismatch`] when `caixa.kind != expected`. The error
/// carries the caixa's `:nome` so the diagnostic names the offending
/// `caixa.lisp` — same shape every renderer's `Error::From<KindMismatch>`
/// converts into the renderer's local error type.
pub fn require_kind(caixa: &Caixa, expected: CaixaKind) -> Result<(), KindMismatch> {
    if caixa.kind == expected {
        Ok(())
    } else {
        Err(KindMismatch {
            nome: caixa.nome.clone(),
            expected,
            actual: caixa.kind,
        })
    }
}

/// Typed `:servicos`-count-mismatch view: the canonical surface every
/// per-Servico `caixa-<target>` renderer raises when it's handed a
/// [`Caixa`] whose `:servicos` list doesn't carry exactly one entry —
/// the V0 contract every Servico-kind caixa satisfies (`caixa-helm`'s
/// `render_chart_for_servico`, `caixa-flux`'s `programs_yaml_entry`, the
/// future per-Servico OCI/wasm packager). Carries the offending caixa's
/// `:nome` alongside the actual count, so the diagnostic reads `caixa
/// "<nome>": :servicos must declare exactly one entry for V0 (got
/// <count>)` — naming which `caixa.lisp` needs author attention, not
/// just the count the renderer rejected.
///
/// Lifted from two identical-shape per-renderer arms in
/// [`caixa-helm`][helm-err] and [`caixa-flux`][flux-err]
/// (`Error::UnsupportedServicoCount(usize)`). The prior arms each
/// carried only the actual count, leaving the user to grep for which
/// `caixa.lisp` triggered the mismatch — exactly the "feira verb whose
/// error path doesn't name the offending caixa" punch-list item the
/// compounding-mandate protocol calls out. Same trajectory as
/// [`KindMismatch`] (which lifted the prior `NotAServico(CaixaKind)` /
/// `NotAnAplicacao(CaixaKind)` per-renderer arms into a typed view
/// naming the offending caixa).
///
/// Renderers wrap this view in their own [`thiserror`] `Error` enum
/// via `#[from]`; the `?` operator at every count-checking call site
/// turns the [`require_single_servico`] result into the renderer's
/// local error type with no manual conversion. Peer to [`require_kind`]
/// on the V0 Servico-shape gate axis (the kind gate refuses the wrong
/// `:kind`; this gate refuses the wrong `:servicos` count) — every
/// per-Servico renderer chains both at its entry point.
///
/// [helm-err]: https://docs.rs/caixa-helm
/// [flux-err]: https://docs.rs/caixa-flux
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("caixa {nome:?}: :servicos must declare exactly one entry for V0 (got {count})")]
pub struct ServicoCountMismatch {
    /// The offending caixa's `:nome` — names which `caixa.lisp` the
    /// renderer was handed, so the diagnostic doesn't require the
    /// user to grep for it.
    pub nome: String,
    /// The `:servicos` list length the offending caixa actually carries.
    /// The expected count is fixed at 1 by the V0 contract — every
    /// `:kind Servico` caixa declares exactly one `ComputeUnit` YAML
    /// pointer, matching the one Helm chart / one programs.yaml entry
    /// each renderer emits.
    pub count: usize,
}

/// Predicate: assert that `caixa.servicos.len() == 1`, returning a typed
/// [`ServicoCountMismatch`] view (carrying [`Caixa::nome`] + the actual
/// count) on rejection. The canonical entry-point every per-Servico
/// renderer wraps in its own [`thiserror`] `Error` variant via
/// `#[from]` — the call site becomes a single
/// `caixa_core::require_single_servico(caixa)?;` in place of the prior
/// inline `if caixa.servicos.len() != 1 { return
/// Err(Error::UnsupportedServicoCount(caixa.servicos.len())); }`
/// block.
///
/// Lifted to a single helper so the V0 `:servicos`-singularity invariant
/// — the same shape the [`crate::Caixa::validate_code_paths`] doc
/// comment already names as load-bearing on caixa-helm + caixa-flux
/// (caixa-core/src/manifest.rs:4108) — lives in exactly one place across
/// every per-Servico renderer. A future per-Servico renderer
/// (`caixa-otel`, the future per-Servico OCI packager, the future M4
/// `wasm.pleme.io/v1alpha1/ComputeUnit` CR materializer) gets the same
/// naming-the-offending-caixa diagnostic for free, and a future change
/// to the V0 invariant (e.g. allowing multi-servico Servicos when the
/// component-model multi-world boundary lands in M5) is one edit here,
/// not a coordinated rewrite of every renderer's per-arm
/// `UnsupportedServicoCount` check.
///
/// Same trajectory as [`require_kind`] / [`KindMismatch`] on the peer
/// V0 Servico-shape axis: every per-Servico renderer reaches for one
/// `caixa_core::require_*` helper per V0 invariant, so the diagnostic
/// shape (named caixa, named field) is uniform across the substrate.
///
/// # Errors
///
/// Returns [`ServicoCountMismatch`] when `caixa.servicos.len() != 1`
/// (both empty and ≥ 2 land on this arm — the V0 contract requires
/// *exactly* one entry, not *at-least* one). The error carries the
/// caixa's `:nome` + the offending count so the diagnostic names the
/// offending `caixa.lisp` — same shape every renderer's
/// `Error::From<ServicoCountMismatch>` converts into the renderer's
/// local error type.
pub fn require_single_servico(caixa: &Caixa) -> Result<(), ServicoCountMismatch> {
    if caixa.servicos.len() == 1 {
        Ok(())
    } else {
        Err(ServicoCountMismatch {
            nome: caixa.nome.clone(),
            count: caixa.servicos.len(),
        })
    }
}

/// K8s DNS-1123 label rule's max length, in bytes — the floor each
/// apiserver-side schema enforces independently on every `metadata.name`
/// / Service name / label value axis a validated identifier lands in.
///
/// Per-axis breakdown of why 63 is the strictest among the rules each
/// validated DNS-1123-label-shaped identifier passes through:
///
///   * `:membros :caixa` lands as the rendered programs.yaml entry's
///     `name:` (consumed by `lareira-fleet-programs` to derive the
///     `wasm.pleme.io/v1alpha1/ComputeUnit.metadata.name`), as the K8s
///     [`Service`][svc] `metadata.name` the future `app-operator`
///     provisions per-member (DNS-1035 label rule:
///     `[a-z]([-a-z0-9]*[a-z0-9])?` max 63), as the
///     [`LABEL_PROGRAM`] label value (K8s label value rule:
///     `[a-z0-9]([-a-z0-9_.]*[a-z0-9])?` max 63), and as a component of
///     the composed `<aplicacao>-<de>-to-<para>` `CiliumNetworkPolicy`
///     `metadata.name`.
///   * `:placement :clusters` lands as the K8s context name keying
///     every per-cluster `kubeconfig`, as the `clusters[]` filter the
///     `lareira-fleet-programs` aggregator applies to scope programs
///     to their owning cluster, and as the namespace prefix /
///     `cluster.x-k8s.io/v1beta1/Cluster.metadata.name` cluster
///     identity the future M4 cross-cluster fan-out emits per entry —
///     all DNS-1123-label territory.
///   * `:children :caixa` lands as the rendered
///     `wasm.pleme.io/v1alpha1/ComputeUnit.metadata.name` per child the
///     supervisor materializes, as the [`LABEL_PROGRAM`] label value on
///     every emitted child's pod identity, and as the per-child
///     [`Service`][svc] `metadata.name` the future wasm-operator
///     provisions — every K8s apiserver-side schema on each landing site
///     enforces the same DNS-1123 label rule on admission.
///
/// Lifted to one const so a future identifier axis reaching for the
/// same rule (the future per-Servico `:nome` gate at the Caixa-load
/// boundary, the M4 `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's
/// per-member / per-cluster validators, the future per-Aplicacao
/// `:nome` gate when `feira init` lands DNS-1123 enforcement on the
/// scaffold's `--nome` flag) reads the limit from one place.
///
/// [svc]: https://kubernetes.io/docs/concepts/services-networking/service/
pub const DNS_1123_LABEL_MAX_LEN: usize = 63;

/// Predicate: assert that `s` is a valid K8s DNS-1123 label. The
/// contract — exactly the regex the K8s apiserver enforces on every
/// `metadata.name` / Service name / label value via OpenAPI v3 admission
/// validation, `[a-z0-9]([-a-z0-9]*[a-z0-9])?` with a 63-byte cap:
///
///   - 1..=63 bytes ([`DNS_1123_LABEL_MAX_LEN`] cap);
///   - lowercase ASCII alphanumeric + hyphen (`[a-z0-9-]` only; no
///     uppercase — K8s rejects, no underscore — DNS-1123 forbids, no
///     dot — a single label is not a subdomain, no Unicode/IDN — must
///     be pre-encoded);
///   - non-hyphen ASCII alphanumeric at both label boundaries
///     (no `-foo`, no `foo-`).
///
/// Returns the parser-shaped reason on rejection (without wrapping in
/// any error variant) so each per-axis caller — `validate_membro_caixa`
/// for `:membros :caixa`, `validate_placement_cluster` for
/// `:placement :clusters`, `validate_child_caixa` for `:children :caixa`,
/// every future per-axis lift (the per-Servico `:nome` gate at the
/// Caixa-load boundary, the M4 CR materializer's per-member /
/// per-cluster validators) — wraps the same reason in its own typed
/// `*Error::*Invalid { <axis>, reason }` variant. The reason wording is
/// axis-agnostic ("DNS-1123 labels allow only `[a-z0-9-]`") so every
/// call site reading the same diagnostic points at the same rule —
/// drift between any two axes' rule enforcement is a build error
/// visible at this predicate, not a per-renderer "this passed validate
/// but failed admission" surprise.
///
/// Empty input is rejected at the call site (each axis has its own
/// narrower `*Empty` variant — [`crate::AplicacaoError::MembroCaixaEmpty`],
/// [`crate::AplicacaoError::PlacementClusterEmpty`],
/// [`crate::SupervisorError::EmptyChildName`]) before this predicate
/// is consulted, mirroring `validate_entrada_host`'s empty-first
/// cascade (c7d05ec).
///
/// Lifted from `caixa-core::aplicacao` (where it was first inlined for
/// `:membros :caixa` in 3f9d7a0 and then reused for `:placement :clusters`
/// in 6cbb900) so the third axis reaching for the rule (`:children
/// :caixa` on the supervisor tree) lands as a thin five-line wrapper
/// rather than re-inlining 40 lines of regex enforcement. The
/// "before its third occurrence" boundary the PRIME DIRECTIVE
/// duplication-budget rule draws (THEORY.md §I.3.5: "the duplication
/// budget is zero") promotes the predicate to a typed substrate-side
/// primitive on the same trajectory the M2-overlay and label-selector
/// helpers (9e3a057, 9d09cfb, 9dbeafd, 31455a7, 07a4544) already follow.
///
/// # Errors
///
/// Returns the parser-shaped reason naming the specific violation
/// (length / boundary / character-class), without wrapping in any
/// error variant — every caller maps the same `String` into its own
/// typed `*Invalid { <axis>, reason }` enum variant.
pub fn is_dns_1123_label(s: &str) -> Result<(), String> {
    if s.len() > DNS_1123_LABEL_MAX_LEN {
        return Err(format!(
            "exceeds DNS-1123 label max length of {DNS_1123_LABEL_MAX_LEN} bytes \
             (got {} bytes; the K8s apiserver rejects longer names at admission \
             time on every Service / Pod / CR `metadata.name` axis)",
            s.len()
        ));
    }
    let bytes = s.as_bytes();
    if !bytes[0].is_ascii_alphanumeric() || !bytes[bytes.len() - 1].is_ascii_alphanumeric() {
        return Err("must start and end with an ASCII alphanumeric character \
                    (no leading or trailing `-`; DNS-1123 label rule)"
            .to_string());
    }
    for &b in bytes {
        let valid = b.is_ascii_digit() || b.is_ascii_lowercase() || b == b'-';
        if !valid {
            let msg = if b.is_ascii_uppercase() {
                format!(
                    "contains uppercase character {ch:?} (K8s DNS-1123 label \
                     names are lowercase-only; use {lower:?})",
                    ch = b as char,
                    lower = s.to_ascii_lowercase()
                )
            } else if b == b'_' {
                "contains `_` (DNS-1123 labels allow only `[a-z0-9-]`; use `-` \
                 instead)"
                    .to_string()
            } else if b == b'.' {
                "contains `.` (a single DNS-1123 label is not a subdomain; \
                 split into separate entries or use `-` to namespace)"
                    .to_string()
            } else {
                format!(
                    "contains invalid character {ch:?} (DNS-1123 labels allow \
                     only `[a-z0-9-]`)",
                    ch = b as char
                )
            };
            return Err(msg);
        }
    }
    Ok(())
}

/// K8s Gateway API v1 `HTTPPathMatch.value` max length, in bytes —
/// the apiserver-side `OpenAPI` schema's `maxLength: 1024` cap. Lifted
/// to a typed const so a future axis reaching for the same bound (the
/// M4 `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's per-path
/// validator, the future per-`HTTPRouteRule` per-path-match emission
/// when M4 lands per-rule overrides, the future `:politicas`-derived
/// per-edge HTTP path overlay's per-path validator) reads the limit
/// from one place. The two landed call sites — `:entrada :paths`
/// entries (caixa-mesh's `HTTPRoute.spec.rules[].matches[].path.value`
/// emission) and `:contratos :endpoint` (caixa-mesh's Cilium L7
/// `path:` rule emission, caixa-mesh/src/lib.rs:311) — both inherit
/// the same cap; drift between either landing site and the K8s CRD
/// schema surfaces at this one const.
pub const GATEWAY_API_HTTP_PATH_MAX_LEN: usize = 1024;

/// Predicate: assert that `path` is a valid HTTP path under both the
/// K8s Gateway API v1 `HTTPPathMatch.value` admission grammar AND the
/// Cilium L7 `path:` rule grammar — the two landing sites every
/// validated pleme-io HTTP-shaped path lands in. The contract:
///
///   - 1..=[`GATEWAY_API_HTTP_PATH_MAX_LEN`] (1024) bytes;
///   - leading `/` (the `PathPrefix` invariant — pre-checked at the
///     call site by each axis's narrower `*NotAbsolute` variant;
///     re-checked here so the predicate is usable from any future
///     call site without a shape-mismatch footgun);
///   - no consecutive `/` characters (HTTP path matchers reject
///     `//` — collapse to a single `/`);
///   - no `/./` or `/../` segments (and no trailing `/.` or `/..`) —
///     path-traversal and no-op segments are rejected outright;
///   - no `?` (query separator: queries are matched separately via
///     `HTTPRoute` `queryParams`, never in the path);
///   - no `#` (fragment separator: fragments are client-side and
///     never reach the gateway);
///   - no whitespace (space, tab — must be percent-encoded as `%20`);
///   - no ASCII control characters (`0x00..0x1F`, `0x7F`);
///   - no non-ASCII bytes (`>= 0x80`) — RFC 3986 requires `%XX`
///     percent-encoding for anything outside the ASCII unreserved +
///     reserved set;
///   - no printable-ASCII byte outside the K8s Gateway API
///     `HTTPPathMatch.value` apiserver-side `OpenAPI` regex
///     `^(?:[-A-Za-z0-9/._~!$&'()*+,;=:@]|[%][0-9a-fA-F]{2})+$`
///     accepted set — namely `"` `<` `>` `[` `\` `]` `^` `` ` `` `{`
///     `|` `}`. These eleven bytes are printable ASCII but RFC 3986's
///     `pchar = unreserved / pct-encoded / sub-delims / ":" / "@"`
///     grammar excludes them, so the apiserver rejects them at
///     admission time on every `HTTPRoute.spec.rules[].matches[].
///     path.value` landing site and the Cilium L7 path matcher
///     refuses them too. Percent-encode (`%XX`) if the literal byte
///     is intended.
///
/// Returns the parser-shaped reason on rejection (without wrapping in
/// any error variant) so each per-axis caller — `validate_entrada_path`
/// for `:entrada :paths` entries, `WitContract::target` for the HTTP-
/// shaped `:contratos :endpoint` axis, every future per-path lift
/// (the M4 CR materializer's per-path validator, the future
/// per-`HTTPRouteRule` per-path-match emission) — wraps the same
/// reason in its own typed `*Invalid { <axis>, reason }` variant. The
/// reason wording is axis-agnostic ("HTTP path matchers reject
/// `//`") so every call site reading the same diagnostic points at
/// the same rule; drift between any two axes' rule enforcement is a
/// build error visible at this predicate, not a per-renderer "this
/// passed validate but failed admission" surprise.
///
/// Empty input is rejected at the call site (each axis has its own
/// narrower `*Empty` variant — [`crate::AplicacaoError::EntradaPathEmpty`],
/// [`crate::AplicacaoError::ContratoEndpointEmpty`]) before this
/// predicate is consulted, mirroring `is_dns_1123_label`'s empty-first
/// cascade. The predicate body re-checks empty + leading-`/`
/// defensively so it can be called from any future call site without
/// a shape-mismatch footgun.
///
/// Lifted from `caixa-core::aplicacao::validate_entrada_path` (where
/// it was first inlined for `:entrada :paths` in 55410e4) at the
/// second occurrence of the HTTP-path-grammar — the `:contratos
/// :endpoint` axis (c4213a4 gated non-empty + leading-`/` only,
/// silently passing the same authoring footguns the `:entrada :paths`
/// gate catches) — so the second axis lands as a thin three-line
/// wrapper at the per-axis call site rather than re-inlining 90 lines
/// of grammar enforcement. Same compounding shape as
/// `is_dns_1123_label` (lifted at its third occurrence in 31bfa43)
/// and the M2-overlay / label-selector helpers (9e3a057, 9d09cfb,
/// 9dbeafd, 31455a7, 07a4544) on the render side — each lifted a
/// recurring shape into a typed primitive at the threshold where the
/// duplication budget would otherwise have been exceeded.
///
/// # Errors
///
/// Returns the parser-shaped reason naming the specific violation
/// (length / character-class / segment / consecutive-slash), without
/// wrapping in any error variant — every caller maps the same
/// `String` into its own typed `*Invalid { <axis>, reason }` enum
/// variant.
pub fn is_gateway_api_http_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("must not be empty".to_string());
    }
    if !path.starts_with('/') {
        return Err("must start with `/` (HTTP path matchers require a leading `/`)".to_string());
    }
    if path.len() > GATEWAY_API_HTTP_PATH_MAX_LEN {
        return Err(format!(
            "exceeds HTTP path max length of {GATEWAY_API_HTTP_PATH_MAX_LEN} bytes \
             (got {} bytes; both the K8s Gateway API HTTPPathMatch.value OpenAPI \
             schema and the Cilium L7 path matcher reject longer values at \
             admission time)",
            path.len()
        ));
    }
    for &b in path.as_bytes() {
        let reason = if b == b'?' {
            Some(
                "must not contain `?` (queries are matched separately via HTTPRoute \
                 `queryParams`, not in the path; drop the `?…` suffix)"
                    .to_string(),
            )
        } else if b == b'#' {
            Some(
                "must not contain `#` (fragments are client-side and never reach \
                 the gateway; drop the `#…` suffix)"
                    .to_string(),
            )
        } else if b == b' ' || b == b'\t' {
            Some(format!(
                "must not contain whitespace character {ch:?} (percent-encode as `%20` \
                 or use `-`/`_` instead)",
                ch = b as char
            ))
        } else if b < 0x20 || b == 0x7F {
            Some(format!(
                "must not contain control character 0x{b:02x} (HTTP path characters \
                 must be printable ASCII; the K8s Gateway API HTTPPathMatch.value and \
                 Cilium L7 path matcher both reject control characters at admission \
                 time)"
            ))
        } else if b >= 0x80 {
            Some(format!(
                "must not contain non-ASCII byte 0x{b:02x} (RFC 3986 requires \
                 percent-encoding `%XX` for characters outside the ASCII unreserved \
                 + reserved set)"
            ))
        } else if matches!(
            b,
            b'"' | b'<' | b'>' | b'[' | b'\\' | b']' | b'^' | b'`' | b'{' | b'|' | b'}'
        ) {
            // The eleven printable-ASCII bytes outside the K8s Gateway
            // API HTTPPathMatch.value apiserver-side OpenAPI regex
            // accepted set. RFC 3986 §3.3 `pchar = unreserved /
            // pct-encoded / sub-delims / ":" / "@"` excludes them from
            // every path-segment, so the apiserver rejects them at
            // admission time on every
            // `HTTPRoute.spec.rules[].matches[].path.value` landing site
            // (and the Cilium L7 path matcher follows the same grammar).
            // Until this gate landed `validate` only refused `?`, `#`,
            // whitespace, control characters, and non-ASCII bytes; the
            // canonical author-side "I wrote a path-template variable"
            // / "I copied an OpenAPI route" footguns silently passed
            // (`/api/cart/{id}` — Gateway API uses `:foo` for path
            // parameters, not `{foo}`; `/api/cart[0]` — index-bracket
            // shape; `/api/<placeholder>` — angle-bracket placeholder;
            // `/api\path` — Windows path-separator typo; `/api/^foo` —
            // accidental shell-regex character) and the failure surfaced
            // at apply time as a Gateway API webhook rejection naming
            // the offending byte but not the offending caixa.lisp slot.
            // Lifting the rejection to caixa-build time makes the
            // canonical Gateway API HTTPPathMatch.value accepted set a
            // structural property of every validated `:entrada :paths`
            // entry and every typed-HTTP `:contratos :endpoint` payload,
            // mirroring the c7d05ec / 55410e4 / 4f0390b trajectory each
            // brought the per-axis accepted set to match the apiserver
            // accepted set verbatim.
            Some(format!(
                "must not contain reserved character {ch:?} (RFC 3986 \
                 path-segment grammar — and the K8s Gateway API \
                 HTTPPathMatch.value apiserver-side OpenAPI regex \
                 `^(?:[-A-Za-z0-9/._~!$&'()*+,;=:@]|[%][0-9a-fA-F]{{2}})+$` — \
                 exclude this byte from the `pchar = unreserved / pct-encoded \
                 / sub-delims / \":\" / \"@\"` set; percent-encode as \
                 `%{b:02X}` if the literal character is intended)",
                ch = b as char
            ))
        } else {
            None
        };
        if let Some(r) = reason {
            return Err(r);
        }
    }
    if path.contains("//") {
        return Err(
            "must not contain consecutive `/` characters (HTTP path matchers reject \
             `//`; collapse to a single `/`)"
                .to_string(),
        );
    }
    if path.contains("/./") || path == "/." || path.ends_with("/.") {
        return Err(
            "must not contain the `.` segment (`/./` or trailing `/.`); it is \
             semantically a no-op and HTTP path matchers reject it"
                .to_string(),
        );
    }
    if path.contains("/../") || path == "/.." || path.ends_with("/..") {
        return Err(
            "must not contain the `..` parent-segment (`/../` or trailing `/..`); \
             path traversal is rejected by HTTP path matchers"
                .to_string(),
        );
    }
    Ok(())
}

/// Max length, in bytes, of a single typed `:contratos :wit` world
/// reference passing the [`is_wit_world_ref`] predicate. 128 bytes —
/// roughly 8× the longest real-world WIT reference the caixa-mesh test
/// fixtures carry (`wasi:keyvalue/store` = 19 bytes) and the WIT registry
/// references its peers under (`wasi:http/proxy@0.2.0` = 21 bytes), so
/// the cap exists to reject the paste-from-binary footgun (a multi-line
/// blob accidentally landed in the `:wit` slot) rather than to constrain
/// legitimate authoring. Lifted as a typed const so a future axis
/// reaching for the same bound (the M4 per-edge WIT registry resolver,
/// the future `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's
/// per-contract WIT validator) reads from one place.
pub const WIT_IDENT_MAX_LEN: usize = 128;

/// Predicate: assert that `s` is a valid WIT (WebAssembly Component
/// Model) world reference — the canonical shape every typed
/// `:contratos :wit` value carries. The contract — modeled on the
/// [WIT IDL grammar][wit] (`namespace:package(/interface)*(@version)?`)
/// restricted to the lowercase subset the pleme-io substrate dispatches
/// on:
///
///   - 1..=[`WIT_IDENT_MAX_LEN`] (128) bytes;
///   - no whitespace, no control characters, no non-ASCII bytes;
///   - exactly one `:` separator splitting the namespace from the
///     package — `wasi:http/proxy`, `nats:pub-sub`, `wasi:keyvalue/store`
///     (no `:` = there's no namespace to dispatch on; multiple `:` =
///     the package half can't parse);
///   - an optional `/`-separated interface suffix (one or more
///     segments — the WIT grammar allows `('/' id)+` after the package);
///   - an optional `@<version>` suffix (one trailing `@` only; the
///     version body is non-empty printable ASCII without `:` or `/`,
///     since those are reserved for the namespace/interface axes);
///   - every identifier segment (namespace, package, each interface)
///     is a lowercase kebab-case ASCII identifier: `[a-z]([a-z0-9]|-)*`,
///     starting with a lowercase letter, no consecutive `-`, no
///     trailing `-`.
///
/// Lowercase-only is deliberate — the substrate's
/// [`crate::aplicacao::WitContract::is_http`] / `is_pubsub` / `is_store`
/// dispatch keys off the lowercase canonical prefix (`wasi:http/`,
/// `nats:`, `wasi:keyvalue/`, `kafka:`, `kv:`, `http:`). An uppercase
/// `WASI:HTTP/proxy` is structurally a valid WIT identifier under the
/// upstream IDL grammar but silently falls through every `is_*` arm and
/// renders as a capability-only L4-only edge — the canonical "I thought
/// I had L7 HTTP routing, got L4-only" footgun. Lifting the lowercase
/// rule to caixa-build time makes the dispatch reachable-by-construction:
/// every validated `:wit` value matches exactly one of the three typed
/// dispatch arms (or the explicit capability arm), structurally.
///
/// Returns the parser-shaped reason on rejection (without wrapping in
/// any error variant) so each per-axis caller — `WitContract::target`
/// for the `:contratos :wit` axis at validate time, the future M4 CR
/// materializer's per-contract WIT validator, the future per-edge WIT
/// registry resolver — wraps the same reason in its own typed
/// `*Invalid { <axis>, reason }` variant. The reason wording is
/// axis-agnostic ("WIT identifiers allow only `[a-z0-9-]`") so every
/// call site reading the same diagnostic points at the same rule;
/// drift between any two axes' rule enforcement is a build error
/// visible at this predicate, not a per-renderer "this passed validate
/// but silently demoted to capability-only" surprise.
///
/// Empty input is rejected here (defensively) and at the call site via
/// the narrower [`crate::AplicacaoError::EmptyWit`] variant — the same
/// empty-first cascade [`is_dns_1123_label`] and
/// [`is_gateway_api_http_path`] carry.
///
/// Lifted as a typed substrate-side primitive on the same trajectory
/// the M2-overlay and label-selector helpers (9e3a057, 9d09cfb, 9dbeafd,
/// 31455a7, 07a4544) and the value-shape predicates (`is_dns_1123_label`,
/// `is_gateway_api_http_path`) already follow — the typed slot's valid
/// set matches its dispatch's accepted set, structurally.
///
/// [wit]: https://github.com/WebAssembly/component-model/blob/main/design/mvp/WIT.md
///
/// # Errors
///
/// Returns the parser-shaped reason naming the specific violation
/// (length / separator / character-class / kebab-shape), without
/// wrapping in any error variant — every caller maps the same
/// `String` into its own typed `*Invalid { <axis>, reason }` enum
/// variant.
pub fn is_wit_world_ref(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("must not be empty".to_string());
    }
    if s.len() > WIT_IDENT_MAX_LEN {
        return Err(format!(
            "exceeds WIT world-reference max length of {WIT_IDENT_MAX_LEN} bytes \
             (got {} bytes; legitimate WIT references rarely exceed ~32 bytes — \
             this length suggests a paste-from-binary or multi-line blob landed \
             in the `:wit` slot)",
            s.len()
        ));
    }
    for &b in s.as_bytes() {
        if b.is_ascii_whitespace() {
            return Err(format!(
                "must not contain whitespace character {ch:?} (WIT world references \
                 are single tokens with no whitespace between identifier segments)",
                ch = b as char
            ));
        }
        if b < 0x20 || b == 0x7F {
            return Err(format!(
                "must not contain control character 0x{b:02x} (WIT world references \
                 are printable ASCII tokens)"
            ));
        }
        if b >= 0x80 {
            return Err(format!(
                "must not contain non-ASCII byte 0x{b:02x} (WIT world references \
                 are restricted to ASCII identifiers + the `:` / `/` / `@` / `-` \
                 separators)"
            ));
        }
    }
    // Split off the optional `@<version>` suffix first so the
    // namespace/package parse below operates on a clean
    // `<ns>:<pkg>(/<iface>)*` head.
    let (head, version) = match s.split_once('@') {
        Some((h, v)) => (h, Some(v)),
        None => (s, None),
    };
    if let Some(ver) = version {
        if ver.is_empty() {
            return Err(
                "trailing `@` must be followed by a version (e.g. `@0.2.0`); drop \
                 the trailing `@` to omit the version pin"
                    .to_string(),
            );
        }
        if ver.contains('@') {
            return Err(
                "must contain at most one `@` separator (the optional version suffix \
                 is `@<version>`, not `@<ver>@<ver>`)"
                    .to_string(),
            );
        }
        if ver.contains(':') || ver.contains('/') {
            return Err(format!(
                "version suffix {ver:?} must not contain `:` or `/` (those separators \
                 are reserved for the namespace and interface axes; the version body \
                 is opaque)"
            ));
        }
    }
    // Then split the head on `:` — exactly one separator, splitting the
    // namespace from the package(/interface) body.
    let Some((ns, rest)) = head.split_once(':') else {
        return Err(format!(
            "must contain a `:` separating the namespace from the package (e.g. \
             `wasi:http/proxy`); got {s:?} with no `:` — pleme-io dispatches `:wit` \
             values on the canonical `<namespace>:<package>` shape and silently \
             demotes unmatched shapes to a capability-only L4 edge"
        ));
    };
    if rest.contains(':') {
        return Err(format!(
            "must contain exactly one `:` separator (between namespace and package); \
             got {s:?} with multiple `:`"
        ));
    }
    is_wit_kebab_id(ns)
        .map_err(|r| format!("namespace {ns:?} is not a valid WIT identifier: {r}"))?;
    let mut segments = rest.split('/');
    let pkg = segments.next().unwrap_or("");
    is_wit_kebab_id(pkg)
        .map_err(|r| format!("package {pkg:?} is not a valid WIT identifier: {r}"))?;
    for iface in segments {
        is_wit_kebab_id(iface)
            .map_err(|r| format!("interface {iface:?} is not a valid WIT identifier: {r}"))?;
    }
    Ok(())
}

/// Predicate: assert that `s` is a lowercase kebab-case ASCII identifier
/// — the WIT IDL `id ::= word ('-' word)*` rule restricted to the
/// lowercase `word ::= [a-z][a-z0-9]*` arm the pleme-io substrate
/// dispatches on. Private because every legitimate caller flows through
/// [`is_wit_world_ref`] (which segments the world reference and runs
/// this predicate per segment); exposing it directly would invite
/// per-axis WIT-shape gates that re-implement the segmenting logic
/// inline.
fn is_wit_kebab_id(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("must not be empty".to_string());
    }
    let bytes = s.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        let msg = if bytes[0].is_ascii_uppercase() {
            format!(
                "must start with a lowercase ASCII letter (got uppercase {ch:?}); \
                 pleme-io dispatches `:wit` values on the lowercase canonical shape \
                 — `wasi:http/proxy` is recognized, `WASI:HTTP/proxy` is silently \
                 demoted to a capability-only edge",
                ch = bytes[0] as char
            )
        } else if bytes[0].is_ascii_digit() {
            format!(
                "must start with a lowercase ASCII letter (got digit {ch:?}); WIT \
                 identifiers begin with a letter, not a digit",
                ch = bytes[0] as char
            )
        } else if bytes[0] == b'-' {
            "must not start with `-` (WIT identifiers are kebab-case words; the \
             leading character is a lowercase letter)"
                .to_string()
        } else {
            format!(
                "must start with a lowercase ASCII letter (got {ch:?}); WIT \
                 identifiers allow only `[a-z0-9-]`",
                ch = bytes[0] as char
            )
        };
        return Err(msg);
    }
    if bytes[bytes.len() - 1] == b'-' {
        return Err(
            "must not end with `-` (WIT identifiers are kebab-case words separated \
             by single hyphens; no trailing `-`)"
                .to_string(),
        );
    }
    let mut prev_hyphen = false;
    for &b in bytes {
        if b == b'-' {
            if prev_hyphen {
                return Err(
                    "must not contain consecutive `-` characters (WIT identifiers \
                     join words with single hyphens, not `--`)"
                        .to_string(),
                );
            }
            prev_hyphen = true;
            continue;
        }
        prev_hyphen = false;
        if b.is_ascii_uppercase() {
            return Err(format!(
                "must be lowercase (got uppercase character {ch:?}); pleme-io \
                 dispatches `:wit` values on the lowercase canonical shape — \
                 `wasi:http/proxy` is recognized, `WASI:HTTP/proxy` is silently \
                 demoted to a capability-only edge",
                ch = b as char
            ));
        }
        if !(b.is_ascii_lowercase() || b.is_ascii_digit()) {
            let msg = if b == b'_' {
                "contains `_` (WIT identifiers are kebab-case; use `-` between \
                 words instead of `_`)"
                    .to_string()
            } else if b == b'.' {
                "contains `.` (WIT identifiers are single kebab-case words; split \
                 into separate namespace/package/interface segments via `:` and \
                 `/` instead of `.`)"
                    .to_string()
            } else {
                format!(
                    "contains invalid character {ch:?} (WIT identifiers allow only \
                     `[a-z0-9-]`)",
                    ch = b as char
                )
            };
            return Err(msg);
        }
    }
    Ok(())
}

/// Max length, in bytes, of a single typed `:contratos :subject` NATS
/// subject passing the [`is_nats_subject`] predicate. 256 bytes —
/// matches the upstream NATS Java client's `MAX_SUBJECT_LENGTH`
/// constant and sits well above the longest legitimate subject the
/// caixa-mesh test fixtures + example checkout-aplicacao carry
/// (`"checkout.events.charge.failed"` = 30 bytes, `"rio.events.order.charged"`
/// = 25 bytes). The cap exists to reject the paste-from-binary footgun
/// (a multi-line blob accidentally landed in the `:subject` slot)
/// rather than to constrain legitimate authoring. Lifted as a typed
/// const so a future axis reaching for the same bound (the M4
/// `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's per-subject
/// validator, the future NATS Stream/Consumer CR emitter for the
/// `nats:pub-sub` branch of `:contratos`, the future per-edge
/// `:politicas`-derived NATS-aware policy overlay) reads from one
/// place.
pub const NATS_SUBJECT_MAX_LEN: usize = 256;

/// Predicate: assert that `s` is a valid NATS subject — the canonical
/// shape every typed `:contratos :subject` value carries. The
/// contract — modeled on the [NATS subject grammar][nats] (dot-
/// separated tokens with `*` / `>` wildcards), restricted to the
/// strict `[A-Za-z0-9_-]` per-token character set the NATS server's
/// subject parser accepts at runtime:
///
///   - 1..=[`NATS_SUBJECT_MAX_LEN`] (256) bytes;
///   - no whitespace, no control characters, no non-ASCII bytes
///     (RFC 3986 requires `%XX` percent-encoding for non-ASCII; NATS
///     subjects predate that and reject any byte outside the strict
///     ASCII identifier set);
///   - one-or-more `.`-separated tokens — no leading `.`, no trailing
///     `.`, no consecutive `.` (NATS rejects empty tokens between
///     separators);
///   - each token is one of:
///     - a concrete identifier `[A-Za-z0-9_-]+` (NATS subjects are
///       case-sensitive; unlike DNS-1123 we don't lowercase-fold,
///       and underscores are permitted since NATS itself accepts them
///       in tokens);
///     - the `*` single-token wildcard (matches exactly one token;
///       allowed at any segment position);
///     - the `>` multi-token wildcard (matches one-or-more trailing
///       tokens; allowed ONLY as the final segment — `foo.>` matches
///       `foo.bar` / `foo.bar.baz`, `foo.>.bar` is rejected outright).
///
/// Returns the parser-shaped reason on rejection (without wrapping in
/// any error variant) so each per-axis caller — `WitContract::target`
/// for the `:contratos :subject` axis at validate time, the future M4
/// CR materializer's per-subject validator, the future NATS Stream/
/// Consumer CR emitter — wraps the same reason in its own typed
/// `*Invalid { <axis>, reason }` variant. The reason wording is axis-
/// agnostic ("NATS subjects reject empty tokens between separators")
/// so every call site reading the same diagnostic points at the same
/// rule; drift between any two axes' rule enforcement is a build
/// error visible at this predicate, not a per-renderer "this passed
/// validate but the NATS server rejected at publish/subscribe" surprise.
///
/// Empty input is rejected here (defensively) and at the call site via
/// the narrower [`crate::AplicacaoError::ContratoSubjectEmpty`] variant
/// — the same empty-first cascade [`is_dns_1123_label`],
/// [`is_gateway_api_http_path`], and [`is_wit_world_ref`] carry.
///
/// Lifted as a typed substrate-side primitive on the same trajectory
/// the M2-overlay and label-selector helpers (9e3a057, 9d09cfb,
/// 9dbeafd, 31455a7, 07a4544) and the value-shape predicates
/// (`is_dns_1123_label`, `is_gateway_api_http_path`,
/// `is_wit_world_ref`) already follow — the typed slot's valid set
/// matches the NATS server's accepted set, structurally.
///
/// [nats]: https://docs.nats.io/nats-concepts/subjects
///
/// # Errors
///
/// Returns the parser-shaped reason naming the specific violation
/// (length / separator / character-class / wildcard-position), without
/// wrapping in any error variant — every caller maps the same
/// `String` into its own typed `*Invalid { <axis>, reason }` enum
/// variant.
pub fn is_nats_subject(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("must not be empty".to_string());
    }
    if s.len() > NATS_SUBJECT_MAX_LEN {
        return Err(format!(
            "exceeds NATS subject max length of {NATS_SUBJECT_MAX_LEN} bytes \
             (got {} bytes; legitimate NATS subjects rarely exceed ~64 bytes — \
             this length suggests a paste-from-binary or multi-line blob landed \
             in the `:subject` slot)",
            s.len()
        ));
    }
    for &b in s.as_bytes() {
        if b == b' ' || b == b'\t' {
            return Err(format!(
                "must not contain whitespace character {ch:?} (NATS subjects \
                 are single tokens with no whitespace between dot-separated \
                 segments)",
                ch = b as char
            ));
        }
        if b < 0x20 || b == 0x7F {
            return Err(format!(
                "must not contain control character 0x{b:02x} (NATS subjects \
                 are printable ASCII tokens; the NATS server's subject parser \
                 rejects control characters at publish/subscribe time)"
            ));
        }
        if b >= 0x80 {
            return Err(format!(
                "must not contain non-ASCII byte 0x{b:02x} (NATS subjects \
                 are restricted to `[A-Za-z0-9_-]` per token + the `.` \
                 separator and the `*` / `>` wildcards)"
            ));
        }
    }
    if s.starts_with('.') {
        return Err(
            "must not start with `.` (NATS subjects reject empty leading \
             tokens; drop the leading `.` separator)"
                .to_string(),
        );
    }
    if s.ends_with('.') {
        return Err(
            "must not end with `.` (NATS subjects reject empty trailing \
             tokens; use the `>` multi-token wildcard to match arbitrary \
             trailing segments instead)"
                .to_string(),
        );
    }
    if s.contains("..") {
        return Err(
            "must not contain consecutive `.` characters (NATS subjects \
             reject empty tokens between separators; use the `*` single-\
             token wildcard to match any one token)"
                .to_string(),
        );
    }
    let segments: Vec<&str> = s.split('.').collect();
    let last_idx = segments.len() - 1;
    for (i, seg) in segments.iter().enumerate() {
        is_nats_subject_segment(seg, i, last_idx)?;
    }
    Ok(())
}

/// Predicate: assert that `seg` is a valid NATS subject token at index
/// `i` of a `total = last_idx + 1`-segment subject. Private because
/// every legitimate caller flows through [`is_nats_subject`] (which
/// splits the subject on `.` and runs this predicate per segment);
/// exposing it directly would invite per-axis NATS-segment gates that
/// re-implement the splitting logic inline.
///
/// Mirrors the [`is_wit_kebab_id`] / [`is_wit_world_ref`] private-helper
/// pair on the WIT predicate.
fn is_nats_subject_segment(seg: &str, i: usize, last_idx: usize) -> Result<(), String> {
    if seg == "*" {
        return Ok(());
    }
    if seg == ">" {
        if i != last_idx {
            return Err(format!(
                "the `>` multi-token wildcard is only allowed as the \
                 final segment (got `>` at segment {one_based} of {total}; \
                 move to the end or use `*` for a single-token wildcard)",
                one_based = i + 1,
                total = last_idx + 1
            ));
        }
        return Ok(());
    }
    for &b in seg.as_bytes() {
        let valid = b.is_ascii_alphanumeric() || b == b'_' || b == b'-';
        if !valid {
            let msg = if b == b'*' {
                "contains `*` mid-segment (NATS wildcards are standalone \
                 tokens — `foo.*.bar` matches one middle token, `foo*` \
                 does not; split into separate `.`-separated segments)"
                    .to_string()
            } else if b == b'>' {
                "contains `>` mid-segment (NATS wildcards are standalone \
                 tokens — `foo.>` matches all trailing tokens, `foo>` \
                 does not; split into separate `.`-separated segments)"
                    .to_string()
            } else {
                format!(
                    "contains invalid character {ch:?} in subject segment \
                     (NATS subject tokens allow only `[A-Za-z0-9_-]`; use \
                     `_` or `-` instead)",
                    ch = b as char
                )
            };
            return Err(msg);
        }
    }
    Ok(())
}

/// Max length, in bytes, of a single typed `:contratos :slot` WASI
/// keyvalue store key/template passing the [`is_wasi_keyvalue_slot`]
/// predicate. 512 bytes — generously above the longest realistic slot
/// template (`"checkout/$orderId"` = 17 bytes, `"users:{tenant}/{id}"`
/// = 19 bytes, `"session.tokens.<sid>"` = 20 bytes) and well under any
/// canonical WASI-keyvalue backend's per-key limit (etcd: 1.5 MB,
/// DynamoDB partition+sort key: 2 KB combined, Redis: 512 MB — the cap
/// is chosen for the *template* slot a typed `:contratos` edge
/// authors, not the realized key at runtime). The cap exists to reject
/// the paste-from-binary footgun (a multi-line blob accidentally landed
/// in the `:slot` slot) rather than to constrain legitimate authoring.
/// Lifted as a typed const so a future axis reaching for the same
/// bound (the M4 `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's
/// per-slot validator, the future per-Servico `:capabilities`
/// `wasi:keyvalue/store` axis's per-slot validator when M4 lands
/// per-capability typed slots, the future per-edge `:politicas`-derived
/// kv-backend-aware policy overlay's per-slot validator) reads from
/// one place. Same lift trajectory as [`NATS_SUBJECT_MAX_LEN`] (which
/// caps the peer pub-sub payload axis at 256 bytes — twice that here
/// because kv slot templates legitimately compose more `/`-separated
/// path segments + template variables than NATS subjects do
/// `.`-separated tokens).
pub const WASI_KV_SLOT_MAX_LEN: usize = 512;

/// Predicate: assert that `s` is a valid WASI keyvalue store slot
/// template — the canonical shape every typed `:contratos :slot` value
/// carries when its `:wit` dispatch resolves to the
/// [`WitTarget::Store`][st] arm (`wasi:keyvalue/store`, `kv:*`). The
/// WASI keyvalue 0.2 specification ([`bucket = string`, `key = string`,
/// both opaque][wasi-kv]) places no syntactic constraints on the key
/// shape, so the substrate enforces the canonical printable-ASCII
/// floor every realistic kv backend admits: no raw whitespace, no
/// control bytes, no non-ASCII bytes, length-bounded by
/// [`WASI_KV_SLOT_MAX_LEN`]. The grammar:
///
///   - 1..=[`WASI_KV_SLOT_MAX_LEN`] (512) bytes;
///   - no whitespace (space, tab — kv slot templates are single-token
///     identifiers / path expressions, whitespace is the canonical
///     paste-from-doc footgun whose runtime behavior varies
///     unpredictably across backends — etcd accepts, Redis accepts
///     but rejects subsequent CLI ops, DynamoDB rejects on write);
///   - no ASCII control characters (`0x00..0x1F`, `0x7F`) — every
///     kv backend either rejects on write (DynamoDB, etcd) or admits
///     and silently breaks at the next read (Redis: `\r\n` corrupts
///     the RESP protocol framing if the slot template is rendered
///     directly into a key without re-encoding);
///   - no non-ASCII bytes (`>= 0x80`) — RFC 3986-style percent-
///     encoding (`%XX`) is the substrate's canonical UTF-8 escape
///     for kv slot templates the author wants to namespace by
///     non-ASCII identifier; raw non-ASCII silently differs between
///     backends (etcd preserves bytes verbatim; Redis-via-RESP3 may
///     re-encode; DynamoDB rejects).
///
/// The predicate is intentionally permissive on structure: all
/// printable ASCII bytes (`0x21..0x7E`) are admitted, including
/// `/` (path separators), `:` (namespace separators), `.`
/// (dot-namespacing), `-`/`_` (identifier separators), `$`/`{`/`}`/`<`/`>`
/// (template-variable syntaxes — the canonical `"checkout/$orderId"`
/// shape carries `$`-prefixed identifiers, alternate `"users:{id}"` /
/// `"session.<sid>"` shapes carry `{}` / `<>` brackets), and the
/// remaining ASCII punctuation. The substrate doesn't know which kv
/// backend the runtime resolves [`WitTarget::Store`][st] to — that
/// choice is per-cluster, made by the operator's kv-provider binding
/// — so the typed slot enforces the intersection-floor every backend
/// admits rather than any one backend's stricter superset.
///
/// Returns the parser-shaped reason on rejection (without wrapping in
/// any error variant) so each per-axis caller — [`WitContract::target`]
/// for the `:contratos :slot` axis at validate time, the future M4 CR
/// materializer's per-slot validator, the future per-Servico
/// `:capabilities wasi:keyvalue/store` per-slot validator — wraps the
/// same reason in its own typed `*Invalid { <axis>, reason }` variant.
/// The reason wording is axis-agnostic ("kv slot templates reject raw
/// whitespace") so every call site reading the same diagnostic points
/// at the same rule; drift between any two axes' rule enforcement is
/// a build error visible at this predicate, not a per-renderer "this
/// passed validate but the kv backend rejected on first write"
/// surprise.
///
/// Empty input is rejected here (defensively) and at the call site
/// via the narrower [`crate::AplicacaoError::ContratoSlotEmpty`]
/// variant — the same empty-first cascade [`is_dns_1123_label`],
/// [`is_gateway_api_http_path`], [`is_wit_world_ref`], and
/// [`is_nats_subject`] all carry.
///
/// Lifted as a typed substrate-side primitive on the same trajectory
/// the peer payload-axis predicates ([`is_gateway_api_http_path`] for
/// `:endpoint`, [`is_nats_subject`] for `:subject`) already follow —
/// the typed slot's valid set matches the kv backend intersection-
/// floor's accepted set, structurally. The fifth value-shape primitive
/// to land in [`crate::render`] after [`is_dns_1123_label`],
/// [`is_gateway_api_http_path`], [`is_wit_world_ref`], and
/// [`is_nats_subject`] — and the one that closes the trajectory across
/// every typed payload axis the [`WitContract::target`] dispatch
/// carries (HTTP `:endpoint`, PubSub `:subject`, Store `:slot`).
///
/// [st]: crate::WitTarget::Store
/// [wasi-kv]: https://github.com/WebAssembly/wasi-keyvalue
///
/// # Errors
///
/// Returns the parser-shaped reason naming the specific violation
/// (length / whitespace / control / non-ASCII), without wrapping in
/// any error variant — every caller maps the same `String` into its
/// own typed `*Invalid { <axis>, reason }` enum variant.
pub fn is_wasi_keyvalue_slot(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("must not be empty".to_string());
    }
    if s.len() > WASI_KV_SLOT_MAX_LEN {
        return Err(format!(
            "exceeds WASI keyvalue slot max length of {WASI_KV_SLOT_MAX_LEN} bytes \
             (got {} bytes; legitimate kv slot templates rarely exceed ~64 bytes — \
             this length suggests a paste-from-binary or multi-line blob landed in \
             the `:slot` slot)",
            s.len()
        ));
    }
    for &b in s.as_bytes() {
        if b == b' ' || b == b'\t' {
            return Err(format!(
                "must not contain whitespace character {ch:?} (kv slot templates \
                 are single-token identifiers / path expressions; raw whitespace \
                 behaves unpredictably across kv backends — percent-encode as `%20` \
                 or use `-`/`_` to namespace)",
                ch = b as char
            ));
        }
        if b < 0x20 || b == 0x7F {
            return Err(format!(
                "must not contain control character 0x{b:02x} (kv slot templates \
                 are printable ASCII; control bytes either get rejected on write \
                 by strict backends — DynamoDB, etcd — or silently corrupt the \
                 next read on permissive ones — Redis RESP framing)"
            ));
        }
        if b >= 0x80 {
            return Err(format!(
                "must not contain non-ASCII byte 0x{b:02x} (RFC 3986 requires \
                 percent-encoding `%XX` for characters outside the ASCII unreserved \
                 + reserved set; raw non-ASCII bytes are admitted by some kv backends \
                 verbatim and re-encoded by others — the typed slot's value set is \
                 the intersection-floor every backend admits identically)"
            ));
        }
    }
    Ok(())
}

/// Max length, in bytes, of a single typed git ref name passing the
/// [`is_git_ref_name`] predicate. 255 bytes — matches the POSIX
/// `NAME_MAX` filesystem-component limit every Git porcelain ultimately
/// stores refs into (loose `refs/<category>/<name>` files under
/// `.git/refs/`, packed-refs index entries). Refs that exceed this cap
/// fail to land on disk at clone/fetch time on every realistic
/// filesystem (ext4, btrfs, xfs, APFS, NTFS), so a `:tag` / `:branch`
/// past that length is unsourceable in practice. The cap exists to
/// reject the paste-from-binary footgun (a multi-line blob accidentally
/// landed in the `:tag` slot) rather than to constrain legitimate
/// authoring — realistic tag/branch names rarely exceed ~32 bytes
/// (`"v0.1.0"` = 6 bytes, `"release-1.0-alpha.1"` = 19 bytes,
/// `"feature/checkout-rewrite"` = 24 bytes). Lifted as a typed const
/// so a future axis reaching for the same bound (the future
/// `lacre.lisp` ref-shape gate on resolved-pin axes, the future M4
/// per-dep CR materializer's per-pin validator) reads from one place.
pub const GIT_REF_NAME_MAX_LEN: usize = 255;

/// Predicate: assert that `s` is a valid Git ref name under the
/// `git check-ref-format --allow-onelevel` rule set — the canonical
/// shape every typed `:fonte (:tipo git …)` `:tag` / `:branch` value
/// carries. The contract — modeled on the [`git check-ref-format`][gcr]
/// grammar the Git porcelain enforces at clone/fetch/checkout time,
/// with the multi-component requirement waived (`:tag "v0.1.0"` and
/// `:branch "main"` are both single-component refs, the canonical
/// leaf form for caixa's `:fonte` pin axes):
///
///   - 1..=[`GIT_REF_NAME_MAX_LEN`] (255) bytes — the POSIX `NAME_MAX`
///     filesystem-component limit Git's loose-ref `.git/refs/<cat>/<name>`
///     storage tops out at;
///   - no ASCII control characters (`0x00..=0x1F`, `0x7F`) — Git's
///     refname parser rejects them, and the `\r` / `\n` arms are the
///     canonical "the paste-from-doc spans multiple lines" footgun;
///   - no whitespace (space, tab) — Git's refname parser rejects them
///     too; a `:tag "v0.1.0 "` (trailing space, from a copy-paste)
///     silently passes string emptiness checks and fails at
///     `git fetch origin tag 'v0.1.0 '` with a quoting-confused error
///     far from the source caixa.lisp;
///   - no non-ASCII bytes (`>= 0x80`) — Git's refname rules predate
///     UTF-8 normalization (NFC vs NFD on APFS silently rewrites the
///     ref body, breaking the lacre's content addressing); the
///     intersection-floor every realistic Git host accepts is ASCII
///     identifiers + the small punctuation set below;
///   - no `~`, `^`, `:`, `?`, `*`, `[`, `\` anywhere — Git reserves
///     these for revision-grammar expressions (`HEAD~3`, `HEAD^`,
///     `:/searched`, glob wildcards, refspec brackets, Windows-path
///     backslash);
///   - no `@{` sequence — Git's reflog grammar (`HEAD@{2 hours ago}`,
///     `branch@{upstream}`);
///   - the bare `@` is not a valid refname (it's the alias for `HEAD`);
///   - no `..` anywhere (Git's `<rev1>..<rev2>` range syntax + the
///     `.` / `..` parent-traversal footgun);
///   - per `/`-separated component: must not begin with `.` (Git
///     refuses to follow loose `.git/refs/<cat>/.<name>` files), must
///     not end with `.lock` (Git's atomic-rename guard suffix), must
///     not be empty (`//` rejected by the no-empty-component arm
///     below);
///   - no leading `/`, no trailing `/`, no consecutive `//`;
///   - no trailing `.` on the whole ref (Git rejects `<name>.`);
///   - no `refs/heads/` or `refs/tags/` prefix — the canonical "I
///     copied the fully-qualified ref name out of `git show-ref`
///     instead of the leaf" footgun (per [`theory/FLAKE-DEDUP.md`][fd]
///     `BranchName` constructor rules); the caixa-resolver prepends
///     the category prefix at clone time, so an author-side
///     `:branch "refs/heads/main"` resolves to a literal ref named
///     `refs/heads/refs/heads/main` on disk.
///
/// Returns the parser-shaped reason on rejection (without wrapping in
/// any error variant) so each per-axis caller — `DepSource::validate`
/// for the `:fonte :tag` / `:fonte :branch` axes at validate time,
/// the future per-pin gate on `lacre.lisp` resolved-ref axes, the
/// future M4 per-dep CR materializer's per-pin validator — wraps the
/// same reason in its own typed `*Invalid { <axis>, reason }` variant.
/// The reason wording is axis-agnostic ("git ref names reject ASCII
/// control characters") so every call site reading the same diagnostic
/// points at the same rule; drift between any two axes' rule
/// enforcement is a build error visible at this predicate, not a
/// per-renderer "this passed validate but `git fetch` rejected at
/// clone time" surprise.
///
/// Empty input is rejected here (defensively) and at each call site
/// via the narrower [`crate::DepError::FontePinEmpty`] variant — the
/// same empty-first cascade [`is_dns_1123_label`],
/// [`is_gateway_api_http_path`], [`is_wit_world_ref`],
/// [`is_nats_subject`], and [`is_wasi_keyvalue_slot`] all carry.
///
/// `:rev` is intentionally NOT routed through this predicate — its
/// author-surface shape is a hex commit-ID (`[0-9a-f]+`), not a
/// refname; a dedicated `is_git_oid` predicate on the parallel
/// hex-shape trajectory carries the reproducibility contract. Routing
/// `:rev` through `is_git_ref_name` would admit `:rev "main"`,
/// defeating the reproducibility contract `:rev` carries vs.
/// `:branch` / `:tag`. The reverse mis-slot — a canonical OID
/// (40-char SHA-1 or 64-char SHA-256 lowercase hex) pasted into the
/// `:tag` / `:branch` slot — is closed by this predicate too: a
/// pre-emption arm below rejects any value whose width and byte set
/// match the canonical OID shape, surfacing the cross-axis mis-slot
/// at validate time with a diagnostic pointing the author at the
/// `:rev` slot. The two predicates' valid sets intersect at exactly
/// the empty set, structurally.
///
/// Lifted as a typed substrate-side primitive on the same trajectory
/// the peer value-shape predicates ([`is_dns_1123_label`],
/// [`is_gateway_api_http_path`], [`is_wit_world_ref`],
/// [`is_nats_subject`], [`is_wasi_keyvalue_slot`]) already follow —
/// the typed slot's valid set matches the Git porcelain's accepted
/// set, structurally. The sixth value-shape primitive to land in
/// [`crate::render`], and the first to gate a non-K8s downstream
/// landing surface (git CLI invocation from caixa-resolver, vs. the
/// K8s apiserver / NATS server / WASI kv backend for the prior five).
///
/// [gcr]: https://git-scm.com/docs/git-check-ref-format
/// [fd]: pleme-io/theory/FLAKE-DEDUP.md §1 `BranchName`
///
/// # Errors
///
/// Returns the parser-shaped reason naming the specific violation
/// (length / control-char / forbidden-char / component-shape / prefix),
/// without wrapping in any error variant — every caller maps the same
/// `String` into its own typed `*Invalid { <axis>, reason }` enum
/// variant.
pub fn is_git_ref_name(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("must not be empty".to_string());
    }
    if s.len() > GIT_REF_NAME_MAX_LEN {
        return Err(format!(
            "exceeds git ref name max length of {GIT_REF_NAME_MAX_LEN} bytes \
             (got {} bytes; legitimate tag/branch names rarely exceed ~32 bytes — \
             this length suggests a paste-from-binary or multi-line blob landed \
             in the `:tag` / `:branch` slot)",
            s.len()
        ));
    }
    // Canonical-OID-shape pre-emption — the structural partition the
    // doc-comment above promises and [`crate::DepSource::validate`]
    // routes the `:fonte` pin axes through ([`is_git_ref_name`] for
    // `:tag` + `:branch`, [`is_git_oid`] for `:rev`): a value that's
    // exactly the canonical Git commit-OID width
    // ([`GIT_OID_SHA1_LEN`] (40) lowercase-hex for SHA-1,
    // [`GIT_OID_SHA256_LEN`] (64) lowercase-hex for SHA-256) is the
    // shape `is_git_oid` accepts; the two predicates' valid sets must
    // intersect at exactly the empty set, so a value of that shape is
    // rejected here. Without this arm a canonical lowercase-hex OID of
    // either canonical width passes every other refname-shape arm in
    // this predicate — pure-hex strings carry none of the forbidden
    // characters, no `..` / `@{` / leading-`/` / trailing-`/` /
    // `.lock`-suffix / `refs/heads/`-prefix — and the cross-axis
    // partition silently fails on the canonical "I copied the SHA out
    // of `git show --format=%H` and pasted it into `:tag` / `:branch`"
    // mis-slot footgun. The pleme-io discipline (CAIXA-SDLC §V — the
    // `:rev` slot carries the reproducibility contract; `:tag` /
    // `:branch` resolve to whatever the upstream has tagged / `HEAD`
    // today) requires that an OID-shaped value live under `:rev`, never
    // under `:tag` / `:branch`; this arm makes that discipline a typed
    // structural property, not a convention.
    //
    // Uppercase hex (`"DEADBEEF…"` 40 chars) is intentionally NOT
    // matched here — uppercase letters are legitimate in refnames per
    // `git check-ref-format`, so an uppercase 40/64-char hex string is a
    // valid refname (`is_git_ref_name` accepts it); the `:rev` axis
    // separately rejects uppercase via [`is_git_oid`]'s lowercase-only
    // contract. Off-canonical lengths (39 / 41 / 63 / 65 hex chars) are
    // also intentionally NOT matched — abbreviated commit IDs are
    // ambiguous across repository history but they're not canonical
    // OIDs either; they remain accepted as refnames here (consistent
    // with `is_git_oid` already rejecting them via its exact-width
    // check).
    if (s.len() == GIT_OID_SHA1_LEN || s.len() == GIT_OID_SHA256_LEN)
        && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(format!(
            "looks like a canonical Git commit OID ({len} lowercase hex \
             characters — the SHA-{algo} OID width); pleme-io's `:fonte` axes \
             partition refnames vs. commit OIDs structurally, so a value of \
             this shape belongs in the `:rev` slot (which routes through \
             `is_git_oid` for the reproducibility contract — one immutable \
             commit, forever), not `:tag` / `:branch` (which route through \
             this predicate for human-readable refs `git fetch` resolves at \
             clone time). Move the value to `:rev`; keeping it under `:tag` \
             / `:branch` is the canonical paste-from-`git show --format=%H` \
             mis-slot footgun, silently demoting an OID to a refname-shaped \
             pin the resolver would attempt to `git fetch tag '<sha>'` and \
             fail at clone time with a quoting-confused porcelain error far \
             from the source caixa.lisp.",
            len = s.len(),
            algo = if s.len() == GIT_OID_SHA1_LEN {
                "1"
            } else {
                "256"
            },
        ));
    }
    if s.starts_with('-') {
        return Err(
            "must not start with `-` (the canonical CLI-argument-injection \
             footgun on the `:tag` / `:branch` axis — caixa-resolver's \
             `git::checkout` invocation routes the ref name verbatim into \
             `git checkout --quiet --detach <ref>` (caixa-resolver/src/git.rs:41) \
             without a `--` argument-list terminator, so a leading `-` value \
             (`:tag \"-stable\"`, `:branch \"-X\"`, `:tag \"-c=core.merge=ours\"`) \
             silently escapes the subprocess argument boundary and gets \
             reinterpreted by `git checkout`'s argument parser as a CLI flag — \
             the canonical short-flag / long-option / config-injection vector. \
             Git's `check-ref-format` grammar does NOT reject a leading `-` \
             (it admits the byte mid-name as a legitimate kebab separator), so \
             every prior shape arm on this predicate passes the value through; \
             the diagnostic moves the gate to the subprocess-argument \
             boundary the resolver consumes. Peer with the \
             [`is_git_repo_url`] leading-`-` arm (the CLI-arg-injection \
             vector on the sibling `:repo` axis where `git clone <repo>` \
             reinterprets a leading `-` as a flag like `-upload-pack=…` / \
             `--config=…`), [`is_cargo_feature_name`] leading-`-` arm, and \
             [`is_dns_1123_label`] leading-`-` arm — every single-token typed \
             string slot the substrate routes through a downstream subprocess \
             / parser rejects the same leading-byte CLI-arg-injection shape \
             at validate time. Drop the leading `-`; use a kebab-separator-\
             between-alphanumeric-segments form like `\"v0.1.0\"` / \
             `\"feature-x\"` / `\"main\"` instead)"
                .to_string(),
        );
    }
    for &b in s.as_bytes() {
        if b == b' ' || b == b'\t' {
            return Err(format!(
                "must not contain whitespace character {ch:?} (git ref names are \
                 single tokens with no whitespace — a trailing space in a `:tag` \
                 / `:branch` value is the canonical paste-from-doc footgun, \
                 silently breaking `git fetch <remote> tag '<value> '` at \
                 clone time)",
                ch = b as char
            ));
        }
        if b < 0x20 || b == 0x7F {
            return Err(format!(
                "must not contain control character 0x{b:02x} (git ref names are \
                 printable ASCII; `\\r` / `\\n` are the canonical \
                 paste-from-multiline-doc footgun and break git's refname parser \
                 at every porcelain entry point)"
            ));
        }
        if b >= 0x80 {
            return Err(format!(
                "must not contain non-ASCII byte 0x{b:02x} (git's refname rules \
                 predate UTF-8 normalization — APFS NFC/NFD silently rewrites the \
                 ref body, breaking the lacre's content addressing across \
                 platforms; the intersection-floor every git host admits is ASCII)"
            ));
        }
        match b {
            b'~' => {
                return Err("must not contain `~` (git reserves `~` for the revision \
                     grammar — `HEAD~3` means `parent of parent of parent of \
                     HEAD`; the bare character is not admitted in a refname)"
                    .to_string());
            }
            b'^' => {
                return Err("must not contain `^` (git reserves `^` for the revision \
                     grammar — `HEAD^` means `first parent of HEAD`; the bare \
                     character is not admitted in a refname)"
                    .to_string());
            }
            b':' => {
                return Err("must not contain `:` (git reserves `:` for revspec / \
                     refspec separators — `:refs/heads/...`, `<src>:<dst>`)"
                    .to_string());
            }
            b'?' => {
                return Err("must not contain `?` (git reserves `?` for refspec glob \
                     wildcards)"
                    .to_string());
            }
            b'*' => {
                return Err("must not contain `*` (git reserves `*` for refspec glob \
                     wildcards — `refs/heads/*:refs/remotes/origin/*`)"
                    .to_string());
            }
            b'[' => {
                return Err("must not contain `[` (git reserves `[` for refspec \
                     bracketed-glob syntax)"
                    .to_string());
            }
            b'\\' => {
                return Err("must not contain `\\` (git's refname grammar rejects \
                     backslash — the canonical Windows-path-leak footgun; use \
                     `/` for hierarchical refs)"
                    .to_string());
            }
            _ => {}
        }
    }
    if s.contains("..") {
        return Err(
            "must not contain `..` (git reserves `..` for the `<rev1>..<rev2>` \
             range grammar; a `..` component would also escape the loose-ref \
             directory tree at clone time)"
                .to_string(),
        );
    }
    if s.contains("@{") {
        return Err(
            "must not contain `@{` (git reserves `@{` for the reflog grammar \
             — `branch@{upstream}`, `HEAD@{2 hours ago}`)"
                .to_string(),
        );
    }
    if s == "@" {
        return Err(
            "must not be the bare `@` (git aliases `@` to `HEAD`; a `:tag` / \
             `:branch` named `@` is unsourceable)"
                .to_string(),
        );
    }
    if s.starts_with('/') {
        return Err(
            "must not begin with `/` (git refnames are relative to the ref \
             category prefix the resolver prepends — drop the leading `/`)"
                .to_string(),
        );
    }
    if s.ends_with('/') {
        return Err(
            "must not end with `/` (git refnames are leaf-or-multi-component; \
             a trailing `/` would resolve to an empty final component)"
                .to_string(),
        );
    }
    if s.contains("//") {
        return Err(
            "must not contain consecutive `/` characters (git refnames reject \
             empty components between separators)"
                .to_string(),
        );
    }
    if s.ends_with('.') {
        return Err(
            "must not end with `.` (git refnames reject a trailing `.` — \
             `<name>.` collides with the `<name>.lock` atomic-rename guard \
             suffix on case-insensitive filesystems)"
                .to_string(),
        );
    }
    if s.starts_with("refs/heads/") || s.starts_with("refs/tags/") {
        return Err(format!(
            "must not carry the fully-qualified `refs/heads/` or `refs/tags/` \
             prefix (this is the canonical `git show-ref` output-leak footgun; \
             the caixa-resolver prepends the category prefix at clone time, so \
             a `:branch \"refs/heads/main\"` would resolve to a literal ref \
             named `refs/heads/refs/heads/main` on disk — drop the prefix and \
             pass the leaf: `{leaf:?}`)",
            leaf = s
                .strip_prefix("refs/heads/")
                .or_else(|| s.strip_prefix("refs/tags/"))
                .unwrap_or(s),
        ));
    }
    for (i, component) in s.split('/').enumerate() {
        if component.starts_with('.') {
            return Err(format!(
                "component {component:?} (segment {one_based} of the `/`-split \
                 refname) must not begin with `.` (git refuses to follow loose \
                 `.git/refs/<cat>/.<name>` files)",
                one_based = i + 1,
            ));
        }
        // Case-insensitive `.lock` check: git enforces the `.lock`
        // suffix as the atomic-rename guard on case-sensitive
        // filesystems (refs/heads/main.lock collides with the
        // in-flight update lockfile); on case-insensitive
        // filesystems (APFS default, NTFS, HFS+) the `.LOCK` /
        // `.Lock` variants collide identically. Rejecting all case
        // permutations matches the broader-rejection intent on the
        // axis the lacre pipeline ultimately stores into.
        if component.len() >= 5
            && component.as_bytes()[component.len() - 5..].eq_ignore_ascii_case(b".lock")
        {
            return Err(format!(
                "component {component:?} (segment {one_based} of the `/`-split \
                 refname) must not end with `.lock` (git uses the `.lock` \
                 suffix as the atomic-rename guard for in-flight ref updates; \
                 a refname ending in `.lock` is unwritable, and the suffix is \
                 case-insensitive on the case-insensitive filesystems Git \
                 supports — APFS default, NTFS, HFS+)",
                one_based = i + 1,
            ));
        }
    }
    Ok(())
}

/// Length, in lowercase-hex characters, of a full Git SHA-1 commit
/// OID — the canonical commit identifier every `git rev-parse HEAD`
/// invocation emits on a SHA-1-hashed repository. `git`'s loose-object
/// store keys every object under `.git/objects/<first-2-hex>/<last-38-hex>`,
/// so the full 40-char OID is the address-of-truth the porcelain consumes
/// at `git fetch <remote> <40-hex>` and `git checkout <40-hex>` time;
/// abbreviated OIDs are admitted by the porcelain through a separate
/// prefix-lookup pass and are ambiguous across repository history (a 7-char
/// prefix that resolves to one commit today can become a collision tomorrow
/// as the repo grows). Lifted as a typed const so the `:fonte :rev`
/// validate gate, the future lacre-side resolved-rev gate, and the future
/// M4 per-dep CR materializer's per-pin validator all read from one place.
pub const GIT_OID_SHA1_LEN: usize = 40;

/// Length, in lowercase-hex characters, of a full Git SHA-256 commit
/// OID — the canonical commit identifier on a SHA-256-hashed repository
/// (Git's [`extensions.objectFormat = sha256`][gitsha256] mode, GA since
/// Git 2.42 / Oct 2023). Doubled width vs. SHA-1: 256 bits = 64 hex chars.
/// Carried alongside [`GIT_OID_SHA1_LEN`] so the typed `:rev` slot admits
/// either canonical hash-algorithm OID without per-renderer branching;
/// the lacre's BLAKE3 content-addressing (THEORY.md §IV — typed reproducibility
/// envelope) is orthogonal to the upstream git's chosen object hash and
/// neither OID width should leak into downstream code paths.
///
/// [gitsha256]: https://git-scm.com/docs/hash-function-transition
pub const GIT_OID_SHA256_LEN: usize = 64;

/// Predicate: assert that `s` is a valid Git commit OID — the canonical
/// shape the typed `:fonte (:tipo git …)` `:rev` axis carries. The
/// reproducibility contract `:rev` carries vs. `:tag` / `:branch`
/// (CAIXA-SDLC §V — Substrate; `:tag` resolves to whatever the upstream
/// has tagged today, `:branch` to whatever the upstream's HEAD points at
/// today, `:rev` to exactly one immutable commit forever — same shape
/// Unison's [content-addressed code identity][unison] gives terms by
/// construction: the hash is the address, the address never moves):
///
///   - exactly [`GIT_OID_SHA1_LEN`] (40, SHA-1) or [`GIT_OID_SHA256_LEN`]
///     (64, SHA-256) characters — the two canonical Git hash-algorithm
///     widths; anything in between is an abbreviated prefix (the
///     canonical `git log --short` / `git rev-parse --short HEAD`
///     paste-from-release-notes footgun), which is ambiguous across
///     repository history and surfaces at clone time as an
///     [`ambiguous argument`][gitambig] error far from the source
///     caixa.lisp;
///   - every byte in `[0-9a-f]` (lowercase ASCII hex) — `git rev-parse`
///     and `git show --format=%H` both emit lowercase exclusively, so an
///     uppercase-bearing `:rev` round-trips inconsistently across the
///     resolver's `git fetch <remote> <:rev>` ↔ `git rev-parse HEAD`
///     equality-check pipeline and fails the lacre's content-addressing
///     equality probe with a confusing case-only diff;
///   - no whitespace, no control bytes, no non-ASCII, no refname
///     punctuation (`~ ^ : ? * [ \`), no `/` separators — every
///     character outside `[0-9a-f]` is rejected on the same predicate
///     arm, so a `:rev "main"` (the canonical "I conflated `:rev`
///     and `:branch`" footgun) lands at the same gate as a
///     `:rev "v0.1.0"` (`:tag` mis-slot) or a `:rev "c0ffee:scratch"`
///     (refname-shape leak); the typed `:rev` slot's valid set
///     intersects the `:tag` / `:branch` slot's valid set at exactly
///     the empty set, structurally — every refname is rejected here,
///     every OID is rejected by [`is_git_ref_name`].
///   - not the all-zero null-OID sentinel (`"0000…0000"` — 40 zeros
///     at SHA-1 width, 64 zeros at SHA-256 width). Git reserves this
///     value as the "no commit" sentinel in `git update-ref` /
///     pre-receive hook flows (`<old-value>` for create, `<new-value>`
///     for delete) and no commit in any object database has this OID,
///     so a `:rev "0000…0000"` is structurally impossible to resolve.
///     The canonical "I copy-pasted the sentinel out of `git
///     update-ref --stdin` docs / pre-receive hook example" footgun
///     would otherwise pass every other shape arm (canonical length,
///     lowercase hex) and surface at `git fetch <remote> 0000…0000`
///     time with a quoting-confused "couldn't find remote ref" error
///     far from the source caixa.lisp, with the lacre's content-
///     address locked to a `git:0000…0000` closure that never equals
///     any upstream's actual `HEAD`. Mirrors `is_git_ref_name`'s
///     canonical-OID-shape pre-emption arm (line 1322) — both
///     predicates carry one self-aware arm that catches values
///     structurally valid for the alphabet but operationally
///     meaningless on the typed axis.
///
/// Returns the parser-shaped reason on rejection (without wrapping in
/// any error variant) so each per-axis caller — [`crate::DepError::FontePinShape`]
/// at validate time on the `:fonte :rev` axis, the future per-pin gate
/// on `lacre.lisp` resolved-rev axes, the future M4 per-dep CR
/// materializer's per-pin validator — wraps the same reason in its own
/// typed `*Invalid { axis, reason }` variant. The reason wording is
/// axis-agnostic ("git commit OIDs are lowercase hex (`[0-9a-f]`)") so
/// every call site reading the same diagnostic points at the same rule.
///
/// Empty input is rejected here (defensively) and at each call site via
/// the narrower [`crate::DepError::FontePinEmpty`] variant — the same
/// empty-first cascade [`is_dns_1123_label`], [`is_gateway_api_http_path`],
/// [`is_wit_world_ref`], [`is_nats_subject`], [`is_wasi_keyvalue_slot`],
/// and [`is_git_ref_name`] all carry.
///
/// Sibling of [`is_git_ref_name`]: the two predicates together bracket
/// the `:fonte` pin axes — refname-shaped (`:tag` / `:branch`) vs.
/// hex-OID-shaped (`:rev`) — so an authored value lands in exactly one
/// of the two valid sets, and a cross-axis mis-slot (`:rev "main"` /
/// `:tag "deadbeef…"`) is a build error at the offending axis's
/// predicate, not a clone-time surprise.
///
/// [unison]: https://www.unison-lang.org/docs/the-big-idea/
/// [gitambig]: https://git-scm.com/docs/git-rev-parse#_specifying_revisions
///
/// # Errors
///
/// Returns the parser-shaped reason naming the specific violation
/// (length / character-class), without wrapping in any error variant —
/// every caller maps the same `String` into its own typed
/// `*Invalid { axis, reason }` enum variant.
pub fn is_git_oid(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("must not be empty".to_string());
    }
    let len = s.len();
    if len != GIT_OID_SHA1_LEN && len != GIT_OID_SHA256_LEN {
        return Err(format!(
            "git commit OIDs are exactly {GIT_OID_SHA1_LEN} hex chars (SHA-1) or \
             {GIT_OID_SHA256_LEN} hex chars (SHA-256); got {len} chars (an \
             abbreviated commit ID is ambiguous across repository history — \
             `git log --short` / `git rev-parse --short HEAD` emit prefixes for \
             human display only, not as reproducible commit addresses; pin the \
             full OID so the resolver's `git fetch <remote> <:rev>` and the \
             lacre's content-addressing equality probe both resolve to exactly \
             one immutable commit, forever)"
        ));
    }
    for (i, b) in s.bytes().enumerate() {
        match b {
            b'0'..=b'9' | b'a'..=b'f' => {}
            b'A'..=b'F' => {
                return Err(format!(
                    "git commit OIDs are lowercase hex (`[0-9a-f]`); got \
                     uppercase character {ch:?} at byte {i} (git porcelain \
                     emits OIDs lowercase exclusively — `git rev-parse HEAD` \
                     and `git show --format=%H` both lowercase on output; a \
                     `:rev` value with `[A-F]` round-trips inconsistently \
                     across the resolver's fetch ↔ `git rev-parse HEAD` \
                     equality-check pipeline and fails the lacre's \
                     content-addressing probe with a confusing case-only diff)",
                    ch = b as char
                ));
            }
            _ => {
                return Err(format!(
                    "git commit OIDs are lowercase hex (`[0-9a-f]`); got non-hex \
                     character {ch:?} at byte {i} (the `:rev` slot's value-shape \
                     contract is a hex commit ID — for refname-shaped pins \
                     (`v0.1.0`, `main`, `feature/checkout`) use `:tag` or \
                     `:branch`, not `:rev`; the substrate's `is_git_ref_name` \
                     and `is_git_oid` predicates partition the `:fonte` axes \
                     structurally, so a cross-axis mis-slot lands at the \
                     offending axis's predicate, not at clone time)",
                    ch = b as char
                ));
            }
        }
    }
    // Null-OID sentinel pre-emption — the all-zero hex string is git's
    // canonical "no commit" sentinel (used in `git update-ref` /
    // pre-receive hook flows as the old-value side of ref-create and the
    // new-value side of ref-delete) and never names a real commit in any
    // repo's object database. A `:rev "0000000000000000000000000000000000000000"`
    // (SHA-1 width) or `:rev "0000…0000"` (SHA-256 width) is the canonical
    // "I copy-pasted the no-such-commit sentinel out of `git
    // update-ref --stdin` docs / pre-receive hook example" footgun: it's
    // shape-valid hex of canonical width but resolves to nothing at
    // `git fetch <remote> 0000…0000` time and surfaces as a fetch failure
    // far from the source caixa.lisp, with the lacre's
    // content-addressing probe locked to a non-resolvable `git:0000…0000`
    // closure that never equals any upstream's actual `HEAD`. Rejecting
    // at the predicate keeps the `:rev` slot's accepted set aligned with
    // its documented reproducibility contract — "exactly one immutable
    // commit, forever" — by structurally refusing the only OID-shaped
    // value the contract cannot uphold (no commit means no immutable
    // resolution). Same pre-emption shape `is_git_ref_name`'s canonical-
    // OID-shape pre-emption arm (caixa-core/src/render.rs:1322) carries
    // — both predicates carry one self-aware arm that catches values
    // structurally valid for the alphabet but operationally meaningless
    // on the typed axis.
    if s.bytes().all(|b| b == b'0') {
        return Err(format!(
            "must not be the all-zero null-OID sentinel ({len} `0` \
             characters — git's canonical `no-such-commit` value used by \
             `git update-ref` / pre-receive hook flows to indicate ref \
             create/delete; no commit in any object database has this OID, \
             so the resolver's `git fetch <remote> 0000…0000` would fail \
             far from the source caixa.lisp and the lacre would lock to a \
             `git:0000…0000` closure that never equals any upstream's \
             actual `HEAD`. The `:rev` slot's reproducibility contract \
             requires a *real* commit OID — the canonical authoring shape \
             is the lowercase-hex value `git rev-parse HEAD` emits for an \
             actual commit, like `\"c99fdb36abc7d3e1f4a5b6789012345678901234\"`)"
        ));
    }
    Ok(())
}

/// `:fonte (:tipo git :repo …)` value max length, in bytes — a generous
/// URL-shaped cap covering every documented author surface (the
/// `github:org/repo` shorthand, the `https://` / `ssh://` / `git://` /
/// `file://` URL schemes, the `git@host:path` scp-style SSH form). The
/// cap mirrors the conservative ceiling typical HTTP gateways and git
/// porcelain entries enforce on URL inputs (the OWASP-recommended URL
/// max of 2048 bytes); a `:repo` value above this bound is structurally
/// untenable on every realistic landing site — the caixa-resolver's
/// `git clone <repo>` invocation, the future M4
/// `mesh.pleme.io/v1alpha1/Caixa` CR materializer's per-dep `repo:`
/// axis, the future lacre BLAKE3 closure's resolved-repo identity — and
/// a value of that length is almost certainly a paste-from-binary slug
/// or a multi-line blob that landed in the slot.
///
/// Lifted as a typed `pub const` (rather than an inline literal at the
/// [`is_git_repo_url`] call site) so a future axis reaching for the same
/// bound (the future lacre-side resolved-repo gate, the M4 CR
/// materializer's per-dep `repo:` admission webhook) reads from one
/// place. Same shape every other typed bound in this module carries
/// ([`DNS_1123_LABEL_MAX_LEN`], [`GATEWAY_API_HTTP_PATH_MAX_LEN`],
/// [`NATS_SUBJECT_MAX_LEN`], [`WASI_KV_SLOT_MAX_LEN`],
/// [`GIT_REF_NAME_MAX_LEN`]).
pub const GIT_REPO_URL_MAX_LEN: usize = 2048;

/// Predicate: assert that `s` is a value-shape-valid `:fonte (:tipo git
/// :repo …)` value — the canonical shape every typed `:deps :fonte`
/// (and future `:deps-dev :fonte`) git-source carries. The contract —
/// modeled on the intersection of (a) the git porcelain's URL-parser
/// accepted set the caixa-resolver invokes at `git clone <repo>` time,
/// (b) the OWASP URL-shape guidance for author-surface inputs that flow
/// to a CLI subprocess, and (c) the typed slot's documented accepted
/// shapes ([`crate::DepSource::Git`] doc comment: `github:org/repo`
/// shorthand, `https://…` / `ssh://…` / `git://…` / `file://…` URL
/// schemes, `git@host:path` scp-style SSH):
///
///   - 1..=[`GIT_REPO_URL_MAX_LEN`] (2048) bytes;
///   - must not start with `-` (the canonical CLI-argument-injection
///     footgun — `git clone <repo>` interprets a leading `-` as a CLI
///     flag, so a `:repo "-upload-pack=evil"` value escapes the
///     subprocess argument boundary and runs an attacker-controlled
///     command; the `--` separator workaround does not fix the typed
///     slot's accepted set, the gate rejects the shape upstream);
///   - no whitespace (space, tab) — every documented form is a single
///     token without whitespace; a `:repo "github:p/x "` (trailing
///     space, paste-from-doc) silently passes the empty check and
///     surfaces at `git clone` time with a quoting-confused error far
///     from the source caixa.lisp;
///   - no ASCII control characters (`0x00..=0x1F`, `0x7F`) — the `\r`
///     / `\n` arms are the canonical "the paste-from-multiline-doc
///     spans multiple lines" footgun, and CRLF injection at the URL
///     boundary is a class of subprocess-arg attack;
///   - no non-ASCII bytes (`>= 0x80`) — IDN hosts must be pre-encoded
///     as Punycode (`xn--…`); raw non-ASCII silently breaks at git's
///     URL parser and may round-trip inconsistently across NFC/NFD
///     normalization on APFS / case-folding filesystems, the same
///     intersection-floor [`is_git_ref_name`] enforces on the peer
///     refname axes;
///   - no `#` URL-fragment-identifier byte (RFC 3986 §3.5) — every
///     documented `:repo` shape (`github:org/repo` shorthand,
///     `https://…` / `ssh://…` / `git://…` / `file://…` URL schemes,
///     `git@host:path` scp-style SSH) carries none; libcurl's URL
///     parser (the layer `git clone <https-url>` invokes) and git's
///     own URL handlers strip the `#fragment` tail before opening
///     the transport, so the byte rides verbatim into the lacre's
///     per-dep content-address (`conteudo: format!("git:{repo}…")`,
///     caixa-resolver/src/resolve.rs) but is silently dropped on the
///     wire — two repos whose values differ only in their fragment
///     anchor (`":repo "https://github.com/foo/bar#readme"` vs
///     `":repo "https://github.com/foo/bar#L42"`) resolve to the
///     byte-identical upstream `git clone` but lock to two distinct
///     BLAKE3 closures, defeating the THEORY.md §V.2 render-
///     determinism contract. The canonical "I copy-pasted the
///     permalink-to-line / anchor-to-README URL out of the browser
///     address bar and forgot to trim the `#`-tail" footgun, and the
///     symmetric "I confused the Nix flake-ref idiom (`github:foo/
///     bar#packageName`) with the bare git `:repo` shape" footgun;
///     `:repo` is a git URL, not a Nix flake reference, so the `#`-
///     suffix is structurally meaningless on this axis;
///   - no `?` URL-query-component byte (RFC 3986 §3.4) — every
///     documented `:repo` shape (`github:org/repo` shorthand,
///     `https://…` / `ssh://…` / `git://…` / `file://…` URL schemes,
///     `git@host:path` scp-style SSH) carries none; GitHub /
///     GitLab / Bitbucket all silently ignore the `?query` tail on
///     a repo URL (the canonical `https://github.com/foo/bar?
///     tab=readme-ov-file` browser-tab deep-link, the `?ref=main`
///     GitHub-tree-URL parameter, the `?utm_source=…` campaign-
///     tracker shape every social-share / newsletter / Slack
///     unfurl appends) and serve the same repo regardless, so the
///     byte rides verbatim into the lacre's per-dep content-
///     address but is silently masked at the wire — two repos
///     whose values differ only in their query tail
///     (`":repo "https://github.com/foo/bar?tab=readme-ov-file"` vs
///     `":repo "https://github.com/foo/bar?utm_source=twitter"`)
///     resolve to the byte-identical upstream `git clone` but lock
///     to two distinct BLAKE3 closures, defeating the THEORY.md
///     §V.2 render-determinism contract on the same axis the `#`
///     fragment arm closes. The Smart-HTTP transport (the layer
///     `git clone <https-url>` uses) appends its own
///     `?service=git-upload-pack` query internally; an
///     author-supplied `?` byte additionally collides with that
///     internal axis at every git porcelain entry-point. The
///     canonical "I copy-pasted the GitHub tree-URL out of the
///     browser address bar and forgot to trim the `?tab=…` /
///     `?ref=…` tail" footgun, peer with the `#` fragment arm on
///     the same paste-from-browser-address-bar trajectory;
///   - no embedded `\` byte (RFC 3986 §3.3 reserves `/` as the path-
///     segment separator; no URL grammar admits `\`) — every
///     documented `:repo` shape (`github:org/repo` shorthand,
///     `https://…` / `ssh://…` / `git://…` / `file://…` URL schemes,
///     `git@host:path` scp-style SSH) uses `/` as the path separator.
///     The canonical Windows-path-confusion footgun: an author
///     pastes `file:///C:\Users\me\repo` from a Windows Explorer
///     address bar / PowerShell `Get-Location` output, or
///     `https://github.com\foo\bar` after a Win32 shell mangled
///     the slashes, or the bare Windows-rooted path `C:\repo` into
///     a slot expecting a `file://` URL. libcurl's URL parser
///     (the layer `git clone <https-url>` invokes) silently
///     translates `\` → `/` on some platforms and refuses it on
///     others — the byte rides verbatim into the lacre's per-dep
///     content-address but is silently rewritten or rejected at
///     the wire, defeating the THEORY.md §V.2 render-determinism
///     contract on the same axis the `#` fragment / `?` query arms
///     close. The peer [`DepError::FonteCaminhoBackslash`] arm
///     (commit 3a4e1d7) closes the same byte on the sibling
///     `:fonte :caminho` path-fonte axis; this arm closes the
///     URL-grammar axis so every byte past `is_git_repo_url`
///     reaches `git clone`'s wire-format intact;
///   - no embedded `{` / `}` byte — RFC 3986 §2 excludes the pair
///     from URL syntax (they sit in the 'delims' / 'unwise' byte
///     set every URL parser is required to refuse or percent-
///     encode), and RFC 6570 reserves the matched pair for URI
///     Template placeholders (the canonical
///     `https://{host}/{org}/{repo}` substitution shape every
///     `OpenAPI` / Swagger / Postman / GitHub Octokit client library
///     / Helm chart-URL fragment carries). The canonical 'I forgot
///     to resolve the template placeholder' footgun: an author
///     pastes `:repo "https://github.com/{org}/{repo}"` from a
///     README quick-start snippet, an `OpenAPI` `servers:` URL, a
///     Helm chart's `home:` template, or the Mustache / Handlebars
///     `{{org}}/{{repo}}` doubled-brace substitution form every
///     CI / `IaC` templating engine emits, expecting the substrate
///     to resolve the placeholder downstream. libcurl percent-
///     encodes `{` / `}` to `%7B` / `%7D` on the wire so the byte
///     round-trips inconsistently between the lacre's per-dep
///     content-address and the resolver's `git clone <repo>`
///     invocation, defeating the THEORY.md §V.2 render-
///     determinism contract on the same axis the `#` fragment /
///     `?` query / `\` backslash arms close; every git porcelain
///     entry-point additionally fetches a nonexistent
///     `{placeholder}`-named path far from the source caixa.lisp;
///   - no embedded `<` / `>` byte — RFC 3986 §2 excludes the pair
///     from URL syntax under the same 'delims' / 'unwise' banner the
///     `{` / `}` arm cites, and no git URL grammar admits either byte:
///     the WHATWG URL spec's 'fragment percent-encode set' maps `<`
///     → `%3C` and `>` → `%3E` so every conformant URL parser
///     refuses or rewrites the literal byte on the wire. Beyond the
///     URL-grammar violation, every POSIX shell lexes `<` as the
///     input-redirection operator and `>` as the output-redirection
///     operator — the canonical paste-from-shell-prompt footgun the
///     peer [`DepError::FonteCaminhoShellRedirection`] arm
///     (commit e457141) closes on the sibling `:fonte :caminho`
///     path-fonte axis. The byte rides verbatim into the lacre's
///     per-dep content-address while libcurl percent-encodes it on
///     the wire — two authors whose `:repo` values differ only in
///     `<`/`>` presence resolve to the byte-identical upstream
///     `git clone` but lock to two distinct BLAKE3 closures,
///     defeating the THEORY.md §V.2 render-determinism contract on
///     the same axis the `#` fragment / `?` query / `\` backslash /
///     `{` / `}` template arms close;
///   - no embedded `` ` `` (backtick) byte — RFC 3986 §2 lists the
///     backtick in the 'delims' / 'unwise' set every URL parser is
///     required to refuse or percent-encode, and no git URL grammar
///     admits the byte: the WHATWG URL spec's 'fragment percent-
///     encode set' maps `` ` `` → `%60` so every conformant URL
///     parser refuses or rewrites the literal byte on the wire.
///     Beyond the URL-grammar violation, every POSIX shell lexes the
///     backtick as the legacy command-substitution operator
///     (`` `<cmd>` `` runs `<cmd>` in a subshell and substitutes its
///     stdout) — the canonical paste-from-shell-prompt RCE-class
///     footgun the peer [`crate::DepError::FonteCaminhoShellCommandSubstitution`]
///     arm (commit c4d62b3) closes on the sibling `:fonte :caminho`
///     path-fonte axis. The byte rides verbatim into the lacre's
///     per-dep content-address while libcurl percent-encodes it on
///     the wire — two authors whose `:repo` values differ only in
///     backtick presence resolve to the byte-identical upstream `git
///     clone` but lock to two distinct BLAKE3 closures, defeating
///     the THEORY.md §V.2 render-determinism contract on the same
///     axis the `#` fragment / `?` query / `\` backslash / `{` / `}`
///     template / `<` / `>` shell-redirection arms close;
///   - must contain a `:` separator at a non-leading position — every
///     documented form carries one (`github:org/repo`, `https://…`,
///     `ssh://…`, `git://…`, `file://…`, `git@host:path`); the
///     bare `org/repo` (no scheme) shape is ambiguous (could be a
///     filesystem path or a missing scheme) and silently passes
///     downstream git porcelain as a local relative path rather than
///     the intended GitHub-shorthand expansion. A leading `:` (`":foo"`)
///     is the canonical "empty scheme" footgun and is rejected too.
///
/// Returns the parser-shaped reason on rejection (without wrapping in
/// any error variant) so each per-axis caller — [`crate::DepError::FonteRepoShape`]
/// at validate time on the `:fonte :repo` axis, the future per-pin gate
/// on `lacre.lisp` resolved-repo axes, the future M4 per-dep CR
/// materializer's per-repo validator — wraps the same reason in its
/// own typed `*Invalid { axis, reason }` variant. The reason wording is
/// axis-agnostic ("git repo URLs reject whitespace") so every call site
/// reading the same diagnostic points at the same rule; drift between
/// any two axes' rule enforcement is a build error visible at this
/// predicate, not a per-resolver "this passed validate but `git clone`
/// rejected" surprise.
///
/// Empty input is rejected here (defensively) and at each call site via
/// the narrower [`crate::DepError::FonteRepoEmpty`] variant — the same
/// empty-first cascade [`is_dns_1123_label`], [`is_gateway_api_http_path`],
/// [`is_wit_world_ref`], [`is_nats_subject`], [`is_wasi_keyvalue_slot`],
/// [`is_git_ref_name`], and [`is_git_oid`] all carry.
///
/// Lifted as the seventh value-shape primitive in this module, peer with
/// [`is_git_ref_name`] (the `:fonte :tag` / `:fonte :branch` refname-
/// shaped axes) and [`is_git_oid`] (the `:fonte :rev` commit-OID axis) —
/// together they bracket the typed `:fonte` slot end-to-end: the
/// `:repo` URL axis (gate here), the refname-pin axes (gate via
/// `is_git_ref_name`), the OID-pin axis (gate via `is_git_oid`). Every
/// validated `:fonte (:tipo git …)` past `DepSource::validate` is
/// guaranteed-acceptable by the caixa-resolver's `git clone`/`git
/// fetch`/`git checkout` invocations, structurally — the parser-of-
/// record divergence the prior trajectory closed on the pin axes is
/// now closed on the last unsealed `:fonte` axis.
///
/// # Errors
///
/// Returns the parser-shaped reason naming the specific violation
/// (length / leading-`-` / whitespace / control-char / non-ASCII /
/// fragment-`#` / query-`?` / backslash-`\` / template-`{`-or-`}` /
/// shell-redirection-`<`-or-`>` / shell-command-substitution-backtick /
/// missing-`:` separator / leading-`:`), without wrapping in any error
/// variant — every caller maps the same `String` into its own typed
/// `*Invalid { axis, reason }` enum variant.
#[allow(
    clippy::too_many_lines,
    reason = "the per-byte rejection cascade is structurally flat by design — \
              every arm carries its own self-locating diagnostic with the offending \
              byte named verbatim plus the canonical paste-from-shape footgun the \
              gate closes, so collapsing onto a single shared `for &b in …` loop \
              would regress the per-arm `feira lint` consumer surface — peer with \
              the `clippy::too_many_lines` allow on `DepSource::validate_caminho` \
              (caixa-core/src/dep.rs:323) on the same cascade-shape rationale"
)]
pub fn is_git_repo_url(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("must not be empty".to_string());
    }
    if s.len() > GIT_REPO_URL_MAX_LEN {
        return Err(format!(
            "exceeds git repo URL max length of {GIT_REPO_URL_MAX_LEN} bytes \
             (got {} bytes; legitimate `github:org/repo` shorthands and \
             `https://…` / `ssh://…` / `git://…` / `file://…` URLs rarely \
             exceed ~128 bytes — this length suggests a paste-from-binary or \
             multi-line blob landed in the `:repo` slot)",
            s.len()
        ));
    }
    if s.starts_with('-') {
        return Err(
            "must not start with `-` (the canonical CLI-argument-injection \
             footgun — `git clone <repo>` interprets a leading `-` as a CLI \
             flag, so a `-upload-pack=…` / `--config=…` value escapes the \
             subprocess argument boundary; use a scheme prefix like \
             `github:org/repo`, `https://host/path`, `ssh://[user@]host/path`, \
             `git://host/path`, `git@host:path`, or `file:///path` for the \
             intended source)"
                .to_string(),
        );
    }
    for &b in s.as_bytes() {
        if b == b' ' || b == b'\t' {
            return Err(format!(
                "must not contain whitespace character {ch:?} (git repo URLs \
                 are single tokens with no whitespace — a trailing space in a \
                 `:repo` value is the canonical paste-from-doc footgun, \
                 silently breaking `git clone '<value> '` at clone time)",
                ch = b as char
            ));
        }
        if b < 0x20 || b == 0x7F {
            return Err(format!(
                "must not contain control character 0x{b:02x} (git repo URLs \
                 are printable ASCII; `\\r` / `\\n` are the canonical paste-\
                 from-multiline-doc footgun and break git's URL parser at \
                 every porcelain entry point, plus CRLF at the URL boundary \
                 is a class of subprocess-arg injection)"
            ));
        }
        if b >= 0x80 {
            return Err(format!(
                "must not contain non-ASCII byte 0x{b:02x} (IDN hosts must be \
                 pre-encoded as Punycode `xn--…`; raw non-ASCII silently \
                 breaks at git's URL parser and round-trips inconsistently \
                 across NFC/NFD normalization on APFS / case-folding \
                 filesystems)"
            ));
        }
        if b == b'#' {
            return Err("must not contain `#` (RFC 3986 §3.5 URL fragment \
                 identifier; libcurl's URL parser — the layer `git \
                 clone <https-url>` invokes — strips the `#fragment` \
                 tail before opening the transport, so the byte rides \
                 verbatim into the lacre's per-dep content-address but \
                 is silently dropped on the wire, defeating the \
                 THEORY.md §V.2 render-determinism contract: two \
                 authors whose `:repo` values differ only in their \
                 fragment anchor (`#readme` vs `#L42`) resolve to the \
                 byte-identical upstream `git clone` but lock to two \
                 distinct BLAKE3 closures. The canonical \
                 paste-from-browser-address-bar footgun (every web URL \
                 to a README section / line-permalink carries one), \
                 and the canonical \"I confused the Nix flake-ref \
                 idiom (`github:foo/bar#packageName`) with the bare \
                 git `:repo` shape\" footgun — `:repo` is a git URL, \
                 not a Nix flake reference, so the `#`-suffix is \
                 structurally meaningless on this axis. Drop the \
                 `#fragment` tail; pin the ref via the typed `:tag` / \
                 `:branch` / `:rev` slot instead)"
                .to_string());
        }
        if b == b'?' {
            return Err("must not contain `?` (RFC 3986 §3.4 URL query \
                 component; every documented `:fonte :repo` shape \
                 (`github:org/repo` shorthand, `https://…` / \
                 `ssh://…` / `git://…` / `file://…` URL schemes, \
                 `git@host:path` scp-style SSH) carries none. GitHub / \
                 GitLab / Bitbucket all silently ignore the `?query` \
                 tail on a repo URL and serve the same repo \
                 regardless, so the byte rides verbatim into the \
                 lacre's per-dep content-address but is silently \
                 masked at the wire — two authors whose `:repo` \
                 values differ only in their query tail \
                 (`?tab=readme-ov-file` vs `?utm_source=twitter`) \
                 resolve to the byte-identical upstream `git clone` \
                 but lock to two distinct BLAKE3 closures, defeating \
                 the THEORY.md §V.2 render-determinism contract on \
                 the same axis the fragment-`#` arm closes. The \
                 Smart-HTTP transport (the layer \
                 `git clone <https-url>` uses) additionally appends \
                 its own `?service=git-upload-pack` query internally; \
                 an author-supplied `?` byte collides with that \
                 internal axis at every git porcelain entry-point. \
                 The canonical paste-from-browser-address-bar \
                 footgun (`?tab=readme-ov-file` GitHub-tab deep-link, \
                 `?ref=main` GitHub-tree-URL parameter, \
                 `?utm_source=…` campaign-tracker every social-share / \
                 newsletter / Slack-unfurl appends). Drop the \
                 `?query` tail; pin the ref via the typed `:tag` / \
                 `:branch` / `:rev` slot instead)"
                .to_string());
        }
        if b == b'\\' {
            return Err("must not contain `\\` (RFC 3986 §3.3 reserves \
                 `/` as the URL path-segment separator; no URL grammar \
                 admits `\\`. Every documented `:fonte :repo` shape \
                 (`github:org/repo` shorthand, `https://…` / \
                 `ssh://…` / `git://…` / `file://…` URL schemes, \
                 `git@host:path` scp-style SSH) uses `/` as the path \
                 separator. The canonical Windows-path-confusion \
                 footgun: an author pastes `file:///C:\\Users\\me\\repo` \
                 from a Windows Explorer address bar / PowerShell \
                 `Get-Location` output, `https://github.com\\foo\\bar` \
                 after a Win32 shell mangled the slashes, or the bare \
                 Windows-rooted path `C:\\repo` into a slot expecting a \
                 `file://` URL. libcurl's URL parser (the layer \
                 `git clone <https-url>` invokes) silently translates \
                 `\\` to `/` on some platforms and refuses it on others, \
                 so the byte rides verbatim into the lacre's per-dep \
                 content-address but is silently rewritten or rejected \
                 at the wire, defeating the THEORY.md §V.2 render-\
                 determinism contract on the same axis the fragment-`#` \
                 and query-`?` arms close. The peer \
                 `DepError::FonteCaminhoBackslash` arm (commit 3a4e1d7) \
                 closes the same byte on the sibling `:fonte :caminho` \
                 path-fonte axis; this arm closes the URL-grammar axis. \
                 Drop the `\\` — use `/` for URL path separators, or \
                 author the `file:///C:/path` form with forward slashes \
                 (the canonical RFC 8089 file-URI shape on Windows-\
                 rooted paths))"
                .to_string());
        }
        if b == b'{' || b == b'}' {
            return Err(format!(
                "must not contain `{ch}` (RFC 3986 §2 excludes `{{` / `}}` \
                 from URL syntax — they sit in the 'delims' / 'unwise' \
                 byte set every URL parser is required to refuse or \
                 percent-encode; RFC 6570 reserves the matched pair for \
                 URI Template placeholders (the canonical \
                 `https://{{host}}/{{org}}/{{repo}}` substitution shape \
                 every OpenAPI / Swagger / Postman / GitHub Octokit \
                 client library / Helm chart-URL fragment carries). The \
                 canonical 'I forgot to resolve the template \
                 placeholder' footgun: an author pastes \
                 `:repo \"https://github.com/{{org}}/{{repo}}\"` from a \
                 README's quick-start snippet, an OpenAPI spec's \
                 `servers:` URL, a Helm chart's `home:` template, or \
                 the Mustache / Handlebars `{{{{org}}}}/{{{{repo}}}}` \
                 doubled-brace substitution form every CI / IaC \
                 templating engine emits, expecting the substrate to \
                 resolve the placeholder downstream. libcurl percent-\
                 encodes `{{` / `}}` to `%7B` / `%7D` on the wire (so \
                 the byte round-trips inconsistently between the \
                 lacre's per-dep content-address and the resolver's \
                 `git clone <repo>` invocation, defeating the THEORY.md \
                 §V.2 render-determinism contract on the same axis the \
                 fragment-`#`, query-`?`, and backslash-`\\` arms close) \
                 while every git porcelain entry-point fetches a \
                 nonexistent literal-`{{placeholder}}`-named path far \
                 from the source caixa.lisp. Resolve the placeholder at \
                 author time — substitute the literal org / repo name \
                 (`https://github.com/pleme-io/hello-rio`), or use \
                 `:fonte (:tipo path :caminho \"<local-path>\")` for a \
                 local workspace dep)",
                ch = b as char
            ));
        }
        if b == b'<' || b == b'>' {
            return Err(format!(
                "must not contain `{ch}` (RFC 3986 §2 excludes `<` / `>` \
                 from URL syntax — they sit in the 'delims' / 'unwise' \
                 byte set every URL parser is required to refuse or \
                 percent-encode, peer with the `{{` / `}}` URI Template \
                 arm on the same paragraph of the same RFC. No git URL \
                 grammar admits either byte: the `github:org/repo` \
                 shorthand carries an alphanumeric / `-` / `_` / `/` \
                 alphabet, every `https://` / `ssh://` / `git://` / \
                 `file://` URL scheme percent-encodes `<` to `%3C` and \
                 `>` to `%3E` on the wire (the WHATWG URL spec's \
                 'fragment percent-encode set' canonical mapping every \
                 conformant URL parser applies), and the `git@host:path` \
                 scp-style SSH shape names a POSIX path component that \
                 carries no shell-metachar bytes. Beyond the URL-grammar \
                 violation, every POSIX shell (sh / bash / zsh / dash / \
                 ksh / fish / nushell) lexes `<` as the input-redirection \
                 operator and `>` as the output-redirection operator — \
                 a `:repo \"https://github.com/foo/bar>build.log\"` (the \
                 canonical 'I pasted a shell pipeline that wrote build \
                 output and forgot to trim the redirect' footgun) or \
                 `:repo \"<README.md\"` (the symmetric input-redirection \
                 paste idiom every doc-quick-start `git clone <…>` line \
                 footnotes) is the canonical paste-from-shell-prompt \
                 footgun the typed slot's accepted set must exclude. The \
                 byte rides verbatim into the lacre's per-dep content-\
                 address (`conteudo: format!(\"git:{{repo}}\")` peer of \
                 the path-axis embedding at caixa-resolver/src/resolve.rs:189) \
                 and into the resolver's `git clone <repo>` \
                 (caixa-resolver/src/git.rs:21) subprocess invocation, \
                 where libcurl's URL parser percent-encodes the byte on \
                 the wire — so two authors whose `:repo` values differ \
                 only in their `<`/`>` presence (one paste-trimmed the \
                 redirect tail, the other didn't) resolve to the byte-\
                 identical upstream `git clone` but lock to two distinct \
                 BLAKE3 closures, defeating the THEORY.md §V.2 render-\
                 determinism contract on the same axis the fragment-`#`, \
                 query-`?`, backslash-`\\`, and template-`{{` / `}}` arms \
                 close. The peer `:fonte :caminho` axis (e457141) closes \
                 the same `<` / `>` byte under the shell-redirection \
                 banner via `DepError::FonteCaminhoShellRedirection`; the \
                 peer `:entrada :paths` axis closes the same bytes as part \
                 of `is_gateway_api_http_path`'s eleven-byte RFC-3986-\
                 reserved set; the peer `:fonte :tag` / `:fonte :branch` \
                 axes (e70d213) close the same bytes as part of \
                 `is_git_ref_name`'s shell-metachar-injection cascade. \
                 The `:repo` URL axis was the last typed git-source \
                 surface still admitting these two bytes; this arm closes \
                 the gap so the substrate-wide 'no shell-redirection / \
                 RFC-3986-unwise byte anywhere in a typed git-source slot' \
                 invariant is now structurally consistent across every \
                 git-source-shaped typed surface. Drop the `<` / `>` tail \
                 — pin the ref via the typed `:tag` / `:branch` / `:rev` \
                 slot, or use `:fonte (:tipo path :caminho \"<local-path>\")` \
                 for a local workspace dep)",
                ch = b as char
            ));
        }
        if b == b'`' {
            return Err(
                "must not contain `` ` `` (RFC 3986 §2 lists the backtick byte \
                 in the 'delims' / 'unwise' set every URL parser is required \
                 to refuse or percent-encode, peer with the `<` / `>` \
                 shell-redirection arm on the same paragraph of the same RFC. \
                 No git URL grammar admits the byte: the `github:org/repo` \
                 shorthand carries an alphanumeric / `-` / `_` / `/` alphabet, \
                 every `https://` / `ssh://` / `git://` / `file://` URL scheme \
                 percent-encodes `` ` `` to `%60` on the wire (the WHATWG URL \
                 spec's 'fragment percent-encode set' canonical mapping every \
                 conformant URL parser applies), and the `git@host:path` \
                 scp-style SSH shape names a POSIX path component that \
                 carries no shell-metachar bytes. Beyond the URL-grammar \
                 violation, every POSIX shell (sh / bash / zsh / dash / ksh / \
                 fish) lexes the backtick as the legacy command-substitution \
                 operator — `` `<cmd>` `` runs `<cmd>` in a subshell and \
                 substitutes its stdout, the canonical RCE-class injection \
                 vector when a string lands in a shell context. A `:repo \
                 \"https://github.com/foo/`whoami`/bar\"` (the canonical \
                 paste-from-shell-prompt footgun where the author copies a \
                 backtick-templated URL from a doc / README quick-start \
                 snippet that expected the substrate to substitute the value \
                 downstream) or the symmetric `:repo \"`git config user.name`\"` \
                 (the dynamic-config-substitution paste idiom every \
                 dev-environment-setup script footnotes) is the canonical \
                 paste-from-shell-prompt footgun the typed slot's accepted \
                 set must exclude. The byte rides verbatim into the lacre's \
                 per-dep content-address (`conteudo: format!(\"git:{repo}\")` \
                 peer of the path-axis embedding at \
                 caixa-resolver/src/resolve.rs) and into the resolver's `git \
                 clone <repo>` (caixa-resolver/src/git.rs) subprocess \
                 invocation, where libcurl's URL parser percent-encodes the \
                 byte on the wire — so two authors whose `:repo` values \
                 differ only in their backtick presence (one paste-trimmed \
                 the substitution wrapper, the other didn't) resolve to the \
                 byte-identical upstream `git clone` but lock to two distinct \
                 BLAKE3 closures, defeating the THEORY.md §V.2 render-\
                 determinism contract on the same axis the fragment-`#`, \
                 query-`?`, backslash-`\\`, template-`{` / `}`, and \
                 shell-redirection-`<` / `>` arms close. The peer `:fonte \
                 :caminho` axis (c4d62b3) closes the same byte under the \
                 shell-command-substitution banner via \
                 `DepError::FonteCaminhoShellCommandSubstitution`; the peer \
                 `:entrada :paths` axis closes the same byte as part of \
                 `is_gateway_api_http_path`'s eleven-byte RFC-3986-reserved \
                 set. Drop the backtick wrapper — substitute the literal \
                 value at author time, or use `:fonte (:tipo path :caminho \
                 \"<local-path>\")` for a local workspace dep)"
                    .to_string(),
            );
        }
        if b == b'|' {
            return Err("must not contain `|` (RFC 3986 §2 lists the pipe byte in \
                 the 'unwise' set every URL parser is required to refuse or \
                 percent-encode, peer with the `{` / `}` URI Template, \
                 `<` / `>` shell-redirection, and `` ` `` shell-command-\
                 substitution arms on the same paragraph of the same RFC. \
                 No git URL grammar admits the byte: the `github:org/repo` \
                 shorthand carries an alphanumeric / `-` / `_` / `/` \
                 alphabet, every `https://` / `ssh://` / `git://` / \
                 `file://` URL scheme percent-encodes `|` to `%7C` on the \
                 wire (the WHATWG URL spec's 'fragment percent-encode set' \
                 canonical mapping every conformant URL parser applies), \
                 and the `git@host:path` scp-style SSH shape names a POSIX \
                 path component that carries no shell-metachar bytes. \
                 Beyond the URL-grammar violation, every POSIX shell (sh / \
                 bash / zsh / dash / ksh / fish / nushell) lexes `|` as the \
                 pipe operator — `<cmd1> | <cmd2>` streams cmd1's stdout to \
                 cmd2's stdin, the canonical command-chaining injection \
                 vector when a string lands in a shell context. A `:repo \
                 \"https://github.com/foo/bar|tee build.log\"` (the \
                 canonical 'I pasted a shell pipeline that tee'd build \
                 output and forgot to trim the pipe tail' footgun) or \
                 `:repo \"github:p/x|cat\"` (the symmetric paste-from-\
                 shell-prompt idiom every quick-start `git clone <…> | …` \
                 line footnotes) is the canonical paste-from-shell-prompt \
                 footgun the typed slot's accepted set must exclude. The \
                 byte rides verbatim into the lacre's per-dep content-\
                 address (`conteudo: format!(\"git:{repo}\")` peer of the \
                 path-axis embedding at caixa-resolver/src/resolve.rs) and \
                 into the resolver's `git clone <repo>` \
                 (caixa-resolver/src/git.rs) subprocess invocation, where \
                 libcurl's URL parser percent-encodes the byte on the wire \
                 — so two authors whose `:repo` values differ only in \
                 their pipe presence (one paste-trimmed the pipeline tail, \
                 the other didn't) resolve to the byte-identical upstream \
                 `git clone` but lock to two distinct BLAKE3 closures, \
                 defeating the THEORY.md §V.2 render-determinism contract \
                 on the same axis the fragment-`#`, query-`?`, backslash-\
                 `\\`, template-`{` / `}`, shell-redirection-`<` / `>`, \
                 and backtick-`` ` `` arms close. The peer `:fonte \
                 :caminho` axis (124106f) closes the same byte under the \
                 shell-pipe banner via `DepError::FonteCaminhoShellPipe`; \
                 the peer `:entrada :paths` axis closes the same byte as \
                 part of `is_gateway_api_http_path`'s eleven-byte \
                 RFC-3986-reserved set; the peer `:fonte :tag` / `:fonte \
                 :branch` axes close the same byte as part of \
                 `is_git_ref_name`'s shell-metachar-injection cascade. \
                 Drop the pipe tail — substitute the literal value at \
                 author time, or use `:fonte (:tipo path :caminho \
                 \"<local-path>\")` for a local workspace dep)"
                .to_string());
        }
        if b == b';' {
            return Err("must not contain `;` (RFC 3986 §2 lists the semicolon \
                 byte in the 'sub-delims' / reserved set every URL parser is \
                 required to percent-encode at the path-segment boundary, peer \
                 with the `{` / `}` URI Template, `<` / `>` shell-redirection, \
                 `` ` `` shell-command-substitution, and `|` shell-pipe arms on \
                 the same paragraph of the same RFC. No git URL grammar admits \
                 the byte: the `github:org/repo` shorthand carries an \
                 alphanumeric / `-` / `_` / `/` alphabet, every `https://` / \
                 `ssh://` / `git://` / `file://` URL scheme percent-encodes `;` \
                 to `%3B` on the wire (the WHATWG URL spec's 'fragment percent-\
                 encode set' canonical mapping every conformant URL parser \
                 applies), and the `git@host:path` scp-style SSH shape names a \
                 POSIX path component that carries no shell-metachar bytes. \
                 Beyond the URL-grammar violation, every POSIX shell (sh / \
                 bash / zsh / dash / ksh / fish / nushell) lexes `;` as the \
                 sequential-command terminator — `<cmd1>; <cmd2>` fires `<cmd2>` \
                 regardless of `<cmd1>`'s exit status, the canonical \
                 command-chaining injection vector when a string lands in a \
                 shell context. A `:repo \
                 \"https://github.com/foo/bar; rm -rf build\"` (the canonical \
                 'I pasted a shell one-liner that chained a cleanup tail after \
                 the URL and forgot to trim the `; <cmd>` tail' footgun) or \
                 `:repo \"github:p/x;;y\"` (the symmetric paste-from-POSIX-\
                 `case`-arm `;;` terminator idiom every shell-snippet footnotes) \
                 is the canonical paste-from-shell-prompt footgun the typed \
                 slot's accepted set must exclude. The byte rides verbatim into \
                 the lacre's per-dep content-address (`conteudo: \
                 format!(\"git:{repo}\")` peer of the path-axis embedding at \
                 caixa-resolver/src/resolve.rs) and into the resolver's `git \
                 clone <repo>` (caixa-resolver/src/git.rs) subprocess \
                 invocation, where libcurl's URL parser percent-encodes the \
                 byte on the wire — so two authors whose `:repo` values differ \
                 only in their semicolon presence (one paste-trimmed the \
                 sequential-command tail, the other didn't) resolve to the \
                 byte-identical upstream `git clone` but lock to two distinct \
                 BLAKE3 closures, defeating the THEORY.md §V.2 render-\
                 determinism contract on the same axis the fragment-`#`, \
                 query-`?`, backslash-`\\`, template-`{` / `}`, \
                 shell-redirection-`<` / `>`, backtick-`` ` ``, and \
                 shell-pipe-`|` arms close. The peer `:fonte :caminho` axis \
                 (05c358e) closes the same byte under the shell-command-\
                 separator banner via `DepError::FonteCaminhoShellSemicolon`; \
                 the peer `:entrada :paths` axis closes the same byte as part \
                 of `is_gateway_api_http_path`'s eleven-byte RFC-3986-reserved \
                 set; the peer `:fonte :tag` / `:fonte :branch` axes close the \
                 same byte as part of `is_git_ref_name`'s shell-metachar-\
                 injection cascade. Drop the `;` tail — substitute the literal \
                 value at author time, or use `:fonte (:tipo path :caminho \
                 \"<local-path>\")` for a local workspace dep)"
                .to_string());
        }
        if b == b'&' {
            return Err("must not contain `&` (RFC 3986 §2 lists the ampersand \
                 byte in the 'sub-delims' / reserved set every URL parser is \
                 required to percent-encode at the path-segment boundary, peer \
                 with the `{` / `}` URI Template, `<` / `>` shell-redirection, \
                 `` ` `` shell-command-substitution, `|` shell-pipe, and `;` \
                 shell-command-separator arms on the same paragraph of the same \
                 RFC. The byte is also the canonical RFC 3986 §3.4 URL query \
                 `key=value` pair separator (`?a=1&b=2`), but the prior `?` arm \
                 already excludes any `?query` tail on a `:repo` value — every \
                 documented `:fonte :repo` shape (`github:org/repo` shorthand, \
                 `https://…` / `ssh://…` / `git://…` / `file://…` URL schemes, \
                 `git@host:path` scp-style SSH) carries no query component, so \
                 the `&` byte cannot appear in a legitimate query position past \
                 the `?` gate either. Every `https://` / `ssh://` / `git://` / \
                 `file://` URL scheme percent-encodes `&` to `%26` on the wire \
                 (the WHATWG URL spec's 'fragment percent-encode set' canonical \
                 mapping every conformant URL parser applies), and the \
                 `git@host:path` scp-style SSH shape names a POSIX path \
                 component that carries no shell-metachar bytes. Beyond the \
                 URL-grammar violation, every interactive shell (bash / zsh / \
                 fish / nushell) lexes `&` two ways: single `&` as the \
                 background-task terminator that detaches the prior command \
                 into the background and returns control to the prompt \
                 immediately (the canonical `cmd &` idiom every long-running \
                 pipeline uses), and double `&&` as the logical-AND list \
                 operator that fires the next command only if the prior \
                 command succeeded (the canonical `make && make install` idiom \
                 every build script carries). A `:repo \
                 \"https://github.com/foo/bar & sleep 1\"` (the canonical \
                 'I pasted a `git clone <url> & sleep 1` background-launch \
                 one-liner and forgot to trim the `& <cmd>` tail' footgun) or \
                 `:repo \"github:p/x && echo done\"` (the symmetric \
                 paste-from-shell-prompt `cd path && cmd` build-chain idiom \
                 every quick-start `git clone <…> && cd <…>` line footnotes) \
                 is the canonical paste-from-shell-prompt footgun the typed \
                 slot's accepted set must exclude. The byte rides verbatim \
                 into the lacre's per-dep content-address (`conteudo: \
                 format!(\"git:{repo}\")` peer of the path-axis embedding at \
                 caixa-resolver/src/resolve.rs) and into the resolver's `git \
                 clone <repo>` (caixa-resolver/src/git.rs) subprocess \
                 invocation, where libcurl's URL parser percent-encodes the \
                 byte on the wire — so two authors whose `:repo` values \
                 differ only in their ampersand presence (one paste-trimmed \
                 the background-launch tail, the other didn't) resolve to \
                 the byte-identical upstream `git clone` but lock to two \
                 distinct BLAKE3 closures, defeating the THEORY.md §V.2 \
                 render-determinism contract on the same axis the \
                 fragment-`#`, query-`?`, backslash-`\\`, template-`{` / `}`, \
                 shell-redirection-`<` / `>`, backtick-`` ` ``, shell-pipe-`|`, \
                 and shell-command-separator-`;` arms close. The peer `:fonte \
                 :caminho` axis (e12e4f3) closes the same byte under the \
                 shell-background / logical-AND banner via \
                 `DepError::FonteCaminhoShellBackground`; the peer `:entrada \
                 :paths` axis closes the same byte as part of \
                 `is_gateway_api_http_path`'s eleven-byte RFC-3986-reserved \
                 set; the peer `:fonte :tag` / `:fonte :branch` axes close \
                 the same byte as part of `is_git_ref_name`'s shell-metachar-\
                 injection cascade. Drop the `&` tail — substitute the literal \
                 value at author time, or use `:fonte (:tipo path :caminho \
                 \"<local-path>\")` for a local workspace dep)"
                .to_string());
        }
        if b == b'$' {
            return Err("must not contain `$` (RFC 3986 §2 lists the dollar \
                 byte in the 'sub-delims' / reserved set every URL parser is \
                 required to percent-encode at the path-segment boundary, peer \
                 with the `;` shell-command-separator and `&` shell-background \
                 arms on the same paragraph of the same RFC. No git URL grammar \
                 admits the byte: the `github:org/repo` shorthand carries an \
                 alphanumeric / `-` / `_` / `/` alphabet, every `https://` / \
                 `ssh://` / `git://` / `file://` URL scheme percent-encodes `$` \
                 to `%24` on the wire (the WHATWG URL spec's 'fragment percent-\
                 encode set' canonical mapping every conformant URL parser \
                 applies), and the `git@host:path` scp-style SSH shape names a \
                 POSIX path component that carries no shell-metachar bytes. \
                 Beyond the URL-grammar violation, every POSIX shell (sh / \
                 bash / zsh / dash / ksh / fish / nushell) lexes `$` as the \
                 variable-expansion / command-substitution operator: `$<name>` \
                 / `${{<name>}}` expands a named variable, `$(<cmd>)` runs a \
                 subshell and substitutes its stdout, and `$((<expr>))` \
                 evaluates an arithmetic expression — every form is a \
                 host-layout / environment-state leak when the byte lands in \
                 a value the resolver passes to a shell-spawned subprocess. A \
                 `:repo \"https://github.com/$ORG/caixa-teia\"` (the canonical \
                 'I pasted a shell one-liner that expanded `$ORG` against the \
                 author's local environment and forgot to substitute the \
                 literal org name' footgun, identical to the f4efe9c peer arm \
                 on the sibling `:caminho` axis that closes `\"$HOME/work/…\"` \
                 / `\"${{WORKSPACE}}/…\"`) or `:repo \"github:p/$(whoami)/x\"` \
                 (the symmetric paste-from-shell-prompt command-substitution \
                 idiom every dev-environment-setup script footnotes) is the \
                 canonical paste-from-shell-prompt footgun the typed slot's \
                 accepted set must exclude. The byte rides verbatim into the \
                 lacre's per-dep content-address (`conteudo: \
                 format!(\"git:{repo}\")` peer of the path-axis embedding at \
                 caixa-resolver/src/resolve.rs) and into the resolver's `git \
                 clone <repo>` (caixa-resolver/src/git.rs) subprocess \
                 invocation, where libcurl's URL parser percent-encodes the \
                 byte on the wire — so two authors whose `:repo` values \
                 differ only in their dollar presence (one substituted the \
                 literal value at author time, the other didn't) resolve to \
                 the byte-identical upstream `git clone` but lock to two \
                 distinct BLAKE3 closures, defeating the THEORY.md §V.2 \
                 render-determinism contract on the same axis the \
                 fragment-`#`, query-`?`, backslash-`\\`, template-`{` / `}`, \
                 shell-redirection-`<` / `>`, backtick-`` ` ``, shell-pipe-`|`, \
                 shell-command-separator-`;`, and shell-background-`&` arms \
                 close. Beyond the determinism axis, a value like \
                 `\"github:$HOME/x\"` is a structural host-layout leak: two \
                 authors with the same `:repo` slot but different `$HOME` \
                 / `$WORKSPACE` / `$PWD` resolve different upstream URLs at \
                 different times — the lacre, far from being a substrate-wide \
                 identity, becomes a per-workstation snapshot of the author's \
                 shell environment. The peer `:fonte :caminho` axis (f4efe9c) \
                 closes the leading-`$` byte under the shell-variable-\
                 expansion banner via `DepError::FonteCaminhoVarExpansion`; \
                 the peer `:entrada :paths` axis closes the same byte as part \
                 of `is_gateway_api_http_path`'s eleven-byte RFC-3986-reserved \
                 set; the peer `:fonte :tag` / `:fonte :branch` axes close \
                 the same byte as part of `is_git_ref_name`'s shell-metachar-\
                 injection cascade — the `:caminho` axis closes only the \
                 leading position because absolute / tilde / var arms there \
                 are leading-byte sentinels, but the `:repo` URL axis closes \
                 the byte anywhere because every per-byte arm on this surface \
                 is positional-agnostic (the substitution / leak shapes \
                 `\"https://$HOST/p/x\"` and `\"github:p/$(whoami)\"` both \
                 carry the byte mid-string). Drop the `$` — substitute the \
                 literal value at author time, or use `:fonte (:tipo path \
                 :caminho \"<local-path>\")` for a local workspace dep)"
                .to_string());
        }
        if b == b'*' {
            return Err("must not contain `*` (RFC 3986 §2 lists the asterisk \
                 byte in the 'sub-delims' / reserved set every URL parser is \
                 required to percent-encode at the path-segment boundary, peer \
                 with the `;` shell-command-separator, `&` shell-background, \
                 and `$` shell-variable-expansion arms on the same paragraph of \
                 the same RFC. No git URL grammar admits the byte: the \
                 `github:org/repo` shorthand carries an alphanumeric / `-` / \
                 `_` / `/` alphabet, every `https://` / `ssh://` / `git://` / \
                 `file://` URL scheme percent-encodes `*` to `%2A` on the wire \
                 (the WHATWG URL spec's 'special-query percent-encode set' \
                 canonical mapping every conformant URL parser applies), and \
                 the `git@host:path` scp-style SSH shape names a POSIX path \
                 component that carries no shell-metachar bytes. Beyond the \
                 URL-grammar violation, every POSIX shell (sh / bash / zsh / \
                 dash / ksh / fish / nushell) lexes `*` as the \
                 pathname-expansion / glob wildcard operator: a single `*` \
                 matches any sequence of characters in a path component \
                 (including the empty sequence), `**` matches across `/` \
                 boundaries under bash's `globstar` shopt, and `foo*` resolves \
                 against the cwd-relative filesystem at command-substitution \
                 time. Beyond shell glob semantics, git itself lexes `*` as \
                 the refspec wildcard operator (`refs/heads/*:refs/remotes/\
                 origin/*` — the same byte the peer `is_git_ref_name` \
                 predicate refuses on `:fonte :tag` / `:fonte :branch`), so a \
                 `:repo` value carrying `*` is structurally ambiguous with \
                 every refspec parser the resolver invokes downstream. A \
                 `:repo \"https://github.com/pleme-io/caixa-*\"` (the canonical \
                 'I pasted a `ls github.com/pleme-io/caixa-*` shell-listing \
                 tail and forgot to substitute the literal repo name' \
                 footgun, identical to the cf9034b peer arm on the sibling \
                 `:caminho` axis that closes `\"../caixa-teia/*\"`) or `:repo \
                 \"github:p/*\"` (the symmetric paste-from-shell-prompt \
                 glob-expansion idiom every quick-listing one-liner footnotes) \
                 is the canonical paste-from-shell-prompt footgun the typed \
                 slot's accepted set must exclude. The byte rides verbatim \
                 into the lacre's per-dep content-address (`conteudo: \
                 format!(\"git:{repo}\")` peer of the path-axis embedding at \
                 caixa-resolver/src/resolve.rs) and into the resolver's `git \
                 clone <repo>` (caixa-resolver/src/git.rs) subprocess \
                 invocation, where libcurl's URL parser percent-encodes the \
                 byte on the wire — so two authors whose `:repo` values \
                 differ only in their asterisk presence (one substituted the \
                 literal repo name at author time, the other didn't) resolve \
                 to the byte-identical upstream `git clone` but lock to two \
                 distinct BLAKE3 closures, defeating the THEORY.md §V.2 \
                 render-determinism contract on the same axis the \
                 fragment-`#`, query-`?`, backslash-`\\`, template-`{` / `}`, \
                 shell-redirection-`<` / `>`, backtick-`` ` ``, shell-pipe-`|`, \
                 shell-command-separator-`;`, shell-background-`&`, and \
                 shell-variable-expansion-`$` arms close. The peer `:fonte \
                 :caminho` axis (cf9034b) closes the same byte under the \
                 shell-glob / pathname-expansion banner via \
                 `DepError::FonteCaminhoShellGlob`; the peer `:fonte :tag` / \
                 `:fonte :branch` axes close the same byte as part of \
                 `is_git_ref_name`'s refspec-wildcard cascade. Drop the `*` — \
                 substitute the literal repo name at author time, or use \
                 `:fonte (:tipo path :caminho \"<local-path>\")` for a local \
                 workspace dep)"
                .to_string());
        }
        if b == b'(' || b == b')' {
            return Err(format!(
                "must not contain `{ch}` (RFC 3986 §2 excludes `(` / `)` \
                 from URL syntax — they sit in the 'sub-delims' / reserved \
                 byte set every URL parser is required to percent-encode at \
                 the path-segment boundary, peer with the `;` \
                 shell-command-separator, `&` shell-background, `$` \
                 shell-variable-expansion, and `*` shell-glob arms on the \
                 same paragraph of the same RFC. No git URL grammar admits \
                 either byte: the `github:org/repo` shorthand carries an \
                 alphanumeric / `-` / `_` / `/` alphabet, every `https://` / \
                 `ssh://` / `git://` / `file://` URL scheme percent-encodes \
                 `(` to `%28` and `)` to `%29` on the wire (the WHATWG URL \
                 spec's 'special-query percent-encode set' canonical mapping \
                 every conformant URL parser applies), and the \
                 `git@host:path` scp-style SSH shape names a POSIX path \
                 component that carries no shell-metachar bytes. Beyond the \
                 URL-grammar violation, every POSIX shell (sh / bash / zsh / \
                 dash / ksh / fish / nushell) lexes `(` / `)` as the \
                 subshell-grouping operator: `(<cmd>)` runs `<cmd>` in a \
                 child shell with a fresh environment scope (the canonical \
                 idiom for sandboxing a `cd` or variable assignment), and \
                 `$(<cmd>)` is the modern Bourne command-substitution shape \
                 the prior `$` arm closes the leading byte of — the closing \
                 `)` byte completes that substitution shape and must be \
                 refused on the same axis. The byte pair is additionally the \
                 canonical regex-alternation grouping operator (`(foo|bar)`) \
                 every doc / README quick-start snippet folds into a paste-\
                 from-doc footgun shape, and the bash brace-expansion \
                 alternation form (`{{foo,bar}}`) the prior `{{` / `}}` URI \
                 Template arm closes on the curly-brace axis routes the \
                 same alternation intent through the parenthesis axis on \
                 every POSIX-portable script. A `:repo \
                 \"https://github.com/(foo|bar)/repo\"` (the canonical 'I \
                 pasted a regex-alternation form from a doc / README and \
                 forgot to substitute one literal org' footgun) or `:repo \
                 \"github:p/x(date)\"` (the symmetric paste-from-shell-\
                 prompt subshell-grouping idiom every dynamic-config-\
                 substitution one-liner footnotes) is the canonical paste-\
                 from-shell-prompt footgun the typed slot's accepted set \
                 must exclude. The byte rides verbatim into the lacre's \
                 per-dep content-address (`conteudo: \
                 format!(\"git:{{repo}}\")` peer of the path-axis embedding \
                 at caixa-resolver/src/resolve.rs) and into the resolver's \
                 `git clone <repo>` (caixa-resolver/src/git.rs) subprocess \
                 invocation, where libcurl's URL parser percent-encodes the \
                 byte on the wire — so two authors whose `:repo` values \
                 differ only in their parenthesis presence (one paste-\
                 trimmed the grouping wrapper, the other didn't) resolve to \
                 the byte-identical upstream `git clone` but lock to two \
                 distinct BLAKE3 closures, defeating the THEORY.md §V.2 \
                 render-determinism contract on the same axis the \
                 fragment-`#`, query-`?`, backslash-`\\`, template-`{{` / \
                 `}}`, shell-redirection-`<` / `>`, backtick-`` ` ``, \
                 shell-pipe-`|`, shell-command-separator-`;`, shell-\
                 background-`&`, shell-variable-expansion-`$`, and shell-\
                 glob-`*` arms close. Drop the `(` / `)` wrapper — \
                 substitute the literal value at author time, or use \
                 `:fonte (:tipo path :caminho \"<local-path>\")` for a local \
                 workspace dep)",
                ch = b as char
            ));
        }
        if b == b'"' {
            return Err("must not contain `\"` (RFC 3986 §2 lists the \
                 double-quote byte in the 'delims' set every URL parser is \
                 required to refuse or percent-encode, peer with the `<` / \
                 `>` shell-redirection and `` ` `` shell-command-substitution \
                 arms on the same paragraph of the same RFC — the four-byte \
                 'delims' subset (`<`, `>`, `\"`, `` ` ``) is the strictest \
                 of the §2 reserved classes, every member structurally \
                 incompatible with every URL grammar at every position. No \
                 git URL grammar admits the byte: the `github:org/repo` \
                 shorthand carries an alphanumeric / `-` / `_` / `/` \
                 alphabet, every `https://` / `ssh://` / `git://` / \
                 `file://` URL scheme percent-encodes `\"` to `%22` on the \
                 wire (the WHATWG URL spec's 'C0 control percent-encode \
                 set' canonical mapping every conformant URL parser \
                 applies), and the `git@host:path` scp-style SSH shape \
                 names a POSIX path component that carries no \
                 shell-metachar bytes. Beyond the URL-grammar violation, \
                 every POSIX shell (sh / bash / zsh / dash / ksh / fish / \
                 nushell) lexes `\"` as the double-quote string delimiter — \
                 a `\"<text>\"` form suppresses word-splitting and \
                 pathname-expansion on `<text>` while still expanding `$`, \
                 `` ` ``, and `\\` substitutions inside, the canonical \
                 'quote the URL so the shell doesn't re-lex the bytes' \
                 idiom every doc / README quick-start snippet wraps the \
                 URL argument with. A `:repo \
                 \"\\\"https://github.com/pleme-io/caixa-teia\\\"\"` (the \
                 canonical paste-from-doc footgun where the author copies \
                 `$ git clone \"https://…\"` from a README's quick-start \
                 snippet and keeps the surrounding double-quote bytes — \
                 the doc quotes the URL so the shell doesn't re-lex \
                 metachars inside, but the typed slot is itself a \
                 byte-level string parser, not a shell context, so the \
                 quote bytes ride into the value verbatim) or `:repo \
                 \"github:p/x\\\"tail\"` (the symmetric stray-quote paste \
                 idiom every shell-history `git clone …` line footnotes) \
                 is the canonical paste-from-shell-quoting footgun the \
                 typed slot's accepted set must exclude. The byte rides \
                 verbatim into the lacre's per-dep content-address \
                 (`conteudo: format!(\"git:{repo}\")` peer of the path-\
                 axis embedding at caixa-resolver/src/resolve.rs) and into \
                 the resolver's `git clone <repo>` \
                 (caixa-resolver/src/git.rs) subprocess invocation, where \
                 libcurl's URL parser percent-encodes the byte on the wire \
                 — so two authors whose `:repo` values differ only in \
                 their double-quote presence (one paste-trimmed the quote \
                 wrapper, the other didn't) resolve to the byte-identical \
                 upstream `git clone` but lock to two distinct BLAKE3 \
                 closures, defeating the THEORY.md §V.2 render-determinism \
                 contract on the same axis the fragment-`#`, query-`?`, \
                 backslash-`\\`, template-`{` / `}`, shell-redirection-\
                 `<` / `>`, backtick-`` ` ``, shell-pipe-`|`, \
                 shell-command-separator-`;`, shell-background-`&`, \
                 shell-variable-expansion-`$`, shell-glob-`*`, and \
                 shell-subshell-grouping-`(` / `)` arms close. The peer \
                 `:entrada :paths` axis closes the same byte as part of \
                 `is_gateway_api_http_path`'s RFC-3986-reserved set; the \
                 `:fonte :tag` / `:fonte :branch` axes close the same byte \
                 as part of `is_git_ref_name`'s shell-metachar-injection \
                 cascade. Drop the `\"` wrapper — paste only the URL \
                 between the quotes, or use `:fonte (:tipo path :caminho \
                 \"<local-path>\")` for a local workspace dep)"
                .to_string());
        }
        if b == b'\'' {
            return Err("must not contain `'` (RFC 3986 §2.2 lists the \
                 single-quote byte in the 'sub-delims' set the URL grammar \
                 admits inside a path segment but every WHATWG-conformant \
                 special-scheme URL parser percent-encodes inside a query \
                 component via the 'special-query percent-encode set' — \
                 the peer position the prior `*` / `(` / `)` 'sub-delims' \
                 arms close and the partner ASCII string-delimiter to the \
                 `\"` 'delims' double-quote byte the prior arm closes. The \
                 byte is the second ASCII shell-string-delimiter — `\"` \
                 and `'` are the only two ASCII bytes a byte-level string \
                 parser sharing a value-shape with a shell argument must \
                 refuse on a URL-shaped slot for paste-from-doc safety. No \
                 documented `:fonte :repo` shape admits the byte: the \
                 `github:org/repo` shorthand carries an alphanumeric / `-` \
                 / `_` / `/` alphabet, every `https://` / `ssh://` / \
                 `git://` / `file://` URL scheme keeps host / path bodies \
                 inside the `unreserved` alphanumeric / `-` / `.` / `_` / \
                 `~` set that excludes the byte, and the `git@host:path` \
                 scp-style SSH shape names a POSIX path component that \
                 carries no shell-metachar bytes. Every POSIX shell (sh / \
                 bash / zsh / dash / ksh / fish / nushell) lexes `'` as \
                 the single-quote / strong-quote string delimiter — a \
                 `'<text>'` form suppresses every form of expansion on \
                 `<text>` (no `$`, no `` ` ``, no `\\`, no glob, no \
                 word-splitting), the canonical 'strong-quote the URL so \
                 the shell doesn't re-lex anything inside' idiom every \
                 doc / README quick-start snippet wraps the URL argument \
                 with as the stricter, security-conscious alternative to \
                 the `\"…\"` weak-quote shape the prior arm closes. A \
                 `:repo \"'https://github.com/pleme-io/caixa-teia'\"` (the \
                 canonical paste-from-doc-shell-quoting footgun where the \
                 author copies `$ git clone 'https://…'` from a README's \
                 quick-start snippet and keeps the surrounding strong-\
                 quote bytes — the doc strong-quotes the URL so the shell \
                 doesn't re-lex any metachars inside, but the typed slot \
                 is itself a byte-level string parser, not a shell \
                 context, so the quote bytes ride into the value verbatim; \
                 the strong-quote idiom is more common than `\"…\"` in \
                 security-conscious docs because it forecloses every \
                 expansion the weak-quote form still admits inside) or \
                 `:repo \"github:p/x'tail\"` (the symmetric stray-quote \
                 paste idiom every shell-history `git clone …` line \
                 carries when the author paste-trimmed one boundary but \
                 not the other) is the canonical paste-from-shell-quoting \
                 footgun the typed slot's accepted set must exclude. The \
                 byte additionally carries the canonical English-\
                 typography apostrophe footgun: an author writes `:repo \
                 \"github:p/repo's-fork\"` (the possessive-form paste-\
                 from-prose idiom every README / commit-message / chat-\
                 thread reference to a repo carries) expecting the \
                 substrate to coerce it to a kebab-case slug; the byte \
                 rides verbatim into the lacre's per-dep content-address \
                 (`conteudo: format!(\"git:{repo}\")` peer of the path-\
                 axis embedding at caixa-resolver/src/resolve.rs) and \
                 into the resolver's `git clone <repo>` (caixa-resolver/\
                 src/git.rs) subprocess invocation, where the upstream \
                 host's git porcelain fetches a literal apostrophe-bearing \
                 path that no host's repo registry resolves (GitHub / \
                 GitLab / Bitbucket / Codeberg / sourcehut all reject `'` \
                 in repo slugs at admission time) — so the lacre locks \
                 to a `git:github:p/repo's-fork` closure that never \
                 resolves at clone time, surfacing as a quoting-confused \
                 'remote ref not found' porcelain error far from the \
                 source caixa.lisp, defeating the THEORY.md §V.2 render-\
                 determinism contract on the same axis the fragment-`#`, \
                 query-`?`, backslash-`\\`, template-`{` / `}`, \
                 shell-redirection-`<` / `>`, backtick-`` ` ``, \
                 shell-pipe-`|`, shell-command-separator-`;`, shell-\
                 background-`&`, shell-variable-expansion-`$`, shell-\
                 glob-`*`, shell-subshell-grouping-`(` / `)`, and shell-\
                 double-quote-`\"` arms close. Together with the prior \
                 `\"` arm, this arm closes both ASCII shell-string-\
                 delimiter bytes on the typed `:repo` URL axis — every \
                 byte the canonical `git clone <repo>` doc-paste idiom \
                 wraps the URL argument with is now refused at validate \
                 time, before the byte rides into the lacre or the \
                 resolver subprocess. Drop the `'` wrapper — paste only \
                 the URL between the quotes, or use `:fonte (:tipo path \
                 :caminho \"<local-path>\")` for a local workspace dep)"
                .to_string());
        }
        if b == b'!' {
            return Err("must not contain `!` (RFC 3986 §2.2 lists the bang byte \
                 in the 'sub-delims' set the URL grammar admits inside a \
                 path segment but every WHATWG-conformant special-scheme \
                 URL parser percent-encodes inside a query component via \
                 the 'special-query percent-encode set' — the peer position \
                 the prior `*` / `(` / `)` / `'` 'sub-delims' arms close. \
                 No documented `:fonte :repo` shape admits the byte: the \
                 `github:org/repo` shorthand carries an alphanumeric / `-` \
                 / `_` / `/` alphabet, every `https://` / `ssh://` / \
                 `git://` / `file://` URL scheme keeps host / path bodies \
                 inside the RFC 3986 `unreserved` alphanumeric / `-` / \
                 `.` / `_` / `~` set that excludes the byte, and the \
                 `git@host:path` scp-style SSH shape names a POSIX path \
                 component that carries no shell-metachar bytes. Beyond \
                 the URL-grammar question, every interactive POSIX shell \
                 with history enabled (bash / ksh / zsh's `bashcompat` \
                 mode / csh / tcsh) lexes `!` as the history-expansion \
                 prefix — `!command` re-runs the most recent history \
                 entry beginning with `command`, `!!` re-runs the prior \
                 command verbatim, `!$` substitutes the last word of the \
                 prior command, `!:N` substitutes the Nth word, the \
                 canonical RCE-class injection vector when a string lands \
                 in a shell context with `set -o histexpand` (bash's \
                 default for interactive sessions). A `:repo \
                 \"https://github.com/foo/bar!sudo\"` (the canonical \
                 paste-from-shell-history footgun where the author copies \
                 a `git clone <url>!sudo make install` one-liner from a \
                 README's quick-start snippet, intending the trailing \
                 `!sudo` as a shell-history reference but the typed slot \
                 is itself a byte-level string parser, not a shell \
                 context, so the bytes ride into the value verbatim) or \
                 `:repo \"github:p/repo!!\"` (the symmetric `!!` repeat-\
                 prior-command paste idiom every shell-history `git \
                 clone …` retry line carries) is the canonical paste-\
                 from-shell-history footgun the typed slot's accepted \
                 set must exclude. Beyond shell-history, the bang byte \
                 carries the canonical English-typography emphasis \
                 footgun: an author writes `:repo \
                 \"github:p/awesome-repo!\"` (the exclamation-form paste-\
                 from-prose idiom every README / chat-thread / commit-\
                 message reference to an enthusiastically-named repo \
                 carries) expecting the substrate to coerce it to a \
                 kebab-case slug; the byte rides verbatim into the \
                 lacre's per-dep content-address (`conteudo: \
                 format!(\"git:{repo}\")` peer of the path-axis \
                 embedding at caixa-resolver/src/resolve.rs) and into \
                 the resolver's `git clone <repo>` (caixa-resolver/\
                 src/git.rs) subprocess invocation, where the upstream \
                 host's git porcelain fetches a literal bang-bearing \
                 path that no host's repo registry resolves (GitHub / \
                 GitLab / Bitbucket / Codeberg / sourcehut all reject \
                 `!` in repo slugs at admission time) — so the lacre \
                 locks to a `git:github:p/awesome-repo!` closure that \
                 never resolves at clone time, surfacing as a 'remote \
                 ref not found' porcelain error far from the source \
                 caixa.lisp, defeating the THEORY.md §V.2 render-\
                 determinism contract on the same axis the fragment-\
                 `#`, query-`?`, backslash-`\\`, template-`{` / `}`, \
                 shell-redirection-`<` / `>`, backtick-`` ` ``, shell-\
                 pipe-`|`, shell-command-separator-`;`, shell-\
                 background-`&`, shell-variable-expansion-`$`, shell-\
                 glob-`*`, shell-subshell-grouping-`(` / `)`, shell-\
                 double-quote-`\"`, and shell-single-quote-`'` arms \
                 close. The peer `:fonte :tag` / `:fonte :branch` axes \
                 (`is_git_ref_name`) deliberately admit `!` (git's \
                 `check-ref-format` accepts it as a printable byte and \
                 the bang carries no refname-grammar meaning); the \
                 `:entrada :paths` axis (`is_gateway_api_http_path`) \
                 similarly admits it (K8s Gateway API HTTPPathMatch.value \
                 OpenAPI regex accepts it). `:repo` is substrate-\
                 internal and strictly narrower than its upstream \
                 grammar by design, so the divergence is intentional: \
                 the shell-history-expansion footgun is real on the \
                 typed `:fonte :repo` axis (every `git clone <url>` \
                 invocation crosses a shell boundary at the caixa-\
                 resolver / `Command::new(\"git\")` subprocess layer) \
                 in a way it isn't on the refname / HTTP-path axes that \
                 never reach shell context. Drop the trailing `!` — \
                 author the bare alphanumeric / `-` / `_` slug, or use \
                 `:fonte (:tipo path :caminho \"<local-path>\")` for a \
                 local workspace dep)"
                .to_string());
        }
        if b == b',' {
            return Err("must not contain `,` (RFC 3986 §2.2 lists the comma byte \
                 in the 'sub-delims' set the URL grammar admits inside a \
                 path segment but every WHATWG-conformant special-scheme \
                 URL parser percent-encodes it inside both the path and \
                 query percent-encode sets — the peer position the prior \
                 `!` / `*` / `(` / `)` / `'` 'sub-delims' arms close. No \
                 documented `:fonte :repo` shape admits the byte: the \
                 `github:org/repo` shorthand carries an alphanumeric / `-` \
                 / `_` / `/` alphabet, every `https://` / `ssh://` / \
                 `git://` / `file://` URL scheme keeps host / path bodies \
                 inside the RFC 3986 `unreserved` alphanumeric / `-` / \
                 `.` / `_` / `~` set that excludes the byte, and the \
                 `git@host:path` scp-style SSH shape names a POSIX path \
                 component that carries no list-separator bytes (every \
                 forge — GitHub / GitLab / Bitbucket / Codeberg / \
                 sourcehut — refuses `,` in repo slugs at admission time). \
                 Beyond the URL-grammar question, the comma byte carries \
                 the canonical list-separator-belongs-to-list-grammar \
                 footgun across every parser-of-record `:fonte :repo` \
                 lands in: an author copies a `git clone <urlA>, <urlB>` \
                 paste-from-CSV-list one-liner from a multi-repo \
                 bootstrap doc (the canonical `git clone --recurse-\
                 submodules <a>, <b>, <c>` README-quickstart idiom every \
                 mono-repo carries) or pastes a JSON-array literal `[\"a\", \
                 \"b\", \"c\"]` from a tooling-config snippet stripped \
                 of its brackets, intending the comma to separate \
                 multiple repo entries but the typed `:repo` slot names \
                 *one* repo (the list-separator belongs to the list \
                 grammar of the enclosing `:deps` slot, not to the \
                 individual `:repo` value). A `:repo \
                 \"github:p/a,github:p/b\"` silently passed every prior \
                 arm and rode into the lacre's per-dep content-address \
                 (`conteudo: format!(\"git:{repo}\")` peer of the path-\
                 axis embedding at caixa-resolver/src/resolve.rs) and \
                 into the resolver's `git clone <repo>` (caixa-\
                 resolver/src/git.rs) subprocess invocation, where the \
                 upstream host's git porcelain fetched a literal comma-\
                 bearing path that no host's repo registry resolves — \
                 so the lacre locks to a `git:github:p/a,github:p/b` \
                 closure that never resolves at clone time, surfacing as \
                 a 'remote ref not found' porcelain error far from the \
                 source caixa.lisp, defeating the THEORY.md §V.2 render-\
                 determinism contract on the same axis the fragment-\
                 `#`, query-`?`, backslash-`\\`, template-`{` / `}`, \
                 shell-redirection-`<` / `>`, backtick-`` ` ``, shell-\
                 pipe-`|`, shell-command-separator-`;`, shell-\
                 background-`&`, shell-variable-expansion-`$`, shell-\
                 glob-`*`, shell-subshell-grouping-`(` / `)`, shell-\
                 double-quote-`\"`, shell-single-quote-`'`, and shell-\
                 history-`!` arms close. Beyond the multi-repo paste, \
                 the byte carries the canonical English-typography \
                 trailing-`,` paste-from-prose footgun: an author writes \
                 `:repo \"github:pleme-io/caixa-feira,\"` (the trailing \
                 comma every README-prose list-of-projects sentence \
                 carries, mistakenly retained when the slug is pasted \
                 mid-sentence) expecting the substrate to coerce it to \
                 a kebab-case slug; the byte rides verbatim. The peer \
                 `:fonte :tag` / `:fonte :branch` axes \
                 (`is_git_ref_name`) deliberately admit `,` (git's \
                 `check-ref-format` accepts it as a printable byte and \
                 the comma carries no refname-grammar meaning); the \
                 `:entrada :paths` axis (`is_gateway_api_http_path`) \
                 similarly admits it (K8s Gateway API HTTPPathMatch.value \
                 OpenAPI regex accepts it). `:repo` is substrate-\
                 internal and strictly narrower than its upstream \
                 grammar by design, so the divergence is intentional: \
                 the list-separator-belongs-to-list-grammar footgun is \
                 real on the typed `:fonte :repo` axis (every `:deps` \
                 entry names exactly one repo and the comma between \
                 entries belongs to the `:deps` list grammar, never to \
                 the value) in a way it isn't on the refname / HTTP-\
                 path axes whose grammars admit the byte without \
                 confusion. Drop the trailing `,` — author the bare \
                 alphanumeric / `-` / `_` slug, or split into multiple \
                 `:deps` entries to express multiple repos)"
                .to_string());
        }
    }
    if s.starts_with(':') {
        return Err(
            "must not start with `:` (the canonical empty-scheme footgun — \
             `:foo` parses as a zero-length scheme that no git porcelain \
             entry-point accepts; use a non-empty scheme prefix like \
             `github:`, `https://`, `ssh://`, `git://`, `file://`, or the \
             `git@host:path` scp-style SSH form)"
                .to_string(),
        );
    }
    if !s.contains(':') {
        return Err(
            "must contain a `:` separator (every documented `:fonte :repo` \
             shape carries one: `github:org/repo` shorthand, `https://…` / \
             `ssh://…` / `git://…` / `file://…` URL schemes, or \
             `git@host:path` scp-style SSH; a bare `org/repo` form is \
             ambiguous — `git clone` reads it as a relative filesystem path \
             rather than the GitHub-shorthand expansion the author probably \
             intended — so prefix it with `github:` for the registry-\
             shorthand resolver convention)"
                .to_string(),
        );
    }
    Ok(())
}

/// Practical cap on a `:caracteristicas` (Cargo-feature-name-shaped)
/// entry, in bytes. Cargo itself enforces no length cap on feature
/// names — its `restricted_names::validate_feature_name` accepts any
/// length — but every realistic feature in the Cargo ecosystem is
/// well under this bound (`derive` 6, `serde_json` 10, the
/// `__private_…` doubled-underscore convention rarely exceeds 32).
/// 64 bytes is the substrate's catch-the-paste-from-binary cap on the
/// peer trajectory `is_dns_1123_label` (63), `is_wit_world_ref` (128),
/// `is_nats_subject` (256), `is_wasi_keyvalue_slot` (512),
/// `is_git_ref_name` (255), `is_git_oid` (40/64),
/// `is_git_repo_url` (2048) carry: an axis-appropriate ceiling above
/// every legitimate authoring shape, tight enough to surface the
/// "paste-from-binary" / "multi-line blob landed in a single-token
/// slot" footgun at validate time.
pub const CARGO_FEATURE_NAME_MAX_LEN: usize = 64;

/// Predicate: assert that `s` is a valid Cargo feature name. The
/// contract — modeled on Cargo's
/// `restricted_names::validate_feature_name` grammar (the parser the
/// Cargo resolver routes every `[dependencies.<dep>.features]` entry
/// through at `cargo metadata` time), narrowed to the strict ASCII
/// subset every realistic feature in the Cargo ecosystem uses:
///
///   - 1..=[`CARGO_FEATURE_NAME_MAX_LEN`] (64) bytes;
///   - first byte: ASCII alphanumeric or `_` (Cargo's parser admits
///     Unicode XID-start characters too; pleme-io narrows to the
///     ASCII subset for the same reason every peer value-shape
///     predicate above narrows — drift between NFC-vs-NFD
///     normalization across filesystems silently rewrites the
///     feature-key, breaking the lacre's content-addressing
///     invariant). Leading `-` / `+` / `.` are explicitly named —
///     each is the canonical "I copy-pasted the
///     `+optional-feature` enablement form from a Cargo doc" /
///     "I confused the dotted-form with feature-name shape"
///     footgun the predicate's diagnostic remediation points at;
///   - remaining bytes: ASCII alphanumeric, `_`, `-`, `+`, or `.`
///     (the Cargo-accepted continuation set). Whitespace, control
///     characters, non-ASCII bytes, `/` / `?` / `#` / `,` /
///     other punctuation are each surfaced with a self-locating
///     reason naming the canonical authoring footgun (multi-token
///     blob, CR/LF paste-from-doc, `/` segment-separator confusion
///     with namespaced-dep features the predicate's call site
///     explicitly does not enable, list-separator-belongs-to-list-
///     grammar miscomprehension).
///
/// Returns the parser-shaped reason on rejection (without wrapping in
/// any error variant) so each per-axis caller — [`crate::Dep::validate`]
/// for the `:deps`/`:deps-dev :caracteristicas` axis at validate time,
/// every future per-feature axis (M4 caixa-resolver's `lacre.lisp`
/// resolved-feature-set materializer, the future per-WitContract
/// `:caracteristicas`-shaped capability-set axis if WIT worlds grow a
/// typed feature toggle, the future per-`UpgradeInstruction` per-
/// capability set axis the §V.2 mes-build extension would carry) —
/// wraps the same reason in its own typed `*Invalid { <axis>, reason }`
/// variant. The reason wording is axis-agnostic ("Cargo feature names
/// reject leading `-`") so every call site reading the same diagnostic
/// points at the same rule; drift between any two axes' rule
/// enforcement is a build error visible at this predicate, not a
/// per-renderer "this passed validate but Cargo rejected at metadata
/// time" surprise.
///
/// Empty input is rejected here (defensively) and at each call site
/// via the narrower [`crate::DepError::CaracteristicaEmpty`] variant —
/// the same empty-first cascade [`is_dns_1123_label`],
/// [`is_gateway_api_http_path`], [`is_wit_world_ref`],
/// [`is_nats_subject`], [`is_wasi_keyvalue_slot`], [`is_git_ref_name`],
/// [`is_git_oid`], and [`is_git_repo_url`] all carry.
///
/// Lifted as a typed substrate-side primitive on the same trajectory
/// the peer value-shape predicates already follow — the typed slot's
/// valid set matches the downstream consumer's accepted set (here,
/// Cargo's TOML-feature-name parser at `cargo metadata` time),
/// structurally. The ninth value-shape primitive to land in
/// [`crate::render`], closing the typed `:deps`/`:deps-dev` surface
/// value-shape trajectory on its last unsealed axis (`:caracteristicas`
/// entries; the per-entry `:nome` / `:versao` / `:fonte` axes are
/// already routed through their respective shape predicates).
///
/// # Errors
///
/// Returns the parser-shaped reason naming the specific violation
/// (length / first-byte-class / continuation-byte-class / whitespace /
/// control-char / non-ASCII / `/`-segment-separator-confusion /
/// `,`-list-separator-confusion), without wrapping in any error
/// variant — every caller maps the same `String` into its own typed
/// `*Invalid { <axis>, reason }` enum variant.
pub fn is_cargo_feature_name(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("must not be empty".to_string());
    }
    if s.len() > CARGO_FEATURE_NAME_MAX_LEN {
        return Err(format!(
            "exceeds Cargo feature name max length of {CARGO_FEATURE_NAME_MAX_LEN} bytes \
             (got {} bytes; legitimate Cargo feature names rarely exceed ~24 bytes — \
             this length suggests a paste-from-binary or multi-token blob landed in \
             the `:caracteristicas` slot)",
            s.len()
        ));
    }
    let bytes = s.as_bytes();
    let first = bytes[0];
    if !(first.is_ascii_alphanumeric() || first == b'_') {
        let msg = if first == b'+' {
            "must not start with `+` (Cargo's feature-name grammar reserves a leading \
             `+` for the activation-syntax inside a `[dependencies.<dep>.features]` \
             list — `:caracteristicas` entries name the feature itself, not its \
             enablement form; drop the leading `+` and author the bare feature name, \
             e.g. `\"http\"` not `\"+http\"`)"
                .to_string()
        } else if first == b'-' {
            "must not start with `-` (Cargo's feature-name grammar rejects a leading \
             hyphen — `-` is a legitimate continuation character between alphanumeric \
             segments but the canonical CLI-argument-injection / kebab-leak footgun at \
             the start; drop the leading `-`, e.g. `\"json\"` not `\"-json\"`)"
                .to_string()
        } else if first == b'.' {
            "must not start with `.` (Cargo's feature-name grammar rejects a leading \
             dot; `.` is a legitimate continuation character but the canonical \
             leading-dot-as-version-suffix / hidden-file footgun at the start. Drop \
             the leading `.`)"
                .to_string()
        } else if first == b' ' || first == b'\t' {
            "must not start with whitespace (Cargo's feature-name grammar rejects \
             whitespace anywhere; the leading-whitespace arm is the canonical \
             paste-from-aligned-doc footgun)"
                .to_string()
        } else if first < 0x20 || first == 0x7F {
            format!(
                "must not start with control character 0x{first:02x} (Cargo's feature-name \
                 grammar rejects ASCII control characters; the CR/LF arm is the canonical \
                 paste-from-multiline-doc footgun)"
            )
        } else if first >= 0x80 {
            format!(
                "must not start with non-ASCII byte 0x{first:02x} (Cargo accepts Unicode \
                 XID-start characters but pleme-io narrows to the strict ASCII subset every \
                 realistic feature name uses; legitimate features are kebab-case ASCII \
                 identifiers like `\"http\"`, `\"json\"`, `\"derive\"`)"
            )
        } else {
            format!(
                "must start with an ASCII alphanumeric character or `_`, got {ch:?} \
                 (Cargo's `restricted_names::validate_feature_name` rejects feature names \
                 whose first character is outside the XID-start + `_` + digit set; \
                 pleme-io narrows to the strict ASCII alphanumeric + `_` subset)",
                ch = first as char
            )
        };
        return Err(msg);
    }
    for &b in &bytes[1..] {
        let valid = b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'+' || b == b'.';
        if !valid {
            let msg = if b == b' ' || b == b'\t' {
                format!(
                    "must not contain whitespace character {ch:?} (Cargo's feature-name \
                     grammar rejects whitespace; feature names are single-token identifiers \
                     — use `-` or `_` to separate kebab-case / snake-case segments instead)",
                    ch = b as char
                )
            } else if b == b',' {
                "must not contain `,` (the comma separator belongs to the \
                 `:caracteristicas` list grammar between entries, not to the feature-name \
                 grammar within an entry — split the value into two separate list entries)"
                    .to_string()
            } else if b == b'/' {
                "must not contain `/` (Cargo's `dep/feat` syntax for namespaced-dep \
                 features applies inside `[dependencies.<dep>.features]` list entries that \
                 already name the parent dep — `:caracteristicas` entries are per-dep \
                 already, so the segment separator within a feature name must be `-`, \
                 `_`, `+`, or `.`)"
                    .to_string()
            } else if b == b'?' {
                "must not contain `?` (Cargo's feature-name grammar rejects URL-reserved \
                 punctuation; use `-`, `_`, `+`, or `.` as a segment separator instead)"
                    .to_string()
            } else if b == b'#' {
                "must not contain `#` (Cargo's feature-name grammar rejects URL-reserved \
                 punctuation; use `-`, `_`, `+`, or `.` as a segment separator instead)"
                    .to_string()
            } else if b < 0x20 || b == 0x7F {
                format!(
                    "must not contain control character 0x{b:02x} (Cargo's feature-name \
                     grammar rejects ASCII control characters; the CR/LF arm is the \
                     canonical paste-from-multiline-doc footgun)"
                )
            } else if b >= 0x80 {
                format!(
                    "must not contain non-ASCII byte 0x{b:02x} (Cargo accepts Unicode \
                     XID-continue characters but pleme-io narrows to the strict ASCII \
                     subset every realistic feature name uses; raw non-ASCII silently \
                     round-trips inconsistently across NFC/NFD normalization on APFS / \
                     case-folding filesystems, breaking the lacre's content-addressing \
                     invariant)"
                )
            } else {
                format!(
                    "contains invalid character {ch:?} (Cargo's feature-name grammar \
                     allows only `[A-Za-z0-9_+\\-.]` after the first character)",
                    ch = b as char
                )
            };
            return Err(msg);
        }
    }
    Ok(())
}

/// Practical cap on a `:licenca` (SPDX-expression-shaped) value, in
/// bytes. The SPDX specification places no length cap on expressions
/// — the grammar admits arbitrarily-nested composite expressions —
/// but every realistic pleme-io fixture stays well under this bound
/// (`MIT` 3, `Apache-2.0` 10, `Apache-2.0 OR MIT` 17, the longest
/// SPDX dual-license-with-exception shape `Apache-2.0 WITH
/// LLVM-exception` 31; a `(MIT OR Apache-2.0) AND BSD-3-Clause AND
/// ISC` composite caps near 50). 256 bytes is the substrate's
/// catch-the-paste-from-binary cap on the peer trajectory
/// `is_dns_1123_label` (63), `is_cargo_feature_name` (64),
/// `is_wit_world_ref` (128), `is_nats_subject` (256),
/// `is_wasi_keyvalue_slot` (512), `is_git_ref_name` (255),
/// `is_git_oid` (40/64), `is_git_repo_url` (2048) carry: an
/// axis-appropriate ceiling above every legitimate authoring shape,
/// tight enough to surface the "paste-from-license-text" /
/// "multi-line license blob landed in the `:licenca` slot" footgun
/// at validate time.
pub const SPDX_EXPRESSION_MAX_LEN: usize = 256;

/// Predicate: assert that `s` is a valid SPDX-expression shape. The
/// contract — modeled on the SPDX 2.1 expression grammar
/// (`compound-expression = simple-expression | "(" compound-expression
/// ")" | compound-expression "WITH" exception-id | compound-expression
/// "AND" compound-expression | compound-expression "OR"
/// compound-expression`; `simple-expression = license-id | license-id
/// "+" | "LicenseRef-" idstring | "DocumentRef-" idstring ":"
/// "LicenseRef-" idstring`; `idstring = 1*(ALPHA / DIGIT / "-" /
/// ".")`), narrowed to the structural alphabet floor every realistic
/// SPDX expression in the wild uses:
///
///   - 1..=[`SPDX_EXPRESSION_MAX_LEN`] (256) bytes;
///   - no leading whitespace (paste-from-aligned-doc footgun);
///   - no trailing whitespace (paste-from-doc footgun — every
///     downstream SPDX parser splits on exact token boundaries and
///     a trailing space breaks the `WITH` / `AND` / `OR` keyword
///     match);
///   - every byte in the SPDX expression alphabet: ASCII alphanumeric
///     plus `.`, `-`, `+`, `(`, `)`, `:` (the `DocumentRef-…:LicenseRef-…`
///     separator), and a single ASCII space (token separator). Tabs,
///     control characters, non-ASCII bytes, `_` (not in `idstring`),
///     `,` (SPDX uses `AND` / `OR` keywords, not comma), `/` (the
///     `dual-license/A` colloquial idiom is non-SPDX), and every other
///     punctuation byte are each surfaced with a self-locating reason
///     naming the canonical authoring footgun.
///
/// The predicate is a *structural* floor — it enforces the alphabet +
/// length the SPDX grammar's character class admits, not the full
/// expression-parse (compound-expression nesting, `AND`/`OR`/`WITH`
/// keyword placement, parenthesis balance, idstring well-formedness
/// per simple-expression production). A future tightening on the
/// `:licenca` axis can extend past this shape predicate into a full
/// SPDX parser + license-id allowlist (peer with how
/// [`is_git_repo_url`] is the structural floor on `:repositorio` and
/// a future flake-resolver might tighten the per-URL-scheme arm into
/// scheme-specific shape predicates). This gate closes the
/// `_`/`,`/`/`/tab/CR/LF/non-ASCII/multi-line-blob footguns
/// structurally at the manifest layer; the parser-shape arms remain
/// for a follow-up routine once a real SPDX-parser dep is justified.
///
/// Returns the parser-shaped reason on rejection (without wrapping in
/// any error variant) so each per-axis caller —
/// [`crate::Caixa::validate_licenca`] for the universal `:licenca`
/// axis at validate time, every future per-license axis (a future
/// `:fonte :license` per-dep license-pin axis, a future
/// per-`UpgradeInstruction` per-component license-compatibility axis,
/// a future `Lacre` per-resolved-dep license-closure axis) — wraps the
/// same reason in its own typed `*Invalid { <axis>, reason }` variant.
/// The reason wording is axis-agnostic ("SPDX expressions reject
/// leading whitespace") so every call site reading the same diagnostic
/// points at the same rule; drift between any two axes' rule
/// enforcement is a build error visible at this predicate, not a
/// per-renderer "this passed validate but `helm lint` rejected the
/// `Chart.yaml license:` value" surprise.
///
/// Empty input is rejected here (defensively) and at each call site
/// via the narrower [`crate::ManifestError::LicencaEmpty`] variant —
/// the same empty-first cascade [`is_dns_1123_label`],
/// [`is_gateway_api_http_path`], [`is_wit_world_ref`],
/// [`is_nats_subject`], [`is_wasi_keyvalue_slot`], [`is_git_ref_name`],
/// [`is_git_oid`], [`is_git_repo_url`], and [`is_cargo_feature_name`]
/// all carry.
///
/// Lifted as a typed substrate-side primitive on the same trajectory
/// the peer value-shape predicates already follow — the typed slot's
/// valid set matches the downstream consumer's accepted set (here,
/// the `caixa-helm` chart `README.md` `## License` section + a
/// future SPDX-aware Chart.yaml `license:` emitter + the future
/// per-resolved-dep license-closure axis a forthcoming `Lacre`
/// extension would carry), structurally.
///
/// # Errors
///
/// Returns the parser-shaped reason naming the specific violation
/// (length / leading-whitespace / trailing-whitespace /
/// alphabet-class / tab / control-char / non-ASCII / `_` /
/// `,`-list-separator-confusion / `/`-dual-license-idiom), without
/// wrapping in any error variant — every caller maps the same
/// `String` into its own typed `*Invalid { <axis>, reason }` enum
/// variant.
pub fn is_spdx_expression_shape(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("must not be empty".to_string());
    }
    if s.len() > SPDX_EXPRESSION_MAX_LEN {
        return Err(format!(
            "exceeds SPDX expression max length of {SPDX_EXPRESSION_MAX_LEN} bytes \
             (got {} bytes; realistic SPDX expressions like `\"Apache-2.0 WITH \
             LLVM-exception\"` rarely exceed ~64 bytes — this length suggests a \
             paste-from-license-text or multi-line blob landed in the `:licenca` \
             slot)",
            s.len()
        ));
    }
    let bytes = s.as_bytes();
    if bytes[0] == b' ' {
        return Err(
            "must not start with whitespace (SPDX expressions are single tokens \
             or token sequences separated by *internal* single ASCII spaces; a \
             leading space is the canonical paste-from-aligned-doc footgun and \
             breaks every downstream SPDX parser that splits on exact token \
             boundaries)"
                .to_string(),
        );
    }
    if *bytes.last().expect("non-empty checked above") == b' ' {
        return Err(
            "must not end with whitespace (SPDX expressions don't terminate with \
             trailing whitespace; the trailing-space arm is the canonical \
             paste-from-doc footgun that breaks downstream parsers which split \
             on exact `AND` / `OR` / `WITH` keyword boundaries)"
                .to_string(),
        );
    }
    for &b in bytes {
        let valid = b.is_ascii_alphanumeric()
            || b == b'.'
            || b == b'-'
            || b == b'+'
            || b == b'('
            || b == b')'
            || b == b':'
            || b == b' ';
        if !valid {
            let msg = if b == b'\t' {
                "must not contain tab character (SPDX expressions use a single \
                 ASCII space between tokens — tabs are the canonical \
                 paste-from-aligned-doc footgun and break downstream parsers \
                 that split on exact `\" \"` boundaries)"
                    .to_string()
            } else if b < 0x20 || b == 0x7F {
                format!(
                    "must not contain control character 0x{b:02x} (SPDX \
                     expressions are printable ASCII; the CR/LF arm is the \
                     canonical paste-from-multiline-doc footgun and lands as a \
                     malformed line in the rendered chart `README.md` `## \
                     License` section)"
                )
            } else if b >= 0x80 {
                format!(
                    "must not contain non-ASCII byte 0x{b:02x} (SPDX identifiers \
                     are ASCII per the `idstring = 1*(ALPHA / DIGIT / \"-\" / \
                     \".\")` production; raw non-ASCII silently round-trips \
                     inconsistently across NFC/NFD normalization on APFS / \
                     case-folding filesystems and breaks at every downstream \
                     SPDX-aware tool)"
                )
            } else if b == b'_' {
                "must not contain `_` (SPDX `idstring` grammar — license-id, \
                 LicenseRef, exception-id — is `1*(ALPHA / DIGIT / \"-\" / \
                 \".\")`; `_` is not in the SPDX alphabet, use `-` as the \
                 segment separator instead, e.g. `\"Apache-2.0\"` not \
                 `\"Apache_2.0\"`)"
                    .to_string()
            } else if b == b',' {
                "must not contain `,` (SPDX expressions compose multiple \
                 licenses via the `AND` / `OR` keywords, not the comma \
                 separator; e.g. `\"MIT OR Apache-2.0\"` not `\"MIT, \
                 Apache-2.0\"`)"
                    .to_string()
            } else if b == b'/' {
                "must not contain `/` (the `dual-license/A` slash form is a \
                 non-SPDX colloquial idiom; SPDX uses the `OR` keyword to \
                 compose: `\"MIT OR Apache-2.0\"` not `\"MIT/Apache-2.0\"`)"
                    .to_string()
            } else if b == b';' {
                "must not contain `;` (SPDX expressions compose multiple \
                 licenses via the `AND` / `OR` keywords, not the semicolon \
                 separator; e.g. `\"MIT AND Apache-2.0\"` not `\"MIT; \
                 Apache-2.0\"`)"
                    .to_string()
            } else {
                format!(
                    "contains invalid character {ch:?} (the SPDX expression \
                     alphabet is `[A-Za-z0-9.+\\-():]` plus single ASCII space; \
                     license IDs / exception IDs are `idstring` `1*(ALPHA / \
                     DIGIT / \"-\" / \".\")`, composition uses `AND` / `OR` / \
                     `WITH` keywords + `(`/`)` grouping)",
                    ch = b as char
                )
            };
            return Err(msg);
        }
    }
    Ok(())
}

/// Maximum byte length of a chart-description-shaped string. The
/// 512-byte cap is the axis-appropriate ceiling for the free-form
/// prose summary the `:descricao` axis carries: every realistic
/// chart description in the wild (`"Canonical Rust→wasm32-wasip2
/// caixa Servico."`, `"Checkout flow."`, `"AWS provider caixa for
/// tatara-lisp"`) sits well under 256 bytes, and the 512-byte cap
/// surfaces the "paste-from-doc multi-paragraph blob landed in the
/// `:descricao` slot" footgun at validate time. Peer with
/// [`WASI_KV_SLOT_MAX_LEN`] (512) on the sibling longer-than-
/// identifier axis; tighter than [`GIT_REPO_URL_MAX_LEN`] (2048)
/// which carries a different axis-class ceiling, and looser than
/// [`SPDX_EXPRESSION_MAX_LEN`] (256) which is the canonical
/// short-identifier-class axis.
pub const CHART_DESCRIPTION_MAX_LEN: usize = 512;

/// Scan `s` for the Unicode bidirectional-override / isolate format
/// codepoints UAX #9 names as the structural prerequisite of the
/// "Trojan Source" attack class (CVE-2021-42574 / Boucher & Anderson
/// 2021): nine codepoints in two contiguous blocks that flip the
/// rendered visual order of every following character until a
/// matching pop, so a string visible to a human reader and the same
/// string consumed by a parser/renderer can disagree on the order of
/// its content bytes.
///
/// The accepted set (rejection list):
///
///   - U+202A `LRE` LEFT-TO-RIGHT EMBEDDING
///   - U+202B `RLE` RIGHT-TO-LEFT EMBEDDING
///   - U+202C `PDF` POP DIRECTIONAL FORMATTING
///   - U+202D `LRO` LEFT-TO-RIGHT OVERRIDE
///   - U+202E `RLO` RIGHT-TO-LEFT OVERRIDE
///   - U+2066 `LRI` LEFT-TO-RIGHT ISOLATE
///   - U+2067 `RLI` RIGHT-TO-LEFT ISOLATE
///   - U+2068 `FSI` FIRST STRONG ISOLATE
///   - U+2069 `PDI` POP DIRECTIONAL ISOLATE
///
/// Returns the first offending codepoint in document order, or
/// `None` when `s` carries none of them. Iterates `chars()` once
/// (single UTF-8 decode pass, peer of every other UTF-8-aware
/// predicate in this module) — the per-predicate caller folds the
/// `Some(c)` into its axis-specific reason wording with the
/// offending codepoint named verbatim as `U+XXXX`.
///
/// Lifted as a shared helper rather than inlined into each per-axis
/// predicate (the PRIME DIRECTIVE duplication-budget rule —
/// THEORY.md §I.3.5: "every recurring shape becomes a generator
/// before it becomes a pattern; every pattern becomes a library
/// before it becomes duplicated code. The duplication budget is
/// zero.") because two predicates ([`is_chart_description_shape`],
/// [`is_chart_maintainer_name_shape`]) carry the same UTF-8
/// free-form-prose accepted set and would otherwise inline the same
/// nine-codepoint match arm verbatim. The third caller — every
/// future per-axis free-form-prose surface (a future Aplicacao-
/// level `:descricao` summary axis, a future per-`:contratos` edge
/// `:descricao` annotation, the future per-`:autores`-email-suffix
/// shape gate) — lands as a thin `if let Some(c) =
/// find_unicode_bidi_override(s) { … }` wrapper rather than
/// re-inlining the same codepoint match.
///
/// The arm is structurally distinct from the per-byte control-char
/// arm `[is_chart_description_shape]` already carries: ASCII control
/// bytes (`0x00..=0x1F` plus `0x7F`) are caught at the per-byte
/// pass; the bidi codepoints all decode to non-ASCII three-byte
/// UTF-8 sequences (`E2 80 AA..=E2 80 AE` for U+202A..=U+202E,
/// `E2 81 A6..=E2 81 A9` for U+2066..=U+2069) — every byte ≥ 0x80
/// per UTF-8 grammar — that the per-byte non-ASCII pass deliberately
/// accepts (Unicode letters, em-dash, arrows are canonical
/// `:descricao` shapes). Only the typed codepoint scan catches them.
fn find_unicode_bidi_override(s: &str) -> Option<char> {
    s.chars().find(|c| {
        matches!(
            *c,
            '\u{202A}'
                | '\u{202B}'
                | '\u{202C}'
                | '\u{202D}'
                | '\u{202E}'
                | '\u{2066}'
                | '\u{2067}'
                | '\u{2068}'
                | '\u{2069}'
        )
    })
}

/// Scan `s` for any of the three non-ASCII Unicode line-break
/// codepoints UAX #14 (Unicode Line Breaking Algorithm) and the
/// YAML 1.1 §4.1 b-char production both treat as line terminators
/// outside the two single-byte ASCII shapes (`\n` LF / `\r` CR) the
/// per-byte arm on the calling predicate already closes:
///
///   - U+0085 `NEL` NEXT LINE
///   - U+2028 `LS`  LINE SEPARATOR
///   - U+2029 `PS`  PARAGRAPH SEPARATOR
///
/// YAML 1.2 §5.4 ("Line Break Characters") explicitly retired these
/// three from the YAML line-break set per the UTR #20 recommendation,
/// so a YAML 1.2-strict parser (the `serde_yaml` / `yaml-rust2` family)
/// preserves them as literal codepoints inside the rendered Chart.yaml
/// scalar — but YAML 1.1 parsers (go-yaml v2 which Helm v3 / kubectl /
/// every Kubernetes client library transitively links, and `ruamel.yaml`
/// in compat mode) still treat them as line terminators per the YAML 1.1
/// b-char production, so the same `:descricao` / `:autores` value
/// authored with an embedded U+2028 parses as a single-line plain-style
/// scalar through one downstream consumer and a multi-line block scalar
/// through another. The cross-parser line-break disagreement breaks the
/// THEORY.md §V.2 render-determinism contract every typed slot carries
/// on the same axis the per-byte `\n` / `\r` arms close for ASCII; the
/// substrate refuses the three codepoints at validate time so the
/// rendered Chart.yaml carries the single-line shape every conformant
/// YAML parser agrees on. Independently, every UAX #14 conformant text
/// consumer (editors, terminals, web UIs like `helm list` /
/// `helm search` / Artifact Hub) breaks the visual line at these
/// codepoints regardless of YAML version, so the author's editor view
/// of `caixa.lisp` disagrees with the chart-consumer's rendered view
/// even when both YAML parsers agree on the byte-level shape.
///
/// Returns the first offending codepoint in document order, or `None`
/// when `s` carries none of them. Iterates `chars()` once (single
/// UTF-8 decode pass, peer of [`find_unicode_bidi_override`] and every
/// other UTF-8-aware predicate in this module) — the per-predicate
/// caller folds the `Some(c)` into its axis-specific reason wording
/// with the offending codepoint named verbatim as `U+XXXX`.
///
/// Lifted as a shared helper rather than inlined into each per-axis
/// predicate (the PRIME DIRECTIVE duplication-budget rule —
/// THEORY.md §I.3.5: "every recurring shape becomes a generator
/// before it becomes a pattern; every pattern becomes a library
/// before it becomes duplicated code. The duplication budget is
/// zero.") because two predicates ([`is_chart_description_shape`],
/// [`is_chart_maintainer_name_shape`]) carry the same UTF-8
/// free-form-prose accepted set and would otherwise inline the same
/// three-codepoint match arm verbatim — sibling lift to the
/// [`find_unicode_bidi_override`] helper one trajectory earlier on
/// the same two predicates. The third caller — every future
/// per-axis free-form-prose surface (a future Aplicacao-level
/// `:descricao` summary axis, a future per-`:contratos` edge
/// `:descricao` annotation, the future per-`:autores`-email-suffix
/// shape gate) — lands as a thin `if let Some(c) =
/// find_unicode_line_break(s) { … }` wrapper rather than re-inlining
/// the same codepoint match.
///
/// The arm is structurally distinct from the per-byte control-char
/// arm `[is_chart_description_shape]` already carries: the ASCII
/// line-break bytes `\n` (`0x0A`) and `\r` (`0x0D`) are caught at the
/// per-byte pass; the three non-ASCII line-break codepoints all
/// decode to multi-byte UTF-8 sequences (`C2 85` for U+0085, `E2 80
/// A8` for U+2028, `E2 80 A9` for U+2029) — every byte ≥ 0x80 per
/// UTF-8 grammar — that the per-byte non-ASCII pass deliberately
/// accepts (Unicode letters, em-dash, arrows are canonical
/// `:descricao` shapes). Only the typed codepoint scan catches them.
fn find_unicode_line_break(s: &str) -> Option<char> {
    s.chars()
        .find(|c| matches!(*c, '\u{0085}' | '\u{2028}' | '\u{2029}'))
}

/// Scan `s` for any of the eight BMP Unicode invisible-format
/// codepoints — the Cf-category zero-width codepoints that have no
/// visible glyph in any conforming font yet ride verbatim through
/// string equality and parser lookup:
///
///   - U+00AD `SHY`    SOFT HYPHEN
///   - U+200B `ZWSP`   ZERO WIDTH SPACE
///   - U+2060 `WJ`     WORD JOINER
///   - U+2061 `FA`     FUNCTION APPLICATION
///   - U+2062 `IT`     INVISIBLE TIMES
///   - U+2063 `IS`     INVISIBLE SEPARATOR
///   - U+2064 `IP`     INVISIBLE PLUS
///   - U+FEFF `ZWNBSP` ZERO WIDTH NO-BREAK SPACE (BOM)
///
/// These codepoints break the THEORY.md §V.2 render-determinism
/// contract on a third axis from the visual-order class the sibling
/// [`find_unicode_bidi_override`] helper closes (the nine UAX #9
/// explicit-direction codepoints flip the rendered visual order) and
/// the single-line/multi-line class the sibling
/// [`find_unicode_line_break`] helper closes (the three UAX #14
/// non-ASCII line-break codepoints split a YAML 1.1 scalar): the
/// *invisible-identity* divergence. The author's editor view of
/// `caixa.lisp`, the chart-consumer's `helm list` / `helm search` /
/// Artifact Hub maintainer column, and every conformant terminal /
/// browser / editor agree on the visible glyph sequence (the
/// codepoint renders as nothing, so `"alice"` and
/// `"alice\u{200B}"` look identical end-to-end) — but the byte
/// sequence the YAML-plain-style-scalar carries verbatim differs
/// from the byte sequence the same author intends to read back, so
/// every byte-level grep / diff / equality comparison over the
/// rendered Chart.yaml disagrees with the visible-glyph match, the
/// Artifact Hub maintainer / description search index lookup misses
/// the authored identity entry because the byte sequence carries
/// invisible codepoints between letters, and a future per-author
/// CLA-signer lookup matches a visually-identical-but-byte-distinct
/// identity (the canonical "invisible-codepoint homograph" footgun).
/// The canonical authoring shapes that introduce these codepoints:
/// paste-from-Microsoft-Word (SHY auto-inserted at every hyphenation
/// candidate), paste-from-text-editor-saved-as-UTF-8-with-BOM (BOM
/// leading byte from Notepad / older VS Code defaults / Excel CSV
/// export), paste-from-typesetting-doc (ZWSP / WJ invisible word-
/// break hints from InDesign / LaTeX-rendered PDF copy-paste).
///
/// Returns the first offending codepoint in document order, or
/// `None` when `s` carries none of them. Iterates `chars()` once
/// (single UTF-8 decode pass, peer of [`find_unicode_bidi_override`]
/// and [`find_unicode_line_break`]) — the per-predicate caller folds
/// the `Some(c)` into its axis-specific reason wording with the
/// offending codepoint named verbatim as `U+XXXX`.
///
/// Excluded from the rejected set, on purpose:
///
///   - U+200C `ZWNJ` ZERO WIDTH NON-JOINER and U+200D `ZWJ` ZERO
///     WIDTH JOINER — both carry semantic compositional load in
///     Devanagari / Bengali / Persian script clusters (the
///     canonical "Persian name authoring" shape relies on ZWNJ to
///     break inappropriate ligatures) and in modern emoji ZWJ
///     sequences (👨‍💻 is `MAN` + U+200D `ZWJ` + `LAPTOP`); the
///     `:autores` / `:descricao` axes admit Unicode prose where
///     such sequences are the canonical authoring shape and a ban
///     would regress legitimate maintainer-name fixtures.
///   - U+200E `LRM` LEFT-TO-RIGHT MARK and U+200F `RLM`
///     RIGHT-TO-LEFT MARK — both are legitimate single-character
///     direction *hints* (not overrides) in mixed-script prose
///     (the canonical "Arabic name with embedded ASCII email"
///     shape relies on RLM to render the visual order reliably
///     across YAML / HTML consumers); the visible-order risk on
///     these axes is closed by the bidi-*override* helper (the 9
///     codepoints UAX #9 names as the Trojan Source vector), not
///     by the bidi-*marks*, so LRM/RLM remain accepted natively.
///   - Codepoints outside the BMP — Variation Selectors
///     Supplement (U+E0100..U+E01EF), Tag characters
///     (U+E0001..U+E007F) — sit outside the BMP and rarely
///     surface in realistic Helm chart metadata pasted from
///     editors; the BMP-restricted set captures the canonical
///     paste-from-Word / paste-from-BOM-editor / paste-from-
///     typesetting-doc / paste-from-math-formula class without
///     committing to a full Unicode `Default_Ignorable_Code_Point`
///     table.
///
/// Lifted as a shared helper rather than inlined into each per-axis
/// predicate (the PRIME DIRECTIVE duplication-budget rule —
/// THEORY.md §I.3.5: "every recurring shape becomes a generator
/// before it becomes a pattern; every pattern becomes a library
/// before it becomes duplicated code. The duplication budget is
/// zero.") because two predicates ([`is_chart_description_shape`],
/// [`is_chart_maintainer_name_shape`]) carry the same UTF-8
/// free-form-prose accepted set and would otherwise inline the same
/// eight-codepoint match arm verbatim — third lift in the UAX-driven
/// render-determinism trio (peer of [`find_unicode_bidi_override`]
/// on the visual-order axis and [`find_unicode_line_break`] on the
/// single-line/multi-line axis). The third caller — every future
/// per-axis free-form-prose surface (a future Aplicacao-level
/// `:descricao` summary axis, a future per-`:contratos` edge
/// `:descricao` annotation, the future per-`:autores`-email-suffix
/// shape gate) — lands as a thin `if let Some(c) =
/// find_unicode_invisible_format(s) { … }` wrapper rather than
/// re-inlining the same codepoint match.
///
/// The arm is structurally distinct from every prior arm on the
/// calling predicates: the per-byte control-char arm catches ASCII
/// `0x00..=0x1F` plus `0x7F` DEL; the per-byte non-ASCII pass
/// admits multi-byte UTF-8 sequences (Unicode letters, em-dash,
/// arrows are canonical shapes); the bidi-override helper catches
/// the 9 visual-order codepoints; the line-break helper catches
/// the 3 single-line-vs-multi-line codepoints. None overlap the
/// eight invisible-format codepoints here — each decodes to a
/// distinct multi-byte UTF-8 sequence (`C2 AD` for U+00AD,
/// `E2 80 8B` for U+200B, `E2 81 A0` for U+2060, `E2 81 A1` for
/// U+2061, `E2 81 A2` for U+2062, `E2 81 A3` for U+2063, `E2 81
/// A4` for U+2064, `EF BB BF` for U+FEFF) the per-byte non-ASCII
/// pass deliberately accepts; only the typed codepoint scan catches
/// them.
///
/// The four math-invisible operators U+2061..=U+2064 carry their
/// semantic load only inside mathematical typesetting (MathML
/// `<mo>` invisible operators, LaTeX `\,\,` thin-space-as-invisible-
/// times) — no realistic Helm chart `:descricao` or `:autores`
/// value is a math formula. The canonical authoring footgun is the
/// paste-from-MathJax-rendered-doc / paste-from-LaTeX-equation /
/// paste-from-InDesign-math-equation shape where MathJax /
/// LaTeX2RTF / InDesign export an invisible-operator codepoint
/// between adjacent symbols to preserve the semantic operator
/// reading for screen readers, and the codepoint silently rides
/// into the YAML scalar — same invisible-identity divergence class
/// the BMP four (SHY / ZWSP / WJ / BOM) close on the paste-from-
/// Word / paste-from-BOM-editor / paste-from-typesetting-doc class.
fn find_unicode_invisible_format(s: &str) -> Option<char> {
    s.chars().find(|c| {
        matches!(
            *c,
            '\u{00AD}'
                | '\u{200B}'
                | '\u{2060}'
                | '\u{2061}'
                | '\u{2062}'
                | '\u{2063}'
                | '\u{2064}'
                | '\u{FEFF}'
        )
    })
}

/// Predicate: assert that `s` is a valid chart-description shape.
/// The `:descricao` axis is a free-form prose summary that lands in
/// the rendered `lareira-<nome>` Helm chart's `Chart.yaml`
/// `description:` field (a YAML scalar consumed by `helm list`,
/// `helm search`, Artifact Hub, and every chart-aware UI) and in
/// the chart's `README.md` header paragraph
/// (`caixa-helm/src/lib.rs:232`, `caixa-helm/src/lib.rs:333`).
/// The contract — modeled on the YAML 1.2 plain-style scalar
/// grammar and the Helm chart spec's expectation that
/// `description:` is a one-line summary:
///
///   - 1..=[`CHART_DESCRIPTION_MAX_LEN`] (512) bytes;
///   - no leading whitespace (paste-from-aligned-doc footgun —
///     YAML plain-style scalars round-trip trim-and-restore on
///     leading whitespace, so an authored `" foo"` lands as `"foo"`
///     in the rendered Chart.yaml and the round-trip back through
///     `caixa.lisp` silently drops the space);
///   - no trailing whitespace (paste-from-doc footgun — every YAML
///     dumper trims trailing whitespace from plain-style scalars,
///     so an authored `"foo "` round-trips inconsistently);
///   - no ASCII control characters anywhere (`0x00..=0x1F` plus
///     `0x7F` DEL) — tabs, newlines, carriage returns, and every
///     other control byte break the single-line YAML scalar shape
///     and the README header paragraph. The newline / CR arms are
///     the canonical paste-from-multiline-doc footgun; the tab arm
///     is the canonical paste-from-aligned-doc footgun; the
///     other-control-byte arm catches every more-exotic
///     paste-from-binary-blob shape (`0x00` NUL, `0x07` BEL,
///     `0x1B` ESC) that would silently land in the rendered
///     `Chart.yaml` as a YAML-illegal byte sequence and fail at
///     `helm lint` time far from the source caixa.lisp;
///   - non-ASCII bytes (UTF-8 continuation sequences) are
///     accepted — the canonical author shapes (`"Canonical
///     Rust→wasm32-wasip2 caixa Servico."`, `"FIXME — describe
///     this caixa"`) carry `→` (U+2192) and `—` (U+2014) and every
///     downstream consumer (YAML 1.2, Helm v3, every chart-aware
///     UI) round-trips Unicode losslessly;
///   - no Unicode bidirectional-override / isolate format
///     codepoints (U+202A `LRE`, U+202B `RLE`, U+202C `PDF`,
///     U+202D `LRO`, U+202E `RLO`, U+2066 `LRI`, U+2067 `RLI`,
///     U+2068 `FSI`, U+2069 `PDI`) — the nine codepoints UAX #9
///     names as the structural prerequisite of the "Trojan Source"
///     attack class (CVE-2021-42574 / Boucher & Anderson 2021)
///     that flip the rendered visual order of every following
///     character until a matching pop. Routed through the lifted
///     [`find_unicode_bidi_override`] helper so the same
///     nine-codepoint accepted set is shared with
///     [`is_chart_maintainer_name_shape`] on the sibling
///     YAML-plain-style-scalar surface, structurally consistent.
///     The non-ASCII byte arm above admits Unicode letters /
///     em-dash / arrows because YAML 1.2 + Helm v3 + every
///     chart-aware UI round-trip them losslessly; the bidi-override
///     codepoints break that round-trip discipline by class
///     (the byte sequence rides verbatim into the rendered
///     `Chart.yaml`'s `description:` value but renders differently
///     in `helm show chart` / Artifact Hub / `helm list` vs the
///     author's editor view of `caixa.lisp`), defeating the
///     THEORY.md §V.2 render-determinism contract every typed
///     slot carries on the same axis the per-byte CR/LF/control
///     arms above close for ASCII.
///   - no non-ASCII Unicode line-break codepoints (U+0085 `NEL`,
///     U+2028 `LS`, U+2029 `PS`) — the three codepoints UAX #14
///     (Unicode Line Breaking Algorithm) and the YAML 1.1 §4.1
///     b-char production both treat as line terminators outside
///     the ASCII `\n` / `\r` arms above. YAML 1.2 §5.4 retired
///     them per UTR #20, so YAML 1.2-strict parsers preserve them
///     verbatim while YAML 1.1 parsers (go-yaml v2 which Helm v3 /
///     kubectl link, `ruamel.yaml` in compat mode) split the
///     scalar on them — the same `:descricao` value parses as
///     single-line through one consumer and multi-line through
///     another, breaking cross-parser determinism on the same
///     axis the per-byte `\n` / `\r` arms close for ASCII.
///     Independently, every UAX #14 conformant text consumer
///     (editors, terminals, `helm list` / Artifact Hub web UIs)
///     breaks the visual line at these codepoints regardless of
///     YAML version, so the author's editor view of `caixa.lisp`
///     and the chart-consumer's rendered view diverge even when
///     both YAML parsers agree on the byte-level shape. Routed
///     through the lifted [`find_unicode_line_break`] helper so
///     the same three-codepoint accepted set is shared with
///     [`is_chart_maintainer_name_shape`], peer of the
///     [`find_unicode_bidi_override`] lift on the same two
///     predicates one trajectory earlier.
///   - no Unicode invisible-format codepoints (U+00AD `SHY`,
///     U+200B `ZWSP`, U+2060 `WJ`, U+2061 `FA` FUNCTION
///     APPLICATION, U+2062 `IT` INVISIBLE TIMES, U+2063 `IS`
///     INVISIBLE SEPARATOR, U+2064 `IP` INVISIBLE PLUS, U+FEFF
///     `ZWNBSP` / BOM) — the eight BMP Cf-category zero-width
///     codepoints with no visible glyph in any conforming font.
///     The author's editor view of `caixa.lisp` and the chart-
///     consumer's `helm list` / Artifact Hub description column
///     agree on the visible glyph sequence (`"Canonical Servico"`
///     and `"Canonical\u{200B}Servico"` render identically), but
///     the byte sequence the YAML-plain-style-scalar carries
///     verbatim differs — every byte-level grep / diff / equality
///     comparison and the Artifact Hub description-search index
///     lookup disagree silently with the visible-glyph match.
///     Closes the canonical paste-from-Microsoft-Word (SHY auto-
///     inserted at hyphenation candidates), paste-from-text-
///     editor-saved-as-UTF-8-with-BOM (leading BOM byte),
///     paste-from-typesetting-doc (ZWSP / WJ invisible word-break
///     hints), and paste-from-MathJax/LaTeX-rendered-formula
///     (FUNCTION APPLICATION / INVISIBLE TIMES / INVISIBLE
///     SEPARATOR / INVISIBLE PLUS — the four math-formula
///     invisible operators MathJax / LaTeX export between
///     adjacent symbols for screen-reader operator semantics)
///     footguns. Routed through the lifted
///     [`find_unicode_invisible_format`] helper so the same
///     eight-codepoint accepted set is shared with
///     [`is_chart_maintainer_name_shape`], third lift in the
///     UAX-driven render-determinism trio (peer of
///     [`find_unicode_bidi_override`] on the visual-order axis
///     and [`find_unicode_line_break`] on the single-line/multi-
///     line axis). The eight-codepoint set excludes U+200C
///     `ZWNJ` / U+200D `ZWJ` (legitimate compositional load in
///     Indic / Persian scripts and emoji ZWJ sequences) and
///     U+200E `LRM` / U+200F `RLM` (legitimate single-character
///     direction hints in mixed-script prose); the visible-order
///     risk on bidi overrides — not marks — is closed by the
///     prior helper.
///
/// The predicate is a *structural* floor — it enforces the
/// single-line printable-UTF-8 shape every realistic chart
/// description carries, not a per-byte alphabet check (which would
/// regress every non-ASCII canonical fixture). Same trajectory as
/// [`is_spdx_expression_shape`] (the ASCII-alphabet floor on the
/// `:licenca` axis) and [`is_git_repo_url`] (the URL-shape floor on
/// the `:repositorio` axis): the typed validator refuses the
/// downstream consumer's would-also-refuse shapes at the source
/// caixa.lisp boundary with the offending value named verbatim.
///
/// Returns the parser-shaped reason on rejection (without wrapping
/// in any error variant) so each per-axis caller —
/// [`crate::Caixa::validate_descricao`] for the universal
/// `:descricao` axis at validate time, every future per-description
/// axis (a future Aplicacao-level `:descricao` summary axis on
/// `mesh.pleme.io/v1alpha1/Caixa` CRs, a future Servico-level
/// per-`:contratos` edge `:descricao` annotation) — wraps the same
/// reason in its own typed `*Invalid { <axis>, reason }` variant.
/// The reason wording is axis-agnostic ("chart descriptions reject
/// leading whitespace") so every call site reading the same
/// diagnostic points at the same rule; drift between any two axes'
/// rule enforcement is a build error visible at this predicate, not
/// a per-renderer "this passed validate but `helm lint` rejected
/// the Chart.yaml `description:` value" surprise.
///
/// Empty input is rejected here (defensively) and at each call
/// site via the narrower [`crate::ManifestError::DescricaoEmpty`]
/// variant — the same empty-first cascade [`is_dns_1123_label`],
/// [`is_gateway_api_http_path`], [`is_wit_world_ref`],
/// [`is_nats_subject`], [`is_wasi_keyvalue_slot`],
/// [`is_git_ref_name`], [`is_git_oid`], [`is_git_repo_url`],
/// [`is_cargo_feature_name`], and [`is_spdx_expression_shape`] all
/// carry.
///
/// # Errors
///
/// Returns the parser-shaped reason naming the specific violation
/// (length / leading-whitespace / trailing-whitespace /
/// tab / newline / carriage-return / other-control-byte /
/// Unicode-bidi-override-codepoint / Unicode-line-break-codepoint),
/// without wrapping in any error variant — every caller maps the
/// same `String` into its own typed `*Invalid { <axis>, reason }`
/// enum variant.
pub fn is_chart_description_shape(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("must not be empty".to_string());
    }
    if s.len() > CHART_DESCRIPTION_MAX_LEN {
        return Err(format!(
            "exceeds chart description max length of {CHART_DESCRIPTION_MAX_LEN} bytes \
             (got {} bytes; realistic chart descriptions like `\"Canonical \
             Rust→wasm32-wasip2 caixa Servico.\"` rarely exceed ~64 bytes — this \
             length suggests a paste-from-doc multi-paragraph blob landed in the \
             `:descricao` slot)",
            s.len()
        ));
    }
    let bytes = s.as_bytes();
    if bytes[0] == b' ' {
        return Err(
            "must not start with whitespace (chart descriptions are single-line YAML \
             plain-style scalars; a leading space is the canonical \
             paste-from-aligned-doc footgun and round-trips inconsistently — every \
             YAML dumper trims leading whitespace from plain-style scalars, so the \
             authored space silently drops in the rendered Chart.yaml)"
                .to_string(),
        );
    }
    if *bytes.last().expect("non-empty checked above") == b' ' {
        return Err(
            "must not end with whitespace (chart descriptions don't terminate with \
             trailing whitespace; every YAML dumper trims trailing whitespace from \
             plain-style scalars, so the authored space round-trips inconsistently \
             back through `caixa.lisp`)"
                .to_string(),
        );
    }
    for &b in bytes {
        if b == b'\t' {
            return Err(
                "must not contain tab character (chart descriptions are single-line \
                 YAML plain-style scalars; tabs are the canonical \
                 paste-from-aligned-doc footgun and break the single-line scalar \
                 shape — every downstream YAML 1.2 parser is forbidden from \
                 emitting indentation tabs and tabs in plain-style scalars are \
                 implementation-defined)"
                    .to_string(),
            );
        }
        if b == b'\n' {
            return Err(
                "must not contain newline (chart descriptions are single-line YAML \
                 plain-style scalars; an embedded newline is the canonical \
                 paste-from-multiline-doc footgun and lands as a multi-line YAML \
                 block scalar in the rendered Chart.yaml — every chart-aware UI \
                 (`helm list`, `helm search`, Artifact Hub) renders the description \
                 in a single-line column, so the embedded newline is silently \
                 dropped at every downstream consumer)"
                    .to_string(),
            );
        }
        if b == b'\r' {
            return Err("must not contain carriage return (chart descriptions are \
                 single-line YAML plain-style scalars; a `\\r` byte is the canonical \
                 paste-from-Windows-CRLF-doc footgun and lands as a literal CR in \
                 the rendered Chart.yaml — every YAML 1.2 parser treats CR as a \
                 line terminator equivalent to LF, so the embedded CR is silently \
                 normalized to a newline at every downstream consumer)"
                .to_string());
        }
        if b < 0x20 || b == 0x7F {
            return Err(format!(
                "must not contain control character 0x{b:02x} (chart descriptions \
                 are printable UTF-8 single-line scalars; the control-byte arm \
                 catches paste-from-binary-blob footguns like `0x00` NUL, `0x07` \
                 BEL, `0x1b` ESC that would silently land in the rendered \
                 Chart.yaml as a YAML-illegal byte sequence and fail at `helm lint` \
                 time far from the source caixa.lisp)"
            ));
        }
    }
    if let Some(c) = find_unicode_bidi_override(s) {
        return Err(format!(
            "must not contain Unicode bidirectional-override codepoint U+{cp:04X} \
             (the nine codepoints UAX #9 names as the structural prerequisite of \
             the \"Trojan Source\" attack class — CVE-2021-42574 / Boucher & \
             Anderson 2021: U+202A `LRE`, U+202B `RLE`, U+202C `PDF`, U+202D `LRO`, \
             U+202E `RLO`, U+2066 `LRI`, U+2067 `RLI`, U+2068 `FSI`, U+2069 `PDI` \
             — flip the rendered visual order of every following character until a \
             matching pop, so a `:descricao` string visible to a human reading \
             `caixa.lisp` and the same string consumed by `helm show chart` / \
             `helm list` / Artifact Hub / every chart-aware UI disagree on the \
             order of the displayed content bytes. The byte sequence \
             ({utf8_seq}) rides verbatim into the rendered Chart.yaml's \
             `description:` value at the same axis the per-byte CR/LF/control \
             arms close for ASCII, but renders differently across consumers, \
             defeating the THEORY.md §V.2 render-determinism contract every typed \
             slot carries. The non-ASCII byte arm above admits Unicode letters / \
             em-dash / arrows because YAML 1.2 + Helm v3 round-trip them \
             losslessly; this codepoint breaks that round-trip discipline by \
             class. Drop the bidi-override codepoint; pure visual right-to-left \
             text (Hebrew, Arabic) is accepted natively without explicit \
             direction marks)",
            cp = c as u32,
            utf8_seq = c
                .encode_utf8(&mut [0u8; 4])
                .bytes()
                .map(|b| format!("0x{b:02X}"))
                .collect::<Vec<_>>()
                .join(" "),
        ));
    }
    if let Some(c) = find_unicode_line_break(s) {
        return Err(format!(
            "must not contain Unicode line-break codepoint U+{cp:04X} (the three \
             codepoints UAX #14 / YAML 1.1 §4.1 name as line terminators outside \
             the ASCII `\\n` / `\\r` arms above: U+0085 `NEL` NEXT LINE, U+2028 \
             `LS` LINE SEPARATOR, U+2029 `PS` PARAGRAPH SEPARATOR. YAML 1.2 §5.4 \
             retired them per UTR #20 so YAML 1.2-strict parsers preserve them \
             verbatim, but YAML 1.1 parsers (go-yaml v2 which Helm v3 / kubectl / \
             every Kubernetes client library transitively links, `ruamel.yaml` in \
             compat mode) still split scalars on them — the same `:descricao` \
             value parses as a single-line plain-style scalar through one \
             downstream consumer and a multi-line block scalar through another, \
             breaking cross-parser determinism on the same axis the per-byte \
             `\\n` / `\\r` arms close for ASCII. Independently, every UAX #14 \
             conformant text consumer (editors, terminals, `helm list` / \
             `helm search` / Artifact Hub web UIs) breaks the visual line at \
             these codepoints regardless of YAML version, so the author's editor \
             view of `caixa.lisp` and the chart-consumer's rendered view of the \
             `description:` field diverge even when both YAML parsers agree on \
             the byte-level shape, defeating the THEORY.md §V.2 render-\
             determinism contract every typed slot carries. The byte sequence \
             ({utf8_seq}) rides verbatim into the rendered Chart.yaml at the \
             same axis the per-byte `\\n` / `\\r` arms close for ASCII. Routed \
             through the shared [`find_unicode_line_break`] helper so the same \
             three-codepoint accepted set lives in exactly one place across the \
             [`is_chart_maintainer_name_shape`] sibling YAML-plain-style-scalar \
             surface, peer of the [`find_unicode_bidi_override`] lift on the \
             same two predicates one trajectory earlier. Drop the non-ASCII \
             line-break codepoint; split the value into separate logical lines \
             at the source if a multi-line summary is intended (the \
             `:descricao` axis is single-line by contract — the multi-paragraph \
             shape belongs in the chart `README.md` body, not the YAML \
             `description:` scalar))",
            cp = c as u32,
            utf8_seq = c
                .encode_utf8(&mut [0u8; 4])
                .bytes()
                .map(|b| format!("0x{b:02X}"))
                .collect::<Vec<_>>()
                .join(" "),
        ));
    }
    if let Some(c) = find_unicode_invisible_format(s) {
        return Err(format!(
            "must not contain Unicode invisible-format codepoint U+{cp:04X} (the \
             eight BMP Cf-category zero-width codepoints with no visible glyph in \
             any conforming font: U+00AD `SHY` SOFT HYPHEN, U+200B `ZWSP` ZERO \
             WIDTH SPACE, U+2060 `WJ` WORD JOINER, U+2061 `FA` FUNCTION \
             APPLICATION, U+2062 `IT` INVISIBLE TIMES, U+2063 `IS` INVISIBLE \
             SEPARATOR, U+2064 `IP` INVISIBLE PLUS, U+FEFF `ZWNBSP` ZERO WIDTH \
             NO-BREAK SPACE / BOM. The invisible-identity divergence: the \
             author's editor view of `caixa.lisp`, the chart-consumer's \
             `helm list` / `helm search` / Artifact Hub description column, \
             and every conformant terminal / browser / editor agree on the \
             visible glyph sequence (the codepoint renders as nothing, so \
             `\"Canonical Servico\"` and `\"Canonical\\u{{200B}}Servico\"` look \
             identical end-to-end), but the byte sequence the YAML-plain-style-\
             scalar carries verbatim differs — every byte-level grep / diff / \
             equality comparison over the rendered Chart.yaml `description:` \
             value disagrees with the visible-glyph match, and the Artifact Hub \
             description-search index lookup misses the authored entry because \
             the byte sequence carries an extra invisible codepoint between \
             letters. The canonical authoring shapes that silently introduce \
             these codepoints: paste-from-Microsoft-Word (SHY auto-inserted at \
             every hyphenation candidate), paste-from-text-editor-saved-as-UTF-8-\
             with-BOM (BOM leading byte from Notepad / older VS Code defaults), \
             paste-from-typesetting-doc (ZWSP / WJ invisible word-break hints \
             from InDesign / LaTeX-rendered PDF copy-paste), and paste-from-\
             MathJax/LaTeX-rendered-formula (FUNCTION APPLICATION / INVISIBLE \
             TIMES / INVISIBLE SEPARATOR / INVISIBLE PLUS — MathJax / LaTeX2RTF \
             / InDesign math-equation export emit one of these between adjacent \
             symbols to preserve operator semantics for screen readers, and the \
             codepoint silently rides into the YAML scalar with no visible \
             trace). The byte sequence ({utf8_seq}) rides verbatim into the \
             rendered Chart.yaml at the same axis the per-byte CR/LF/control \
             arms close for ASCII, but renders as nothing across consumers, \
             defeating the THEORY.md §V.2 render-determinism contract on a \
             third axis from the bidi-override (visual-order) and line-break \
             (single-line vs multi-line) classes the prior arms close. Routed \
             through the shared [`find_unicode_invisible_format`] helper so \
             the eight-codepoint accepted set lives in exactly one place \
             across the [`is_chart_maintainer_name_shape`] sibling \
             YAML-plain-style-scalar surface, third lift in the UAX-driven \
             render-determinism trio (peer of [`find_unicode_bidi_override`] \
             on the visual-order axis and [`find_unicode_line_break`] on the \
             single-line/multi-line axis). Drop the invisible codepoint; emoji \
             ZWJ sequences (U+200D for the 👨‍💻 family) and bidi direction-mark \
             codepoints (U+200E `LRM` / U+200F `RLM`) are accepted natively — \
             only the eight zero-semantic-content codepoints are rejected)",
            cp = c as u32,
            utf8_seq = c
                .encode_utf8(&mut [0u8; 4])
                .bytes()
                .map(|b| format!("0x{b:02X}"))
                .collect::<Vec<_>>()
                .join(" "),
        ));
    }
    Ok(())
}

/// Maximum byte length of a chart-maintainer-name-shaped string. The
/// 128-byte cap is the axis-appropriate ceiling for the per-entry
/// identifier the `:autores` Vec axis carries: every realistic Helm
/// chart maintainer name in the wild (`"pleme-io"`, `"Pleme
/// Contributors"`, `"alice <alice@example.com>"`, `"François
/// Dupont"`) sits well under 64 bytes, and the 128-byte cap surfaces
/// the "paste-from-doc multi-paragraph blob landed in a single
/// `:autores` entry" footgun at validate time. Tighter than
/// [`CHART_DESCRIPTION_MAX_LEN`] (512) on the sibling free-form-prose
/// axis where multi-sentence summaries are the canonical shape;
/// peer with [`WIT_IDENT_MAX_LEN`] (128) on the sibling
/// short-identifier-class axis.
pub const CHART_MAINTAINER_NAME_MAX_LEN: usize = 128;

/// Predicate: assert that `s` is a valid chart-maintainer-name shape.
/// The `:autores` axis is a per-entry maintainer identifier that lands
/// in the rendered `lareira-<nome>` Helm chart's `Chart.yaml`
/// `maintainers: [{name: …, email: null}]` array via
/// [`caixa-helm`]'s `build_chart_yaml` (`caixa-helm/src/lib.rs:251`);
/// each entry becomes the `name:` value of a single `Maintainer`
/// record (a YAML scalar consumed by `helm list`, `helm search`,
/// Artifact Hub's maintainer index, and every chart-aware UI). The
/// contract — modeled on the same YAML 1.2 plain-style scalar
/// grammar [`is_chart_description_shape`] enforces on the sibling
/// `:descricao` axis, with a tighter length cap for the per-entry
/// identifier class:
///
///   - 1..=[`CHART_MAINTAINER_NAME_MAX_LEN`] (128) bytes;
///   - no leading whitespace (paste-from-aligned-doc footgun —
///     YAML plain-style scalars round-trip trim-and-restore on
///     leading whitespace, so an authored `" pleme-io"` lands as
///     `"pleme-io"` in the rendered Chart.yaml and the round-trip
///     back through `caixa.lisp` silently drops the space);
///   - no trailing whitespace (paste-from-doc footgun — every YAML
///     dumper trims trailing whitespace from plain-style scalars,
///     so an authored `"pleme-io "` round-trips inconsistently);
///   - no ASCII control characters anywhere (`0x00..=0x1F` plus
///     `0x7F` DEL) — tabs, newlines, carriage returns, and every
///     other control byte break the single-line YAML scalar shape
///     and the `helm list` / `helm search` / Artifact Hub
///     maintainer-column rendering. The newline / CR arms are the
///     canonical paste-from-multiline-doc footgun (the author
///     pasted a multi-line block of author records into one
///     `:autores` entry instead of splitting them into one entry
///     per author); the tab arm is the canonical
///     paste-from-aligned-doc footgun; the other-control-byte
///     arm catches every more-exotic paste-from-binary-blob shape;
///   - non-ASCII bytes (UTF-8 continuation sequences) are accepted
///     — realistic maintainer names carry Unicode (`"François"`,
///     `"日本語"`, `"naïve"`) and every downstream consumer
///     (YAML 1.2, Helm v3, every chart-aware UI) round-trips
///     Unicode losslessly;
///   - no Unicode bidirectional-override / isolate format
///     codepoints (U+202A `LRE`, U+202B `RLE`, U+202C `PDF`,
///     U+202D `LRO`, U+202E `RLO`, U+2066 `LRI`, U+2067 `RLI`,
///     U+2068 `FSI`, U+2069 `PDI`) — the nine codepoints UAX #9
///     names as the structural prerequisite of the "Trojan Source"
///     attack class (CVE-2021-42574). A maintainer-name with an
///     embedded `RLO` flips the visual order of every trailing
///     byte, so an `:autores "alice\u{202E}example.com<bob@"` (the
///     paste-from-attacker-crafted-doc footgun) renders in
///     `helm list`'s maintainer column / Artifact Hub as
///     `alice<@bob>moc.elpmaxe` but rides verbatim into the
///     rendered Chart.yaml `maintainers:` array — same Trojan
///     Source class [`is_chart_description_shape`] closes on the
///     sibling `:descricao` axis. Routed through the same lifted
///     [`find_unicode_bidi_override`] helper so the nine-codepoint
///     accepted set is shared, structurally consistent.
///   - no non-ASCII Unicode line-break codepoints (U+0085 `NEL`,
///     U+2028 `LS`, U+2029 `PS`) — the three codepoints UAX #14
///     (Unicode Line Breaking Algorithm) and YAML 1.1 §4.1 b-char
///     production both treat as line terminators outside the
///     ASCII `\n` / `\r` arms above. YAML 1.2 §5.4 retired them
///     per UTR #20 so the cross-parser line-break disagreement
///     (go-yaml v2 / YAML 1.1 still splits; YAML 1.2-strict
///     parsers preserve) breaks the THEORY.md §V.2 render-
///     determinism contract on the same axis the per-byte `\n` /
///     `\r` arms close for ASCII. A maintainer-name with an
///     embedded U+2028 parses as one entry through a YAML 1.2
///     parser and as two `maintainers:` array entries through a
///     YAML 1.1 parser — same paste-from-multiline-doc class the
///     `\n` arm above closes, extended to the non-ASCII line-break
///     codepoints the per-byte non-ASCII pass deliberately
///     admits for Unicode letters. Routed through the same lifted
///     [`find_unicode_line_break`] helper so the three-codepoint
///     accepted set is shared with [`is_chart_description_shape`]
///     on the sibling YAML-plain-style-scalar surface,
///     structurally consistent.
///   - no Unicode invisible-format codepoints (U+00AD `SHY`,
///     U+200B `ZWSP`, U+2060 `WJ`, U+2061 `FA` FUNCTION
///     APPLICATION, U+2062 `IT` INVISIBLE TIMES, U+2063 `IS`
///     INVISIBLE SEPARATOR, U+2064 `IP` INVISIBLE PLUS, U+FEFF
///     `ZWNBSP` / BOM) — the eight BMP Cf-category zero-width
///     codepoints with no visible glyph. A maintainer-name with
///     an embedded U+200B (`"alice\u{200B}"`) renders identically
///     to `"alice"` in `helm list` / Artifact Hub's maintainer
///     column, yet the byte sequence is distinct — the Artifact
///     Hub maintainer-index lookup misses the authored `"alice"`
///     entry, and a future CLA-signer lookup matches a visually-
///     identical-but-byte-distinct identity (the canonical
///     invisible-codepoint homograph footgun on the maintainer-
///     identity axis). Closes the canonical paste-from-Microsoft-
///     Word (SHY), paste-from-text-editor-saved-as-UTF-8-with-BOM
///     (BOM), paste-from-typesetting-doc (ZWSP / WJ), and
///     paste-from-MathJax/LaTeX-rendered-formula (FUNCTION
///     APPLICATION / INVISIBLE TIMES / INVISIBLE SEPARATOR /
///     INVISIBLE PLUS — math-formula invisible operators
///     MathJax / LaTeX2RTF / InDesign emit between symbols for
///     screen-reader operator semantics) footguns. Routed through
///     the same lifted [`find_unicode_invisible_format`] helper
///     so the eight-codepoint accepted set is shared with
///     [`is_chart_description_shape`], third lift in the UAX-
///     driven render-determinism trio (peer of
///     [`find_unicode_bidi_override`] on the visual-order axis
///     and [`find_unicode_line_break`] on the single-line/multi-
///     line axis). The eight-codepoint set excludes U+200C
///     `ZWNJ` / U+200D `ZWJ` (emoji ZWJ sequences are canonical
///     for modern maintainer-display names) and U+200E `LRM` /
///     U+200F `RLM` (mixed-script direction hints are canonical
///     for "Arabic name with embedded ASCII email" shapes).
///
/// Same structural single-line printable-UTF-8 floor as
/// [`is_chart_description_shape`] — both `:descricao` and `:autores`
/// land as YAML plain-style scalars in the same `Chart.yaml` and
/// share every paste-from-doc footgun the YAML 1.2 grammar refuses
/// at parse time. The two predicates differ only on the byte
/// length cap: 512 bytes for `:descricao` (multi-sentence prose
/// shape) vs 128 bytes for `:autores` entries (short-identifier
/// shape). Returns the parser-shaped reason on rejection (without
/// wrapping in any error variant) so each per-axis caller —
/// [`crate::Caixa::validate_autores`] for the universal `:autores`
/// axis at validate time, every future per-maintainer-name axis (a
/// future caixa-registry maintainer-index entry, a future
/// chart-author CLA-signer lookup) — wraps the same reason in its
/// own typed `*Invalid { <axis>, reason }` variant.
///
/// Empty input is rejected here (defensively) and at each call
/// site via the narrower [`crate::ManifestError::AutorEmpty`]
/// variant — the same empty-first cascade [`is_dns_1123_label`],
/// [`is_gateway_api_http_path`], [`is_wit_world_ref`],
/// [`is_nats_subject`], [`is_wasi_keyvalue_slot`],
/// [`is_git_ref_name`], [`is_git_oid`], [`is_git_repo_url`],
/// [`is_cargo_feature_name`], [`is_spdx_expression_shape`], and
/// [`is_chart_description_shape`] all carry.
///
/// # Errors
///
/// Returns the parser-shaped reason naming the specific violation
/// (length / leading-whitespace / trailing-whitespace / tab /
/// newline / carriage-return / other-control-byte /
/// Unicode-bidi-override-codepoint), without wrapping in any error
/// variant — every caller maps the same `String` into its own typed
/// `*Invalid { <axis>, reason }` enum variant.
pub fn is_chart_maintainer_name_shape(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("must not be empty".to_string());
    }
    if s.len() > CHART_MAINTAINER_NAME_MAX_LEN {
        return Err(format!(
            "exceeds chart maintainer name max length of \
             {CHART_MAINTAINER_NAME_MAX_LEN} bytes (got {} bytes; realistic chart \
             maintainer names like `\"pleme-io\"`, `\"Pleme Contributors\"`, \
             `\"alice <alice@example.com>\"` rarely exceed ~64 bytes — this \
             length suggests a paste-from-doc multi-paragraph blob landed in a \
             single `:autores` entry instead of being split into one entry per \
             author)",
            s.len()
        ));
    }
    let bytes = s.as_bytes();
    if bytes[0] == b' ' {
        return Err(
            "must not start with whitespace (chart maintainer names are \
             single-line YAML plain-style scalars; a leading space is the \
             canonical paste-from-aligned-doc footgun and round-trips \
             inconsistently — every YAML dumper trims leading whitespace from \
             plain-style scalars, so the authored space silently drops in the \
             rendered Chart.yaml)"
                .to_string(),
        );
    }
    if *bytes.last().expect("non-empty checked above") == b' ' {
        return Err(
            "must not end with whitespace (chart maintainer names don't \
             terminate with trailing whitespace; every YAML dumper trims \
             trailing whitespace from plain-style scalars, so the authored \
             space round-trips inconsistently back through `caixa.lisp`)"
                .to_string(),
        );
    }
    for &b in bytes {
        if b == b'\t' {
            return Err(
                "must not contain tab character (chart maintainer names are \
                 single-line YAML plain-style scalars; tabs are the canonical \
                 paste-from-aligned-doc footgun and break the single-line \
                 scalar shape — every downstream YAML 1.2 parser is forbidden \
                 from emitting indentation tabs and tabs in plain-style scalars \
                 are implementation-defined)"
                    .to_string(),
            );
        }
        if b == b'\n' {
            return Err("must not contain newline (chart maintainer names are \
                 single-line YAML plain-style scalars; an embedded newline is \
                 the canonical paste-from-multiline-doc footgun — the author \
                 pasted a multi-line block of author records into one \
                 `:autores` entry instead of splitting them into one entry per \
                 author, and the result lands as a multi-line YAML block scalar \
                 in the rendered Chart.yaml `maintainers:` array)"
                .to_string());
        }
        if b == b'\r' {
            return Err("must not contain carriage return (chart maintainer \
                 names are single-line YAML plain-style scalars; a `\\r` byte \
                 is the canonical paste-from-Windows-CRLF-doc footgun and \
                 lands as a literal CR in the rendered Chart.yaml — every YAML \
                 1.2 parser treats CR as a line terminator equivalent to LF, \
                 so the embedded CR is silently normalized to a newline at \
                 every downstream consumer)"
                .to_string());
        }
        if b < 0x20 || b == 0x7F {
            return Err(format!(
                "must not contain control character 0x{b:02x} (chart \
                 maintainer names are printable UTF-8 single-line scalars; the \
                 control-byte arm catches paste-from-binary-blob footguns like \
                 `0x00` NUL, `0x07` BEL, `0x1b` ESC that would silently land \
                 in the rendered Chart.yaml as a YAML-illegal byte sequence \
                 and fail at `helm lint` time far from the source caixa.lisp)"
            ));
        }
    }
    if let Some(c) = find_unicode_bidi_override(s) {
        return Err(format!(
            "must not contain Unicode bidirectional-override codepoint U+{cp:04X} \
             (the nine codepoints UAX #9 names as the structural prerequisite of \
             the \"Trojan Source\" attack class — CVE-2021-42574 / Boucher & \
             Anderson 2021: U+202A `LRE`, U+202B `RLE`, U+202C `PDF`, U+202D `LRO`, \
             U+202E `RLO`, U+2066 `LRI`, U+2067 `RLI`, U+2068 `FSI`, U+2069 `PDI` \
             — flip the rendered visual order of every following character until a \
             matching pop, so an `:autores` entry visible to a human reading \
             `caixa.lisp` and the same entry consumed by `helm list` / Artifact \
             Hub's maintainer column disagree on the order of the displayed \
             content bytes. The byte sequence ({utf8_seq}) rides verbatim into \
             the rendered Chart.yaml `maintainers:` array at the same axis the \
             per-byte CR/LF/control arms close for ASCII, but renders \
             differently across consumers, defeating the THEORY.md §V.2 \
             render-determinism contract every typed slot carries. Routed through \
             the shared [`find_unicode_bidi_override`] helper so the same \
             nine-codepoint accepted set lives in exactly one place across the \
             [`is_chart_description_shape`] sibling YAML-plain-style-scalar \
             surface, structurally consistent. Drop the bidi-override codepoint; \
             pure visual right-to-left maintainer names (Hebrew, Arabic) are \
             accepted natively without explicit direction marks)",
            cp = c as u32,
            utf8_seq = c
                .encode_utf8(&mut [0u8; 4])
                .bytes()
                .map(|b| format!("0x{b:02X}"))
                .collect::<Vec<_>>()
                .join(" "),
        ));
    }
    if let Some(c) = find_unicode_line_break(s) {
        return Err(format!(
            "must not contain Unicode line-break codepoint U+{cp:04X} (the three \
             codepoints UAX #14 / YAML 1.1 §4.1 name as line terminators outside \
             the ASCII `\\n` / `\\r` arms above: U+0085 `NEL` NEXT LINE, U+2028 \
             `LS` LINE SEPARATOR, U+2029 `PS` PARAGRAPH SEPARATOR. YAML 1.2 §5.4 \
             retired them per UTR #20 so YAML 1.2-strict parsers preserve them \
             verbatim, but YAML 1.1 parsers (go-yaml v2 which Helm v3 / kubectl / \
             every Kubernetes client library transitively links, `ruamel.yaml` in \
             compat mode) still split scalars on them — an `:autores` entry with \
             an embedded U+2028 parses as one `maintainers:` array entry through \
             a YAML 1.2 parser and as two entries through a YAML 1.1 parser, \
             breaking cross-parser determinism on the same axis the per-byte \
             `\\n` / `\\r` arms close for ASCII. Independently, every UAX #14 \
             conformant text consumer (editors, terminals, `helm list` / \
             Artifact Hub's maintainer column) breaks the visual line at these \
             codepoints regardless of YAML version, so the author's editor view \
             of `caixa.lisp` and the chart-consumer's rendered view of the \
             `maintainers:` entry diverge even when both YAML parsers agree on \
             the byte-level shape, defeating the THEORY.md §V.2 render-\
             determinism contract every typed slot carries. The byte sequence \
             ({utf8_seq}) rides verbatim into the rendered Chart.yaml at the \
             same axis the per-byte `\\n` / `\\r` arms close for ASCII. Routed \
             through the shared [`find_unicode_line_break`] helper so the same \
             three-codepoint accepted set lives in exactly one place across the \
             [`is_chart_description_shape`] sibling YAML-plain-style-scalar \
             surface, peer of the [`find_unicode_bidi_override`] lift on the \
             same two predicates one trajectory earlier. Drop the non-ASCII \
             line-break codepoint; split the value into separate `:autores` \
             list entries at the source — the per-entry shape is single-line by \
             contract)",
            cp = c as u32,
            utf8_seq = c
                .encode_utf8(&mut [0u8; 4])
                .bytes()
                .map(|b| format!("0x{b:02X}"))
                .collect::<Vec<_>>()
                .join(" "),
        ));
    }
    if let Some(c) = find_unicode_invisible_format(s) {
        return Err(format!(
            "must not contain Unicode invisible-format codepoint U+{cp:04X} (the \
             eight BMP Cf-category zero-width codepoints with no visible glyph: \
             U+00AD `SHY` SOFT HYPHEN, U+200B `ZWSP` ZERO WIDTH SPACE, U+2060 \
             `WJ` WORD JOINER, U+2061 `FA` FUNCTION APPLICATION, U+2062 `IT` \
             INVISIBLE TIMES, U+2063 `IS` INVISIBLE SEPARATOR, U+2064 `IP` \
             INVISIBLE PLUS, U+FEFF `ZWNBSP` ZERO WIDTH NO-BREAK SPACE / BOM. \
             The maintainer-identity divergence: the author's editor view of \
             `caixa.lisp` and the `helm list` / Artifact Hub maintainer column \
             agree on the visible glyph sequence (`\"alice\"` and \
             `\"alice\\u{{200B}}\"` render identically as `alice`), but the byte \
             sequence the YAML-plain-style-scalar carries verbatim differs — \
             the Artifact Hub maintainer-index lookup misses the authored \
             `\"alice\"` entry because the byte sequence carries an extra \
             invisible codepoint, a future per-maintainer CLA-signer lookup \
             matches a visually-identical-but-byte-distinct identity (the \
             canonical invisible-codepoint homograph footgun), and every \
             byte-level diff / grep / equality comparison over the Chart.yaml \
             `maintainers:` array disagrees with the visible-glyph match. The \
             canonical authoring shapes that silently introduce these \
             codepoints: paste-from-Microsoft-Word (SHY auto-inserted at \
             every hyphenation candidate), paste-from-text-editor-saved-as-\
             UTF-8-with-BOM (BOM leading byte from Notepad / older VS Code \
             defaults / Excel CSV export), paste-from-typesetting-doc (ZWSP / \
             WJ invisible word-break hints from InDesign / LaTeX-rendered PDF \
             copy-paste), and paste-from-MathJax/LaTeX-rendered-formula \
             (FUNCTION APPLICATION / INVISIBLE TIMES / INVISIBLE SEPARATOR / \
             INVISIBLE PLUS — MathJax / LaTeX2RTF / InDesign math-equation \
             export emit one of these between adjacent symbols to preserve \
             operator semantics for screen readers, and the codepoint silently \
             rides into the YAML scalar with no visible trace). The byte \
             sequence ({utf8_seq}) rides verbatim into the rendered \
             Chart.yaml, but renders as nothing across consumers, defeating \
             the THEORY.md §V.2 render-determinism contract on a third axis \
             from the bidi-override (visual-order) and line-break (single-\
             line vs multi-line) classes the prior arms close. Routed through \
             the shared [`find_unicode_invisible_format`] helper so the \
             eight-codepoint accepted set is shared with \
             [`is_chart_description_shape`], third lift in the UAX-driven \
             render-determinism trio (peer of [`find_unicode_bidi_override`] \
             on the visual-order axis and [`find_unicode_line_break`] on the \
             single-line/multi-line axis). Drop the invisible codepoint; emoji \
             ZWJ sequences (U+200D for the 👨‍💻 family) and bidi direction-mark \
             codepoints (U+200E `LRM` / U+200F `RLM`) are accepted natively \
             for mixed-script maintainer names — only the eight zero-semantic-\
             content codepoints are rejected)",
            cp = c as u32,
            utf8_seq = c
                .encode_utf8(&mut [0u8; 4])
                .bytes()
                .map(|b| format!("0x{b:02X}"))
                .collect::<Vec<_>>()
                .join(" "),
        ));
    }
    Ok(())
}

/// Maximum byte length of a chart-keyword-shaped string. The 20-byte
/// cap matches Cargo's `[package] keywords` rule
/// (<https://doc.rust-lang.org/cargo/reference/manifest.html#the-keywords-field>:
/// "Each keyword should be ASCII text, start with a letter, and only
/// contain letters, numbers, _ or -. Keywords are case-insensitive and
/// limited to a maximum length of 20 characters.") — the same parser
/// crates.io routes its `keywords:` array entries through at publish
/// time. Tighter than every peer length cap on the typed Caixa surface
/// ([`CHART_MAINTAINER_NAME_MAX_LEN`] 128 on the sibling chart-metadata
/// `Vec<String>` axis, [`CARGO_FEATURE_NAME_MAX_LEN`] 64 on the sibling
/// `:caracteristicas` per-entry axis, [`CHART_DESCRIPTION_MAX_LEN`] 512
/// on the free-form-prose axis); the search-tag class is the tightest
/// short-identifier shape on the typed surface — every realistic
/// `:etiquetas` entry in the wild (`"iac"`, `"aws"`, `"pangea"`,
/// `"hello-world"`, `"tatara-lisp"`, `"caixa-servico"`,
/// `"infrastructure"`, `"pangea-native"`) sits well under 20 bytes,
/// and the 20-byte cap surfaces the "paste-from-doc multi-tag blob
/// landed in a single `:etiquetas` entry" footgun (`"web-service web
/// app"`, `"mesh,http,grpc"`) at validate time.
pub const CHART_KEYWORD_MAX_LEN: usize = 20;

/// Predicate: assert that `s` is a valid chart-keyword shape. The
/// `:etiquetas` axis is a per-entry registry-search-tag identifier
/// that lands in the rendered `lareira-<nome>` Helm chart's
/// `Chart.yaml` `keywords:` array via [`caixa-helm`]'s
/// `build_chart_yaml` (folded through a [`std::collections::BTreeSet`]
/// alongside the four substrate-fixed tags `lareira` / `wasm` /
/// `tatara-lisp` / `caixa-servico`) and indexes the chart through
/// Artifact Hub's keyword-search axis + the future caixa-registry's
/// keyword index. The contract — modeled on Cargo's crates.io
/// `[package] keywords` grammar (the parser the crates.io publish API
/// routes every `keywords:` entry through at publish time), narrowed
/// to the strict ASCII subset every realistic search tag uses:
///
///   - 1..=[`CHART_KEYWORD_MAX_LEN`] (20) bytes;
///   - first byte: ASCII letter (`A-Z` or `a-z`). Leading digit, `-`,
///     `_`, whitespace, control, and non-ASCII are each surfaced with
///     a self-locating reason naming the canonical authoring footgun
///     (paste-from-numbered-list `"1foo"`, kebab-leak `"-foo"`,
///     snake-leak `"_foo"`, paste-from-aligned-doc whitespace,
///     paste-from-Unicode-doc non-ASCII);
///   - remaining bytes: ASCII alphanumeric, `_`, or `-` (Cargo's
///     crates.io-accepted continuation set; tighter than
///     [`is_cargo_feature_name`]'s `_`/`-`/`+`/`.` continuation set —
///     `+` and `.` are not part of the keyword grammar). Whitespace,
///     `,` / `/` / `;` / `.` list-separator confusions, control bytes,
///     and non-ASCII bytes are each surfaced with a self-locating
///     reason naming the canonical authoring footgun (multi-tag blob
///     in one entry, CSV-list-belongs-to-list-grammar miscomprehension,
///     CR/LF paste-from-doc, NFC/NFD normalization drift).
///
/// Returns the parser-shaped reason on rejection (without wrapping in
/// any error variant) so each per-axis caller —
/// [`crate::Caixa::validate_etiquetas`] for the universal `:etiquetas`
/// axis at validate time, every future per-keyword axis (a future
/// caixa-registry keyword-index lookup, a future Artifact Hub-keyword
/// scraper validator, a future per-Aplicacao aggregated keyword set)
/// — wraps the same reason in its own typed `*Invalid { <axis>, reason }`
/// variant.
///
/// Empty input is rejected here (defensively) and at each call site
/// via the narrower [`crate::ManifestError::EtiquetaEmpty`] variant —
/// the same empty-first cascade [`is_dns_1123_label`],
/// [`is_gateway_api_http_path`], [`is_wit_world_ref`],
/// [`is_nats_subject`], [`is_wasi_keyvalue_slot`], [`is_git_ref_name`],
/// [`is_git_oid`], [`is_git_repo_url`], [`is_cargo_feature_name`],
/// [`is_spdx_expression_shape`], [`is_chart_description_shape`], and
/// [`is_chart_maintainer_name_shape`] all carry at their call sites.
///
/// # Errors
///
/// Returns the parser-shaped reason naming the specific violation
/// (length / first-byte-class / continuation-byte-class / whitespace /
/// control-char / non-ASCII / `,`-list-separator-confusion /
/// `/`-path-separator-confusion / `;`-list-separator-confusion /
/// `.`-namespace-confusion), without wrapping in any error variant —
/// every caller maps the same `String` into its own typed
/// `*Invalid { <axis>, reason }` enum variant.
pub fn is_chart_keyword_shape(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("must not be empty".to_string());
    }
    if s.len() > CHART_KEYWORD_MAX_LEN {
        return Err(format!(
            "exceeds chart keyword max length of {CHART_KEYWORD_MAX_LEN} bytes (got \
             {} bytes; legitimate `:etiquetas` search tags rarely exceed ~12 bytes — \
             this length suggests a paste-from-doc multi-tag blob landed in a single \
             `:etiquetas` entry instead of being split into one entry per tag, e.g. \
             `(\"mesh\" \"http\" \"grpc\")` not `(\"mesh-http-grpc-rpc-wasm\")`. \
             Cargo's crates.io publish API enforces the same 20-byte cap on its \
             `keywords:` array at publish time)",
            s.len()
        ));
    }
    let bytes = s.as_bytes();
    let first = bytes[0];
    if !first.is_ascii_alphabetic() {
        let msg = if first == b' ' || first == b'\t' {
            "must not start with whitespace (chart keywords are single-token \
             search-tag identifiers; the leading-whitespace arm is the canonical \
             paste-from-aligned-doc footgun and round-trips inconsistently — every \
             YAML 1.2 dumper trims leading whitespace from plain-style scalars, so \
             the authored space silently drops in the rendered Chart.yaml \
             `keywords:` array)"
                .to_string()
        } else if first == b'-' {
            "must not start with `-` (Cargo's crates.io keyword grammar rejects a \
             leading hyphen — `-` is a legitimate continuation character between \
             alphanumeric segments but the canonical CLI-argument-injection / \
             kebab-leak footgun at the start; drop the leading `-`, e.g. \
             `\"tatara-lisp\"` not `\"-tatara-lisp\"`)"
                .to_string()
        } else if first == b'_' {
            "must not start with `_` (Cargo's crates.io keyword grammar requires the \
             first character be an ASCII letter — `_` is a legitimate continuation \
             character between alphanumeric segments but the canonical \
             snake-leak / hidden-identifier footgun at the start; drop the leading \
             `_`, e.g. `\"caixa-servico\"` not `\"_caixa_servico\"`)"
                .to_string()
        } else if first.is_ascii_digit() {
            format!(
                "must not start with digit {ch:?} (Cargo's crates.io keyword grammar \
                 requires the first character be an ASCII letter — a digit at the \
                 start is the canonical paste-from-numbered-list footgun, e.g. the \
                 author copied `1. mesh` from a numbered doc and the `1` leaked \
                 into the tag; drop the leading digit, e.g. `\"v2\"` not `\"2v\"`)",
                ch = first as char
            )
        } else if first < 0x20 || first == 0x7F {
            format!(
                "must not start with control character 0x{first:02x} (Cargo's \
                 crates.io keyword grammar rejects ASCII control characters; the \
                 CR/LF arm is the canonical paste-from-multiline-doc footgun)"
            )
        } else if first >= 0x80 {
            format!(
                "must not start with non-ASCII byte 0x{first:02x} (Cargo's \
                 crates.io keyword grammar is strict ASCII; the non-ASCII arm \
                 catches the canonical paste-from-Unicode-doc footgun — every \
                 legitimate search tag is a kebab-case ASCII identifier like \
                 `\"mesh\"`, `\"wasm\"`, `\"tatara-lisp\"`. Raw non-ASCII silently \
                 round-trips inconsistently across NFC/NFD normalization on APFS / \
                 case-folding filesystems and breaks the Artifact Hub keyword \
                 search index lookup)"
            )
        } else {
            format!(
                "must start with an ASCII letter, got {ch:?} (Cargo's crates.io \
                 keyword grammar rejects every non-letter first character — the \
                 canonical search tags are kebab-case ASCII identifiers starting \
                 with a letter, like `\"mesh\"`, `\"wasm\"`, `\"hello-world\"`)",
                ch = first as char
            )
        };
        return Err(msg);
    }
    for &b in &bytes[1..] {
        let valid = b.is_ascii_alphanumeric() || b == b'_' || b == b'-';
        if !valid {
            let msg = if b == b' ' || b == b'\t' {
                format!(
                    "must not contain whitespace character {ch:?} (Cargo's \
                     crates.io keyword grammar rejects whitespace; search tags are \
                     single-token identifiers — use `-` or `_` to separate \
                     kebab-case / snake-case segments instead, or split into \
                     separate `:etiquetas` entries: `(\"web\" \"service\")` not \
                     `(\"web service\")`)",
                    ch = b as char
                )
            } else if b == b',' {
                "must not contain `,` (the comma separator belongs to the \
                 `:etiquetas` list grammar between entries, not to the keyword \
                 grammar within an entry — split the value into separate list \
                 entries: `(\"mesh\" \"http\" \"grpc\")` not `(\"mesh,http,grpc\")`. \
                 The author confused the CSV-style list-separator convention with \
                 the list grammar)"
                    .to_string()
            } else if b == b'/' {
                "must not contain `/` (Cargo's crates.io keyword grammar rejects \
                 path-style separators within a tag; the segment separator within \
                 a search tag is `-` or `_`, and multi-segment paths belong as \
                 separate `:etiquetas` entries: `(\"caixa\" \"servico\")` not \
                 `(\"caixa/servico\")`)"
                    .to_string()
            } else if b == b';' {
                "must not contain `;` (the semicolon separator is not part of the \
                 `:etiquetas` list grammar — split the value into separate list \
                 entries: `(\"mesh\" \"http\")` not `(\"mesh;http\")`. The author \
                 confused another lisp-list-style separator with the list \
                 grammar)"
                    .to_string()
            } else if b == b'.' {
                "must not contain `.` (Cargo's crates.io keyword grammar excludes \
                 `.` from the continuation set — the canonical \
                 namespace-confusion / version-suffix footgun, e.g. `\"http.1\"` \
                 / `\"v1.0\"`; use `-` instead, e.g. `\"http-1\"` / `\"v1-0\"`)"
                    .to_string()
            } else if b == b'\n' {
                "must not contain newline (chart keywords are single-line \
                 single-token identifiers; an embedded newline is the canonical \
                 paste-from-multiline-doc footgun — the author pasted a multi-tag \
                 block into one `:etiquetas` entry instead of splitting into one \
                 entry per tag)"
                    .to_string()
            } else if b == b'\r' {
                "must not contain carriage return (chart keywords are single-line \
                 single-token identifiers; a `\\r` byte is the canonical \
                 paste-from-Windows-CRLF-doc footgun and lands as a literal CR in \
                 the rendered Chart.yaml `keywords:` array)"
                    .to_string()
            } else if b < 0x20 || b == 0x7F {
                format!(
                    "must not contain control character 0x{b:02x} (Cargo's \
                     crates.io keyword grammar rejects ASCII control characters; \
                     the control-byte arm catches paste-from-binary-blob footguns \
                     like `0x00` NUL, `0x07` BEL, `0x1b` ESC, `0x7f` DEL that \
                     would silently land in the rendered Chart.yaml \
                     `keywords:` array as a YAML-illegal byte sequence)"
                )
            } else if b >= 0x80 {
                format!(
                    "must not contain non-ASCII byte 0x{b:02x} (Cargo's crates.io \
                     keyword grammar is strict ASCII; the non-ASCII arm catches \
                     the canonical paste-from-Unicode-doc footgun — raw non-ASCII \
                     silently round-trips inconsistently across NFC/NFD \
                     normalization on APFS / case-folding filesystems and breaks \
                     the Artifact Hub keyword search index lookup)"
                )
            } else {
                format!(
                    "contains invalid character {ch:?} (Cargo's crates.io keyword \
                     grammar allows only `[A-Za-z0-9_-]` after the first \
                     character)",
                    ch = b as char
                )
            };
            return Err(msg);
        }
    }
    Ok(())
}

/// Tagged reason a caixa-author-supplied path can fail the
/// sandboxed-relative shape gate every callback / script path must
/// pass for the layout checker's `root.join(p)` to stay inside the
/// caixa root.
///
/// Returned by [`is_sandboxed_relative_path`] so each per-axis caller
/// — [`crate::BehaviorSpec::validate`] on `:behavior :on-*` paths
/// (b0c8389), [`crate::UpgradeInstruction::validate`]'s `StateChange`
/// arm on `:upgrade-from :state-change :script` (26da2c7), every
/// future axis admitting a user-supplied path — match-and-wraps the
/// tag into its own typed `*Invalid { slot, path }` enum variant so
/// the diagnostic still names *which slot* carried the malformed
/// value. The tag is axis-agnostic; the wrapping per-axis variant
/// carries the slot identity.
///
/// Sibling discriminator-style of the per-arm reason substrings every
/// value-shape predicate already exposes (`is_dns_1123_label`,
/// `is_gateway_api_http_path`, …) — but typed rather than string-
/// shaped, because the per-axis variants for path violations were
/// already split three ways (`EmptyPath` / `AbsolutePath` /
/// `ParentEscape` in `BehaviorError`; `EmptyScript` / `AbsoluteScript`
/// / `ParentEscapeScript` in `UpgradeError`), so collapsing them to a
/// single `*PathInvalid { reason }` variant would *regress* the
/// diagnostic shape rather than preserve it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathShapeViolation {
    /// The path string is empty — `PathBuf::new()` or the
    /// canonical "I declared the slot but left the value blank"
    /// authoring footgun. `root.join(PathBuf::new())` resolves to
    /// `root` itself, silently pointing the runtime's `LisleLoader`
    /// at the project root rather than a file.
    Empty,
    /// The path is absolute — `Path::join` *replaces* the base
    /// with an absolute right-hand side, so `root.join("/etc/passwd")`
    /// resolves to `"/etc/passwd"` and escapes the project sandbox
    /// entirely. The Lunatic-style sandbox discipline
    /// ([`theory/INSPIRATIONS.md` §III.1][i31]) requires every
    /// author-supplied path to live under the caixa root.
    ///
    /// [i31]: https://github.com/pleme-io/theory/blob/main/INSPIRATIONS.md
    Absolute,
    /// The path contains a [`Component::ParentDir`] component anywhere
    /// — `root.join("../sibling/x")` traverses above the caixa root,
    /// the same sandbox-escape vector via parent-directory traversal.
    /// Caught regardless of where the `..` component sits (leading,
    /// mid-path, trailing) so a future relaxation that only checks
    /// one position surfaces at this one predicate.
    ParentEscape,
}

/// Predicate: assert that `path` is a *sandboxed-relative* path —
/// the shape every caixa-author-supplied callback / script path must
/// take so the layout checker's `root.join(p)` resolves inside the
/// caixa root sandbox. The contract:
///
///   - non-empty (`PathBuf::new()` → `Empty`);
///   - relative (absolute paths replace the base under
///     [`Path::join`] semantics → `Absolute`);
///   - no [`Component::ParentDir`] components anywhere (traversal
///     above the caixa root → `ParentEscape`).
///
/// Returns [`PathShapeViolation`] tagging the specific failure;
/// each per-axis caller match-and-wraps the variant in its own
/// typed `*Invalid { slot, path }` enum variant so the diagnostic
/// still names *which slot* carried the malformed value. The
/// arm-ordering is the same `Empty → Absolute → ParentEscape`
/// every prior inlined copy followed (b0c8389 [`crate::BehaviorSpec`],
/// 26da2c7 [`crate::UpgradeInstruction::StateChange`]), so any
/// caller migrating to the lifted predicate preserves its existing
/// per-slot diagnostic precedence by construction.
///
/// Lifted from `caixa-core::behavior` and `caixa-core::upgrade`
/// where the same three-step gate was inlined verbatim across two
/// call sites — the PRIME DIRECTIVE duplication-budget rule
/// (THEORY.md §I.3.5: "every recurring shape becomes a generator
/// before it becomes a pattern; every pattern becomes a library
/// before it becomes duplicated code. The duplication budget is
/// zero.") promotes the gate to a typed substrate-side predicate
/// on the same trajectory the M2-overlay and label-selector helpers
/// (9e3a057, 9d09cfb, 9dbeafd, 31455a7, 07a4544, 8b4db42) already
/// follow. The third caller — the future M3/M4 axis admitting a
/// user-supplied path (the future `:entrada :tls-cert` /
/// `:entrada :tls-key` PEM-file axes, the future
/// `mesh.pleme.io/v1alpha1/Caixa` CR materializer's per-path
/// validator, the future per-Servico pre-warm script axis) — lands
/// as a thin five-line wrapper rather than re-inlining the same
/// three checks.
///
/// Pairs with the per-axis empty / absolute / parent-escape variants
/// on [`crate::BehaviorError`] and [`crate::UpgradeError`] — those
/// remain the typed surface authors see; this predicate is the
/// single-source-of-truth gate the caixa-build pipeline consults to
/// produce them.
///
/// # Errors
///
/// Returns the [`PathShapeViolation`] tag identifying the specific
/// violation ([`PathShapeViolation::Empty`] / [`PathShapeViolation::Absolute`]
/// / [`PathShapeViolation::ParentEscape`]) so each per-axis caller
/// match-and-wraps it into its own typed `*Path` / `*Script` enum
/// variant (preserving the per-slot diagnostic granularity the inline
/// pre-lift gates already produced).
pub fn is_sandboxed_relative_path(path: &Path) -> Result<(), PathShapeViolation> {
    if path.as_os_str().is_empty() {
        return Err(PathShapeViolation::Empty);
    }
    if path.is_absolute() {
        return Err(PathShapeViolation::Absolute);
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(PathShapeViolation::ParentEscape);
    }
    Ok(())
}

/// The canonical tatara-lisp source-file extension every M2 typed
/// path-slot the M2.5 wasm-engine instantiator reads through
/// `tatara_lisp::read` at instance-start time must terminate in.
///
/// Strict lowercase: the byte-size / duration codecs and every other
/// shape-gate predicate in this module are case-sensitive on unit /
/// scheme / label boundaries, so a strict `lisp` shape matches the
/// downstream accepted set without case-folding drift (an uppercase
/// `.LISP` / `.Lisp` shape that a case-insensitive volume's existence
/// check would match the on-disk file would still mismatch the
/// canonical form the codec emits, breaking the THEORY.md §V.2.7
/// render-determinism contract every typed slot carries).
pub const LISP_SOURCE_EXTENSION: &str = "lisp";

/// Predicate: assert that `path` terminates in the canonical
/// [`LISP_SOURCE_EXTENSION`] (lowercase `.lisp`) — the file-type
/// shape every M2 typed path-slot the wasm-engine instantiator reads
/// as tatara-lisp source must take. The contract:
///
///   - the path has an extension component (no-extension paths like
///     `"lib/init"` or `"a"` fail);
///   - the extension's UTF-8 string form is exactly `"lisp"` —
///     lowercase, no trailing residue, no double-extension shadow
///     like `".lisp.bak"`.
///
/// Returns `true` on accept, `false` on reject. Each per-axis caller
/// — [`crate::BehaviorSpec::validate`] on `:behavior :on-*` paths
/// (c97815a), [`crate::UpgradeInstruction::StateChange::validate`]
/// on `:upgrade-from :state-change :script` (this commit), every
/// future axis admitting a tatara-lisp source path — wraps the
/// boolean into its own typed `*NonLispExtension { slot, path }` /
/// `*NonLispExtensionScript { script }` enum variant so the
/// diagnostic still names *which slot* carried the non-`.lisp`
/// value. The predicate is axis-agnostic; the wrapping per-axis
/// variant carries the slot identity.
///
/// Lifted from `caixa-core::behavior` where the same single-line
/// gate (`path.extension().and_then(|ext| ext.to_str()) ==
/// Some("lisp")`) was inlined verbatim across the first call site
/// (`BehaviorSpec::validate_callback_path`) — the PRIME DIRECTIVE
/// duplication-budget rule (THEORY.md §I.3.5: "every recurring shape
/// becomes a generator before it becomes a pattern; every pattern
/// becomes a library before it becomes duplicated code. The
/// duplication budget is zero.") promotes the gate to a typed
/// substrate-side predicate on the same trajectory the path-shape
/// gate [`is_sandboxed_relative_path`] already follows (lifted from
/// the same two call sites once the second consumer appeared). The
/// third caller — the future `:bibliotecas` per-entry tatara-lisp
/// source-file axis (the `feira build` loop reads each through the
/// same `tatara_lisp::read` reader at parse time), the future `:exe`
/// `:kind Binario` entry-point axis (the nix-built binary's entry
/// point loads as Lisp source), the future M2.5 wasm-engine
/// pre-warm hook axis — lands as a thin two-line wrapper rather
/// than re-inlining the same extension check.
///
/// Pairs with the per-axis `*NonLispExtension` / `*NonLispExtensionScript`
/// variants on [`crate::BehaviorError`] and [`crate::UpgradeError`]
/// — those remain the typed surface authors see; this predicate is
/// the single-source-of-truth gate the caixa-build pipeline consults
/// to produce them.
#[must_use]
pub fn is_lisp_extension(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some(LISP_SOURCE_EXTENSION)
}

/// The canonical compound suffix every `:servicos` entry — the
/// ComputeUnit-CR axis the M2 typed-substrate caixa-helm /
/// caixa-flux renderers consume via `serde_yaml::from_str` — must
/// terminate in. Two-segment shape (`.computeunit.yaml`) rather than
/// a single `.yaml` extension: the `.computeunit` segment routes
/// authoring-time to the typed `ComputeUnit` CR shape the
/// `pleme-computeunit` library chart resolves, distinguishing the
/// slot's accepted set from the open `.yaml` universe (Helm
/// `values.yaml`, FluxCD `Kustomization.yaml`, the generic K8s
/// manifest YAML every operator emits) — same axis-discipline the
/// peer [`LISP_SOURCE_EXTENSION`] sibling carries on the tatara-lisp-
/// source axis but with a compound suffix because
/// [`Path::extension`] only returns the post-last-`.` segment
/// (`"yaml"` for `foo.computeunit.yaml`), so the predicate routes
/// through [`Path::file_name`] and a string `ends_with` check on the
/// full suffix instead.
///
/// Strict lowercase: every other shape-gate predicate in this module
/// is case-sensitive on unit / scheme / label boundaries, so a strict
/// `.computeunit.yaml` shape matches the downstream accepted set
/// without case-folding drift (an uppercase `.COMPUTEUNIT.YAML` shape
/// that a case-insensitive volume's existence check would match the
/// on-disk file would still mismatch the canonical form every in-tree
/// `:servicos` fixture and the `Caixa::template` scaffold emit,
/// breaking the THEORY.md §V.2.7 render-determinism contract every
/// typed slot carries).
pub const COMPUTEUNIT_YAML_SUFFIX: &str = ".computeunit.yaml";

/// Predicate: assert that `path` terminates in the canonical
/// [`COMPUTEUNIT_YAML_SUFFIX`] (lowercase `.computeunit.yaml`) — the
/// file-type shape every `:servicos` entry, the ComputeUnit-CR axis
/// the M2 typed-substrate caixa-helm / caixa-flux renderers consume
/// via `serde_yaml::from_str`, must take. The contract:
///
///   - the path has a final file-name component (paths ending in `/`
///     fail);
///   - the file name's UTF-8 string form ends in
///     `.computeunit.yaml` — lowercase, no case-folding;
///   - at least one byte precedes the suffix (the degenerate hidden-
///     file `.computeunit.yaml` shape — file name exactly equal to
///     the suffix — fails: the substrate identifies each ComputeUnit
///     by the file-stem segment that precedes `.computeunit.yaml`,
///     so an empty stem is structurally an unidentified Servico).
///
/// Returns `true` on accept, `false` on reject. The per-axis caller
/// — [`crate::Caixa::validate_code_paths`] on the `:servicos` axis —
/// wraps the boolean into its own typed
/// `ManifestError::CodePathNonComputeUnitYamlExtension { slot, path }`
/// variant so the diagnostic still names the offending slot and the
/// offending path verbatim. Peer of [`is_lisp_extension`] on the
/// tatara-lisp-source axis (`:bibliotecas` 64772a9); same axis-
/// agnostic predicate discipline, here on the compound-suffix axis
/// [`Path::extension`] can't express on its own. The third caller —
/// the future M2.5 caixa-operator `:servicos` admission webhook
/// keying off the same accepted set, the M4
/// `mesh.pleme.io/v1alpha1/ComputeUnit` CR materializer's per-
/// `:servicos` shape gate, the future `feira fmt`'s `:servicos`
/// canonical-form normalizer — lands as a thin wrapper rather than
/// re-inlining the same compound-suffix check.
///
/// Pairs with the per-axis
/// [`crate::ManifestError::CodePathNonComputeUnitYamlExtension`]
/// variant — that remains the typed surface authors see; this
/// predicate is the single-source-of-truth gate the caixa-build
/// pipeline consults to produce it.
#[must_use]
pub fn is_computeunit_yaml_extension(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| {
            name.len() > COMPUTEUNIT_YAML_SUFFIX.len() && name.ends_with(COMPUTEUNIT_YAML_SUFFIX)
        })
}

/// Canonical camelCase YAML key for the `:limits` slot's overlay.
pub const M2_KEY_LIMITS: &str = "limits";
/// Canonical camelCase YAML key for the `:behavior` slot's overlay.
pub const M2_KEY_BEHAVIOR: &str = "behavior";
/// Canonical camelCase YAML key for the `:upgrade-from` slot's overlay.
pub const M2_KEY_UPGRADE_FROM: &str = "upgradeFrom";

/// Canonical YAML key for the M3 `:placement` slot's overlay on a
/// rendered programs.yaml entry. The lareira-fleet-programs aggregator
/// (and the future `app-operator` per-Aplicacao reconciler) both key
/// off this exact spelling to filter entries by `placement.clusters`
/// for cross-cluster fanout (MESH-COMPOSITION §III.4) and to dispatch
/// on `placement.estrategia` for distributed-app takeover semantics
/// (§II.1, §V cross-cluster federation). Lifted as a const alongside
/// the M2 keys so the Aplicacao-side renderer
/// ([`crate::aplicacao::Placement`] → caixa-mesh
/// `programs_for_aplicacao`) and every consumer (the M4 cluster-fanout
/// renderer, the future `mesh.pleme.io/v1alpha1/Aplicacao` CR
/// materializer, the `app-operator`'s placement-strategy dispatcher)
/// spell the same key exactly the same way — drift here = a
/// programs.yaml entry whose placement is silently dropped at the
/// aggregator's filter step (visible only as "the workload doesn't
/// land where the typed slot said it should").
pub const M3_KEY_PLACEMENT: &str = "placement";

/// Canonical pleme-io label namespace prefix. Every cluster object
/// emitted by any caixa-side renderer that needs to carry the
/// pleme-io workload identity uses this prefix; runtime label
/// injectors (`lareira-fleet-programs` chart's pod template,
/// `pleme-computeunit` library chart's identity sidecar, the
/// caixa-operator's pod-mutating webhook) and runtime label
/// consumers (Cilium identity-based policy, Hubble flow attribution,
/// `caixa-mesh`'s policy / Gateway emission, future
/// observability/tracing renderers) all spell the same prefix
/// exactly the same way — drift between *any* of those = a
/// CiliumNetworkPolicy that matches no pods, a Hubble flow that
/// can't be correlated to its workload, an OpenTelemetry resource
/// attribute that doesn't join to its caixa lacre.
///
/// Lifted to a const so a future top-level rebrand or multi-tenant
/// label-namespace migration is a one-line edit, not a search-and-
/// replace across every renderer crate.
pub const PLEME_LABEL_PREFIX: &str = "pleme.pleme.io";

/// Canonical pleme-io label key naming the **Aplicacao** the workload
/// belongs to. Together with [`LABEL_PROGRAM`] this is the load-bearing
/// identity tuple every per-Aplicacao mesh renderer (Cilium, Gateway,
/// future caixa-otel) keys off — `(LABEL_APLICACAO, LABEL_PROGRAM)` =
/// the unique workload selector inside one cluster.
pub const LABEL_APLICACAO: &str = "pleme.pleme.io/aplicacao";

/// Canonical pleme-io label key naming the **program** (i.e. the
/// caixa Servico's `:nome`) a pod runs. `LABEL_APLICACAO` +
/// `LABEL_PROGRAM` together pick exactly one workload identity in one
/// cluster. Used as the `matchLabels` axis on every Cilium
/// `endpointSelector` / `fromEndpoints` rule and on Gateway API
/// `backendRefs` selectors emitted by [`crate`]'s downstream
/// renderers.
pub const LABEL_PROGRAM: &str = "pleme.pleme.io/program";

/// Canonical pleme-io label key naming the **contrato** (the M3
/// `:contratos` edge: `<de>-to-<para>`) a CiliumNetworkPolicy enforces.
/// Carried on the policy's *own* labels (not on workload pods) so
/// Hubble + cluster operators can group flows by typed contrato edge,
/// not just by source/destination pod identity.
pub const LABEL_CONTRATO: &str = "pleme.pleme.io/contrato";

/// Canonical K8s API key naming the resource's API-version selector
/// (e.g. `cilium.io/v2`, `gateway.networking.k8s.io/v1`,
/// `wasm.pleme.io/v1alpha1`). Lifted to a const so a future API-server
/// rename or a multi-version-skew migration is a one-line edit, not a
/// search-and-replace across every per-target renderer.
pub const KUBE_KEY_API_VERSION: &str = "apiVersion";
/// Canonical K8s API key naming the resource's kind discriminator
/// (e.g. `CiliumNetworkPolicy`, `Gateway`, `HTTPRoute`, `ComputeUnit`).
pub const KUBE_KEY_KIND: &str = "kind";
/// Canonical K8s API key naming the resource's metadata block.
pub const KUBE_KEY_METADATA: &str = "metadata";
/// Canonical K8s API key naming the resource's name (under metadata).
pub const KUBE_KEY_NAME: &str = "name";
/// Canonical K8s API key naming the resource's namespace (under metadata).
pub const KUBE_KEY_NAMESPACE: &str = "namespace";
/// Canonical K8s API key naming the resource's labels (under metadata).
pub const KUBE_KEY_LABELS: &str = "labels";
/// Canonical K8s API key naming the `matchLabels` axis of a
/// [`LabelSelector`][k8s-ls] — the equality-based projection of the
/// selector schema (the other axis, `matchExpressions`, is set-based
/// and intentionally out-of-scope for the V0 [`label_selector`]
/// helper). Spelled exactly as the K8s apiserver expects (camelCase
/// `matchLabels`, not `match_labels` / `MatchLabels` / `match-labels`)
/// so the rendered YAML round-trips through every K8s schema parser
/// (Cilium CRDs, Gateway API, `ComputeUnit`, future
/// `mesh.pleme.io/v1alpha1/Aplicacao`) without per-renderer string
/// drift.
///
/// [k8s-ls]: https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.31/#labelselector-v1-meta
pub const KUBE_KEY_MATCH_LABELS: &str = "matchLabels";

/// Default cluster-wide K8s namespace every caixa renderer emits
/// objects into when the source caixa doesn't pin its own. The single
/// source of truth both [`caixa-flux`][cf]'s programs.yaml /
/// GitRepository / HelmRelease / Kustomization emitters and
/// [`caixa-mesh`][cm]'s programs fan-out / CiliumNetworkPolicy /
/// Gateway / HTTPRoute emitters consult — re-exported by each
/// renderer's lib as `pub use caixa_core::DEFAULT_NAMESPACE`, so a
/// future per-cluster-namespace rebrand (e.g. moving to `pleme-system`
/// once `tatara-system` outlives its scoping intent) is a one-line
/// edit here, not a coordinated rewrite across every renderer
/// crate's `metadata.namespace` slot.
///
/// Until this lift landed both renderers carried their own `pub const
/// DEFAULT_NAMESPACE: &str = "tatara-system"` declarations
/// (caixa-flux/src/lib.rs:77, caixa-mesh/src/lib.rs:172), with the
/// `caixa-mesh` site's doc-comment explicitly acknowledging the
/// duplication ("Mirrors `caixa_flux::DEFAULT_NAMESPACE`"); a future
/// rebrand on either side without a coordinated edit on the other
/// would have silently emitted into two distinct namespaces on the
/// same cluster's apply — Servicos at programs.yaml's namespace,
/// their Aplicacao's NetworkPolicies / Gateways / HTTPRoutes at a
/// drifted one — and the CiliumNetworkPolicy's `endpointSelector`
/// would match no pods (different namespace), silently dropping every
/// L7 contrato flow at apply time with no diagnostic naming the
/// namespace-drift root cause.
///
/// Lifting it to caixa-core's render-constants block alongside the
/// peer [`LABEL_APLICACAO`] / [`LABEL_PROGRAM`] / [`LABEL_CONTRATO`]
/// label-namespace constants and the canonical [`KUBE_KEY_NAMESPACE`]
/// API-key constant makes the namespace-axis discipline structural:
/// every renderer that reaches for the default namespace consults the
/// same `&'static str`, and every future renderer (the M4
/// `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer, the future
/// per-edge `CiliumClusterwideEnvoyConfig` emitter, the future
/// caixa-otel collector-pipeline emitter) inherits the same value by
/// construction, with no opportunity for per-renderer drift. Same
/// "the typed constant lives in one place" discipline the
/// [`PLEME_LABEL_PREFIX`] (a8d4d57) and [`KUBE_KEY_API_VERSION`] /
/// [`KUBE_KEY_KIND`] / [`KUBE_KEY_METADATA`] lifts apply on the peer
/// shared-string axes.
///
/// [cf]: ../../caixa_flux/index.html
/// [cm]: ../../caixa_mesh/index.html
pub const DEFAULT_NAMESPACE: &str = "tatara-system";

/// Canonical Helm library-chart name every `lareira-<nome>` chart depends
/// on — the `pleme-computeunit` library chart in
/// `pleme-io/helmworks/charts/pleme-computeunit` that owns the K8s
/// resource templates (ComputeUnit + Service + ScaledObject + ConfigMap)
/// every per-Servico chart consumes via Helm's per-dep alias convention
/// (when no `alias:` is set on a dependency, values are scoped under the
/// dependency's `name:`).
///
/// The single source of truth all three downstream library-name consumers
/// reach for:
///
///   - [`caixa-helm`][ch]'s `DEFAULT_LIBRARY_NAME` re-export — the
///     default value of `RenderOpts::library_name`, which drives both
///     the Chart.yaml `dependencies[0].name` axis
///     (`build_chart_yaml`) and the values.yaml wrap key
///     (`build_values_yaml`) so the rendered `lareira-<nome>` chart's
///     dep declaration and its values block agree by construction
///     (the 17ebd1a `opts.library_name` lift).
///   - [`caixa-flux`][cf]'s `DEFAULT_LIBRARY_NAME` re-export — the
///     wrap key the `cluster_bundle` `helmrelease.yaml` template uses
///     under `spec.values.<library>:` to thread the per-cluster
///     overrides (`enabled: true`) through to the rendered chart's
///     dep block. Helm's per-dep alias convention scopes those values
///     under the dependency's `name:`, so this wrap key must match the
///     chart's `dependencies[0].name` exactly — drift here silently
///     routes the values block nowhere at `helm template` /
///     `helm install` time, and the cluster comes up with the library
///     chart's defaults rather than the typed per-cluster overrides.
///   - Every future per-Servico renderer the absorption-roadmap
///     acknowledges (the M4 `mesh.pleme.io/v1alpha1/Aplicacao` CR
///     materializer's per-edge library-chart resolver, the future
///     per-cluster image-registry mirror's `<registry>-computeunit`
///     fork, the future per-edition library-chart variant the
///     substrate forks once `pleme-computeunit` outlives its scoping
///     intent).
///
/// Until this lift landed the canonical library-chart name lived as
/// two production-code call sites: a `pub const DEFAULT_LIBRARY_NAME:
/// &str = "pleme-computeunit"` in `caixa-helm` (the
/// `RenderOpts::library_name` default, consumed by both the chart's dep
/// name axis and the values.yaml wrap key axis) and an inline literal
/// `pleme-computeunit:` in `caixa-flux`'s `cluster_bundle`
/// `helmrelease.yaml` format-string template (the wrap key the per-
/// cluster `enabled: true` override is scoped under). Both consumers
/// reach for the same load-bearing Helm library-chart name, but no
/// shared constant linked them — the canonical
/// "duplicated `pub const` / inline literal across two renderers"
/// drift footgun the [`DEFAULT_NAMESPACE`] (a085b26) and
/// [`DEFAULT_SERVICO_PORT`] (1e22add) lifts close on the peer
/// canonical-K8s-axis-constant surface.
///
/// A future library-chart rebrand — the substrate forking
/// `pleme-computeunit` to `<registry>-computeunit` for a per-cluster
/// image-registry mirror, or to `aplicacao-computeunit` for the M4
/// typed-Aplicacao renderer's sibling library chart, or to any
/// per-edition variant the absorption-roadmap names — without a
/// coordinated edit on both consumers would have silently emitted a
/// per-Servico chart whose dep declared the new library name (because
/// the chart-side override flowed through `opts.library_name`) but
/// whose flux-side `HelmRelease.values.pleme-computeunit:` wrap key
/// still scoped under the old literal. Helm's per-dep values router
/// would route the per-cluster `enabled: true` override to *nowhere*
/// at `helm template` / `helm install` time, and the cluster's apply
/// would come up with the library chart's defaults — `enabled: false`,
/// the typed values block from the chart's own `values.yaml` rather
/// than the flux-side override — silently no-op'ing every per-cluster
/// override the operator set, far from the rebrand commit's source.
/// The apply-time symptom (the workload comes up with the library
/// chart's defaults instead of the per-cluster overrides) is invisible
/// at admission and surfaces only as "the service is up but not doing
/// what we configured it to do", typically far from the rebrand commit.
///
/// Lifting it to caixa-core's render-constants block alongside the
/// peer [`DEFAULT_NAMESPACE`] / [`DEFAULT_SERVICO_PORT`] makes the
/// library-name axis discipline structural: every renderer that
/// reaches for the canonical library-chart name consults the same
/// `&'static str`, and every future renderer inherits the same value
/// by construction with no opportunity for per-renderer drift. Same
/// "the typed constant lives in one place" discipline the
/// [`PLEME_LABEL_PREFIX`] (a8d4d57) / [`KUBE_KEY_API_VERSION`] /
/// [`LAREIRA_CHART_NAME_PREFIX`] lifts apply on the peer
/// shared-string axes.
///
/// [ch]: ../../caixa_helm/index.html
/// [cf]: ../../caixa_flux/index.html
pub const DEFAULT_LIBRARY_NAME: &str = "pleme-computeunit";

/// Canonical Helm chart-name prefix for every per-Servico chart the
/// substrate emits — the `"lareira-"` segment of the well-known
/// `lareira-<nome>` shape every caixa Servico renderer prepends to a
/// caixa's `:nome` to derive its [`Chart.yaml` `name:`][chart-yaml] field,
/// its OCI artifact reference (`oci://<registry>/lareira-<nome>`), and
/// the resulting cluster-side `HelmRelease` `release_name`. The single
/// source of truth all three downstream Servico renderers consult —
/// [`caixa-helm`][cf]'s `render_chart_for_servico` chart-dir name
/// (caixa-helm/src/lib.rs:207), [`caixa-flux`][cm]'s `cluster_bundle`
/// `HelmRelease` `chart:` field (caixa-flux/src/lib.rs:329), and
/// [`caixa-tatara`][ct]'s `process_for_aplicacao` `release_name` +
/// `derive_chart_ref` OCI ref (caixa-tatara/src/lib.rs:124,182) — so a
/// future per-chart-name-prefix rebrand (e.g. moving to `forno-` once
/// `lareira-` outlives its scoping intent, or any segment-namespace
/// migration the chart-publishing pipeline requires) is a one-line edit
/// here, not a coordinated rewrite across every renderer crate's chart-
/// name-derivation site.
///
/// Until this lift landed all three renderers carried inline
/// `format!("lareira-{}", caixa.nome)` / `format!("lareira-{name}")` /
/// `format!("oci://{}/lareira-{}", registry, caixa.nome.as_str())`
/// expressions — three verbatim copies of the same substrate-wide
/// naming convention. The PRIME DIRECTIVE duplication budget of zero
/// (THEORY.md §I.3.5) lands the lift here at the third occurrence: a
/// future rebrand on any one site without a coordinated edit on the
/// others would have silently published a chart at one name, registered
/// its OCI ref at a second, and resolved the `HelmRelease` at a third —
/// the cluster's apply would surface as a `chart pull failed: image not
/// found` error far from the source rebrand commit, with no field
/// naming the prefix-drift root cause.
///
/// Lifting it to caixa-core's render-constants block alongside the peer
/// [`DEFAULT_NAMESPACE`] (a085b26) makes the chart-name-prefix axis
/// discipline structural: every renderer that derives a per-Servico
/// chart name consults [`lareira_chart_name`], and every future renderer
/// (the future per-cluster snapshot bundle emitter, the future M4
/// `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's chart-ref slot,
/// the future caixa-otel collector chart name) inherits the same prefix
/// by construction, with no opportunity for per-renderer drift. Same
/// "the typed constant lives in one place" discipline the
/// [`PLEME_LABEL_PREFIX`] / [`DEFAULT_NAMESPACE`] / [`KUBE_KEY_API_VERSION`]
/// lifts apply on the peer shared-string axes.
///
/// [chart-yaml]: https://helm.sh/docs/topics/charts/#the-chartyaml-file
/// [cf]: ../../caixa_helm/index.html
/// [cm]: ../../caixa_flux/index.html
/// [ct]: ../../caixa_tatara/index.html
pub const LAREIRA_CHART_NAME_PREFIX: &str = "lareira-";

/// Derive the canonical per-Servico Helm chart name from a caixa's
/// `:nome` — the substrate-wide `lareira-<nome>` shape every
/// per-Servico renderer ([`caixa-helm`][cf]'s `render_chart_for_servico`
/// chart-dir name, [`caixa-flux`][cm]'s `cluster_bundle` `HelmRelease`
/// `chart:` field, [`caixa-tatara`][ct]'s `process_for_aplicacao`
/// `release_name`, and the `oci://<registry>/lareira-<nome>` OCI ref)
/// composes by prepending [`LAREIRA_CHART_NAME_PREFIX`].
///
/// Single source of truth for the prefix-application: every consumer
/// reaches for this helper rather than re-deriving the `format!(…)`
/// shape inline, so a future change to the prefix axis (the lift's
/// raison d'être) is one edit here, not a coordinated sweep across
/// every renderer.
///
/// The input `nome` is the caixa's typed `:nome` field, already
/// DNS-1123-label-validated at [`Caixa::validate_nome`] (6c992f8) —
/// every value reaching this helper is structurally a valid Helm
/// chart-name segment. The prepended prefix is a fixed lowercase ASCII
/// alphanumeric + hyphen string, so the concatenation is structurally a
/// valid Helm chart name by construction (Helm's chart-name accepted
/// set is the DNS-1123 label rule, and DNS-1123 labels concatenate with
/// the prefix-and-hyphen separator into valid DNS-1123 labels as long
/// as the joint length stays ≤ 63 bytes; the M4 admission webhook will
/// pin the joint-length invariant when it lands).
///
/// [cf]: ../../caixa_helm/index.html
/// [cm]: ../../caixa_flux/index.html
/// [ct]: ../../caixa_tatara/index.html
#[must_use]
pub fn lareira_chart_name(nome: &str) -> String {
    format!("{LAREIRA_CHART_NAME_PREFIX}{nome}")
}

/// The `:nome`-side budget the [`lareira_chart_name`] composition
/// imposes on every caixa `:nome` reaching a renderer that derives a
/// `lareira-<nome>` artifact (`caixa-helm`'s `ChartDir.name` +
/// `Chart.yaml` `name:`, `caixa-flux`'s `cluster_bundle` `HelmRelease`
/// `chart:` slot, `caixa-tatara`'s `process_for_aplicacao`
/// `release_name` + `oci://<registry>/lareira-<nome>` chart ref).
///
/// The joint length of `lareira-` + `<nome>` must satisfy the K8s
/// DNS-1123 label cap ([`DNS_1123_LABEL_MAX_LEN`] = 63) every downstream
/// consumer enforces — Helm's `Chart.yaml::name` field (`helm lint`
/// rejects at chart-package time per the DNS-1123 rule), the
/// `HelmRelease`'s `release_name` field (the Helm operator's tracking
/// secret name is derived from `release_name` and is itself a DNS-1123
/// label), the rendered chart's K8s object `metadata.name` axes that
/// embed the chart name as a prefix. The arithmetic is therefore
/// `DNS_1123_LABEL_MAX_LEN - LAREIRA_CHART_NAME_PREFIX.len()` = 63 - 8
/// = 55 bytes the caixa's `:nome` may itself occupy.
///
/// Lifted to a `pub const` so a future change to either axis
/// ([`LAREIRA_CHART_NAME_PREFIX`] rebrand, [`DNS_1123_LABEL_MAX_LEN`]
/// shift if Helm/K8s ever relax the chart-name rule) re-derives the
/// budget mechanically — every per-axis call site
/// ([`is_lareira_chart_name_shape`] consults it, the
/// `Caixa::validate_nome_chart_name_budget` diagnostic names it
/// verbatim) inherits the new value with no coordinated edit.
pub const LAREIRA_CHART_NAME_NOME_MAX_LEN: usize =
    DNS_1123_LABEL_MAX_LEN - LAREIRA_CHART_NAME_PREFIX.len();

/// Predicate: assert that `nome` produces a [`lareira_chart_name`]
/// output satisfying the K8s DNS-1123 label rule — the joint-length
/// invariant the canonical `lareira_chart_name` helper's doc comment
/// (f7320d7) defers to "the M4 admission webhook will pin … when it
/// lands". This predicate lands it at the manifest-validate layer
/// rather than waiting for the apiserver.
///
/// Returns the parser-shaped reason on rejection (without wrapping in
/// any error variant) — same call-site discipline as the peer
/// [`is_dns_1123_label`] predicate. Each per-axis caller wraps the
/// returned reason in its own typed `*Error::*Exceeded { … }` variant
/// (today: `Caixa::validate_nome_chart_name_budget` → the new
/// [`crate::ManifestError::NomeChartNameBudgetExceeded`] arm).
///
/// The predicate composes via [`lareira_chart_name`] + [`is_dns_1123_label`]
/// — the same two primitives every renderer consults — so a future
/// rebrand of either axis (`LAREIRA_CHART_NAME_PREFIX`,
/// `DNS_1123_LABEL_MAX_LEN`) re-derives the budget mechanically. A
/// `:nome` that already passes [`is_dns_1123_label`] (≤63 bytes,
/// boundary-anchored, `[a-z0-9-]` only) but whose prefixed chart name
/// exceeds the joint cap is what this gate catches — every byte the
/// inner DNS-1123 check accepts the prefixed form may still reject.
///
/// # Errors
///
/// Returns a parser-shaped reason naming the budget
/// ([`LAREIRA_CHART_NAME_NOME_MAX_LEN`]), the offending `:nome`
/// length, and the rendered chart name's length — so the diagnostic is
/// self-locating and the author can shorten in one edit.
pub fn is_lareira_chart_name_shape(nome: &str) -> Result<(), String> {
    let chart_name = lareira_chart_name(nome);
    if chart_name.len() > DNS_1123_LABEL_MAX_LEN {
        return Err(format!(
            "produces `{chart_name}` ({chart_len} bytes), which exceeds the \
             DNS-1123 label max length of {DNS_1123_LABEL_MAX_LEN} bytes that \
             Helm's `Chart.yaml::name` field and every downstream K8s artifact \
             derived from the chart name enforce; the per-`:nome` budget is \
             {budget} bytes (DNS-1123 cap minus the `{prefix}` prefix), shorten \
             `:nome` to ≤ {budget} bytes",
            chart_name = chart_name,
            chart_len = chart_name.len(),
            budget = LAREIRA_CHART_NAME_NOME_MAX_LEN,
            prefix = LAREIRA_CHART_NAME_PREFIX,
        ));
    }
    Ok(())
}

/// Build the canonical Cilium `matchLabels` selector for a single
/// pleme-io program **scoped to its Aplicacao** — the safe default
/// every per-Aplicacao mesh renderer (caixa-mesh's
/// `cilium_network_policies` `fromEndpoints`, future per-edge policy
/// emission, Gateway API `backendRefs` filters) should use, since
/// two different Aplicacaos can carry programs with the same `:nome`
/// in the same cluster (e.g. two `cart` Servicos under different
/// applications) and a `LABEL_PROGRAM`-only selector would match
/// pods belonging to the wrong Aplicacao.
///
/// Returned as a [`BTreeMap`] keyed by `&'static str` so iteration is
/// alphabetical (THEORY.md §V.2.7 render determinism: the rendered
/// YAML's `matchLabels:` block appears in a deterministic order
/// independent of source-code declaration order). The two keys
/// alphabetize as [`LABEL_APLICACAO`] before [`LABEL_PROGRAM`], the
/// same order the renderer's `serde_yaml::Mapping` iteration will
/// preserve through to the rendered YAML.
#[must_use]
pub fn pleme_program_in_aplicacao_selector(
    program: &str,
    aplicacao: &str,
) -> BTreeMap<&'static str, String> {
    let mut out = BTreeMap::new();
    out.insert(LABEL_APLICACAO, aplicacao.to_string());
    out.insert(LABEL_PROGRAM, program.to_string());
    out
}

/// Build the canonical Cilium `matchLabels` selector for a single
/// pleme-io program **without** the Aplicacao constraint —
/// deliberately broader than [`pleme_program_in_aplicacao_selector`]
/// for the cases where matching a program across every Aplicacao that
/// hosts it is the *intent* (cluster-wide rate limits, breakglass
/// observability, the per-cluster operator identity scope).
///
/// **Prefer [`pleme_program_in_aplicacao_selector`]** for typed
/// per-Aplicacao mesh emission — using `pleme_program_selector` there
/// would let a policy unintentionally match a same-named program in
/// a different Aplicacao. Both helpers exist so the caller's *intent*
/// (Aplicacao-scoped vs. cluster-wide) is named at the call site,
/// not buried in inline label-key string literals.
#[must_use]
pub fn pleme_program_selector(program: &str) -> BTreeMap<&'static str, String> {
    let mut out = BTreeMap::new();
    out.insert(LABEL_PROGRAM, program.to_string());
    out
}

/// Convert a typed string-valued mapping (e.g. one of the canonical
/// [`pleme_program_selector`] / [`pleme_program_in_aplicacao_selector`]
/// selectors, or any caller-built `BTreeMap<&'static str, String>`)
/// into a [`serde_yaml::Value::Mapping`] with `String → String` shape —
/// the surface every Cilium / Gateway / HTTPRoute / ComputeUnit
/// `matchLabels` / `metadata.labels` / `selector` field expects.
///
/// Iteration order is whatever the input iterator yields; pass a
/// [`BTreeMap`] for alphabetical determinism (THEORY.md §V.2.7 render
/// determinism: rendered YAML key order is independent of source-code
/// declaration order). The two pleme-io selector helpers above already
/// return `BTreeMap`s for exactly this reason.
///
/// Lifted from `caixa-mesh`'s prior `yaml_string_mapping` private
/// helper to make the same primitive available to every other
/// `caixa-<target>` renderer that needs to emit a string→string YAML
/// mapping (the future per-Aplicacao Gateway-API filter rules, the
/// caixa-otel resource-attribute emitter, the `app-operator`'s typed
/// CR materializer, the per-cluster CiliumClusterwideEnvoyConfig
/// renderer for `:politicas` defaults). Without the lift each new
/// renderer would re-inline the same five-line `for (k, v)` body and
/// inherit the same drift footguns.
#[must_use]
pub fn yaml_string_mapping<K, V, M>(m: M) -> serde_yaml::Value
where
    M: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    let mut out = serde_yaml::Mapping::new();
    for (k, v) in m {
        out.insert(
            serde_yaml::Value::String(k.into()),
            serde_yaml::Value::String(v.into()),
        );
    }
    serde_yaml::Value::Mapping(out)
}

/// Wrap a typed string-valued label mapping in the canonical K8s
/// [`LabelSelector`][k8s-ls] shape — `{matchLabels: <string-string-map>}`
/// — and return it as a [`serde_yaml::Value::Mapping`] ready to drop
/// directly under any K8s field that takes a label selector
/// (Cilium `endpointSelector` / `fromEndpoints[].matchLabels`, Gateway
/// API `BackendRef` filters, ComputeUnit `selector`, Service
/// `spec.selector`, the future `mesh.pleme.io/v1alpha1/Aplicacao` CR
/// `spec.selector`).
///
/// Lifted from two inline `serde_yaml::Mapping::new() +
/// insert(Value::String("matchLabels".into()), yaml_string_mapping(_))`
/// blocks in `caixa-mesh::cilium_network_policies` (the destination
/// `endpointSelector` and the source `fromEndpoints[0]` selector) so
/// the next renderer to land — the per-`:politicas`
/// `CiliumClusterwideEnvoyConfig` emitter (MESH-COMPOSITION §III.2 #3),
/// the `app-operator`'s typed `mesh.pleme.io/v1alpha1/Aplicacao` CR
/// materializer (§III.2 #5), the M4 cross-cluster fan-out's per-cluster
/// `Service`/`HTTPRoute backendRefs` selectors, the future `caixa-otel`
/// OpenTelemetry-Collector resource-selector pipeline — gets the
/// canonical K8s label-selector shape for free with one function call,
/// instead of re-inlining the same four-line `Mapping::new() +
/// insert("matchLabels", yaml_string_mapping(_))` boilerplate.
///
/// V0 emits the equality-based selector axis only (`matchLabels`); the
/// set-based axis ([`matchExpressions`][k8s-ls]) is deliberately out
/// of scope. A future `:contratos` axis whose selector needs
/// `matchExpressions` (e.g. `In`, `NotIn`, `Exists`, `DoesNotExist`
/// operators against a label key) is a future struct-shaped extension
/// of this helper —
/// e.g. a richer [`LabelSelector`] view type with `match_labels` +
/// `match_expressions` fields — not a per-renderer rewrite of
/// every selector emission site.
///
/// Iteration order is whatever the input iterator yields; pass a
/// [`BTreeMap`] for alphabetical determinism (THEORY.md §V.2.7 render
/// determinism: rendered YAML key order is independent of source-code
/// declaration order). The two pleme-io selector helpers
/// ([`pleme_program_selector`] / [`pleme_program_in_aplicacao_selector`])
/// already return `BTreeMap`s for exactly this reason, so a
/// `label_selector(pleme_program_in_aplicacao_selector(_, _))` call
/// renders deterministically end-to-end.
///
/// [k8s-ls]: https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.31/#labelselector-v1-meta
#[must_use]
pub fn label_selector<K, V, M>(labels: M) -> serde_yaml::Value
where
    M: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    let mut out = serde_yaml::Mapping::new();
    out.insert(
        serde_yaml::Value::String(KUBE_KEY_MATCH_LABELS.to_string()),
        yaml_string_mapping(labels),
    );
    serde_yaml::Value::Mapping(out)
}

/// Build the canonical K8s-resource skeleton — the
/// `apiVersion` + `kind` + `metadata.{name, namespace, labels?}`
/// block every cluster artifact emitted by every caixa-side renderer
/// carries — and return it as a fresh [`serde_yaml::Mapping`] the
/// caller adds its `spec:` (and any other top-level keys) to.
///
/// `labels` is inserted under `metadata.labels` only when non-empty.
/// An empty `labels` map leaves the labels key absent — the K8s API
/// server's interpretation of "no labels declared" is "labels key
/// missing", not `labels: {}` (which serializes differently in some
/// YAML libraries and is a sharp tool for label-based selectors that
/// match the empty set silently).
///
/// Iteration order under `metadata` is alphabetical (the inner
/// projection is a [`BTreeMap`] keyed by `&'static str`), so the
/// rendered YAML's `metadata:` block appears in
/// `labels?, name, namespace` order regardless of source-code
/// declaration order. Same render-determinism contract the M2 overlay
/// helper and the pleme-io selector helpers enshrine.
///
/// Lifted from three inline `serde_yaml::Mapping::new()` blocks in
/// `caixa-mesh` ([`cilium_network_policies`][cnp] CNP construction,
/// [`gateway_routes`][gw] Gateway construction, the same fn's
/// HTTPRoute construction) so the next renderer to land — the
/// per-`:politicas` `CiliumClusterwideEnvoyConfig` emitter, the
/// `app-operator`'s typed `mesh.pleme.io/v1alpha1/Aplicacao` CR
/// materializer, the M4 cross-cluster fan-out's per-cluster Kustomization
/// and HelmRelease emission, the future `caixa-otel`
/// OpenTelemetry-Collector pipeline emitter — gets the canonical
/// skeleton for free with one function call, instead of re-inlining
/// the same five-key insert() boilerplate.
///
/// [cnp]: https://docs.cilium.io/en/stable/security/policy/index.html
/// [gw]: https://gateway-api.sigs.k8s.io/
#[must_use]
pub fn kube_resource_skeleton(
    api_version: &str,
    kind: &str,
    name: &str,
    namespace: &str,
    labels: BTreeMap<&'static str, String>,
) -> serde_yaml::Mapping {
    let mut metadata: BTreeMap<&'static str, serde_yaml::Value> = BTreeMap::new();
    metadata.insert(KUBE_KEY_NAME, serde_yaml::Value::String(name.to_string()));
    metadata.insert(
        KUBE_KEY_NAMESPACE,
        serde_yaml::Value::String(namespace.to_string()),
    );
    if !labels.is_empty() {
        metadata.insert(KUBE_KEY_LABELS, yaml_string_mapping(labels));
    }

    let mut metadata_map = serde_yaml::Mapping::new();
    for (k, v) in metadata {
        metadata_map.insert(serde_yaml::Value::String(k.to_string()), v);
    }

    let mut out = serde_yaml::Mapping::new();
    out.insert(
        serde_yaml::Value::String(KUBE_KEY_API_VERSION.to_string()),
        serde_yaml::Value::String(api_version.to_string()),
    );
    out.insert(
        serde_yaml::Value::String(KUBE_KEY_KIND.to_string()),
        serde_yaml::Value::String(kind.to_string()),
    );
    out.insert(
        serde_yaml::Value::String(KUBE_KEY_METADATA.to_string()),
        serde_yaml::Value::Mapping(metadata_map),
    );
    out
}

/// Build a single-field [`serde_yaml::Value::Mapping`] from a typed
/// `Option<T>` slot — `None` when the slot is unset, `Some(Mapping {
/// inner_key: f(t) })` otherwise.
///
/// The canonical shape every per-`:politicas` overlay across `caixa-mesh`
/// uses to wire a typed `MeshPolicy` axis through to its single-key
/// cluster artifact:
///
///   * `:politicas :timeout`        → `timeouts: { request: <duration> }`
///     (Gateway API `HTTPRoute.spec.rules[].timeouts`, wired in 5f477a6)
///   * `:politicas :retries`        → `retry: { attempts: <number> }`
///     (Gateway API `HTTPRoute.spec.rules[].retry`, wired in 23b7f00)
///   * `:politicas :mtls-required`  → `authentication: { mode: <enum> }`
///     (Cilium `CiliumNetworkPolicy.spec.ingress[].authentication`,
///     wired in 878bf81)
///
/// Until this lift the three call sites each carried a verbatim copy
/// of the same six-line block — `let mut m = serde_yaml::Mapping::new();
/// m.insert(Value::String(<key>.into()), <value>); Value::Mapping(m)` —
/// wrapped in `spec.politicas.<axis>.map(|v| { … })`. Three-of-the-pattern
/// across one emit-site (and now structurally one-of-the-pattern in each
/// of the next two emit-sites the M3.x roadmap acknowledges: the
/// `:circuit-breaker` and `:rate-limit` axes' `CiliumClusterwideEnvoyConfig`
/// emitter, MESH-COMPOSITION §III.2 #3) overflows the duplication
/// budget; this helper is the lifted typed primitive.
///
/// The caller passes:
///   * the typed `Option<T>` slot,
///   * the inner YAML key the artifact's per-axis schema names
///     (`request` / `attempts` / `mode` for the three landed overlays;
///     `consecutiveErrors` / `requestsPerUnit` for the two roadmap
///     axes), and
///   * a closure converting the typed `T` into the inner field's
///     [`serde_yaml::Value`] (typically a `String` for canonical
///     duration / enum scalars or a `Number` for typed integer
///     attempt counts).
///
/// Returns `Some(Mapping)` when the slot is `Some`, `None` otherwise —
/// the caller's `if let Some(overlay) = … { rule.insert(<outer_key>,
/// overlay.clone()) }` guard for the *outer* key (`timeouts` / `retry`
/// / `authentication` — which the per-rule iteration applies to every
/// emitted item) becomes the single emission gate, and the *inner*
/// shape is built once by the closure.
///
/// Pairs with the `MeshPolicy::is_empty` predicate at the typed-axis
/// emptiness layer: `is_empty()` short-circuits the whole `:politicas`
/// block when every axis is `None`; this helper short-circuits the
/// per-axis overlay when its single axis is `None`. Two layers, same
/// "named-axis-with-None-means-skip-emit" contract THEORY.md §V.2.7
/// render determinism extends to.
#[must_use]
pub fn single_field_overlay<T, F>(
    slot: Option<T>,
    inner_key: &'static str,
    f: F,
) -> Option<serde_yaml::Value>
where
    F: FnOnce(T) -> serde_yaml::Value,
{
    slot.map(|v| {
        let mut m = serde_yaml::Mapping::new();
        m.insert(serde_yaml::Value::String(inner_key.into()), f(v));
        serde_yaml::Value::Mapping(m)
    })
}

/// Render the M2 typed-slot YAML overlay for a Caixa: the camelCase
/// `(key, value)` fragments every per-Servico renderer
/// ([`caixa-helm`]'s values block, [`caixa-flux`]'s programs.yaml
/// entry) merges into its target with `or_insert` semantics so explicit
/// `spec.*` fields from the ComputeUnit YAML take precedence over the
/// manifest-derived overlay.
///
/// Keys (alphabetically ordered, since the return type is
/// [`BTreeMap`]) match the ComputeUnit / pleme-computeunit values
/// schema:
///
///   * [`M2_KEY_BEHAVIOR`] — present iff `caixa.behavior` is `Some`
///     and `BehaviorSpec::is_empty` returns `false`.
///   * [`M2_KEY_LIMITS`] — present iff `caixa.limits` is `Some` and
///     `LimitsSpec::is_empty` returns `false`.
///   * [`M2_KEY_UPGRADE_FROM`] — present iff `caixa.upgrade_from` is
///     non-empty.
///
/// An entirely empty M2 surface returns an empty map; the renderer
/// merges zero fragments and emits no extra keys (the per-renderer
/// "empty M2 slots do not appear" tests pin this invariant —
/// `caixa_helm::tests::empty_m2_slots_do_not_appear` and
/// `caixa_flux::tests::empty_m2_slots_do_not_appear_in_programs_yaml_entry`).
///
/// # Errors
///
/// Returns [`RenderError::Yaml`] if `serde_yaml::to_value` fails for
/// any of the typed M2 slot values. The prior inline block silently
/// substituted [`serde_yaml::Value::Null`] in this case, which renders
/// as e.g. `limits: null` — indistinguishable from "the author omitted
/// the slot" once it leaves the typed surface.
pub fn servico_m2_overlay(
    caixa: &Caixa,
) -> Result<BTreeMap<&'static str, serde_yaml::Value>, RenderError> {
    let mut out = BTreeMap::new();
    if let Some(limits) = &caixa.limits {
        if !limits.is_empty() {
            let v = serde_yaml::to_value(limits).map_err(|source| RenderError::Yaml {
                slot: M2_KEY_LIMITS,
                source,
            })?;
            out.insert(M2_KEY_LIMITS, v);
        }
    }
    if let Some(behavior) = &caixa.behavior {
        if !behavior.is_empty() {
            let v = serde_yaml::to_value(behavior).map_err(|source| RenderError::Yaml {
                slot: M2_KEY_BEHAVIOR,
                source,
            })?;
            out.insert(M2_KEY_BEHAVIOR, v);
        }
    }
    if !caixa.upgrade_from.is_empty() {
        let v = serde_yaml::to_value(&caixa.upgrade_from).map_err(|source| RenderError::Yaml {
            slot: M2_KEY_UPGRADE_FROM,
            source,
        })?;
        out.insert(M2_KEY_UPGRADE_FROM, v);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BehaviorSpec, CaixaKind, LimitsSpec, UpgradeFromEntry, UpgradeInstruction};
    use std::path::PathBuf;
    use std::time::Duration;

    fn bare_servico() -> Caixa {
        Caixa {
            nome: "hello-rio".into(),
            versao: "0.1.0".into(),
            kind: CaixaKind::Servico,
            edicao: Some("2026".into()),
            descricao: None,
            repositorio: None,
            licenca: None,
            autores: vec![],
            etiquetas: vec![],
            deps: vec![],
            deps_dev: vec![],
            exe: vec![],
            bibliotecas: vec![],
            servicos: vec!["servicos/hello-rio.computeunit.yaml".into()],
            limits: None,
            behavior: None,
            upgrade_from: vec![],
            estrategia: None,
            max_restarts: None,
            restart_window: None,
            children: vec![],
            membros: vec![],
            contratos: vec![],
            politicas: None,
            placement: None,
            entrada: None,
        }
    }

    #[test]
    fn empty_caixa_returns_empty_overlay() {
        let overlay = servico_m2_overlay(&bare_servico()).unwrap();
        assert!(
            overlay.is_empty(),
            "a Caixa with no M2 slots emits zero overlay fragments"
        );
    }

    #[test]
    fn empty_typed_specs_are_skipped_like_unset_ones() {
        // `Some(LimitsSpec::default())` (every axis None) and
        // `Some(BehaviorSpec::default())` (every callback None) must
        // round-trip identical to `None` — the is_empty()-skip
        // invariant the renderers' "empty M2 slots do not appear"
        // tests pinned inline before this lift.
        let mut c = bare_servico();
        c.limits = Some(LimitsSpec::default());
        c.behavior = Some(BehaviorSpec::default());
        let overlay = servico_m2_overlay(&c).unwrap();
        assert!(overlay.is_empty());
    }

    #[test]
    fn limits_slot_appears_under_camelcase_key() {
        let mut c = bare_servico();
        c.limits = Some(LimitsSpec {
            memory: Some(64 * 1024 * 1024),
            fuel: Some(1_000_000),
            wall_clock: Some(Duration::from_secs(30)),
            cpu: Some(500),
        });
        let overlay = servico_m2_overlay(&c).unwrap();
        assert_eq!(overlay.len(), 1);
        let limits = overlay.get(M2_KEY_LIMITS).expect("limits key present");
        assert_eq!(limits.get("memory").and_then(|m| m.as_str()), Some("64MiB"));
        assert_eq!(
            limits.get("wallClock").and_then(|m| m.as_str()),
            Some("30s")
        );
    }

    #[test]
    fn behavior_slot_appears_under_camelcase_key() {
        let mut c = bare_servico();
        c.behavior = Some(BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            on_call: Some(PathBuf::from("lib/handlers.lisp")),
            ..Default::default()
        });
        let overlay = servico_m2_overlay(&c).unwrap();
        let behavior = overlay.get(M2_KEY_BEHAVIOR).expect("behavior key present");
        assert_eq!(
            behavior.get("onInit").and_then(|v| v.as_str()),
            Some("lib/init.lisp")
        );
        assert_eq!(
            behavior.get("onCall").and_then(|v| v.as_str()),
            Some("lib/handlers.lisp")
        );
    }

    #[test]
    fn upgrade_from_slot_appears_under_camelcase_key() {
        let mut c = bare_servico();
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.0.9".into(),
            instructions: vec![UpgradeInstruction::LoadModule {
                module: "hello-rio".into(),
            }],
        }];
        let overlay = servico_m2_overlay(&c).unwrap();
        let upgrade = overlay
            .get(M2_KEY_UPGRADE_FROM)
            .expect("upgradeFrom key present");
        let arr = upgrade.as_sequence().expect("sequence");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0].get("from").and_then(|v| v.as_str()), Some("0.0.9"));
    }

    #[test]
    fn all_three_slots_appear_in_alphabetical_iteration_order() {
        // BTreeMap iteration is sorted by key — pin that the renderers
        // can rely on a deterministic iteration order, which feeds
        // into deterministic YAML output (the value-as-proof property
        // THEORY.md §V.2.7 "render determinism" requires).
        let mut c = bare_servico();
        c.limits = Some(LimitsSpec {
            memory: Some(64 * 1024 * 1024),
            ..Default::default()
        });
        c.behavior = Some(BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            ..Default::default()
        });
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.0.9".into(),
            instructions: vec![UpgradeInstruction::LoadModule {
                module: "hello-rio".into(),
            }],
        }];
        let overlay = servico_m2_overlay(&c).unwrap();
        let keys: Vec<_> = overlay.keys().copied().collect();
        assert_eq!(
            keys,
            vec![M2_KEY_BEHAVIOR, M2_KEY_LIMITS, M2_KEY_UPGRADE_FROM]
        );
    }

    #[test]
    fn pleme_label_consts_share_canonical_prefix() {
        // Single-source-of-truth invariant: every pleme-io label key
        // is `<PLEME_LABEL_PREFIX>/<axis>`. A future label-namespace
        // rebrand is a one-line PLEME_LABEL_PREFIX edit + this test
        // pins the contract that no other label leaks past the lift.
        for k in [LABEL_APLICACAO, LABEL_PROGRAM, LABEL_CONTRATO] {
            assert!(
                k.starts_with(PLEME_LABEL_PREFIX),
                "label key {k:?} must share the {PLEME_LABEL_PREFIX:?} prefix"
            );
            // Each label is `<prefix>/<axis>` — the suffix is non-empty
            // (the `/` separator is followed by the axis name).
            let suffix = k.strip_prefix(PLEME_LABEL_PREFIX).unwrap();
            assert!(suffix.starts_with('/'));
            assert!(suffix.len() > 1, "axis name must be non-empty for {k:?}");
        }
    }

    #[test]
    fn pleme_label_consts_have_expected_canonical_values() {
        // Pin the actual string values so a typo in the lift can't
        // silently rebrand the whole pleme-io label namespace. These
        // strings are part of the cluster-side contract with the
        // lareira-fleet-programs chart + Cilium identity layer + Hubble
        // flow attribution; changing any of them is a coordinated
        // multi-repo migration, not an incidental edit.
        assert_eq!(PLEME_LABEL_PREFIX, "pleme.pleme.io");
        assert_eq!(LABEL_APLICACAO, "pleme.pleme.io/aplicacao");
        assert_eq!(LABEL_PROGRAM, "pleme.pleme.io/program");
        assert_eq!(LABEL_CONTRATO, "pleme.pleme.io/contrato");
    }

    #[test]
    fn default_namespace_pins_canonical_value() {
        // Pin the actual string so a typo in this lift can't silently
        // rebrand the cluster-side namespace every renderer emits
        // into. The string is part of the cluster-side contract with
        // the lareira-fleet-programs aggregator chart, the per-cluster
        // CiliumNetworkPolicy `endpointSelector` namespace scope, the
        // Gateway / HTTPRoute apply namespace, and the future M4
        // `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's apply
        // namespace; changing it is a coordinated multi-repo migration
        // (the per-cluster k8s repo's namespaces, every
        // lareira-fleet-programs HelmRelease's targetNamespace, every
        // ComputeUnit's `metadata.namespace`), not an incidental edit.
        // Peer to `pleme_label_consts_have_expected_canonical_values`
        // on the canonical-string-value-pin axis for the
        // `PLEME_LABEL_PREFIX` / `LABEL_*` constants.
        assert_eq!(DEFAULT_NAMESPACE, "tatara-system");
    }

    #[test]
    fn default_namespace_is_a_valid_dns_1123_label() {
        // Cross-axis invariant: the default namespace lands as
        // `metadata.namespace` on every emitted K8s object across every
        // renderer, and the K8s apiserver enforces the DNS-1123 label
        // rule on every `metadata.namespace`. Pinning this here means
        // a future rebrand on the canonical `DEFAULT_NAMESPACE`
        // declaration can't silently land a value the apiserver
        // refuses at the *first* renderer to apply against a cluster,
        // far from the rebrand commit's source — the typed
        // [`is_dns_1123_label`] floor rejects it at caixa-core build
        // time on the canonical lift, before any renderer consumes
        // the value. Same trajectory as `:membros :caixa` /
        // `:placement :clusters` / `:contratos :de`/`:para` /
        // `:entrada :para` / `:placement :affinity` (dfd4902 — the
        // five typed-identifier axes on the Aplicacao surface that
        // already land on this same `is_dns_1123_label` floor at
        // their respective validate gates), now extended onto the
        // canonical-namespace-default axis the renderers share.
        assert!(
            is_dns_1123_label(DEFAULT_NAMESPACE).is_ok(),
            "DEFAULT_NAMESPACE {DEFAULT_NAMESPACE:?} must be a valid \
             DNS-1123 label — every K8s apiserver-side schema enforces \
             this rule on `metadata.namespace`"
        );
    }

    // ── lareira-<nome> chart-name prefix lift ──────────────────────
    //
    // The lift pins the substrate-wide `lareira-` chart-name prefix
    // as the single source of truth every per-Servico renderer
    // (caixa-helm, caixa-flux, caixa-tatara) reaches for, peer to the
    // [`DEFAULT_NAMESPACE`] (a085b26) lift on the canonical-namespace
    // axis. Pinning the prefix value, the helper's
    // construction-shape, and the DNS-1123-label round-trip for the
    // canonical-fixture input forms the structural floor every future
    // renderer consumer inherits by construction.

    #[test]
    fn lareira_chart_name_prefix_pins_canonical_value() {
        // Pin the actual string value so a typo on the canonical lift
        // can't silently rebrand the substrate's per-Servico Helm chart
        // namespace. The string is part of the contract with the OCI
        // chart-publishing pipeline (`oci://<registry>/lareira-<nome>`),
        // the per-cluster HelmRelease `chart:` field (which Flux
        // resolves through the OCI ref), and the historical
        // `pleme-io/helmworks/charts/lareira-<name>/` source tree
        // layout (caixa-helm/src/lib.rs:7); changing it is a
        // coordinated multi-repo migration, not an incidental edit.
        // Peer to `default_namespace_pins_canonical_value` on the
        // canonical-string-value-pin axis for the
        // `DEFAULT_NAMESPACE` constant.
        assert_eq!(LAREIRA_CHART_NAME_PREFIX, "lareira-");
    }

    #[test]
    fn lareira_chart_name_composes_prefix_and_nome() {
        // Pin the helper's construction shape — the chart name is the
        // prefix concatenated with the caixa's `:nome` verbatim, with
        // no intermediate hyphen, no path separator, no trimming. Pin
        // the canonical hello-rio fixture (the in-tree
        // `caixa-helm` test fixture at caixa-helm/src/lib.rs:431
        // already asserts `dir.name == "lareira-hello-rio"`, which
        // this helper now derives) and a peer fixture
        // (`checkout-aplicacao` member) to sweep the typical author
        // surface.
        assert_eq!(lareira_chart_name("hello-rio"), "lareira-hello-rio");
        assert_eq!(lareira_chart_name("cart"), "lareira-cart");
        assert_eq!(lareira_chart_name("worker"), "lareira-worker");
    }

    #[test]
    fn lareira_chart_name_starts_with_prefix() {
        // Cross-axis invariant: every output of the helper begins with
        // the lifted prefix verbatim — a future refactor that
        // accidentally introduced a different prefix-application
        // shape (e.g. `format!("{nome}-lareira")` transposition, or a
        // `to_uppercase()` case fold) would surface here. The
        // structural pin holds for the empty `:nome` shape too
        // (a value `validate_nome` rejects upstream, but the helper
        // itself imposes no shape on the input).
        for nome in ["hello-rio", "cart", "worker", "a", ""] {
            let chart = lareira_chart_name(nome);
            assert!(
                chart.starts_with(LAREIRA_CHART_NAME_PREFIX),
                "lareira_chart_name({nome:?}) = {chart:?} must start with the lifted prefix \
                 {LAREIRA_CHART_NAME_PREFIX:?}"
            );
        }
    }

    #[test]
    fn lareira_chart_name_round_trips_through_dns_1123_for_validated_nome() {
        // Cross-axis invariant: every `:nome` past
        // [`Caixa::validate_nome`] (6c992f8) is a valid DNS-1123 label,
        // and the prepended `lareira-` segment is itself a valid
        // DNS-1123 label prefix (lowercase ASCII + hyphen with a
        // terminating-hyphen continuation). The composition therefore
        // round-trips through [`is_dns_1123_label`] for every
        // `:nome` whose joint length with the prefix stays ≤ 63 bytes
        // (the DNS-1123 label cap). The canonical author surface sits
        // far below that cap (the in-tree fixtures range from
        // `"a"` = 9-byte chart name to `"checkout"` = 16 bytes, with
        // the cap admitting up to 55-byte `:nome` values). Pin the
        // round-trip for the canonical-fixture set so a future renderer
        // that lands the helper's output verbatim as a K8s
        // `metadata.name` (caixa-helm's `ChartDir.name`,
        // caixa-flux's HelmRelease `chart:` field, caixa-tatara's
        // `release_name`) inherits the apiserver-valid floor by
        // construction.
        for nome in ["hello-rio", "cart", "worker", "checkout", "a"] {
            let chart = lareira_chart_name(nome);
            assert!(
                is_dns_1123_label(&chart).is_ok(),
                "lareira_chart_name({nome:?}) = {chart:?} must be a valid DNS-1123 label"
            );
        }
    }

    #[test]
    fn lareira_chart_name_prefix_is_a_valid_dns_1123_segment_continuation() {
        // The lifted prefix is one substring of the rendered chart
        // name; pin its grammar so a future rebrand can't land a
        // value that would invalidate the joint DNS-1123 label
        // structurally. The prefix must:
        //   - be lowercase ASCII alphanumeric + hyphen (the DNS-1123
        //     accepted set), so its bytes don't widen the joint
        //     accepted set;
        //   - end with a hyphen (so the concatenation slot doesn't
        //     accidentally merge with the leading character of the
        //     `:nome` it precedes).
        assert!(
            LAREIRA_CHART_NAME_PREFIX
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
            "LAREIRA_CHART_NAME_PREFIX {LAREIRA_CHART_NAME_PREFIX:?} must use only DNS-1123-label \
             bytes (lowercase ASCII alphanumeric + hyphen)"
        );
        assert!(
            LAREIRA_CHART_NAME_PREFIX.ends_with('-'),
            "LAREIRA_CHART_NAME_PREFIX {LAREIRA_CHART_NAME_PREFIX:?} must end with `-` so \
             concatenation with the caixa's `:nome` produces a hyphenated joint label"
        );
    }

    // ── is_lareira_chart_name_shape — joint-length budget on `:nome` ─────
    //
    // The canonical [`lareira_chart_name`] helper's own doc comment
    // (f7320d7) explicitly defers: "the M4 admission webhook will pin
    // the joint-length invariant when it lands". These tests land it
    // at the manifest-validate layer instead — the predicate consults
    // [`lareira_chart_name`] + [`is_dns_1123_label`] (no third primitive)
    // so a future rebrand of either axis re-derives the budget
    // mechanically and the test suite re-pins through the same lifts.

    #[test]
    fn lareira_chart_name_nome_max_len_pins_arithmetic() {
        // Pin the arithmetic so a future shift in either input axis
        // surfaces here. The const is mechanically derived from
        // [`DNS_1123_LABEL_MAX_LEN`] (63 — the K8s apiserver cap every
        // chart-name-derived `metadata.name` inherits) minus
        // [`LAREIRA_CHART_NAME_PREFIX`].len() (8 — the canonical
        // chart-name prefix the lift f7320d7 made structural). The
        // landing value: 55 bytes the caixa's `:nome` may itself
        // occupy under the joint chart-name cap.
        assert_eq!(LAREIRA_CHART_NAME_NOME_MAX_LEN, 55);
        assert_eq!(
            LAREIRA_CHART_NAME_NOME_MAX_LEN,
            DNS_1123_LABEL_MAX_LEN - LAREIRA_CHART_NAME_PREFIX.len()
        );
    }

    #[test]
    fn is_lareira_chart_name_shape_accepts_canonical_fixtures() {
        // Positive control: every in-tree fixture `:nome` (caixa-helm,
        // caixa-flux, caixa-mesh, caixa-tatara tests, the
        // checkout-aplicacao example) sits far below the cap. The
        // predicate must not regress this baseline shape.
        for nome in [
            "hello-rio",
            "cart",
            "worker",
            "checkout",
            "a",
            "akeyless-attest",
        ] {
            is_lareira_chart_name_shape(nome).unwrap_or_else(|e| {
                panic!("canonical :nome {nome:?} must pass chart-name budget, got {e:?}")
            });
        }
    }

    #[test]
    fn is_lareira_chart_name_shape_accepts_nome_at_budget() {
        // Boundary-accepting case at the 55-byte cap — the joint
        // chart name is exactly 63 bytes, the DNS-1123 label cap.
        let at_cap = "a".repeat(LAREIRA_CHART_NAME_NOME_MAX_LEN);
        assert_eq!(at_cap.len(), LAREIRA_CHART_NAME_NOME_MAX_LEN);
        is_lareira_chart_name_shape(&at_cap).unwrap();
        assert_eq!(lareira_chart_name(&at_cap).len(), DNS_1123_LABEL_MAX_LEN);
    }

    #[test]
    fn is_lareira_chart_name_shape_rejects_nome_one_over_budget() {
        // Fail-before-pass-after pin: 56 bytes is the smallest `:nome`
        // length that overflows the joint chart-name cap. The inner
        // [`is_dns_1123_label`] check accepts it (56 ≤ 63), so prior
        // to this gate it silently passed `Caixa::validate_nome` and
        // surfaced as a `helm lint` / apiserver rejection on the
        // rendered chart name far from the source caixa.lisp.
        let over = "a".repeat(LAREIRA_CHART_NAME_NOME_MAX_LEN + 1);
        let err = is_lareira_chart_name_shape(&over).unwrap_err();
        assert!(
            err.contains("63") && err.contains("64") && err.contains("55"),
            "diagnostic must name the DNS-1123 cap (63), the actual chart-name length (64), \
             and the per-`:nome` budget (55), got {err:?}"
        );
        assert!(
            err.contains("lareira-"),
            "diagnostic must name the canonical prefix verbatim, got {err:?}"
        );
    }

    #[test]
    fn is_lareira_chart_name_shape_diagnostic_carries_offending_chart_name() {
        // The rendered chart name appears verbatim in the diagnostic
        // so the author sees exactly the string the apiserver would
        // have rejected — no re-derivation required to grep the source.
        let over = "x".repeat(LAREIRA_CHART_NAME_NOME_MAX_LEN + 5);
        let err = is_lareira_chart_name_shape(&over).unwrap_err();
        let expected_chart = lareira_chart_name(&over);
        assert!(
            err.contains(&expected_chart),
            "diagnostic must carry the rendered chart name {expected_chart:?} verbatim, \
             got {err:?}"
        );
    }

    #[test]
    fn is_lareira_chart_name_shape_composes_through_canonical_helper() {
        // Cross-axis invariant: the predicate is defined exactly as
        // `is_dns_1123_label(lareira_chart_name(nome))` for the length
        // arm — no inline `format!("lareira-{nome}")` shape duplicating
        // the canonical lift. Pinning this composition closes the
        // drift footgun where a future predicate refactor re-inlines
        // the prefix-and-`:nome` concatenation and diverges from the
        // canonical helper. Sweep across the boundary so both sides
        // (accept + reject) consult the same helper.
        for delta in 0..=2usize {
            let nome = "z".repeat(LAREIRA_CHART_NAME_NOME_MAX_LEN.saturating_sub(delta));
            let predicate_ok = is_lareira_chart_name_shape(&nome).is_ok();
            let canonical_ok = is_dns_1123_label(&lareira_chart_name(&nome)).is_ok();
            assert_eq!(
                predicate_ok,
                canonical_ok,
                "predicate / canonical-composition divergence for :nome of len {} \
                 (predicate_ok = {predicate_ok}, canonical_ok = {canonical_ok})",
                nome.len()
            );
        }
    }

    #[test]
    fn pleme_program_selector_carries_only_program() {
        let sel = pleme_program_selector("cart");
        assert_eq!(sel.len(), 1);
        assert_eq!(sel.get(LABEL_PROGRAM).map(String::as_str), Some("cart"));
        assert!(sel.get(LABEL_APLICACAO).is_none());
    }

    #[test]
    fn pleme_program_in_aplicacao_selector_carries_both_axes() {
        let sel = pleme_program_in_aplicacao_selector("cart", "checkout");
        assert_eq!(sel.len(), 2);
        assert_eq!(sel.get(LABEL_PROGRAM).map(String::as_str), Some("cart"));
        assert_eq!(
            sel.get(LABEL_APLICACAO).map(String::as_str),
            Some("checkout")
        );
    }

    #[test]
    fn pleme_program_in_aplicacao_selector_iterates_alphabetically() {
        // BTreeMap iteration is sorted by key — pin that the renderer
        // (which translates the selector into a serde_yaml::Mapping
        // by iteration) gets a deterministic key order. `aplicacao`
        // sorts before `program`, so the rendered YAML's
        // `matchLabels:` block appears in that order regardless of
        // call-site arg order. Mirrors the M2 overlay helper's
        // alphabetical-iteration determinism property
        // (THEORY.md §V.2.7 render determinism).
        let sel = pleme_program_in_aplicacao_selector("cart", "checkout");
        let keys: Vec<_> = sel.keys().copied().collect();
        assert_eq!(keys, vec![LABEL_APLICACAO, LABEL_PROGRAM]);
    }

    #[test]
    fn pleme_program_in_aplicacao_selector_arg_order_independent() {
        // Renaming the program vs. the aplicacao must each only affect
        // its own axis — pin that the helper doesn't transpose its
        // args silently (a footgun the prior inline-string approach
        // had: `program: <de>` and `aplicacao: <name>` were two
        // adjacent insert() calls with structurally identical arms,
        // trivially swappable in a refactor).
        let sel = pleme_program_in_aplicacao_selector("cart", "checkout");
        assert_eq!(sel.get(LABEL_PROGRAM).map(String::as_str), Some("cart"));
        assert_eq!(
            sel.get(LABEL_APLICACAO).map(String::as_str),
            Some("checkout")
        );
        let swapped = pleme_program_in_aplicacao_selector("checkout", "cart");
        assert_eq!(
            swapped.get(LABEL_PROGRAM).map(String::as_str),
            Some("checkout")
        );
        assert_eq!(
            swapped.get(LABEL_APLICACAO).map(String::as_str),
            Some("cart")
        );
    }

    #[test]
    fn yaml_string_mapping_empty_input_returns_empty_mapping() {
        // Empty input → empty Mapping. Pinned because the caller's
        // emptiness contract (e.g. caixa-mesh's CNP labels block: the
        // policy's metadata.labels exists iff there are pleme-prefixed
        // labels to carry) depends on this being faithful.
        let v: serde_yaml::Value = yaml_string_mapping(BTreeMap::<&'static str, String>::new());
        let m = v.as_mapping().expect("mapping shape");
        assert!(m.is_empty());
    }

    #[test]
    fn yaml_string_mapping_round_trips_string_values() {
        let mut input = BTreeMap::new();
        input.insert("foo", "1".to_string());
        input.insert("bar", "2".to_string());
        let v = yaml_string_mapping(input);
        let m = v.as_mapping().expect("mapping shape");
        assert_eq!(m.len(), 2);
        assert_eq!(
            m.get(serde_yaml::Value::String("foo".into()))
                .and_then(|x| x.as_str()),
            Some("1")
        );
        assert_eq!(
            m.get(serde_yaml::Value::String("bar".into()))
                .and_then(|x| x.as_str()),
            Some("2")
        );
    }

    #[test]
    fn yaml_string_mapping_iterates_alphabetically_on_btreemap() {
        // Pin that BTreeMap input → alphabetical iteration → alphabetical
        // YAML key order. THEORY.md §V.2.7 render determinism.
        let mut input = BTreeMap::new();
        input.insert("zebra", "z".to_string());
        input.insert("apple", "a".to_string());
        input.insert("mango", "m".to_string());
        let v = yaml_string_mapping(input);
        let m = v.as_mapping().expect("mapping shape");
        let keys: Vec<&str> = m.iter().filter_map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["apple", "mango", "zebra"]);
    }

    #[test]
    fn yaml_string_mapping_accepts_pleme_selector_helpers() {
        // The lift's load-bearing use case: passing the typed pleme-io
        // selectors directly into yaml_string_mapping yields the K8s
        // matchLabels surface every Cilium / Gateway selector field
        // expects, with the alphabetical key order the pleme helpers'
        // own determinism contract guarantees. Pinning end-to-end
        // composition so a future refactor of either helper can't
        // silently break the integration.
        let v = yaml_string_mapping(pleme_program_in_aplicacao_selector("cart", "checkout"));
        let m = v.as_mapping().expect("mapping shape");
        assert_eq!(m.len(), 2);
        assert_eq!(
            m.get(serde_yaml::Value::String(LABEL_PROGRAM.into()))
                .and_then(|x| x.as_str()),
            Some("cart")
        );
        assert_eq!(
            m.get(serde_yaml::Value::String(LABEL_APLICACAO.into()))
                .and_then(|x| x.as_str()),
            Some("checkout")
        );
    }

    #[test]
    fn kube_key_consts_have_expected_values() {
        // Pin the actual string values — these are part of the K8s API
        // surface that every emitted artifact's apiserver-side parser
        // (Cilium, Gateway API, wasm-operator) depends on. Changing any
        // of them is a coordinated multi-renderer migration, not an
        // incidental edit.
        assert_eq!(KUBE_KEY_API_VERSION, "apiVersion");
        assert_eq!(KUBE_KEY_KIND, "kind");
        assert_eq!(KUBE_KEY_METADATA, "metadata");
        assert_eq!(KUBE_KEY_NAME, "name");
        assert_eq!(KUBE_KEY_NAMESPACE, "namespace");
        assert_eq!(KUBE_KEY_LABELS, "labels");
        assert_eq!(KUBE_KEY_MATCH_LABELS, "matchLabels");
    }

    // ── label_selector — typed K8s LabelSelector wrapper ─────────────────

    #[test]
    fn label_selector_wraps_in_match_labels_envelope() {
        // The lift's contract: input labels appear under the canonical
        // `matchLabels` key, and the outer Value is a Mapping with
        // exactly that one key. Pinning the shape so a future
        // refactor can't silently drop the wrapper (which would emit
        // bare `aplicacao: …, program: …` directly under the K8s
        // selector field — a structurally invalid LabelSelector that
        // some apiserver-side parsers tolerate by matching the empty
        // set, a sharp footgun).
        let mut labels = BTreeMap::new();
        labels.insert(LABEL_APLICACAO, "checkout".to_string());
        labels.insert(LABEL_PROGRAM, "cart".to_string());
        let sel = label_selector(labels);
        let m = sel.as_mapping().expect("mapping shape");
        assert_eq!(m.len(), 1);
        let inner = m
            .get(serde_yaml::Value::String(KUBE_KEY_MATCH_LABELS.into()))
            .and_then(|v| v.as_mapping())
            .expect("matchLabels inner mapping");
        assert_eq!(inner.len(), 2);
        assert_eq!(
            inner
                .get(serde_yaml::Value::String(LABEL_APLICACAO.into()))
                .and_then(|x| x.as_str()),
            Some("checkout")
        );
        assert_eq!(
            inner
                .get(serde_yaml::Value::String(LABEL_PROGRAM.into()))
                .and_then(|x| x.as_str()),
            Some("cart")
        );
    }

    #[test]
    fn label_selector_empty_input_yields_empty_match_labels() {
        // Empty input → `{matchLabels: {}}`. The outer wrapper is
        // present (the K8s LabelSelector schema requires it as a
        // structural anchor, and apiserver-side parsers that see a
        // bare `{}` selector match-everything; pinning the wrapper
        // means an empty pleme-io selector at the call site renders
        // as the canonical "no labels declared, match nothing
        // specific" shape rather than an outright missing key).
        let v: serde_yaml::Value = label_selector(BTreeMap::<&'static str, String>::new());
        let m = v.as_mapping().expect("mapping shape");
        assert_eq!(m.len(), 1);
        let inner = m
            .get(serde_yaml::Value::String(KUBE_KEY_MATCH_LABELS.into()))
            .and_then(|v| v.as_mapping())
            .expect("matchLabels inner mapping");
        assert!(inner.is_empty());
    }

    #[test]
    fn label_selector_accepts_pleme_selector_helpers() {
        // The lift's load-bearing use case: passing the typed pleme-io
        // selectors directly into `label_selector` yields the K8s
        // LabelSelector shape every Cilium / Gateway / future
        // app-operator selector field expects. Pinning end-to-end
        // composition so a future refactor of either helper can't
        // silently break the integration.
        let v = label_selector(pleme_program_in_aplicacao_selector("cart", "checkout"));
        let inner = v
            .as_mapping()
            .and_then(|m| m.get(serde_yaml::Value::String(KUBE_KEY_MATCH_LABELS.into())))
            .and_then(|v| v.as_mapping())
            .expect("matchLabels inner mapping");
        assert_eq!(inner.len(), 2);
        assert_eq!(
            inner
                .get(serde_yaml::Value::String(LABEL_PROGRAM.into()))
                .and_then(|x| x.as_str()),
            Some("cart")
        );
        assert_eq!(
            inner
                .get(serde_yaml::Value::String(LABEL_APLICACAO.into()))
                .and_then(|x| x.as_str()),
            Some("checkout")
        );

        // Single-axis variant — only LABEL_PROGRAM under matchLabels.
        let v = label_selector(pleme_program_selector("cart"));
        let inner = v
            .as_mapping()
            .and_then(|m| m.get(serde_yaml::Value::String(KUBE_KEY_MATCH_LABELS.into())))
            .and_then(|v| v.as_mapping())
            .unwrap();
        assert_eq!(inner.len(), 1);
        assert_eq!(
            inner
                .get(serde_yaml::Value::String(LABEL_PROGRAM.into()))
                .and_then(|x| x.as_str()),
            Some("cart")
        );
    }

    #[test]
    fn label_selector_inner_iterates_alphabetically_on_btreemap() {
        // BTreeMap input → alphabetical iteration → alphabetical YAML
        // key order under `matchLabels`. THEORY.md §V.2.7 render
        // determinism: the rendered YAML's matchLabels: block appears
        // in a deterministic order independent of source-code
        // declaration order.
        let mut input = BTreeMap::new();
        input.insert("zebra", "z".to_string());
        input.insert("apple", "a".to_string());
        input.insert("mango", "m".to_string());
        let v = label_selector(input);
        let inner = v
            .as_mapping()
            .and_then(|m| m.get(serde_yaml::Value::String(KUBE_KEY_MATCH_LABELS.into())))
            .and_then(|v| v.as_mapping())
            .unwrap();
        let keys: Vec<&str> = inner.iter().filter_map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["apple", "mango", "zebra"]);
    }

    #[test]
    fn label_selector_does_not_introduce_match_expressions_axis() {
        // V0 emits matchLabels only — pinning that the helper doesn't
        // pre-insert an empty `matchExpressions: []` block (which some
        // apiserver-side parsers tolerate but renders noisily and
        // shifts the per-rule diff). A future set-based selector
        // extension is a deliberate API change to this helper, not an
        // incidental shape leak.
        let v = label_selector(pleme_program_selector("cart"));
        let m = v.as_mapping().unwrap();
        assert!(
            m.get(serde_yaml::Value::String("matchExpressions".into()))
                .is_none(),
            "label_selector must not pre-insert a matchExpressions key (V0 is matchLabels-only)"
        );
    }

    #[test]
    fn kube_resource_skeleton_carries_three_top_level_keys_no_spec() {
        // The skeleton emits exactly apiVersion + kind + metadata; the
        // caller adds spec (and any other top-level keys) themselves.
        // Pin that contract so a future caller doesn't accidentally
        // double-insert apiVersion / kind / metadata after the
        // skeleton call.
        let skel = kube_resource_skeleton(
            "cilium.io/v2",
            "CiliumNetworkPolicy",
            "p-1",
            "tatara-system",
            BTreeMap::new(),
        );
        assert_eq!(skel.len(), 3);
        assert_eq!(
            skel.get(serde_yaml::Value::String(KUBE_KEY_API_VERSION.into()))
                .and_then(|v| v.as_str()),
            Some("cilium.io/v2")
        );
        assert_eq!(
            skel.get(serde_yaml::Value::String(KUBE_KEY_KIND.into()))
                .and_then(|v| v.as_str()),
            Some("CiliumNetworkPolicy")
        );
        assert!(
            skel.get(serde_yaml::Value::String(KUBE_KEY_METADATA.into()))
                .is_some()
        );
    }

    #[test]
    fn kube_resource_skeleton_metadata_carries_name_and_namespace() {
        let skel = kube_resource_skeleton(
            "gateway.networking.k8s.io/v1",
            "Gateway",
            "checkout",
            "tatara-system",
            BTreeMap::new(),
        );
        let metadata = skel
            .get(serde_yaml::Value::String(KUBE_KEY_METADATA.into()))
            .and_then(|v| v.as_mapping())
            .expect("metadata mapping");
        assert_eq!(
            metadata
                .get(serde_yaml::Value::String(KUBE_KEY_NAME.into()))
                .and_then(|v| v.as_str()),
            Some("checkout")
        );
        assert_eq!(
            metadata
                .get(serde_yaml::Value::String(KUBE_KEY_NAMESPACE.into()))
                .and_then(|v| v.as_str()),
            Some("tatara-system")
        );
    }

    #[test]
    fn kube_resource_skeleton_omits_labels_when_empty() {
        // Empty labels → metadata.labels key absent (NOT present-as-empty).
        // K8s API server treats a missing labels key as "no labels
        // declared"; an empty-mapping `labels: {}` serializes
        // differently in some YAML libraries and is a sharp tool for
        // label-based selectors that match the empty set silently.
        let skel = kube_resource_skeleton(
            "gateway.networking.k8s.io/v1",
            "HTTPRoute",
            "r-1",
            "tatara-system",
            BTreeMap::new(),
        );
        let metadata = skel
            .get(serde_yaml::Value::String(KUBE_KEY_METADATA.into()))
            .and_then(|v| v.as_mapping())
            .unwrap();
        assert!(
            metadata
                .get(serde_yaml::Value::String(KUBE_KEY_LABELS.into()))
                .is_none(),
            "metadata.labels must be absent when no labels passed"
        );
        // metadata then has exactly 2 keys: name, namespace.
        assert_eq!(metadata.len(), 2);
    }

    #[test]
    fn kube_resource_skeleton_includes_labels_when_present() {
        let mut labels = BTreeMap::new();
        labels.insert(LABEL_APLICACAO, "checkout".to_string());
        labels.insert(LABEL_CONTRATO, "cart-to-catalog".to_string());
        let skel = kube_resource_skeleton(
            "cilium.io/v2",
            "CiliumNetworkPolicy",
            "p-1",
            "tatara-system",
            labels,
        );
        let metadata = skel
            .get(serde_yaml::Value::String(KUBE_KEY_METADATA.into()))
            .and_then(|v| v.as_mapping())
            .unwrap();
        let labels_block = metadata
            .get(serde_yaml::Value::String(KUBE_KEY_LABELS.into()))
            .and_then(|v| v.as_mapping())
            .expect("metadata.labels mapping present");
        assert_eq!(
            labels_block
                .get(serde_yaml::Value::String(LABEL_APLICACAO.into()))
                .and_then(|v| v.as_str()),
            Some("checkout")
        );
        assert_eq!(
            labels_block
                .get(serde_yaml::Value::String(LABEL_CONTRATO.into()))
                .and_then(|v| v.as_str()),
            Some("cart-to-catalog")
        );
    }

    #[test]
    fn kube_resource_skeleton_metadata_iterates_alphabetically() {
        // Pin that the inner BTreeMap projection makes the rendered
        // YAML's metadata: block alphabetical (labels, name, namespace),
        // regardless of insert order. THEORY.md §V.2.7 render determinism.
        let mut labels = BTreeMap::new();
        labels.insert(LABEL_APLICACAO, "checkout".to_string());
        let skel = kube_resource_skeleton(
            "cilium.io/v2",
            "CiliumNetworkPolicy",
            "p-1",
            "tatara-system",
            labels,
        );
        let metadata = skel
            .get(serde_yaml::Value::String(KUBE_KEY_METADATA.into()))
            .and_then(|v| v.as_mapping())
            .unwrap();
        let keys: Vec<&str> = metadata.iter().filter_map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            vec![KUBE_KEY_LABELS, KUBE_KEY_NAME, KUBE_KEY_NAMESPACE]
        );
    }

    #[test]
    fn kube_resource_skeleton_top_level_iterates_in_insert_order() {
        // The top-level Mapping is a plain serde_yaml::Mapping (insert-
        // ordered), and the skeleton inserts apiVersion → kind →
        // metadata in that order. Pin so a future refactor doesn't
        // silently shift the rendered YAML's top-level key order
        // (which K8s tooling tolerates but humans + diff readability
        // care about — apiVersion-first is the K8s convention).
        let skel = kube_resource_skeleton(
            "cilium.io/v2",
            "CiliumNetworkPolicy",
            "p-1",
            "tatara-system",
            BTreeMap::new(),
        );
        let keys: Vec<&str> = skel.iter().filter_map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            vec![KUBE_KEY_API_VERSION, KUBE_KEY_KIND, KUBE_KEY_METADATA]
        );
    }

    #[test]
    fn kube_resource_skeleton_does_not_introduce_spec_key() {
        // Sanity: the skeleton is metadata-only — `spec` is the caller's
        // responsibility. Pinning so a future "be helpful" refactor
        // doesn't auto-insert an empty `spec: {}` (which would silently
        // shadow caller-side spec construction).
        let skel = kube_resource_skeleton(
            "cilium.io/v2",
            "CiliumNetworkPolicy",
            "p-1",
            "tatara-system",
            BTreeMap::new(),
        );
        assert!(
            skel.get(serde_yaml::Value::String("spec".into())).is_none(),
            "skeleton must not pre-insert a spec key"
        );
    }

    // ── require_kind / KindMismatch — typed kind-check predicate ─────

    #[test]
    fn require_kind_accepts_matching_kind() {
        // A Servico-kind caixa passes a `require_kind(_, Servico)`
        // check — the happy path every renderer sees on a correctly-
        // authored caixa.lisp, surfaced as `Ok(())` so the renderer's
        // call site reads as a one-liner gate rather than a typed
        // pattern match.
        let c = bare_servico();
        require_kind(&c, CaixaKind::Servico).unwrap();
    }

    #[test]
    fn require_kind_rejects_with_typed_mismatch() {
        // A Biblioteca-kind caixa fails a `require_kind(_, Servico)`
        // check with a typed [`KindMismatch`] view that names the
        // offending caixa's `:nome` plus both the expected and actual
        // kinds. Pinning the typed shape so a future Display-format
        // tweak can't silently drop any of the three load-bearing
        // fields (which would regress the "feira verb whose error
        // path doesn't name the offending caixa" punch-list item the
        // protocol calls out).
        let mut c = bare_servico();
        c.kind = CaixaKind::Biblioteca;
        c.servicos = vec![];
        let err = require_kind(&c, CaixaKind::Servico).unwrap_err();
        assert_eq!(err.nome, "hello-rio");
        assert_eq!(err.expected, CaixaKind::Servico);
        assert_eq!(err.actual, CaixaKind::Biblioteca);
    }

    #[test]
    fn kind_mismatch_display_names_offending_caixa_nome() {
        // The Display impl is the load-bearing surface every renderer's
        // `#[error("{0}")] NotAXKind(#[from] KindMismatch)` arm prints
        // through. Pinning the exact rendered form so a future format
        // change is a one-line edit + a one-line test update, not a
        // silent regression of the diagnostic clarity.
        let err = KindMismatch {
            nome: "checkout".into(),
            expected: CaixaKind::Aplicacao,
            actual: CaixaKind::Servico,
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("checkout"),
            "Display must name the offending caixa nome (got: {msg:?})"
        );
        assert!(
            msg.contains("Aplicacao"),
            "Display must name the expected kind (got: {msg:?})"
        );
        assert!(
            msg.contains("Servico"),
            "Display must name the actual kind (got: {msg:?})"
        );
    }

    #[test]
    fn require_kind_distinguishes_every_pair_of_kinds() {
        // Sanity: the predicate is kind-axis-agnostic — it works for
        // every kind / expected pair, not just Servico/Biblioteca.
        // Pinning that the caller can use `require_kind` for any of
        // the five typed kinds (Biblioteca, Binario, Servico,
        // Supervisor, Aplicacao) without a special-cased helper per
        // kind. Same idiom every per-target renderer key off.
        let mut c = bare_servico();
        c.kind = CaixaKind::Aplicacao;
        c.servicos = vec![];
        let err = require_kind(&c, CaixaKind::Supervisor).unwrap_err();
        assert_eq!(err.expected, CaixaKind::Supervisor);
        assert_eq!(err.actual, CaixaKind::Aplicacao);
        require_kind(&c, CaixaKind::Aplicacao).unwrap();
    }

    // ── require_single_servico / ServicoCountMismatch — V0 Servico-shape ─

    #[test]
    fn require_single_servico_accepts_singleton_list() {
        // The happy path: the canonical V0 Servico carries exactly one
        // `:servicos` entry (the ComputeUnit YAML pointer), the same
        // shape every in-tree fixture + canonical example uses. Surfaced
        // as `Ok(())` so the renderer's call site reads as a one-liner
        // gate beside the peer [`require_kind`] check rather than a
        // typed pattern match.
        let c = bare_servico();
        assert_eq!(
            c.servicos.len(),
            1,
            "fixture pin: bare_servico() is singleton"
        );
        require_single_servico(&c).unwrap();
    }

    #[test]
    fn require_single_servico_rejects_empty_list_with_typed_mismatch() {
        // A Servico-kind caixa with zero `:servicos` entries fails
        // `require_single_servico` with a typed [`ServicoCountMismatch`]
        // view that names the offending caixa's `:nome` + the actual
        // count (0). Pinning the typed shape so a future Display-format
        // tweak can't silently drop either of the two load-bearing
        // fields (which would regress the "feira verb whose error path
        // doesn't name the offending caixa" punch-list item the protocol
        // calls out — same shape every peer per-axis lift carries).
        let mut c = bare_servico();
        c.servicos = vec![];
        let err = require_single_servico(&c).unwrap_err();
        assert_eq!(err.nome, "hello-rio");
        assert_eq!(err.count, 0);
    }

    #[test]
    fn require_single_servico_rejects_multi_entry_list_with_typed_mismatch() {
        // The peer arm on the upper-bound axis: a Servico-kind caixa
        // with ≥ 2 `:servicos` entries fails the same gate, with the
        // typed view carrying the actual count (2). Both empty and
        // multi-entry lists land on the same [`ServicoCountMismatch`]
        // arm — the V0 contract requires *exactly* one entry, not
        // *at-least* one — so the single helper closes both directions
        // of the V0 invariant in one call site.
        let mut c = bare_servico();
        c.servicos = vec![
            "servicos/hello-rio.computeunit.yaml".into(),
            "servicos/extra.computeunit.yaml".into(),
        ];
        let err = require_single_servico(&c).unwrap_err();
        assert_eq!(err.nome, "hello-rio");
        assert_eq!(err.count, 2);
    }

    #[test]
    fn servico_count_mismatch_display_names_offending_caixa_nome() {
        // The Display impl is the load-bearing surface every renderer's
        // `#[error("{0}")] UnsupportedServicoCount(#[from]
        // ServicoCountMismatch)` arm prints through. Pinning the exact
        // rendered form so a future format change is a one-line edit +
        // a one-line test update, not a silent regression of the
        // diagnostic clarity that motivated the lift (the prior
        // per-renderer `UnsupportedServicoCount(usize)` arm named only
        // the count). Same shape every peer [`KindMismatch`] / typed-
        // view Display tests pin.
        let err = ServicoCountMismatch {
            nome: "checkout".into(),
            count: 3,
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("checkout"),
            "Display must name the offending caixa nome (got: {msg:?})"
        );
        assert!(
            msg.contains('3'),
            "Display must name the actual count (got: {msg:?})"
        );
        assert!(
            msg.contains(":servicos"),
            "Display must name the offending field axis (got: {msg:?})"
        );
        assert!(
            msg.contains("exactly one"),
            "Display must name the V0 invariant (got: {msg:?})"
        );
    }

    #[test]
    fn overlay_kind_agnostic_for_field_projection() {
        // The helper projects fields, not kind — every Caixa carries
        // the M2 slot fields by construction. Renderer-level kind
        // gates (NotAServico in caixa-helm / caixa-flux) are the
        // shape filter; this helper is the field projector. Keeping
        // them separate means the same overlay can apply to any
        // future per-kind renderer (e.g. when M2.4 supervisor
        // rendering acquires its own M2-shaped overlay path).
        let mut c = bare_servico();
        c.kind = CaixaKind::Biblioteca;
        c.servicos = vec![];
        c.limits = Some(LimitsSpec {
            memory: Some(crate::LIMITS_MEMORY_WASM32_PAGE_BYTES),
            ..Default::default()
        });
        let overlay = servico_m2_overlay(&c).unwrap();
        assert!(overlay.contains_key(M2_KEY_LIMITS));
    }

    // ── single_field_overlay — typed per-axis overlay primitive ──────────

    #[test]
    fn single_field_overlay_none_yields_none() {
        // Empty-axis-skip semantic at the typed-primitive layer: a
        // `None` slot returns `None`, not `Some(empty Mapping)`. The
        // caller's `if let Some(overlay) = …` guard then becomes the
        // single emission gate, and a malformed `outer: {}` (the
        // empty-mapping form some K8s parsers reject) is structurally
        // impossible by construction.
        let v: Option<serde_yaml::Value> = single_field_overlay::<u32, _>(None, "attempts", |n| {
            serde_yaml::Value::Number(n.into())
        });
        assert!(v.is_none());
    }

    #[test]
    fn single_field_overlay_some_yields_single_field_mapping() {
        // The Some arm builds exactly one inner key/value pair, no
        // more, no less. Pinning the shape so a future refactor can't
        // accidentally introduce a second field (which would render
        // as a malformed `timeouts: { request: "30s", <leak>: ... }`
        // overlay block).
        let v = single_field_overlay(Some(30u32), "attempts", |n| {
            serde_yaml::Value::Number(n.into())
        })
        .expect("Some arm yields Some(...)");
        let m = v.as_mapping().expect("mapping shape");
        assert_eq!(m.len(), 1);
        assert_eq!(
            m.get(serde_yaml::Value::String("attempts".into()))
                .and_then(|x| x.as_u64()),
            Some(30)
        );
    }

    #[test]
    fn single_field_overlay_threads_typed_value_through_closure() {
        // The closure receives the unwrapped typed `T` (not the
        // wrapping `Option<T>`), so the per-overlay value-shaping
        // logic stays at the call site. Three different Value shapes
        // pin the closure's type-flow: a `String` (for canonical
        // duration / enum scalars), a `Number` (for typed integer
        // attempt counts), and a derived `Bool` (for tristate enums).
        // Mirrors the three landed overlays' shapes letter-for-letter.
        let dur = single_field_overlay(Some("30s".to_string()), "request", |s| {
            serde_yaml::Value::String(s)
        })
        .unwrap();
        assert_eq!(dur.get("request").and_then(|v| v.as_str()), Some("30s"));

        let num = single_field_overlay(Some(3u32), "attempts", |n| {
            serde_yaml::Value::Number(n.into())
        })
        .unwrap();
        assert_eq!(num.get("attempts").and_then(|v| v.as_u64()), Some(3));

        // The mtls tristate's two non-None arms map to enum strings,
        // not raw bools (the Cilium CRD's `mode: required|disabled`
        // shape — pinned end-to-end at every emit site by the
        // `cnp_authentication_mode_serialized_as_yaml_string` test).
        let mode = single_field_overlay(Some(true), "mode", |b| {
            serde_yaml::Value::String(if b { "required" } else { "disabled" }.into())
        })
        .unwrap();
        assert_eq!(mode.get("mode").and_then(|v| v.as_str()), Some("required"));
    }

    #[test]
    fn single_field_overlay_outer_key_is_callers_concern() {
        // The helper builds the *inner* (single-field) Mapping; the
        // *outer* key (`timeouts` / `retry` / `authentication`) is
        // the caller's `if let Some(overlay) = … { rule.insert(<outer>,
        // overlay.clone()) }` insertion. Pinning that the helper's
        // returned Value carries no outer-key wrapping — emitting the
        // outer-key-wrapped form here would silently double-wrap
        // every overlay (`timeouts: { timeouts: { request: "30s" } }`
        // post-insertion).
        let v = single_field_overlay(Some(30u32), "attempts", |n| {
            serde_yaml::Value::Number(n.into())
        })
        .unwrap();
        let m = v.as_mapping().unwrap();
        // Only the inner key — no `timeouts:` / `retry:` /
        // `authentication:` wrapper at this layer.
        for k in ["timeouts", "retry", "authentication"] {
            assert!(
                m.get(serde_yaml::Value::String(k.into())).is_none(),
                "single_field_overlay must not pre-insert the outer key {k:?} \
                 (the caller's per-rule insert is the canonical insertion site)"
            );
        }
    }

    #[test]
    fn single_field_overlay_value_is_clonable_for_per_rule_dispatch() {
        // The build-once-clone-many idiom every emit-site uses: the
        // overlay is computed once per renderer call (so the closure
        // runs exactly once) and `.clone()`d into each rule of the
        // emitted sequence. Pin that the returned Value is in fact
        // cloneable (a `serde_yaml::Value` always is, but the test
        // pins the contract end-to-end so a future refactor that
        // returns a non-Cloneable wrapper surfaces here).
        let v = single_field_overlay(Some(30u32), "attempts", |n| {
            serde_yaml::Value::Number(n.into())
        })
        .unwrap();
        let v_clone = v.clone();
        assert_eq!(v, v_clone);
    }

    // ── is_dns_1123_label — shared DNS-1123 label predicate ──────────────

    #[test]
    fn dns_1123_label_accepts_canonical_forms() {
        // Substrate-side pin: the predicate accepts the same canonical
        // shapes its three caller axes (`:membros :caixa`,
        // `:placement :clusters`, `:children :caixa`) accept at their own
        // gates. Drift between this list and the per-axis positive-set
        // sweeps surfaces here — one source of truth for the rule.
        for s in [
            "worker",
            "a",
            "0",
            "cache-v2",
            "payment-retry",
            "2-pool",
            "mar-east",
        ] {
            is_dns_1123_label(s)
                .unwrap_or_else(|e| panic!("canonical DNS-1123 label {s:?} must pass: {e:?}"));
        }
    }

    #[test]
    fn dns_1123_label_rejects_uppercase_with_lower_suggestion() {
        // The diagnostic carries the lower-cased fix verbatim so every
        // caller's per-axis `*Invalid { reason }` wrapping the predicate's
        // output reads back as a one-edit-fix suggestion. Pinned at the
        // substrate layer so the suggestion shape lives in one place.
        let err = is_dns_1123_label("Rio").unwrap_err();
        assert!(err.contains("uppercase"), "got: {err:?}");
        assert!(err.contains("\"rio\""), "got: {err:?}");
    }

    #[test]
    fn dns_1123_label_rejects_at_64_byte_boundary() {
        // The 63-byte cap pin — both the boundary-exceeding case and
        // the boundary-accepting case in one place, so a future cap
        // shift surfaces both arms simultaneously.
        let max_ok = "a".repeat(63);
        is_dns_1123_label(&max_ok).unwrap();
        let too_long = "a".repeat(64);
        let err = is_dns_1123_label(&too_long).unwrap_err();
        assert!(err.contains("63"), "got: {err:?}");
        assert!(err.contains("64"), "got: {err:?}");
    }

    // ── is_gateway_api_http_path — shared HTTP-path predicate ────────────

    #[test]
    fn gateway_api_http_path_accepts_canonical_forms() {
        // Substrate-side pin: the predicate accepts the same canonical
        // shapes both caller axes (`:entrada :paths` and `:contratos
        // :endpoint`) accept at their own gates. Drift between this
        // list and the per-axis positive-set sweeps surfaces here —
        // one source of truth for the rule. Includes the bare-root
        // `/` (the catch-all both renderers fall back to), the
        // `/foo..bar` interior-`..`-substring (not a `..` segment),
        // the `/...` and `/foo.` `.`-bearing names (not `.` segments),
        // and the percent-encoded form.
        for p in [
            "/",
            "/api/cart",
            "/healthz",
            "/api/.config",
            "/v1/products",
            "/products/:id",
            "/api/cart/",
            "/api/caf%C3%A9",
            "/foo..bar",
            "/...",
            "/charge",
        ] {
            is_gateway_api_http_path(p)
                .unwrap_or_else(|e| panic!("canonical HTTP path {p:?} must pass: {e:?}"));
        }
    }

    #[test]
    fn gateway_api_http_path_rejects_each_arm_with_substring_pinned_reason() {
        // Substrate-side diagnostic-shape pin: each grammar arm
        // surfaces its own distinct reason substring. Pinned here so
        // a future reason-wording rephrase that drops any of these
        // substrings surfaces at this one place, not piecemeal across
        // every per-axis test sweep.
        for (path, needle) in [
            ("/api?q=1", "must not contain `?`"),
            ("/api#frag", "must not contain `#`"),
            ("/api my", "whitespace"),
            ("/api\x01x", "control character"),
            ("/api/café", "non-ASCII"),
            ("/api//x", "consecutive `/`"),
            ("/api/./x", "`.` segment"),
            ("/api/../x", "`..` parent-segment"),
        ] {
            let err = is_gateway_api_http_path(path)
                .err()
                .unwrap_or_else(|| panic!("path {path:?} must be rejected"));
            assert!(
                err.contains(needle),
                "path {path:?} reason must contain {needle:?}; got {err:?}"
            );
        }
    }

    #[test]
    fn gateway_api_http_path_rejects_at_1025_byte_boundary() {
        // The 1024-byte cap pin — both the boundary-exceeding case and
        // the boundary-accepting case in one place, so a future cap
        // shift surfaces both arms simultaneously, mirroring
        // `dns_1123_label_rejects_at_64_byte_boundary` on the peer
        // predicate.
        let max_ok = format!("/{}", "a".repeat(1023));
        assert_eq!(max_ok.len(), 1024);
        is_gateway_api_http_path(&max_ok).unwrap();
        let too_long = format!("/{}", "a".repeat(1024));
        assert_eq!(too_long.len(), 1025);
        let err = is_gateway_api_http_path(&too_long).unwrap_err();
        assert!(err.contains("1024"), "got: {err:?}");
        assert!(err.contains("1025"), "got: {err:?}");
    }

    #[test]
    fn gateway_api_http_path_rejects_empty_defensively() {
        // The predicate is called only after each caller's narrower
        // `*Empty` arm has fired; re-checking here keeps the predicate
        // usable from any future call site without an empty-precondition
        // footgun, and avoids a panic on `bytes[0]`-style indexing if
        // a future arm is added. Same defensive empty-check
        // `validate_entrada_path` carries at its call site (55410e4).
        let err = is_gateway_api_http_path("").unwrap_err();
        assert!(err.contains("empty"), "got: {err:?}");
    }

    #[test]
    fn gateway_api_http_path_rejects_not_absolute_defensively() {
        // Defensive re-check of the leading-`/` invariant the per-axis
        // call site enforces with its own narrower `*NotAbsolute` arm;
        // ensures the predicate is callable from any future call site
        // without a shape-mismatch footgun.
        let err = is_gateway_api_http_path("api/cart").unwrap_err();
        assert!(err.contains('/'), "got: {err:?}");
    }

    #[test]
    fn gateway_api_http_path_rejects_every_reserved_printable_ascii_byte() {
        // Substrate-side sweep: every one of the eleven printable-ASCII
        // bytes outside the K8s Gateway API HTTPPathMatch.value
        // apiserver-side OpenAPI regex
        // `^(?:[-A-Za-z0-9/._~!$&'()*+,;=:@]|[%][0-9a-fA-F]{2})+$`
        // accepted set surfaces a self-locating reason naming the
        // offending byte verbatim plus the canonical `%XX` percent-
        // encoding remediation. RFC 3986 §3.3's `pchar = unreserved /
        // pct-encoded / sub-delims / ":" / "@"` grammar excludes these
        // bytes from every path segment, so the apiserver rejects them
        // at admission time on every
        // `HTTPRoute.spec.rules[].matches[].path.value` landing site —
        // peer with the `?` / `#` / whitespace / control / non-ASCII
        // arms `gateway_api_http_path_rejects_each_arm_with_substring_
        // pinned_reason` covers.
        //
        // Each char surfaces in a path-shape that pins the canonical
        // authoring footgun the K8s apiserver would otherwise catch
        // far from the caixa.lisp: `{id}` / `[0]` / `<placeholder>`
        // template forms, the Windows path-separator typo, the
        // shell-regex character footgun, the SQL-string-literal /
        // YAML-flow-mapping accidents.
        for (path, ch) in [
            ("/api/cart\"path", '"'),
            ("/api/cart<id>", '<'),
            ("/api/cart/<id>", '<'),
            ("/api/cart[0]", '['),
            ("/api/cart\\path", '\\'),
            ("/api/cart]", ']'),
            ("/api/cart/^foo", '^'),
            ("/api/cart/`foo", '`'),
            ("/api/cart/{id}", '{'),
            ("/api/cart|alt", '|'),
            ("/api/cart}", '}'),
        ] {
            let err = is_gateway_api_http_path(path)
                .err()
                .unwrap_or_else(|| panic!("path {path:?} must be rejected"));
            assert!(
                err.contains("reserved character"),
                "path {path:?} reason must name the reserved-character axis; got {err:?}"
            );
            assert!(
                err.contains(&format!("{ch:?}")),
                "path {path:?} reason must name the offending byte {ch:?} verbatim; got {err:?}"
            );
            let hex = format!("%{:02X}", ch as u8);
            assert!(
                err.contains(&hex),
                "path {path:?} reason must surface the canonical {hex:?} percent-encoding \
                 remediation; got {err:?}"
            );
        }
    }

    #[test]
    fn gateway_api_http_path_reserved_char_arm_fires_before_consecutive_slash() {
        // Precedence pin: the per-byte loop runs before the post-loop
        // structural arms (`//`, `/./`, `/../`), so a path that is
        // *both* reserved-char-bearing and consecutive-`/`-bearing
        // surfaces the more self-locating reserved-character diagnostic
        // first, naming the offending byte verbatim. Mirrors the
        // existing `?` / `#` / whitespace / control / non-ASCII arms'
        // implicit precedence the
        // `gateway_api_http_path_rejects_each_arm_with_substring_
        // pinned_reason` pin already establishes for the peer per-byte
        // shapes.
        let err = is_gateway_api_http_path("/api/{id}//x").unwrap_err();
        assert!(
            err.contains("reserved character") && err.contains("'{'"),
            "got: {err:?}"
        );
        assert!(
            !err.contains("consecutive"),
            "the reserved-char arm must fire before the consecutive-`/` arm; got: {err:?}"
        );
    }

    #[test]
    fn gateway_api_http_path_accepts_percent_encoded_reserved_chars() {
        // Positive-control complement to the reserved-byte rejection
        // sweep: every one of the eleven reserved printable-ASCII bytes
        // is admissible *when* properly percent-encoded, matching the
        // canonical Gateway API HTTPPathMatch.value apiserver-side
        // OpenAPI regex's `[%][0-9a-fA-F]{2}` alternative. Pins the
        // canonical remediation pathway the reserved-byte arm's reason
        // wording names — author who carries a literal `{` percent-
        // encodes as `%7B` and the typed slot accepts.
        for path in [
            "/api/cart%22path",
            "/api/cart%3Cid%3E",
            "/api/cart%5B0%5D",
            "/api/cart%5Cpath",
            "/api/cart/%5Efoo",
            "/api/cart/%60foo",
            "/api/cart/%7Bid%7D",
            "/api/cart%7Calt",
        ] {
            is_gateway_api_http_path(path)
                .unwrap_or_else(|e| panic!("percent-encoded path {path:?} must pass: {e:?}"));
        }
    }

    // ── is_wit_world_ref — shared WIT world-reference predicate ──────────

    #[test]
    fn wit_world_ref_accepts_canonical_forms() {
        // Substrate-side pin: the predicate accepts every canonical
        // WIT identifier the `:contratos :wit` axis already carries in
        // the test fixtures + the example checkout-aplicacao (each
        // hand-curated to match real WIT registry references). Drift
        // between this list and the per-axis positive-set sweep
        // surfaces here — one source of truth for the rule. Includes
        // every shape variant: HTTP-prefixed (`wasi:http/proxy`),
        // KV-prefixed (`wasi:keyvalue/store`), pubsub-prefixed
        // (`nats:pub-sub`, `kafka:topic`), capability-only
        // (`custom:exchange`, `pleme:cap/audit`), the optional
        // `@<version>` suffix (`wasi:http/proxy@0.2.0`), and the
        // multi-segment `/iface/iface` form the WIT IDL grammar allows.
        for s in [
            "wasi:http/proxy",
            "wasi:keyvalue/store",
            "nats:pub-sub",
            "kafka:topic",
            "custom:exchange",
            "pleme:cap/audit",
            "http:server",
            "kv:store",
            "wasi:http/proxy@0.2.0",
            "wasi:keyvalue/store@0.2.0-rc.1",
            "pleme:cap/audit/v2",
        ] {
            is_wit_world_ref(s)
                .unwrap_or_else(|e| panic!("canonical WIT reference {s:?} must pass: {e:?}"));
        }
    }

    #[test]
    fn wit_world_ref_rejects_each_arm_with_substring_pinned_reason() {
        // Substrate-side diagnostic-shape pin: each grammar arm
        // surfaces its own distinct reason substring. Pinned here so a
        // future reason-wording rephrase that drops any of these
        // substrings surfaces at this one place, not piecemeal across
        // every per-axis test sweep. Mirrors
        // `gateway_api_http_path_rejects_each_arm_with_substring_pinned_reason`
        // on the peer predicate.
        for (s, needle) in [
            // Missing `:` separator → silent capability demotion.
            ("wasi-http/proxy", "must contain a `:`"),
            // Multiple `:` → can't split into ns + pkg.
            ("wasi:http:proxy", "exactly one `:`"),
            // Uppercase → silently bypasses the lowercase dispatch.
            ("WASI:http/proxy", "lowercase"),
            ("wasi:HTTP/proxy", "lowercase"),
            // Empty package half → can't resolve via WIT registry.
            ("wasi:", "must not be empty"),
            // Empty namespace half.
            (":http/proxy", "must not be empty"),
            // Underscore → DNS-1123 / WIT kebab-case footgun.
            ("wasi:http_proxy", "_"),
            // Leading digit → WIT identifiers begin with a letter.
            ("wasi:1http/proxy", "digit"),
            // Consecutive hyphens → invalid kebab-case.
            ("wasi:pub--sub", "consecutive `-`"),
            // Trailing hyphen → invalid kebab-case.
            ("wasi:proxy-", "must not end with `-`"),
            // Whitespace inside the token.
            ("wasi:http proxy", "whitespace"),
            // Control characters.
            ("wasi:http\x01proxy", "control character"),
            // Non-ASCII byte (café-style un-percent-encoded literal).
            ("wasi:caf\u{e9}/proxy", "non-ASCII"),
            // Trailing `@` with no version body.
            ("wasi:http/proxy@", "trailing `@`"),
            // Version body carrying `:` or `/`.
            ("wasi:http/proxy@0.2:rc1", "must not contain `:` or `/`"),
            // Doubled `@`.
            ("wasi:http/proxy@0.2@beta", "at most one `@`"),
        ] {
            let err = is_wit_world_ref(s)
                .err()
                .unwrap_or_else(|| panic!("WIT reference {s:?} must be rejected"));
            assert!(
                err.contains(needle),
                "WIT reference {s:?} reason must contain {needle:?}; got {err:?}"
            );
        }
    }

    #[test]
    fn wit_world_ref_rejects_empty_defensively() {
        // The predicate is called from `WitContract::target()` only
        // after the per-axis `EmptyWit` arm has fired at validate
        // time; re-checking here keeps the predicate usable from any
        // future call site without an empty-precondition footgun.
        // Same defensive empty-check `is_dns_1123_label` /
        // `is_gateway_api_http_path` carry at their call sites.
        let err = is_wit_world_ref("").unwrap_err();
        assert!(err.contains("empty"), "got: {err:?}");
    }

    #[test]
    fn wit_world_ref_rejects_at_129_byte_boundary() {
        // The 128-byte cap pin — both the boundary-exceeding case and
        // the boundary-accepting case in one place, so a future cap
        // shift surfaces both arms simultaneously, mirroring
        // `dns_1123_label_rejects_at_64_byte_boundary` and
        // `gateway_api_http_path_rejects_at_1025_byte_boundary` on the
        // peer predicates. Constructed as `wasi:<long-pkg>` so the
        // kebab-shape arms don't fire first and obscure the cap arm.
        let pad = "a".repeat(123); // 5 + 123 = 128 (`wasi:` + pad)
        let max_ok = format!("wasi:{pad}");
        assert_eq!(max_ok.len(), 128);
        is_wit_world_ref(&max_ok).unwrap();
        let pad_over = "a".repeat(124);
        let too_long = format!("wasi:{pad_over}");
        assert_eq!(too_long.len(), 129);
        let err = is_wit_world_ref(&too_long).unwrap_err();
        assert!(err.contains("128"), "got: {err:?}");
        assert!(err.contains("129"), "got: {err:?}");
    }

    // ── is_nats_subject — shared NATS subject predicate ──────────────────

    #[test]
    fn nats_subject_accepts_canonical_forms() {
        // Substrate-side pin: the predicate accepts every canonical
        // NATS subject the `:contratos :subject` axis carries in the
        // caixa-mesh test fixtures + the example checkout-aplicacao
        // (each hand-curated to match real NATS server-side admission
        // shapes). Drift between this list and the per-axis positive-
        // set sweep surfaces here — one source of truth for the rule.
        // Includes single-token subjects, multi-dot subjects, snake-
        // case + kebab-case tokens (NATS accepts both), digit-bearing
        // tokens, the `*` single-token wildcard at every segment
        // position, and the `>` multi-token wildcard at the final
        // position (the two NATS subscription patterns the protocol
        // defines). Mirrors the canonical-forms sweeps on the peer
        // value-shape predicates (`gateway_api_http_path_accepts_…`,
        // `wit_world_ref_accepts_…`).
        for s in [
            "checkout.events.charge.failed",
            "rio.events.order.charged",
            "orders",
            "orders.123",
            "snake_case.token",
            "kebab-case.token",
            "MixedCase.Token",
            "alpha.beta.gamma.delta.epsilon",
            "orders.*.charged",
            "*.events.*",
            "orders.>",
            "*",
            ">",
        ] {
            is_nats_subject(s)
                .unwrap_or_else(|e| panic!("canonical NATS subject {s:?} must pass: {e:?}"));
        }
    }

    #[test]
    fn nats_subject_rejects_each_arm_with_substring_pinned_reason() {
        // Substrate-side diagnostic-shape pin: each grammar arm
        // surfaces its own distinct reason substring. Pinned here so
        // a future reason-wording rephrase that drops any of these
        // substrings surfaces at this one place, not piecemeal across
        // every per-axis test sweep. Mirrors
        // `gateway_api_http_path_rejects_each_arm_with_substring_pinned_reason`
        // and `wit_world_ref_rejects_each_arm_with_substring_pinned_reason`
        // on the peer predicates.
        for (s, needle) in [
            // Whitespace inside the token.
            ("foo bar", "whitespace"),
            ("foo\tbar", "whitespace"),
            // Control characters.
            ("foo\x01bar", "control character"),
            // Non-ASCII byte (un-percent-encoded café-style literal).
            ("foo.caf\u{e9}", "non-ASCII"),
            // Leading `.` — empty leading token.
            (".foo", "must not start with `.`"),
            // Trailing `.` — empty trailing token.
            ("foo.", "must not end with `.`"),
            // Consecutive `.` — empty token between separators.
            ("foo..bar", "consecutive `.`"),
            // Non-trailing `>` multi-token wildcard.
            ("foo.>.bar", "only allowed as the final segment"),
            // Mid-segment `*` (not a standalone wildcard token).
            ("foo*.bar", "`*` mid-segment"),
            // Mid-segment `>` (not a standalone wildcard token).
            ("foo>", "`>` mid-segment"),
            // `.` is the separator, so `,` (or any other punctuation)
            // surfaces as an invalid-character arm.
            ("foo,bar", "invalid character"),
            // `:` reserved-looking — distinct invalid-character arm
            // (pinned separately so a future relaxation that accepts
            // `:` mid-segment surfaces here, not in some downstream
            // renderer's "this passed validate but the NATS server
            // rejected at publish" footgun).
            ("foo:bar", "invalid character"),
        ] {
            let err = is_nats_subject(s)
                .err()
                .unwrap_or_else(|| panic!("NATS subject {s:?} must be rejected"));
            assert!(
                err.contains(needle),
                "NATS subject {s:?} reason must contain {needle:?}; got {err:?}"
            );
        }
    }

    #[test]
    fn nats_subject_rejects_empty_defensively() {
        // The predicate is called from `WitContract::target()` only
        // after the per-axis `ContratoSubjectEmpty` arm has fired at
        // validate time; re-checking here keeps the predicate usable
        // from any future call site without an empty-precondition
        // footgun. Same defensive empty-check `is_dns_1123_label`,
        // `is_gateway_api_http_path`, and `is_wit_world_ref` carry at
        // their call sites.
        let err = is_nats_subject("").unwrap_err();
        assert!(err.contains("empty"), "got: {err:?}");
    }

    #[test]
    fn nats_subject_rejects_at_257_byte_boundary() {
        // The 256-byte cap pin — both the boundary-exceeding case and
        // the boundary-accepting case in one place, so a future cap
        // shift surfaces both arms simultaneously, mirroring
        // `dns_1123_label_rejects_at_64_byte_boundary`,
        // `gateway_api_http_path_rejects_at_1025_byte_boundary`, and
        // `wit_world_ref_rejects_at_129_byte_boundary` on the peer
        // predicates. Constructed as a single all-`a` token (no `.`)
        // so the segment / wildcard arms don't fire first and obscure
        // the cap arm.
        let max_ok = "a".repeat(256);
        assert_eq!(max_ok.len(), 256);
        is_nats_subject(&max_ok).unwrap();
        let too_long = "a".repeat(257);
        assert_eq!(too_long.len(), 257);
        let err = is_nats_subject(&too_long).unwrap_err();
        assert!(err.contains("256"), "got: {err:?}");
        assert!(err.contains("257"), "got: {err:?}");
    }

    #[test]
    fn nats_subject_lone_wildcard_tokens_validate() {
        // The two NATS wildcards stand alone as the entire subject —
        // a `subscribe("*")` matches any single-token publish, a
        // `subscribe(">")` matches every NATS message on the connection.
        // Both are protocol-legal; the typed substrate accepts them
        // structurally and leaves the "should the typed `:contratos`
        // edge subscribe to literally everything?" question to a
        // future semantic-level gate. Pinned alongside the canonical-
        // forms sweep so a future tighten that disallows lone wildcards
        // surfaces both arms simultaneously.
        is_nats_subject("*").unwrap();
        is_nats_subject(">").unwrap();
    }

    #[test]
    fn nats_subject_trailing_multi_wildcard_validates() {
        // `>` at the final segment is the canonical "match all trailing
        // tokens" subscription pattern. Pinned alongside the non-
        // trailing-`>` rejection arm so the boundary between the two
        // is in one place — a future relaxation that allows `>` at
        // non-trailing positions or a tighten that disallows trailing
        // `>` surfaces both arms simultaneously.
        is_nats_subject("orders.>").unwrap();
        is_nats_subject("orders.events.>").unwrap();
        // And the `*` single-token wildcard combines freely with the
        // trailing `>` — the canonical "match one middle token, then
        // anything trailing" subscription pattern.
        is_nats_subject("orders.*.>").unwrap();
    }

    // ── is_wasi_keyvalue_slot — shared kv slot-template predicate ────────

    #[test]
    fn wasi_kv_slot_accepts_canonical_forms() {
        // Substrate-side pin: the predicate accepts every canonical kv
        // slot template the `:contratos :slot` axis carries in the
        // caixa-mesh test fixtures + plausible authoring patterns
        // (each maps to a realistic wasi:keyvalue/store key the runtime
        // resolves on dispatch). Drift between this list and the
        // per-axis positive-set sweep surfaces here — one source of
        // truth for the rule. Includes:
        //   - single-token identifiers (`"checkout"`, `"events"`);
        //   - dot-namespaced templates (`"session.tokens.<sid>"`);
        //   - path-namespaced templates with `$`-prefixed variables
        //     (`"checkout/$orderId"`, the canonical Akka-cluster-
        //     sharding-style template);
        //   - colon-namespaced templates with brace placeholders
        //     (`"users:{tenant}/{id}"`, the canonical multi-tenant
        //     Redis-key shape);
        //   - angle-bracket placeholders (`"session.<sid>"`);
        //   - underscore identifiers (`"snake_case_key"`);
        //   - kebab identifiers (`"kebab-case-key"`);
        //   - mixed-case (`"MixedCase"` — kv slot templates are case-
        //     sensitive; the predicate doesn't lowercase-fold);
        //   - digit-bearing tokens (`"shard0"`, `"v2/key"`);
        //   - percent-encoded fragments (`"users/caf%C3%A9"`); the
        //     encoded form is the *valid* shape, the raw `café` is
        //     rejected on the non-ASCII arm.
        // Mirrors the canonical-forms sweeps on the peer value-shape
        // predicates (`gateway_api_http_path_accepts_…`,
        // `nats_subject_accepts_canonical_forms`).
        for s in [
            "checkout",
            "events",
            "checkout/$orderId",
            "users:{tenant}/{id}",
            "session.<sid>",
            "session.tokens.<sid>",
            "snake_case_key",
            "kebab-case-key",
            "MixedCase",
            "shard0",
            "v2/key",
            "users/caf%C3%A9",
        ] {
            is_wasi_keyvalue_slot(s)
                .unwrap_or_else(|e| panic!("canonical kv slot {s:?} must pass: {e:?}"));
        }
    }

    #[test]
    fn wasi_kv_slot_rejects_each_arm_with_substring_pinned_reason() {
        // Substrate-side diagnostic-shape pin: each grammar arm
        // surfaces its own distinct reason substring. Pinned here so
        // a future reason-wording rephrase that drops any of these
        // substrings surfaces at this one place, not piecemeal across
        // every per-axis test sweep. Mirrors
        // `nats_subject_rejects_each_arm_with_substring_pinned_reason`
        // and `gateway_api_http_path_rejects_each_arm_with_substring_pinned_reason`
        // on the peer predicates.
        for (s, needle) in [
            // Raw space inside the template — the canonical paste-from-
            // doc footgun.
            ("check out/$order", "whitespace"),
            // Tab byte — distinct arm-pinned reason from the space arm.
            ("check\tout", "whitespace"),
            // Control character (SOH = 0x01) — pinned separately from
            // the whitespace arm so a future relaxation that admits
            // raw whitespace but still rejects controls surfaces here.
            ("checkout/\x01order", "control character"),
            // Newline — the canonical "the paste-from-binary slug
            // spans multiple lines" footgun. Distinct from the
            // whitespace arm because `\n` is a control character.
            ("checkout\norder", "control character"),
            // DEL byte (0x7F) — the upper boundary of the control-
            // character range, pinned so a future relaxation that
            // only checks `< 0x20` surfaces here.
            ("checkout\x7forder", "control character"),
            // Un-percent-encoded non-ASCII byte — the canonical
            // "I copied the key from a doc with smart quotes /
            // accented characters" footgun. Author must percent-
            // encode (the canonical-forms sweep covers
            // `"users/caf%C3%A9"`).
            ("ch\u{e9}ckout/$order", "non-ASCII"),
        ] {
            let err = is_wasi_keyvalue_slot(s)
                .err()
                .unwrap_or_else(|| panic!("kv slot {s:?} must be rejected"));
            assert!(
                err.contains(needle),
                "kv slot {s:?} reason must contain {needle:?}; got {err:?}"
            );
        }
    }

    #[test]
    fn wasi_kv_slot_rejects_empty_defensively() {
        // The predicate is called from `WitContract::target()` only
        // after the per-axis `ContratoSlotEmpty` arm has fired at
        // validate time; re-checking here keeps the predicate usable
        // from any future call site without an empty-precondition
        // footgun. Same defensive empty-check `is_dns_1123_label`,
        // `is_gateway_api_http_path`, `is_wit_world_ref`, and
        // `is_nats_subject` carry at their call sites.
        let err = is_wasi_keyvalue_slot("").unwrap_err();
        assert!(err.contains("empty"), "got: {err:?}");
    }

    #[test]
    fn wasi_kv_slot_rejects_at_513_byte_boundary() {
        // The 512-byte cap pin — both the boundary-exceeding case and
        // the boundary-accepting case in one place, so a future cap
        // shift surfaces both arms simultaneously, mirroring
        // `dns_1123_label_rejects_at_64_byte_boundary`,
        // `gateway_api_http_path_rejects_at_1025_byte_boundary`,
        // `wit_world_ref_rejects_at_129_byte_boundary`, and
        // `nats_subject_rejects_at_257_byte_boundary` on the peer
        // predicates. Constructed as a single all-`a` token (no
        // separator / template syntax) so only the cap arm fires.
        let max_ok = "a".repeat(512);
        assert_eq!(max_ok.len(), 512);
        is_wasi_keyvalue_slot(&max_ok).unwrap();
        let too_long = "a".repeat(513);
        assert_eq!(too_long.len(), 513);
        let err = is_wasi_keyvalue_slot(&too_long).unwrap_err();
        assert!(err.contains("512"), "got: {err:?}");
        assert!(err.contains("513"), "got: {err:?}");
    }

    #[test]
    fn wasi_kv_slot_admits_full_printable_ascii_range() {
        // Structural pin: the predicate admits every printable ASCII
        // byte from `0x21` (`!`) to `0x7E` (`~`) inclusive, including
        // every template-variable bracket the documented authoring
        // patterns use (`$`, `{`, `}`, `<`, `>`) and every namespace
        // separator (`/`, `:`, `.`, `-`, `_`). Drift here = a future
        // tighten that removes any byte from the admitted set surfaces
        // a name-the-byte test failure, not piecemeal across per-axis
        // sweeps. Constructed as a single all-bytes template (`b!`,
        // `b"`, …, `b~`) — the predicate doesn't impose structure,
        // only character-class.
        for b in 0x21u8..=0x7E {
            let s = std::str::from_utf8(&[b]).unwrap().to_string();
            is_wasi_keyvalue_slot(&s)
                .unwrap_or_else(|e| panic!("printable ASCII byte 0x{b:02x} must pass: {e:?}"));
        }
    }

    #[test]
    fn git_ref_name_accepts_canonical_forms() {
        // Substrate-side pin: the predicate accepts every canonical
        // refname the `:fonte :tag` / `:fonte :branch` axes carry in
        // realistic authoring patterns (each maps to a refname `git
        // fetch <remote> tag '<value>'` and `git checkout '<value>'`
        // resolve cleanly at clone time). Drift between this list and
        // any per-axis positive-set sweep surfaces here — one source
        // of truth for the rule. Includes:
        //   - semver tag with `v` prefix (`"v0.1.0"`, the canonical
        //     pleme-io release shape);
        //   - bare semver tag (`"0.1.0"`, the npm / Cargo idiom);
        //   - pre-release tag (`"v0.1.0-alpha.1"`);
        //   - release-line tag with hyphens (`"release-1.0"`);
        //   - leaf branch (`"main"` / `"master"`);
        //   - hierarchical feature branch (`"feature/checkout"`);
        //   - multi-component branch with hyphens and digits
        //     (`"user-1/feat-x-v2"`);
        //   - dot-bearing tag (`"v0.1.0.rc1"`, mid-component dot
        //     allowed — only consecutive `..` and trailing `.` are
        //     rejected).
        // Mirrors the canonical-forms sweeps on the peer value-shape
        // predicates (`wasi_kv_slot_accepts_canonical_forms`,
        // `nats_subject_accepts_canonical_forms`).
        for s in [
            "v0.1.0",
            "0.1.0",
            "v0.1.0-alpha.1",
            "release-1.0",
            "main",
            "master",
            "feature/checkout",
            "user-1/feat-x-v2",
            "v0.1.0.rc1",
            "stable",
        ] {
            is_git_ref_name(s)
                .unwrap_or_else(|e| panic!("canonical git ref {s:?} must pass: {e:?}"));
        }
    }

    #[test]
    fn git_ref_name_rejects_each_arm_with_substring_pinned_reason() {
        // Substrate-side diagnostic-shape pin: each grammar arm
        // surfaces its own distinct reason substring. Pinned here so
        // a future reason-wording rephrase that drops any of these
        // substrings surfaces at this one place, not piecemeal across
        // every per-axis test sweep. Mirrors
        // `wasi_kv_slot_rejects_each_arm_with_substring_pinned_reason`
        // and `nats_subject_rejects_each_arm_with_substring_pinned_reason`
        // on the peer predicates.
        for (s, needle) in [
            // Trailing space — the canonical paste-from-doc footgun.
            ("v0.1.0 ", "whitespace"),
            // Embedded space (branch with spaces).
            ("feature/foo bar", "whitespace"),
            // Tab byte.
            ("v0.1.0\t", "whitespace"),
            // Newline — the canonical "paste-from-multiline-doc"
            // footgun. Distinct from the whitespace arm because `\n`
            // is a control character.
            ("v0.1.0\n", "control character"),
            // DEL byte (0x7F) — upper boundary of the control range.
            ("v0.1.0\x7f", "control character"),
            // Non-ASCII byte (the canonical "I copied the tag from a
            // doc with smart quotes" footgun).
            ("v0.1.0\u{e9}", "non-ASCII"),
            // Tilde — git's revision grammar (`HEAD~3`).
            ("v0.1.0~1", "`~`"),
            // Caret — git's revision grammar (`HEAD^`).
            ("v0.1.0^", "`^`"),
            // Colon — git's refspec separator.
            ("v0.1.0:rebase", "`:`"),
            // Question mark — git's refspec glob.
            ("v0.1.0?", "`?`"),
            // Asterisk — git's refspec glob.
            ("v0.1.*", "`*`"),
            // Open bracket — git's refspec glob.
            ("v0.1.0[1]", "`[`"),
            // Backslash — the canonical Windows-path-leak footgun.
            ("feature\\foo", "`\\`"),
            // Consecutive dots — git's `<rev1>..<rev2>` range grammar.
            ("v0.1..0", "`..`"),
            // Reflog grammar.
            ("main@{upstream}", "`@{`"),
            // The bare `@` — git aliases to `HEAD`.
            ("@", "bare `@`"),
            // Leading slash.
            ("/main", "begin with `/`"),
            // Trailing slash.
            ("feature/", "end with `/`"),
            // Consecutive slashes.
            ("feature//foo", "consecutive `/`"),
            // Trailing dot.
            ("v0.1.0.", "end with `.`"),
            // Fully-qualified branch ref — the canonical
            // `git show-ref`-output-leak footgun.
            ("refs/heads/main", "fully-qualified"),
            // Fully-qualified tag ref.
            ("refs/tags/v0.1.0", "fully-qualified"),
            // Component beginning with `.` (per-component rule).
            ("feature/.hidden", "begin with `.`"),
            // Component ending with `.lock` (per-component rule).
            ("feature/main.lock", "`.lock`"),
            // Leaf ref named `<x>.lock` — same per-component rule on
            // the single-component refname.
            ("main.lock", "`.lock`"),
            // Case-insensitive `.LOCK` — APFS / NTFS / HFS+ admit
            // both spellings as the same on-disk file, so a
            // `:tag "v1.LOCK"` collides with git's atomic-rename
            // guard on case-insensitive filesystems. Pinned
            // separately from the canonical lowercase arm so a
            // future relaxation that only catches lowercase
            // surfaces here.
            ("v1.LOCK", "`.lock`"),
            ("feature/Main.Lock", "`.lock`"),
        ] {
            let err = is_git_ref_name(s)
                .err()
                .unwrap_or_else(|| panic!("git ref {s:?} must be rejected"));
            assert!(
                err.contains(needle),
                "git ref {s:?} reason must contain {needle:?}; got {err:?}"
            );
        }
    }

    #[test]
    fn git_ref_name_rejects_empty_defensively() {
        // The predicate is called from `DepSource::validate` only
        // after the per-axis `FontePinEmpty` arm has fired at
        // validate time; re-checking here keeps the predicate usable
        // from any future call site without an empty-precondition
        // footgun. Same defensive empty-check `is_dns_1123_label`,
        // `is_gateway_api_http_path`, `is_wit_world_ref`,
        // `is_nats_subject`, and `is_wasi_keyvalue_slot` carry at
        // their call sites.
        let err = is_git_ref_name("").unwrap_err();
        assert!(err.contains("empty"), "got: {err:?}");
    }

    #[test]
    fn git_ref_name_rejects_at_256_byte_boundary() {
        // The 255-byte cap pin — both the boundary-exceeding case and
        // the boundary-accepting case in one place, so a future cap
        // shift surfaces both arms simultaneously, mirroring
        // `dns_1123_label_rejects_at_64_byte_boundary`,
        // `gateway_api_http_path_rejects_at_1025_byte_boundary`,
        // `wit_world_ref_rejects_at_129_byte_boundary`,
        // `nats_subject_rejects_at_257_byte_boundary`, and
        // `wasi_kv_slot_rejects_at_513_byte_boundary` on the peer
        // predicates. Constructed as a single all-`a` leaf so only
        // the cap arm fires.
        let max_ok = "a".repeat(255);
        assert_eq!(max_ok.len(), 255);
        is_git_ref_name(&max_ok).unwrap();
        let too_long = "a".repeat(256);
        assert_eq!(too_long.len(), 256);
        let err = is_git_ref_name(&too_long).unwrap_err();
        assert!(err.contains("255"), "got: {err:?}");
        assert!(err.contains("256"), "got: {err:?}");
    }

    #[test]
    fn git_ref_name_qualified_prefix_diagnostic_quotes_leaf() {
        // Diagnostic-shape pin: the `refs/heads/` / `refs/tags/`
        // rejection arm enumerates the leaf the author probably
        // meant, so the author's grep target is the *intended*
        // refname literal rather than the (rejected) qualified form.
        // Pinned across both prefixes so a future relaxation that
        // drops the leaf-suggestion surfaces here.
        for (qualified, leaf) in [
            ("refs/heads/main", "main"),
            ("refs/tags/v0.1.0", "v0.1.0"),
            ("refs/heads/feature/checkout", "feature/checkout"),
        ] {
            let err = is_git_ref_name(qualified).unwrap_err();
            assert!(
                err.contains(&format!("{leaf:?}")),
                "qualified ref {qualified:?} diagnostic must quote the leaf \
                 {leaf:?}; got {err:?}"
            );
        }
    }

    // ── is_git_ref_name canonical-OID-shape partition arm ────────────────

    #[test]
    fn git_ref_name_rejects_canonical_sha1_oid() {
        // The fail-before-pass-after pin on the canonical SHA-1 OID
        // partition arm: a 40-char lowercase-hex string is the shape
        // `is_git_oid` accepts, so `is_git_ref_name` must reject it.
        // Until this arm landed `is_git_ref_name` accepted every
        // 40-char lowercase-hex string (pure hex carries none of the
        // forbidden refname characters, no `..`/`@{`/`/`-prefix/
        // `/`-suffix/`.lock`-suffix/`refs/heads/`-prefix), silently
        // breaking the cross-axis partition the
        // [`DepSource::validate`] gate routes the `:fonte` axes
        // through and admitting `:tag "deadbeef…"` /
        // `:branch "deadbeef…"` as legitimate refnames — the
        // canonical paste-from-`git show --format=%H` mis-slot
        // footgun. The diagnostic names the `:rev` axis so the author
        // grep-fixes in one edit.
        for oid in [
            "0123456789abcdef0123456789abcdef01234567",
            "deadbeefcafebabe0123456789abcdef01234567",
            "ffffffffffffffffffffffffffffffffffffffff",
            "0000000000000000000000000000000000000000",
        ] {
            assert_eq!(oid.len(), GIT_OID_SHA1_LEN);
            let err = is_git_ref_name(oid).unwrap_err();
            assert!(
                err.contains("OID") && err.contains(":rev"),
                "canonical SHA-1 OID {oid:?} must surface a diagnostic \
                 naming OID + `:rev`; got {err:?}"
            );
            assert!(
                err.contains("SHA-1"),
                "canonical SHA-1 OID {oid:?} diagnostic must name the \
                 hash algorithm; got {err:?}"
            );
        }
    }

    #[test]
    fn git_ref_name_rejects_canonical_sha256_oid() {
        // The fail-before-pass-after pin on the canonical SHA-256 OID
        // partition arm — Git 2.42+ `extensions.objectFormat = sha256`
        // mode. 64-char lowercase-hex strings are equally OID-shaped
        // and must surface the same `:rev`-axis diagnostic. Pinned
        // separately from SHA-1 so a future relaxation that only
        // catches one width surfaces here.
        let sha256_zeros = "0".repeat(GIT_OID_SHA256_LEN);
        let sha256_ones = "f".repeat(GIT_OID_SHA256_LEN);
        let sha256_mixed = format!("deadbeefcafebabe{}", "0123456789abcdef".repeat(3));
        for oid in [&sha256_zeros, &sha256_ones, &sha256_mixed] {
            assert_eq!(oid.len(), GIT_OID_SHA256_LEN);
            let err = is_git_ref_name(oid).unwrap_err();
            assert!(
                err.contains("OID") && err.contains(":rev"),
                "canonical SHA-256 OID {oid:?} must surface a \
                 diagnostic naming OID + `:rev`; got {err:?}"
            );
            assert!(
                err.contains("SHA-256"),
                "canonical SHA-256 OID {oid:?} diagnostic must name \
                 the hash algorithm; got {err:?}"
            );
        }
    }

    #[test]
    fn git_ref_name_partition_excludes_off_by_one_lengths() {
        // Boundary pin: lengths that *aren't* exactly 40 or 64 hex
        // characters are NOT canonical OIDs, so the partition arm
        // must not fire — they remain accepted as refnames (consistent
        // with `is_git_oid` rejecting them on its exact-width check).
        // Abbreviated OIDs (`"c0ffee0"`, 7-char prefix) are ambiguous
        // across repository history and `is_git_oid` rejects them
        // separately, but they're legitimate refname shapes per `git
        // check-ref-format`, so `is_git_ref_name` accepts them here.
        // Pinned across the 39/41/63/65-char and abbreviated arms so
        // a future widening of the partition arm to "any hex-shaped
        // value" surfaces here as a regression rather than silently
        // rejecting valid refnames.
        for accept in [
            // 39 hex chars — one short of SHA-1 width.
            "0123456789abcdef0123456789abcdef0123456",
            // 41 hex chars — one over SHA-1 width.
            "0123456789abcdef0123456789abcdef012345670",
            // 63 hex chars — one short of SHA-256 width.
            &"a".repeat(63),
            // 65 hex chars — one over SHA-256 width.
            &"a".repeat(65),
            // Abbreviated 7-char SHA — the `git log --short` width.
            "c0ffee0",
            // Pure-numeric 8-char (looks vaguely SHA-shaped but
            // isn't canonical-width).
            "00000000",
        ] {
            is_git_ref_name(accept).unwrap_or_else(|e| {
                panic!(
                    "off-canonical-width hex-shaped value {accept:?} \
                     (len {len}) must still pass is_git_ref_name — \
                     the partition arm is exact-width 40/64, not a \
                     prefix or pattern: {e:?}",
                    len = accept.len()
                )
            });
        }
    }

    #[test]
    fn git_ref_name_partition_excludes_uppercase_canonical_widths() {
        // Boundary pin: the partition arm targets the canonical
        // *lowercase-hex* OID shape `git rev-parse HEAD` /
        // `git show --format=%H` emit. Uppercase or mixed-case
        // 40/64-char hex strings are legitimate refnames per
        // `git check-ref-format` (uppercase letters are admitted in
        // refnames), so `is_git_ref_name` accepts them here; the
        // `:rev` axis separately rejects uppercase OIDs via
        // [`is_git_oid`]'s lowercase-only contract — so neither
        // axis silently admits an uppercase-hex value cross-slot.
        // Pinned across both widths + both uppercase variants so a
        // future relaxation of either predicate surfaces here.
        for accept in [
            // Uppercase 40-char hex — passes is_git_ref_name (valid
            // refname), rejected by is_git_oid on lowercase contract.
            "DEADBEEFCAFEBABE0123456789ABCDEF01234567",
            // Mixed case 40-char hex.
            "DeadBeefCafeBabe0123456789abcdef01234567",
            // Uppercase 64-char hex.
            &"A".repeat(64),
        ] {
            is_git_ref_name(accept).unwrap_or_else(|e| {
                panic!(
                    "uppercase canonical-width hex value {accept:?} \
                     must still pass is_git_ref_name — the partition \
                     arm targets lowercase-canonical only (uppercase \
                     is a legitimate refname character per \
                     git-check-ref-format); the `:rev` axis catches \
                     uppercase via is_git_oid's lowercase contract: \
                     {e:?}"
                )
            });
            // And confirm is_git_oid rejects it on the lowercase arm
            // (so neither axis silently admits the value).
            let oid_err = is_git_oid(accept).unwrap_err();
            assert!(
                oid_err.contains("lowercase") || oid_err.contains("uppercase"),
                "uppercase hex value {accept:?} must be rejected by \
                 is_git_oid on its lowercase contract; got {oid_err:?}"
            );
        }
    }

    #[test]
    fn git_ref_name_partition_arm_fires_before_per_byte_scan() {
        // Order pin: the partition arm runs after the length check
        // but before the per-byte refname-character scan, so a
        // canonical-OID-shaped value surfaces the `:rev`-axis
        // diagnostic rather than (e.g.) falling through to a generic
        // per-component arm. Pinned via a canonical OID — pure hex
        // can't violate any of the per-byte / `..` / `@{` / `/` /
        // `.lock` / `refs/heads/` arms (which is precisely why the
        // partition arm is needed), so position-wise this pin
        // forecloses a future refactor that splits the partition arm
        // across the scan (where uppercase / mixed-case canonical-
        // width values would silently route through one branch).
        let oid = "0123456789abcdef0123456789abcdef01234567";
        let err = is_git_ref_name(oid).unwrap_err();
        // The diagnostic mentions OID + `:rev`; it does NOT contain
        // any of the per-byte-arm needle substrings the
        // `git_ref_name_rejects_each_arm_with_substring_pinned_reason`
        // sweep pins, structurally — canonical OIDs can't violate
        // those arms.
        assert!(err.contains("OID"), "got: {err:?}");
        assert!(err.contains(":rev"), "got: {err:?}");
    }

    #[test]
    fn git_ref_name_rejects_leading_hyphen_cli_arg_injection() {
        // The CLI-arg-injection arm pin on the `:tag` / `:branch` axis.
        // Git's `check-ref-format` grammar admits a leading `-` (the
        // byte is a legitimate kebab continuation), so every prior
        // shape arm passes the value through; the diagnostic moves
        // the gate to the subprocess-argument boundary the resolver
        // consumes. Pinned across the canonical CLI-arg-injection
        // shapes — short-flag-shaped `"-X"`, long-option-shaped
        // `"-stable"`, git-config-injection-shaped
        // `"-c=core.merge=ours"`, the canonical
        // `"--upload-pack=…"` long-flag form, and the
        // `"--config"`-shape repeat-arg form — every shape would
        // silently escape `git checkout --quiet --detach <ref>` (the
        // resolver's invocation in `caixa-resolver/src/git.rs:41`,
        // no `--` argument-list terminator) and get reinterpreted by
        // `git checkout`'s argument parser. Peer with the
        // `is_git_repo_url` leading-`-` arm (same vector on the
        // sibling `:repo` axis), `is_cargo_feature_name` leading-`-`
        // arm, and `is_dns_1123_label` leading-`-` arm — the
        // substrate-wide "no leading `-` anywhere in a typed
        // single-token string slot routed through a subprocess
        // argument" invariant is now structurally consistent across
        // every value-shape-gated typed surface.
        for s in [
            "-X",                     // short-flag-shape
            "-stable",                // long-option-shape
            "-c=core.merge=ours",     // git-config-injection-shape
            "--upload-pack=cat /etc", // long-flag with-value
            "--config",               // repeat-arg shape
            "-",                      // degenerate single-byte
        ] {
            let err = is_git_ref_name(s)
                .err()
                .unwrap_or_else(|| panic!("git ref {s:?} must be rejected"));
            assert!(
                err.contains("`-`"),
                "git ref {s:?} reason must surface the leading-`-` arm: {err:?}"
            );
            assert!(
                err.contains("CLI-argument-injection"),
                "git ref {s:?} reason must name the CLI-argument-injection \
                 vector: {err:?}"
            );
        }
        // Positive control: a mid-name `-` (the canonical kebab
        // separator) passes — `"v0-1-0"`, `"feature-x"`, `"main-2"`
        // — pinning that the arm only fires at the leading position,
        // not anywhere else.
        for s in ["v0-1-0", "feature-x", "main-2"] {
            is_git_ref_name(s).unwrap_or_else(|e| {
                panic!("mid-name `-` ref {s:?} must pass the leading-`-` arm: {e:?}")
            });
        }
    }

    #[test]
    fn git_ref_name_leading_hyphen_fires_before_per_byte_scan() {
        // Cascade-precedence pin: a `"-flag\n"` value carries both a
        // leading `-` and an embedded `\n` control byte; the leading-`-`
        // arm fires first (the byte sits at the leading position the
        // arm probes, before the per-byte cascade loop's control-byte
        // arm). Mirrors the order pin
        // `git_ref_name_partition_arm_fires_before_per_byte_scan`
        // establishes on the canonical-OID partition arm — both
        // pre-loop arms structurally precede the per-byte scan.
        let err = is_git_ref_name("-flag\n").unwrap_err();
        assert!(err.contains("`-`"), "got: {err:?}");
        assert!(
            !err.contains("control character"),
            "leading-`-` arm must fire before the control-byte per-byte arm: {err:?}"
        );
    }

    #[test]
    fn git_ref_name_leading_hyphen_fires_after_canonical_oid_partition() {
        // Cascade-precedence pin: the partition arm structurally
        // precedes the leading-`-` arm because a canonical OID shape
        // (40 / 64 lowercase hex bytes) cannot start with `-` — the
        // byte sets are disjoint, so the precedence pin is a no-op at
        // value level. The pin matters only at the diagnostic-shape
        // level — it ensures a future codec round-trip that
        // synthesizes a probe-as-both value (impossible today;
        // possible if the OID partition arm ever relaxes its byte
        // set) surfaces the more self-locating `:rev`-mis-slot
        // diagnostic rather than the broader CLI-arg-injection one.
        let oid = "0123456789abcdef0123456789abcdef01234567";
        let err = is_git_ref_name(oid).unwrap_err();
        assert!(err.contains("OID"), "got: {err:?}");
        assert!(
            !err.contains("CLI-argument-injection"),
            "OID partition arm must precede leading-`-` arm: {err:?}"
        );
    }

    // ── is_git_oid — `:fonte :rev` value-shape predicate ────────────────

    #[test]
    fn git_oid_canonical_widths_match_sha1_and_sha256() {
        // The single-source-of-truth pin on the two canonical widths.
        // Drift between the predicate's accepted widths and the const
        // values would surface here as a build error, not as a silent
        // round-trip break at the renderer layer. Mirrors
        // `wasm32_memory_cap_matches_parsed_4_gib` (9d49a3a) — the
        // constant equality pin keeps the contract one place.
        assert_eq!(GIT_OID_SHA1_LEN, 40);
        assert_eq!(GIT_OID_SHA256_LEN, 64);
        // Doubled width: SHA-256 is exactly twice SHA-1 in hex char
        // count (256 / 4 = 64; 160 / 4 = 40). Pinned so a future
        // hash-algorithm widening reads the relationship here.
        assert_eq!(GIT_OID_SHA256_LEN, GIT_OID_SHA1_LEN * 2 - 16);
    }

    #[test]
    fn git_oid_accepts_canonical_sha1() {
        // Positive control on the SHA-1 OID width: 40 lowercase hex
        // characters — the canonical `git rev-parse HEAD` emission
        // shape every realistic pleme-io upstream uses today. The all-
        // `f` boundary is the lexicographically-largest OID (a real
        // commit's hash could land here, and the predicate accepts it
        // because it's structurally a valid OID — the null-OID
        // sentinel arm partitions the all-`0` boundary only, not the
        // all-`f` one).
        is_git_oid("0123456789abcdef0123456789abcdef01234567").unwrap();
        is_git_oid("deadbeefcafebabe0123456789abcdef01234567").unwrap();
        is_git_oid("ffffffffffffffffffffffffffffffffffffffff").unwrap();
    }

    #[test]
    fn git_oid_accepts_canonical_sha256() {
        // Positive control on the SHA-256 OID width: 64 lowercase hex
        // characters — `git`'s `extensions.objectFormat = sha256`
        // emission (GA since Git 2.42 / Oct 2023). Doubled SHA-1 width.
        let sha256_one = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(sha256_one.len(), 64);
        is_git_oid(sha256_one).unwrap();
        let sha256_fs = "f".repeat(64);
        is_git_oid(&sha256_fs).unwrap();
    }

    #[test]
    fn git_oid_rejects_null_oid_sentinel_sha1() {
        // Canonical "I copy-pasted the no-such-commit sentinel out of
        // `git update-ref --stdin` docs / pre-receive hook example"
        // footgun on the SHA-1 width — the all-zero 40-char hex
        // string is git's `null OID` sentinel (used to indicate ref
        // create / delete in update-ref flows) and never names a real
        // commit in any repo's object database. Until the null-OID
        // arm landed it passed every other shape arm (canonical
        // length, lowercase hex) and surfaced at `git fetch <remote>
        // 0000…0000` time with a quoting-confused "couldn't find
        // remote ref" error far from the source caixa.lisp, with the
        // lacre's content-address locked to a `git:0000…0000` closure
        // that never equals any upstream's actual `HEAD`. The
        // diagnostic carries the `40` width verbatim so a future
        // SHA-256 fixture surfaces the same arm at the doubled width
        // boundary.
        let null_sha1 = "0".repeat(40);
        let err = is_git_oid(&null_sha1).unwrap_err();
        assert!(
            err.contains("null-OID sentinel"),
            "reason must name the sentinel: {err}",
        );
        assert!(err.contains("40"), "reason must name the width: {err}",);
        assert!(
            err.contains("no-such-commit") || err.contains("update-ref"),
            "reason must reference git's null-OID semantics: {err}",
        );
    }

    #[test]
    fn git_oid_rejects_null_oid_sentinel_sha256() {
        // Same sentinel on the SHA-256 width — `git`'s
        // `extensions.objectFormat = sha256` mode (GA Git 2.42 / Oct
        // 2023) carries the same null-OID semantics on the doubled
        // 64-char width. Pinned separately so a future relaxation that
        // only catches the SHA-1 width surfaces here, peer with the
        // SHA-1 / SHA-256 pair-pinning posture
        // `git_oid_accepts_canonical_sha1` /
        // `git_oid_accepts_canonical_sha256` already establishes for
        // the positive controls.
        let null_sha256 = "0".repeat(64);
        let err = is_git_oid(&null_sha256).unwrap_err();
        assert!(
            err.contains("null-OID sentinel"),
            "reason must name the sentinel: {err}",
        );
        assert!(err.contains("64"), "reason must name the width: {err}",);
    }

    #[test]
    fn git_oid_null_oid_fires_after_length_and_hex_arms() {
        // Cascade-precedence pin: the null-OID arm runs *after* the
        // length + character-class arms, so an off-by-one-length all-
        // zeros value surfaces the narrower `abbreviated` diagnostic
        // (the length arm's own reason wording) before the structural
        // null-OID diagnostic, and an uppercase all-zeros value (which
        // can't actually exist — `0` has no case — but pinned via the
        // mixed-case-but-non-null fixture) routes the same way. The
        // null-OID arm is the *fourth* arm, structurally the
        // lexicographic-content-arm after length and per-byte
        // character-class.
        let off_by_one_zeros = "0".repeat(41);
        let err = is_git_oid(&off_by_one_zeros).unwrap_err();
        assert!(
            err.contains("abbreviated"),
            "off-by-one-length all-zeros surfaces length arm first: {err}",
        );
        // The all-`f` 40-char value — same boundary class as null-OID
        // but at the opposite hex extreme — passes the predicate,
        // confirming the null-OID arm doesn't over-fire on lexicographic
        // boundaries.
        is_git_oid("ffffffffffffffffffffffffffffffffffffffff").unwrap();
    }

    #[test]
    fn git_oid_rejects_empty_defensively() {
        // The predicate is called from `crate::dep::DepSource::validate`
        // only after the per-axis `FontePinEmpty` arm has fired at
        // validate time; re-checking here keeps the predicate usable
        // from any future call site without an empty-precondition
        // footgun. Same defensive empty-check `is_dns_1123_label`,
        // `is_gateway_api_http_path`, `is_wit_world_ref`,
        // `is_nats_subject`, `is_wasi_keyvalue_slot`, and
        // `is_git_ref_name` carry at their call sites.
        let err = is_git_oid("").unwrap_err();
        assert!(err.contains("empty"), "got: {err:?}");
    }

    #[test]
    fn git_oid_rejects_each_arm_with_substring_pinned_reason() {
        // Substrate-side diagnostic-shape pin: each grammar arm
        // surfaces its own distinct reason substring. Pinned here so a
        // future reason-wording rephrase that drops any of these
        // substrings surfaces at this one place, not piecemeal across
        // every per-axis test sweep. Mirrors
        // `git_ref_name_rejects_each_arm_with_substring_pinned_reason`,
        // `wasi_kv_slot_rejects_each_arm_with_substring_pinned_reason`,
        // and `nats_subject_rejects_each_arm_with_substring_pinned_reason`
        // on the peer predicates.
        for (s, needle) in [
            // Abbreviated 7-char prefix — the canonical `git log
            // --short` paste-from-release-notes footgun.
            ("c0ffee0", "abbreviated"),
            // Abbreviated 12-char prefix — `git log --short=12`.
            ("c0ffee001234", "abbreviated"),
            // Off-by-one above SHA-1 width.
            ("0123456789abcdef0123456789abcdef012345670", "abbreviated"),
            // Off-by-one below SHA-256 width.
            (
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde",
                "abbreviated",
            ),
            // Off-by-one above SHA-256 width.
            (
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0",
                "abbreviated",
            ),
            // Uppercase SHA-1 — `git porcelain` lowercases on output.
            ("DEADBEEFCAFEBABE0123456789ABCDEF01234567", "uppercase"),
            // Mixed-case SHA-1 — same path as pure-uppercase; the first
            // uppercase byte fires the arm.
            ("deadbeefCAFEbabe0123456789abcdef01234567", "uppercase"),
            // Non-hex character at exact SHA-1 length — the cross-axis
            // mis-slot footgun (a refname-style char landing in `:rev`).
            // `g` is the first non-hex byte; the non-hex arm fires
            // ahead of any other rule. The hyphen / colon / slash arms
            // are the same path on the same predicate.
            ("g123456789abcdef0123456789abcdef01234567", "non-hex"),
            ("0123456789abcdef-123456789abcdef01234567", "non-hex"),
            ("0123456789abcdef/123456789abcdef01234567", "non-hex"),
            ("0123456789abcdef:123456789abcdef01234567", "non-hex"),
            // Whitespace inside an otherwise-SHA-shaped value (length
            // 41 — fails the length arm first; pinned to ensure the
            // diagnostic surfaces *some* parser wording).
            ("0123456789abcdef0123456789abcdef01234567 ", "abbreviated"),
        ] {
            let err = is_git_oid(s)
                .err()
                .unwrap_or_else(|| panic!("git OID {s:?} must be rejected"));
            assert!(
                err.contains(needle),
                "git OID {s:?} reason must contain {needle:?}; got {err:?}"
            );
        }
    }

    #[test]
    fn git_oid_rejects_at_canonical_width_boundaries() {
        // Boundary pin on the two canonical widths simultaneously: 39
        // (below SHA-1), 40 (SHA-1 exactly), 41 (just above), 63 (just
        // below SHA-256), 64 (SHA-256 exactly), 65 (just above). Pinned
        // so a future relaxation that admits "close enough" widths
        // surfaces here. The failing-length fixtures use all-zero hex
        // so only the length arm fires (the null-OID sentinel arm is
        // structurally downstream of the length arm — a non-canonical
        // length fires the abbreviated diagnostic before the null
        // diagnostic). The passing-length fixtures use a non-null hex
        // value so the null-OID arm doesn't fire (the all-zero
        // canonical-width value is the sentinel and is rejected by its
        // own arm, pinned in `git_oid_rejects_null_oid_sentinel_*`).
        let nonzero_sha1 = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(nonzero_sha1.len(), 40);
        let nonzero_sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(nonzero_sha256.len(), 64);
        for (len, ok) in [
            (1usize, false),
            (7, false),
            (39, false),
            (40, true),
            (41, false),
            (63, false),
            (64, true),
            (65, false),
            (128, false),
        ] {
            let s = if ok && len == 40 {
                nonzero_sha1.to_string()
            } else if ok && len == 64 {
                nonzero_sha256.to_string()
            } else {
                "0".repeat(len)
            };
            let result = is_git_oid(&s);
            if ok {
                result.unwrap_or_else(|e| panic!("len {len} must pass: {e:?}"));
            } else {
                let err = result.expect_err(&format!("len {len} must fail"));
                assert!(
                    err.contains("abbreviated") || err.contains(&len.to_string()),
                    "len {len} reason must name the offending length or surface \
                     the abbreviation arm, got {err:?}"
                );
            }
        }
    }

    #[test]
    fn git_oid_rejection_is_disjoint_from_ref_name_acceptance() {
        // Structural pin: the two predicates partition the `:fonte`
        // pin axes — every canonical refname is rejected by
        // `is_git_oid`, and every canonical OID is rejected by
        // `is_git_ref_name`. The intersection of the two valid sets
        // is exactly the empty set. Drift here = a value that passes
        // both predicates would land at *both* axes silently, defeating
        // the structural "cross-axis mis-slot is a build error"
        // contract. Pinned with a representative cross-set so a future
        // predicate weakening surfaces here.
        let canonical_refnames = [
            "v0.1.0",
            "main",
            "feature/checkout",
            "release-1.0",
            "user-1/feat-x-v2",
        ];
        for refname in canonical_refnames {
            is_git_ref_name(refname).unwrap_or_else(|e| {
                panic!("setup: canonical refname {refname:?} must pass is_git_ref_name: {e:?}")
            });
            assert!(
                is_git_oid(refname).is_err(),
                "canonical refname {refname:?} must NOT pass is_git_oid \
                 (predicate-partition pin)"
            );
        }
        let canonical_oids = [
            "0123456789abcdef0123456789abcdef01234567",
            "deadbeefcafebabe0123456789abcdef01234567",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        ];
        for oid in canonical_oids {
            is_git_oid(oid).unwrap_or_else(|e| {
                panic!("setup: canonical OID {oid:?} must pass is_git_oid: {e:?}")
            });
            assert!(
                is_git_ref_name(oid).is_err(),
                "canonical OID {oid:?} must NOT pass is_git_ref_name \
                 (predicate-partition pin)"
            );
        }
    }

    // ── is_sandboxed_relative_path — `:behavior :on-*` + `:upgrade-from ─
    // ── :state-change :script` value-shape predicate ────────────────────

    #[test]
    fn sandboxed_relative_path_accepts_canonical_relative_paths() {
        // Positive controls: every documented authoring shape across
        // the two existing call sites (`:behavior :on-init` / `:on-call`
        // / `:on-cast` / `:on-info` / `:on-state-change` / `:on-terminate`
        // and `:upgrade-from :state-change :script`) — bare filename,
        // standard `lib/` subdirectory, deeply-nested migrations
        // subdirectory, sibling-folder-shaped path, and explicit
        // current-dir-relative-prefixed path. Pin every leg so a
        // future tightening that rejects any of these (e.g. demanding
        // a `lib/` prefix specifically, or forbidding the explicit
        // `./` segment) surfaces here as a test-failure at the predicate
        // boundary, not piecemeal across per-axis call sites.
        for relpath in [
            "init.lisp",
            "lib/init.lisp",
            "lib/handlers.lisp",
            "lib/migrations/v01-to-v02.lisp",
            "callbacks/on_call.lisp",
            "./lib/init.lisp",
            "a",
        ] {
            is_sandboxed_relative_path(Path::new(relpath)).unwrap_or_else(|v| {
                panic!("canonical relative path {relpath:?} must pass, got {v:?}")
            });
        }
    }

    #[test]
    fn sandboxed_relative_path_rejects_empty() {
        // The fail-before-pass-after pin on the empty arm. Both
        // `PathBuf::new()` (no bytes) and `PathBuf::from("")` (empty
        // string) hit the `as_os_str().is_empty()` precondition; both
        // resolve to `root` under `root.join(p)` and silently point the
        // `LisleLoader` at the project directory rather than a file.
        assert_eq!(
            is_sandboxed_relative_path(Path::new("")),
            Err(PathShapeViolation::Empty)
        );
        let blank = PathBuf::new();
        assert_eq!(
            is_sandboxed_relative_path(&blank),
            Err(PathShapeViolation::Empty)
        );
    }

    #[test]
    fn sandboxed_relative_path_rejects_absolute() {
        // The fail-before-pass-after pin on the absolute arm. Sweep
        // the canonical sandbox-escape paste-from-shell-prompt
        // footguns: an `/etc/...` Lunatic-style sandbox bypass, a
        // user-home leak that the renderer's `root.join(p)` would
        // silently replace, the project-relative-shaped `/lib/...`
        // typo where the author meant `lib/...` without a leading
        // slash, and the bare root `/`. `Path::join` replaces the
        // base with an absolute right-hand side, so every one of
        // these resolves verbatim to outside the caixa root regardless
        // of where the layout checker rooted itself.
        for abs in [
            "/etc/passwd",
            "/home/user/escape.lisp",
            "/lib/init.lisp",
            "/",
        ] {
            assert_eq!(
                is_sandboxed_relative_path(Path::new(abs)),
                Err(PathShapeViolation::Absolute),
                "absolute path {abs:?} must surface as PathShapeViolation::Absolute"
            );
        }
    }

    #[test]
    fn sandboxed_relative_path_rejects_parent_escape_at_every_position() {
        // The fail-before-pass-after pin on the parent-escape arm.
        // Position sweep — `..` as a leading component (the canonical
        // "I meant the sibling caixa" mis-author), as a mid-path
        // component (the canonical "lib/../../escape" path-traversal
        // that's structurally identical regardless of how many `..`
        // segments stack), as a trailing component (lib/.., resolving
        // to the project root via a delayed escape), and the bare `..`
        // (project parent directory). Each must surface as
        // `PathShapeViolation::ParentEscape` regardless of position —
        // pinned per-position so a future relaxation that only
        // checks one position surfaces at this one place, not
        // piecemeal across per-axis call sites.
        for escape in [
            "../sibling/init.lisp",
            "lib/../../escaped.lisp",
            "lib/..",
            "..",
            "lib/handlers/../../escape.lisp",
        ] {
            assert_eq!(
                is_sandboxed_relative_path(Path::new(escape)),
                Err(PathShapeViolation::ParentEscape),
                "parent-escape path {escape:?} must surface as \
                 PathShapeViolation::ParentEscape"
            );
        }
    }

    #[test]
    fn sandboxed_relative_path_arm_ordering_is_empty_absolute_parent_escape() {
        // Order pin: the predicate evaluates Empty → Absolute →
        // ParentEscape — the same arm-ordering both inlined call sites
        // followed verbatim (b0c8389 `BehaviorSpec::validate`'s
        // `validate_callback_path`, 26da2c7
        // `UpgradeInstruction::StateChange::validate`). A future
        // reordering would silently flip which diagnostic the per-axis
        // wrapper surfaces (e.g. an absolute-and-empty hybrid value
        // would suddenly raise `Absolute` instead of `Empty`). Pinned
        // here so a future reorder surfaces at the predicate boundary.
        //
        // The empty case can't *also* be absolute (empty paths are
        // relative-by-construction) or parent-escaping, so the
        // empty-first ordering only matters relative to the OS-string
        // emptiness check vs. the absolute-prefix check. Pin the two
        // legs that *can* compose: an absolute path with `..` segments
        // must raise `Absolute` (not `ParentEscape`); an absolute-but-
        // not-parent-escaping path must also raise `Absolute`. The
        // arm-ordering pin is structural — every parent-escape case
        // tested above is relative, so the ParentEscape arm is reached
        // only when both Empty and Absolute arms have been cleared.
        assert_eq!(
            is_sandboxed_relative_path(Path::new("/etc/../passwd")),
            Err(PathShapeViolation::Absolute),
            "absolute path with `..` segments must surface as Absolute (not \
             ParentEscape) — Empty → Absolute → ParentEscape arm-ordering pin"
        );
    }

    #[test]
    fn sandboxed_relative_path_distinguishes_curdir_from_parent_escape() {
        // Boundary pin: `Component::CurDir` (`.`) is NOT a sandbox
        // escape — `root.join("./lib/x.lisp")` resolves to
        // `root/lib/x.lisp`, identical to `root.join("lib/x.lisp")`,
        // so `./` segments must pass the predicate. The arm-ordering
        // check above pins that `Component::ParentDir` is the only
        // escape vector caught here. Pinned separately so a future
        // tightening that *does* reject `.` segments (e.g. requiring
        // canonical normalized form) lands at this one predicate.
        is_sandboxed_relative_path(Path::new("./lib/init.lisp")).unwrap();
        is_sandboxed_relative_path(Path::new("lib/./handlers.lisp")).unwrap();
    }

    #[test]
    fn sandboxed_relative_path_violations_are_distinct_variants() {
        // Diagnostic-shape pin: the three `PathShapeViolation` variants
        // are distinct enum tags so each per-axis caller can match-and-
        // wrap into its own typed `*Path` / `*Script` variant without
        // a string-parse step (the trap [`is_dns_1123_label`] etc.
        // avoid by returning `Result<(), String>` — but the path-shape
        // callers were already split three ways across `BehaviorError`
        // / `UpgradeError`, so a `String` return would *regress* the
        // diagnostic shape rather than preserve it). The PartialEq /
        // Copy / Hash derives on `PathShapeViolation` are pinned here
        // so a future API rework reads the requirement off this test.
        let v1 = PathShapeViolation::Empty;
        let v2 = PathShapeViolation::Absolute;
        let v3 = PathShapeViolation::ParentEscape;
        assert_ne!(v1, v2);
        assert_ne!(v2, v3);
        assert_ne!(v1, v3);
        // Copy + Eq round-trip: predicate consumers like
        // `BehaviorSpec::validate` and `UpgradeInstruction::validate`
        // pattern-match on the variant without consuming it.
        let v_copy = v1;
        assert_eq!(v1, v_copy);
    }

    #[test]
    fn sandboxed_relative_path_matches_inlined_call_site_semantics() {
        // End-to-end pin: every value the two pre-lift inline gates
        // (`BehaviorSpec::validate_callback_path` and
        // `UpgradeInstruction::StateChange::validate`'s inline arms)
        // accepted-or-rejected must surface from the lifted predicate
        // with identically-classified violation tags. Drift here would
        // mean a previously-accepted authoring shape would suddenly
        // fail (or vice versa) silently across the lift commit. Pinned
        // by sweeping the canonical authoring shapes both pre-lift call
        // sites' tests cover.
        // Pre-lift accepts (must still pass):
        for accept in [
            "lib/init.lisp",
            "lib/handlers.lisp",
            "lib/migrations.lisp",
            "lib/cleanup.lisp",
            "lib/migrations/v01-to-v02.lisp",
            "callbacks/handle_call.lisp",
        ] {
            is_sandboxed_relative_path(Path::new(accept))
                .unwrap_or_else(|v| panic!("pre-lift accept {accept:?} regressed, got {v:?}"));
        }
        // Pre-lift rejects (must still reject, with the same tag):
        let cases: &[(&str, PathShapeViolation)] = &[
            ("", PathShapeViolation::Empty),
            ("/etc/passwd", PathShapeViolation::Absolute),
            ("/etc/migrations.lisp", PathShapeViolation::Absolute),
            (
                "../sibling/migrations.lisp",
                PathShapeViolation::ParentEscape,
            ),
            ("lib/../../escaped.lisp", PathShapeViolation::ParentEscape),
        ];
        for (reject, expected) in cases {
            assert_eq!(
                is_sandboxed_relative_path(Path::new(reject)).unwrap_err(),
                *expected,
                "pre-lift reject {reject:?} must classify as {expected:?}"
            );
        }
    }

    // ── is_lisp_extension — `:behavior :on-*` + `:upgrade-from ───────────
    // ── :state-change :script` file-type predicate ───────────────────────

    #[test]
    fn lisp_extension_accepts_canonical_shapes() {
        // Positive controls: every documented authoring shape across
        // both existing call sites — bare filename, standard `lib/`
        // subdirectory, deeply-nested migrations subdirectory,
        // explicit current-dir-relative prefix, mid-path `./`
        // segment, single-letter stem, and the multi-dot stem
        // (`lib/migrations/v.0.1.lisp`) an author might use to
        // encode the migration's `:from` version into the filename.
        // The predicate only inspects the terminating extension —
        // `Path::extension()` returns the substring after the final
        // `.` — so the multi-dot stem is structurally accepted
        // because the final extension is still `lisp`. Drift here =
        // a future tightening that rejects any of these surfaces as
        // a test-failure at the predicate boundary, not piecemeal
        // across per-axis call sites (`BehaviorSpec::validate`,
        // `UpgradeInstruction::StateChange::validate`).
        for relpath in [
            "init.lisp",
            "lib/init.lisp",
            "lib/handlers.lisp",
            "lib/migrations.lisp",
            "lib/migrations/v01-to-v02.lisp",
            "./lib/init.lisp",
            "lib/./handlers.lisp",
            "lib/migrations/v.0.1.lisp",
            "a.lisp",
        ] {
            assert!(
                is_lisp_extension(Path::new(relpath)),
                "canonical `.lisp` shape {relpath:?} must pass is_lisp_extension"
            );
        }
    }

    #[test]
    fn lisp_extension_rejects_no_extension() {
        // The fail-before-pass-after pin on the no-extension shape.
        // A path with no `.` component (`Path::extension()` returns
        // `None`) is the canonical "I declared the slot but forgot
        // the `.lisp` extension" authoring footgun. The wasm-engine's
        // `tatara_lisp::read` consumer can't infer the file type from
        // the path alone, so the gate refuses the value at validate
        // time.
        for relpath in [
            "lib/init",
            "init",
            "lib/handlers",
            "lib/migrations/v01-to-v02",
            "a",
        ] {
            assert!(
                !is_lisp_extension(Path::new(relpath)),
                "no-extension shape {relpath:?} must fail is_lisp_extension"
            );
        }
    }

    #[test]
    fn lisp_extension_rejects_wrong_extension() {
        // Wrong-extension sweep: the canonical authoring footguns
        // an author might drag in from the workspace tree (`.txt`,
        // `.md`, `.json`, `.yaml`, `.toml`), the `.rs` shape that
        // an IDE auto-complete might propose, the `.lisp.bak` shape
        // an editor might leave behind (the predicate only inspects
        // the *terminating* extension — `Path::extension()` returns
        // `bak` here, not `lisp.bak` — so the gate refuses it as a
        // no-`.lisp` final extension), and the `.lispx` / `.lis`
        // near-miss shapes that a typo would produce. Each must
        // fail the predicate — the wasm-engine's `tatara_lisp::read`
        // consumer rejects all of these at hot-upgrade migration /
        // instance-start time.
        for relpath in [
            "lib/init.rs",
            "lib/init.txt",
            "lib/init.md",
            "lib/init.json",
            "lib/init.yaml",
            "lib/init.toml",
            "lib/init.lisp.bak",
            "lib/init.lispx",
            "lib/init.lis",
        ] {
            assert!(
                !is_lisp_extension(Path::new(relpath)),
                "wrong-extension shape {relpath:?} must fail is_lisp_extension"
            );
        }
    }

    #[test]
    fn lisp_extension_is_case_sensitive() {
        // Strict lowercase pin: every case-folded shape a
        // case-insensitive volume's existence check would match the
        // on-disk file must still fail the predicate — the
        // canonical-form codec emits lowercase `.lisp` verbatim, so
        // a case-folded shape mismatches the round-trip-stable
        // canonical form (THEORY.md §V.2.7 render-determinism).
        // Same case-sensitive discipline the byte-size / duration
        // codecs and every other shape-gate predicate in `render.rs`
        // (label / scheme / unit boundaries) carry. Pinned at the
        // predicate boundary so any future case-folding regression
        // surfaces here rather than piecemeal across per-axis call
        // sites.
        for relpath in [
            "lib/init.LISP",
            "lib/init.Lisp",
            "lib/init.LiSp",
            "lib/init.lISP",
            "lib/init.LISp",
        ] {
            assert!(
                !is_lisp_extension(Path::new(relpath)),
                "case-folded `.lisp` shape {relpath:?} must fail is_lisp_extension \
                 (strict lowercase, render-determinism pin)"
            );
        }
    }

    #[test]
    fn lisp_extension_constant_matches_predicate() {
        // Cross-pin: the [`LISP_SOURCE_EXTENSION`] const and the
        // predicate's accepted set are the same single source of
        // truth. Drift would let a future renderer / per-axis
        // wrapper emit `.<const>` while the predicate accepts only
        // `.lisp` (or vice versa), silently breaking the
        // round-trip-stable canonical form. Pinned by constructing
        // a path from the const and round-tripping through the
        // predicate.
        assert_eq!(LISP_SOURCE_EXTENSION, "lisp");
        let p = PathBuf::from(format!("lib/init.{LISP_SOURCE_EXTENSION}"));
        assert!(
            is_lisp_extension(&p),
            "path constructed from LISP_SOURCE_EXTENSION must pass is_lisp_extension"
        );
    }

    #[test]
    fn lisp_extension_matches_inlined_call_site_semantics() {
        // End-to-end pin: every value the pre-lift inline gate
        // (`BehaviorSpec::validate_callback_path`, c97815a) accepted-
        // or-rejected must surface from the lifted predicate
        // identically. Drift here would mean a previously-accepted
        // authoring shape would suddenly fail (or vice versa)
        // silently across the lift commit. Sweeps the canonical
        // authoring shapes the pre-lift call site's tests covered
        // verbatim.
        // Pre-lift accepts (must still pass):
        for accept in [
            "lib/init.lisp",
            "lib/handlers.lisp",
            "lib/migrations/v01-to-v02.lisp",
            "init.lisp",
            "a.lisp",
            "./lib/init.lisp",
            "lib/./handlers.lisp",
            "lib/migrations/v.0.1.lisp",
        ] {
            assert!(
                is_lisp_extension(Path::new(accept)),
                "pre-lift accept {accept:?} regressed"
            );
        }
        // Pre-lift rejects (must still reject):
        for reject in [
            "lib/init",
            "init",
            "lib/init.rs",
            "lib/init.txt",
            "lib/init.lisp.bak",
            "lib/init.lispx",
            "lib/init.LISP",
            "lib/init.Lisp",
        ] {
            assert!(
                !is_lisp_extension(Path::new(reject)),
                "pre-lift reject {reject:?} regressed"
            );
        }
    }

    // ── is_computeunit_yaml_extension — `:servicos` compound-suffix predicate ───

    #[test]
    fn computeunit_yaml_extension_accepts_canonical_shapes() {
        // Positive controls: every canonical authoring shape every
        // in-tree fixture and the `Caixa::template` scaffold use. The
        // predicate inspects the final file-name component and checks
        // for the compound `.computeunit.yaml` suffix with at least
        // one byte of stem preceding it.
        for relpath in [
            "servicos/demo.computeunit.yaml",
            "servicos/hello-rio.computeunit.yaml",
            "servicos/my-service.computeunit.yaml",
            "servicos/a.computeunit.yaml",
            "./servicos/demo.computeunit.yaml",
            "servicos/./demo.computeunit.yaml",
            "servicos/sub/nested.computeunit.yaml",
            "servicos/v0.1.computeunit.yaml",
        ] {
            assert!(
                is_computeunit_yaml_extension(Path::new(relpath)),
                "canonical `.computeunit.yaml` shape {relpath:?} must pass \
                 is_computeunit_yaml_extension"
            );
        }
    }

    #[test]
    fn computeunit_yaml_extension_rejects_no_extension() {
        // No-extension shape — the canonical "I declared the slot
        // but forgot the `.computeunit.yaml` suffix" footgun. The
        // peer caixa-helm / caixa-flux `serde_yaml::from_str`
        // consumer can't infer the file type from the path alone, so
        // the gate refuses the value at validate time.
        for relpath in ["servicos/demo", "demo", "servicos/sub/nested"] {
            assert!(
                !is_computeunit_yaml_extension(Path::new(relpath)),
                "no-extension shape {relpath:?} must fail \
                 is_computeunit_yaml_extension"
            );
        }
    }

    #[test]
    fn computeunit_yaml_extension_rejects_wrong_extension() {
        // Wrong-extension sweep across the canonical authoring footguns
        // an author might drag in from the workspace tree — bare
        // `.yaml` (the canonical "I forgot the `.computeunit` segment"
        // typo), `.yml` (Helm-shorthand leak), `.json` (FluxCD
        // bundle leak), `.toml` (Cargo workspace leak), `.txt`
        // / `.md` (paste-from-doc footguns), `.yaml.bak` (editor
        // backup), the near-miss `.computeunit.yam` / `.computeunit.yamls`
        // typo, and the off-by-one-segment `computeunit-yaml`
        // / `computeunit_yaml` shapes. Each must fail the predicate.
        for relpath in [
            "servicos/demo.yaml",
            "servicos/demo.yml",
            "servicos/demo.json",
            "servicos/demo.toml",
            "servicos/demo.txt",
            "servicos/demo.md",
            "servicos/demo.computeunit.yaml.bak",
            "servicos/demo.computeunit.yam",
            "servicos/demo.computeunit.yamls",
            "servicos/demo.computeunit",
            "servicos/demo-computeunit.yaml",
            "servicos/demo_computeunit.yaml",
        ] {
            assert!(
                !is_computeunit_yaml_extension(Path::new(relpath)),
                "wrong-extension shape {relpath:?} must fail \
                 is_computeunit_yaml_extension"
            );
        }
    }

    #[test]
    fn computeunit_yaml_extension_is_case_sensitive() {
        // Strict lowercase pin: every case-folded shape a
        // case-insensitive volume's existence check would match the
        // on-disk file must still fail the predicate — the canonical-
        // form codec emits lowercase `.computeunit.yaml` verbatim, so
        // a case-folded shape mismatches the round-trip-stable
        // canonical form (THEORY.md §V.2.7 render-determinism). Same
        // case-sensitive discipline the byte-size / duration codecs
        // and the peer `is_lisp_extension` predicate carry.
        for relpath in [
            "servicos/demo.ComputeUnit.yaml",
            "servicos/demo.COMPUTEUNIT.yaml",
            "servicos/demo.computeunit.YAML",
            "servicos/demo.computeunit.Yaml",
            "servicos/demo.COMPUTEUNIT.YAML",
        ] {
            assert!(
                !is_computeunit_yaml_extension(Path::new(relpath)),
                "case-folded `.computeunit.yaml` shape {relpath:?} must fail \
                 is_computeunit_yaml_extension (strict lowercase, \
                 render-determinism pin)"
            );
        }
    }

    #[test]
    fn computeunit_yaml_extension_rejects_empty_stem() {
        // Degenerate hidden-file shape: a file name exactly equal to
        // the suffix (`.computeunit.yaml` — no stem preceding the
        // suffix) is the structural "Servico declared with no
        // identity" footgun. The substrate identifies each ComputeUnit
        // by the file-stem segment that precedes `.computeunit.yaml`
        // (the rendered `lareira-<stem>` Helm chart, the per-Servico
        // `metadata.name`, the M3 `:contratos` membership lookup), so
        // an empty stem leaves the Servico unidentifiable. Predicate
        // pin: the `name.len() > SUFFIX.len()` bound rejects the
        // hidden-file shape at the predicate boundary.
        for relpath in [".computeunit.yaml", "servicos/.computeunit.yaml"] {
            assert!(
                !is_computeunit_yaml_extension(Path::new(relpath)),
                "empty-stem shape {relpath:?} must fail \
                 is_computeunit_yaml_extension"
            );
        }
    }

    #[test]
    fn computeunit_yaml_extension_constant_matches_predicate() {
        // Cross-pin: the [`COMPUTEUNIT_YAML_SUFFIX`] const and the
        // predicate's accepted set are the same single source of
        // truth. Drift would let a future renderer / per-axis wrapper
        // emit `<stem><const>` while the predicate accepts only
        // `.computeunit.yaml` (or vice versa), silently breaking the
        // round-trip-stable canonical form. Pinned by constructing a
        // path from the const and round-tripping through the
        // predicate. Mirrors the peer
        // `lisp_extension_constant_matches_predicate` pin.
        assert_eq!(COMPUTEUNIT_YAML_SUFFIX, ".computeunit.yaml");
        let p = PathBuf::from(format!("servicos/demo{COMPUTEUNIT_YAML_SUFFIX}"));
        assert!(
            is_computeunit_yaml_extension(&p),
            "path constructed from COMPUTEUNIT_YAML_SUFFIX must pass \
             is_computeunit_yaml_extension"
        );
    }

    // ── is_cargo_feature_name — shared `:caracteristicas` feature-name predicate ──

    #[test]
    fn cargo_feature_name_accepts_canonical_forms() {
        // Substrate-side pin: the predicate accepts every canonical Cargo
        // feature name shape `:caracteristicas` entries carry. Drift between
        // this list and the per-axis `dep::tests::validate_accepts_canonical_caracteristicas`
        // positive-set sweep surfaces here — one source of truth for the
        // rule. Includes single-token (`http`), kebab-case (`runtime-tokio`),
        // snake-case (`derive_macros`), namespaced-dot (`tokio.full`),
        // version-suffix (`v0.1`), `+`-separated (`http+json`), leading
        // underscore (`_internal`), doubled-underscore (`__private`),
        // and digit-starting (`v0_1`) — the canonical authoring shapes
        // every realistic Cargo feature in the pleme-io ecosystem uses.
        for s in [
            "http",
            "json",
            "derive",
            "serde",
            "serde_json",
            "runtime-tokio",
            "tokio.full",
            "v0.1",
            "v1",
            "http+json",
            "_internal",
            "__private",
            "default",
            "rt-multi-thread",
            "12factor",
            "feat.v2",
            "client+server",
        ] {
            is_cargo_feature_name(s)
                .unwrap_or_else(|e| panic!("canonical Cargo feature name {s:?} must pass: {e:?}"));
        }
    }

    #[test]
    fn cargo_feature_name_rejects_each_arm_with_substring_pinned_reason() {
        // Substrate-side diagnostic-shape pin: each grammar arm
        // surfaces its own distinct reason substring. Pinned here so a
        // future reason-wording rephrase that drops any of these
        // substrings surfaces at this one place, not piecemeal across
        // every per-axis test sweep. Mirrors
        // `git_repo_url`'s and `git_ref_name`'s arm-substring sweeps
        // on the peer predicates.
        for (s, needle) in [
            // Leading `+` — the canonical paste-from-`+optional-feature`
            // activation-form-in-feature-name-slot footgun.
            ("+http", "`+`"),
            // Leading `-` — kebab-leak / CLI-arg-injection adjacent.
            ("-json", "`-`"),
            // Leading `.` — dotted-version-suffix-as-feature-name typo.
            (".feat", "`.`"),
            // Whitespace inside — multi-token blob.
            ("http feature", "whitespace"),
            // Tab inside.
            ("http\tjson", "whitespace"),
            // Leading whitespace — paste-from-aligned-doc.
            (" http", "whitespace"),
            // Comma — list-separator-belongs-to-list-grammar.
            ("http,json", "`,`"),
            // Forward slash — Cargo's `dep/feat` namespaced-dep syntax.
            ("http/json", "`/`"),
            // Question mark — URL-reserved.
            ("http?", "`?`"),
            // Hash — URL-reserved.
            ("http#frag", "`#`"),
            // Embedded control character.
            ("http\x01json", "control character"),
            // Newline — paste-from-multiline-doc.
            ("http\njson", "control character"),
            // DEL byte (0x7F).
            ("http\x7fjson", "control character"),
            // Non-ASCII byte — un-percent-encoded character.
            ("caf\u{e9}", "non-ASCII"),
            // Non-ASCII at first byte.
            ("\u{e9}feat", "non-ASCII"),
            // Forbidden punctuation in the continuation set.
            ("http@1", "invalid character"),
            ("http&json", "invalid character"),
            ("http=v1", "invalid character"),
        ] {
            let err = is_cargo_feature_name(s)
                .err()
                .unwrap_or_else(|| panic!("Cargo feature name {s:?} must be rejected"));
            assert!(
                err.contains(needle),
                "Cargo feature name {s:?} reason must contain {needle:?}; got {err:?}"
            );
        }
    }

    #[test]
    fn cargo_feature_name_rejects_empty_defensively() {
        // The predicate is called from `crate::dep::Dep::validate_caracteristicas`
        // only after the per-axis `CaracteristicaEmpty` arm has fired
        // at validate time; re-checking here keeps the predicate usable
        // from any future call site without an empty-precondition
        // footgun. Same defensive empty-check `is_dns_1123_label`,
        // `is_gateway_api_http_path`, `is_wit_world_ref`,
        // `is_nats_subject`, `is_wasi_keyvalue_slot`, `is_git_ref_name`,
        // `is_git_oid`, and `is_git_repo_url` carry at their call sites.
        let err = is_cargo_feature_name("").unwrap_err();
        assert!(err.contains("empty"), "got: {err:?}");
    }

    #[test]
    fn cargo_feature_name_rejects_at_65_byte_boundary() {
        // The 64-byte cap pin — both the boundary-exceeding case and
        // the boundary-accepting case in one place, so a future cap
        // shift surfaces both arms simultaneously, mirroring
        // `dns_1123_label_rejects_at_64_byte_boundary`,
        // `gateway_api_http_path_rejects_at_1025_byte_boundary`,
        // `wit_world_ref_rejects_at_129_byte_boundary`,
        // `nats_subject_rejects_at_257_byte_boundary`,
        // `wasi_kv_slot_rejects_at_513_byte_boundary`, and
        // `git_ref_name_rejects_at_256_byte_boundary` on the peer
        // predicates. Constructed as a single all-`a` token so only
        // the cap arm fires.
        let max_ok = "a".repeat(CARGO_FEATURE_NAME_MAX_LEN);
        assert_eq!(max_ok.len(), 64);
        is_cargo_feature_name(&max_ok).unwrap();
        let too_long = "a".repeat(CARGO_FEATURE_NAME_MAX_LEN + 1);
        assert_eq!(too_long.len(), 65);
        let err = is_cargo_feature_name(&too_long).unwrap_err();
        assert!(err.contains("64"), "got: {err:?}");
        assert!(err.contains("65"), "got: {err:?}");
    }

    #[test]
    fn cargo_feature_name_first_byte_diagnostics_name_the_leading_char() {
        // Diagnostic-shape pin: the leading-character rejection arms
        // name the specific punctuation (`+`, `-`, `.`) verbatim so the
        // author's grep target is unambiguous. Pinned across the three
        // canonical leading-char footguns so a future relaxation that
        // drops any of the three surfaces here. The `+`-arm's wording
        // additionally points the author at the canonical Cargo
        // `+<feature>` activation-form-vs-feature-name discipline so
        // the paste-from-doc footgun lands its remediation in the
        // diagnostic itself.
        let err_plus = is_cargo_feature_name("+http").unwrap_err();
        assert!(err_plus.contains("`+`"), "got: {err_plus:?}");
        assert!(
            err_plus.contains("activation"),
            "got: {err_plus:?} (must name the Cargo +<feature> activation-form)"
        );
        let err_hyphen = is_cargo_feature_name("-json").unwrap_err();
        assert!(err_hyphen.contains("`-`"), "got: {err_hyphen:?}");
        let err_dot = is_cargo_feature_name(".feat").unwrap_err();
        assert!(err_dot.contains("`.`"), "got: {err_dot:?}");
    }

    // ── is_spdx_expression_shape — shared `:licenca` SPDX-expression predicate ──

    #[test]
    fn spdx_expression_shape_accepts_canonical_forms() {
        // Substrate-side pin: the predicate accepts every canonical
        // SPDX expression shape the `:licenca` axis carries. Drift
        // between this list and the per-axis
        // `manifest::tests::validate_licenca_accepts_canonical_expressions`
        // positive-set sweep surfaces here — one source of truth for
        // the rule. Covers single-license, `OR`/`AND`-compound,
        // `WITH`-exception, parenthesis-grouped, `+`-suffix, and
        // `LicenseRef-` / `DocumentRef-:LicenseRef-` shapes.
        for s in [
            "MIT",
            "Apache-2.0",
            "BSD-3-Clause",
            "MPL-2.0",
            "GPL-3.0-or-later",
            "GPL-2.0+",
            "Apache-2.0 OR MIT",
            "Apache-2.0 AND MIT",
            "Apache-2.0 WITH LLVM-exception",
            "(MIT OR Apache-2.0) AND BSD-3-Clause",
            "(MIT OR Apache-2.0) AND BSD-3-Clause AND ISC",
            "LicenseRef-MyLicense",
            "DocumentRef-spdx-tool:LicenseRef-MIT-Style",
            "x",
        ] {
            is_spdx_expression_shape(s)
                .unwrap_or_else(|e| panic!("canonical SPDX expression {s:?} must pass: {e:?}"));
        }
    }

    #[test]
    fn spdx_expression_shape_rejects_each_arm_with_substring_pinned_reason() {
        // Substrate-side diagnostic-shape pin: each alphabet arm
        // surfaces its own distinct reason substring. Pinned here so a
        // future reason-wording rephrase that drops any of these
        // substrings surfaces at this one place, not piecemeal across
        // every per-axis test sweep. Mirrors
        // `cargo_feature_name_rejects_each_arm_with_substring_pinned_reason`
        // on the peer predicate.
        for (s, needle) in [
            // Leading whitespace — paste-from-aligned-doc.
            (" MIT", "whitespace"),
            // Trailing whitespace — paste-from-doc.
            ("MIT ", "whitespace"),
            // Tab inside — tab-from-aligned-doc.
            ("MIT\tOR Apache-2.0", "tab"),
            // Embedded control character.
            ("MIT\x01OR Apache-2.0", "control character"),
            // Newline — paste-from-multiline-doc.
            ("MIT\nOR Apache-2.0", "control character"),
            // CRLF — paste-from-multiline-doc.
            ("MIT\rApache-2.0", "control character"),
            // DEL byte (0x7F).
            ("MIT\x7fApache-2.0", "control character"),
            // Non-ASCII byte — smart-quote paste.
            ("MIT\u{a0}OR Apache-2.0", "non-ASCII"),
            // Non-ASCII at first byte — fullwidth letter.
            ("\u{ff2d}IT", "non-ASCII"),
            // Underscore — snake-case-instead-of-kebab-case typo.
            ("Apache_2.0", "`_`"),
            // Comma — list-separator-belongs-to-list-grammar.
            ("MIT, Apache-2.0", "`,`"),
            // Forward slash — colloquial dual-license idiom.
            ("MIT/Apache-2.0", "`/`"),
            // Semicolon — list-separator confusion.
            ("MIT; Apache-2.0", "`;`"),
            // Forbidden punctuation in the alphabet.
            ("MIT@1.0", "invalid character"),
            ("MIT&Apache-2.0", "invalid character"),
            ("MIT=Apache-2.0", "invalid character"),
            ("MIT*1.0", "invalid character"),
        ] {
            let err = is_spdx_expression_shape(s)
                .err()
                .unwrap_or_else(|| panic!("SPDX expression {s:?} must be rejected"));
            assert!(
                err.contains(needle),
                "SPDX expression {s:?} reason must contain {needle:?}; got {err:?}"
            );
        }
    }

    #[test]
    fn spdx_expression_shape_rejects_empty_defensively() {
        // The predicate is called from `crate::Caixa::validate_licenca`
        // only after the per-axis `LicencaEmpty` arm has fired at
        // validate time; re-checking here keeps the predicate usable
        // from any future call site without an empty-precondition
        // footgun. Same defensive empty-check `is_dns_1123_label`,
        // `is_gateway_api_http_path`, `is_wit_world_ref`,
        // `is_nats_subject`, `is_wasi_keyvalue_slot`,
        // `is_git_ref_name`, `is_git_oid`, `is_git_repo_url`, and
        // `is_cargo_feature_name` carry at their call sites.
        let err = is_spdx_expression_shape("").unwrap_err();
        assert!(err.contains("empty"), "got: {err:?}");
    }

    #[test]
    fn spdx_expression_shape_rejects_at_257_byte_boundary() {
        // The 256-byte cap pin — both the boundary-exceeding case and
        // the boundary-accepting case in one place, so a future cap
        // shift surfaces both arms simultaneously, mirroring the peer
        // cap-boundary pins. Constructed as a single all-`a` token so
        // only the cap arm fires (256 `a` bytes is alphabet-valid).
        let max_ok = "a".repeat(SPDX_EXPRESSION_MAX_LEN);
        assert_eq!(max_ok.len(), 256);
        is_spdx_expression_shape(&max_ok).unwrap();
        let too_long = "a".repeat(SPDX_EXPRESSION_MAX_LEN + 1);
        assert_eq!(too_long.len(), 257);
        let err = is_spdx_expression_shape(&too_long).unwrap_err();
        assert!(err.contains("256"), "got: {err:?}");
        assert!(err.contains("257"), "got: {err:?}");
    }

    // ── is_chart_description_shape — shared `:descricao` chart-description predicate ──

    #[test]
    fn chart_description_shape_accepts_canonical_forms() {
        // Substrate-side pin: the predicate accepts every canonical
        // chart-description shape the `:descricao` axis carries.
        // Drift between this list and the per-axis
        // `manifest::tests::validate_descricao_accepts_canonical_summary`
        // positive-set sweep surfaces here — one source of truth for
        // the rule. Covers ASCII summaries, the Unicode `→` from the
        // canonical Rust→wasm fixture, and the Unicode `—` em-dash
        // from the `Caixa::template` scaffold every `feira init`
        // emits.
        for s in [
            "Canonical Rust→wasm32-wasip2 caixa Servico.",
            "Checkout flow.",
            "AWS provider caixa for tatara-lisp",
            "FIXME — describe this caixa",
            "x",
        ] {
            is_chart_description_shape(s)
                .unwrap_or_else(|e| panic!("canonical chart description {s:?} must pass: {e:?}"));
        }
    }

    #[test]
    fn chart_description_shape_rejects_each_arm_with_substring_pinned_reason() {
        // Substrate-side diagnostic-shape pin: each arm surfaces its
        // own distinct reason substring. Pinned here so a future
        // reason-wording rephrase that drops any of these substrings
        // surfaces at this one place, not piecemeal across every
        // per-axis test sweep. Mirrors
        // `spdx_expression_shape_rejects_each_arm_with_substring_pinned_reason`
        // on the peer predicate.
        for (s, needle) in [
            // Leading whitespace — paste-from-aligned-doc.
            (" Checkout flow.", "whitespace"),
            // Trailing whitespace — paste-from-doc.
            ("Checkout flow. ", "whitespace"),
            // Tab inside — tab-from-aligned-doc.
            ("Checkout\tflow.", "tab"),
            // Newline — paste-from-multiline-doc.
            ("Checkout\nflow.", "newline"),
            // Carriage return — paste-from-Windows-CRLF-doc.
            ("Checkout\rflow.", "carriage return"),
            // NUL byte — paste-from-binary-blob.
            ("Checkout\x00flow.", "control character"),
            // BEL byte — paste-from-binary-blob.
            ("Checkout\x07flow.", "control character"),
            // ESC byte — paste-from-binary-blob.
            ("Checkout\x1bflow.", "control character"),
            // DEL byte (0x7F).
            ("Checkout\x7fflow.", "control character"),
        ] {
            let err = is_chart_description_shape(s)
                .err()
                .unwrap_or_else(|| panic!("chart description {s:?} must be rejected"));
            assert!(
                err.contains(needle),
                "chart description {s:?} reason must contain {needle:?}; got {err:?}"
            );
        }
    }

    #[test]
    fn chart_description_shape_accepts_unicode() {
        // Positive control on the non-ASCII arm: the predicate must
        // accept Unicode beyond the ASCII alphabet — the canonical
        // pleme-io descricao fixtures carry `→` (U+2192) and `—`
        // (U+2014), and every downstream consumer (YAML 1.2, Helm v3,
        // every chart-aware UI) round-trips Unicode losslessly.
        // Mirrors the spdx-rejects-non-ASCII arm by inverting it — a
        // future tightening that bans non-ASCII bytes would regress
        // every canonical fixture and surface here as a regression.
        for s in [
            "Canonical Rust→wasm32-wasip2",
            "FIXME — describe this caixa",
            "Caixa pour le projet tâche",
            "日本語の説明",
            "naïve",
        ] {
            is_chart_description_shape(s)
                .unwrap_or_else(|e| panic!("Unicode chart description {s:?} must pass: {e:?}"));
        }
    }

    #[test]
    fn chart_description_shape_rejects_empty_defensively() {
        // The predicate is called from `crate::Caixa::validate_descricao`
        // only after the per-axis `DescricaoEmpty` arm has fired at
        // validate time; re-checking here keeps the predicate usable
        // from any future call site without an empty-precondition
        // footgun. Same defensive empty-check `is_dns_1123_label`,
        // `is_gateway_api_http_path`, `is_wit_world_ref`,
        // `is_nats_subject`, `is_wasi_keyvalue_slot`,
        // `is_git_ref_name`, `is_git_oid`, `is_git_repo_url`,
        // `is_cargo_feature_name`, and `is_spdx_expression_shape`
        // carry at their call sites.
        let err = is_chart_description_shape("").unwrap_err();
        assert!(err.contains("empty"), "got: {err:?}");
    }

    #[test]
    fn chart_description_shape_rejects_at_513_byte_boundary() {
        // The 512-byte cap pin — both the boundary-exceeding case and
        // the boundary-accepting case in one place, so a future cap
        // shift surfaces both arms simultaneously, mirroring the peer
        // cap-boundary pins. Constructed as a single all-`a` token so
        // only the cap arm fires (512 `a` bytes is alphabet-valid).
        let max_ok = "a".repeat(CHART_DESCRIPTION_MAX_LEN);
        assert_eq!(max_ok.len(), 512);
        is_chart_description_shape(&max_ok).unwrap();
        let too_long = "a".repeat(CHART_DESCRIPTION_MAX_LEN + 1);
        assert_eq!(too_long.len(), 513);
        let err = is_chart_description_shape(&too_long).unwrap_err();
        assert!(err.contains("512"), "got: {err:?}");
        assert!(err.contains("513"), "got: {err:?}");
    }

    #[test]
    fn chart_description_shape_rejects_each_unicode_bidi_override_codepoint() {
        // The Trojan Source (CVE-2021-42574) arm — pins every UAX #9
        // bidirectional-override / isolate format codepoint as a
        // structural rejection on the typed `:descricao` axis. The
        // per-byte non-ASCII pass deliberately admits Unicode letters
        // / em-dash / arrows because the canonical fixtures carry them
        // (`Canonical Rust→wasm32-wasip2`, `FIXME — describe this
        // caixa`); only the typed codepoint scan catches the nine
        // bidi-override codepoints that flip the rendered visual order
        // of every following character, so a future drop of any one
        // arm here surfaces as a `must be rejected` panic at this one
        // place rather than as a silent regression downstream. Each
        // case carries an alphabet-valid prefix + suffix so only the
        // bidi-override arm fires.
        for (cp, name) in [
            ('\u{202A}', "U+202A"),
            ('\u{202B}', "U+202B"),
            ('\u{202C}', "U+202C"),
            ('\u{202D}', "U+202D"),
            ('\u{202E}', "U+202E"),
            ('\u{2066}', "U+2066"),
            ('\u{2067}', "U+2067"),
            ('\u{2068}', "U+2068"),
            ('\u{2069}', "U+2069"),
        ] {
            let s = format!("alice{cp}bob");
            let err = is_chart_description_shape(&s)
                .err()
                .unwrap_or_else(|| panic!("chart description with {name} must be rejected"));
            assert!(
                err.contains(name),
                "chart description reason for {name} must name the codepoint verbatim; got {err:?}"
            );
            assert!(
                err.contains("bidirectional-override")
                    || err.contains("Unicode bidi")
                    || err.contains("Trojan Source"),
                "chart description reason for {name} must name the Trojan-Source banner; \
                 got {err:?}"
            );
        }
    }

    #[test]
    fn chart_description_shape_accepts_pure_rtl_text_without_bidi_override() {
        // Positive control on the bidi-override arm: pure visual
        // right-to-left scripts (Hebrew, Arabic) decode to non-bidi-
        // override codepoints and the predicate must accept them
        // natively — banning all RTL would regress every Hebrew /
        // Arabic-authored caixa, which the substrate explicitly
        // supports via the non-ASCII byte arm. The structural axis the
        // bidi-override arm closes is the explicit direction-mark
        // codepoint, not the RTL script itself.
        for s in [
            // Hebrew word (RTL script, no bidi-override codepoint).
            "שלום",
            // Arabic word (RTL script, no bidi-override codepoint).
            "مرحبا",
            // Mixed LTR / RTL caixa — the canonical multilingual
            // description shape every YAML 1.2 + Helm v3 + Artifact
            // Hub consumer round-trips losslessly.
            "Caixa para שלום",
        ] {
            is_chart_description_shape(s).unwrap_or_else(|e| {
                panic!("pure-RTL chart description {s:?} must pass without bidi override: {e:?}")
            });
        }
    }

    #[test]
    fn chart_description_shape_rejects_each_unicode_line_break_codepoint() {
        // The non-ASCII Unicode line-break arm — pins each of the three
        // UAX #14 / YAML 1.1 §4.1 line-break codepoints outside the
        // ASCII `\n` / `\r` bytes already caught at the per-byte pass.
        // Each case carries an alphabet-valid prefix + suffix so only
        // the line-break arm fires; the per-byte `\n` / `\r` arms
        // would shadow the codepoint scan if the line-break helper
        // accepted single-byte ASCII line terminators. A future drop
        // of any one arm here surfaces as a `must be rejected` panic
        // at this one place rather than as a silent regression
        // through YAML 1.1-compat downstream consumers (go-yaml v2 /
        // Helm v3 / kubectl). Mirrors the peer
        // `chart_maintainer_name_shape_rejects_each_unicode_line_break_codepoint`
        // on the sibling predicate — both predicates route through the
        // same lifted `find_unicode_line_break` helper.
        for (cp, name) in [
            ('\u{0085}', "U+0085"),
            ('\u{2028}', "U+2028"),
            ('\u{2029}', "U+2029"),
        ] {
            let s = format!("first line{cp}second line");
            let err = is_chart_description_shape(&s)
                .err()
                .unwrap_or_else(|| panic!("chart description with {name} must be rejected"));
            assert!(
                err.contains(name),
                "chart description reason for {name} must name the codepoint verbatim; got {err:?}"
            );
            assert!(
                err.contains("line-break") || err.contains("UAX #14") || err.contains("YAML 1.1"),
                "chart description reason for {name} must name the Unicode-line-break banner; \
                 got {err:?}"
            );
        }
    }

    #[test]
    fn chart_description_shape_accepts_non_line_break_unicode() {
        // Positive control on the line-break arm: the predicate must
        // accept every non-line-break Unicode shape the canonical
        // fixtures carry. Pinned alongside the per-codepoint rejection
        // sweep so a future helper widening that accidentally rejects
        // a non-line-break codepoint (the structural-floor regression
        // class) surfaces here as a single-source-of-truth pin. The
        // canonical multilingual descriptions, RTL text, em-dash and
        // arrows must all pass.
        for s in [
            "Canonical Rust→wasm32-wasip2 caixa Servico.",
            "FIXME — describe this caixa",
            "Caixa para שלום",
            "日本語の説明テスト",
            // U+00A0 NO-BREAK SPACE is NOT a line-break codepoint
            // (UAX #14 class GL — Glue, non-breaking) — must pass.
            "Caixa\u{00A0}for tests",
        ] {
            is_chart_description_shape(s).unwrap_or_else(|e| {
                panic!(
                    "non-line-break Unicode chart description {s:?} must pass without rejection: \
                     {e:?}"
                )
            });
        }
    }

    #[test]
    fn chart_description_shape_rejects_each_unicode_invisible_format_codepoint() {
        // The Unicode invisible-format arm — pins each of the eight
        // BMP Cf-category zero-width codepoints with no visible glyph
        // in any conforming font. The per-byte non-ASCII pass
        // deliberately admits multi-byte UTF-8 sequences (Unicode
        // letters / arrows / em-dash are canonical fixtures); only the
        // typed codepoint scan catches these eight. Each case carries
        // an alphabet-valid prefix + suffix so only the invisible-
        // format arm fires. A future drop of any one arm here surfaces
        // as a `must be rejected` panic at this one place rather than
        // as a silent regression through invisible-codepoint-homograph
        // downstream consumers (Artifact Hub description-search
        // misses, byte-level diff / grep / equality disagreement with
        // the visible-glyph match). Peer of
        // `chart_maintainer_name_shape_rejects_each_unicode_invisible_format_codepoint`
        // on the sibling predicate — both predicates route through the
        // same lifted `find_unicode_invisible_format` helper. Covers
        // the four paste-from-Word / paste-from-BOM-editor / paste-
        // from-typesetting shapes (U+00AD / U+200B / U+2060 / U+FEFF)
        // and the four math-formula invisible operators (U+2061
        // FUNCTION APPLICATION / U+2062 INVISIBLE TIMES / U+2063
        // INVISIBLE SEPARATOR / U+2064 INVISIBLE PLUS — the canonical
        // paste-from-MathJax / paste-from-LaTeX-rendered-formula
        // footgun where the renderer emits an invisible operator
        // between adjacent symbols for screen-reader operator
        // semantics).
        for (cp, name) in [
            ('\u{00AD}', "U+00AD"),
            ('\u{200B}', "U+200B"),
            ('\u{2060}', "U+2060"),
            ('\u{2061}', "U+2061"),
            ('\u{2062}', "U+2062"),
            ('\u{2063}', "U+2063"),
            ('\u{2064}', "U+2064"),
            ('\u{FEFF}', "U+FEFF"),
        ] {
            let s = format!("Canonical{cp}Servico");
            let err = is_chart_description_shape(&s)
                .err()
                .unwrap_or_else(|| panic!("chart description with {name} must be rejected"));
            assert!(
                err.contains(name),
                "chart description reason for {name} must name the codepoint verbatim; got {err:?}"
            );
            assert!(
                err.contains("invisible-format")
                    || err.contains("Cf-category")
                    || err.contains("zero-width"),
                "chart description reason for {name} must name the invisible-format banner; \
                 got {err:?}"
            );
        }
    }

    #[test]
    fn chart_description_shape_accepts_non_invisible_format_unicode() {
        // Positive control on the invisible-format arm: the predicate
        // must accept every non-invisible-format Unicode shape canonical
        // fixtures carry — including U+200C ZWNJ / U+200D ZWJ
        // (legitimate compositional load in Indic / Persian scripts and
        // emoji ZWJ sequences) and U+200E LRM / U+200F RLM (legitimate
        // single-character direction hints in mixed-script prose). A
        // future helper widening that accidentally rejects any of these
        // would regress legitimate fixture shapes and surfaces here as
        // a single-source-of-truth pin. Mirrors
        // `chart_maintainer_name_shape_accepts_non_invisible_format_unicode`
        // on the sibling predicate.
        for s in [
            "Canonical Rust→wasm32-wasip2 caixa Servico.",
            "FIXME — describe this caixa",
            // Emoji ZWJ sequence (U+200D) — must NOT be rejected: the
            // canonical multi-codepoint emoji authoring shape every
            // chart-aware UI renders as a single glyph.
            "Caixa for the 👨\u{200D}💻 family",
            // ZWNJ (U+200C) — legitimate Persian / Indic script
            // composition; the helper must NOT claim it.
            "Caixa for می\u{200C}باشد",
            // Bidi marks LRM (U+200E) and RLM (U+200F) — legitimate
            // single-character direction hints, separate class from
            // the bidi *overrides* the prior helper rejects.
            "Caixa for ASCII\u{200E}embedded in RTL",
            "Caixa for \u{200F}RTL hint",
        ] {
            is_chart_description_shape(s).unwrap_or_else(|e| {
                panic!(
                    "non-invisible-format Unicode chart description {s:?} must pass without \
                     rejection: {e:?}"
                )
            });
        }
    }

    // ── is_chart_maintainer_name_shape — shared `:autores` chart-maintainer predicate ──

    #[test]
    fn chart_maintainer_name_shape_accepts_canonical_forms() {
        // Substrate-side pin: the predicate accepts every canonical
        // chart-maintainer-name shape the `:autores` axis carries.
        // Drift between this list and the per-axis
        // `manifest::tests::validate_autores_accepts_canonical_forms`
        // positive-set sweep surfaces here — one source of truth for
        // the rule. Covers the hello-rio / checkout-aplicacao
        // `:autores ("pleme-io")` fixture, the multi-author
        // `"Pleme Contributors"` shape, and the canonical Helm
        // `"name <email>"` shape downstream packaging surfaces emit.
        for s in [
            "pleme-io",
            "Pleme Contributors",
            "alice <alice@example.com>",
            "bob <bob@example.com>",
            "Acme Corporation",
            "x",
        ] {
            is_chart_maintainer_name_shape(s).unwrap_or_else(|e| {
                panic!("canonical chart maintainer name {s:?} must pass: {e:?}")
            });
        }
    }

    #[test]
    fn chart_maintainer_name_shape_rejects_each_arm_with_substring_pinned_reason() {
        // Substrate-side diagnostic-shape pin: each arm surfaces its
        // own distinct reason substring. Pinned here so a future
        // reason-wording rephrase that drops any of these substrings
        // surfaces at this one place, not piecemeal across every
        // per-axis test sweep. Mirrors
        // `chart_description_shape_rejects_each_arm_with_substring_pinned_reason`
        // on the peer predicate.
        for (s, needle) in [
            // Leading whitespace — paste-from-aligned-doc.
            (" pleme-io", "whitespace"),
            // Trailing whitespace — paste-from-doc.
            ("pleme-io ", "whitespace"),
            // Tab inside — tab-from-aligned-doc.
            ("Pleme\tContributors", "tab"),
            // Newline — paste-from-multiline-doc (author pasted
            // multi-line author block into one entry).
            ("alice\nbob", "newline"),
            // Carriage return — paste-from-Windows-CRLF-doc.
            ("alice\rbob", "carriage return"),
            // NUL byte — paste-from-binary-blob.
            ("alice\x00bob", "control character"),
            // BEL byte — paste-from-binary-blob.
            ("alice\x07bob", "control character"),
            // ESC byte — paste-from-binary-blob.
            ("alice\x1bbob", "control character"),
            // DEL byte (0x7F).
            ("alice\x7fbob", "control character"),
        ] {
            let err = is_chart_maintainer_name_shape(s)
                .err()
                .unwrap_or_else(|| panic!("chart maintainer name {s:?} must be rejected"));
            assert!(
                err.contains(needle),
                "chart maintainer name {s:?} reason must contain {needle:?}; got {err:?}"
            );
        }
    }

    #[test]
    fn chart_maintainer_name_shape_accepts_unicode() {
        // Positive control on the non-ASCII arm: the predicate must
        // accept Unicode beyond the ASCII alphabet — realistic
        // maintainer names carry Unicode (`François`, `日本語`,
        // `naïve`), and every downstream consumer (YAML 1.2, Helm v3,
        // every chart-aware UI) round-trips Unicode losslessly. A
        // future tightening that bans non-ASCII bytes would regress
        // every Unicode-named maintainer and surface here as a
        // regression. Mirrors the peer
        // `chart_description_shape_accepts_unicode`.
        for s in [
            "François Dupont",
            "日本語の名前",
            "naïve <naive@example.com>",
            "André",
        ] {
            is_chart_maintainer_name_shape(s)
                .unwrap_or_else(|e| panic!("Unicode chart maintainer name {s:?} must pass: {e:?}"));
        }
    }

    #[test]
    fn chart_maintainer_name_shape_rejects_empty_defensively() {
        // The predicate is called from `crate::Caixa::validate_autores`
        // only after the per-axis `AutorEmpty` arm has fired at
        // validate time; re-checking here keeps the predicate usable
        // from any future call site without an empty-precondition
        // footgun. Same defensive empty-check `is_dns_1123_label`,
        // `is_gateway_api_http_path`, `is_wit_world_ref`,
        // `is_nats_subject`, `is_wasi_keyvalue_slot`,
        // `is_git_ref_name`, `is_git_oid`, `is_git_repo_url`,
        // `is_cargo_feature_name`, `is_spdx_expression_shape`, and
        // `is_chart_description_shape` carry at their call sites.
        let err = is_chart_maintainer_name_shape("").unwrap_err();
        assert!(err.contains("empty"), "got: {err:?}");
    }

    #[test]
    fn chart_maintainer_name_shape_rejects_at_129_byte_boundary() {
        // The 128-byte cap pin — both the boundary-exceeding case and
        // the boundary-accepting case in one place, so a future cap
        // shift surfaces both arms simultaneously, mirroring the peer
        // cap-boundary pins (`chart_description_shape_rejects_at_513_byte_boundary`
        // on the 512-byte sibling, `spdx_expression_shape_rejects_at_257_byte_boundary`
        // on the 256-byte sibling). Constructed as a single all-`a`
        // token so only the cap arm fires (128 `a` bytes is
        // alphabet-valid).
        let max_ok = "a".repeat(CHART_MAINTAINER_NAME_MAX_LEN);
        assert_eq!(max_ok.len(), 128);
        is_chart_maintainer_name_shape(&max_ok).unwrap();
        let too_long = "a".repeat(CHART_MAINTAINER_NAME_MAX_LEN + 1);
        assert_eq!(too_long.len(), 129);
        let err = is_chart_maintainer_name_shape(&too_long).unwrap_err();
        assert!(err.contains("128"), "got: {err:?}");
        assert!(err.contains("129"), "got: {err:?}");
    }

    #[test]
    fn chart_maintainer_name_shape_rejects_each_unicode_bidi_override_codepoint() {
        // The Trojan Source (CVE-2021-42574) arm — pins every UAX #9
        // bidirectional-override / isolate format codepoint as a
        // structural rejection on the typed `:autores` axis. Mirrors
        // `chart_description_shape_rejects_each_unicode_bidi_override_codepoint`
        // on the peer predicate — both predicates route through the
        // same lifted `find_unicode_bidi_override` helper, so dropping
        // any one of the nine arms from the helper's match would
        // regress both peer test sweeps simultaneously at this one
        // structural floor rather than at piecemeal per-axis call
        // sites. The canonical attacker shape: an `:autores
        // "alice\u{202E}example.com<bob@"` entry renders in `helm
        // list`'s maintainer column / Artifact Hub as the visually-
        // reversed `alice<@bob>moc.elpmaxe` while riding verbatim
        // into the Chart.yaml `maintainers:` array — exactly the
        // class this arm closes.
        for (cp, name) in [
            ('\u{202A}', "U+202A"),
            ('\u{202B}', "U+202B"),
            ('\u{202C}', "U+202C"),
            ('\u{202D}', "U+202D"),
            ('\u{202E}', "U+202E"),
            ('\u{2066}', "U+2066"),
            ('\u{2067}', "U+2067"),
            ('\u{2068}', "U+2068"),
            ('\u{2069}', "U+2069"),
        ] {
            let s = format!("alice{cp}bob");
            let err = is_chart_maintainer_name_shape(&s)
                .err()
                .unwrap_or_else(|| panic!("chart maintainer name with {name} must be rejected"));
            assert!(
                err.contains(name),
                "chart maintainer name reason for {name} must name the codepoint verbatim; \
                 got {err:?}"
            );
            assert!(
                err.contains("bidirectional-override")
                    || err.contains("Unicode bidi")
                    || err.contains("Trojan Source"),
                "chart maintainer name reason for {name} must name the Trojan-Source banner; \
                 got {err:?}"
            );
        }
    }

    #[test]
    fn chart_maintainer_name_shape_accepts_pure_rtl_text_without_bidi_override() {
        // Positive control on the bidi-override arm: pure visual
        // right-to-left scripts (Hebrew, Arabic) decode to non-bidi-
        // override codepoints and the predicate must accept them
        // natively — banning all RTL would regress every Hebrew /
        // Arabic-authored maintainer-name entry, which the substrate
        // supports via the non-ASCII byte arm. Peer of
        // `chart_description_shape_accepts_pure_rtl_text_without_bidi_override`
        // on the sibling YAML-plain-style-scalar surface.
        for s in [
            // Pure Hebrew maintainer name.
            "שלום",
            // Pure Arabic maintainer name.
            "مرحبا",
            // Mixed-script — canonical multilingual maintainer
            // shape every YAML 1.2 + Helm v3 round-trips losslessly.
            "Acme שלום",
        ] {
            is_chart_maintainer_name_shape(s).unwrap_or_else(|e| {
                panic!(
                    "pure-RTL chart maintainer name {s:?} must pass without bidi override: {e:?}"
                )
            });
        }
    }

    #[test]
    fn chart_maintainer_name_shape_rejects_each_unicode_line_break_codepoint() {
        // The non-ASCII Unicode line-break arm — pins each of the three
        // UAX #14 / YAML 1.1 §4.1 line-break codepoints outside the
        // ASCII `\n` / `\r` bytes already caught at the per-byte pass.
        // The canonical YAML-1.1-vs-YAML-1.2 paste-from-doc footgun: an
        // `:autores "alice\u{2028}bob"` entry parses as one
        // `maintainers:` array entry through a YAML 1.2-strict parser
        // and as two entries through a YAML 1.1 parser (go-yaml v2 /
        // Helm v3). Mirrors
        // `chart_description_shape_rejects_each_unicode_line_break_codepoint`
        // on the peer predicate — both predicates route through the
        // same lifted `find_unicode_line_break` helper, so dropping
        // any one of the three arms from the helper's match would
        // regress both peer test sweeps simultaneously at this one
        // structural floor.
        for (cp, name) in [
            ('\u{0085}', "U+0085"),
            ('\u{2028}', "U+2028"),
            ('\u{2029}', "U+2029"),
        ] {
            let s = format!("alice{cp}bob");
            let err = is_chart_maintainer_name_shape(&s)
                .err()
                .unwrap_or_else(|| panic!("chart maintainer name with {name} must be rejected"));
            assert!(
                err.contains(name),
                "chart maintainer name reason for {name} must name the codepoint verbatim; \
                 got {err:?}"
            );
            assert!(
                err.contains("line-break") || err.contains("UAX #14") || err.contains("YAML 1.1"),
                "chart maintainer name reason for {name} must name the Unicode-line-break banner; \
                 got {err:?}"
            );
        }
    }

    #[test]
    fn chart_maintainer_name_shape_accepts_non_line_break_unicode() {
        // Positive control on the line-break arm: the predicate must
        // accept every non-line-break Unicode shape canonical
        // maintainer names carry. Pinned alongside the per-codepoint
        // rejection sweep so a future helper widening that
        // accidentally rejects a non-line-break codepoint surfaces
        // here as a single-source-of-truth pin. Peer of
        // `chart_description_shape_accepts_non_line_break_unicode`
        // on the sibling YAML-plain-style-scalar surface.
        for s in [
            "François Dupont",
            "日本語の名前",
            "naïve <naive@example.com>",
            "André",
            // U+00A0 NO-BREAK SPACE is NOT a line-break codepoint
            // (UAX #14 class GL — Glue, non-breaking) and is the
            // canonical authoring shape for unbreakable space inside
            // a multi-token maintainer name — must pass.
            "Acme\u{00A0}Corp",
        ] {
            is_chart_maintainer_name_shape(s).unwrap_or_else(|e| {
                panic!(
                    "non-line-break Unicode chart maintainer name {s:?} must pass without \
                     rejection: {e:?}"
                )
            });
        }
    }

    #[test]
    fn chart_maintainer_name_shape_rejects_each_unicode_invisible_format_codepoint() {
        // The Unicode invisible-format arm — pins each of the eight
        // BMP Cf-category zero-width codepoints with no visible glyph.
        // The canonical maintainer-identity homograph footgun: an
        // `:autores "alice\u{200B}"` entry renders identically to
        // `:autores "alice"` in `helm list` / Artifact Hub's
        // maintainer column, but the byte sequence is distinct — the
        // Artifact Hub maintainer-index lookup misses the authored
        // `"alice"` entry, a future CLA-signer lookup matches a
        // visually-identical-but-byte-distinct identity. Mirrors
        // `chart_description_shape_rejects_each_unicode_invisible_format_codepoint`
        // on the peer predicate — both predicates route through the
        // same lifted `find_unicode_invisible_format` helper, so
        // dropping any one of the eight arms from the helper's match
        // would regress both peer test sweeps simultaneously at this
        // one structural floor. Covers the four paste-from-Word /
        // paste-from-BOM-editor / paste-from-typesetting shapes
        // (U+00AD / U+200B / U+2060 / U+FEFF) and the four math-
        // formula invisible operators (U+2061 FUNCTION APPLICATION /
        // U+2062 INVISIBLE TIMES / U+2063 INVISIBLE SEPARATOR /
        // U+2064 INVISIBLE PLUS — paste-from-MathJax / paste-from-
        // LaTeX-rendered-formula footgun).
        for (cp, name) in [
            ('\u{00AD}', "U+00AD"),
            ('\u{200B}', "U+200B"),
            ('\u{2060}', "U+2060"),
            ('\u{2061}', "U+2061"),
            ('\u{2062}', "U+2062"),
            ('\u{2063}', "U+2063"),
            ('\u{2064}', "U+2064"),
            ('\u{FEFF}', "U+FEFF"),
        ] {
            let s = format!("alice{cp}bob");
            let err = is_chart_maintainer_name_shape(&s)
                .err()
                .unwrap_or_else(|| panic!("chart maintainer name with {name} must be rejected"));
            assert!(
                err.contains(name),
                "chart maintainer name reason for {name} must name the codepoint verbatim; \
                 got {err:?}"
            );
            assert!(
                err.contains("invisible-format")
                    || err.contains("Cf-category")
                    || err.contains("zero-width"),
                "chart maintainer name reason for {name} must name the invisible-format banner; \
                 got {err:?}"
            );
        }
    }

    #[test]
    fn chart_maintainer_name_shape_accepts_non_invisible_format_unicode() {
        // Positive control on the invisible-format arm: the predicate
        // must accept the legitimate-use codepoints the helper
        // deliberately excludes — U+200C ZWNJ / U+200D ZWJ (emoji ZWJ
        // sequences are canonical for modern maintainer-display names;
        // Indic / Persian script composition relies on ZWNJ to break
        // inappropriate ligatures) and U+200E LRM / U+200F RLM
        // (mixed-script direction hints are canonical for "Arabic name
        // with embedded ASCII email" shapes). Peer of
        // `chart_description_shape_accepts_non_invisible_format_unicode`
        // on the sibling YAML-plain-style-scalar surface.
        for s in [
            "François Dupont",
            "naïve <naive@example.com>",
            // Emoji ZWJ sequence (U+200D) — canonical multi-codepoint
            // emoji authoring shape.
            "Joe 👨\u{200D}💻 Developer",
            // ZWNJ (U+200C) — legitimate Persian / Indic composition.
            "Persian می\u{200C}باشد maintainer",
            // Bidi marks LRM / RLM — legitimate direction hints in
            // mixed-script maintainer names.
            "Arabic\u{200F}name <maintainer@example.com>",
            "ASCII\u{200E}embedded in RTL context",
        ] {
            is_chart_maintainer_name_shape(s).unwrap_or_else(|e| {
                panic!(
                    "non-invisible-format Unicode chart maintainer name {s:?} must pass without \
                     rejection: {e:?}"
                )
            });
        }
    }

    #[test]
    fn find_unicode_bidi_override_pins_the_nine_codepoint_accepted_set() {
        // The shared helper's accepted set — pinned in one place so
        // every per-predicate caller (`is_chart_description_shape`,
        // `is_chart_maintainer_name_shape`, every future free-form-
        // prose surface) reads from one canonical accepted set. The
        // nine UAX #9 bidirectional-override / isolate format
        // codepoints in document order, plus negative controls on
        // bytes the helper must NOT reject (ASCII / non-bidi Unicode
        // letters / arrows / em-dash / RTL letters). A future shift
        // in the accepted set surfaces here as a single-source-of-
        // truth edit at this one test rather than across every
        // per-predicate per-arm sweep.
        for cp in [
            '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}', '\u{2066}', '\u{2067}',
            '\u{2068}', '\u{2069}',
        ] {
            let s = format!("a{cp}b");
            assert_eq!(
                find_unicode_bidi_override(&s),
                Some(cp),
                "helper must flag bidi override U+{:04X} on input {s:?}",
                cp as u32
            );
        }
        for s in [
            "alice",
            "Canonical Rust→wasm32-wasip2",
            "FIXME — describe this caixa",
            "François Dupont",
            "日本語の説明",
            "naïve",
            "שלום",
            "مرحبا",
        ] {
            assert_eq!(
                find_unicode_bidi_override(s),
                None,
                "helper must accept {s:?} (no bidi-override codepoint)"
            );
        }
        // Empty input — defensive precondition for the helper's
        // call-site contract on any future caller that doesn't gate
        // emptiness ahead of the scan.
        assert_eq!(find_unicode_bidi_override(""), None);
    }

    #[test]
    fn find_unicode_line_break_pins_the_three_codepoint_accepted_set() {
        // The shared helper's accepted set — pinned in one place so
        // every per-predicate caller (`is_chart_description_shape`,
        // `is_chart_maintainer_name_shape`, every future free-form-
        // prose surface) reads from one canonical accepted set. The
        // three UAX #14 / YAML 1.1 §4.1 non-ASCII line-break
        // codepoints in document order, plus negative controls on
        // bytes the helper must NOT reject (ASCII text, Unicode
        // letters / arrows / em-dash / RTL letters, the canonical
        // non-line-break U+00A0 NBSP shape downstream YAML 1.2 +
        // Helm v3 + every chart-aware UI round-trip losslessly). A
        // future shift in the accepted set surfaces here as a
        // single-source-of-truth edit at this one test rather than
        // across every per-predicate per-arm sweep. Peer of
        // `find_unicode_bidi_override_pins_the_nine_codepoint_accepted_set`
        // on the sibling lifted-helper one trajectory earlier.
        for cp in ['\u{0085}', '\u{2028}', '\u{2029}'] {
            let s = format!("a{cp}b");
            assert_eq!(
                find_unicode_line_break(&s),
                Some(cp),
                "helper must flag line-break codepoint U+{:04X} on input {s:?}",
                cp as u32
            );
        }
        for s in [
            "alice",
            "Canonical Rust→wasm32-wasip2",
            "FIXME — describe this caixa",
            "François Dupont",
            "日本語の説明",
            "naïve",
            "שלום",
            "مرحبا",
            // U+00A0 NO-BREAK SPACE — UAX #14 class GL (Glue,
            // non-breaking) — must NOT be rejected: the canonical
            // unbreakable-space shape every typed maintainer-name
            // axis admits.
            "Acme\u{00A0}Corp",
            // U+0009 TAB and U+000A LF and U+000D CR — ASCII
            // line-break / whitespace bytes the per-byte arm on the
            // calling predicate already closes; the helper must NOT
            // claim them as its own (single-source-of-truth: ASCII
            // arms live in the per-byte loop, the helper closes the
            // non-ASCII codepoints).
            "alice\tbob",
            "alice\nbob",
            "alice\rbob",
        ] {
            assert_eq!(
                find_unicode_line_break(s),
                None,
                "helper must accept {s:?} (no non-ASCII line-break codepoint)"
            );
        }
        // Empty input — defensive precondition for the helper's
        // call-site contract on any future caller that doesn't gate
        // emptiness ahead of the scan.
        assert_eq!(find_unicode_line_break(""), None);
    }

    #[test]
    fn find_unicode_invisible_format_pins_the_eight_codepoint_accepted_set() {
        // The shared helper's accepted set — pinned in one place so
        // every per-predicate caller (`is_chart_description_shape`,
        // `is_chart_maintainer_name_shape`, every future free-form-
        // prose surface) reads from one canonical accepted set. The
        // eight BMP Cf-category zero-width codepoints in document
        // order — the four paste-from-Word / paste-from-BOM-editor /
        // paste-from-typesetting-doc shapes (U+00AD SHY / U+200B ZWSP /
        // U+2060 WJ / U+FEFF ZWNBSP-BOM) and the four math-formula
        // invisible operators (U+2061 FUNCTION APPLICATION / U+2062
        // INVISIBLE TIMES / U+2063 INVISIBLE SEPARATOR / U+2064
        // INVISIBLE PLUS — paste-from-MathJax / paste-from-LaTeX-
        // rendered-formula / paste-from-InDesign-math-equation
        // shapes) — plus negative controls on codepoints the helper
        // must NOT reject — the deliberate exclusions: U+200C ZWNJ /
        // U+200D ZWJ (emoji ZWJ sequences + Indic / Persian script
        // composition) and U+200E LRM / U+200F RLM (mixed-script
        // direction hints). A future shift in the accepted set
        // surfaces here as a single-source-of-truth edit at this one
        // test rather than across every per-predicate per-arm sweep.
        // Third pin in the UAX-driven render-determinism trio (peer of
        // `find_unicode_bidi_override_pins_the_nine_codepoint_accepted_set`
        // on the visual-order axis and
        // `find_unicode_line_break_pins_the_three_codepoint_accepted_set`
        // on the single-line/multi-line axis).
        for cp in [
            '\u{00AD}', '\u{200B}', '\u{2060}', '\u{2061}', '\u{2062}', '\u{2063}', '\u{2064}',
            '\u{FEFF}',
        ] {
            let s = format!("a{cp}b");
            assert_eq!(
                find_unicode_invisible_format(&s),
                Some(cp),
                "helper must flag invisible-format codepoint U+{:04X} on input {s:?}",
                cp as u32
            );
        }
        for s in [
            "alice",
            "Canonical Rust→wasm32-wasip2",
            "FIXME — describe this caixa",
            "François Dupont",
            "日本語の説明",
            "naïve",
            "שלום",
            "مرحبا",
            // U+00A0 NO-BREAK SPACE — class GL (Glue), visible-width
            // codepoint — must NOT be claimed by the invisible-format
            // helper (the canonical unbreakable-space shape).
            "Acme\u{00A0}Corp",
            // U+200C ZWNJ — deliberately excluded (Indic / Persian
            // composition + emoji ZWJ-adjacent context).
            "می\u{200C}باشد",
            // U+200D ZWJ — deliberately excluded (emoji ZWJ
            // sequences are canonical: 👨‍💻 is MAN + ZWJ + LAPTOP).
            "Joe 👨\u{200D}💻 Developer",
            // U+200E LRM — deliberately excluded (direction-hint
            // mark, not a direction-override; legitimate in
            // mixed-script prose).
            "ASCII\u{200E}embedded",
            // U+200F RLM — deliberately excluded (mirror of LRM
            // on the RTL axis).
            "Arabic\u{200F}name",
            // Bidi-override codepoints (U+202A..U+202E, U+2066..U+2069)
            // — caught by the sibling `find_unicode_bidi_override`
            // helper, not this one (single-source-of-truth: each
            // helper closes exactly its class).
            "alice\u{202E}bob",
            // Line-break codepoints (U+0085, U+2028, U+2029) — caught
            // by the sibling `find_unicode_line_break` helper.
            "alice\u{2028}bob",
        ] {
            assert_eq!(
                find_unicode_invisible_format(s),
                None,
                "helper must accept {s:?} (no invisible-format codepoint in the four-codepoint set)"
            );
        }
        // Empty input — defensive precondition for the helper's
        // call-site contract on any future caller that doesn't gate
        // emptiness ahead of the scan.
        assert_eq!(find_unicode_invisible_format(""), None);
    }

    // ── is_chart_keyword_shape — shared `:etiquetas` chart-keyword predicate ──

    #[test]
    fn chart_keyword_shape_accepts_canonical_forms() {
        // Substrate-side pin: the predicate accepts every canonical
        // chart-keyword shape the `:etiquetas` axis carries. Drift
        // between this list and the per-axis
        // `manifest::tests::validate_etiquetas_accepts_canonical_shaped_forms`
        // positive-set sweep surfaces here — one source of truth for
        // the rule. Covers the example fixtures'
        // `:etiquetas` lists (`"example"`, `"aplicacao"`, `"mesh"`,
        // `"ecommerce"`, `"demo"`, `"infrastructure"`, `"aws"`,
        // `"akeyless"`, `"pangea-native"`) and the substrate-fixed
        // tags caixa-helm unions in at chart render (`"lareira"`,
        // `"wasm"`, `"tatara-lisp"`, `"caixa-servico"`).
        for s in [
            "example",
            "aplicacao",
            "mesh",
            "ecommerce",
            "demo",
            "infrastructure",
            "aws",
            "akeyless",
            "pangea-native",
            "hello-world",
            "wasm",
            "rust",
            "tatara-lisp",
            "caixa-servico",
            "lareira",
            "Foo",
            "Bar123",
            "x",
            "snake_case_tag",
        ] {
            is_chart_keyword_shape(s)
                .unwrap_or_else(|e| panic!("canonical chart keyword {s:?} must pass: {e:?}"));
        }
    }

    #[test]
    fn chart_keyword_shape_rejects_each_arm_with_substring_pinned_reason() {
        // Substrate-side diagnostic-shape pin: each arm surfaces its
        // own distinct reason substring. Pinned here so a future
        // reason-wording rephrase that drops any of these substrings
        // surfaces at this one place, not piecemeal across every
        // per-axis test sweep. Mirrors
        // `chart_maintainer_name_shape_rejects_each_arm_with_substring_pinned_reason`
        // on the peer predicate.
        for (s, needle) in [
            // Leading whitespace — paste-from-aligned-doc.
            (" mesh", "whitespace"),
            // Leading hyphen — kebab-leak footgun.
            ("-foo", "`-`"),
            // Leading underscore — snake-leak footgun.
            ("_foo", "`_`"),
            // Leading digit — paste-from-numbered-list footgun.
            ("1foo", "digit"),
            // Embedded whitespace — multi-tag-blob footgun.
            ("web service", "whitespace"),
            // Tab inside — tab-from-aligned-doc.
            ("mesh\thttp", "whitespace"),
            // Newline — paste-from-multiline-doc.
            ("mesh\nhttp", "newline"),
            // Carriage return — paste-from-Windows-CRLF-doc.
            ("mesh\rhttp", "carriage return"),
            // Comma — CSV-list-separator confusion.
            ("mesh,http", "`,`"),
            // Slash — path-separator confusion.
            ("caixa/servico", "`/`"),
            // Semicolon — alt-list-separator confusion.
            ("mesh;http", "`;`"),
            // Period — namespace / version-suffix confusion.
            ("http.1", "`.`"),
            // NUL byte — paste-from-binary-blob.
            ("mesh\x00http", "control character"),
            // DEL byte (0x7F).
            ("mesh\x7fhttp", "control character"),
            // Non-ASCII inside.
            ("café", "non-ASCII"),
            // Non-ASCII leading.
            ("éclair", "non-ASCII"),
        ] {
            let err = is_chart_keyword_shape(s)
                .err()
                .unwrap_or_else(|| panic!("chart keyword {s:?} must be rejected"));
            assert!(
                err.contains(needle),
                "chart keyword {s:?} reason must contain {needle:?}; got {err:?}"
            );
        }
    }

    #[test]
    fn chart_keyword_shape_rejects_empty_defensively() {
        // The predicate is called from `crate::Caixa::validate_etiquetas`
        // only after the per-axis `EtiquetaEmpty` arm has fired at
        // validate time; re-checking here keeps the predicate usable
        // from any future call site without an empty-precondition
        // footgun. Same defensive empty-check `is_dns_1123_label`,
        // `is_gateway_api_http_path`, `is_wit_world_ref`,
        // `is_nats_subject`, `is_wasi_keyvalue_slot`,
        // `is_git_ref_name`, `is_git_oid`, `is_git_repo_url`,
        // `is_cargo_feature_name`, `is_spdx_expression_shape`,
        // `is_chart_description_shape`, and
        // `is_chart_maintainer_name_shape` carry at their call sites.
        let err = is_chart_keyword_shape("").unwrap_err();
        assert!(err.contains("empty"), "got: {err:?}");
    }

    #[test]
    fn chart_keyword_shape_rejects_at_21_byte_boundary() {
        // The 20-byte cap pin — both the boundary-exceeding case and
        // the boundary-accepting case in one place, so a future cap
        // shift surfaces both arms simultaneously, mirroring the peer
        // cap-boundary pins
        // (`chart_maintainer_name_shape_rejects_at_129_byte_boundary`
        // on the 128-byte sibling,
        // `chart_description_shape_rejects_at_513_byte_boundary` on
        // the 512-byte sibling). Constructed as a single all-`a`
        // token so only the cap arm fires (20 `a` bytes is alphabet-
        // valid).
        let max_ok = "a".repeat(CHART_KEYWORD_MAX_LEN);
        assert_eq!(max_ok.len(), 20);
        is_chart_keyword_shape(&max_ok).unwrap();
        let too_long = "a".repeat(CHART_KEYWORD_MAX_LEN + 1);
        assert_eq!(too_long.len(), 21);
        let err = is_chart_keyword_shape(&too_long).unwrap_err();
        assert!(err.contains("20"), "got: {err:?}");
        assert!(err.contains("21"), "got: {err:?}");
    }
}
