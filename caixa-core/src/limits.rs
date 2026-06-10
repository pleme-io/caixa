//! Lunatic-style per-process resource limits — the typed slot of
//! `caixa.lisp` that wasm-engine consumes at component instantiation.
//!
//! See `theory/INSPIRATIONS.md` §III.1 for the prior-art frame: every
//! caixa Servico runs sandboxed by default; no "trust the author".
//!
//! ```lisp
//! (defcaixa
//!   :nome   "my-service"
//!   :versao "0.1.0"
//!   :kind   Servico
//!   :limits ((:memory     "64MiB")     ;; max linear memory per instance
//!            (:fuel       1000000)     ;; max wasm-instructions per request
//!            (:wall-clock "30s")       ;; max wall-clock per request
//!            (:cpu        "500m"))     ;; soft cgroup CPU share (millicores)
//!   :servicos ("servicos/my-service.computeunit.yaml"))
//! ```
//!
//! Authors omit the slot for "no limits" (today's behavior). When set,
//! wasm-engine M2 wires:
//!
//!   - [`LimitsSpec::memory`]      → `wasmtime::StoreLimits::memory_size`
//!   - [`LimitsSpec::fuel`]        → `Store::set_fuel` + per-tick refill
//!   - [`LimitsSpec::wall_clock`]  → epoch deadline cancellation
//!   - [`LimitsSpec::cpu`]         → cgroup-v2 hint propagated via the pod spec

use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// Hard upper bound for `:limits :memory`, in bytes — the
/// `wasm32-wasip2` linear-memory ceiling. The canonical caixa Servico
/// compilation target ([`theory/CAIXA-SDLC.md` §V — *Substrate /
/// Nix*][sdlc-v]) is `wasm32-wasip2`, whose linear memory is 32-bit-
/// addressed at a 64 KiB page size; the in-spec maximum is
/// `2^16 pages × 2^16 bytes/page = 2^32` bytes = 4 GiB exactly.
/// A `:limits :memory` value above this bound is structurally
/// unreachable under wasm32: wasmtime's `Store::limiter` cannot grow
/// past the 32-bit address space, so an authored `"8GiB"` either
/// silently saturates at the engine's effective cap or surfaces as a
/// `memory.grow` trap at runtime, far from the source caixa.lisp.
///
/// Pairs with [`LimitsError::MemoryZero`] (the zero-floor gate added
/// by the prior typed-shape lift on this axis) to bracket the valid
/// `:memory` set top-to-bottom: every validated value lies in
/// `1..=LIMITS_MEMORY_WASM32_MAX_BYTES` (inclusive on both ends).
/// Renderers ([`crate::render::servico_m2_overlay`] and the M2.5
/// `wasm-engine` instantiator the ABSORPTION-ROADMAP names as the
/// downstream wiring) consume the typed value with no re-validation
/// — the value-shape gate is the structural contract.
///
/// Lifted as a typed `pub const` (rather than an inline literal at
/// the [`LimitsSpec::validate`] call site) so the bound has exactly
/// one source of truth — a future axis reaching for the same value
/// (a future `memory64`-target opt-in raising the cap to 2^64, a
/// wasm-engine smoke test asserting the engine's effective limit
/// matches the typed bound, the M4 `mesh.pleme.io/v1alpha1/Caixa`
/// CR materializer's per-`:limits :memory` admission webhook)
/// reads from one place. Same shape every other typed bound in this
/// crate carries ([`crate::render::DNS_1123_LABEL_MAX_LEN`],
/// [`crate::render::GATEWAY_API_HTTP_PATH_MAX_LEN`],
/// [`crate::render::NATS_SUBJECT_MAX_LEN`]).
///
/// [sdlc-v]: https://github.com/pleme-io/theory/blob/main/CAIXA-SDLC.md
pub const LIMITS_MEMORY_WASM32_MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Structural floor for `:limits :memory`, in bytes — the
/// `wasm32-wasip2` linear-memory page size. The wasm spec defines
/// linear memory in fixed 64 KiB pages (`2^16` bytes); every typed
/// memory cap is consumed by `wasmtime::StoreLimits::memory_size` as a
/// per-component byte ceiling against which the engine checks every
/// `memory.grow` request. A cap below one page (`< 65536` bytes) is
/// structurally a "no wasm linear memory allowed" cap — instantiation
/// of any wasm component that declares `(memory 1)` (i.e. min=1 page,
/// the canonical default for every cdylib-shaped wasm component cargo
/// emits) fails immediately with `memory minimum size of 1 pages
/// exceeds memory limits`; a min=0 component traps the first
/// `memory.grow(1)` because the next-page allocation would cross the
/// sub-page cap. Either way the typed value the wasm-engine consumes
/// is operationally indistinguishable from [`LimitsError::MemoryZero`]
/// (no memory at all), but the diagnostic surfaces at engine-load
/// time rather than at caixa-build time, far from the source
/// caixa.lisp.
///
/// Pairs with [`LIMITS_MEMORY_WASM32_MAX_BYTES`] (the 4 GiB upper
/// cap added by the prior typed-shape lift on this axis) to bracket
/// the valid `:memory` set top-to-bottom in *operational* units, not
/// just byte units: every validated value lies in
/// `LIMITS_MEMORY_WASM32_PAGE_BYTES..=LIMITS_MEMORY_WASM32_MAX_BYTES`
/// inclusive on both ends — i.e. at least one wasm32 linear memory
/// page can be allocated, and at most the wasm32 address-space
/// ceiling fits.
///
/// Lifted as a typed `pub const` (rather than an inline literal at
/// the [`LimitsSpec::validate`] call site) so the bound has exactly
/// one source of truth — a future axis reaching for the same value
/// (a future `memory64`-target opt-in raising the page size, the M4
/// `mesh.pleme.io/v1alpha1/Caixa` CR materializer's per-`:limits
/// :memory` admission webhook, a wasm-engine smoke test asserting
/// every instantiated component can fit one page within its
/// configured cap) reads from one place. Same single-source-of-truth
/// shape every typed bound in this crate carries
/// ([`LIMITS_MEMORY_WASM32_MAX_BYTES`],
/// [`crate::render::DNS_1123_LABEL_MAX_LEN`]).
pub const LIMITS_MEMORY_WASM32_PAGE_BYTES: u64 = 64 * 1024;

/// Upper-bound ceiling on the `:limits :wall-clock` axis — every
/// validated [`LimitsSpec::wall_clock`] past [`LimitsSpec::validate`]
/// lies in `1ms..=LIMITS_WALL_CLOCK_MAX` (inclusive on both ends,
/// integer-millisecond magnitudes by the canonical-form gate
/// immediately preceding).
///
/// The typed field is `Option<Duration>` (the zero-floor arm
/// [`LimitsError::WallClockZero`] already rejects `Duration::ZERO`, and
/// the canonical-form arm [`LimitsError::WallClockNotCanonical`]
/// already rejects sub-millisecond residue), so a programmatic struct
/// literal (`LimitsSpec { wall_clock: Some(Duration::from_secs(86_400)),
/// .. }` — 24h) and the equivalent author-surface form
/// (`(:limits (:wall-clock "24h"))` — the codec emits `"<n>h"` for any
/// integer-hour magnitude) both round-trip cleanly through serde — a
/// structurally unbounded `Duration` ceiling. A `:wall-clock` value far
/// above the per-process production band (Lunatic / Wasmtime documented
/// per-call deadlines sit in the seconds-to-minutes range; Kubernetes
/// activeDeadlineSeconds typical `≤ 3600s`; the longest per-request
/// timeout any upstream HTTP runtime documents — Kubernetes
/// ingress-nginx `proxy_read_timeout` — caps at the same 3600s) turns
/// the typed per-process deadline into a nominal-only contract: the
/// wasm-engine's epoch-deadline cancellation reaches for a `Duration`
/// so long no realistic synchronous wasm call can hit it, the runaway-
/// process invariant the MESH-COMPOSITION §V "no infinite blocking" CSE
/// invariant pins at the per-Servico layer degenerates to a runtime,
/// not build-time, contract. Pairs with the
/// [`crate::POLICY_TIMEOUT_MAX`] cap on the sibling `:politicas :timeout`
/// mesh-edge axis and the [`crate::POLICY_BREAKER_WINDOW_MAX`] cap on
/// the sibling `:politicas :circuit-breaker :window` rolling-window
/// axis — all three close the "structurally unbounded `Duration`
/// ceiling on a typed slot" footgun the prior zero-floor-and-canonical-
/// form-only checks left open.
///
/// The 1h (3600s = `3_600_000` ms) ceiling matches the largest unit
/// the shared duration codec emits (`"<n>h"` for any integer-hour
/// magnitude) — every value in the canonical authoring form's
/// `<integer><unit>` grammar at or below this cap renders to a clean
/// canonical string — and matches the two sibling typed-`Duration`
/// caps already lifted to this surface
/// ([`crate::POLICY_TIMEOUT_MAX`], [`crate::POLICY_BREAKER_WINDOW_MAX`]).
/// The three typed-`Duration` axes — per-process `:limits :wall-clock`,
/// per-edge `:politicas :timeout`, per-breaker `:politicas
/// :circuit-breaker :window` — now share a single uniform top edge so
/// the next typed-slot wiring (the wasm-engine M2.5 epoch-deadline
/// cancellation hook, the future caixa-helm `pleme-computeunit` chart's
/// `:limits` value mapping, the M4 `mesh.pleme.io/v1alpha1/Caixa` CR
/// materializer's per-`:limits :wall-clock` admission webhook) reaches
/// for any of the three knowing the value is in `1ms..=1h` without
/// re-validating at the renderer layer. The cap sits above the
/// documented per-request playbook band (Envoy / Istio / Linkerd
/// production `≤ 60s`, AWS App Mesh / ingress-nginx typical `≤ 300s`,
/// Kubernetes activeDeadlineSeconds typical `≤ 3600s`) and below the
/// clearly-pathological "effectively no deadline" floor (`24h`, `7d`,
/// `Duration::MAX`): a value the author can plausibly want for a
/// long-running synchronous workflow, but a hard wall above which the
/// per-process deadline is structurally a non-deadline.
///
/// Lifted as a typed `pub const` so the bound has exactly one source
/// of truth — the wasm-engine M2.5 epoch-deadline wiring, a wasm-engine
/// smoke test asserting the engine's epoch interrupt fires within the
/// typed bound, the M4 `mesh.pleme.io/v1alpha1/Caixa` CR materializer's
/// per-`:limits :wall-clock` admission webhook all read from one place.
/// Same shape every other typed upper bound in this crate carries
/// ([`LIMITS_MEMORY_WASM32_MAX_BYTES`], [`crate::POLICY_TIMEOUT_MAX`],
/// [`crate::POLICY_BREAKER_WINDOW_MAX`],
/// [`crate::render::DNS_1123_LABEL_MAX_LEN`],
/// [`crate::render::NATS_SUBJECT_MAX_LEN`]).
pub const LIMITS_WALL_CLOCK_MAX: Duration = Duration::from_secs(3600);

/// Upper-bound ceiling on the `:limits :cpu` axis, in Kubernetes
/// millicores — every validated [`LimitsSpec::cpu`] past
/// [`LimitsSpec::validate`] lies in `1..=LIMITS_CPU_MILLICORES_MAX`
/// (inclusive on both ends).
///
/// The typed field is `Option<u32>` (the zero-floor arm
/// [`LimitsError::CpuZero`] already rejects `Some(0)` — a zero cgroup
/// share starves the process), so a programmatic struct literal
/// (`LimitsSpec { cpu: Some(u32::MAX), .. }` — ≈ 4.3 million cores)
/// and the equivalent author-surface form (`(:limits (:cpu
/// "1000000m"))` — the millicore codec parses any `u32`-shaped
/// magnitude) both round-trip cleanly through serde — a structurally
/// unbounded `u32` ceiling. The runtime substrate consuming the value
/// ([`crate::render::servico_m2_overlay`]'s `pleme-computeunit.limits.cpu`
/// projection, the M2.5 `wasm-engine` instantiator the
/// `ABSORPTION-ROADMAP` names as the downstream wiring, the future
/// M4 `mesh.pleme.io/v1alpha1/Caixa` CR materializer's admission
/// webhook) lands the value verbatim as the K8s pod's
/// `resources.requests.cpu`. A value far above the largest commodity
/// node's vCPU count turns the typed slot into an unschedulable hint:
/// the Kubernetes scheduler refuses to bind the pod to any node
/// (insufficient `cpu` available), the Servico sits `Pending`
/// indefinitely, and the per-process CSE invariant (every typed
/// `:cpu` reaches a node) is a runtime, not build-time, contract —
/// the canonical declared-but-unschedulable footgun the sibling
/// `:limits :memory` wasm32-cap arm closes on its peer "cannot be
/// honored" shape.
///
/// The `128_000` (128 cores) ceiling matches the largest commercially
/// common non-metal cloud Kubernetes node vCPU count (AWS m7i.32xlarge
/// / c7i.32xlarge = 128 vCPU; Azure HBv3-128rs = 128 vCPU; GCP
/// c3-standard-128 = 128 vCPU — every major managed-Kubernetes provider
/// tops out at 128 vCPU on its general-purpose non-metal SKUs) and sits
/// two orders of magnitude above every realistic per-Servico
/// production-playbook band (the canonical caixa Servico runs in the
/// 100m–2000m band; the in-tree
/// `limits_slot_propagates_into_values_block` smoke test pins
/// `cpu: Some(500)` = 500m as the load-bearing example, peer to the
/// `caixa-flux` projector's identical 500m default). A value above this
/// cap is structurally unschedulable on any commercial managed
/// Kubernetes node pool: GKE Standard / EKS managed / AKS default
/// node-group SKU ladders cap at 128 vCPU per node for general-purpose
/// instance families, so a `:cpu` request above `128_000m` cannot bind to
/// any node the operator can provision through the standard
/// cloud-provider control plane. The wasm32-wasip2 single-threaded
/// execution model the canonical caixa Servico targets
/// ([`theory/CAIXA-SDLC.md` §V][sdlc-v]) reinforces the structural
/// argument: a single wasm component cannot saturate more than one
/// core, so even the Lunatic-style supervised-multi-process host
/// (`theory/INSPIRATIONS.md` §III.1) — which fans wasm processes across
/// the host runtime's Tokio thread pool — bounds its useful CPU request
/// to the host node's vCPU count, never higher.
///
/// Lifted as a typed `pub const` (rather than an inline literal at the
/// [`LimitsSpec::validate`] call site) so the bound has exactly one
/// source of truth — the future M4
/// `mesh.pleme.io/v1alpha1/Caixa` CR materializer's per-`:limits :cpu`
/// admission webhook, the caixa-helm `pleme-computeunit` chart's
/// resource-request mapping, the M2.5 `wasm-engine` host-runtime
/// thread-pool sizing hint all read from one place. Same shape every
/// other typed upper bound in this crate carries
/// ([`LIMITS_MEMORY_WASM32_MAX_BYTES`], [`LIMITS_WALL_CLOCK_MAX`],
/// [`crate::POLICY_TIMEOUT_MAX`], [`crate::POLICY_BREAKER_WINDOW_MAX`],
/// [`crate::POLICY_RATE_LIMIT_MAX`],
/// [`crate::render::DNS_1123_LABEL_MAX_LEN`]).
///
/// [sdlc-v]: https://github.com/pleme-io/theory/blob/main/CAIXA-SDLC.md
pub const LIMITS_CPU_MILLICORES_MAX: u32 = 128_000;

/// Per-process limits. All fields optional — `None` = unbounded for that axis.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LimitsSpec {
    /// Max linear memory in bytes. Authored as a byte-size string
    /// (`"64MiB"`, `"1GiB"`, `"512KB"`). Round-trips back to the same
    /// canonical string on serialize.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "ser_byte_size",
        deserialize_with = "de_byte_size"
    )]
    pub memory: Option<u64>,

    /// Max wasm instructions per outermost call (`wasmtime` fuel).
    /// Plain integer; `None` = unbounded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fuel: Option<u64>,

    /// Wall-clock cap per outermost call. Authored as a duration
    /// string (`"30s"`, `"500ms"`, `"2m"`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "ser_duration",
        deserialize_with = "de_duration"
    )]
    pub wall_clock: Option<Duration>,

    /// Soft CPU share. Authored as a Kubernetes-style millicore string
    /// (`"500m"` for half a core, `"2"` or `"2000m"` for two cores).
    /// Stored as millicores (u32).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "ser_millicores",
        deserialize_with = "de_millicores"
    )]
    pub cpu: Option<u32>,
}

