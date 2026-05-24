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
    /// fields agree, and that each carried value is itself
    /// value-shape valid:
    ///
    ///   - HTTP world (`wasi:http/*`, `http:*`) ⇒ exactly `:endpoint`,
    ///     non-empty, leading-`/` (Cilium L7 `path` + Gateway API
    ///     `PathPrefix` invariant — same shape required of `:entrada
    ///     :paths`)
    ///   - `PubSub` world (`nats:*`, `kafka:*`) ⇒ exactly `:subject`,
    ///     non-empty (NATS / Kafka publish without a subject is a
    ///     no-op subscribe, never the author's intent)
    ///   - Store world (`wasi:keyvalue/*`, `kv:*`) ⇒ exactly `:slot`,
    ///     non-empty (an empty slot template addresses the bucket
    ///     root, defeating the per-key isolation the slot exists for)
    ///   - Anything else ⇒ none of the three; the contract is a pure
    ///     typed capability edge with no payload selector.
    ///
    /// Translates the Apollo Federation discipline ("conflicts are
    /// errors at compile time, not warnings at runtime";
    /// MESH-COMPOSITION §II.3) onto pleme-io's typed Aplicacao surface:
    /// a contract whose WIT shape disagrees with its target field, or
    /// whose target field carries a value-shape-invalid string, is a
    /// build error — not a silent renderer drop. The returned
    /// [`WitTarget`] view's `&str` payload is therefore guaranteed
    /// non-empty (and absolute, for `Http`); every downstream consumer
    /// (caixa-mesh's L7 emission, the M3 Gateway/HTTPRoute renderer,
    /// the M4 per-edge policy resolver) can rely on that without
    /// re-checking.
    pub fn target(&self) -> Result<WitTarget<'_>, AplicacaoError> {
        let endpoint = self.endpoint.as_deref();
        let subject = self.subject.as_deref();
        let slot = self.slot.as_deref();
        let edge = || (self.de.clone(), self.para.clone(), self.wit.clone());

        // The `:wit` value drives every downstream dispatch — the
        // is_http/is_pubsub/is_store prefix matchers below, the
        // caixa-mesh L7-vs-L4 emission, the cycle-detector's pub-sub
        // exclusion. Until this gate landed `target()` accepted any
        // non-empty string and silently demoted unrecognized shapes to
        // a capability-only edge (`:wit "WASI:HTTP/proxy"` — uppercase
        // typo, `:wit "wasi-http/proxy"` — hyphen-instead-of-colon typo,
        // `:wit "wasi:http proxy"` — whitespace, `:wit "wasi:"` — empty
        // package, the paste-from-binary footgun a multi-line blob
        // accidentally landing in the slot, the un-percent-encoded
        // non-ASCII byte) — the canonical "I thought I had L7 HTTP
        // routing, got L4-only" footgun. Empty is still pre-checked at
        // the [`AplicacaoSpec::validate`] call site via the narrower
        // [`AplicacaoError::EmptyWit`] variant (and fires first at the
        // validate layer); the value-shape gate here picks up the
        // structurally-invalid non-empty cases the empty check misses,
        // and remains correct under direct `target()` calls outside
        // validate (the predicate's defensive empty arm returns a
        // parser-shaped reason rather than silently falling through to
        // the Capability arm). Same trajectory as c4213a4 (WitContract
        // endpoint/subject/slot value-shape gates lifted into
        // `target()`) on the peer payload axes.
        if let Err(reason) = crate::render::is_wit_world_ref(&self.wit) {
            let (de, para, wit) = edge();
            return Err(AplicacaoError::ContratoWitInvalid {
                de,
                para,
                wit,
                reason,
            });
        }

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
            let ep = endpoint.ok_or_else(|| {
                let (de, para, wit) = edge();
                AplicacaoError::ContratoMissingTarget {
                    de,
                    para,
                    wit,
                    expected: "endpoint",
                }
            })?;
            if ep.is_empty() {
                return Err(AplicacaoError::ContratoEndpointEmpty {
                    de: self.de.clone(),
                    para: self.para.clone(),
                });
            }
            if !ep.starts_with('/') {
                return Err(AplicacaoError::ContratoEndpointNotAbsolute {
                    de: self.de.clone(),
                    para: self.para.clone(),
                    endpoint: ep.to_string(),
                });
            }
            // The `:endpoint` lands verbatim as a Cilium L7 `path:` rule
            // (caixa-mesh/src/lib.rs:311) and shares the K8s Gateway
            // API v1 HTTPPathMatch.value admission grammar with the
            // sibling `:entrada :paths` axis. Until this gate landed
            // `target()` only refused the empty string + the missing-
            // leading-`/` form; a structurally invalid endpoint
            // (`"/charge?token=X"` — query in path slot, `"/foo bar"` —
            // un-percent-encoded whitespace, `"/api/café"` — non-ASCII,
            // `"/api//bar"` — consecutive slash, `"/api/../etc"` —
            // path-traversal segment, the >1024-byte slug) silently
            // passed validate and the failure surfaced at apply time
            // as a Cilium policy rejection / silent traffic drop, far
            // from the source caixa.lisp. Same Gateway API HTTPPathMatch
            // grammar `:entrada :paths` already gates (55410e4), now
            // shared with `:contratos :endpoint` through the lifted
            // `crate::render::is_gateway_api_http_path` predicate.
            if let Err(reason) = crate::render::is_gateway_api_http_path(ep) {
                return Err(AplicacaoError::ContratoEndpointInvalid {
                    de: self.de.clone(),
                    para: self.para.clone(),
                    endpoint: ep.to_string(),
                    reason,
                });
            }
            return Ok(WitTarget::Http { endpoint: ep });
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
            let s = subject.ok_or_else(|| {
                let (de, para, wit) = edge();
                AplicacaoError::ContratoMissingTarget {
                    de,
                    para,
                    wit,
                    expected: "subject",
                }
            })?;
            if s.is_empty() {
                return Err(AplicacaoError::ContratoSubjectEmpty {
                    de: self.de.clone(),
                    para: self.para.clone(),
                });
            }
            // The `:subject` lands at runtime as the NATS subject the
            // producer publishes to and the consumer subscribes from.
            // Until this gate landed `target()` only refused the
            // empty string; a structurally invalid subject
            // (`"foo..bar"` — empty token between separators,
            // `"foo.>.bar"` — non-trailing `>` wildcard the NATS
            // server's subject parser rejects, `"foo bar"` —
            // un-percent-encoded whitespace, `"foo.café"` —
            // un-percent-encoded non-ASCII, `".foo"` / `"foo."` —
            // empty leading/trailing tokens, the >256-byte
            // paste-from-binary slug) silently passed validate and
            // the failure surfaced at runtime as a NATS server-side
            // `-ERR 'Invalid Subject'` on publish / subscribe, or as
            // a silent message drop, far from the source caixa.lisp.
            // Same Gateway API HTTPPathMatch / WIT-IDL grammar
            // trajectory `:contratos :endpoint` (4f0390b) and
            // `:contratos :wit` (6226bf4) already gate, now shared
            // with `:contratos :subject` through the lifted
            // `crate::render::is_nats_subject` predicate.
            if let Err(reason) = crate::render::is_nats_subject(s) {
                return Err(AplicacaoError::ContratoSubjectInvalid {
                    de: self.de.clone(),
                    para: self.para.clone(),
                    subject: s.to_string(),
                    reason,
                });
            }
            return Ok(WitTarget::PubSub { subject: s });
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
            let sl = slot.ok_or_else(|| {
                let (de, para, wit) = edge();
                AplicacaoError::ContratoMissingTarget {
                    de,
                    para,
                    wit,
                    expected: "slot",
                }
            })?;
            if sl.is_empty() {
                return Err(AplicacaoError::ContratoSlotEmpty {
                    de: self.de.clone(),
                    para: self.para.clone(),
                });
            }
            // Value-shape gate on the third (and last) typed payload
            // axis the `WitContract::target` dispatch carries — the
            // peer of [`crate::render::is_gateway_api_http_path`] for
            // `:endpoint` (4f0390b) and [`crate::render::is_nats_subject`]
            // for `:subject` (63e18a0). Until this gate landed
            // `target()` only refused the empty string; a structurally
            // invalid slot (`"check out/$order"` — un-percent-encoded
            // whitespace whose runtime behavior varies unpredictably
            // across kv backends, `"checkout/\x01order"` — control
            // character that Redis admits but corrupts on next read
            // and DynamoDB rejects outright, `"chéckout/$order"` —
            // un-percent-encoded non-ASCII byte each backend re-encodes
            // differently, `"checkout\n/$order"` — embedded newline,
            // the 513-byte paste-from-binary slug) silently passed
            // validate and surfaced at runtime as a per-backend kv
            // write rejection (DynamoDB / etcd) or as a silent
            // next-read corruption (Redis-via-RESP3), far from the
            // source caixa.lisp with no field naming which `:contratos`
            // edge carried the typo. The lifted predicate makes the
            // kv-backend intersection-floor a substrate-level
            // invariant at validate time, not a runtime "this passed
            // validate but the kv backend rejected on first write"
            // surprise — closes the typed payload-axis value-shape
            // trajectory across all three legs of the four
            // [`WitTarget`] arms (HTTP / PubSub / Store / Capability)
            // that caixa-mesh + the future kv emitters land in.
            if let Err(reason) = crate::render::is_wasi_keyvalue_slot(sl) {
                return Err(AplicacaoError::ContratoSlotInvalid {
                    de: self.de.clone(),
                    para: self.para.clone(),
                    slot: sl.to_string(),
                    reason,
                });
            }
            return Ok(WitTarget::Store { slot: sl });
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

/// Borrowed identity key for the typed-graph duplicate-`:contratos`
/// gate (see [`AplicacaoSpec::validate`]): every field that
/// distinguishes one contract from another, in declaration order
/// (`(de, para, wit, endpoint, subject, slot)`). Two [`WitContract`]s
/// with equal [`ContratoIdentity`]s are the same typed edge declared
/// twice — the graph-edge analogue of duplicate `:membros` /
/// `:placement :clusters` / `:entrada :paths` entries. Lifted as a
/// type alias so the duplicate-gate's `HashSet<…>` type doesn't trip
/// clippy's `type_complexity` lint (and so a future axis added to
/// `WitContract` is one alias edit, not a coordinated rewrite of
/// every set instantiation).
type ContratoIdentity<'a> = (
    &'a str,
    &'a str,
    &'a str,
    Option<&'a str>,
    Option<&'a str>,
    Option<&'a str>,
);

