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
    #[error("duration: missing magnitude in {0:?}")]
    EmptyDuration(String),
    #[error("duration: unknown unit {unit:?} (expected one of ms, s, m, h)")]
    UnknownDurationUnit { unit: String },
    #[error("duration: failed to parse magnitude {0:?}")]
    BadDurationMagnitude(String),
    #[error("millicores: bad value {0:?} (expected `<int>m` or `<int>`)")]
    BadMillicores(String),
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
    let num: f64 = num_part
        .trim()
        .parse()
        .map_err(|_| LimitsError::BadByteMagnitude(num_part.into()))?;
    if num < 0.0 {
        return Err(LimitsError::BadByteMagnitude(num_part.into()));
    }
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
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok((num * multiplier as f64) as u64)
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
    let num: f64 = num_part
        .trim()
        .parse()
        .map_err(|_| LimitsError::BadDurationMagnitude(num_part.into()))?;
    if num < 0.0 {
        return Err(LimitsError::BadDurationMagnitude(num_part.into()));
    }
    let dur = match unit.trim() {
        "ms" => Duration::from_secs_f64(num / 1000.0),
        "s" | "" => Duration::from_secs_f64(num),
        "m" => Duration::from_secs_f64(num * 60.0),
        "h" => Duration::from_secs_f64(num * 3600.0),
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
    let s = s.trim();
    if let Some(stripped) = s.strip_suffix('m') {
        stripped
            .trim()
            .parse()
            .map_err(|_| LimitsError::BadMillicores(s.into()))
    } else {
        // "2" → 2000 millicores
        s.parse::<u32>()
            .map(|n| n.saturating_mul(1000))
            .map_err(|_| LimitsError::BadMillicores(s.into()))
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
}