impl LimitsSpec {
    /// True when no axis is bounded.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.memory.is_none()
            && self.fuel.is_none()
            && self.wall_clock.is_none()
            && self.cpu.is_none()
    }

    /// Reject operationally-meaningless zero values on every declared
    /// axis. Each axis remains optional — omitting a field expresses
    /// "no bound on this axis"; the bug being closed is *carrying* a
    /// zero value, which the wasm-engine consumes as "trap the first
    /// instruction" / "instantiation refused" / "immediate timeout"
    /// rather than the author's intended "an unspecified bound".
    ///
    /// Mirrors the discipline applied to `:politicas` axes in
    /// `AplicacaoSpec::validate` and to `SupervisorSpec::max_restarts`
    /// — every typed value carried by a slot is either absent or
    /// meaningfully non-zero.
    pub fn validate(&self) -> Result<(), LimitsError> {
        if self.memory == Some(0) {
            return Err(LimitsError::MemoryZero);
        }
        // Structural-floor gate on `:memory`: every validated value
        // must also accommodate at least one wasm32 linear memory
        // page ([`LIMITS_MEMORY_WASM32_PAGE_BYTES`] = 64 KiB). The
        // zero-floor arm immediately above closes the literal
        // `Some(0)` shape; this page-floor arm closes the structural-
        // zero class (`Some(1)`..`Some(65535)` — values that pass the
        // numeric zero check but that the wasm32 engine consumes as
        // "no memory at all" because instantiation of any component
        // declaring `(memory 1)` traps with `memory minimum size of 1
        // pages exceeds memory limits`, and a `(memory 0)` component
        // traps the first `memory.grow(1)` because the next-page
        // allocation would cross the sub-page cap). Until this gate
        // landed the byte-size codec accepted any `Option<u64>` past
        // zero (the prior numeric-zero arm's only floor), so
        // `(:memory "32KiB")` round-tripped cleanly through serde and
        // the per-axis CSE invariant (no value the wasm-engine can't
        // honor) was a runtime, not build-time, contract on every
        // sub-page input. Closes the same gap the wasm32 *upper*
        // ceiling closes on the top edge — the typed `:memory` axis
        // is now operationally bracketed
        // (`LIMITS_MEMORY_WASM32_PAGE_BYTES`..=`LIMITS_MEMORY_WASM32_MAX_BYTES`),
        // not just numerically (`1..=LIMITS_MEMORY_WASM32_MAX_BYTES`).
        // Same top-and-bottom-edge discipline the prior trajectory
        // applied to `:politicas :retries` (`PolicyRetriesZero` →
        // `PolicyRetriesExceedsCap`) and `:circuit-breaker
        // :max-failures` (`PolicyBreakerZeroFailures` →
        // `PolicyBreakerMaxFailuresExceedsCap`).
        if let Some(m) = self.memory {
            if m < LIMITS_MEMORY_WASM32_PAGE_BYTES {
                return Err(LimitsError::MemoryBelowWasm32Page { bytes: m });
            }
        }
        // Upper-bound gate on `:memory`: every validated value must
        // also fit within the wasm32-wasip2 linear-memory ceiling
        // ([`LIMITS_MEMORY_WASM32_MAX_BYTES`] = 4 GiB). The
        // zero-floor and page-floor arms immediately above and this
        // cap arm together bracket the typed `:memory` axis
        // structurally — every validated value lies in
        // `LIMITS_MEMORY_WASM32_PAGE_BYTES..=LIMITS_MEMORY_WASM32_MAX_BYTES`.
        // Until the prior cap gate landed the byte-size codec
        // accepted any `Option<u64>` past zero (the parser's only
        // upper bound was `u64::MAX`), so `(:memory "8GiB")`
        // round-tripped cleanly through serde and the per-axis CSE
        // invariant (no value the wasm-engine can't honor) was a
        // runtime, not build-time, contract. Same trajectory as the
        // typed-shape gates [`AplicacaoSpec::validate_politicas`]
        // (the canonical-rate-limit-window upper bound) and the
        // [`is_dns_1123_label`]/[`is_gateway_api_http_path`] length
        // caps lift to the typed surface.
        if let Some(m) = self.memory {
            if m > LIMITS_MEMORY_WASM32_MAX_BYTES {
                return Err(LimitsError::MemoryExceedsWasm32Cap { bytes: m });
            }
        }
        if self.fuel == Some(0) {
            return Err(LimitsError::FuelZero);
        }
        if matches!(self.wall_clock, Some(d) if d.is_zero()) {
            return Err(LimitsError::WallClockZero);
        }
        // Sub-millisecond residue gate on the typed `:wall-clock` axis.
        // The peer typed-`Duration` axes already routed through the
        // shared `supervisor::duration_codec` (`:politicas :timeout`
        // a4ae535, `:circuit-breaker :window` a4ae535) gate on
        // `is_integer_millisecond_duration` because the codec's `render`
        // truncates to `as_millis()` and parses with integer-ms
        // granularity; the in-module `render_duration` / `parse_duration`
        // pair in this crate carries the same `as_millis()`-truncation
        // shape, so the same sub-millisecond-residue footgun lived on
        // this axis until this gate landed. A programmatic struct
        // literal (`LimitsSpec { wall_clock: Some(Duration::from_micros(1500)), .. }`,
        // or any wasm-engine / M2.5 caller propagating a `Duration`
        // from a non-`<integer><unit>`-string source) would either
        // truncate-to-`ms` on first serialize (`from_micros(1500)` =
        // 1_500_000 ns → renders `"1ms"` → parses back to
        // `Duration::from_millis(1)` = 1_000_000 ns ≠ original) or —
        // for sub-millisecond magnitudes — render as the literal `"0s"`
        // (`from_micros(500)` → `as_millis() == 0` → renders `"0s"`)
        // the zero-floor arm immediately above rejects on re-validate,
        // either way breaking the THEORY.md §V.2.7 render-determinism
        // contract every typed slot carries. The shared predicate
        // [`crate::supervisor::duration_codec::is_integer_millisecond_duration`]
        // lives next to the codec — single source of truth, drift
        // between the codec's accepted granularity and any typed
        // `Duration` slot's accepted set is a single-edit fix at the
        // predicate rather than a silent round-trip break the next
        // consumer (the wasm-engine `wall_clock` deadline cancellation
        // MESH-COMPOSITION §V names, the future caixa-helm
        // `pleme-computeunit` chart's `:limits` value mapping) discovers
        // at apply time. The zero-floor arm strictly precedes this
        // canonical gate so `Duration::ZERO` (`subsec_nanos() == 0` — a
        // value the canonical arm would otherwise accept) surfaces the
        // more self-locating `WallClockZero` diagnostic with its
        // omit-axis remediation directly named, peer to the
        // `PolicyTimeoutZero` → `PolicyTimeoutNotCanonical` and
        // `PolicyBreakerZeroWindow` → `PolicyBreakerWindowNotCanonical`
        // cross-arm ordering on `:politicas`.
        if let Some(w) = self.wall_clock
            && !crate::supervisor::duration_codec::is_integer_millisecond_duration(w)
        {
            return Err(LimitsError::WallClockNotCanonical { wall_clock: w });
        }
        // Upper-bound gate on `:wall-clock`: every validated value must
        // also fit within the typed-`Duration` ceiling
        // ([`LIMITS_WALL_CLOCK_MAX`] = 1h = 3600s). The zero-floor,
        // canonical-form, and this cap arm together bracket the typed
        // `:wall-clock` axis structurally — every validated value lies
        // in `1ms..=LIMITS_WALL_CLOCK_MAX`, integer-millisecond
        // granularity. Until this gate landed the duration codec
        // accepted any `Duration` past zero with integer-ms residue
        // (the parser's only upper bound was `Duration::MAX`), so
        // `(:wall-clock "24h")` round-tripped cleanly through serde and
        // the per-process CSE invariant (no value the wasm-engine's
        // epoch-deadline cancellation can't honor as a meaningful
        // bound) was a runtime, not build-time, contract. Same
        // trajectory as the sibling typed-`Duration` cap lifts on
        // [`crate::POLICY_TIMEOUT_MAX`] (per-edge mesh-policy deadline)
        // and [`crate::POLICY_BREAKER_WINDOW_MAX`] (per-breaker
        // rolling-window) — the three typed-`Duration` axes now share
        // a single uniform top edge at the codec's largest emitted
        // unit. The canonical-form arm strictly precedes this cap so a
        // `Duration` that is *both* sub-millisecond and above-cap
        // surfaces the more fundamental round-trip-shape diagnostic
        // first (the cap's `1ms..=1h` remediation would be misleading
        // when no integer-ms form of the offending value exists). Same
        // posture every peer zero-then-shape-then-cap chain uses on
        // this surface (`MemoryZero` → `MemoryBelowWasm32Page` →
        // `MemoryExceedsWasm32Cap`).
        if let Some(w) = self.wall_clock
            && w > LIMITS_WALL_CLOCK_MAX
        {
            return Err(LimitsError::WallClockExceedsCap { wall_clock: w });
        }
        if self.cpu == Some(0) {
            return Err(LimitsError::CpuZero);
        }
        // Upper-bound gate on `:cpu`: every validated value must also
        // fit within the largest commercially-common non-metal cloud
        // Kubernetes node vCPU count ([`LIMITS_CPU_MILLICORES_MAX`] =
        // 128 cores = 128_000 millicores). The zero-floor arm
        // immediately above closes the literal `Some(0)` shape; this
        // cap arm closes the structurally-unschedulable class
        // (`Some(128_001)`..`Some(u32::MAX)` — values that pass the
        // numeric zero check but that the Kubernetes scheduler cannot
        // bind to any node because no general-purpose managed-K8s
        // SKU provides >128 vCPU per node). Until this gate landed
        // the millicore codec accepted any `Option<u32>` past zero
        // (the prior numeric-zero arm's only floor), so
        // `(:cpu "1000000m")` (1000 cores) round-tripped cleanly
        // through serde and the per-axis CSE invariant (no value the
        // Kubernetes scheduler can't honor) was a runtime, not
        // build-time, contract on every above-cap input: the
        // `pleme-computeunit` chart's `resources.requests.cpu`
        // landed verbatim, the pod sat `Pending` indefinitely with a
        // `0/N nodes are available: N Insufficient cpu` event, and
        // the typed `:cpu` slot became an unschedulable hint far from
        // the source caixa.lisp. Closes the same gap the
        // wasm32-wasip2 upper ceiling closes on the `:memory` axis —
        // the typed `:cpu` axis is now operationally bracketed
        // (`1..=LIMITS_CPU_MILLICORES_MAX`), not just numerically
        // (`1..=u32::MAX`). Same top-and-bottom-edge discipline the
        // prior trajectory applied to every sibling cap arm on this
        // surface ([`LimitsError::MemoryExceedsWasm32Cap`],
        // [`LimitsError::WallClockExceedsCap`],
        // [`crate::AplicacaoError::PolicyTimeoutExceedsCap`],
        // [`crate::AplicacaoError::PolicyRetriesExceedsCap`],
        // [`crate::AplicacaoError::PolicyBreakerMaxFailuresExceedsCap`],
        // [`crate::AplicacaoError::PolicyBreakerWindowExceedsCap`],
        // [`crate::AplicacaoError::PolicyRateLimitExceedsCap`],
        // [`crate::SupervisorError::MaxRestartsExceedsCap`]).
        if let Some(m) = self.cpu
            && m > LIMITS_CPU_MILLICORES_MAX
        {
            return Err(LimitsError::CpuExceedsCap { millicores: m });
        }
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LimitsError {
    #[error("byte-size: missing magnitude in {0:?}")]
    EmptyByteSize(String),
    #[error("byte-size: unknown unit {unit:?} (expected one of B, KB, MB, GB, KiB, MiB, GiB)")]
    UnknownByteUnit { unit: String },
    #[error("byte-size: failed to parse magnitude {0:?}")]
    BadByteMagnitude(String),
    #[error(
        "byte-size: magnitude {value:?} is not a non-negative integer — the canonical \
         authoring form for `:limits :memory` is `<integer><unit>` (e.g. `\"1024\"`, \
         `\"64MiB\"`, `\"1GiB\"`) with no decimal point and no leading `+` sign. A \
         fractional / decimal-shaped magnitude (`\"1.5KiB\"`, `\"1.0MiB\"`, `\"0.5GiB\"`, \
         `\"+1024\"`) round-trips through `render_byte_size` to a *different* canonical \
         form (`\"1536\"`, `\"1MiB\"`, `\"512MiB\"`, `\"1KiB\"`) on first serialize — \
         breaking the THEORY.md §V.2.7 render-determinism contract every typed slot \
         carries. Pick an integer magnitude in the unit that divides cleanly (write \
         `\"1536\"` instead of `\"1.5KiB\"`; `\"512MiB\"` instead of `\"0.5GiB\"`)"
    )]
    NonIntegerByteMagnitude { value: String },
    #[error("duration: missing magnitude in {0:?}")]
    EmptyDuration(String),
    #[error("duration: unknown unit {unit:?} (expected one of ms, s, m, h)")]
    UnknownDurationUnit { unit: String },
    #[error("duration: failed to parse magnitude {0:?}")]
    BadDurationMagnitude(String),
    #[error(
        "duration: magnitude {value:?} is not a non-negative integer — the canonical \
         authoring form for `:limits :wall-clock` is `<integer><unit>` (e.g. `\"30s\"`, \
         `\"500ms\"`, `\"2m\"`, `\"1h\"`) with no decimal point and no leading `+` sign. A \
         fractional / decimal-shaped magnitude (`\"1.5s\"`, `\"1.0s\"`, `\"0.5m\"`, \
         `\"+30s\"`, `\"-30s\"`) round-trips through `render_duration` to a *different* \
         canonical form (`\"1500ms\"`, `\"1s\"`, `\"30s\"`, `\"30s\"`) on first serialize \
         — breaking the THEORY.md Part V render-determinism contract every typed slot \
         carries. Pick an integer magnitude in the unit that divides cleanly (write \
         `\"1500ms\"` instead of `\"1.5s\"`; `\"30s\"` instead of `\"0.5m\"`)"
    )]
    NonIntegerDurationMagnitude { value: String },
    #[error("millicores: bad value {0:?} (expected `<int>m` or `<int>`)")]
    BadMillicores(String),
    #[error(
        "millicores: magnitude {value:?} is not a non-negative integer — the canonical \
         authoring form for `:limits :cpu` is `<integer>m` (Kubernetes millicores, e.g. \
         `\"500m\"` for half a core, `\"2000m\"` for two cores) or the bare-core \
         shorthand `<integer>` (e.g. `\"2\"` = `\"2000m\"`), with no decimal point and \
         no leading `+` sign. A fractional / decimal-shaped magnitude (`\"1.5\"`, \
         `\"500.0m\"`, `\"+500m\"`, `\"-100m\"`) round-trips through `render_millicores` \
         to a *different* canonical form (`\"1500m\"`, `\"500m\"`, `\"500m\"`, \
         parse-rejection) on first serialize — breaking the THEORY.md Part V \
         render-determinism contract every typed slot carries. Pick an integer magnitude \
         in millicores (write `\"1500m\"` instead of `\"1.5\"`; `\"500m\"` instead of \
         `\"500.0m\"`)"
    )]
    NonIntegerMillicoreMagnitude { value: String },
    #[error(
        ":limits :memory must be > 0 — wasmtime StoreLimits refuses a zero memory cap; omit the field for unbounded"
    )]
    MemoryZero,
    #[error(
        ":limits :memory ({bytes} bytes) is below the wasm32-wasip2 linear-memory page size (64 KiB = 65536 bytes) — a sub-page cap cannot hold a single wasm linear memory page, so instantiation of any component declaring `(memory 1)` traps with `memory minimum size of 1 pages exceeds memory limits` and a `(memory 0)` component traps the first `memory.grow(1)`. Pin a value ≥ 64 KiB (e.g. `\"64KiB\"`, `\"1MiB\"`, `\"64MiB\"`) or omit the field for unbounded"
    )]
    MemoryBelowWasm32Page { bytes: u64 },
    #[error(
        ":limits :memory ({bytes} bytes) exceeds the wasm32-wasip2 linear-memory ceiling (4 GiB = 4294967296 bytes); pin a value ≤ 4 GiB or omit the field for unbounded"
    )]
    MemoryExceedsWasm32Cap { bytes: u64 },
    #[error(
        ":limits :fuel must be > 0 — wasmtime traps the first instruction at fuel=0; omit the field for unbounded"
    )]
    FuelZero,
    #[error(
        ":limits :wall-clock must be > 0 — a zero deadline expires before the call starts; omit the field for unbounded"
    )]
    WallClockZero,
    #[error(
        ":limits :wall-clock ({wall_clock:?}) carries a sub-millisecond residue the typed `:wall-clock` duration codec cannot round-trip — \
         the codec truncates to `as_millis()` before picking the canonical unit, so a value with `subsec_nanos() % 1_000_000 != 0` either \
         truncates on first serialize (e.g. `Duration::from_micros(1500)` → \"1ms\" → `Duration::from_millis(1)` ≠ original) or renders \
         as \"0s\" the `WallClockZero` arm then rejects on re-validate. Pin an integer-millisecond magnitude in the canonical authoring form \
         (`<integer><unit>` for unit ∈ {{ms, s, m, h}}, e.g. `\"500ms\"`, `\"30s\"`, `\"2m\"`, `\"1h\"`) or omit the field for unbounded"
    )]
    WallClockNotCanonical { wall_clock: Duration },
    #[error(
        ":limits :wall-clock ({wall_clock:?}) exceeds the per-process ceiling \
         (LIMITS_WALL_CLOCK_MAX = 1h = 3600s) — a value above this cap turns the typed \
         per-call deadline into a nominal-only contract (the wasm-engine's epoch-deadline \
         cancellation reaches for a `Duration` so long no realistic synchronous wasm call \
         can hit it), and the MESH-COMPOSITION §V \"no infinite blocking\" CSE invariant \
         degenerates to enforcement only at the per-Servico cgroup / Kubernetes \
         activeDeadlineSeconds layer — far above the per-call granularity the typed \
         `:limits :wall-clock` slot is meant to express. Pin a value in 1ms..=1h \
         (Envoy / Istio / Linkerd production per-request playbooks all recommend ≤ 60s; \
         AWS App Mesh / ingress-nginx typical ≤ 300s; the longest per-request \
         `proxy_read_timeout` ingress-nginx documents maxes out at the same 3600s ceiling) \
         or omit :wall-clock to express `no per-process deadline on this axis` (the \
         deadline then relies entirely on the cluster-level cgroup / pod \
         activeDeadlineSeconds bound)"
    )]
    WallClockExceedsCap { wall_clock: Duration },
    #[error(
        ":limits :cpu must be > 0m — a zero cgroup share starves the process; omit the field for unbounded"
    )]
    CpuZero,
    #[error(
        ":limits :cpu ({millicores}m) exceeds the per-process ceiling \
         (LIMITS_CPU_MILLICORES_MAX = 128_000m = 128 cores) — a value above this cap is \
         structurally unschedulable on every commercially-common managed-Kubernetes node \
         pool (GKE Standard / EKS managed / AKS default general-purpose SKU ladders top out \
         at 128 vCPU per node; AWS m7i.32xlarge / c7i.32xlarge, Azure HBv3-128rs, GCP \
         c3-standard-128 all sit at the same 128-vCPU ceiling), so the resulting \
         `pleme-computeunit` chart's `resources.requests.cpu` lands as a hint the \
         Kubernetes scheduler cannot bind to any node — the pod sits `Pending` indefinitely \
         with a `0/N nodes are available: N Insufficient cpu` event, and the typed `:cpu` \
         slot becomes an unschedulable contract far from the source caixa.lisp. The \
         wasm32-wasip2 single-threaded execution model the canonical caixa Servico targets \
         reinforces the structural argument: a single wasm component cannot saturate more \
         than one core, so even the Lunatic-style supervised-multi-process host bounds its \
         useful CPU request to the host node's vCPU count. Pin a value in 1m..=128000m \
         (the canonical caixa Servico runs in the 100m..=2000m band — every in-tree \
         example uses 500m; AWS App Mesh / Envoy / Istio per-pod CPU production playbooks \
         all sit ≤ 8000m / 8 cores; the longest documented per-Servico CPU request any \
         pleme-io substrate playbook recommends maxes at ~16 cores) or omit :cpu to \
         express `no per-process CPU hint on this axis` (the cgroup share then defaults to \
         the cluster-level `LimitRange` / `ResourceQuota` policy the operator pins on the \
         host namespace)"
    )]
    CpuExceedsCap { millicores: u32 },
}

