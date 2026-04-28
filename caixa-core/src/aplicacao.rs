//! Typed Aplicacao — the fourth caixa kind that turns a graph of
//! Servicos into a single declarative application (mesh).
//!
//! See `theory/MESH-COMPOSITION.md` for the design frame: an
//! Aplicacao composes [`crate::CaixaKind::Servico`] caixas via WIT-typed
//! `:contratos` (inter-Servico edges), declares mesh-level
//! `:politicas` (timeouts, retries, breakers, mTLS), pins
//! `:placement` strategy (single-node / replicated / sharded), and
//! exposes `:entrada` (gateway).
//!
//! ```lisp
//! (defcaixa
//!   :nome      "checkout"
//!   :versao    "0.1.0"
//!   :kind      Aplicacao
//!   :membros   ((:caixa "catalog"     :versao "^0.1")
//!               (:caixa "cart"        :versao "^0.1")
//!               (:caixa "payment"     :versao "^0.2"))
//!   :contratos ((:de "cart" :para "catalog"
//!                :wit "wasi:http/proxy" :endpoint "/products/:id")
//!               (:de "cart" :para "payment"
//!                :wit "wasi:http/proxy" :endpoint "/charge"))
//!   :politicas ((:timeout "30s")
//!               (:retries 3)
//!               (:circuit-breaker (:max-failures 5 :window "60s"))
//!               (:mtls-required t))
//!   :placement (:estrategia replicated
//!               :clusters   ("rio" "mar" "plo"))
//!   :entrada   (:host  "checkout.quero.cloud"
//!               :para  "cart"
//!               :paths ("/api/cart" "/api/products")))
//! ```
//!
//! All the typed slots compose with the M2 primitives the Servicos
//! they reference already declare (`:limits`, `:behavior`,
//! `:upgrade-from`). The Aplicacao adds the *graph-level*
//! standardization on top.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::supervisor; // we reuse the duration-string codec at module scope

// ── inter-Servico contracts ──────────────────────────────────────────

/// One typed edge in the Aplicacao graph. The build refuses any
/// contract whose `:de` or `:para` doesn't appear in `:membros`, and
/// (M3+) cross-checks the `:wit` shape against both Servicos'
/// declared imports/exports.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WitContract {
    /// Caller Servico — must reference an entry in the Aplicacao's
    /// `:membros`. The Servico's caixa.lisp must declare a matching
    /// `:capabilities` import for the `:wit` world.
    pub de: String,

    /// Callee Servico — must reference an entry in `:membros`. The
    /// Servico must declare a matching `:capabilities` export.
    pub para: String,

    /// WIT world reference — e.g. `"wasi:http/proxy"`,
    /// `"wasi:keyvalue/store"`, `"nats:pub-sub"`. Strings for V0;
    /// M4 promotes these to a typed enum once the WIT registry
    /// stabilizes in tatara-lisp.
    pub wit: String,

    /// HTTP endpoint path, present when `:wit` is HTTP-shaped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,

    /// NATS / event-stream subject, present when `:wit` is pub-sub-shaped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,

    /// Key/value or queue slot, present when `:wit` is store-shaped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
}

impl WitContract {
    /// True when this contract targets an HTTP-shaped WIT world.
    #[must_use]
    pub fn is_http(&self) -> bool {
        self.wit.starts_with("wasi:http/") || self.wit.starts_with("http:")
    }

    /// True when this contract targets a pub-sub-shaped WIT world.
    #[must_use]
    pub fn is_pubsub(&self) -> bool {
        self.wit.starts_with("nats:") || self.wit.starts_with("kafka:")
    }

    /// True when this contract targets a key/value-shaped WIT world.
    #[must_use]
    pub fn is_store(&self) -> bool {
        self.wit.starts_with("wasi:keyvalue/") || self.wit.starts_with("kv:")
    }

