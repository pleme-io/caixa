//! OTP-shaped supervisor trees, encoded as a typed `:kind Supervisor`
//! caixa with a strategy + restart-policy children list.
//!
//! See `theory/INSPIRATIONS.md` §II.2 + §III.2 for the prior-art frame
//! (Erlang OTP supervisor + Lunatic supervisor strategies as Rust types).
//!
//! ```lisp
//! (defcaixa
//!   :nome           "my-app-root"
//!   :versao         "0.1.0"
//!   :kind           Supervisor
//!   :estrategia     OneForOne
//!   :max-restarts   5
//!   :restart-window "60s"
//!   :children       ((:caixa "worker"       :versao "^0.1" :restart Permanent)
//!                    (:caixa "cache-server" :versao "^0.1" :restart Transient)
//!                    (:caixa "scratch-job"  :versao "^0.1" :restart Temporary)))
//! ```
//!
//! wasm-operator (M3) walks the tree, materializes one ComputeUnit per
//! child, and applies the strategy on child failure. The Rust types
//! here are the typed contract; the runtime owns lifecycle.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One of the four canonical Erlang/OTP restart strategies.
///
/// The strategy decides what happens to *sibling* children when one
/// child dies. Per-child behaviour is governed by [`RestartPolicy`].
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    gen_platform::TypedDispatcher,
    gen_platform::Discriminant,
    gen_platform::IsVariant,
    gen_platform::FromStrKind,
)]
#[discriminant(also_display)]
pub enum RestartStrategy {
    /// On child failure, restart only that child. Default; matches
    /// most "tree of independent workers" use cases.
    OneForOne,
    /// On child failure, restart every child. Used when children
    /// share state and must be in sync.
    OneForAll,
    /// On child failure, restart the failed child and every child
    /// started *after* it (preserving startup order). Used when later
    /// children depend on earlier ones.
    RestForOne,
    /// Dynamic children of the same shape, started on demand. The
    /// supervisor doesn't know its children at boot; they're added as
    /// they're needed (e.g. one child per session).
    SimpleOneForOne,
}

impl Default for RestartStrategy {
    fn default() -> Self {
        Self::OneForOne
    }
}

/// Per-child restart policy.
///
/// Permanent / Temporary / Transient match Erlang/OTP semantics 1:1.
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    gen_platform::TypedDispatcher,
    gen_platform::Discriminant,
    gen_platform::IsVariant,
    gen_platform::FromStrKind,
)]
#[discriminant(also_display)]
pub enum RestartPolicy {
    /// Always restart the child, regardless of how it died. Used for
    /// long-running services that must always be up.
    Permanent,
    /// Never restart. Used for one-shot work whose completion is
    /// itself the success signal (`oneShot` triggers map here).
    Temporary,
    /// Restart only when the child died *abnormally* (non-zero exit
    /// or unhandled exception). A clean exit completes the child.
    Transient,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self::Permanent
    }
}

// Fleet-wide dispatcher-catalog registrations for caixa's OTP
// supervisor surface — two more typed shadows over Erlang/OTP
// primitives the substrate now mechanically tracks (see
// theory/UNIFIED-COMPUTING-MODEL.md §VI for the roadmap +
// theory/TYPED-ABSORPTION.md for the absorption arc).
gen_platform::register_dispatcher!("caixa.restart-strategy", RestartStrategy);
gen_platform::register_dispatcher!("caixa.restart-policy", RestartPolicy);

/// One child entry in the supervisor's `:children` list.
///
/// Every child references another caixa by `:caixa <nome>` + version
/// constraint. The supervisor materializes one ComputeUnit per entry.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChildSpec {
    /// The child caixa's `:nome`. Must resolve via the same dependency
    /// resolution path as `:deps` (caixa-resolver).
    pub caixa: String,

    /// Semver constraint (`"^0.1"`, `"~0.1.2"`, etc.) — same shape as
    /// [`crate::dep::Dep::versao`].
    pub versao: String,

    /// Restart policy — defaults to [`RestartPolicy::Permanent`].
    #[serde(default)]
    pub restart: RestartPolicy,
}

/// Supervisor-typed slots that live alongside the standard Caixa
/// fields when `:kind Supervisor`. Held flat in [`crate::Caixa`] so
/// the manifest stays a single typed form; this struct exists for
/// validation + conversion.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorSpec {
    /// Restart strategy. Defaults to [`RestartStrategy::OneForOne`].
    #[serde(default)]
    pub estrategia: RestartStrategy,

    /// Max restarts within [`Self::restart_window`] before the
    /// supervisor itself terminates (and its parent supervisor decides
    /// what to do). Default 5.
    #[serde(default = "default_max_restarts")]
    pub max_restarts: u32,

    /// Sliding window for `max_restarts`. Authored as a duration
    /// string (`"60s"`, `"5m"`); absent = "never reset". A `Some(0s)`
    /// is rejected by [`Self::validate`] — Erlang/OTP's
    /// `MaxIntensity / Period` invariant requires a positive window
    /// (a zero-period supervisor either trips on the first failure or
    /// never trips, depending on operator interpretation, neither of
    /// which is the author's intent). Omit the slot to express "no
    /// reset"; carry a positive duration to express the sliding window.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "duration_codec"
    )]
    pub restart_window: Option<Duration>,

    /// Static children. Empty for `SimpleOneForOne` (children added
    /// dynamically); required for the other three strategies.
    #[serde(default)]
    pub children: Vec<ChildSpec>,
}

const fn default_max_restarts() -> u32 {
    5
}

/// Upper-bound ceiling on the `:supervisor :max-restarts` axis — every
/// validated [`SupervisorSpec::max_restarts`] past
/// [`SupervisorSpec::validate`] lies in `1..=SUPERVISOR_MAX_RESTARTS_MAX`.
///
/// The typed field is `u32` (the zero-floor arm
/// [`SupervisorError::ZeroMaxRestarts`] already brackets the bottom edge),
/// so a programmatic struct literal
/// (`SupervisorSpec { max_restarts: u32::MAX, .. }`) and the equivalent
/// author-surface form (`:max-restarts 4294967295` or any
/// `:max-restarts 100000`-shape typo landing in the slot) both round-trip
/// cleanly through serde — a structurally unbounded `u32` ceiling. The
/// runtime substrate consuming the value (Erlang/OTP's
/// `MaxIntensity / Period` ratio, the future wasm-operator's
/// per-supervisor restart-intensity counter, the M4
/// `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's admission webhook)
/// then turned a typed `:max-restarts` policy into a no-op supervisor: the
/// escalation threshold is structurally so high that no realistic
/// restarts-per-`:restart-window` traffic shape can reach it, the
/// supervisor never escalates to its parent, and a bad child can loop
/// inside the window indefinitely with the parent supervisor structurally
/// never receiving the "this subtree has exceeded its restart budget"
/// signal the typed slot is meant to express — the canonical
/// "supervisor intensity declared, no escalation" footgun, exactly the
/// peer of the [`crate::aplicacao::POLICY_BREAKER_MAX_FAILURES_MAX`] cap
/// on the `:politicas :circuit-breaker :max-failures` axis (both are
/// "trip the next-higher protection layer after N events in a rolling
/// window" counters with identical degenerate-at-the-high-end shape).
///
/// The `1000` ceiling matches the sibling
/// [`crate::aplicacao::POLICY_BREAKER_MAX_FAILURES_MAX`] (the closest
/// peer — same "events-per-window trip threshold" semantics, same `u32`
/// type, same no-op-at-the-high-end failure mode) so the M4
/// `mesh.pleme.io/v1alpha1/Supervisor` / `.../Aplicacao` CR materializers
/// and the future wasm-operator's per-supervisor restart-intensity
/// counter reach for either field knowing the value is in `1..=1000`
/// without re-validating at the reconciler layer. The cap sits two
/// orders of magnitude above every documented Erlang/OTP production
/// playbook recommendation (Learn You Some Erlang's
/// `{intensity, 5, 60}` worker-supervisor default, Elixir's `Supervisor`
/// `max_restarts: 3` default, OTP's `supervisor` callback module
/// `MaxR = 1` / `MaxT = 5` "minimal-restart" default, Riak Core's
/// typical `MaxR ∈ 5..=100`, RabbitMQ's broker-supervisor `MaxR = 5`
/// default) and below the clearly-pathological "effectively no
/// escalation" floor (`10_000`, `100_000`, `u32::MAX`): a value the
/// author can plausibly want at hyperscale (a long-running supervisor
/// over a very-flaky pool tolerating thousands of transient restarts
/// before escalating), but a hard wall above which the typed policy is
/// structurally a no-op carried verbatim on every emitted child-restart
/// reconciliation contract.
///
/// Lifted as a typed `pub const` so the bound has exactly one source of
/// truth — the future M4 `mesh.pleme.io/v1alpha1/Supervisor` CR
/// materializer's admission webhook and the wasm-operator-side
/// per-supervisor restart-intensity reconciler read from one place. Same
/// shape every other typed upper bound in this crate carries
/// ([`crate::aplicacao::POLICY_BREAKER_MAX_FAILURES_MAX`],
/// [`crate::aplicacao::POLICY_RETRIES_MAX`],
/// [`crate::aplicacao::POLICY_RATE_LIMIT_MAX`],
/// [`crate::LIMITS_MEMORY_WASM32_MAX_BYTES`],
/// [`crate::render::DNS_1123_LABEL_MAX_LEN`],
/// [`crate::render::NATS_SUBJECT_MAX_LEN`]).
pub const SUPERVISOR_MAX_RESTARTS_MAX: u32 = 1000;

/// Upper-bound ceiling on the `:supervisor :restart-window` axis —
/// every validated `Some(`[`SupervisorSpec::restart_window`]`)` past
/// [`SupervisorSpec::validate`] lies in `1ms..=SUPERVISOR_RESTART_WINDOW_MAX`
/// (inclusive on both ends, integer-millisecond magnitudes by the
/// canonical-form gate immediately preceding).
///
/// The typed field is `Option<Duration>` (the zero-floor arm
/// [`SupervisorError::RestartWindowZero`] already rejects
/// `Some(Duration::ZERO)`, and the canonical-form arm
/// [`SupervisorError::RestartWindowNotCanonical`] already rejects
/// sub-millisecond residue), so a programmatic struct literal
/// (`SupervisorSpec { restart_window: Some(Duration::from_secs(86_400)),
/// .. }` — 24h) and the equivalent author-surface form
/// (`(:supervisor (:restart-window "24h"))` — the shared duration codec
/// emits `"<n>h"` for any integer-hour magnitude) both round-trip
/// cleanly through serde — a structurally unbounded `Duration` ceiling.
/// A `:restart-window` value far above the documented Erlang/OTP
/// `MaxIntensity / Period` production-playbook band (Learn You Some
/// Erlang's `{intensity, 5, 60}` worker-supervisor `Period = 60s`
/// default, Elixir's `Supervisor` `max_seconds: 5` default, OTP's
/// `supervisor` callback module `MaxT = 5..=60` typical, Riak Core's
/// `MaxT ∈ 10s..=300s`, RabbitMQ broker-supervisor `MaxT = 5s` default)
/// degenerates the supervisor's restart-intensity counter into a
/// lifetime counter: the rolling failure-counting window is structurally
/// so long that transient restarts are never forgotten, so the
/// `MaxIntensity / Period` ratio degenerates from "trip the parent
/// supervisor when the child has exceeded its restart budget *within
/// the recent window*" to "trip the parent when the child has exceeded
/// its restart budget *over its lifetime*" — every transient restart
/// counts against the budget forever, the supervisor's reset semantic
/// never reaches the child, and the typed `:restart-window` slot
/// becomes a no-op rolling window carried on every emitted hierarchical
/// reconciliation contract. The canonical
/// rolling-window-degenerates-to-lifetime-counter footgun the sibling
/// [`crate::POLICY_BREAKER_WINDOW_MAX`] cap closes on the peer
/// `:politicas :circuit-breaker :window` axis with identical shape (both
/// are "rolling failure-counting window with a per-`Period` reset" Duration
/// axes whose lifetime-counter degenerate at the high end is the same
/// "the reset semantic never fires" CSE invariant violation).
///
/// The `1h` (3600s = `3_600_000` ms) ceiling matches the largest unit
/// the shared duration codec emits (`"<n>h"` for any integer-hour
/// magnitude) — every value in the canonical authoring form's
/// `<integer><unit>` grammar at or below this cap renders to a clean
/// canonical string — and matches the three sibling typed-`Duration`
/// caps already lifted to this surface
/// ([`crate::LIMITS_WALL_CLOCK_MAX`], [`crate::POLICY_TIMEOUT_MAX`],
/// [`crate::POLICY_BREAKER_WINDOW_MAX`]). All four typed-`Duration`
/// axes — per-process `:limits :wall-clock`, per-edge `:politicas
/// :timeout`, per-breaker `:politicas :circuit-breaker :window`, and
/// per-supervisor `:supervisor :restart-window` — now share a single
/// uniform top edge at the codec's largest emitted unit so the next
/// typed-slot wiring (the future wasm-operator's per-supervisor
/// `MaxIntensity / Period` reconciler, the M4
/// `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's admission
/// webhook, the `caixa-operator`'s hierarchical reconciliation
/// scheduler) reaches for any of the four knowing the value is in
/// `1ms..=1h` without re-validating at the renderer layer. The cap sits
/// two orders of magnitude above every documented Erlang/OTP / Elixir /
/// Riak Core / RabbitMQ production-playbook recommendation band
/// (`5s..=300s`) and below the clearly-pathological "rolling window
/// degenerates to lifetime counter" floor (`24h`, `7d`, `Duration::MAX`):
/// a value the author can plausibly want for a very-low-traffic
/// long-tail failure-restart window over a hyperscale-flaky child pool,
/// but a hard wall above which the rolling-window contract is
/// structurally a lifetime-counter contract.
///
/// Lifted as a typed `pub const` so the bound has exactly one source
/// of truth — the future M4 `mesh.pleme.io/v1alpha1/Supervisor` CR
/// materializer's admission webhook, the wasm-operator-side
/// per-supervisor `MaxIntensity / Period` reconciler, and the
/// `caixa-operator`'s hierarchical reconciliation scheduler all read
/// from one place. Same shape every other typed upper bound in this
/// crate carries ([`SUPERVISOR_MAX_RESTARTS_MAX`],
/// [`crate::aplicacao::POLICY_BREAKER_MAX_FAILURES_MAX`],
/// [`crate::aplicacao::POLICY_RETRIES_MAX`],
/// [`crate::aplicacao::POLICY_RATE_LIMIT_MAX`],
/// [`crate::LIMITS_MEMORY_WASM32_MAX_BYTES`],
/// [`crate::LIMITS_WALL_CLOCK_MAX`], [`crate::POLICY_TIMEOUT_MAX`],
/// [`crate::POLICY_BREAKER_WINDOW_MAX`],
/// [`crate::render::DNS_1123_LABEL_MAX_LEN`],
/// [`crate::render::NATS_SUBJECT_MAX_LEN`]).
pub const SUPERVISOR_RESTART_WINDOW_MAX: Duration = Duration::from_secs(3600);

impl Default for SupervisorSpec {
    fn default() -> Self {
        Self {
            estrategia: RestartStrategy::default(),
            max_restarts: default_max_restarts(),
            restart_window: Some(Duration::from_secs(60)),
            children: Vec::new(),
        }
    }
}

