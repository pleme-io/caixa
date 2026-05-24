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
        // Upper-bound gate on `:memory`: every validated value must
        // also fit within the wasm32-wasip2 linear-memory ceiling
        // ([`LIMITS_MEMORY_WASM32_MAX_BYTES`] = 4 GiB). The
        // zero-floor arm immediately above and this cap arm together
        // bracket the typed `:memory` axis structurally — every
        // validated value lies in `1..=LIMITS_MEMORY_WASM32_MAX_BYTES`.
        // Until this gate landed the byte-size codec accepted any
        // `Option<u64>` past zero (the parser's only upper bound was
        // `u64::MAX`), so `(:memory "8GiB")` round-tripped cleanly
        // through serde and the per-axis CSE invariant (no value the
        // wasm-engine can't honor) was a runtime, not build-time,
        // contract. Same trajectory as the typed-shape gates
        // [`AplicacaoSpec::validate_politicas`] (the
        // canonical-rate-limit-window upper bound) and the
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
        if self.cpu == Some(0) {
            return Err(LimitsError::CpuZero);
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
        ":limits :cpu must be > 0m — a zero cgroup share starves the process; omit the field for unbounded"
    )]
    CpuZero,
}

// ── byte-size codec ────────────────────────────────────────────────────

fn parse_byte_size(s: &str) -> Result<u64, LimitsError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(LimitsError::EmptyByteSize(s.into()));
    }
    let split_at = s
        .find(|c: char| c.is_ascii_alphabetic())
        .unwrap_or(s.len());
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
            return Err(LimitsError::UnknownByteUnit {
                unit: other.into(),
            });
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
        Some(s) => parse_byte_size(&s).map(Some).map_err(serde::de::Error::custom),
    }
}

// ── duration codec ─────────────────────────────────────────────────────

fn parse_duration(s: &str) -> Result<Duration, LimitsError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(LimitsError::EmptyDuration(s.into()));
    }
    let split_at = s
        .find(|c: char| c.is_ascii_alphabetic())
        .unwrap_or(s.len());
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
            return Err(LimitsError::UnknownDurationUnit {
                unit: other.into(),
            });
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
        Some(s) => parse_duration(&s).map(Some).map_err(serde::de::Error::custom),
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
        Some(s) => parse_millicores(&s).map(Some).map_err(serde::de::Error::custom),
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
        assert_eq!(parse_byte_size("4GiB").unwrap(), LIMITS_MEMORY_WASM32_MAX_BYTES);
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
        assert!(matches!(err, LimitsError::BadByteMagnitude(_)), "got {err:?}");
        let err = parse_byte_size("--1").unwrap_err();
        assert!(matches!(err, LimitsError::BadByteMagnitude(_)), "got {err:?}");
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
            let reparsed = parse_duration(&rendered).unwrap_or_else(|e| {
                panic!("render({d:?}) = {rendered:?} must reparse, got {e:?}")
            });
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
}