    /// Typed view of the contract's payload target. Enforces that the
    /// `:wit` shape and the carried `:endpoint`/`:subject`/`:slot`
    /// fields agree:
    ///
    ///   - HTTP world (`wasi:http/*`, `http:*`) ⇒ exactly `:endpoint`
    ///   - `PubSub` world (`nats:*`, `kafka:*`) ⇒ exactly `:subject`
    ///   - Store world (`wasi:keyvalue/*`, `kv:*`) ⇒ exactly `:slot`
    ///   - Anything else ⇒ none of the three; the contract is a pure
    ///     typed capability edge with no payload selector.
    ///
    /// Translates the Apollo Federation discipline ("conflicts are
    /// errors at compile time, not warnings at runtime";
    /// MESH-COMPOSITION §II.3) onto pleme-io's typed Aplicacao surface:
    /// a contract whose WIT shape disagrees with its target field is
    /// a build error, not a silent renderer drop.
    pub fn target(&self) -> Result<WitTarget<'_>, AplicacaoError> {
        let endpoint = self.endpoint.as_deref();
        let subject = self.subject.as_deref();
        let slot = self.slot.as_deref();
        let edge = || (self.de.clone(), self.para.clone(), self.wit.clone());

        if self.is_http() {
            if subject.is_some() || slot.is_some() {
                let (de, para, wit) = edge();
                return Err(AplicacaoError::ContratoWrongTarget {
                    de,
                    para,
                    wit,
                    expected: "endpoint",
                });
            }
            return endpoint
                .map(|e| WitTarget::Http { endpoint: e })
                .ok_or_else(|| {
                    let (de, para, wit) = edge();
                    AplicacaoError::ContratoMissingTarget {
                        de,
                        para,
                        wit,
                        expected: "endpoint",
                    }
                });
        }
        if self.is_pubsub() {
            if endpoint.is_some() || slot.is_some() {
                let (de, para, wit) = edge();
                return Err(AplicacaoError::ContratoWrongTarget {
                    de,
                    para,
                    wit,
                    expected: "subject",
                });
            }
            return subject
                .map(|s| WitTarget::PubSub { subject: s })
                .ok_or_else(|| {
                    let (de, para, wit) = edge();
                    AplicacaoError::ContratoMissingTarget {
                        de,
                        para,
                        wit,
                        expected: "subject",
                    }
                });
        }
        if self.is_store() {
            if endpoint.is_some() || subject.is_some() {
                let (de, para, wit) = edge();
                return Err(AplicacaoError::ContratoWrongTarget {
                    de,
                    para,
                    wit,
                    expected: "slot",
                });
            }
            return slot.map(|s| WitTarget::Store { slot: s }).ok_or_else(|| {
                let (de, para, wit) = edge();
                AplicacaoError::ContratoMissingTarget {
                    de,
                    para,
                    wit,
                    expected: "slot",
                }
            });
        }

        // Unrecognized WIT world — must not carry any payload target.
        if endpoint.is_some() || subject.is_some() || slot.is_some() {
            let (de, para, wit) = edge();
            return Err(AplicacaoError::ContratoWrongTarget {
                de,
                para,
                wit,
                expected: "none",
            });
        }
        Ok(WitTarget::Capability)
    }
}

