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
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash,
         gen_platform::TypedDispatcher,
         gen_platform::Discriminant,
         gen_platform::IsVariant,
         gen_platform::FromStrKind)]
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
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash,
         gen_platform::TypedDispatcher,
         gen_platform::Discriminant,
         gen_platform::IsVariant,
         gen_platform::FromStrKind)]
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
gen_platform::register_dispatcher!("caixa.restart-policy",   RestartPolicy);

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
        if matches!(self.restart_window, Some(d) if d.is_zero()) {
            return Err(SupervisorError::RestartWindowZero);
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
        ":restart-window must be > 0 when set — Erlang/OTP's MaxIntensity/Period \
         requires Period > 0; a zero window either trips on the first failure or \
         never trips depending on operator interpretation. Omit :restart-window to \
         express `never reset`; carry a positive duration to express the window."
    )]
    RestartWindowZero,
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
        // The digit-only gate guarantees every byte is `[0-9]`, so the
        // only way `u64::from_str` can fail here is overflow (the
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
}
