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

/// Upper-bound ceiling on the `:limits :fuel` axis, in wasm
/// instructions per outermost call — every validated
/// [`LimitsSpec::fuel`] past [`LimitsSpec::validate`] lies in
/// `1..=LIMITS_FUEL_MAX` (inclusive on both ends).
///
/// The typed field is `Option<u64>` (the zero-floor arm
/// [`LimitsError::FuelZero`] already rejects `Some(0)` — wasmtime
/// traps the first instruction at `fuel=0`), so a programmatic
/// struct literal (`LimitsSpec { fuel: Some(u64::MAX), .. }` —
/// ≈ 1.8 × 10¹⁹ instructions) and the equivalent author-surface
/// form (`(:limits (:fuel 18446744073709551615))`) both
/// round-trip cleanly through serde — a structurally unbounded
/// `u64` ceiling. The runtime substrate consuming the value
/// ([`crate::render::servico_m2_overlay`]'s
/// `pleme-computeunit.limits.fuel` projection, the M2.5
/// `wasm-engine` `Store::set_fuel` call the
/// `ABSORPTION-ROADMAP` names as the downstream wiring, the
/// future M4 `mesh.pleme.io/v1alpha1/Caixa` CR materializer's
/// admission webhook) lands the value verbatim as the
/// wasmtime store's per-call fuel budget. A value far above any
/// reachable wasm execution count turns the typed slot into a
/// no-op budget: the sibling [`LIMITS_WALL_CLOCK_MAX`] (1h)
/// cap fires before the fuel counter ever drains, the per-call
/// fuel-tracking contract degenerates to "rely on `:wall-clock`
/// instead" enforcement, and the per-process CSE invariant
/// (every typed `:fuel` is a meaningful budget the wasm-engine
/// can actually consume) is a runtime, not build-time, contract
/// on every above-cap input — the canonical declared-but-no-op
/// footgun the sibling `:wall-clock` / `:cpu` / `:memory` cap
/// arms close on the peer "cannot be honored" /
/// "unschedulable hint" / "no-op budget" shapes, and the peer
/// `:politicas :rate-limit` / `:politicas :timeout` /
/// `:politicas :circuit-breaker :window` /
/// `:supervisor :max-restarts` cap arms close on every other
/// `Option<numeric>` axis on the typed Caixa surface.
///
/// The `1_000_000_000_000` (10¹² = 1 trillion wasm instructions)
/// ceiling matches the operational envelope the sibling
/// [`LIMITS_WALL_CLOCK_MAX`] cap pins: at wasmtime's documented
/// fuel-tracked execution rate (~10⁸–10⁹ fuel-units per second
/// on modern x86_64 / aarch64 hosts running wasmtime through
/// Cranelift — the substrate's wasm32-wasip2 default backend per
/// the `caixa-feira` runner), the largest realistic per-call
/// fuel budget reachable within `LIMITS_WALL_CLOCK_MAX` (1h)
/// sits at ~3.6 × 10¹¹–3.6 × 10¹² fuel-units. The 10¹² cap is
/// the round-number ceiling above this operational envelope,
/// sits six orders of magnitude above the canonical fixture
/// (the in-tree `Caixa::template` documentation and
/// `caixa-feira` examples carry `:fuel 1_000_000` = 10⁶,
/// peer to wasmtime's official `Store::set_fuel(1_000_000)`
/// example in the `wasmtime` book), and surfaces every
/// paste-from-binary / overflow / u64-magnitude-typo footgun
/// (`u64::MAX`, `0xFFFF_FFFF_FFFF_FFFF`, large hex literals
/// confused for instruction-count budgets) at validate time.
/// A value above this cap is operationally a no-op fuel
/// counter: the wall-clock deadline ([`LIMITS_WALL_CLOCK_MAX`]
/// = 3600s × ~10⁹ fuel/sec ≈ 3.6 × 10¹² instructions reachable)
/// fires before the fuel counter could ever be drained,
/// so the typed `:fuel` slot becomes a no-op budget far from
/// the source caixa.lisp. The wasm32-wasip2 single-threaded
/// execution model the canonical caixa Servico targets
/// ([`theory/CAIXA-SDLC.md` §V][sdlc-v]) reinforces the
/// structural argument: a single wasm component cannot
/// out-execute its host's CPU clock, so even the Lunatic-style
/// supervised-multi-process host (`theory/INSPIRATIONS.md`
/// §III.1) bounds its useful fuel-per-call budget to a
/// per-clock-tick magnitude, never higher.
///
/// Lifted as a typed `pub const` (rather than an inline literal
/// at the [`LimitsSpec::validate`] call site) so the bound has
/// exactly one source of truth — the future M4
/// `mesh.pleme.io/v1alpha1/Caixa` CR materializer's per-`:limits
/// :fuel` admission webhook, the caixa-helm `pleme-computeunit`
/// chart's fuel-budget mapping, the M2.5 `wasm-engine` host-
/// runtime `Store::set_fuel` propagation all read from one
/// place. Same shape every other typed upper bound in this
/// crate carries ([`LIMITS_MEMORY_WASM32_MAX_BYTES`],
/// [`LIMITS_WALL_CLOCK_MAX`], [`LIMITS_CPU_MILLICORES_MAX`],
/// [`crate::POLICY_TIMEOUT_MAX`],
/// [`crate::POLICY_BREAKER_WINDOW_MAX`],
/// [`crate::POLICY_RATE_LIMIT_MAX`],
/// [`crate::SUPERVISOR_MAX_RESTARTS_MAX`],
/// [`crate::render::DNS_1123_LABEL_MAX_LEN`]).
///
/// [sdlc-v]: https://github.com/pleme-io/theory/blob/main/CAIXA-SDLC.md
pub const LIMITS_FUEL_MAX: u64 = 1_000_000_000_000;

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
        self.memory().is_none()
            && self.fuel().is_none()
            && self.wall_clock().is_none()
            && self.cpu().is_none()
    }

    /// Substrate-canonical per-`:limits` `:memory` Lunatic-per-process
    /// wasm32-linear-memory byte-cap scalar accessor every consumer of
    /// the Servico's `wasmtime::StoreLimits::memory_size` propagation
    /// keys off — returns the author-declared `:limits :memory` typed
    /// byte-cap verbatim as an `Option<u64>`, copied out of the typed
    /// slot's own `Option<u64>` storage (`Option<u64>` is `Copy`, so
    /// the accessor returns by value; no borrow of `&self` past the
    /// call). `None` when the slot is absent (the "no memory cap
    /// declared — engine-default applies, today the pre-M2 unbounded-
    /// linear-memory shape" arm the module-level docstring names on
    /// [`LimitsSpec::memory`] itself — [`LimitsSpec::is_empty`]'s
    /// `memory().is_none()` arm reads this predicate too, so an
    /// authored-but-unset `:limits (:memory ())` round-trips to a
    /// `servico_m2_overlay` emission structurally identical to one
    /// that omits the slot entirely).
    ///
    /// The `:limits :memory` slot carries the "per-process wasm32
    /// linear-memory byte-cap" Lunatic-shaped sandboxing contract
    /// (`theory/INSPIRATIONS.md` §III.1) — the typed slot's
    /// `Option<u64>` accept-set (zero-floor rejected through
    /// [`LimitsError::MemoryZero`], wasm32-page-floor rejected through
    /// [`LimitsError::MemoryBelowWasm32Page`], upper-bounded by
    /// [`LIMITS_MEMORY_WASM32_MAX_BYTES`], authored as a byte-size
    /// string that round-trips back to the canonical form through
    /// [`ser_byte_size`] / [`de_byte_size`]) maps onto the wasmtime
    /// `Store::limiter`-side `memory_size` projection the wasm-engine
    /// M2 wires and, via [`crate::render::servico_m2_overlay`], onto
    /// the `pleme-computeunit` Helm-library-chart values sub-block's
    /// `limits.memory` key that lands as the ComputeUnit CR's
    /// `spec.limits.memory` field.
    ///
    /// Prior to this lift the `.memory` field was accessed inline at
    /// four sites inside `impl LimitsSpec` — [`LimitsSpec::is_empty`]'s
    /// `self.memory.is_none()` arm and three [`LimitsSpec::validate`]
    /// arms (the numeric zero-floor arm at line 397, the wasm32-page
    /// structural floor arm at line 427, and the wasm32 upper-cap
    /// arm at line 449) — four open-coded field-accesses that
    /// expressed no compile-time link back to the typed slot. A
    /// future extension of the `:limits :memory` axis to a richer
    /// author surface — a per-instance memory-declaration override
    /// the operator pins through a future ComputeUnit CR-side
    /// `spec.limits.memory` overlay, a split of the single `u64`
    /// byte-cap into a `{min, max}` pair once wasm32's `(memory M N)`
    /// two-arg form promotes past its current single-`max` typed
    /// bound, a wasm64 promotion once the wasm-engine grows past the
    /// wasm32 4 GiB structural ceiling — would have had to be
    /// threaded through every open-coded copy in lockstep or the
    /// emptiness predicate and the validate call would silently
    /// disagree on which cap a given [`LimitsSpec`] resolves to.
    /// Lifting the resolution to a typed method on the substrate
    /// primitive means every downstream consumer of the Servico's
    /// per-`:limits` byte-cap surface reaches for exactly one typed
    /// dispatch — the resolver's accept-set migrates as a unit on any
    /// future axis addition.
    ///
    /// First `Option<Copy-T>`-return accessor on the M2 slot family
    /// (peer of the sibling per-`:politicas` [`crate::MeshPolicy::mtls_required`]
    /// c0110f1 `Option<bool>` accessor, per-`:politicas`
    /// [`crate::MeshPolicy::retries`] bdfb399 `Option<u32>` accessor,
    /// and per-`:politicas` [`crate::MeshPolicy::timeout`] 7073d0f
    /// `Option<Duration>` accessor on the M3 mesh-slot family — same
    /// "one typed dispatch on the substrate primitive, thin
    /// projections at each consumer" discipline extended onto the
    /// peer per-`:limits` typed-`u64` optional-scalar axis; opens the
    /// "optional per-slot Copy-T scalar" projection pattern the
    /// sibling per-`:limits` `:fuel` (Option<u64>) / `:wall-clock`
    /// (Option<Duration>) / `:cpu` (Option<u32>) future lifts fold
    /// on). Named `memory()` to match the storage field's name; the
    /// accessor's identity maps onto the canonical Lunatic-shaped
    /// `theory/INSPIRATIONS.md` §III.1 vocabulary the slot's docstring
    /// already carries.
    #[must_use]
    pub const fn memory(&self) -> Option<u64> {
        self.memory
    }

    /// Substrate-canonical per-`:limits` `:fuel` wasmtime-per-call
    /// wasm-instruction budget scalar accessor every consumer of the
    /// Servico's `wasmtime::Store::set_fuel` propagation keys off —
    /// returns the author-declared `:limits :fuel` typed
    /// wasm-instruction budget verbatim as an `Option<u64>`, copied
    /// out of the typed slot's own `Option<u64>` storage
    /// (`Option<u64>` is `Copy`, so the accessor returns by value; no
    /// borrow of `&self` past the call). `None` when the slot is
    /// absent (the "no fuel budget declared — engine-default applies,
    /// today the pre-M2 unbounded-fuel-counter shape" arm the
    /// module-level docstring names on [`LimitsSpec::fuel`] itself —
    /// [`LimitsSpec::is_empty`]'s `fuel().is_none()` arm reads this
    /// predicate too, so an authored-but-unset `:limits (:fuel ())`
    /// round-trips to a `servico_m2_overlay` emission structurally
    /// identical to one that omits the slot entirely).
    ///
    /// The `:limits :fuel` slot carries the "per-call wasm-instruction
    /// budget" wasmtime-shaped sandboxing contract
    /// (`theory/INSPIRATIONS.md` §III.1 — Lunatic's supervised
    /// wasm-`Store`-per-process fuel accounting, translated onto
    /// pleme-io's typed `:limits` slot) — the typed slot's
    /// `Option<u64>` accept-set (zero-floor rejected through
    /// [`LimitsError::FuelZero`] because wasmtime traps the first
    /// instruction at `fuel=0`, upper-bounded by [`LIMITS_FUEL_MAX`]
    /// (10¹² wasm instructions — the operationally-reachable
    /// per-call budget within the sibling [`LIMITS_WALL_CLOCK_MAX`]
    /// 1h ceiling)) maps onto the wasmtime `Store::set_fuel` call
    /// the M2.5 wasm-engine wires per outermost call and, via
    /// [`crate::render::servico_m2_overlay`], onto the
    /// `pleme-computeunit` Helm-library-chart values sub-block's
    /// `limits.fuel` key that lands as the `ComputeUnit` CR's
    /// `spec.limits.fuel` field.
    ///
    /// Prior to this lift the `.fuel` field was accessed inline at
    /// two sites inside `impl LimitsSpec` — [`LimitsSpec::is_empty`]'s
    /// `self.fuel.is_none()` arm and [`LimitsSpec::validate`]'s
    /// `if let Some(f) = self.fuel { … }` zero-floor + upper-cap
    /// bracket arm — two open-coded field-accesses that expressed no
    /// compile-time link back to the typed slot. A future extension
    /// of the `:limits :fuel` axis to a richer author surface — a
    /// per-instance `ComputeUnit` CR-side `spec.limits.fuel` overlay
    /// the operator pins per-cluster, a wasm-instruction-count →
    /// wasmtime-fuel-unit rescale once the fuel-tracking backend
    /// switches from Cranelift's implicit 1:1 count to a
    /// per-opcode-weighted budget, a split of the single
    /// per-outermost-call `u64` budget into a `{per_call, per_second}`
    /// pair once the wasm-engine grows a sustained-throughput cap —
    /// would have had to be threaded through every open-coded copy in
    /// lockstep or the emptiness predicate and the validate call
    /// would silently disagree on which fuel budget a given
    /// [`LimitsSpec`] resolves to. Lifting the resolution to a typed
    /// method on the substrate primitive means every downstream
    /// consumer of the Servico's per-`:limits` fuel-budget surface
    /// reaches for exactly one typed dispatch — the resolver's
    /// accept-set migrates as a unit on any future axis addition.
    ///
    /// Second `Option<Copy-T>`-return accessor on the M2 slot family
    /// (peer of the sibling per-`:limits` [`LimitsSpec::memory`]
    /// (620c067) `Option<u64>` accessor — same typed-`u64`
    /// optional-scalar shape, extended to the peer per-`:limits`
    /// wasm-instruction-budget axis; sibling to
    /// [`crate::MeshPolicy::mtls_required`] (c0110f1) / [`crate::MeshPolicy::retries`]
    /// (bdfb399) / [`crate::MeshPolicy::timeout`] (7073d0f) on the
    /// closed M3 mesh-slot `Option<Copy-T>` accessor family). The
    /// pair `(memory(), fuel())` jointly projects the two `Option<u64>`
    /// axes every M2 `:limits` consumer that fans on
    /// wasm-linear-memory-cap + wasm-fuel-budget keys off. Two of the
    /// four `:limits` axes now route through a typed dispatch on the
    /// substrate primitive; the two remaining (`wall_clock:
    /// Option<Duration>`, `cpu: Option<u32>`) fold on the same
    /// one-line accessor + is_empty-arm-route + validate-arm-route +
    /// three-test pattern. Named `fuel()` to match the storage field's
    /// name; the accessor's identity maps onto the canonical
    /// wasmtime-`Store::set_fuel`-shaped vocabulary the slot's
    /// docstring already carries.
    #[must_use]
    pub const fn fuel(&self) -> Option<u64> {
        self.fuel
    }

    /// Substrate-canonical per-`:limits` `:wall-clock` wasmtime-per-call
    /// wall-clock deadline scalar accessor every consumer of the
    /// Servico's `wasmtime::Store::epoch_deadline_*` / `wasi:clocks`
    /// propagation keys off — returns the author-declared `:limits
    /// :wall-clock` typed `Duration` verbatim as an `Option<Duration>`,
    /// copied out of the typed slot's own `Option<Duration>` storage
    /// (`Duration` is `Copy`, so `Option<Duration>` is `Copy` and the
    /// accessor returns by value; no borrow of `&self` past the call).
    /// `None` when the slot is absent (the "no wall-clock deadline
    /// declared — engine-default applies, today the pre-M2
    /// unbounded-wall-clock shape" arm the module-level docstring names
    /// on [`LimitsSpec::wall_clock`] itself — [`LimitsSpec::is_empty`]'s
    /// `wall_clock().is_none()` arm reads this predicate too, so an
    /// authored-but-unset `:limits (:wall-clock ())` round-trips to a
    /// `servico_m2_overlay` emission structurally identical to one that
    /// omits the slot entirely).
    ///
    /// The `:limits :wall-clock` slot carries the "per-outermost-call
    /// wall-clock deadline" wasmtime-shaped sandboxing contract
    /// (`theory/INSPIRATIONS.md` §III.1 — Lunatic's supervised
    /// wasm-`Store`-per-process epoch-deadline accounting, translated
    /// onto pleme-io's typed `:limits` slot) — the typed slot's
    /// `Option<Duration>` accept-set (zero-floor rejected through
    /// [`LimitsError::WallClockZero`] because a zero deadline traps the
    /// first instruction; integer-millisecond granularity enforced
    /// through [`LimitsError::WallClockNotCanonical`] because the
    /// duration codec's canonical form emits `"1500ms"` not `"1.5s"`
    /// and the operator's wall-clock scheduler quantizes at
    /// milliseconds; upper-bounded by [`LIMITS_WALL_CLOCK_MAX`] (1h —
    /// the coarsest per-call deadline any operationally-reachable
    /// Servico can honor without spanning multiple scheduler epochs))
    /// maps onto the wasmtime `Store::epoch_deadline_*` call the M2.5
    /// wasm-engine wires per outermost call and, via
    /// [`crate::render::servico_m2_overlay`], onto the
    /// `pleme-computeunit` Helm-library-chart values sub-block's
    /// `limits.wallClock` key that lands as the `ComputeUnit` CR's
    /// `spec.limits.wallClock` field.
    ///
    /// Prior to this lift the `.wall_clock` field was accessed inline at
    /// two sites inside `impl LimitsSpec` — [`LimitsSpec::is_empty`]'s
    /// `self.wall_clock.is_none()` arm and [`LimitsSpec::validate`]'s
    /// `if let Some(w) = self.wall_clock { … }` zero-floor +
    /// canonical-form + upper-cap bracket arm — two open-coded
    /// field-accesses that expressed no compile-time link back to the
    /// typed slot. A future extension of the `:limits :wall-clock` axis
    /// to a richer author surface — a per-instance `ComputeUnit`
    /// CR-side `spec.limits.wallClock` overlay the operator pins
    /// per-cluster, a wall-clock-vs-monotonic-clock discriminator once
    /// the wasm-engine grows a `:limits (:wall-clock (:kind monotonic
    /// …))` axis, a split of the single per-outermost-call `Duration`
    /// budget into a `{deadline, warn_at}` pair once the wasm-engine
    /// grows a soft-deadline warning surface — would have had to be
    /// threaded through every open-coded copy in lockstep or the
    /// emptiness predicate and the validate call would silently
    /// disagree on which deadline a given [`LimitsSpec`] resolves to.
    /// Lifting the resolution to a typed method on the substrate
    /// primitive means every downstream consumer of the Servico's
    /// per-`:limits` wall-clock-deadline surface reaches for exactly
    /// one typed dispatch — the resolver's accept-set migrates as a
    /// unit on any future axis addition.
    ///
    /// Third `Option<Copy-T>`-return accessor on the M2 slot family
    /// (peer of the sibling per-`:limits` [`LimitsSpec::memory`]
    /// (620c067) `Option<u64>` accessor and per-`:limits`
    /// [`LimitsSpec::fuel`] (795dee7) `Option<u64>` accessor — same
    /// typed-optional-scalar shape extended to the peer per-`:limits`
    /// wall-clock-deadline axis; sibling to [`crate::MeshPolicy::timeout`]
    /// (7073d0f) on the closed M3 mesh-slot `Option<Duration>` accessor
    /// axis — same typed-`Duration` shape extended from the M3
    /// per-call-timeout to the M2 per-outermost-call deadline). The
    /// triple `(memory(), fuel(), wall_clock())` jointly projects three
    /// of the four `Option<Copy-T>` axes every M2 `:limits` consumer
    /// that fans on wasm-linear-memory-cap + wasm-fuel-budget +
    /// wall-clock-deadline keys off. Three of the four `:limits` axes
    /// now route through a typed dispatch on the substrate primitive;
    /// the one remaining (`cpu: Option<u32>`) folds on the same
    /// one-line accessor + is_empty-arm-route + validate-arm-route +
    /// three-test pattern in the next run, closing the M2 `:limits`
    /// slot family's `Option<Copy-T>` accessor axis. Named `wall_clock()`
    /// to match the storage field's name; the accessor's identity maps
    /// onto the canonical wasmtime-`Store::epoch_deadline_*`-shaped
    /// vocabulary the slot's docstring already carries.
    #[must_use]
    pub const fn wall_clock(&self) -> Option<Duration> {
        self.wall_clock
    }

    /// Substrate-canonical per-`:limits` `:cpu` Kubernetes-millicore
    /// soft cgroup-share scalar accessor every consumer of the Servico's
    /// pod-spec `resources.requests.cpu` propagation keys off — returns
    /// the author-declared `:limits :cpu` typed millicore magnitude
    /// verbatim as an `Option<u32>`, copied out of the typed slot's own
    /// `Option<u32>` storage (`Option<u32>` is `Copy`, so the accessor
    /// returns by value; no borrow of `&self` past the call). `None`
    /// when the slot is absent (the "no cpu share declared —
    /// scheduler-default applies, today the pre-M2 unbounded-cpu-share
    /// shape" arm the module-level docstring names on
    /// [`LimitsSpec::cpu`] itself — [`LimitsSpec::is_empty`]'s
    /// `cpu().is_none()` arm reads this predicate too, so an
    /// authored-but-unset `:limits (:cpu ())` round-trips to a
    /// `servico_m2_overlay` emission structurally identical to one that
    /// omits the slot entirely).
    ///
    /// The `:limits :cpu` slot carries the "per-process soft cgroup-v2
    /// CPU share" Kubernetes-scheduler-shaped sandboxing hint
    /// (`theory/INSPIRATIONS.md` §III.1 — Lunatic's supervised
    /// wasm-`Store`-per-process host-runtime CPU accounting, translated
    /// onto pleme-io's typed `:limits` slot as a scheduler-facing
    /// millicore request the pod's kubelet propagates to the container's
    /// cgroup) — the typed slot's `Option<u32>` accept-set (zero-floor
    /// rejected through [`LimitsError::CpuZero`] because a zero cgroup
    /// share starves the process; upper-bounded by
    /// [`LIMITS_CPU_MILLICORES_MAX`] (128 cores — the largest commercially-
    /// common non-metal cloud Kubernetes node vCPU count on managed GKE
    /// / EKS / AKS general-purpose SKUs)) maps onto the K8s pod spec's
    /// `spec.containers[].resources.requests.cpu` field the
    /// M2.5 `wasm-engine` host-runtime lands on the `ComputeUnit` CR-side
    /// pod template and, via [`crate::render::servico_m2_overlay`], onto
    /// the `pleme-computeunit` Helm-library-chart values sub-block's
    /// `limits.cpu` key that lands as the `ComputeUnit` CR's
    /// `spec.limits.cpu` field.
    ///
    /// Prior to this lift the `.cpu` field was accessed inline at two
    /// sites inside `impl LimitsSpec` — [`LimitsSpec::is_empty`]'s
    /// `self.cpu.is_none()` arm and [`LimitsSpec::validate`]'s
    /// `if let Some(m) = self.cpu { … }` zero-floor + upper-cap bracket
    /// arm — two open-coded field-accesses that expressed no
    /// compile-time link back to the typed slot. A future extension of
    /// the `:limits :cpu` axis to a richer author surface — a
    /// per-instance `ComputeUnit` CR-side `spec.limits.cpu` overlay the
    /// operator pins per-cluster, a split of the single `u32` millicore
    /// request into a `{request, limit}` pair once the pod spec's
    /// `resources.requests.cpu` / `resources.limits.cpu` distinction
    /// promotes past its current single-request author surface, a
    /// millicore → cgroup-v2 `cpu.weight` rescale once the operator's
    /// scheduler-facing translation lands past its current kubelet
    /// passthrough — would have had to be threaded through every
    /// open-coded copy in lockstep or the emptiness predicate and the
    /// validate call would silently disagree on which cgroup share a
    /// given [`LimitsSpec`] resolves to. Lifting the resolution to a
    /// typed method on the substrate primitive means every downstream
    /// consumer of the Servico's per-`:limits` cpu-share surface reaches
    /// for exactly one typed dispatch — the resolver's accept-set
    /// migrates as a unit on any future axis addition.
    ///
    /// Fourth and final `Option<Copy-T>`-return accessor on the M2 slot
    /// family (peer of the sibling per-`:limits` [`LimitsSpec::memory`]
    /// (620c067) `Option<u64>` accessor, per-`:limits`
    /// [`LimitsSpec::fuel`] (795dee7) `Option<u64>` accessor, and
    /// per-`:limits` [`LimitsSpec::wall_clock`] (8cb717b)
    /// `Option<Duration>` accessor — same typed-optional-scalar shape
    /// extended to the peer per-`:limits` cgroup-cpu-share axis; sibling
    /// to [`crate::MeshPolicy::mtls_required`] (c0110f1) /
    /// [`crate::MeshPolicy::retries`] (bdfb399) /
    /// [`crate::MeshPolicy::timeout`] (7073d0f) on the closed M3
    /// mesh-slot `Option<Copy-T>` accessor family). The four-tuple
    /// `(memory(), fuel(), wall_clock(), cpu())` jointly projects every
    /// `Option<Copy-T>` axis on the M2 `:limits` slot every consumer
    /// that fans on wasm-linear-memory-cap + wasm-fuel-budget +
    /// wall-clock-deadline + cgroup-cpu-share keys off — closes the M2
    /// `:limits` slot family's `Option<Copy-T>` accessor axis (the
    /// last unlifted `:limits` field-access site on the M2 slot family;
    /// every axis now routes through a typed dispatch on the substrate
    /// primitive, with no open-coded field access anywhere on the impl).
    /// Named `cpu()` to match the storage field's name; the accessor's
    /// identity maps onto the canonical Kubernetes-`resources.requests.cpu`-
    /// shaped vocabulary the slot's docstring already carries.
    #[must_use]
    pub const fn cpu(&self) -> Option<u32> {
        self.cpu
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
        // Route the `:memory` axis's four value-shape gates
        // (zero-floor → wasm32-page-floor → wasm32-address-cap →
        // page-multiple) through the substrate helper
        // [`crate::render::require_positive_quantum_multiple_bounded_u64`]
        // rather than four sequential inline
        // `if let Some(m) = self.memory()` guards each restating one
        // arm. Brings the `:memory` axis onto the same "one substrate
        // helper per typed axis" discipline the peer `:fuel` (routed
        // through [`crate::render::require_positive_bounded_u64`]),
        // `:wall-clock` (through
        // [`crate::render::require_positive_canonical_bounded_duration`]),
        // and `:cpu` (through
        // [`crate::render::require_positive_bounded_u32`]) axes
        // already carry — every `LimitsSpec::validate` axis is now
        // exactly one typed-helper dispatch, with the four-arm
        // ordering (zero → below-quantum → cap → not-multiple)
        // promoted from a per-site convention four inline blocks
        // re-derived by hand to a structural contract on the
        // substrate primitive. Byte-equal today: the helper fires the
        // same four arms in the same canonical order at the same
        // boundary values, threading the offending byte count into
        // the same `MemoryBelowWasm32Page` / `MemoryExceedsWasm32Cap`
        // / `MemoryNotPageMultiple` discriminator fields the four
        // pre-lift inline arms already carried, so every existing
        // per-arm test in this module continues to pin the same
        // shape unchanged. Pinned end-to-end by
        // `validate_memory_axis_routes_through_quantum_multiple_bounded_helper`.
        if let Some(m) = self.memory() {
            crate::render::require_positive_quantum_multiple_bounded_u64(
                m,
                LIMITS_MEMORY_WASM32_PAGE_BYTES,
                LIMITS_MEMORY_WASM32_MAX_BYTES,
                || LimitsError::MemoryZero,
                |bytes| LimitsError::MemoryBelowWasm32Page { bytes },
                |bytes| LimitsError::MemoryExceedsWasm32Cap { bytes },
                |bytes| LimitsError::MemoryNotPageMultiple { bytes },
            )?;
        }
        // Zero-floor + upper-cap bracket on the typed `:fuel` axis. See
        // [`crate::render::require_positive_bounded_u64`] for the
        // ordering discipline (zero-floor arm strictly precedes cap arm
        // so `Some(0)` surfaces the self-locating `FuelZero` diagnostic
        // with its omit-axis remediation directly named, not the
        // misleading `0 > LIMITS_FUEL_MAX == false` cap-arm miss).
        // Until this bracket landed the `Option<u64>` slot accepted any
        // value past zero (the parser's only upper bound was `u64::MAX`),
        // so `(:fuel 18446744073709551615)` round-tripped cleanly
        // through serde and the per-process CSE invariant (no value the
        // wasm-engine's fuel counter can't honor as a meaningful budget
        // before the sibling `:wall-clock` deadline fires) was a
        // runtime, not build-time, contract on every above-cap input
        // — the canonical declared-but-no-op footgun the sibling
        // [`LimitsError::MemoryExceedsWasm32Cap`] /
        // [`LimitsError::WallClockExceedsCap`] /
        // [`LimitsError::CpuExceedsCap`] arms close on the peer
        // "cannot be honored" / "unschedulable hint" /
        // "nominal-only deadline" shapes, the peer
        // [`crate::AplicacaoError::PolicyTimeoutExceedsCap`] /
        // [`crate::AplicacaoError::PolicyBreakerWindowExceedsCap`] /
        // [`crate::AplicacaoError::PolicyRateLimitExceedsCap`] arms
        // close on the no-op-deadline / lifetime-counter / no-op-limiter
        // shapes, and the
        // [`crate::SupervisorError::MaxRestartsExceedsCap`] arm closes
        // on the no-op-supervisor shape. The four `:limits` axes are
        // now uniformly bracketed top and bottom (`:memory` in
        // `LIMITS_MEMORY_WASM32_PAGE_BYTES..=LIMITS_MEMORY_WASM32_MAX_BYTES`,
        // `:fuel` in `1..=LIMITS_FUEL_MAX`, `:wall-clock` in
        // `1ms..=LIMITS_WALL_CLOCK_MAX`, `:cpu` in
        // `1..=LIMITS_CPU_MILLICORES_MAX`).
        if let Some(f) = self.fuel() {
            crate::render::require_positive_bounded_u64(
                f,
                LIMITS_FUEL_MAX,
                || LimitsError::FuelZero,
                |fuel| LimitsError::FuelExceedsCap { fuel },
            )?;
        }
        if let Some(w) = self.wall_clock() {
            // Zero-floor + integer-millisecond canonical-form +
            // upper-cap bracket on the typed `:wall-clock` axis. See
            // [`crate::render::require_positive_canonical_bounded_duration`]
            // for the full three-arm ordering discipline (zero-floor
            // strictly precedes canonical-form so `Duration::ZERO`
            // surfaces the self-locating `WallClockZero` diagnostic;
            // canonical-form strictly precedes the cap arm so a
            // sub-millisecond above-cap value surfaces the more
            // fundamental round-trip-shape diagnostic first) and the
            // three peer typed-`Duration` sites that share this
            // canonical bracket ([`crate::MeshPolicy::timeout`],
            // [`crate::CircuitBreaker::window`],
            // [`crate::SupervisorSpec::restart_window`]). Every
            // validated value lies in `1ms..=LIMITS_WALL_CLOCK_MAX`
            // (1ms..=1h), integer-millisecond granularity.
            crate::render::require_positive_canonical_bounded_duration(
                w,
                LIMITS_WALL_CLOCK_MAX,
                || LimitsError::WallClockZero,
                |wall_clock| LimitsError::WallClockNotCanonical { wall_clock },
                |wall_clock| LimitsError::WallClockExceedsCap { wall_clock },
            )?;
        }
        // Zero-floor + upper-cap bracket on the typed `:cpu` axis. See
        // [`crate::render::require_positive_bounded_u32`] for the
        // ordering discipline (zero-floor arm strictly precedes cap arm
        // so `Some(0)` surfaces the self-locating `CpuZero` diagnostic
        // with its omit-axis remediation directly named, not the
        // misleading `0 > LIMITS_CPU_MILLICORES_MAX == false` cap-arm
        // miss). The bracket set is `1..=LIMITS_CPU_MILLICORES_MAX`
        // (128 cores = 128_000 millicores — the largest commercially-
        // common non-metal cloud Kubernetes node vCPU count). Until
        // this bracket landed the millicore codec accepted any
        // `Option<u32>` past zero (the prior numeric-zero arm's only
        // floor), so `(:cpu "1000000m")` (1000 cores) round-tripped
        // cleanly through serde and the per-axis CSE invariant (no
        // value the Kubernetes scheduler can't honor) was a runtime,
        // not build-time, contract on every above-cap input: the
        // `pleme-computeunit` chart's `resources.requests.cpu` landed
        // verbatim, the pod sat `Pending` indefinitely with a `0/N
        // nodes are available: N Insufficient cpu` event, and the
        // typed `:cpu` slot became an unschedulable hint far from the
        // source caixa.lisp. Closes the same gap the wasm32-wasip2
        // upper ceiling closes on the `:memory` axis — the typed `:cpu`
        // axis is now operationally bracketed. Peer with every sibling
        // cap arm on this surface ([`LimitsError::MemoryExceedsWasm32Cap`],
        // [`LimitsError::WallClockExceedsCap`],
        // [`crate::AplicacaoError::PolicyTimeoutExceedsCap`],
        // [`crate::AplicacaoError::PolicyRetriesExceedsCap`],
        // [`crate::AplicacaoError::PolicyBreakerMaxFailuresExceedsCap`],
        // [`crate::AplicacaoError::PolicyBreakerWindowExceedsCap`],
        // [`crate::AplicacaoError::PolicyRateLimitExceedsCap`],
        // [`crate::SupervisorError::MaxRestartsExceedsCap`]).
        if let Some(m) = self.cpu() {
            crate::render::require_positive_bounded_u32(
                m,
                LIMITS_CPU_MILLICORES_MAX,
                || LimitsError::CpuZero,
                |millicores| LimitsError::CpuExceedsCap { millicores },
            )?;
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
    #[error(
        "byte-size: magnitude {value:?} has a non-canonical leading zero — the canonical \
         authoring form for `:limits :memory` is `<integer><unit>` (e.g. `\"64MiB\"`, \
         `\"1GiB\"`, `\"512KiB\"`, `\"1024\"`) with no leading-zero padding on the magnitude. \
         A leading-zero magnitude (`\"064MiB\"`, `\"01024\"`, `\"00KiB\"`, `\"0500MB\"`) round-trips \
         through `render_byte_size` to a *different* canonical form (`\"64MiB\"`, `\"1KiB\"`, \
         `\"0\"`, `\"500MB\"`) on first serialize — breaking the THEORY.md Part V \
         render-determinism contract every typed slot carries. Strip the leading zeros \
         (write `\"64MiB\"` instead of `\"064MiB\"`)"
    )]
    LeadingZeroByteMagnitude { value: String },
    #[error(
        "byte-size: value {value:?} contains whitespace byte 0x{byte:02x} — the canonical \
         authoring form for `:limits :memory` is `<integer><unit>` (e.g. `\"64MiB\"`, \
         `\"1GiB\"`, `\"512KiB\"`, `\"1024\"`) with no whitespace bytes anywhere. A \
         whitespace-carrying shape (`\" 64MiB\"`, `\"64MiB \"`, `\"64 MiB\"`, `\"\\t64MiB\"`, \
         `\"64MiB\\n\"`) round-trips through `render_byte_size` to a *different* canonical \
         form (`\"64MiB\"`) on first serialize — breaking the THEORY.md Part V \
         render-determinism contract every typed slot carries. Strip every whitespace byte \
         (write `\"64MiB\"` verbatim)"
    )]
    WhitespaceInByteSize { value: String, byte: u8 },
    #[error(
        "byte-size: value {value:?} contains a non-ASCII Unicode whitespace character \
         {ch:?} (U+{codepoint:04X}) — the canonical authoring form for `:limits :memory` \
         is `<integer><unit>` (e.g. `\"64MiB\"`, `\"1GiB\"`, `\"512KiB\"`, `\"1024\"`) \
         with no whitespace characters anywhere (ASCII or Unicode). A non-ASCII-whitespace-\
         carrying shape (`\"\\u{{00A0}}64MiB\"` — paste-from-typography NBSP prefix; \
         `\"64MiB\\u{{2028}}\"` — paste-from-web-doc line-separator suffix; \
         `\"64\\u{{2003}}MiB\"` — paste-from-typography EM-SPACE between magnitude and \
         unit) survives the pre-existing `u8::is_ascii_whitespace` byte-scan (none of \
         its bytes match the ASCII whitespace set) but `str::trim` (which uses \
         `char::is_whitespace` — the Unicode `White_Space` property, strictly wider than \
         the ASCII byte set) silently strips it at parse entry, and the value round-trips \
         through `render_byte_size` to a *different* canonical form (`\"64MiB\"`) on \
         first serialize — breaking the THEORY.md Part V render-determinism contract \
         every typed slot carries. Strip every non-ASCII whitespace character (write \
         `\"64MiB\"` verbatim with only ASCII bytes)"
    )]
    NonAsciiWhitespaceInByteSize {
        value: String,
        ch: char,
        codepoint: u32,
    },
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
    #[error(
        "duration: magnitude {value:?} has a non-canonical leading zero — the canonical \
         authoring form for `:limits :wall-clock` is `<integer><unit>` (e.g. `\"30s\"`, \
         `\"500ms\"`, `\"2m\"`, `\"1h\"`) with no leading-zero padding on the magnitude. \
         A leading-zero magnitude (`\"030s\"`, `\"00s\"`, `\"01h\"`, `\"0500ms\"`) round-trips \
         through `render_duration` to a *different* canonical form (`\"30s\"`, `\"0s\"`, \
         `\"1h\"`, `\"500ms\"`) on first serialize — breaking the THEORY.md Part V \
         render-determinism contract every typed slot carries. Strip the leading zeros \
         (write `\"30s\"` instead of `\"030s\"`)"
    )]
    LeadingZeroDurationMagnitude { value: String },
    #[error(
        "duration: value {value:?} contains whitespace byte 0x{byte:02x} — the canonical \
         authoring form for `:limits :wall-clock` is `<integer><unit>` (e.g. `\"30s\"`, \
         `\"500ms\"`, `\"2m\"`, `\"1h\"`) with no whitespace bytes anywhere. A \
         whitespace-carrying shape (`\" 30s\"`, `\"30s \"`, `\"30 s\"`, `\"\\t30s\"`, \
         `\"30s\\n\"`) round-trips through `render_duration` to a *different* canonical form \
         (`\"30s\"`) on first serialize — breaking the THEORY.md Part V render-determinism \
         contract every typed slot carries. Strip every whitespace byte (write `\"30s\"` \
         verbatim)"
    )]
    WhitespaceInDuration { value: String, byte: u8 },
    #[error(
        "duration: value {value:?} contains a non-ASCII Unicode whitespace character \
         {ch:?} (U+{codepoint:04X}) — the canonical authoring form for `:limits :wall-clock` \
         is `<integer><unit>` (e.g. `\"30s\"`, `\"500ms\"`, `\"2m\"`, `\"1h\"`) with no \
         whitespace characters anywhere (ASCII or Unicode). A non-ASCII-whitespace-\
         carrying shape (`\"\\u{{00A0}}30s\"` — paste-from-typography NBSP prefix; \
         `\"30s\\u{{2028}}\"` — paste-from-web-doc line-separator suffix; \
         `\"30\\u{{2003}}s\"` — paste-from-typography EM-SPACE between magnitude and \
         unit) survives the pre-existing `u8::is_ascii_whitespace` byte-scan (none of \
         its bytes match the ASCII whitespace set) but `str::trim` (which uses \
         `char::is_whitespace` — the Unicode `White_Space` property, strictly wider than \
         the ASCII byte set) silently strips it at parse entry, and the value round-trips \
         through `render_duration` to a *different* canonical form (`\"30s\"`) on first \
         serialize — breaking the THEORY.md Part V render-determinism contract every \
         typed slot carries. Strip every non-ASCII whitespace character (write `\"30s\"` \
         verbatim with only ASCII bytes)"
    )]
    NonAsciiWhitespaceInDuration {
        value: String,
        ch: char,
        codepoint: u32,
    },
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
        "millicores: magnitude {value:?} has a non-canonical leading zero — the canonical \
         authoring form for `:limits :cpu` is `<integer>m` (Kubernetes millicores, e.g. \
         `\"500m\"` for half a core, `\"2000m\"` for two cores) or the bare-core shorthand \
         `<integer>` (e.g. `\"2\"` = `\"2000m\"`) with no leading-zero padding on the \
         magnitude. A leading-zero magnitude (`\"0500m\"`, `\"00m\"`, `\"02\"`, `\"01500m\"`) \
         round-trips through `render_millicores` to a *different* canonical form (`\"500m\"`, \
         `\"0m\"`, `\"2000m\"`, `\"1500m\"`) on first serialize — breaking the THEORY.md Part \
         V render-determinism contract every typed slot carries. Strip the leading zeros \
         (write `\"500m\"` instead of `\"0500m\"`; `\"2\"` instead of `\"02\"`)"
    )]
    LeadingZeroMillicoreMagnitude { value: String },
    #[error(
        "millicores: value {value:?} contains whitespace byte 0x{byte:02x} — the canonical \
         authoring form for `:limits :cpu` is `<integer>m` (Kubernetes millicores, e.g. \
         `\"500m\"`, `\"2000m\"`) or the bare-core shorthand `<integer>` (e.g. `\"2\"`) \
         with no whitespace bytes anywhere. A whitespace-carrying shape (`\" 500m\"`, \
         `\"500m \"`, `\"500 m\"`, `\"\\t500m\"`, `\"500m\\n\"`) round-trips through \
         `render_millicores` to a *different* canonical form (`\"500m\"`) on first \
         serialize — breaking the THEORY.md Part V render-determinism contract every \
         typed slot carries. Strip every whitespace byte (write `\"500m\"` verbatim)"
    )]
    WhitespaceInMillicores { value: String, byte: u8 },
    #[error(
        "millicores: value {value:?} contains a non-ASCII Unicode whitespace character \
         {ch:?} (U+{codepoint:04X}) — the canonical authoring form for `:limits :cpu` is \
         `<integer>m` (Kubernetes millicores, e.g. `\"500m\"`, `\"2000m\"`) or the \
         bare-core shorthand `<integer>` (e.g. `\"2\"`) with no whitespace characters \
         anywhere (ASCII or Unicode). A non-ASCII-whitespace-carrying shape \
         (`\"\\u{{00A0}}500m\"` — paste-from-typography NBSP prefix; \
         `\"500m\\u{{2028}}\"` — paste-from-web-doc line-separator suffix; \
         `\"500\\u{{2003}}m\"` — paste-from-typography EM-SPACE between magnitude and \
         unit) survives the pre-existing `u8::is_ascii_whitespace` byte-scan (none of \
         its bytes match the ASCII whitespace set) but `str::trim` (which uses \
         `char::is_whitespace` — the Unicode `White_Space` property, strictly wider than \
         the ASCII byte set) silently strips it at parse entry, and the value round-trips \
         through `render_millicores` to a *different* canonical form (`\"500m\"`) on \
         first serialize — breaking the THEORY.md Part V render-determinism contract \
         every typed slot carries. Strip every non-ASCII whitespace character (write \
         `\"500m\"` verbatim with only ASCII bytes)"
    )]
    NonAsciiWhitespaceInMillicores {
        value: String,
        ch: char,
        codepoint: u32,
    },
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
        ":limits :memory ({bytes} bytes) carries a sub-page residue the wasm32-wasip2 \
         linear-memory model cannot honor — the wasm spec defines linear memory in \
         fixed 64 KiB pages (LIMITS_MEMORY_WASM32_PAGE_BYTES = 65536 bytes) and \
         wasmtime's StoreLimits::memory_size is consumed as a page-quantized ceiling: \
         the engine can grow at most floor({bytes} / 65536) pages, and the bytes in \
         [floor({bytes} / 65536) * 65536, {bytes}] are structural dead space the \
         runtime cannot honor. Pin a page-aligned value in 64KiB..=4GiB \
         (the canonical authoring magnitudes — `\"64KiB\"`, `\"128KiB\"`, `\"1MiB\"`, \
         `\"64MiB\"`, `\"1GiB\"`, `\"4GiB\"` — every power-of-1024 unit the byte-size \
         codec emits divides cleanly by the page size) or omit the field for unbounded"
    )]
    MemoryNotPageMultiple { bytes: u64 },
    #[error(
        ":limits :fuel must be > 0 — wasmtime traps the first instruction at fuel=0; omit the field for unbounded"
    )]
    FuelZero,
    #[error(
        ":limits :fuel ({fuel} instructions) exceeds the per-process ceiling \
         (LIMITS_FUEL_MAX = 1_000_000_000_000 = 10^12 wasm instructions) — a value \
         above this cap turns the typed per-call fuel counter into a no-op budget: \
         the sibling `:wall-clock` cap (LIMITS_WALL_CLOCK_MAX = 1h = 3600s) fires \
         before the fuel counter could ever be drained (wasmtime's documented \
         fuel-tracked execution rate sits at ~10^8–10^9 fuel-units per second on \
         modern x86_64 / aarch64 hosts running wasmtime through Cranelift, so the \
         largest realistic per-call fuel budget reachable within 1h sits at ~3.6 × \
         10^11–3.6 × 10^12 fuel-units, and a value above 10^12 is structurally \
         unreachable as a per-call counter), so the typed `:fuel` slot becomes a \
         declared-but-no-op contract far from the source caixa.lisp. Pin a value \
         in 1..=1_000_000_000_000 (the canonical caixa Servico runs in the \
         10^6..=10^9 fuel band — the in-tree `Caixa::template` documentation and \
         `caixa-feira` examples carry `:fuel 1_000_000` = 10^6, peer to \
         wasmtime's official `Store::set_fuel(1_000_000)` example in the wasmtime \
         book; production-shape per-request fuel budgets sit in the 10^7..=10^9 \
         band for compute-bound workloads) or omit :fuel to express `no per-call \
         fuel budget on this axis` (the wasm-engine then relies entirely on the \
         sibling `:wall-clock` cgroup / Kubernetes activeDeadlineSeconds deadline)"
    )]
    FuelExceedsCap { fuel: u64 },
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
    // Paired whitespace-rejection arm — the ASCII byte-scan
    // (paste-from-aligned-doc leading space, shell-history trailing
    // space, typography space between magnitude and unit, block-scalar
    // tab, multi-line trailing newline) closes the WhatWG-conformant
    // ASCII whitespace bytes (`0x20`, `0x09`, `0x0A`, `0x0C`, `0x0D`);
    // the non-ASCII `char::is_whitespace` scan closes the strictly-
    // complementary Unicode `White_Space` class (NBSP `\u{00A0}`, LINE
    // SEPARATOR `\u{2028}`, EM-SPACE `\u{2003}`, and the peer
    // typography codepoints) that `str::trim` at parse entry silently
    // strips. Either drift class would round-trip through
    // `render_byte_size` to a *different* canonical form on next emit
    // — breaking the THEORY.md Part V render-determinism contract every
    // typed slot carries. Diagnostics stay typed at
    // `WhitespaceInByteSize` / `NonAsciiWhitespaceInByteSize` so the
    // failing byte / char + U+XXXX codepoint reaches the author verbatim
    // rather than being value-laundered through a downstream
    // `BadByteMagnitude` arm.
    //
    // Routed through the lifted [`crate::render::reject_whitespace`]
    // primitive — the substrate-side single-owner gate every typed-
    // magnitude codec in caixa-core (`parse_byte_size` /
    // `parse_duration` / `parse_millicores` /
    // `supervisor::duration_codec` / `rate_limit_codec`) shares. Drift
    // between any two codec sites' paired-arm rejection set becomes a
    // single-edit fix at the composed predicate rather than five
    // independent paired-arm re-inlines diverging over time.
    crate::render::reject_whitespace(
        s,
        |byte| LimitsError::WhitespaceInByteSize {
            value: s.into(),
            byte,
        },
        |ch| LimitsError::NonAsciiWhitespaceInByteSize {
            value: s.into(),
            ch,
            codepoint: ch as u32,
        },
    )?;
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
    //
    // Routed through the lifted
    // [`crate::render::is_digit_only_magnitude`] predicate — the
    // single source of truth every typed-magnitude codec in
    // caixa-core (`parse_byte_size` / `parse_duration` /
    // `parse_millicores` / `supervisor::duration_codec` /
    // `rate_limit_codec`) shares. Drift between any two codec sites'
    // digit-only rejection set becomes a single-edit fix at the
    // shared predicate rather than five independent
    // `!<var>.is_empty() && <var>.bytes().all(|b| b.is_ascii_digit())`
    // scans diverging over time — same "single lifted source of truth"
    // discipline the peer canonical-form predicates
    // ([`crate::render::find_ascii_whitespace_byte`] /
    // [`crate::render::find_non_ascii_whitespace_char`] /
    // [`crate::render::is_leading_zero_padded_magnitude`]) carry on
    // the whitespace and leading-zero-padding drift-class axes.
    let digit_only = crate::render::is_digit_only_magnitude(num_trim);
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
    // Leading-zero arm — peer with the `parse_duration` leading-zero
    // arm (39762d7), the `supervisor::duration_codec` leading-zero arm
    // (9178904) and the `rate_limit_codec` leading-zero arm (4f46830)
    // on the same canonical-form render-determinism axis. The
    // digit-only gate accepts `"0064MiB"`, `"01024"`, `"00KiB"`,
    // `"0500MB"` as `u64::from_str` parses them losslessly (= 64, 1024,
    // 0, 500), but `render_byte_size` emits the leading-zero-stripped
    // form (`"64MiB"`, `"1KiB"`, `"0"`, `"500MB"`) — a *different*
    // canonical string on the next emit, breaking the THEORY.md Part V
    // render-determinism contract the same way `"+1024"` did before the
    // leading-`+` arm landed. The single-byte magnitude `"0"` (or
    // `"0B"` / `"0KiB"`) round-trips losslessly through
    // `render_byte_size` (`render_byte_size(0)` emits `"0"`) — the
    // downstream semantic-zero gate [`LimitsError::MemoryZero`] refuses
    // zero-magnitude authoring at the typed-validate layer above, so
    // the single-byte `"0"` stays in the accepted set at this codec
    // layer and the diagnostic partitioning between canonical-form
    // drift (this arm) and semantic-zero (the downstream gate) remains
    // stable. Same codec-layer / typed-validate-layer partition the
    // peer codecs preserve.
    //
    // Routed through the lifted
    // [`crate::render::is_leading_zero_padded_magnitude`] predicate —
    // the single source of truth every typed-magnitude codec in
    // caixa-core (`parse_byte_size` / `parse_duration` /
    // `parse_millicores` / `supervisor::duration_codec` /
    // `rate_limit_codec`) shares. Drift between any two codec sites'
    // leading-zero rejection set becomes a single-edit fix at the
    // shared predicate rather than five independent
    // `s.len() > 1 && s.as_bytes()[0] == b'0'` scans diverging over
    // time — same "single lifted source of truth" discipline the
    // peer whitespace predicates
    // ([`crate::render::find_ascii_whitespace_byte`] /
    // [`crate::render::find_non_ascii_whitespace_char`]) carry on
    // their strictly-complementary axes.
    if crate::render::is_leading_zero_padded_magnitude(num_trim) {
        return Err(LimitsError::LeadingZeroByteMagnitude {
            value: num_trim.into(),
        });
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
    // Route through the canonical [`crate::render::serialize_option_via_str`]
    // — the substrate-side single-owner primitive for the forward arm
    // of the typed-magnitude codec family. See its docstring for the
    // full sibling roster and the compounding rationale that pins this
    // lift; load-bearing pinned by
    // `tests::ser_byte_size_routes_through_render_serialize_option_via_str_canonical`.
    crate::render::serialize_option_via_str(v, s, render_byte_size)
}