// ── byte-size codec ────────────────────────────────────────────────────

fn parse_byte_size(s: &str) -> Result<u64, LimitsError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(LimitsError::EmptyByteSize(s.into()));
    }
    let split_at = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
    let (num_part, unit) = s.split_at(split_at);
    let num_trim = num_part.trim();
    // The canonical authoring form for `:limits :memory` is
    // `<integer><unit>` — every magnitude `render_byte_size` emits is a
    // non-negative integer with no decimal point and no leading sign,
    // so the parser's accepted set must match for serialize/deserialize
    // to round-trip without canonical-form drift. Until this gate
    // landed the parser accepted any `f64`-shaped magnitude
    // (`"1.5KiB"` → 1536 bytes, `"1.0MiB"` → 1MiB, `"0.5GiB"` → 512MiB,
    // `"+1024"` → 1024) and serde silently round-tripped the value to
    // a *different* canonical string on the next emit (`"1.5KiB"` →
    // 1536 → `"1536"`, `"1.0MiB"` → 1048576 → `"1MiB"`, `"0.5GiB"` →
    // 536870912 → `"512MiB"`, `"+1024"` → 1024 → `"1KiB"`) — breaking
    // the THEORY.md §V.2.7 render-determinism contract every typed slot
    // carries.
    //
    // Strict canonical form: every byte of the magnitude is an ASCII
    // digit (no `.`, no `+`, no `-`). On current Rust `u64::from_str`
    // permissively accepts a leading `+` (`"+1024"` → 1024) — that's a
    // canonical-drift shape `render_byte_size` never emits, so the
    // digit-only check is what closes the leading-sign class; relying
    // on `u64::from_str`'s strictness alone would silently admit it.
    // On non-digit-only inputs the gate distinguishes "non-canonical-
    // but-numeric" (parses as f64 or i64, so it's an authoring-shape
    // footgun) from "garbage" (parses as neither, so it's not a
    // numeric input at all) — the diagnostic names the offending
    // magnitude shape verbatim rather than collapsing both authoring
    // footguns into a single opaque `BadByteMagnitude`.
    //
    // Same canonical-form discipline
    // [`crate::AplicacaoSpec::validate_politicas`]'s
    // [`is_canonical_rate_limit_window`] gate (808017c) applies to the
    // rate-limit `:window` axis — the codec's accepted set matches its
    // emitted set, structurally.
    //
    // (Scientific-notation magnitudes like `"1e3KiB"` are also rejected,
    // but on a different arm: the parser splits on the first ASCII-
    // alphabetic byte, so the `e` is read as a unit prefix and the
    // input falls into the `UnknownByteUnit { unit: "e3KiB" }` branch
    // before this gate is consulted — that's the existing diagnostic
    // for the scientific-shape footgun, and this gate is additive to
    // it.)
    let digit_only = !num_trim.is_empty() && num_trim.bytes().all(|b| b.is_ascii_digit());
    if !digit_only {
        // Distinguish "non-canonical-but-numeric" (`"1.5"`, `"1.0"`,
        // `"+1024"`, `"-1"`) from "garbage" (`"abc"`, `"--1"`) so the
        // diagnostic names the offending magnitude shape verbatim.
        // Use f64 + i64 fallbacks for the "numeric" detection so every
        // non-digit-only-but-parseable input lands on
        // `NonIntegerByteMagnitude` regardless of sign or fractionality.
        let numeric = num_trim.parse::<f64>().is_ok() || num_trim.parse::<i64>().is_ok();
        if numeric {
            return Err(LimitsError::NonIntegerByteMagnitude {
                value: num_trim.into(),
            });
        }
        return Err(LimitsError::BadByteMagnitude(num_part.into()));
    }
    // `digit_only` guarantees every byte is `[0-9]`, so the only way
    // u64::from_str can fail here is overflow (the magnitude exceeds
    // u64::MAX). Surface that as `BadByteMagnitude` with an overflow-
    // shaped wording so the diagnostic names the offending magnitude
    // verbatim rather than collapsing onto the non-canonical arm.
    let num: u64 = num_trim.parse::<u64>().map_err(|_| {
        LimitsError::BadByteMagnitude(format!("{num_trim} (digit-only magnitude overflows u64)"))
    })?;
    let multiplier: u64 = match unit.trim() {
        "" | "B" => 1,
        "KB" => 1_000,
        "MB" => 1_000_000,
        "GB" => 1_000_000_000,
        "KiB" => 1024,
        "MiB" => 1024 * 1024,
        "GiB" => 1024 * 1024 * 1024,
        other => {
            return Err(LimitsError::UnknownByteUnit { unit: other.into() });
        }
    };
    // Overflow surfaces as `BadByteMagnitude` (a u64-saturating
    // multiply would silently truncate to `u64::MAX` and then the
    // wasm32-cap gate at validate time would catch it — but a u64
    // overflow is a parse-shaped failure on the author's input, not a
    // domain-cap rejection on a well-formed value, so it surfaces here
    // as a parser diagnostic naming the offending magnitude × unit
    // pair rather than as `MemoryExceedsWasm32Cap { bytes: u64::MAX }`
    // far from the author's intent).
    num.checked_mul(multiplier).ok_or_else(|| {
        LimitsError::BadByteMagnitude(format!(
            "{num_trim}{unit_trim} overflows u64 (magnitude × unit > 2^64-1)",
            unit_trim = unit.trim()
        ))
    })
}

fn render_byte_size(n: u64) -> String {
    // Prefer the largest power-of-1024 unit that divides cleanly; fall
    // back to bytes if nothing matches.
    const UNITS: &[(u64, &str)] = &[
        (1024 * 1024 * 1024, "GiB"),
        (1024 * 1024, "MiB"),
        (1024, "KiB"),
    ];
    for (mult, label) in UNITS {
        if n >= *mult && n % mult == 0 {
            return format!("{}{label}", n / mult);
        }
    }
    format!("{n}")
}

fn ser_byte_size<S: Serializer>(v: &Option<u64>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(n) => s.serialize_str(&render_byte_size(*n)),
        None => s.serialize_none(),
    }
}

fn de_byte_size<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    let opt: Option<String> = Option::deserialize(d)?;
    match opt {
        None => Ok(None),
        Some(s) => parse_byte_size(&s)
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

// ── duration codec ─────────────────────────────────────────────────────

fn parse_duration(s: &str) -> Result<Duration, LimitsError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(LimitsError::EmptyDuration(s.into()));
    }
    let split_at = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
    let (num_part, unit) = s.split_at(split_at);
    let num_trim = num_part.trim();
    // The canonical authoring form for `:limits :wall-clock` is
    // `<integer><unit>` — every magnitude `render_duration` emits is a
    // non-negative integer with no decimal point and no leading sign,
    // so the parser's accepted set must match for serialize/deserialize
    // to round-trip without canonical-form drift. Until this gate
    // landed the parser accepted any `f64`-shaped magnitude
    // (`"1.5s"` → 1500ms, `"1.0s"` → 1s, `"0.5m"` → 30s, `"+30s"` →
    // 30s) and serde silently round-tripped the value to a *different*
    // canonical string on the next emit (`"1.5s"` → 1500ms →
    // `"1500ms"`, `"1.0s"` → 1s → `"1s"`, `"0.5m"` → 30s → `"30s"`,
    // `"+30s"` → 30s → `"30s"`) — breaking the THEORY.md Part V
    // render-determinism contract every typed slot carries. The same
    // canonical-form discipline `parse_byte_size`'s integer-magnitude
    // gate (the immediate predecessor on the peer `:limits :memory`
    // codec) applies; this gate is the direct successor on the
    // `:limits :wall-clock` codec.
    //
    // Strict canonical form: every byte of the magnitude is an ASCII
    // digit (no `.`, no `+`, no `-`). On current Rust `u64::from_str`
    // permissively accepts a leading `+` (`"+30"` → 30) — that's a
    // canonical-drift shape `render_duration` never emits, so the
    // digit-only check is what closes the leading-sign class; relying
    // on `u64::from_str`'s strictness alone would silently admit it.
    // On non-digit-only inputs the gate distinguishes "non-canonical-
    // but-numeric" (parses as f64 or i64 — surfaced as the new
    // `NonIntegerDurationMagnitude` variant with a self-locating
    // diagnostic) from "garbage" (parses as neither — surfaced as the
    // existing `BadDurationMagnitude` so its narrower diagnostic
    // remains load-bearing).
    let digit_only = !num_trim.is_empty() && num_trim.bytes().all(|b| b.is_ascii_digit());
    if !digit_only {
        let numeric = num_trim.parse::<f64>().is_ok() || num_trim.parse::<i64>().is_ok();
        if numeric {
            return Err(LimitsError::NonIntegerDurationMagnitude {
                value: num_trim.into(),
            });
        }
        return Err(LimitsError::BadDurationMagnitude(num_part.into()));
    }
    // `digit_only` guarantees every byte is `[0-9]`, so the only way
    // u64::from_str can fail here is overflow.
    let num: u64 = num_trim.parse::<u64>().map_err(|_| {
        LimitsError::BadDurationMagnitude(format!(
            "{num_trim} (digit-only magnitude overflows u64)"
        ))
    })?;
    // Multiply on u64 with overflow detection — every unit conversion
    // is integer-exact for an integer magnitude, so the codec drops
    // `Duration::from_secs_f64` entirely. Overflow surfaces at parse
    // time with a parser-shaped diagnostic naming the offending
    // magnitude × unit pair (matches `parse_byte_size`'s overflow arm).
    let unit_trim = unit.trim();
    let dur = match unit_trim {
        "ms" => Duration::from_millis(num),
        "s" | "" => Duration::from_secs(num),
        "m" => Duration::from_secs(num.checked_mul(60).ok_or_else(|| {
            LimitsError::BadDurationMagnitude(format!(
                "{num_trim}{unit_trim} overflows u64 (magnitude × 60 > 2^64-1)"
            ))
        })?),
        "h" => Duration::from_secs(num.checked_mul(3600).ok_or_else(|| {
            LimitsError::BadDurationMagnitude(format!(
                "{num_trim}{unit_trim} overflows u64 (magnitude × 3600 > 2^64-1)"
            ))
        })?),
        other => {
            return Err(LimitsError::UnknownDurationUnit { unit: other.into() });
        }
    };
    Ok(dur)
}

fn render_duration(d: Duration) -> String {
    let total_ms = d.as_millis();
    if total_ms == 0 {
        return "0s".into();
    }
    if total_ms % (3600 * 1000) == 0 {
        return format!("{}h", total_ms / (3600 * 1000));
    }
    if total_ms % (60 * 1000) == 0 {
        return format!("{}m", total_ms / (60 * 1000));
    }
    if total_ms % 1000 == 0 {
        return format!("{}s", total_ms / 1000);
    }
    format!("{total_ms}ms")
}

fn ser_duration<S: Serializer>(v: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(d) => s.serialize_str(&render_duration(*d)),
        None => s.serialize_none(),
    }
}