impl SupervisorSpec {
    /// Validate the supervisor's typed shape — strategy ↔ children
    /// invariants, max_restarts > 0, restart_window > 0 when set,
    /// per-child non-empty + duplicate-free names.
    ///
    /// Mirrors the value-shape discipline applied to every other
    /// typed slot:
    ///
    ///   - `Some(Duration::ZERO)` on a Duration-bearing axis is the
    ///     same "0 means the opposite of what you think" footgun
    ///     closed for `:politicas :timeout` (Envoy interprets a zero
    ///     timeout as `infinite`), `:politicas :circuit-breaker
    ///     :window`, and `:limits :wall-clock`. The
    ///     `MaxIntensity / Period` ratio in Erlang/OTP's
    ///     `supervisor` requires `Period > 0`; a zero period either
    ///     trips on the first failure or never trips depending on
    ///     operator interpretation, neither of which is the
    ///     author's intent. Omit `:restart-window` to express "no
    ///     reset"; carry a positive duration to express the window.
    ///   - duplicate `:children` `:caixa` names are the same
    ///     graph-node-set / multiset distinction closed for
    ///     `:membros` (4bb3f3d), `:placement :clusters` (c7c7799),
    ///     and `:entrada :paths` (eb3456d). Two children with the
    ///     same `:caixa` materialize as two ComputeUnits with the
    ///     same name in the cluster's HelmRelease values, one
    ///     silently overwriting the other. Erlang/OTP's
    ///     `child_spec.id` is required-unique per supervisor;
    ///     pleme-io enforces the same set-not-multiset shape on
    ///     `:caixa` (the load-bearing identity in our renderer).
    pub fn validate(&self) -> Result<(), SupervisorError> {
        match self.estrategia {
            RestartStrategy::SimpleOneForOne => {
                // SimpleOneForOne: children added at runtime. Static
                // list must be empty (one shape declared elsewhere).
                if !self.children.is_empty() {
                    return Err(SupervisorError::SimpleOneForOneWithStaticChildren);
                }
            }
            _ => {
                if self.children.is_empty() {
                    return Err(SupervisorError::NoChildren {
                        estrategia: self.estrategia,
                    });
                }
            }
        }
        if self.max_restarts == 0 {
            return Err(SupervisorError::ZeroMaxRestarts);
        }
        // Upper-bound ceiling on the typed `:max-restarts` axis. The
        // typed slot is `u32` and the zero-floor arm immediately above
        // already brackets the bottom edge; until this gate landed the
        // top edge ran all the way to `u32::MAX` and a struct-literal
        // `SupervisorSpec { max_restarts: 100_000, .. }` (or the
        // equivalent author-surface `:max-restarts 100000` /
        // `:max-restarts 4294967295` typo landing in the slot) silently
        // passed validate. The runtime substrate consuming the value
        // (Erlang/OTP's `MaxIntensity / Period` ratio, the future
        // wasm-operator's per-supervisor restart-intensity counter, the
        // M4 `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's
        // admission webhook) then turned a typed `:max-restarts`
        // policy into a no-op supervisor: the escalation threshold is
        // structurally so high that no realistic
        // restarts-per-`:restart-window` traffic shape can reach it,
        // the supervisor never escalates to its parent, and a bad
        // child can loop inside the window indefinitely with the
        // parent supervisor structurally never receiving the "this
        // subtree has exceeded its restart budget" signal the typed
        // slot is meant to express. Lifting the rejection to a
        // build-time gate at `SupervisorSpec::validate` brackets the
        // typed `:max-restarts` set structurally — every validated
        // value lies in `1..=SUPERVISOR_MAX_RESTARTS_MAX` — and
        // matches the same top-and-bottom-edge discipline the
        // [`crate::aplicacao::POLICY_BREAKER_MAX_FAILURES_MAX`] cap
        // applies on the peer `:politicas :circuit-breaker
        // :max-failures` axis (zero-floor `PolicyBreakerZeroFailures`
        // + upper-floor `PolicyBreakerMaxFailuresExceedsCap`): both
        // axes are "trip the next-higher protection layer after N
        // events in a rolling window" counters with identical
        // degenerate-at-the-high-end shape. The zero-floor gate
        // strictly precedes this cap arm so `0` surfaces the more
        // self-locating `ZeroMaxRestarts` diagnostic (with its
        // counter-axis remediation directly named) rather than the
        // cap-shape diagnostic; the cap arm strictly precedes the
        // sibling `:restart-window` zero-floor / canonical-millisecond
        // arms so an over-cap `max_restarts` paired with a
        // structurally invalid window surfaces the cap diagnostic
        // first, mirroring the
        // `PolicyBreakerMaxFailuresExceedsCap` / window-axis cross-arm
        // ordering on the peer `:politicas :circuit-breaker` slot.
        if self.max_restarts > SUPERVISOR_MAX_RESTARTS_MAX {
            return Err(SupervisorError::MaxRestartsExceedsCap {
                max_restarts: self.max_restarts,
            });
        }
        if matches!(self.restart_window, Some(d) if d.is_zero()) {
            return Err(SupervisorError::RestartWindowZero);
        }
        // Sub-millisecond residue gate on the typed `:restart-window`
        // axis. The shared `duration_codec` (which serializes /
        // deserializes this slot via `with = "duration_codec"`)
        // truncates to `as_millis()` before picking the canonical unit,
        // so any `Duration` whose `subsec_nanos() % 1_000_000 != 0`
        // either truncates on first serialize (e.g.
        // `Duration::from_micros(1500)` = 1_500_000 ns → renders
        // `"1ms"` → parses back to `Duration::from_millis(1)` =
        // 1_000_000 ns ≠ original) or — for sub-millisecond magnitudes
        // — renders as the literal `"0s"` (`from_micros(500)` →
        // `as_millis() == 0` → renders `"0s"`) the zero-floor arm
        // immediately above rejects on re-validate. The serde path is
        // already gated at the codec layer (a4ae535 — every
        // `{"restartWindow":"1.5s"}`-shaped JSON payload surfaces a
        // structured diagnostic at deserialize); this arm closes the
        // programmatic-struct-literal path the codec gate can't see
        // (`SupervisorSpec { restart_window: Some(Duration::from_micros(1500)), .. }`,
        // or any wasm-operator / supervisor-tree caller propagating a
        // `Duration` from a non-`<integer><unit>`-string source). The
        // shared predicate
        // [`duration_codec::is_integer_millisecond_duration`] lives
        // next to the codec — single source of truth, drift between
        // the codec's accepted granularity and any typed `Duration`
        // slot's accepted set is a single-edit fix at the predicate
        // rather than a silent round-trip break the next consumer (the
        // future wasm-operator's per-supervisor MaxIntensity/Period
        // accounting, the M4 `mesh.pleme.io/v1alpha1/Supervisor` CR
        // materializer's `:restart-window` admission webhook)
        // discovers at apply time. Closes the fourth (and last) typed-
        // `Duration` axis on this discipline — peer with
        // [`crate::LimitsError::WallClockNotCanonical`] (82fc3ef on
        // `:limits :wall-clock`) and
        // [`crate::AplicacaoError::PolicyTimeoutNotCanonical`] /
        // [`crate::AplicacaoError::PolicyBreakerWindowNotCanonical`]
        // (a4ae535 on the two `:politicas` axes). The zero-floor arm
        // strictly precedes this canonical gate so `Duration::ZERO`
        // (`subsec_nanos() == 0` — a value the canonical arm would
        // otherwise accept) surfaces the more self-locating
        // `RestartWindowZero` diagnostic with its omit-axis
        // remediation directly named, peer to the
        // `WallClockZero` → `WallClockNotCanonical` and
        // `PolicyTimeoutZero` → `PolicyTimeoutNotCanonical` cross-arm
        // ordering on the sibling axes.
        if let Some(w) = self.restart_window
            && !duration_codec::is_integer_millisecond_duration(w)
        {
            return Err(SupervisorError::RestartWindowNotCanonical { window: w });
        }
        // Upper-bound gate on `:restart-window`: every validated value
        // must also fit within the typed-`Duration` ceiling
        // ([`SUPERVISOR_RESTART_WINDOW_MAX`] = 1h = 3600s). The zero-floor,
        // canonical-form, and this cap arm together bracket the typed
        // `:restart-window` axis structurally — every validated value
        // lies in `1ms..=SUPERVISOR_RESTART_WINDOW_MAX`, integer-millisecond
        // granularity. Until this gate landed the shared duration codec
        // accepted any `Duration` past zero with integer-ms residue
        // (the parser's only upper bound was `u64::MAX` seconds, the
        // largest value the `<integer>h` form can encode), so
        // `(:supervisor (:restart-window "24h"))` round-tripped cleanly
        // through serde and the per-supervisor CSE invariant (no
        // `:restart-window` the operator's `MaxIntensity / Period`
        // reconciler can't honor as a meaningful rolling-window reset)
        // was a runtime, not build-time, contract. Same trajectory as
        // the three sibling typed-`Duration` cap lifts on
        // [`crate::LIMITS_WALL_CLOCK_MAX`] (per-process wasm-engine
        // deadline), [`crate::POLICY_TIMEOUT_MAX`] (per-edge mesh-policy
        // deadline), and [`crate::POLICY_BREAKER_WINDOW_MAX`]
        // (per-breaker rolling-window) — the four typed-`Duration` axes
        // now share a single uniform top edge at the codec's largest
        // emitted unit so the next typed-slot wiring (the future
        // wasm-operator's per-supervisor `MaxIntensity / Period`
        // reconciler, the M4 `mesh.pleme.io/v1alpha1/Supervisor` CR
        // materializer's admission webhook, the `caixa-operator`'s
        // hierarchical reconciliation scheduler) reaches for any of the
        // four knowing the value is in `1ms..=1h` without re-validating
        // at the renderer layer. The canonical-form arm strictly
        // precedes this cap so a `Duration` that is *both*
        // sub-millisecond and above-cap surfaces the more fundamental
        // round-trip-shape diagnostic first (the cap's `1ms..=1h`
        // remediation would be misleading when no integer-ms form of
        // the offending value exists). Same posture every peer
        // zero-then-shape-then-cap chain uses on this surface
        // (`WallClockZero` → `WallClockNotCanonical` →
        // `WallClockExceedsCap`, `PolicyTimeoutZero` →
        // `PolicyTimeoutNotCanonical` → `PolicyTimeoutExceedsCap`,
        // `PolicyBreakerZeroWindow` → `PolicyBreakerWindowNotCanonical`
        // → `PolicyBreakerWindowExceedsCap`).
        if let Some(w) = self.restart_window
            && w > SUPERVISOR_RESTART_WINDOW_MAX
        {
            return Err(SupervisorError::RestartWindowExceedsCap { window: w });
        }
        let mut seen = std::collections::HashSet::new();
        for child in &self.children {
            if child.caixa.is_empty() {
                return Err(SupervisorError::EmptyChildName);
            }
            // Every emitted cluster artifact's `metadata.name` for a
            // supervised child derives from this `:children :caixa` value
            // verbatim — the rendered `wasm.pleme.io/v1alpha1/ComputeUnit
            // .metadata.name` per child, the [`crate::LABEL_PROGRAM`]
            // label value on every child's pod identity, and the per-
            // child K8s [`Service`][svc] `metadata.name` the future
            // wasm-operator (M3) provisions for inter-child supervision
            // tree wiring. Each apiserver-side schema on each landing
            // site enforces the DNS-1123 label rule on admission; a
            // structurally invalid child name (`"Worker"`, `"my_worker"`,
            // `"team.worker"`, `"-worker"`, `"worker-"`, the >63-byte
            // UUID-shaped mistaken-identity slug) silently passes the
            // prior empty-/duplicate-only gate and the failure surfaces
            // at `kubectl apply` time as a `metadata.name: Invalid value`
            // rejection, far from the source caixa.lisp, with no field
            // naming the offending `:children` entry. Lifting the gate
            // to caixa-build time mirrors the `:membros :caixa` value-
            // shape trajectory (3f9d7a0) and the `:placement :clusters`
            // trajectory (6cbb900) onto the third DNS-1123-label-shaped
            // identifier axis — the supervisor tree's child names —
            // through the lifted [`crate::render::is_dns_1123_label`]
            // predicate (the "before its third occurrence" boundary the
            // PRIME DIRECTIVE duplication-budget rule draws, THEORY.md
            // §I.3.5).
            //
            // [svc]: https://kubernetes.io/docs/concepts/services-networking/service/
            if let Err(reason) = crate::render::is_dns_1123_label(&child.caixa) {
                return Err(SupervisorError::ChildCaixaInvalid {
                    caixa: child.caixa.clone(),
                    reason,
                });
            }
            if child.versao.is_empty() {
                return Err(SupervisorError::EmptyChildVersion {
                    caixa: child.caixa.clone(),
                });
            }
            // The author surface for `:children :versao` is the same
            // Cargo-shaped semver requirement string `:deps :versao` and
            // `:membros :versao` carry — and the lacre pipeline resolves
            // all three axes through the same
            // [`crate::version::parse_requirement`] entry-point. Until
            // this gate landed `validate` only refused the empty string
            // (`EmptyChildVersion`); a malformed-but-non-empty
            // requirement (`"^bad-version"`, `"^^0.1"`, the canonical
            // git-tag-shape-leaking-into-:versao `"v0.1"` typo, `">="`,
            // the accidental `"abc"`) silently passed validate and the
            // `semver::Error` surfaced at lacre-resolve time, far from
            // the source caixa.lisp, with no field naming which
            // `:children` entry carried the typo. Mirroring 9888b13's
            // `:membros :versao` lift onto the third `:versao` typed
            // axis: every `ChildSpec::versao` past validate is
            // round-trippable through [`crate::parse_requirement`]
            // without re-checking at the resolver layer, and the three
            // `:versao` typed surfaces (`:deps`, `:membros`, `:children`)
            // are now structurally equivalent.
            if let Err(e) = crate::parse_requirement(&child.versao) {
                return Err(SupervisorError::ChildVersaoInvalid {
                    caixa: child.caixa.clone(),
                    versao: child.versao.clone(),
                    reason: e.to_string(),
                });
            }
            if !seen.insert(child.caixa.as_str()) {
                return Err(SupervisorError::DuplicateChildCaixa {
                    caixa: child.caixa.clone(),
                });
            }
        }
        Ok(())
    }
}