fn de_byte_size<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    // Route through the canonical [`crate::render::deserialize_option_via_str`]
    // — the substrate-side single-owner primitive for the reverse arm
    // of the typed-magnitude codec family. See its docstring for the
    // full sibling roster and the compounding rationale that pins this
    // lift; load-bearing pinned by
    // `tests::de_byte_size_routes_through_render_deserialize_option_via_str_canonical`.
    crate::render::deserialize_option_via_str(d, parse_byte_size)
}

// ── duration codec ─────────────────────────────────────────────────────

fn parse_duration(s: &str) -> Result<Duration, LimitsError> {
    // Paired whitespace-rejection arm — same canonical-form
    // render-determinism discipline as the peer `parse_byte_size` /
    // `parse_millicores` / `supervisor::duration_codec::parse` /
    // `rate_limit_codec::parse` sites: the ASCII byte-scan closes the
    // WhatWG-conformant whitespace bytes every downstream YAML / JSON /
    // TOML parser can feed through a quoted-scalar value verbatim
    // (`0x20`, `0x09`, `0x0A`, `0x0C`, `0x0D`), the non-ASCII
    // `char::is_whitespace` scan closes the strictly-complementary
    // Unicode `White_Space` class (NBSP `\u{00A0}`, LINE SEPARATOR
    // `\u{2028}`, EM-SPACE `\u{2003}`, and the peer typography
    // codepoints) that `str::trim` at parse entry silently strips.
    // Either drift class would round-trip through `render_duration` to
    // a *different* canonical form on next emit — breaking the
    // THEORY.md Part V render-determinism contract. Diagnostics stay
    // typed at `WhitespaceInDuration` / `NonAsciiWhitespaceInDuration`.
    //
    // Routed through the lifted [`crate::render::reject_whitespace`]
    // primitive — the substrate-side single-owner paired-arm gate every
    // typed-magnitude codec in caixa-core shares.
    crate::render::reject_whitespace(
        s,
        |byte| LimitsError::WhitespaceInDuration {
            value: s.into(),
            byte,
        },
        |ch| LimitsError::NonAsciiWhitespaceInDuration {
            value: s.into(),
            ch,
            codepoint: ch as u32,
        },
    )?;
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
    //
    // Routed through the lifted
    // [`crate::render::is_digit_only_magnitude`] predicate — the same
    // source of truth the four peer typed-magnitude codec sites share.
    let digit_only = crate::render::is_digit_only_magnitude(num_trim);
    if !digit_only {
        let numeric = num_trim.parse::<f64>().is_ok() || num_trim.parse::<i64>().is_ok();
        if numeric {
            return Err(LimitsError::NonIntegerDurationMagnitude {
                value: num_trim.into(),
            });
        }
        return Err(LimitsError::BadDurationMagnitude(num_part.into()));
    }
    // Leading-zero arm — peer with the `supervisor::duration_codec`
    // leading-zero arm (9178904) and the `rate_limit_codec`
    // leading-zero arm (4f46830) on the same canonical-form
    // render-determinism axis. The digit-only gate accepts `"030s"`,
    // `"00s"`, `"01h"`, `"0500ms"` as `u64::from_str` parses them
    // losslessly (= 30, 0, 1, 500), but `render_duration` emits the
    // leading-zero-stripped form (`"30s"`, `"0s"`, `"1h"`, `"500ms"`)
    // — a *different* canonical string on the next emit, breaking the
    // THEORY.md Part V render-determinism contract the same way
    // `"+30s"` did before the leading-`+` arm landed. The single-byte
    // magnitude `"0"` (or `"0s"` / `"0ms"`) round-trips losslessly
    // through `render_duration` (`render_duration(Duration::ZERO)`
    // emits `"0s"`) — the downstream semantic-zero gate
    // [`LimitsError::WallClockZero`] refuses zero-magnitude authoring
    // at the typed-validate layer above, so the single-byte `"0"`
    // stays in the accepted set at this codec layer and the
    // diagnostic partitioning between canonical-form drift (this arm)
    // and semantic-zero (the downstream gate) remains stable. Same
    // codec-layer / typed-validate-layer partition the peer codecs
    // preserve.
    //
    // Routed through the lifted
    // [`crate::render::is_leading_zero_padded_magnitude`] predicate —
    // the same source of truth the four peer typed-magnitude codec
    // sites share.
    if crate::render::is_leading_zero_padded_magnitude(num_trim) {
        return Err(LimitsError::LeadingZeroDurationMagnitude {
            value: num_trim.into(),
        });
    }
    // The digit-only gate guarantees every byte is `[0-9]`, and the
    // leading-zero arm above guarantees the magnitude is either the
    // single byte `"0"` or starts with `[1-9]`, so the only way
    // `u64::from_str` can fail here is overflow.
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

fn ser_duration<S: Serializer>(v: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
    // Route through the canonical [`crate::render::serialize_option_via_str`]
    // — the substrate-side single-owner primitive for the forward arm
    // of the typed-magnitude codec family — around the canonical
    // [`crate::supervisor::duration_codec::render`] duration-byte
    // dispatch. The `render` dispatch is itself the load-bearing
    // single-owner primitive for duration bytes across every caixa
    // typed-duration surface (`:limits :wall-clock`,
    // `:politicas :timeout`, `:circuit-breaker :window`, future OTP
    // `gen_server` per-call timeouts); the outer
    // `serialize_option_via_str` closes the `Some(_) => serialize_str`
    // / `None => serialize_none` `Option`-arm dispatch every peer
    // typed-magnitude serializer shares. Load-bearing pinned by
    // `tests::ser_duration_routes_through_supervisor_duration_codec_render_canonical`.
    crate::render::serialize_option_via_str(v, s, crate::supervisor::duration_codec::render)
}

fn de_duration<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
    // Route through the canonical [`crate::render::deserialize_option_via_str`]
    // — the substrate-side single-owner primitive for the reverse arm
    // of the typed-magnitude codec family. See its docstring for the
    // full sibling roster and the compounding rationale that pins this
    // lift.
    crate::render::deserialize_option_via_str(d, parse_duration)
}