/// Render the target payload field of a [`WitContract`] as a stable
/// human-readable label (`:endpoint "/charge"`, `:subject "events.x"`,
/// `:slot "checkout/$order"`, or `(capability — no payload)` when the
/// WIT world is a pure capability edge).
///
/// Used by the [`AplicacaoSpec::validate`] duplicate-`:contratos` gate
/// so the diagnostic names *which* identical edge was declared twice
/// (not just which `(de, para, wit)` triple). Lifted to a typed
/// helper so the format is the single source of truth — every future
/// duplicate-edge diagnostic (per-edge policy resolver in M4, the
/// `feira app graph` view) reaches for the same label shape rather
/// than rolling its own.
fn contrato_target_label(c: &WitContract) -> String {
    if let Some(e) = &c.endpoint {
        format!(":endpoint {e:?}")
    } else if let Some(s) = &c.subject {
        format!(":subject {s:?}")
    } else if let Some(s) = &c.slot {
        format!(":slot {s:?}")
    } else {
        "(capability — no payload)".to_string()
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

impl MeshPolicy {
    /// True when no `:politicas` axis carries a value — every field is
    /// `None`. The same emptiness contract every other M2/M3 typed
    /// surface carries ([`crate::LimitsSpec::is_empty`],
    /// [`crate::BehaviorSpec::is_empty`]): renderers that overlay the
    /// typed slot onto a cluster artifact key off this predicate to
    /// decide "emit the slot" vs "skip the slot entirely", so an
    /// authored-but-unset `:politicas (())` round-trips to a rendered
    /// artifact that's structurally identical to one that omits the
    /// slot. Lifted as a typed predicate (rather than per-renderer
    /// inline `politicas.timeout.is_none() && politicas.retries.is_none()
    /// && …` chains) so a future axis added to `MeshPolicy` (per-edge
    /// :politicas overlay in M4, per-Aplicacao traffic-shaping in M5)
    /// is one struct-field edit + one `&& self.<axis>.is_none()` here,
    /// not a coordinated rewrite of every consumer that's reaching
    /// for the emptiness semantic.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.timeout.is_none()
            && self.retries.is_none()
            && self.circuit_breaker.is_none()
            && self.mtls_required.is_none()
            && self.rate_limit.is_none()
    }
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

/// True when `window` is exactly one of the three canonical rate-limit
/// windows the [`rate_limit_codec`] round-trips losslessly: 1 second
/// (`"<n>/s"`), 1 minute (`"<n>/m"`), or 1 hour (`"<n>/h"`). Lifted
/// to a typed predicate (rather than an inline disjunction at the
/// [`AplicacaoSpec::validate_politicas`] call site) so the
/// canonical-window set lives in exactly one place — drift between
/// the codec's accepted unit set and the validate gate's accepted
/// window set is a build error visible at this predicate, not a
/// silent round-trip break at the codec layer. Same shape every other
/// predicate-on-the-typed-slot helper carries
/// ([`MeshPolicy::is_empty`], [`crate::LimitsSpec::is_empty`],
/// [`crate::BehaviorSpec::is_empty`]).
#[must_use]
fn is_canonical_rate_limit_window(window: Duration) -> bool {
    let secs = window.as_secs();
    window.subsec_nanos() == 0 && (secs == 1 || secs == 60 || secs == 3600)
}

/// K8s Gateway API v1 `Listener.hostname` / `HTTPRoute.spec.hostnames`
/// max length, in bytes — same value the apiserver-side OpenAPI
/// schema enforces (`maxLength: 253`, ultimately the RFC 1035 / RFC
/// 1123 DNS-name limit). Lifted as a typed const so a future M4 axis
/// reaching for the same bound (the `mesh.pleme.io/v1alpha1/Aplicacao`
/// CR materializer's per-host validation) reads from one place.
const ENTRADA_HOST_MAX_LEN: usize = 253;

/// Per-label max length for a DNS-1123 host label — same value the
/// Gateway API regex `[a-z0-9]([-a-z0-9]*[a-z0-9])?` bounds via the
/// apiserver-side OpenAPI schema (RFC 1035 / RFC 1123 label limit).
const ENTRADA_HOST_LABEL_MAX_LEN: usize = 63;

/// Reject `:membros :caixa` values the K8s apiserver would refuse at
/// admission time. Thin wrapper around [`crate::render::is_dns_1123_label`]
/// that maps the shared parser-shaped reason into the
/// [`AplicacaoError::MembroCaixaInvalid`] variant, so the diagnostic
/// is self-locating (the offending `caixa:` is named verbatim) and
/// the author can grep their caixa.lisp for `:caixa "<name>"` and
/// fix it in one edit. Same diagnostic shape as
/// [`AplicacaoError::EntradaHostInvalid`] (c7d05ec) and
/// [`AplicacaoError::MembroVersaoInvalid`] (9888b13).
fn validate_membro_caixa(caixa: &str) -> Result<(), AplicacaoError> {
    // Empty is already gated by `MembroCaixaEmpty` at the call site;
    // re-checking here keeps the predicate usable from any future
    // call site (the M4 CR materializer) without an empty-check
    // footgun.
    if caixa.is_empty() {
        return Err(AplicacaoError::MembroCaixaEmpty);
    }
    crate::render::is_dns_1123_label(caixa).map_err(|reason| AplicacaoError::MembroCaixaInvalid {
        caixa: caixa.to_string(),
        reason,
    })
}

/// Reject `:placement :clusters` entries the K8s apiserver would refuse
/// at admission time. Thin wrapper around [`crate::render::is_dns_1123_label`]
/// that maps the shared parser-shaped reason into the
/// [`AplicacaoError::PlacementClusterInvalid`] variant.
///
/// Cluster names land in DNS-1123-label territory across every consumer:
/// the K8s context name keying `kubeconfig`, the `clusters[]` filter
/// the `lareira-fleet-programs` aggregator applies to scope programs to
/// their owning cluster (caixa-mesh's `placement.clusters` overlay,
/// 4d91c0b), the namespace prefix the future cross-cluster fan-out
/// emits per entry, and the `cluster.x-k8s.io/v1beta1/Cluster.metadata.name`
/// cluster identity the M4 CR materializer round-trips. Each apiserver-
/// side schema enforces the DNS-1123 label rule on admission; a
/// structurally invalid cluster name (`"Rio"`, `"my_cluster"`,
/// `"team.rio"`, `"-rio"`, `"rio-"`, the >63-byte UUID-shaped
/// mistaken-identity slug) silently passes the prior empty-/duplicate-
/// only gate and the failure surfaces as a no-match at filter time —
/// the workload doesn't land in the named cluster, with no diagnostic
/// naming the offending `:clusters` entry. Lifting the gate to caixa-
/// build time mirrors the `:membros :caixa` value-shape trajectory
/// (3f9d7a0) on the peer name axis.
///
/// The diagnostic carries the offending `cluster:` verbatim plus a
/// parser-shaped `reason:` naming the specific violation, so the
/// author can grep their caixa.lisp for `:clusters` and fix it in
/// one edit. Same diagnostic shape as
/// [`AplicacaoError::MembroCaixaInvalid`] (3f9d7a0).
fn validate_placement_cluster(cluster: &str) -> Result<(), AplicacaoError> {
    // Empty is already gated by `PlacementClusterEmpty` at the call
    // site; re-checking here keeps the predicate usable from any
    // future call site (the M4 CR materializer's per-cluster validator)
    // without an empty-check footgun.
    if cluster.is_empty() {
        return Err(AplicacaoError::PlacementClusterEmpty);
    }
    crate::render::is_dns_1123_label(cluster).map_err(|reason| {
        AplicacaoError::PlacementClusterInvalid {
            cluster: cluster.to_string(),
            reason,
        }
    })
}

/// Reject `:entrada :host` values the K8s Gateway API v1 apiserver
/// would refuse at admission time. The contract — exactly the regex
/// the Gateway API CRD's OpenAPI schema enforces on `Listener.hostname`
/// and `HTTPRoute.spec.hostnames[]`,
/// `^(\*\.)?[a-z0-9]([-a-z0-9]*[a-z0-9])?(\.[a-z0-9]([-a-z0-9]*[a-z0-9])?)*$`
/// (max length 253; per-label max length 63):
///
///   - lowercase RFC 1123 DNS subdomain (`[a-z0-9-]` only; no
///     uppercase, no underscore, no Unicode/IDN — IDN must be
///     pre-encoded as Punycode `xn--…` by the author);
///   - exactly one optional leading wildcard label (`*.`); a wildcard
///     in any non-leading label position is rejected;
///   - each `.`-separated label is 1..=63 bytes, with non-hyphen
///     alphanumeric at both boundaries (no `-foo`, no `foo-`);
///   - total length 1..=253 bytes;
///   - no IPv4 literal (Gateway API forbids IP literals);
///   - no scheme (`https://`, `http://`), no port (`:8080`), no
///     whitespace, no path (`/`).
///
/// Lifted as a typed gate (rather than an inline cascade in
/// `validate()`) so the contract lives in one place — every future
/// per-host axis (the M4 `mesh.pleme.io/v1alpha1/Aplicacao` CR
/// materializer's host validator, the future per-`:entrada` SAN
/// emission for cert-manager Certificates, the multi-`:entrada`
/// host-collision gate when M4 lands `:entrada` as a `Vec`) reaches
/// for the same predicate, not its own. Same compounding shape as
/// `is_canonical_rate_limit_window` (808017c) and
/// `contrato_target_label` (5dbcfaf).
///
/// The diagnostic carries the offending `host:` verbatim plus a
/// parser-shaped `reason:` naming the specific violation, so the
/// author can grep their caixa.lisp for `:host "<host>"` and fix it
/// in one edit. Same diagnostic shape as `MembroVersaoInvalid`
/// (9888b13).
fn validate_entrada_host(host: &str) -> Result<(), AplicacaoError> {
    // Empty is already gated by `EmptyEntradaHost` at the call site;
    // re-checking here keeps the predicate usable from any future
    // call site (M4 CR materializer) without an empty-check footgun.
    if host.is_empty() {
        return Err(AplicacaoError::EmptyEntradaHost);
    }
    if host.len() > ENTRADA_HOST_MAX_LEN {
        return Err(AplicacaoError::EntradaHostInvalid {
            host: host.to_string(),
            reason: format!(
                "exceeds Gateway API v1 Hostname max length of {ENTRADA_HOST_MAX_LEN} bytes \
                 (got {} bytes; the K8s apiserver rejects longer hostnames at admission time)",
                host.len()
            ),
        });
    }
    if host.contains("://") {
        return Err(AplicacaoError::EntradaHostInvalid {
            host: host.to_string(),
            reason: "must not carry a scheme (drop the `https://` or `http://` prefix; \
                     Gateway API takes the bare hostname)"
                .to_string(),
        });
    }
    if host.contains('/') {
        return Err(AplicacaoError::EntradaHostInvalid {
            host: host.to_string(),
            reason: "must not carry a path (drop the `/…` suffix; Gateway API path \
                     matching is in `:entrada :paths`)"
                .to_string(),
        });
    }
    if host.bytes().any(|b| b.is_ascii_whitespace()) {
        return Err(AplicacaoError::EntradaHostInvalid {
            host: host.to_string(),
            reason: "must not contain whitespace".to_string(),
        });
    }

    // Strip the optional single leading wildcard label *before* the
    // trailing-dot check so the bare `"*."` form surfaces the more
    // self-locating "wildcard without domain" diagnostic instead of
    // the generic "trailing dot" one.
    let (had_wildcard, rest) = match host.strip_prefix("*.") {
        Some(r) => (true, r),
        None => (false, host),
    };
    if had_wildcard && rest.is_empty() {
        return Err(AplicacaoError::EntradaHostInvalid {
            host: host.to_string(),
            reason: "wildcard `*.` must be followed by a domain (e.g. `*.example.com`)".to_string(),
        });
    }
    if rest.contains('*') {
        return Err(AplicacaoError::EntradaHostInvalid {
            host: host.to_string(),
            reason: "wildcard `*` is allowed only as the first label (`*.example.com`); \
                     no inner or trailing `*` labels"
                .to_string(),
        });
    }
    if rest.ends_with('.') {
        return Err(AplicacaoError::EntradaHostInvalid {
            host: host.to_string(),
            reason: "must not have a trailing `.` (Gateway API hostnames are not \
                     fully-qualified with a root dot; the apiserver regex rejects \
                     trailing dots)"
                .to_string(),
        });
    }

    // Reject pure IPv4 literals: four dot-separated labels, every
    // label all-ASCII-digits. Gateway API v1 explicitly forbids IP
    // literals as Hostnames.
    let labels: Vec<&str> = rest.split('.').collect();
    if labels.len() == 4
        && labels
            .iter()
            .all(|l| !l.is_empty() && l.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(AplicacaoError::EntradaHostInvalid {
            host: host.to_string(),
            reason: "must not be an IPv4 literal (Gateway API v1 Hostname forbids IP \
                     literals; use a DNS name)"
                .to_string(),
        });
    }

    // Per-label shape: 1..=63 bytes, lowercase ASCII alphanumeric +
    // hyphen, with non-hyphen at both boundaries.
    for label in &labels {
        if label.is_empty() {
            return Err(AplicacaoError::EntradaHostInvalid {
                host: host.to_string(),
                reason: "has an empty label (consecutive `..` or a leading `.`)".to_string(),
            });
        }
        if label.len() > ENTRADA_HOST_LABEL_MAX_LEN {
            return Err(AplicacaoError::EntradaHostInvalid {
                host: host.to_string(),
                reason: format!(
                    "label {label:?} exceeds DNS-1123 label max length of \
                     {ENTRADA_HOST_LABEL_MAX_LEN} bytes (got {} bytes)",
                    label.len()
                ),
            });
        }
        let bytes = label.as_bytes();
        if !bytes[0].is_ascii_alphanumeric() || !bytes[bytes.len() - 1].is_ascii_alphanumeric() {
            return Err(AplicacaoError::EntradaHostInvalid {
                host: host.to_string(),
                reason: format!(
                    "label {label:?} must start and end with an alphanumeric \
                     (no leading or trailing `-`)"
                ),
            });
        }
        for &b in bytes {
            let valid = b.is_ascii_digit() || b.is_ascii_lowercase() || b == b'-';
            if !valid {
                let msg = if b.is_ascii_uppercase() {
                    format!(
                        "label {label:?} contains uppercase character {ch:?} \
                         (Gateway API hostnames are lowercase-only; use {lower:?})",
                        ch = b as char,
                        lower = label.to_ascii_lowercase()
                    )
                } else if b == b'_' {
                    format!(
                        "label {label:?} contains `_` (Gateway API hostnames \
                         allow only `[a-z0-9-]`; use `-` instead)"
                    )
                } else {
                    format!(
                        "label {label:?} contains invalid character {ch:?} \
                         (Gateway API hostnames allow only `[a-z0-9-]`)",
                        ch = b as char
                    )
                };
                return Err(AplicacaoError::EntradaHostInvalid {
                    host: host.to_string(),
                    reason: msg,
                });
            }
        }
    }
    Ok(())
}

/// Reject `:entrada :paths` entries the K8s Gateway API v1 apiserver
/// would refuse at admission time. Thin wrapper around
/// [`crate::render::is_gateway_api_http_path`] that maps the shared
/// parser-shaped reason into the [`AplicacaoError::EntradaPathInvalid`]
/// variant, preserving the more self-locating
/// [`AplicacaoError::EntradaPathEmpty`] /
/// [`AplicacaoError::EntradaPathNotAbsolute`] diagnostics when the
/// path fails those narrower invariants first.
///
/// The contract is the canonical HTTP-path grammar — `1..=
/// [`crate::render::GATEWAY_API_HTTP_PATH_MAX_LEN`] (1024) bytes,
/// leading `/`, no consecutive `/`, no `.`/`..` segments, no `?`/`#`/
/// whitespace/control/non-ASCII bytes — shared with the
/// `:contratos :endpoint` axis through the lifted predicate so drift
/// between either landing site and the K8s apiserver-side
/// HTTPPathMatch.value OpenAPI schema is a build error visible at
/// the predicate, not a per-renderer "this passed validate but failed
/// admission" surprise. The diagnostic carries the offending `path:`
/// verbatim plus a parser-shaped `reason:` naming the specific
/// violation, so the author can grep their caixa.lisp for `:paths`
/// and fix it in one edit. Same diagnostic shape as
/// [`AplicacaoError::ContratoEndpointInvalid`] on the peer HTTP-path
/// axis.
fn validate_entrada_path(path: &str) -> Result<(), AplicacaoError> {
    // Empty and missing-leading-`/` are already gated at the call
    // site by `EntradaPathEmpty` and `EntradaPathNotAbsolute`; re-
    // checking here keeps the per-axis narrower diagnostics in force
    // when the predicate is reached directly (and `is_gateway_api_http_path`
    // itself defends against `bytes[0]`-style indexing on empty
    // input).
    if path.is_empty() {
        return Err(AplicacaoError::EntradaPathEmpty);
    }
    if !path.starts_with('/') {
        return Err(AplicacaoError::EntradaPathNotAbsolute {
            path: path.to_string(),
        });
    }
    crate::render::is_gateway_api_http_path(path).map_err(|reason| {
        AplicacaoError::EntradaPathInvalid {
            path: path.to_string(),
            reason,
        }
    })
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
        let rate_trim = rate_str.trim();
        // The canonical authoring form for `:politicas :rate-limit` is
        // `<integer>/<s|m|h>` — every magnitude [`render`] emits is a
        // non-negative integer with no decimal point and no leading
        // sign, so the parser's accepted set must match for
        // serialize/deserialize to round-trip without canonical-form
        // drift. Until this gate landed the parser accepted any
        // `u32::from_str`-shaped magnitude — and current Rust
        // `u32::from_str` permissively accepts a leading `+` (`"+100"`
        // → 100), so `"+100/s"` parsed to `RateLimit { 100, 1s }` and
        // serde silently round-tripped to `"100/s"` on the next emit
        // (a *different* canonical string) — breaking the THEORY.md
        // Part V render-determinism contract on the fifth typed-codec
        // surface in caixa-core (peer with the four duration codecs the
        // 1c55a2a / 818dd38 / d1fd67b / 737a676 / d53c922 trajectory
        // already covered: `supervisor::duration_codec` backing three
        // typed-duration slots, `limits::parse_duration` backing
        // `:limits :wall-clock`, `limits::parse_byte_size` backing
        // `:limits :memory`). The fractional / decimal-shaped sibling
        // (`"1.5/s"`, `"1.0/s"`, `"0.5/m"`) lands on `u32::from_str`'s
        // existing rejection arm, but the diagnostic is value-laundered
        // (the bare `"rate-limit rate \"1.5\" not a u32"` wording
        // doesn't name the canonical-form remediation or the round-trip
        // drift the next emit would produce); this gate lifts the
        // fractional arm onto the same canonical-form diagnostic the
        // peer codecs carry.
        //
        // Strict canonical form: every byte of the magnitude is an
        // ASCII digit (no `.`, no `+`, no `-`). On non-digit-only
        // inputs the gate distinguishes "non-canonical-but-numeric"
        // (parses as f64 or i64 — surfaced with a self-locating
        // diagnostic naming the canonical authoring form and the
        // round-trip drift the rejected shape would produce on first
        // serialize) from "garbage" (parses as neither — surfaced with
        // the existing narrower `"not a u32"` wording so its
        // diagnostic shape remains stable for the parser-shape footgun
        // case).
        let digit_only = !rate_trim.is_empty() && rate_trim.bytes().all(|b| b.is_ascii_digit());
        if !digit_only {
            let numeric = rate_trim.parse::<f64>().is_ok() || rate_trim.parse::<i64>().is_ok();
            if numeric {
                return Err(format!(
                    "rate-limit: rate {rate_trim:?} is not a non-negative integer — the \
                     canonical authoring form for `:politicas :rate-limit` is \
                     `<integer>/<s|m|h>` (e.g. `\"100/s\"`, `\"5000/m\"`, `\"10000/h\"`) \
                     with no decimal point and no leading `+` / `-` sign. A fractional / \
                     signed magnitude (`\"1.5/s\"`, `\"+100/s\"`, `\"-1/s\"`) round-trips \
                     through `render` to a *different* canonical form (`\"1/s\"`, \
                     `\"100/s\"`, parser-reject) on first serialize — breaking the \
                     THEORY.md Part V render-determinism contract every typed slot \
                     carries. Pick an integer rate that fits the desired window \
                     (write `\"6000/m\"` instead of `\"1.66/s\"`)"
                ));
            }
            return Err(format!("rate-limit rate {rate_str:?} not a u32"));
        }
        // The digit-only gate guarantees every byte is `[0-9]`, so the
        // only way `u32::from_str` can fail here is overflow (the
        // magnitude exceeds `u32::MAX`). Surface that with an
        // overflow-shaped wording so the diagnostic names the offending
        // magnitude verbatim rather than collapsing onto the
        // non-canonical arm. Same shape `supervisor::duration_codec`
        // (1c55a2a) carries on the peer duration-codec axis.
        let rate: u32 = rate_trim.parse::<u32>().map_err(|_| {
            format!("rate-limit rate {rate_trim:?} (digit-only magnitude overflows u32)")
        })?;
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
            // Defensive fallback for non-canonical windows. Note:
            // [`AplicacaoSpec::validate_politicas`] rejects any
            // non-canonical `:rate-limit :window` via
            // [`AplicacaoError::PolicyRateLimitWindowNotCanonical`], so
            // a validated `RateLimit` never reaches this branch. The
            // emitted `<n>/<k>s` form is *not* round-trippable through
            // [`parse`] (which accepts only `s`/`m`/`h` unit strings,
            // not `<k>s` with an explicit count) — the validate gate
            // is what makes the round-trip a structural property; this
            // branch exists only so a programmatic non-validated
            // serialize doesn't panic.
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
    ///   - `:membros` is non-empty; every entry has a non-empty `:caixa`
    ///     and a non-empty `:versao`; no two entries share the same
    ///     `:caixa` (MESH-COMPOSITION §III.1 — the graph nodes are a set,
    ///     not a multiset)
    ///   - every `:contratos` :de + :para must be in `:membros`
    ///   - no two `:contratos` entries agree on
    ///     `(de, para, wit, endpoint, subject, slot)` — the typed-graph
    ///     edges are a set, not a multiset (peer of the `:membros` /
    ///     `:placement :clusters` / `:entrada :paths` duplicate gates)
    ///   - `:entrada :para` must be in `:membros`
    ///   - `:placement Sharded` must declare `:shard-key` (non-empty);
    ///     `:placement Replicated`/`SingleNode` must NOT declare
    ///     `:shard-key` — only the hash-keyed Akka-cluster-sharding axis
    ///     consumes it (MESH-COMPOSITION §II.4), and the typed partition
    ///     between strategy and shard-key is symmetric: every validated
    ///     `Placement` has `shard_key.is_some()` iff `estrategia ==
    ///     Sharded`
    ///   - every `:placement` strategy must declare ≥1 `:clusters` entry —
    ///     `Replicated`/`SingleNode` need hosting clusters, `Sharded` needs
    ///     the shard pool (MESH-COMPOSITION §III.1)
    ///   - every `:clusters` entry is non-empty and unique
    ///   - `:placement :affinity`, when set, is non-empty
    ///   - the synchronous-`:contratos` subgraph is acyclic
    ///     (MESH-COMPOSITION §III.3)
    ///   - every declared `:politicas` value is operationally meaningful
    ///     (zero timeout, zero retries, zero breaker thresholds, zero rate
    ///     limit are all build errors — MESH-COMPOSITION §V CSE invariants;
    ///     omit the field instead to express "no policy on this axis")
    pub fn validate(&self) -> Result<(), AplicacaoError> {
        self.validate_membros()?;
        let names: std::collections::HashSet<&str> =
            self.membros.iter().map(|m| m.caixa.as_str()).collect();

        // Identity key for the typed-edge duplicate gate below: every
        // field that distinguishes one contract from another. Two
        // entries that agree on all six are *the same edge declared
        // twice*, the typed-graph analogue of duplicate `:membros` /
        // `:placement :clusters` / `:entrada :paths` entries (which
        // are already build errors at this layer). Rejecting it at the
        // validate gate closes a renderer-side footgun: caixa-mesh's
        // `cilium_network_policies` keys each emitted policy by
        // `<aplicacao>-<de>-to-<para>`, so two contracts with identical
        // (de, para) and identical payload would land as two K8s
        // objects with colliding `metadata.name`, rejected at apply
        // time far from the source caixa.lisp.
        let mut seen_contracts: std::collections::HashSet<ContratoIdentity<'_>> =
            std::collections::HashSet::new();
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
            // Contract identity: (de, para, wit, endpoint, subject, slot).
            // Two contracts that match on all six are the same typed edge
            // declared twice — author error, not a legitimate variant of
            // "same caller-callee pair, different payload" (e.g.
            // cart→catalog at /products vs /search), which keeps distinct
            // identity keys via the differing endpoint payloads.
            let key = (
                c.de.as_str(),
                c.para.as_str(),
                c.wit.as_str(),
                c.endpoint.as_deref(),
                c.subject.as_deref(),
                c.slot.as_deref(),
            );
            if !seen_contracts.insert(key) {
                return Err(AplicacaoError::ContratoDuplicate {
                    de: c.de.clone(),
                    para: c.para.clone(),
                    wit: c.wit.clone(),
                    target: contrato_target_label(c),
                });
            }
        }

        // Cycles in the synchronous-edge subgraph are build errors
        // (MESH-COMPOSITION §III.3). Pub-sub edges are excluded — they
        // are "acyclic by construction" because the publisher fires
        // and forgets, so no caller blocks on a downstream that loops
        // back to it.
        self.detect_sync_cycles()?;

        if let Some(e) = &self.entrada {
            if !names.contains(e.para.as_str()) {
                return Err(AplicacaoError::EntradaMemberMissing {
                    para: e.para.clone(),
                });
            }
            if e.host.is_empty() {
                return Err(AplicacaoError::EmptyEntradaHost);
            }
            // The `:host` lands verbatim as a K8s Gateway API v1
            // `Listener.hostname` *and* `HTTPRoute.spec.hostnames[0]` —
            // both apiserver-validated against the same restrictive
            // pattern: lowercase RFC 1123 DNS subdomain, optional
            // single leading wildcard label (`*.`), max length 253,
            // per-label max length 63, no IP literals, no scheme,
            // no port. Until this gate landed `validate()` only
            // refused the empty string (`EmptyEntradaHost`); a
            // structurally invalid hostname (`"https://example.com"`,
            // `"checkout.quero.cloud:8080"`, `"1.2.3.4"`,
            // `"_underscored.example.com"`, `"FOO.example.com"`,
            // `"checkout.quero.cloud."`) silently passed validate
            // and the apiserver `field is invalid` error surfaced at
            // `kubectl apply` time, far from the source caixa.lisp.
            // Lifting the gate to caixa-build time mirrors the
            // `:entrada :paths` value-shape trajectory (eb3456d) and
            // closes the last unstructured `:entrada` axis.
            validate_entrada_host(&e.host)?;
            if e.port == 0 {
                return Err(AplicacaoError::EntradaPortZero);
            }
            // Each `:entrada :paths` entry becomes a K8s Gateway API
            // HTTPRoute `matches[].path.value`. The Gateway API rejects
            // values that don't start with `/` for `type: PathPrefix`,
            // and an empty value is meaningless. Surface those as build
            // errors (MESH-COMPOSITION §III.3) rather than apply-time
            // failures. Empty `:paths` itself is fine — caixa-mesh
            // falls back to a single `/` catch-all.
            let mut seen = std::collections::HashSet::new();
            for p in &e.paths {
                if p.is_empty() {
                    return Err(AplicacaoError::EntradaPathEmpty);
                }
                if !p.starts_with('/') {
                    return Err(AplicacaoError::EntradaPathNotAbsolute { path: p.clone() });
                }
                // Per-entry value-shape gate: the path lands verbatim
                // as a K8s Gateway API HTTPRoute `matches[].path.value`
                // (caixa-mesh/src/lib.rs:498), apiserver-validated
                // against `maxLength: 1024` + the Gateway API webhook's
                // path-grammar rules (no `//`, no `/./`, no `/../`, no
                // query/fragment separators, no whitespace, no control
                // characters, no non-ASCII bytes). Until this gate
                // landed `validate` only refused the empty string and
                // missing-leading-slash (eb3456d); a structurally
                // invalid path (`"/api?q=1"`, `"/api#frag"`,
                // `"/api bar"`, `"/api/../etc"`, `"/api//cart"`, a
                // 1025-byte URL-shaped slug) silently passed validate
                // and the failure surfaced at `kubectl apply` time as
                // a Gateway API webhook rejection, far from the source
                // caixa.lisp, with no field naming the offending
                // `:paths` entry. Lifting the gate to caixa-build time
                // mirrors the `:entrada :host` value-shape trajectory
                // (c7d05ec) on the sibling axis — every author surface
                // that emits a Gateway API field now matches the
                // apiserver's accepted set at validate time.
                validate_entrada_path(p)?;
                if !seen.insert(p.as_str()) {
                    return Err(AplicacaoError::EntradaPathDuplicate { path: p.clone() });
                }
            }
        }

        self.validate_placement()?;

        self.validate_politicas()?;

        Ok(())
    }

    /// Reject `:membros` values that are operationally meaningless. The
    /// `:membros` slot is the graph node set (MESH-COMPOSITION §III.1):
    /// every entry names a Servico that participates in the Aplicacao,
    /// and the rendered programs.yaml fan-out emits one entry per
    /// `:membros`. Three authoring footguns are closed here:
    ///
    ///   - `:caixa ""` — caixa-mesh's `programs_for_aplicacao` would emit
    ///     a `programs:` entry whose `name:` is the empty string, which
    ///     downstream `lareira-fleet-programs` rejects at template time
    ///     with a non-localized error;
    ///   - `:versao ""` — caixa-resolver's lacre pipeline can't resolve
    ///     an empty semver constraint, so the failure surfaces far from
    ///     the source caixa.lisp;
    ///   - duplicate `:caixa` names — two entries with the same name
    ///     produce duplicate programs.yaml entries (one silently
    ///     overwrites the other in the cluster's HelmRelease values), and
    ///     contract membership lookups against `:contratos` collapse the
    ///     two onto one node, masking authoring mistakes.
    ///
    /// Same value-shape discipline as `:placement :clusters` (where empty
    /// + duplicate cluster names are rejected) and `:entrada :paths`
    /// (where empty + duplicate path entries are rejected). Lifting these
    /// invariants to the typed surface mirrors the MESH-COMPOSITION
    /// §III.3 promise that the `:membros` set — the load-bearing identity
    /// of the application graph — is well-formed by construction.
    fn validate_membros(&self) -> Result<(), AplicacaoError> {
        if self.membros.is_empty() {
            return Err(AplicacaoError::NoMembros);
        }
        let mut seen = std::collections::HashSet::new();
        for m in &self.membros {
            if m.caixa.is_empty() {
                return Err(AplicacaoError::MembroCaixaEmpty);
            }
            // Every emitted cluster artifact's `metadata.name` derives
            // from a `:membros :caixa` value verbatim — the rendered
            // programs.yaml entry's `name:` (caixa-mesh/src/lib.rs:133),
            // the [`crate::LABEL_PROGRAM`] label value on every CNP
            // endpointSelector / fromEndpoints (caixa-mesh/src/lib.rs:263,
            // 272), the composed CiliumNetworkPolicy `metadata.name`
            // (caixa-mesh/src/lib.rs:250), and the Gateway API HTTPRoute
            // `metadata.name` when the member is the `:entrada :para`
            // target (caixa-mesh/src/lib.rs:423). Each apiserver-side
            // schema enforces the DNS-1123 label rule on admission;
            // a structurally invalid member name (`"Cart"`, `"my_cart"`,
            // `"my.cart"`, `"-cart"`, `"cart-"`, the >63-byte UUID-shaped
            // mistaken-identity slug) silently passes the prior empty-/
            // duplicate-only gate and the failure surfaces at `kubectl
            // apply` time as a `metadata.name: Invalid value` rejection,
            // far from the source caixa.lisp, with no field naming the
            // offending `:membros` entry. Lifting the gate to caixa-build
            // time mirrors the `:entrada :host` value-shape trajectory
            // (c7d05ec) on the peer axis — every author surface that
            // emits a K8s name now matches the apiserver's accepted set
            // at validate time.
            validate_membro_caixa(&m.caixa)?;
            if m.versao.is_empty() {
                return Err(AplicacaoError::MembroVersaoEmpty {
                    caixa: m.caixa.clone(),
                });
            }
            // The author surface for `:versao` is the same Cargo-shaped
            // semver requirement string (`"^0.1"`, `"~0.1.2"`, `"0.1.0"`,
            // `"*"`) every `:deps` entry carries — and the lacre pipeline
            // resolves both axes through the same
            // [`crate::version::parse_requirement`] entry-point. Until
            // this gate landed `validate_membros` only refused the empty
            // string (`MembroVersaoEmpty`); a malformed-but-non-empty
            // requirement (`"^bad-version"`, `"~~"`, `">= "`, the
            // accidental Cargo-vs-npm `"^0.1.x"` typo) silently passed
            // validate and the parse failure surfaced at lacre-resolve
            // time — far from the source caixa.lisp, with a
            // `semver::Error` not naming which `:membros` entry carried
            // the typo. Mirroring the c4213a4 / b0c8389 / 808017c
            // typed-shape gate trajectory: the typed slot's valid set
            // matches its codec's accepted set, structurally — every
            // `Membro::versao` past validate is round-trippable through
            // [`crate::parse_requirement`] without re-checking at the
            // resolver layer.
            if let Err(e) = crate::parse_requirement(&m.versao) {
                return Err(AplicacaoError::MembroVersaoInvalid {
                    caixa: m.caixa.clone(),
                    versao: m.versao.clone(),
                    reason: e.to_string(),
                });
            }
            if !seen.insert(m.caixa.as_str()) {
                return Err(AplicacaoError::MembroDuplicate {
                    caixa: m.caixa.clone(),
                });
            }
        }
        Ok(())
    }

    /// Reject `:placement` values that are operationally meaningless or
    /// internally contradictory. Each strategy variant has the same
    /// invariants on `:clusters` (non-empty list, non-empty unique
    /// entries) — the §III.1 author surface is uniform on this axis,
    /// even though the *meaning* of the list differs by strategy
    /// (`Replicated`/`SingleNode` host the app; `Sharded` defines the
    /// shard pool).
    ///
    /// Empty cluster names or a `Some("")` `:shard-key`/`:affinity`
    /// are the same authoring footgun closed for `:politicas` zero
    /// values and `:entrada` empty paths: the field is *declared* but
    /// carries no meaning, so downstream renderers either skip it
    /// silently (cluster-fanout drops the empty entry, no diagnostic)
    /// or apply it literally and fail at admission time. Lifting both
    /// to build errors mirrors MESH-COMPOSITION §III.3's "placement
    /// violation is a build error" promise.
    ///
    /// `:shard-key` and `:estrategia` are typed-partitioned: the slot
    /// is required exactly when `:estrategia Sharded` (hash-keyed
    /// distribution, Akka cluster-sharding convention, §II.4) and
    /// refused on `:estrategia Replicated`/`SingleNode` (where no
    /// hash-keyed routing axis consumes it). The partition closes the
    /// "I think I configured sharding" footgun where an author writes
    /// `:placement (:estrategia Replicated :shard-key "tenantId")` and
    /// the typed slot's value silently vanishes at the renderer layer
    /// — every validated `Placement` past this call satisfies
    /// `shard_key.is_some() == matches!(estrategia, Sharded)`.
    fn validate_placement(&self) -> Result<(), AplicacaoError> {
        // Every strategy needs at least one named cluster: `Replicated`
        // and `SingleNode` use the list as hosting/takeover candidates
        // (Erlang/OTP distributed-app convention — see MESH-COMPOSITION
        // §II.1), while `Sharded` uses it as the shard pool
        // (Akka cluster-sharding convention — §II.4). An empty list is
        // meaningless under any of the three.
        if self.placement.clusters.is_empty() {
            return Err(AplicacaoError::PlacementWithoutClusters {
                estrategia: self.placement.estrategia,
            });
        }
        let mut seen = std::collections::HashSet::new();
        for c in &self.placement.clusters {
            // Per-entry value-shape gate: the cluster name lands in
            // every K8s context / `lareira-fleet-programs` aggregator
            // filter / future M4 CR materializer's per-cluster axis
            // a validated `:clusters` entry passes through, each
            // enforcing the DNS-1123 label rule on admission. Same
            // typed-shape trajectory as `:membros :caixa` (3f9d7a0)
            // on the peer name axis — both axes' validated values
            // are guaranteed-accepted by the apiserver without
            // re-validation at any downstream renderer or admission
            // layer.
            validate_placement_cluster(c)?;
            if !seen.insert(c.as_str()) {
                return Err(AplicacaoError::PlacementClusterDuplicate { cluster: c.clone() });
            }
        }
        if let Some(a) = &self.placement.affinity {
            if a.is_empty() {
                return Err(AplicacaoError::PlacementAffinityEmpty);
            }
        }
        match self.placement.estrategia {
            PlacementStrategy::Sharded => match &self.placement.shard_key {
                None => return Err(AplicacaoError::ShardedWithoutKey),
                Some(k) if k.is_empty() => return Err(AplicacaoError::ShardedKeyEmpty),
                Some(_) => {}
            },
            // `:shard-key` is the Akka-cluster-sharding axis
            // (MESH-COMPOSITION §II.4) — hash-keyed entity distribution
            // across the cluster pool. `Replicated` (active-active across
            // every named cluster) and `SingleNode` (Erlang/OTP
            // distributed-app takeover/failover, §II.1) have no hash-keyed
            // routing axis to consume the slot; downstream renderers
            // (caixa-mesh's `placement.shardKey` overlay at
            // caixa-mesh/src/lib.rs:909, the future M4 Akka-style cluster-
            // sharding reconciler) ignore `:shard-key` outside the
            // `Sharded` arm by construction. Until this gate landed an
            // author who wrote `:placement (:estrategia Replicated
            // :shard-key "tenantId")` (an off-by-one strategy typo, a
            // copy-paste from a Sharded sibling caixa, the "I think I
            // configured sharding" footgun) silently passed validate and
            // the typed slot's value vanished at the renderer layer with
            // no diagnostic — the canonical "declared-but-inert" footgun
            // the empty-:affinity / empty-shard-key / zero-:politicas /
            // empty-:contratos-target gates already close on every other
            // declare-but-no-opinion axis (2d71a9a / 5dbcfaf / c7c7799).
            // Lifting the rejection to a build-time gate closes the
            // Sharded ↔ non-Sharded partition over the typed
            // `:placement` slot: every validated `Placement` past this
            // call has `shard_key.is_some()` iff `estrategia ==
            // Sharded`, structurally — the future Akka reconciler can
            // reach for `placement.shard_key` knowing it's `Some` exactly
            // when the strategy consumes it, without re-deriving the
            // partition from inline strategy probes.
            PlacementStrategy::Replicated | PlacementStrategy::SingleNode => {
                if let Some(k) = &self.placement.shard_key {
                    return Err(AplicacaoError::ShardKeyOnNonSharded {
                        estrategia: self.placement.estrategia,
                        shard_key: k.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Reject `:politicas` values that are operationally meaningless.
    /// Each axis is optional — omitting it expresses "no policy on this
    /// axis". Carrying a *zero* value for a declared axis is the bug
    /// this function rejects: zero is either
    ///
    ///   - re-interpreted as "infinite" by downstream proxies (Envoy's
    ///     `RouteAction.timeout = 0s` disables the timeout entirely),
    ///     directly contradicting MESH-COMPOSITION §V CSE invariant
    ///     "every Aplicacao declares :politicas :timeout (no infinite
    ///     blocking)", or
    ///   - a renderer footgun (a 0-failure circuit breaker trips on the
    ///     first call; a 0-rate rate-limit denies every request).
    ///
    /// Lifting these "0 means the opposite of what you think" idioms to
    /// the typed Aplicacao surface as build errors mirrors the §III.3
    /// promise that contract drift, capability leaks, and cycles are all
    /// build errors — not runtime surprises.
    fn validate_politicas(&self) -> Result<(), AplicacaoError> {
        let p = &self.politicas;
        if let Some(t) = p.timeout {
            if t.is_zero() {
                return Err(AplicacaoError::PolicyTimeoutZero);
            }
        }
        if let Some(r) = p.retries {
            if r == 0 {
                return Err(AplicacaoError::PolicyRetriesZero);
            }
        }
        if let Some(cb) = &p.circuit_breaker {
            if cb.max_failures == 0 {
                return Err(AplicacaoError::PolicyBreakerZeroFailures);
            }
            if cb.window.is_zero() {
                return Err(AplicacaoError::PolicyBreakerZeroWindow);
            }
        }
        if let Some(rl) = &p.rate_limit {
            if rl.rate == 0 {
                return Err(AplicacaoError::PolicyRateLimitZero);
            }
            // The `:rate-limit` author surface is the canonical
            // `"<n>/<s|m|h>"` form, and the [`rate_limit_codec`] parser
            // accepts exactly the three-unit set (1s/60s/3600s) the
            // [`rate_limit_codec::render`] formatter emits the canonical
            // unit suffix for. A `RateLimit` whose `:window` is anything
            // else (zero, 30s, 45s, 120s, 86400s, …) is constructible
            // programmatically (struct literals in Rust + the typed
            // `Duration` field) but renders to a `<n>/<k>s` fragment
            // (the codec's fall-through) the parser then rejects on
            // round-trip — silently breaking the THEORY.md §V.2.7
            // render-determinism contract for any consumer that
            // serializes-then-deserializes the typed slot. Lifting the
            // canonical-window invariant to a build-time gate at
            // `validate_politicas` makes the codec's round-trip property
            // a structural property of the validated typed value:
            // every `RateLimit` past `AplicacaoSpec::validate` has a
            // window the codec round-trips losslessly, so the next
            // typed-slot wiring (the future `CiliumClusterwideEnvoyConfig`
            // emitter for `:politicas :rate-limit`, MESH-COMPOSITION
            // §III.2 #3) reaches for `rate_limit.window` knowing the
            // value is in the codec's accepted set without re-validating
            // at the renderer layer. Same trajectory as c4213a4 (typed
            // WitContract endpoint/subject/slot value-shape gates) and
            // the b0c8389 :behavior + :upgrade-from script-path lifts:
            // the typed slot's valid set matches its codec's accepted
            // set, structurally.
            if !is_canonical_rate_limit_window(rl.window) {
                return Err(AplicacaoError::PolicyRateLimitWindowNotCanonical {
                    window: rl.window,
                });
            }
        }
        Ok(())
    }

    /// Detect cycles in the synchronous-edge subgraph of `:contratos`.
    /// A synchronous edge is any contract whose typed [`WitTarget`] is
    /// `Http`, `Store`, or `Capability` — the caller blocks on the
    /// callee, so a cycle would deadlock at runtime. Pub-sub edges
    /// (`WitTarget::PubSub`) are skipped: an event publisher does not
    /// block on its subscribers, so they can never close a sync loop.
    ///
    /// Iterative DFS with three-coloring; the reported cycle is the
    /// path of caixa names traversed from the back-edge target around
    /// to itself, in declaration order. Adjacency lists and DFS roots
    /// are visited in `BTreeMap` key order so the diagnostic is
    /// deterministic across runs.
    fn detect_sync_cycles(&self) -> Result<(), AplicacaoError> {
        use std::collections::{BTreeMap, BTreeSet};

        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Mark {
            White,
            Gray,
            Black,
        }

        let mut adj: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for m in &self.membros {
            adj.entry(m.caixa.as_str()).or_default();
        }
        for c in &self.contratos {
            // target() was already called by validate(); re-running here
            // keeps detect_sync_cycles self-contained for callers that
            // reuse it (M4 per-edge policy resolver) without revalidating.
            if matches!(c.target()?, WitTarget::PubSub { .. }) {
                continue;
            }
            adj.entry(c.de.as_str())
                .or_default()
                .insert(c.para.as_str());
        }

        let mut color: BTreeMap<&str, Mark> = adj.keys().map(|k| (*k, Mark::White)).collect();
        let mut parent: BTreeMap<&str, &str> = BTreeMap::new();

        // Stable DFS root order — BTreeMap iteration is sorted by key.
        let roots: Vec<&str> = adj.keys().copied().collect();

        // Frame: (node, sorted-neighbours snapshot, next-edge index).
        for root in roots {
            if color.get(root).copied().unwrap_or(Mark::White) != Mark::White {
                continue;
            }
            let root_neighbors: Vec<&str> = adj
                .get(root)
                .map(|s| s.iter().copied().collect())
                .unwrap_or_default();
            let mut stack: Vec<(&str, Vec<&str>, usize)> = vec![(root, root_neighbors, 0)];
            color.insert(root, Mark::Gray);

            loop {
                // Read+advance the top frame in one borrow scope so we
                // can later mutate the stack (push/pop) without holding
                // a borrow across.
                let step: Option<(&str, Option<&str>)> = stack.last_mut().map(|top| {
                    let node = top.0;
                    if top.2 >= top.1.len() {
                        (node, None)
                    } else {
                        let nxt = top.1[top.2];
                        top.2 += 1;
                        (node, Some(nxt))
                    }
                });
                let Some((node, nxt_opt)) = step else { break };
                let Some(nxt) = nxt_opt else {
                    color.insert(node, Mark::Black);
                    stack.pop();
                    continue;
                };
                let nxt_color = color.get(nxt).copied().unwrap_or(Mark::White);
                match nxt_color {
                    Mark::Gray => {
                        // Reconstruct the cycle from `node` back through
                        // the parent chain to `nxt`, then close.
                        let mut cycle = Vec::new();
                        let mut cur = node;
                        cycle.push(cur.to_string());
                        while cur != nxt {
                            match parent.get(cur).copied() {
                                Some(p) => {
                                    cur = p;
                                    cycle.push(cur.to_string());
                                }
                                None => break,
                            }
                        }
                        cycle.reverse();
                        cycle.push(nxt.to_string());
                        return Err(AplicacaoError::ContratoCycle { cycle });
                    }
                    Mark::White => {
                        parent.insert(nxt, node);
                        color.insert(nxt, Mark::Gray);
                        let nxt_neighbors: Vec<&str> = adj
                            .get(nxt)
                            .map(|s| s.iter().copied().collect())
                            .unwrap_or_default();
                        stack.push((nxt, nxt_neighbors, 0));
                    }
                    Mark::Black => {}
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
    #[error(
        ":membros entry has empty :caixa (every member must name a Servico; \
         omit the entry instead of carrying an empty name)"
    )]
    MembroCaixaEmpty,
    #[error(
        ":membros entry :caixa {caixa:?} is not a valid DNS-1123 label: {reason} \
         (the K8s apiserver enforces this rule on every `metadata.name` / Service \
         name / label value the member name lands in; use a lowercase \
         alphanumeric + hyphen identifier like `\"checkout\"` or `\"cart-v2\"`)"
    )]
    MembroCaixaInvalid { caixa: String, reason: String },
    #[error(
        ":membros entry {caixa:?} has empty :versao (every member must pin a \
         semver constraint that resolves through the lacre pipeline)"
    )]
    MembroVersaoEmpty { caixa: String },
    #[error(
        ":membros entry {caixa:?} :versao {versao:?} is not a valid semver \
         requirement: {reason} (use Cargo-shaped forms like `\"^0.1\"`, \
         `\"~0.1.2\"`, `\"0.1.0\"`, or `\"*\"` — the same shape `:deps :versao` \
         carries; the lacre pipeline resolves both through the same parser)"
    )]
    MembroVersaoInvalid {
        caixa: String,
        versao: String,
        reason: String,
    },
    #[error(
        ":membros entry {caixa:?} appears more than once (the graph node set \
         is a set, not a multiset; duplicate members produce duplicate \
         programs.yaml entries and ambiguous :contratos membership lookups)"
    )]
    MembroDuplicate { caixa: String },
    #[error("contrato references caixa {caixa:?} not declared in :membros")]
    ContratoMemberMissing { caixa: String },
    #[error("contrato {de:?} → {para:?} has empty :wit")]
    EmptyWit { de: String, para: String },
    #[error(
        "contrato {de:?} → {para:?} :wit {wit:?} is not a valid WIT world reference: \
         {reason} (the substrate dispatches `:wit` values on the canonical \
         lowercase `<namespace>:<package>(/<interface>)?(@<version>)?` shape — \
         `wasi:http/proxy`, `nats:pub-sub`, `wasi:keyvalue/store` — and silently \
         demotes unmatched shapes to a capability-only L4 edge; use a lowercase \
         kebab-case identifier per segment)"
    )]
    ContratoWitInvalid {
        de: String,
        para: String,
        wit: String,
        reason: String,
    },
    #[error(":entrada routes to caixa {para:?} not declared in :membros")]
    EntradaMemberMissing { para: String },
    #[error(":entrada must declare a non-empty :host")]
    EmptyEntradaHost,
    #[error(
        ":entrada :host {host:?} is not a valid Gateway API v1 Hostname: {reason} \
         (the K8s apiserver enforces the same shape on Gateway `Listener.hostname` and \
         `HTTPRoute.spec.hostnames` at admission time; use a lowercase RFC 1123 DNS name \
         like `\"checkout.quero.cloud\"` or `\"*.quero.cloud\"`)"
    )]
    EntradaHostInvalid { host: String, reason: String },
    #[error(":entrada :port must be in 1..=65535, got 0")]
    EntradaPortZero,
    #[error(":entrada :paths entry is empty (use the empty list to match all)")]
    EntradaPathEmpty,
    #[error(
        ":entrada :paths entry {path:?} must start with `/` (Gateway API PathPrefix invariant)"
    )]
    EntradaPathNotAbsolute { path: String },
    #[error(
        ":entrada :paths entry {path:?} is not a valid Gateway API v1 HTTPPathMatch \
         value: {reason} (the K8s apiserver enforces the same shape on \
         `HTTPRoute.spec.rules[].matches[].path.value` at admission time; use a \
         single-`/`-prefixed printable-ASCII path like `\"/api/cart\"` — RFC 3986 \
         requires percent-encoding `%XX` for non-ASCII and whitespace)"
    )]
    EntradaPathInvalid { path: String, reason: String },
    #[error(":entrada :paths entry {path:?} appears more than once")]
    EntradaPathDuplicate { path: String },
    #[error(
        ":placement {estrategia:?} requires at least one :clusters entry \
         (Replicated/SingleNode: hosting/takeover candidates; Sharded: shard pool)"
    )]
    PlacementWithoutClusters { estrategia: PlacementStrategy },
    #[error(":placement :clusters entry is empty (cluster names must be non-empty)")]
    PlacementClusterEmpty,
    #[error(
        ":placement :clusters entry {cluster:?} is not a valid DNS-1123 label: {reason} \
         (cluster names land in the K8s context keying every per-cluster `kubeconfig`, \
         in the `lareira-fleet-programs` aggregator's `clusters[]` filter, and in the \
         future M4 cross-cluster fan-out's per-entry namespace prefix / cluster identity \
         — each enforces the DNS-1123 label rule; use a lowercase alphanumeric + hyphen \
         identifier like `\"rio\"` or `\"mar-east\"`)"
    )]
    PlacementClusterInvalid { cluster: String, reason: String },
    #[error(":placement :clusters entry {cluster:?} appears more than once")]
    PlacementClusterDuplicate { cluster: String },
    #[error(
        ":placement :affinity must be non-empty when set (omit :affinity to express \
         `no placement hint`)"
    )]
    PlacementAffinityEmpty,
    #[error(":placement Sharded requires :shard-key")]
    ShardedWithoutKey,
    #[error(
        ":placement Sharded :shard-key must be non-empty (a `Some(\"\")` shard key \
         hashes every entity onto the same shard, defeating sharding entirely)"
    )]
    ShardedKeyEmpty,
    #[error(
        ":placement {estrategia:?} carries :shard-key {shard_key:?} — only :estrategia \
         Sharded consumes :shard-key (hash-keyed entity distribution, Akka cluster-sharding \
         convention); :estrategia Replicated runs every cluster active-active and \
         :estrategia SingleNode takes over a single cluster at a time (Erlang/OTP \
         distributed-app convention) — both ignore the slot. Drop :shard-key, or switch \
         to :estrategia Sharded if hash-keyed routing is the intent"
    )]
    ShardKeyOnNonSharded {
        estrategia: PlacementStrategy,
        shard_key: String,
    },
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
    #[error(
        "HTTP contrato {de:?} → {para:?} :endpoint is empty (use a non-empty path \
         like `/charge`; an empty endpoint renders as a `path: \"\"` Cilium L7 rule \
         that matches no traffic and silently drops every request)"
    )]
    ContratoEndpointEmpty { de: String, para: String },
    #[error(
        "HTTP contrato {de:?} → {para:?} :endpoint {endpoint:?} must start with `/` \
         (Cilium L7 :path + Gateway API PathPrefix invariant — same shape required of \
         :entrada :paths)"
    )]
    ContratoEndpointNotAbsolute {
        de: String,
        para: String,
        endpoint: String,
    },
    #[error(
        "HTTP contrato {de:?} → {para:?} :endpoint {endpoint:?} is not a valid \
         Cilium L7 `path:` / Gateway API v1 HTTPPathMatch value: {reason} (caixa-mesh \
         emits the :endpoint verbatim as the Cilium L7 `path:` rule at \
         caixa-mesh/src/lib.rs:311; the K8s apiserver enforces the same HTTPPathMatch \
         shape on `:entrada :paths`. Use a single-`/`-prefixed printable-ASCII path \
         like `\"/charge\"` — RFC 3986 requires percent-encoding `%XX` for non-ASCII \
         and whitespace)"
    )]
    ContratoEndpointInvalid {
        de: String,
        para: String,
        endpoint: String,
        reason: String,
    },
    #[error(
        "pub-sub contrato {de:?} → {para:?} :subject is empty (publish without a \
         subject is a no-op subscribe; omit :subject only if the WIT world is not \
         pub-sub-shaped)"
    )]
    ContratoSubjectEmpty { de: String, para: String },
    #[error(
        "pub-sub contrato {de:?} → {para:?} :subject {subject:?} is not a valid \
         NATS subject: {reason} (the NATS server's subject parser enforces the \
         same shape — `.`-separated tokens of `[A-Za-z0-9_-]`, with the `*` \
         single-token and `>` multi-token wildcards — at publish/subscribe time; \
         use a token-by-token form like `\"checkout.events.charge.failed\"` or \
         `\"orders.*.completed\"` — a malformed subject silently drops every \
         message at runtime far from the source caixa.lisp)"
    )]
    ContratoSubjectInvalid {
        de: String,
        para: String,
        subject: String,
        reason: String,
    },
    #[error(
        "store contrato {de:?} → {para:?} :slot is empty (an empty slot template \
         addresses the bucket root, defeating the per-key isolation the slot exists \
         for; omit :slot only if the WIT world is not store-shaped)"
    )]
    ContratoSlotEmpty { de: String, para: String },
    #[error(
        "store contrato {de:?} → {para:?} :slot {slot:?} is not a valid \
         WASI keyvalue store slot template: {reason} (the substrate enforces \
         the printable-ASCII intersection-floor every kv backend admits — \
         use a single-token path / template expression like `\"checkout/$orderId\"`, \
         `\"users:{{tenant}}/{{id}}\"`, or `\"session.tokens.<sid>\"`; RFC 3986 requires \
         percent-encoding `%XX` for non-ASCII and whitespace — a malformed \
         slot either gets rejected on write by strict backends or silently \
         corrupts the next read on permissive ones, far from the source caixa.lisp)"
    )]
    ContratoSlotInvalid {
        de: String,
        para: String,
        slot: String,
        reason: String,
    },
    #[error(
        "synchronous :contratos form a cycle ({}); break with a NATS pub-sub edge \
         or an event-sourced indirection (MESH-COMPOSITION §III.3)",
        cycle.join(" → ")
    )]
    ContratoCycle { cycle: Vec<String> },
    #[error(
        ":contratos entry {de:?} → {para:?} (:wit {wit:?} {target}) appears more \
         than once (the typed graph edges are a set, not a multiset; duplicate \
         contracts would render as colliding CiliumNetworkPolicy `metadata.name` \
         values that K8s admission rejects far from the source caixa.lisp)"
    )]
    ContratoDuplicate {
        de: String,
        para: String,
        wit: String,
        target: String,
    },
    #[error(
        ":politicas :timeout must be > 0 (Envoy interprets a zero timeout as `infinite`, \
         contradicting MESH-COMPOSITION §V `no infinite blocking`); omit :timeout to \
         express `no per-call deadline on this axis`"
    )]
    PolicyTimeoutZero,
    #[error(
        ":politicas :retries must be > 0 when set; omit :retries to express \
         `no retries on transient failure`"
    )]
    PolicyRetriesZero,
    #[error(
        ":politicas :circuit-breaker :max-failures must be > 0 (a zero-threshold \
         breaker trips on the first call); omit :circuit-breaker to disable it"
    )]
    PolicyBreakerZeroFailures,
    #[error(
        ":politicas :circuit-breaker :window must be > 0 (a zero-window breaker \
         tracks no failures); omit :circuit-breaker to disable it"
    )]
    PolicyBreakerZeroWindow,
    #[error(
        ":politicas :rate-limit rate must be > 0 (a zero-rate limit denies every \
         request); omit :rate-limit to disable rate limiting"
    )]
    PolicyRateLimitZero,
    #[error(
        ":politicas :rate-limit :window must be exactly 1s, 1m (60s), or 1h (3600s) — \
         the canonical authoring forms `\"<n>/s\"`, `\"<n>/m\"`, `\"<n>/h\"` the \
         rate-limit codec round-trips losslessly; got {window:?} which renders to a \
         non-round-trippable form (omit :rate-limit to disable, or pick one of the \
         three canonical windows)"
    )]
    PolicyRateLimitWindowNotCanonical { window: Duration },
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
    fn rejects_empty_membro_caixa() {
        // A `:caixa ""` entry has no name to render into programs.yaml
        // and no caixa.lisp to resolve at lacre time.
        let mut s = three_member_spec();
        s.membros[1].caixa = String::new();
        assert_eq!(s.validate().unwrap_err(), AplicacaoError::MembroCaixaEmpty);
    }

    #[test]
    fn rejects_empty_membro_versao() {
        // A `:versao ""` entry can't pin a semver constraint, so the
        // lacre pipeline fails far from the source.
        let mut s = three_member_spec();
        s.membros[2].versao = String::new();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::MembroVersaoEmpty { ref caixa } if caixa == "payment"),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_duplicate_membro_caixa() {
        // Two `:membros` entries with the same `:caixa` collapse to one
        // node in the membership HashSet, which masks `:contratos`
        // membership errors and produces duplicate programs.yaml entries.
        let mut s = three_member_spec();
        s.membros.push(membro("cart", "^0.2"));
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::MembroDuplicate { ref caixa } if caixa == "cart"),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_invalid_membro_versao_requirement() {
        // The fail-before-pass-after pin: a non-empty but malformed
        // semver requirement (`"^bad-version"`) silently passed
        // `validate()` on every pre-gate codebase because the prior
        // shape only refused the empty string. The parse failure
        // surfaced far downstream at lacre-resolve time with a
        // `semver::Error` that didn't name which `:membros` entry
        // carried the typo. The new gate moves the check to caixa-build
        // time at the source caixa.lisp.
        let mut s = three_member_spec();
        s.membros[2].versao = "^bad-version".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::MembroVersaoInvalid { ref caixa, ref versao, .. }
                    if caixa == "payment" && versao == "^bad-version"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_membro_versao_with_double_caret_typo() {
        // `"^^0.1"` is the canonical doubled-caret typo — looks like a
        // Cargo-shaped requirement on first glance but fails the parser
        // because semver doesn't accept stacked operators. Pin this
        // adjacent-shape footgun explicitly so a future relaxation that
        // accepts "looks-canonical-but-isn't" forms surfaces here.
        let mut s = three_member_spec();
        s.membros[0].versao = "^^0.1".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::MembroVersaoInvalid { ref caixa, ref versao, .. }
                    if caixa == "catalog" && versao == "^^0.1"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_membro_versao_with_v_prefixed_tag() {
        // `"v0.1"` is the canonical "git-tag-shape leaking into the
        // semver requirement slot" typo — an author copies the
        // publish-side git-tag string verbatim into `:versao`, but
        // Cargo's semver parser rejects the leading `v` (only digits +
        // canonical operators are valid in the major-version
        // position). The gate's diagnostic names which member entry
        // carried the v-prefix so the fix is one edit, not a grep
        // through every member's `:versao`. (Note: bare `x`-glob
        // shorthands like `^0.1.x` are *accepted* by the semver crate
        // as an `*` wildcard on the patch axis — they're a Cargo-side
        // valid shape, not a typo, so the gate intentionally lets them
        // through.)
        let mut s = three_member_spec();
        s.membros[1].versao = "v0.1".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::MembroVersaoInvalid { ref caixa, ref versao, .. }
                    if caixa == "cart" && versao == "v0.1"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn accepts_canonical_membro_versao_forms() {
        // The four Cargo-shaped requirement forms `:deps :versao`
        // already accepts via `crate::parse_requirement` must pass the
        // membros gate without re-validating at the resolver layer.
        // Pin every leg so a future tightening of the canonical set
        // surfaces here as a test failure.
        for form in [
            "^0.1",      // caret — minor-range pin (the most common shape)
            "~0.1.2",    // tilde — patch-range pin
            "0.1.0",     // exact — single-version pin
            "*",         // wildcard — explicitly any-version (semver::VersionReq::STAR)
            ">=0.1, <2", // multi-range — comma-separated comparators
        ] {
            let mut s = three_member_spec();
            for m in &mut s.membros {
                m.versao = form.into();
            }
            s.validate()
                .unwrap_or_else(|e| panic!("canonical form {form:?} must validate, got {e:?}"));
        }
    }

    #[test]
    fn membro_versao_empty_takes_precedence_over_invalid() {
        // Order pin: the existing `MembroVersaoEmpty` diagnostic
        // (which doesn't try to parse) fires before the new
        // `MembroVersaoInvalid` parse-side diagnostic, so an empty
        // `:versao` keeps its narrower error message — `parse_requirement`
        // would also reject `""`, but the empty-string arm is the more
        // self-locating diagnostic for the author.
        let mut s = three_member_spec();
        s.membros[1].versao = String::new();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::MembroVersaoEmpty { ref caixa } if caixa == "cart"),
            "got {err:?}"
        );
    }

    #[test]
    fn membro_versao_invalid_fires_before_duplicate_check() {
        // Order pin: a malformed requirement on a non-duplicate entry
        // surfaces *its own* diagnostic (which names the offending
        // `:versao` string), even when a later entry would otherwise
        // collapse onto an earlier name. The per-entry shape gate runs
        // inline before the duplicate-key insert, parallel to
        // `membros_validation_runs_before_contratos_membership_check`
        // and `duplicate_contrato_gate_runs_after_target_shape_check`.
        let mut s = three_member_spec();
        s.membros[0].versao = "^bad".into();
        s.membros.push(membro("cart", "^0.2")); // would otherwise raise MembroDuplicate
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::MembroVersaoInvalid { ref caixa, .. } if caixa == "catalog"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn membro_versao_invalid_diagnostic_carries_offending_versao() {
        // The diagnostic-shape pin: the error names the offending
        // `:versao` value verbatim so the author can grep their
        // caixa.lisp without re-running the build, and carries a
        // non-empty `reason` from `semver::VersionReq::parse` so the
        // parser's own wording flows through to the diagnostic.
        let mut s = three_member_spec();
        s.membros[2].versao = "not-a-req".into();
        let err = s.validate().unwrap_err();
        let AplicacaoError::MembroVersaoInvalid {
            caixa,
            versao,
            reason,
        } = err
        else {
            panic!("expected MembroVersaoInvalid, got other variant");
        };
        assert_eq!(caixa, "payment");
        assert_eq!(versao, "not-a-req");
        assert!(
            !reason.is_empty(),
            "MembroVersaoInvalid `reason` must carry the parser's wording verbatim"
        );
    }

    #[test]
    fn membro_versao_invalid_runs_before_contratos_check() {
        // A malformed `:versao` on any member must surface its own
        // diagnostic (which names *which* member to fix) before any
        // `:contratos` membership lookup raises `ContratoMemberMissing`.
        // The `:contratos` gate runs after `validate_membros`, so this
        // is structurally guaranteed — pin it explicitly so a future
        // refactor that reorders the gates surfaces here.
        let mut s = three_member_spec();
        s.membros[1].versao = "^^0.1".into();
        // Add a contrato whose `:para` doesn't exist — would normally
        // raise ContratoMemberMissing at the membership lookup, but
        // the membros gate must fire first.
        s.contratos
            .push(contract_http("cart", "phantom", "/never-reached"));
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::MembroVersaoInvalid { .. }),
            "expected MembroVersaoInvalid to fire before ContratoMemberMissing, got {err:?}"
        );
    }

    #[test]
    fn membros_validation_runs_before_contratos_membership_check() {
        // If `:membros` carries a duplicate, the membership-collapse
        // would silently accept a `:contratos :para "phantom"` so long
        // as some entry hashes to "phantom". Pinning order: the
        // duplicate-membros error fires first, regardless of whether
        // contratos reference real members.
        let mut s = three_member_spec();
        s.membros = vec![
            membro("cart", "^0.1"),
            membro("cart", "^0.2"),
            membro("catalog", "^0.1"),
            membro("payment", "^0.1"),
        ];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::MembroDuplicate { ref caixa } if caixa == "cart"),
            "got {err:?}"
        );
    }

    #[test]
    fn distinct_membros_validate() {
        // Pin the happy-path: every `:membros` entry has a non-empty
        // `:caixa`, a non-empty `:versao`, and the set is duplicate-free.
        // The fixture already satisfies this; this test makes the
        // invariant explicit so a future refactor of the fixture can't
        // silently break the guarantee.
        three_member_spec().validate().unwrap();
    }

    // ── :membros :caixa DNS-1123 label value-shape gate ───────────────────

    #[test]
    fn rejects_membro_caixa_with_uppercase() {
        // The canonical "I copied the Servico's display name verbatim"
        // typo — caixa names are lowercase per K8s DNS-1123 label rule,
        // but author tools often round-trip a TitleCase or CamelCase
        // identifier from an ADR or a sketch. Pin the diagnostic names
        // the offending name and suggests the lower-cased fix in one
        // edit, mirroring the `rejects_entrada_host_with_uppercase`
        // gate's shape (c7d05ec).
        let mut s = three_member_spec();
        s.membros[1].caixa = "Cart".into();
        let err = s.validate().unwrap_err();
        let AplicacaoError::MembroCaixaInvalid { caixa, reason } = err else {
            panic!("expected MembroCaixaInvalid, got other variant");
        };
        assert_eq!(caixa, "Cart");
        assert!(
            reason.contains("uppercase"),
            "diagnostic must name the violation as `uppercase` (got: {reason:?})"
        );
        assert!(
            reason.contains("\"cart\""),
            "diagnostic must suggest the lower-cased fix verbatim (got: {reason:?})"
        );
    }

    #[test]
    fn rejects_membro_caixa_with_underscore() {
        // The canonical "I'm thinking of a Python module / Postgres
        // table" leak — `_` is forbidden by every DNS-1123 / DNS-1035
        // label schema. K8s rejects `metadata.name: my_cart` at admission
        // time with an opaque `field is invalid` (no source-citing
        // diagnostic). The gate moves it to caixa-build time.
        let mut s = three_member_spec();
        s.membros[0].caixa = "my_cart".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::MembroCaixaInvalid { ref caixa, ref reason }
                    if caixa == "my_cart" && reason.contains('_')
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_membro_caixa_with_dot() {
        // A `:membros :caixa` entry is a single DNS-1123 *label*, not a
        // subdomain — even though K8s `metadata.name` itself accepts
        // dots (DNS-1123 subdomain rule), this string also lands as a
        // K8s Service name (DNS-1035 label — no dots) and as a label
        // value on identity-based Cilium selectors. The strictest floor
        // among the use sites wins. The "I want to namespace my member
        // names with `.`" intent is expressed via `-` (e.g. `cart-v2`).
        let mut s = three_member_spec();
        s.membros[2].caixa = "team.cart".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::MembroCaixaInvalid { ref caixa, ref reason }
                    if caixa == "team.cart" && reason.contains('.')
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_membro_caixa_with_leading_hyphen() {
        // DNS-1123 / DNS-1035 boundary rule: labels must start and end
        // with an alphanumeric. The K8s apiserver rejects `-cart`
        // outright; the renderer would emit a `metadata.name: "-cart"`
        // that fails admission far from the source caixa.lisp.
        let mut s = three_member_spec();
        s.membros[0].caixa = "-cart".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::MembroCaixaInvalid { ref caixa, ref reason }
                    if caixa == "-cart" && reason.contains("start and end")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_membro_caixa_with_trailing_hyphen() {
        // The symmetric arm of the boundary rule. Pin separately so
        // both ends of the label are covered against a future relaxation
        // that only checks one boundary.
        let mut s = three_member_spec();
        s.membros[1].caixa = "cart-".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::MembroCaixaInvalid { ref caixa, .. }
                    if caixa == "cart-"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_membro_caixa_with_unicode() {
        // DNS-1123 is ASCII-only; IDN must be pre-encoded as Punycode
        // (`xn--…`) by the author before it reaches K8s. The byte-by-
        // byte ASCII validity check rejects multi-byte UTF-8 sequences
        // by the first byte that fails the `[a-z0-9-]` predicate.
        let mut s = three_member_spec();
        s.membros[2].caixa = "café".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::MembroCaixaInvalid { ref caixa, .. }
                    if caixa == "café"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_membro_caixa_with_whitespace() {
        // Whitespace is the canonical "I pasted from a sketch / doc"
        // footgun. The apiserver rejects every `metadata.name` value
        // carrying whitespace; pin the gate fires at the right boundary.
        let mut s = three_member_spec();
        s.membros[0].caixa = "my cart".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::MembroCaixaInvalid { ref caixa, .. }
                    if caixa == "my cart"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_membro_caixa_too_long() {
        // 64 bytes exceeds the DNS-1123 label cap by one — the boundary
        // pin. K8s Service name + DNS-1123 label both cap at 63 bytes
        // exactly. The gate's reason names both the cap and the actual
        // length so the author can shorten in one edit.
        let mut s = three_member_spec();
        let too_long = "a".repeat(64);
        s.membros[1].caixa = too_long.clone();
        let err = s.validate().unwrap_err();
        let AplicacaoError::MembroCaixaInvalid { caixa, reason } = err else {
            panic!("expected MembroCaixaInvalid");
        };
        assert_eq!(caixa, too_long);
        assert!(
            reason.contains("63") && reason.contains("64"),
            "diagnostic must name the cap (63) and the actual length (64): {reason:?}"
        );
    }

    #[test]
    fn membro_caixa_max_length_validates() {
        // 63 bytes exactly — the K8s DNS-1123 label cap. Pin the boundary
        // so a future tightening (e.g. dropping to 62) surfaces here as
        // a regression, mirroring `entrada_host_max_length_validates`
        // (c7d05ec).
        let mut s = three_member_spec();
        s.membros[2].caixa = "a".repeat(63);
        s.entrada.as_mut().unwrap().para = "a".repeat(63);
        // remove contratos referencing the renamed member; they'd
        // raise ContratoMemberMissing otherwise
        s.contratos
            .retain(|c| c.de != "payment" && c.para != "payment");
        s.validate().unwrap();
    }

    #[test]
    fn accepts_canonical_membro_caixa_forms() {
        // The DNS-1123 label shapes a caixa author is realistically
        // going to write: single-word lowercase, hyphen-joined, ending
        // in a digit-suffixed version (`cart-v2`), starting with a
        // digit (`3rd-party-shim` — DNS-1123 allows this, unlike
        // DNS-1035 which requires a letter at position 0), single-
        // character (`a` — boundary). Pin every leg so a future
        // tightening that bans (e.g.) digit-start identifiers surfaces
        // here.
        for form in [
            "checkout",
            "cart",
            "cart-v2",
            "a",
            "c0",
            "3rd-party-shim",
            "x-1-2-3-4",
        ] {
            let mut s = three_member_spec();
            // Renaming a member also requires updating downstream refs;
            // drop everything else and rebuild a minimal spec around
            // just the one renamed member.
            s.membros = vec![membro(form, "^0.1")];
            s.contratos = vec![];
            s.entrada = None;
            s.validate()
                .unwrap_or_else(|e| panic!("canonical form {form:?} must validate, got {e:?}"));
        }
    }

    #[test]
    fn membro_caixa_empty_takes_precedence_over_invalid() {
        // Order pin: the existing `MembroCaixaEmpty` diagnostic
        // (which doesn't try to parse) fires before the new
        // `MembroCaixaInvalid` parse-side diagnostic, so an empty
        // `:caixa` keeps its narrower error message — the new gate
        // would also reject `""`, but the empty-string arm is the more
        // self-locating diagnostic for the author. Mirrors the
        // `entrada_host_empty_takes_precedence_over_invalid` pin
        // (c7d05ec).
        let mut s = three_member_spec();
        s.membros[1].caixa = String::new();
        let err = s.validate().unwrap_err();
        assert_eq!(err, AplicacaoError::MembroCaixaEmpty);
    }

    #[test]
    fn membro_caixa_invalid_fires_before_versao_check() {
        // Order pin: an invalid-shape `:caixa` surfaces *its own*
        // diagnostic (which names the offending caixa name), even when
        // the same entry's `:versao` is also empty/invalid. The shape
        // gate runs first because the diagnostic is more self-locating —
        // an empty/invalid `:versao` on an invalid-shape caixa name is
        // a downstream-fix-after-the-caixa-rename concern.
        let mut s = three_member_spec();
        s.membros[1].caixa = "Cart".into();
        s.membros[1].versao = String::new(); // would otherwise raise MembroVersaoEmpty
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::MembroCaixaInvalid { ref caixa, .. } if caixa == "Cart"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn membro_caixa_invalid_fires_before_duplicate_check() {
        // Order pin: a malformed-shape `:caixa` on an earlier entry
        // surfaces *its own* diagnostic, even when a later entry would
        // otherwise collapse onto a duplicate name. The per-entry shape
        // gate runs inline before the duplicate-key insert, parallel
        // to `membro_versao_invalid_fires_before_duplicate_check`.
        let mut s = three_member_spec();
        s.membros[0].caixa = "Catalog".into();
        s.membros.push(membro("cart", "^0.2")); // would otherwise raise MembroDuplicate
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::MembroCaixaInvalid { ref caixa, .. } if caixa == "Catalog"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn membro_caixa_invalid_diagnostic_carries_offending_caixa() {
        // The diagnostic-shape pin: the error names the offending
        // `:caixa` value verbatim so the author can grep their
        // caixa.lisp without re-running the build, and carries a
        // non-empty `reason` naming the specific violation. Same
        // shape every typed-shape gate enshrines (c7d05ec's
        // `entrada_host_diagnostic_carries_offending_host`,
        // 9888b13's `membro_versao_invalid_diagnostic_carries_offending_versao`).
        let mut s = three_member_spec();
        s.membros[2].caixa = "BAD_NAME".into();
        let err = s.validate().unwrap_err();
        let AplicacaoError::MembroCaixaInvalid { caixa, reason } = err else {
            panic!("expected MembroCaixaInvalid");
        };
        assert_eq!(caixa, "BAD_NAME");
        assert!(
            !reason.is_empty(),
            "MembroCaixaInvalid `reason` must carry a parser-shaped wording"
        );
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

    // ── value-shape on WitTarget payload (endpoint / subject / slot) ──────

    #[test]
    fn rejects_http_contrato_with_empty_endpoint() {
        // `Some("")` for an HTTP endpoint passes the presence check
        // (target() previously returned WitTarget::Http { endpoint: "" })
        // but renders as a `path: ""` Cilium L7 rule that matches no
        // traffic. Same value-shape footgun closed for :entrada :paths
        // entries (eb3456d).
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "cart".into(),
            para: "catalog".into(),
            wit: "wasi:http/proxy".into(),
            endpoint: Some(String::new()),
            subject: None,
            slot: None,
        });
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::ContratoEndpointEmpty { ref de, ref para }
                if de == "cart" && para == "catalog"),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_http_contrato_with_relative_endpoint() {
        // Cilium L7 :path + Gateway API PathPrefix both require a
        // leading `/`. Same shape required of :entrada :paths
        // (eb3456d). Lifted into target() so every consumer of the
        // typed WitTarget view inherits the guarantee.
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "cart".into(),
            para: "catalog".into(),
            wit: "wasi:http/proxy".into(),
            endpoint: Some("products/:id".into()),
            subject: None,
            slot: None,
        });
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::ContratoEndpointNotAbsolute { ref endpoint, .. }
                if endpoint == "products/:id"),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_pubsub_contrato_with_empty_subject() {
        // NATS / Kafka publish without a subject is a no-op subscribe;
        // never the author's intent. Same empty-string rejection as
        // :membros :caixa, :placement :clusters entries, :entrada
        // :paths entries — every value carried by every typed slot is
        // value-shape-checked at validate().
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "cart".into(),
            para: "catalog".into(),
            wit: "nats:pub-sub".into(),
            endpoint: None,
            subject: Some(String::new()),
            slot: None,
        });
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::ContratoSubjectEmpty { ref de, ref para }
                if de == "cart" && para == "catalog"),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_store_contrato_with_empty_slot() {
        // An empty slot template addresses the bucket root, defeating
        // the per-key isolation the slot exists for — a footgun on
        // `wasi:keyvalue/store` whose closest analog is the empty
        // shard-key rejected on :placement Sharded (c7c7799).
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "cart".into(),
            para: "catalog".into(),
            wit: "wasi:keyvalue/store".into(),
            endpoint: None,
            subject: None,
            slot: Some(String::new()),
        });
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::ContratoSlotEmpty { ref de, ref para }
                if de == "cart" && para == "catalog"),
            "got {err:?}"
        );
    }

    #[test]
    fn http_contrato_root_endpoint_validates() {
        // Pin the boundary case: a single-`/` endpoint is the catch-all
        // form the Gateway HTTPRoute renderer falls back to when
        // :entrada :paths is empty (caixa-mesh::gateway_routes), so it
        // must remain a valid contrato endpoint too.
        let mut s = three_member_spec();
        s.contratos.push(contract_http("cart", "catalog", "/"));
        s.validate().unwrap();
    }

    // ── :contratos :endpoint value-shape gate ────────────────────────────
    //
    // Mirrors the `:entrada :paths` value-shape suite on the peer
    // HTTP-path axis. Until this gate landed `WitContract::target()`
    // only refused the empty string + the missing-leading-`/` form
    // (c4213a4); a structurally invalid endpoint passed validate and
    // landed verbatim as a Cilium L7 `path:` rule
    // (caixa-mesh/src/lib.rs:311) that either silently dropped all
    // traffic or was rejected at apply time by Cilium policy admission.
    // Every authoring footgun the K8s Gateway API webhook / Cilium
    // policy validator would catch on admission now becomes a caixa-
    // build-time `ContratoEndpointInvalid` with the offending
    // `:endpoint` + `:de` + `:para` named verbatim. Same diagnostic
    // shape as `EntradaPathInvalid` on the sibling axis; same shared
    // predicate (`crate::render::is_gateway_api_http_path`) ensures
    // drift between the two axes' rule enforcement is a build error
    // at the predicate.

    fn contrato_endpoint_err(ep: &str) -> AplicacaoError {
        // Fresh spec per call so the would-be-duplicate edge
        // `(cart, catalog, wasi:http/proxy, ep)` doesn't collide with
        // `three_member_spec`'s pre-existing
        // `(cart, catalog, …, /products/:id)` entry — only the
        // endpoint payload differs.
        let mut s = three_member_spec();
        s.contratos.push(contract_http("cart", "catalog", ep));
        s.validate().unwrap_err()
    }

    #[test]
    fn rejects_http_contrato_endpoint_with_query() {
        // Fail-before-pass-after pin — pre-gate the `?token=X` suffix
        // silently rendered as a Cilium L7 `path: "/charge?token=X"`
        // rule the L7 matcher would never satisfy.
        let err = contrato_endpoint_err("/charge?token=X");
        assert!(
            matches!(err, AplicacaoError::ContratoEndpointInvalid { ref endpoint, ref reason, .. }
                if endpoint == "/charge?token=X" && reason.contains("must not contain `?`")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_http_contrato_endpoint_with_fragment() {
        let err = contrato_endpoint_err("/charge#frag");
        assert!(
            matches!(err, AplicacaoError::ContratoEndpointInvalid { ref endpoint, ref reason, .. }
                if endpoint == "/charge#frag" && reason.contains("must not contain `#`")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_http_contrato_endpoint_with_whitespace() {
        let err = contrato_endpoint_err("/foo bar");
        assert!(
            matches!(err, AplicacaoError::ContratoEndpointInvalid { ref endpoint, ref reason, .. }
                if endpoint == "/foo bar" && reason.contains("whitespace")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_http_contrato_endpoint_with_control_char() {
        let err = contrato_endpoint_err("/api/\x01bar");
        assert!(
            matches!(err, AplicacaoError::ContratoEndpointInvalid { ref endpoint, ref reason, .. }
                if endpoint == "/api/\x01bar" && reason.contains("control character")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_http_contrato_endpoint_with_non_ascii() {
        let err = contrato_endpoint_err("/api/café");
        assert!(
            matches!(err, AplicacaoError::ContratoEndpointInvalid { ref endpoint, ref reason, .. }
                if endpoint == "/api/café" && reason.contains("non-ASCII")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_http_contrato_endpoint_with_consecutive_slashes() {
        let err = contrato_endpoint_err("/api//cart");
        assert!(
            matches!(err, AplicacaoError::ContratoEndpointInvalid { ref endpoint, ref reason, .. }
                if endpoint == "/api//cart" && reason.contains("consecutive `/`")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_http_contrato_endpoint_with_dot_segment() {
        let err = contrato_endpoint_err("/api/./cart");
        assert!(
            matches!(err, AplicacaoError::ContratoEndpointInvalid { ref endpoint, ref reason, .. }
                if endpoint == "/api/./cart" && reason.contains("`.` segment")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_http_contrato_endpoint_with_parent_segment() {
        // Path-traversal in a contrato endpoint is the canonical
        // "L7 rule that the workload's HTTP server's path-resolution
        // logic interprets differently than the policy enforcer"
        // footgun. Rejected outright at validate time.
        let err = contrato_endpoint_err("/api/../etc");
        assert!(
            matches!(err, AplicacaoError::ContratoEndpointInvalid { ref endpoint, ref reason, .. }
                if endpoint == "/api/../etc" && reason.contains("`..` parent-segment")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_http_contrato_endpoint_too_long() {
        // 1025-byte endpoint — one over the Gateway API
        // HTTPPathMatch.value `maxLength: 1024` cap. The Cilium L7
        // path matcher has no inherent length limit but the policy
        // CR itself rides through the K8s apiserver, which enforces
        // ConfigMap-shaped limits; sharing the Gateway API cap is the
        // conservative floor.
        let big = format!("/api/{}", "a".repeat(1020));
        assert_eq!(big.len(), 1025);
        let err = contrato_endpoint_err(&big);
        assert!(
            matches!(err, AplicacaoError::ContratoEndpointInvalid { ref endpoint, ref reason, .. }
                if endpoint == &big && reason.contains("max length of 1024")),
            "got {err:?}"
        );
    }

    #[test]
    fn http_contrato_endpoint_max_length_validates() {
        // 1024-byte endpoint — exactly the cap. Boundary pin: drift
        // in the cap surfaces here and at
        // `rejects_http_contrato_endpoint_too_long` simultaneously,
        // mirroring `entrada_path_max_length_validates` on the peer
        // axis.
        let big = format!("/api/{}", "a".repeat(1019));
        assert_eq!(big.len(), 1024);
        let mut s = three_member_spec();
        s.contratos.push(contract_http("cart", "catalog", &big));
        s.validate().unwrap();
    }

    #[test]
    fn http_contrato_endpoint_accepts_canonical_forms() {
        // Positive-set sweep: every canonical HTTP-path shape the
        // sibling `:entrada :paths` axis accepts (the bare-root `/`,
        // plain paths, hidden-file-style `.config` segments distinct
        // from the `.` segment, digit-bearing segments, the canonical
        // route-template `:param` form, trailing-slash form,
        // percent-encoded segments, the `/foo..bar` interior-`..`-
        // substring forms that are NOT `..` segments) must remain a
        // valid contrato endpoint too. Drift between this list and
        // the entrada path positive sweep surfaces at the shared
        // `is_gateway_api_http_path` substrate-side suite — one
        // source of truth. Uses a fresh `(payment, catalog)` edge so
        // none of the swept endpoints collide with the pre-existing
        // `(cart, catalog, /products/:id)` / `(cart, payment,
        // /charge)` entries in `three_member_spec`.
        for ep in [
            "/",
            "/charge",
            "/v1/charge",
            "/api/.config",
            "/products/:id",
            "/api/cart/",
            "/api/caf%C3%A9",
            "/foo..bar",
            "/...",
        ] {
            let mut s = three_member_spec();
            s.contratos.push(contract_http("payment", "catalog", ep));
            s.validate()
                .unwrap_or_else(|e| panic!("expected {ep:?} to validate, got {e:?}"));
        }
    }

    #[test]
    fn contrato_endpoint_empty_takes_precedence_over_invalid() {
        // Ordering pin: `ContratoEndpointEmpty` is the more self-
        // locating diagnostic on `""` and must lead — the value-
        // shape gate is only reached after the empty-check fires.
        // Mirrors `entrada_path_empty_takes_precedence_over_invalid`
        // on the peer axis.
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "cart".into(),
            para: "catalog".into(),
            wit: "wasi:http/proxy".into(),
            endpoint: Some(String::new()),
            subject: None,
            slot: None,
        });
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::ContratoEndpointEmpty { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn contrato_endpoint_not_absolute_takes_precedence_over_invalid() {
        // Ordering pin: an endpoint without a leading `/` surfaces the
        // narrower `ContratoEndpointNotAbsolute` diagnostic first; the
        // value-shape gate is only consulted on endpoints that already
        // satisfy the absolute-prefix invariant. Mirrors
        // `entrada_path_not_absolute_takes_precedence_over_invalid`.
        let err = contrato_endpoint_err("bad path");
        assert!(
            matches!(err, AplicacaoError::ContratoEndpointNotAbsolute { ref endpoint, .. }
                if endpoint == "bad path"),
            "got {err:?}"
        );
    }

    #[test]
    fn contrato_endpoint_invalid_diagnostic_carries_offending_endpoint() {
        // Diagnostic-shape pin — the offending `:endpoint` + `:de` +
        // `:para` + a non-empty reason flow through verbatim so the
        // author can grep their caixa.lisp for the offending contrato
        // block and fix it in one edit. Same shape as
        // `entrada_path_diagnostic_carries_offending_path`.
        let err = contrato_endpoint_err("/api?q=1");
        match err {
            AplicacaoError::ContratoEndpointInvalid {
                de,
                para,
                endpoint,
                reason,
            } => {
                assert_eq!(de, "cart");
                assert_eq!(para, "catalog");
                assert_eq!(endpoint, "/api?q=1");
                assert!(!reason.is_empty(), "reason field must be non-empty");
            }
            other => panic!("expected ContratoEndpointInvalid, got {other:?}"),
        }
    }

    #[test]
    fn target_view_payload_is_guaranteed_nonempty_after_target_call() {
        // The compounding theorem: every &str inside a WitTarget
        // returned by target() is non-empty (and absolute, for Http).
        // Renderers downstream of typed_view() can rely on this
        // without re-checking — the type system carries the proof.
        let http = contract_http("cart", "catalog", "/x");
        match http.target().unwrap() {
            WitTarget::Http { endpoint } => {
                assert!(!endpoint.is_empty());
                assert!(endpoint.starts_with('/'));
            }
            other => panic!("expected Http, got {other:?}"),
        }
        let nats = WitContract {
            de: "a".into(),
            para: "b".into(),
            wit: "nats:pub-sub".into(),
            endpoint: None,
            subject: Some("topic.x".into()),
            slot: None,
        };
        match nats.target().unwrap() {
            WitTarget::PubSub { subject } => assert!(!subject.is_empty()),
            other => panic!("expected PubSub, got {other:?}"),
        }
        let kv = WitContract {
            de: "a".into(),
            para: "b".into(),
            wit: "wasi:keyvalue/store".into(),
            endpoint: None,
            subject: None,
            slot: Some("checkout/$orderId".into()),
        };
        match kv.target().unwrap() {
            WitTarget::Store { slot } => assert!(!slot.is_empty()),
            other => panic!("expected Store, got {other:?}"),
        }
    }

    #[test]
    fn target_diagnostic_names_offending_endpoint_value() {
        // When the malformed endpoint string is non-trivial, the
        // diagnostic carries the actual value back to the author —
        // not a generic "endpoint malformed" error.
        let bad = WitContract {
            de: "src".into(),
            para: "dst".into(),
            wit: "wasi:http/proxy".into(),
            endpoint: Some("api/v1/charge".into()),
            subject: None,
            slot: None,
        };
        match bad.target().unwrap_err() {
            AplicacaoError::ContratoEndpointNotAbsolute { de, para, endpoint } => {
                assert_eq!(de, "src");
                assert_eq!(para, "dst");
                assert_eq!(endpoint, "api/v1/charge");
            }
            other => panic!("expected ContratoEndpointNotAbsolute, got {other:?}"),
        }
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

    // ── :contratos :wit value-shape gate ─────────────────────────────────
    //
    // Mirrors the `:contratos :endpoint` value-shape suite on the peer
    // dispatch-discriminator axis. Until this gate landed
    // `WitContract::target()` accepted any non-empty string and
    // silently demoted unrecognized shapes to a capability-only L4
    // edge — the canonical "I thought I had L7 HTTP routing, got
    // L4-only" footgun. Every authoring footgun the WIT registry's
    // own grammar rejects (uppercase, hyphen-for-colon typo,
    // whitespace, empty package, doubled `@`, …) now becomes a
    // caixa-build-time `ContratoWitInvalid` with the offending
    // `:wit` + `:de` + `:para` named verbatim. Same diagnostic shape
    // as `ContratoEndpointInvalid` on the sibling axis; same shared
    // predicate (`crate::render::is_wit_world_ref`) ensures drift
    // between any two axes' rule enforcement is a build error at the
    // predicate, not piecemeal across renderers.

    fn contrato_wit_err(wit: &str) -> AplicacaoError {
        // Fresh spec per call so the new contract doesn't collide on
        // identity with `three_member_spec`'s pre-existing entries.
        // The new edge uses `(payment, catalog)` — a pair the fixture
        // doesn't already declare — with no payload field set, so the
        // wit-shape gate fires before any payload-shape arm.
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "payment".into(),
            para: "catalog".into(),
            wit: wit.into(),
            endpoint: None,
            subject: None,
            slot: None,
        });
        s.validate().unwrap_err()
    }

    #[test]
    fn rejects_wit_with_uppercase_namespace() {
        // Fail-before-pass-after pin — pre-gate `:wit "WASI:http/proxy"`
        // didn't match the lowercase `wasi:http/` prefix is_http() keys
        // off, so the dispatch fell through to the capability arm and
        // the contract silently rendered as an L4-only Cilium edge.
        // The new gate surfaces the uppercase typo at validate time
        // with the offending `:wit` named.
        let err = contrato_wit_err("WASI:http/proxy");
        assert!(
            matches!(err, AplicacaoError::ContratoWitInvalid { ref wit, ref reason, .. }
                if wit == "WASI:http/proxy" && reason.contains("lowercase")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_wit_with_hyphen_for_colon_typo() {
        // The canonical "I forgot the `:` separator" typo — pre-gate
        // this passed as Capability silently, so the renderer emitted
        // an L4-only policy where the author expected L7 HTTP rules.
        let err = contrato_wit_err("wasi-http/proxy");
        assert!(
            matches!(err, AplicacaoError::ContratoWitInvalid { ref wit, ref reason, .. }
                if wit == "wasi-http/proxy" && reason.contains("must contain a `:`")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_wit_with_multiple_colons() {
        // Doubled `:` — the namespace/package split has nowhere to
        // anchor, so the dispatch silently demotes to Capability.
        let err = contrato_wit_err("wasi:http:proxy");
        assert!(
            matches!(err, AplicacaoError::ContratoWitInvalid { ref wit, ref reason, .. }
                if wit == "wasi:http:proxy" && reason.contains("exactly one `:`")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_wit_with_empty_package() {
        // `wasi:` — namespace alone with no package. Pre-gate this
        // failed neither the is_http nor is_pubsub nor is_store
        // prefix check (none of `wasi:http/`, `wasi:keyvalue/` match
        // a bare `wasi:`), so it silently demoted to Capability.
        let err = contrato_wit_err("wasi:");
        assert!(
            matches!(err, AplicacaoError::ContratoWitInvalid { ref wit, ref reason, .. }
                if wit == "wasi:" && reason.contains("package") && reason.contains("must not be empty")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_wit_with_underscore() {
        // Underscore — WIT identifiers are kebab-case, same rule
        // DNS-1123 enforces on its peer axes. The diagnostic carries
        // the explicit "use `-` instead" remediation.
        let err = contrato_wit_err("wasi:http_proxy");
        assert!(
            matches!(err, AplicacaoError::ContratoWitInvalid { ref wit, ref reason, .. }
                if wit == "wasi:http_proxy" && reason.contains('_')),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_wit_with_whitespace() {
        // Whitespace mid-token — the prefix check matches but the
        // package-and-onward parse silently demoted to Capability.
        let err = contrato_wit_err("wasi:http proxy");
        assert!(
            matches!(err, AplicacaoError::ContratoWitInvalid { ref wit, ref reason, .. }
                if wit == "wasi:http proxy" && reason.contains("whitespace")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_wit_with_non_ascii() {
        // Un-percent-encoded non-ASCII byte — the canonical "I copied
        // the package name from a doc with smart quotes / accented
        // characters" footgun.
        let err = contrato_wit_err("wasi:caf\u{e9}/proxy");
        assert!(
            matches!(err, AplicacaoError::ContratoWitInvalid { ref wit, ref reason, .. }
                if wit == "wasi:caf\u{e9}/proxy" && reason.contains("non-ASCII")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_wit_with_consecutive_hyphens() {
        // `pub--sub` — WIT identifiers join words with single hyphens.
        let err = contrato_wit_err("nats:pub--sub");
        assert!(
            matches!(err, AplicacaoError::ContratoWitInvalid { ref wit, ref reason, .. }
                if wit == "nats:pub--sub" && reason.contains("consecutive `-`")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_wit_with_trailing_at_no_version() {
        // `wasi:http/proxy@` — the version-suffix author started to
        // type `@0.2.0` and stopped, leaving a stray `@`. The WIT
        // parser would reject this; surface it at validate time.
        let err = contrato_wit_err("wasi:http/proxy@");
        assert!(
            matches!(err, AplicacaoError::ContratoWitInvalid { ref wit, ref reason, .. }
                if wit == "wasi:http/proxy@" && reason.contains("trailing `@`")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_wit_too_long() {
        // 129-byte WIT reference — one over the WIT_IDENT_MAX_LEN cap.
        // The legitimate-shape arms all pass (lowercase, single `:`,
        // kebab-case identifiers); only the cap arm fires. Surfaces
        // the paste-from-binary / accidental-multi-line-blob landing
        // footgun. Mirrors `rejects_http_contrato_endpoint_too_long`
        // on the peer axis.
        let big = format!("wasi:{}", "a".repeat(124));
        assert_eq!(big.len(), 129);
        let err = contrato_wit_err(&big);
        assert!(
            matches!(err, AplicacaoError::ContratoWitInvalid { ref wit, ref reason, .. }
                if wit == &big && reason.contains("max length of 128")),
            "got {err:?}"
        );
    }

    #[test]
    fn wit_max_length_validates() {
        // 128-byte WIT reference — exactly the cap. Boundary pin:
        // drift in the cap surfaces here and at `rejects_wit_too_long`
        // simultaneously, mirroring
        // `http_contrato_endpoint_max_length_validates` on the peer
        // axis.
        let big = format!("wasi:{}", "a".repeat(123));
        assert_eq!(big.len(), 128);
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "payment".into(),
            para: "catalog".into(),
            wit: big,
            endpoint: None,
            subject: None,
            slot: None,
        });
        s.validate().unwrap();
    }

    #[test]
    fn wit_accepts_canonical_forms_at_aplicacao_layer() {
        // Positive-set sweep through the AplicacaoSpec::validate
        // surface (rather than the substrate-side predicate directly)
        // — pins every shape the existing test fixtures + the
        // checkout-aplicacao example carry, so the gate's accept-set
        // matches the substrate's emit-set. Drift between this list
        // and `render::tests::wit_world_ref_accepts_canonical_forms`
        // surfaces at the substrate layer's positive sweep — one
        // source of truth for the rule.
        for wit in [
            "wasi:http/proxy",
            "wasi:keyvalue/store",
            "nats:pub-sub",
            "kafka:topic",
            "custom:exchange",
            "pleme:cap/audit",
            "wasi:http/proxy@0.2.0",
        ] {
            // Payload field paired to the dispatched WIT shape so the
            // shape-↔-target arm doesn't fire instead of the wit-shape
            // arm we're exercising.
            let (endpoint, subject, slot) =
                if wit.starts_with("wasi:http/") || wit.starts_with("http:") {
                    (Some("/x".into()), None, None)
                } else if wit.starts_with("nats:") || wit.starts_with("kafka:") {
                    (None, Some("topic.x".into()), None)
                } else if wit.starts_with("wasi:keyvalue/") || wit.starts_with("kv:") {
                    (None, None, Some("bucket/$key".into()))
                } else {
                    (None, None, None)
                };
            let mut s = three_member_spec();
            s.contratos.push(WitContract {
                de: "payment".into(),
                para: "catalog".into(),
                wit: wit.into(),
                endpoint,
                subject,
                slot,
            });
            s.validate()
                .unwrap_or_else(|e| panic!("canonical WIT {wit:?} must validate, got {e:?}"));
        }
    }

    #[test]
    fn empty_wit_takes_precedence_over_invalid() {
        // Ordering pin: `EmptyWit` is the more self-locating
        // diagnostic on `""` and must lead — the value-shape gate is
        // only reached after the empty-check fires. Mirrors
        // `contrato_endpoint_empty_takes_precedence_over_invalid` on
        // the peer payload axis.
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "payment".into(),
            para: "catalog".into(),
            wit: String::new(),
            endpoint: None,
            subject: None,
            slot: None,
        });
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EmptyWit { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn wit_invalid_fires_before_payload_shape_arm() {
        // Ordering pin: a malformed `:wit` surfaces *its own*
        // diagnostic (which names the offending wit verbatim) before
        // any payload-field check — a contrato whose wit is
        // structurally invalid AND carries a wrong target field
        // returns `ContratoWitInvalid`, not `ContratoWrongTarget`,
        // because the dispatch on the wit is what decides which
        // payload field is "right" in the first place. Without this
        // ordering, the author would see "wrong target field" for a
        // wit that hasn't even been parsed, which doesn't name the
        // root cause.
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "payment".into(),
            para: "catalog".into(),
            // Hyphen-for-colon typo + endpoint set: pre-gate this
            // raised `ContratoWrongTarget { expected: "none" }` (the
            // Capability arm rejecting the endpoint), masking the
            // real authoring mistake (the wit isn't `wasi:http/proxy`).
            wit: "wasi-http/proxy".into(),
            endpoint: Some("/x".into()),
            subject: None,
            slot: None,
        });
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::ContratoWitInvalid { ref wit, .. }
                if wit == "wasi-http/proxy"),
            "got {err:?}"
        );
    }

    #[test]
    fn wit_invalid_diagnostic_carries_offending_wit() {
        // Diagnostic-shape pin — the offending `:wit` + `:de` +
        // `:para` + a non-empty reason flow through verbatim so the
        // author can grep their caixa.lisp for the offending contrato
        // block and fix it in one edit. Same shape as
        // `contrato_endpoint_invalid_diagnostic_carries_offending_endpoint`.
        let err = contrato_wit_err("WASI:HTTP/proxy");
        match err {
            AplicacaoError::ContratoWitInvalid {
                de,
                para,
                wit,
                reason,
            } => {
                assert_eq!(de, "payment");
                assert_eq!(para, "catalog");
                assert_eq!(wit, "WASI:HTTP/proxy");
                assert!(!reason.is_empty(), "reason field must be non-empty");
            }
            other => panic!("expected ContratoWitInvalid, got {other:?}"),
        }
    }

    // ── :contratos :subject value-shape gate ─────────────────────────────
    //
    // Mirrors the `:contratos :endpoint` / `:contratos :wit` value-shape
    // suites on the peer payload axes. Until this gate landed
    // `WitContract::target()` only refused the empty string; a
    // structurally invalid subject silently passed validate and the
    // failure surfaced at runtime as a NATS server-side `-ERR 'Invalid
    // Subject'` on publish / subscribe, or as a silent message drop,
    // far from the source caixa.lisp. Every authoring footgun the
    // NATS server's subject parser would catch on admission now
    // becomes a caixa-build-time `ContratoSubjectInvalid` with the
    // offending `:subject` + `:de` + `:para` named verbatim. Same
    // diagnostic shape as `ContratoEndpointInvalid` /
    // `ContratoWitInvalid` on the peer payload axes; same shared
    // predicate (`crate::render::is_nats_subject`) ensures drift
    // between any two axes' rule enforcement is a build error at the
    // predicate, not piecemeal across renderers.

    fn contrato_subject_err(subject: &str) -> AplicacaoError {
        // Fresh spec per call so the new contract doesn't collide on
        // identity with `three_member_spec`'s pre-existing entries.
        // The new edge uses `(payment, catalog)` — a pair the fixture
        // doesn't already declare — with `:wit "nats:pub-sub"` and the
        // varying `:subject`, so the subject-shape gate fires cleanly
        // after the wit-shape gate (which `"nats:pub-sub"` passes).
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "payment".into(),
            para: "catalog".into(),
            wit: "nats:pub-sub".into(),
            endpoint: None,
            subject: Some(subject.into()),
            slot: None,
        });
        s.validate().unwrap_err()
    }

    #[test]
    fn rejects_pubsub_contrato_subject_with_whitespace() {
        // Fail-before-pass-after pin — pre-gate `"foo bar"` silently
        // landed at the NATS server as a malformed subject the parser
        // rejects with `-ERR 'Invalid Subject'`. Now caught at the
        // source caixa.lisp.
        let err = contrato_subject_err("foo bar");
        assert!(
            matches!(err, AplicacaoError::ContratoSubjectInvalid { ref subject, ref reason, .. }
                if subject == "foo bar" && reason.contains("whitespace")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_pubsub_contrato_subject_with_control_char() {
        let err = contrato_subject_err("foo\x01bar");
        assert!(
            matches!(err, AplicacaoError::ContratoSubjectInvalid { ref subject, ref reason, .. }
                if subject == "foo\x01bar" && reason.contains("control character")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_pubsub_contrato_subject_with_non_ascii() {
        // Un-percent-encoded non-ASCII byte — the canonical "I copied
        // the subject from a doc with smart quotes / accented
        // characters" footgun.
        let err = contrato_subject_err("foo.caf\u{e9}");
        assert!(
            matches!(err, AplicacaoError::ContratoSubjectInvalid { ref subject, ref reason, .. }
                if subject == "foo.caf\u{e9}" && reason.contains("non-ASCII")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_pubsub_contrato_subject_with_leading_dot() {
        // Empty leading token — NATS rejects.
        let err = contrato_subject_err(".foo");
        assert!(
            matches!(err, AplicacaoError::ContratoSubjectInvalid { ref subject, ref reason, .. }
                if subject == ".foo" && reason.contains("must not start with `.`")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_pubsub_contrato_subject_with_trailing_dot() {
        // Empty trailing token — NATS rejects. The remediation
        // (use `>` instead) is in the reason string.
        let err = contrato_subject_err("foo.");
        assert!(
            matches!(err, AplicacaoError::ContratoSubjectInvalid { ref subject, ref reason, .. }
                if subject == "foo." && reason.contains("must not end with `.`")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_pubsub_contrato_subject_with_consecutive_dots() {
        // The canonical "I forgot to fill in the middle segment"
        // typo — `"foo..bar"`. NATS rejects empty tokens.
        let err = contrato_subject_err("foo..bar");
        assert!(
            matches!(err, AplicacaoError::ContratoSubjectInvalid { ref subject, ref reason, .. }
                if subject == "foo..bar" && reason.contains("consecutive `.`")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_pubsub_contrato_subject_with_non_trailing_multi_wildcard() {
        // `foo.>.bar` — `>` is the multi-token wildcard, only allowed
        // as the final segment. Pre-gate this passed as a typed edge
        // and surfaced at runtime as a NATS subscribe rejection.
        let err = contrato_subject_err("foo.>.bar");
        assert!(
            matches!(err, AplicacaoError::ContratoSubjectInvalid { ref subject, ref reason, .. }
                if subject == "foo.>.bar" && reason.contains("only allowed as the final segment")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_pubsub_contrato_subject_with_mid_segment_star() {
        // `foo*.bar` — NATS wildcards are standalone tokens. The
        // remediation is in the reason string.
        let err = contrato_subject_err("foo*.bar");
        assert!(
            matches!(err, AplicacaoError::ContratoSubjectInvalid { ref subject, ref reason, .. }
                if subject == "foo*.bar" && reason.contains("`*` mid-segment")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_pubsub_contrato_subject_with_invalid_char() {
        // `foo,bar` — comma is not a valid NATS subject character.
        // Pinned separately from the wildcard arms so the invalid-
        // character diagnostic is in force.
        let err = contrato_subject_err("foo,bar");
        assert!(
            matches!(err, AplicacaoError::ContratoSubjectInvalid { ref subject, ref reason, .. }
                if subject == "foo,bar" && reason.contains("invalid character")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_pubsub_contrato_subject_too_long() {
        // 257-byte subject — one over the NATS_SUBJECT_MAX_LEN cap.
        // The legitimate-shape arms all pass (one all-`a` token, no
        // `.`, no wildcards); only the cap arm fires. Surfaces the
        // paste-from-binary / accidental-multi-line-blob landing
        // footgun. Mirrors `rejects_http_contrato_endpoint_too_long`
        // on the peer axis.
        let big = "a".repeat(257);
        assert_eq!(big.len(), 257);
        let err = contrato_subject_err(&big);
        assert!(
            matches!(err, AplicacaoError::ContratoSubjectInvalid { ref subject, ref reason, .. }
                if subject == &big && reason.contains("max length of 256")),
            "got {err:?}"
        );
    }

    #[test]
    fn pubsub_contrato_subject_max_length_validates() {
        // 256-byte subject — exactly the cap. Boundary pin: drift in
        // the cap surfaces here and at
        // `rejects_pubsub_contrato_subject_too_long` simultaneously,
        // mirroring `http_contrato_endpoint_max_length_validates` and
        // `wit_max_length_validates` on the peer axes.
        let big = "a".repeat(256);
        assert_eq!(big.len(), 256);
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "payment".into(),
            para: "catalog".into(),
            wit: "nats:pub-sub".into(),
            endpoint: None,
            subject: Some(big),
            slot: None,
        });
        s.validate().unwrap();
    }

    #[test]
    fn pubsub_contrato_subject_accepts_canonical_forms() {
        // Positive-set sweep: every canonical NATS subject shape the
        // substrate-side `is_nats_subject` predicate accepts (the
        // multi-dot `events.order.charged`, the snake_case / kebab-
        // case / mixed-case tokens, the digit-bearing tokens, the
        // single-token wildcard `*` at every segment position, and
        // the trailing `>` multi-token wildcard) must remain a valid
        // contrato subject too. Drift between this list and the
        // substrate-side `nats_subject_accepts_canonical_forms` sweep
        // surfaces at the shared predicate — one source of truth.
        // Uses a fresh `(payment, catalog)` edge so none of the swept
        // subjects collide with the pre-existing entries in
        // `three_member_spec`.
        for subject in [
            "checkout.events.charge.failed",
            "rio.events.order.charged",
            "orders",
            "orders.123",
            "snake_case.token",
            "kebab-case.token",
            "MixedCase.Token",
            "orders.*.charged",
            "*.events.*",
            "orders.>",
        ] {
            let mut s = three_member_spec();
            s.contratos.push(WitContract {
                de: "payment".into(),
                para: "catalog".into(),
                wit: "nats:pub-sub".into(),
                endpoint: None,
                subject: Some(subject.into()),
                slot: None,
            });
            s.validate()
                .unwrap_or_else(|e| panic!("expected {subject:?} to validate, got {e:?}"));
        }
    }

    #[test]
    fn contrato_subject_empty_takes_precedence_over_invalid() {
        // Ordering pin: `ContratoSubjectEmpty` is the more self-
        // locating diagnostic on `""` and must lead — the value-shape
        // gate is only reached after the empty-check fires. Mirrors
        // `contrato_endpoint_empty_takes_precedence_over_invalid` on
        // the peer payload axis.
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "payment".into(),
            para: "catalog".into(),
            wit: "nats:pub-sub".into(),
            endpoint: None,
            subject: Some(String::new()),
            slot: None,
        });
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::ContratoSubjectEmpty { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn contrato_subject_invalid_diagnostic_carries_offending_subject() {
        // Diagnostic-shape pin — the offending `:subject` + `:de` +
        // `:para` + a non-empty reason flow through verbatim so the
        // author can grep their caixa.lisp for the offending contrato
        // block and fix it in one edit. Same shape as
        // `contrato_endpoint_invalid_diagnostic_carries_offending_endpoint`
        // and `wit_invalid_diagnostic_carries_offending_wit`.
        let err = contrato_subject_err("foo..bar");
        match err {
            AplicacaoError::ContratoSubjectInvalid {
                de,
                para,
                subject,
                reason,
            } => {
                assert_eq!(de, "payment");
                assert_eq!(para, "catalog");
                assert_eq!(subject, "foo..bar");
                assert!(!reason.is_empty(), "reason field must be non-empty");
            }
            other => panic!("expected ContratoSubjectInvalid, got {other:?}"),
        }
    }

    #[test]
    fn target_view_pubsub_subject_passes_through_to_typed_view() {
        // The compounding theorem on the pub-sub axis: every
        // `WitTarget::PubSub { subject }` returned by `target()` carries
        // a NATS-server-accepted subject. Renderers downstream of
        // `typed_view()` (caixa-mesh's CNP L4 emitter, the future
        // NATS Stream/Consumer CR emitter, the future `feira app graph`
        // view's subject labeller) can rely on this without re-checking
        // — the type system carries the proof. Mirrors
        // `target_view_payload_is_guaranteed_nonempty_after_target_call`
        // on the peer axes.
        let nats = WitContract {
            de: "a".into(),
            para: "b".into(),
            wit: "nats:pub-sub".into(),
            endpoint: None,
            subject: Some("orders.events.*.charged".into()),
            slot: None,
        };
        match nats.target().unwrap() {
            WitTarget::PubSub { subject } => {
                assert_eq!(subject, "orders.events.*.charged");
            }
            other => panic!("expected PubSub, got {other:?}"),
        }
    }

    // ── :contratos :slot value-shape gate ────────────────────────────────
    //
    // Mirrors the `:contratos :endpoint` (4f0390b) + `:contratos :subject`
    // (63e18a0) value-shape suites on the peer payload axes. Until this
    // gate landed `WitContract::target()` only refused the empty string
    // for the Store arm; a structurally invalid slot (raw whitespace,
    // control character, non-ASCII byte, paste-from-binary multi-line
    // blob) silently passed validate and surfaced at runtime as a
    // per-backend kv write rejection or a silent next-read corruption,
    // far from the source caixa.lisp with no field naming which
    // `:contratos` edge carried the typo. Every authoring footgun the
    // kv backend intersection-floor would catch on write now becomes a
    // caixa-build-time `ContratoSlotInvalid` with the offending
    // `:slot` + `:de` + `:para` named verbatim. Same diagnostic shape
    // as `ContratoEndpointInvalid` / `ContratoSubjectInvalid` on the
    // peer payload axes; same shared predicate
    // (`crate::render::is_wasi_keyvalue_slot`) ensures drift between
    // any two axes' rule enforcement is a build error at the
    // predicate, not piecemeal across renderers. Closes the typed
    // payload-axis value-shape trajectory across all three legs of the
    // four `WitTarget` arms (HTTP / PubSub / Store / Capability).

    fn contrato_slot_err(slot: &str) -> AplicacaoError {
        // Fresh spec per call so the new contract doesn't collide on
        // identity with `three_member_spec`'s pre-existing entries
        // and doesn't close a synchronous cycle the cycle detector
        // would reject before the slot-shape gate fires. The new edge
        // uses `(payment, catalog)` — a pair the fixture doesn't
        // already declare in either direction (the fixture carries
        // `cart -> catalog` and `cart -> payment`, so `payment ->
        // catalog` doesn't form a cycle on the sync subgraph) — with
        // `:wit "wasi:keyvalue/store"` and the varying `:slot`, so the
        // slot-shape gate fires cleanly after the wit-shape gate
        // (which `"wasi:keyvalue/store"` passes). Same edge pair the
        // peer `contrato_subject_err` helper uses (63e18a0).
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "payment".into(),
            para: "catalog".into(),
            wit: "wasi:keyvalue/store".into(),
            endpoint: None,
            subject: None,
            slot: Some(slot.into()),
        });
        s.validate().unwrap_err()
    }

    #[test]
    fn rejects_store_contrato_slot_with_whitespace() {
        // Fail-before-pass-after pin — pre-gate `"check out/$order"`
        // silently landed at the kv backend with whitespace whose
        // runtime behavior varies unpredictably across backends (etcd
        // accepts, Redis accepts then breaks on next CLI op, DynamoDB
        // rejects on write). Now caught at the source caixa.lisp.
        let err = contrato_slot_err("check out/$order");
        assert!(
            matches!(err, AplicacaoError::ContratoSlotInvalid { ref slot, ref reason, .. }
                if slot == "check out/$order" && reason.contains("whitespace")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_store_contrato_slot_with_tab() {
        // Tab byte arm-pinned separately from the space arm so a
        // future relaxation that admits one but not the other surfaces
        // here.
        let err = contrato_slot_err("check\tout");
        assert!(
            matches!(err, AplicacaoError::ContratoSlotInvalid { ref slot, ref reason, .. }
                if slot == "check\tout" && reason.contains("whitespace")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_store_contrato_slot_with_control_char() {
        // SOH (0x01) — distinct from the whitespace arm. Redis admits
        // and corrupts on RESP protocol framing; DynamoDB rejects on
        // write.
        let err = contrato_slot_err("checkout/\x01order");
        assert!(
            matches!(err, AplicacaoError::ContratoSlotInvalid { ref slot, ref reason, .. }
                if slot == "checkout/\x01order" && reason.contains("control character")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_store_contrato_slot_with_newline() {
        // Embedded newline — the canonical "the paste-from-binary slug
        // spans multiple lines" footgun. Distinct from the whitespace
        // arm because `\n` is a control character (0x0A).
        let err = contrato_slot_err("checkout\norder");
        assert!(
            matches!(err, AplicacaoError::ContratoSlotInvalid { ref slot, ref reason, .. }
                if slot == "checkout\norder" && reason.contains("control character")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_store_contrato_slot_with_non_ascii() {
        // Un-percent-encoded non-ASCII byte — the canonical "I copied
        // the slot from a doc with accented characters" footgun. Each
        // kv backend re-encodes non-ASCII differently (etcd preserves
        // bytes verbatim; Redis-via-RESP3 may re-encode; DynamoDB
        // rejects), so the typed slot's value set is the intersection-
        // floor every backend admits identically (printable ASCII).
        let err = contrato_slot_err("ch\u{e9}ckout/$order");
        assert!(
            matches!(err, AplicacaoError::ContratoSlotInvalid { ref slot, ref reason, .. }
                if slot == "ch\u{e9}ckout/$order" && reason.contains("non-ASCII")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_store_contrato_slot_too_long() {
        // 513-byte slot — one over the WASI_KV_SLOT_MAX_LEN cap. The
        // legitimate-shape arms all pass (a single all-`a` token, no
        // separators); only the cap arm fires. Surfaces the paste-
        // from-binary / accidental-multi-line-blob landing footgun.
        // Mirrors `rejects_pubsub_contrato_subject_too_long` and
        // `rejects_http_contrato_endpoint_too_long` on the peer
        // payload axes.
        let big = "a".repeat(513);
        assert_eq!(big.len(), 513);
        let err = contrato_slot_err(&big);
        assert!(
            matches!(err, AplicacaoError::ContratoSlotInvalid { ref slot, ref reason, .. }
                if slot == &big && reason.contains("max length of 512")),
            "got {err:?}"
        );
    }

    #[test]
    fn store_contrato_slot_max_length_validates() {
        // 512-byte slot — exactly the cap. Boundary pin: drift in the
        // cap surfaces here and at `rejects_store_contrato_slot_too_long`
        // simultaneously, mirroring
        // `pubsub_contrato_subject_max_length_validates` and
        // `http_contrato_endpoint_max_length_validates` on the peer
        // payload axes.
        let big = "a".repeat(512);
        assert_eq!(big.len(), 512);
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "payment".into(),
            para: "catalog".into(),
            wit: "wasi:keyvalue/store".into(),
            endpoint: None,
            subject: None,
            slot: Some(big),
        });
        s.validate().unwrap();
    }

    #[test]
    fn store_contrato_slot_accepts_canonical_forms() {
        // Positive-set sweep: every canonical kv slot template the
        // substrate-side `is_wasi_keyvalue_slot` predicate accepts
        // (single-token identifiers, path-namespaced `$`-templates,
        // colon-namespaced `{}`-templates, dot-namespaced `<>`-templates,
        // snake_case / kebab-case / MixedCase tokens, digit-bearing
        // tokens, percent-encoded fragments) must remain valid
        // contrato slots too. Drift between this list and the
        // substrate-side `wasi_kv_slot_accepts_canonical_forms` sweep
        // surfaces at the shared predicate — one source of truth.
        // Uses a fresh `(payment, catalog)` edge so none of the swept
        // slots collide with the pre-existing entries in
        // `three_member_spec`.
        for slot in [
            "checkout",
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
            let mut s = three_member_spec();
            s.contratos.push(WitContract {
                de: "payment".into(),
                para: "catalog".into(),
                wit: "wasi:keyvalue/store".into(),
                endpoint: None,
                subject: None,
                slot: Some(slot.into()),
            });
            s.validate()
                .unwrap_or_else(|e| panic!("expected slot {slot:?} to validate, got {e:?}"));
        }
    }

    #[test]
    fn contrato_slot_empty_takes_precedence_over_invalid() {
        // Ordering pin: `ContratoSlotEmpty` is the more self-locating
        // diagnostic on `""` and must lead — the value-shape gate is
        // only reached after the empty-check fires. Mirrors
        // `contrato_subject_empty_takes_precedence_over_invalid` and
        // `contrato_endpoint_empty_takes_precedence_over_invalid` on
        // the peer payload axes.
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "payment".into(),
            para: "catalog".into(),
            wit: "wasi:keyvalue/store".into(),
            endpoint: None,
            subject: None,
            slot: Some(String::new()),
        });
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::ContratoSlotEmpty { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn contrato_slot_invalid_diagnostic_carries_offending_slot() {
        // Diagnostic-shape pin — the offending `:slot` + `:de` +
        // `:para` + a non-empty reason flow through verbatim so the
        // author can grep their caixa.lisp for the offending contrato
        // block and fix it in one edit. Same shape as
        // `contrato_subject_invalid_diagnostic_carries_offending_subject`
        // and `contrato_endpoint_invalid_diagnostic_carries_offending_endpoint`
        // on the peer payload axes.
        let err = contrato_slot_err("check out/$order");
        match err {
            AplicacaoError::ContratoSlotInvalid {
                de,
                para,
                slot,
                reason,
            } => {
                assert_eq!(de, "payment");
                assert_eq!(para, "catalog");
                assert_eq!(slot, "check out/$order");
                assert!(!reason.is_empty(), "reason field must be non-empty");
            }
            other => panic!("expected ContratoSlotInvalid, got {other:?}"),
        }
    }

    #[test]
    fn target_view_store_slot_passes_through_to_typed_view() {
        // The compounding theorem on the store axis: every
        // `WitTarget::Store { slot }` returned by `target()` carries a
        // kv-backend-accepted slot template. Renderers downstream of
        // `typed_view()` (the future per-Servico `:capabilities
        // wasi:keyvalue/store` axis emitter, the future `feira app
        // graph` view's slot labeller, the future kv-provider CR
        // materializer) can rely on this without re-checking — the
        // type system carries the proof. Mirrors
        // `target_view_pubsub_subject_passes_through_to_typed_view` on
        // the peer payload axis.
        let store = WitContract {
            de: "a".into(),
            para: "b".into(),
            wit: "wasi:keyvalue/store".into(),
            endpoint: None,
            subject: None,
            slot: Some("checkout/$orderId".into()),
        };
        match store.target().unwrap() {
            WitTarget::Store { slot } => {
                assert_eq!(slot, "checkout/$orderId");
            }
            other => panic!("expected Store, got {other:?}"),
        }
    }

    #[test]
    fn rejects_self_loop_in_synchronous_contratos() {
        let mut s = three_member_spec();
        s.contratos.push(contract_http("cart", "cart", "/loop"));
        let err = s.validate().unwrap_err();
        match err {
            AplicacaoError::ContratoCycle { cycle } => {
                assert_eq!(cycle, vec!["cart".to_string(), "cart".to_string()]);
            }
            other => panic!("expected ContratoCycle, got {other:?}"),
        }
    }

    #[test]
    fn rejects_two_node_synchronous_cycle() {
        let mut s = three_member_spec();
        // existing edges: cart → catalog, cart → payment
        // adding catalog → cart closes a 2-cycle on the HTTP subgraph
        s.contratos
            .push(contract_http("catalog", "cart", "/refresh"));
        let err = s.validate().unwrap_err();
        match err {
            AplicacaoError::ContratoCycle { cycle } => {
                // Cycle traversal should mention both endpoints, with
                // the back-edge target appearing as both first and last
                // element to close the loop.
                assert!(cycle.len() >= 3);
                assert_eq!(cycle.first(), cycle.last());
                let body: std::collections::HashSet<_> = cycle.iter().cloned().collect();
                assert!(body.contains("cart"));
                assert!(body.contains("catalog"));
            }
            other => panic!("expected ContratoCycle, got {other:?}"),
        }
    }

    #[test]
    fn rejects_three_node_synchronous_cycle() {
        let mut s = three_member_spec();
        // Reset to a clean 3-cycle: catalog → cart → payment → catalog
        s.contratos = vec![
            contract_http("catalog", "cart", "/x"),
            contract_http("cart", "payment", "/y"),
            contract_http("payment", "catalog", "/z"),
        ];
        let err = s.validate().unwrap_err();
        match err {
            AplicacaoError::ContratoCycle { cycle } => {
                assert_eq!(cycle.first(), cycle.last());
                let body: std::collections::HashSet<_> = cycle.iter().cloned().collect();
                assert_eq!(body.len(), 3);
                assert!(body.contains("cart"));
                assert!(body.contains("catalog"));
                assert!(body.contains("payment"));
            }
            other => panic!("expected ContratoCycle, got {other:?}"),
        }
    }

    #[test]
    fn pubsub_edge_breaks_cycle_per_mesh_composition_iii_3() {
        // MESH-COMPOSITION §III.3 explicitly says NATS pub-sub is
        // "acyclic by construction" — so a cycle whose closing edge
        // is pub-sub should NOT raise ContratoCycle.
        let mut s = three_member_spec();
        s.contratos = vec![
            contract_http("catalog", "cart", "/x"),
            contract_http("cart", "payment", "/y"),
            // Closing edge is pub-sub — async; not a sync deadlock.
            WitContract {
                de: "payment".into(),
                para: "catalog".into(),
                wit: "nats:pub-sub".into(),
                endpoint: None,
                subject: Some("checkout.events.charge.completed".into()),
                slot: None,
            },
        ];
        s.validate().expect("pub-sub edge breaks the sync cycle");
    }

    #[test]
    fn store_edge_counts_as_synchronous_for_cycle_detection() {
        // wasi:keyvalue/store is request/response; a cycle through one
        // *is* a sync deadlock, just like HTTP.
        let mut s = three_member_spec();
        s.contratos = vec![
            contract_http("catalog", "cart", "/x"),
            WitContract {
                de: "cart".into(),
                para: "catalog".into(),
                wit: "wasi:keyvalue/store".into(),
                endpoint: None,
                subject: None,
                slot: Some("session/$id".into()),
            },
        ];
        let err = s.validate().unwrap_err();
        assert!(matches!(err, AplicacaoError::ContratoCycle { .. }));
    }

    #[test]
    fn capability_edge_counts_as_synchronous_for_cycle_detection() {
        // Capability-only edges (unknown WIT shape, no payload) default
        // to synchronous — safer; authors with truly async capability
        // semantics can model them as pub-sub explicitly.
        let mut s = three_member_spec();
        s.contratos = vec![
            contract_http("catalog", "cart", "/x"),
            WitContract {
                de: "cart".into(),
                para: "catalog".into(),
                wit: "custom:exchange".into(),
                endpoint: None,
                subject: None,
                slot: None,
            },
        ];
        let err = s.validate().unwrap_err();
        assert!(matches!(err, AplicacaoError::ContratoCycle { .. }));
    }

    #[test]
    fn long_acyclic_chain_validates() {
        // A long sync chain (no back-edges) must validate even when
        // every node is reachable from the first.
        let mut s = three_member_spec();
        s.membros = vec![
            membro("a", "^0.1"),
            membro("b", "^0.1"),
            membro("c", "^0.1"),
            membro("d", "^0.1"),
            membro("e", "^0.1"),
        ];
        s.contratos = vec![
            contract_http("a", "b", "/1"),
            contract_http("b", "c", "/2"),
            contract_http("c", "d", "/3"),
            contract_http("d", "e", "/4"),
        ];
        s.entrada.as_mut().unwrap().para = "a".into();
        s.validate().unwrap();
    }

    #[test]
    fn diamond_acyclic_validates() {
        // a → b, a → c, b → d, c → d. Two paths to d, no cycle.
        let mut s = three_member_spec();
        s.membros = vec![
            membro("a", "^0.1"),
            membro("b", "^0.1"),
            membro("c", "^0.1"),
            membro("d", "^0.1"),
        ];
        s.contratos = vec![
            contract_http("a", "b", "/1"),
            contract_http("a", "c", "/2"),
            contract_http("b", "d", "/3"),
            contract_http("c", "d", "/4"),
        ];
        s.entrada.as_mut().unwrap().para = "a".into();
        s.validate().unwrap();
    }

    // ── duplicate-`:contratos` build-error gate ──────────────────────────

    #[test]
    fn rejects_duplicate_http_contrato() {
        // Fail-before-pass-after pin: the fixture's `cart → catalog`
        // HTTP edge appears once. Push an identical entry — same
        // (de, para, wit, endpoint) — and validate() must reject it.
        // Until this gate landed the typed surface accepted the
        // duplicate silently and caixa-mesh's `cilium_network_policies`
        // emitted two `CiliumNetworkPolicy` objects with identical
        // `metadata.name` (`<aplicacao>-<de>-to-<para>`), which K8s
        // admission rejects on `kubectl apply` far from the source.
        let mut s = three_member_spec();
        s.contratos
            .push(contract_http("cart", "catalog", "/products/:id"));
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ContratoDuplicate { ref de, ref para, ref wit, .. }
                    if de == "cart" && para == "catalog" && wit == "wasi:http/proxy"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_duplicate_pubsub_contrato() {
        // Same gate on the pub-sub edge axis. Two `nats:pub-sub`
        // edges with identical (de, para, subject) are degenerate;
        // pin that the typed surface refuses both at validate time.
        let mut s = three_member_spec();
        let pubsub = WitContract {
            de: "payment".into(),
            para: "cart".into(),
            wit: "nats:pub-sub".into(),
            endpoint: None,
            subject: Some("checkout.events.charge.failed".into()),
            slot: None,
        };
        s.contratos.push(pubsub.clone());
        s.contratos.push(pubsub);
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ContratoDuplicate { ref de, ref para, ref wit, .. }
                    if de == "payment" && para == "cart" && wit == "nats:pub-sub"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_duplicate_store_contrato() {
        // Same gate on the key-value edge axis. Two `wasi:keyvalue/store`
        // edges with identical (de, para, slot) collapse to one mesh-
        // policy edge; pin the build error.
        let mut s = three_member_spec();
        let store = WitContract {
            de: "cart".into(),
            para: "payment".into(),
            wit: "wasi:keyvalue/store".into(),
            endpoint: None,
            subject: None,
            slot: Some("checkout/$orderId".into()),
        };
        // Drop the conflicting HTTP `cart → payment` edge from the
        // fixture so the duplicate-store pair is the only one
        // distinguishable on this pair.
        s.contratos
            .retain(|c| !(c.de == "cart" && c.para == "payment"));
        s.contratos.push(store.clone());
        s.contratos.push(store);
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ContratoDuplicate { ref de, ref para, ref wit, .. }
                    if de == "cart" && para == "payment" && wit == "wasi:keyvalue/store"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_duplicate_capability_contrato() {
        // Same gate on the pure-capability axis (no payload selector).
        // Two contracts with identical (de, para, wit) and no
        // endpoint/subject/slot are duplicate edges; pin so a future
        // `target_label` change can't accidentally collapse the
        // capability arm into a None-shaped key that compares equal
        // to a populated one.
        let mut s = three_member_spec();
        let capability = WitContract {
            de: "cart".into(),
            para: "catalog".into(),
            wit: "pleme:cap/audit".into(),
            endpoint: None,
            subject: None,
            slot: None,
        };
        s.contratos.push(capability.clone());
        s.contratos.push(capability);
        let err = s.validate().unwrap_err();
        match err {
            AplicacaoError::ContratoDuplicate {
                de,
                para,
                wit,
                target,
            } => {
                assert_eq!(de, "cart");
                assert_eq!(para, "catalog");
                assert_eq!(wit, "pleme:cap/audit");
                assert!(
                    target.contains("capability"),
                    "capability-edge duplicate diagnostic must surface the \
                     no-payload shape (got target = {target:?})"
                );
            }
            other => panic!("expected ContratoDuplicate, got {other:?}"),
        }
    }

    #[test]
    fn accepts_distinct_http_paths_between_same_pair() {
        // Negative pin: two HTTP contracts cart → catalog at distinct
        // endpoints (`/products/:id` and `/search`) are *not*
        // duplicates — they're distinct typed edges differing on the
        // payload axis. The duplicate-gate must not over-match here,
        // since the cart-calls-catalog-on-multiple-paths shape is the
        // canonical multi-endpoint pattern (MESH-COMPOSITION §III.1
        // example: cart calls catalog at /products/:id, payment at
        // /charge — same shape extends to two paths on one para).
        let mut s = three_member_spec();
        s.contratos
            .push(contract_http("cart", "catalog", "/search"));
        s.validate()
            .expect("distinct endpoints between same (de, para) must validate");
    }

    #[test]
    fn accepts_same_endpoint_on_different_pairs() {
        // Negative pin: the same `/charge` endpoint reused on two
        // different (de, para) pairs is two distinct edges, not a
        // duplicate. Pinning this shape so the gate's identity key
        // includes both `de` and `para` (not just `(wit, endpoint)`).
        let mut s = three_member_spec();
        s.contratos
            .push(contract_http("payment", "catalog", "/charge"));
        s.validate()
            .expect("same endpoint reused on distinct (de, para) must validate");
    }

    #[test]
    fn rejects_duplicate_contrato_diagnostic_names_offending_target() {
        // Pin the diagnostic shape: the duplicate-edge error names
        // *which* target field carried the conflict, so the author
        // doesn't have to re-grep the source caixa.lisp to find it.
        // Same self-locating diagnostic discipline as
        // ContratoEndpointEmpty / ContratoSubjectEmpty / etc.
        let mut s = three_member_spec();
        s.contratos
            .push(contract_http("cart", "catalog", "/products/:id"));
        let err = s.validate().unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("\"/products/:id\""),
            "duplicate-contrato diagnostic must name the offending \
             :endpoint payload (got: {msg:?})"
        );
        assert!(
            msg.contains("cart") && msg.contains("catalog"),
            "diagnostic must name both endpoints of the duplicate edge \
             (got: {msg:?})"
        );
    }

    #[test]
    fn duplicate_contrato_gate_runs_after_membership_check() {
        // Order pin: a duplicate contract whose `:de` is *also* not in
        // `:membros` surfaces the membership error first — the
        // missing-member diagnostic is more locating than the
        // duplicate-edge one (the author has to fix the membership
        // before the duplicate is meaningful). Same ordering
        // discipline as `membros_validation_runs_before_contratos_membership_check`.
        let mut s = three_member_spec();
        s.contratos.push(contract_http("phantom", "catalog", "/x"));
        s.contratos.push(contract_http("phantom", "catalog", "/x"));
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::ContratoMemberMissing { ref caixa } if caixa == "phantom"),
            "membership-missing must fire before duplicate-edge (got {err:?})"
        );
    }

    #[test]
    fn duplicate_contrato_gate_runs_after_target_shape_check() {
        // Order pin: a contract with a malformed target (e.g. an HTTP
        // wit world with an empty :endpoint) surfaces the target-shape
        // error first, not the duplicate one. Even when two such
        // malformed entries are identical, the per-contract `target()`
        // check fires inside the loop *before* the duplicate-key
        // insert, so the diagnostic remains the most-locating one.
        let mut s = three_member_spec();
        let malformed = WitContract {
            de: "cart".into(),
            para: "catalog".into(),
            wit: "wasi:http/proxy".into(),
            endpoint: Some(String::new()),
            subject: None,
            slot: None,
        };
        s.contratos.push(malformed.clone());
        s.contratos.push(malformed);
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::ContratoEndpointEmpty { .. }),
            "endpoint-empty must fire before duplicate-edge (got {err:?})"
        );
    }

    #[test]
    fn rejects_entrada_path_without_leading_slash() {
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec!["/api/cart".into(), "api/products".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaPathNotAbsolute { ref path } if path == "api/products"),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_empty_entrada_path() {
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec!["/api/cart".into(), "".into()];
        assert_eq!(s.validate().unwrap_err(), AplicacaoError::EntradaPathEmpty);
    }

    #[test]
    fn rejects_duplicate_entrada_paths() {
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec![
            "/api/cart".into(),
            "/api/products".into(),
            "/api/cart".into(),
        ];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaPathDuplicate { ref path } if path == "/api/cart"),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_zero_entrada_port() {
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().port = 0;
        assert_eq!(s.validate().unwrap_err(), AplicacaoError::EntradaPortZero);
    }

    // ── :entrada :paths value-shape gate ─────────────────────────────
    //
    // Mirrors the `:entrada :host` value-shape suite (c7d05ec) on the
    // sibling `:paths` axis. Every authoring footgun the K8s Gateway
    // API v1 apiserver / webhook would catch on `HTTPRoute.spec.rules[]
    // .matches[].path.value` (caixa-mesh/src/lib.rs:498) at admission
    // time now becomes a caixa-build-time `EntradaPathInvalid` with
    // the offending `:paths` entry named verbatim.

    #[test]
    fn rejects_entrada_path_with_query() {
        // Fail-before-pass-after pin — pre-gate the `?q=1` suffix
        // silently passed validate and the Gateway API webhook
        // rejected it at apply time with no source citation.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec!["/api/cart?q=1".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaPathInvalid { ref path, ref reason }
                if path == "/api/cart?q=1" && reason.contains("must not contain `?`")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_path_with_fragment() {
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec!["/api/cart#frag".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaPathInvalid { ref path, ref reason }
                if path == "/api/cart#frag" && reason.contains("must not contain `#`")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_path_with_space() {
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec!["/api/my cart".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaPathInvalid { ref path, ref reason }
                if path == "/api/my cart" && reason.contains("whitespace")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_path_with_tab() {
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec!["/api/\tcart".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaPathInvalid { ref path, ref reason }
                if path == "/api/\tcart" && reason.contains("whitespace")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_path_with_control_char() {
        // 0x01 (SOH) — a non-whitespace control char surfaces the
        // distinct "control character" reason arm, separate from
        // the whitespace arm. Pinned so a future refactor that
        // collapses the two arms can't accidentally drop the more
        // self-locating diagnostic.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec!["/api/\x01cart".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaPathInvalid { ref path, ref reason }
                if path == "/api/\x01cart" && reason.contains("control character")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_path_with_non_ascii() {
        // `café` — the un-percent-encoded UTF-8 footgun the RFC 3986
        // unreserved-set rule rejects. The Gateway API webhook
        // rejects literal non-ASCII bytes; percent-encoding is the
        // only way to author non-ASCII in a path.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec!["/api/café".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaPathInvalid { ref path, ref reason }
                if path == "/api/café" && reason.contains("non-ASCII")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_path_with_consecutive_slashes() {
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec!["/api//cart".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaPathInvalid { ref path, ref reason }
                if path == "/api//cart" && reason.contains("consecutive `/`")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_path_with_dot_segment() {
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec!["/api/./cart".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaPathInvalid { ref path, ref reason }
                if path == "/api/./cart" && reason.contains("`.` segment")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_path_with_trailing_dot_segment() {
        // The bare `/.` and the trailing `/foo/.` are both rejected
        // by the Gateway API webhook; pinned separately so a future
        // narrowing that catches only the inner form surfaces here.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec!["/api/.".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaPathInvalid { ref path, ref reason }
                if path == "/api/." && reason.contains("`.` segment")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_path_with_parent_segment() {
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec!["/api/../etc".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaPathInvalid { ref path, ref reason }
                if path == "/api/../etc" && reason.contains("`..` parent-segment")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_path_with_trailing_parent_segment() {
        // Trailing `/..` — symmetric arm of the parent-segment rule,
        // pinned separately so a future relaxation that only checks
        // the inner form (`/../`) surfaces here.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec!["/api/..".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaPathInvalid { ref path, ref reason }
                if path == "/api/.." && reason.contains("`..` parent-segment")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_path_too_long() {
        // 1025-byte path — one over the Gateway API HTTPPathMatch.value
        // maxLength cap of 1024. Use a `/api/` prefix + a 1020-byte
        // ASCII-alphanumeric body so only the length rule fires.
        let mut s = three_member_spec();
        let big = format!("/api/{}", "a".repeat(1020));
        assert_eq!(big.len(), 1025);
        s.entrada.as_mut().unwrap().paths = vec![big.clone()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaPathInvalid { ref path, ref reason }
                if path == &big && reason.contains("max length of 1024")),
            "got {err:?}"
        );
    }

    #[test]
    fn entrada_path_max_length_validates() {
        // 1024-byte path — exactly the Gateway API HTTPPathMatch.value
        // maxLength cap. Boundary pin: drift in the cap surfaces here
        // and at `rejects_entrada_path_too_long` simultaneously.
        let mut s = three_member_spec();
        let big = format!("/api/{}", "a".repeat(1019));
        assert_eq!(big.len(), 1024);
        s.entrada.as_mut().unwrap().paths = vec![big];
        s.validate().unwrap();
    }

    #[test]
    fn entrada_accepts_canonical_paths() {
        // Positive-control sweep — every form the Gateway API
        // apiserver accepts must round-trip through validate. Covers
        // the root catch-all, plain paths, dot-prefixed segments
        // (hidden-file-style, distinct from `.` and `..` segments
        // which are rejected), digit-bearing segments, the canonical
        // route-template `:param` form (`:` is RFC 3986 reserved-set
        // valid in paths), trailing-slash form, percent-encoded
        // segments, and an interior `..` *substring* (`/foo..bar` is
        // not the `..` segment and is allowed).
        for path in [
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
        ] {
            let mut s = three_member_spec();
            s.entrada.as_mut().unwrap().paths = vec![path.into()];
            s.validate()
                .unwrap_or_else(|e| panic!("expected {path:?} to validate, got {e:?}"));
        }
    }

    #[test]
    fn entrada_path_empty_takes_precedence_over_invalid() {
        // Ordering pin: `EntradaPathEmpty` is the more self-locating
        // diagnostic on `""` and must lead — `validate_entrada_path`
        // is only reached after the empty-check fires at the call
        // site. (The predicate itself defends against direct
        // invocation by returning the same error on `""`.)
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec!["".into()];
        assert_eq!(s.validate().unwrap_err(), AplicacaoError::EntradaPathEmpty);
    }

    #[test]
    fn entrada_path_not_absolute_takes_precedence_over_invalid() {
        // Ordering pin: a path without a leading `/` surfaces the
        // narrower `EntradaPathNotAbsolute` diagnostic first; the
        // value-shape gate is only consulted on paths that already
        // satisfy the absolute-prefix invariant.
        let mut s = three_member_spec();
        // `bad path` would fire the whitespace rule under the
        // value-shape gate, but missing-leading-`/` is the more
        // self-locating diagnostic.
        s.entrada.as_mut().unwrap().paths = vec!["bad path".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaPathNotAbsolute { ref path } if path == "bad path"),
            "got {err:?}"
        );
    }

    #[test]
    fn entrada_path_invalid_fires_before_duplicate_check() {
        // Ordering pin: a malformed path on the *first* entry of a
        // would-be duplicate pair fires the value-shape gate before
        // the duplicate gate, mirroring the
        // `placement_cluster_invalid_fires_before_duplicate_check`
        // (6cbb900) pattern on the peer axis.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec!["/api?q".into(), "/api?q".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaPathInvalid { ref path, .. } if path == "/api?q"),
            "got {err:?}"
        );
    }

    #[test]
    fn entrada_path_diagnostic_carries_offending_path() {
        // Diagnostic-shape pin — the offending path + a non-empty
        // reason flow through verbatim so the author can grep their
        // caixa.lisp for `:paths` and fix it in one edit. Same shape
        // as `entrada_host_diagnostic_carries_offending_host` (c7d05ec).
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec!["/api?q=1".into()];
        let err = s.validate().unwrap_err();
        match err {
            AplicacaoError::EntradaPathInvalid { path, reason } => {
                assert_eq!(path, "/api?q=1");
                assert!(!reason.is_empty(), "reason field must be non-empty");
            }
            other => panic!("expected EntradaPathInvalid, got {other:?}"),
        }
    }

    // ── :entrada :host value-shape gate ──────────────────────────────
    //
    // Mirrors the `:entrada :paths` value-shape suite (eb3456d) on
    // the sibling `:host` axis. Every authoring footgun the K8s
    // Gateway API v1 apiserver would catch at admission time becomes
    // a caixa-build-time `EntradaHostInvalid` with the offending
    // `:host` named verbatim. Same diagnostic shape as
    // `MembroVersaoInvalid` (9888b13).

    #[test]
    fn rejects_entrada_host_with_scheme() {
        // Fail-before-pass-after pin — pre-gate codebases silently
        // accepted `https://…` and the apiserver rejected it at apply
        // time with no source citation.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "https://checkout.quero.cloud".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref host, .. }
                if host == "https://checkout.quero.cloud"),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_with_port() {
        // The `:8080` port suffix is the canonical "I forgot the port
        // belongs in `:entrada :port`" footgun.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "checkout.quero.cloud:8080".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref host, .. }
                if host == "checkout.quero.cloud:8080"),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_with_path() {
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "checkout.quero.cloud/api".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref host, .. }
                if host == "checkout.quero.cloud/api"),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_with_uppercase() {
        // Gateway API regex is `[a-z0-9]…` strictly — uppercase is
        // rejected, not silently lower-cased.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "Checkout.quero.cloud".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref reason, .. }
                if reason.contains("uppercase")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_with_underscore() {
        // RFC 1123 allows `[a-z0-9-]` only; underscore is the
        // canonical "I'm thinking of HTTP cookies / SRV records" leak.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "checkout_app.quero.cloud".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref reason, .. }
                if reason.contains('_')),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_ipv4_literal() {
        // Gateway API v1 explicitly forbids IP literals as Hostnames.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "10.0.0.1".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref reason, .. }
                if reason.contains("IPv4")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_with_trailing_dot() {
        // The Gateway API regex anchors at end-of-string with no
        // trailing `.` allowance — the FQDN root-dot form is rejected.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "checkout.quero.cloud.".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref host, .. }
                if host == "checkout.quero.cloud."),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_with_leading_dot() {
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = ".checkout.quero.cloud".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref reason, .. }
                if reason.contains("empty label")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_with_consecutive_dots() {
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "checkout..quero.cloud".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref reason, .. }
                if reason.contains("empty label")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_with_leading_hyphen_label() {
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "-checkout.quero.cloud".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref reason, .. }
                if reason.contains("alphanumeric")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_with_trailing_hyphen_label() {
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "checkout-.quero.cloud".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref reason, .. }
                if reason.contains("alphanumeric")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_with_inner_wildcard() {
        // Gateway API allows `*` only as the first label (`*.foo`);
        // any inner or trailing `*` is rejected.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "checkout.*.quero.cloud".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref reason, .. }
                if reason.contains("wildcard")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_bare_wildcard() {
        // `*.` with no domain is meaningless; Gateway API rejects it.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "*.".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref reason, .. }
                if reason.contains("wildcard")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_with_whitespace() {
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "checkout .quero.cloud".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref reason, .. }
                if reason.contains("whitespace")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_too_long() {
        // Total length cap = 253; build a 254-byte host out of two
        // 63-byte labels + one 62-byte label + dots.
        let mut s = three_member_spec();
        let big = format!(
            "{}.{}.{}.{}",
            "a".repeat(63),
            "b".repeat(63),
            "c".repeat(63),
            "d".repeat(254 - 63 * 3 - 3)
        );
        assert_eq!(big.len(), 254);
        s.entrada.as_mut().unwrap().host = big;
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref reason, .. }
                if reason.contains("max length of 253")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_label_too_long() {
        let mut s = three_member_spec();
        // 64-byte label — one over the per-label cap.
        s.entrada.as_mut().unwrap().host = format!("{}.quero.cloud", "x".repeat(64));
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref reason, .. }
                if reason.contains("label max length of 63")),
            "got {err:?}"
        );
    }

    #[test]
    fn entrada_host_diagnostic_carries_offending_host() {
        // Diagnostic-shape pin — the offending host + a non-empty
        // reason flow through verbatim so the author can grep their
        // caixa.lisp for `:host "<host>"` and fix it in one edit.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "checkout.quero.cloud:8080".into();
        let err = s.validate().unwrap_err();
        match err {
            AplicacaoError::EntradaHostInvalid { host, reason } => {
                assert_eq!(host, "checkout.quero.cloud:8080");
                assert!(!reason.is_empty(), "reason field must be non-empty");
            }
            other => panic!("expected EntradaHostInvalid, got {other:?}"),
        }
    }

    #[test]
    fn entrada_host_empty_takes_precedence_over_invalid() {
        // Ordering pin: `EmptyEntradaHost` is the more self-locating
        // diagnostic on `""` and must lead — `validate_entrada_host`
        // is only reached after the empty-check fires at the call
        // site. (The predicate itself defends against direct
        // invocation by returning the same error on `""`.)
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = String::new();
        assert_eq!(s.validate().unwrap_err(), AplicacaoError::EmptyEntradaHost);
    }

    #[test]
    fn entrada_host_member_missing_takes_precedence_over_host_invalid() {
        // Ordering pin: a missing :para member is the more
        // self-locating diagnostic and fires before the host gate.
        let mut s = three_member_spec();
        let e = s.entrada.as_mut().unwrap();
        e.para = "ghost".into();
        e.host = "BAD HOST".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaMemberMissing { ref para } if para == "ghost"),
            "got {err:?}"
        );
    }

    #[test]
    fn entrada_host_invalid_fires_before_port_zero() {
        // Ordering pin: the host gate fires before the port gate so
        // a malformed host is named even when the port is also wrong.
        let mut s = three_member_spec();
        let e = s.entrada.as_mut().unwrap();
        e.host = "Checkout.quero.cloud".into();
        e.port = 0;
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref host, .. }
                if host == "Checkout.quero.cloud"),
            "got {err:?}"
        );
    }

    #[test]
    fn entrada_accepts_canonical_hosts() {
        // Positive-control sweep — every form the Gateway API
        // apiserver accepts must round-trip through validate. Covers
        // a plain DNS subdomain, a leading wildcard, a single-label
        // host (cluster-internal), a max-length-edge label, a
        // hyphen-bearing label, and a Punycode IDN label.
        for host in [
            "checkout.quero.cloud",
            "*.quero.cloud",
            "checkout",
            // 63-byte label — exactly the per-label cap.
            "abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0.quero.cloud",
            "foo-bar.quero.cloud",
            // Punycode IDN — valid because the author pre-encoded.
            "xn--bcher-kva.example.com",
        ] {
            let mut s = three_member_spec();
            s.entrada.as_mut().unwrap().host = host.into();
            s.validate()
                .unwrap_or_else(|e| panic!("expected {host:?} to validate, got {e:?}"));
        }
    }

    #[test]
    fn entrada_host_max_length_validates() {
        // 253-byte host is the cap exactly — must validate. Build a
        // 253-byte host out of three 63-byte labels + one 61-byte
        // label + 3 dots = 252 bytes, then pad one byte to 253.
        let mut s = three_member_spec();
        let host = format!(
            "{}.{}.{}.{}",
            "a".repeat(63),
            "b".repeat(63),
            "c".repeat(63),
            "d".repeat(253 - 63 * 3 - 3)
        );
        assert_eq!(host.len(), 253);
        s.entrada.as_mut().unwrap().host = host;
        s.validate().unwrap();
    }

    #[test]
    fn entrada_with_empty_paths_validates() {
        // Empty `:paths` is the documented "match every path" form;
        // caixa-mesh's gateway_routes synthesizes a `/` catch-all.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec![];
        s.validate().unwrap();
    }

    #[test]
    fn entrada_root_path_validates() {
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec!["/".into()];
        s.validate().unwrap();
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

    #[test]
    fn rejects_zero_policy_timeout() {
        let mut s = three_member_spec();
        s.politicas.timeout = Some(Duration::ZERO);
        assert_eq!(s.validate().unwrap_err(), AplicacaoError::PolicyTimeoutZero);
    }

    #[test]
    fn rejects_zero_policy_retries() {
        let mut s = three_member_spec();
        s.politicas.retries = Some(0);
        assert_eq!(s.validate().unwrap_err(), AplicacaoError::PolicyRetriesZero);
    }

    #[test]
    fn rejects_circuit_breaker_zero_max_failures() {
        let mut s = three_member_spec();
        s.politicas.circuit_breaker = Some(CircuitBreaker {
            max_failures: 0,
            window: Duration::from_secs(60),
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyBreakerZeroFailures
        );
    }

    #[test]
    fn rejects_circuit_breaker_zero_window() {
        let mut s = three_member_spec();
        s.politicas.circuit_breaker = Some(CircuitBreaker {
            max_failures: 5,
            window: Duration::ZERO,
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyBreakerZeroWindow
        );
    }

    #[test]
    fn rejects_zero_rate_limit() {
        let mut s = three_member_spec();
        s.politicas.rate_limit = Some(RateLimit {
            rate: 0,
            window: Duration::from_secs(1),
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyRateLimitZero
        );
    }

    #[test]
    fn rejects_rate_limit_zero_window() {
        // `RateLimit { rate: 100, window: Duration::ZERO }` is
        // constructible programmatically (the typed `Duration` field
        // imposes no nonzero invariant) but renders through
        // `rate_limit_codec::render` as `"100/0s"` — a fragment the
        // codec's `parse` rejects as `unknown rate-limit window unit
        // "0s"`. Until this validate-time gate landed the typed slot
        // accepted the value silently and the round-trip break only
        // surfaced at deserialize time (potentially in a downstream
        // consumer that never re-validates). Pin the rejection at
        // `AplicacaoSpec::validate` so the typed slot's valid set
        // matches the codec's round-trippable set structurally.
        let mut s = three_member_spec();
        s.politicas.rate_limit = Some(RateLimit {
            rate: 100,
            window: Duration::ZERO,
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyRateLimitWindowNotCanonical {
                window: Duration::ZERO
            }
        );
    }

    #[test]
    fn rejects_rate_limit_arbitrary_seconds_window() {
        // 45 seconds is a valid `Duration` but not one of the three
        // canonical rate-limit windows the codec round-trips
        // (1s / 60s / 3600s). Renders as `"100/45s"`, which the parser
        // refuses on round-trip — same round-trip-break shape the
        // zero-window arm above pins, with a non-zero magnitude to
        // guard against a future "reject only zero" half-measure.
        let mut s = three_member_spec();
        let window = Duration::from_secs(45);
        s.politicas.rate_limit = Some(RateLimit { rate: 100, window });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyRateLimitWindowNotCanonical { window }
        );
    }

    #[test]
    fn rejects_rate_limit_two_minute_window() {
        // 120 seconds = 2 minutes is a "looks-canonical" but
        // not-canonical window: it's a clean integer multiple of the
        // minute unit, but the codec only round-trips the
        // unit-magnitude-1 forms (`"<n>/m"` ≡ 60s, *not* `"<n>/2m"`).
        // A `Duration::from_secs(120)` window renders as `"100/120s"`
        // which the parser rejects. Pinning this case rules out a
        // future "accept any clean multiple of s/m/h" relaxation
        // that would silently break the codec contract.
        let mut s = three_member_spec();
        let window = Duration::from_secs(120);
        s.politicas.rate_limit = Some(RateLimit { rate: 50, window });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyRateLimitWindowNotCanonical { window }
        );
    }

    #[test]
    fn rejects_rate_limit_subsecond_window() {
        // A sub-second window (e.g. 500ms) is a valid `Duration` but
        // unrepresentable in the codec's `<n>/<s|m|h>` author surface.
        // Pin the rejection so a future relaxation can't silently
        // admit fractional-second windows that the codec can't
        // round-trip.
        let mut s = three_member_spec();
        let window = Duration::from_millis(500);
        s.politicas.rate_limit = Some(RateLimit { rate: 200, window });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyRateLimitWindowNotCanonical { window }
        );
    }

    #[test]
    fn rate_limit_zero_rate_takes_precedence_over_non_canonical_window() {
        // Both axes are invalid here: rate == 0 *and* window is
        // non-canonical. The validate gate must fire on rate first
        // (matching the existing `rejects_zero_rate_limit` ordering),
        // so the existing diagnostic continues to lead with the
        // simpler "zero rate" framing. Pinning the order of checks
        // so a future refactor that reorders the arms surfaces here
        // as a test failure rather than a silent diagnostic
        // regression.
        let mut s = three_member_spec();
        s.politicas.rate_limit = Some(RateLimit {
            rate: 0,
            window: Duration::from_secs(45),
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyRateLimitZero
        );
    }

    #[test]
    fn rate_limit_canonical_windows_validate() {
        // The three canonical windows the codec round-trips
        // losslessly — 1s / 60s / 3600s — must all pass `validate()`
        // unchanged. Pin the full canonical set as a positive case
        // (the existing `rate_limit_round_trip_seconds` /
        // `rate_limit_round_trip_minutes` tests pin the
        // serialize-then-deserialize property at the codec layer; this
        // test pins the validate-side complement so a future tightening
        // of the canonical set — e.g. dropping `:hour` — surfaces here
        // as a test failure rather than a silent contract narrowing).
        for secs in [1u64, 60, 3600] {
            let mut s = three_member_spec();
            s.politicas.rate_limit = Some(RateLimit {
                rate: 100,
                window: Duration::from_secs(secs),
            });
            s.validate().expect("canonical window must validate");
        }
    }

    #[test]
    fn rate_limit_validated_value_round_trips_through_codec() {
        // The structural property the validate gate enforces:
        // every `RateLimit` past `AplicacaoSpec::validate` round-trips
        // losslessly through the `rate_limit_codec` (serialize → string
        // → deserialize → equal value). Pin this end-to-end so a future
        // change to either side (the validate gate's accepted window
        // set, the codec's parse/render unit set) that breaks the
        // alignment surfaces here. The previous-state shape (typed
        // slot accepts arbitrary `Duration`, codec only round-trips
        // 1s/60s/3600s) would fail this test for a `Duration::from_secs(45)`
        // window — the validate gate now forecloses that.
        for secs in [1u64, 60, 3600] {
            let mut s = three_member_spec();
            s.politicas.rate_limit = Some(RateLimit {
                rate: 250,
                window: Duration::from_secs(secs),
            });
            s.validate().unwrap();
            let json = serde_json::to_string(&s.politicas).unwrap();
            let back: MeshPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(
                back.rate_limit, s.politicas.rate_limit,
                "every validated :rate-limit must round-trip losslessly through the codec"
            );
        }
    }

    #[test]
    fn rate_limit_canonical_per_hour_renders_with_h_suffix() {
        // The hour-window canonical form (`"<n>/h"`) was missing from
        // the prior `rate_limit_round_trip_seconds` / `_minutes` test
        // pair. Now that the validate gate pins 3600s as part of the
        // canonical set, pin its serialize-side render shape too so
        // the third leg of the s/m/h tripod is explicitly tested.
        let policy = MeshPolicy {
            rate_limit: Some(RateLimit {
                rate: 10000,
                window: Duration::from_secs(3600),
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&policy).unwrap();
        assert!(
            json.contains("\"10000/h\""),
            "hour-window canonical form must render with `h` suffix (got: {json})"
        );
        let back: MeshPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(back.rate_limit.unwrap().window, Duration::from_secs(3600));
    }

    #[test]
    fn is_canonical_rate_limit_window_predicate_tracks_codec() {
        // Pin the predicate's accepted set against the codec's
        // accepted set explicitly. A future addition to the codec
        // (e.g. accepting `:day`/`:week` as authoring units) must be
        // accompanied by a parallel addition here, and a regression
        // that drops one of the three canonical units from either
        // side surfaces as a test failure. The predicate is the
        // single source of truth for the canonical-window set; this
        // test enshrines that the codec's parse arms and the
        // predicate's accept arms agree exactly.
        assert!(super::is_canonical_rate_limit_window(Duration::from_secs(
            1
        )));
        assert!(super::is_canonical_rate_limit_window(Duration::from_secs(
            60
        )));
        assert!(super::is_canonical_rate_limit_window(Duration::from_secs(
            3600
        )));
        // Non-canonical windows the predicate rejects.
        assert!(!super::is_canonical_rate_limit_window(Duration::ZERO));
        assert!(!super::is_canonical_rate_limit_window(Duration::from_secs(
            2
        )));
        assert!(!super::is_canonical_rate_limit_window(Duration::from_secs(
            30
        )));
        assert!(!super::is_canonical_rate_limit_window(Duration::from_secs(
            120
        )));
        assert!(!super::is_canonical_rate_limit_window(Duration::from_secs(
            86400
        )));
        // Sub-second windows: even `Duration::from_millis(1000)` is
        // exactly 1s and accepted; `Duration::from_millis(500)` is
        // sub-second and rejected.
        assert!(super::is_canonical_rate_limit_window(
            Duration::from_millis(1000)
        ));
        assert!(!super::is_canonical_rate_limit_window(
            Duration::from_millis(500)
        ));
        assert!(!super::is_canonical_rate_limit_window(
            Duration::from_millis(1500)
        ));
    }

    #[test]
    fn empty_politicas_validates() {
        // Omitting every policy axis is fine — defaults express "no
        // policy on this axis", not "policy = 0". The fixture's typical
        // values continue to validate; this test pins that
        // MeshPolicy::default() is a clean pass through validate().
        let mut s = three_member_spec();
        s.politicas = MeshPolicy::default();
        s.validate().unwrap();
    }

    #[test]
    fn typical_politicas_validates_with_every_axis_set() {
        // The full §III.1 example block (timeout + retries + breaker +
        // mtls + rate-limit) — every axis nonzero — must remain a
        // clean pass.
        let mut s = three_member_spec();
        s.politicas = MeshPolicy {
            timeout: Some(Duration::from_secs(30)),
            retries: Some(3),
            circuit_breaker: Some(CircuitBreaker {
                max_failures: 5,
                window: Duration::from_secs(60),
            }),
            mtls_required: Some(true),
            rate_limit: Some(RateLimit {
                rate: 100,
                window: Duration::from_secs(1),
            }),
        };
        s.validate().unwrap();
    }

    #[test]
    fn rejects_empty_cluster_name() {
        let mut s = three_member_spec();
        s.placement.clusters = vec!["rio".into(), "".into()];
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PlacementClusterEmpty
        );
    }

    #[test]
    fn rejects_duplicate_cluster_names() {
        let mut s = three_member_spec();
        s.placement.clusters = vec!["rio".into(), "mar".into(), "rio".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::PlacementClusterDuplicate { ref cluster } if cluster == "rio"),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_placement_cluster_with_uppercase() {
        // The canonical "I copied the cluster's display name verbatim"
        // typo — K8s context names are lowercase per DNS-1123 label
        // rule, but org docs often round-trip a TitleCase identifier
        // (`Rio`, `Mar-East`) from an ADR. Mirrors the
        // `rejects_membro_caixa_with_uppercase` gate's shape (3f9d7a0)
        // on the peer name axis.
        let mut s = three_member_spec();
        s.placement.clusters = vec!["Rio".into(), "mar".into()];
        let err = s.validate().unwrap_err();
        let AplicacaoError::PlacementClusterInvalid { cluster, reason } = err else {
            panic!("expected PlacementClusterInvalid, got other variant");
        };
        assert_eq!(cluster, "Rio");
        assert!(
            reason.contains("uppercase"),
            "diagnostic must name the violation as `uppercase` (got: {reason:?})"
        );
        assert!(
            reason.contains("\"rio\""),
            "diagnostic must suggest the lower-cased fix verbatim (got: {reason:?})"
        );
    }

    #[test]
    fn rejects_placement_cluster_with_underscore() {
        // The canonical "I'm thinking of an env var / hostname slug"
        // leak — `_` is forbidden by every DNS-1123 / DNS-1035 label
        // schema. K8s context filtering on `my_cluster` silently misses
        // the cluster the author intended; the gate moves it to caixa-
        // build time. Same shape as `rejects_membro_caixa_with_underscore`
        // (3f9d7a0).
        let mut s = three_member_spec();
        s.placement.clusters = vec!["my_cluster".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::PlacementClusterInvalid { ref cluster, ref reason }
                    if cluster == "my_cluster" && reason.contains('_')
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_placement_cluster_with_dot() {
        // A `:placement :clusters` entry is a single DNS-1123 *label*,
        // not a subdomain — even though K8s context names sometimes
        // carry a dotted form via kubeconfig conventions, the strictest
        // floor among the use sites (DNS-1035 cluster.x-k8s.io
        // `metadata.name`, Cilium identity label values) wins. The "I
        // want to namespace my cluster names with `.`" intent is
        // expressed via `-` (`mar-east`).
        let mut s = three_member_spec();
        s.placement.clusters = vec!["team.rio".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::PlacementClusterInvalid { ref cluster, ref reason }
                    if cluster == "team.rio" && reason.contains('.')
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_placement_cluster_with_leading_hyphen() {
        // DNS-1123 / DNS-1035 boundary rule: labels must start and end
        // with an alphanumeric. The K8s apiserver rejects `-rio`
        // outright; the rendered fan-out would emit a `metadata.name:
        // "-rio"` that fails admission far from the source caixa.lisp.
        let mut s = three_member_spec();
        s.placement.clusters = vec!["-rio".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::PlacementClusterInvalid { ref cluster, ref reason }
                    if cluster == "-rio" && reason.contains("start and end")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_placement_cluster_with_trailing_hyphen() {
        // The symmetric arm of the boundary rule. Pin separately so
        // both ends are covered against a future relaxation that only
        // checks one boundary (parallel to
        // `rejects_membro_caixa_with_trailing_hyphen`, 3f9d7a0).
        let mut s = three_member_spec();
        s.placement.clusters = vec!["rio-".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::PlacementClusterInvalid { ref cluster, .. }
                    if cluster == "rio-"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_placement_cluster_with_unicode() {
        // DNS-1123 is ASCII-only; IDN must be pre-encoded as Punycode
        // before it reaches K8s. The byte-by-byte ASCII validity check
        // rejects multi-byte UTF-8 sequences by the first byte that
        // fails `[a-z0-9-]`.
        let mut s = three_member_spec();
        s.placement.clusters = vec!["rió".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::PlacementClusterInvalid { ref cluster, .. }
                    if cluster == "rió"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_placement_cluster_with_whitespace() {
        // Whitespace is the canonical "I pasted from a sketch / doc"
        // footgun. The apiserver rejects every cluster `metadata.name`
        // value carrying whitespace.
        let mut s = three_member_spec();
        s.placement.clusters = vec!["rio cluster".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::PlacementClusterInvalid { ref cluster, .. }
                    if cluster == "rio cluster"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_placement_cluster_too_long() {
        // 64 bytes exceeds the DNS-1123 label cap by one — the boundary
        // pin. The diagnostic names both the cap (63) and the actual
        // length so the author can shorten in one edit. Mirrors
        // `rejects_membro_caixa_too_long` (3f9d7a0).
        let mut s = three_member_spec();
        let too_long = "a".repeat(64);
        s.placement.clusters = vec![too_long.clone()];
        let err = s.validate().unwrap_err();
        let AplicacaoError::PlacementClusterInvalid { cluster, reason } = err else {
            panic!("expected PlacementClusterInvalid");
        };
        assert_eq!(cluster, too_long);
        assert!(
            reason.contains("63") && reason.contains("64"),
            "diagnostic must name the cap (63) and the actual length (64): {reason:?}"
        );
    }

    #[test]
    fn placement_cluster_max_length_validates() {
        // 63 bytes exactly — the DNS-1123 label cap. Boundary pin so a
        // future tightening (e.g. dropping to 62) surfaces here as a
        // regression, mirroring `membro_caixa_max_length_validates`
        // (3f9d7a0).
        let mut s = three_member_spec();
        s.placement.clusters = vec!["a".repeat(63)];
        s.validate().unwrap();
    }

    #[test]
    fn accepts_canonical_placement_cluster_forms() {
        // The DNS-1123 label shapes a caixa author is realistically
        // going to write for cluster names: single-word lowercase
        // (`rio`), regional hyphen-joined (`mar-east`), single
        // character (`a` — boundary), digit-start (`3-prod` — DNS-1123
        // allows this, unlike DNS-1035), version-suffixed (`prod-v2`).
        // Pin every leg so a future tightening that bans (e.g.) digit-
        // start identifiers surfaces here.
        for form in ["rio", "mar", "mar-east", "a", "p1", "3-prod", "prod-v2"] {
            let mut s = three_member_spec();
            s.placement.clusters = vec![form.into()];
            s.validate().unwrap_or_else(|e| {
                panic!("canonical cluster form {form:?} must validate, got {e:?}")
            });
        }
    }

    #[test]
    fn placement_cluster_empty_takes_precedence_over_invalid() {
        // Order pin: the existing `PlacementClusterEmpty` diagnostic
        // (which doesn't try to parse) fires before the new
        // `PlacementClusterInvalid` parse-side diagnostic, so an empty
        // `:clusters` entry keeps its narrower error message — the new
        // gate would also reject `""`, but the empty-string arm is the
        // more self-locating diagnostic. Mirrors the
        // `membro_caixa_empty_takes_precedence_over_invalid` pin
        // (3f9d7a0).
        let mut s = three_member_spec();
        s.placement.clusters = vec!["rio".into(), "".into()];
        let err = s.validate().unwrap_err();
        assert_eq!(err, AplicacaoError::PlacementClusterEmpty);
    }

    #[test]
    fn placement_cluster_invalid_fires_before_duplicate_check() {
        // Order pin: a malformed-shape `:clusters` entry surfaces *its
        // own* diagnostic, even when a later entry would otherwise
        // collapse onto a duplicate name. The per-entry shape gate runs
        // inline before the duplicate-key insert, parallel to
        // `membro_caixa_invalid_fires_before_duplicate_check` (3f9d7a0).
        let mut s = three_member_spec();
        s.placement.clusters = vec!["Rio".into(), "rio".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::PlacementClusterInvalid { ref cluster, .. } if cluster == "Rio"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn placement_cluster_invalid_diagnostic_carries_offending_cluster() {
        // The diagnostic-shape pin: the error names the offending
        // `:clusters` value verbatim so the author can grep their
        // caixa.lisp without re-running the build, and carries a
        // non-empty `reason` naming the specific violation. Same shape
        // every typed-shape gate enshrines
        // (3f9d7a0's `membro_caixa_invalid_diagnostic_carries_offending_caixa`,
        // c7d05ec's `entrada_host_diagnostic_carries_offending_host`).
        let mut s = three_member_spec();
        s.placement.clusters = vec!["BAD_CLUSTER".into()];
        let err = s.validate().unwrap_err();
        let AplicacaoError::PlacementClusterInvalid { cluster, reason } = err else {
            panic!("expected PlacementClusterInvalid");
        };
        assert_eq!(cluster, "BAD_CLUSTER");
        assert!(
            !reason.is_empty(),
            "PlacementClusterInvalid `reason` must carry a parser-shaped wording"
        );
    }

    #[test]
    fn rejects_sharded_with_empty_clusters() {
        // §III.1: Sharded uses :clusters as the shard pool. An empty
        // pool means "shard across no clusters" — meaningless, same as
        // Replicated with no hosts.
        let mut s = three_member_spec();
        s.placement.estrategia = PlacementStrategy::Sharded;
        s.placement.shard_key = Some("$tenantId".into());
        s.placement.clusters = vec![];
        assert!(matches!(
            s.validate().unwrap_err(),
            AplicacaoError::PlacementWithoutClusters {
                estrategia: PlacementStrategy::Sharded
            }
        ));
    }

    #[test]
    fn rejects_sharded_with_empty_shard_key() {
        let mut s = three_member_spec();
        s.placement.estrategia = PlacementStrategy::Sharded;
        s.placement.shard_key = Some("".into());
        assert_eq!(s.validate().unwrap_err(), AplicacaoError::ShardedKeyEmpty);
    }

    #[test]
    fn rejects_shard_key_under_replicated_strategy() {
        // The fail-before-pass-after pin: a `:placement (:estrategia
        // Replicated :shard-key "tenantId")` manifest carries the
        // hash-keyed-distribution slot on a strategy that never consumes
        // it. Before the gate the typed slot's value silently vanished
        // at the renderer layer (caixa-mesh emits `placement.shardKey`
        // verbatim regardless of strategy; the Akka-style cluster-
        // sharding reconciler keys off `estrategia == Sharded` and
        // ignores the slot otherwise), with no diagnostic. Lifting the
        // rejection to a build-time gate makes the
        // `shard_key.is_some() == matches!(estrategia, Sharded)`
        // partition a structural property of every validated
        // [`Placement`].
        let mut s = three_member_spec();
        // The fixture already uses Replicated; just add a shard-key.
        s.placement.shard_key = Some("$tenantId".into());
        let err = s.validate().unwrap_err();
        let AplicacaoError::ShardKeyOnNonSharded {
            estrategia,
            shard_key,
        } = err
        else {
            panic!("expected ShardKeyOnNonSharded, got {err:?}");
        };
        assert_eq!(estrategia, PlacementStrategy::Replicated);
        assert_eq!(shard_key, "$tenantId");
    }

    #[test]
    fn rejects_shard_key_under_singlenode_strategy() {
        // Peer of the Replicated case above on the SingleNode arm: OTP
        // distributed-app takeover (one cluster runs at a time) has no
        // hash-keyed routing axis to consume `:shard-key` either, so
        // the rejection fires on both non-Sharded arms uniformly.
        let mut s = three_member_spec();
        s.placement.estrategia = PlacementStrategy::SingleNode;
        s.placement.shard_key = Some("$tenantId".into());
        let err = s.validate().unwrap_err();
        let AplicacaoError::ShardKeyOnNonSharded {
            estrategia,
            shard_key,
        } = err
        else {
            panic!("expected ShardKeyOnNonSharded, got {err:?}");
        };
        assert_eq!(estrategia, PlacementStrategy::SingleNode);
        assert_eq!(shard_key, "$tenantId");
    }

    #[test]
    fn rejects_empty_shard_key_under_replicated_strategy() {
        // The `Some("")` case under non-Sharded is rejected by
        // [`AplicacaoError::ShardKeyOnNonSharded`] (the strategy gate
        // fires before the empty-value gate), not
        // [`AplicacaoError::ShardedKeyEmpty`] (which is reserved for
        // the `Sharded` arm). Pin the partition so a future reorder of
        // the validate_placement match arms doesn't silently swap which
        // diagnostic the author sees — both are author errors, but
        // ShardKeyOnNonSharded names which strategy is the actual fix
        // (drop the slot, or switch to Sharded), while ShardedKeyEmpty
        // only says "pick a non-empty key".
        let mut s = three_member_spec();
        s.placement.shard_key = Some(String::new());
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ShardKeyOnNonSharded {
                    estrategia: PlacementStrategy::Replicated,
                    ref shard_key,
                } if shard_key.is_empty()
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn replicated_without_shard_key_validates() {
        // The complement of the rejection: `:placement :estrategia
        // Replicated` with `:shard-key None` is the canonical happy
        // path on every existing fixture. Pin the no-shard-key case so
        // the new gate doesn't accidentally fire on `None`.
        let mut s = three_member_spec();
        assert!(matches!(
            s.placement.estrategia,
            PlacementStrategy::Replicated
        ));
        s.placement.shard_key = None;
        s.validate().unwrap();
    }

    #[test]
    fn singlenode_without_shard_key_validates() {
        // Peer of the Replicated no-shard-key case on the SingleNode
        // arm — both non-Sharded strategies must validate cleanly when
        // the slot is omitted.
        let mut s = three_member_spec();
        s.placement.estrategia = PlacementStrategy::SingleNode;
        s.placement.shard_key = None;
        s.validate().unwrap();
    }

    #[test]
    fn rejects_empty_affinity_hint() {
        let mut s = three_member_spec();
        s.placement.affinity = Some("".into());
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PlacementAffinityEmpty
        );
    }

    #[test]
    fn placement_without_affinity_validates() {
        // Omitting :affinity is fine — the placement engine falls back
        // to the default heuristic. Pin the no-hint case so the
        // affinity-empty rejection doesn't accidentally fire on `None`.
        let mut s = three_member_spec();
        s.placement.affinity = None;
        s.validate().unwrap();
    }

    #[test]
    fn singlenode_with_takeover_candidates_validates() {
        // OTP distributed-application convention (MESH-COMPOSITION
        // §II.1): SingleNode runs on one cluster at a time but the
        // :clusters list enumerates the takeover candidates. Multiple
        // entries are not a contradiction — they are the failover pool.
        let mut s = three_member_spec();
        s.placement.estrategia = PlacementStrategy::SingleNode;
        s.placement.clusters = vec!["rio".into(), "mar".into(), "plo".into()];
        s.validate().unwrap();
    }

    // ── MeshPolicy::is_empty() — typed emptiness predicate ────────────────

    #[test]
    fn mesh_policy_default_is_empty() {
        // The Default impl carries None on every axis — the typed
        // analog of an unset `:politicas (())` slot. Renderers that
        // overlay the policy onto a cluster artifact key off this
        // predicate to skip the slot entirely; pinning so a future
        // axis added to MeshPolicy can't silently break the contract
        // (a new field whose Default is non-None would flip is_empty
        // to false on every existing caixa, surfacing here).
        assert!(MeshPolicy::default().is_empty());
    }

    #[test]
    fn mesh_policy_with_only_timeout_is_not_empty() {
        let p = MeshPolicy {
            timeout: Some(Duration::from_secs(30)),
            ..Default::default()
        };
        assert!(!p.is_empty());
    }

    #[test]
    fn mesh_policy_with_only_retries_is_not_empty() {
        let p = MeshPolicy {
            retries: Some(3),
            ..Default::default()
        };
        assert!(!p.is_empty());
    }

    #[test]
    fn mesh_policy_with_only_circuit_breaker_is_not_empty() {
        let p = MeshPolicy {
            circuit_breaker: Some(CircuitBreaker {
                max_failures: 5,
                window: Duration::from_secs(60),
            }),
            ..Default::default()
        };
        assert!(!p.is_empty());
    }

    #[test]
    fn mesh_policy_with_only_mtls_required_is_not_empty() {
        // Even `mtls_required: Some(false)` (an explicit opt-out) is
        // not empty — the author *named* the axis, the renderer needs
        // to honor that vs. fall back to the cluster default.
        let p = MeshPolicy {
            mtls_required: Some(false),
            ..Default::default()
        };
        assert!(!p.is_empty());
    }

    #[test]
    fn mesh_policy_with_only_rate_limit_is_not_empty() {
        let p = MeshPolicy {
            rate_limit: Some(RateLimit {
                rate: 100,
                window: Duration::from_secs(1),
            }),
            ..Default::default()
        };
        assert!(!p.is_empty());
    }

    #[test]
    fn mesh_policy_is_empty_round_trips_through_three_member_fixture() {
        // The three-member happy-path fixture sets timeout + retries +
        // mtls_required — every populated axis must read non-empty.
        // Pin the round-trip so the M3.x per-:politicas emitter (the
        // M3.x roadmap CiliumClusterwideEnvoyConfig artifact) can rely
        // on is_empty() to decide whether to emit at all without
        // re-deriving the contract from inline field probes.
        assert!(!three_member_spec().politicas.is_empty());
    }

    // ── shared duration codec: cross-slot integer-magnitude gate ──
    //
    // The integer-magnitude discipline applied to
    // `supervisor::duration_codec::parse` lifts onto every typed slot
    // that routes through the shared codec — `MeshPolicy::timeout`
    // (`:politicas :timeout`) and `CircuitBreaker::window`
    // (`:politicas :circuit-breaker :window`) on the Aplicacao side.
    // These cross-slot tests pin that the gate fires at the serde
    // layer for both typed slots, not just for the supervisor side.

    #[test]
    fn policy_timeout_serde_rejects_fractional_seconds() {
        // `MeshPolicy::timeout` uses `with = "supervisor::duration_codec"`,
        // so the shared codec's integer-magnitude gate applies on
        // deserialize. `"1.5s"` previously parsed to 1500ms and round-
        // tripped to `"1500ms"` on next emit — DRIFT. Now refused at
        // deserialize with the canonical-form diagnostic naming the
        // offending `"1.5"` and the remediation `"1500ms"`.
        let payload = r#"{"timeout":"1.5s"}"#;
        let err = serde_json::from_str::<MeshPolicy>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not a non-negative integer"),
            "expected integer-magnitude diagnostic in {msg:?}"
        );
        assert!(msg.contains("\"1.5\""), "missing magnitude in {msg:?}");
        assert!(
            msg.contains("\"1500ms\""),
            "missing canonical-form remediation in {msg:?}"
        );
    }

    #[test]
    fn policy_timeout_serde_rejects_leading_plus_sign() {
        // Pin the leading-`+` arm cross-slot — the prior f64 parser
        // accepted `"+30s"` silently and round-tripped to `"30s"`.
        let payload = r#"{"timeout":"+30s"}"#;
        let err = serde_json::from_str::<MeshPolicy>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("\"+30\""), "missing magnitude in {msg:?}");
    }

    #[test]
    fn circuit_breaker_window_serde_rejects_fractional_minutes() {
        // `CircuitBreaker::window` uses `with =
        // "supervisor::duration_codec_required"` (the required-Duration
        // variant that delegates to the same shared parser). `"0.5m"`
        // parsed to 30s and round-tripped to `"30s"` on next emit —
        // DRIFT closed.
        let payload = r#"{"maxFailures":5,"window":"0.5m"}"#;
        let err = serde_json::from_str::<CircuitBreaker>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not a non-negative integer"),
            "expected integer-magnitude diagnostic in {msg:?}"
        );
        assert!(msg.contains("\"0.5\""), "missing magnitude in {msg:?}");
        assert!(
            msg.contains("\"30s\""),
            "missing canonical-form remediation in {msg:?}"
        );
    }

    #[test]
    fn circuit_breaker_window_serde_accepts_integer_canonical_form() {
        // Pin the happy-path on the cross-slot side: every canonical
        // author shape `render` ever emits parses cleanly through the
        // shared codec on the `CircuitBreaker` slot. The
        // codec's accepted set (post-gate) is exactly its emitted set
        // for the integer-magnitude class.
        for window_lit in ["30s", "500ms", "2m", "1h"] {
            let payload = format!(r#"{{"maxFailures":5,"window":"{window_lit}"}}"#);
            let cb: CircuitBreaker = serde_json::from_str(&payload).unwrap_or_else(|e| {
                panic!("expected {window_lit:?} to parse cleanly through shared codec: {e}")
            });
            assert_eq!(cb.max_failures, 5);
        }
    }

    // ── rate_limit_codec: integer-magnitude gate ──
    //
    // The integer-magnitude discipline the 1c55a2a / 818dd38 / d1fd67b
    // / 737a676 / d53c922 trajectory landed on every typed-duration /
    // typed-byte-size codec in caixa-core lifts onto the fifth typed
    // codec — `rate_limit_codec` — through the digit-only magnitude
    // gate on the `<rate>` half of the `<rate>/<unit>` author surface.
    // These tests pin the gate at the serde layer for `:politicas
    // :rate-limit` (the only typed slot the codec backs), and at the
    // codec-internal `parse` layer for the canonical positive cases.

    #[test]
    fn rate_limit_serde_rejects_fractional_rate() {
        // `"1.5/s"` previously hit `u32::from_str`'s rejection arm with
        // the value-laundered `"rate-limit rate \"1.5\" not a u32"`
        // wording, which didn't name the canonical-form remediation or
        // the round-trip drift the next emit would produce. Now refused
        // at deserialize with the canonical-form diagnostic naming the
        // offending `"1.5"` magnitude and the round-trip drift wording.
        let payload = r#"{"rateLimit":"1.5/s"}"#;
        let err = serde_json::from_str::<MeshPolicy>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not a non-negative integer"),
            "expected integer-magnitude diagnostic in {msg:?}"
        );
        assert!(msg.contains("\"1.5\""), "missing magnitude in {msg:?}");
        assert!(
            msg.contains("THEORY.md"),
            "missing render-determinism contract citation in {msg:?}"
        );
    }

    #[test]
    fn rate_limit_serde_rejects_leading_plus_sign() {
        // `u32::from_str("+100")` returns `Ok(100)` (Rust's
        // permissive-`+` parse), so `"+100/s"` silently parsed to
        // `RateLimit { 100, 1s }` and round-tripped through `render` to
        // `"100/s"` — a *different* canonical string on the next emit,
        // breaking the THEORY.md Part V render-determinism contract
        // exactly the way the peer duration codecs' `"+30s"` case did.
        // This is the load-bearing class the digit-only gate closes
        // beyond what `u32::from_str`'s strictness covers on its own.
        let payload = r#"{"rateLimit":"+100/s"}"#;
        let err = serde_json::from_str::<MeshPolicy>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not a non-negative integer"),
            "expected integer-magnitude diagnostic in {msg:?}"
        );
        assert!(msg.contains("\"+100\""), "missing magnitude in {msg:?}");
    }

    #[test]
    fn rate_limit_serde_rejects_leading_minus_sign() {
        // The signed-negative arm: `"-1/s"` lands on the
        // non-canonical-but-numeric branch via the `i64` fallback (the
        // `f64` parse also succeeds), surfacing the canonical-form
        // diagnostic. Replaces the prior value-laundered "not a u32"
        // wording with the unified diagnostic across signs.
        let payload = r#"{"rateLimit":"-1/s"}"#;
        let err = serde_json::from_str::<MeshPolicy>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not a non-negative integer"),
            "expected integer-magnitude diagnostic in {msg:?}"
        );
        assert!(msg.contains("\"-1\""), "missing magnitude in {msg:?}");
    }

    #[test]
    fn rate_limit_serde_rejects_decimal_shaped_integer() {
        // `"100.0/s"` is integer-valued numerically but not in the
        // codec's accepted set — `render` emits `"100/s"`, so the
        // round-trip would drift. Lifted to the canonical-form
        // diagnostic peer with the duration codec's `"1.0s"` case
        // (1c55a2a).
        let payload = r#"{"rateLimit":"100.0/s"}"#;
        let err = serde_json::from_str::<MeshPolicy>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not a non-negative integer"),
            "expected integer-magnitude diagnostic in {msg:?}"
        );
        assert!(msg.contains("\"100.0\""), "missing magnitude in {msg:?}");
    }

    #[test]
    fn rate_limit_serde_garbage_still_falls_through_to_not_a_u32() {
        // Non-numeric, non-digit-only input lands on the existing
        // narrower `"not a u32"` arm (preserved for diagnostic-shape
        // stability on the parser-shape footgun case). Pin this so a
        // future relaxation of the numeric-fallback predicate doesn't
        // silently collapse garbage onto the canonical-form arm — same
        // partition the peer duration codecs draw between
        // `NonIntegerDurationMagnitude` and `BadDurationMagnitude`.
        let payload = r#"{"rateLimit":"abc/s"}"#;
        let err = serde_json::from_str::<MeshPolicy>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not a u32"),
            "garbage magnitude must surface the narrower `not a u32` wording, got: {msg:?}"
        );
        assert!(
            !msg.contains("not a non-negative integer"),
            "garbage magnitude must NOT surface the canonical-form arm, got: {msg:?}"
        );
    }

    #[test]
    fn rate_limit_serde_u32_overflow_surfaces_as_overflow() {
        // `u32::MAX + 1` (= 4_294_967_296) is digit-only but exceeds
        // u32's range. The digit-only gate passes; `u32::from_str`
        // fails on overflow. Surface that with the overflow-shaped
        // diagnostic naming the offending magnitude verbatim, peer
        // with `supervisor::duration_codec`'s overflow arm. Pinning
        // the wording so a future refactor doesn't silently collapse
        // overflow onto the canonical-form arm.
        let payload = r#"{"rateLimit":"4294967296/s"}"#;
        let err = serde_json::from_str::<MeshPolicy>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("overflows u32"),
            "expected overflow diagnostic in {msg:?}"
        );
        assert!(
            msg.contains("\"4294967296\""),
            "missing offending magnitude in {msg:?}"
        );
    }

    #[test]
    fn rate_limit_serde_accepts_integer_canonical_forms() {
        // Pin the happy-path: every canonical author shape `render`
        // ever emits parses cleanly through the codec post-gate. The
        // codec's accepted set (post-gate) is exactly its emitted set
        // for the integer-magnitude class — same property
        // `parse_byte_size`'s and `parse_duration`'s integer-magnitude
        // gates guarantee on the peer codecs. Iterating across rate
        // magnitudes (including `"0"`, which the codec accepts even
        // though `validate_politicas` rejects `rate == 0` at the typed
        // layer above) closes the codec contract at the parse layer
        // independently of the validate layer.
        for rate_lit in ["0", "1", "100", "5000", "1000000", "4294967295"] {
            for unit_lit in ["s", "m", "h"] {
                let lit = format!("{rate_lit}/{unit_lit}");
                let payload = format!(r#"{{"rateLimit":{lit:?}}}"#);
                let policy: MeshPolicy = serde_json::from_str(&payload).unwrap_or_else(|e| {
                    panic!("expected {lit:?} to parse cleanly through rate_limit_codec: {e}")
                });
                let rl = policy.rate_limit.expect("rate_limit must be Some");
                assert_eq!(
                    rl.rate,
                    rate_lit.parse::<u32>().unwrap(),
                    "rate mismatch for {lit:?}"
                );
            }
        }
    }

    #[test]
    fn rate_limit_serde_round_trip_holds_for_every_canonical_form() {
        // The structural property the gate enforces: serialize ∘
        // deserialize is the identity on every canonical author shape.
        // Peer of `parse_byte_size`'s and `parse_duration`'s
        // `_round_trips_through_render_for_every_canonical_form` tests
        // on the rate-limit axis. Before the gate, `"+100/s"` violated
        // this (`parse` → `RateLimit { 100, 1s }` → `render` →
        // `"100/s"` ≠ `"+100/s"`); the gate forecloses that class.
        for rate in [1u32, 100, 5000, 1_000_000] {
            for (window, unit) in [
                (Duration::from_secs(1), "s"),
                (Duration::from_secs(60), "m"),
                (Duration::from_secs(3600), "h"),
            ] {
                let policy = MeshPolicy {
                    rate_limit: Some(RateLimit { rate, window }),
                    ..Default::default()
                };
                let json = serde_json::to_string(&policy).unwrap();
                let expected = format!("\"{rate}/{unit}\"");
                assert!(
                    json.contains(&expected),
                    "expected {expected:?} in {json:?}"
                );
                let back: MeshPolicy = serde_json::from_str(&json).unwrap();
                assert_eq!(
                    back.rate_limit, policy.rate_limit,
                    "round-trip for {json:?}"
                );
            }
        }
    }
}