/// Cross-slot coherence gate on the supervision tree: no
/// `:children :caixa` entry may name the supervisor's own `:nome`.
///
/// A supervisor that lists itself as a child is a degenerate self-parent
/// — the supervision tree is a DAG rooted at the supervisor (OTP child
/// specs reference *distinct* child processes; a supervisor is never its
/// own child), and the wasm-operator's hierarchical reconciliation would
/// otherwise be handed a node that is its own parent: a one-node cycle it
/// either rejects far from the source `caixa.lisp` or recurses on. Because
/// every `:nome` is a globally-unique substrate identity (DNS-1123 label +
/// lacre closure root), a child whose `:caixa` equals the supervisor's
/// `:nome` *is* the supervisor itself, not a coincidentally-named peer.
///
/// Lives outside [`SupervisorSpec::validate`] because the typed view
/// carries the children but not the parent `:nome`; mirrors the
/// cross-slot precedence gate `validate_upgrade_from_against_versao`
/// (which likewise reads one slot against another at the
/// [`crate::layout`] wire-up site) and the mesh self-edge gate
/// `AplicacaoSpec`'s `ContratoSelfLoop` — the same "an edge from a graph
/// node to itself is structurally not a tree/mesh edge" discipline, here
/// on the supervision-tree axis.
pub fn validate_no_self_supervision(
    children: &[ChildSpec],
    parent_nome: &str,
) -> Result<(), SupervisorError> {
    for child in children {
        if child.caixa == parent_nome {
            return Err(SupervisorError::ChildSupervisesSelf {
                caixa: parent_nome.to_string(),
            });
        }
    }
    Ok(())
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SupervisorError {
    #[error("supervisor :estrategia {estrategia:?} requires at least one :children entry")]
    NoChildren { estrategia: RestartStrategy },
    #[error(
        "SimpleOneForOne supervisors must declare zero static children (children spawn dynamically)"
    )]
    SimpleOneForOneWithStaticChildren,
    #[error(":max-restarts must be > 0")]
    ZeroMaxRestarts,
    #[error(
        ":supervisor :max-restarts ({max_restarts}) exceeds the supervisor-policy ceiling \
         (SUPERVISOR_MAX_RESTARTS_MAX = 1000) — a value above this cap turns the typed \
         restart-intensity policy into a no-op supervisor: the escalation threshold is \
         structurally so high that no realistic restarts-per-:restart-window traffic shape \
         can reach it, so the supervisor never escalates to its parent and a bad child can \
         loop inside the window indefinitely. Every typed-slot consumer (Erlang/OTP's \
         MaxIntensity/Period ratio, the future wasm-operator's per-supervisor \
         restart-intensity counter, the M4 mesh.pleme.io/v1alpha1/Supervisor CR \
         materializer's admission webhook) emits a `:max-restarts` declaration that is \
         structurally never reached. Pin a value in 1..=1000 (Erlang/OTP / Elixir / Riak \
         Core / RabbitMQ production playbooks recommend 3..=100; the OTP `supervisor` \
         callback module's `MaxR = 1` minimal-restart default sits at the bottom of the \
         band) or restructure the supervision tree (split the flaky child into its own \
         sub-supervisor with a tighter budget) if you need a higher restart tolerance."
    )]
    MaxRestartsExceedsCap { max_restarts: u32 },
    #[error(
        ":restart-window must be > 0 when set — Erlang/OTP's MaxIntensity/Period \
         requires Period > 0; a zero window either trips on the first failure or \
         never trips depending on operator interpretation. Omit :restart-window to \
         express `never reset`; carry a positive duration to express the window."
    )]
    RestartWindowZero,
    #[error(
        ":supervisor :restart-window ({window:?}) carries a sub-millisecond residue the shared `duration_codec` cannot round-trip — \
         the codec truncates to `as_millis()` before picking the canonical unit, so a value with `subsec_nanos() % 1_000_000 != 0` either \
         truncates on first serialize (e.g. `Duration::from_micros(1500)` → \"1ms\" → `Duration::from_millis(1)` ≠ original) or renders \
         as \"0s\" the `RestartWindowZero` arm then rejects on re-validate. Pin an integer-millisecond magnitude in the canonical authoring form \
         (`<integer><unit>` for unit ∈ {{ms, s, m, h}}, e.g. `\"500ms\"`, `\"30s\"`, `\"2m\"`, `\"1h\"`) or omit the field for `never reset`"
    )]
    RestartWindowNotCanonical { window: Duration },
    #[error(
        ":supervisor :restart-window ({window:?}) exceeds the supervisor-policy ceiling \
         (SUPERVISOR_RESTART_WINDOW_MAX = 1h = 3600s) — a value above this cap turns the typed \
         per-supervisor rolling-window restart-intensity counter into a lifetime counter: the \
         failure-counting window is structurally so long that transient restarts are never \
         forgotten, the MaxIntensity/Period ratio degenerates from `trip the parent supervisor \
         when the child has exceeded its restart budget within the recent window` to `trip the \
         parent when the child has exceeded its restart budget over its lifetime`, and the \
         supervisor's reset semantic never reaches the child — every typed-slot consumer \
         (Erlang/OTP's MaxIntensity/Period reconciler, the future wasm-operator's \
         per-supervisor restart-intensity counter, the M4 mesh.pleme.io/v1alpha1/Supervisor CR \
         materializer's admission webhook, the caixa-operator's hierarchical reconciliation \
         scheduler) emits a `:restart-window` declaration that is structurally a no-op rolling \
         window. Pin a value in 1ms..=1h (Learn You Some Erlang's `{{intensity, 5, 60}}` \
         worker-supervisor `Period = 60s` default, Elixir's `Supervisor` `max_seconds: 5` \
         default, OTP's `supervisor` callback module `MaxT = 5..=60` typical, Riak Core's \
         `MaxT ∈ 10s..=300s`, RabbitMQ broker-supervisor `MaxT = 5s` default — every Erlang/OTP \
         / Elixir production playbook sits in the 5s..=300s band; the longest documented \
         per-supervisor restart-window any pleme-io substrate playbook recommends maxes at \
         ~30m) or omit :restart-window to express `never reset` (the supervisor's restart \
         budget then becomes a strict lifetime counter by design, not a degenerate one — the \
         author surfaces the lifetime-counter semantic explicitly at the slot, rather than \
         hiding it behind a rolling-window declaration the cap arm rejects)"
    )]
    RestartWindowExceedsCap { window: Duration },
    #[error("child entry has empty :caixa name")]
    EmptyChildName,
    #[error(
        "child :caixa {caixa:?} is not a valid DNS-1123 label: {reason} \
         (the K8s apiserver enforces this rule on every `metadata.name` / Service \
         name / label value the child name lands in — the per-child \
         `wasm.pleme.io/v1alpha1/ComputeUnit.metadata.name`, the `LABEL_PROGRAM` \
         label value, and the future wasm-operator per-child Service `metadata.name` \
         — each apiserver-side schema rejects names that don't match; use a \
         lowercase alphanumeric + hyphen identifier like `\"worker\"` or `\"cache-v2\"`)"
    )]
    ChildCaixaInvalid { caixa: String, reason: String },
    #[error("child {caixa:?} has empty :versao constraint")]
    EmptyChildVersion { caixa: String },
    #[error(
        "child {caixa:?} :versao {versao:?} is not a valid semver requirement: \
         {reason} (use Cargo-shaped forms like `\"^0.1\"`, `\"~0.1.2\"`, \
         `\"0.1.0\"`, or `\"*\"` — the same shape `:deps :versao` and \
         `:membros :versao` carry; the lacre pipeline resolves all three \
         through the same parser)"
    )]
    ChildVersaoInvalid {
        caixa: String,
        versao: String,
        reason: String,
    },
    #[error(
        "child {caixa:?} appears more than once (Erlang/OTP requires unique \
         child_spec.id per supervisor; duplicate children materialize as duplicate \
         ComputeUnits in the rendered chart, one silently overwriting the other)"
    )]
    DuplicateChildCaixa { caixa: String },
    #[error(
        "supervisor {caixa:?} lists itself as a :children entry — a supervisor is \
         never its own child (the supervision tree is a DAG rooted at the supervisor; \
         OTP child specs reference distinct child processes). Since every :nome is a \
         globally-unique substrate identity, a child naming the supervisor's own :nome \
         is a one-node reconciliation cycle, not a coincidentally-named peer; drop the \
         self-referential :children entry or rename it to the actual child caixa."
    )]
    ChildSupervisesSelf { caixa: String },
}

/// Shared duration string codec for the typed slots that take a
/// duration (`restart_window`, `MeshPolicy::timeout`,
/// `CircuitBreaker::window`, …). Public so [`crate::aplicacao`] can
/// reuse it without duplicating the parser.
pub mod duration_codec {
    use super::Duration;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(d) => s.serialize_str(&render(*d)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        let opt: Option<String> = Option::deserialize(d)?;
        match opt {
            None => Ok(None),
            Some(s) => parse(&s).map(Some).map_err(serde::de::Error::custom),
        }
    }

    pub(crate) fn parse(s: &str) -> Result<Duration, String> {
        // Whitespace-rejection arm — peer with the leading-`+` /
        // fractional-magnitude arm below (`"+30s"`, `"1.5s"`) and the
        // leading-zero arm below (`"030s"`) on the same canonical-form
        // render-determinism axis. Until this gate landed the parser
        // silently tolerated leading / trailing / internal whitespace
        // via the top-level `s.trim()` at parse entry and the per-part
        // `num_part.trim()` / `unit.trim()` calls below, so every
        // whitespace-carrying shape (`" 30s"` — paste-from-aligned-doc
        // / YAML-quoted-plain-scalar leading-space; `"30s "` —
        // paste-from-shell-history trailing-space; `"30 s"` —
        // paste-from-typography whitespace-between-magnitude-and-unit;
        // `"30\ts"` — peer tab byte between magnitude and unit;
        // `"30s\n"` — trailing newline from a multi-line paste;
        // `"\t30s"` — paste-from-indented-doc / YAML-block-scalar tab
        // byte) parsed to the same `Duration::from_secs(30)` and serde
        // silently round-tripped to `"30s"` on the next emit (a
        // *different* canonical string) — breaking the THEORY.md Part V
        // render-determinism contract on three typed-duration slots at
        // once (`:supervisor :restart-window`, `:politicas :timeout`,
        // `:politicas :circuit-breaker :window`) via the shared codec.
        //
        // The canonical author shape is `<integer><unit>` (or
        // `<integer>` for the bare-integer-as-seconds shorthand) with
        // no whitespace bytes anywhere — every string [`render`] emits
        // carries none, so the parser's accepted set must match for
        // serialize / deserialize to round-trip losslessly. This gate
        // makes the pre-existing `s.trim()` / `num_part.trim()` /
        // `unit.trim()` calls below strict no-ops on the accepted set
        // (every byte-position match they would perform is now already
        // trimmed away by the accepted set itself), while the arm
        // surfaces every rejected whitespace-carrying shape with a
        // self-locating diagnostic naming the offending byte and the
        // canonical form the author intended, peer with every prior
        // canonical-form-drift arm on this codec.
        //
        // `u8::is_ascii_whitespace()` covers the five ASCII whitespace
        // bytes every downstream YAML / JSON / TOML parser can feed
        // through a quoted-scalar value verbatim — space (`0x20`), tab
        // (`0x09`), LF (`0x0A`), FF (`0x0C`), CR (`0x0D`) — the
        // WhatWG-conformant "ASCII whitespace" set (deliberately
        // narrower than POSIX's `[:space:]` which also admits VT
        // `0x0B`). This is the exact byte set the sibling
        // `crate::aplicacao::rate_limit_codec` (1ad7755, the immediate
        // predecessor on the whitespace-rejection axis) refuses on the
        // `:politicas :rate-limit` codec; this gate lifts the same
        // discipline onto the shared duration codec that backs three
        // typed-duration slots at once, extending the closed
        // canonical-form-drift set from
        // `{fractional, signed, leading-zero}` to
        // `{fractional, signed, leading-zero, whitespace-anywhere}` on
        // the shared duration codec.
        //
        // Peer with the future whitespace-rejection arms on the two
        // remaining typed-magnitude codecs the trajectory
        // acknowledges: `limits::parse_duration` backing
        // `:limits :wall-clock`, `limits::parse_byte_size` backing
        // `:limits :memory` — each carries the same
        // whitespace-tolerance drift today; this gate lands the
        // discipline on the shared duration codec first because it
        // is the closest peer on the trajectory to the
        // `rate_limit_codec` predecessor (both are `caixa-core`
        // typed-magnitude codecs the M3 `:politicas` axis routes
        // through, and both close the whitespace-drift class on the
        // same THEORY.md Part V render-determinism contract).
        if let Some(b) = s.bytes().find(|b| b.is_ascii_whitespace()) {
            return Err(format!(
                "duration: value {s:?} contains whitespace byte 0x{b:02x} — the canonical \
                 authoring form for the typed duration slots routed through this shared codec \
                 (`:supervisor :restart-window`, `:politicas :timeout`, \
                 `:politicas :circuit-breaker :window`) is `<integer><unit>` (e.g. \
                 `\"30s\"`, `\"500ms\"`, `\"2m\"`, `\"1h\"`) with no whitespace bytes \
                 anywhere. A whitespace-carrying shape (`\" 30s\"`, `\"30s \"`, `\"30 s\"`, \
                 `\"\\t30s\"`, `\"30s\\n\"`) round-trips through `render` to a *different* \
                 canonical form (`\"30s\"`) on first serialize — breaking the THEORY.md \
                 Part V render-determinism contract every typed slot carries. Strip every \
                 whitespace byte (write `\"30s\"` verbatim)"
            ));
        }
        let s = s.trim();
        let split = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
        let (num_part, unit) = s.split_at(split);
        let num_trim = num_part.trim();
        // The canonical authoring form for every typed slot routed
        // through this shared codec — `:supervisor :restart-window`,
        // `:politicas :timeout`, `:politicas :circuit-breaker :window`
        // — is `<integer><unit>`. Every magnitude [`render`] emits is a
        // non-negative integer with no decimal point and no leading
        // sign, so the parser's accepted set must match for
        // serialize/deserialize to round-trip without canonical-form
        // drift. Until this gate landed the parser accepted any
        // `f64`-shaped magnitude (`"1.5s"` → 1500ms, `"1.0s"` → 1s,
        // `"0.5m"` → 30s, `"+30s"` → 30s) and serde silently round-
        // tripped the value to a *different* canonical string on the
        // next emit (`"1.5s"` → 1500ms → `"1500ms"`, `"1.0s"` → 1s →
        // `"1s"`, `"0.5m"` → 30s → `"30s"`, `"+30s"` → 30s → `"30s"`)
        // — breaking the THEORY.md Part V render-determinism contract
        // on three typed slots at once. Same canonical-form discipline
        // `crate::limits::parse_duration` (818dd38, the immediate
        // predecessor on the peer `:limits :wall-clock` codec) applies;
        // this gate lifts the discipline onto the shared codec that
        // backs the remaining three typed-duration slots in caixa-core.
        //
        // Strict canonical form: every byte of the magnitude is an
        // ASCII digit (no `.`, no `+`, no `-`). On non-digit-only
        // inputs the gate distinguishes "non-canonical-but-numeric"
        // (parses as f64 or i64 — surfaced with a self-locating
        // diagnostic naming the canonical authoring form, the
        // round-trip drift each rejected shape would produce on first
        // serialize, and the canonical-form remediation) from
        // "garbage" (parses as neither — surfaced with the existing
        // narrower "bad duration magnitude" wording so its diagnostic
        // shape remains stable for the parser-shape footgun case).
        // The pre-existing `num < 0.0` arm is now unreachable — the
        // digit-only gate strictly precedes magnitude parsing, and a
        // leading `-` is not an ASCII digit, so `"-30s"` lands on the
        // non-canonical-but-numeric branch with the `-30` named
        // verbatim in the diagnostic rather than the prior
        // value-laundered "negative duration in \"-30s\"" wording.
        let digit_only = !num_trim.is_empty() && num_trim.bytes().all(|b| b.is_ascii_digit());
        if !digit_only {
            let numeric = num_trim.parse::<f64>().is_ok() || num_trim.parse::<i64>().is_ok();
            if numeric {
                return Err(format!(
                    "duration: magnitude {num_trim:?} is not a non-negative integer — the \
                     canonical authoring form for the typed duration slots routed through \
                     this shared codec (`:supervisor :restart-window`, `:politicas :timeout`, \
                     `:politicas :circuit-breaker :window`) is `<integer><unit>` (e.g. \
                     `\"30s\"`, `\"500ms\"`, `\"2m\"`, `\"1h\"`) with no decimal point and \
                     no leading `+` / `-` sign. A fractional / decimal-shaped magnitude \
                     (`\"1.5s\"`, `\"1.0s\"`, `\"0.5m\"`, `\"+30s\"`, `\"-30s\"`) round-trips \
                     through `render` to a *different* canonical form (`\"1500ms\"`, `\"1s\"`, \
                     `\"30s\"`, `\"30s\"`, `\"30s\"`) on first serialize — breaking the \
                     THEORY.md Part V render-determinism contract every typed slot carries. \
                     Pick an integer magnitude in the unit that divides cleanly (write \
                     `\"1500ms\"` instead of `\"1.5s\"`; `\"30s\"` instead of `\"0.5m\"`)"
                ));
            }
            return Err(format!("bad duration magnitude in {s:?}"));
        }
        // Leading-zero arm — peer with the `rate_limit_codec` leading-
        // zero arm (4f46830) on the same canonical-form render-
        // determinism axis. The digit-only gate accepts `"030s"`,
        // `"00s"`, `"01h"`, `"0500ms"` as `u64::from_str` parses them
        // losslessly (= 30, 0, 1, 500), but `render` emits the leading-
        // zero-stripped form (`"30s"`, `"0s"`, `"1h"`, `"500ms"`) — a
        // *different* canonical string on the next emit, breaking the
        // THEORY.md Part V render-determinism contract the same way
        // `"+30s"` did before the leading-`+` arm landed. The single-
        // byte magnitude `"0"` (or `"0s"` / `"0ms"`) round-trips
        // losslessly through `render` (`render(Duration::ZERO)` emits
        // `"0s"`) — the downstream semantic-zero gates (e.g.
        // `SupervisorError::ZeroRestartWindow` on
        // `:supervisor :restart-window`,
        // `AplicacaoError::PolicyTimeoutZero` /
        // `PolicyCircuitBreakerWindowZero` on the typed `:politicas`
        // duration slots) refuse zero-magnitude authoring at the typed-
        // validate layer above, so the single-byte `"0"` stays in the
        // accepted set at this codec layer and the diagnostic
        // partitioning between canonical-form drift (this arm) and
        // semantic-zero (the downstream gates) remains stable.
        // Peer with the future leading-zero arms on the two remaining
        // typed-magnitude codecs the trajectory acknowledges:
        // `limits::parse_duration` backing `:limits :wall-clock`,
        // `limits::parse_byte_size` backing `:limits :memory` — each
        // carries the same canonical-form-drift class today; this
        // gate lands the discipline on the shared duration codec
        // first because the `rate_limit_codec` predecessor on the
        // same canonical-form-drift axis is the closest peer on the
        // trajectory.
        if num_trim.len() > 1 && num_trim.as_bytes()[0] == b'0' {
            return Err(format!(
                "duration: magnitude {num_trim:?} has a non-canonical leading zero — the \
                 canonical authoring form for the typed duration slots routed through \
                 this shared codec (`:supervisor :restart-window`, `:politicas :timeout`, \
                 `:politicas :circuit-breaker :window`) is `<integer><unit>` (e.g. \
                 `\"30s\"`, `\"500ms\"`, `\"2m\"`, `\"1h\"`) with no leading-zero padding \
                 on the magnitude. A leading-zero magnitude (`\"030s\"`, `\"00s\"`, \
                 `\"01h\"`, `\"0500ms\"`) round-trips through `render` to a *different* \
                 canonical form (`\"30s\"`, `\"0s\"`, `\"1h\"`, `\"500ms\"`) on first \
                 serialize — breaking the THEORY.md Part V render-determinism contract \
                 every typed slot carries. Strip the leading zeros (write \
                 `\"30s\"` instead of `\"030s\"`)"
            ));
        }
        // The digit-only gate guarantees every byte is `[0-9]`, and
        // the leading-zero arm above guarantees the magnitude is
        // either the single byte `"0"` or starts with `[1-9]`, so
        // the only way `u64::from_str` can fail here is overflow (the
        // magnitude exceeds `u64::MAX`). Surface that with an
        // overflow-shaped wording so the diagnostic names the offending
        // magnitude verbatim rather than collapsing onto the
        // non-canonical arm. The codec now operates on `u64` end-to-end
        // — every accepted magnitude is integer-exact; no f64 mantissa
        // drift between author-supplied magnitude and the consumer's
        // `Duration` value. Same shape `crate::limits::parse_duration`
        // (818dd38) carries on the peer `:limits :wall-clock` axis.
        let num: u64 = num_trim.parse::<u64>().map_err(|_| {
            format!("bad duration magnitude in {s:?} (digit-only magnitude overflows u64)")
        })?;
        let unit_trim = unit.trim();
        let dur = match unit_trim {
            "ms" => Duration::from_millis(num),
            "s" | "" => Duration::from_secs(num),
            "m" => Duration::from_secs(num.checked_mul(60).ok_or_else(|| {
                format!("duration {num}{unit_trim} overflows u64 (magnitude × 60 > 2^64-1)")
            })?),
            "h" => Duration::from_secs(num.checked_mul(3600).ok_or_else(|| {
                format!("duration {num}{unit_trim} overflows u64 (magnitude × 3600 > 2^64-1)")
            })?),
            other => return Err(format!("unknown duration unit {other:?}")),
        };
        Ok(dur)
    }

