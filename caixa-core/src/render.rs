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
///     reserved set.
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
            memory: Some(1024),
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
}