fn de_duration<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
    let opt: Option<String> = Option::deserialize(d)?;
    match opt {
        None => Ok(None),
        Some(s) => parse_duration(&s)
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

// ── millicores codec ───────────────────────────────────────────────────

fn parse_millicores(s: &str) -> Result<u32, LimitsError> {
    let s_trim = s.trim();
    if s_trim.is_empty() {
        return Err(LimitsError::BadMillicores(s.into()));
    }
    let (magnitude, has_m_suffix) = match s_trim.strip_suffix('m') {
        Some(stripped) => (stripped.trim(), true),
        None => (s_trim, false),
    };
    if magnitude.is_empty() {
        // Bare `"m"` (or `" m "`) — no magnitude was authored. The
        // canonical millicores authoring form requires a magnitude in
        // front of the unit (`"500m"`, not `"m"`). Surface as
        // `BadMillicores` so the existing narrower-arm wording stays
        // load-bearing for "no recognizable magnitude" inputs.
        return Err(LimitsError::BadMillicores(s.into()));
    }
    // The canonical authoring form for `:limits :cpu` is `<integer>m`
    // (Kubernetes millicores) or the bare-core shorthand `<integer>`
    // (`"2"` = 2000 millicores). Every magnitude `render_millicores`
    // emits is a non-negative integer (`format!("{m}m")`) — no decimal
    // point, no leading sign — so the parser's accepted set must match
    // for serialize/deserialize to round-trip without canonical-form
    // drift. Until this gate landed the parser accepted any
    // `u32::from_str`-shaped magnitude (`"+500m"` → 500, `"+2"` →
    // 2000) and serde silently round-tripped the value to a *different*
    // canonical string on the next emit (`"+500m"` → `"500m"`, `"+2"`
    // → `"2000m"`) — breaking the THEORY.md Part V render-determinism
    // contract every typed slot carries. Closes the sixth (and last)
    // typed-codec surface in caixa-core on the integer-magnitude
    // canonical-form axis, peer with the five duration / byte-size /
    // rate-limit codecs the prior trajectory (1c55a2a / 818dd38 /
    // d1fd67b / f479c41 / d53c922) covered.
    //
    // Strict canonical form: every byte of the magnitude is an ASCII
    // digit (no `.`, no `+`, no `-`). On current Rust `u32::from_str`
    // permissively accepts a leading `+` (`"+500"` → 500) — that's a
    // canonical-drift shape `render_millicores` never emits, so the
    // digit-only check is what closes the leading-sign class; relying
    // on `u32::from_str`'s strictness alone would silently admit it.
    // On non-digit-only inputs the gate distinguishes "non-canonical-
    // but-numeric" (parses as f64 or i64 — surfaced as the new
    // `NonIntegerMillicoreMagnitude` variant naming the offending
    // magnitude verbatim with the canonical-form remediation) from
    // "garbage" (parses as neither — surfaced as the existing
    // `BadMillicores` so its narrower diagnostic shape remains
    // load-bearing for the not-a-numeric-input class).
    let digit_only = magnitude.bytes().all(|b| b.is_ascii_digit());
    if !digit_only {
        let numeric = magnitude.parse::<f64>().is_ok() || magnitude.parse::<i64>().is_ok();
        if numeric {
            return Err(LimitsError::NonIntegerMillicoreMagnitude {
                value: magnitude.into(),
            });
        }
        return Err(LimitsError::BadMillicores(s.into()));
    }
    // `digit_only` guarantees every byte is `[0-9]`, so the only way
    // `u32::from_str` can fail here is overflow (the magnitude exceeds
    // `u32::MAX`). Surface that as `BadMillicores` with an overflow-
    // shaped wording so the diagnostic names the offending magnitude
    // verbatim rather than collapsing onto the non-canonical arm —
    // matches `parse_byte_size` / `parse_duration` / `rate_limit_codec`
    // overflow-arm shape on the peer typed codecs.
    let num: u32 = magnitude.parse::<u32>().map_err(|_| {
        LimitsError::BadMillicores(format!("{magnitude} (digit-only magnitude overflows u32)"))
    })?;
    if has_m_suffix {
        Ok(num)
    } else {
        // Bare-core shorthand: `"2"` = 2000 millicores. Use
        // `checked_mul` (not the prior `saturating_mul`) so a
        // magnitude that overflows u32 on the × 1000 conversion
        // surfaces a parser-shaped diagnostic at parse time rather
        // than silently saturating to `u32::MAX` (which would land
        // as the cap value far from the author's intent and bypass
        // any future validate-time upper-bound gate the `:cpu` axis
        // grows). Matches `parse_byte_size`'s overflow-arm shape on
        // the magnitude × unit multiply.
        num.checked_mul(1000).ok_or_else(|| {
            LimitsError::BadMillicores(format!(
                "{magnitude} cores × 1000 overflows u32 (write the value in millicores: max \"{}m\")",
                u32::MAX
            ))
        })
    }
}

fn render_millicores(m: u32) -> String {
    format!("{m}m")
}

fn ser_millicores<S: Serializer>(v: &Option<u32>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(m) => s.serialize_str(&render_millicores(*m)),
        None => s.serialize_none(),
    }
}