/// Typed view of a [`WitContract`]'s payload target. Each variant
/// carries the field its WIT shape requires; constructing a `Http`
/// view without an endpoint is impossible by the type system.
///
/// Renderers (caixa-mesh L7 rules, feira app graph) match on this
/// instead of probing `Option<String>` fields one by one — the
/// "which payload field is set?" question is answered once, at
/// validation time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WitTarget<'a> {
    /// HTTP-shaped WIT world. Carries the configured request path.
    Http { endpoint: &'a str },
    /// Pub-sub-shaped WIT world. Carries the event-stream subject.
    PubSub { subject: &'a str },
    /// Key-value-shaped WIT world. Carries the slot template.
    Store { slot: &'a str },
    /// A typed capability edge with no payload selector — the WIT
    /// world stands on its own (rare; reserved for plain capability
    /// imports or M4-and-later WIT worlds we haven't shaped yet).
    Capability,
}

// ── one Aplicacao member ─────────────────────────────────────────────

/// A Servico participating in the Aplicacao. Same shape as
/// `crate::supervisor::ChildSpec` but without a restart policy —
/// supervision is per-Servico (each member has its own
/// `:supervisor`), the Aplicacao orchestrates *placement*.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Membro {
    /// Member caixa's `:nome`. Resolves through the same dep
    /// resolution path as `crate::dep::Dep`.
    pub caixa: String,

    /// Semver constraint.
    pub versao: String,
}

// ── mesh-level policies ──────────────────────────────────────────────

/// Mesh policies that apply to every `:contratos` edge unless
/// overridden per-edge in M4. V0 is a single global policy block.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MeshPolicy {
    /// Per-call timeout. Authored as a duration string (`"30s"`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "supervisor::duration_codec"
    )]
    pub timeout: Option<Duration>,

    /// Number of retries on transient failure. None = no retries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retries: Option<u32>,

    /// Circuit breaker config. Trips after N failures within W
    /// duration; closes after a cooldown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub circuit_breaker: Option<CircuitBreaker>,

    /// Whether mTLS is required for every contrato. Default: true
    /// (sandboxing-by-default; explicit opt-out only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtls_required: Option<bool>,

    /// Token-bucket rate limit. Authored as `"100/s"` or
    /// `"5000/m"`; stored as `(rate, window)`.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "rate_limit_codec"
    )]
    pub rate_limit: Option<RateLimit>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CircuitBreaker {
    pub max_failures: u32,
    #[serde(with = "supervisor::duration_codec_required")]
    pub window: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimit {
    /// Requests per window.
    pub rate: u32,
    /// Window duration.
    pub window: Duration,
}

mod rate_limit_codec {
    use super::{Duration, RateLimit};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<RateLimit>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(rl) => s.serialize_str(&render(*rl)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<RateLimit>, D::Error> {
        let opt: Option<String> = Option::deserialize(d)?;
        match opt {
            None => Ok(None),
            Some(s) => parse(&s).map(Some).map_err(serde::de::Error::custom),
        }
    }

    fn parse(s: &str) -> Result<RateLimit, String> {
        let s = s.trim();
        let (rate_str, unit) = s
            .split_once('/')
            .ok_or_else(|| format!("rate-limit must be `<n>/<unit>`, got {s:?}"))?;
        let rate: u32 = rate_str
            .trim()
            .parse()
            .map_err(|_| format!("rate-limit rate {rate_str:?} not a u32"))?;
        let window = match unit.trim() {
            "s" => Duration::from_secs(1),
            "m" => Duration::from_secs(60),
            "h" => Duration::from_secs(3600),
            other => return Err(format!("unknown rate-limit window unit {other:?}")),
        };
        Ok(RateLimit { rate, window })
    }

    fn render(rl: RateLimit) -> String {
        let unit = if rl.window.as_secs() == 1 {
            "s"
        } else if rl.window.as_secs() == 60 {
            "m"
        } else if rl.window.as_secs() == 3600 {
            "h"
        } else {
            // Round-trip unit-agnostic: fall through to seconds.
            return format!("{}/{}s", rl.rate, rl.window.as_secs());
        };
        format!("{}/{unit}", rl.rate)
    }
}

// ── placement strategy ───────────────────────────────────────────────

/// How the Aplicacao distributes across clusters. Three options:
///
/// - `SingleNode` — one cluster runs the app at a time; takeover on
///   death (Erlang/OTP distributed-app semantics).
/// - `Replicated` — every named cluster runs an instance (active-active).
/// - `Sharded` — entities distribute by hash key across clusters
///   (Akka cluster sharding).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlacementStrategy {
    SingleNode,
    Replicated,
    Sharded,
}

impl Default for PlacementStrategy {
    fn default() -> Self {
        Self::Replicated
    }
}

/// Where the Aplicacao runs.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    /// Distribution strategy.
    #[serde(default)]
    pub estrategia: PlacementStrategy,

    /// Named clusters that host this Aplicacao. Required for
    /// `Replicated` and `SingleNode`; for `Sharded` declares the
    /// shard pool.
    #[serde(default)]
    pub clusters: Vec<String>,

    /// Optional hint to the placement engine: `"data-locality"`,
    /// `"low-latency"`, etc. Drives M3 Adaptive compression weights.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affinity: Option<String>,

    /// Sharding key — required when `:estrategia Sharded`. M3 deliverable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shard_key: Option<String>,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            estrategia: PlacementStrategy::default(),
            clusters: Vec::new(),
            affinity: None,
            shard_key: None,
        }
    }
}