    /// Render a [`Duration`] in the canonical pleme-io duration string
    /// form (`"30s"`, `"1m"`, `"1h"`, `"500ms"`). The same form every
    /// caixa typed-duration slot serializes to and the same form K8s
    /// Gateway API HTTPRoute `timeouts` / `backendRequest` and Cilium
    /// EnvoyConfig per-route timeouts both expect (an integer
    /// followed by `s`/`m`/`h`/`ms`, no fractional values, no leading
    /// `+`). Lifted to `pub` so caixa-side renderers
    /// (`caixa-mesh::gateway_routes`'s :politicas :timeout overlay,
    /// the future per-:politicas `CiliumClusterwideEnvoyConfig`
    /// emitter, the future caixa-otel collector pipeline emitter) can
    /// consume the same canonical formatter without re-inlining the
    /// magnitude/unit decision tree (and inheriting the same drift
    /// footguns: a subtly different `300ms` vs `0.3s` rendering breaks
    /// downstream apply-time parsing in non-obvious ways).
    pub fn render(d: Duration) -> String {
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

    /// True iff `d` round-trips losslessly through [`render`] + [`parse`].
    ///
    /// [`render`] truncates a `Duration` to `as_millis()` before picking the
    /// largest divisor unit, so any sub-millisecond residue
    /// (`d.subsec_nanos() % 1_000_000 != 0`) silently breaks the THEORY.md
    /// §V.2.7 render-determinism contract:
    ///
    ///   - `Duration::from_micros(1500)` (= `1_500_000` ns) → `as_millis() == 1`
    ///     → renders `"1ms"` → parses back to `Duration::from_millis(1)` =
    ///     `1_000_000` ns ≠ original `1_500_000` ns;
    ///   - `Duration::from_nanos(1)` (= 1 ns) → `as_millis() == 0` →
    ///     renders the literal `"0s"`, which the per-axis zero-floor gate
    ///     on every typed-`Duration` slot then rejects on re-validate.
    ///
    /// Lifted to a `pub` predicate next to the [`render`] / [`parse`] pair so
    /// the codec's round-trippable accepted set lives in exactly one place —
    /// every typed-`Duration` slot that routes through this shared codec
    /// (`SupervisorSpec::restart_window` via [`super::duration_codec`],
    /// [`crate::MeshPolicy::timeout`] / [`crate::CircuitBreaker::window`] via
    /// `supervisor::duration_codec` + [`super::duration_codec_required`]) and
    /// every typed-`Duration` slot whose own codec shares the same
    /// `as_millis()`-truncation shape ([`crate::LimitsSpec::wall_clock`] via
    /// [`crate::limits`]'s in-module `parse_duration` / `render_duration`
    /// pair) calls this predicate from its `validate()` to bracket the
    /// accepted set against the codec's accepted set, structurally. Drift
    /// between the codec's granularity and any typed slot's accepted set is
    /// then a single-source-of-truth edit at this predicate rather than a
    /// silent round-trip break the next consumer discovers at apply time.
    ///
    /// Peer of [`crate::aplicacao::POLICY_RETRIES_MAX`] /
    /// [`crate::LIMITS_MEMORY_WASM32_MAX_BYTES`] and the
    /// `is_dns_1123_label` / `is_canonical_rate_limit_window` predicate
    /// family — same "typed-slot's valid set matches its codec's accepted
    /// set, structurally" discipline carried at the codec layer.
    #[must_use]
    pub fn is_integer_millisecond_duration(d: Duration) -> bool {
        d.subsec_nanos().is_multiple_of(1_000_000)
    }
}

/// Required-Duration variant for fields that aren't Option<Duration>.
pub mod duration_codec_required {
    use super::Duration;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::duration_codec::render(*v))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let s = String::deserialize(d)?;
        super::duration_codec::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn child(name: &str, ver: &str, restart: RestartPolicy) -> ChildSpec {
        ChildSpec {
            caixa: name.into(),
            versao: ver.into(),
            restart,
        }
    }

    #[test]
    fn default_has_one_for_one_and_5_restarts_in_60s() {
        let s = SupervisorSpec::default();
        assert_eq!(s.estrategia, RestartStrategy::OneForOne);
        assert_eq!(s.max_restarts, 5);
        assert_eq!(s.restart_window, Some(Duration::from_secs(60)));
        assert!(s.children.is_empty());
    }

    #[test]
    fn validate_one_for_one_requires_children() {
        let mut s = SupervisorSpec::default();
        s.children = vec![];
        assert!(matches!(
            s.validate().unwrap_err(),
            SupervisorError::NoChildren { .. }
        ));
        s.children = vec![child("worker", "^0.1", RestartPolicy::Permanent)];
        s.validate().unwrap();
    }