fn de_millicores<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    let opt: Option<String> = Option::deserialize(d)?;
    match opt {
        None => Ok(None),
        Some(s) => parse_millicores(&s)
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_byte_size_known_units() {
        assert_eq!(parse_byte_size("64MiB").unwrap(), 64 * 1024 * 1024);
        assert_eq!(parse_byte_size("1GiB").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_byte_size("512KiB").unwrap(), 512 * 1024);
        assert_eq!(parse_byte_size("1KB").unwrap(), 1_000);
        assert_eq!(parse_byte_size("1024").unwrap(), 1024);
    }

    #[test]
    fn parse_byte_size_rejects_unknown() {
        assert!(matches!(
            parse_byte_size("1YiB"),
            Err(LimitsError::UnknownByteUnit { .. })
        ));
        assert!(matches!(
            parse_byte_size("not-a-number"),
            Err(LimitsError::BadByteMagnitude(_))
        ));
    }

    #[test]
    fn parse_duration_known_units() {
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_duration("2m").unwrap(), Duration::from_secs(120));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
    }

    #[test]
    fn parse_millicores_both_forms() {
        assert_eq!(parse_millicores("500m").unwrap(), 500);
        assert_eq!(parse_millicores("2").unwrap(), 2000);
    }

    #[test]
    fn render_byte_size_canonical() {
        assert_eq!(render_byte_size(64 * 1024 * 1024), "64MiB");
        assert_eq!(render_byte_size(1024 * 1024 * 1024), "1GiB");
        assert_eq!(render_byte_size(1024), "1KiB");
        assert_eq!(render_byte_size(123), "123");
    }

    #[test]
    fn render_duration_canonical() {
        assert_eq!(render_duration(Duration::from_secs(30)), "30s");
        assert_eq!(render_duration(Duration::from_millis(500)), "500ms");
        assert_eq!(render_duration(Duration::from_secs(120)), "2m");
        assert_eq!(render_duration(Duration::from_secs(3600)), "1h");
    }

    #[test]
    fn limits_round_trip_through_json() {
        let limits = LimitsSpec {
            memory: Some(64 * 1024 * 1024),
            fuel: Some(1_000_000),
            wall_clock: Some(Duration::from_secs(30)),
            cpu: Some(500),
        };
        let json = serde_json::to_string(&limits).unwrap();
        let back: LimitsSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(limits, back);
    }

    #[test]
    fn empty_limits_serialises_to_empty_object() {
        let limits = LimitsSpec::default();
        assert!(limits.is_empty());
        let json = serde_json::to_string(&limits).unwrap();
        assert_eq!(json, "{}");
    }

    // ── value-shape: zero on any declared axis is rejected ────────────────

    #[test]
    fn validate_accepts_default_unbounded_limits() {
        // Every axis None → "no bound declared" is the omit-the-slot
        // shape and stays valid. This is the pre-M2 default behaviour.
        LimitsSpec::default().validate().unwrap();
    }

    #[test]
    fn validate_accepts_full_nonzero_limits() {
        let l = LimitsSpec {
            memory: Some(64 * 1024 * 1024),
            fuel: Some(1_000_000),
            wall_clock: Some(Duration::from_secs(30)),
            cpu: Some(500),
        };
        l.validate().unwrap();
    }

    #[test]
    fn validate_rejects_zero_memory() {
        let l = LimitsSpec {
            memory: Some(0),
            ..Default::default()
        };
        assert_eq!(l.validate().unwrap_err(), LimitsError::MemoryZero);
    }

    #[test]
    fn validate_rejects_zero_fuel() {
        let l = LimitsSpec {
            fuel: Some(0),
            ..Default::default()
        };
        assert_eq!(l.validate().unwrap_err(), LimitsError::FuelZero);
    }

    #[test]
    fn validate_rejects_zero_wall_clock() {
        let l = LimitsSpec {
            wall_clock: Some(Duration::ZERO),
            ..Default::default()
        };
        assert_eq!(l.validate().unwrap_err(), LimitsError::WallClockZero);
    }

    #[test]
    fn validate_rejects_zero_cpu() {
        let l = LimitsSpec {
            cpu: Some(0),
            ..Default::default()
        };
        assert_eq!(l.validate().unwrap_err(), LimitsError::CpuZero);
    }

    #[test]
    fn validate_rejects_first_zero_axis_deterministically() {
        // Memory is checked first; with multiple zero axes, the
        // diagnostic names :memory rather than reporting some other
        // axis non-deterministically.
        let l = LimitsSpec {
            memory: Some(0),
            fuel: Some(0),
            wall_clock: Some(Duration::ZERO),
            cpu: Some(0),
        };
        assert_eq!(l.validate().unwrap_err(), LimitsError::MemoryZero);
    }

    // ── value-shape: :memory upper bound — wasm32-wasip2 4 GiB ceiling ────

    #[test]
    fn wasm32_memory_cap_matches_parsed_4_gib() {
        // The cap constant tracks the canonical "4 GiB" byte-size
        // codec output structurally — drift between the codec's
        // accepted magnitude for `"4GiB"` and the validate gate's
        // accepted upper bound would surface here, not as a silent
        // round-trip break at the renderer layer. Same single-source-
        // of-truth shape the is_canonical_rate_limit_window predicate
        // gives the rate-limit window set.
        assert_eq!(
            parse_byte_size("4GiB").unwrap(),
            LIMITS_MEMORY_WASM32_MAX_BYTES
        );
        assert_eq!(LIMITS_MEMORY_WASM32_MAX_BYTES, 4 * 1024 * 1024 * 1024);
        assert_eq!(LIMITS_MEMORY_WASM32_MAX_BYTES, 1u64 << 32);
    }

    #[test]
    fn validate_accepts_memory_at_wasm32_cap() {
        // 4 GiB exactly is the wasm32 in-spec maximum — `2^16 pages ×
        // 2^16 bytes/page`. The validate gate is inclusive on the
        // upper end (mirrors the inclusive lower-end rejection: zero
        // is *out*, one is *in*; 4 GiB+1 is *out*, 4 GiB is *in*).
        let l = LimitsSpec {
            memory: Some(LIMITS_MEMORY_WASM32_MAX_BYTES),
            ..Default::default()
        };
        l.validate().unwrap();
    }

    #[test]
    fn validate_rejects_memory_one_byte_above_wasm32_cap() {
        // Boundary case: exactly 1 byte past the cap. Catches a
        // future "strictly less than" half-measure and pins the
        // diagnostic to name the offending byte count verbatim.
        let bytes = LIMITS_MEMORY_WASM32_MAX_BYTES + 1;
        let l = LimitsSpec {
            memory: Some(bytes),
            ..Default::default()
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::MemoryExceedsWasm32Cap { bytes }
        );
    }

    #[test]
    fn validate_rejects_memory_8_gib() {
        // The "obvious authoring footgun" case: a value the byte-size
        // codec accepts cleanly (`"8GiB"` → 8 * 1024^3 bytes) and
        // serde round-trips silently, but no wasm32 component can
        // honor. Until this gate landed `validate` accepted it.
        let bytes = parse_byte_size("8GiB").unwrap();
        let l = LimitsSpec {
            memory: Some(bytes),
            ..Default::default()
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::MemoryExceedsWasm32Cap { bytes }
        );
    }

    #[test]
    fn validate_memory_zero_takes_precedence_over_cap_check() {
        // Memory zero is structurally meaningless under *any* wasm
        // engine (zero-cap traps the first allocation); above-cap is
        // wasm32-specific. The zero arm fires first so the canonical
        // "omit the slot for unbounded" remediation in the existing
        // MemoryZero diagnostic still leads — pinning this precedence
        // guards against a future re-ordering that would surface the
        // wasm32-specific message in the case where the simpler
        // zero-floor message is more actionable.
        let l = LimitsSpec {
            memory: Some(0),
            ..Default::default()
        };
        assert_eq!(l.validate().unwrap_err(), LimitsError::MemoryZero);
    }

    #[test]
    fn validate_rejects_memory_cap_before_other_axes() {
        // With both an above-cap :memory and a zero :fuel, the
        // diagnostic names :memory rather than :fuel — peer of the
        // existing `validate_rejects_first_zero_axis_deterministically`
        // ordering pin.
        let bytes = LIMITS_MEMORY_WASM32_MAX_BYTES + 1024;
        let l = LimitsSpec {
            memory: Some(bytes),
            fuel: Some(0),
            wall_clock: Some(Duration::ZERO),
            cpu: Some(0),
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::MemoryExceedsWasm32Cap { bytes }
        );
    }

    #[test]
    fn above_cap_value_still_round_trips_through_serde() {
        // The byte-size codec accepts the above-cap value (the cap
        // lives in the validate gate, not the codec). This pins that
        // the structural property is "above-cap is rejected by
        // validate" — not "above-cap is unparseable by the codec";
        // the latter would prevent the diagnostic from naming the
        // offending byte count at all, since deserialize would fail
        // first.
        let l = LimitsSpec {
            memory: Some(LIMITS_MEMORY_WASM32_MAX_BYTES + 1),
            ..Default::default()
        };
        let json = serde_json::to_string(&l).unwrap();
        let back: LimitsSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(l, back);
        assert!(back.validate().is_err());
    }

    // ── value-shape: :memory lower bound — wasm32-wasip2 64 KiB page floor ─

    #[test]
    fn wasm32_memory_page_matches_parsed_64_kib() {
        // The page-floor constant tracks the canonical "64 KiB"
        // byte-size codec output structurally — drift between the
        // codec's accepted magnitude for `"64KiB"` and the validate
        // gate's accepted lower bound would surface here, not as a
        // silent round-trip break at the renderer layer. Same single-
        // source-of-truth shape `wasm32_memory_cap_matches_parsed_4_gib`
        // pins on the peer upper-cap bound and
        // `is_canonical_rate_limit_window` gives the rate-limit window
        // set. The page-size identities (2^16, integer-divides the
        // upper cap exactly 2^16 times) are pinned alongside so a
        // future memory64-target opt-in raising one bound surfaces
        // here if the other bound's relationship to it drifts.
        assert_eq!(
            parse_byte_size("64KiB").unwrap(),
            LIMITS_MEMORY_WASM32_PAGE_BYTES
        );
        assert_eq!(LIMITS_MEMORY_WASM32_PAGE_BYTES, 64 * 1024);
        assert_eq!(LIMITS_MEMORY_WASM32_PAGE_BYTES, 1u64 << 16);
        assert_eq!(
            LIMITS_MEMORY_WASM32_MAX_BYTES / LIMITS_MEMORY_WASM32_PAGE_BYTES,
            1u64 << 16,
            "the wasm32 page count cap is 2^16 pages exactly",
        );
        assert_eq!(
            LIMITS_MEMORY_WASM32_MAX_BYTES % LIMITS_MEMORY_WASM32_PAGE_BYTES,
            0
        );
    }

    #[test]
    fn validate_rejects_memory_below_wasm32_page() {
        // The fail-before-pass-after pin: until this gate landed a
        // `(:memory "32KiB")` (or any programmatic struct literal with
        // a sub-page byte count — `LimitsSpec { memory: Some(50000),
        // .. }`) silently passed validate, the byte-size codec
        // round-tripped cleanly through serde, and the wasm-engine
        // either refused instantiation (`memory minimum size of 1
        // pages exceeds memory limits` on any cdylib-shaped component
        // declaring `(memory 1)`) or trapped the first `memory.grow(1)`
        // far from the source caixa.lisp.
        let bytes = parse_byte_size("32KiB").unwrap();
        let l = LimitsSpec {
            memory: Some(bytes),
            ..Default::default()
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::MemoryBelowWasm32Page { bytes }
        );
    }

    #[test]
    fn validate_rejects_memory_one_byte_below_page() {
        // Boundary case: exactly 1 byte below the page-size floor
        // (`LIMITS_MEMORY_WASM32_PAGE_BYTES - 1` = 65535 bytes). Pins
        // the inclusive-upper-end / strict-lower-end relationship on
        // the page-floor arm: 65535 is *out*, 65536 is *in*. Catches a
        // future "strictly greater than" half-measure and matches the
        // peer `validate_rejects_memory_one_byte_above_wasm32_cap`
        // shape on the top edge.
        let bytes = LIMITS_MEMORY_WASM32_PAGE_BYTES - 1;
        let l = LimitsSpec {
            memory: Some(bytes),
            ..Default::default()
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::MemoryBelowWasm32Page { bytes }
        );
    }

    #[test]
    fn validate_rejects_memory_one_byte() {
        // The far-floor case: a `(:memory "1")` cap is non-zero (so
        // `MemoryZero` doesn't fire) but structurally cannot hold any
        // wasm linear memory page. The page-floor gate at this layer
        // surfaces a self-locating diagnostic naming the offending
        // byte count verbatim rather than a downstream wasm-engine
        // instantiation failure whose error message points at the
        // engine's internals, not the caixa.lisp `:memory` slot.
        let l = LimitsSpec {
            memory: Some(1),
            ..Default::default()
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::MemoryBelowWasm32Page { bytes: 1 }
        );
    }

    #[test]
    fn validate_accepts_memory_at_wasm32_page() {
        // 64 KiB exactly is the wasm32 linear-memory page size — the
        // smallest cap that admits one wasm `(memory 1)` page. The
        // page-floor gate is inclusive on the lower end (mirrors the
        // inclusive upper-end acceptance: 4 GiB is *in*, 4 GiB+1 is
        // *out*; 64 KiB is *in*, 64 KiB-1 is *out*).
        let l = LimitsSpec {
            memory: Some(LIMITS_MEMORY_WASM32_PAGE_BYTES),
            ..Default::default()
        };
        l.validate().unwrap();
    }

    #[test]
    fn validate_accepts_multi_page_memory() {
        // The positive-control sweep: every typed `:memory` cap that
        // admits at least one wasm linear memory page (i.e. ≥
        // `LIMITS_MEMORY_WASM32_PAGE_BYTES`) passes `validate`. Sweeps
        // single-page, two-page, the canonical 64 MiB / 1 GiB / 4 GiB
        // upper-bound boundary so a future tightening of either edge
        // surfaces here. Peer of
        // `validate_accepts_integer_millisecond_wall_clock_values` on
        // the sibling `:wall-clock` axis.
        for bytes in [
            LIMITS_MEMORY_WASM32_PAGE_BYTES,
            2 * LIMITS_MEMORY_WASM32_PAGE_BYTES,
            64 * 1024 * 1024,
            1024 * 1024 * 1024,
            LIMITS_MEMORY_WASM32_MAX_BYTES,
        ] {
            let l = LimitsSpec {
                memory: Some(bytes),
                ..Default::default()
            };
            l.validate()
                .unwrap_or_else(|e| panic!("multi-page {bytes} must validate, got {e:?}"));
        }
    }

    #[test]
    fn validate_memory_zero_takes_precedence_over_page_floor() {
        // Cross-arm ordering pin: `Some(0)` would otherwise pass the
        // page-floor arm's `m < PAGE_BYTES` check (0 < 65536), but the
        // zero-floor arm strictly precedes the page-floor arm so the
        // more self-locating `MemoryZero` diagnostic (with its omit-
        // axis remediation directly named, applicable under *any* wasm
        // engine not just wasm32) leads. Same posture every peer
        // zero-then-shape gate uses on this surface
        // (`PolicyTimeoutZero` → `PolicyTimeoutNotCanonical`,
        // `PolicyBreakerZeroWindow` → `PolicyBreakerWindowNotCanonical`,
        // `WallClockZero` → `WallClockNotCanonical`).
        let l = LimitsSpec {
            memory: Some(0),
            ..Default::default()
        };
        assert_eq!(l.validate().unwrap_err(), LimitsError::MemoryZero);
    }

    #[test]
    fn validate_memory_page_floor_takes_precedence_over_other_axes() {
        // With a sub-page `:memory` and zero values on every other
        // axis, the diagnostic names `:memory` rather than `:fuel` /
        // `:wall-clock` / `:cpu` — peer of the existing
        // `validate_rejects_first_zero_axis_deterministically` and
        // `validate_rejects_memory_cap_before_other_axes` ordering
        // pins. Memory is the first axis the validate cascade checks,
        // so a sub-page value surfaces before any other-axis
        // diagnostic regardless of how many other axes are
        // simultaneously invalid.
        let bytes = LIMITS_MEMORY_WASM32_PAGE_BYTES / 2;
        let l = LimitsSpec {
            memory: Some(bytes),
            fuel: Some(0),
            wall_clock: Some(Duration::ZERO),
            cpu: Some(0),
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::MemoryBelowWasm32Page { bytes }
        );
    }

    #[test]
    fn memory_page_floor_diagnostic_carries_offending_bytes() {
        // Diagnostic-shape pin: the page-floor arm names the
        // offending byte count verbatim so the author's grep lands on
        // the field's value, not a generic "memory too small" message.
        // Same shape every other typed-cap arm on this surface
        // carries (`MemoryExceedsWasm32Cap` carries the offending byte
        // count verbatim, `WallClockNotCanonical` carries the
        // offending `Duration` verbatim, `PolicyRetriesExceedsCap`
        // carries the offending retry count verbatim).
        let l = LimitsSpec {
            memory: Some(50_000),
            ..Default::default()
        };
        let err = l.validate().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("50000"),
            "diagnostic must carry the offending byte count verbatim (got {msg:?})"
        );
        assert!(
            msg.contains("64 KiB") || msg.contains("65536"),
            "diagnostic must name the page-size floor (got {msg:?})"
        );
    }

    #[test]
    fn below_page_value_still_round_trips_through_serde() {
        // The byte-size codec accepts the sub-page value (the floor
        // lives in the validate gate, not the codec) — peer of
        // `above_cap_value_still_round_trips_through_serde` on the top
        // edge. Pins that the structural property is "sub-page is
        // rejected by validate" — not "sub-page is unparseable by the
        // codec"; the latter would prevent the diagnostic from naming
        // the offending byte count at all, since deserialize would
        // fail first.
        let l = LimitsSpec {
            memory: Some(LIMITS_MEMORY_WASM32_PAGE_BYTES - 1),
            ..Default::default()
        };
        let json = serde_json::to_string(&l).unwrap();
        let back: LimitsSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(l, back);
        assert!(back.validate().is_err());
    }

    // ── canonical-form: integer-magnitude byte-size codec gate ────────────
    //
    // Every magnitude `render_byte_size` emits is a non-negative integer
    // (no decimal point, no leading sign, no scientific notation). The
    // parser's accepted set must match for parse → render → parse to
    // round-trip without canonical-form drift. The tests below pin every
    // canonical-drift shape — fractional (`"1.5KiB"`), decimal-shaped-
    // integer (`"1.0MiB"`), half-unit (`"0.5GiB"`), leading-`+`
    // (`"+1024"`) — plus the scientific-notation dispatch path (caught
    // by `UnknownByteUnit` on a different arm), the two complement-side
    // pins (the integer happy paths the gate must continue to accept),
    // the round-trip convergence property (parse → render → parse must
    // converge on a single canonical form for every accepted input),
    // the BadByteMagnitude-precedence pin (genuinely unparseable inputs
    // keep their narrower diagnostic), the overflow-surface pin
    // (u64-overflow on magnitude × unit surfaces at parse time), and
    // the serde-path pin (the gate fires at deserialize, before any
    // validate gate runs).

    #[test]
    fn parse_byte_size_rejects_fractional_kib() {
        // The fail-before-pass-after pin: `"1.5KiB"` parsed cleanly on
        // every pre-gate codebase (f64::parse accepts the decimal), the
        // codec produced 1536 bytes, and `render_byte_size(1536)`
        // emitted `"1536"` on the next serialize — silently drifting
        // the canonical form away from the author's intent. The new
        // gate surfaces the round-trip break at the parser layer with
        // a self-locating diagnostic (the offending magnitude verbatim,
        // the canonical-form remediation in the wording).
        let err = parse_byte_size("1.5KiB").unwrap_err();
        assert!(
            matches!(err, LimitsError::NonIntegerByteMagnitude { ref value } if value == "1.5"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_byte_size_rejects_decimal_shaped_integer() {
        // The canonical-drift case where the *value* is integer but
        // the *form* carries a redundant decimal point — `"1.0MiB"`
        // parses to 1 MiB (integer), but the renderer emits `"1MiB"`
        // on the next serialize (no decimal point). The parse-shape
        // gate fires here too so the codec's accepted set is exactly
        // the renderer's emitted set — no `"1.0MiB"` ↔ `"1MiB"` drift
        // surviving a round-trip silently.
        let err = parse_byte_size("1.0MiB").unwrap_err();
        assert!(
            matches!(err, LimitsError::NonIntegerByteMagnitude { ref value } if value == "1.0"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_byte_size_rejects_half_gib() {
        // `"0.5GiB"` parses to 536870912 bytes = 512MiB; the renderer
        // emits `"512MiB"` on the next serialize. Pin the round-trip
        // drift on the explicitly-fractional case sized to land on a
        // unit boundary, so the gate's coverage includes both the
        // "doesn't land on a boundary" (1.5KiB → 1536) and "lands on
        // a smaller-unit boundary" (0.5GiB → 512MiB) drift shapes.
        let err = parse_byte_size("0.5GiB").unwrap_err();
        assert!(
            matches!(err, LimitsError::NonIntegerByteMagnitude { ref value } if value == "0.5"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_byte_size_rejects_scientific_notation_via_unit_arm() {
        // Scientific-notation magnitudes are canonical-form drift too
        // — the renderer never emits `"1e3KiB"` for any value. But
        // they're caught on a *different* arm than the fractional /
        // leading-`+` shapes: the parser's split-on-first-alphabetic-
        // byte heuristic reads the `e` as a unit prefix, so the input
        // falls into the existing `UnknownByteUnit { unit: "e3KiB" }`
        // diagnostic before the `NonIntegerByteMagnitude` gate is
        // consulted. Pin this dispatch path so a future relaxation of
        // the split heuristic (e.g. recognizing `e` as part of a
        // scientific-notation magnitude) surfaces here as a test
        // failure — at which point the `NonIntegerByteMagnitude` gate
        // would correctly take over, and this test would flip to that
        // arm with no other change required.
        let err = parse_byte_size("1e3KiB").unwrap_err();
        assert!(
            matches!(err, LimitsError::UnknownByteUnit { ref unit } if unit == "e3KiB"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_byte_size_rejects_leading_plus() {
        // `"+1024"` parses through f64 as 1024 bytes; the renderer
        // emits `"1KiB"` on the next serialize. The leading `+` is
        // not a renderer-emitted shape, so it falls in the same
        // canonical-drift class as the fractional / scientific forms
        // — surfacing under the same diagnostic keeps the gate's
        // coverage uniform across every non-canonical-but-numeric
        // input shape the parser would otherwise accept.
        let err = parse_byte_size("+1024").unwrap_err();
        assert!(
            matches!(err, LimitsError::NonIntegerByteMagnitude { ref value } if value == "+1024"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_byte_size_continues_to_accept_integer_magnitudes() {
        // The complement-side pin: every canonical integer-magnitude
        // form the renderer emits must continue to parse to the same
        // value the renderer produced. Sweep the five canonical
        // authoring shapes (unitless integer, KiB, MiB, GiB, KB) so a
        // future tightening of the parser surfaces here as a test
        // failure rather than a silent regression in the canonical
        // authoring set.
        assert_eq!(parse_byte_size("1024").unwrap(), 1024);
        assert_eq!(parse_byte_size("1KiB").unwrap(), 1024);
        assert_eq!(parse_byte_size("64MiB").unwrap(), 64 * 1024 * 1024);
        assert_eq!(parse_byte_size("1GiB").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_byte_size("1000KB").unwrap(), 1_000_000);
    }

    #[test]
    fn parse_byte_size_round_trips_through_render_for_every_canonical_form() {
        // The structural property the gate makes load-bearing: every
        // value the parser accepts round-trips through `render_byte_size`
        // to a string the parser also accepts — and to the *same* value.
        // Sweep the values the renderer emits canonically (1024 / 1MiB
        // / 1GiB / 1536 / 64MiB) so a future codec change that breaks
        // round-trip convergence surfaces here, not at a downstream
        // renderer that double-emits a typed slot.
        for n in [1u64, 1023, 1024, 1536, 64 * 1024 * 1024, 1024 * 1024 * 1024] {
            let rendered = render_byte_size(n);
            let reparsed = parse_byte_size(&rendered)
                .unwrap_or_else(|e| panic!("render({n}) = {rendered:?} must reparse, got {e:?}"));
            assert_eq!(
                reparsed, n,
                "round-trip drift on {n}: rendered={rendered:?}, reparsed={reparsed}",
            );
        }
    }

    #[test]
    fn parse_byte_size_keeps_bad_magnitude_for_unparseable_input() {
        // The precedence pin: the new `NonIntegerByteMagnitude` arm
        // distinguishes *non-canonical-but-numeric* (`"1.5"`, `"1.0"`,
        // `"+1024"`, `"-1"`) from *genuinely-unparseable* (`"abc"`,
        // `"--1"`) so the existing `BadByteMagnitude` diagnostic's
        // wording remains load-bearing for the latter class — the gate
        // is additive, not replacing. Pin both arms so a future
        // relaxation that collapses them surfaces here.
        let err = parse_byte_size("abc").unwrap_err();
        assert!(
            matches!(err, LimitsError::BadByteMagnitude(_)),
            "got {err:?}"
        );
        let err = parse_byte_size("--1").unwrap_err();
        assert!(
            matches!(err, LimitsError::BadByteMagnitude(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_byte_size_overflow_surfaces_as_bad_magnitude() {
        // `u64::MAX KiB` overflows the u64 result; the parser surfaces
        // the overflow as a `BadByteMagnitude` (not as a saturated
        // `u64::MAX` value that the wasm32-cap validate gate then
        // catches), so the diagnostic names the offending magnitude ×
        // unit pair at parse time rather than as
        // `MemoryExceedsWasm32Cap { bytes: u64::MAX }` far from the
        // author's intent. (`u64::MAX` itself parses cleanly with no
        // unit since `u64::MAX × 1 = u64::MAX` fits.)
        let err = parse_byte_size("18446744073709551615KiB").unwrap_err();
        let LimitsError::BadByteMagnitude(reason) = err else {
            panic!("expected BadByteMagnitude(overflow), got other variant");
        };
        assert!(
            reason.contains("overflow"),
            "overflow diagnostic must mention overflow (got {reason:?})"
        );
    }

    // ── canonical-form: integer-magnitude duration codec gate ─────────────
    //
    // Direct successor to the `parse_byte_size` integer-magnitude gate on
    // the peer `:limits :memory` codec — every magnitude `render_duration`
    // emits is a non-negative integer (no decimal point, no leading sign,
    // no scientific notation). The parser's accepted set must match for
    // parse → render → parse to round-trip without canonical-form drift.
    // Pins every canonical-drift shape — fractional (`"1.5s"`),
    // decimal-shaped-integer (`"1.0s"`), half-unit (`"0.5m"`),
    // leading-`+` (`"+30s"`), leading-`-` (`"-30s"`) — plus the
    // complement-side pin (integer happy paths), the round-trip
    // convergence property, the BadDurationMagnitude-precedence pin
    // (genuinely unparseable inputs keep their narrower diagnostic), the
    // overflow-surface pin (u64-overflow on magnitude × unit surfaces at
    // parse time), and the serde-path pin (the gate fires at deserialize,
    // before any validate gate runs).

    #[test]
    fn parse_duration_rejects_fractional_seconds() {
        // The fail-before-pass-after pin: `"1.5s"` parsed cleanly on
        // every pre-gate codebase (f64::parse accepts the decimal), the
        // codec produced 1500ms, and `render_duration(1500ms)` emitted
        // `"1500ms"` on the next serialize — silently drifting the
        // canonical form away from the author's intent. The new gate
        // surfaces the round-trip break at the parser layer with a
        // self-locating diagnostic.
        let err = parse_duration("1.5s").unwrap_err();
        assert!(
            matches!(err, LimitsError::NonIntegerDurationMagnitude { ref value } if value == "1.5"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_duration_rejects_decimal_shaped_integer() {
        // The canonical-drift case where the *value* is integer but the
        // *form* carries a redundant decimal point — `"1.0s"` parses to
        // 1s (integer), but the renderer emits `"1s"` on the next
        // serialize (no decimal point). The parse-shape gate fires here
        // too so the codec's accepted set is exactly the renderer's
        // emitted set.
        let err = parse_duration("1.0s").unwrap_err();
        assert!(
            matches!(err, LimitsError::NonIntegerDurationMagnitude { ref value } if value == "1.0"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_duration_rejects_half_minute() {
        // `"0.5m"` parses to 30s; the renderer emits `"30s"` on the
        // next serialize. Pin the round-trip drift on the explicitly-
        // fractional case sized to land on a smaller-unit boundary, so
        // the gate's coverage includes both the "doesn't land on a
        // boundary" (1.5s → 1500ms) and "lands on a smaller-unit
        // boundary" (0.5m → 30s) drift shapes — the same two-shape
        // pattern the byte-size gate covers (1.5KiB → 1536, 0.5GiB →
        // 512MiB).
        let err = parse_duration("0.5m").unwrap_err();
        assert!(
            matches!(err, LimitsError::NonIntegerDurationMagnitude { ref value } if value == "0.5"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_duration_rejects_leading_plus() {
        // `"+30s"` parses through f64 as 30s; the renderer emits `"30s"`
        // on the next serialize. The leading `+` is not a renderer-
        // emitted shape, so it falls in the same canonical-drift class
        // as the fractional forms — surfacing under the same diagnostic
        // keeps the gate's coverage uniform across every non-canonical-
        // but-numeric input shape the parser would otherwise accept.
        let err = parse_duration("+30s").unwrap_err();
        assert!(
            matches!(err, LimitsError::NonIntegerDurationMagnitude { ref value } if value == "+30"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_duration_rejects_negative_seconds_via_integer_gate() {
        // The negative-magnitude class — pre-gate the parser routed
        // negatives through the `num < 0.0` check to `BadDurationMagnitude`;
        // the new digit-only gate fires earlier and routes the same
        // input to `NonIntegerDurationMagnitude` (negatives are not
        // digit-only). Pin the new diagnostic so a future relaxation
        // that re-routes negatives back to the old arm surfaces here.
        let err = parse_duration("-30s").unwrap_err();
        assert!(
            matches!(err, LimitsError::NonIntegerDurationMagnitude { ref value } if value == "-30"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_duration_continues_to_accept_integer_magnitudes() {
        // The complement-side pin: every canonical integer-magnitude
        // form the renderer emits must continue to parse to the same
        // value the renderer produced. Sweep the canonical authoring
        // shapes (ms, bare-s, s, m, h, and the bare-integer "0" zero-
        // shape) so a future tightening of the parser surfaces here as
        // a test failure rather than a silent regression.
        assert_eq!(parse_duration("0s").unwrap(), Duration::ZERO);
        assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("2m").unwrap(), Duration::from_secs(120));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_duration("3600").unwrap(), Duration::from_secs(3600));
    }

    #[test]
    fn parse_duration_round_trips_through_render_for_every_canonical_form() {
        // The structural property the gate makes load-bearing: every
        // value the parser accepts round-trips through `render_duration`
        // to a string the parser also accepts — and to the *same* value.
        // Sweep the values the renderer emits canonically (ms / s / m /
        // h boundaries plus a non-aligned millisecond) so a future
        // codec change that breaks round-trip convergence surfaces here.
        for d in [
            Duration::from_millis(1),
            Duration::from_millis(500),
            Duration::from_millis(1500),
            Duration::from_secs(1),
            Duration::from_secs(30),
            Duration::from_secs(60),
            Duration::from_secs(120),
            Duration::from_secs(3600),
        ] {
            let rendered = render_duration(d);
            let reparsed = parse_duration(&rendered)
                .unwrap_or_else(|e| panic!("render({d:?}) = {rendered:?} must reparse, got {e:?}"));
            assert_eq!(
                reparsed, d,
                "round-trip drift on {d:?}: rendered={rendered:?}, reparsed={reparsed:?}",
            );
        }
    }

    #[test]
    fn parse_duration_keeps_bad_magnitude_for_unparseable_input() {
        // The precedence pin: the new `NonIntegerDurationMagnitude` arm
        // distinguishes *non-canonical-but-numeric* (`"1.5"`, `"+30"`,
        // `"-30"`) from *genuinely-unparseable* (`"abc"`, `"--1"`) so
        // the existing `BadDurationMagnitude` diagnostic's wording
        // remains load-bearing for the latter class — the gate is
        // additive, not replacing.
        let err = parse_duration("abcs").unwrap_err();
        assert!(
            matches!(err, LimitsError::BadDurationMagnitude(_)),
            "got {err:?}"
        );
        let err = parse_duration("--1s").unwrap_err();
        assert!(
            matches!(err, LimitsError::BadDurationMagnitude(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_duration_overflow_surfaces_as_bad_magnitude() {
        // `u64::MAX h` overflows the seconds computation (magnitude ×
        // 3600); the parser surfaces the overflow as a
        // `BadDurationMagnitude` with an overflow-shaped wording so the
        // diagnostic names the offending magnitude × unit pair at parse
        // time. Matches `parse_byte_size`'s overflow-surface arm
        // structurally.
        let err = parse_duration("18446744073709551615h").unwrap_err();
        let LimitsError::BadDurationMagnitude(reason) = err else {
            panic!("expected BadDurationMagnitude(overflow), got other variant");
        };
        assert!(
            reason.contains("overflow"),
            "overflow diagnostic must mention overflow (got {reason:?})"
        );
    }

    #[test]
    fn de_duration_rejects_fractional_value_through_serde() {
        // The serde-path pin: a `:limits :wall-clock` carrying a
        // fractional magnitude (`"1.5s"`) must fail at deserialize time,
        // not silently round-trip the value through the f64 parser. Pin
        // both the success-on-canonical path (the integer form
        // deserializes cleanly) and the failure-on-non-canonical path
        // (the fractional form is rejected by the codec before any
        // validate gate runs).
        let json = r#"{"wallClock":"1.5s"}"#;
        let err = serde_json::from_str::<LimitsSpec>(json).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("non-negative integer"),
            "serde diagnostic must surface the integer-magnitude reason verbatim \
             (got {msg:?})"
        );

        // The integer-form complement — same author-side intent
        // (1.5s = 1500ms), written in the canonical form the renderer
        // would emit, deserializes cleanly.
        let json = r#"{"wallClock":"1500ms"}"#;
        let l: LimitsSpec = serde_json::from_str(json).unwrap();
        assert_eq!(l.wall_clock, Some(Duration::from_millis(1500)));
    }

    #[test]
    fn de_byte_size_rejects_fractional_value_through_serde() {
        // The serde-path pin: a `:limits :memory` carrying a fractional
        // magnitude (`"1.5KiB"`) must fail at deserialize time, not
        // silently round-trip the value through the f64 parser. Pin
        // both the success-on-canonical path (the integer form
        // deserializes cleanly) and the failure-on-non-canonical path
        // (the fractional form is rejected by the codec before any
        // validate gate runs).
        let json = r#"{"memory":"1.5KiB"}"#;
        let err = serde_json::from_str::<LimitsSpec>(json).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("non-negative integer"),
            "serde diagnostic must surface the integer-magnitude reason verbatim (got {msg:?})"
        );

        // The integer-form complement — same author-side intent
        // (1.5KiB = 1536 bytes), written in the canonical form the
        // renderer would emit, deserializes cleanly.
        let json = r#"{"memory":"1536"}"#;
        let l: LimitsSpec = serde_json::from_str(json).unwrap();
        assert_eq!(l.memory, Some(1536));
    }

    // ── canonical-form: integer-magnitude millicores codec gate ───────────
    //
    // Direct successor to the `parse_byte_size` / `parse_duration` /
    // shared `supervisor::duration_codec` / `rate_limit_codec`
    // integer-magnitude gates on the four peer typed codecs in
    // caixa-core — closes the sixth (and last) typed-codec surface in
    // the crate. Every magnitude `render_millicores` emits is a
    // non-negative integer (`format!("{m}m")`) — no decimal point, no
    // leading sign, no scientific notation. The parser's accepted set
    // must match for parse → render → parse to round-trip without
    // canonical-form drift. Pins every canonical-drift shape —
    // leading-`+` (`"+500m"` / `"+2"`, the load-bearing class the
    // digit-only gate closes beyond `u32::from_str` strictness),
    // leading-`-` (`"-100m"`), fractional (`"1.5"`), decimal-shaped-
    // integer on both authoring paths (`"500.0m"` / `"2.0"`), the
    // bare-`m`-with-no-magnitude pin, the empty-string pin, the
    // garbage-precedence pin (genuinely unparseable inputs keep the
    // narrower `BadMillicores` diagnostic), the u32-overflow surface
    // pin on both the `m`-suffix and bare-core multiply paths, the
    // complement-side pin (every integer happy path the gate must
    // continue to accept), the round-trip convergence property, and
    // the serde-path pin (the gate fires at deserialize, before any
    // validate gate runs).

    #[test]
    fn parse_millicores_rejects_fractional_magnitude() {
        // The fail-before-pass-after pin on the bare-core path:
        // `"1.5"` parsed cleanly on no pre-gate codebase (`u32::from_str`
        // rejects the decimal), but the diagnostic was value-laundered
        // (the bare `BadMillicores("1.5")` wording didn't name the
        // canonical-form remediation or the round-trip drift the next
        // emit would produce — `1.5 cores × 1000 = 1500 millicores` →
        // `"1500m"` on the renderer). The gate routes the same input to
        // `NonIntegerMillicoreMagnitude` with the canonical-form wording.
        let err = parse_millicores("1.5").unwrap_err();
        assert!(
            matches!(err, LimitsError::NonIntegerMillicoreMagnitude { ref value } if value == "1.5"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_millicores_rejects_decimal_shaped_integer_with_suffix() {
        // The canonical-drift case on the `m`-suffix path where the
        // *value* is integer but the *form* carries a redundant decimal
        // point — `"500.0m"` parses to 500 millicores (integer), but
        // the renderer emits `"500m"` on the next serialize (no decimal
        // point). The parse-shape gate fires here too so the codec's
        // accepted set is exactly the renderer's emitted set — same
        // shape as `parse_byte_size`'s `"1.0MiB"` case.
        let err = parse_millicores("500.0m").unwrap_err();
        assert!(
            matches!(err, LimitsError::NonIntegerMillicoreMagnitude { ref value } if value == "500.0"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_millicores_rejects_decimal_shaped_integer_bare_core() {
        // The decimal-shaped-integer pin on the bare-core path —
        // `"2.0"` would be 2000 millicores (the canonical `"2000m"`),
        // but the redundant decimal point is not a renderer-emitted
        // shape. Surfaces under the same diagnostic as the `m`-suffix
        // path so the gate's coverage is uniform across both authoring
        // paths.
        let err = parse_millicores("2.0").unwrap_err();
        assert!(
            matches!(err, LimitsError::NonIntegerMillicoreMagnitude { ref value } if value == "2.0"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_millicores_rejects_leading_plus_sign_with_suffix() {
        // The load-bearing class the digit-only gate closes beyond
        // `u32::from_str`'s strictness: current Rust `u32::from_str`
        // permissively accepts `"+500"` → 500, so `"+500m"` parsed
        // cleanly through the pre-gate codec to `RateLimit`-shaped
        // 500 millicores and serde silently round-tripped to `"500m"`
        // on the next emit — a *different* canonical string. Same
        // shape as `parse_byte_size`'s `"+1024"` (875 commit) and
        // `parse_duration`'s `"+30s"` (1027 commit) cases on the peer
        // codecs.
        let err = parse_millicores("+500m").unwrap_err();
        assert!(
            matches!(err, LimitsError::NonIntegerMillicoreMagnitude { ref value } if value == "+500"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_millicores_rejects_leading_plus_sign_bare_core() {
        // The leading-`+` pin on the bare-core path — `"+2"` parsed
        // through `u32::from_str` as 2 → 2000 millicores → `"2000m"`
        // on the renderer; canonical-drift. The digit-only gate routes
        // the same input to `NonIntegerMillicoreMagnitude`, peer with
        // the `m`-suffix path.
        let err = parse_millicores("+2").unwrap_err();
        assert!(
            matches!(err, LimitsError::NonIntegerMillicoreMagnitude { ref value } if value == "+2"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_millicores_rejects_leading_minus_sign() {
        // The negative-magnitude class — pre-gate `u32::from_str`
        // rejected negatives but the diagnostic collapsed onto the
        // opaque `BadMillicores("-100m")` wording. The digit-only gate
        // fires earlier and routes the same input to
        // `NonIntegerMillicoreMagnitude` (negatives are not digit-only,
        // and `i64::from_str` accepts the leading sign so the numeric
        // arm matches). Pin the new diagnostic so a future relaxation
        // that re-routes negatives back to the old arm surfaces here.
        let err = parse_millicores("-100m").unwrap_err();
        assert!(
            matches!(err, LimitsError::NonIntegerMillicoreMagnitude { ref value } if value == "-100"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_millicores_rejects_empty_string() {
        // The empty-input pin — `""` is not a magnitude at all. Pre-
        // gate this fell through to `s.parse::<u32>()` and surfaced as
        // a generic parse failure with the same `BadMillicores("")`
        // wording; the explicit empty-check at the top of the codec
        // surfaces the same diagnostic earlier and makes the empty-
        // input class structurally distinct from the digit-only /
        // numeric / garbage arms below.
        let err = parse_millicores("").unwrap_err();
        assert!(matches!(err, LimitsError::BadMillicores(_)), "got {err:?}");
    }

    #[test]
    fn parse_millicores_rejects_bare_unit_with_no_magnitude() {
        // The bare-`m`-with-no-magnitude pin — `"m"` strips to `""`,
        // which is not a magnitude at all. The canonical millicores
        // authoring form requires a magnitude in front of the unit
        // (`"500m"`, not `"m"`). Surface as `BadMillicores` so the
        // narrower-arm wording stays load-bearing for this class.
        let err = parse_millicores("m").unwrap_err();
        assert!(matches!(err, LimitsError::BadMillicores(_)), "got {err:?}");
    }

    #[test]
    fn parse_millicores_garbage_still_falls_through_to_bad_millicores() {
        // The precedence pin: the new `NonIntegerMillicoreMagnitude`
        // arm distinguishes *non-canonical-but-numeric* (`"1.5"`,
        // `"+500m"`, `"-100m"`, `"500.0m"`) from *genuinely-
        // unparseable* (`"abc"`, `"--1m"`, `"foo"`) so the existing
        // `BadMillicores` diagnostic's wording remains load-bearing
        // for the latter class — the gate is additive, not replacing.
        // Pin both arms so a future relaxation that collapses them
        // surfaces here.
        let err = parse_millicores("abc").unwrap_err();
        assert!(matches!(err, LimitsError::BadMillicores(_)), "got {err:?}");
        let err = parse_millicores("--1m").unwrap_err();
        assert!(matches!(err, LimitsError::BadMillicores(_)), "got {err:?}");
        let err = parse_millicores("foo").unwrap_err();
        assert!(matches!(err, LimitsError::BadMillicores(_)), "got {err:?}");
    }

    #[test]
    fn parse_millicores_u32_overflow_with_suffix_surfaces_as_overflow() {
        // The u32-overflow surface pin on the `m`-suffix path: a
        // magnitude exceeding `u32::MAX` (4294967296 = u32::MAX + 1)
        // surfaces as `BadMillicores` with an overflow-shaped wording
        // naming the offending magnitude verbatim. The digit-only
        // guard guarantees every byte is `[0-9]`, so overflow is the
        // only remaining `u32::from_str` failure mode — the overflow
        // arm is no longer in unreachable-by-prior-gate territory.
        // Matches the overflow-arm shape on `parse_byte_size` /
        // `parse_duration` / `rate_limit_codec`.
        let err = parse_millicores("4294967296m").unwrap_err();
        let LimitsError::BadMillicores(reason) = err else {
            panic!("expected BadMillicores(overflow), got other variant");
        };
        assert!(
            reason.contains("overflow"),
            "overflow diagnostic must mention overflow (got {reason:?})"
        );
    }

    #[test]
    fn parse_millicores_bare_core_overflow_surfaces_as_overflow() {
        // The u32-overflow surface pin on the bare-core path: a
        // magnitude that fits u32 on its own but overflows on the
        // `× 1000` conversion to millicores surfaces as
        // `BadMillicores` with an overflow-shaped wording. Pre-gate
        // the codec used `saturating_mul(1000)` which silently
        // saturated the result at `u32::MAX` — landing as the cap
        // value far from the author's intent and bypassing any
        // future validate-time upper-bound gate the `:cpu` axis
        // grows. The `checked_mul` rewrite surfaces the overflow at
        // parse time. (4294968 cores × 1000 = 4294968000 > u32::MAX
        // = 4294967295 — the smallest digit-string that overflows
        // u32 on the × 1000 multiply while fitting u32 on its own.)
        let err = parse_millicores("4294968").unwrap_err();
        let LimitsError::BadMillicores(reason) = err else {
            panic!("expected BadMillicores(× 1000 overflow), got other variant");
        };
        assert!(
            reason.contains("overflow"),
            "× 1000 overflow diagnostic must mention overflow (got {reason:?})"
        );
    }

    #[test]
    fn parse_millicores_continues_to_accept_canonical_forms() {
        // The complement-side pin: every canonical integer-magnitude
        // form the renderer emits must continue to parse to the same
        // value the renderer produced. Sweep the canonical authoring
        // shapes on both paths (the `m`-suffix path: `"0m"`, `"500m"`,
        // `"2000m"`; the bare-core shorthand: `"0"`, `"2"`, `"4"`) so
        // a future tightening of the parser surfaces here as a test
        // failure rather than a silent regression. The `0` case is at
        // the codec layer only; `validate_rejects_zero_cpu` rejects
        // `Some(0)` one level up.
        assert_eq!(parse_millicores("0m").unwrap(), 0);
        assert_eq!(parse_millicores("500m").unwrap(), 500);
        assert_eq!(parse_millicores("1500m").unwrap(), 1500);
        assert_eq!(parse_millicores("2000m").unwrap(), 2000);
        assert_eq!(parse_millicores("0").unwrap(), 0);
        assert_eq!(parse_millicores("2").unwrap(), 2000);
        assert_eq!(parse_millicores("4").unwrap(), 4000);
    }

    #[test]
    fn parse_millicores_round_trips_through_render_for_every_canonical_form() {
        // The structural property the gate makes load-bearing: every
        // value the parser accepts round-trips through
        // `render_millicores` to a string the parser also accepts —
        // and to the *same* value. Sweep the values the renderer emits
        // canonically (zero, sub-core, single-core boundary, multi-
        // core, and a non-1000-multiple millicore value) so a future
        // codec change that breaks round-trip convergence surfaces
        // here, not at a downstream renderer that double-emits a
        // typed slot.
        for m in [0u32, 1, 100, 500, 1000, 1500, 2000, 12345] {
            let rendered = render_millicores(m);
            let reparsed = parse_millicores(&rendered)
                .unwrap_or_else(|e| panic!("render({m}) = {rendered:?} must reparse, got {e:?}"));
            assert_eq!(
                reparsed, m,
                "round-trip drift on {m}: rendered={rendered:?}, reparsed={reparsed}",
            );
        }
    }

    #[test]
    fn de_millicores_rejects_leading_plus_through_serde() {
        // The serde-path pin: a `:limits :cpu` carrying a leading-`+`
        // magnitude (`"+500m"`) must fail at deserialize time, not
        // silently round-trip the value through `u32::from_str`'s
        // permissive sign-acceptance. Pin both the success-on-canonical
        // path (the integer form deserializes cleanly) and the
        // failure-on-non-canonical path (the leading-`+` form is
        // rejected by the codec before any validate gate runs).
        let json = r#"{"cpu":"+500m"}"#;
        let err = serde_json::from_str::<LimitsSpec>(json).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("non-negative integer"),
            "serde diagnostic must surface the integer-magnitude reason verbatim \
             (got {msg:?})"
        );

        // The integer-form complement — same author-side intent
        // (500 millicores), written in the canonical form the renderer
        // would emit, deserializes cleanly.
        let json = r#"{"cpu":"500m"}"#;
        let l: LimitsSpec = serde_json::from_str(json).unwrap();
        assert_eq!(l.cpu, Some(500));
    }

    // ── canonical-form: integer-millisecond :wall-clock gate ──────────────
    //
    // The peer typed-`Duration` axes routed through
    // `supervisor::duration_codec` (`:politicas :timeout` a4ae535,
    // `:circuit-breaker :window` a4ae535) already gate on
    // `is_integer_millisecond_duration` because the codec's `render`
    // truncates to `as_millis()` and parses with integer-ms granularity;
    // this crate's in-module `render_duration` / `parse_duration` pair
    // carries the same `as_millis()`-truncation shape, so the same sub-
    // millisecond-residue footgun lived on this axis until this gate
    // landed. The tests below pin the fail-before-pass-after boundary,
    // the diagnostic shape, the cross-arm zero-then-canonical ordering
    // matching the `:politicas` peer, the integer-ms happy-path sweep,
    // and the codec round-trip property (every validated `wall_clock`
    // survives serialize → deserialize equality).

    #[test]
    fn validate_rejects_sub_millisecond_wall_clock() {
        // The fail-before-pass-after pin: a programmatic
        // `Duration::from_micros(1500)` (= 1_500_000 ns) silently passed
        // validate on every pre-gate codebase, then truncated to
        // `as_millis() == 1` on first serialize — `render_duration`
        // emits `"1ms"`, the codec parses it back to
        // `Duration::from_millis(1)` = 1_000_000 ns, the typed
        // `wall_clock` no longer matches its rendered form.
        let l = LimitsSpec {
            wall_clock: Some(Duration::from_micros(1500)),
            ..Default::default()
        };
        match l.validate().unwrap_err() {
            LimitsError::WallClockNotCanonical { wall_clock } => {
                assert_eq!(wall_clock, Duration::from_micros(1500));
            }
            other => panic!("expected WallClockNotCanonical, got {other:?}"),
        }
    }

    #[test]
    fn validate_rejects_one_nanosecond_wall_clock() {
        // The far-sub-ms case: `Duration::from_nanos(1)` is non-zero
        // (so `WallClockZero` doesn't fire) but `as_millis() == 0`, so
        // `render_duration` emits the literal `"0s"` — the next serde
        // round-trip would parse back to `Duration::ZERO`, which the
        // `WallClockZero` arm then rejects on re-validate. The
        // canonical-form gate at this layer surfaces a self-locating
        // diagnostic naming the offending Duration verbatim rather
        // than a downstream `WallClockZero` whose remediation points
        // at omitting the slot.
        let l = LimitsSpec {
            wall_clock: Some(Duration::from_nanos(1)),
            ..Default::default()
        };
        match l.validate().unwrap_err() {
            LimitsError::WallClockNotCanonical { wall_clock } => {
                assert_eq!(wall_clock, Duration::from_nanos(1));
            }
            other => panic!("expected WallClockNotCanonical, got {other:?}"),
        }
    }

    #[test]
    fn validate_rejects_nanosecond_past_canonical_boundary() {
        // The 1-ns-past-1ms boundary case: a `Duration` carrying
        // 1_000_001 ns is structurally past the integer-ms granularity
        // floor — `subsec_nanos() % 1_000_000 == 1`. The codec
        // round-trip would truncate to `1ms` and the consumer would
        // observe a 1-ns drift on every emit. Same boundary the peer
        // `is_integer_millisecond_duration_predicate_tracks_codec` test
        // in aplicacao.rs pins for the `:politicas` axes.
        let w = Duration::from_nanos(1_000_001);
        let l = LimitsSpec {
            wall_clock: Some(w),
            ..Default::default()
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::WallClockNotCanonical { wall_clock: w }
        );
    }

    #[test]
    fn validate_accepts_integer_millisecond_wall_clock_values() {
        // The positive-control sweep: every `Duration` the codec can
        // round-trip losslessly — the canonical `<integer>{ms,s,m,h}`
        // set the `render_duration` / `parse_duration` pair emits and
        // accepts — passes `validate` without surfacing the new
        // canonical-form arm. Mirrors
        // `accepts_policy_retries_typical_values` /
        // `accepts_circuit_breaker_max_failures_typical_values` on
        // sibling axes.
        for w in [
            Duration::from_millis(1),
            Duration::from_millis(500),
            Duration::from_millis(1500),
            Duration::from_secs(1),
            Duration::from_secs(30),
            Duration::from_secs(60),
            Duration::from_secs(120),
            Duration::from_secs(3600),
        ] {
            let l = LimitsSpec {
                wall_clock: Some(w),
                ..Default::default()
            };
            l.validate()
                .unwrap_or_else(|e| panic!("integer-ms {w:?} must validate, got {e:?}"));
        }
    }

    #[test]
    fn validate_wall_clock_zero_takes_precedence_over_canonical_gate() {
        // Cross-arm ordering pin: `Duration::ZERO` has
        // `subsec_nanos() == 0` and would otherwise pass the
        // canonical-form arm — the zero-floor arm must fire first so
        // the more self-locating `WallClockZero` diagnostic (with its
        // omit-axis remediation directly named) leads. Same posture
        // every peer zero-then-shape gate uses
        // (`PolicyTimeoutZero` → `PolicyTimeoutNotCanonical`,
        // `PolicyBreakerZeroWindow` → `PolicyBreakerWindowNotCanonical`).
        let l = LimitsSpec {
            wall_clock: Some(Duration::ZERO),
            ..Default::default()
        };
        assert_eq!(l.validate().unwrap_err(), LimitsError::WallClockZero);
    }

    #[test]
    fn wall_clock_canonical_diagnostic_carries_offending_duration() {
        // Diagnostic-shape pin: the canonical-form arm names the
        // offending `Duration` verbatim so the author's grep lands on
        // the field's value, not a generic "duration not canonical"
        // message. Same shape every other typed-cap arm on this
        // surface carries (`MemoryExceedsWasm32Cap` carries the
        // offending byte count verbatim, `PolicyRetriesExceedsCap`
        // carries the offending retry count verbatim,
        // `PolicyBreakerMaxFailuresExceedsCap` carries the offending
        // u32 verbatim).
        let w = Duration::from_micros(500);
        let l = LimitsSpec {
            wall_clock: Some(w),
            ..Default::default()
        };
        let err = l.validate().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("500"),
            "diagnostic must carry the offending magnitude verbatim (got {msg:?})"
        );
    }

    #[test]
    fn wall_clock_validated_value_round_trips_through_codec() {
        // The structural property the canonical-ms gate enforces:
        // every `LimitsSpec::wall_clock` past `LimitsSpec::validate`
        // round-trips losslessly through the in-module duration codec
        // (serialize → string → deserialize → equal value). Pin this
        // end-to-end so a future change to either side (the validate
        // gate's accepted granularity, the codec's parse/render unit
        // set) that breaks the alignment surfaces here. Peer of
        // `policy_timeout_validated_value_round_trips_through_codec` /
        // `circuit_breaker_window_validated_value_round_trips_through_codec`
        // on the sibling `:politicas` axes.
        for w in [
            Duration::from_millis(1),
            Duration::from_millis(1500),
            Duration::from_secs(30),
            Duration::from_secs(3600),
        ] {
            let l = LimitsSpec {
                wall_clock: Some(w),
                ..Default::default()
            };
            l.validate().unwrap();
            let json = serde_json::to_string(&l).unwrap();
            let back: LimitsSpec = serde_json::from_str(&json).unwrap();
            assert_eq!(
                back.wall_clock, l.wall_clock,
                "every validated :wall-clock must round-trip losslessly through the codec"
            );
        }
    }

    // ── value-shape: :wall-clock upper bound — 1h ceiling ──────────────────
    //
    // The third typed-`Duration` axis brought to the uniform top edge
    // `LIMITS_WALL_CLOCK_MAX` = 1h established by the prior cap lifts
    // on `:politicas :timeout` (POLICY_TIMEOUT_MAX) and
    // `:politicas :circuit-breaker :window` (POLICY_BREAKER_WINDOW_MAX).
    // Mirrors the test discipline those peers carry: the
    // fail-before-pass-after pin, the 1ms-boundary pin, the
    // far-above-cap sweep (24h / 7d / ~11.5d — the values a
    // `(:wall-clock "24h")` typo or copy-paste typically lands), the
    // inclusive-at-cap positive control, the production-band positive-
    // control sweep, the cross-arm zero-then-cap and
    // canonical-then-cap ordering pins, the diagnostic-shape pin
    // carrying the offending `Duration` verbatim, and the cap-value
    // literal-identity + codec-round-trip pins anchoring the constant
    // to the codec's largest emitted unit and to its peer constants.

    #[test]
    fn validate_rejects_wall_clock_above_cap() {
        // The fail-before-pass-after pin: 3601s = 1h + 1s is
        // structurally one canonical-tick past the
        // [`LIMITS_WALL_CLOCK_MAX`] ceiling (1h = 3600s) — an
        // integer-millisecond magnitude the canonical-form arm above
        // accepts cleanly, that the in-module duration codec
        // round-trips losslessly as `"3601s"`, and that silently
        // passed validate on every pre-gate codebase because the typed
        // slot's only checks were the zero-floor and canonical-form
        // arms. The wasm-engine consuming the value (the M2.5
        // `wasm-engine`'s epoch-deadline cancellation hook, the future
        // caixa-helm `pleme-computeunit` chart's `:limits` value
        // mapping) reaches for a `Duration` so long no realistic
        // synchronous wasm call hits it, far from the source
        // caixa.lisp.
        let w = LIMITS_WALL_CLOCK_MAX + Duration::from_secs(1);
        let l = LimitsSpec {
            wall_clock: Some(w),
            ..Default::default()
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::WallClockExceedsCap { wall_clock: w }
        );
    }

    #[test]
    fn validate_rejects_wall_clock_one_millisecond_above_cap() {
        // Boundary case: exactly 1ms past the cap (the granularity the
        // canonical-form gate enforces). Catches a future "strictly
        // less than" half-measure and pins the diagnostic to name the
        // offending `Duration` verbatim. Peer of
        // `rejects_policy_timeout_one_millisecond_above_cap` /
        // `rejects_circuit_breaker_window_one_millisecond_above_cap`
        // on the sibling typed-`Duration` axes' top edges.
        let w = LIMITS_WALL_CLOCK_MAX + Duration::from_millis(1);
        let l = LimitsSpec {
            wall_clock: Some(w),
            ..Default::default()
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::WallClockExceedsCap { wall_clock: w }
        );
    }

    #[test]
    fn validate_rejects_wall_clock_far_above_cap() {
        // The "obvious authoring footgun" case: a `(:wall-clock "24h")`
        // or `(:wall-clock "7d")` — values the canonical-form arm
        // accepts as integer-millisecond magnitudes, the codec
        // round-trips losslessly through serde, but the wasm-engine
        // cannot honor as a meaningful per-call deadline. Until this
        // gate landed validate accepted them. Pin the common
        // above-cap values (24h, 7d, ~11.5d) so a future relaxation
        // that drops the upper bound surfaces here.
        for w in [
            Duration::from_secs(86_400),    // 24h
            Duration::from_secs(604_800),   // 7d
            Duration::from_secs(1_000_000), // ~11.5 days
        ] {
            let l = LimitsSpec {
                wall_clock: Some(w),
                ..Default::default()
            };
            assert_eq!(
                l.validate().unwrap_err(),
                LimitsError::WallClockExceedsCap { wall_clock: w }
            );
        }
    }

    #[test]
    fn validate_accepts_wall_clock_at_cap() {
        // The boundary value — exactly [`LIMITS_WALL_CLOCK_MAX`] (1h)
        // — must validate. The cap is inclusive on the top edge,
        // matching the [`crate::POLICY_TIMEOUT_MAX`] /
        // [`crate::POLICY_BREAKER_WINDOW_MAX`] /
        // [`LIMITS_MEMORY_WASM32_MAX_BYTES`] discipline on the sibling
        // capped axes. Pin the boundary explicitly so a future
        // off-by-one tightening (`>= LIMITS_WALL_CLOCK_MAX` instead of
        // `>`) surfaces here as a test failure rather than a silent
        // contract narrowing.
        let l = LimitsSpec {
            wall_clock: Some(LIMITS_WALL_CLOCK_MAX),
            ..Default::default()
        };
        l.validate()
            .expect("wall_clock == LIMITS_WALL_CLOCK_MAX must validate");
    }

    #[test]
    fn validate_accepts_wall_clock_typical_values() {
        // The documented per-request production-playbook band positive-
        // control sweep — every value Envoy / Istio / Linkerd / AWS
        // App Mesh / Kubernetes ingress-nginx recommend
        // (1ms..=3600s) must pass, plus a sweep through the
        // long-running-workflow band (5m, 15m, 30m, 1h) the cap
        // accepts. Mirrors `accepts_policy_timeout_typical_values` on
        // the sibling `:politicas :timeout` axis.
        for w in [
            Duration::from_millis(1),
            Duration::from_millis(500),
            Duration::from_secs(1),
            Duration::from_secs(10),
            Duration::from_secs(15), // Envoy default
            Duration::from_secs(30),
            Duration::from_secs(60),  // AWS App Mesh typical
            Duration::from_secs(300), // 5m
            Duration::from_secs(900), // 15m
            Duration::from_secs(1800),
            Duration::from_secs(3600), // exactly 1h, the cap
        ] {
            let l = LimitsSpec {
                wall_clock: Some(w),
                ..Default::default()
            };
            l.validate()
                .unwrap_or_else(|e| panic!("wall_clock={w:?} must validate; got {e:?}"));
        }
    }

    #[test]
    fn wall_clock_zero_takes_precedence_over_cap() {
        // The cross-arm ordering pin: `Duration::ZERO` is structurally
        // outside both `>= 1ms` (zero-floor) and `<= LIMITS_WALL_CLOCK_MAX`
        // (cap), but the zero-floor diagnostic is the more
        // self-locating one (it directly names the omit-axis
        // remediation), so the validate gate must fire on zero first.
        // Same shape every other zero-then-shape ordering on this
        // surface uses (`MemoryZero` then `MemoryExceedsWasm32Cap`,
        // `PolicyTimeoutZero` then `PolicyTimeoutExceedsCap`).
        let l = LimitsSpec {
            wall_clock: Some(Duration::ZERO),
            ..Default::default()
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::WallClockZero,
            "Duration::ZERO must surface the zero-floor diagnostic, not the cap diagnostic"
        );
    }

    #[test]
    fn wall_clock_canonical_takes_precedence_over_cap() {
        // The cross-arm ordering pin: a `Duration` that is *both*
        // sub-millisecond (non-canonical-form) and structurally above
        // the cap surfaces the canonical-form diagnostic first,
        // because the round-trip-shape break is the more fundamental
        // issue (the value can't even round-trip through the codec, so
        // the cap diagnostic naming `1ms..=1h` would be misleading —
        // there's no integer-ms form of the offending value). Pin the
        // order so a future refactor that reorders the arms surfaces
        // here as a test failure rather than a silent diagnostic
        // regression. Peer of
        // `policy_timeout_canonical_takes_precedence_over_cap`.
        let w = LIMITS_WALL_CLOCK_MAX + Duration::from_nanos(1);
        let l = LimitsSpec {
            wall_clock: Some(w),
            ..Default::default()
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::WallClockNotCanonical { wall_clock: w },
            "sub-ms above-cap value must surface the canonical-form diagnostic, not the cap diagnostic"
        );
    }

    #[test]
    fn wall_clock_cap_diagnostic_carries_offending_value() {
        // The diagnostic-shape pin: the offending `Duration` is
        // carried verbatim into the
        // [`LimitsError::WallClockExceedsCap`] variant so the surfaced
        // error message names the value the author wrote, not just
        // the cap. Same self-locating diagnostic shape every other
        // typed-cap arm on this surface carries
        // (`MemoryExceedsWasm32Cap` carries the offending byte count
        // verbatim, `PolicyTimeoutExceedsCap` carries the offending
        // `Duration` verbatim).
        let w = Duration::from_secs(7200); // 2h
        let l = LimitsSpec {
            wall_clock: Some(w),
            ..Default::default()
        };
        let err = l.validate().unwrap_err();
        assert!(
            matches!(err, LimitsError::WallClockExceedsCap { wall_clock } if wall_clock == w),
            "got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("7200"),
            ":limits :wall-clock cap diagnostic must carry the offending value verbatim (got: {msg})"
        );
    }

    #[test]
    fn wall_clock_cap_pins_canonical_value() {
        // The [`LIMITS_WALL_CLOCK_MAX`] constant pins the value at
        // exactly 1 hour (3600s = 3_600_000ms) — the largest unit the
        // shared duration codec emits as a clean canonical string
        // (`"<n>h"`). Pinning the literal value here surfaces a future
        // drift (a relaxation to 24h, a tightening to 5m) as a
        // deliberate test edit, not a silent contract narrowing.
        //
        // The three typed-`Duration` caps on the validation surface
        // (`LIMITS_WALL_CLOCK_MAX` per-process, `POLICY_TIMEOUT_MAX`
        // per-edge, `POLICY_BREAKER_WINDOW_MAX` per-breaker) share a
        // single uniform top edge at the codec's largest emitted unit
        // — a structural-property invariant the equality assertions
        // here enshrine, so a future drift on any of the three
        // surfaces as a deliberate test edit. Same shape every other
        // typed-cap value pin uses
        // (`policy_timeout_cap_pins_canonical_value`,
        // `circuit_breaker_window_cap_pins_canonical_value`).
        assert_eq!(LIMITS_WALL_CLOCK_MAX, Duration::from_secs(3600));
        assert_eq!(LIMITS_WALL_CLOCK_MAX.as_millis(), 3_600_000);
        assert_eq!(LIMITS_WALL_CLOCK_MAX, crate::POLICY_TIMEOUT_MAX);
        assert_eq!(LIMITS_WALL_CLOCK_MAX, crate::POLICY_BREAKER_WINDOW_MAX);
    }

    #[test]
    fn wall_clock_cap_value_round_trips_through_codec() {
        // The codec round-trip property the cap arm preserves: the
        // [`LIMITS_WALL_CLOCK_MAX`] constant itself round-trips through
        // the in-module duration codec — every value at the cap
        // renders to a clean canonical string (`"1h"`) and parses back
        // to the same `Duration`. Pin this so a future drift between
        // the cap constant and the codec's largest emitted unit
        // surfaces here. Same shape every other typed boundary pin on
        // this surface uses
        // (`wasm32_memory_cap_matches_parsed_4_gib`,
        // `policy_timeout_cap_value_round_trips_through_codec`).
        let l = LimitsSpec {
            wall_clock: Some(LIMITS_WALL_CLOCK_MAX),
            ..Default::default()
        };
        let json = serde_json::to_string(&l).unwrap();
        assert!(
            json.contains("\"1h\""),
            "the LIMITS_WALL_CLOCK_MAX value must render to the canonical \"1h\" form (got: {json})"
        );
        let back: LimitsSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.wall_clock, Some(LIMITS_WALL_CLOCK_MAX));
        l.validate()
            .expect("LIMITS_WALL_CLOCK_MAX itself must pass validate");
    }

    // ── value-shape: :cpu upper bound — 128-core schedulability ceiling ─────
    //
    // The third `LimitsSpec` axis brought to a top-edge cap, peer to
    // the `:memory` wasm32 ceiling and the `:wall-clock` 1h ceiling.
    // Mirrors the test discipline those peers carry: the
    // fail-before-pass-after pin, the one-millicore-boundary pin, the
    // far-above-cap sweep, the inclusive-at-cap positive control, the
    // production-band positive-control sweep, the cross-arm zero-then-
    // cap ordering pin, the diagnostic-shape pin carrying the offending
    // value verbatim, and the cap-value literal-identity + codec
    // round-trip pins anchoring the constant.

    #[test]
    fn validate_rejects_cpu_above_cap() {
        // The fail-before-pass-after pin: 128_001m = 128 cores + 1
        // millicore is structurally one canonical-tick past the
        // [`LIMITS_CPU_MILLICORES_MAX`] ceiling — a `u32` magnitude the
        // millicore codec round-trips losslessly as `"128001m"`, and
        // that silently passed validate on every pre-gate codebase
        // because the typed slot's only check was the zero-floor arm.
        // The Kubernetes scheduler consuming the value (via the
        // `pleme-computeunit` chart's `resources.requests.cpu`
        // projection) cannot bind the pod to any node, far from the
        // source caixa.lisp.
        let m = LIMITS_CPU_MILLICORES_MAX + 1;
        let l = LimitsSpec {
            cpu: Some(m),
            ..Default::default()
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::CpuExceedsCap { millicores: m }
        );
    }

    #[test]
    fn validate_rejects_cpu_far_above_cap() {
        // The "obvious authoring footgun" case: a `(:cpu "1000000m")`
        // (1000 cores) or `(:cpu "4294967295m")` (≈ u32::MAX) — values
        // the millicore codec accepts cleanly, the codec round-trips
        // losslessly through serde, but the Kubernetes scheduler
        // cannot bind to any node. Until this gate landed validate
        // accepted them. Pin the common above-cap values (1000 cores,
        // 10_000 cores, u32::MAX) so a future relaxation that drops
        // the upper bound surfaces here. Peer of
        // `validate_rejects_memory_8_gib` /
        // `validate_rejects_wall_clock_far_above_cap`.
        for m in [1_000_000_u32, 10_000_000, u32::MAX] {
            let l = LimitsSpec {
                cpu: Some(m),
                ..Default::default()
            };
            assert_eq!(
                l.validate().unwrap_err(),
                LimitsError::CpuExceedsCap { millicores: m }
            );
        }
    }

    #[test]
    fn validate_accepts_cpu_at_cap() {
        // The boundary value — exactly [`LIMITS_CPU_MILLICORES_MAX`]
        // (128 cores = 128_000m) — must validate. The cap is inclusive
        // on the top edge, matching the discipline on every sibling
        // capped axis ([`LIMITS_MEMORY_WASM32_MAX_BYTES`],
        // [`LIMITS_WALL_CLOCK_MAX`], [`crate::POLICY_TIMEOUT_MAX`],
        // [`crate::POLICY_BREAKER_WINDOW_MAX`],
        // [`crate::POLICY_RATE_LIMIT_MAX`]). Pin the boundary
        // explicitly so a future off-by-one tightening
        // (`>= LIMITS_CPU_MILLICORES_MAX` instead of `>`) surfaces here
        // as a test failure rather than a silent contract narrowing.
        let l = LimitsSpec {
            cpu: Some(LIMITS_CPU_MILLICORES_MAX),
            ..Default::default()
        };
        l.validate()
            .expect("cpu == LIMITS_CPU_MILLICORES_MAX must validate");
    }

    #[test]
    fn validate_accepts_cpu_typical_values() {
        // The documented production-playbook band positive-control
        // sweep — every value the canonical caixa Servico runs in
        // (100m..=2000m) must pass, plus a sweep through the larger
        // burstable / multi-component-host band (4000m, 8000m, 16000m,
        // 32000m, 64000m, 128000m) the cap accepts. Mirrors
        // `accepts_wall_clock_typical_values` on the sibling
        // `:wall-clock` axis.
        for m in [
            1_u32,   // smallest non-zero
            100,     // typical small worker
            500,     // canonical test default (peer to limits/flux/helm)
            1_000,   // 1 core, single-threaded wasm32 saturation
            2_000,   // 2 cores
            4_000,   // typical burstable
            8_000,   // upper realistic per-Servico band
            16_000,  // documented heavy-Servico ceiling
            32_000,  // wide-node multi-component-host
            64_000,  // half the cap
            128_000, // exactly at cap
        ] {
            let l = LimitsSpec {
                cpu: Some(m),
                ..Default::default()
            };
            l.validate()
                .unwrap_or_else(|e| panic!("cpu={m}m must validate; got {e:?}"));
        }
    }

    #[test]
    fn cpu_zero_takes_precedence_over_cap() {
        // The cross-arm ordering pin: `Some(0)` is structurally outside
        // both `>= 1` (zero-floor) and `<= LIMITS_CPU_MILLICORES_MAX`
        // (cap), but the zero-floor diagnostic is the more
        // self-locating one (it directly names the omit-axis
        // remediation), so the validate gate must fire on zero first.
        // Same shape every other zero-then-cap ordering on this surface
        // uses (`MemoryZero` then `MemoryExceedsWasm32Cap`,
        // `WallClockZero` then `WallClockExceedsCap`).
        let l = LimitsSpec {
            cpu: Some(0),
            ..Default::default()
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::CpuZero,
            "Some(0) must surface the zero-floor diagnostic, not the cap diagnostic"
        );
    }

    #[test]
    fn validate_rejects_cpu_cap_after_earlier_axes() {
        // Cross-axis ordering: when both an above-cap `:cpu` and an
        // earlier-axis violation are present, the earlier axis must
        // fire first. The validate sequence is :memory → :fuel →
        // :wall-clock → :cpu, so a paired memory-zero + cpu-above-cap
        // input surfaces `MemoryZero`, never the cpu-cap diagnostic.
        // Pins the canonical axis order so a future refactor that
        // reorders the arms surfaces here as a test failure rather
        // than a silent diagnostic regression. Peer of
        // `validate_rejects_first_zero_axis_deterministically` and
        // `validate_rejects_memory_cap_before_other_axes`.
        let l = LimitsSpec {
            memory: Some(0),
            fuel: None,
            wall_clock: None,
            cpu: Some(LIMITS_CPU_MILLICORES_MAX + 1),
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::MemoryZero,
            "earlier-axis violation must take precedence over later-axis cap violation"
        );
    }

    #[test]
    fn cpu_cap_diagnostic_carries_offending_value() {
        // The diagnostic-shape pin: the offending millicore count is
        // carried verbatim into the [`LimitsError::CpuExceedsCap`]
        // variant so the surfaced error message names the value the
        // author wrote, not just the cap. Same self-locating
        // diagnostic shape every other typed-cap arm on this surface
        // carries (`MemoryExceedsWasm32Cap` carries the offending byte
        // count verbatim, `WallClockExceedsCap` carries the offending
        // `Duration` verbatim).
        let m = 256_000_u32; // 256 cores — double the cap
        let l = LimitsSpec {
            cpu: Some(m),
            ..Default::default()
        };
        let err = l.validate().unwrap_err();
        assert!(
            matches!(err, LimitsError::CpuExceedsCap { millicores } if millicores == m),
            "got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("256000"),
            ":limits :cpu cap diagnostic must carry the offending value verbatim (got: {msg})"
        );
    }

    #[test]
    fn cpu_cap_pins_canonical_value() {
        // The [`LIMITS_CPU_MILLICORES_MAX`] constant pins the value at
        // exactly 128 cores (128_000 millicores) — the largest
        // commercially-common non-metal cloud Kubernetes node vCPU
        // count. Pinning the literal value here surfaces a future
        // drift (a relaxation to 256 cores, a tightening to 64 cores)
        // as a deliberate test edit, not a silent contract narrowing.
        // Same shape every other typed-cap value pin uses
        // (`wall_clock_cap_pins_canonical_value`,
        // `wasm32_memory_cap_matches_parsed_4_gib`).
        assert_eq!(LIMITS_CPU_MILLICORES_MAX, 128_000);
        assert_eq!(LIMITS_CPU_MILLICORES_MAX, 128 * 1000);
    }

    #[test]
    fn cpu_cap_value_round_trips_through_codec() {
        // The codec round-trip property the cap arm preserves: the
        // [`LIMITS_CPU_MILLICORES_MAX`] constant itself round-trips
        // through the in-module millicore codec — the cap value
        // renders to a clean canonical string (`"128000m"`) and parses
        // back to the same `u32`. Pin this so a future drift between
        // the cap constant and the codec's accepted magnitude surfaces
        // here. Same shape every other typed boundary pin on this
        // surface uses (`wasm32_memory_cap_matches_parsed_4_gib`,
        // `wall_clock_cap_value_round_trips_through_codec`).
        let l = LimitsSpec {
            cpu: Some(LIMITS_CPU_MILLICORES_MAX),
            ..Default::default()
        };
        let json = serde_json::to_string(&l).unwrap();
        assert!(
            json.contains("\"128000m\""),
            "the LIMITS_CPU_MILLICORES_MAX value must render to the canonical \"128000m\" form (got: {json})"
        );
        let back: LimitsSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cpu, Some(LIMITS_CPU_MILLICORES_MAX));
        l.validate()
            .expect("LIMITS_CPU_MILLICORES_MAX itself must pass validate");
    }
}