// ── external entry point ─────────────────────────────────────────────

/// External entry point — what an outside caller sees. Renders to a
/// Gateway / Ingress + a route to the named member Servico.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Entrada {
    /// Public hostname (e.g. `"checkout.quero.cloud"`).
    pub host: String,

    /// Member Servico the gateway routes to. Must be in `:membros`.
    pub para: String,

    /// Optional path filter — if set, only matching paths route to
    /// this Aplicacao (the rest fall through to other route rules).
    #[serde(default)]
    pub paths: Vec<String>,

    /// Default port on the destination Servico (the trigger.service.port).
    #[serde(default = "default_port")]
    pub port: u16,
}

const fn default_port() -> u16 {
    8080
}

// ── the typed view ───────────────────────────────────────────────────

/// Typed composition view of the flat Aplicacao slots on
/// [`crate::Caixa`]. Built via [`crate::Caixa::aplicacao_view`] for
/// validation + downstream renderer consumption.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AplicacaoSpec {
    pub membros: Vec<Membro>,
    pub contratos: Vec<WitContract>,
    pub politicas: MeshPolicy,
    pub placement: Placement,
    pub entrada: Option<Entrada>,
}

impl AplicacaoSpec {
    /// Validate the typed shape:
    ///   - every `:contratos` :de + :para must be in `:membros`
    ///   - `:entrada :para` must be in `:membros`
    ///   - `:placement Sharded` must declare `:shard-key`
    ///   - `:placement Replicated` / `SingleNode` must declare ≥1 cluster
    pub fn validate(&self) -> Result<(), AplicacaoError> {
        if self.membros.is_empty() {
            return Err(AplicacaoError::NoMembros);
        }
        let names: std::collections::HashSet<&str> =
            self.membros.iter().map(|m| m.caixa.as_str()).collect();

        for c in &self.contratos {
            if !names.contains(c.de.as_str()) {
                return Err(AplicacaoError::ContratoMemberMissing {
                    caixa: c.de.clone(),
                });
            }
            if !names.contains(c.para.as_str()) {
                return Err(AplicacaoError::ContratoMemberMissing {
                    caixa: c.para.clone(),
                });
            }
            if c.wit.is_empty() {
                return Err(AplicacaoError::EmptyWit {
                    de: c.de.clone(),
                    para: c.para.clone(),
                });
            }
            // Shape ↔ target consistency — surfaces "HTTP wit without
            // :endpoint", "NATS wit with :endpoint set", etc. as named
            // build errors instead of silent renderer drops.
            c.target()?;
        }

        if let Some(e) = &self.entrada {
            if !names.contains(e.para.as_str()) {
                return Err(AplicacaoError::EntradaMemberMissing {
                    para: e.para.clone(),
                });
            }
            if e.host.is_empty() {
                return Err(AplicacaoError::EmptyEntradaHost);
            }
        }

        match self.placement.estrategia {
            PlacementStrategy::Sharded => {
                if self.placement.shard_key.is_none() {
                    return Err(AplicacaoError::ShardedWithoutKey);
                }
            }
            PlacementStrategy::Replicated | PlacementStrategy::SingleNode => {
                if self.placement.clusters.is_empty() {
                    return Err(AplicacaoError::PlacementWithoutClusters {
                        estrategia: self.placement.estrategia,
                    });
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AplicacaoError {
    #[error("Aplicacao must declare at least one :membros entry")]
    NoMembros,
    #[error("contrato references caixa {caixa:?} not declared in :membros")]
    ContratoMemberMissing { caixa: String },
    #[error("contrato {de:?} → {para:?} has empty :wit")]
    EmptyWit { de: String, para: String },
    #[error(":entrada routes to caixa {para:?} not declared in :membros")]
    EntradaMemberMissing { para: String },
    #[error(":entrada must declare a non-empty :host")]
    EmptyEntradaHost,
    #[error(":placement {estrategia:?} requires at least one :clusters entry")]
    PlacementWithoutClusters { estrategia: PlacementStrategy },
    #[error(":placement Sharded requires :shard-key")]
    ShardedWithoutKey,
    #[error("contrato {de:?} → {para:?} (:wit {wit:?}) is missing required `:{expected}` field")]
    ContratoMissingTarget {
        de: String,
        para: String,
        wit: String,
        expected: &'static str,
    },
    #[error(
        "contrato {de:?} → {para:?} (:wit {wit:?}) carries the wrong target field — \
         expected `:{expected}` only"
    )]
    ContratoWrongTarget {
        de: String,
        para: String,
        wit: String,
        expected: &'static str,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn membro(name: &str, ver: &str) -> Membro {
        Membro {
            caixa: name.into(),
            versao: ver.into(),
        }
    }

    fn contract_http(de: &str, para: &str, ep: &str) -> WitContract {
        WitContract {
            de: de.into(),
            para: para.into(),
            wit: "wasi:http/proxy".into(),
            endpoint: Some(ep.into()),
            subject: None,
            slot: None,
        }
    }

    fn three_member_spec() -> AplicacaoSpec {
        AplicacaoSpec {
            membros: vec![
                membro("catalog", "^0.1"),
                membro("cart", "^0.1"),
                membro("payment", "^0.2"),
            ],
            contratos: vec![
                contract_http("cart", "catalog", "/products/:id"),
                contract_http("cart", "payment", "/charge"),
            ],
            politicas: MeshPolicy {
                timeout: Some(Duration::from_secs(30)),
                retries: Some(3),
                mtls_required: Some(true),
                ..Default::default()
            },
            placement: Placement {
                estrategia: PlacementStrategy::Replicated,
                clusters: vec!["rio".into(), "mar".into()],
                affinity: Some("data-locality".into()),
                shard_key: None,
            },
            entrada: Some(Entrada {
                host: "checkout.quero.cloud".into(),
                para: "cart".into(),
                paths: vec!["/api/cart".into(), "/api/products".into()],
                port: 8080,
            }),
        }
    }

    #[test]
    fn happy_path_validates() {
        three_member_spec().validate().unwrap();
    }

    #[test]
    fn rejects_empty_membros() {
        let mut s = three_member_spec();
        s.membros = vec![];
        assert_eq!(s.validate().unwrap_err(), AplicacaoError::NoMembros);
    }

    #[test]
    fn rejects_contrato_with_unknown_de() {
        let mut s = three_member_spec();
        s.contratos.push(contract_http("phantom", "catalog", "/x"));
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::ContratoMemberMissing { caixa } if caixa == "phantom")
        );
    }

    #[test]
    fn rejects_contrato_with_unknown_para() {
        let mut s = three_member_spec();
        s.contratos.push(contract_http("cart", "phantom", "/x"));
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::ContratoMemberMissing { caixa } if caixa == "phantom")
        );
    }

    #[test]
    fn rejects_empty_wit() {
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "cart".into(),
            para: "catalog".into(),
            wit: "".into(),
            endpoint: None,
            subject: None,
            slot: None,
        });
        let err = s.validate().unwrap_err();
        assert!(matches!(err, AplicacaoError::EmptyWit { .. }));
    }

    #[test]
    fn rejects_entrada_to_unknown_member() {
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().para = "phantom".into();
        assert!(matches!(
            s.validate().unwrap_err(),
            AplicacaoError::EntradaMemberMissing { .. }
        ));
    }

    #[test]
    fn rejects_replicated_without_clusters() {
        let mut s = three_member_spec();
        s.placement.clusters = vec![];
        assert!(matches!(
            s.validate().unwrap_err(),
            AplicacaoError::PlacementWithoutClusters { .. }
        ));
    }

    #[test]
    fn rejects_sharded_without_key() {
        let mut s = three_member_spec();
        s.placement.estrategia = PlacementStrategy::Sharded;
        s.placement.shard_key = None;
        s.placement.clusters = vec!["rio".into()];
        assert_eq!(s.validate().unwrap_err(), AplicacaoError::ShardedWithoutKey);
    }

    #[test]
    fn sharded_with_key_validates() {
        let mut s = three_member_spec();
        s.placement.estrategia = PlacementStrategy::Sharded;
        s.placement.shard_key = Some("$tenantId".into());
        s.validate().unwrap();
    }

    #[test]
    fn round_trip_via_json_preserves_shape() {
        let s = three_member_spec();
        let json = serde_json::to_string(&s.membros).unwrap();
        let back: Vec<Membro> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s.membros);

        let json = serde_json::to_string(&s.contratos).unwrap();
        let back: Vec<WitContract> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s.contratos);

        let json = serde_json::to_string(&s.placement).unwrap();
        let back: Placement = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s.placement);

        let json = serde_json::to_string(&s.entrada).unwrap();
        let back: Option<Entrada> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s.entrada);
    }

    #[test]
    fn rate_limit_round_trip_seconds() {
        let policy = MeshPolicy {
            rate_limit: Some(RateLimit {
                rate: 100,
                window: Duration::from_secs(1),
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&policy).unwrap();
        assert!(json.contains("\"100/s\""));
        let back: MeshPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(back.rate_limit.unwrap().rate, 100);
        assert_eq!(back.rate_limit.unwrap().window, Duration::from_secs(1));
    }

    #[test]
    fn rate_limit_round_trip_minutes() {
        let policy = MeshPolicy {
            rate_limit: Some(RateLimit {
                rate: 5000,
                window: Duration::from_secs(60),
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&policy).unwrap();
        assert!(json.contains("\"5000/m\""));
    }

    #[test]
    fn circuit_breaker_round_trip() {
        let policy = MeshPolicy {
            circuit_breaker: Some(CircuitBreaker {
                max_failures: 5,
                window: Duration::from_secs(60),
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&policy).unwrap();
        let back: MeshPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(back.circuit_breaker.unwrap().max_failures, 5);
        assert_eq!(
            back.circuit_breaker.unwrap().window,
            Duration::from_secs(60)
        );
    }

    #[test]
    fn rejects_http_contrato_without_endpoint() {
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "cart".into(),
            para: "catalog".into(),
            wit: "wasi:http/proxy".into(),
            endpoint: None,
            subject: None,
            slot: None,
        });
        let err = s.validate().unwrap_err();
        assert!(matches!(
            err,
            AplicacaoError::ContratoMissingTarget {
                expected: "endpoint",
                ..
            }
        ));
    }

    #[test]
    fn rejects_http_contrato_with_subject() {
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "cart".into(),
            para: "catalog".into(),
            wit: "wasi:http/proxy".into(),
            endpoint: Some("/x".into()),
            subject: Some("not.allowed.here".into()),
            slot: None,
        });
        let err = s.validate().unwrap_err();
        assert!(matches!(
            err,
            AplicacaoError::ContratoWrongTarget {
                expected: "endpoint",
                ..
            }
        ));
    }

    #[test]
    fn rejects_pubsub_contrato_without_subject() {
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "cart".into(),
            para: "catalog".into(),
            wit: "nats:pub-sub".into(),
            endpoint: None,
            subject: None,
            slot: None,
        });
        let err = s.validate().unwrap_err();
        assert!(matches!(
            err,
            AplicacaoError::ContratoMissingTarget {
                expected: "subject",
                ..
            }
        ));
    }

    #[test]
    fn rejects_pubsub_contrato_with_endpoint() {
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "cart".into(),
            para: "catalog".into(),
            wit: "kafka:topic".into(),
            endpoint: Some("/wrong".into()),
            subject: Some("topic.x".into()),
            slot: None,
        });
        let err = s.validate().unwrap_err();
        assert!(matches!(
            err,
            AplicacaoError::ContratoWrongTarget {
                expected: "subject",
                ..
            }
        ));
    }

    #[test]
    fn rejects_store_contrato_without_slot() {
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "cart".into(),
            para: "catalog".into(),
            wit: "wasi:keyvalue/store".into(),
            endpoint: None,
            subject: None,
            slot: None,
        });
        let err = s.validate().unwrap_err();
        assert!(matches!(
            err,
            AplicacaoError::ContratoMissingTarget {
                expected: "slot",
                ..
            }
        ));
    }

    #[test]
    fn rejects_unknown_wit_with_target_set() {
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "cart".into(),
            para: "catalog".into(),
            wit: "custom:exchange".into(),
            endpoint: Some("/leaked".into()),
            subject: None,
            slot: None,
        });
        let err = s.validate().unwrap_err();
        assert!(matches!(
            err,
            AplicacaoError::ContratoWrongTarget {
                expected: "none",
                ..
            }
        ));
    }

    #[test]
    fn unknown_wit_capability_only_validates() {
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "cart".into(),
            para: "catalog".into(),
            // A WIT world we haven't yet shaped — accept it as a typed
            // capability edge so authors aren't blocked while the WIT
            // registry catches up. No payload field may be carried.
            wit: "custom:exchange".into(),
            endpoint: None,
            subject: None,
            slot: None,
        });
        s.validate().unwrap();
        let added = s.contratos.last().unwrap();
        assert_eq!(added.target().unwrap(), WitTarget::Capability);
    }

    #[test]
    fn target_typed_view_round_trips_each_shape() {
        let http = contract_http("cart", "catalog", "/products/:id");
        assert_eq!(
            http.target().unwrap(),
            WitTarget::Http {
                endpoint: "/products/:id"
            }
        );
        let nats = WitContract {
            de: "a".into(),
            para: "b".into(),
            wit: "nats:pub-sub".into(),
            endpoint: None,
            subject: Some("topic.x".into()),
            slot: None,
        };
        assert_eq!(
            nats.target().unwrap(),
            WitTarget::PubSub { subject: "topic.x" }
        );
        let kv = WitContract {
            de: "a".into(),
            para: "b".into(),
            wit: "wasi:keyvalue/store".into(),
            endpoint: None,
            subject: None,
            slot: Some("checkout/$orderId".into()),
        };
        assert_eq!(
            kv.target().unwrap(),
            WitTarget::Store {
                slot: "checkout/$orderId"
            }
        );
    }

    #[test]
    fn wit_contract_kind_predicates() {
        let http = contract_http("a", "b", "/x");
        assert!(http.is_http());
        assert!(!http.is_pubsub());
        assert!(!http.is_store());

        let nats = WitContract {
            de: "a".into(),
            para: "b".into(),
            wit: "nats:pub-sub".into(),
            endpoint: None,
            subject: Some("topic.x".into()),
            slot: None,
        };
        assert!(nats.is_pubsub());
        assert!(!nats.is_http());

        let kv = WitContract {
            de: "a".into(),
            para: "b".into(),
            wit: "wasi:keyvalue/store".into(),
            endpoint: None,
            subject: None,
            slot: Some("checkout/$orderId".into()),
        };
        assert!(kv.is_store());
        assert!(!kv.is_http());
    }

    #[test]
    fn placement_strategy_variants_round_trip() {
        for s in [
            PlacementStrategy::SingleNode,
            PlacementStrategy::Replicated,
            PlacementStrategy::Sharded,
        ] {
            let p = Placement {
                estrategia: s,
                clusters: vec!["rio".into()],
                affinity: None,
                shard_key: if matches!(s, PlacementStrategy::Sharded) {
                    Some("$key".into())
                } else {
                    None
                },
            };
            let json = serde_json::to_string(&p).unwrap();
            let back: Placement = serde_json::from_str(&json).unwrap();
            assert_eq!(back, p);
        }
    }
}