    #[test]
    fn validate_simple_one_for_one_forbids_static_children() {
        let mut s = SupervisorSpec {
            estrategia: RestartStrategy::SimpleOneForOne,
            ..SupervisorSpec::default()
        };
        s.children
            .push(child("w", "^0.1", RestartPolicy::Permanent));
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::SimpleOneForOneWithStaticChildren
        );
        s.children.clear();
        s.validate().unwrap();
    }

    #[test]
    fn validate_rejects_zero_max_restarts() {
        let s = SupervisorSpec {
            max_restarts: 0,
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(s.validate().unwrap_err(), SupervisorError::ZeroMaxRestarts);
    }

    // ── upper-cap: SUPERVISOR_MAX_RESTARTS_MAX brackets the typed slot ─────
    //
    // The cap arm lifts the `:politicas :circuit-breaker :max-failures` /
    // `POLICY_BREAKER_MAX_FAILURES_MAX` (2b51ace) discipline onto the peer
    // `:supervisor :max-restarts` axis — both fields are "trip the
    // next-higher protection layer after N events in a rolling window"
    // counters with identical degenerate-at-the-high-end shape, so the
    // typed-slot's accepted set lies in `1..=1000` on the supervisor side
    // exactly as it lies in `1..=1000` on the breaker side.

    #[test]
    fn validate_rejects_max_restarts_above_cap() {
        // The fail-before-pass-after pin: `SUPERVISOR_MAX_RESTARTS_MAX +
        // 1` is structurally one past the cap and silently passed
        // validate on every pre-gate codebase because the typed slot's
        // only check was the zero-floor arm. The no-op-supervisor vector
        // only surfaced at the runtime substrate (Erlang/OTP
        // MaxIntensity/Period ratio, the future wasm-operator's
        // per-supervisor restart-intensity counter) far from the source
        // caixa.lisp with no field naming the offending supervisor.
        let s = SupervisorSpec {
            max_restarts: SUPERVISOR_MAX_RESTARTS_MAX + 1,
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::MaxRestartsExceedsCap {
                max_restarts: SUPERVISOR_MAX_RESTARTS_MAX + 1,
            }
        );
    }

    #[test]
    fn validate_rejects_max_restarts_far_above_cap() {
        // The `u32::MAX` worst case — the four-billion-restart
        // threshold a typo (`:max-restarts 4294967295`) or a
        // struct-literal copy-paste lands in the slot. Pin the cap
        // arm's coverage explicitly across the full `u32` overflow so
        // a future relaxation that drops the upper bound surfaces
        // here. Same shape every other typed-cap arm on this surface
        // carries (POLICY_BREAKER_MAX_FAILURES_MAX,
        // POLICY_RETRIES_MAX, POLICY_RATE_LIMIT_MAX).
        let s = SupervisorSpec {
            max_restarts: u32::MAX,
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::MaxRestartsExceedsCap {
                max_restarts: u32::MAX,
            }
        );
    }

    #[test]
    fn validate_accepts_max_restarts_at_cap() {
        // The boundary value — exactly SUPERVISOR_MAX_RESTARTS_MAX —
        // must validate. The cap is inclusive on the top edge,
        // matching the POLICY_BREAKER_MAX_FAILURES_MAX /
        // POLICY_RETRIES_MAX / LIMITS_MEMORY_WASM32_MAX_BYTES
        // discipline on the sibling capped axes. Pin the boundary
        // explicitly so a future off-by-one tightening
        // (`>= SUPERVISOR_MAX_RESTARTS_MAX` instead of `>`) surfaces
        // here as a test failure rather than a silent contract
        // narrowing.
        let s = SupervisorSpec {
            max_restarts: SUPERVISOR_MAX_RESTARTS_MAX,
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        s.validate()
            .expect("max_restarts == SUPERVISOR_MAX_RESTARTS_MAX must validate");
    }

    #[test]
    fn validate_accepts_max_restarts_typical_values() {
        // The documented production-playbook band positive-control
        // sweep — every value Erlang/OTP / Elixir / Riak Core /
        // RabbitMQ recommend (1..=100) must pass, plus a sweep
        // through the hyperscale band (200, 500, 1000) the cap
        // accepts. Pin the inclusive validated set explicitly so a
        // future tightening of the ceiling surfaces here.
        for n in [1u32, 3, 5, 10, 20, 50, 100, 200, 500, 1000] {
            let s = SupervisorSpec {
                max_restarts: n,
                children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
                ..SupervisorSpec::default()
            };
            s.validate()
                .unwrap_or_else(|e| panic!("max_restarts={n} must validate; got {e:?}"));
        }
    }

    #[test]
    fn zero_max_restarts_takes_precedence_over_cap() {
        // The cross-arm ordering pin: `0` is structurally outside
        // both `1..` (zero-floor) and `..=SUPERVISOR_MAX_RESTARTS_MAX`
        // (cap), but the zero-floor diagnostic is the more
        // self-locating one (it directly names the counter-axis
        // remediation), so the validate gate must fire on zero first.
        // Same shape every other zero-then-shape ordering on this
        // surface uses (PolicyRetriesZero then
        // PolicyRetriesExceedsCap; PolicyBreakerZeroFailures then
        // PolicyBreakerMaxFailuresExceedsCap).
        let s = SupervisorSpec {
            max_restarts: 0,
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::ZeroMaxRestarts,
            "max_restarts == 0 must surface the zero-floor diagnostic, not the cap diagnostic"
        );
    }

    #[test]
    fn max_restarts_cap_takes_precedence_over_restart_window_gates() {
        // The cross-arm ordering pin between the cap and the sibling
        // `:restart-window` gates (zero-window, canonical-window). A
        // supervisor carrying both an over-cap `max_restarts` AND a
        // structurally invalid window (zero, sub-ms) must surface the
        // cap diagnostic first — the cap arm is wired immediately
        // after the zero-restart arm and strictly before the window
        // arms, so the offending value the diagnostic names matches
        // the order the author would discover the gates by reading
        // top-to-bottom through `SupervisorSpec::validate`. Pin the
        // order so a future refactor that reorders the arms surfaces
        // here as a test failure rather than a silent diagnostic
        // regression. Peer of
        // `circuit_breaker_max_failures_cap_takes_precedence_over_window_gates`
        // on the sibling `:politicas :circuit-breaker` slot.
        let s = SupervisorSpec {
            max_restarts: SUPERVISOR_MAX_RESTARTS_MAX + 1,
            restart_window: Some(Duration::ZERO),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::MaxRestartsExceedsCap {
                max_restarts: SUPERVISOR_MAX_RESTARTS_MAX + 1,
            },
            "over-cap max_restarts must surface the cap diagnostic before any window-axis diagnostic"
        );
    }

    #[test]
    fn max_restarts_cap_diagnostic_carries_offending_value() {
        // The diagnostic-shape pin: the offending `u32` is carried
        // verbatim into the `SupervisorError::MaxRestartsExceedsCap`
        // variant so the surfaced error message names the value the
        // author wrote (`":supervisor :max-restarts (50000) exceeds the
        // supervisor-policy ceiling …"`), not just the cap. Same
        // self-locating diagnostic shape every other typed-cap arm on
        // this surface carries
        // (`AplicacaoError::PolicyBreakerMaxFailuresExceedsCap` carries
        // the offending failure count verbatim,
        // `AplicacaoError::PolicyRetriesExceedsCap` carries the offending
        // retries count verbatim).
        let s = SupervisorSpec {
            max_restarts: 50_000,
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::MaxRestartsExceedsCap {
                    max_restarts: 50_000
                }
            ),
            "got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("50000"),
            ":supervisor :max-restarts cap diagnostic must carry the offending value verbatim (got: {msg})"
        );
    }

    #[test]
    fn supervisor_max_restarts_cap_pins_canonical_value() {
        // The SUPERVISOR_MAX_RESTARTS_MAX constant pins the value at
        // 1000 — the same ceiling the peer
        // POLICY_BREAKER_MAX_FAILURES_MAX cap carries on the
        // `:politicas :circuit-breaker :max-failures` axis (both are
        // "trip the next-higher protection layer after N events in a
        // rolling window" counters with identical
        // degenerate-at-the-high-end shape; uniform top edge so the
        // M4 CR materializers and the wasm-operator reconciler reach
        // for either field knowing the value is in `1..=1000`). Two
        // orders of magnitude above every documented Erlang/OTP /
        // Elixir / Riak Core / RabbitMQ production-playbook
        // recommendation band and below the clearly-pathological
        // "effectively no escalation" floor (10_000, 100_000,
        // u32::MAX). Pinning the literal value here surfaces a future
        // drift (a relaxation to 10_000, a tightening to 100) as a
        // deliberate test edit, not a silent contract narrowing.
        assert_eq!(SUPERVISOR_MAX_RESTARTS_MAX, 1000);
    }

    #[test]
    fn validate_rejects_empty_child_name() {
        let s = SupervisorSpec {
            children: vec![child("", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(s.validate().unwrap_err(), SupervisorError::EmptyChildName);
    }

    #[test]
    fn validate_rejects_empty_child_version() {
        let s = SupervisorSpec {
            children: vec![child("w", "", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert!(matches!(
            s.validate().unwrap_err(),
            SupervisorError::EmptyChildVersion { .. }
        ));
    }

    // ── value-shape: parse-as-VersionReq on :children :versao ─────────────

    #[test]
    fn validate_rejects_invalid_child_versao_requirement() {
        // The fail-before-pass-after pin: a non-empty but malformed
        // semver requirement (`"^bad-version"`) silently passed
        // `validate()` on every pre-gate codebase because the prior
        // shape only refused the empty string. The parse failure
        // surfaced far downstream at lacre-resolve time with a
        // `semver::Error` that didn't name which `:children` entry
        // carried the typo. The new gate moves the check to caixa-build
        // time at the source caixa.lisp — the third `:versao` typed
        // axis (`:children`) joins `:deps` and `:membros` (9888b13) at
        // structural parity.
        let s = SupervisorSpec {
            children: vec![
                child("worker", "^0.1", RestartPolicy::Permanent),
                child("cache", "^bad-version", RestartPolicy::Transient),
            ],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildVersaoInvalid { ref caixa, ref versao, .. }
                    if caixa == "cache" && versao == "^bad-version"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_child_versao_with_double_caret_typo() {
        // `"^^0.1"` is the canonical doubled-caret typo — looks
        // Cargo-shaped on first glance but fails the parser because
        // semver doesn't accept stacked operators. Pin this
        // adjacent-shape footgun explicitly so a future relaxation that
        // accepts "looks-canonical-but-isn't" forms surfaces here.
        let s = SupervisorSpec {
            children: vec![child("worker", "^^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildVersaoInvalid { ref caixa, ref versao, .. }
                    if caixa == "worker" && versao == "^^0.1"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_child_versao_with_v_prefixed_tag() {
        // `"v0.1"` is the canonical "git-tag-shape leaking into the
        // semver requirement slot" typo — an author copies the
        // publish-side git-tag string verbatim into `:versao`, but
        // Cargo's semver parser rejects the leading `v`. Same
        // adjacent-shape footgun pinned for `:membros :versao`
        // (9888b13).
        let s = SupervisorSpec {
            children: vec![child("worker", "v0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildVersaoInvalid { ref caixa, ref versao, .. }
                    if caixa == "worker" && versao == "v0.1"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_accepts_canonical_child_versao_forms() {
        // The Cargo-shaped requirement forms `:deps :versao` and
        // `:membros :versao` already accept via
        // `crate::parse_requirement` must pass the children gate
        // without re-validating at the resolver layer. Pin every leg so
        // a future tightening of the canonical set surfaces here as a
        // test failure.
        for form in [
            "^0.1",      // caret — minor-range pin (the most common shape)
            "~0.1.2",    // tilde — patch-range pin
            "0.1.0",     // exact — single-version pin
            "*",         // wildcard — any version (semver::VersionReq::STAR)
            ">=0.1, <2", // multi-range — comma-separated comparators
        ] {
            let s = SupervisorSpec {
                children: vec![child("worker", form, RestartPolicy::Permanent)],
                ..SupervisorSpec::default()
            };
            s.validate()
                .unwrap_or_else(|e| panic!("canonical form {form:?} must validate, got {e:?}"));
        }
    }

    #[test]
    fn child_versao_empty_takes_precedence_over_invalid() {
        // Order pin: the existing `EmptyChildVersion` diagnostic (which
        // doesn't try to parse) fires before the new
        // `ChildVersaoInvalid` parse-side diagnostic, so an empty
        // `:versao` keeps its narrower error message —
        // `parse_requirement` would also reject `""`, but the
        // empty-string arm is the more self-locating diagnostic for the
        // author. Same ordering discipline as
        // `membro_versao_empty_takes_precedence_over_invalid` in
        // aplicacao.rs.
        let s = SupervisorSpec {
            children: vec![child("worker", "", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, SupervisorError::EmptyChildVersion { ref caixa } if caixa == "worker"),
            "got {err:?}"
        );
    }

    #[test]
    fn child_versao_invalid_fires_before_duplicate_check() {
        // Order pin: a malformed requirement on a non-duplicate entry
        // surfaces *its own* diagnostic (which names the offending
        // `:versao` string), even when a later entry would otherwise
        // collapse onto an earlier name. The per-entry shape gate runs
        // inline before the duplicate-key insert — parallel to
        // `membro_versao_invalid_fires_before_duplicate_check` in
        // aplicacao.rs and the b0c8389 / c4213a4 ordering discipline.
        let s = SupervisorSpec {
            children: vec![
                child("worker", "^bad", RestartPolicy::Permanent),
                child("cache", "^0.1", RestartPolicy::Transient),
                child("worker", "^0.2", RestartPolicy::Permanent), // would otherwise raise DuplicateChildCaixa
            ],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildVersaoInvalid { ref caixa, .. } if caixa == "worker"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn child_versao_invalid_diagnostic_carries_offending_versao() {
        // The diagnostic-shape pin: the error names the offending
        // `:versao` value verbatim so the author can grep their
        // caixa.lisp without re-running the build, and carries a
        // non-empty `reason` from `semver::VersionReq::parse` so the
        // parser's own wording flows through to the diagnostic.
        let s = SupervisorSpec {
            children: vec![child("worker", "not-a-req", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        let SupervisorError::ChildVersaoInvalid {
            caixa,
            versao,
            reason,
        } = err
        else {
            panic!("expected ChildVersaoInvalid, got other variant");
        };
        assert_eq!(caixa, "worker");
        assert_eq!(versao, "not-a-req");
        assert!(
            !reason.is_empty(),
            "ChildVersaoInvalid `reason` must carry the parser's wording verbatim"
        );
    }

    // ── value-shape: DNS-1123 label rule on :children :caixa ──────────────

    #[test]
    fn validate_rejects_child_caixa_with_uppercase() {
        // The canonical "I copied the Servico's display name verbatim"
        // typo — child caixa names are lowercase per K8s DNS-1123 label
        // rule. The diagnostic names the offending name and suggests the
        // lower-cased fix in one edit, mirroring the
        // `rejects_membro_caixa_with_uppercase` gate's shape (3f9d7a0).
        let s = SupervisorSpec {
            children: vec![child("Worker", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        let SupervisorError::ChildCaixaInvalid { caixa, reason } = err else {
            panic!("expected ChildCaixaInvalid, got other variant");
        };
        assert_eq!(caixa, "Worker");
        assert!(
            reason.contains("uppercase"),
            "diagnostic must name the violation as `uppercase` (got: {reason:?})"
        );
        assert!(
            reason.contains("\"worker\""),
            "diagnostic must suggest the lower-cased fix verbatim (got: {reason:?})"
        );
    }

    #[test]
    fn validate_rejects_child_caixa_with_underscore() {
        // The canonical "I'm thinking of a Python module / Postgres
        // table" leak — `_` is forbidden by every DNS-1123 / DNS-1035
        // label schema. K8s rejects `metadata.name: my_worker` at
        // admission time with an opaque `field is invalid` (no source-
        // citing diagnostic). The gate moves it to caixa-build time.
        let s = SupervisorSpec {
            children: vec![child("my_worker", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildCaixaInvalid { ref caixa, ref reason }
                    if caixa == "my_worker" && reason.contains('_')
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_child_caixa_with_dot() {
        // A `:children :caixa` entry is a single DNS-1123 label, not a
        // subdomain. The K8s Service / ComputeUnit `metadata.name` rules
        // forbid dots. Same shape as `rejects_membro_caixa_with_dot`
        // (3f9d7a0) on the peer name axis.
        let s = SupervisorSpec {
            children: vec![child("team.worker", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildCaixaInvalid { ref caixa, ref reason }
                    if caixa == "team.worker" && reason.contains('.')
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_child_caixa_with_leading_hyphen() {
        // DNS-1123 / DNS-1035 boundary rule: labels must start and end
        // with an alphanumeric. The K8s apiserver rejects `-worker`
        // outright; the renderer would emit a `metadata.name: "-worker"`
        // that fails admission far from the source caixa.lisp.
        let s = SupervisorSpec {
            children: vec![child("-worker", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildCaixaInvalid { ref caixa, ref reason }
                    if caixa == "-worker" && reason.contains("start and end")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_child_caixa_with_trailing_hyphen() {
        // The symmetric arm of the boundary rule. Pin separately so
        // both ends of the label are covered against a future relaxation
        // that only checks one boundary.
        let s = SupervisorSpec {
            children: vec![child("worker-", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildCaixaInvalid { ref caixa, .. }
                    if caixa == "worker-"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_child_caixa_with_unicode() {
        // DNS-1123 is ASCII-only; IDN must be pre-encoded as Punycode
        // (`xn--…`) by the author before it reaches K8s. The byte-by-
        // byte ASCII validity check rejects multi-byte UTF-8 sequences
        // by the first byte that fails the `[a-z0-9-]` predicate.
        let s = SupervisorSpec {
            children: vec![child("café", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildCaixaInvalid { ref caixa, .. }
                    if caixa == "café"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_child_caixa_with_whitespace() {
        // Whitespace is the canonical "I pasted from a sketch / doc"
        // footgun. The apiserver rejects every `metadata.name` value
        // carrying whitespace; pin the gate fires at the right boundary.
        let s = SupervisorSpec {
            children: vec![child("my worker", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildCaixaInvalid { ref caixa, .. }
                    if caixa == "my worker"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_child_caixa_too_long() {
        // The 64-byte boundary pin. DNS-1123 / DNS-1035 cap labels at
        // 63 bytes; the K8s apiserver rejects every `metadata.name`
        // axis over the limit at admission time. The diagnostic names
        // both the cap and the actual length so the author can shorten
        // in one edit, mirroring `rejects_membro_caixa_too_long`
        // (3f9d7a0) and `rejects_placement_cluster_too_long` (6cbb900).
        let too_long = "a".repeat(64);
        let s = SupervisorSpec {
            children: vec![child(&too_long, "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        let SupervisorError::ChildCaixaInvalid { caixa, reason } = err else {
            panic!("expected ChildCaixaInvalid, got other variant");
        };
        assert_eq!(caixa, too_long);
        assert!(
            reason.contains("63"),
            "diagnostic must name the 63-byte cap (got: {reason:?})"
        );
        assert!(
            reason.contains("64"),
            "diagnostic must name the actual length (got: {reason:?})"
        );
    }

    #[test]
    fn child_caixa_max_length_validates() {
        // The 63-byte boundary control pin — exactly-at-the-cap is
        // accepted, mirroring `membro_caixa_max_length_validates`
        // (3f9d7a0) and `placement_cluster_max_length_validates`
        // (6cbb900). Pinned separately so a future off-by-one tightening
        // surfaces here.
        let max_label = "a".repeat(63);
        let s = SupervisorSpec {
            children: vec![child(&max_label, "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        s.validate().unwrap();
    }

    #[test]
    fn validate_accepts_canonical_child_caixa_forms() {
        // The realistic shapes a supervised child's `:caixa` carries —
        // single-word `worker`, version-suffixed `cache-v2`, single-char
        // `a`, two-char `db`, digit-start `2-pool`, longer hyphen-joined
        // `payment-retry`, all-digit `0`. Pin every leg so a future
        // tightening (e.g. requiring a leading lowercase letter) surfaces
        // here as a test failure. Mirrors `accepts_canonical_membro_caixa_forms`
        // (3f9d7a0) and `accepts_canonical_placement_cluster_forms`
        // (6cbb900).
        for form in [
            "worker",
            "cache-v2",
            "a",
            "db",
            "2-pool",
            "payment-retry",
            "0",
        ] {
            let s = SupervisorSpec {
                children: vec![child(form, "^0.1", RestartPolicy::Permanent)],
                ..SupervisorSpec::default()
            };
            s.validate()
                .unwrap_or_else(|e| panic!("canonical form {form:?} must validate, got {e:?}"));
        }
    }

    #[test]
    fn child_caixa_empty_takes_precedence_over_invalid() {
        // Order pin: the existing `EmptyChildName` diagnostic (which
        // doesn't try to parse the DNS-1123 shape) fires before the new
        // `ChildCaixaInvalid` per-axis gate, so an empty `:caixa` keeps
        // its narrower error message — `is_dns_1123_label` would reject
        // the empty string too (boundary check on the first byte), but
        // the empty-string arm is the more self-locating diagnostic for
        // the author. Same ordering discipline as
        // `membro_caixa_empty_takes_precedence_over_invalid` in
        // aplicacao.rs.
        let s = SupervisorSpec {
            children: vec![child("", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert_eq!(err, SupervisorError::EmptyChildName);
    }

    #[test]
    fn child_caixa_invalid_fires_before_versao_check() {
        // Order pin: the per-axis shape gate runs inline before the
        // per-entry versao check, so a malformed `:caixa` on an entry
        // whose `:versao` would also fail surfaces the more self-
        // locating name-axis diagnostic first. Parallel to
        // `membro_versao_invalid_fires_before_duplicate_check` (9888b13)
        // and `placement_cluster_invalid_fires_before_duplicate_check`
        // (6cbb900).
        let s = SupervisorSpec {
            children: vec![child("My_Worker", "", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildCaixaInvalid { ref caixa, .. } if caixa == "My_Worker"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn child_caixa_invalid_fires_before_duplicate_check() {
        // Order pin: a malformed name on a non-duplicate entry surfaces
        // its own diagnostic, even when a later entry would otherwise
        // collapse onto an earlier name. The per-entry shape gate runs
        // inline before the duplicate-key HashSet insert, mirroring
        // `placement_cluster_invalid_fires_before_duplicate_check`
        // (6cbb900).
        let s = SupervisorSpec {
            children: vec![
                child("Worker", "^0.1", RestartPolicy::Permanent),
                child("cache", "^0.1", RestartPolicy::Transient),
                child("worker", "^0.2", RestartPolicy::Permanent), // would otherwise raise DuplicateChildCaixa
            ],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildCaixaInvalid { ref caixa, .. } if caixa == "Worker"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn child_caixa_invalid_diagnostic_carries_offending_caixa() {
        // The diagnostic-shape pin: the error names the offending
        // `:caixa` verbatim plus a non-empty parser-shaped `reason` so
        // the author can grep their caixa.lisp without re-running the
        // build. Mirrors the diagnostic-shape sweep on every prior
        // value-shape gate (3f9d7a0, 6cbb900, c7d05ec).
        let s = SupervisorSpec {
            children: vec![child("My_Worker", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        let SupervisorError::ChildCaixaInvalid { caixa, reason } = err else {
            panic!("expected ChildCaixaInvalid, got other variant");
        };
        assert_eq!(caixa, "My_Worker");
        assert!(
            !reason.is_empty(),
            "ChildCaixaInvalid `reason` must carry the parser's wording verbatim"
        );
    }

    // ── value-shape: zero restart_window + duplicate child names ──────────

    #[test]
    fn validate_accepts_none_restart_window() {
        // Omitted `:restart-window` is the "never reset" sentinel —
        // valid by design. Mirrors :limits axes where None = unbounded.
        let s = SupervisorSpec {
            restart_window: None,
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        s.validate().unwrap();
    }

    #[test]
    fn validate_rejects_zero_restart_window() {
        // Same "0 means the opposite of what you think" footgun closed
        // for :politicas :timeout (Envoy treats 0s as infinite) and
        // :limits :wall-clock (wasmtime traps before the call starts).
        // Erlang/OTP's MaxIntensity/Period requires Period > 0.
        let s = SupervisorSpec {
            restart_window: Some(Duration::ZERO),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::RestartWindowZero
        );
    }

    // ── value-shape: integer-ms canonical-form on :restart-window ─────────
    //
    // The fourth (and last) typed-`Duration` axis in caixa-core to get
    // the integer-millisecond canonical-form gate — peer with
    // `:limits :wall-clock` (82fc3ef), `:politicas :timeout` (a4ae535),
    // and `:politicas :circuit-breaker :window` (a4ae535). The serde
    // path is already gated at the shared codec layer (see
    // `restart_window_serde_rejects_fractional_seconds`); this arm
    // closes the programmatic-struct-literal path the codec gate can't
    // see.

    #[test]
    fn validate_rejects_sub_millisecond_restart_window() {
        // The fail-before-pass-after pin: a programmatic
        // `Duration::from_micros(1500)` (= 1_500_000 ns) silently passed
        // `validate` on every pre-gate codebase, then truncated to
        // `as_millis() == 1` on first serialize — the shared codec
        // emits `"1ms"`, parses it back to `Duration::from_millis(1)` =
        // 1_000_000 ns, the typed `restart_window` no longer matches
        // its rendered form.
        let s = SupervisorSpec {
            restart_window: Some(Duration::from_micros(1500)),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        match s.validate().unwrap_err() {
            SupervisorError::RestartWindowNotCanonical { window } => {
                assert_eq!(window, Duration::from_micros(1500));
            }
            other => panic!("expected RestartWindowNotCanonical, got {other:?}"),
        }
    }

    #[test]
    fn validate_rejects_one_nanosecond_restart_window() {
        // The far-sub-ms case: `Duration::from_nanos(1)` is non-zero
        // (so `RestartWindowZero` doesn't fire) but `as_millis() == 0`,
        // so the shared codec emits the literal `"0s"` — the next
        // serde round-trip would parse back to `Duration::ZERO`, which
        // the `RestartWindowZero` arm then rejects on re-validate. The
        // canonical-form gate at this layer surfaces a self-locating
        // diagnostic naming the offending Duration verbatim rather
        // than a downstream `RestartWindowZero` whose remediation
        // points at omitting the slot.
        let s = SupervisorSpec {
            restart_window: Some(Duration::from_nanos(1)),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        match s.validate().unwrap_err() {
            SupervisorError::RestartWindowNotCanonical { window } => {
                assert_eq!(window, Duration::from_nanos(1));
            }
            other => panic!("expected RestartWindowNotCanonical, got {other:?}"),
        }
    }

    #[test]
    fn validate_rejects_nanosecond_past_canonical_boundary_restart_window() {
        // The 1-ns-past-1ms boundary case: a `Duration` carrying
        // 1_000_001 ns is structurally past the integer-ms granularity
        // floor — `subsec_nanos() % 1_000_000 == 1`. The codec round-
        // trip would truncate to `1ms` and the consumer would observe
        // a 1-ns drift on every emit. Same boundary the peer
        // `validate_rejects_nanosecond_past_canonical_boundary` test
        // in limits.rs pins for the `:limits :wall-clock` axis.
        let w = Duration::from_nanos(1_000_001);
        let s = SupervisorSpec {
            restart_window: Some(w),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::RestartWindowNotCanonical { window: w }
        );
    }

    #[test]
    fn validate_accepts_integer_millisecond_restart_window_values() {
        // The positive-control sweep: every `Duration` the shared
        // codec can round-trip losslessly — the canonical
        // `<integer>{ms,s,m,h}` set the codec's `render` / `parse`
        // pair emits and accepts — passes `validate` without
        // surfacing the new canonical-form arm. Mirrors
        // `validate_accepts_integer_millisecond_wall_clock_values` on
        // the sibling `:limits :wall-clock` axis.
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
            let s = SupervisorSpec {
                restart_window: Some(w),
                children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
                ..SupervisorSpec::default()
            };
            s.validate()
                .unwrap_or_else(|e| panic!("integer-ms {w:?} must validate, got {e:?}"));
        }
    }

    #[test]
    fn validate_restart_window_zero_takes_precedence_over_canonical_gate() {
        // Cross-arm ordering pin: `Duration::ZERO` has
        // `subsec_nanos() == 0` and would otherwise pass the
        // canonical-form arm — the zero-floor arm must fire first so
        // the more self-locating `RestartWindowZero` diagnostic (with
        // its omit-axis remediation directly named) leads. Same
        // posture every peer zero-then-shape gate uses
        // (`WallClockZero` → `WallClockNotCanonical`,
        // `PolicyTimeoutZero` → `PolicyTimeoutNotCanonical`,
        // `PolicyBreakerZeroWindow` → `PolicyBreakerWindowNotCanonical`).
        let s = SupervisorSpec {
            restart_window: Some(Duration::ZERO),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::RestartWindowZero
        );
    }

    #[test]
    fn restart_window_canonical_diagnostic_carries_offending_duration() {
        // Diagnostic-shape pin: the canonical-form arm names the
        // offending `Duration` verbatim so the author's grep lands on
        // the field's value, not a generic "duration not canonical"
        // message. Same shape every other typed-canonical-form arm
        // on this surface carries (`WallClockNotCanonical` carries
        // the offending `Duration` verbatim,
        // `PolicyTimeoutNotCanonical` carries the offending
        // `Duration` verbatim).
        let w = Duration::from_micros(500);
        let s = SupervisorSpec {
            restart_window: Some(w),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("500"),
            "diagnostic must carry the offending magnitude verbatim (got {msg:?})"
        );
        assert!(
            msg.contains("sub-millisecond"),
            "diagnostic must name the sub-millisecond residue class (got {msg:?})"
        );
    }

    #[test]
    fn restart_window_validated_value_round_trips_through_codec() {
        // The structural property the canonical-ms gate enforces:
        // every `SupervisorSpec::restart_window` past
        // `SupervisorSpec::validate` round-trips losslessly through
        // the shared duration codec (serialize → string →
        // deserialize → equal value). Pin this end-to-end so a future
        // change to either side (the validate gate's accepted
        // granularity, the codec's parse/render unit set) that breaks
        // the alignment surfaces here. Peer of
        // `wall_clock_validated_value_round_trips_through_codec` on
        // the sibling `:limits :wall-clock` axis.
        for w in [
            Duration::from_millis(1),
            Duration::from_millis(1500),
            Duration::from_secs(30),
            Duration::from_secs(3600),
        ] {
            let s = SupervisorSpec {
                restart_window: Some(w),
                children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
                ..SupervisorSpec::default()
            };
            s.validate().unwrap();
            let json = serde_json::to_string(&s).unwrap();
            let back: SupervisorSpec = serde_json::from_str(&json).unwrap();
            assert_eq!(back.restart_window, Some(w));
        }
    }

    // ── value-shape: upper cap on :restart-window ─────────────────────────
    //
    // The fourth (and last) typed-`Duration` axis in caixa-core to get
    // the 1h upper cap — peer with `:limits :wall-clock` (51e0dbd),
    // `:politicas :timeout` (2e8ee7e), and `:politicas
    // :circuit-breaker :window` (379a814). Brackets the typed
    // `:restart-window` axis structurally: every validated value lies
    // in `1ms..=SUPERVISOR_RESTART_WINDOW_MAX`, integer-millisecond
    // granularity, closing the
    // rolling-window-degenerates-to-lifetime-counter footgun the prior
    // zero-floor-and-canonical-form-only checks left open.

    #[test]
    fn validate_rejects_restart_window_above_cap() {
        // The fail-before-pass-after pin: 3601s = 1h + 1s is
        // structurally one canonical-tick past the
        // [`SUPERVISOR_RESTART_WINDOW_MAX`] ceiling (1h = 3600s) — an
        // integer-millisecond magnitude the canonical-form arm above
        // accepts cleanly, that the shared duration codec round-trips
        // losslessly as `"3601s"`, and that silently passed validate on
        // every pre-gate codebase because the typed slot's only checks
        // were the zero-floor and canonical-form arms. The runtime
        // substrate consuming the value (Erlang/OTP's MaxIntensity/
        // Period reconciler, the future wasm-operator's per-supervisor
        // restart-intensity counter) reaches for a `Duration` so long
        // no realistic restart-recovery pattern resets the counter,
        // far from the source caixa.lisp.
        let w = SUPERVISOR_RESTART_WINDOW_MAX + Duration::from_secs(1);
        let s = SupervisorSpec {
            restart_window: Some(w),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::RestartWindowExceedsCap { window: w }
        );
    }

    #[test]
    fn validate_rejects_restart_window_one_millisecond_above_cap() {
        // Boundary case: exactly 1ms past the cap (the granularity the
        // canonical-form gate enforces). Catches a future "strictly
        // less than" half-measure and pins the diagnostic to name the
        // offending `Duration` verbatim. Peer of
        // `validate_rejects_wall_clock_one_millisecond_above_cap` /
        // `rejects_policy_timeout_one_millisecond_above_cap` /
        // `rejects_circuit_breaker_window_one_millisecond_above_cap`
        // on the sibling typed-`Duration` axes' top edges.
        let w = SUPERVISOR_RESTART_WINDOW_MAX + Duration::from_millis(1);
        let s = SupervisorSpec {
            restart_window: Some(w),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::RestartWindowExceedsCap { window: w }
        );
    }

    #[test]
    fn validate_rejects_restart_window_far_above_cap() {
        // The "obvious authoring footgun" case: a `(:restart-window "24h")`,
        // `(:restart-window "7d")`, or any "I want a lifetime counter
        // but wrote a `<integer>h` magnitude anyway" typo — values the
        // canonical-form arm accepts as integer-millisecond magnitudes,
        // the codec round-trips losslessly through serde, but the
        // operator's `MaxIntensity / Period` reconciler cannot honor
        // as a meaningful rolling window. Until this gate landed
        // validate accepted them. Pin the common above-cap values (24h,
        // 7d, ~11.5d) so a future relaxation that drops the upper bound
        // surfaces here.
        for w in [
            Duration::from_secs(86_400),    // 24h
            Duration::from_secs(604_800),   // 7d
            Duration::from_secs(1_000_000), // ~11.5 days
        ] {
            let s = SupervisorSpec {
                restart_window: Some(w),
                children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
                ..SupervisorSpec::default()
            };
            assert_eq!(
                s.validate().unwrap_err(),
                SupervisorError::RestartWindowExceedsCap { window: w }
            );
        }
    }

    #[test]
    fn validate_accepts_restart_window_at_cap() {
        // The boundary value — exactly [`SUPERVISOR_RESTART_WINDOW_MAX`]
        // (1h) — must validate. The cap is inclusive on the top edge,
        // matching the [`crate::LIMITS_WALL_CLOCK_MAX`] /
        // [`crate::POLICY_TIMEOUT_MAX`] /
        // [`crate::POLICY_BREAKER_WINDOW_MAX`] discipline on the sibling
        // capped axes. Pin the boundary explicitly so a future
        // off-by-one tightening (`>= SUPERVISOR_RESTART_WINDOW_MAX`
        // instead of `>`) surfaces here as a test failure rather than a
        // silent contract narrowing.
        let s = SupervisorSpec {
            restart_window: Some(SUPERVISOR_RESTART_WINDOW_MAX),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        s.validate()
            .expect("restart_window == SUPERVISOR_RESTART_WINDOW_MAX must validate");
    }

    #[test]
    fn validate_accepts_restart_window_typical_values() {
        // The documented Erlang/OTP / Elixir / Riak Core / RabbitMQ
        // per-supervisor production-playbook band positive-control
        // sweep — every value Learn You Some Erlang's `{intensity, 5,
        // 60}` worker-supervisor `Period = 60s` default, Elixir's
        // `Supervisor` `max_seconds: 5` default, OTP's `supervisor`
        // callback module `MaxT = 5..=60` typical, Riak Core's `MaxT ∈
        // 10s..=300s`, and RabbitMQ broker-supervisor `MaxT = 5s`
        // default recommend (5s..=300s) must pass, plus a sweep
        // through the long-tail-flaky-pool band (5m, 15m, 30m, 1h) the
        // cap accepts. Mirrors `validate_accepts_wall_clock_typical_values`
        // on the sibling `:limits :wall-clock` axis.
        for w in [
            Duration::from_millis(1),
            Duration::from_millis(500),
            Duration::from_secs(1),
            Duration::from_secs(5),  // RabbitMQ broker-supervisor default
            Duration::from_secs(10), // Riak Core lower
            Duration::from_secs(30),
            Duration::from_secs(60),  // Learn You Some Erlang default
            Duration::from_secs(120), // OTP supervisor MaxT typical
            Duration::from_secs(300), // Riak Core upper
            Duration::from_secs(900), // 15m
            Duration::from_secs(1800),
            Duration::from_secs(3600), // exactly 1h, the cap
        ] {
            let s = SupervisorSpec {
                restart_window: Some(w),
                children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
                ..SupervisorSpec::default()
            };
            s.validate()
                .unwrap_or_else(|e| panic!("restart_window={w:?} must validate; got {e:?}"));
        }
    }

    #[test]
    fn restart_window_zero_takes_precedence_over_cap() {
        // The cross-arm ordering pin: `Duration::ZERO` is structurally
        // outside both `>= 1ms` (zero-floor) and `<=
        // SUPERVISOR_RESTART_WINDOW_MAX` (cap), but the zero-floor
        // diagnostic is the more self-locating one (it directly names
        // the omit-axis remediation), so the validate gate must fire
        // on zero first. Same shape every other zero-then-cap ordering
        // on this surface uses (`WallClockZero` then
        // `WallClockExceedsCap`, `PolicyTimeoutZero` then
        // `PolicyTimeoutExceedsCap`, `PolicyBreakerZeroWindow` then
        // `PolicyBreakerWindowExceedsCap`).
        let s = SupervisorSpec {
            restart_window: Some(Duration::ZERO),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::RestartWindowZero,
            "Duration::ZERO must surface the zero-floor diagnostic, not the cap diagnostic"
        );
    }

    #[test]
    fn restart_window_canonical_takes_precedence_over_cap() {
        // The cross-arm ordering pin: a `Duration` that is *both*
        // sub-millisecond (non-canonical-form) and structurally above
        // the cap surfaces the canonical-form diagnostic first,
        // because the round-trip-shape break is the more fundamental
        // issue (the value can't even round-trip through the codec,
        // so the cap diagnostic naming `1ms..=1h` would be misleading
        // — there's no integer-ms form of the offending value). Pin
        // the order so a future refactor that reorders the arms
        // surfaces here as a test failure rather than a silent
        // diagnostic regression. Peer of
        // `wall_clock_canonical_takes_precedence_over_cap` /
        // `policy_timeout_canonical_takes_precedence_over_cap`.
        let w = SUPERVISOR_RESTART_WINDOW_MAX + Duration::from_nanos(1);
        let s = SupervisorSpec {
            restart_window: Some(w),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::RestartWindowNotCanonical { window: w },
            "sub-ms above-cap value must surface the canonical-form diagnostic, not the cap diagnostic"
        );
    }

    #[test]
    fn max_restarts_cap_takes_precedence_over_restart_window_cap() {
        // The cross-arm ordering pin between the `:max-restarts` cap
        // and the sibling `:restart-window` cap. A supervisor carrying
        // both an over-cap `max_restarts` AND an over-cap window must
        // surface the `MaxRestartsExceedsCap` diagnostic first — the
        // cap arm is wired immediately after the zero-restart arm and
        // strictly before every window-axis arm (zero / canonical /
        // cap), so the offending value the diagnostic names matches
        // the order the author would discover the gates by reading
        // top-to-bottom through `SupervisorSpec::validate`. Pin the
        // order so a future refactor that reorders the arms surfaces
        // here as a test failure rather than a silent diagnostic
        // regression. Peer of
        // `max_restarts_cap_takes_precedence_over_restart_window_gates`
        // on the sibling zero / canonical window arms.
        let w = SUPERVISOR_RESTART_WINDOW_MAX + Duration::from_secs(1);
        let s = SupervisorSpec {
            max_restarts: SUPERVISOR_MAX_RESTARTS_MAX + 1,
            restart_window: Some(w),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::MaxRestartsExceedsCap {
                max_restarts: SUPERVISOR_MAX_RESTARTS_MAX + 1,
            },
            "over-cap max_restarts must surface the cap diagnostic before any window-axis diagnostic"
        );
    }

    #[test]
    fn restart_window_cap_diagnostic_carries_offending_value() {
        // The diagnostic-shape pin: the offending `Duration` is
        // carried verbatim into the
        // [`SupervisorError::RestartWindowExceedsCap`] variant so the
        // surfaced error message names the value the author wrote,
        // not just the cap. Same self-locating diagnostic shape every
        // other typed-cap arm on this surface carries
        // (`WallClockExceedsCap` carries the offending `Duration`
        // verbatim, `PolicyTimeoutExceedsCap` carries the offending
        // `Duration` verbatim, `PolicyBreakerWindowExceedsCap` carries
        // the offending `Duration` verbatim).
        let w = Duration::from_secs(7200); // 2h
        let s = SupervisorSpec {
            restart_window: Some(w),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, SupervisorError::RestartWindowExceedsCap { window } if window == w),
            "got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("7200"),
            ":supervisor :restart-window cap diagnostic must carry the offending value verbatim (got: {msg})"
        );
    }

    #[test]
    fn supervisor_restart_window_cap_pins_canonical_value() {
        // The SUPERVISOR_RESTART_WINDOW_MAX constant pins the value at
        // exactly 1 hour (3600s = 3_600_000ms) — the largest unit the
        // shared duration codec emits as a clean canonical string
        // (`"<n>h"`). Pinning the literal value here surfaces a future
        // drift (a relaxation to 24h, a tightening to 5m) as a
        // deliberate test edit, not a silent contract narrowing.
        //
        // The four typed-`Duration` caps on the validation surface
        // (`LIMITS_WALL_CLOCK_MAX` per-process, `POLICY_TIMEOUT_MAX`
        // per-edge, `POLICY_BREAKER_WINDOW_MAX` per-breaker,
        // `SUPERVISOR_RESTART_WINDOW_MAX` per-supervisor) share a
        // single uniform top edge at the codec's largest emitted unit
        // — a structural-property invariant the equality assertions
        // here enshrine, so a future drift on any of the four
        // surfaces as a deliberate test edit. Same shape every other
        // typed-cap value pin uses
        // (`wall_clock_cap_pins_canonical_value`,
        // `policy_timeout_cap_pins_canonical_value`,
        // `circuit_breaker_window_cap_pins_canonical_value`).
        assert_eq!(SUPERVISOR_RESTART_WINDOW_MAX, Duration::from_secs(3600));
        assert_eq!(SUPERVISOR_RESTART_WINDOW_MAX.as_millis(), 3_600_000);
        assert_eq!(SUPERVISOR_RESTART_WINDOW_MAX, crate::LIMITS_WALL_CLOCK_MAX);
        assert_eq!(SUPERVISOR_RESTART_WINDOW_MAX, crate::POLICY_TIMEOUT_MAX);
        assert_eq!(
            SUPERVISOR_RESTART_WINDOW_MAX,
            crate::POLICY_BREAKER_WINDOW_MAX
        );
    }

    #[test]
    fn restart_window_cap_value_round_trips_through_codec() {
        // The codec round-trip property the cap arm preserves: the
        // [`SUPERVISOR_RESTART_WINDOW_MAX`] constant itself round-trips
        // through the shared duration codec — every value at the cap
        // serializes to the canonical `"1h"` form and parses back
        // identically. Pin the round-trip so a future change to the
        // codec's unit set or to the cap's magnitude that breaks the
        // round-trip property surfaces here. Peer of
        // `wall_clock_cap_value_round_trips_through_codec` on the
        // sibling `:limits :wall-clock` axis.
        let s = SupervisorSpec {
            restart_window: Some(SUPERVISOR_RESTART_WINDOW_MAX),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        s.validate().unwrap();
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            json.contains("\"1h\""),
            "SUPERVISOR_RESTART_WINDOW_MAX must serialize to the canonical `\"1h\"` form (got {json})"
        );
        let back: SupervisorSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.restart_window, Some(SUPERVISOR_RESTART_WINDOW_MAX));
    }

    #[test]
    fn validate_rejects_duplicate_child_caixa() {
        // Two children with the same :caixa render to two ComputeUnits
        // with the same name in the cluster's HelmRelease values —
        // one silently overwrites the other. Erlang/OTP's child_spec.id
        // is required-unique per supervisor; same set-not-multiset
        // discipline applied here as for :membros / :placement
        // :clusters / :entrada :paths.
        let s = SupervisorSpec {
            children: vec![
                child("worker", "^0.1", RestartPolicy::Permanent),
                child("cache", "^0.1", RestartPolicy::Transient),
                child("worker", "^0.2", RestartPolicy::Permanent),
            ],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, SupervisorError::DuplicateChildCaixa { ref caixa } if caixa == "worker"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_duplicate_child_diagnostic_names_first_collision() {
        // Iteration walks the :children list in declaration order —
        // the diagnostic names the first repeat, deterministically,
        // even when multiple names duplicate.
        let s = SupervisorSpec {
            children: vec![
                child("a", "^0.1", RestartPolicy::Permanent),
                child("b", "^0.1", RestartPolicy::Permanent),
                child("a", "^0.1", RestartPolicy::Permanent),
                child("b", "^0.1", RestartPolicy::Permanent),
            ],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, SupervisorError::DuplicateChildCaixa { ref caixa } if caixa == "a"),
            "got {err:?}"
        );
    }

    // ── self-supervision cross-slot gate ──────────────────────────

    #[test]
    fn validate_no_self_supervision_rejects_self_referential_child() {
        // A supervisor whose `:children` lists its own `:nome` is a
        // one-node reconciliation cycle — rejected, naming the parent.
        let children = vec![
            child("worker", "^0.1", RestartPolicy::Permanent),
            child("orquestra", "^0.1", RestartPolicy::Permanent),
        ];
        let err = validate_no_self_supervision(&children, "orquestra").unwrap_err();
        assert!(
            matches!(err, SupervisorError::ChildSupervisesSelf { ref caixa } if caixa == "orquestra"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_no_self_supervision_accepts_distinct_children() {
        // Positive control: distinct child names (including a child that
        // is itself a supervisor — nested trees are valid OTP) pass.
        let children = vec![
            child("worker", "^0.1", RestartPolicy::Permanent),
            child("sub-tree", "^0.1", RestartPolicy::Permanent),
        ];
        validate_no_self_supervision(&children, "orquestra").unwrap();
    }

    #[test]
    fn validate_no_self_supervision_empty_children_is_ok() {
        // SimpleOneForOne / no-static-children supervisors have nothing
        // to self-reference — the gate is vacuously satisfied.
        validate_no_self_supervision(&[], "orquestra").unwrap();
    }

    #[test]
    fn validate_simple_one_for_one_skips_uniqueness_check() {
        // SimpleOneForOne supervisors carry no static children — the
        // duplicate-child loop never runs. A zero-window declaration
        // on a SimpleOneForOne supervisor still trips the window check
        // (window applies to dynamic children too).
        let s = SupervisorSpec {
            estrategia: RestartStrategy::SimpleOneForOne,
            restart_window: None,
            children: vec![],
            ..SupervisorSpec::default()
        };
        s.validate().unwrap();
        let s_zero = SupervisorSpec {
            estrategia: RestartStrategy::SimpleOneForOne,
            restart_window: Some(Duration::ZERO),
            children: vec![],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s_zero.validate().unwrap_err(),
            SupervisorError::RestartWindowZero
        );
    }

    #[test]
    fn validate_zero_window_runs_after_max_restarts_check() {
        // Pin the order: max_restarts == 0 fires before
        // restart_window == 0s, so an author with both wrong sees the
        // counter-axis diagnostic first (matches the order in the
        // struct and in the doc comment).
        let s = SupervisorSpec {
            max_restarts: 0,
            restart_window: Some(Duration::ZERO),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(s.validate().unwrap_err(), SupervisorError::ZeroMaxRestarts);
    }

    #[test]
    fn round_trip_all_strategies() {
        for strat in [
            RestartStrategy::OneForOne,
            RestartStrategy::OneForAll,
            RestartStrategy::RestForOne,
            RestartStrategy::SimpleOneForOne,
        ] {
            let s = SupervisorSpec {
                estrategia: strat,
                children: if matches!(strat, RestartStrategy::SimpleOneForOne) {
                    vec![]
                } else {
                    vec![child("w", "^0.1", RestartPolicy::Permanent)]
                },
                ..SupervisorSpec::default()
            };
            let json = serde_json::to_string(&s).unwrap();
            let back: SupervisorSpec = serde_json::from_str(&json).unwrap();
            assert_eq!(s, back);
        }
    }

    #[test]
    fn round_trip_all_restart_policies() {
        for policy in [
            RestartPolicy::Permanent,
            RestartPolicy::Temporary,
            RestartPolicy::Transient,
        ] {
            let c = child("w", "^0.1", policy);
            let json = serde_json::to_string(&c).unwrap();
            let back: ChildSpec = serde_json::from_str(&json).unwrap();
            assert_eq!(c, back);
        }
    }

    #[test]
    fn duration_codec_round_trip_canonical_units() {
        // Note the canonical-form rule: durations serialize to the
        // *largest* unit that divides cleanly, so 60s ↔ "1m" and not
        // "60s" — but the round-trip preserves the underlying Duration.
        let cases = [
            ("30s", Duration::from_secs(30)),
            ("5m", Duration::from_secs(300)),
            ("1h", Duration::from_secs(3600)),
            ("500ms", Duration::from_millis(500)),
        ];
        for (lit, dur) in cases {
            let s = SupervisorSpec {
                children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
                restart_window: Some(dur),
                ..SupervisorSpec::default()
            };
            let json = serde_json::to_string(&s).unwrap();
            assert!(
                json.contains(&format!("\"{lit}\"")),
                "expected \"{lit}\" in {json}"
            );
            let back: SupervisorSpec = serde_json::from_str(&json).unwrap();
            assert_eq!(back.restart_window, Some(dur));
        }
    }

    #[test]
    fn duration_canonicalizes_to_largest_unit() {
        // 60 seconds → "1m" (largest cleanly-divisible unit), but the
        // typed Duration still equals 60s on the way back.
        let s = SupervisorSpec {
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            restart_window: Some(Duration::from_secs(60)),
            ..SupervisorSpec::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"1m\""), "{json}");
        let back: SupervisorSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.restart_window, Some(Duration::from_secs(60)));
    }

    #[test]
    fn three_child_one_for_one_validates() {
        let s = SupervisorSpec {
            estrategia: RestartStrategy::OneForOne,
            max_restarts: 5,
            restart_window: Some(Duration::from_secs(60)),
            children: vec![
                child("worker", "^0.1", RestartPolicy::Permanent),
                child("cache", "^0.1", RestartPolicy::Transient),
                child("scratch", "^0.1", RestartPolicy::Temporary),
            ],
        };
        s.validate().unwrap();
    }

    #[test]
    fn json_uses_pascal_case_for_strategy_and_policy() {
        // Variant names are PascalCase by default in serde, matching
        // tatara-lisp's enum convention (`:estrategia OneForOne`).
        let c = child("w", "^0.1", RestartPolicy::Permanent);
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"Permanent\""));
        assert!(!json.contains("\"permanent\""));

        let s = SupervisorSpec {
            estrategia: RestartStrategy::OneForOne,
            children: vec![c],
            ..SupervisorSpec::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"estrategia\":\"OneForOne\""));
    }

    // ── shared duration codec: integer-magnitude canonical-form gate ──
    //
    // The gate lifts the discipline `crate::limits::parse_duration`
    // (818dd38) carries on the peer `:limits :wall-clock` codec onto
    // the shared codec backing the remaining three typed-duration
    // slots: `:supervisor :restart-window`, `:politicas :timeout`, and
    // `:politicas :circuit-breaker :window`. Every magnitude `render`
    // emits is a non-negative integer with no decimal point and no
    // leading sign, so the codec's accepted set must match for
    // serialize/deserialize to round-trip without canonical-form
    // drift.

    #[test]
    fn parse_accepts_integer_canonical_units() {
        // Pin the happy-path: every canonical author shape `render`
        // ever emits parses to the same `Duration` value, so the
        // codec's accepted set is at least a superset of its emitted
        // set on the canonical-unit axis.
        for (lit, dur) in [
            ("30s", Duration::from_secs(30)),
            ("500ms", Duration::from_millis(500)),
            ("2m", Duration::from_secs(120)),
            ("1h", Duration::from_secs(3600)),
            ("0s", Duration::ZERO),
        ] {
            assert_eq!(
                duration_codec::parse(lit).unwrap(),
                dur,
                "parse({lit:?}) should be {dur:?}"
            );
        }
    }

    #[test]
    fn parse_accepts_bare_integer_as_seconds() {
        // The `"s" | ""` arm: a bare integer with no unit is read as
        // seconds. Pin this so the unit-empty form keeps parsing (it
        // renders to `"<n>s"` on serialize — that's a unit-choice
        // drift the integer-magnitude gate does NOT close, matching
        // the `parse_byte_size` `"1024"` → `"1KiB"` scope decision in
        // the peer `:limits :memory` codec).
        assert_eq!(
            duration_codec::parse("30").unwrap(),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn parse_rejects_fractional_seconds_with_canonical_form_diagnostic() {
        // `"1.5s"` parses as f64 to 1.5 → renders back as `"1500ms"`
        // on first serialize — DRIFT. The integer-magnitude gate names
        // the offending `"1.5"` verbatim and points at the canonical
        // remediation `"1500ms"`.
        let err = duration_codec::parse("1.5s").unwrap_err();
        assert!(err.contains("\"1.5\""), "missing magnitude in {err:?}");
        assert!(
            err.contains("not a non-negative integer"),
            "missing canonical-form reason in {err:?}"
        );
        assert!(
            err.contains("\"1500ms\""),
            "missing canonical-form remediation in {err:?}"
        );
    }

    #[test]
    fn parse_rejects_decimal_shaped_integer_seconds() {
        // `"1.0s"` is the trickiest drift class: numerically `1.0s` is
        // `1s` exactly, so the round-trip looks correct — but the
        // emitted canonical form is `"1s"`, not `"1.0s"`. Gate the
        // decimal-shape-with-integer-value form so author intent is
        // never silently rewritten.
        let err = duration_codec::parse("1.0s").unwrap_err();
        assert!(err.contains("\"1.0\""), "missing magnitude in {err:?}");
        assert!(
            err.contains("not a non-negative integer"),
            "missing canonical-form reason in {err:?}"
        );
    }

    #[test]
    fn parse_rejects_half_unit_minute() {
        // `"0.5m"` is the unit-fraction footgun — author writes a
        // human-readable half-minute, serde silently rewrites to
        // `"30s"` on next emit. The gate names the offending
        // magnitude `"0.5"` and points at the integer-in-smaller-unit
        // form.
        let err = duration_codec::parse("0.5m").unwrap_err();
        assert!(err.contains("\"0.5\""), "missing magnitude in {err:?}");
        assert!(
            err.contains("\"30s\""),
            "missing canonical-form remediation in {err:?}"
        );
    }

    #[test]
    fn parse_rejects_leading_plus_sign() {
        // `u64::from_str` rejects `"+30"` but `f64::from_str` accepts
        // it as `30.0` — the prior parser used f64 so `"+30s"` parsed
        // cleanly to 30s and round-tripped to `"30s"` on next emit
        // (DRIFT). The digit-only gate closes the leading-sign class
        // first; the diagnostic names `"+30"` verbatim.
        let err = duration_codec::parse("+30s").unwrap_err();
        assert!(err.contains("\"+30\""), "missing magnitude in {err:?}");
        assert!(
            err.contains("not a non-negative integer"),
            "missing canonical-form reason in {err:?}"
        );
    }

    #[test]
    fn parse_rejects_leading_minus_sign() {
        // The former `num < 0.0` arm: `"-30s"` parsed as f64 to -30,
        // rejected with `"negative duration in \"-30s\""`. Under the
        // integer-magnitude gate the diagnostic is unified — `-30` is
        // non-digit-only, f64-numeric, and surfaces with the canonical-
        // form reason (no leading `+` / `-` sign) naming the offending
        // `"-30"` verbatim. Same diagnostic shape as every other
        // rejected non-integer magnitude.
        let err = duration_codec::parse("-30s").unwrap_err();
        assert!(err.contains("\"-30\""), "missing magnitude in {err:?}");
        assert!(
            err.contains("not a non-negative integer"),
            "missing canonical-form reason in {err:?}"
        );
    }

    #[test]
    fn parse_garbage_still_falls_through_to_bad_magnitude() {
        // Non-digit-only AND non-numeric (`"--1s"`, `"abc"`) falls
        // through to the narrower "bad duration magnitude" arm — the
        // canonical-form diagnostic is reserved for the parser-shape
        // footgun case, not the "not a number at all" case. Same
        // shape `parse_byte_size`'s `BadByteMagnitude` arm carries on
        // the peer `:limits :memory` codec.
        let err = duration_codec::parse("--1s").unwrap_err();
        assert!(
            err.contains("bad duration magnitude"),
            "expected bad-magnitude wording in {err:?}"
        );
    }

    #[test]
    fn parse_digit_only_magnitude_carries_zero_f64_drift() {
        // The accepted set is now closed under `u64`-exact integer
        // arithmetic: `"500ms"` → `Duration::from_millis(500)` exactly,
        // `"3600s"` → `Duration::from_secs(3600)` exactly, `"1h"` →
        // `Duration::from_secs(3600)` exactly, no f64 mantissa drift
        // possible. Pin the integer-exact arms across the four unit
        // suffixes so a future refactor that reaches back for f64
        // (`from_secs_f64`, `mul_f64`) surfaces here.
        assert_eq!(
            duration_codec::parse("3600s").unwrap(),
            Duration::from_secs(3600)
        );
        assert_eq!(
            duration_codec::parse("60m").unwrap(),
            Duration::from_secs(3600)
        );
        assert_eq!(
            duration_codec::parse("1h").unwrap(),
            Duration::from_secs(3600)
        );
        assert_eq!(
            duration_codec::parse("999ms").unwrap(),
            Duration::from_millis(999)
        );
    }

    #[test]
    fn restart_window_serde_rejects_fractional_seconds() {
        // The shared codec backs `SupervisorSpec::restart_window`
        // (`with = "duration_codec"`) — so the gate applies on serde
        // deserialize for the typed Supervisor slot. A
        // `{"restartWindow":"1.5s"}` payload that previously round-
        // tripped to a different canonical string on next serialize
        // is now refused at deserialize with the integer-magnitude
        // diagnostic.
        let payload = r#"{"estrategia":"OneForOne","maxRestarts":5,
            "restartWindow":"1.5s",
            "children":[{"caixa":"w","versao":"^0.1","restart":"Permanent"}]}"#;
        let err = serde_json::from_str::<SupervisorSpec>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not a non-negative integer"),
            "expected integer-magnitude diagnostic in {msg:?}"
        );
        assert!(msg.contains("\"1.5\""), "missing magnitude in {msg:?}");
    }

    #[test]
    fn restart_window_serde_rejects_leading_plus() {
        // The `u64::from_str` leading-`+` permissiveness gap that
        // motivated the digit-only gate (the `f64`-side accepted
        // `"+30"`, the prior parser silently round-tripped to `"30s"`)
        // is now closed on the shared codec — surfaces as a structured
        // diagnostic at the serde layer for every typed-duration slot.
        let payload = r#"{"estrategia":"OneForOne","maxRestarts":5,
            "restartWindow":"+30s",
            "children":[{"caixa":"w","versao":"^0.1","restart":"Permanent"}]}"#;
        let err = serde_json::from_str::<SupervisorSpec>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("\"+30\""), "missing magnitude in {msg:?}");
        assert!(
            msg.contains("not a non-negative integer"),
            "missing canonical-form reason in {msg:?}"
        );
    }

    #[test]
    fn parse_rejects_leading_zero_magnitude() {
        // `"030s"` is digit-only, so the existing non-digit-only / sign
        // / fractional arm doesn't catch it — `u64::from_str("030")`
        // returns `Ok(30)`, so before this gate `"030s"` parsed to
        // `Duration::from_secs(30)` and round-tripped through `render`
        // to `"30s"` — a *different* canonical string on the next emit,
        // breaking the THEORY.md Part V render-determinism contract
        // exactly the way `"+30s"` did before the leading-`+` arm
        // landed. Peer with the `rate_limit_codec` leading-zero arm
        // (4f46830) on the same canonical-form-drift axis.
        let err = duration_codec::parse("030s").unwrap_err();
        assert!(
            err.contains("non-canonical leading zero"),
            "expected leading-zero diagnostic in {err:?}"
        );
        assert!(err.contains("\"030\""), "missing magnitude in {err:?}");
        assert!(
            err.contains("\"30s\""),
            "missing canonical-form remediation in {err:?}"
        );
        assert!(
            err.contains("THEORY.md"),
            "missing render-determinism citation in {err:?}"
        );
    }

    #[test]
    fn parse_rejects_multi_digit_zero_magnitude() {
        // `"00s"` and `"00ms"` are the all-zero leading-zero footgun —
        // digit-only, parse losslessly to `Duration::ZERO`, but render
        // back to `"0s"` (the single-byte canonical form) on the next
        // emit. The leading-zero arm refuses the drift class at the
        // codec layer; the semantic-zero gate downstream
        // (`SupervisorError::ZeroRestartWindow`, etc.) would refuse
        // the single-byte canonical form `"0s"` separately on the
        // typed-validate layer.
        let err = duration_codec::parse("00s").unwrap_err();
        assert!(
            err.contains("non-canonical leading zero"),
            "expected leading-zero diagnostic in {err:?}"
        );
        assert!(err.contains("\"00\""), "missing magnitude in {err:?}");
    }

    #[test]
    fn parse_rejects_leading_zero_per_hour_window() {
        // `"01h"` is the per-hour-window footgun — multi-byte magnitude
        // starting with `0`, parses losslessly to `Duration::from_secs(3600)`,
        // renders to `"1h"` (DRIFT). The arm is unit-agnostic: every
        // canonical unit suffix the codec accepts (`ms` / `s` / `m` /
        // `h` / bare-integer-as-seconds) inherits the same gate.
        let err = duration_codec::parse("01h").unwrap_err();
        assert!(
            err.contains("non-canonical leading zero"),
            "expected leading-zero diagnostic in {err:?}"
        );
        assert!(err.contains("\"01\""), "missing magnitude in {err:?}");
    }

    #[test]
    fn parse_rejects_leading_zero_bare_integer_as_seconds() {
        // The `parse_accepts_bare_integer_as_seconds` happy-path
        // (`"30"` → 30s) inherits the leading-zero arm: `"030"` is
        // multi-byte starts-with-`0`, parses losslessly to
        // `Duration::from_secs(30)`, renders to `"30s"` (DRIFT). The
        // bare-integer surface accepts permissive unit-empty
        // shorthand but still must reject leading-zero padding.
        let err = duration_codec::parse("030").unwrap_err();
        assert!(
            err.contains("non-canonical leading zero"),
            "expected leading-zero diagnostic in {err:?}"
        );
        assert!(err.contains("\"030\""), "missing magnitude in {err:?}");
    }

    #[test]
    fn parse_accepts_single_zero_magnitude_at_codec_layer() {
        // The codec-layer / typed-validate-layer boundary: `"0s"` /
        // `"0ms"` / `"0"` are the single-byte canonical-zero forms —
        // each round-trips losslessly through `render`
        // (`render(Duration::ZERO)` → `"0s"`), so the codec layer
        // accepts them. The downstream semantic-zero gates
        // (`SupervisorError::ZeroRestartWindow`,
        // `AplicacaoError::PolicyTimeoutZero`,
        // `AplicacaoError::PolicyCircuitBreakerWindowZero`) refuse
        // zero-magnitude authoring at the typed-validate layer above,
        // peer with the `rate_limit_codec` codec-layer / typed-
        // validate-layer partition for `"0/s"`.
        assert_eq!(duration_codec::parse("0s").unwrap(), Duration::ZERO);
        assert_eq!(duration_codec::parse("0ms").unwrap(), Duration::ZERO);
        assert_eq!(duration_codec::parse("0").unwrap(), Duration::ZERO);
    }

    #[test]
    fn parse_accepts_canonical_magnitude_with_leading_one() {
        // The complementary boundary: a future tightening cannot
        // drift into rejecting valid canonical magnitudes that
        // happen to start with `1` (or any digit `[1-9]`). Pin
        // every canonical-unit suffix so the leading-zero arm
        // remains strictly narrower than the digit-only arm.
        assert_eq!(
            duration_codec::parse("100ms").unwrap(),
            Duration::from_millis(100)
        );
        assert_eq!(
            duration_codec::parse("100s").unwrap(),
            Duration::from_secs(100)
        );
        assert_eq!(
            duration_codec::parse("10m").unwrap(),
            Duration::from_secs(600)
        );
        assert_eq!(
            duration_codec::parse("10h").unwrap(),
            Duration::from_secs(36_000)
        );
    }

    #[test]
    fn restart_window_serde_rejects_leading_zero() {
        // The shared codec backs `SupervisorSpec::restart_window`
        // (`with = "duration_codec"`) — so the leading-zero arm
        // applies on serde deserialize for the typed Supervisor slot.
        // A `{"restartWindow":"030s"}` payload that previously round-
        // tripped to a different canonical string on next serialize
        // is now refused at deserialize with the leading-zero
        // diagnostic. Peer with `restart_window_serde_rejects_leading_plus`
        // / `restart_window_serde_rejects_fractional_seconds` on the
        // same canonical-form-drift axis.
        let payload = r#"{"estrategia":"OneForOne","maxRestarts":5,
            "restartWindow":"030s",
            "children":[{"caixa":"w","versao":"^0.1","restart":"Permanent"}]}"#;
        let err = serde_json::from_str::<SupervisorSpec>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("non-canonical leading zero"),
            "expected leading-zero diagnostic in {msg:?}"
        );
        assert!(msg.contains("\"030\""), "missing magnitude in {msg:?}");
    }

    #[test]
    fn parse_rejects_leading_whitespace() {
        // `" 30s"` — the canonical paste-from-aligned-doc /
        // paste-from-YAML-quoted-plain-scalar footgun. Before this
        // gate the top-level `s.trim()` at parse entry silently ate
        // the leading space and parsed the value to
        // `Duration::from_secs(30)`, which then round-tripped through
        // `render` to `"30s"` (a *different* canonical string on the
        // next emit) — the exact canonical-form-drift class the
        // leading-`+` / leading-zero arms already close, extended
        // to the whitespace-byte class. Peer with the sibling
        // `rate_limit_codec` whitespace-rejection arm (1ad7755) on
        // the M3 `:politicas` axis.
        let err = duration_codec::parse(" 30s").unwrap_err();
        assert!(
            err.contains("contains whitespace byte"),
            "expected whitespace diagnostic in {err:?}"
        );
        assert!(err.contains("0x20"), "missing offending byte in {err:?}");
        assert!(
            err.contains("THEORY.md"),
            "missing render-determinism contract citation in {err:?}"
        );
    }

    #[test]
    fn parse_rejects_trailing_whitespace() {
        // `"30s "` — the canonical shell-history / trailing-space
        // paste footgun. Before this gate the top-level `s.trim()`
        // silently ate the trailing space and parsed to
        // `Duration::from_secs(30)`, round-tripping to `"30s"` on the
        // next emit — same canonical-form drift as the leading-space
        // sibling, closed on the same whitespace-byte arm.
        let err = duration_codec::parse("30s ").unwrap_err();
        assert!(
            err.contains("contains whitespace byte"),
            "expected whitespace diagnostic in {err:?}"
        );
        assert!(err.contains("0x20"), "missing offending byte in {err:?}");
    }

    #[test]
    fn parse_rejects_internal_whitespace_between_magnitude_and_unit() {
        // `"30 s"` — the canonical typographically-spaced author
        // shape (the same idiom every prose reference to a duration
        // renders as, mistakenly retained when the value is pasted
        // into a codec-shaped slot). Before this gate the per-part
        // `num_part.trim()` / `unit.trim()` calls silently ate the
        // whitespace between the magnitude and the unit and parsed
        // the value to `Duration::from_secs(30)`, round-tripping to
        // `"30s"` — the codec's *internal* whitespace-tolerance
        // vector, orthogonal to the leading / trailing surface but
        // the same canonical-form-drift class. Pins the arm as
        // strictly stronger than the pre-existing top-level
        // `s.trim()` behavior: it fires on whitespace anywhere in
        // the value, not just at the string boundary.
        let err = duration_codec::parse("30 s").unwrap_err();
        assert!(
            err.contains("contains whitespace byte"),
            "expected whitespace diagnostic in {err:?}"
        );
        assert!(err.contains("0x20"), "missing offending byte in {err:?}");
    }

    #[test]
    fn parse_rejects_tab_byte() {
        // `"\t30s"` — the canonical paste-from-indented-doc /
        // paste-from-YAML-block-scalar footgun where a tab byte leads
        // the magnitude. Pins that the gate covers tab (`0x09`) as
        // well as space (`0x20`) — both are `u8::is_ascii_whitespace`
        // members and both would be silently swallowed by `s.trim()`
        // pre-gate. The `is_ascii_whitespace` coverage extends beyond
        // space alone to the full ASCII-whitespace set (space `0x20`,
        // tab `0x09`, LF `0x0A`, FF `0x0C`, CR `0x0D`); this test pins
        // the tab arm as a representative of the non-space members.
        let err = duration_codec::parse("\t30s").unwrap_err();
        assert!(
            err.contains("contains whitespace byte"),
            "expected whitespace diagnostic in {err:?}"
        );
        assert!(
            err.contains("0x09"),
            "missing offending tab byte in {err:?}"
        );
    }

    #[test]
    fn restart_window_serde_rejects_whitespace() {
        // The shared codec backs `SupervisorSpec::restart_window`
        // (`with = "duration_codec"`) — so the whitespace arm
        // applies on serde deserialize for the typed Supervisor slot.
        // A `{"restartWindow":" 30s"}` payload that previously round-
        // tripped to a different canonical string on next serialize
        // is now refused at deserialize with the whitespace-byte
        // diagnostic. Peer with `restart_window_serde_rejects_leading_zero`
        // / `restart_window_serde_rejects_leading_plus` /
        // `restart_window_serde_rejects_fractional_seconds` on the
        // same canonical-form-drift axis.
        let payload = r#"{"estrategia":"OneForOne","maxRestarts":5,
            "restartWindow":" 30s",
            "children":[{"caixa":"w","versao":"^0.1","restart":"Permanent"}]}"#;
        let err = serde_json::from_str::<SupervisorSpec>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("contains whitespace byte"),
            "expected whitespace diagnostic in {msg:?}"
        );
        assert!(msg.contains("0x20"), "missing offending byte in {msg:?}");
    }
}