// ── millicores codec ───────────────────────────────────────────────────

fn parse_millicores(s: &str) -> Result<u32, LimitsError> {
    // Paired whitespace-rejection arm — same canonical-form
    // render-determinism discipline as the peer `parse_byte_size` /
    // `parse_duration` / `supervisor::duration_codec::parse` /
    // `rate_limit_codec::parse` sites: the ASCII byte-scan closes the
    // WhatWG-conformant whitespace bytes (`0x20`, `0x09`, `0x0A`,
    // `0x0C`, `0x0D`), the non-ASCII `char::is_whitespace` scan closes
    // the strictly-complementary Unicode `White_Space` class (NBSP
    // `\u{00A0}`, LINE SEPARATOR `\u{2028}`, EM-SPACE `\u{2003}`, and
    // the peer typography codepoints) that `str::trim` at parse entry
    // silently strips. Either drift class would round-trip through
    // `render_millicores` to a *different* canonical form on next emit
    // — breaking the THEORY.md Part V render-determinism contract.
    // Diagnostics stay typed at `WhitespaceInMillicores` /
    // `NonAsciiWhitespaceInMillicores` — peer with every prior
    // canonical-form-drift arm on this codec
    // (`NonIntegerMillicoreMagnitude`, `LeadingZeroMillicoreMagnitude`).
    //
    // Routed through the lifted [`crate::render::reject_whitespace`]
    // primitive — the substrate-side single-owner paired-arm gate every
    // typed-magnitude codec in caixa-core shares.
    crate::render::reject_whitespace(
        s,
        |byte| LimitsError::WhitespaceInMillicores {
            value: s.into(),
            byte,
        },
        |ch| LimitsError::NonAsciiWhitespaceInMillicores {
            value: s.into(),
            ch,
            codepoint: ch as u32,
        },
    )?;
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
    //
    // Routed through the lifted
    // [`crate::render::is_digit_only_magnitude`] predicate — the same
    // source of truth the four peer typed-magnitude codec sites share.
    // The predicate carries a `!<var>.is_empty()` gate that is
    // strictly no-op here (the `magnitude.is_empty()` arm above
    // already surfaces an empty magnitude as
    // [`LimitsError::BadMillicores`] before this line is reached), so
    // the semantics are preserved verbatim: on every reachable input
    // the predicate returns `magnitude.bytes().all(|b|
    // b.is_ascii_digit())`, byte-for-byte what the removed inline
    // expression computed.
    let digit_only = crate::render::is_digit_only_magnitude(magnitude);
    if !digit_only {
        let numeric = magnitude.parse::<f64>().is_ok() || magnitude.parse::<i64>().is_ok();
        if numeric {
            return Err(LimitsError::NonIntegerMillicoreMagnitude {
                value: magnitude.into(),
            });
        }
        return Err(LimitsError::BadMillicores(s.into()));
    }
    // Leading-zero arm — peer with the `parse_byte_size` leading-zero
    // arm (cea9a78), the `parse_duration` leading-zero arm (39762d7),
    // the `supervisor::duration_codec` leading-zero arm (9178904) and
    // the `rate_limit_codec` leading-zero arm (4f46830) on the same
    // canonical-form render-determinism axis. The digit-only gate
    // accepts `"0500m"`, `"00m"`, `"02"`, `"01500m"` as `u32::from_str`
    // parses them losslessly (= 500, 0, 2, 1500), but `render_millicores`
    // emits the leading-zero-stripped form (`"500m"`, `"0m"`, `"2000m"`,
    // `"1500m"`) — a *different* canonical string on the next emit,
    // breaking the THEORY.md Part V render-determinism contract the
    // same way `"+500m"` did before the leading-`+` arm landed. The
    // single-byte magnitude `"0"` (or `"0m"`) round-trips losslessly
    // through `render_millicores` (`render_millicores(0)` emits `"0m"`)
    // — the downstream semantic-zero gate [`LimitsError::CpuZero`]
    // refuses zero-magnitude authoring at the typed-validate layer
    // above, so the single-byte `"0"` stays in the accepted set at this
    // codec layer and the diagnostic partitioning between canonical-
    // form drift (this arm) and semantic-zero (the downstream gate)
    // remains stable. Same codec-layer / typed-validate-layer partition
    // the peer codecs preserve. Closes the sixth (and last) typed
    // numeric-codec surface in caixa-core on the integer-magnitude
    // leading-zero axis — the trajectory the prior `parse_byte_size`
    // arm (cea9a78) explicitly named.
    //
    // Routed through the lifted
    // [`crate::render::is_leading_zero_padded_magnitude`] predicate —
    // the same source of truth the four peer typed-magnitude codec
    // sites share.
    if crate::render::is_leading_zero_padded_magnitude(magnitude) {
        return Err(LimitsError::LeadingZeroMillicoreMagnitude {
            value: magnitude.into(),
        });
    }
    // The digit-only gate guarantees every byte is `[0-9]`, and the
    // leading-zero arm above guarantees the magnitude is either the
    // single byte `"0"` or starts with `[1-9]`, so the only way
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
    // Route through the canonical [`crate::render::serialize_option_via_str`]
    // — see peer `ser_byte_size` / `ser_duration` routing notes above.
    crate::render::serialize_option_via_str(v, s, render_millicores)
}

