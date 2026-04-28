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
}