fn de_millicores<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    // Route through the canonical [`crate::render::deserialize_option_via_str`]
    // — see peer `de_byte_size` / `de_duration` routing notes above.
    crate::render::deserialize_option_via_str(d, parse_millicores)
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
    fn ser_byte_size_routes_through_render_serialize_option_via_str_canonical() {
        // Routing pin: `ser_byte_size` (the `#[serde(serialize_with = …)]`
        // hook on `LimitsSpec::memory`) MUST emit exactly the bytes the
        // canonical `crate::render::serialize_option_via_str` primitive
        // produces when threaded through the peer `render_byte_size`
        // dispatch. Any future accidental re-inline of a bespoke
        // `match v { Some(_) => s.serialize_str(_), None =>
        // s.serialize_none() }` block inside this module — the shape
        // this lift removed — surfaces here as a byte-value drift on
        // the very first canonical form the two implementations
        // disagree on. Peer of
        // `ser_duration_routes_through_supervisor_duration_codec_render_canonical`
        // on the sibling `LimitsSpec::wall_clock` axis; same "one
        // canonical dispatch per axis, thin projections at each
        // consumer" discipline the sibling caixa-core substrate
        // primitives already carry.
        for n in [
            0u64,
            1,
            1023,
            1024,
            64 * 1024 * 1024,
            4 * 1024 * 1024 * 1024,
        ] {
            let limits = LimitsSpec {
                memory: Some(n),
                fuel: None,
                wall_clock: None,
                cpu: None,
            };
            let json: serde_json::Value =
                serde_json::from_str(&serde_json::to_string(&limits).unwrap()).unwrap();
            let emitted = json[crate::render::M2_LIMITS_KEY_MEMORY]
                .as_str()
                .expect("memory must serialize to a string");
            let canonical = render_byte_size(n);
            assert_eq!(
                emitted, canonical,
                "ser_byte_size drifted from render_byte_size via \
                 serialize_option_via_str on {n} bytes",
            );
        }
    }

    #[test]
    fn de_byte_size_routes_through_render_deserialize_option_via_str_canonical() {
        // Routing pin: `de_byte_size` (the
        // `#[serde(deserialize_with = …)]` hook on
        // `LimitsSpec::memory`) MUST accept exactly the canonical
        // string set the peer `parse_byte_size` function accepts, and
        // reject everything else with the parser's typed `LimitsError`
        // surfaced through `serde::de::Error::custom` — the shape the
        // lifted `crate::render::deserialize_option_via_str` primitive
        // enforces. A future accidental re-inline of a bespoke `let
        // opt: Option<String> = Option::deserialize(d)?; match opt {
        // … }` block inside this module — the shape this lift removed
        // — that drifted on either arm (silently accepting a value the
        // parser rejects, or swallowing a parser error as `Ok(None)`)
        // surfaces here.
        for raw in ["64MiB", "1024", "0", "4GiB"] {
            let field = crate::render::M2_LIMITS_KEY_MEMORY;
            let payload = format!("{{\"{field}\":\"{raw}\"}}");
            let limits: LimitsSpec =
                serde_json::from_str(&payload).expect("canonical memory string must round-trip");
            let canonical = parse_byte_size(raw).expect("parse_byte_size accepts canonical form");
            assert_eq!(
                limits.memory,
                Some(canonical),
                "de_byte_size drifted from parse_byte_size via \
                 deserialize_option_via_str on {raw:?}",
            );
        }
        // Null-arm pin: `null` folds to `None` without invoking the
        // parser — the exact contract the lifted primitive's null-arm
        // test pins.
        let field = crate::render::M2_LIMITS_KEY_MEMORY;
        let null_payload = format!("{{\"{field}\":null}}");
        let empty: LimitsSpec = serde_json::from_str(&null_payload)
            .expect("null memory field must fold to LimitsSpec::memory = None");
        assert_eq!(
            empty.memory, None,
            "de_byte_size must fold null → None via \
             deserialize_option_via_str's null-arm",
        );
        // Reject-arm pin: a bogus string surfaces the parser's error
        // through `serde::de::Error::custom` — not `Ok(None)`.
        let bad_payload = format!("{{\"{field}\":\"64XiB\"}}");
        let err = serde_json::from_str::<LimitsSpec>(&bad_payload)
            .expect_err("bogus memory string must surface the parser's error");
        let err_text = err.to_string();
        assert!(
            err_text.contains("64XiB") || err_text.contains("XiB"),
            "de_byte_size must surface parse_byte_size's typed \
             LimitsError through serde::de::Error::custom — got \
             {err_text:?}",
        );
    }

    #[test]
    fn ser_duration_routes_through_supervisor_duration_codec_render_canonical() {
        // Routing pin: `ser_duration` (the `#[serde(serialize_with = …)]`
        // hook on `LimitsSpec::wall_clock`) MUST emit exactly the bytes
        // the canonical `crate::supervisor::duration_codec::render`
        // primitive produces. Any future accidental re-introduction of a
        // sibling free-function `render_duration` shadow inside this
        // module — or a per-slot `serialize_with` closure that inlines
        // its own magnitude/unit decision tree — surfaces here as a
        // byte-value drift on the very first canonical form the two
        // implementations disagree on, well before the drift reaches any
        // downstream renderer's `wall_clock:` overlay. Same "one
        // canonical dispatch per axis, thin projections at each consumer"
        // discipline the sibling caixa-core substrate primitives already
        // carry on the peer WIT-shape / M2 supervisor-strategy / M3
        // mesh-slot free-function classifier families.
        for d in [
            Duration::from_secs(30),
            Duration::from_millis(500),
            Duration::from_secs(120),
            Duration::from_secs(3600),
            Duration::from_millis(0),
            Duration::from_millis(1500),
        ] {
            let limits = LimitsSpec {
                memory: None,
                fuel: None,
                wall_clock: Some(d),
                cpu: None,
            };
            let json: serde_json::Value =
                serde_json::from_str(&serde_json::to_string(&limits).unwrap()).unwrap();
            let emitted = json[crate::render::M2_LIMITS_KEY_WALL_CLOCK]
                .as_str()
                .expect("wall_clock must serialize to a string");
            let canonical = crate::supervisor::duration_codec::render(d);
            assert_eq!(
                emitted, canonical,
                "ser_duration drifted from supervisor::duration_codec::render on {d:?}",
            );
        }
    }

    #[test]
    fn parse_byte_size_routes_whitespace_through_render_reject_whitespace_canonical() {
        // Routing pin: the paired whitespace-rejection block at the
        // top of `parse_byte_size` MUST route through the substrate-
        // side [`crate::render::reject_whitespace`] primitive — the
        // single-owner paired-arm gate every typed-magnitude codec
        // in caixa-core shares. Any future accidental re-inline of a
        // bespoke
        //
        // ```ignore
        // if let Some(byte) = find_ascii_whitespace_byte(s) { … }
        // if let Some(ch)   = find_non_ascii_whitespace_char(s)  { … }
        // ```
        //
        // block inside this module — the shape this lift removed —
        // that drifted on either arm surfaces here as a variant-shape
        // drift on the very first canonical form the two
        // implementations disagree on. Byte-shape pins cover the
        // ASCII WhatWG-conformant set (space / tab / LF / FF / CR)
        // and the strictly-complementary non-ASCII Unicode
        // `White_Space` class (NBSP / LINE SEPARATOR / EM-SPACE /
        // IDEOGRAPHIC SPACE) on the exemplar `:limits :memory` axis
        // — peer of the pre-existing `ser_byte_size_routes_through_
        // render_serialize_option_via_str_canonical` /
        // `de_byte_size_routes_through_render_deserialize_option_
        // via_str_canonical` pins on the sibling codec-hook axis.
        for (raw, byte) in [
            (" 64MiB", 0x20u8),
            ("64MiB ", 0x20u8),
            ("64 MiB", 0x20u8),
            ("\t64MiB", 0x09u8),
            ("64MiB\n", 0x0Au8),
        ] {
            let err = parse_byte_size(raw)
                .expect_err("ASCII-whitespace-carrying byte-size input must be rejected");
            let via_primitive = crate::render::reject_whitespace::<LimitsError, _, _>(
                raw,
                |b| LimitsError::WhitespaceInByteSize {
                    value: raw.into(),
                    byte: b,
                },
                |ch| LimitsError::NonAsciiWhitespaceInByteSize {
                    value: raw.into(),
                    ch,
                    codepoint: ch as u32,
                },
            )
            .expect_err("primitive must reject the same ASCII-whitespace shape");
            assert_eq!(
                err, via_primitive,
                "parse_byte_size drifted from crate::render::reject_whitespace \
                 on ASCII-whitespace input {raw:?}"
            );
            assert!(
                matches!(
                    err,
                    LimitsError::WhitespaceInByteSize { value: ref v, byte: b } if v == raw && b == byte
                ),
                "parse_byte_size must surface WhitespaceInByteSize {{ value: {raw:?}, byte: 0x{byte:02x} }}"
            );
        }
        for (raw, expected_ch) in [
            ("\u{00A0}64MiB", '\u{00A0}'),
            ("64\u{2003}MiB", '\u{2003}'),
            ("64MiB\u{2028}", '\u{2028}'),
            ("\u{3000}64MiB", '\u{3000}'),
        ] {
            let err = parse_byte_size(raw)
                .expect_err("non-ASCII-whitespace-carrying byte-size input must be rejected");
            let via_primitive = crate::render::reject_whitespace::<LimitsError, _, _>(
                raw,
                |b| LimitsError::WhitespaceInByteSize {
                    value: raw.into(),
                    byte: b,
                },
                |ch| LimitsError::NonAsciiWhitespaceInByteSize {
                    value: raw.into(),
                    ch,
                    codepoint: ch as u32,
                },
            )
            .expect_err("primitive must reject the same non-ASCII-whitespace shape");
            assert_eq!(
                err, via_primitive,
                "parse_byte_size drifted from crate::render::reject_whitespace \
                 on non-ASCII-whitespace input {raw:?}"
            );
            assert!(
                matches!(
                    err,
                    LimitsError::NonAsciiWhitespaceInByteSize { value: ref v, ch, codepoint }
                        if v == raw && ch == expected_ch && codepoint == expected_ch as u32
                ),
                "parse_byte_size must surface NonAsciiWhitespaceInByteSize \
                 {{ value: {raw:?}, ch: {expected_ch:?}, codepoint: {cp:#06X} }}",
                cp = expected_ch as u32
            );
        }
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

    // ── drift-detection: serde-derive-to-M2_LIMITS_KEY_* identity ────────

    #[test]
    fn limits_spec_serde_keys_match_lifted_m2_limits_key_consts() {
        // Load-bearing invariant: the four `M2_LIMITS_KEY_*` consts
        // (`M2_LIMITS_KEY_MEMORY` / `M2_LIMITS_KEY_FUEL` /
        // `M2_LIMITS_KEY_WALL_CLOCK` / `M2_LIMITS_KEY_CPU`) name the
        // exact camelCase JSON keys the `#[serde(rename_all = "camelCase")]`
        // attribute on `LimitsSpec` emits, and every test-side probe
        // across the caixa-core / caixa-flux / caixa-helm renderer test
        // fixtures navigates into the rendered `:limits` overlay
        // sub-block by consulting one of these four `&'static str`s.
        // Serialize a fully-populated LimitsSpec and pin that each
        // canonical byte-sequence appears verbatim in the JSON — a
        // future accidental `rename_all = "snake_case"` /
        // `"kebab-case"` / verbatim-field-name flip at the derive
        // attribute (any of which would silently break every test-side
        // probe that reaches for one of the four consts) surfaces here
        // as a build-time test failure at `limits.rs`, not as an
        // apply-time `.get(<stale-canonical-const>)` returning `None`
        // far from the derive-attr drift's commit. Same discipline the
        // sibling M3 `PlacementStrategy::as_str` lift (0a2f653)
        // established on the peer per-`:placement :estrategia` axis:
        // one canonical byte-string per typed sub-key axis, pinned to
        // the load-bearing serde derivation at the type itself.
        let limits = LimitsSpec {
            memory: Some(64 * 1024 * 1024),
            fuel: Some(1_000_000),
            wall_clock: Some(Duration::from_secs(30)),
            cpu: Some(500),
        };
        let json = serde_json::to_string(&limits).unwrap();
        for key in [
            crate::render::M2_LIMITS_KEY_MEMORY,
            crate::render::M2_LIMITS_KEY_FUEL,
            crate::render::M2_LIMITS_KEY_WALL_CLOCK,
            crate::render::M2_LIMITS_KEY_CPU,
        ] {
            let quoted = format!("\"{key}\"");
            assert!(
                json.contains(&quoted),
                "serialized LimitsSpec must carry the lifted \
                 M2_LIMITS_KEY_* byte-sequence {quoted} verbatim in \
                 the JSON emission (got: {json})",
            );
        }
    }

    #[test]
    fn m2_limits_key_consts_are_pairwise_distinct() {
        // Cross-axis drift-detection pin: a future collapse of two
        // canonical sub-key byte-strings onto the same value (e.g. an
        // accidental copy-paste flip of `M2_LIMITS_KEY_CPU` to also
        // read `"memory"`) would silently reroute every test-side
        // probe on one axis onto the sibling axis's overlay entry and
        // pass every propagation-probe test that expected only the
        // stale axis's value. Peer of the sibling three-way distinct
        // pin on the `FLUX_GITREPOSITORY_REF_KEY_*` trio (7d40380).
        let all = [
            crate::render::M2_LIMITS_KEY_MEMORY,
            crate::render::M2_LIMITS_KEY_FUEL,
            crate::render::M2_LIMITS_KEY_WALL_CLOCK,
            crate::render::M2_LIMITS_KEY_CPU,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(
                    a, b,
                    "M2_LIMITS_KEY_* consts must be pairwise-distinct \
                     canonical byte-sequences — got `{a}` == `{b}`",
                );
            }
        }
    }

    #[test]
    fn m2_limits_key_consts_are_lower_camel_case_shape() {
        // Shape-pin: every `M2_LIMITS_KEY_*` const must be a
        // lowerCamelCase byte-sequence (no `snake_case` underscores,
        // no `kebab-case` hyphens, no `PascalCase` leading capital, no
        // whitespace / colons / dots) — the canonical shape the
        // `#[serde(rename_all = "camelCase")]` derive produces on
        // `LimitsSpec`. A future flip to a non-camelCase attribute at
        // the derive surfaces both here (this test fails on the
        // stale-constant shape) and at
        // `limits_spec_serde_keys_match_lifted_m2_limits_key_consts`
        // (that test fails on the mismatch between const and derive).
        for key in [
            crate::render::M2_LIMITS_KEY_MEMORY,
            crate::render::M2_LIMITS_KEY_FUEL,
            crate::render::M2_LIMITS_KEY_WALL_CLOCK,
            crate::render::M2_LIMITS_KEY_CPU,
        ] {
            assert!(
                !key.is_empty(),
                "M2_LIMITS_KEY_* must be non-empty (got {key:?})"
            );
            let first = key.chars().next().unwrap();
            assert!(
                first.is_ascii_lowercase(),
                "M2_LIMITS_KEY_* must lead with an ASCII-lowercase byte \
                 (got {key:?}, leads with {first:?})",
            );
            assert!(
                key.chars().all(|c| c.is_ascii_alphanumeric()),
                "M2_LIMITS_KEY_* must be ASCII-alphanumeric only \
                 — no `_` / `-` / `:` / `.` / whitespace (got {key:?})",
            );
        }
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

    // ── value-shape: :memory page-multiple granularity gate ───────────────

    #[test]
    fn validate_rejects_memory_one_byte_above_page() {
        // The fail-before-pass-after pin: until this gate landed a
        // `LimitsSpec { memory: Some(LIMITS_MEMORY_WASM32_PAGE_BYTES +
        // 1), .. }` (65537 bytes — one wasm32 page plus a 1-byte
        // unreachable residue) silently passed validate, the byte-size
        // codec round-tripped cleanly through serde (`render_byte_size`
        // falls through to `"65537"` on any non-power-of-1024 magnitude),
        // and wasmtime's `StoreLimits::memory_size` consumed the value
        // verbatim as a page-quantized ceiling — the engine grew at
        // most floor(65537 / 65536) = 1 page, and the byte at offset
        // 65536 became structural dead space the runtime cannot honor.
        let bytes = LIMITS_MEMORY_WASM32_PAGE_BYTES + 1;
        let l = LimitsSpec {
            memory: Some(bytes),
            ..Default::default()
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::MemoryNotPageMultiple { bytes }
        );
    }

    #[test]
    fn validate_rejects_memory_just_below_two_pages() {
        // Boundary case: exactly 1 byte below two pages (`2 *
        // LIMITS_MEMORY_WASM32_PAGE_BYTES - 1` = 131071 bytes). Pins
        // the inclusive-page-boundary / strict-sub-page-residue
        // relationship on the page-multiple arm: 131071 is *out*
        // (sub-page residue), 131072 is *in* (exactly two pages).
        // Matches the peer `validate_rejects_memory_one_byte_below_page`
        // / `validate_rejects_memory_one_byte_above_wasm32_cap` shape
        // on the surrounding edges.
        let bytes = 2 * LIMITS_MEMORY_WASM32_PAGE_BYTES - 1;
        let l = LimitsSpec {
            memory: Some(bytes),
            ..Default::default()
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::MemoryNotPageMultiple { bytes }
        );
    }

    #[test]
    fn validate_rejects_memory_100000_bytes() {
        // The "obvious authoring footgun" case: a magnitude the
        // byte-size codec accepts cleanly (`"100000"` → 100000 bytes
        // ≈ 97.65 KiB) and serde round-trips silently, but no wasm32
        // engine can honor as a meaningful ceiling — the engine grows
        // at most floor(100000 / 65536) = 1 page, and the 34464 bytes
        // between offsets 65536 and 100000 are structural dead space.
        // Until this gate landed `validate` accepted it. Peer of
        // `validate_rejects_memory_8_gib` on the cap arm.
        let bytes = parse_byte_size("100000").unwrap();
        let l = LimitsSpec {
            memory: Some(bytes),
            ..Default::default()
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::MemoryNotPageMultiple { bytes }
        );
    }

    #[test]
    fn validate_accepts_every_page_aligned_value_through_serde() {
        // Positive-control sweep through the byte-size codec: every
        // canonical magnitude `render_byte_size` emits at or above
        // the page floor divides cleanly by the page size, so the
        // page-multiple gate accepts the entire canonical-output
        // domain at and above the page floor. The sweep walks
        // single-page (`"64KiB"`), two-page (`"128KiB"`), every
        // power-of-1024 unit (`"1MiB"`, `"64MiB"`, `"1GiB"`, `"4GiB"`),
        // and the cap (`"4GiB"`) — pinning that the codec's
        // emitted-canonical-form set is a structural subset of the
        // validate gate's accepted set. Drift between the codec's
        // emit alphabet and the validate gate would surface here
        // rather than at a future serializer round trip.
        for s in ["64KiB", "128KiB", "1MiB", "64MiB", "1GiB", "4GiB"] {
            let bytes = parse_byte_size(s).unwrap();
            assert_eq!(
                bytes % LIMITS_MEMORY_WASM32_PAGE_BYTES,
                0,
                "canonical byte-size codec output {s:?} ({bytes}) must be page-aligned",
            );
            let l = LimitsSpec {
                memory: Some(bytes),
                ..Default::default()
            };
            l.validate()
                .unwrap_or_else(|e| panic!("canonical {s:?} = {bytes} must validate, got {e:?}"));
        }
    }

    #[test]
    fn validate_memory_below_page_takes_precedence_over_page_multiple() {
        // Cross-arm ordering pin: `Some(1)` would otherwise pass the
        // page-multiple arm's `m % PAGE_BYTES != 0` check (1 % 65536
        // == 1 ≠ 0), but the page-floor arm strictly precedes the
        // page-multiple arm so the more self-locating
        // `MemoryBelowWasm32Page` diagnostic (with its "single page
        // cannot fit" remediation, applicable to every sub-page
        // value uniformly) leads. Peer of `MemoryZero` →
        // `MemoryBelowWasm32Page` precedence on the zero edge:
        // every value `m` in the range `1..=PAGE_BYTES-1` satisfies
        // both `m < PAGE_BYTES` and `m % PAGE_BYTES != 0`, but the
        // structurally-narrower diagnostic (page-floor) leads.
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
    fn validate_memory_cap_takes_precedence_over_page_multiple() {
        // Cross-arm ordering pin: `LIMITS_MEMORY_WASM32_MAX_BYTES + 1`
        // (4 GiB + 1 byte) is *both* above-cap and not page-aligned.
        // The cap arm strictly precedes the page-multiple arm so the
        // more aggressive cap-shape diagnostic leads (the page-multiple
        // remediation would be misleading when the offending value
        // exceeds the wasm32 address-space ceiling anyway — the
        // canonical fix collapses both into "pin a page-aligned value
        // ≤ 4 GiB"). Peer of `WallClockNotCanonical` →
        // `WallClockExceedsCap` ordering on the sibling `:wall-clock`
        // axis (with the inverse polarity — there the granularity
        // gate leads because sub-millisecond residue breaks serde
        // round-trip; here the cap leads because both gates' offending
        // values round-trip cleanly through serde and the broader
        // magnitude constraint is the more aggressive one).
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
    fn validate_rejects_memory_page_multiple_before_other_axes() {
        // With a sub-page-residue `:memory` and zero values on every
        // other axis, the diagnostic names `:memory` rather than
        // `:fuel` / `:wall-clock` / `:cpu` — peer of the existing
        // `validate_memory_page_floor_takes_precedence_over_other_axes`
        // and `validate_rejects_memory_cap_before_other_axes` ordering
        // pins. Memory is the first axis the validate cascade checks,
        // so a sub-page-residue value surfaces before any other-axis
        // diagnostic regardless of how many other axes are
        // simultaneously invalid.
        let bytes = LIMITS_MEMORY_WASM32_PAGE_BYTES + 1;
        let l = LimitsSpec {
            memory: Some(bytes),
            fuel: Some(0),
            wall_clock: Some(Duration::ZERO),
            cpu: Some(0),
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::MemoryNotPageMultiple { bytes }
        );
    }

    #[test]
    fn memory_page_multiple_diagnostic_carries_offending_bytes() {
        // Diagnostic-shape pin: the page-multiple arm names the
        // offending byte count verbatim so the author's grep lands on
        // the field's value, not a generic "memory not aligned"
        // message. Same shape every other typed-cap arm on this
        // surface carries (`MemoryExceedsWasm32Cap` carries the
        // offending byte count verbatim, `WallClockNotCanonical`
        // carries the offending `Duration` verbatim).
        let bytes = LIMITS_MEMORY_WASM32_PAGE_BYTES + 12345;
        let l = LimitsSpec {
            memory: Some(bytes),
            ..Default::default()
        };
        let err = l.validate().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains(&bytes.to_string()),
            "diagnostic must carry the offending byte count verbatim (got {msg:?})"
        );
        assert!(
            msg.contains("64 KiB") || msg.contains("65536") || msg.contains("page"),
            "diagnostic must name the page-size granularity (got {msg:?})"
        );
    }

    #[test]
    fn sub_page_residue_value_still_round_trips_through_serde() {
        // The byte-size codec accepts the sub-page-residue value (the
        // page-multiple gate lives in validate, not in the codec) —
        // peer of `above_cap_value_still_round_trips_through_serde`
        // and `below_page_value_still_round_trips_through_serde`.
        // Pins that the structural property is "sub-page-residue is
        // rejected by validate" — not "sub-page-residue is
        // unparseable by the codec"; the latter would prevent the
        // diagnostic from naming the offending byte count at all,
        // since deserialize would fail first. The render-then-parse
        // round trip also pins the codec's flow-through-to-bytes
        // shape on non-power-of-1024 magnitudes: `render_byte_size`
        // falls through every `(mult, label)` arm whose `n % mult !=
        // 0` and emits the bare byte count.
        let bytes = LIMITS_MEMORY_WASM32_PAGE_BYTES + 1;
        let l = LimitsSpec {
            memory: Some(bytes),
            ..Default::default()
        };
        let json = serde_json::to_string(&l).unwrap();
        let back: LimitsSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(l, back);
        assert!(back.validate().is_err());
    }

    #[test]
    fn validate_memory_axis_routes_through_quantum_multiple_bounded_helper() {
        // Byte-parity pin on the pre-lift `if self.memory() == Some(0)
        // { … } if let Some(m) = self.memory() { if m <
        // LIMITS_MEMORY_WASM32_PAGE_BYTES { … } } if let Some(m) =
        // self.memory() { if m > LIMITS_MEMORY_WASM32_MAX_BYTES { … } }
        // if let Some(m) = self.memory() && m %
        // LIMITS_MEMORY_WASM32_PAGE_BYTES != 0 { … }` four-sequential-
        // `if let` shape the `LimitsSpec::validate` `:memory` axis
        // routed through today via
        // `crate::render::require_positive_quantum_multiple_bounded_u64`.
        // Refuses a future accidental split between the helper's
        // four-arm ordering (zero → below-quantum → cap → not-multiple)
        // and the four typed `LimitsError::Memory*` variants each arm
        // threads its offending byte count into — a swap of any two
        // arms in the helper, or a partial widening (e.g. removing the
        // page-multiple arm), or a widening of the `on_below_quantum`
        // arm's closure to the `MemoryExceedsWasm32Cap` variant instead
        // of `MemoryBelowWasm32Page` — would break exactly one row of
        // this pin, matching the pre-lift shape the four consumer sites
        // route through today. Same shape as
        // `as_seq_body_partitions_the_same_arm_set_as_seq_delims` in
        // caixa-ast and the peer `require_positive_bounded_u64` tests
        // in the sibling render.rs test module.
        //
        // (Some(bytes) → expected LimitsError)
        let quantum = LIMITS_MEMORY_WASM32_PAGE_BYTES;
        let cap = LIMITS_MEMORY_WASM32_MAX_BYTES;
        let cases: &[(u64, LimitsError)] = &[
            (0, LimitsError::MemoryZero),
            (1, LimitsError::MemoryBelowWasm32Page { bytes: 1 }),
            (
                quantum - 1,
                LimitsError::MemoryBelowWasm32Page { bytes: quantum - 1 },
            ),
            (
                cap + 1,
                LimitsError::MemoryExceedsWasm32Cap { bytes: cap + 1 },
            ),
            (
                cap + quantum,
                LimitsError::MemoryExceedsWasm32Cap {
                    bytes: cap + quantum,
                },
            ),
            (
                quantum + 1,
                LimitsError::MemoryNotPageMultiple { bytes: quantum + 1 },
            ),
            (
                quantum + 12_345,
                LimitsError::MemoryNotPageMultiple {
                    bytes: quantum + 12_345,
                },
            ),
        ];
        for (bytes, expected) in cases {
            let l = LimitsSpec {
                memory: Some(*bytes),
                ..Default::default()
            };
            assert_eq!(
                l.validate().unwrap_err(),
                *expected,
                "memory={bytes} must surface the {expected:?} arm via the substrate helper",
            );
        }
        // Positive-control: every quantum-multiple in `quantum..=cap`
        // passes, closing the four-arm cascade with an `Ok(())` shape.
        for bytes in [quantum, quantum * 2, quantum * 100, cap] {
            let l = LimitsSpec {
                memory: Some(bytes),
                ..Default::default()
            };
            l.validate().unwrap();
        }
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

    // ── canonical-form: leading-zero byte-size codec gate ─────────────────
    //
    // Direct successor to the `parse_duration` leading-zero arm (39762d7),
    // the `supervisor::duration_codec` leading-zero arm (9178904), and the
    // `rate_limit_codec` leading-zero arm (4f46830) — the same canonical-
    // form render-determinism axis applied to the last typed-numeric codec
    // that still admitted leading-zero magnitudes. The digit-only gate
    // immediately above accepts every `u64::from_str`-parseable magnitude
    // including leading-zero padding, but `render_byte_size` always emits
    // the stripped form (`64MiB`, never `064MiB`) — silently drifting the
    // canonical string across a parse/render round-trip. Pins each
    // canonical leading-zero shape across the unit-set the codec admits
    // (KB / MB / GB / KiB / MiB / GiB / bare-integer), the all-zero
    // degenerate case, the codec-vs-validate-layer partition (single-byte
    // `"0"` stays accepted at the codec because the typed-validate gate
    // `MemoryZero` refuses semantic-zero authoring), the complement-side
    // pin (`1`..=`9`-led magnitudes stay accepted), and the serde-path pin
    // (the gate fires at deserialize, before any validate gate runs).

    #[test]
    fn parse_byte_size_rejects_leading_zero_magnitude() {
        // The fail-before-pass-after pin: `"064MiB"` parsed cleanly on
        // every pre-gate codebase (`u64::from_str` accepts the leading
        // zero), the codec produced 64 MiB, and
        // `render_byte_size(64*1024*1024)` emitted `"64MiB"` on the next
        // serialize — silently dropping the leading zero and drifting
        // the canonical form away from the author's intent. The new
        // gate surfaces the round-trip break at the parser layer with a
        // self-locating diagnostic, peer with
        // `parse_duration_rejects_leading_zero_magnitude` on the sibling
        // codec.
        let err = parse_byte_size("064MiB").unwrap_err();
        assert!(
            matches!(err, LimitsError::LeadingZeroByteMagnitude { ref value } if value == "064"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_byte_size_rejects_multi_digit_zero_magnitude() {
        // `"00MiB"` is the degenerate leading-zero case — every byte is
        // `0`. `u64::from_str("00")` = 0, and the codec produces 0;
        // `render_byte_size(0)` emits `"0"` on the next serialize —
        // drift from `"00MiB"` to `"0"`. The leading-zero arm refuses
        // the drift class at the codec layer while leaving the
        // canonical single-byte `"0"` accepted. Peer with
        // `parse_duration_rejects_multi_digit_zero_magnitude` on the
        // sibling codec.
        let err = parse_byte_size("00MiB").unwrap_err();
        assert!(
            matches!(err, LimitsError::LeadingZeroByteMagnitude { ref value } if value == "00"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_byte_size_rejects_leading_zero_in_gib_unit() {
        // `"01GiB"` parses to 1 GiB; the renderer emits `"1GiB"` on the
        // next serialize. The leading-zero class is a property of the
        // magnitude, not the unit — pin a per-GiB magnitude alongside
        // the per-MiB / per-KiB / bare-integer pins so the gate's
        // coverage is structural across every canonical unit suffix
        // the codec accepts. Mirrors the per-hour pin
        // `parse_duration_rejects_leading_zero_in_hour_window` carries
        // on the sibling codec.
        let err = parse_byte_size("01GiB").unwrap_err();
        assert!(
            matches!(err, LimitsError::LeadingZeroByteMagnitude { ref value } if value == "01"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_byte_size_rejects_leading_zero_in_kib_unit() {
        // `"0512KiB"` parses to 512 KiB; the renderer emits `"512KiB"`
        // on the next serialize. Pin the per-KiB magnitude alongside
        // the per-MiB / per-GiB pins so the gate's coverage extends to
        // the smallest-unit power-of-1024 suffix the codec admits.
        let err = parse_byte_size("0512KiB").unwrap_err();
        assert!(
            matches!(err, LimitsError::LeadingZeroByteMagnitude { ref value } if value == "0512"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_byte_size_rejects_leading_zero_in_decimal_units() {
        // `"0500MB"` parses to 500 MB (decimal-unit family — `KB` /
        // `MB` / `GB` powers of 1000, distinct from the `KiB` / `MiB` /
        // `GiB` powers-of-1024 family); the renderer emits the
        // appropriate canonical form on the next serialize. Pin the
        // decimal-unit family alongside the power-of-1024 family so the
        // gate's coverage is structural across both unit families the
        // codec admits.
        for (s, expected) in [("0500MB", "0500"), ("01KB", "01"), ("00GB", "00")] {
            let err = parse_byte_size(s).unwrap_err();
            assert!(
                matches!(err, LimitsError::LeadingZeroByteMagnitude { value: ref v } if v == expected),
                "got {err:?} for {s:?}"
            );
        }
    }

    #[test]
    fn parse_byte_size_rejects_leading_zero_bare_integer() {
        // The bare-integer (no unit) shorthand inherits the leading-
        // zero arm: `"01024"` parses losslessly to 1024 bytes but
        // `render_byte_size(1024)` emits `"1KiB"` on the next serialize.
        // Pin the bare-integer path so a future relaxation that
        // special-cases the unitless shorthand surfaces here as a test
        // failure. Mirrors the bare-integer pin
        // `parse_duration_rejects_leading_zero_bare_integer_as_seconds`
        // carries on the sibling codec.
        let err = parse_byte_size("01024").unwrap_err();
        assert!(
            matches!(err, LimitsError::LeadingZeroByteMagnitude { ref value } if value == "01024"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_byte_size_accepts_single_zero_magnitude_at_codec_layer() {
        // The codec-layer / typed-validate-layer boundary pin: the
        // single-byte `"0"` magnitude round-trips losslessly through
        // `render_byte_size` (`render_byte_size(0)` emits `"0"`), so it
        // stays accepted at this codec layer across every canonical
        // unit suffix. The downstream `LimitsError::MemoryZero` gate is
        // what refuses zero-magnitude authoring at the typed-validate
        // layer above — the partition keeps the canonical-form-drift
        // diagnostic (this arm) and the semantic-zero diagnostic (the
        // validate gate) disjoint. Mirrors the
        // `parse_duration_accepts_single_zero_magnitude_at_codec_layer`
        // partition pin on the sibling codec.
        assert_eq!(parse_byte_size("0").unwrap(), 0);
        assert_eq!(parse_byte_size("0B").unwrap(), 0);
        assert_eq!(parse_byte_size("0KiB").unwrap(), 0);
        assert_eq!(parse_byte_size("0MiB").unwrap(), 0);
        assert_eq!(parse_byte_size("0GiB").unwrap(), 0);
        assert_eq!(parse_byte_size("0KB").unwrap(), 0);
    }

    #[test]
    fn parse_byte_size_accepts_canonical_magnitude_with_leading_one() {
        // The complement-side pin on the leading-zero arm: magnitudes
        // beginning with `1`..=`9` stay accepted across every canonical
        // unit suffix the codec accepts. Pin this so a future
        // tightening cannot drift into rejecting valid canonical
        // magnitudes — peer with the
        // `parse_duration_accepts_canonical_magnitude_with_leading_one`
        // pin on the sibling codec.
        assert_eq!(parse_byte_size("1").unwrap(), 1);
        assert_eq!(parse_byte_size("1KiB").unwrap(), 1024);
        assert_eq!(parse_byte_size("1MiB").unwrap(), 1024 * 1024);
        assert_eq!(parse_byte_size("1GiB").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_byte_size("64MiB").unwrap(), 64 * 1024 * 1024);
        assert_eq!(parse_byte_size("9").unwrap(), 9);
    }

    #[test]
    fn de_byte_size_rejects_leading_zero_through_serde() {
        // The serde-path pin: a `:limits :memory` carrying a
        // leading-zero magnitude (`"064MiB"`) must fail at deserialize
        // time, not silently round-trip the value through the parser.
        // The gate fires at deserialize, before any validate gate runs
        // — peer with `de_duration_rejects_leading_zero_through_serde`
        // on the sibling codec.
        let json = r#"{"memory":"064MiB"}"#;
        let err = serde_json::from_str::<LimitsSpec>(json).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("leading zero"),
            "serde diagnostic must surface the leading-zero reason verbatim (got {msg:?})"
        );
    }

    // ── canonical-form: whitespace-rejection byte-size codec gate ─────────
    //
    // Direct successor to the `parse_duration` whitespace-rejection arm
    // (ebc3a75), the `supervisor::duration_codec` whitespace-rejection
    // arm (a7ae622), and the `rate_limit_codec` whitespace-rejection arm
    // (1ad7755) on the same canonical-form render-determinism axis. The
    // pre-gate top-level `s.trim()` at parse entry and the per-part
    // `num_part.trim()` / `unit.trim()` calls silently ate leading /
    // trailing / internal whitespace, so every whitespace-carrying
    // shape parsed to the same byte magnitude and round-tripped through
    // `render_byte_size` to a *different* canonical string on next
    // serialize — the same canonical-form-drift class the leading-`+` /
    // fractional / leading-zero arms already close on this codec.
    // `u8::is_ascii_whitespace` covers the five WhatWG-conformant ASCII
    // whitespace bytes (space `0x20`, tab `0x09`, LF `0x0A`, FF `0x0C`,
    // CR `0x0D`). Closes the whitespace-rejection axis across every
    // typed-magnitude codec in caixa-core.

    #[test]
    fn parse_byte_size_rejects_leading_whitespace() {
        // The fail-before-pass-after pin: `" 64MiB"` — the canonical
        // paste-from-aligned-doc / paste-from-YAML-quoted-plain-scalar
        // footgun. Before this gate the top-level `s.trim()` at parse
        // entry silently ate the leading space and parsed the value to
        // 64 * 1024 * 1024 bytes, which then round-tripped through
        // `render_byte_size` to `"64MiB"` (a *different* canonical
        // string on the next emit) — the exact canonical-form-drift
        // class the leading-`+` / leading-zero arms already close,
        // extended to the whitespace-byte class. Peer with the sibling
        // `parse_duration_rejects_leading_whitespace` arm (ebc3a75) on
        // the shared canonical-form-drift trajectory.
        let err = parse_byte_size(" 64MiB").unwrap_err();
        assert!(
            matches!(err, LimitsError::WhitespaceInByteSize { ref value, byte } if value == " 64MiB" && byte == 0x20),
            "got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("whitespace byte 0x20"),
            "diagnostic must surface the offending byte verbatim (got {msg:?})"
        );
        assert!(
            msg.contains("THEORY.md"),
            "diagnostic must cite the render-determinism contract (got {msg:?})"
        );
    }

    #[test]
    fn parse_byte_size_rejects_trailing_whitespace() {
        // `"64MiB "` — the canonical shell-history / trailing-space
        // paste footgun. Before this gate the top-level `s.trim()`
        // silently ate the trailing space and parsed to 64 * 1024 *
        // 1024 bytes, round-tripping to `"64MiB"` on the next emit —
        // same canonical-form drift as the leading-space sibling,
        // closed on the same whitespace-byte arm.
        let err = parse_byte_size("64MiB ").unwrap_err();
        assert!(
            matches!(err, LimitsError::WhitespaceInByteSize { ref value, byte } if value == "64MiB " && byte == 0x20),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_byte_size_rejects_internal_whitespace_between_magnitude_and_unit() {
        // `"64 MiB"` — the canonical typographically-spaced author
        // shape (the same idiom every prose reference to a byte-size
        // renders as, mistakenly retained when the value is pasted
        // into a codec-shaped slot). Before this gate the per-part
        // `num_part.trim()` / `unit.trim()` calls silently ate the
        // whitespace between the magnitude and the unit and parsed the
        // value to 64 * 1024 * 1024 bytes, round-tripping to `"64MiB"`
        // — the codec's *internal* whitespace-tolerance vector,
        // orthogonal to the leading / trailing surface but the same
        // canonical-form-drift class. Pins the arm as strictly
        // stronger than the pre-existing top-level `s.trim()`
        // behavior: it fires on whitespace anywhere in the value, not
        // just at the string boundary.
        let err = parse_byte_size("64 MiB").unwrap_err();
        assert!(
            matches!(err, LimitsError::WhitespaceInByteSize { ref value, byte } if value == "64 MiB" && byte == 0x20),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_byte_size_rejects_tab_byte() {
        // `"\t64MiB"` — the canonical paste-from-indented-doc /
        // paste-from-YAML-block-scalar footgun where a tab byte leads
        // the magnitude. Pins that the gate covers tab (`0x09`) as
        // well as space (`0x20`) — both are `u8::is_ascii_whitespace`
        // members and both would be silently swallowed by `s.trim()`
        // pre-gate. The `is_ascii_whitespace` coverage extends beyond
        // space alone to the full ASCII-whitespace set (space `0x20`,
        // tab `0x09`, LF `0x0A`, FF `0x0C`, CR `0x0D`); this test pins
        // the tab arm as a representative of the non-space members.
        let err = parse_byte_size("\t64MiB").unwrap_err();
        assert!(
            matches!(err, LimitsError::WhitespaceInByteSize { ref value, byte } if value == "\t64MiB" && byte == 0x09),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_byte_size_rejects_trailing_newline() {
        // `"64MiB\n"` — the canonical multi-line-paste footgun where
        // a trailing LF byte survives the paste. Pins the LF member
        // (`0x0A`) of the `is_ascii_whitespace` set as a peer to the
        // space and tab pins above — every non-space non-tab
        // whitespace byte the WhatWG ASCII-whitespace set covers is
        // refused by the same arm.
        let err = parse_byte_size("64MiB\n").unwrap_err();
        assert!(
            matches!(err, LimitsError::WhitespaceInByteSize { ref value, byte } if value == "64MiB\n" && byte == 0x0a),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_byte_size_accepts_whitespace_free_canonical_forms() {
        // The complement-side pin: every canonical whitespace-free
        // authoring form the renderer emits stays accepted post-gate.
        // Sweep the canonical unit suffixes plus the bare-integer
        // shorthand so a future tightening of the whitespace arm that
        // over-fires on the accepted set surfaces here as a test
        // failure. Peer with the
        // `parse_duration_accepts_whitespace_free_canonical_forms` pin
        // on the sibling codec.
        assert_eq!(parse_byte_size("64MiB").unwrap(), 64 * 1024 * 1024);
        assert_eq!(parse_byte_size("1GiB").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_byte_size("512KiB").unwrap(), 512 * 1024);
        assert_eq!(parse_byte_size("1KB").unwrap(), 1_000);
        assert_eq!(parse_byte_size("1024").unwrap(), 1024);
        assert_eq!(parse_byte_size("0").unwrap(), 0);
    }

    #[test]
    fn de_byte_size_rejects_whitespace_through_serde() {
        // The serde-path pin: a `:limits :memory` carrying a
        // whitespace-byte-carrying value (`" 64MiB"`) must fail at
        // deserialize time, not silently round-trip the value through
        // the pre-existing top-level `s.trim()`. The gate fires at
        // deserialize, before any validate gate runs — peer with the
        // existing `de_byte_size_rejects_leading_zero_through_serde` /
        // `de_duration_rejects_whitespace_through_serde` pins on the
        // same canonical-form-drift axis.
        let json = r#"{"memory":" 64MiB"}"#;
        let err = serde_json::from_str::<LimitsSpec>(json).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("whitespace byte"),
            "serde diagnostic must surface the whitespace reason verbatim (got {msg:?})"
        );
        assert!(
            msg.contains("0x20"),
            "serde diagnostic must name the offending byte (got {msg:?})"
        );

        // The whitespace-free complement — same author-side intent,
        // written in the canonical form the renderer would emit,
        // deserializes cleanly.
        let json = r#"{"memory":"64MiB"}"#;
        let l: LimitsSpec = serde_json::from_str(json).unwrap();
        assert_eq!(l.memory, Some(64 * 1024 * 1024));
    }

    // ── canonical-form: non-ASCII Unicode `White_Space` byte-size gate ────
    //
    // Direct successor to the `parse_byte_size` ASCII-whitespace arm
    // (24a8ad4) — closes the strictly-complementary class the byte-scan
    // above cannot see. `str::trim` uses `char::is_whitespace` (Unicode
    // `White_Space`, strictly wider than the ASCII byte set); a leading /
    // trailing / internal NBSP (`\u{00A0}`) / LINE SEPARATOR (`\u{2028}`)
    // / EM-SPACE (`\u{2003}`) survives the byte-scan but is silently
    // stripped by the top-level trim, drifting to canonical `"64MiB"` on
    // round-trip. Pins the arm through the lifted
    // [`crate::render::find_non_ascii_whitespace_char`] predicate.

    #[test]
    fn parse_byte_size_rejects_leading_nbsp() {
        // NBSP (`\u{00A0}` = UTF-8 `0xC2 0xA0`) — the canonical
        // paste-from-typography / paste-from-word-processor footgun.
        // Before this arm landed the byte-scan missed it (neither `0xC2`
        // nor `0xA0` is `is_ascii_whitespace`) and `str::trim` at parse
        // entry silently stripped it, yielding the same `64 * 1024 *
        // 1024` bytes as the whitespace-free canonical form and drifting
        // to `"64MiB"` on next serialize.
        let s = "\u{00A0}64MiB";
        let err = parse_byte_size(s).unwrap_err();
        assert!(
            matches!(err, LimitsError::NonAsciiWhitespaceInByteSize { ref value, ch, codepoint } if value == s && ch == '\u{00A0}' && codepoint == 0x00A0),
            "got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("U+00A0"),
            "diagnostic must surface the codepoint verbatim (got {msg:?})"
        );
        assert!(
            msg.contains("THEORY.md"),
            "diagnostic must cite the render-determinism contract (got {msg:?})"
        );
    }

    #[test]
    fn parse_byte_size_rejects_internal_line_separator() {
        // LINE SEPARATOR (`\u{2028}`) between magnitude and unit — the
        // canonical paste-from-web-doc footgun (many rendering engines
        // insert `\u{2028}` at soft-wrap boundaries in RTF/HTML → plain
        // text conversion). Pins the arm on a non-space non-NBSP Unicode
        // `White_Space` member.
        let s = "64\u{2028}MiB";
        let err = parse_byte_size(s).unwrap_err();
        assert!(
            matches!(err, LimitsError::NonAsciiWhitespaceInByteSize { ref value, ch, codepoint } if value == s && ch == '\u{2028}' && codepoint == 0x2028),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_byte_size_rejects_trailing_ideographic_space() {
        // IDEOGRAPHIC SPACE (`\u{3000}`) — the CJK-typography paste
        // footgun (canonical U+3000 is the full-width space that
        // Japanese / Chinese IMEs emit when input is auto-widened). Pins
        // the arm at the top edge of the `char::is_whitespace` set.
        let s = "64MiB\u{3000}";
        let err = parse_byte_size(s).unwrap_err();
        assert!(
            matches!(err, LimitsError::NonAsciiWhitespaceInByteSize { ref value, ch, codepoint } if value == s && ch == '\u{3000}' && codepoint == 0x3000),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_byte_size_accepts_ascii_only_canonical_forms_after_unicode_arm() {
        // Positive-control pin: every ASCII-only canonical form the
        // renderer emits stays accepted through the new arm — the
        // lifted predicate is a strict no-op on ASCII input.
        assert_eq!(parse_byte_size("64MiB").unwrap(), 64 * 1024 * 1024);
        assert_eq!(parse_byte_size("1GiB").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_byte_size("512KiB").unwrap(), 512 * 1024);
        assert_eq!(parse_byte_size("1024").unwrap(), 1024);
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
        // value the parser accepts round-trips through the canonical
        // [`crate::supervisor::duration_codec::render`] primitive to a
        // string the parser also accepts — and to the *same* value.
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
            let rendered = crate::supervisor::duration_codec::render(d);
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

    // ── canonical-form: leading-zero duration codec gate ─────────────────
    //
    // Direct successor to the `supervisor::duration_codec` leading-zero
    // arm (9178904) and the `rate_limit_codec` leading-zero arm (4f46830)
    // — closes the leading-zero canonical-form-drift class on the
    // `:limits :wall-clock` codec. Every magnitude `render_duration`
    // emits is a non-negative integer with no leading-zero padding; the
    // parser's accepted set must match for parse → render → parse to
    // round-trip without canonical-form drift. The single-byte `"0"`
    // round-trips losslessly (`render_duration(Duration::ZERO)` emits
    // `"0s"`) and the downstream [`LimitsError::WallClockZero`] gate
    // refuses zero-magnitude authoring at the typed-validate layer above
    // — the codec-layer / typed-validate-layer partition is what keeps
    // the diagnostic partitioning stable.

    #[test]
    fn parse_duration_rejects_leading_zero_magnitude() {
        // The fail-before-pass-after pin: `"030s"` parsed cleanly on
        // every pre-gate codebase (`u64::from_str` accepts the leading
        // zero), the codec produced 30s, and `render_duration(30s)`
        // emitted `"30s"` on the next serialize — silently dropping
        // the leading zero and drifting the canonical form away from
        // the author's intent. The new gate surfaces the round-trip
        // break at the parser layer with a self-locating diagnostic.
        let err = parse_duration("030s").unwrap_err();
        assert!(
            matches!(err, LimitsError::LeadingZeroDurationMagnitude { ref value } if value == "030"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_duration_rejects_multi_digit_zero_magnitude() {
        // `"00s"` is the degenerate leading-zero case — every byte is
        // `0`. `u64::from_str("00")` = 0, and the codec produces
        // `Duration::ZERO`; `render_duration(Duration::ZERO)` emits
        // `"0s"` on the next serialize — drift from `"00s"` to `"0s"`.
        // The leading-zero arm refuses the drift class at the codec
        // layer while leaving the canonical single-byte `"0s"` accepted.
        let err = parse_duration("00s").unwrap_err();
        assert!(
            matches!(err, LimitsError::LeadingZeroDurationMagnitude { ref value } if value == "00"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_duration_rejects_leading_zero_in_hour_window() {
        // `"01h"` parses to 1h; the renderer emits `"1h"` on the next
        // serialize. The leading-zero class is a property of the
        // magnitude, not the unit — pin a per-hour magnitude alongside
        // the per-second / per-ms pins so the gate's coverage is
        // structural across every canonical unit suffix the codec
        // accepts. Mirrors the `_per_hour_window` pin the
        // `supervisor::duration_codec` and `rate_limit_codec` leading-
        // zero arms carry on the peer codecs.
        let err = parse_duration("01h").unwrap_err();
        assert!(
            matches!(err, LimitsError::LeadingZeroDurationMagnitude { ref value } if value == "01"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_duration_rejects_leading_zero_bare_integer_as_seconds() {
        // The bare-integer-as-seconds shorthand (`"30"` → 30s, no unit
        // suffix because the parser routes the empty `unit` slot to
        // `Duration::from_secs`) inherits the leading-zero arm: `"030"`
        // parses losslessly to 30s but `render_duration(30s)` emits
        // `"30s"` on the next serialize. Pin the bare-integer path so a
        // future relaxation that special-cases the unitless shorthand
        // surfaces here as a test failure.
        let err = parse_duration("030").unwrap_err();
        assert!(
            matches!(err, LimitsError::LeadingZeroDurationMagnitude { ref value } if value == "030"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_duration_accepts_single_zero_magnitude_at_codec_layer() {
        // The codec-layer / typed-validate-layer boundary pin: the
        // single-byte `"0"` magnitude round-trips losslessly through
        // `render_duration` (`render_duration(Duration::ZERO)` emits
        // `"0s"`), so it stays accepted at this codec layer across
        // every canonical unit suffix. The downstream
        // `LimitsError::WallClockZero` gate is what refuses
        // zero-magnitude authoring at the typed-validate layer above
        // — the partition keeps the canonical-form-drift diagnostic
        // (this arm) and the semantic-zero diagnostic (the validate
        // gate) disjoint.
        assert_eq!(parse_duration("0s").unwrap(), Duration::ZERO);
        assert_eq!(parse_duration("0ms").unwrap(), Duration::ZERO);
        assert_eq!(parse_duration("0m").unwrap(), Duration::ZERO);
        assert_eq!(parse_duration("0h").unwrap(), Duration::ZERO);
        assert_eq!(parse_duration("0").unwrap(), Duration::ZERO);
    }

    #[test]
    fn parse_duration_accepts_canonical_magnitude_with_leading_one() {
        // The complement-side pin on the leading-zero arm: magnitudes
        // beginning with `1`..=`9` stay accepted across every canonical
        // unit suffix the codec accepts. Pin this so a future
        // tightening cannot drift into rejecting valid canonical
        // magnitudes — peer with the `_accepts_canonical_magnitude_with_leading_one`
        // pin the `supervisor::duration_codec` and `rate_limit_codec`
        // leading-zero arms carry.
        assert_eq!(parse_duration("1ms").unwrap(), Duration::from_millis(1));
        assert_eq!(parse_duration("1s").unwrap(), Duration::from_secs(1));
        assert_eq!(parse_duration("1m").unwrap(), Duration::from_secs(60));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_duration("100ms").unwrap(), Duration::from_millis(100));
        assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
    }

    // ── canonical-form: whitespace-rejection duration codec gate ─────────
    //
    // Direct successor to the `supervisor::duration_codec` whitespace-
    // rejection arm (a7ae622) and the `rate_limit_codec` whitespace-
    // rejection arm (1ad7755) on the same canonical-form
    // render-determinism axis. The pre-gate top-level `s.trim()` at
    // parse entry and the per-part `num_part.trim()` / `unit.trim()`
    // calls silently ate leading / trailing / internal whitespace, so
    // every whitespace-carrying shape parsed to the same integer
    // magnitude and round-tripped through `render_duration` to a
    // *different* canonical string on next serialize — the same
    // canonical-form-drift class the leading-`+` / fractional /
    // leading-zero arms already close on this codec. `u8::is_ascii_whitespace`
    // covers the five WhatWG-conformant ASCII whitespace bytes
    // (space `0x20`, tab `0x09`, LF `0x0A`, FF `0x0C`, CR `0x0D`).

    #[test]
    fn parse_duration_rejects_leading_whitespace() {
        // The fail-before-pass-after pin: `" 30s"` — the canonical
        // paste-from-aligned-doc / paste-from-YAML-quoted-plain-scalar
        // footgun. Before this gate the top-level `s.trim()` at parse
        // entry silently ate the leading space and parsed the value to
        // `Duration::from_secs(30)`, which then round-tripped through
        // `render_duration` to `"30s"` (a *different* canonical string
        // on the next emit) — the exact canonical-form-drift class the
        // leading-`+` / leading-zero arms already close, extended to
        // the whitespace-byte class. Peer with the sibling
        // `supervisor::duration_codec` `parse_rejects_leading_whitespace`
        // arm (a7ae622) on the shared duration-codec trajectory.
        let err = parse_duration(" 30s").unwrap_err();
        assert!(
            matches!(err, LimitsError::WhitespaceInDuration { ref value, byte } if value == " 30s" && byte == 0x20),
            "got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("whitespace byte 0x20"),
            "diagnostic must surface the offending byte verbatim (got {msg:?})"
        );
        assert!(
            msg.contains("THEORY.md"),
            "diagnostic must cite the render-determinism contract (got {msg:?})"
        );
    }

    #[test]
    fn parse_duration_rejects_trailing_whitespace() {
        // `"30s "` — the canonical shell-history / trailing-space paste
        // footgun. Before this gate the top-level `s.trim()` silently
        // ate the trailing space and parsed to `Duration::from_secs(30)`,
        // round-tripping to `"30s"` on the next emit — same canonical-
        // form drift as the leading-space sibling, closed on the same
        // whitespace-byte arm.
        let err = parse_duration("30s ").unwrap_err();
        assert!(
            matches!(err, LimitsError::WhitespaceInDuration { ref value, byte } if value == "30s " && byte == 0x20),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_duration_rejects_internal_whitespace_between_magnitude_and_unit() {
        // `"30 s"` — the canonical typographically-spaced author shape
        // (the same idiom every prose reference to a duration renders as,
        // mistakenly retained when the value is pasted into a codec-
        // shaped slot). Before this gate the per-part `num_part.trim()`
        // / `unit.trim()` calls silently ate the whitespace between the
        // magnitude and the unit and parsed the value to
        // `Duration::from_secs(30)`, round-tripping to `"30s"` — the
        // codec's *internal* whitespace-tolerance vector, orthogonal
        // to the leading / trailing surface but the same canonical-
        // form-drift class. Pins the arm as strictly stronger than the
        // pre-existing top-level `s.trim()` behavior: it fires on
        // whitespace anywhere in the value, not just at the string
        // boundary.
        let err = parse_duration("30 s").unwrap_err();
        assert!(
            matches!(err, LimitsError::WhitespaceInDuration { ref value, byte } if value == "30 s" && byte == 0x20),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_duration_rejects_tab_byte() {
        // `"\t30s"` — the canonical paste-from-indented-doc /
        // paste-from-YAML-block-scalar footgun where a tab byte leads
        // the magnitude. Pins that the gate covers tab (`0x09`) as well
        // as space (`0x20`) — both are `u8::is_ascii_whitespace` members
        // and both would be silently swallowed by `s.trim()` pre-gate.
        // The `is_ascii_whitespace` coverage extends beyond space alone
        // to the full ASCII-whitespace set (space `0x20`, tab `0x09`,
        // LF `0x0A`, FF `0x0C`, CR `0x0D`); this test pins the tab arm
        // as a representative of the non-space members.
        let err = parse_duration("\t30s").unwrap_err();
        assert!(
            matches!(err, LimitsError::WhitespaceInDuration { ref value, byte } if value == "\t30s" && byte == 0x09),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_duration_rejects_trailing_newline() {
        // `"30s\n"` — the canonical multi-line-paste footgun where a
        // trailing LF byte survives the paste. Pins the LF member
        // (`0x0A`) of the `is_ascii_whitespace` set as a peer to the
        // space and tab pins above — every non-space non-tab whitespace
        // byte the WhatWG ASCII-whitespace set covers is refused by
        // the same arm.
        let err = parse_duration("30s\n").unwrap_err();
        assert!(
            matches!(err, LimitsError::WhitespaceInDuration { ref value, byte } if value == "30s\n" && byte == 0x0a),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_duration_accepts_whitespace_free_canonical_forms() {
        // The complement-side pin: every canonical whitespace-free
        // authoring form the renderer emits stays accepted post-gate.
        // Sweep the canonical unit suffixes plus the bare-integer
        // shorthand so a future tightening of the whitespace arm that
        // over-fires on the accepted set surfaces here as a test
        // failure. Peer with the `parse_duration_continues_to_accept_integer_magnitudes`
        // pin the fractional / leading-`+` gate carries.
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_duration("2m").unwrap(), Duration::from_secs(120));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_duration("0s").unwrap(), Duration::ZERO);
        assert_eq!(parse_duration("3600").unwrap(), Duration::from_secs(3600));
    }

    #[test]
    fn de_duration_rejects_whitespace_through_serde() {
        // The serde-path pin: a `:limits :wall-clock` carrying a
        // whitespace-byte-carrying value (`" 30s"`) must fail at
        // deserialize time, not silently round-trip the value through
        // the pre-existing top-level `s.trim()`. The gate fires at
        // deserialize, before any validate gate runs — peer with the
        // existing `de_duration_rejects_leading_zero_through_serde` /
        // `de_duration_rejects_fractional_value_through_serde` pins on
        // the same canonical-form-drift axis.
        let json = r#"{"wallClock":" 30s"}"#;
        let err = serde_json::from_str::<LimitsSpec>(json).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("whitespace byte"),
            "serde diagnostic must surface the whitespace reason verbatim (got {msg:?})"
        );
        assert!(
            msg.contains("0x20"),
            "serde diagnostic must name the offending byte (got {msg:?})"
        );

        // The whitespace-free complement — same author-side intent,
        // written in the canonical form the renderer would emit,
        // deserializes cleanly.
        let json = r#"{"wallClock":"30s"}"#;
        let l: LimitsSpec = serde_json::from_str(json).unwrap();
        assert_eq!(l.wall_clock, Some(Duration::from_secs(30)));
    }

    // ── canonical-form: non-ASCII Unicode `White_Space` duration gate ─────
    //
    // Successor to the `parse_duration` ASCII-whitespace arm (ebc3a75)
    // — closes the strictly-complementary class the byte-scan cannot
    // see, through the lifted
    // [`crate::render::find_non_ascii_whitespace_char`] predicate.

    #[test]
    fn parse_duration_rejects_leading_nbsp() {
        // NBSP prefix — paste-from-typography footgun. Byte-scan misses,
        // `str::trim` strips silently, drifting to `"30s"` on next
        // emit.
        let s = "\u{00A0}30s";
        let err = parse_duration(s).unwrap_err();
        assert!(
            matches!(err, LimitsError::NonAsciiWhitespaceInDuration { ref value, ch, codepoint } if value == s && ch == '\u{00A0}' && codepoint == 0x00A0),
            "got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("U+00A0"),
            "diagnostic must name codepoint (got {msg:?})"
        );
    }

    #[test]
    fn parse_duration_rejects_internal_em_space() {
        // EM-SPACE (`\u{2003}`) between magnitude and unit — canonical
        // paste-from-typography footgun on the `<integer><unit>` shape.
        let s = "30\u{2003}s";
        let err = parse_duration(s).unwrap_err();
        assert!(
            matches!(err, LimitsError::NonAsciiWhitespaceInDuration { ref value, ch, codepoint } if value == s && ch == '\u{2003}' && codepoint == 0x2003),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_duration_accepts_ascii_only_canonical_forms_after_unicode_arm() {
        // Positive-control pin: every ASCII-only canonical form the
        // renderer emits stays accepted through the new arm.
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
    }

    #[test]
    fn de_duration_rejects_leading_zero_through_serde() {
        // The serde-path pin: a `:limits :wall-clock` carrying a
        // leading-zero magnitude (`"030s"`) must fail at deserialize
        // time, not silently round-trip the value through the parser.
        // The gate fires at deserialize, before any validate gate runs
        // — peer with the existing `de_duration_rejects_fractional_value_through_serde`
        // pin on the same canonical-form-drift axis.
        let json = r#"{"wallClock":"030s"}"#;
        let err = serde_json::from_str::<LimitsSpec>(json).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("leading zero"),
            "serde diagnostic must surface the leading-zero reason verbatim (got {msg:?})"
        );

        let json = r#"{"wallClock":"30s"}"#;
        let l: LimitsSpec = serde_json::from_str(json).unwrap();
        assert_eq!(l.wall_clock, Some(Duration::from_secs(30)));
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

    // ── canonical-form: leading-zero millicores codec gate ────────────────
    //
    // Direct successor to the `parse_byte_size` / `parse_duration` /
    // `supervisor::duration_codec` / `rate_limit_codec` leading-zero
    // arms (cea9a78 / 39762d7 / 9178904 / 4f46830) — closes the sixth
    // (and last) typed numeric-codec surface in caixa-core on the
    // integer-magnitude leading-zero axis. Every magnitude
    // `render_millicores` emits is the leading-zero-stripped form
    // (`format!("{m}m")` — no leading-zero padding), so a digit-only-
    // but-leading-zero magnitude parses losslessly through `u32::from_str`
    // and serde silently round-trips the value to a *different*
    // canonical string on the next emit. Pins every canonical-drift
    // shape on the `m`-suffix and bare-core paths, the codec-vs-
    // typed-validate-layer boundary (the single-byte `"0"` stays in the
    // codec's accepted set; `CpuZero` refuses it at validate), the
    // complement-side pin (every canonical leading-`[1-9]` magnitude
    // continues to parse cleanly), and the serde-path pin.

    #[test]
    fn parse_millicores_rejects_leading_zero_magnitude_with_suffix() {
        // The fail-before-pass-after pin on the `m`-suffix path:
        // `"0500m"` parsed cleanly on no pre-gate codebase
        // (`u32::from_str` accepts `"0500"` → 500), then `render_millicores`
        // emitted `"500m"` on the next serialize — canonical-form drift.
        // The leading-zero arm routes the same input to
        // `LeadingZeroMillicoreMagnitude` with the canonical-form
        // remediation wording. Peer with the `parse_byte_size` `"064MiB"`
        // case and the `parse_duration` `"030s"` case.
        let err = parse_millicores("0500m").unwrap_err();
        assert!(
            matches!(err, LimitsError::LeadingZeroMillicoreMagnitude { ref value } if value == "0500"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_millicores_rejects_multi_digit_zero_magnitude_with_suffix() {
        // The multi-zero pin on the `m`-suffix path: `"00m"` parses to 0
        // millicores at the codec, but the renderer emits `"0m"` on the
        // next serialize — the single canonical zero form on this axis.
        // The leading-zero arm rejects multi-byte leading-zero shapes
        // even when the value is zero; the single-byte `"0m"` /
        // bare-`"0"` stays in the codec's accepted set per the boundary
        // pin below. Peer with the `parse_byte_size` `"00MiB"` case and
        // the `parse_duration` `"00s"` case.
        let err = parse_millicores("00m").unwrap_err();
        assert!(
            matches!(err, LimitsError::LeadingZeroMillicoreMagnitude { ref value } if value == "00"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_millicores_rejects_leading_zero_bare_core() {
        // The leading-zero pin on the bare-core path: `"02"` parsed to
        // 2 cores → 2000 millicores at the codec, but `render_millicores`
        // emits `"2000m"` on the next serialize — canonical-form drift.
        // The bare-core shorthand carries the same leading-zero discipline
        // as the `m`-suffix path; both authoring paths converge to the
        // same gate. Peer with the `parse_byte_size` bare-integer
        // `"01024"` case.
        let err = parse_millicores("02").unwrap_err();
        assert!(
            matches!(err, LimitsError::LeadingZeroMillicoreMagnitude { ref value } if value == "02"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_millicores_rejects_leading_zero_multi_digit_with_suffix() {
        // The multi-digit leading-zero pin on the `m`-suffix path:
        // `"01500m"` parses to 1500 millicores at the codec, but the
        // renderer emits `"1500m"` on the next serialize — canonical-form
        // drift on a non-zero magnitude. Sweeps a different magnitude
        // shape than the `"0500m"` case so a future tightening that
        // misses the multi-digit-leading-zero class surfaces here.
        let err = parse_millicores("01500m").unwrap_err();
        assert!(
            matches!(err, LimitsError::LeadingZeroMillicoreMagnitude { ref value } if value == "01500"),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_millicores_accepts_single_zero_magnitude_at_codec_layer() {
        // The codec-layer / typed-validate-layer boundary pin: the
        // single-byte magnitude `"0"` (bare) and `"0m"` (with suffix)
        // round-trip losslessly through `render_millicores` (which
        // emits `"0m"` for 0 millicores), so they stay in the codec's
        // accepted set. The downstream `CpuZero` gate refuses
        // semantic-zero authoring at the typed-validate layer above —
        // the diagnostic partitioning between canonical-form drift
        // (the leading-zero arm) and semantic-zero (the `CpuZero` gate)
        // remains stable. Same codec-layer / typed-validate-layer
        // partition the peer codecs preserve.
        assert_eq!(parse_millicores("0").unwrap(), 0);
        assert_eq!(parse_millicores("0m").unwrap(), 0);
    }

    #[test]
    fn parse_millicores_accepts_canonical_magnitude_with_leading_one() {
        // The complement-side pin: every canonical leading-`[1-9]`
        // magnitude continues to parse cleanly through the leading-zero
        // arm, on both the `m`-suffix and bare-core paths. Sweep the
        // canonical values the renderer emits across the unit-multiplier
        // boundary (sub-core, single-core, multi-core) so a future
        // tightening cannot drift into rejecting valid canonical
        // magnitudes. Same complement-side discipline the peer
        // `parse_byte_size_accepts_canonical_magnitude_with_leading_one`
        // and `parse_duration_accepts_canonical_magnitude_with_leading_one`
        // pins enforce on the sibling codecs.
        assert_eq!(parse_millicores("1m").unwrap(), 1);
        assert_eq!(parse_millicores("500m").unwrap(), 500);
        assert_eq!(parse_millicores("1500m").unwrap(), 1500);
        assert_eq!(parse_millicores("9000m").unwrap(), 9000);
        assert_eq!(parse_millicores("1").unwrap(), 1000);
        assert_eq!(parse_millicores("2").unwrap(), 2000);
        assert_eq!(parse_millicores("9").unwrap(), 9000);
    }

    #[test]
    fn de_millicores_rejects_leading_zero_through_serde() {
        // The serde-path pin: a `:limits :cpu` carrying a leading-zero
        // magnitude (`"0500m"`) must fail at deserialize time, not
        // silently round-trip the value through `u32::from_str`'s
        // leading-zero-permissive accepting. Pin both the success-on-
        // canonical path (the leading-zero-stripped form deserializes
        // cleanly) and the failure-on-non-canonical path (the leading-
        // zero form is rejected by the codec before any validate gate
        // runs). Peer with the
        // `de_byte_size_rejects_leading_zero_through_serde` and
        // `de_duration_rejects_leading_zero_through_serde` pins on the
        // sibling codecs.
        let json = r#"{"cpu":"0500m"}"#;
        let err = serde_json::from_str::<LimitsSpec>(json).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("leading zero"),
            "serde diagnostic must surface the leading-zero reason verbatim \
             (got {msg:?})"
        );

        // The integer-form complement — same author-side intent
        // (500 millicores), written in the canonical form the renderer
        // would emit, deserializes cleanly.
        let json = r#"{"cpu":"500m"}"#;
        let l: LimitsSpec = serde_json::from_str(json).unwrap();
        assert_eq!(l.cpu, Some(500));
    }

    // ── canonical-form: whitespace-rejection millicores codec gate ────────
    //
    // Direct successor to the `parse_byte_size` (24a8ad4), `parse_duration`
    // (ebc3a75), `supervisor::duration_codec` (a7ae622), and
    // `rate_limit_codec` (1ad7755) whitespace-rejection arms — closes the
    // fifth (and last) typed-magnitude codec surface in caixa-core on the
    // ASCII-whitespace axis. The pre-gate top-level `s.trim()` at parse
    // entry and the per-part `magnitude.trim()` calls silently ate leading
    // / trailing / internal whitespace, so every whitespace-carrying shape
    // parsed to the same millicore value and round-tripped through
    // `render_millicores` to a *different* canonical string on next
    // serialize — the same canonical-form-drift class the leading-`+` /
    // fractional / leading-zero arms already close on this codec.

    #[test]
    fn parse_millicores_rejects_leading_whitespace() {
        // `" 500m"` — the canonical paste-from-aligned-doc / YAML-quoted-
        // plain-scalar footgun. Before this gate the top-level `s.trim()`
        // at parse entry silently ate the leading space and parsed the
        // value to 500 millicores, round-tripping to `"500m"` on next
        // serialize.
        let err = parse_millicores(" 500m").unwrap_err();
        assert!(
            matches!(err, LimitsError::WhitespaceInMillicores { ref value, byte } if value == " 500m" && byte == 0x20),
            "got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("whitespace byte 0x20"),
            "diagnostic must surface the offending byte verbatim (got {msg:?})"
        );
        assert!(
            msg.contains("THEORY.md"),
            "diagnostic must cite the render-determinism contract (got {msg:?})"
        );
    }

    #[test]
    fn parse_millicores_rejects_trailing_whitespace() {
        // `"500m "` — the canonical shell-history trailing-space footgun.
        let err = parse_millicores("500m ").unwrap_err();
        assert!(
            matches!(err, LimitsError::WhitespaceInMillicores { ref value, byte } if value == "500m " && byte == 0x20),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_millicores_rejects_internal_whitespace_between_magnitude_and_unit() {
        // `"500 m"` — the typographically-spaced author shape (the same
        // idiom every prose reference to millicores renders as). Before
        // this gate the per-part `magnitude.trim()` silently ate the
        // internal space and parsed the value to 500 millicores.
        let err = parse_millicores("500 m").unwrap_err();
        assert!(
            matches!(err, LimitsError::WhitespaceInMillicores { ref value, byte } if value == "500 m" && byte == 0x20),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_millicores_rejects_tab_byte() {
        // `"\t500m"` — the paste-from-indented-doc / YAML-block-scalar tab
        // footgun. Pins the tab (`0x09`) arm alongside the space arm above.
        let err = parse_millicores("\t500m").unwrap_err();
        assert!(
            matches!(err, LimitsError::WhitespaceInMillicores { ref value, byte } if value == "\t500m" && byte == 0x09),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_millicores_rejects_trailing_newline() {
        // `"500m\n"` — the multi-line-paste footgun where a trailing LF
        // byte survives the paste. Pins the LF member (`0x0A`) of the
        // `is_ascii_whitespace` set.
        let err = parse_millicores("500m\n").unwrap_err();
        assert!(
            matches!(err, LimitsError::WhitespaceInMillicores { ref value, byte } if value == "500m\n" && byte == 0x0a),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_millicores_accepts_whitespace_free_canonical_forms() {
        // The complement-side pin: every canonical whitespace-free
        // authoring form the renderer emits stays accepted post-gate.
        // Sweep the canonical `m`-suffix path plus the bare-core shorthand
        // so a future tightening of the whitespace arm that over-fires on
        // the accepted set surfaces here as a test failure.
        assert_eq!(parse_millicores("500m").unwrap(), 500);
        assert_eq!(parse_millicores("2000m").unwrap(), 2000);
        assert_eq!(parse_millicores("1m").unwrap(), 1);
        assert_eq!(parse_millicores("0m").unwrap(), 0);
        assert_eq!(parse_millicores("2").unwrap(), 2000);
        assert_eq!(parse_millicores("0").unwrap(), 0);
    }

    #[test]
    fn de_millicores_rejects_whitespace_through_serde() {
        // The serde-path pin: a `:limits :cpu` carrying a whitespace-byte-
        // carrying value (`" 500m"`) must fail at deserialize time, not
        // silently round-trip the value through the pre-existing top-level
        // `s.trim()`. Peer with the
        // `de_byte_size_rejects_whitespace_through_serde` and
        // `de_duration_rejects_whitespace_through_serde` pins on the
        // sibling codecs.
        let json = r#"{"cpu":" 500m"}"#;
        let err = serde_json::from_str::<LimitsSpec>(json).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("whitespace byte"),
            "serde diagnostic must surface the whitespace reason verbatim (got {msg:?})"
        );
        assert!(
            msg.contains("0x20"),
            "serde diagnostic must name the offending byte (got {msg:?})"
        );

        // The whitespace-free complement — same author-side intent,
        // written in the canonical form the renderer would emit,
        // deserializes cleanly.
        let json = r#"{"cpu":"500m"}"#;
        let l: LimitsSpec = serde_json::from_str(json).unwrap();
        assert_eq!(l.cpu, Some(500));
    }

    // ── canonical-form: non-ASCII Unicode `White_Space` millicores gate ───
    //
    // Direct successor to the ASCII-whitespace arm above — closes the
    // strictly-complementary class the byte-scan cannot see. `str::trim`
    // uses `char::is_whitespace` (Unicode `White_Space`, strictly wider
    // than the ASCII byte set); a leading / trailing / internal NBSP
    // (`\u{00A0}`) / LINE SEPARATOR (`\u{2028}`) / EM-SPACE (`\u{2003}`)
    // survives the byte-scan but is silently stripped by the top-level
    // trim, drifting to canonical `"500m"` on round-trip. Pins the arm
    // through the lifted [`crate::render::find_non_ascii_whitespace_char`]
    // predicate — the same shared predicate 1b75b38 landed on the four
    // peer typed-magnitude codecs, extended here to the fifth.

    #[test]
    fn parse_millicores_rejects_leading_nbsp() {
        // NBSP (`\u{00A0}` = UTF-8 `0xC2 0xA0`) — the paste-from-typography
        // / paste-from-word-processor footgun. Before this arm landed the
        // byte-scan missed it (neither `0xC2` nor `0xA0` is
        // `is_ascii_whitespace`) and `str::trim` at parse entry silently
        // stripped it, yielding the same 500 millicores as the whitespace-
        // free canonical form and drifting to `"500m"` on next serialize.
        let s = "\u{00A0}500m";
        let err = parse_millicores(s).unwrap_err();
        assert!(
            matches!(err, LimitsError::NonAsciiWhitespaceInMillicores { ref value, ch, codepoint } if value == s && ch == '\u{00A0}' && codepoint == 0x00A0),
            "got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("U+00A0"),
            "diagnostic must surface the codepoint verbatim (got {msg:?})"
        );
        assert!(
            msg.contains("THEORY.md"),
            "diagnostic must cite the render-determinism contract (got {msg:?})"
        );
    }

    #[test]
    fn parse_millicores_rejects_internal_em_space() {
        // EM-SPACE (`\u{2003}`) between magnitude and unit — pins the arm
        // on an internal-position non-NBSP Unicode `White_Space` member.
        let s = "500\u{2003}m";
        let err = parse_millicores(s).unwrap_err();
        assert!(
            matches!(err, LimitsError::NonAsciiWhitespaceInMillicores { ref value, ch, codepoint } if value == s && ch == '\u{2003}' && codepoint == 0x2003),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_millicores_rejects_trailing_line_separator() {
        // LINE SEPARATOR (`\u{2028}`) — the canonical paste-from-web-doc
        // footgun (many rendering engines insert `\u{2028}` at soft-wrap
        // boundaries in RTF/HTML → plain text conversion). Pins the arm on
        // a trailing-position Unicode `White_Space` member.
        let s = "500m\u{2028}";
        let err = parse_millicores(s).unwrap_err();
        assert!(
            matches!(err, LimitsError::NonAsciiWhitespaceInMillicores { ref value, ch, codepoint } if value == s && ch == '\u{2028}' && codepoint == 0x2028),
            "got {err:?}"
        );
    }

    #[test]
    fn parse_millicores_accepts_ascii_only_canonical_forms_after_unicode_arm() {
        // Positive-control pin: every ASCII-only canonical form the
        // renderer emits stays accepted through the new arm — the lifted
        // predicate is a strict no-op on ASCII input.
        assert_eq!(parse_millicores("500m").unwrap(), 500);
        assert_eq!(parse_millicores("2000m").unwrap(), 2000);
        assert_eq!(parse_millicores("1m").unwrap(), 1);
        assert_eq!(parse_millicores("2").unwrap(), 2000);
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

    // ── value-shape: :fuel upper bound — 10^12 no-op-budget ceiling ────────
    //
    // The fourth and final `LimitsSpec` axis brought to a top-edge
    // cap, closing the open edge the 857dfcc CPU-cap commit body
    // explicitly named: "three of the four axes carry a top-and-bottom
    // edge gate; only `:fuel` remains with a zero-floor-only shape."
    // Mirrors the test discipline every sibling capped axis carries:
    // the fail-before-pass-after pin, the one-instruction-boundary
    // pin, the far-above-cap sweep, the inclusive-at-cap positive
    // control, the production-band positive-control sweep, the
    // cross-arm zero-then-cap ordering pin, the cross-axis
    // earlier-then-later precedence pin, the diagnostic-shape pin
    // carrying the offending value verbatim, and the cap-value
    // literal-identity + codec round-trip pins anchoring the
    // constant.

    #[test]
    fn validate_rejects_fuel_above_cap() {
        // The fail-before-pass-after pin: `LIMITS_FUEL_MAX + 1` =
        // one wasm-instruction past the structural ceiling — a `u64`
        // magnitude the typed slot round-trips losslessly through
        // serde, and that silently passed validate on every pre-gate
        // codebase because the typed slot's only check was the
        // zero-floor arm. The wasm-engine consuming the value (via
        // `Store::set_fuel` projection in the M2.5 host runtime)
        // accepts the magnitude but the sibling `:wall-clock` 1h cap
        // fires before the fuel counter could ever drain — the typed
        // `:fuel` slot becomes a no-op budget far from the source
        // caixa.lisp.
        let f = LIMITS_FUEL_MAX + 1;
        let l = LimitsSpec {
            fuel: Some(f),
            ..Default::default()
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::FuelExceedsCap { fuel: f }
        );
    }

    #[test]
    fn validate_rejects_fuel_far_above_cap() {
        // The "obvious authoring footgun" case: a `(:fuel
        // 1000000000000000)` (10^15 instructions), a paste-from-binary
        // `u64::MAX`, or a hex-literal-confused-for-decimal magnitude
        // — values the `u64` slot accepts cleanly, the codec
        // round-trips losslessly through serde, but the wasm-engine
        // can never honor as a meaningful counter. Until this gate
        // landed validate accepted them. Pin the common above-cap
        // values (10x cap, 1000x cap, `u64::MAX`) so a future
        // relaxation that drops the upper bound surfaces here. Peer
        // of `validate_rejects_cpu_far_above_cap` /
        // `validate_rejects_memory_8_gib` /
        // `validate_rejects_wall_clock_far_above_cap`.
        for f in [LIMITS_FUEL_MAX * 10, LIMITS_FUEL_MAX * 1_000, u64::MAX] {
            let l = LimitsSpec {
                fuel: Some(f),
                ..Default::default()
            };
            assert_eq!(
                l.validate().unwrap_err(),
                LimitsError::FuelExceedsCap { fuel: f }
            );
        }
    }

    #[test]
    fn validate_accepts_fuel_at_cap() {
        // The boundary value — exactly [`LIMITS_FUEL_MAX`] (10^12
        // wasm instructions) — must validate. The cap is inclusive
        // on the top edge, matching the discipline on every sibling
        // capped axis ([`LIMITS_MEMORY_WASM32_MAX_BYTES`],
        // [`LIMITS_WALL_CLOCK_MAX`], [`LIMITS_CPU_MILLICORES_MAX`],
        // [`crate::POLICY_TIMEOUT_MAX`],
        // [`crate::POLICY_BREAKER_WINDOW_MAX`],
        // [`crate::POLICY_RATE_LIMIT_MAX`]). Pin the boundary
        // explicitly so a future off-by-one tightening
        // (`>= LIMITS_FUEL_MAX` instead of `>`) surfaces here as a
        // test failure rather than a silent contract narrowing.
        let l = LimitsSpec {
            fuel: Some(LIMITS_FUEL_MAX),
            ..Default::default()
        };
        l.validate().expect("fuel == LIMITS_FUEL_MAX must validate");
    }

    #[test]
    fn validate_accepts_fuel_typical_values() {
        // The documented production-playbook band positive-control
        // sweep — every value the canonical caixa Servico runs in
        // (10^6..=10^9 fuel-units) must pass, plus a sweep through
        // the larger compute-bound-Servico band (10^10, 10^11) the
        // cap accepts. The canonical fixture is `1_000_000` =
        // wasmtime's documented `Store::set_fuel(1_000_000)` example.
        // Mirrors `validate_accepts_cpu_typical_values` on the
        // sibling `:cpu` axis.
        for f in [
            1_u64,             // smallest non-zero
            1_000,             // tiny per-call budget
            1_000_000,         // canonical fixture (10^6) — wasmtime book example
            10_000_000,        // typical small-Servico (10^7)
            100_000_000,       // typical heavier-Servico (10^8)
            1_000_000_000,     // 1 billion — upper realistic per-call (10^9)
            100_000_000_000,   // 10^11 — heavy compute-bound (10x below cap)
            500_000_000_000,   // half the cap
            1_000_000_000_000, // exactly at cap (10^12)
        ] {
            let l = LimitsSpec {
                fuel: Some(f),
                ..Default::default()
            };
            l.validate()
                .unwrap_or_else(|e| panic!("fuel={f} must validate; got {e:?}"));
        }
    }

    #[test]
    fn fuel_zero_takes_precedence_over_cap() {
        // The cross-arm ordering pin: `Some(0)` is structurally
        // outside both `>= 1` (zero-floor) and `<= LIMITS_FUEL_MAX`
        // (cap), but the zero-floor diagnostic is the more
        // self-locating one (it directly names the omit-axis
        // remediation and the wasmtime-traps-at-zero semantics), so
        // the validate gate must fire on zero first. Same shape every
        // other zero-then-cap ordering on this surface uses
        // (`MemoryZero` then `MemoryExceedsWasm32Cap`,
        // `WallClockZero` then `WallClockExceedsCap`, `CpuZero` then
        // `CpuExceedsCap`).
        let l = LimitsSpec {
            fuel: Some(0),
            ..Default::default()
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::FuelZero,
            "Some(0) must surface the zero-floor diagnostic, not the cap diagnostic"
        );
    }

    #[test]
    fn validate_rejects_fuel_cap_after_earlier_axes() {
        // Cross-axis ordering: when both an above-cap `:fuel` and an
        // earlier-axis violation are present, the earlier axis must
        // fire first. The validate sequence is :memory → :fuel →
        // :wall-clock → :cpu, so a paired memory-zero + fuel-above-
        // cap input surfaces `MemoryZero`, never the fuel-cap
        // diagnostic. Pins the canonical axis order so a future
        // refactor that reorders the arms surfaces here as a test
        // failure rather than a silent diagnostic regression. Peer
        // of `validate_rejects_cpu_cap_after_earlier_axes`.
        let l = LimitsSpec {
            memory: Some(0),
            fuel: Some(LIMITS_FUEL_MAX + 1),
            wall_clock: None,
            cpu: None,
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::MemoryZero,
            "earlier-axis violation must take precedence over later-axis cap violation"
        );
    }

    #[test]
    fn validate_rejects_fuel_cap_before_later_axes() {
        // Cross-axis ordering on the other side: when both an
        // above-cap `:fuel` and a later-axis violation are present,
        // the `:fuel` cap must fire before the `:wall-clock` /
        // `:cpu` zero-floor diagnostics. The validate sequence is
        // :memory → :fuel → :wall-clock → :cpu, so a paired
        // fuel-above-cap + wall-clock-zero input surfaces
        // `FuelExceedsCap`, not `WallClockZero`. Pins the canonical
        // axis order on the new arm's downstream side, peer to the
        // upstream pin `validate_rejects_fuel_cap_after_earlier_axes`.
        let l = LimitsSpec {
            memory: None,
            fuel: Some(LIMITS_FUEL_MAX + 1),
            wall_clock: Some(Duration::ZERO),
            cpu: Some(0),
        };
        assert_eq!(
            l.validate().unwrap_err(),
            LimitsError::FuelExceedsCap {
                fuel: LIMITS_FUEL_MAX + 1
            },
            ":fuel cap diagnostic must take precedence over later-axis zero-floor diagnostics"
        );
    }

    #[test]
    fn fuel_cap_diagnostic_carries_offending_value() {
        // The diagnostic-shape pin: the offending fuel count is
        // carried verbatim into the [`LimitsError::FuelExceedsCap`]
        // variant so the surfaced error message names the value the
        // author wrote, not just the cap. Same self-locating
        // diagnostic shape every other typed-cap arm on this surface
        // carries (`MemoryExceedsWasm32Cap` carries the offending
        // byte count verbatim, `WallClockExceedsCap` carries the
        // offending `Duration` verbatim, `CpuExceedsCap` carries the
        // offending millicore count verbatim).
        let f = 5_000_000_000_000_u64; // 5 trillion — 5x the cap
        let l = LimitsSpec {
            fuel: Some(f),
            ..Default::default()
        };
        let err = l.validate().unwrap_err();
        assert!(
            matches!(err, LimitsError::FuelExceedsCap { fuel } if fuel == f),
            "got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("5000000000000"),
            ":limits :fuel cap diagnostic must carry the offending value verbatim (got: {msg})"
        );
    }

    #[test]
    fn fuel_cap_pins_canonical_value() {
        // The [`LIMITS_FUEL_MAX`] constant pins the value at exactly
        // 10^12 (1 trillion wasm instructions) — the round-number
        // ceiling above the operational envelope the sibling
        // [`LIMITS_WALL_CLOCK_MAX`] (1h) × wasmtime's fuel-tracked
        // execution rate (~10^9 fuel/sec) yields. Pinning the
        // literal value here surfaces a future drift (a relaxation
        // to 10^15, a tightening to 10^9) as a deliberate test edit,
        // not a silent contract narrowing. Same shape every other
        // typed-cap value pin uses (`cpu_cap_pins_canonical_value`,
        // `wall_clock_cap_pins_canonical_value`,
        // `wasm32_memory_cap_matches_parsed_4_gib`).
        assert_eq!(LIMITS_FUEL_MAX, 1_000_000_000_000);
        assert_eq!(LIMITS_FUEL_MAX, 10_u64.pow(12));
    }

    #[test]
    fn fuel_cap_value_round_trips_through_serde() {
        // The serde round-trip property the cap arm preserves: the
        // [`LIMITS_FUEL_MAX`] constant itself round-trips through
        // the in-module `u64` serde codec — the cap value renders as
        // the bare integer literal and parses back to the same
        // `u64`. Pin this so a future drift between the cap constant
        // and the codec's accepted magnitude (a future custom u64
        // serializer that introduces lossy formatting) surfaces
        // here. Same shape every other typed boundary pin on this
        // surface uses (`wasm32_memory_cap_matches_parsed_4_gib`,
        // `wall_clock_cap_value_round_trips_through_codec`,
        // `cpu_cap_value_round_trips_through_codec`).
        let l = LimitsSpec {
            fuel: Some(LIMITS_FUEL_MAX),
            ..Default::default()
        };
        let json = serde_json::to_string(&l).unwrap();
        assert!(
            json.contains("1000000000000"),
            "the LIMITS_FUEL_MAX value must render verbatim as the bare integer 10^12 \
             (got: {json})"
        );
        let back: LimitsSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.fuel, Some(LIMITS_FUEL_MAX));
        l.validate()
            .expect("LIMITS_FUEL_MAX itself must pass validate");
    }

    // ── per-`:limits :memory` accessor pins (LimitsSpec::memory) ─────────

    #[test]
    fn limits_memory_returns_option_u64_byte_equal_across_permutations() {
        // The canonical per-`:limits` `:memory` Lunatic-per-process
        // wasm32-linear-memory byte-cap scalar pin: [`LimitsSpec::memory`]
        // must return the `:limits :memory` typed `u64` verbatim as an
        // `Option<u64>`, byte-equal to the raw field access across the
        // three canonical shape-arms — `None` (no cap declared —
        // engine-default applies), `Some(LIMITS_MEMORY_WASM32_PAGE_BYTES)`
        // (the structural minimum a validated `:limits :memory` may
        // carry, one wasm32 linear-memory page), `Some(64 * 1024 *
        // 1024)` (the canonical 64 MiB byte-cap the module-level
        // docstring names).
        //
        // Peer of the sibling per-`:politicas` [`crate::MeshPolicy::mtls_required`]
        // (c0110f1) / [`crate::MeshPolicy::retries`] (bdfb399) /
        // [`crate::MeshPolicy::timeout`] (7073d0f) accessor pin trio on
        // the sibling `Option<Copy-T>`-return axis, extended to the
        // peer per-`:limits` typed-`u64` optional-scalar shape —
        // first `Option<Copy-T>`-return accessor on the M2 slot family.
        // Pins against a future silent detour that re-derived the cap
        // from a peer axis (an accidental `.fuel`-collapse that
        // assumed the two `Option<u64>` axes carry the same value), a
        // `None` → `Some(0)` "zero means unbounded" collapse (the
        // canonical `Option<u64>` → `u64` collapse footgun the
        // [`LimitsError::MemoryZero`] validate arm guards on the peer
        // zero-floor axis), or a per-arm variant swap that landed on
        // one consumer without the other.
        for memory in [
            None,
            Some(LIMITS_MEMORY_WASM32_PAGE_BYTES),
            Some(64 * 1024 * 1024),
        ] {
            let l = LimitsSpec {
                memory,
                ..LimitsSpec::default()
            };
            assert_eq!(
                l.memory(),
                memory,
                "LimitsSpec::memory must return :limits :memory verbatim \
                 (got {:?}, expected {memory:?})",
                l.memory(),
            );
            assert_eq!(
                l.memory(),
                l.memory,
                "LimitsSpec::memory must byte-equal the raw .memory \
                 field access across every value in the accept-set",
            );
        }
    }

    #[test]
    fn limits_is_empty_memory_arm_routes_through_accessor() {
        // Composition pin: [`LimitsSpec::is_empty`]'s `memory` arm
        // must key off [`LimitsSpec::memory`], not the raw `.memory`
        // field access. Structurally: setting ONLY the `memory` slot
        // on an otherwise-default LimitsSpec must flip `is_empty()`
        // from `true` (all-`None`) to `false` (one axis carries a
        // value); the flip must be observed across every value in the
        // accept-set since the emptiness semantic reads "any axis
        // carries a value" — not "any axis carries a value above a
        // threshold" — the same non-collapsing shape the sibling M3
        // [`crate::MeshPolicy::is_empty`] predicate carries on its
        // peer `Option<Copy-T>`-typed slot surfaces.
        //
        // Pins against a future silent detour that re-derived the
        // emptiness predicate off a peer axis (an accidental
        // `.fuel.is_none()`-only chain that dropped the `memory` arm
        // entirely), an accessor-side detour that no longer names the
        // substrate-primitive typed dispatch (an accidental
        // `self.memory.unwrap_or(0) == 0` fallback in the accessor
        // that would silently classify both `None` and `Some(0)` as
        // the same value), or a threshold collapse (a
        // `self.memory().is_some_and(|m| m > 0)` that would silently
        // classify `Some(0)` as unset).
        //
        // Peer of the sibling per-`:politicas`
        // [`crate::MeshPolicy::is_empty`] `mtls_required` arm
        // accessor-composition pin (c0110f1) on the sibling optional-
        // scalar axis — same "the emptiness / shape-gate predicate
        // must route through the substrate-primitive typed dispatch"
        // discipline extended onto the peer per-`:limits` emptiness
        // predicate.
        let empty = LimitsSpec::default();
        assert!(
            empty.is_empty(),
            "LimitsSpec::default() must be is_empty() — every axis \
             defaults to None",
        );
        for memory in [
            Some(LIMITS_MEMORY_WASM32_PAGE_BYTES),
            Some(64 * 1024 * 1024),
            Some(LIMITS_MEMORY_WASM32_MAX_BYTES),
        ] {
            let l = LimitsSpec {
                memory,
                ..LimitsSpec::default()
            };
            assert!(
                !l.is_empty(),
                "LimitsSpec::is_empty must return false when :memory \
                 is {memory:?} — the emptiness predicate reads \"any \
                 axis carries a value\", not \"any axis carries a \
                 value above a threshold\"",
            );
            assert_eq!(
                l.memory().is_none(),
                l.is_empty(),
                "when :memory is the only set axis, is_empty() must \
                 equal memory().is_none() — the accessor and the \
                 emptiness predicate must route through the same \
                 substrate-primitive typed dispatch on the :memory \
                 arm",
            );
        }
    }

    #[test]
    fn limits_memory_projects_option_u64_by_copy() {
        // The by-copy pin: [`LimitsSpec::memory`] returns `Option<u64>`
        // by copy — `Option<u64>` is `Copy` and the accessor must
        // return by value, not by reference. Peer of the sibling per-
        // `:politicas` [`crate::MeshPolicy::mtls_required`] (c0110f1)
        // borrow-invariant pin on the peer `Option<bool>` shape,
        // extended onto the peer `Option<u64>` copy-invariant shape —
        // the accessor's returned `Option<u64>` must outlive `&self`
        // (multiple calls must return equal values from a dropped-
        // `&self` copy, since the returned Option carries no borrow),
        // and calling the accessor twice on the same LimitsSpec must
        // yield the same `Option<u64>` verbatim (idempotent, no side
        // effects on `&self`).
        //
        // Pins against a future silent detour that returned
        // `Option<&u64>` (which would type-check but silently break
        // every downstream caller — the future `wasmtime::Store::limiter`
        // wire path consumes `Option<u64>` by value and `&u64` would
        // fold to a detached copy at the call site), an accidental
        // `Option::as_ref()` projection (`self.memory.as_ref()` would
        // also type-check but return `Option<&u64>`), or a one-arm-
        // only accessor that reads `Some(*m)` in the Some arm but
        // reads a fresh `Default::default()` in the None arm.
        for memory in [
            None,
            Some(LIMITS_MEMORY_WASM32_PAGE_BYTES),
            Some(64 * 1024 * 1024),
            Some(LIMITS_MEMORY_WASM32_MAX_BYTES),
        ] {
            let l = LimitsSpec {
                memory,
                ..LimitsSpec::default()
            };
            let first = l.memory();
            let second = l.memory();
            assert_eq!(
                first, second,
                "LimitsSpec::memory must be idempotent — two \
                 successive calls on the same &self must return the \
                 same Option<u64>",
            );
            assert_eq!(
                first, memory,
                "LimitsSpec::memory must return :limits :memory \
                 verbatim by copy — got {first:?}, expected {memory:?}",
            );
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn validate_memory_arms_route_through_lifted_memory_accessor() {
        // Composition pin: every value-shape gate in
        // [`LimitsSpec::validate`] on the `:memory` axis (the
        // zero-floor `MemoryZero` arm, the sub-page `MemoryBelowWasm32Page`
        // arm, the above-cap `MemoryExceedsWasm32Cap` arm, the
        // non-page-multiple `MemoryNotPageMultiple` arm) must key off
        // [`LimitsSpec::memory`], not the raw `self.memory` field
        // access. Peer of the sibling per-`:politicas`
        // [`crate::AplicacaoSpec::validate_politicas`] `:timeout` /
        // `:retries` arm converge pin (1017b9d) on the sibling M3
        // mesh-slot family, extended onto the M2 per-`:limits`
        // `:memory` axis; peer of the sibling per-`:limits` `:fuel` /
        // `:wall-clock` / `:cpu` arms in the same fan-out that
        // already route through `self.fuel()` / `self.wall_clock()`
        // / `self.cpu()` at :880 / :888 / :942.
        //
        // Assertion shape: for each memory value in the
        // accept-and-refuse set, `LimitsSpec::memory()` must byte-
        // equal the raw `.memory` field it borrows from, and the
        // validate call on a `LimitsSpec { memory: <v>, ..default() }`
        // fixture must surface the same variant/Ok discriminant the
        // accessor-composed spec surfaces. Together they catch any
        // future silent detour — an accessor drift that no longer
        // shipped the raw slot verbatim, a validate-branch rebrand to
        // a peer-axis field read, an accidental `Option`-collapse in
        // any of the four arms — at caixa-core build time rather than
        // at a downstream runtime declared-but-inert-limits divergence
        // at the wasmtime `Store::limiter` boundary.
        //
        // `#[allow(clippy::too_many_lines)]` per the same discipline
        // peer over-100-line composition pins in this module accept
        // (see e.g. `limits_is_empty_memory_arm_routes_through_accessor`,
        // `limits_memory_returns_option_u64_byte_equal_across_permutations`).
        for memory in [
            None,
            Some(0),                                   // → MemoryZero
            Some(1),                                   // → MemoryBelowWasm32Page (sub-page)
            Some(LIMITS_MEMORY_WASM32_PAGE_BYTES - 1), // → MemoryBelowWasm32Page (at-under-page)
            Some(LIMITS_MEMORY_WASM32_PAGE_BYTES),     // → Ok (at-page-floor)
            Some(LIMITS_MEMORY_WASM32_PAGE_BYTES + 1), // → MemoryNotPageMultiple (one-past-page)
            Some(2 * LIMITS_MEMORY_WASM32_PAGE_BYTES), // → Ok (multi-page)
            Some(LIMITS_MEMORY_WASM32_MAX_BYTES),      // → Ok (at-cap)
            Some(LIMITS_MEMORY_WASM32_MAX_BYTES + 1),  // → MemoryExceedsWasm32Cap (one-past-cap)
        ] {
            let l = LimitsSpec {
                memory,
                ..LimitsSpec::default()
            };
            // (1) The accessor must byte-equal the raw field it wraps.
            assert_eq!(
                l.memory(),
                l.memory,
                "LimitsSpec::memory() must byte-equal the raw \
                 .memory field for {memory:?} — an accessor detour \
                 that dropped the raw slot's Option<u64> verbatim \
                 would silently split validate's :memory arms from \
                 every peer emit-site consumer that also routes \
                 through the accessor (the future wasmtime \
                 Store::limiter wire path, the caixa-helm \
                 resources.limits.memory materializer)",
            );
            // (2) Two successive validate() calls must yield the same
            // variant/Ok discriminant — the accessor-projected reads
            // and the raw-projected reads must produce identical
            // validation outcomes.
            let first = l.validate();
            let second = l.validate();
            assert_eq!(
                first, second,
                "LimitsSpec::validate must be idempotent on :memory \
                 {memory:?} — two successive calls must surface the \
                 same variant/Ok discriminant, catching any accessor \
                 detour that would introduce a value-dependent side \
                 effect on the &self projection",
            );
        }
        // (3) The specific arm-order shape the four converged sites
        // encode: `MemoryZero` (raw-`Some(0)`) precedes the page-floor
        // arm, which precedes the cap arm, which precedes the page-
        // multiple arm. Each arm must fire off the accessor-projected
        // read on its specific fixture value.
        assert_eq!(
            LimitsSpec {
                memory: Some(0),
                ..LimitsSpec::default()
            }
            .validate(),
            Err(LimitsError::MemoryZero),
            "MemoryZero must fire on Some(0) via the accessor projection",
        );
        assert_eq!(
            LimitsSpec {
                memory: Some(1),
                ..LimitsSpec::default()
            }
            .validate(),
            Err(LimitsError::MemoryBelowWasm32Page { bytes: 1 }),
            "MemoryBelowWasm32Page must fire on Some(1) via the accessor projection",
        );
        assert_eq!(
            LimitsSpec {
                memory: Some(LIMITS_MEMORY_WASM32_MAX_BYTES + 1),
                ..LimitsSpec::default()
            }
            .validate(),
            Err(LimitsError::MemoryExceedsWasm32Cap {
                bytes: LIMITS_MEMORY_WASM32_MAX_BYTES + 1
            }),
            "MemoryExceedsWasm32Cap must fire on one-past-cap via the accessor projection",
        );
        assert_eq!(
            LimitsSpec {
                memory: Some(LIMITS_MEMORY_WASM32_PAGE_BYTES + 1),
                ..LimitsSpec::default()
            }
            .validate(),
            Err(LimitsError::MemoryNotPageMultiple {
                bytes: LIMITS_MEMORY_WASM32_PAGE_BYTES + 1
            }),
            "MemoryNotPageMultiple must fire on one-past-page-floor via the accessor projection",
        );
        assert_eq!(
            LimitsSpec {
                memory: Some(LIMITS_MEMORY_WASM32_PAGE_BYTES),
                ..LimitsSpec::default()
            }
            .validate(),
            Ok(()),
            "at-page-floor must pass validate via the accessor projection",
        );
        assert_eq!(
            LimitsSpec {
                memory: Some(LIMITS_MEMORY_WASM32_MAX_BYTES),
                ..LimitsSpec::default()
            }
            .validate(),
            Ok(()),
            "at-cap must pass validate via the accessor projection",
        );
    }

    // ── per-`:limits :fuel` accessor pins (LimitsSpec::fuel) ─────────

    #[test]
    fn limits_fuel_returns_option_u64_byte_equal_across_permutations() {
        // The canonical per-`:limits` `:fuel` wasmtime-per-call
        // wasm-instruction budget scalar pin: [`LimitsSpec::fuel`]
        // must return the `:limits :fuel` typed `u64` verbatim as an
        // `Option<u64>`, byte-equal to the raw field access across
        // the three canonical shape-arms — `None` (no fuel budget
        // declared — engine-default applies), `Some(1)` (the
        // structural minimum a validated `:limits :fuel` may carry,
        // one wasm instruction; wasmtime traps the first instruction
        // at `fuel=0`, so `Some(1)` is the smallest budget that
        // executes any code), `Some(1_000_000)` (the canonical 10⁶
        // fuel-unit budget the in-tree `Caixa::template` and the
        // wasmtime book's `Store::set_fuel(1_000_000)` example both
        // carry).
        //
        // Peer of the sibling per-`:limits` [`LimitsSpec::memory`]
        // (620c067) accessor byte-equality pin on the peer typed-`u64`
        // optional-scalar axis, extended to the wasm-instruction-budget
        // shape — second `Option<Copy-T>`-return accessor on the M2
        // slot family. Pins against a future silent detour that
        // re-derived the fuel budget from a peer axis (an accidental
        // `.memory`-collapse that assumed the two `Option<u64>` axes
        // carry the same value — the two axes share a shape but not
        // a semantic, `:memory` counts linear-memory bytes and `:fuel`
        // counts wasm instructions), a `None` → `Some(0)` "zero means
        // unbounded" collapse (the canonical `Option<u64>` → `u64`
        // collapse footgun the [`LimitsError::FuelZero`] validate arm
        // guards on the peer zero-floor axis; wasmtime interprets
        // `fuel=0` as "trap the first instruction" not "no bound"), or
        // a per-arm variant swap that landed on one consumer without
        // the other.
        for fuel in [None, Some(1_u64), Some(1_000_000_u64)] {
            let l = LimitsSpec {
                fuel,
                ..LimitsSpec::default()
            };
            assert_eq!(
                l.fuel(),
                fuel,
                "LimitsSpec::fuel must return :limits :fuel verbatim \
                 (got {:?}, expected {fuel:?})",
                l.fuel(),
            );
            assert_eq!(
                l.fuel(),
                l.fuel,
                "LimitsSpec::fuel must byte-equal the raw .fuel \
                 field access across every value in the accept-set",
            );
        }
    }

    #[test]
    fn limits_is_empty_fuel_arm_routes_through_accessor() {
        // Composition pin: [`LimitsSpec::is_empty`]'s `fuel` arm
        // must key off [`LimitsSpec::fuel`], not the raw `.fuel`
        // field access. Structurally: setting ONLY the `fuel` slot
        // on an otherwise-default LimitsSpec must flip `is_empty()`
        // from `true` (all-`None`) to `false` (one axis carries a
        // value); the flip must be observed across every value in
        // the accept-set since the emptiness semantic reads "any
        // axis carries a value" — not "any axis carries a value
        // above a threshold" — the same non-collapsing shape the
        // sibling M3 [`crate::MeshPolicy::is_empty`] predicate
        // carries on its peer `Option<Copy-T>`-typed slot surfaces
        // and the sibling per-`:limits` [`LimitsSpec::memory`]
        // (620c067) `is_empty()` accessor-composition pin carries on
        // the peer `Option<u64>` axis.
        //
        // Pins against a future silent detour that re-derived the
        // emptiness predicate off a peer axis (an accidental
        // `.memory.is_none()`-only chain that dropped the `fuel` arm
        // entirely), an accessor-side detour that no longer names the
        // substrate-primitive typed dispatch (an accidental
        // `self.fuel.unwrap_or(0) == 0` fallback in the accessor
        // that would silently classify both `None` and `Some(0)` as
        // the same value — a footgun the [`LimitsError::FuelZero`]
        // validate arm explicitly closes since `fuel=0` traps rather
        // than expresses "unbounded"), or a threshold collapse (a
        // `self.fuel().is_some_and(|f| f > 0)` that would silently
        // classify `Some(0)` as unset).
        //
        // Peer of the sibling per-`:limits` [`LimitsSpec::memory`]
        // (620c067) `is_empty` composition pin on the peer
        // `Option<u64>` axis — same "the emptiness predicate must
        // route through the substrate-primitive typed dispatch"
        // discipline extended onto the peer per-`:limits` `:fuel`
        // arm.
        let empty = LimitsSpec::default();
        assert!(
            empty.is_empty(),
            "LimitsSpec::default() must be is_empty() — every axis \
             defaults to None",
        );
        for fuel in [Some(1_u64), Some(1_000_000_u64), Some(LIMITS_FUEL_MAX)] {
            let l = LimitsSpec {
                fuel,
                ..LimitsSpec::default()
            };
            assert!(
                !l.is_empty(),
                "LimitsSpec::is_empty must return false when :fuel \
                 is {fuel:?} — the emptiness predicate reads \"any \
                 axis carries a value\", not \"any axis carries a \
                 value above a threshold\"",
            );
            assert_eq!(
                l.fuel().is_none(),
                l.is_empty(),
                "when :fuel is the only set axis, is_empty() must \
                 equal fuel().is_none() — the accessor and the \
                 emptiness predicate must route through the same \
                 substrate-primitive typed dispatch on the :fuel \
                 arm",
            );
        }
    }

    #[test]
    fn limits_fuel_projects_option_u64_by_copy() {
        // The by-copy pin: [`LimitsSpec::fuel`] returns `Option<u64>`
        // by copy — `Option<u64>` is `Copy` and the accessor must
        // return by value, not by reference. Peer of the sibling per-
        // `:limits` [`LimitsSpec::memory`] (620c067) copy-invariant
        // pin on the peer `Option<u64>` shape — the accessor's
        // returned `Option<u64>` must outlive `&self` (multiple calls
        // must return equal values from a dropped-`&self` copy, since
        // the returned Option carries no borrow), and calling the
        // accessor twice on the same LimitsSpec must yield the same
        // `Option<u64>` verbatim (idempotent, no side effects on
        // `&self`).
        //
        // Pins against a future silent detour that returned
        // `Option<&u64>` (which would type-check but silently break
        // every downstream caller — the future `wasmtime::Store::set_fuel`
        // wire path consumes `u64` by value and `&u64` would fold to
        // a detached copy at the call site), an accidental
        // `Option::as_ref()` projection (`self.fuel.as_ref()` would
        // also type-check but return `Option<&u64>`), or a one-arm-
        // only accessor that reads `Some(*f)` in the Some arm but
        // reads a fresh `Default::default()` in the None arm.
        for fuel in [
            None,
            Some(1_u64),
            Some(1_000_000_u64),
            Some(LIMITS_FUEL_MAX),
        ] {
            let l = LimitsSpec {
                fuel,
                ..LimitsSpec::default()
            };
            let first = l.fuel();
            let second = l.fuel();
            assert_eq!(
                first, second,
                "LimitsSpec::fuel must be idempotent — two \
                 successive calls on the same &self must return the \
                 same Option<u64>",
            );
            assert_eq!(
                first, fuel,
                "LimitsSpec::fuel must return :limits :fuel \
                 verbatim by copy — got {first:?}, expected {fuel:?}",
            );
        }
    }

    // ── per-`:limits :wall-clock` accessor pins (LimitsSpec::wall_clock) ─

    #[test]
    fn limits_wall_clock_returns_option_duration_byte_equal_across_permutations() {
        // The canonical per-`:limits` `:wall-clock` wasmtime-per-call
        // wall-clock deadline scalar pin: [`LimitsSpec::wall_clock`]
        // must return the `:limits :wall-clock` typed `Duration`
        // verbatim as an `Option<Duration>`, byte-equal to the raw
        // field access across the three canonical shape-arms — `None`
        // (no wall-clock deadline declared — engine-default applies),
        // `Some(Duration::from_millis(1))` (the structural minimum a
        // validated `:limits :wall-clock` may carry, the
        // integer-millisecond floor
        // [`LimitsError::WallClockNotCanonical`] rejects everything
        // sub-ms; `Duration::ZERO` is separately rejected by
        // [`LimitsError::WallClockZero`]), `Some(Duration::from_secs(30))`
        // (the canonical 30s deadline the module-level docstring
        // names).
        //
        // Peer of the sibling per-`:limits` [`LimitsSpec::memory`]
        // (620c067) / [`LimitsSpec::fuel`] (795dee7) accessor
        // byte-equality pins on the peer typed-`u64` optional-scalar
        // axes, extended to the wall-clock-deadline `Option<Duration>`
        // shape — third `Option<Copy-T>`-return accessor on the M2 slot
        // family. Sibling to [`crate::MeshPolicy::timeout`] (7073d0f) on
        // the M3 mesh-slot family's peer `Option<Duration>` accessor
        // axis — same typed-`Duration` shape extended from the M3
        // per-call-timeout axis to the M2 per-outermost-call-deadline
        // axis. Pins against a future silent detour that re-derived the
        // wall-clock deadline from a peer axis (an accidental
        // `.fuel`-collapse that assumed the wall-clock deadline and
        // the fuel budget carry the same value — the two axes serve
        // different sandboxing purposes, wall-clock tracks scheduler
        // real time and fuel tracks wasm instructions), a `None` →
        // `Some(Duration::ZERO)` "zero means unbounded" collapse (the
        // canonical `Option<Duration>` → `Duration` collapse footgun
        // the [`LimitsError::WallClockZero`] validate arm guards on the
        // peer zero-floor axis; a zero deadline traps the first
        // instruction), or a per-arm variant swap that landed on one
        // consumer without the other.
        for wall_clock in [
            None,
            Some(Duration::from_millis(1)),
            Some(Duration::from_secs(30)),
        ] {
            let l = LimitsSpec {
                wall_clock,
                ..LimitsSpec::default()
            };
            assert_eq!(
                l.wall_clock(),
                wall_clock,
                "LimitsSpec::wall_clock must return :limits :wall-clock verbatim \
                 (got {:?}, expected {wall_clock:?})",
                l.wall_clock(),
            );
            assert_eq!(
                l.wall_clock(),
                l.wall_clock,
                "LimitsSpec::wall_clock must byte-equal the raw .wall_clock \
                 field access across every value in the accept-set",
            );
        }
    }

    #[test]
    fn limits_is_empty_wall_clock_arm_routes_through_accessor() {
        // Composition pin: [`LimitsSpec::is_empty`]'s `wall_clock` arm
        // must key off [`LimitsSpec::wall_clock`], not the raw
        // `.wall_clock` field access. Structurally: setting ONLY the
        // `wall_clock` slot on an otherwise-default LimitsSpec must
        // flip `is_empty()` from `true` (all-`None`) to `false` (one
        // axis carries a value); the flip must be observed across every
        // value in the accept-set since the emptiness semantic reads
        // "any axis carries a value" — not "any axis carries a value
        // above a threshold" — the same non-collapsing shape the
        // sibling M3 [`crate::MeshPolicy::is_empty`] predicate carries
        // on its peer `Option<Copy-T>`-typed slot surfaces and the
        // sibling per-`:limits` [`LimitsSpec::memory`] (620c067) /
        // [`LimitsSpec::fuel`] (795dee7) `is_empty()` accessor-
        // composition pins carry on the peer `Option<u64>` axes.
        //
        // Pins against a future silent detour that re-derived the
        // emptiness predicate off a peer axis (an accidental
        // `.memory.is_none()`-only chain that dropped the `wall_clock`
        // arm entirely), an accessor-side detour that no longer names
        // the substrate-primitive typed dispatch (an accidental
        // `self.wall_clock.unwrap_or(Duration::ZERO).is_zero()` fallback
        // in the accessor that would silently classify both `None` and
        // `Some(Duration::ZERO)` as the same value — a footgun the
        // [`LimitsError::WallClockZero`] validate arm explicitly closes
        // since a zero deadline traps rather than expresses
        // "unbounded"), or a threshold collapse (a
        // `self.wall_clock().is_some_and(|w| !w.is_zero())` that would
        // silently classify `Some(Duration::ZERO)` as unset).
        //
        // Peer of the sibling per-`:limits` [`LimitsSpec::memory`]
        // (620c067) / [`LimitsSpec::fuel`] (795dee7) `is_empty`
        // composition pins on the peer `Option<u64>` axes — same "the
        // emptiness predicate must route through the substrate-
        // primitive typed dispatch" discipline extended onto the peer
        // per-`:limits` `:wall-clock` arm.
        let empty = LimitsSpec::default();
        assert!(
            empty.is_empty(),
            "LimitsSpec::default() must be is_empty() — every axis \
             defaults to None",
        );
        for wall_clock in [
            Some(Duration::from_millis(1)),
            Some(Duration::from_secs(30)),
            Some(LIMITS_WALL_CLOCK_MAX),
        ] {
            let l = LimitsSpec {
                wall_clock,
                ..LimitsSpec::default()
            };
            assert!(
                !l.is_empty(),
                "LimitsSpec::is_empty must return false when :wall-clock \
                 is {wall_clock:?} — the emptiness predicate reads \"any \
                 axis carries a value\", not \"any axis carries a \
                 value above a threshold\"",
            );
            assert_eq!(
                l.wall_clock().is_none(),
                l.is_empty(),
                "when :wall-clock is the only set axis, is_empty() must \
                 equal wall_clock().is_none() — the accessor and the \
                 emptiness predicate must route through the same \
                 substrate-primitive typed dispatch on the :wall-clock \
                 arm",
            );
        }
    }

    #[test]
    fn limits_wall_clock_projects_option_duration_by_copy() {
        // The by-copy pin: [`LimitsSpec::wall_clock`] returns
        // `Option<Duration>` by copy — `Duration` is `Copy` (so
        // `Option<Duration>` is `Copy`) and the accessor must return by
        // value, not by reference. Peer of the sibling per-`:limits`
        // [`LimitsSpec::memory`] (620c067) / [`LimitsSpec::fuel`]
        // (795dee7) copy-invariant pins on the peer `Option<u64>`
        // shape, extended onto the peer `Option<Duration>` shape — the
        // accessor's returned `Option<Duration>` must outlive `&self`
        // (multiple calls must return equal values from a dropped-
        // `&self` copy, since the returned Option carries no borrow),
        // and calling the accessor twice on the same LimitsSpec must
        // yield the same `Option<Duration>` verbatim (idempotent, no
        // side effects on `&self`).
        //
        // Pins against a future silent detour that returned
        // `Option<&Duration>` (which would type-check but silently
        // break every downstream caller — the future
        // `wasmtime::Store::epoch_deadline_*` wire path consumes
        // `Duration` by value and `&Duration` would fold to a detached
        // copy at the call site), an accidental `Option::as_ref()`
        // projection (`self.wall_clock.as_ref()` would also type-check
        // but return `Option<&Duration>`), or a one-arm-only accessor
        // that reads `Some(*w)` in the Some arm but reads a fresh
        // `Default::default()` (which would collapse to
        // `Duration::ZERO`, not `None`) in the None arm.
        for wall_clock in [
            None,
            Some(Duration::from_millis(1)),
            Some(Duration::from_secs(30)),
            Some(LIMITS_WALL_CLOCK_MAX),
        ] {
            let l = LimitsSpec {
                wall_clock,
                ..LimitsSpec::default()
            };
            let first = l.wall_clock();
            let second = l.wall_clock();
            assert_eq!(
                first, second,
                "LimitsSpec::wall_clock must be idempotent — two \
                 successive calls on the same &self must return the \
                 same Option<Duration>",
            );
            assert_eq!(
                first, wall_clock,
                "LimitsSpec::wall_clock must return :limits :wall-clock \
                 verbatim by copy — got {first:?}, expected {wall_clock:?}",
            );
        }
    }

    // ── per-`:limits :cpu` accessor pins (LimitsSpec::cpu) ───────────

    #[test]
    fn limits_cpu_returns_option_u32_byte_equal_across_permutations() {
        // The canonical per-`:limits` `:cpu` Kubernetes-millicore
        // soft cgroup-share scalar pin: [`LimitsSpec::cpu`] must return
        // the `:limits :cpu` typed `u32` verbatim as an `Option<u32>`,
        // byte-equal to the raw field access across the three canonical
        // shape-arms — `None` (no cgroup share declared —
        // scheduler-default applies), `Some(1)` (the structural minimum
        // a validated `:limits :cpu` may carry, one millicore; a zero
        // cgroup share is separately rejected by
        // [`LimitsError::CpuZero`]), `Some(500)` (the canonical 500m
        // half-a-core share the in-tree
        // `limits_slot_propagates_into_values_block` smoke test carries
        // as the load-bearing example, peer to the `caixa-flux`
        // projector's identical 500m default).
        //
        // Peer of the sibling per-`:limits` [`LimitsSpec::memory`]
        // (620c067) / [`LimitsSpec::fuel`] (795dee7) /
        // [`LimitsSpec::wall_clock`] (8cb717b) accessor byte-equality
        // pins on the peer typed-`u64` / `u64` / `Duration`
        // optional-scalar axes, extended to the cgroup-cpu-share
        // `Option<u32>` shape — fourth and final `Option<Copy-T>`-return
        // accessor on the M2 slot family, closing the M2 `:limits`
        // `Option<Copy-T>` accessor axis. Sibling to
        // [`crate::MeshPolicy::retries`] (bdfb399) on the M3 mesh-slot
        // family's peer `Option<u32>` accessor axis — same typed-`u32`
        // shape extended from the M3 per-edge-transient-failure-retry-
        // budget axis to the M2 per-process-cgroup-cpu-share axis.
        // Pins against a future silent detour that re-derived the cpu
        // share from a peer axis (an accidental `.retries`-collapse that
        // assumed the two `Option<u32>` axes carry the same value — the
        // two axes share a shape but not a semantic, M2 `:cpu` counts
        // millicores of soft cgroup share and M3 `:retries` counts
        // per-edge transient-failure retry budget), a `None` → `Some(0)`
        // "zero means unbounded" collapse (the canonical `Option<u32>` →
        // `u32` collapse footgun the [`LimitsError::CpuZero`] validate
        // arm guards on the peer zero-floor axis; a zero cgroup share
        // starves the process rather than expressing "unbounded"), or a
        // per-arm variant swap that landed on one consumer without the
        // other.
        for cpu in [None, Some(1_u32), Some(500_u32)] {
            let l = LimitsSpec {
                cpu,
                ..LimitsSpec::default()
            };
            assert_eq!(
                l.cpu(),
                cpu,
                "LimitsSpec::cpu must return :limits :cpu verbatim \
                 (got {:?}, expected {cpu:?})",
                l.cpu(),
            );
            assert_eq!(
                l.cpu(),
                l.cpu,
                "LimitsSpec::cpu must byte-equal the raw .cpu \
                 field access across every value in the accept-set",
            );
        }
    }

    #[test]
    fn limits_is_empty_cpu_arm_routes_through_accessor() {
        // Composition pin: [`LimitsSpec::is_empty`]'s `cpu` arm must key
        // off [`LimitsSpec::cpu`], not the raw `.cpu` field access.
        // Structurally: setting ONLY the `cpu` slot on an
        // otherwise-default LimitsSpec must flip `is_empty()` from
        // `true` (all-`None`) to `false` (one axis carries a value);
        // the flip must be observed across every value in the
        // accept-set since the emptiness semantic reads "any axis
        // carries a value" — not "any axis carries a value above a
        // threshold" — the same non-collapsing shape the sibling M3
        // [`crate::MeshPolicy::is_empty`] predicate carries on its
        // peer `Option<Copy-T>`-typed slot surfaces and the sibling
        // per-`:limits` [`LimitsSpec::memory`] (620c067) /
        // [`LimitsSpec::fuel`] (795dee7) / [`LimitsSpec::wall_clock`]
        // (8cb717b) `is_empty()` accessor-composition pins carry on the
        // peer `Option<u64>` / `Option<u64>` / `Option<Duration>` axes.
        //
        // Pins against a future silent detour that re-derived the
        // emptiness predicate off a peer axis (an accidental
        // `.memory.is_none()`-only chain that dropped the `cpu` arm
        // entirely), an accessor-side detour that no longer names the
        // substrate-primitive typed dispatch (an accidental
        // `self.cpu.unwrap_or(0) == 0` fallback in the accessor that
        // would silently classify both `None` and `Some(0)` as the same
        // value — a footgun the [`LimitsError::CpuZero`] validate arm
        // explicitly closes since a zero cgroup share starves the
        // process rather than expressing "unbounded"), or a threshold
        // collapse (a `self.cpu().is_some_and(|m| m > 0)` that would
        // silently classify `Some(0)` as unset).
        //
        // Peer of the sibling per-`:limits` [`LimitsSpec::memory`]
        // (620c067) / [`LimitsSpec::fuel`] (795dee7) /
        // [`LimitsSpec::wall_clock`] (8cb717b) `is_empty` composition
        // pins on the peer `Option<u64>` / `Option<u64>` /
        // `Option<Duration>` axes — same "the emptiness predicate must
        // route through the substrate-primitive typed dispatch"
        // discipline extended onto the peer per-`:limits` `:cpu` arm.
        // Closes the M2 `:limits` `is_empty`-composition family — every
        // arm now routes through its typed accessor, no open-coded
        // field access remains.
        let empty = LimitsSpec::default();
        assert!(
            empty.is_empty(),
            "LimitsSpec::default() must be is_empty() — every axis \
             defaults to None",
        );
        for cpu in [Some(1_u32), Some(500_u32), Some(LIMITS_CPU_MILLICORES_MAX)] {
            let l = LimitsSpec {
                cpu,
                ..LimitsSpec::default()
            };
            assert!(
                !l.is_empty(),
                "LimitsSpec::is_empty must return false when :cpu \
                 is {cpu:?} — the emptiness predicate reads \"any \
                 axis carries a value\", not \"any axis carries a \
                 value above a threshold\"",
            );
            assert_eq!(
                l.cpu().is_none(),
                l.is_empty(),
                "when :cpu is the only set axis, is_empty() must \
                 equal cpu().is_none() — the accessor and the \
                 emptiness predicate must route through the same \
                 substrate-primitive typed dispatch on the :cpu \
                 arm",
            );
        }
    }

    #[test]
    fn limits_cpu_projects_option_u32_by_copy() {
        // The by-copy pin: [`LimitsSpec::cpu`] returns `Option<u32>` by
        // copy — `Option<u32>` is `Copy` and the accessor must return
        // by value, not by reference. Peer of the sibling per-`:limits`
        // [`LimitsSpec::memory`] (620c067) / [`LimitsSpec::fuel`]
        // (795dee7) / [`LimitsSpec::wall_clock`] (8cb717b)
        // copy-invariant pins on the peer `Option<u64>` / `Option<u64>`
        // / `Option<Duration>` shapes, extended onto the peer
        // `Option<u32>` copy-invariant shape — the accessor's returned
        // `Option<u32>` must outlive `&self` (multiple calls must
        // return equal values from a dropped-`&self` copy, since the
        // returned Option carries no borrow), and calling the accessor
        // twice on the same LimitsSpec must yield the same
        // `Option<u32>` verbatim (idempotent, no side effects on
        // `&self`).
        //
        // Pins against a future silent detour that returned
        // `Option<&u32>` (which would type-check but silently break
        // every downstream caller — the future K8s pod-spec
        // `resources.requests.cpu` wire path consumes `u32` by value
        // and `&u32` would fold to a detached copy at the call site),
        // an accidental `Option::as_ref()` projection
        // (`self.cpu.as_ref()` would also type-check but return
        // `Option<&u32>`), or a one-arm-only accessor that reads
        // `Some(*m)` in the Some arm but reads a fresh
        // `Default::default()` in the None arm.
        for cpu in [
            None,
            Some(1_u32),
            Some(500_u32),
            Some(LIMITS_CPU_MILLICORES_MAX),
        ] {
            let l = LimitsSpec {
                cpu,
                ..LimitsSpec::default()
            };
            let first = l.cpu();
            let second = l.cpu();
            assert_eq!(
                first, second,
                "LimitsSpec::cpu must be idempotent — two \
                 successive calls on the same &self must return the \
                 same Option<u32>",
            );
            assert_eq!(
                first, cpu,
                "LimitsSpec::cpu must return :limits :cpu \
                 verbatim by copy — got {first:?}, expected {cpu:?}",
            );
        }
    }
}
