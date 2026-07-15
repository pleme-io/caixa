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

/// Canonical lowercase byte-prefix set the substrate's WIT-shape
/// dispatch routes `wasi:http/*` / `http:*` values through as the
/// HTTP-shaped arm. The single source of truth every consumer that
/// classifies a `:wit` value as HTTP-shaped consults —
/// [`WitContract::is_http`] on the typed contract, the
/// `AplicacaoSpec::validate` positive-sweep test's payload-dispatch
/// helper, and every future renderer that routes an L7 emission off a
/// bare `&str` (the M4 per-edge WIT registry resolver, the future
/// `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer). Spelled
/// exactly as the [`is_wit_world_ref`][iwr] predicate documents the
/// canonical lowercase prefixes ("`wasi:http/`, `nats:`,
/// `wasi:keyvalue/`, `kafka:`, `kv:`, `http:`") so drift between the
/// substrate's accept-set and this crate's dispatch-set is a
/// build-time compile error (unused-import), not a per-renderer
/// silent L7-→-L4 demotion at apply time.
///
/// [iwr]: crate::render::is_wit_world_ref
pub const WIT_HTTP_SHAPE_PREFIXES: &[&str] = &["wasi:http/", "http:"];

/// Canonical lowercase byte-prefix set the substrate's WIT-shape
/// dispatch routes `nats:*` / `kafka:*` values through as the
/// pub-sub-shaped arm. Peer of [`WIT_HTTP_SHAPE_PREFIXES`] /
/// [`WIT_STORE_SHAPE_PREFIXES`] on the shape-dispatch axis; see the
/// HTTP constant's docstring for the full lift rationale.
pub const WIT_PUBSUB_SHAPE_PREFIXES: &[&str] = &["nats:", "kafka:"];

/// Canonical lowercase byte-prefix set the substrate's WIT-shape
/// dispatch routes `wasi:keyvalue/*` / `kv:*` values through as the
/// key/value-store-shaped arm. Peer of [`WIT_HTTP_SHAPE_PREFIXES`] /
/// [`WIT_PUBSUB_SHAPE_PREFIXES`] on the shape-dispatch axis; see the
/// HTTP constant's docstring for the full lift rationale.
pub const WIT_STORE_SHAPE_PREFIXES: &[&str] = &["wasi:keyvalue/", "kv:"];

/// True when `wit` — a raw `:contratos :wit` value — starts with any
/// entry in the `prefixes` accept-set. The single canonical
/// prefix-driven WIT-shape classification combinator every peer
/// per-shape predicate ([`wit_shape_is_http`], [`wit_shape_is_pubsub`],
/// [`wit_shape_is_store`]) routes through, closing the 3-site
/// duplication of the `PREFIXES.iter().any(|p| wit.starts_with(p))`
/// combinator the prior open-coded implementations each carried.
///
/// A future 4th WIT-shape dispatch arm (a hypothetical `wasi:sockets/*`
/// / `tcp:*` transport-layer shape, an `oci:*` capability-import
/// carrier) becomes exactly one new [`WIT_*_SHAPE_PREFIXES`] const +
/// one new `wit_shape_is_<name>` one-liner routing through this
/// combinator, not a fourth copy of the `iter().any(starts_with)`
/// combinator paired to its own prefix-set. Same "one canonical
/// combinator, thin per-arm projections" discipline the peer
/// [`WitTarget::payload_pair`] (6788ed6) already established for the
/// downstream per-arm `(field, payload)` dispatch, extended to the
/// upstream per-arm `PREFIXES → bool` dispatch.
#[must_use]
pub fn wit_shape_matches(wit: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|p| wit.starts_with(p))
}

/// True when `wit` — a raw `:contratos :wit` value — targets an
/// HTTP-shaped WIT world (starts with any prefix in
/// [`WIT_HTTP_SHAPE_PREFIXES`]). The single dispatch predicate every
/// consumer routes L7-HTTP emission through, whether they carry a
/// full [`WitContract`] on hand ([`WitContract::is_http`] delegates
/// here) or only the raw `wit` string (the positive-sweep test's
/// payload-dispatch helper, future renderers that classify off a
/// bare `&str`). Lifting to a free function makes the shape-dispatch
/// arm reachable without materializing a scratch [`WitContract`] at
/// every classification point, and pins the six-prefix accept-set at
/// one place so future additions (e.g. an `"https:"` peer of
/// `"http:"`) reach every consumer by construction. Routes through
/// the lifted [`wit_shape_matches`] combinator so the
/// `PREFIXES.iter().any(|p| wit.starts_with(p))` scan lives at one
/// canonical primitive, not one open-coded copy per peer arm.
#[must_use]
pub fn wit_shape_is_http(wit: &str) -> bool {
    wit_shape_matches(wit, WIT_HTTP_SHAPE_PREFIXES)
}

/// True when `wit` — a raw `:contratos :wit` value — targets a
/// pub-sub-shaped WIT world (starts with any prefix in
/// [`WIT_PUBSUB_SHAPE_PREFIXES`]). Peer of [`wit_shape_is_http`] /
/// [`wit_shape_is_store`] on the shape-dispatch axis; see
/// [`wit_shape_is_http`] for the lift rationale. Routes through the
/// lifted [`wit_shape_matches`] combinator.
#[must_use]
pub fn wit_shape_is_pubsub(wit: &str) -> bool {
    wit_shape_matches(wit, WIT_PUBSUB_SHAPE_PREFIXES)
}

/// True when `wit` — a raw `:contratos :wit` value — targets a
/// key/value-store-shaped WIT world (starts with any prefix in
/// [`WIT_STORE_SHAPE_PREFIXES`]). Peer of [`wit_shape_is_http`] /
/// [`wit_shape_is_pubsub`] on the shape-dispatch axis; see
/// [`wit_shape_is_http`] for the lift rationale. Routes through the
/// lifted [`wit_shape_matches`] combinator.
#[must_use]
pub fn wit_shape_is_store(wit: &str) -> bool {
    wit_shape_matches(wit, WIT_STORE_SHAPE_PREFIXES)
}

impl WitContract {
    /// True when this contract targets an HTTP-shaped WIT world.
    #[must_use]
    pub fn is_http(&self) -> bool {
        wit_shape_is_http(&self.wit)
    }

    /// True when this contract targets a pub-sub-shaped WIT world.
    #[must_use]
    pub fn is_pubsub(&self) -> bool {
        wit_shape_is_pubsub(&self.wit)
    }

    /// True when this contract targets a key/value-shaped WIT world.
    #[must_use]
    pub fn is_store(&self) -> bool {
        wit_shape_is_store(&self.wit)
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
                    expected: WitTarget::HTTP_FIELD_NAME,
                });
            }
            let ep = endpoint.ok_or_else(|| {
                let (de, para, wit) = edge();
                AplicacaoError::ContratoMissingTarget {
                    de,
                    para,
                    wit,
                    expected: WitTarget::HTTP_FIELD_NAME,
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
                    expected: WitTarget::PUBSUB_FIELD_NAME,
                });
            }
            let s = subject.ok_or_else(|| {
                let (de, para, wit) = edge();
                AplicacaoError::ContratoMissingTarget {
                    de,
                    para,
                    wit,
                    expected: WitTarget::PUBSUB_FIELD_NAME,
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
                    expected: WitTarget::STORE_FIELD_NAME,
                });
            }
            let sl = slot.ok_or_else(|| {
                let (de, para, wit) = edge();
                AplicacaoError::ContratoMissingTarget {
                    de,
                    para,
                    wit,
                    expected: WitTarget::STORE_FIELD_NAME,
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
                expected: WitTarget::CAPABILITY_EXPECTED,
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

impl<'a> WitTarget<'a> {
    /// Canonical author-facing `:contratos` payload field name for the
    /// HTTP-shaped arm — the `expected: &'static str` scalar the
    /// [`AplicacaoError::ContratoMissingTarget`] /
    /// [`AplicacaoError::ContratoWrongTarget`] diagnostic threads
    /// through, the `:endpoint "…"` keyword the [`WitTarget::label`]
    /// duplicate-edge diagnostic emits, and the `endpoint=…` prefix
    /// the `feira app graph` verb prints. Peer of
    /// [`WitTarget::PUBSUB_FIELD_NAME`] / [`WitTarget::STORE_FIELD_NAME`]
    /// on the payload-field-name axis; declared as a peer const next
    /// to the [`WitTarget::Http`] variant so a future rename on the
    /// author-surface `(defcaixa … :contratos ((:de … :para … :wit …
    /// :endpoint …)))` field lands in exactly one place, not scattered
    /// across the [`WitContract::target`] gate's six `expected:`
    /// literals, the label template, and every downstream consumer
    /// that prints a per-arm prefix. Same trajectory as the peer
    /// [`WitTarget::label`] lift (174e96a): a single source of truth
    /// for the arm's shape, next to the variant declaration.
    pub const HTTP_FIELD_NAME: &'static str = "endpoint";
    /// Canonical author-facing `:contratos` payload field name for the
    /// pub-sub-shaped arm. Peer of [`WitTarget::HTTP_FIELD_NAME`] /
    /// [`WitTarget::STORE_FIELD_NAME`] on the payload-field-name axis;
    /// see [`WitTarget::HTTP_FIELD_NAME`] for the full lift rationale.
    pub const PUBSUB_FIELD_NAME: &'static str = "subject";
    /// Canonical author-facing `:contratos` payload field name for the
    /// key/value-store-shaped arm. Peer of
    /// [`WitTarget::HTTP_FIELD_NAME`] / [`WitTarget::PUBSUB_FIELD_NAME`]
    /// on the payload-field-name axis; see
    /// [`WitTarget::HTTP_FIELD_NAME`] for the full lift rationale.
    pub const STORE_FIELD_NAME: &'static str = "slot";

    /// Canonical stable human-readable label the payload-less
    /// [`WitTarget::Capability`] arm renders as under [`Self::label`] —
    /// the byte-string every consumer that formats a payload-less
    /// typed capability edge as text lands on (the
    /// [`AplicacaoSpec::validate`] duplicate-`:contratos` diagnostic
    /// naming which identical edge was declared twice, the future
    /// `feira app graph` verb's per-arm prefix, the future M4 per-edge
    /// policy resolver's audit view, the operator's mesh-graph audit).
    /// Peer of the payload-arm [`Self::HTTP_FIELD_NAME`] /
    /// [`Self::PUBSUB_FIELD_NAME`] / [`Self::STORE_FIELD_NAME`]
    /// author-facing label-scalar consts — the same
    /// "one canonical declaration per arm, next to the variant, so a
    /// future rename lands in one place" discipline extended to the
    /// payload-less arm. Until this lift landed the byte-string sat
    /// twice — once inline in [`Self::label`]'s [`WitTarget::Capability`]
    /// match arm, once in the pin test asserting the label's
    /// [`WitTarget::Capability`] output — with no compile-time link
    /// between the two: a rebrand on either side (an operator-facing
    /// vocabulary shift, a per-consumer disambiguation like
    /// `"(capability — no payload; typed edge only)"`) would silently
    /// desynchronize until a downstream consumer surfaced the drift at
    /// runtime.
    pub const CAPABILITY_LABEL: &'static str = "(capability — no payload)";

    /// Canonical `expected:` scalar the
    /// [`AplicacaoError::ContratoWrongTarget`] diagnostic threads
    /// through for the payload-less [`WitTarget::Capability`] arm — the
    /// byte-string authors read as "this WIT world's shape is not one
    /// of {`HTTP`, `PubSub`, `Store`}, so it must not carry
    /// `:endpoint` / `:subject` / `:slot`". Peer of the payload-arm
    /// [`Self::HTTP_FIELD_NAME`] / [`Self::PUBSUB_FIELD_NAME`] /
    /// [`Self::STORE_FIELD_NAME`] consts on the
    /// `ContratoWrongTarget::expected` axis — the fourth arm of the
    /// same "which payload field name goes in the diagnostic" dispatch
    /// the three payload-arm consts cover, extended to the payload-less
    /// arm. Until this lift landed the byte-string sat twice — once
    /// inline in the [`Self::target`] Capability-arm rejection at the
    /// production dispatch, once in the pin test asserting the
    /// diagnostic's `expected:` scalar carries `"none"` verbatim — with
    /// no compile-time link between the two: a rebrand on either side
    /// (an author-facing vocabulary shift to `"capability"` /
    /// `"(none)"` / `"no-payload"` as the WIT registry's shape
    /// vocabulary sharpens, a per-consumer disambiguation as M4 splits
    /// [`WitTarget::Capability`] into per-shape peers) would silently
    /// desynchronize until a downstream consumer surfaced the drift at
    /// runtime. Same "one canonical declaration per arm, next to the
    /// variant, so a future rename lands in one place" discipline the
    /// peer [`Self::CAPABILITY_LABEL`] lift (7ed03a3-era) already
    /// established for the payload-less arm's human-readable label
    /// axis; this lift extends it onto the peer diagnostic-scalar axis
    /// so both halves of the "how does the Capability arm surface at
    /// its two consumer axes (human-readable label, wrong-target
    /// diagnostic)" pipeline route through peer consts declared next
    /// to the variant.
    pub const CAPABILITY_EXPECTED: &'static str = "none";

    /// The `(author-facing field name, payload)` pair this typed target
    /// arm carries — `Some((HTTP_FIELD_NAME, endpoint))` for
    /// [`Self::Http`], `Some((PUBSUB_FIELD_NAME, subject))` for
    /// [`Self::PubSub`], `Some((STORE_FIELD_NAME, slot))` for
    /// [`Self::Store`], `None` for the payload-less
    /// [`Self::Capability`] arm.
    ///
    /// Lifted as the single 4-arm dispatch that both [`Self::label`]
    /// (formats `":{field} {payload:?}"` on `Some`, falls to
    /// [`Self::CAPABILITY_LABEL`] on `None`) and [`Self::field_name`]
    /// (returns the first component) route through, so a future
    /// [`WitTarget`] variant addition — the M4-and-later per-edge WIT
    /// registry may split [`Self::Http`] into `Rest` / `Grpc` peers,
    /// or extend [`Self::Store`] with a `Queue`-shaped peer — becomes
    /// exactly one new match-arm here (a compile-time exhaustiveness
    /// error otherwise), not a coordinated three-way rewrite of the
    /// prior [`Self::label`] template + [`Self::field_name`] dispatch
    /// + every downstream consumer that reaches for the pair.
    ///
    /// Until this lift landed the three payload arms sat in
    /// [`Self::label`] as three near-identical `format!(":{} {…:?}", …)`
    /// invocations (one per variant, each hand-quoting the paired
    /// [`Self::HTTP_FIELD_NAME`] / [`Self::PUBSUB_FIELD_NAME`] /
    /// [`Self::STORE_FIELD_NAME`] const) — the canonical
    /// "same shape, written N times" duplication THEORY.md §I.3.5
    /// ("Generation first, composition second, hand-authoring last;
    /// the duplication budget is zero") promotes to a build-time
    /// concern, with each per-arm site paired to its own const with no
    /// compile-time link between the format template and the arm's
    /// payload extraction.
    #[must_use]
    pub const fn payload_pair(&self) -> Option<(&'static str, &'a str)> {
        match *self {
            WitTarget::Http { endpoint } => Some((Self::HTTP_FIELD_NAME, endpoint)),
            WitTarget::PubSub { subject } => Some((Self::PUBSUB_FIELD_NAME, subject)),
            WitTarget::Store { slot } => Some((Self::STORE_FIELD_NAME, slot)),
            WitTarget::Capability => None,
        }
    }

    /// The canonical author-facing `:contratos` payload field name
    /// this typed target arm carries (`Http` → `Some("endpoint")`,
    /// `PubSub` → `Some("subject")`, `Store` → `Some("slot")`), or
    /// `None` for the payload-less `Capability` arm.
    ///
    /// Routes through [`Self::payload_pair`] — the single 4-arm
    /// dispatch [`Self::label`] also reads — so a future variant
    /// addition is one match-arm edit at [`Self::payload_pair`], not a
    /// per-consumer rewrite. Same "exhaustive-match at one canonical
    /// dispatch, thin projections at each consumer" trajectory the
    /// peer [`PlacementStrategy::as_str`] / [`std::fmt::Display`]
    /// pair (0a2f653) landed on the sibling M3 typed-enum axis.
    #[must_use]
    pub const fn field_name(&self) -> Option<&'static str> {
        match self.payload_pair() {
            Some((f, _)) => Some(f),
            None => None,
        }
    }

    /// Render this typed target as a stable human-readable label
    /// (`:endpoint "/charge"`, `:subject "events.x"`,
    /// `:slot "checkout/$order"`, or `(capability — no payload)` when
    /// the WIT world is a pure capability edge).
    ///
    /// Used by the [`AplicacaoSpec::validate`] duplicate-`:contratos`
    /// gate so the diagnostic names *which* identical edge was
    /// declared twice (not just which `(de, para, wit)` triple).
    /// Routes through the single 4-arm [`Self::payload_pair`] dispatch
    /// on the payload-carrying arms (`Some((field, payload)) →
    /// format!(":{field} {payload:?}")`) and through the lifted
    /// [`Self::CAPABILITY_LABEL`] const on the payload-less
    /// [`Self::Capability`] arm — so a future variant addition (the
    /// M4-and-later per-edge WIT registry may split [`Self::Http`]
    /// into `Rest` / `Grpc`, or extend [`Self::Store`] with a
    /// `Queue`-shaped peer) becomes a single new match-arm on
    /// [`Self::payload_pair`] rather than a rewrite of this template
    /// (and every downstream consumer that reaches for the label
    /// shape: the per-edge policy resolver in M4, the `feira app
    /// graph` view, the operator's mesh-graph audit). Until this
    /// lift landed the three payload arms carried three near-identical
    /// per-arm `format!(":{} {…:?}", …)` invocations, and the
    /// [`Self::Capability`] arm carried the payload-less byte-string
    /// twice (once inline here, once in the pin test) — closing the
    /// duplication trajectory the peer [`Self::HTTP_FIELD_NAME`] /
    /// [`Self::PUBSUB_FIELD_NAME`] / [`Self::STORE_FIELD_NAME`] (174e96a
    /// / 4a1e490) peer-const lifts already established for the
    /// payload-carrying arms.
    #[must_use]
    pub fn label(&self) -> String {
        match self.payload_pair() {
            Some((field, payload)) => format!(":{field} {payload:?}"),
            None => Self::CAPABILITY_LABEL.to_string(),
        }
    }
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

/// Canonical `(unit-suffix, seconds-per-window)` bijection every
/// consumer of the `:politicas :rate-limit` unit table reads from —
/// [`rate_limit_codec::parse`]'s `unit → Duration` dispatch,
/// [`rate_limit_codec::render`]'s `Duration → unit` projection, and the
/// [`is_canonical_rate_limit_window`] predicate the
/// [`AplicacaoSpec::validate_politicas`] gate keys off. Until this table
/// landed the `{"s" ↔ 1s, "m" ↔ 60s, "h" ↔ 3600s}` bijection was
/// scattered across four peer sites — the codec's `match unit` parse
/// arm, the codec's `if secs == 1 { "s" } else if …` render cascade,
/// and the predicate's `secs == 1 || secs == 60 || secs == 3600`
/// disjunction — each carrying its own hand-written copy of the same
/// three (str, u64) pairs with no compile-time link between them. A
/// future rate-limit-unit addition (a `"d"` day suffix, a `"ms"`
/// sub-second window once Envoy's `rate_limit_action` grows fractional
/// support) would have to be threaded through all three sites in
/// lockstep or a drift would silently split the accepted-window set: a
/// unit accepted by parse but unknown to render would round-trip
/// through the codec to the fallback `<n>/<k>s` shape (breaking the
/// THEORY.md §V.2.7 render-determinism contract every typed slot
/// carries), and a unit accepted by parse but unknown to the predicate
/// would silently pass validate and land at the renderer as a
/// non-canonical fallback.
///
/// Lifting the pairs to one `const` collapses the three call sites onto
/// one canonical projection each ([`rate_limit_window_unit`] for the
/// `Duration → unit` direction, [`rate_limit_window_from_unit`] for the
/// inverse), so a future unit addition is exactly one row appended
/// here — every consumer picks it up by construction. Same
/// "one canonical table, thin projections at each consumer" discipline
/// [`PlacementStrategy::as_str`] (cc8f749) applies on the sibling M3
/// typed-enum axis, and [`WitTarget::payload_pair`] (6788ed6) applies
/// on the peer typed-contract-payload axis.
const RATE_LIMIT_UNIT_TABLE: &[(&str, u64)] = &[("s", 1), ("m", 60), ("h", 3600)];

/// Canonical rate-limit unit suffix for `window`, or `None` when
/// `window` isn't one of the [`RATE_LIMIT_UNIT_TABLE`] entries (i.e.
/// carries a non-canonical magnitude the codec's round-trip would break
/// on). Reads the `Duration → unit` half of the lifted bijection so
/// [`rate_limit_codec::render`] and [`is_canonical_rate_limit_window`]
/// share the same projection — a future unit added to the table reaches
/// both consumers by construction.
#[must_use]
fn rate_limit_window_unit(window: Duration) -> Option<&'static str> {
    if window.subsec_nanos() != 0 {
        return None;
    }
    let secs = window.as_secs();
    RATE_LIMIT_UNIT_TABLE
        .iter()
        .find_map(|(unit, s)| (*s == secs).then_some(*unit))
}

/// Canonical rate-limit `Duration` for a unit suffix, or `None` when
/// the suffix isn't one of the [`RATE_LIMIT_UNIT_TABLE`] entries. Reads
/// the `unit → Duration` half of the lifted bijection so
/// [`rate_limit_codec::parse`] shares the same projection — a future
/// unit added to the table reaches parse by construction.
#[must_use]
fn rate_limit_window_from_unit(unit: &str) -> Option<Duration> {
    RATE_LIMIT_UNIT_TABLE
        .iter()
        .find_map(|(u, secs)| (*u == unit).then_some(Duration::from_secs(*secs)))
}

/// True when `window` is exactly one of the three canonical rate-limit
/// windows the [`rate_limit_codec`] round-trips losslessly: 1 second
/// (`"<n>/s"`), 1 minute (`"<n>/m"`), or 1 hour (`"<n>/h"`). Routes
/// through [`rate_limit_window_unit`] — the single `Duration → unit`
/// projection [`rate_limit_codec::render`] also consumes — so the
/// canonical-window set lives in one lifted [`RATE_LIMIT_UNIT_TABLE`]
/// entry per unit, drift between the codec's accepted unit set and the
/// validate gate's accepted window set is a build error visible at the
/// table, not a silent round-trip break at the codec layer. Same shape
/// every other predicate-on-the-typed-slot helper carries
/// ([`MeshPolicy::is_empty`], [`crate::LimitsSpec::is_empty`],
/// [`crate::BehaviorSpec::is_empty`]).
#[must_use]
fn is_canonical_rate_limit_window(window: Duration) -> bool {
    rate_limit_window_unit(window).is_some()
}

/// Upper-bound ceiling on the `:politicas :timeout` axis — every
/// validated [`MeshPolicy::timeout`] past
/// [`AplicacaoSpec::validate_politicas`] lies in `1ms..=POLICY_TIMEOUT_MAX`
/// (inclusive on both ends, integer-millisecond magnitudes by the
/// canonical-form gate immediately preceding).
///
/// The typed field is `Option<Duration>` (the zero-floor arm
/// [`AplicacaoError::PolicyTimeoutZero`] already rejects
/// `Duration::ZERO`, and the canonical-form arm
/// [`AplicacaoError::PolicyTimeoutNotCanonical`] already rejects
/// sub-millisecond residue), so a programmatic struct literal
/// (`MeshPolicy { timeout: Some(Duration::from_secs(86_400)), .. }` —
/// 24h) and the equivalent author-surface form
/// (`(:politicas (:timeout "24h"))` — the codec emits `"h"` for any
/// integer-hour magnitude) both round-trip cleanly through serde — a
/// structurally unbounded `Duration` ceiling. A `:timeout` value far
/// above the documented production-playbook band (Envoy default `15s`,
/// Istio per-route typical `≤ 30s`, AWS App Mesh `httpRouteTimeout`
/// schema typical `≤ 60s`, Linkerd `request_timeout` typical `10s`,
/// Kubernetes ingress-nginx `proxy_read_timeout` default `60s` capped
/// at `~3600s`) silently degenerates the mesh-policy contract: the
/// per-call deadline is structurally so long that no realistic
/// synchronous-`:contratos` traversal can reach it, so the typed slot
/// becomes a no-op carried on every emitted Envoy / Cilium L7 timeout
/// overlay — the MESH-COMPOSITION §V CSE invariant "no infinite
/// blocking" degenerates to a nominal-only contract on the
/// synchronous-call path. Pairs with the [`POLICY_RETRIES_MAX`] cap on
/// the sibling `:politicas :retries` axis and the
/// [`POLICY_BREAKER_MAX_FAILURES_MAX`] cap on the sibling
/// `:politicas :circuit-breaker :max-failures` axis — all three close
/// the "structurally unbounded ceiling on a typed `:politicas` axis"
/// footgun the prior zero-floor-and-canonical-form-only checks left
/// open.
///
/// The 1h (3600s = `3_600_000` ms) ceiling matches the largest unit the
/// shared duration codec emits (`"<n>h"` for any integer-hour
/// magnitude) — every value in the canonical authoring form's
/// `<integer><unit>` grammar at or below this cap renders to a clean
/// canonical string. The cap sits an order of magnitude above every
/// documented production-playbook recommendation band (Envoy default
/// `15s`, Istio production `≤ 30s`, Linkerd production `≤ 10s`, AWS
/// App Mesh production `≤ 60s`) and at the Kubernetes ingress-nginx
/// configured maximum (`proxy_read_timeout` typical max `3600s`),
/// below the clearly-pathological "effectively no timeout" floor
/// (`24h`, `7d`, `Duration::MAX`): a value the author can plausibly
/// want for a long-running synchronous workflow, but a hard wall above
/// which the mesh-level deadline is structurally a non-deadline.
/// Lifted as a typed `pub const` so the bound has exactly one source
/// of truth — the future M4 `mesh.pleme.io/v1alpha1/Aplicacao` CR
/// materializer's admission webhook and the caixa-mesh-side
/// `CiliumClusterwideEnvoyConfig` per-`:politicas` overlay
/// (MESH-COMPOSITION §III.2 #3) read from one place. Same shape every
/// other typed upper bound in this crate carries
/// ([`POLICY_RETRIES_MAX`], [`POLICY_BREAKER_MAX_FAILURES_MAX`],
/// [`crate::LIMITS_MEMORY_WASM32_MAX_BYTES`],
/// [`crate::render::DNS_1123_LABEL_MAX_LEN`],
/// [`crate::render::NATS_SUBJECT_MAX_LEN`]).
pub const POLICY_TIMEOUT_MAX: Duration = Duration::from_secs(3600);

/// Upper-bound ceiling on the `:politicas :retries` axis — every
/// validated [`MeshPolicy::retries`] past
/// [`AplicacaoSpec::validate_politicas`] lies in `1..=POLICY_RETRIES_MAX`.
///
/// The typed slot is `Option<u32>` (`None` = no retries on transient
/// failure; `Some(0)` already rejected by the
/// [`AplicacaoError::PolicyRetriesZero`] zero-floor arm), so a
/// programmatic struct literal (`MeshPolicy { retries: Some(100_000),
/// .. }`) and the equivalent author-surface form
/// (`(:politicas (:retries 100000))`) both round-trip cleanly through
/// serde / the codec — a structurally unbounded `u32` ceiling. The
/// runtime substrate that consumes the value (Envoy's
/// `retry_policy.num_retries`, the `CiliumClusterwideEnvoyConfig`
/// per-`:politicas` overlay MESH-COMPOSITION §III.2 #3 names, AWS
/// App Mesh's `gRPCRouteRetryPolicy.maxRetries` whose schema-side
/// admission cap is 10) translates a four-billion-retry policy into a
/// thundering-herd amplification vector on transient failure — the
/// caller's one request fans out to `retries` server-side calls per
/// edge per traversal, multiplying load by `(retries+1)^depth` across
/// the synchronous-`:contratos` subgraph. The MESH-COMPOSITION §V CSE
/// invariant "no infinite blocking" pairs with a no-runaway-amplification
/// invariant on the retry axis; both belong at the typed-slot layer.
///
/// The `10` ceiling matches AWS App Mesh's explicit hard cap (the only
/// upstream mesh-policy schema that documents one) and sits above the
/// Envoy / Istio practical-recommendation band (`num_retries ≤ 5` in
/// every documented production playbook): a value the author can
/// plausibly want, but a hard wall above which the policy is
/// structurally a footgun. Lifted as a typed `pub const` so the bound
/// has exactly one source of truth — a future axis reaching for the
/// same value (the M4 `mesh.pleme.io/v1alpha1/Aplicacao` CR
/// materializer's admission webhook, the caixa-mesh-side
/// `CiliumClusterwideEnvoyConfig` overlay's per-edge cap) reads from
/// one place. Same shape every other typed upper bound in this crate
/// carries ([`crate::LIMITS_MEMORY_WASM32_MAX_BYTES`],
/// [`crate::render::DNS_1123_LABEL_MAX_LEN`],
/// [`crate::render::GATEWAY_API_HTTP_PATH_MAX_LEN`],
/// [`crate::render::NATS_SUBJECT_MAX_LEN`]).
pub const POLICY_RETRIES_MAX: u32 = 10;

/// Upper-bound ceiling on the `:politicas :circuit-breaker :max-failures`
/// axis — every validated [`CircuitBreaker::max_failures`] past
/// [`AplicacaoSpec::validate_politicas`] lies in
/// `1..=POLICY_BREAKER_MAX_FAILURES_MAX`.
///
/// The typed field is `u32` (the zero-floor arm
/// [`AplicacaoError::PolicyBreakerZeroFailures`] already rejects
/// `0` — a breaker that trips on the first call), so a programmatic
/// struct literal (`CircuitBreaker { max_failures: u32::MAX, .. }`)
/// and the equivalent author-surface form
/// (`(:circuit-breaker (:max-failures 4294967295))`) both round-trip
/// cleanly through serde — a structurally unbounded `u32` ceiling. A
/// `max_failures` value far above the documented production-playbook
/// band (Hystrix `circuitBreaker.requestVolumeThreshold` default 20,
/// Istio `outlierDetection.consecutive5xxErrors` default 5, Envoy
/// `outlier_detection.consecutive_5xx` default 5, Polly / Resilience4j
/// typical 5–50) silently disables the breaker's protection role:
/// the threshold is structurally so high that no realistic
/// failures-per-`:window` traffic shape can reach it, so the breaker
/// never trips and the typed slot becomes a no-op carried on every
/// emitted Envoy / Cilium L7 overlay. Pairs with the
/// [`POLICY_RETRIES_MAX`] cap on the sibling `:politicas :retries`
/// axis — both close the "structurally unbounded `u32` ceiling on a
/// typed policy axis" footgun the prior zero-floor-only checks left
/// open.
///
/// The `1000` ceiling sits an order of magnitude above every
/// documented upstream production-playbook recommendation band (the
/// highest is Hystrix's 20-default `requestVolumeThreshold`, the
/// Istio / Envoy / Polly / Resilience4j ones all sit ≤ 50) and below
/// the clearly-pathological "effectively no protection"
/// floor (`10_000`, `100_000`, `u32::MAX`): a value the author can
/// plausibly want at hyperscale, but a hard wall above which the
/// policy is structurally a no-op. Lifted as a typed `pub const` so
/// the bound has exactly one source of truth — the future M4
/// `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's admission
/// webhook and the caixa-mesh-side `CiliumClusterwideEnvoyConfig`
/// per-`:politicas` overlay (MESH-COMPOSITION §III.2 #3) read from
/// one place. Same shape every other typed upper bound in this crate
/// carries ([`POLICY_RETRIES_MAX`],
/// [`crate::LIMITS_MEMORY_WASM32_MAX_BYTES`],
/// [`crate::render::DNS_1123_LABEL_MAX_LEN`],
/// [`crate::render::NATS_SUBJECT_MAX_LEN`]).
pub const POLICY_BREAKER_MAX_FAILURES_MAX: u32 = 1000;

/// Upper-bound ceiling on the `:politicas :circuit-breaker :window` axis —
/// every validated [`CircuitBreaker::window`] past
/// [`AplicacaoSpec::validate_politicas`] lies in
/// `1ms..=POLICY_BREAKER_WINDOW_MAX` (inclusive on both ends,
/// integer-millisecond magnitudes by the canonical-form gate
/// immediately preceding).
///
/// The typed field is `Duration` (the zero-floor arm
/// [`AplicacaoError::PolicyBreakerZeroWindow`] already rejects
/// `Duration::ZERO`, and the canonical-form arm
/// [`AplicacaoError::PolicyBreakerWindowNotCanonical`] already rejects
/// sub-millisecond residue), so a programmatic struct literal
/// (`CircuitBreaker { window: Duration::from_secs(86_400), .. }` — 24h)
/// and the equivalent author-surface form
/// (`(:circuit-breaker (:window "24h"))` — the codec emits `"h"` for any
/// integer-hour magnitude) both round-trip cleanly through serde — a
/// structurally unbounded `Duration` ceiling. A `:window` value far
/// above the documented production-playbook band (Hystrix
/// `metrics.rollingStats.timeInMilliseconds` default `10s`,
/// resilience4j `slidingWindowSize` time-based typical `10s..=60s`,
/// Istio `outlierDetection.interval` default `10s`, Envoy
/// `outlier_detection.interval` default `10s`, AWS App Mesh
/// circuit-breaker time-window typical `30s..=300s`) degenerates the
/// breaker's role: a rolling-window failure counter whose window is
/// hours long is operationally a lifetime counter, the breaker's
/// "recent failures" memory is structurally so long that transient
/// failures are never forgotten, and the typed slot becomes a no-op
/// trigger that trips once and stays tripped for the lifetime of the
/// component carried on every emitted Envoy / Cilium L7 overlay.
///
/// The 1h (3600s = `3_600_000` ms) ceiling matches the largest unit the
/// shared duration codec emits (`"<n>h"` for any integer-hour
/// magnitude) — every value in the canonical authoring form's
/// `<integer><unit>` grammar at or below this cap renders to a clean
/// canonical string — and matches the sibling [`POLICY_TIMEOUT_MAX`]
/// cap on the first typed-`Duration` `:politicas` axis: the two
/// duration-typed `:politicas` axes now share a single uniform top
/// edge so the next typed-slot wiring (the future caixa-mesh
/// `CiliumClusterwideEnvoyConfig` per-`:politicas` overlay, the M4
/// `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's per-policy
/// admission webhook) reaches for either field knowing the value is
/// in `1ms..=1h` without re-validating at the renderer layer. The cap
/// sits two orders of magnitude above every documented upstream
/// production-playbook recommendation band (Hystrix / resilience4j /
/// Istio / Envoy all default to 10s; AWS App Mesh maxes out at ~5m)
/// and below the clearly-pathological "rolling window degenerates to
/// lifetime counter" floor (`24h`, `7d`, `Duration::MAX`): a value the
/// author can plausibly want for a very-low-traffic long-tail
/// failure-detection window, but a hard wall above which the breaker's
/// rolling-window contract is structurally a lifetime-counter contract.
/// Lifted as a typed `pub const` so the bound has exactly one source
/// of truth — the future M4 `mesh.pleme.io/v1alpha1/Aplicacao` CR
/// materializer's admission webhook and the caixa-mesh-side
/// `CiliumClusterwideEnvoyConfig` per-`:politicas` overlay
/// (MESH-COMPOSITION §III.2 #3) read from one place. Same shape every
/// other typed upper bound in this crate carries
/// ([`POLICY_TIMEOUT_MAX`], [`POLICY_RETRIES_MAX`],
/// [`POLICY_BREAKER_MAX_FAILURES_MAX`],
/// [`crate::LIMITS_MEMORY_WASM32_MAX_BYTES`],
/// [`crate::render::DNS_1123_LABEL_MAX_LEN`],
/// [`crate::render::NATS_SUBJECT_MAX_LEN`]).
pub const POLICY_BREAKER_WINDOW_MAX: Duration = Duration::from_secs(3600);

/// Upper-bound ceiling on the `:politicas :rate-limit` rate axis —
/// every validated [`RateLimit::rate`] past
/// [`AplicacaoSpec::validate_politicas`] lies in
/// `1..=POLICY_RATE_LIMIT_MAX`.
///
/// The typed field is `u32` (the zero-floor arm
/// [`AplicacaoError::PolicyRateLimitZero`] already rejects `0` — a
/// zero-rate limit denies every request, the canonical "I forgot
/// that 0 means deny-everything" footgun), so a programmatic struct
/// literal (`RateLimit { rate: u32::MAX, window: Duration::from_secs(1) }`)
/// and the equivalent author-surface form (`(:rate-limit "4294967295/s")`
/// — the `rate_limit_codec` parses any `u32`-shaped magnitude) both
/// round-trip cleanly through serde — a structurally unbounded `u32`
/// ceiling. The runtime substrate consuming the value (Envoy's
/// `local_rate_limit.token_bucket.max_tokens`, the future
/// `CiliumClusterwideEnvoyConfig` per-`:politicas` overlay
/// MESH-COMPOSITION §III.2 #3 names) translates a four-billion-token
/// rate-limit into a no-op rate-limiter: the bucket capacity is
/// structurally so high no realistic per-edge traffic shape can
/// drain it, the limiter never trips, and the typed slot becomes a
/// "rate-limit declared, no enforcement" footgun — the canonical
/// declared-but-inert shape every other `:politicas` cap arm
/// closes ([`POLICY_RETRIES_MAX`] thundering-herd amplification,
/// [`POLICY_BREAKER_MAX_FAILURES_MAX`] no-op-breaker, etc.).
///
/// The `1_000_000` (1M) ceiling sits two-to-three orders of magnitude
/// above every documented upstream production-playbook recommendation
/// band (Envoy `local_rate_limit` typical `10..=10_000` RPS, Istio
/// `RateLimitFilter` typical `10..=10_000` RPS, Cloudflare WAF
/// rate-rule Free / Pro `10_000` req/min, AWS API Gateway account
/// default `10_000` RPS, Kong typical `100..=10_000`, NGINX
/// `limit_req_zone` typical `1..=1_000` RPS) and below the
/// clearly-pathological "paste-from-binary blob" floor (`100_000_000`,
/// `u32::MAX`): a value the author can plausibly want at hyperscale
/// (Cloudflare Enterprise rate-plans run to ~6M/min ≈ 1M/h on the
/// /h-window arm), but a hard wall above which the policy is
/// structurally a no-op carried verbatim on every emitted Envoy /
/// Cilium L7 overlay. The cap brackets all three canonical windows
/// the [`rate_limit_codec`] accepts: at `1M/s` (absurd hyperscale
/// ceiling, ~1M RPS per edge), at `1M/m` (~16.7k RPS, the
/// hyperscale-tier WAF band), at `1M/h` (~277 RPS, the common
/// per-endpoint API band). Lifted as a typed `pub const` so the bound
/// has exactly one source of truth — the future M4
/// `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's admission
/// webhook and the caixa-mesh-side `CiliumClusterwideEnvoyConfig`
/// per-`:politicas` overlay (MESH-COMPOSITION §III.2 #3) read from
/// one place. Same shape every other typed upper bound in this crate
/// carries ([`POLICY_TIMEOUT_MAX`], [`POLICY_RETRIES_MAX`],
/// [`POLICY_BREAKER_MAX_FAILURES_MAX`], [`POLICY_BREAKER_WINDOW_MAX`],
/// [`crate::LIMITS_MEMORY_WASM32_MAX_BYTES`],
/// [`crate::LIMITS_WALL_CLOCK_MAX`],
/// [`crate::render::DNS_1123_LABEL_MAX_LEN`],
/// [`crate::render::NATS_SUBJECT_MAX_LEN`]).
pub const POLICY_RATE_LIMIT_MAX: u32 = 1_000_000;

// `:entrada :host` total-length and per-label cap axes route through
// the lifted [`crate::render::GATEWAY_API_HOSTNAME_MAX_LEN`] (253) and
// [`crate::render::DNS_1123_LABEL_MAX_LEN`] (63) canonical bounds. The
// pair of aplicacao-private aliases the previous `validate_entrada_host`
// arms consumed (`ENTRADA_HOST_MAX_LEN = 253`, `ENTRADA_HOST_LABEL_MAX_LEN
// = 63`) were structurally the same K8s Gateway API v1 Hostname
// admission-schema bounds — the total-length cap on the OpenAPI
// `Hostname` type and the per-`.`-separated-label DNS-1123 cap on the
// same regex — that the peer axes at the caixa-core::render level pin,
// so hoisting both readers onto the shared lifted constants closes the
// third-occurrence duplication threshold structurally: the M4
// `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's per-host / per-
// label validator, the future per-`Certificate` SAN emitter, and every
// other per-Gateway-API-Hostname landing site reach the same one place
// as the `:entrada :host` gate does — no per-axis alias drift surface
// between them, by construction.

/// Max byte length for an Akka-cluster-sharding `:placement :shard-key`
/// extractor expression — the upper bound `validate_placement_shard_key`
/// enforces on every well-shaped shard-key past validate. The realistic
/// shard-key forms in the wild (`tenantId`, `customerId`, `$tenantId`,
/// `metadata.tenantId`, `${tenant}`, `$.user.id`) all sit well under 64
/// bytes; the 63-byte cap mirrors the DNS-1123 label cap on the peer
/// `:placement :affinity` / `:placement :clusters` identifier-shaped
/// axes and surfaces the canonical "paste-from-doc multi-line blob landed
/// in `:shard-key`" footgun at validate time rather than at the future
/// M4 Akka-style cluster-sharding reconciler's hash-extractor pass.
const PLACEMENT_SHARD_KEY_MAX_LEN: usize = 63;

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
    // footgun. The shared
    // [`crate::render::require_valid_dns_1123_label`] helper brackets
    // the empty-first + shape cascade every peer name axis
    // (`:placement :clusters`, `:placement :affinity`, `:contratos
    // :de`/`:para`, `:entrada :para`, `:children :caixa`, `:nome`,
    // `:upgrade-from :module`) routes through, so drift between the
    // eight axes' accepted DNS-1123-label sets is structurally
    // impossible.
    crate::render::require_valid_dns_1123_label(
        caixa,
        || AplicacaoError::MembroCaixaEmpty,
        |reason| AplicacaoError::MembroCaixaInvalid {
            caixa: caixa.to_string(),
            reason,
        },
    )
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
    // without an empty-check footgun. Routes through the shared
    // [`crate::render::require_valid_dns_1123_label`] gate the peer
    // name axes each land on.
    crate::render::require_valid_dns_1123_label(
        cluster,
        || AplicacaoError::PlacementClusterEmpty,
        |reason| AplicacaoError::PlacementClusterInvalid {
            cluster: cluster.to_string(),
            reason,
        },
    )
}

/// Reject `:placement :affinity` hints whose shape can never legitimately
/// land in any downstream selector or label-keyed routing axis. Thin
/// wrapper around [`crate::render::is_dns_1123_label`] that maps the
/// shared parser-shaped reason into the
/// [`AplicacaoError::PlacementAffinityInvalid`] variant, so the
/// diagnostic is self-locating (the offending `:affinity` is named
/// verbatim) and the author can grep their caixa.lisp for
/// `:affinity "<hint>"` and fix it in one edit.
///
/// The `:affinity` slot carries a placement-engine hint — canonical
/// examples in the M3 surface are `"data-locality"`, `"low-latency"`,
/// `"anti-affinity"` — that flows verbatim into the M3 Adaptive
/// compression overlay and the future M4 placement-engine's per-hint
/// routing axis. Each downstream consumer (caixa-mesh's
/// `placement.affinity` overlay at caixa-mesh/src/lib.rs:126, the
/// future M4 `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's
/// `spec.placement.affinity` admission rule, the future M4 per-hint
/// node-affinity / pod-affinity rule generator keying off the same
/// value as a K8s `app.pleme.io/affinity-hint=<value>` label
/// selector) requires the value to be a DNS-1123 label — K8s label
/// values are bounded by `[a-z0-9A-Z_.-]{,63}` with a stricter
/// `[a-z0-9]([-a-z0-9]*[a-z0-9])?` floor in every identity-keyed
/// admission rule the apiserver enforces.
///
/// Until this gate landed an `:affinity "DataLocality"` (the canonical
/// TitleCase-from-an-ADR typo), `:affinity "data_locality"` (the
/// Python-module-name leak), `:affinity "data.locality"` (the
/// namespace-dot-on-a-label confusion), `:affinity "-data-locality"` /
/// `:affinity "data-locality-"` (boundary-hyphen violation),
/// `:affinity "data locality"` (paste-from-doc whitespace),
/// `:affinity "data-localité"` (un-Punycode-encoded IDN), or the
/// 64-byte over-cap slug silently passed the empty-only check and the
/// failure surfaced as a no-match at the M3 Adaptive compression
/// overlay's filter time (`placement.affinity` carried a malformed
/// value, no node matched, the workload landed on the default
/// heuristic) — the canonical "declared-but-inert" footgun mirroring
/// the empty-:affinity / empty-shard-key / zero-:politicas /
/// empty-:contratos-target gates already close on every other
/// declare-but-no-opinion axis. Lifting the rejection to a build-time
/// gate closes the fifth typed slot on the Aplicacao surface to land
/// on the canonical DNS-1123 label floor (after the four Servico-name
/// reference axes: `:membros :caixa` 3f9d7a0, `:placement :clusters`
/// 6c8c00b, `:contratos :de`/`:para` 8d5af6b, `:entrada :para`
/// b0e8748).
///
/// Same diagnostic shape as [`AplicacaoError::PlacementClusterInvalid`]
/// (6c8c00b) on the sibling `:placement :clusters` axis — both axes'
/// validated values are guaranteed-accepted by the apiserver without
/// re-validation at any downstream renderer or admission layer.
fn validate_placement_affinity(affinity: &str) -> Result<(), AplicacaoError> {
    // Empty is gated separately at the call site for a self-locating
    // diagnostic; re-checking here keeps the predicate usable from any
    // future call site (the M4 CR materializer's per-affinity
    // validator) without an empty-check footgun. Routes through the
    // shared [`crate::render::require_valid_dns_1123_label`] gate the
    // peer name axes each land on.
    crate::render::require_valid_dns_1123_label(
        affinity,
        || AplicacaoError::PlacementAffinityEmpty,
        |reason| AplicacaoError::PlacementAffinityInvalid {
            affinity: affinity.to_string(),
            reason,
        },
    )
}

/// Reject `:placement :shard-key` extractor expressions whose shape can
/// never legitimately drive the future M4 Akka-style cluster-sharding
/// reconciler's hash-extractor pass. Maps the per-byte / length checks
/// into the [`AplicacaoError::ShardKeyInvalid`] variant, so the
/// diagnostic is self-locating (the offending `:shard-key` value is
/// named verbatim alongside the parser-shaped reason) and the author can
/// grep their caixa.lisp for `:shard-key "<expr>"` and fix it in one
/// edit.
///
/// The `:shard-key` slot is the Akka-cluster-sharding `ExtractEntityId`
/// axis (MESH-COMPOSITION §II.4) — a single-token entity-id extractor
/// expression naming the message property to hash on. The realistic
/// shapes in the wild (`tenantId` / `customerId` / `userId` — bare
/// property name; `$tenantId` — Akka entity-id placeholder;
/// `metadata.tenantId` / `$.user.id` — JSONPath-style nested reference;
/// `${tenant}` — interpolation-style template) all sit in the printable
/// ASCII subset; the realistic *non-shapes* (a paste-from-doc
/// multi-line blob landing in `:shard-key`, an embedded space from a
/// paste-from-aligned-doc, a trailing newline from a paste-from-shell
/// heredoc, a non-ASCII byte from a paste-from-Unicode-doc, the
/// `:shard-key "tenant Id"` typo) silently passed the prior empty-only
/// check and the failure surfaces at the future M4 reconciler's hash
/// pass as a runtime extractor-evaluation error far from the source
/// `caixa.lisp`, with no field naming which member's `:shard-key`
/// carried the offending value.
///
/// The contract — the printable ASCII single-token intersection-floor
/// every Akka-style entity-id extractor implementation admits:
///
///   - 1..=[`PLACEMENT_SHARD_KEY_MAX_LEN`] (63) bytes — same cap as the
///     peer DNS-1123-label-shaped `:placement :affinity` /
///     `:placement :clusters` identifier axes; realistic shard-keys sit
///     well under 32 bytes, the cap surfaces paste-from-doc multi-line
///     blob footguns at validate time;
///   - every byte in the printable ASCII range `0x21..=0x7E` —
///     rejects whitespace (space, tab, CR, LF — `"$tenant Id"` /
///     `"$tenantId\n"` from paste-from-aligned-doc /
///     paste-from-shell-heredoc), control characters (`\x00..\x1F`,
///     `\x7F` — the canonical "embedded null from a copy-paste-binary
///     footgun"), and non-ASCII bytes (`"$tenàntId"` —
///     un-Punycode-encoded IDN that round-trips inconsistently across
///     NFC/NFD normalization).
///
/// The accepted set is broader than the DNS-1123 label floor the peer
/// `:placement :clusters` / `:placement :affinity` axes use because the
/// `:shard-key` value is not a K8s `metadata.name` / label-selector
/// landing site; it's an extractor expression the future Akka-style
/// reconciler reads as a property reference. The realistic forms
/// (`$tenantId`, `metadata.tenantId`, `${tenant}`, `$.user.id`) carry
/// `$` / `.` / `{` / `}` characters that the DNS-1123 grammar forbids
/// but every Akka-style entity-id extractor parses. The
/// printable-ASCII-token floor accepts every shape any such extractor
/// would accept while rejecting the cross-implementation footguns
/// (whitespace breaks token boundaries; non-ASCII round-trips
/// inconsistently across YAML emitters and NFC/NFD normalization;
/// control characters silently corrupt the next read).
///
/// Until this gate landed `validate_placement` only refused the
/// `Some("")` empty arm via [`AplicacaoError::ShardedKeyEmpty`]; a
/// structurally invalid `:shard-key` (`":shard-key \" $tenantId\""` —
/// leading space from paste-from-aligned-doc, `":shard-key \"$tenant
/// Id\""` — embedded space, `":shard-key \"$tenantId\\n\""` — trailing
/// newline from paste-from-shell-heredoc, `":shard-key \"$tenàntId\""`
/// — un-Punycode-encoded IDN, `":shard-key \"$tenantId\\x01\""` —
/// control character from paste-from-binary, the 64-byte over-cap
/// paste-from-doc multi-line slug) silently passed validate. The future
/// M4 Akka-style cluster-sharding reconciler's hash-extractor pass
/// would then surface the malformed value either as a runtime
/// extractor-evaluation error (whitespace breaks the extractor's token
/// boundary, no match) or as a silently-different shard assignment
/// across YAML emitters (non-ASCII normalizes differently between the
/// caixa-mesh-side YAML emitter and the in-cluster reconciler's YAML
/// parser, the same entity ID maps to two distinct shards on a
/// re-render). Lifting the shape gate to caixa-build time makes the
/// extractor-floor invariant a structural property of every validated
/// `Placement`: every `Sharded` placement past `validate_placement` has
/// a `:shard-key` the future M4 reconciler can hash without
/// re-validating at the runtime layer.
///
/// Mirrors the [`AplicacaoError::ContratoSlotInvalid`] /
/// [`AplicacaoError::ContratoSubjectInvalid`] /
/// [`AplicacaoError::ContratoEndpointInvalid`] payload-axis shape gates
/// on the peer `:contratos` payload axes — each lifts the
/// runtime-side parser's intersection-floor to a caixa-build-time gate,
/// closing the canonical "this passed validate but the runtime parser
/// rejected it" surprise.
fn validate_placement_shard_key(key: &str) -> Result<(), AplicacaoError> {
    // Empty is gated separately at the call site via the more
    // self-locating [`AplicacaoError::ShardedKeyEmpty`] diagnostic;
    // re-checking here keeps the predicate usable from any future call
    // site (the M4 CR materializer's per-shard-key validator) without
    // an empty-check footgun.
    if key.is_empty() {
        return Err(AplicacaoError::ShardedKeyEmpty);
    }
    if key.len() > PLACEMENT_SHARD_KEY_MAX_LEN {
        return Err(AplicacaoError::ShardKeyInvalid {
            shard_key: key.to_string(),
            reason: format!(
                "exceeds :shard-key max length of {PLACEMENT_SHARD_KEY_MAX_LEN} bytes \
                 (got {} bytes; realistic Akka-style entity-id extractor expressions \
                 — `tenantId`, `$tenantId`, `metadata.tenantId`, `${{tenant}}` — sit \
                 well under 32 bytes, this length suggests a paste-from-doc \
                 multi-line blob landed in `:shard-key` instead of a single-token \
                 extractor expression)",
                key.len()
            ),
        });
    }
    for &b in key.as_bytes() {
        if (0x21..=0x7E).contains(&b) {
            continue;
        }
        let reason = if b == b' ' {
            "contains a space (Akka-style entity-id extractor expressions are \
             single-token references like `tenantId` / `$tenantId` / `metadata.tenantId`; \
             whitespace breaks the extractor's token boundary at the runtime layer, \
             and the paste-from-aligned-doc / paste-from-CSV footgun silently lands \
             a multi-token blob in one `:shard-key` slot)"
                .to_string()
        } else if b == b'\t' {
            "contains a tab character (paste-from-aligned-doc footgun; the \
             Akka-style entity-id extractor reads `:shard-key` as a single-token \
             reference, embedded whitespace breaks the token boundary at the \
             runtime hash-extractor pass)"
                .to_string()
        } else if b == b'\n' || b == b'\r' {
            format!(
                "contains line terminator 0x{b:02x} (paste-from-shell-heredoc / \
                 paste-from-multiline-doc footgun; the Akka-style entity-id \
                 extractor reads `:shard-key` as a single-token reference, embedded \
                 newlines either truncate the value at the YAML emitter layer or \
                 break the token boundary at the runtime hash-extractor pass)"
            )
        } else if b < 0x20 || b == 0x7F {
            format!(
                "contains control character 0x{b:02x} (the canonical \
                 paste-from-binary / paste-from-screen-cleared-terminal footgun; \
                 control characters silently corrupt round-trip serialization \
                 across YAML emitters and break the runtime hash-extractor's \
                 single-token parser)"
            )
        } else {
            format!(
                "contains non-ASCII byte 0x{b:02x} (the canonical \
                 paste-from-Unicode-doc footgun; non-ASCII bytes round-trip \
                 inconsistently across NFC/NFD normalization on APFS / ext4 / \
                 across YAML emitter implementations — the same entity ID can \
                 silently map to two distinct shards on a re-render. Use a \
                 printable-ASCII extractor expression like `tenantId`, \
                 `$tenantId`, or `metadata.tenantId`)"
            )
        };
        return Err(AplicacaoError::ShardKeyInvalid {
            shard_key: key.to_string(),
            reason,
        });
    }
    Ok(())
}

/// Reject `:contratos :de` / `:contratos :para` values whose shape
/// can never legitimately match a validated `:membros :caixa`. Thin
/// wrapper around [`crate::render::is_dns_1123_label`] that maps the
/// shared parser-shaped reason into the
/// [`AplicacaoError::ContratoCaixaInvalid`] variant, so the per-edge
/// diagnostic is self-locating (which slot — `:de` or `:para` — and
/// the offending value verbatim) and the author can grep their
/// caixa.lisp for `:de "<name>"` / `:para "<name>"` and fix it in
/// one edit.
///
/// Until this gate landed an empty or DNS-1123-malformed `:de` /
/// `:para` (`:de ""`, `:de "Cart"` the canonical TitleCase-from-an-ADR
/// typo, `:de "my_cart"` the Python-module-name leak, `:de "team.cart"`
/// the namespace-dot-on-a-label confusion, `:de "-cart"` / `:de "cart-"`
/// the boundary-hyphen violation, the 64-byte over-cap slug, `:de "café"`
/// un-Punycode-encoded IDN) silently passed the per-axis check and
/// surfaced as [`AplicacaoError::ContratoMemberMissing`] at the
/// membership lookup — diagnostic-framed as "this caixa is not in
/// `:membros`" when the root cause is "this `:de` value is not a
/// well-shaped Servico-name identifier and could never legitimately
/// match any validated member". Because every `:membros :caixa` is
/// shape-validated through [`validate_membro_caixa`] (3f9d7a0), the
/// `names` HashSet structurally never contains an empty / malformed
/// string, so the membership lookup arm misframes every empty /
/// malformed input. Lifting the shape arm ahead of the lookup
/// preserves the legitimate `ContratoMemberMissing` arm (a
/// well-shaped `:de` that simply isn't in `:membros` — a phantom
/// reference) while routing every structurally-impossible-to-match
/// input through the narrower self-locating shape diagnostic.
///
/// Same diagnostic shape as [`AplicacaoError::MembroCaixaInvalid`]
/// (3f9d7a0) and [`AplicacaoError::PlacementClusterInvalid`]
/// (6c8c00b) — the third Aplicacao-level Servico-name reference axis
/// to land on the canonical [`crate::render::is_dns_1123_label`]
/// floor. The `slot: &'static str` field carries the kebab-case
/// `:de` / `:para` tag verbatim, mirroring [`BehaviorSpec::validate`]'s
/// per-callback-slot diagnostic shape and the
/// [`ManifestError::CodePathDuplicate`] (e113ace) / [`DepError::DepIsSelf`]
/// (85f102c) cross-list-tag pattern.
fn validate_contrato_caixa(slot: &'static str, caixa: &str) -> Result<(), AplicacaoError> {
    // Routes through the shared
    // [`crate::render::require_valid_dns_1123_label`] gate the peer
    // name axes each land on. The `slot: &'static str` field flows
    // through both error variants so the diagnostic names which
    // per-edge axis (`:de` vs `:para`) the offending value came from.
    crate::render::require_valid_dns_1123_label(
        caixa,
        || AplicacaoError::ContratoCaixaEmpty { slot },
        |reason| AplicacaoError::ContratoCaixaInvalid {
            slot,
            caixa: caixa.to_string(),
            reason,
        },
    )
}

/// Reject `:entrada :para` values whose shape can never legitimately
/// match a validated `:membros :caixa`. Thin wrapper around
/// [`crate::render::is_dns_1123_label`] that maps the shared parser-
/// shaped reason into the [`AplicacaoError::EntradaParaInvalid`]
/// variant, so the diagnostic is self-locating (the offending
/// `:entrada :para` value is named verbatim) and the author can grep
/// their caixa.lisp for `:para "<name>"` and fix it in one edit.
///
/// Until this gate landed an empty or DNS-1123-malformed `:entrada
/// :para` (`:para ""`, `:para "Cart"` the canonical TitleCase-from-an-
/// ADR typo, `:para "my_cart"` the Python-module-name leak,
/// `:para "team.cart"` the namespace-dot-on-a-label confusion,
/// `:para "-cart"` / `:para "cart-"` the boundary-hyphen violation,
/// the 64-byte over-cap slug, `:para "café"` un-Punycode-encoded IDN)
/// silently passed the per-axis check and surfaced as
/// [`AplicacaoError::EntradaMemberMissing`] at the membership lookup
/// — diagnostic-framed as "this caixa is not in `:membros`" when the
/// root cause is "this `:entrada :para` value is not a well-shaped
/// Servico-name identifier and could never legitimately match any
/// validated member". Because every `:membros :caixa` is shape-
/// validated through [`validate_membro_caixa`] (3f9d7a0), the `names`
/// `HashSet` structurally never contains an empty / malformed string,
/// so the membership lookup arm misframes every empty / malformed
/// input. Lifting the shape arm ahead of the lookup preserves the
/// legitimate `EntradaMemberMissing` arm (a well-shaped `:para` that
/// simply isn't in `:membros` — a phantom reference) while routing
/// every structurally-impossible-to-match input through the narrower
/// self-locating shape diagnostic.
///
/// Same diagnostic shape as [`AplicacaoError::MembroCaixaInvalid`]
/// (3f9d7a0), [`AplicacaoError::PlacementClusterInvalid`] (6c8c00b),
/// and [`AplicacaoError::ContratoCaixaInvalid`] (8d5af6b) — the
/// fourth and last Aplicacao-level Servico-name reference axis to
/// land on the canonical [`crate::render::is_dns_1123_label`] floor.
/// No `slot: &'static str` field because there is only one axis
/// (`:entrada :para`), unlike the dual-axis `:contratos :de`/`:para`;
/// the simpler shape mirrors [`validate_membro_caixa`] and
/// [`validate_placement_cluster`].
fn validate_entrada_para(para: &str) -> Result<(), AplicacaoError> {
    // Empty is gated separately at the call site for a self-locating
    // diagnostic; re-checking here keeps the predicate usable from any
    // future call site (the M4 CR materializer's per-`:entrada`
    // validator) without an empty-check footgun. Routes through the
    // shared [`crate::render::require_valid_dns_1123_label`] gate the
    // peer name axes each land on.
    crate::render::require_valid_dns_1123_label(
        para,
        || AplicacaoError::EntradaParaEmpty,
        |reason| AplicacaoError::EntradaParaInvalid {
            para: para.to_string(),
            reason,
        },
    )
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
/// [`WitTarget::label`] (previously the free `contrato_target_label`
/// helper, 5dbcfaf; lifted onto the typed [`WitTarget`] enum so the
/// per-variant label match is compiler-checked-exhaustive).
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
    if host.len() > crate::render::GATEWAY_API_HOSTNAME_MAX_LEN {
        return Err(AplicacaoError::EntradaHostInvalid {
            host: host.to_string(),
            reason: format!(
                "exceeds Gateway API v1 Hostname max length of {cap} bytes \
                 (got {} bytes; the K8s apiserver rejects longer hostnames at admission time)",
                host.len(),
                cap = crate::render::GATEWAY_API_HOSTNAME_MAX_LEN,
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
    // After the `://` scheme-prefix and `/` path arms have ruled out the
    // two `:`-bearing shapes the Gateway API actively rejects with
    // location-shaped diagnostics, any remaining `:` in the host body is
    // either the canonical "I put the port in the `:host` slot"
    // authoring footgun (`"checkout.quero.cloud:8080"` — the `:port`
    // slot lives one axis away on the same `:entrada` block) or an
    // unbracketed IPv6 literal (`"2001:db8::1"`) which Gateway API v1
    // Hostname forbids identically to the IPv4-literal arm below. Both
    // shapes silently fell through the `://` and `/` arms before this
    // lift and surfaced as a deep `label "<rest>:<port>" contains
    // invalid character ':'` diagnostic from the per-byte loop near the
    // bottom of this predicate, which named the offending byte but not
    // the canonical authoring fix — for the port case the author has to
    // know the `:entrada` block carries a separate `:port u16` slot
    // (`caixa-core/src/aplicacao.rs:1667`, `default_port = 8080`) and
    // move the value over; for the IPv6 case the author has to know
    // Gateway API v1 forbids IP literals across the board. The contract
    // doc-comment above already promises "no port (`:8080`)" verbatim
    // in the rejected-shape enumeration but the predicate's
    // implementation refused the `:` only as a side-effect of the
    // per-label `[a-z0-9-]` character-class loop; this arm brings the
    // implementation in line with the documented contract by surfacing
    // the canonical fix at the top-level shape gate, peer with how the
    // `://` arm names the scheme prefix and the `/` arm names the
    // `:entrada :paths` axis. Same compounding trajectory the recent
    // `is_gateway_api_http_path` (6a17961) per-byte tightening followed
    // — the typed slot's rejected set matches the apiserver's rejected
    // set, structurally, with a self-locating diagnostic at the
    // offending axis instead of a deep parser-shape leak.
    if host.contains(':') {
        return Err(AplicacaoError::EntradaHostInvalid {
            host: host.to_string(),
            reason: "must not contain `:` (the port belongs in the `:entrada :port` \
                     slot — a separate `u16` axis on the same `:entrada` block, \
                     defaulting to 8080 — not in the host body; drop the `:<port>` \
                     suffix and author the bare hostname. If you intended an IPv6 \
                     literal (`2001:db8::1` / `::1` / `fe80::1`), Gateway API v1 \
                     Hostname forbids IP literals identically to the IPv4-literal \
                     arm — use a DNS name)"
                .to_string(),
        });
    }
    // Routed through the lifted [`crate::render::find_ascii_whitespace_byte`]
    // predicate — the same single source of truth every peer
    // ASCII-whitespace scan in caixa-core flows through: the four
    // typed-magnitude codec sites (`limits::parse_byte_size` backing
    // `:limits :memory`, `limits::parse_duration` backing `:limits
    // :wall-clock`, `limits::parse_millicores` backing `:limits :cpu`,
    // `aplicacao::rate_limit_codec::parse` backing `:politicas
    // :rate-limit`) and the shared duration codec
    // (`supervisor::duration_codec::parse`) backing `:supervisor
    // :restart-window` / `:politicas :timeout` / `:politicas
    // :circuit-breaker :window`. This landing closes the last string-typed
    // slot in caixa-core still calling `.bytes().any(|b|
    // b.is_ascii_whitespace())` inline — every ASCII-whitespace scan
    // across every typed slot now shares one predicate, so a future
    // stricter classification (BOM `\u{FEFF}` / ZWSP `\u{200B}` / ZWJ
    // `\u{200D}` — the "invisible but not `char::is_whitespace`" class
    // deliberately excluded from the peer non-ASCII predicate) can
    // extend at this shared site in one edit rather than seven
    // independent scans diverging over time. Naming the offending byte
    // in the diagnostic (`0x20` space / `0x09` tab / `0x0a` LF / `0x0c`
    // FF / `0x0d` CR) matches the substrate-wide "the diagnostic carries
    // the offending byte verbatim" discipline every peer codec site
    // already carries (`limits.rs:722` / `limits.rs:784` / `limits.rs:845`
    // / `supervisor.rs:823` / `aplicacao.rs:1640`).
    if let Some(b) = crate::render::find_ascii_whitespace_byte(host) {
        return Err(AplicacaoError::EntradaHostInvalid {
            host: host.to_string(),
            reason: format!(
                "contains ASCII whitespace byte 0x{b:02x} (Gateway API v1 \
                 Hostname is a single-token DNS name — leading, trailing, \
                 or embedded whitespace breaks the K8s apiserver's Hostname \
                 regex at admission time; the paste-from-aligned-doc / \
                 paste-from-shell-history / paste-from-CSV footgun silently \
                 lands a multi-token blob in `:entrada :host`. Strip every \
                 whitespace byte and author the bare hostname — space \
                 `0x20`, tab `0x09`, LF `0x0a`, FF `0x0c`, CR `0x0d` all \
                 refuse identically)"
            ),
        });
    }
    // Peer of the ASCII-whitespace scan above: route the non-ASCII
    // subset of Unicode `White_Space` through the shared
    // [`crate::render::find_non_ascii_whitespace_char`] predicate — the
    // single source of truth every peer non-ASCII-whitespace scan in
    // caixa-core flows through: `limits::parse_byte_size` (`:limits
    // :memory`), `limits::parse_duration` (`:limits :wall-clock`),
    // `limits::parse_millicores` (`:limits :cpu`),
    // `aplicacao::rate_limit_codec::parse` (`:politicas :rate-limit`),
    // and `supervisor::duration_codec::parse` (`:supervisor
    // :restart-window` / `:politicas :timeout` / `:politicas
    // :circuit-breaker :window`). Before this arm, a NBSP-prefixed host
    // (`"\u{00A0}checkout.quero.cloud"` — paste-from-typography), a
    // LINE-SEPARATOR-suffixed host (`"checkout.quero.cloud\u{2028}"` —
    // paste-from-web-doc), or an EM-SPACE-split host
    // (`"checkout.\u{2003}quero.cloud"` — paste-from-typography)
    // survived this predicate's ASCII byte-scan (none of the UTF-8
    // bytes of `\u{00A0}` / `\u{2028}` / `\u{2003}` match
    // `u8::is_ascii_whitespace`), then landed on the per-label
    // `bytes[0].is_ascii_alphanumeric()` arm near the bottom of this
    // predicate with the generic `label "…" must start and end with an
    // alphanumeric` diagnostic — a "far from source at build-time"
    // leak that names the label-shape violation but not the
    // paste-from-typography origin the author actually needs to fix.
    // Peer with the four codec sites the 1b75b38 landing pinned: the
    // typed slot's diagnostic axis names the offending codepoint
    // (`U+XXXX`) verbatim rather than laundering the value through a
    // downstream label-shape arm, so the author can grep their
    // caixa.lisp for the invisible codepoint at the surfaced position
    // rather than eyeball a multi-byte host for embedded NBSP / LINE
    // SEPARATOR / EM-SPACE. Same "single lifted source of truth"
    // discipline the peer ASCII-whitespace arm (720ac3b) carries:
    // drift between any two typed-slot sites' non-ASCII-whitespace
    // rejection set becomes a single-edit fix at the shared predicate
    // rather than N independent inline scans diverging over time, and
    // a future stricter classification (BOM `\u{FEFF}` / ZWSP
    // `\u{200B}` / ZWJ `\u{200D}` — the "invisible but not
    // `char::is_whitespace`" class the peer non-ASCII predicate's
    // doc-comment names as the follow-up trajectory) extends at the
    // shared predicate in one edit rather than seven.
    if let Some(ch) = crate::render::find_non_ascii_whitespace_char(host) {
        return Err(AplicacaoError::EntradaHostInvalid {
            host: host.to_string(),
            reason: format!(
                "contains non-ASCII Unicode whitespace character {ch:?} \
                 (U+{codepoint:04X}) — Gateway API v1 Hostname is a \
                 single-token DNS name limited to `[a-z0-9-]` labels; \
                 the paste-from-typography footgun silently lands an \
                 invisible codepoint (NBSP `U+00A0`, LINE SEPARATOR \
                 `U+2028`, EM-SPACE `U+2003`, IDEOGRAPHIC SPACE \
                 `U+3000`, and every other member of the Unicode \
                 `White_Space` property outside the ASCII byte range) \
                 in `:entrada :host`, which the K8s apiserver's \
                 Hostname regex refuses at admission time far from the \
                 caixa.lisp source line. Strip every non-ASCII \
                 whitespace character and author the bare hostname \
                 with only ASCII bytes (write \"checkout.quero.cloud\" \
                 verbatim)",
                codepoint = ch as u32,
            ),
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
        if label.len() > crate::render::DNS_1123_LABEL_MAX_LEN {
            return Err(AplicacaoError::EntradaHostInvalid {
                host: host.to_string(),
                reason: format!(
                    "label {label:?} exceeds DNS-1123 label max length of \
                     {cap} bytes (got {} bytes)",
                    label.len(),
                    cap = crate::render::DNS_1123_LABEL_MAX_LEN,
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
    // `Duration` is no longer named here — the codec routes through
    // the module-scope [`super::rate_limit_window_from_unit`] /
    // [`super::rate_limit_window_unit`] projections that carry the
    // canonical typed `Duration` unit-table axis on their signatures.
    use super::RateLimit;
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
        // Whitespace-rejection arm — peer with the leading-`+`
        // (`"+100/s"`) and leading-zero (`"0100/s"`) arms below on the
        // same canonical-form render-determinism axis. Until this gate
        // landed the parser silently tolerated leading / trailing /
        // internal whitespace via the top-level `s.trim()` and the
        // per-part `rate_str.trim()` / `unit.trim()` calls, so every
        // whitespace-carrying shape (`" 100/s"`, `"100/s "`,
        // `"100 /s"`, `"100/ s"`, `"100 / s"`, `"100/s\n"`,
        // `"\t100/s"`) parsed to the same `RateLimit { 100, 1s }` and
        // serde silently round-tripped to `"100/s"` on the next emit
        // (a *different* canonical string) — breaking the THEORY.md
        // Part V render-determinism contract on the same
        // canonical-form-drift axis the leading-`+` arm below (the
        // 4eeae98 predecessor) and the leading-zero arm below (the
        // 4f46830 predecessor) already close.
        //
        // The canonical author shape is `<integer>/<s|m|h>` with no
        // whitespace bytes anywhere — every string [`render`] emits
        // carries none, so the parser's accepted set must match for
        // serialize / deserialize to round-trip losslessly. This gate
        // makes the pre-existing `s.trim()` / `rate_str.trim()` /
        // `unit.trim()` calls below strict no-ops on the accepted set
        // (every byte-position match they would perform is now already
        // trimmed away by the accepted set itself), while the arm
        // surfaces every rejected whitespace-carrying shape with a
        // self-locating diagnostic naming the offending byte and the
        // canonical form the author intended, peer with every prior
        // canonical-form-drift arm on this codec.
        //
        // Routed through the lifted
        // [`crate::render::find_ascii_whitespace_byte`] predicate — the
        // same source of truth the four peer typed-magnitude codec
        // sites (`limits::parse_byte_size`, `limits::parse_duration`,
        // `limits::parse_millicores`, `supervisor::duration_codec`)
        // share. `u8::is_ascii_whitespace()` at the predicate covers
        // the five WhatWG-conformant ASCII whitespace bytes (space,
        // tab, LF, FF, CR); the "single lifted predicate" discipline
        // the peer non-ASCII arm below carries on the strictly-
        // complementary Unicode `White_Space` class extends here to
        // the ASCII byte set as well.
        if let Some(b) = crate::render::find_ascii_whitespace_byte(s) {
            return Err(format!(
                "rate-limit: value {s:?} contains whitespace byte 0x{b:02x} — the canonical \
                 authoring form for `:politicas :rate-limit` is `<integer>/<s|m|h>` (e.g. \
                 `\"100/s\"`, `\"5000/m\"`, `\"10000/h\"`) with no whitespace bytes \
                 anywhere. A whitespace-carrying shape (`\" 100/s\"`, `\"100/s \"`, \
                 `\"100 /s\"`, `\"100/ s\"`, `\"100 / s\"`, `\"100/s\\n\"`, `\"\\t100/s\"`) \
                 round-trips through `render` to a *different* canonical form (`\"100/s\"`) \
                 on first serialize — breaking the THEORY.md Part V render-determinism \
                 contract every typed slot carries. Strip every whitespace byte (write \
                 `\"100/s\"` verbatim)"
            ));
        }
        // Non-ASCII Unicode `White_Space` arm — the strictly-
        // complementary class the ASCII arm above cannot see.
        // `str::trim` at the top of every peer codec uses
        // `char::is_whitespace` (Unicode `White_Space`, strictly
        // wider than the ASCII byte set), so an NBSP (`\u{00A0}`) /
        // LINE SEPARATOR (`\u{2028}`) / EM-SPACE (`\u{2003}`)
        // survives the byte-scan (its UTF-8 bytes are not in
        // `is_ascii_whitespace`), gets silently stripped by the
        // top-level `s.trim()` below, and the value round-trips
        // through `render` to a *different* canonical form
        // (`\"100/s\"`) on next emit — breaking the THEORY.md Part V
        // render-determinism contract every typed slot carries.
        // Closed here (`:politicas :rate-limit`) and at the three
        // peer codec sites (`limits::parse_byte_size`,
        // `limits::parse_duration`, `supervisor::duration_codec`)
        // through the shared
        // [`crate::render::find_non_ascii_whitespace_char`] predicate
        // — the "single lifted predicate across all four codec sites
        // in one follow-up run" the 24a8ad4 commit body's `Forward
        // compounding` bullet named as the next compounding step.
        if let Some(ch) = crate::render::find_non_ascii_whitespace_char(s) {
            return Err(format!(
                "rate-limit: value {s:?} contains non-ASCII Unicode whitespace character \
                 {ch:?} (U+{cp:04X}) — the canonical authoring form for `:politicas \
                 :rate-limit` is `<integer>/<s|m|h>` (e.g. `\"100/s\"`, `\"5000/m\"`, \
                 `\"10000/h\"`) with no whitespace characters anywhere (ASCII or Unicode). \
                 A non-ASCII-whitespace-carrying shape (`\"\\u{{00A0}}100/s\"`, \
                 `\"100/s\\u{{2028}}\"`, `\"100\\u{{2003}}/s\"`) survives the ASCII \
                 byte-scan but `str::trim` (which uses `char::is_whitespace` — the \
                 Unicode `White_Space` property, strictly wider than the ASCII byte set) \
                 silently strips it at parse entry, and the value round-trips through \
                 `render` to a *different* canonical form (`\"100/s\"`) on first \
                 serialize — breaking the THEORY.md Part V render-determinism contract \
                 every typed slot carries. Strip every non-ASCII whitespace character \
                 (write `\"100/s\"` verbatim with only ASCII bytes)",
                cp = ch as u32
            ));
        }
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
        //
        // Routed through the lifted
        // [`crate::render::is_digit_only_magnitude`] predicate — the
        // same source of truth the four peer typed-magnitude codec
        // sites share.
        let digit_only = crate::render::is_digit_only_magnitude(rate_trim);
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
        // Leading-zero arm — peer with the prior `"+100/s"` arm above
        // (4eeae98's predecessor) on the same canonical-form
        // render-determinism axis. The digit-only gate accepts
        // `"0100/s"`, `"00/s"`, `"007/h"` as `u32::from_str` parses
        // them losslessly (= 100, 0, 7), but `render` emits the
        // leading-zero-stripped form (`"100/s"`, `"0/s"`, `"7/h"`) —
        // a *different* canonical string on the next emit, breaking
        // the THEORY.md Part V render-determinism contract the same
        // way `"+100/s"` did before the leading-`+` arm landed. The
        // single-byte magnitude `"0"` itself round-trips losslessly
        // through `render` (`render(0)` emits `"0/s"`) — the
        // downstream [`AplicacaoError::PolicyRateLimitZero`] gate is
        // what refuses rate-zero authoring, so `"0/s"` stays in the
        // accepted set at this codec layer and the diagnostic
        // partitioning between canonical-form drift (this arm) and
        // semantic-zero (the downstream gate) remains stable.
        // Peer with the future leading-zero arms on the three peer
        // typed-magnitude codecs the trajectory acknowledges:
        // `supervisor::duration_codec`, `limits::parse_duration`,
        // `limits::parse_byte_size` — each carries the same
        // canonical-form-drift class today; this gate lands the
        // discipline on the fourth typed-magnitude codec in
        // caixa-core first because the peer `"+100/s"` arm above is
        // the closest predecessor on the trajectory.
        //
        // Routed through the lifted
        // [`crate::render::is_leading_zero_padded_magnitude`]
        // predicate — the same source of truth the four peer
        // typed-magnitude codec sites share.
        if crate::render::is_leading_zero_padded_magnitude(rate_trim) {
            return Err(format!(
                "rate-limit: rate {rate_trim:?} has a non-canonical leading zero — the \
                 canonical authoring form for `:politicas :rate-limit` is \
                 `<integer>/<s|m|h>` (e.g. `\"100/s\"`, `\"5000/m\"`, `\"10000/h\"`) \
                 with no leading-zero padding on the magnitude. A leading-zero magnitude \
                 (`\"0100/s\"`, `\"00/s\"`, `\"007/h\"`) round-trips through `render` to \
                 a *different* canonical form (`\"100/s\"`, `\"0/s\"`, `\"7/h\"`) on \
                 first serialize — breaking the THEORY.md Part V render-determinism \
                 contract every typed slot carries. Strip the leading zeros (write \
                 `\"100/s\"` instead of `\"0100/s\"`)"
            ));
        }
        // The digit-only gate guarantees every byte is `[0-9]`, and
        // the leading-zero arm above guarantees the magnitude is
        // either the single byte `"0"` or starts with `[1-9]`, so
        // the only way `u32::from_str` can fail here is overflow
        // (the magnitude exceeds `u32::MAX`). Surface that with an
        // overflow-shaped wording so the diagnostic names the
        // offending magnitude verbatim rather than collapsing onto
        // the non-canonical arm. Same shape
        // `supervisor::duration_codec` (1c55a2a) carries on the peer
        // duration-codec axis.
        let rate: u32 = rate_trim.parse::<u32>().map_err(|_| {
            format!("rate-limit rate {rate_trim:?} (digit-only magnitude overflows u32)")
        })?;
        // The `{"s" ↔ 1s, "m" ↔ 60s, "h" ↔ 3600s}` bijection lives at
        // module scope as the lifted [`super::RATE_LIMIT_UNIT_TABLE`]
        // const; this parse arm now consumes only the `unit → Duration`
        // projection [`super::rate_limit_window_from_unit`], so a future
        // rate-limit-unit addition (a `"d"` day suffix once Envoy's
        // `rate_limit_action` grows daily-bucket support) is one row
        // appended to the table — parse, render, and
        // `is_canonical_rate_limit_window` all pick it up by construction.
        let unit = unit.trim();
        let window = super::rate_limit_window_from_unit(unit)
            .ok_or_else(|| format!("unknown rate-limit window unit {unit:?}"))?;
        Ok(RateLimit { rate, window })
    }

    fn render(rl: RateLimit) -> String {
        // The `{"s" ↔ 1s, "m" ↔ 60s, "h" ↔ 3600s}` bijection lives at
        // module scope as the lifted [`super::RATE_LIMIT_UNIT_TABLE`]
        // const; this render arm now consumes only the `Duration → unit`
        // projection [`super::rate_limit_window_unit`], which returns
        // `None` on every non-canonical window (the sub-second /
        // non-`{1, 60, 3600}` shapes the validate gate rejects). Same
        // helper the sibling [`super::is_canonical_rate_limit_window`]
        // predicate reads — so a future rate-limit-unit addition
        // (a `"d"` day suffix once Envoy's `rate_limit_action` grows
        // daily-bucket support) is one row appended to the table and
        // both consumers pick it up by construction.
        if let Some(unit) = super::rate_limit_window_unit(rl.window) {
            format!("{}/{unit}", rl.rate)
        } else {
            // Defensive fallback for non-canonical windows. Note:
            // [`AplicacaoSpec::validate_politicas`] rejects any
            // non-canonical `:rate-limit :window` via
            // [`AplicacaoError::PolicyRateLimitWindowNotCanonical`], so
            // a validated `RateLimit` never reaches this branch. The
            // emitted `<n>/<k>s` form is *not* round-trippable through
            // [`parse`] (which accepts only the [`super::RATE_LIMIT_UNIT_TABLE`]
            // suffixes, not `<k>s` with an explicit count) — the
            // validate gate is what makes the round-trip a structural
            // property; this branch exists only so a programmatic
            // non-validated serialize doesn't panic.
            format!("{}/{}s", rl.rate, rl.window.as_secs())
        }
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

impl PlacementStrategy {
    /// Canonical camelCase-schema discriminator scalar this variant
    /// serializes as under [`crate::M3_PLACEMENT_KEY_ESTRATEGIA`]. The
    /// three arms return the paired [`crate::M3_PLACEMENT_ESTRATEGIA_SINGLE_NODE`]
    /// / [`crate::M3_PLACEMENT_ESTRATEGIA_REPLICATED`] /
    /// [`crate::M3_PLACEMENT_ESTRATEGIA_SHARDED`] lifted constants so
    /// every substrate consumer that dispatches on the strategy (the
    /// `lareira-fleet-programs` aggregator, the future `app-operator`
    /// reconciler, the M3 Adaptive compression pass) reads the same
    /// byte-string the `Serialize` derive emits — the pin test in
    /// [`tests::placement_strategy_variants_serialize_to_lifted_scalar_values`]
    /// asserts the two paths agree.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SingleNode => crate::render::M3_PLACEMENT_ESTRATEGIA_SINGLE_NODE,
            Self::Replicated => crate::render::M3_PLACEMENT_ESTRATEGIA_REPLICATED,
            Self::Sharded => crate::render::M3_PLACEMENT_ESTRATEGIA_SHARDED,
        }
    }
}

/// [`std::fmt::Display`] routed through [`PlacementStrategy::as_str`], so
/// the pretty-printed byte-string every consumer that formats the strategy
/// as user-facing text lands on (the M3 [`AplicacaoError::PlacementWithoutClusters`]
/// / [`AplicacaoError::ShardKeyOnNonSharded`] `#[error(":placement
/// {estrategia} …")]` diagnostic templates, the future `feira app graph`
/// per-Aplicacao strategy line, the future M4 CR materializer's per-
/// admission-webhook rejection body) reaches for the same lifted
/// [`crate::M3_PLACEMENT_ESTRATEGIA_SINGLE_NODE`] /
/// [`crate::M3_PLACEMENT_ESTRATEGIA_REPLICATED`] /
/// [`crate::M3_PLACEMENT_ESTRATEGIA_SHARDED`] const the wire-format
/// `Serialize` derive already emits under
/// [`crate::M3_PLACEMENT_KEY_ESTRATEGIA`] and the
/// [`PlacementStrategy::as_str`] helper already returns.
///
/// Until this lift landed the sibling OTP-shape typed enums —
/// [`crate::supervisor::RestartStrategy`] / [`crate::supervisor::RestartPolicy`]
/// (both derive `gen_platform::Discriminant` with `#[discriminant(also_display)]`
/// so [`std::fmt::Display`] routes through the same discriminant string
/// the wire format emits) — carried a stable [`std::fmt::Display`]
/// surface but [`PlacementStrategy`] did not; every consumer reaching
/// for a strategy byte-string past the wire format had to pick between
/// three paths ([`PlacementStrategy::as_str`], the [`Serialize`] derive's
/// serialized string, `format!("{variant:?}")` on the [`std::fmt::Debug`]
/// derive), any two of which a future variant rename or
/// `#[serde(rename_all = "kebab-case")]` attribute would silently
/// desynchronize — with the failure surfacing as a downstream renderer /
/// operator's per-strategy dispatch reading one spelling while the wire
/// format emitted another, far from the source rebrand commit and with
/// no field naming the drift. Routing `Display` through
/// [`PlacementStrategy::as_str`] makes the three paths
/// (`Debug` for structural inspection, `Display` for user-facing text,
/// `Serialize` for the wire format) converge on the same lifted
/// [`crate::M3_PLACEMENT_ESTRATEGIA_*`] const set: the wire byte-string,
/// the diagnostic byte-string, and the pretty-printed byte-string move
/// as a single unit through one canonical declaration each, by
/// construction. Same trajectory as [`PlacementStrategy::as_str`]
/// (cc8f749) on the sibling wire-vs-const single-source axis — this lift
/// closes the third path.
///
/// Pin tests
/// [`tests::placement_strategy_display_routes_through_as_str_helper`]
/// and
/// [`tests::placement_strategy_display_matches_serialized_wire_byte_string`]
/// assert the three paths agree byte-for-byte on every variant, so a
/// future variant rename or per-arm serde attribute drift is a build
/// error visible at caixa-core test time, not a silent per-consumer
/// dispatch miss at apply / reconcile time.
impl std::fmt::Display for PlacementStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
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

/// Canonical default L4 port every typed Servico exposes on its
/// in-cluster K8s Service (the `trigger.service.port` axis the
/// `pleme-computeunit` library chart emits, the `:entrada :port` author
/// surface defaults to when the author omits the slot, and the
/// `caixa-mesh` `CiliumNetworkPolicy` L4-fallback substitutes when no
/// `:entrada` block matches the per-`:contratos` destination Servico).
/// The single source of truth all three typed-port consumers reach for:
///
///   - [`Entrada::port`]'s serde default (via the
///     [`default_port`] helper this constant feeds); the author surface
///     `(:entrada (:host … :para …))` without an explicit `:port` slot
///     reads back as a typed [`Entrada`] carrying this exact value;
///   - the
///     [`caixa_mesh::cilium_network_policies`][cm] `CiliumNetworkPolicy`
///     emitter's per-`(:de, :para)` L4 `toPorts[].ports[].port`
///     fallback, fired when the typed `:entrada` block doesn't name
///     the per-`:contratos` destination Servico — the typed
///     `:contratos` graph carries no per-destination port axis (the
///     destination port is the destination Servico's
///     `lareira-<nome>` chart's `trigger.service.port`, which the
///     Aplicacao-level renderer has no visibility into without a
///     resolver round-trip), so the renderer falls back to the
///     substrate's canonical Servico-port assumption — by
///     construction the same value the destination's own
///     `pleme-computeunit` chart emits, the same value the
///     destination's own typed `:entrada :port` slot defaults to;
///   - every future per-Servico renderer the absorption-roadmap
///     acknowledges (the future M4 `mesh.pleme.io/v1alpha1/Aplicacao`
///     CR materializer's per-edge port resolver, the future
///     per-`:politicas :rate-limit` `CiliumClusterwideEnvoyConfig`
///     emitter's per-route bucket key, the future caixa-otel
///     collector-pipeline emitter's per-Servico scrape port).
///
/// Until this lift landed the value `8080` lived at two production-code
/// call-sites: the [`default_port`] helper at
/// `caixa-core/src/aplicacao.rs:1712` (the typed slot's serde default)
/// and the `.unwrap_or(8080)` literal at
/// `caixa-mesh/src/lib.rs:344` (the L4-fallback in
/// [`caixa_mesh::cilium_network_policies`]'s per-`(:de, :para)` port
/// resolver). A future Servico-port rebrand — the substrate moving the
/// canonical port to `80` (HTTP's IANA-assigned port) once the cluster
/// gateway grows direct `:80` listeners, to `8443` once the substrate
/// moves to mTLS-by-default at the Servico boundary, to a per-cluster
/// override the operator pins through a future
/// `:placement :default-port` slot — without a coordinated edit on
/// both sides would silently emit Servicos listening on one port and
/// their Aplicacao's `CiliumNetworkPolicy` whitelisting a drifted one.
/// The CNP's apply-time symptom (the policy is admitted but every L4
/// flow on the destination Servico's actual port silently drops because
/// it doesn't match the whitelisted port) is far from the rebrand
/// commit's source, and Cilium's per-L4-drop diagnostic surfaces only
/// in hubble traces, not in `kubectl describe`. Lifting the literal to
/// a shared constant closes the drift footgun structurally — both
/// consumers read from the same `u16`, so any rebrand reaches both
/// sites by construction.
///
/// Mirrors the [`crate::DEFAULT_NAMESPACE`] lift (a085b26) on the peer
/// per-renderer canonical-K8s-axis constant — the namespace string
/// and the canonical Servico port both lived as duplicated literals
/// across caixa-core / caixa-mesh / caixa-flux before their respective
/// lifts. Same "the typed constant lives in one place" discipline the
/// [`crate::PLEME_LABEL_PREFIX`] / [`crate::LAREIRA_CHART_NAME_PREFIX`]
/// / [`crate::KUBE_KEY_API_VERSION`] lifts apply on the peer
/// shared-string axes.
///
/// [cm]: ../../caixa_mesh/fn.cilium_network_policies.html
pub const DEFAULT_SERVICO_PORT: u16 = 8080;

const fn default_port() -> u16 {
    DEFAULT_SERVICO_PORT
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
    ///   - no `:contratos` edge is a self-edge (`:de == :para`) — a
    ///     contract is an inter-Servico edge, so a Servico contracting
    ///     with itself is a build error under every WIT shape
    ///     (MESH-COMPOSITION §III.1)
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
            // Per-axis value-shape gate on every `:contratos` name
            // reference, before any graph-membership lookup. Empty +
            // DNS-1123-malformed `:de`/`:para` values silently fell
            // through to `ContratoMemberMissing` at the lookup arm
            // because every `:membros :caixa` is shape-validated
            // (3f9d7a0), so the `names` set structurally cannot contain
            // an empty / malformed string and the membership-lookup
            // diagnostic always misframed the root cause as
            // "this caixa is not in `:membros`". The shape gate runs
            // ahead of the lookup so structurally-impossible-to-match
            // inputs route through the narrower self-locating
            // diagnostic, preserving the legitimate "well-shaped
            // phantom reference" arm. `:de` runs before `:para` per
            // the canonical edge-direction order the existing
            // membership lookup, self-edge check, target dispatch,
            // and diagnostic strings already use.
            validate_contrato_caixa(crate::render::CONTRATO_AUTHOR_KEY_DE, &c.de)?;
            validate_contrato_caixa(crate::render::CONTRATO_AUTHOR_KEY_PARA, &c.para)?;
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
            // A `:contratos` entry is an *inter*-Servico contract
            // (MESH-COMPOSITION §III.1 — "Servico A calls Servico B"): a
            // typed edge between two distinct graph nodes. An edge whose
            // `:de` equals its `:para` is a Servico contracting with
            // itself — a degenerate edge under every WIT shape. The
            // synchronous shapes were caught only incidentally, and with
            // a misleading diagnostic: `detect_sync_cycles` reported
            // `cart → cart` as a `ContratoCycle` whose path is
            // `["cart", "cart"]` — framing a self-edge as a multi-node
            // deadlock. The pub-sub shape slipped through entirely
            // (`detect_sync_cycles` excludes `WitTarget::PubSub`, so a
            // `nats:pub-sub` edge from a member to itself silently
            // validated, then rendered a `CiliumNetworkPolicy` whose
            // endpointSelector and fromEndpoints both name the same
            // program — a self-allow rule that is a no-op, since
            // intra-pod traffic never traverses the mesh). A self-edge's
            // runtime meaning is an in-process call, which doesn't go
            // through the mesh at all, so no `:contratos` edge can carry
            // it. Firing the gate before the `:wit`/`target()` shape
            // checks means the structural "this edge can't exist" error
            // precedes the narrower payload-shape diagnostics, and shape-
            // agnostically covers all four `WitTarget` arms (HTTP / Store
            // / Capability / PubSub) at one point — closing the pub-sub
            // hole and replacing the misleading cycle diagnostic in one
            // gate. Peer of the duplicate-`:contratos` / duplicate-
            // `:membros` set gates: both reject a structurally
            // ill-formed graph at the typed surface, before the renderer
            // emits a K8s object that fails or no-ops far from the source
            // caixa.lisp.
            if c.de == c.para {
                return Err(AplicacaoError::ContratoSelfLoop {
                    caixa: c.de.clone(),
                    wit: c.wit.clone(),
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
            // build errors instead of silent renderer drops. Threaded
            // through the duplicate-edge diagnostic below (via
            // [`WitTarget::label`]) so the "which typed target arm did
            // the duplicate carry" question is answered by the typed
            // enum's variant discriminator, not by re-probing the raw
            // `Option<String>` payload fields.
            let target_view = c.target()?;
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
            crate::render::insert_first_seen(&mut seen_contracts, key, || {
                AplicacaoError::ContratoDuplicate {
                    de: c.de.clone(),
                    para: c.para.clone(),
                    wit: c.wit.clone(),
                    target: target_view.label(),
                }
            })?;
        }

        // Cycles in the synchronous-edge subgraph are build errors
        // (MESH-COMPOSITION §III.3). Pub-sub edges are excluded — they
        // are "acyclic by construction" because the publisher fires
        // and forgets, so no caller blocks on a downstream that loops
        // back to it.
        self.detect_sync_cycles()?;

        if let Some(e) = &self.entrada {
            // Shape gate on `:entrada :para` runs ahead of the
            // membership lookup. Every `:membros :caixa` past
            // `validate_membro_caixa` is a valid DNS-1123 label
            // (3f9d7a0), so the `names` set structurally cannot
            // contain an empty / malformed string and the membership-
            // lookup diagnostic always misframed the root cause as
            // "this caixa is not in `:membros`". The shape gate
            // routes structurally-impossible-to-match inputs through
            // the narrower self-locating diagnostic, preserving the
            // legitimate "well-shaped phantom reference" arm — the
            // same trajectory the peer `:membros :caixa` (3f9d7a0),
            // `:placement :clusters` (6c8c00b), and `:contratos :de`
            // / `:para` (8d5af6b) axes already follow. This closes
            // the fourth and last Aplicacao-level Servico-name
            // reference axis on the canonical DNS-1123 floor.
            validate_entrada_para(&e.para)?;
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
                crate::render::insert_first_seen(&mut seen, p.as_str(), || {
                    AplicacaoError::EntradaPathDuplicate { path: p.clone() }
                })?;
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
            // 272), the composed `CiliumNetworkPolicy` `metadata.name`
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
            // The author surface for `:versao` is the same Cargo-shaped
            // semver requirement string (`"^0.1"`, `"~0.1.2"`, `"0.1.0"`,
            // `"*"`) every `:deps` entry carries — and the lacre pipeline
            // resolves both axes through the same
            // [`crate::version::parse_requirement`] entry-point. The
            // shared [`crate::render::require_valid_versao_requirement`]
            // helper brackets the empty-first + parse cascade both peer
            // axes ([`crate::dep::Dep::validate`] on `:deps :versao`,
            // [`crate::SupervisorSpec::validate`] on `:children :versao`)
            // route through, so drift between the three axes' accepted
            // requirement sets is structurally impossible and the parse-
            // side no-op the empty-first arm closes (semver's empty
            // parse yields an implicit `*`) lives in exactly one
            // predicate.
            crate::render::require_valid_versao_requirement(
                &m.versao,
                || AplicacaoError::MembroVersaoEmpty {
                    caixa: m.caixa.clone(),
                },
                |reason| AplicacaoError::MembroVersaoInvalid {
                    caixa: m.caixa.clone(),
                    versao: m.versao.clone(),
                    reason,
                },
            )?;
            crate::render::insert_first_seen(&mut seen, m.caixa.as_str(), || {
                AplicacaoError::MembroDuplicate {
                    caixa: m.caixa.clone(),
                }
            })?;
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
            crate::render::insert_first_seen(&mut seen, c.as_str(), || {
                AplicacaoError::PlacementClusterDuplicate { cluster: c.clone() }
            })?;
        }
        if let Some(a) = &self.placement.affinity {
            // Per-hint value-shape gate: the `:affinity` value lands
            // verbatim in the M3 Adaptive compression overlay
            // (caixa-mesh's `placement.affinity` emission) and every
            // future M4 placement-engine routing axis keying off the
            // hint as a K8s `app.pleme.io/affinity-hint=<value>` label
            // selector — each enforces the DNS-1123 label rule on
            // admission. Same typed-shape trajectory as `:placement
            // :clusters` (6c8c00b) on the sibling slot and the four
            // Servico-name reference axes (`:membros :caixa` 3f9d7a0,
            // `:placement :clusters` 6c8c00b, `:contratos :de`/`:para`
            // 8d5af6b, `:entrada :para` b0e8748) — the fifth typed slot
            // on the Aplicacao surface to land on the canonical
            // [`crate::render::is_dns_1123_label`] floor.
            validate_placement_affinity(a)?;
        }
        match self.placement.estrategia {
            PlacementStrategy::Sharded => match &self.placement.shard_key {
                None => return Err(AplicacaoError::ShardedWithoutKey),
                Some(k) if k.is_empty() => return Err(AplicacaoError::ShardedKeyEmpty),
                // Per-axis value-shape gate on the Akka-cluster-sharding
                // `:shard-key` extractor expression. The shape gate runs
                // after the more self-locating `ShardedKeyEmpty` arm so
                // a `:shard-key ""` surfaces the narrower empty
                // diagnostic first; every non-empty `:shard-key` past
                // this call is guaranteed to be a printable-ASCII
                // single-token reference the future M4 Akka-style
                // cluster-sharding reconciler can hash without
                // re-validating at the runtime layer. Mirrors the
                // payload-axis shape gates on the peer `:contratos`
                // `:endpoint`/`:subject`/`:slot` axes (4f0390b /
                // 63e18a0 / c4213a4) — each lifts the runtime parser's
                // intersection-floor to a caixa-build-time gate.
                Some(k) => validate_placement_shard_key(k)?,
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
            // Zero-floor + integer-millisecond canonical-form +
            // upper-cap bracket on the typed `:timeout` axis. See
            // [`crate::render::require_positive_canonical_bounded_duration`]
            // for the full three-arm ordering discipline (zero-floor
            // strictly precedes the canonical-form arm so
            // `Duration::ZERO` surfaces the self-locating
            // `PolicyTimeoutZero` diagnostic naming the omit-axis
            // remediation; canonical-form strictly precedes the cap
            // arm so a sub-millisecond above-cap `Duration` surfaces
            // the more fundamental round-trip-shape diagnostic first)
            // and the four peer typed-`Duration` sites that now share
            // this canonical bracket. Every validated value lies in
            // `1ms..=POLICY_TIMEOUT_MAX` (1ms..=1h), integer-millisecond
            // granularity — the same top-and-bottom-edge discipline
            // [`POLICY_RETRIES_MAX`] and
            // [`POLICY_BREAKER_MAX_FAILURES_MAX`] apply on the sibling
            // capped-`u32` `:politicas` axes.
            crate::render::require_positive_canonical_bounded_duration(
                t,
                POLICY_TIMEOUT_MAX,
                || AplicacaoError::PolicyTimeoutZero,
                |timeout| AplicacaoError::PolicyTimeoutNotCanonical { timeout },
                |timeout| AplicacaoError::PolicyTimeoutExceedsCap { timeout },
            )?;
        }
        if let Some(r) = p.retries {
            // Zero-floor + upper-cap bracket on the typed `:retries`
            // axis. See [`crate::render::require_positive_bounded_u32`]
            // for the ordering discipline (zero-floor arm strictly
            // precedes cap arm so `Some(0)` surfaces the self-locating
            // `PolicyRetriesZero` diagnostic with its omit-axis
            // remediation directly named, not the misleading
            // `0 > POLICY_RETRIES_MAX == false` cap-arm miss). Until
            // this bracket landed the top edge ran all the way to
            // `u32::MAX` and a struct-literal `MeshPolicy { retries:
            // Some(100_000), .. }` (or the equivalent author-surface
            // `(:retries 100000)` / `(:retries 4294967295)` typo
            // landing in the slot) silently passed validate. The
            // runtime substrate consuming the value (Envoy's
            // `retry_policy.num_retries`, the future
            // `CiliumClusterwideEnvoyConfig` per-`:politicas` overlay
            // MESH-COMPOSITION §III.2 #3 names) then turned a typed
            // policy into a thundering-herd amplification vector —
            // the caller's one request fans out to `retries`
            // server-side calls per edge per traversal, multiplying
            // load by `(retries+1)^depth` across the
            // synchronous-`:contratos` subgraph at the precise moment
            // the substrate is already failing (transient failure is
            // the trigger), exactly the failure mode AWS App Mesh's
            // explicit `maxRetries ≤ 10` schema cap exists to prevent.
            // The bracket set is `1..=POLICY_RETRIES_MAX`. Peer with
            // the sibling capped-`u32` `:politicas` axes
            // (`max_failures`, `rate_limit.rate`) and the peer capped-
            // `u32` axes in `:supervisor :max-restarts` +
            // `:limits :cpu`; all five now route through the same
            // canonical bracket helper.
            crate::render::require_positive_bounded_u32(
                r,
                POLICY_RETRIES_MAX,
                || AplicacaoError::PolicyRetriesZero,
                |retries| AplicacaoError::PolicyRetriesExceedsCap { retries },
            )?;
        }
        if let Some(cb) = &p.circuit_breaker {
            // Zero-floor + upper-cap bracket on the typed
            // `:max-failures` axis. See
            // [`crate::render::require_positive_bounded_u32`] for the
            // ordering discipline (zero-floor arm strictly precedes
            // cap arm so `max_failures == 0` surfaces the
            // self-locating `PolicyBreakerZeroFailures` diagnostic
            // with its omit-axis remediation directly named, not the
            // misleading `0 > POLICY_BREAKER_MAX_FAILURES_MAX ==
            // false` cap-arm miss). Until this bracket landed the top
            // edge ran all the way to `u32::MAX` and a struct-literal
            // `CircuitBreaker { max_failures: 100_000, .. }` (or the
            // equivalent author-surface `(:max-failures 100000)` /
            // `(:max-failures 4294967295)` typo landing in the slot)
            // silently passed validate. The runtime substrate
            // consuming the value (Envoy's
            // `outlier_detection.consecutive_5xx`, the future
            // `CiliumClusterwideEnvoyConfig` per-`:politicas` overlay
            // MESH-COMPOSITION §III.2 #3 names) then turned a typed
            // breaker policy into a no-op — the trip threshold is
            // structurally so high that no realistic
            // failures-per-`:window` traffic shape can reach it, the
            // breaker never trips, and every typed-slot consumer
            // emits an Envoy / Cilium L7 overlay carrying a
            // protection that is structurally never enforced. The
            // bracket set is `1..=POLICY_BREAKER_MAX_FAILURES_MAX`;
            // peer with `retries` and `rate_limit.rate` on the same
            // helper.
            crate::render::require_positive_bounded_u32(
                cb.max_failures,
                POLICY_BREAKER_MAX_FAILURES_MAX,
                || AplicacaoError::PolicyBreakerZeroFailures,
                |max_failures| AplicacaoError::PolicyBreakerMaxFailuresExceedsCap { max_failures },
            )?;
            // Zero-floor + integer-millisecond canonical-form +
            // upper-cap bracket on the typed `:window` axis. See
            // [`crate::render::require_positive_canonical_bounded_duration`]
            // for the full three-arm ordering discipline (peer to the
            // `:timeout` site immediately above); every validated
            // value lies in `1ms..=POLICY_BREAKER_WINDOW_MAX`
            // (1ms..=1h), integer-millisecond granularity — the same
            // top-and-bottom-edge discipline
            // [`POLICY_TIMEOUT_MAX`] applies on the sibling
            // duration-typed `:politicas :timeout` axis.
            crate::render::require_positive_canonical_bounded_duration(
                cb.window,
                POLICY_BREAKER_WINDOW_MAX,
                || AplicacaoError::PolicyBreakerZeroWindow,
                |window| AplicacaoError::PolicyBreakerWindowNotCanonical { window },
                |window| AplicacaoError::PolicyBreakerWindowExceedsCap { window },
            )?;
        }
        if let Some(rl) = &p.rate_limit {
            // Zero-floor + upper-cap bracket on the typed
            // `:rate-limit` rate axis. See
            // [`crate::render::require_positive_bounded_u32`] for the
            // ordering discipline (zero-floor arm strictly precedes
            // cap arm so `rl.rate == 0` surfaces the self-locating
            // `PolicyRateLimitZero` diagnostic with its omit-axis
            // remediation directly named, not the misleading
            // `0 > POLICY_RATE_LIMIT_MAX == false` cap-arm miss).
            // Until this bracket landed the top edge ran all the way
            // to `u32::MAX` and a struct-literal
            // `RateLimit { rate: u32::MAX, .. }` (or the equivalent
            // author-surface `(:rate-limit "4294967295/s")` /
            // `(:rate-limit "100000000/m")` typo landing in the slot)
            // silently passed validate. The runtime substrate
            // consuming the value (Envoy's
            // `local_rate_limit.token_bucket.max_tokens`, the future
            // `CiliumClusterwideEnvoyConfig` per-`:politicas` overlay
            // MESH-COMPOSITION §III.2 #3 names) then turned a typed
            // rate-limit policy into a no-op limiter: the bucket
            // capacity is structurally so high that no realistic
            // per-edge traffic shape can drain it, the limiter never
            // trips, and every typed-slot consumer emits a "rate
            // declared" L7 overlay carrying enforcement that is
            // structurally never reached — the canonical
            // declared-but-inert footgun the sibling
            // [`POLICY_BREAKER_MAX_FAILURES_MAX`] cap arm closes on
            // the peer no-op-breaker shape. The bracket set is
            // `1..=POLICY_RATE_LIMIT_MAX`; peer with `retries` and
            // `max_failures` on the same helper. The rate bracket
            // strictly precedes the window-canonical gate so a
            // structurally absurd rate magnitude surfaces the more
            // fundamental amplification-shape diagnostic before the
            // narrower codec-round-trip-shape diagnostic on `:window`.
            crate::render::require_positive_bounded_u32(
                rl.rate,
                POLICY_RATE_LIMIT_MAX,
                || AplicacaoError::PolicyRateLimitZero,
                |rate| AplicacaoError::PolicyRateLimitExceedsCap { rate },
            )?;
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

/// Cross-slot coherence gate on the Aplicacao graph: no `:membros :caixa`
/// entry may name the Aplicacao's own `:nome`.
///
/// An Aplicacao that lists itself as a member is a degenerate self-edge in
/// the typed graph — the application graph is a DAG rooted at the Aplicacao
/// (MESH-COMPOSITION §III.1 names `:membros` as the set of *constituent*
/// Servicos that compose the app; an Aplicacao is never its own constituent),
/// and the lacre pipeline's closure-resolution would otherwise be handed a
/// node that is its own parent: a one-node cycle it either rejects far from
/// the source `caixa.lisp` (the resolver detecting infinite recursion on the
/// closure walk) or, worse, recurses on until it exhausts the lacre stack.
/// Because every `:nome` is a globally-unique substrate identity (DNS-1123
/// label + lacre closure root), a member whose `:caixa` equals the
/// Aplicacao's `:nome` *is* the Aplicacao itself, not a coincidentally-named
/// peer.
///
/// Lives outside [`AplicacaoSpec::validate`] because the typed view carries
/// the membros but not the parent `:nome`; mirrors the cross-slot precedence
/// gate `validate_upgrade_from_against_versao` and the supervision-tree
/// self-parent gate `crate::supervisor::validate_no_self_supervision`
/// (ad4abf1) — the same "an edge from a graph node to itself is structurally
/// not a tree/mesh edge" discipline, here on the second typed-graph axis
/// (the Aplicacao :membros set; the supervision-tree :children list was the
/// first). Closes the kind ↔ self-edge coverage on both typed-graph kinds:
/// every validated Supervisor's children are distinct from its `:nome`,
/// every validated Aplicacao's membros are distinct from its `:nome`. The
/// transitive consequence is that `:entrada :para` and `:contratos`
/// `:de`/`:para` — already gated to be members of `:membros` — also cannot
/// name the Aplicacao itself, without re-deriving the partition.
pub fn validate_no_self_membership(
    membros: &[Membro],
    parent_nome: &str,
) -> Result<(), AplicacaoError> {
    for m in membros {
        if m.caixa == parent_nome {
            return Err(AplicacaoError::MembroIsSelfAplicacao {
                caixa: parent_nome.to_string(),
            });
        }
    }
    Ok(())
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
    #[error(
        "aplicacao {caixa:?} lists itself as a :membros entry — an Aplicacao is \
         never its own constituent Servico (the application graph is a DAG rooted \
         at the Aplicacao; :membros names the *other* caixas that compose the \
         app, not the app itself). Since every :nome is a globally-unique \
         substrate identity, a member naming the Aplicacao's own :nome is a \
         one-node lacre-closure recursion, not a coincidentally-named peer; \
         drop the self-referential :membros entry or rename it to the actual \
         constituent caixa."
    )]
    MembroIsSelfAplicacao { caixa: String },
    #[error(
        "contrato {slot} is empty (every :contratos entry's :de and :para must name a \
         caixa declared in :membros; omit the contract or fill the {slot} field with a \
         member name)"
    )]
    ContratoCaixaEmpty { slot: &'static str },
    #[error(
        "contrato {slot} {caixa:?} is not a valid DNS-1123 label: {reason} (every \
         :contratos {slot} value names a member of :membros, which is itself a \
         DNS-1123 label per the K8s apiserver's `metadata.name` rule on every \
         object the member name lands in — Service, Pod, identity-based Cilium \
         selector; use a lowercase alphanumeric + hyphen identifier like \
         `\"checkout\"` or `\"cart-v2\"`)"
    )]
    ContratoCaixaInvalid {
        slot: &'static str,
        caixa: String,
        reason: String,
    },
    #[error("contrato references caixa {caixa:?} not declared in :membros")]
    ContratoMemberMissing { caixa: String },
    #[error(
        "contrato {caixa:?} → {caixa:?} (:wit {wit:?}) is a self-edge — a :contratos \
         entry is an inter-Servico contract whose :de and :para must name distinct \
         :membros; a Servico's calls to itself are in-process, not mesh edges (drop \
         the contract, or point :para at the member it actually calls)"
    )]
    ContratoSelfLoop { caixa: String, wit: String },
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
    #[error(
        ":entrada :para is empty (every :entrada must route to a caixa declared in \
         :membros; fill the :para field with a member name)"
    )]
    EntradaParaEmpty,
    #[error(
        ":entrada :para {para:?} is not a valid DNS-1123 label: {reason} (every \
         :entrada :para value names a member of :membros, which is itself a DNS-1123 \
         label per the K8s apiserver's `metadata.name` rule on every object the \
         member name lands in — Service backendRefs, HTTPRoute spec, identity-based \
         Cilium selector; use a lowercase alphanumeric + hyphen identifier like \
         `\"checkout\"` or `\"cart-v2\"`)"
    )]
    EntradaParaInvalid { para: String, reason: String },
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
        ":placement {estrategia} requires at least one :clusters entry \
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
    #[error(
        ":placement :affinity {affinity:?} is not a valid DNS-1123 label: {reason} \
         (placement hints land verbatim in the M3 Adaptive compression overlay's \
         `placement.affinity` field and in every future M4 placement-engine routing \
         axis keying off the hint as a K8s `app.pleme.io/affinity-hint=<value>` label \
         selector — both enforce the DNS-1123 label rule on admission; use a \
         lowercase alphanumeric + hyphen hint like `\"data-locality\"`, \
         `\"low-latency\"`, or `\"anti-affinity\"`)"
    )]
    PlacementAffinityInvalid { affinity: String, reason: String },
    #[error(":placement Sharded requires :shard-key")]
    ShardedWithoutKey,
    #[error(
        ":placement Sharded :shard-key must be non-empty (a `Some(\"\")` shard key \
         hashes every entity onto the same shard, defeating sharding entirely)"
    )]
    ShardedKeyEmpty,
    #[error(
        ":placement Sharded :shard-key {shard_key:?} is not a valid Akka-style \
         entity-id extractor expression: {reason} (the future M4 Akka-style \
         cluster-sharding reconciler — MESH-COMPOSITION §II.4 — reads `:shard-key` \
         as a single-token property reference and hashes the extracted entity ID \
         to compute shard placement; use a printable-ASCII extractor expression \
         like `\"tenantId\"`, `\"$tenantId\"`, `\"metadata.tenantId\"`, or \
         `\"${{tenant}}\"`)"
    )]
    ShardKeyInvalid { shard_key: String, reason: String },
    #[error(
        ":placement {estrategia} carries :shard-key {shard_key:?} — only :estrategia \
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
         contracts would render as colliding `CiliumNetworkPolicy` `metadata.name` \
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
        ":politicas :retries ({retries}) exceeds the mesh-policy ceiling \
         (POLICY_RETRIES_MAX = 10) — a value above this cap turns the typed \
         retry policy into a thundering-herd amplification vector on transient \
         failure (one caller request fans out to `(retries+1)^depth` server-side \
         calls across the synchronous-:contratos subgraph), exactly the failure \
         mode AWS App Mesh's `maxRetries ≤ 10` schema cap exists to prevent. \
         Pin a value in 1..=10 (Envoy / Istio production playbooks recommend ≤ 5) \
         or omit :retries to disable retries entirely"
    )]
    PolicyRetriesExceedsCap { retries: u32 },
    #[error(
        ":politicas :circuit-breaker :max-failures must be > 0 (a zero-threshold \
         breaker trips on the first call); omit :circuit-breaker to disable it"
    )]
    PolicyBreakerZeroFailures,
    #[error(
        ":politicas :circuit-breaker :max-failures ({max_failures}) exceeds the \
         mesh-policy ceiling (POLICY_BREAKER_MAX_FAILURES_MAX = 1000) — a value \
         above this cap turns the typed breaker policy into a no-op: the trip \
         threshold is structurally so high that no realistic failures-per-:window \
         traffic shape can reach it, so the breaker never trips and every typed-slot \
         consumer (the future CiliumClusterwideEnvoyConfig per-:politicas overlay, \
         Envoy's outlier_detection.consecutive_5xx) emits a protection that is \
         structurally never enforced. Pin a value in 1..=1000 (Hystrix / Istio / \
         Envoy / Polly / Resilience4j production playbooks recommend 5..=50) or \
         omit :circuit-breaker to disable the breaker entirely"
    )]
    PolicyBreakerMaxFailuresExceedsCap { max_failures: u32 },
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
        ":politicas :rate-limit rate ({rate}) exceeds the mesh-policy ceiling \
         (POLICY_RATE_LIMIT_MAX = 1000000) — a value above this cap turns the typed \
         rate-limit policy into a no-op limiter: the token-bucket capacity is \
         structurally so high that no realistic per-edge traffic shape can drain it, \
         so the limiter never trips and every typed-slot consumer (the future \
         CiliumClusterwideEnvoyConfig per-:politicas overlay, Envoy's \
         local_rate_limit.token_bucket.max_tokens) emits a rate-limit declaration \
         that is structurally never enforced. Pin a value in 1..=1000000 (Envoy / \
         Istio / Kong / NGINX production playbooks recommend 10..=10000 RPS; \
         Cloudflare / AWS API Gateway typical 10000..=100000 per-minute; \
         Cloudflare Enterprise rate-plans run to ~1M per-hour) or omit :rate-limit \
         to disable rate limiting entirely"
    )]
    PolicyRateLimitExceedsCap { rate: u32 },
    #[error(
        ":politicas :rate-limit :window must be exactly 1s, 1m (60s), or 1h (3600s) — \
         the canonical authoring forms `\"<n>/s\"`, `\"<n>/m\"`, `\"<n>/h\"` the \
         rate-limit codec round-trips losslessly; got {window:?} which renders to a \
         non-round-trippable form (omit :rate-limit to disable, or pick one of the \
         three canonical windows)"
    )]
    PolicyRateLimitWindowNotCanonical { window: Duration },
    #[error(
        ":politicas :timeout must be an integer number of milliseconds — the canonical \
         authoring form `\"<integer><unit>\"` for unit ∈ {{`ms`,`s`,`m`,`h`}} the shared \
         duration codec round-trips losslessly; got {timeout:?} which carries a \
         sub-millisecond residue that either truncates to a different `Duration` on \
         re-parse (e.g. `Duration::from_micros(1500)` → renders `\"1ms\"` → parses back \
         to 1ms, not 1.5ms) or renders as `\"0s\"` (sub-millisecond magnitude) the \
         zero-floor gate rejects on re-validate. Pick an integer-millisecond magnitude \
         (e.g. `\"30s\"`, `\"1500ms\"`, `\"2m\"`, `\"1h\"`)"
    )]
    PolicyTimeoutNotCanonical { timeout: Duration },
    #[error(
        ":politicas :timeout ({timeout:?}) exceeds the mesh-policy ceiling \
         (POLICY_TIMEOUT_MAX = 1h = 3600s) — a value above this cap turns the typed \
         per-call deadline into a nominal-only contract (Envoy / Cilium L7 timeout \
         overlays carry a deadline so long no realistic synchronous-:contratos \
         traversal can reach it), and the MESH-COMPOSITION §V \"no infinite blocking\" \
         CSE invariant degenerates to enforcement only at the per-Servico \
         `:limits :wall-clock` layer — far above the per-edge granularity the typed \
         `:politicas :timeout` slot is meant to express. Pin a value in 1ms..=1h \
         (Envoy / Istio / Linkerd / AWS App Mesh production playbooks all recommend \
         ≤ 60s; the Kubernetes ingress-nginx documented `proxy_read_timeout` band \
         maxes out at the same `3600s` ceiling) or omit :timeout to express \
         `no per-call deadline on this axis` (the synchronous-call deadline then \
         relies entirely on the per-Servico `:limits :wall-clock` axis)"
    )]
    PolicyTimeoutExceedsCap { timeout: Duration },
    #[error(
        ":politicas :circuit-breaker :window must be an integer number of milliseconds — \
         the canonical authoring form `\"<integer><unit>\"` for unit ∈ {{`ms`,`s`,`m`,`h`}} \
         the shared duration codec round-trips losslessly; got {window:?} which carries a \
         sub-millisecond residue that either truncates to a different `Duration` on \
         re-parse or renders as `\"0s\"` the zero-floor gate rejects on re-validate. \
         Pick an integer-millisecond magnitude (e.g. `\"60s\"`, `\"500ms\"`, `\"2m\"`)"
    )]
    PolicyBreakerWindowNotCanonical { window: Duration },
    #[error(
        ":politicas :circuit-breaker :window ({window:?}) exceeds the mesh-policy ceiling \
         (POLICY_BREAKER_WINDOW_MAX = 1h = 3600s) — a value above this cap turns the typed \
         rolling-window breaker into a lifetime-counter breaker: the failure-counting window \
         is structurally so long that transient failures are never forgotten, the breaker \
         trips once and stays tripped for the lifetime of the component, and every typed-slot \
         consumer (the future CiliumClusterwideEnvoyConfig per-:politicas overlay, Envoy's \
         outlier_detection.interval) emits a \"rolling\" window that exists only nominally. \
         Pin a value in 1ms..=1h (Hystrix / resilience4j / Istio / Envoy production playbooks \
         default to 10s; AWS App Mesh maxes out at ~5m) or omit :circuit-breaker to disable \
         the breaker entirely"
    )]
    PolicyBreakerWindowExceedsCap { window: Duration },
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

    // ── :contratos :de / :para DNS-1123 label value-shape gate ──────────

    #[test]
    fn rejects_contrato_de_empty() {
        // `:de ""` previously fell through to `ContratoMemberMissing`
        // (with `caixa: ""`) because the validated `:membros :caixa`
        // set never contains the empty string. The narrower
        // `ContratoCaixaEmpty { slot: ":de" }` diagnostic now names
        // the offending slot.
        let mut s = three_member_spec();
        s.contratos.push(contract_http("", "catalog", "/x"));
        let err = s.validate().unwrap_err();
        assert_eq!(
            err,
            AplicacaoError::ContratoCaixaEmpty {
                slot: crate::render::CONTRATO_AUTHOR_KEY_DE
            },
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_contrato_para_empty() {
        // Symmetric arm to `:de ""` — `:para ""` previously fell
        // through to `ContratoMemberMissing { caixa: "" }`.
        let mut s = three_member_spec();
        s.contratos.push(contract_http("cart", "", "/x"));
        let err = s.validate().unwrap_err();
        assert_eq!(
            err,
            AplicacaoError::ContratoCaixaEmpty {
                slot: crate::render::CONTRATO_AUTHOR_KEY_PARA
            },
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_contrato_de_with_uppercase() {
        // The canonical "I copied the Servico's TitleCase display
        // name from an ADR" typo. Until this gate landed `:de "Cart"`
        // surfaced `ContratoMemberMissing { caixa: "Cart" }` — framed
        // as "this caixa isn't in `:membros`" when the root cause is
        // "this `:de` value's shape can never legitimately match a
        // validated member (DNS-1123 labels are lowercase)". The
        // narrower diagnostic names the offending slot, the value
        // verbatim, and the parser-shaped reason.
        let mut s = three_member_spec();
        s.contratos.push(contract_http("Cart", "catalog", "/x"));
        let err = s.validate().unwrap_err();
        let AplicacaoError::ContratoCaixaInvalid {
            slot,
            caixa,
            reason,
        } = err
        else {
            panic!("expected ContratoCaixaInvalid, got other variant");
        };
        assert_eq!(slot, crate::render::CONTRATO_AUTHOR_KEY_DE);
        assert_eq!(caixa, "Cart");
        assert!(
            reason.contains("uppercase"),
            "diagnostic must name the violation as `uppercase` (got: {reason:?})"
        );
    }

    #[test]
    fn rejects_contrato_para_with_underscore() {
        // The canonical "I'm thinking of a Python module" leak —
        // `_` is forbidden by every DNS-1123 / DNS-1035 label schema.
        // Pin the `:para` axis surfaces the same diagnostic shape as
        // the `:de` axis on the underscore violation.
        let mut s = three_member_spec();
        s.contratos.push(contract_http("cart", "my_catalog", "/x"));
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ContratoCaixaInvalid { slot, ref caixa, ref reason }
                    if slot == crate::render::CONTRATO_AUTHOR_KEY_PARA && caixa == "my_catalog" && reason.contains('_')
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_contrato_de_with_dot() {
        // A `:contratos :de` value is a single DNS-1123 *label*, not
        // a subdomain — mirroring the `:membros :caixa` floor. The
        // strictest floor among the use sites wins.
        let mut s = three_member_spec();
        s.contratos
            .push(contract_http("team.cart", "catalog", "/x"));
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ContratoCaixaInvalid { slot, ref caixa, ref reason }
                    if slot == crate::render::CONTRATO_AUTHOR_KEY_DE && caixa == "team.cart" && reason.contains('.')
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_contrato_para_with_unicode() {
        // DNS-1123 is ASCII-only; IDN must be pre-encoded as Punycode
        // (`xn--…`) before it reaches K8s. The byte-by-byte ASCII
        // validity check rejects multi-byte UTF-8 by the first
        // non-`[a-z0-9-]` byte.
        let mut s = three_member_spec();
        s.contratos.push(contract_http("cart", "café", "/x"));
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ContratoCaixaInvalid { slot, ref caixa, .. }
                    if slot == crate::render::CONTRATO_AUTHOR_KEY_PARA && caixa == "café"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_contrato_de_with_leading_hyphen() {
        // DNS-1123 boundary rule: labels must start and end with an
        // alphanumeric. K8s rejects `-cart` outright; the narrower
        // shape diagnostic now names the violation at caixa-build
        // time rather than the misframed membership-lookup arm.
        let mut s = three_member_spec();
        s.contratos.push(contract_http("-cart", "catalog", "/x"));
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ContratoCaixaInvalid { slot, ref caixa, ref reason }
                    if slot == crate::render::CONTRATO_AUTHOR_KEY_DE && caixa == "-cart" && reason.contains("start and end")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn contrato_de_empty_takes_precedence_over_invalid() {
        // Order pin: the `ContratoCaixaEmpty` arm fires before the
        // `ContratoCaixaInvalid` parse-side arm — same empty-first
        // cascade `validate_membro_caixa` / `validate_placement_cluster`
        // / `validate_entrada_host` already establish on their peer
        // name axes. The empty string is a structurally distinct
        // authoring footgun (the author left the field blank, vs.
        // typed a malformed value), so it gets its own diagnostic.
        let mut s = three_member_spec();
        s.contratos.push(contract_http("", "catalog", "/x"));
        let err = s.validate().unwrap_err();
        assert_eq!(
            err,
            AplicacaoError::ContratoCaixaEmpty {
                slot: crate::render::CONTRATO_AUTHOR_KEY_DE
            }
        );
    }

    #[test]
    fn contrato_de_shape_fires_before_para_shape() {
        // Per-axis order pin: within one `:contratos` entry, the `:de`
        // shape gate fires before the `:para` shape gate — same
        // edge-direction order the existing `ContratoMemberMissing` /
        // `ContratoSelfLoop` / target-dispatch checks use, so the
        // diagnostic for a contract with both `:de` and `:para`
        // malformed is stable. Authors fixing the surfaced `:de`
        // first will see `:para`'s diagnostic on re-run.
        let mut s = three_member_spec();
        s.contratos.push(contract_http("Cart", "Catalog", "/x"));
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ContratoCaixaInvalid { slot, ref caixa, .. }
                    if slot == crate::render::CONTRATO_AUTHOR_KEY_DE && caixa == "Cart"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn contrato_shape_fires_before_membership_lookup() {
        // The load-bearing pin: an invalid-shape `:de` surfaces its
        // *own* diagnostic, not the misframed `ContratoMemberMissing`.
        // Because every `:membros :caixa` is shape-validated (3f9d7a0),
        // an invalid-shape `:de` could never legitimately match any
        // member — the prior `ContratoMemberMissing` diagnostic was
        // a structural impossibility framed as a graph-membership
        // failure. The shape gate now routes every such input through
        // the narrower self-locating diagnostic.
        let mut s = three_member_spec();
        s.contratos.push(contract_http("Cart", "catalog", "/x"));
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ContratoCaixaInvalid { slot, .. } if slot == crate::render::CONTRATO_AUTHOR_KEY_DE
            ),
            "got {err:?}"
        );
        // And the symmetric case: an invalid-shape `:para` surfaces
        // its own diagnostic too, even when `:de` is well-shaped.
        let mut s = three_member_spec();
        s.contratos.push(contract_http("cart", "Catalog", "/x"));
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ContratoCaixaInvalid { slot, .. } if slot == crate::render::CONTRATO_AUTHOR_KEY_PARA
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn contrato_shape_fires_before_self_edge_check() {
        // A `:de "Cart" :para "Cart"` entry is two distinct authoring
        // bugs: the shape violation (uppercase) and the self-edge
        // violation. The narrower per-axis shape diagnostic surfaces
        // first because fixing the shape may reveal that the author
        // also meant to point `:para` at a different member — the
        // self-edge framing is only useful once both endpoints have
        // valid shape.
        let mut s = three_member_spec();
        s.contratos.push(contract_http("Cart", "Cart", "/x"));
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ContratoCaixaInvalid { slot, ref caixa, .. }
                    if slot == crate::render::CONTRATO_AUTHOR_KEY_DE && caixa == "Cart"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn contrato_well_shaped_phantom_still_raises_member_missing() {
        // Strict-improvement pin: a well-shaped `:de` that simply
        // isn't in `:membros` (a phantom reference — author meant
        // to add the member but didn't, or renamed and missed an
        // update) still surfaces `ContratoMemberMissing`, unchanged.
        // The shape gate only intercepts inputs that could never
        // legitimately match a validated member; legitimately-shaped
        // phantom references remain on the graph-membership axis.
        let mut s = three_member_spec();
        s.contratos
            .push(contract_http("phantom-shim", "catalog", "/x"));
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ContratoMemberMissing { ref caixa }
                    if caixa == "phantom-shim"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn contrato_caixa_invalid_diagnostic_carries_offending_slot_and_value() {
        // The diagnostic-shape pin: the error names the offending
        // slot (`:de` or `:para`) verbatim and the offending value
        // verbatim plus a non-empty parser-shaped reason, so the
        // author can grep their caixa.lisp for `:de "<name>"` /
        // `:para "<name>"` and fix it in one edit. Same diagnostic
        // shape as `MembroCaixaInvalid` (3f9d7a0) and
        // `PlacementClusterInvalid` (6c8c00b).
        let mut s = three_member_spec();
        s.contratos.push(contract_http("cart", "BAD_NAME", "/x"));
        let err = s.validate().unwrap_err();
        let AplicacaoError::ContratoCaixaInvalid {
            slot,
            caixa,
            reason,
        } = err
        else {
            panic!("expected ContratoCaixaInvalid, got {err:?}");
        };
        assert_eq!(slot, crate::render::CONTRATO_AUTHOR_KEY_PARA);
        assert_eq!(caixa, "BAD_NAME");
        assert!(
            !reason.is_empty(),
            "ContratoCaixaInvalid `reason` must carry a parser-shaped wording"
        );
    }

    #[test]
    fn contrato_author_key_consts_pin_canonical_kebab_case_labels() {
        // Scalar-value pin: the two author-facing kebab-case labels the
        // `(:contratos ((:de "<caixa>" :para "<caixa>" …) …))` surface
        // admits on the `:contratos` per-entry endpoint-shape axis,
        // one arm per typed sub-slot. Mirrors the peer scalar-value
        // pin the sibling top-level M2 / M3 / Supervisor
        // author-facing-label consts carry
        // (`m3_top_level_author_key_consts_pin_canonical_kebab_case_labels`
        // for the parent [`crate::render::M3_AUTHOR_KEY_CONTRATOS`]
        // slot itself), so every altitude of the typed-slot algebra
        // shares the same "one canonical byte-string per arm"
        // discipline. A future rebrand (`:de` → `:from` matching the
        // OTP `appup` [`crate::render::M2_UPGRADE_FROM_KEY_FROM`]
        // sibling, `:para` → `:to` matching the same, or
        // `:de`/`:para` → `:source`/`:target` matching the WIT
        // world's `import`/`export` half-vocabulary) lands as an
        // edit to exactly one const, and every consumer that reaches
        // for the label picks it up at build time rather than at
        // runtime as a downstream `ContratoCaixaEmpty` /
        // `ContratoCaixaInvalid` `slot: <stale-kebab-case>`
        // diagnostic mismatch far from the rename's commit.
        assert_eq!(crate::render::CONTRATO_AUTHOR_KEY_DE, ":de");
        assert_eq!(crate::render::CONTRATO_AUTHOR_KEY_PARA, ":para");
    }

    #[test]
    fn contrato_shape_gate_routes_through_lifted_contrato_author_key_consts() {
        // Production-through-const pin: the two per-axis labels the
        // per-`:contratos` entry endpoint-shape gate at
        // [`AplicacaoSpec::validate`] passes as the `slot: &'static str`
        // argument to [`validate_contrato_caixa`] route through the
        // lifted [`crate::render::CONTRATO_AUTHOR_KEY_DE`] /
        // [`crate::render::CONTRATO_AUTHOR_KEY_PARA`] consts, so a
        // future rebrand that reaches the const but not the gate (or
        // vice versa) surfaces here at build time rather than at
        // runtime as a downstream [`AplicacaoError::ContratoCaixaEmpty`]
        // `slot: <stale-kebab-case>` diagnostic far from the rename's
        // commit. Mirror of the peer
        // [`manifest::declared_mesh_slots_route_through_lifted_m3_author_key_consts`]
        // pin (882f498) on the sibling M3 top-level slot axis.
        let mut s = three_member_spec();
        s.contratos.push(contract_http("", "catalog", "/x"));
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::ContratoCaixaEmpty {
                slot: crate::render::CONTRATO_AUTHOR_KEY_DE
            }
        );
        let mut s = three_member_spec();
        s.contratos.push(contract_http("cart", "", "/x"));
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::ContratoCaixaEmpty {
                slot: crate::render::CONTRATO_AUTHOR_KEY_PARA
            }
        );
    }

    #[test]
    fn accepts_canonical_contrato_caixa_forms() {
        // The DNS-1123 label shapes a caixa author is realistically
        // going to write on a `:contratos :de` / `:para`. Pin every
        // leg so a future tightening that bans (e.g.) digit-start
        // identifiers surfaces here, mirroring
        // `accepts_canonical_membro_caixa_forms` on the peer name
        // axis.
        for form in ["cart", "cart-v2", "a", "c0", "3rd-party-shim", "x-1-2-3-4"] {
            let mut s = three_member_spec();
            s.membros = vec![membro("checkout", "^0.1"), membro(form, "^0.1")];
            s.contratos = vec![contract_http("checkout", form, "/x")];
            s.entrada = None;
            s.validate().unwrap_or_else(|e| {
                panic!("canonical form {form:?} must validate on `:para`, got {e:?}")
            });

            let mut s = three_member_spec();
            s.membros = vec![membro(form, "^0.1"), membro("catalog", "^0.1")];
            s.contratos = vec![contract_http(form, "catalog", "/x")];
            s.entrada = None;
            s.validate().unwrap_or_else(|e| {
                panic!("canonical form {form:?} must validate on `:de`, got {e:?}")
            });
        }
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

    // ── :entrada :para DNS-1123 label value-shape gate ───────────────────

    #[test]
    fn rejects_entrada_para_empty() {
        // `:para ""` previously fell through to
        // `EntradaMemberMissing { para: "" }` because the validated
        // `:membros :caixa` set never contains the empty string. The
        // narrower `EntradaParaEmpty` diagnostic now names the
        // offending slot directly — same empty-first cascade
        // `MembroCaixaEmpty` / `PlacementClusterEmpty` /
        // `ContratoCaixaEmpty` establish on the peer name axes.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().para = String::new();
        let err = s.validate().unwrap_err();
        assert_eq!(err, AplicacaoError::EntradaParaEmpty, "got {err:?}");
    }

    #[test]
    fn rejects_entrada_para_with_uppercase() {
        // The canonical "I copied the Servico's TitleCase display
        // name from an ADR" typo. Until this gate landed `:para "Cart"`
        // surfaced `EntradaMemberMissing { para: "Cart" }` — framed
        // as "this caixa isn't in `:membros`" when the root cause is
        // "this `:para` value's shape can never legitimately match a
        // validated member (DNS-1123 labels are lowercase)". The
        // narrower diagnostic names the value verbatim plus the
        // parser-shaped reason.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().para = "Cart".into();
        let err = s.validate().unwrap_err();
        let AplicacaoError::EntradaParaInvalid { para, reason } = err else {
            panic!("expected EntradaParaInvalid, got other variant");
        };
        assert_eq!(para, "Cart");
        assert!(
            reason.contains("uppercase"),
            "diagnostic must name the violation as `uppercase` (got: {reason:?})"
        );
    }

    #[test]
    fn rejects_entrada_para_with_underscore() {
        // The canonical "I'm thinking of a Python module" leak —
        // `_` is forbidden by every DNS-1123 / DNS-1035 label schema.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().para = "my_cart".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::EntradaParaInvalid { ref para, ref reason }
                    if para == "my_cart" && reason.contains('_')
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_para_with_dot() {
        // An `:entrada :para` value is a single DNS-1123 *label*, not
        // a subdomain — mirroring the `:membros :caixa` floor. The
        // strictest floor among the use sites wins.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().para = "team.cart".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::EntradaParaInvalid { ref para, ref reason }
                    if para == "team.cart" && reason.contains('.')
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_para_with_unicode() {
        // DNS-1123 is ASCII-only; IDN must be pre-encoded as Punycode
        // (`xn--…`) before it reaches K8s.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().para = "café".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::EntradaParaInvalid { ref para, .. } if para == "café"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_para_with_leading_hyphen() {
        // DNS-1123 boundary rule: labels must start and end with an
        // alphanumeric. K8s rejects `-cart` outright.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().para = "-cart".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::EntradaParaInvalid { ref para, ref reason }
                    if para == "-cart" && reason.contains("start and end")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_para_with_trailing_hyphen() {
        // Symmetric boundary arm.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().para = "cart-".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::EntradaParaInvalid { ref para, ref reason }
                    if para == "cart-" && reason.contains("start and end")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_para_too_long() {
        // 64-byte over-cap slug — the DNS-1123 label rule caps at 63
        // bytes per label. K8s rejects longer names at admission on
        // every `metadata.name` axis.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().para = "a".repeat(64);
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::EntradaParaInvalid { ref para, ref reason }
                    if para.len() == 64 && reason.contains("max length")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn entrada_para_empty_takes_precedence_over_invalid() {
        // Order pin: the `EntradaParaEmpty` arm fires before the
        // `EntradaParaInvalid` parse-side arm — same empty-first
        // cascade `validate_membro_caixa` / `validate_placement_cluster`
        // / `validate_contrato_caixa` already establish.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().para = String::new();
        assert_eq!(s.validate().unwrap_err(), AplicacaoError::EntradaParaEmpty);
    }

    #[test]
    fn entrada_para_shape_fires_before_membership_lookup() {
        // The load-bearing pin: an invalid-shape `:para` surfaces its
        // *own* diagnostic, not the misframed `EntradaMemberMissing`.
        // Because every `:membros :caixa` is shape-validated (3f9d7a0),
        // an invalid-shape `:para` could never legitimately match any
        // member — the prior `EntradaMemberMissing` diagnostic framed
        // a structural impossibility as a graph-membership failure.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().para = "Cart".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::EntradaParaInvalid { ref para, .. } if para == "Cart"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn entrada_para_shape_fires_before_host_gate() {
        // Per-`:entrada` order pin: the `:para` shape gate fires
        // before the `:host` gate, mirroring the existing
        // `entrada_host_member_missing_takes_precedence_over_host_invalid`
        // ordering where the member-lookup arm preceded the host gate.
        // The shape gate slots ahead of that, so a malformed `:para`
        // surfaces its own diagnostic even when `:host` is also wrong.
        let mut s = three_member_spec();
        let e = s.entrada.as_mut().unwrap();
        e.para = "Cart".into();
        e.host = "BAD HOST".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::EntradaParaInvalid { ref para, .. } if para == "Cart"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn entrada_para_well_shaped_phantom_still_raises_member_missing() {
        // Strict-improvement pin: a well-shaped `:para` that simply
        // isn't in `:membros` (a phantom reference — author meant to
        // add the member but didn't, or renamed and missed an
        // update) still surfaces `EntradaMemberMissing`, unchanged.
        // The shape gate only intercepts inputs that could never
        // legitimately match a validated member.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().para = "phantom-shim".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::EntradaMemberMissing { ref para }
                    if para == "phantom-shim"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn entrada_para_invalid_diagnostic_carries_offending_para() {
        // The diagnostic-shape pin: the error names the offending
        // `:para` value verbatim plus a non-empty parser-shaped
        // reason, so the author can grep their caixa.lisp for
        // `:para "<name>"` and fix it in one edit. Same diagnostic
        // shape as `MembroCaixaInvalid` (3f9d7a0),
        // `PlacementClusterInvalid` (6c8c00b), and
        // `ContratoCaixaInvalid` (8d5af6b).
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().para = "BAD_NAME".into();
        let err = s.validate().unwrap_err();
        let AplicacaoError::EntradaParaInvalid { para, reason } = err else {
            panic!("expected EntradaParaInvalid, got {err:?}");
        };
        assert_eq!(para, "BAD_NAME");
        assert!(
            !reason.is_empty(),
            "EntradaParaInvalid `reason` must carry a parser-shaped wording"
        );
    }

    #[test]
    fn accepts_canonical_entrada_para_forms() {
        // Positive-control sweep covering the DNS-1123 label shapes a
        // caixa author is realistically going to write on `:entrada
        // :para`. Pin every leg so a future tightening that bans
        // (e.g.) digit-start identifiers surfaces here, mirroring
        // `accepts_canonical_membro_caixa_forms` and
        // `accepts_canonical_contrato_caixa_forms` on the peer name
        // axes.
        for form in ["cart", "cart-v2", "a", "c0", "3rd-party-shim", "x-1-2-3-4"] {
            let mut s = three_member_spec();
            s.membros = vec![membro(form, "^0.1"), membro("catalog", "^0.1")];
            s.contratos = vec![contract_http(form, "catalog", "/x")];
            s.entrada = Some(Entrada {
                host: "checkout.quero.cloud".into(),
                para: form.into(),
                paths: vec!["/api".into()],
                port: 8080,
            });
            s.validate().unwrap_or_else(|e| {
                panic!("canonical form {form:?} must validate on `:entrada :para`, got {e:?}")
            });
        }
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
                expected: WitTarget::HTTP_FIELD_NAME,
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
                expected: WitTarget::HTTP_FIELD_NAME,
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
                expected: WitTarget::PUBSUB_FIELD_NAME,
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
                expected: WitTarget::PUBSUB_FIELD_NAME,
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
                expected: WitTarget::STORE_FIELD_NAME,
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
                expected: WitTarget::CAPABILITY_EXPECTED,
                ..
            }
        ));
    }

    #[test]
    fn wit_target_capability_expected_pins_wrong_target_diagnostic_scalar() {
        // Pin the Capability-arm `ContratoWrongTarget::expected` scalar
        // single-sourced onto [`WitTarget::CAPABILITY_EXPECTED`] — the
        // fourth arm of the same "which payload field name goes in the
        // diagnostic" dispatch the payload-arm [`WitTarget::HTTP_FIELD_NAME`]
        // / [`WitTarget::PUBSUB_FIELD_NAME`] / [`WitTarget::STORE_FIELD_NAME`]
        // consts cover on the peer HTTP / PubSub / Store arms
        // (`wit_target_field_name_pins_per_variant`). Until this lift
        // landed the byte-string sat twice — once inline in the
        // [`WitContract::target`] Capability-arm rejection at the
        // production dispatch, once in `rejects_unknown_wit_with_target_set`
        // pinning against the same literal — with no compile-time link
        // between them. Same "one canonical declaration, next to the
        // variant" trajectory the peer [`WitTarget::CAPABILITY_LABEL`]
        // lift established for the payload-less arm's human-readable
        // label axis; this test is the shape peer of
        // `wit_target_label_pins_per_variant`'s Capability-arm assertion
        // pair (routes-through-const + scalar-value pin) on the
        // wrong-target diagnostic-scalar axis.
        //
        // Fail-before-pass-after was verified locally by mutating the
        // const declaration to `"capability"` — the scalar-value pin
        // below fires (`"capability" != "none"`) and the routes-through
        // assertion below still holds (production and const walk in
        // lockstep), which is the correct behavior: a rename on the
        // const drifts here first, not at a downstream consumer.
        assert_eq!(WitTarget::CAPABILITY_EXPECTED, "none");

        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "cart".into(),
            para: "catalog".into(),
            wit: "custom:exchange".into(),
            endpoint: Some("/leaked".into()),
            subject: None,
            slot: None,
        });
        match s.validate().unwrap_err() {
            AplicacaoError::ContratoWrongTarget { expected, .. } => {
                assert_eq!(expected, WitTarget::CAPABILITY_EXPECTED);
            }
            other => panic!("expected ContratoWrongTarget, got {other:?}"),
        }
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
            // arm we're exercising. Routes off the same
            // `wit_shape_is_http` / `wit_shape_is_pubsub` /
            // `wit_shape_is_store` free functions the production
            // `WitContract::is_http` / `is_pubsub` / `is_store`
            // methods delegate to (both consult the lifted
            // `WIT_HTTP_SHAPE_PREFIXES` / `WIT_PUBSUB_SHAPE_PREFIXES`
            // / `WIT_STORE_SHAPE_PREFIXES` prefix sets), so any
            // future prefix addition to the routing accept-set
            // reaches this test's payload-dispatch arm by
            // construction — no per-test-site drift can hide a
            // shape-→-target-slot mismatch that would silently
            // demote a canonical `:wit` value to the
            // `(None, None, None)` capability-only arm and let the
            // `AplicacaoSpec::validate` positive sweep pass on a
            // shape it should exercise as HTTP / pub-sub / store.
            let (endpoint, subject, slot) = if wit_shape_is_http(wit) {
                (Some("/x".into()), None, None)
            } else if wit_shape_is_pubsub(wit) {
                (None, Some("topic.x".into()), None)
            } else if wit_shape_is_store(wit) {
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
    fn wit_shape_predicates_accept_canonical_prefix_set() {
        // Positive-set sweep pinning every prefix in
        // WIT_HTTP_SHAPE_PREFIXES / WIT_PUBSUB_SHAPE_PREFIXES /
        // WIT_STORE_SHAPE_PREFIXES against the three free-function
        // dispatch predicates. The six prefixes are the load-bearing
        // routing keys the substrate's WIT-shape dispatch consults
        // (L7-HTTP-vs-L4, pub-sub-cycle exclusion,
        // key/value-store-slot admission); any drift between the
        // free-function accept-set and this list surfaces here
        // rather than at apply time as a silent
        // shape-→-capability-only demotion.
        assert!(wit_shape_is_http("wasi:http/proxy"));
        assert!(wit_shape_is_http("wasi:http/proxy@0.2.0"));
        assert!(wit_shape_is_http("http:incoming"));

        assert!(wit_shape_is_pubsub("nats:pub-sub"));
        assert!(wit_shape_is_pubsub("kafka:topic"));

        assert!(wit_shape_is_store("wasi:keyvalue/store"));
        assert!(wit_shape_is_store("kv:cache/session"));
    }

    #[test]
    fn wit_shape_predicates_reject_uncanonical_forms() {
        // Negative-set pin: the six canonical prefixes are
        // lowercase-only (mirrors the `is_wit_world_ref` substrate
        // predicate's lowercase invariant — see its docstring on the
        // "I thought I had L7 HTTP routing, got L4-only" footgun).
        // The empty string, an uppercase-prefixed form, a hyphen-
        // instead-of-colon typo, and a bare kebab identifier all miss
        // every shape arm — reachable-by-construction only via the
        // `is_wit_world_ref` gate that admission-checks the `:wit`
        // value first, but pinned here so any future
        // free-function change (e.g. a case-insensitive
        // `wit.to_ascii_lowercase().starts_with(p)` slip) surfaces at
        // this unit level.
        for wit in ["", "WASI:HTTP/proxy", "wasi-http/proxy", "custom-shape"] {
            assert!(!wit_shape_is_http(wit), "{wit:?} must not be HTTP");
            assert!(!wit_shape_is_pubsub(wit), "{wit:?} must not be pubsub");
            assert!(!wit_shape_is_store(wit), "{wit:?} must not be store");
        }
    }

    #[test]
    fn wit_shape_predicates_partition_canonical_set() {
        // Every canonical prefix routes to exactly one shape arm —
        // the three prefix sets are pairwise disjoint. Pins the
        // routing property [`WitContract::target`] relies on: an
        // `is_http()` return of `true` guarantees `is_pubsub()` and
        // `is_store()` return `false`, so the shape-→-target-slot
        // dispatch (endpoint vs subject vs slot) is unambiguous.
        // Drift (e.g. a future `"kv:"` moved into the HTTP set
        // without removal from the store set) would silently route
        // one prefix to two arms and the first-matching-arm order
        // becomes load-bearing — this pin surfaces it as a build
        // error instead.
        for prefix in WIT_HTTP_SHAPE_PREFIXES {
            let sample = format!("{prefix}x");
            assert!(wit_shape_is_http(&sample));
            assert!(!wit_shape_is_pubsub(&sample));
            assert!(!wit_shape_is_store(&sample));
        }
        for prefix in WIT_PUBSUB_SHAPE_PREFIXES {
            let sample = format!("{prefix}x");
            assert!(!wit_shape_is_http(&sample));
            assert!(wit_shape_is_pubsub(&sample));
            assert!(!wit_shape_is_store(&sample));
        }
        for prefix in WIT_STORE_SHAPE_PREFIXES {
            let sample = format!("{prefix}x");
            assert!(!wit_shape_is_http(&sample));
            assert!(!wit_shape_is_pubsub(&sample));
            assert!(wit_shape_is_store(&sample));
        }
    }

    #[test]
    fn wit_shape_matches_scans_prefix_set_with_starts_with_semantics() {
        // Positive pin: [`wit_shape_matches`] is exactly the
        // `PREFIXES.iter().any(|p| wit.starts_with(p))` combinator,
        // parameterized on the accept-set. Two-prefix accept-set,
        // one-prefix accept-set, and empty accept-set (which must
        // reject everything, including the empty string — an empty
        // `any()` fold returns `false`) all pinned so a future
        // reimplementation that swaps `starts_with` for `contains`,
        // `==`, or a case-folded comparator surfaces at unit-test
        // time.
        let two = &["wasi:http/", "http:"];
        assert!(wit_shape_matches("wasi:http/proxy", two));
        assert!(wit_shape_matches("http:incoming", two));
        assert!(!wit_shape_matches("wasi:keyvalue/store", two));

        let one = &["nats:"];
        assert!(wit_shape_matches("nats:pub-sub", one));
        assert!(!wit_shape_matches("kafka:topic", one));

        // Empty accept-set matches nothing — the identity element
        // for the disjunctive `any()` fold across the prefix set.
        // Reachable via a future `wit_shape_is_<name>` const paired
        // to a still-empty prefix table on a nascent shape-arm draft.
        let empty: &[&str] = &[];
        assert!(!wit_shape_matches("wasi:http/proxy", empty));
        assert!(!wit_shape_matches("", empty));

        // starts_with, not contains: a prefix embedded mid-string
        // never matches. Pins the routing invariant [`WitContract::target`]
        // relies on (an authored `:wit "custom:wasi:http/"` string
        // does not silently route through the HTTP arm just because
        // it happens to contain the canonical HTTP prefix).
        assert!(!wit_shape_matches("custom:wasi:http/proxy", two));
    }

    #[test]
    fn wit_shape_predicates_delegate_to_wit_shape_matches() {
        // Equivalence pin: each per-shape predicate is exactly
        // `wit_shape_matches(wit, WIT_<SHAPE>_SHAPE_PREFIXES)`. Sweeps
        // every canonical prefix + the empty string + one negative
        // sample against every peer so a future predicate that grew
        // its own inline `iter().any(starts_with)` (rather than
        // delegating through the lifted combinator) drifts loudly here
        // — the peer-const table's contents must agree with the
        // predicate's accept-set by construction.
        let samples = [
            String::new(),
            "wasi:http/proxy".to_string(),
            "http:incoming".to_string(),
            "nats:pub-sub".to_string(),
            "kafka:topic".to_string(),
            "wasi:keyvalue/store".to_string(),
            "kv:cache/session".to_string(),
            "custom-shape".to_string(),
            "WASI:HTTP/proxy".to_string(),
        ];
        for wit in &samples {
            assert_eq!(
                wit_shape_is_http(wit),
                wit_shape_matches(wit, WIT_HTTP_SHAPE_PREFIXES),
                "wit_shape_is_http drifted from combinator on {wit:?}",
            );
            assert_eq!(
                wit_shape_is_pubsub(wit),
                wit_shape_matches(wit, WIT_PUBSUB_SHAPE_PREFIXES),
                "wit_shape_is_pubsub drifted from combinator on {wit:?}",
            );
            assert_eq!(
                wit_shape_is_store(wit),
                wit_shape_matches(wit, WIT_STORE_SHAPE_PREFIXES),
                "wit_shape_is_store drifted from combinator on {wit:?}",
            );
        }
    }

    #[test]
    fn wit_contract_shape_methods_delegate_to_free_functions() {
        // Equivalence pin: `WitContract::is_http` / `is_pubsub` /
        // `is_store` are `&self` conveniences on top of the free
        // functions — for every canonical prefix the method's return
        // matches its free-function peer. Sweeps the union of the
        // three prefix sets so a future method that grew its own
        // inline prefix logic (rather than delegating) drifts loudly
        // here on the first prefix the free function accepts and the
        // method doesn't.
        for shape_set in [
            WIT_HTTP_SHAPE_PREFIXES,
            WIT_PUBSUB_SHAPE_PREFIXES,
            WIT_STORE_SHAPE_PREFIXES,
        ] {
            for prefix in shape_set {
                let c = WitContract {
                    de: "cart".into(),
                    para: "catalog".into(),
                    wit: format!("{prefix}x"),
                    endpoint: None,
                    subject: None,
                    slot: None,
                };
                assert_eq!(c.is_http(), wit_shape_is_http(&c.wit));
                assert_eq!(c.is_pubsub(), wit_shape_is_pubsub(&c.wit));
                assert_eq!(c.is_store(), wit_shape_is_store(&c.wit));
            }
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
        // A synchronous self-edge (`cart → cart` over HTTP) is now
        // rejected by the dedicated `ContratoSelfLoop` gate — a precise
        // "this edge is degenerate" diagnostic — rather than incidentally
        // by the cycle detector framing it as a `["cart", "cart"]`
        // multi-node deadlock.
        let mut s = three_member_spec();
        s.contratos.push(contract_http("cart", "cart", "/loop"));
        let err = s.validate().unwrap_err();
        match err {
            AplicacaoError::ContratoSelfLoop { caixa, wit } => {
                assert_eq!(caixa, "cart");
                assert_eq!(wit, "wasi:http/proxy");
            }
            other => panic!("expected ContratoSelfLoop, got {other:?}"),
        }
    }

    #[test]
    fn rejects_self_loop_in_pubsub_contratos() {
        // The cycle detector excludes pub-sub edges (acyclic by
        // construction), so before the explicit gate a `nats:pub-sub`
        // self-edge silently validated and rendered a self-allow CNP.
        // The shape-agnostic `ContratoSelfLoop` gate closes that hole.
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "payment".into(),
            para: "payment".into(),
            wit: "nats:pub-sub".into(),
            endpoint: None,
            subject: Some("rio.events.payment".into()),
            slot: None,
        });
        let err = s.validate().unwrap_err();
        match err {
            AplicacaoError::ContratoSelfLoop { caixa, wit } => {
                assert_eq!(caixa, "payment");
                assert_eq!(wit, "nats:pub-sub");
            }
            other => panic!("expected ContratoSelfLoop, got {other:?}"),
        }
    }

    #[test]
    fn self_loop_fires_before_payload_shape_check() {
        // The structural "this edge can't exist" error precedes the
        // narrower payload-shape diagnostics: a self-edge carrying an
        // otherwise-malformed endpoint still reports ContratoSelfLoop,
        // not ContratoEndpointInvalid.
        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "cart".into(),
            para: "cart".into(),
            wit: "wasi:http/proxy".into(),
            endpoint: Some("not-absolute".into()),
            subject: None,
            slot: None,
        });
        match s.validate().unwrap_err() {
            AplicacaoError::ContratoSelfLoop { caixa, .. } => assert_eq!(caixa, "cart"),
            other => panic!("expected ContratoSelfLoop, got {other:?}"),
        }
    }

    #[test]
    fn self_loop_fires_before_membership_is_satisfied_but_after_missing_member() {
        // A self-edge naming a non-member reports the more fundamental
        // ContratoMemberMissing first (the member doesn't exist), so the
        // self-loop gate is reached only once both endpoints resolve.
        let mut s = three_member_spec();
        s.contratos.push(contract_http("ghost", "ghost", "/loop"));
        match s.validate().unwrap_err() {
            AplicacaoError::ContratoMemberMissing { caixa } => assert_eq!(caixa, "ghost"),
            other => panic!("expected ContratoMemberMissing, got {other:?}"),
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
        // emitted two ``CiliumNetworkPolicy`` objects with identical
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
    fn wit_target_label_pins_per_variant_format() {
        // Label format is the single source of truth every duplicate-
        // `:contratos` diagnostic + every future `feira app graph`
        // consumer routes through. Pin the shape per variant so a
        // future edit to `WitTarget::label` (e.g. a JSON emitter that
        // strips the leading `:`, or a rename from `endpoint` →
        // `path`) surfaces as a red-red test rather than as a silent
        // downstream diagnostic drift. Together with the exhaustive
        // `match` on `WitTarget` inside `label()`, adding a future
        // variant (M4 `Rest` / `Grpc` split, `Queue`-shaped `Store`
        // peer, per-edge WIT registry variants) is a compile error at
        // the label site — not a fall-through into the `Capability`
        // "no payload" default the prior raw-field-probe helper
        // silently landed on.
        assert_eq!(
            WitTarget::Http {
                endpoint: "/charge",
            }
            .label(),
            "\
:endpoint \"/charge\""
        );
        assert_eq!(
            WitTarget::PubSub {
                subject: "events.checkout.paid",
            }
            .label(),
            "\
:subject \"events.checkout.paid\""
        );
        assert_eq!(
            WitTarget::Store {
                slot: "checkout/$order",
            }
            .label(),
            "\
:slot \"checkout/$order\""
        );
        assert_eq!(WitTarget::Capability.label(), "(capability — no payload)");
        // Capability-arm label routes through the lifted
        // [`WitTarget::CAPABILITY_LABEL`] const so the "one canonical
        // declaration per arm, next to the variant" discipline the
        // peer payload-arm [`WitTarget::HTTP_FIELD_NAME`] /
        // [`WitTarget::PUBSUB_FIELD_NAME`] / [`WitTarget::STORE_FIELD_NAME`]
        // consts already carry extends to the payload-less arm; the
        // byte-string equality pin below plus this label-routes-
        // through-the-const pin make a future rebrand on either the
        // const declaration or the `label()` template a build error
        // here rather than a downstream consumer surprise.
        assert_eq!(WitTarget::Capability.label(), WitTarget::CAPABILITY_LABEL,);
        assert_eq!(WitTarget::CAPABILITY_LABEL, "(capability — no payload)");
    }

    #[test]
    fn wit_target_payload_pair_pins_per_variant() {
        // Pin the per-arm `(field-name, payload)` pair single-sourced
        // onto [`WitTarget::payload_pair`] — the single 4-arm dispatch
        // both [`WitTarget::label`] (formats `":{field} {payload:?}"`
        // on `Some`, falls to [`WitTarget::CAPABILITY_LABEL`] on `None`)
        // and [`WitTarget::field_name`] (returns the first component)
        // route through. Until this lift landed [`WitTarget::label`]
        // dispatched on the same three arms with a per-arm
        // `format!(":{} {…:?}", …)` invocation each, hand-quoting the
        // paired [`WitTarget::HTTP_FIELD_NAME`] /
        // [`WitTarget::PUBSUB_FIELD_NAME`] /
        // [`WitTarget::STORE_FIELD_NAME`] const at every site — the
        // canonical "same shape, written N times" duplication
        // THEORY.md §I.3.5 promotes to a build-time concern. A future
        // [`WitTarget`] variant addition (`Rest`/`Grpc` split of
        // [`WitTarget::Http`], `Queue`-shaped peer of
        // [`WitTarget::Store`]) is one match-arm edit at
        // [`WitTarget::payload_pair`], visible here as a compile-time
        // exhaustiveness error on both this pin and the label-format
        // pin above.
        assert_eq!(
            WitTarget::Http {
                endpoint: "/charge"
            }
            .payload_pair(),
            Some((WitTarget::HTTP_FIELD_NAME, "/charge")),
        );
        assert_eq!(
            WitTarget::PubSub {
                subject: "events.x",
            }
            .payload_pair(),
            Some((WitTarget::PUBSUB_FIELD_NAME, "events.x")),
        );
        assert_eq!(
            WitTarget::Store {
                slot: "checkout/$order",
            }
            .payload_pair(),
            Some((WitTarget::STORE_FIELD_NAME, "checkout/$order")),
        );
        assert_eq!(WitTarget::Capability.payload_pair(), None);
    }

    #[test]
    fn wit_target_field_name_pins_per_variant() {
        // Pin the per-arm author-facing `:contratos` payload field
        // name single-sourced onto [`WitTarget::HTTP_FIELD_NAME`] /
        // [`WitTarget::PUBSUB_FIELD_NAME`] / [`WitTarget::STORE_FIELD_NAME`]
        // + returned by [`WitTarget::field_name`]. Every downstream
        // consumer (the [`WitContract::target`] gate's `expected:`
        // scalar, the [`WitTarget::label`] template's keyword prefix,
        // the `feira app graph` verb's `endpoint=…` prefix) routes
        // through the same three peer consts, so a rename on the
        // author-surface `(defcaixa … :contratos ((:de … :para …
        // :wit … :endpoint …)))` field lands in exactly one place.
        assert_eq!(
            WitTarget::Http {
                endpoint: "/charge"
            }
            .field_name(),
            Some(WitTarget::HTTP_FIELD_NAME),
        );
        assert_eq!(
            WitTarget::PubSub {
                subject: "events.x",
            }
            .field_name(),
            Some(WitTarget::PUBSUB_FIELD_NAME),
        );
        assert_eq!(
            WitTarget::Store {
                slot: "checkout/$order",
            }
            .field_name(),
            Some(WitTarget::STORE_FIELD_NAME),
        );
        // Capability arm carries no payload field — the diagnostic
        // never reports `expected: "capability"` because the gate's
        // Capability arm accepts no payload at all (it fires the
        // "expected: none" WrongTarget error instead), so the field-
        // name method returns None here rather than a placeholder.
        assert_eq!(WitTarget::Capability.field_name(), None);

        // Peer const scalar values pinned so a rename on either side
        // (author-surface field name in the `(defcaixa …)` DSL, or
        // the diagnostic's `expected:` scalar) can't drift without
        // failing here first.
        assert_eq!(WitTarget::HTTP_FIELD_NAME, "endpoint");
        assert_eq!(WitTarget::PUBSUB_FIELD_NAME, "subject");
        assert_eq!(WitTarget::STORE_FIELD_NAME, "slot");
    }

    #[test]
    fn wit_target_field_names_are_pairwise_distinct() {
        // Distinctness pin: if any two of the three payload-field-name
        // scalars ever collapse (e.g. an accidental `endpoint` copy-
        // paste over the `subject` const), the [`WitContract::target`]
        // gate's diagnostic would point authors at the wrong field —
        // an "expected `:endpoint`" error on a pub-sub edge would
        // silently misroute the fix. Same cross-axis-distinctness
        // discipline as the peer M3 `:placement :estrategia` variant-
        // discriminator scalar-value pins (cc8f749) applied to the
        // payload-field-name axis.
        assert_ne!(WitTarget::HTTP_FIELD_NAME, WitTarget::PUBSUB_FIELD_NAME);
        assert_ne!(WitTarget::HTTP_FIELD_NAME, WitTarget::STORE_FIELD_NAME);
        assert_ne!(WitTarget::PUBSUB_FIELD_NAME, WitTarget::STORE_FIELD_NAME);
    }

    #[test]
    fn wit_target_field_name_routes_through_label_and_expected_diagnostic() {
        // Consumer-side pin: the same three peer consts thread through
        // both the [`WitTarget::label`] template (leading-`:` keyword
        // prefix in the duplicate-`:contratos` diagnostic) and the
        // [`WitContract::target`] gate's [`AplicacaoError::
        // ContratoMissingTarget`] `expected:` scalar (the field the
        // author needs to add). Pin both routes at once so a future
        // refactor can't accidentally split them onto separate string
        // literals — the "one place, everywhere reaches for it"
        // invariant the peer const set carries.
        let http_label = WitTarget::Http { endpoint: "/x" }.label();
        assert!(
            http_label.starts_with(&format!(":{} ", WitTarget::HTTP_FIELD_NAME)),
            "label must lead with :{} keyword (got {http_label:?})",
            WitTarget::HTTP_FIELD_NAME,
        );

        let mut s = three_member_spec();
        s.contratos.push(WitContract {
            de: "cart".into(),
            para: "catalog".into(),
            wit: "kafka:topic".into(),
            endpoint: None,
            subject: None,
            slot: None,
        });
        match s.validate().unwrap_err() {
            AplicacaoError::ContratoMissingTarget { expected, .. } => {
                assert_eq!(expected, WitTarget::PUBSUB_FIELD_NAME);
            }
            other => panic!("expected ContratoMissingTarget, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_pubsub_diagnostic_names_offending_subject() {
        // Peer of `rejects_duplicate_contrato_diagnostic_names_offending_target`
        // on the pub-sub target axis: the duplicate-edge diagnostic
        // must name the `:subject` payload verbatim (not just the
        // `(de, para, wit)` triple). Prior to lifting the label onto
        // [`WitTarget::label`] the diagnostic derived the label from
        // raw [`WitContract`] `Option<String>` probes — a future
        // `WitTarget` variant addition (M4 per-edge WIT registry)
        // would silently fall through to the `Capability` "no
        // payload" default without a compiler warning. Pinning the
        // pub-sub arm's format closes the second of three
        // payload-carrying `WitTarget` arms this diagnostic threads
        // through.
        let mut s = three_member_spec();
        let pubsub = WitContract {
            de: "payment".into(),
            para: "cart".into(),
            wit: "nats:pub-sub".into(),
            endpoint: None,
            subject: Some("events.checkout.paid".into()),
            slot: None,
        };
        s.contratos.push(pubsub.clone());
        s.contratos.push(pubsub);
        let err = s.validate().unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(":subject \"events.checkout.paid\""),
            "duplicate-pubsub diagnostic must name the offending \
             :subject payload (got: {msg:?})"
        );
    }

    #[test]
    fn duplicate_store_diagnostic_names_offending_slot() {
        // Peer of the HTTP + pub-sub duplicate-diagnostic pins on the
        // key-value target axis: the diagnostic must name the `:slot`
        // payload verbatim. Third of three payload-carrying
        // `WitTarget` arms this diagnostic threads through, closing
        // the per-arm label pin trilogy (`Http` — 6841,
        // `PubSub` + `Store` — this test + peer above).
        let mut s = three_member_spec();
        let store = WitContract {
            de: "cart".into(),
            para: "payment".into(),
            wit: "wasi:keyvalue/store".into(),
            endpoint: None,
            subject: None,
            slot: Some("checkout/$orderId".into()),
        };
        s.contratos
            .retain(|c| !(c.de == "cart" && c.para == "payment"));
        s.contratos.push(store.clone());
        s.contratos.push(store);
        let err = s.validate().unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(":slot \"checkout/$orderId\""),
            "duplicate-store diagnostic must name the offending :slot \
             payload (got: {msg:?})"
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

    #[test]
    fn rejects_entrada_path_with_curly_brace_template_form() {
        // Per-axis pin on the shared `is_gateway_api_http_path`
        // reserved-byte arm: the canonical "I wrote an OpenAPI
        // path-template `{id}` instead of the Gateway API `:id` form"
        // footgun the K8s apiserver would otherwise catch at admission
        // time on every `HTTPRoute.spec.rules[].matches[].path.value`
        // landing site, far from the caixa.lisp. Surfaces as
        // `EntradaPathInvalid` carrying the offending path verbatim
        // plus the canonical `%7B`/`%7D` percent-encoding remediation
        // — the substrate-side `gateway_api_http_path_rejects_every_
        // reserved_printable_ascii_byte` predicate-level sweep pins the
        // full eleven-byte set; this per-axis pin confirms the
        // diagnostic flows through to the `EntradaPathInvalid` variant.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec!["/api/cart/{id}".into()];
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaPathInvalid { ref path, ref reason }
                if path == "/api/cart/{id}"
                    && reason.contains("reserved character")
                    && reason.contains("'{'")
                    && reason.contains("%7B")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_http_contrato_endpoint_with_curly_brace_template_form() {
        // Per-axis peer of `rejects_entrada_path_with_curly_brace_
        // template_form` on the sibling `:contratos :endpoint` axis.
        // Same shared `is_gateway_api_http_path` reserved-byte arm
        // fires through `ContratoEndpointInvalid`, with the offending
        // endpoint + `:de` + `:para` + reason flowing through verbatim.
        // Pins that the lifted predicate's tightening lands on both
        // caller axes simultaneously — one source of truth for the
        // Gateway API HTTPPathMatch.value accepted set.
        let err = contrato_endpoint_err("/api/cart/{id}");
        assert!(
            matches!(err, AplicacaoError::ContratoEndpointInvalid { ref endpoint, ref reason, .. }
                if endpoint == "/api/cart/{id}"
                    && reason.contains("reserved character")
                    && reason.contains("'{'")
                    && reason.contains("%7B")),
            "got {err:?}"
        );
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
        // belongs in `:entrada :port`" footgun. The top-level `:` arm
        // (introduced after the per-label loop-only impl silently
        // surfaced a deep "label \"cloud:8080\" contains invalid
        // character ':'" leak) names the canonical fix verbatim — the
        // `:entrada :port` slot.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "checkout.quero.cloud:8080".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref host, ref reason }
                if host == "checkout.quero.cloud:8080"
                && reason.contains(":entrada :port")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_with_trailing_colon() {
        // Trailing `:` (e.g. an in-progress `:host "example.com:"`
        // edit) — the per-label loop would land it as a deep
        // "label \"com:\" must start and end with an alphanumeric"
        // / "contains invalid character ':'" leak. The top-level
        // `:` arm pre-empts with the canonical `:port` slot
        // diagnostic.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "checkout.quero.cloud:".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref host, ref reason }
                if host == "checkout.quero.cloud:"
                && reason.contains(":entrada :port")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_unbracketed_ipv6_literal() {
        // Unbracketed IPv6 literal — Gateway API v1 Hostname forbids IP
        // literals across the board (peer with `rejects_entrada_host_
        // ipv4_literal` above for the four-label-all-digit IPv4 arm).
        // Before this top-level `:` arm landed the per-label loop
        // surfaced a single-label byte-class diagnostic that named the
        // `:` byte but not the IP-literal prohibition. The top-level
        // `:` arm names both the `:port` slot and the IP-literal
        // prohibition verbatim, so an author whose `:host "2001:..."`
        // value lands here gets a self-locating fix either way.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "2001:db8::1".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref host, ref reason }
                if host == "2001:db8::1"
                && reason.contains("IPv6")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_wildcard_with_port() {
        // Wildcard host with port suffix — the `*.` strip and the
        // per-label loop on `["foo", "quero", "cloud:8080"]` would
        // surface the deep byte-class leak. The top-level `:` arm sits
        // upstream of the `*.` strip, so it names the canonical `:port`
        // fix verbatim regardless of whether the host is wildcard-led.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "*.quero.cloud:8080".into();
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::EntradaHostInvalid { ref host, ref reason }
                if host == "*.quero.cloud:8080"
                && reason.contains(":entrada :port")),
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
    fn rejects_entrada_host_space_names_offending_byte() {
        // Embedded space in the `:entrada :host` axis surfaces the
        // byte-naming diagnostic through the lifted
        // `find_ascii_whitespace_byte` predicate. Peer with the
        // sibling `parse_rejects_leading_whitespace` pins on
        // `supervisor::duration_codec` (a7ae622) — same "the
        // diagnostic carries the offending byte's `0x{b:02x}` shape"
        // discipline extended from the shared duration codec to the
        // Gateway API v1 Hostname axis.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "checkout .quero.cloud".into();
        let err = s.validate().unwrap_err();
        let AplicacaoError::EntradaHostInvalid { reason, .. } = err else {
            panic!("expected EntradaHostInvalid, got {err:?}");
        };
        assert!(
            reason.contains("ASCII whitespace byte"),
            "expected byte-naming diagnostic, got {reason:?}"
        );
        assert!(
            reason.contains("0x20"),
            "expected offending space byte 0x20, got {reason:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_tab_names_offending_byte() {
        // Embedded tab byte in the `:entrada :host` axis — the
        // canonical paste-from-YAML-block-scalar / paste-from-
        // indented-doc footgun. Pins that the lifted predicate covers
        // the full ASCII-whitespace set (`u8::is_ascii_whitespace` —
        // space `0x20`, tab `0x09`, LF `0x0a`, FF `0x0c`, CR `0x0d`),
        // not just the leading-space case the pre-lift `.bytes().any`
        // arm's opaque "must not contain whitespace" reason already
        // covered. Peer with `parse_rejects_tab_byte` on
        // `supervisor::duration_codec` (a7ae622).
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "checkout.\tquero.cloud".into();
        let err = s.validate().unwrap_err();
        let AplicacaoError::EntradaHostInvalid { reason, .. } = err else {
            panic!("expected EntradaHostInvalid, got {err:?}");
        };
        assert!(
            reason.contains("ASCII whitespace byte"),
            "expected byte-naming diagnostic, got {reason:?}"
        );
        assert!(
            reason.contains("0x09"),
            "expected offending tab byte 0x09, got {reason:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_lf_names_offending_byte() {
        // Embedded LF byte in the `:entrada :host` axis — the
        // canonical paste-from-shell-heredoc / paste-from-multiline-
        // doc footgun the caixa-mesh YAML emitter would silently
        // reinterpret at the Gateway API v1 HTTPRoute admission
        // layer (an embedded LF byte in a YAML plain scalar either
        // truncates the value at the emitter or crashes the parser
        // on the k8s-apiserver side). Pins the third representative
        // of the full ASCII-whitespace set through the shared
        // predicate.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "checkout\n.quero.cloud".into();
        let err = s.validate().unwrap_err();
        let AplicacaoError::EntradaHostInvalid { reason, .. } = err else {
            panic!("expected EntradaHostInvalid, got {err:?}");
        };
        assert!(
            reason.contains("ASCII whitespace byte"),
            "expected byte-naming diagnostic, got {reason:?}"
        );
        assert!(
            reason.contains("0x0a"),
            "expected offending LF byte 0x0a, got {reason:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_nbsp_names_offending_codepoint() {
        // Leading NBSP (`U+00A0`, `\u{00A0}`) in the `:entrada :host`
        // axis — the canonical paste-from-typography /
        // paste-from-word-processor footgun. Before the non-ASCII
        // Unicode `White_Space` scan lifted through the shared
        // `find_non_ascii_whitespace_char` predicate, the UTF-8 bytes
        // of NBSP (`0xC2 0xA0`) survived the ASCII byte-scan (neither
        // `0xC2` nor `0xA0` is `u8::is_ascii_whitespace`) and landed
        // on the per-label `bytes[0].is_ascii_alphanumeric()` arm
        // with the far-from-source `label "…" must start and end
        // with an alphanumeric` diagnostic — burying the
        // paste-from-typography origin under a label-shape leak.
        // Peer with the sibling non-ASCII-whitespace pins at
        // `limits::parse_byte_size` (`parse_byte_size_rejects_leading_nbsp`
        // — 1b75b38), `limits::parse_duration`,
        // `limits::parse_millicores`, and the shared duration codec
        // — same "the diagnostic carries the offending Unicode
        // codepoint's `U+XXXX` shape" discipline extended from every
        // typed-magnitude codec to the Gateway API v1 Hostname axis.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "\u{00A0}checkout.quero.cloud".into();
        let err = s.validate().unwrap_err();
        let AplicacaoError::EntradaHostInvalid { reason, .. } = err else {
            panic!("expected EntradaHostInvalid, got {err:?}");
        };
        assert!(
            reason.contains("non-ASCII Unicode whitespace character"),
            "expected non-ASCII codepoint-naming diagnostic, got {reason:?}"
        );
        assert!(
            reason.contains("U+00A0"),
            "expected offending NBSP codepoint U+00A0, got {reason:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_line_separator_names_offending_codepoint() {
        // Trailing LINE SEPARATOR (`U+2028`, `\u{2028}`) in the
        // `:entrada :host` axis — the canonical paste-from-web-doc /
        // paste-from-published-HTML footgun. `char::is_whitespace`
        // returns true for `U+2028` per the Unicode `White_Space`
        // property, so `str::trim` at any downstream site would
        // silently strip it — same drift class as NBSP but on a
        // different codepoint region. Pins the second representative
        // (non-Latin-1 `char::is_whitespace` member) through the
        // shared predicate. Peer with
        // `parse_byte_size_rejects_internal_line_separator` on
        // `limits::parse_byte_size` (1b75b38).
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "checkout.quero.cloud\u{2028}".into();
        let err = s.validate().unwrap_err();
        let AplicacaoError::EntradaHostInvalid { reason, .. } = err else {
            panic!("expected EntradaHostInvalid, got {err:?}");
        };
        assert!(
            reason.contains("non-ASCII Unicode whitespace character"),
            "expected non-ASCII codepoint-naming diagnostic, got {reason:?}"
        );
        assert!(
            reason.contains("U+2028"),
            "expected offending LINE SEPARATOR codepoint U+2028, got {reason:?}"
        );
    }

    #[test]
    fn rejects_entrada_host_ideographic_space_names_offending_codepoint() {
        // Embedded IDEOGRAPHIC SPACE (`U+3000`, `\u{3000}`) between
        // labels in the `:entrada :host` axis — the canonical
        // paste-from-CJK-typography footgun (CJK IMEs default to
        // full-width whitespace when the space bar is pressed in
        // Japanese / Chinese input modes). Pins the third
        // representative of the non-ASCII Unicode `White_Space` set
        // through the shared predicate: the CJK block, distinct from
        // the Latin-1 NBSP `U+00A0` and the punctuation-region LINE
        // SEPARATOR `U+2028` — covering the same axis breadth the
        // sibling `parse_byte_size_rejects_trailing_ideographic_space`
        // (1b75b38) pins on `limits::parse_byte_size`.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().host = "checkout\u{3000}.quero.cloud".into();
        let err = s.validate().unwrap_err();
        let AplicacaoError::EntradaHostInvalid { reason, .. } = err else {
            panic!("expected EntradaHostInvalid, got {err:?}");
        };
        assert!(
            reason.contains("non-ASCII Unicode whitespace character"),
            "expected non-ASCII codepoint-naming diagnostic, got {reason:?}"
        );
        assert!(
            reason.contains("U+3000"),
            "expected offending IDEOGRAPHIC SPACE codepoint U+3000, got {reason:?}"
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
    fn entrada_host_total_length_cap_threads_lifted_render_const() {
        // Cross-crate-side pin: the aplicacao-side `:entrada :host`
        // total-length gate now reads the K8s Gateway API v1 Hostname
        // `maxLength: 253` cap from the lifted
        // [`crate::render::GATEWAY_API_HOSTNAME_MAX_LEN`] canonical source
        // of truth — the same constant every future Gateway-API-Hostname
        // landing site (the M4 `mesh.pleme.io/v1alpha1/Aplicacao` CR
        // materializer's per-host validator, the future per-`Certificate`
        // SAN emitter for cert-manager, the multi-`:entrada`
        // host-collision gate when M4 lands `:entrada` as a `Vec`) reads
        // from. Before the lift, the aplicacao-side reader consumed a
        // private const alias `ENTRADA_HOST_MAX_LEN` sitting at the same
        // 253-byte value as the peer render-side canonical bounds
        // ([`GATEWAY_API_HTTP_PATH_MAX_LEN`], [`DNS_1123_LABEL_MAX_LEN`],
        // [`NATS_SUBJECT_MAX_LEN`], [`WASI_KV_SLOT_MAX_LEN`],
        // [`WIT_IDENT_MAX_LEN`]) but structurally split from them at the
        // module boundary — a future 253-byte drift on either side would
        // silently split into two axes' worth of admission-schema mismatch
        // without a build-time signal. Pin the cap through a fresh 254-
        // byte host that hits the total-length arm, then read the reason
        // for the exact byte count the shared constant carries: any future
        // regression on the lift (a private alias reintroduced, a hard-
        // coded literal at the arm, a mismatch between the aplicacao-side
        // and render-side canonicals) surfaces as this pin's diagnostic
        // failing to match, not as a per-cluster admission rejection far
        // from the caixa.lisp source line.
        let mut s = three_member_spec();
        let over_cap = format!(
            "{}.{}.{}.{}",
            "a".repeat(63),
            "b".repeat(63),
            "c".repeat(63),
            "d".repeat(crate::render::GATEWAY_API_HOSTNAME_MAX_LEN + 1 - 63 * 3 - 3)
        );
        assert_eq!(
            over_cap.len(),
            crate::render::GATEWAY_API_HOSTNAME_MAX_LEN + 1
        );
        s.entrada.as_mut().unwrap().host = over_cap;
        let err = s.validate().unwrap_err();
        match err {
            AplicacaoError::EntradaHostInvalid { reason, .. } => {
                let needle = format!(
                    "max length of {} bytes",
                    crate::render::GATEWAY_API_HOSTNAME_MAX_LEN,
                );
                assert!(
                    reason.contains(&needle),
                    "diagnostic must name the lifted \
                     GATEWAY_API_HOSTNAME_MAX_LEN cap verbatim, got: {reason:?}",
                );
            }
            other => panic!("expected EntradaHostInvalid, got {other:?}"),
        }
    }

    #[test]
    fn entrada_host_per_label_cap_threads_lifted_dns_1123_const() {
        // Peer of [`entrada_host_total_length_cap_threads_lifted_render_const`]
        // on the per-label-cap axis. Before the lift, the aplicacao-side
        // per-label arm consumed a private const alias
        // `ENTRADA_HOST_LABEL_MAX_LEN` sitting at the same 63-byte value
        // as [`crate::render::DNS_1123_LABEL_MAX_LEN`] but structurally
        // split from it at the module boundary — every `.`-separated
        // label in a Gateway API v1 Hostname is a DNS-1123 label under
        // the apiserver's OpenAPI regex `[a-z0-9]([-a-z0-9]*[a-z0-9])?`,
        // so the private alias's 63 and the canonical const's 63 were
        // pinning the same underlying rule twice. Pin the cap through a
        // 64-byte label that hits the per-label arm, then read the reason
        // for the exact byte count the shared constant carries: any
        // future drift on either side (a private alias reintroduced, a
        // hard-coded literal at the arm, a mismatch between the two
        // 63-byte pins) surfaces at this pin's diagnostic rather than at
        // a per-cluster admission rejection whose "field is invalid"
        // opacity misframes the root cause.
        let mut s = three_member_spec();
        let over_cap_label = format!(
            "{}.quero.cloud",
            "x".repeat(crate::render::DNS_1123_LABEL_MAX_LEN + 1),
        );
        s.entrada.as_mut().unwrap().host = over_cap_label;
        let err = s.validate().unwrap_err();
        match err {
            AplicacaoError::EntradaHostInvalid { reason, .. } => {
                let needle = format!(
                    "label max length of {} bytes",
                    crate::render::DNS_1123_LABEL_MAX_LEN,
                );
                assert!(
                    reason.contains(&needle),
                    "diagnostic must name the lifted DNS_1123_LABEL_MAX_LEN \
                     cap verbatim on the per-label arm, got: {reason:?}",
                );
            }
            other => panic!("expected EntradaHostInvalid, got {other:?}"),
        }
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
        // The author-supplied bare-root `:entrada :paths` entry is the
        // same byte-shape the peer emit-side catch-all constant
        // [`crate::GATEWAY_API_DEFAULT_HTTP_ROUTE_PATH`] renders when
        // the author's `:paths` list is empty — sweeping the test-side
        // probe literal onto the lifted const closes the two-axis pin
        // (author-side admit + emit-side canonical fallback) around
        // one `&'static str`, so a future rebrand of the catch-all
        // reaches both consumers by construction. Peer to
        // [`crate::tests::gateway_api_default_http_route_path_pins_canonical_root_literal`]
        // on the canonical-literal pin surface.
        let mut s = three_member_spec();
        s.entrada.as_mut().unwrap().paths = vec![crate::GATEWAY_API_DEFAULT_HTTP_ROUTE_PATH.into()];
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
    fn placement_strategy_variants_serialize_to_lifted_scalar_values() {
        // The fail-before-pass-after pin: pre-lift there was no
        // single-source binding between the [`PlacementStrategy`]
        // variant name the `Serialize` derive emits and the byte-
        // string every downstream cluster-side dispatcher (the
        // `lareira-fleet-programs` aggregator's per-entry strategy
        // branch, the future `app-operator` reconciler, the M3
        // Adaptive compression pass's per-strategy weighting) probes
        // verbatim under [`crate::M3_PLACEMENT_KEY_ESTRATEGIA`]. A
        // future `#[serde(rename_all = "kebab-case")]` attribute on
        // the enum — or a variant rename in the source — would
        // silently rebrand the emitted scalar under one spelling
        // while every downstream dispatcher still probed the other,
        // with the failure surfacing at the aggregator's dispatch
        // step or the operator's reconcile posture (workloads coming
        // up under the `default()` `Replicated` arm rather than the
        // typed slot's declared strategy) far from the source
        // rebrand commit and with no field naming the drift. Pinning
        // the two paths (the `Serialize` derive's serialized string
        // AND the [`PlacementStrategy::as_str`] helper) to the same
        // three lifted [`crate::M3_PLACEMENT_ESTRATEGIA_SINGLE_NODE`]
        // / [`crate::M3_PLACEMENT_ESTRATEGIA_REPLICATED`] /
        // [`crate::M3_PLACEMENT_ESTRATEGIA_SHARDED`] byte-strings
        // makes any future drift on either endpoint fail here at
        // caixa-core build time.
        for (variant, expected) in [
            (
                PlacementStrategy::SingleNode,
                crate::render::M3_PLACEMENT_ESTRATEGIA_SINGLE_NODE,
            ),
            (
                PlacementStrategy::Replicated,
                crate::render::M3_PLACEMENT_ESTRATEGIA_REPLICATED,
            ),
            (
                PlacementStrategy::Sharded,
                crate::render::M3_PLACEMENT_ESTRATEGIA_SHARDED,
            ),
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(
                json,
                format!("\"{expected}\""),
                "PlacementStrategy::{variant:?} must serialize to {expected:?}"
            );
            assert_eq!(
                variant.as_str(),
                expected,
                "PlacementStrategy::{variant:?}.as_str() must return the lifted \
                 M3_PLACEMENT_ESTRATEGIA_* constant"
            );
        }
    }

    #[test]
    fn placement_strategy_display_routes_through_as_str_helper() {
        // The fail-before-pass-after pin: pre-lift the sibling
        // OTP-shape typed enums [`crate::supervisor::RestartStrategy`]
        // / [`crate::supervisor::RestartPolicy`] both carried a stable
        // [`std::fmt::Display`] surface via their
        // `#[discriminant(also_display)]` gen-platform derive, but
        // [`PlacementStrategy`] did not — every consumer reaching for
        // a strategy byte-string past the wire format had to pick
        // between three paths ([`PlacementStrategy::as_str`], the
        // `Serialize` derive's serialized string, or `format!("{v:?}")`
        // on the `Debug` derive), any two of which a future variant
        // rename or `#[serde(rename_all = "kebab-case")]` attribute
        // would silently desynchronize. Wiring [`std::fmt::Display`]
        // through [`PlacementStrategy::as_str`] closes the third path:
        // every `format!("{v}")` call reaches the same lifted
        // [`crate::M3_PLACEMENT_ESTRATEGIA_*`] const the wire format
        // and the [`PlacementStrategy::as_str`] helper already route
        // through, so a future variant rename lands at exactly one
        // place. Pin the routing here so a future
        // `impl std::fmt::Display for PlacementStrategy` reimplementation
        // that hand-rolls the arms instead of delegating to
        // [`PlacementStrategy::as_str`] fails at caixa-core build time.
        for variant in [
            PlacementStrategy::SingleNode,
            PlacementStrategy::Replicated,
            PlacementStrategy::Sharded,
        ] {
            assert_eq!(
                variant.to_string(),
                variant.as_str(),
                "PlacementStrategy::{variant:?} Display must route through \
                 PlacementStrategy::as_str (single source of truth: the lifted \
                 M3_PLACEMENT_ESTRATEGIA_* const the wire format also emits)"
            );
        }
    }

    #[test]
    fn placement_strategy_display_matches_serialized_wire_byte_string() {
        // The fail-before-pass-after pin on the second half of the
        // three-path convergence: `Display` (user-facing text) agrees
        // byte-for-byte with the `Serialize` derive's wire format
        // (canonical camelCase-schema `M3_PLACEMENT_KEY_ESTRATEGIA`
        // scalar) on every variant. Pre-lift the two paths were
        // structurally independent — a future
        // `#[serde(rename_all = "kebab-case")]` attribute on the enum
        // would silently rebrand the emitted wire scalar
        // (`single-node`, `replicated`, `sharded`) while every consumer
        // that pretty-prints the strategy (the M3 diagnostic templates,
        // the future `feira app graph` per-Aplicacao strategy line,
        // the future M4 CR materializer's admission-webhook rejection
        // body) would still emit the TitleCase form the `as_str` /
        // `Display` route returns, with the mismatch surfacing at
        // consumer parse time / operator dispatch time far from the
        // source rebrand commit. Pin the two paths byte-for-byte here
        // so any future serde-attribute or variant-rename drift is a
        // caixa-core-build-time test failure at this call, not a
        // silent per-consumer dispatch miss.
        for variant in [
            PlacementStrategy::SingleNode,
            PlacementStrategy::Replicated,
            PlacementStrategy::Sharded,
        ] {
            let wire = serde_json::to_string(&variant).unwrap();
            // Strip the outer `"…"` the JSON string form carries — the
            // wire scalar the K8s / YAML apiserver consumes is the
            // enclosed byte-string, not the quote wrapper.
            let unquoted = wire
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .expect("serialized PlacementStrategy is a JSON string");
            assert_eq!(
                variant.to_string(),
                unquoted,
                "PlacementStrategy::{variant:?} Display byte-string must match the \
                 Serialize derive's wire byte-string (three-path convergence: \
                 Display + as_str + Serialize all resolve to the same \
                 M3_PLACEMENT_ESTRATEGIA_* const)"
            );
        }
    }

    #[test]
    fn placement_without_clusters_diagnostic_carries_strategy_display_byte_string() {
        // Pin the M3 diagnostic template routes through the typed
        // [`PlacementStrategy`] Display byte-string (rebound from the
        // prior `{estrategia:?}` `Debug` route). Pre-lift the two
        // routes emitted identical bytes (the `Debug` derive on a
        // unit variant emits the variant name verbatim, exactly what
        // `as_str` returns), but the two paths were structurally
        // independent — a future `#[serde(rename_all = "…")]`
        // attribute or variant rename would coordinate the wire /
        // `Display` / `as_str` triple through the lifted const but
        // leave the `Debug` route on the compiler-derived variant name,
        // silently desynchronizing the diagnostic byte-string from the
        // wire byte-string. Rebinding the template onto `Display`
        // ties the diagnostic to the same lifted
        // [`crate::M3_PLACEMENT_ESTRATEGIA_*`] const the wire format
        // emits — drift becomes structurally impossible. Pin the
        // byte-string here so a future edit that reverts the template
        // to `{estrategia:?}` is caught at caixa-core test time, not
        // at consumer dispatch time.
        for (variant, expected_scalar) in [
            (
                PlacementStrategy::SingleNode,
                crate::render::M3_PLACEMENT_ESTRATEGIA_SINGLE_NODE,
            ),
            (
                PlacementStrategy::Replicated,
                crate::render::M3_PLACEMENT_ESTRATEGIA_REPLICATED,
            ),
            (
                PlacementStrategy::Sharded,
                crate::render::M3_PLACEMENT_ESTRATEGIA_SHARDED,
            ),
        ] {
            let err = AplicacaoError::PlacementWithoutClusters {
                estrategia: variant,
            };
            let msg = err.to_string();
            assert!(
                msg.starts_with(&format!(":placement {expected_scalar} requires")),
                "PlacementWithoutClusters diagnostic for {variant:?} must open \
                 with the lifted `{expected_scalar}` scalar via Display; got {msg:?}"
            );
        }
    }

    #[test]
    fn shard_key_on_non_sharded_diagnostic_carries_strategy_display_byte_string() {
        // Peer of
        // [`placement_without_clusters_diagnostic_carries_strategy_display_byte_string`]
        // on the second M3 diagnostic that carries the typed
        // [`PlacementStrategy`] in its `#[error(…)]` template. Both
        // diagnostics now route the strategy scalar through the same
        // [`std::fmt::Display`] surface, tying the diagnostic
        // byte-string to the lifted [`crate::M3_PLACEMENT_ESTRATEGIA_*`]
        // const set the wire format also emits. The two non-Sharded
        // arms are exercised here (the diagnostic exists to flag a
        // `:shard-key` slot the current strategy will never consume);
        // the peer `Sharded` arm never reaches this diagnostic (the
        // `Sharded` strategy consumes `:shard-key` — the
        // [`AplicacaoError::ShardedWithoutKey`] arm reports the missing
        // slot instead).
        for (variant, expected_scalar) in [
            (
                PlacementStrategy::SingleNode,
                crate::render::M3_PLACEMENT_ESTRATEGIA_SINGLE_NODE,
            ),
            (
                PlacementStrategy::Replicated,
                crate::render::M3_PLACEMENT_ESTRATEGIA_REPLICATED,
            ),
        ] {
            let err = AplicacaoError::ShardKeyOnNonSharded {
                estrategia: variant,
                shard_key: "$tenantId".into(),
            };
            let msg = err.to_string();
            assert!(
                msg.starts_with(&format!(":placement {expected_scalar} carries")),
                "ShardKeyOnNonSharded diagnostic for {variant:?} must open with \
                 the lifted `{expected_scalar}` scalar via Display; got {msg:?}"
            );
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
    fn rejects_policy_retries_above_cap() {
        // The fail-before-pass-after pin: `Some(11)` is structurally
        // one past the [`POLICY_RETRIES_MAX`] ceiling and silently
        // passed validate on every pre-gate codebase because the
        // typed slot's only check was the zero-floor arm. The
        // thundering-herd amplification vector only surfaced at the
        // runtime substrate (Envoy / Cilium L7 retry overlay)
        // far from the source caixa.lisp with no field naming the
        // offending policy.
        let mut s = three_member_spec();
        s.politicas.retries = Some(POLICY_RETRIES_MAX + 1);
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyRetriesExceedsCap {
                retries: POLICY_RETRIES_MAX + 1
            }
        );
    }

    #[test]
    fn rejects_policy_retries_far_above_cap() {
        // The `u32::MAX` worst case — the four-billion-retry policy
        // a typo (`(:retries 4294967295)`) or struct-literal
        // copy-paste lands in the slot. Pin the cap arm's coverage
        // explicitly across the full `u32` overflow so a future
        // relaxation that drops the upper bound surfaces here.
        let mut s = three_member_spec();
        s.politicas.retries = Some(u32::MAX);
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyRetriesExceedsCap { retries: u32::MAX }
        );
    }

    #[test]
    fn accepts_policy_retries_at_cap() {
        // The boundary value — exactly [`POLICY_RETRIES_MAX`] —
        // must validate. The cap is inclusive on the top edge,
        // matching the [`crate::LIMITS_MEMORY_WASM32_MAX_BYTES`]
        // discipline on the sibling [`crate::LimitsSpec::memory`]
        // axis. Pin the boundary explicitly so a future off-by-one
        // tightening (`>= POLICY_RETRIES_MAX` instead of `>`)
        // surfaces here as a test failure rather than a silent
        // contract narrowing.
        let mut s = three_member_spec();
        s.politicas.retries = Some(POLICY_RETRIES_MAX);
        s.validate()
            .expect("retries == POLICY_RETRIES_MAX must validate");
    }

    #[test]
    fn accepts_policy_retries_typical_values() {
        // The full inclusive `1..=POLICY_RETRIES_MAX` sweep —
        // every value in the validated set must pass. The
        // Envoy / Istio production-playbook recommendation band
        // (`num_retries ≤ 5`) and the AWS App Mesh schema cap
        // (`maxRetries ≤ 10`) both lie within this set.
        for r in 1..=POLICY_RETRIES_MAX {
            let mut s = three_member_spec();
            s.politicas.retries = Some(r);
            s.validate()
                .unwrap_or_else(|e| panic!("retries={r} must validate; got {e:?}"));
        }
    }

    #[test]
    fn policy_retries_zero_takes_precedence_over_cap() {
        // The cross-arm ordering pin: `Some(0)` is structurally
        // outside both `1..` (zero-floor) and `..=POLICY_RETRIES_MAX`
        // (cap), but the zero-floor diagnostic is the more
        // self-locating one (it directly names the omit-axis
        // remediation), so the validate gate must fire on zero
        // first. Pin the order so a future refactor that reorders
        // the arms surfaces here as a test failure rather than a
        // silent diagnostic regression. Same shape every other
        // zero-then-shape ordering on this surface uses
        // ([`AplicacaoError::PolicyTimeoutZero`] then
        // [`AplicacaoError::PolicyTimeoutNotCanonical`];
        // [`AplicacaoError::PolicyBreakerZeroWindow`] then
        // [`AplicacaoError::PolicyBreakerWindowNotCanonical`]).
        let mut s = three_member_spec();
        s.politicas.retries = Some(0);
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyRetriesZero,
            "Some(0) must surface the zero-floor diagnostic, not the cap diagnostic"
        );
    }

    #[test]
    fn policy_retries_cap_diagnostic_carries_offending_value() {
        // The diagnostic-shape pin: the offending `u32` is carried
        // verbatim into the [`AplicacaoError::PolicyRetriesExceedsCap`]
        // variant so the surfaced error message names the value the
        // author wrote (`":politicas :retries (47) exceeds the
        // mesh-policy ceiling …"`), not just the cap. Same
        // self-locating diagnostic shape every other typed-cap arm
        // on this surface carries
        // ([`crate::LimitsError::MemoryExceedsWasm32Cap`] carries the
        // offending byte count verbatim).
        let mut s = three_member_spec();
        s.politicas.retries = Some(47);
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::PolicyRetriesExceedsCap { retries: 47 }),
            "got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("47"),
            ":politicas :retries cap diagnostic must carry the offending value verbatim (got: {msg})"
        );
    }

    #[test]
    fn policy_retries_cap_is_aws_app_mesh_aligned() {
        // The [`POLICY_RETRIES_MAX`] constant pins the value at 10,
        // matching AWS App Mesh's `gRPCRouteRetryPolicy.maxRetries`
        // schema cap — the only upstream mesh-policy schema that
        // documents an explicit hard cap. Pinning the literal value
        // here surfaces a future drift (a relaxation to 20, a
        // tightening to 5) as a deliberate test edit, not a silent
        // contract narrowing.
        assert_eq!(POLICY_RETRIES_MAX, 10);
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
    fn rejects_circuit_breaker_max_failures_above_cap() {
        // The fail-before-pass-after pin: `1001` is structurally one
        // past the [`POLICY_BREAKER_MAX_FAILURES_MAX`] ceiling and
        // silently passed validate on every pre-gate codebase
        // because the typed slot's only check was the zero-floor
        // arm. The breaker-no-op vector only surfaced at the runtime
        // substrate (Envoy / Cilium L7 outlier-detection overlay)
        // far from the source caixa.lisp with no field naming the
        // offending policy.
        let mut s = three_member_spec();
        s.politicas.circuit_breaker = Some(CircuitBreaker {
            max_failures: POLICY_BREAKER_MAX_FAILURES_MAX + 1,
            window: Duration::from_secs(60),
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyBreakerMaxFailuresExceedsCap {
                max_failures: POLICY_BREAKER_MAX_FAILURES_MAX + 1,
            }
        );
    }

    #[test]
    fn rejects_circuit_breaker_max_failures_far_above_cap() {
        // The `u32::MAX` worst case — the four-billion-failure
        // threshold a typo (`(:max-failures 4294967295)`) or a
        // struct-literal copy-paste lands in the slot. Pin the cap
        // arm's coverage explicitly across the full `u32` overflow
        // so a future relaxation that drops the upper bound surfaces
        // here.
        let mut s = three_member_spec();
        s.politicas.circuit_breaker = Some(CircuitBreaker {
            max_failures: u32::MAX,
            window: Duration::from_secs(60),
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyBreakerMaxFailuresExceedsCap {
                max_failures: u32::MAX,
            }
        );
    }

    #[test]
    fn accepts_circuit_breaker_max_failures_at_cap() {
        // The boundary value — exactly
        // [`POLICY_BREAKER_MAX_FAILURES_MAX`] — must validate. The
        // cap is inclusive on the top edge, matching the
        // [`POLICY_RETRIES_MAX`] / [`crate::LIMITS_MEMORY_WASM32_MAX_BYTES`]
        // discipline on the sibling capped axes. Pin the boundary
        // explicitly so a future off-by-one tightening
        // (`>= POLICY_BREAKER_MAX_FAILURES_MAX` instead of `>`)
        // surfaces here as a test failure rather than a silent
        // contract narrowing.
        let mut s = three_member_spec();
        s.politicas.circuit_breaker = Some(CircuitBreaker {
            max_failures: POLICY_BREAKER_MAX_FAILURES_MAX,
            window: Duration::from_secs(60),
        });
        s.validate()
            .expect("max_failures == POLICY_BREAKER_MAX_FAILURES_MAX must validate");
    }

    #[test]
    fn accepts_circuit_breaker_max_failures_typical_values() {
        // The documented production-playbook band positive-control
        // sweep — every value Hystrix / Istio / Envoy / Polly /
        // Resilience4j recommend (5..=50) must pass, plus a sweep
        // through the hyperscale band (100, 500, 1000) the cap
        // accepts. Pin the inclusive validated set explicitly so a
        // future tightening of the ceiling surfaces here.
        for n in [1u32, 5, 10, 20, 50, 100, 500, 1000] {
            let mut s = three_member_spec();
            s.politicas.circuit_breaker = Some(CircuitBreaker {
                max_failures: n,
                window: Duration::from_secs(60),
            });
            s.validate()
                .unwrap_or_else(|e| panic!("max_failures={n} must validate; got {e:?}"));
        }
    }

    #[test]
    fn circuit_breaker_zero_max_failures_takes_precedence_over_cap() {
        // The cross-arm ordering pin: `0` is structurally outside
        // both `1..` (zero-floor) and `..=POLICY_BREAKER_MAX_FAILURES_MAX`
        // (cap), but the zero-floor diagnostic is the more
        // self-locating one (it directly names the omit-axis
        // remediation), so the validate gate must fire on zero
        // first. Same shape every other zero-then-shape ordering on
        // this surface uses
        // ([`AplicacaoError::PolicyRetriesZero`] then
        // [`AplicacaoError::PolicyRetriesExceedsCap`];
        // [`AplicacaoError::PolicyTimeoutZero`] then
        // [`AplicacaoError::PolicyTimeoutNotCanonical`]).
        let mut s = three_member_spec();
        s.politicas.circuit_breaker = Some(CircuitBreaker {
            max_failures: 0,
            window: Duration::from_secs(60),
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyBreakerZeroFailures,
            "max_failures == 0 must surface the zero-floor diagnostic, not the cap diagnostic"
        );
    }

    #[test]
    fn circuit_breaker_max_failures_cap_takes_precedence_over_window_gates() {
        // The cross-arm ordering pin between the cap and the
        // sibling `:window` gates (zero-window, canonical-window).
        // A breaker carrying both an over-cap `max_failures` AND a
        // structurally invalid window (zero, sub-ms) must surface
        // the cap diagnostic first — the cap arm is wired
        // immediately after the zero-failure arm and strictly
        // before the window arms, so the offending value the
        // diagnostic names matches the order the author would
        // discover the gates by reading top-to-bottom through
        // [`AplicacaoSpec::validate_politicas`]. Pin the order so a
        // future refactor that reorders the arms surfaces here as a
        // test failure rather than a silent diagnostic regression.
        let mut s = three_member_spec();
        s.politicas.circuit_breaker = Some(CircuitBreaker {
            max_failures: POLICY_BREAKER_MAX_FAILURES_MAX + 1,
            window: Duration::ZERO,
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyBreakerMaxFailuresExceedsCap {
                max_failures: POLICY_BREAKER_MAX_FAILURES_MAX + 1,
            },
            "over-cap max_failures must surface the cap diagnostic before any window-axis diagnostic"
        );
    }

    #[test]
    fn policy_breaker_max_failures_cap_diagnostic_carries_offending_value() {
        // The diagnostic-shape pin: the offending `u32` is carried
        // verbatim into the
        // [`AplicacaoError::PolicyBreakerMaxFailuresExceedsCap`]
        // variant so the surfaced error message names the value the
        // author wrote (`":politicas :circuit-breaker :max-failures
        // (50000) exceeds the mesh-policy ceiling …"`), not just
        // the cap. Same self-locating diagnostic shape every other
        // typed-cap arm on this surface carries
        // ([`AplicacaoError::PolicyRetriesExceedsCap`] carries the
        // offending retry count verbatim,
        // [`crate::LimitsError::MemoryExceedsWasm32Cap`] carries the
        // offending byte count verbatim).
        let mut s = three_member_spec();
        s.politicas.circuit_breaker = Some(CircuitBreaker {
            max_failures: 50_000,
            window: Duration::from_secs(60),
        });
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::PolicyBreakerMaxFailuresExceedsCap {
                    max_failures: 50_000
                }
            ),
            "got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("50000"),
            ":politicas :circuit-breaker :max-failures cap diagnostic must carry the offending value verbatim (got: {msg})"
        );
    }

    #[test]
    fn policy_breaker_max_failures_cap_pins_canonical_value() {
        // The [`POLICY_BREAKER_MAX_FAILURES_MAX`] constant pins the
        // value at 1000 — an order of magnitude above every
        // documented production-playbook recommendation band
        // (Hystrix `requestVolumeThreshold` default 20, Istio
        // `outlierDetection.consecutive5xxErrors` default 5, Envoy
        // `outlier_detection.consecutive_5xx` default 5, Polly /
        // Resilience4j typical 5..=50) and below the
        // clearly-pathological "effectively no protection" floor
        // (10_000, 100_000, u32::MAX). Pinning the literal value
        // here surfaces a future drift (a relaxation to 10_000, a
        // tightening to 100) as a deliberate test edit, not a
        // silent contract narrowing.
        assert_eq!(POLICY_BREAKER_MAX_FAILURES_MAX, 1000);
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
    fn rejects_policy_rate_limit_above_cap() {
        // The fail-before-pass-after pin: `rate = POLICY_RATE_LIMIT_MAX + 1`
        // is structurally one past the cap and silently passed
        // validate on every pre-gate codebase because the typed slot's
        // only `rate` check was the zero-floor arm. The no-op-limiter
        // shape only surfaced at the runtime substrate (Envoy's
        // `local_rate_limit.token_bucket.max_tokens`, the future
        // Cilium L7 rate-limit overlay) far from the source caixa.lisp
        // with no field naming the offending policy.
        let mut s = three_member_spec();
        s.politicas.rate_limit = Some(RateLimit {
            rate: POLICY_RATE_LIMIT_MAX + 1,
            window: Duration::from_secs(1),
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyRateLimitExceedsCap {
                rate: POLICY_RATE_LIMIT_MAX + 1
            }
        );
    }

    #[test]
    fn rejects_policy_rate_limit_far_above_cap() {
        // The `u32::MAX` worst case — the four-billion-token rate-limit
        // a typo (`(:rate-limit "4294967295/s")`) or struct-literal
        // copy-paste lands in the slot. Pin the cap arm's coverage
        // explicitly across the full `u32` overflow so a future
        // relaxation that drops the upper bound surfaces here. Peer to
        // `rejects_policy_retries_far_above_cap` on the sibling
        // `:retries` axis and `rejects_policy_breaker_max_failures_far_above_cap`
        // on the sibling `:max-failures` axis.
        let mut s = three_member_spec();
        s.politicas.rate_limit = Some(RateLimit {
            rate: u32::MAX,
            window: Duration::from_secs(1),
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyRateLimitExceedsCap { rate: u32::MAX }
        );
    }

    #[test]
    fn accepts_policy_rate_limit_at_cap() {
        // The boundary value — exactly [`POLICY_RATE_LIMIT_MAX`] —
        // must validate. The cap is inclusive on the top edge, matching
        // every other typed upper bound in this crate
        // ([`POLICY_RETRIES_MAX`], [`POLICY_BREAKER_MAX_FAILURES_MAX`],
        // [`crate::LIMITS_MEMORY_WASM32_MAX_BYTES`]). Pin the boundary
        // across all three canonical windows so a future off-by-one
        // tightening (`>= POLICY_RATE_LIMIT_MAX` instead of `>`) or a
        // window-conditional cap surfaces here as a test failure rather
        // than a silent contract narrowing.
        for secs in [1u64, 60, 3600] {
            let mut s = three_member_spec();
            s.politicas.rate_limit = Some(RateLimit {
                rate: POLICY_RATE_LIMIT_MAX,
                window: Duration::from_secs(secs),
            });
            s.validate().unwrap_or_else(|e| {
                panic!("rate == POLICY_RATE_LIMIT_MAX must validate (window={secs}s); got {e:?}",)
            });
        }
    }

    #[test]
    fn accepts_policy_rate_limit_typical_values() {
        // The documented production-playbook recommendation band —
        // Envoy / Istio / Kong / NGINX 10..=10_000 RPS, Cloudflare /
        // AWS API Gateway 10_000..=100_000 per-minute, Cloudflare
        // Enterprise ~1M per-hour. Every value in the validated set
        // must pass; pin the band explicitly so a future tightening
        // surfaces here.
        for rate in [1u32, 10, 100, 1_000, 10_000, 100_000, 1_000_000] {
            for secs in [1u64, 60, 3600] {
                let mut s = three_member_spec();
                s.politicas.rate_limit = Some(RateLimit {
                    rate,
                    window: Duration::from_secs(secs),
                });
                s.validate().unwrap_or_else(|e| {
                    panic!("rate={rate} window={secs}s must validate; got {e:?}")
                });
            }
        }
    }

    #[test]
    fn policy_rate_limit_zero_takes_precedence_over_cap() {
        // The cross-arm ordering pin: `rate == 0` is structurally
        // outside both `1..` (zero-floor) and `..=POLICY_RATE_LIMIT_MAX`
        // (cap), but the zero-floor diagnostic is the more
        // self-locating one (it directly names the omit-axis
        // remediation). Pin the order so a future refactor that
        // reorders the arms surfaces here as a test failure rather
        // than a silent diagnostic regression. Same shape every other
        // zero-then-cap ordering on this surface uses
        // ([`AplicacaoError::PolicyRetriesZero`] then
        // [`AplicacaoError::PolicyRetriesExceedsCap`];
        // [`AplicacaoError::PolicyBreakerZeroFailures`] then
        // [`AplicacaoError::PolicyBreakerMaxFailuresExceedsCap`]).
        let mut s = three_member_spec();
        s.politicas.rate_limit = Some(RateLimit {
            rate: 0,
            window: Duration::from_secs(1),
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyRateLimitZero,
            "rate == 0 must surface the zero-floor diagnostic, not the cap diagnostic"
        );
    }

    #[test]
    fn policy_rate_limit_cap_takes_precedence_over_non_canonical_window() {
        // Two-axis-bad pin: rate above cap *and* window non-canonical.
        // The validate gate must fire on the rate cap first — the
        // amplification-shape (no-op limiter) diagnostic is the more
        // fundamental one; the window-canonical diagnostic is the
        // narrower codec-round-trip shape. Pin the ordering so a future
        // refactor that reorders the rate-then-window check arms
        // surfaces here as a test failure rather than a silent
        // diagnostic regression.
        let mut s = three_member_spec();
        s.politicas.rate_limit = Some(RateLimit {
            rate: POLICY_RATE_LIMIT_MAX + 1,
            window: Duration::from_secs(45),
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyRateLimitExceedsCap {
                rate: POLICY_RATE_LIMIT_MAX + 1
            },
            "above-cap rate must surface the cap diagnostic, not the window diagnostic"
        );
    }

    #[test]
    fn policy_rate_limit_cap_diagnostic_carries_offending_value() {
        // The diagnostic-shape pin: the offending `u32` is carried
        // verbatim into the [`AplicacaoError::PolicyRateLimitExceedsCap`]
        // variant so the surfaced error message names the value the
        // author wrote (`":politicas :rate-limit rate (5000000) exceeds
        // the mesh-policy ceiling …"`), not just the cap. Same
        // self-locating diagnostic shape every other typed-cap arm on
        // this surface carries ([`AplicacaoError::PolicyRetriesExceedsCap`]
        // carries the offending retries count verbatim,
        // [`AplicacaoError::PolicyBreakerMaxFailuresExceedsCap`] carries
        // the offending failure count verbatim).
        let mut s = three_member_spec();
        s.politicas.rate_limit = Some(RateLimit {
            rate: 5_000_000,
            window: Duration::from_secs(1),
        });
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::PolicyRateLimitExceedsCap { rate: 5_000_000 }
            ),
            "got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("5000000"),
            ":politicas :rate-limit cap diagnostic must carry the offending value verbatim (got: {msg})"
        );
    }

    #[test]
    fn policy_rate_limit_cap_pins_canonical_value() {
        // The [`POLICY_RATE_LIMIT_MAX`] constant pins the value at
        // 1_000_000 — two-to-three orders of magnitude above every
        // documented production-playbook recommendation band (Envoy /
        // Istio / Kong / NGINX 10..=10_000 RPS, Cloudflare / AWS API
        // Gateway 10_000..=100_000 per-minute) and below the
        // clearly-pathological "paste-from-binary blob" floor
        // (100_000_000, u32::MAX). Pinning the literal value here
        // surfaces a future drift (a relaxation to 10_000_000, a
        // tightening to 100_000) as a deliberate test edit, not a
        // silent contract narrowing.
        assert_eq!(POLICY_RATE_LIMIT_MAX, 1_000_000);
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
    fn rate_limit_unit_table_projections_are_mutual_inverses() {
        // Bidirection pin against the lifted [`RATE_LIMIT_UNIT_TABLE`]
        // (the canonical `{"s" ↔ 1s, "m" ↔ 60s, "h" ↔ 3600s}`
        // bijection every consumer of the rate-limit unit surface
        // reads from). Until this table landed the three (str,
        // Duration) pairs sat scattered across four peer sites —
        // `rate_limit_codec::parse`'s `match unit` arm, `render`'s
        // `if secs == 1 { "s" } else if …` cascade, and
        // `is_canonical_rate_limit_window`'s `secs == 1 || 60 ||
        // 3600` disjunction — each carrying its own hand-written copy
        // with no compile-time link between them. A future
        // rate-limit-unit addition (a `"d"` day suffix, a `"ms"`
        // sub-second window) would have to be threaded through all
        // three sites in lockstep or a drift would silently split
        // the accepted-window set. Lifting the pairs onto one const
        // + two projection helpers collapses the surface: this pin
        // enshrines that both projections agree on every table row
        // and neither leaks a spurious entry the other doesn't
        // recognize.
        for (unit, secs) in [("s", 1u64), ("m", 60), ("h", 3600)] {
            let window = super::rate_limit_window_from_unit(unit)
                .unwrap_or_else(|| panic!("canonical unit {unit:?} must resolve to a Duration"));
            assert_eq!(
                window,
                Duration::from_secs(secs),
                "unit {unit:?} must resolve to {secs}s"
            );
            assert_eq!(
                super::rate_limit_window_unit(window),
                Some(unit),
                "Duration({secs}s) must render as {unit:?}"
            );
        }
        // Non-table units yield None on the `unit → Duration`
        // projection — a future `"d"` addition to the table would
        // flip this arm; today it pins the current three-row table's
        // rejection semantics.
        assert!(super::rate_limit_window_from_unit("d").is_none());
        assert!(super::rate_limit_window_from_unit("ms").is_none());
        assert!(super::rate_limit_window_from_unit("").is_none());
        // Non-table Durations yield None on the `Duration → unit`
        // projection — pins that the two projections agree on the
        // "not in the table" semantic too, so a drift where the
        // parse-side accepts a value the render-side can't emit is
        // a build error at the two-arm pair, not a silent codec
        // round-trip break.
        assert!(super::rate_limit_window_unit(Duration::from_secs(2)).is_none());
        assert!(super::rate_limit_window_unit(Duration::from_secs(86_400)).is_none());
        assert!(super::rate_limit_window_unit(Duration::from_millis(1500)).is_none());
    }

    #[test]
    fn rejects_policy_timeout_sub_millisecond() {
        // A purely sub-millisecond `Duration` (`from_micros(500)` =
        // 500_000 ns) is not the zero `Duration` — the `is_zero()`
        // arm passes — but `as_millis() == 0`, so the shared codec's
        // `render` arm returns the literal `"0s"`, which the
        // codec's `parse` arm then deserializes as `Duration::ZERO`
        // and the `PolicyTimeoutZero` zero-floor gate would reject
        // on re-validate. Pin the rejection at the typed slot's
        // canonical-floor gate so the round-trip break surfaces at
        // validate time, naming the offending `Duration`, rather
        // than at the next serialize → deserialize round-trip far
        // from the source `caixa.lisp`.
        let mut s = three_member_spec();
        let timeout = Duration::from_micros(500);
        s.politicas.timeout = Some(timeout);
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyTimeoutNotCanonical { timeout }
        );
    }

    #[test]
    fn rejects_policy_timeout_non_integer_millisecond() {
        // A `Duration` with non-integer-millisecond residue
        // (`from_micros(1500)` = 1.5 ms = 1_500_000 ns) renders
        // through the shared codec's `render` arm as `"1ms"` (the
        // `as_millis()` floor truncates), which the codec's `parse`
        // arm then deserializes as `Duration::from_millis(1)` =
        // 1_000_000 ns — silently *different* from the original.
        // Pin the rejection so this round-trip break surfaces at
        // validate time, where the offending `Duration` is named,
        // rather than as a silent value-laundered round-trip on the
        // next codec round-trip.
        let mut s = three_member_spec();
        let timeout = Duration::from_micros(1500);
        s.politicas.timeout = Some(timeout);
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyTimeoutNotCanonical { timeout }
        );
    }

    #[test]
    fn accepts_policy_timeout_integer_millisecond_forms() {
        // The codec's accepted set — integer multiples of 1ms — is
        // the typed slot's accepted set: `1ms`, `500ms`, `30s`, `2m`,
        // `1h` all pass the canonical gate. Pin the canonical-forms
        // sweep so a future tightening of the codec's grammar (e.g.
        // dropping `:ms`) surfaces here as a test failure rather
        // than a silent contract narrowing on the typed slot.
        for timeout in [
            Duration::from_millis(1),
            Duration::from_millis(500),
            Duration::from_millis(1500),
            Duration::from_secs(30),
            Duration::from_secs(120),
            Duration::from_secs(3600),
        ] {
            let mut s = three_member_spec();
            s.politicas.timeout = Some(timeout);
            s.validate()
                .expect("integer-millisecond :timeout must validate");
        }
    }

    #[test]
    fn policy_timeout_zero_takes_precedence_over_canonical() {
        // `Duration::ZERO` carries `subsec_nanos() == 0` and would
        // pass the canonical-millisecond gate; the more self-locating
        // `PolicyTimeoutZero` arm (which names the omit-axis
        // remediation directly) must fire first. Pin the ordering so
        // a future refactor that reorders the arms surfaces here as a
        // test failure rather than a silent diagnostic regression.
        let mut s = three_member_spec();
        s.politicas.timeout = Some(Duration::ZERO);
        assert_eq!(s.validate().unwrap_err(), AplicacaoError::PolicyTimeoutZero);
    }

    #[test]
    fn policy_timeout_canonical_diagnostic_carries_offending_duration() {
        // The diagnostic envelope carries the offending `Duration`
        // verbatim so the author can grep their `caixa.lisp` for
        // `:timeout "<value>"` and fix it in one edit. Same
        // diagnostic shape every other typed-slot canonical-form
        // gate (`PolicyRateLimitWindowNotCanonical`) uses on the
        // peer `:rate-limit :window` axis.
        let mut s = three_member_spec();
        let timeout = Duration::from_nanos(1_000_001);
        s.politicas.timeout = Some(timeout);
        match s.validate().unwrap_err() {
            AplicacaoError::PolicyTimeoutNotCanonical { timeout: t } => {
                assert_eq!(t, timeout, "diagnostic must carry the offending Duration");
            }
            other => panic!("expected PolicyTimeoutNotCanonical, got {other:?}"),
        }
    }

    #[test]
    fn rejects_policy_timeout_above_cap() {
        // The fail-before-pass-after pin: 3601s = 1h + 1s is
        // structurally one canonical-tick past the
        // [`POLICY_TIMEOUT_MAX`] ceiling (1h = 3600s) — an
        // integer-millisecond magnitude the canonical-form arm above
        // accepts cleanly, that the codec round-trips losslessly as
        // `"3601s"`, and that silently passed validate on every
        // pre-gate codebase because the typed slot's only checks were
        // the zero-floor and canonical-form arms. The mesh-level
        // deadline degenerates only at the runtime substrate (Envoy
        // / Cilium L7 timeout overlay) far from the source
        // `caixa.lisp` with no field naming the offending policy.
        let mut s = three_member_spec();
        let timeout = POLICY_TIMEOUT_MAX + Duration::from_secs(1);
        s.politicas.timeout = Some(timeout);
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyTimeoutExceedsCap { timeout }
        );
    }

    #[test]
    fn rejects_policy_timeout_one_millisecond_above_cap() {
        // Boundary case: exactly 1ms past the cap (the granularity
        // the canonical-form gate enforces). Catches a future
        // "strictly less than" half-measure and pins the diagnostic
        // to name the offending `Duration` verbatim. Peer of
        // [`crate::limits`]'s `validate_rejects_memory_one_byte_above_wasm32_cap`
        // boundary pin on the sibling `:limits :memory` top edge.
        let mut s = three_member_spec();
        let timeout = POLICY_TIMEOUT_MAX + Duration::from_millis(1);
        s.politicas.timeout = Some(timeout);
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyTimeoutExceedsCap { timeout }
        );
    }

    #[test]
    fn rejects_policy_timeout_far_above_cap() {
        // The "obvious authoring footgun" case: a `(:timeout "24h")`
        // or `(:timeout "86400s")` — values the canonical-form arm
        // accepts as integer-millisecond magnitudes, the codec
        // round-trips losslessly through serde, but the mesh-level
        // policy cannot honor (a 24-hour synchronous-`:contratos`
        // deadline is operationally indistinguishable from
        // omit-the-axis). Until this gate landed validate accepted
        // it. Pin both common above-cap values (24h, 7d) so a future
        // relaxation that drops the upper bound surfaces here.
        for timeout in [
            Duration::from_secs(86_400),    // 24h
            Duration::from_secs(604_800),   // 7d
            Duration::from_secs(1_000_000), // ~11.5 days
        ] {
            let mut s = three_member_spec();
            s.politicas.timeout = Some(timeout);
            assert_eq!(
                s.validate().unwrap_err(),
                AplicacaoError::PolicyTimeoutExceedsCap { timeout }
            );
        }
    }

    #[test]
    fn accepts_policy_timeout_at_cap() {
        // The boundary value — exactly [`POLICY_TIMEOUT_MAX`] (1h) —
        // must validate. The cap is inclusive on the top edge,
        // matching the [`POLICY_RETRIES_MAX`] /
        // [`POLICY_BREAKER_MAX_FAILURES_MAX`] /
        // [`crate::LIMITS_MEMORY_WASM32_MAX_BYTES`] discipline on the
        // sibling capped axes. Pin the boundary explicitly so a
        // future off-by-one tightening (`>= POLICY_TIMEOUT_MAX`
        // instead of `>`) surfaces here as a test failure rather
        // than a silent contract narrowing.
        let mut s = three_member_spec();
        s.politicas.timeout = Some(POLICY_TIMEOUT_MAX);
        s.validate()
            .expect("timeout == POLICY_TIMEOUT_MAX must validate");
    }

    #[test]
    fn accepts_policy_timeout_typical_values() {
        // The documented production-playbook band positive-control
        // sweep — every value Envoy / Istio / Linkerd / AWS App Mesh
        // / Kubernetes ingress-nginx recommend (1s..=60s) must pass,
        // plus a sweep through the long-running-workflow band
        // (5m, 15m, 30m, 1h) the cap accepts. Pin the inclusive
        // validated set explicitly so a future tightening of the
        // ceiling surfaces here as a deliberate test edit, not a
        // silent contract narrowing.
        for timeout in [
            Duration::from_millis(1),
            Duration::from_millis(500),
            Duration::from_secs(1),
            Duration::from_secs(10),
            Duration::from_secs(15), // Envoy default
            Duration::from_secs(30),
            Duration::from_secs(60), // AWS App Mesh typical
            Duration::from_secs(300),
            Duration::from_secs(900),
            Duration::from_secs(1800),
            Duration::from_secs(3600), // exactly 1h, the cap
        ] {
            let mut s = three_member_spec();
            s.politicas.timeout = Some(timeout);
            s.validate()
                .unwrap_or_else(|e| panic!("timeout={timeout:?} must validate; got {e:?}"));
        }
    }

    #[test]
    fn policy_timeout_zero_takes_precedence_over_cap() {
        // The cross-arm ordering pin: `Duration::ZERO` is
        // structurally outside both `>= 1ms` (zero-floor) and
        // `<= POLICY_TIMEOUT_MAX` (cap), but the zero-floor
        // diagnostic is the more self-locating one (it directly
        // names the omit-axis remediation), so the validate gate
        // must fire on zero first. Same shape every other
        // zero-then-shape ordering on this surface uses
        // ([`AplicacaoError::PolicyRetriesZero`] then
        // [`AplicacaoError::PolicyRetriesExceedsCap`];
        // [`AplicacaoError::PolicyBreakerZeroFailures`] then
        // [`AplicacaoError::PolicyBreakerMaxFailuresExceedsCap`]).
        let mut s = three_member_spec();
        s.politicas.timeout = Some(Duration::ZERO);
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyTimeoutZero,
            "Duration::ZERO must surface the zero-floor diagnostic, not the cap diagnostic"
        );
    }

    #[test]
    fn policy_timeout_canonical_takes_precedence_over_cap() {
        // The cross-arm ordering pin: a `Duration` that is *both*
        // sub-millisecond (non-canonical-form) and structurally
        // above the cap surfaces the canonical-form diagnostic
        // first, because the round-trip-shape break is the more
        // fundamental issue (the value can't even round-trip
        // through the codec, so the cap diagnostic naming
        // `1ms..=1h` would be misleading — there's no integer-ms
        // form of the offending value). Pin the order so a future
        // refactor that reorders the arms surfaces here as a test
        // failure rather than a silent diagnostic regression.
        let mut s = three_member_spec();
        // A `Duration` with `subsec_nanos() == 1` (sub-ms residue)
        // *and* total magnitude above the 1h cap.
        let timeout = POLICY_TIMEOUT_MAX + Duration::from_nanos(1);
        s.politicas.timeout = Some(timeout);
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyTimeoutNotCanonical { timeout },
            "sub-ms above-cap value must surface the canonical-form diagnostic, not the cap diagnostic"
        );
    }

    #[test]
    fn policy_timeout_cap_diagnostic_carries_offending_value() {
        // The diagnostic-shape pin: the offending `Duration` is
        // carried verbatim into the
        // [`AplicacaoError::PolicyTimeoutExceedsCap`] variant so the
        // surfaced error message names the value the author wrote
        // (`":politicas :timeout (Duration { secs: 7200, nanos: 0 })
        // exceeds the mesh-policy ceiling …"`), not just the cap.
        // Same self-locating diagnostic shape every other typed-cap
        // arm on this surface carries
        // ([`AplicacaoError::PolicyRetriesExceedsCap`] carries the
        // offending retry count verbatim).
        let mut s = three_member_spec();
        let timeout = Duration::from_secs(7200); // 2h
        s.politicas.timeout = Some(timeout);
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::PolicyTimeoutExceedsCap { timeout: t } if t == timeout),
            "got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("7200"),
            ":politicas :timeout cap diagnostic must carry the offending value verbatim (got: {msg})"
        );
    }

    #[test]
    fn policy_timeout_cap_pins_canonical_value() {
        // The [`POLICY_TIMEOUT_MAX`] constant pins the value at
        // exactly 1 hour (3600s = 3_600_000ms) — the largest unit
        // the shared duration codec emits as a clean canonical
        // string (`"<n>h"`). Pinning the literal value here surfaces
        // a future drift (a relaxation to 24h, a tightening to 5m)
        // as a deliberate test edit, not a silent contract
        // narrowing. Same shape every other typed-cap value pin on
        // this surface uses (`policy_retries_cap_is_aws_app_mesh_aligned`).
        assert_eq!(POLICY_TIMEOUT_MAX, Duration::from_secs(3600));
        assert_eq!(POLICY_TIMEOUT_MAX.as_millis(), 3_600_000);
    }

    #[test]
    fn policy_timeout_cap_value_round_trips_through_codec() {
        // The codec round-trip property the cap arm preserves: the
        // [`POLICY_TIMEOUT_MAX`] constant itself round-trips through
        // the shared duration codec — every value at the cap renders
        // to a clean canonical string (`"1h"`) and parses back to
        // the same `Duration`. Pin this so a future drift between
        // the cap constant and the codec's largest emitted unit
        // surfaces here. Same shape every other typed boundary pin
        // on this surface uses
        // (`wasm32_memory_cap_matches_parsed_4_gib`).
        let policy = MeshPolicy {
            timeout: Some(POLICY_TIMEOUT_MAX),
            ..Default::default()
        };
        let json = serde_json::to_string(&policy).unwrap();
        // The codec emits `"1h"` for the canonical 1-hour magnitude.
        assert!(
            json.contains("\"1h\""),
            "the POLICY_TIMEOUT_MAX value must render to the canonical \"1h\" form (got: {json})"
        );
        let back: MeshPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(back.timeout, Some(POLICY_TIMEOUT_MAX));
    }

    #[test]
    fn rejects_circuit_breaker_window_sub_millisecond() {
        // Peer of the `:timeout` sub-millisecond arm on the second
        // typed-`Duration` `:politicas` axis: a purely sub-ms
        // `Duration` (`from_micros(500)`) renders through the shared
        // codec as `"0s"`, which the codec parses back to
        // `Duration::ZERO`, which the `PolicyBreakerZeroWindow`
        // zero-floor gate then rejects on re-validate.
        let mut s = three_member_spec();
        let window = Duration::from_micros(500);
        s.politicas.circuit_breaker = Some(CircuitBreaker {
            max_failures: 5,
            window,
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyBreakerWindowNotCanonical { window }
        );
    }

    #[test]
    fn rejects_circuit_breaker_window_non_integer_millisecond() {
        // Peer of the `:timeout` non-integer-ms arm: a `Duration`
        // with non-integer-millisecond residue renders through the
        // shared codec as the truncated `"<n>ms"` form, parsing back
        // to a *different* `Duration` on the next round-trip.
        let mut s = three_member_spec();
        let window = Duration::from_micros(1500);
        s.politicas.circuit_breaker = Some(CircuitBreaker {
            max_failures: 5,
            window,
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyBreakerWindowNotCanonical { window }
        );
    }

    #[test]
    fn accepts_circuit_breaker_window_integer_millisecond_forms() {
        // The canonical-forms sweep on the breaker axis: every
        // integer-ms multiple the codec round-trips losslessly
        // passes the canonical gate.
        for window in [
            Duration::from_millis(1),
            Duration::from_millis(500),
            Duration::from_millis(1500),
            Duration::from_secs(30),
            Duration::from_secs(60),
            Duration::from_secs(3600),
        ] {
            let mut s = three_member_spec();
            s.politicas.circuit_breaker = Some(CircuitBreaker {
                max_failures: 5,
                window,
            });
            s.validate()
                .expect("integer-millisecond :circuit-breaker :window must validate");
        }
    }

    #[test]
    fn circuit_breaker_zero_window_takes_precedence_over_canonical() {
        // `Duration::ZERO` would pass the canonical-ms gate (the
        // sub-ns residue is zero) but must surface the narrower
        // `PolicyBreakerZeroWindow` diagnostic with its omit-axis
        // remediation.
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
    fn circuit_breaker_zero_failures_takes_precedence_over_window_canonical() {
        // Both axes invalid: max_failures == 0 *and* window is
        // sub-ms. The validate gate must fire on max_failures first
        // (matching the existing ordering pin
        // `rejects_circuit_breaker_zero_max_failures` enshrines), so
        // the existing diagnostic continues to lead with the simpler
        // "zero threshold" framing.
        let mut s = three_member_spec();
        s.politicas.circuit_breaker = Some(CircuitBreaker {
            max_failures: 0,
            window: Duration::from_micros(500),
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyBreakerZeroFailures
        );
    }

    #[test]
    fn circuit_breaker_window_canonical_diagnostic_carries_offending_duration() {
        let mut s = three_member_spec();
        let window = Duration::from_nanos(60_000_000_001);
        s.politicas.circuit_breaker = Some(CircuitBreaker {
            max_failures: 5,
            window,
        });
        match s.validate().unwrap_err() {
            AplicacaoError::PolicyBreakerWindowNotCanonical { window: w } => {
                assert_eq!(w, window, "diagnostic must carry the offending Duration");
            }
            other => panic!("expected PolicyBreakerWindowNotCanonical, got {other:?}"),
        }
    }

    #[test]
    fn rejects_circuit_breaker_window_above_cap() {
        // The fail-before-pass-after pin: 3601s = 1h + 1s is
        // structurally one canonical-tick past the
        // [`POLICY_BREAKER_WINDOW_MAX`] ceiling (1h = 3600s) — an
        // integer-millisecond magnitude the canonical-form arm above
        // accepts cleanly, that the codec round-trips losslessly as
        // `"3601s"`, and that silently passed validate on every
        // pre-gate codebase because the typed slot's only checks were
        // the zero-floor and canonical-form arms. The
        // rolling-window-to-lifetime-counter degeneration surfaces
        // only at the runtime substrate (Envoy's outlier_detection
        // interval, the future CiliumClusterwideEnvoyConfig overlay)
        // far from the source `caixa.lisp` with no field naming the
        // offending policy.
        let mut s = three_member_spec();
        let window = POLICY_BREAKER_WINDOW_MAX + Duration::from_secs(1);
        s.politicas.circuit_breaker = Some(CircuitBreaker {
            max_failures: 5,
            window,
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyBreakerWindowExceedsCap { window }
        );
    }

    #[test]
    fn rejects_circuit_breaker_window_one_millisecond_above_cap() {
        // Boundary case: exactly 1ms past the cap (the granularity the
        // canonical-form gate enforces). Catches a future "strictly
        // less than" half-measure and pins the diagnostic to name the
        // offending `Duration` verbatim. Peer of
        // `rejects_policy_timeout_one_millisecond_above_cap` on the
        // sibling duration-typed `:politicas :timeout` top edge.
        let mut s = three_member_spec();
        let window = POLICY_BREAKER_WINDOW_MAX + Duration::from_millis(1);
        s.politicas.circuit_breaker = Some(CircuitBreaker {
            max_failures: 5,
            window,
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyBreakerWindowExceedsCap { window }
        );
    }

    #[test]
    fn rejects_circuit_breaker_window_far_above_cap() {
        // The "obvious authoring footgun" case: a `(:window "24h")` or
        // `(:window "86400s")` — values the canonical-form arm
        // accepts as integer-millisecond magnitudes, the codec
        // round-trips losslessly through serde, but the
        // rolling-window breaker contract cannot honor (a 24-hour
        // rolling failure window is operationally a lifetime counter).
        // Until this gate landed validate accepted it. Pin both common
        // above-cap values (24h, 7d) so a future relaxation that
        // drops the upper bound surfaces here.
        for window in [
            Duration::from_secs(86_400),    // 24h
            Duration::from_secs(604_800),   // 7d
            Duration::from_secs(1_000_000), // ~11.5 days
        ] {
            let mut s = three_member_spec();
            s.politicas.circuit_breaker = Some(CircuitBreaker {
                max_failures: 5,
                window,
            });
            assert_eq!(
                s.validate().unwrap_err(),
                AplicacaoError::PolicyBreakerWindowExceedsCap { window }
            );
        }
    }

    #[test]
    fn accepts_circuit_breaker_window_at_cap() {
        // The boundary value — exactly [`POLICY_BREAKER_WINDOW_MAX`]
        // (1h) — must validate. The cap is inclusive on the top edge,
        // matching the [`POLICY_TIMEOUT_MAX`] /
        // [`POLICY_RETRIES_MAX`] / [`POLICY_BREAKER_MAX_FAILURES_MAX`]
        // / [`crate::LIMITS_MEMORY_WASM32_MAX_BYTES`] discipline on the
        // sibling capped axes. Pin the boundary explicitly so a
        // future off-by-one tightening (`>= POLICY_BREAKER_WINDOW_MAX`
        // instead of `>`) surfaces here as a test failure rather than
        // a silent contract narrowing.
        let mut s = three_member_spec();
        s.politicas.circuit_breaker = Some(CircuitBreaker {
            max_failures: 5,
            window: POLICY_BREAKER_WINDOW_MAX,
        });
        s.validate()
            .expect("window == POLICY_BREAKER_WINDOW_MAX must validate");
    }

    #[test]
    fn accepts_circuit_breaker_window_typical_values() {
        // The documented production-playbook band positive-control
        // sweep — every value Hystrix / resilience4j / Istio / Envoy
        // / AWS App Mesh recommend (1s..=300s) must pass, plus a sweep
        // through the long-tail failure-detection band (15m, 30m, 1h)
        // the cap accepts. Pin the inclusive validated set explicitly
        // so a future tightening of the ceiling surfaces here as a
        // deliberate test edit, not a silent contract narrowing.
        for window in [
            Duration::from_millis(1),
            Duration::from_millis(500),
            Duration::from_secs(1),
            Duration::from_secs(10), // Hystrix / Istio / Envoy default
            Duration::from_secs(30),
            Duration::from_secs(60),  // resilience4j typical
            Duration::from_secs(300), // AWS App Mesh typical
            Duration::from_secs(900),
            Duration::from_secs(1800),
            Duration::from_secs(3600), // exactly 1h, the cap
        ] {
            let mut s = three_member_spec();
            s.politicas.circuit_breaker = Some(CircuitBreaker {
                max_failures: 5,
                window,
            });
            s.validate()
                .unwrap_or_else(|e| panic!("window={window:?} must validate; got {e:?}"));
        }
    }

    #[test]
    fn circuit_breaker_zero_window_takes_precedence_over_cap() {
        // The cross-arm ordering pin: `Duration::ZERO` is structurally
        // outside both `>= 1ms` (zero-floor) and
        // `<= POLICY_BREAKER_WINDOW_MAX` (cap), but the zero-floor
        // diagnostic is the more self-locating one (it directly names
        // the omit-axis remediation), so the validate gate must fire
        // on zero first. Same shape every other zero-then-cap
        // ordering on this surface uses
        // ([`AplicacaoError::PolicyTimeoutZero`] then
        // [`AplicacaoError::PolicyTimeoutExceedsCap`];
        // [`AplicacaoError::PolicyBreakerZeroFailures`] then
        // [`AplicacaoError::PolicyBreakerMaxFailuresExceedsCap`]).
        let mut s = three_member_spec();
        s.politicas.circuit_breaker = Some(CircuitBreaker {
            max_failures: 5,
            window: Duration::ZERO,
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyBreakerZeroWindow,
            "Duration::ZERO must surface the zero-floor diagnostic, not the cap diagnostic"
        );
    }

    #[test]
    fn circuit_breaker_window_canonical_takes_precedence_over_cap() {
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
        // `policy_timeout_canonical_takes_precedence_over_cap` on the
        // sibling duration-typed `:politicas :timeout` axis.
        let mut s = three_member_spec();
        let window = POLICY_BREAKER_WINDOW_MAX + Duration::from_nanos(1);
        s.politicas.circuit_breaker = Some(CircuitBreaker {
            max_failures: 5,
            window,
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyBreakerWindowNotCanonical { window },
            "sub-ms above-cap value must surface the canonical-form diagnostic, not the cap diagnostic"
        );
    }

    #[test]
    fn circuit_breaker_max_failures_cap_takes_precedence_over_window_cap() {
        // The cross-arm ordering pin between the two breaker axes: a
        // `CircuitBreaker` whose *both* `max_failures` is above its
        // cap *and* `window` is above its cap surfaces the
        // max-failures cap diagnostic first, because the validate
        // gate visits the failures arm before the window arm. Pin the
        // order so a future refactor that reorders the breaker arms
        // surfaces here.
        let mut s = three_member_spec();
        let window = POLICY_BREAKER_WINDOW_MAX + Duration::from_secs(1);
        s.politicas.circuit_breaker = Some(CircuitBreaker {
            max_failures: POLICY_BREAKER_MAX_FAILURES_MAX + 1,
            window,
        });
        assert_eq!(
            s.validate().unwrap_err(),
            AplicacaoError::PolicyBreakerMaxFailuresExceedsCap {
                max_failures: POLICY_BREAKER_MAX_FAILURES_MAX + 1
            },
            "both-axes-above-cap must surface the max-failures cap diagnostic first (arm order)"
        );
    }

    #[test]
    fn circuit_breaker_window_cap_diagnostic_carries_offending_value() {
        // The diagnostic-shape pin: the offending `Duration` is
        // carried verbatim into the
        // [`AplicacaoError::PolicyBreakerWindowExceedsCap`] variant so
        // the surfaced error message names the value the author wrote
        // (`":politicas :circuit-breaker :window (Duration { secs:
        // 7200, nanos: 0 }) exceeds the mesh-policy ceiling …"`), not
        // just the cap. Same self-locating diagnostic shape every
        // other typed-cap arm on this surface carries
        // ([`AplicacaoError::PolicyTimeoutExceedsCap`] carries the
        // offending `Duration` verbatim).
        let mut s = three_member_spec();
        let window = Duration::from_secs(7200); // 2h
        s.politicas.circuit_breaker = Some(CircuitBreaker {
            max_failures: 5,
            window,
        });
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, AplicacaoError::PolicyBreakerWindowExceedsCap { window: w } if w == window),
            "got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("7200"),
            ":politicas :circuit-breaker :window cap diagnostic must carry the offending value verbatim (got: {msg})"
        );
    }

    #[test]
    fn circuit_breaker_window_cap_pins_canonical_value() {
        // The [`POLICY_BREAKER_WINDOW_MAX`] constant pins the value at
        // exactly 1 hour (3600s = 3_600_000ms) — the largest unit the
        // shared duration codec emits as a clean canonical string
        // (`"<n>h"`) and the same value [`POLICY_TIMEOUT_MAX`] pins on
        // the sibling duration-typed `:politicas :timeout` axis (the
        // two duration-typed `:politicas` axes share a uniform top
        // edge). Pinning the literal value here surfaces a future
        // drift (a relaxation to 24h, a tightening to 5m) as a
        // deliberate test edit, not a silent contract narrowing. Same
        // shape every other typed-cap value pin on this surface uses
        // (`policy_timeout_cap_pins_canonical_value`).
        assert_eq!(POLICY_BREAKER_WINDOW_MAX, Duration::from_secs(3600));
        assert_eq!(POLICY_BREAKER_WINDOW_MAX.as_millis(), 3_600_000);
        assert_eq!(
            POLICY_BREAKER_WINDOW_MAX, POLICY_TIMEOUT_MAX,
            "the two duration-typed `:politicas` caps share the same top edge"
        );
    }

    #[test]
    fn circuit_breaker_window_cap_value_round_trips_through_codec() {
        // The codec round-trip property the cap arm preserves: the
        // [`POLICY_BREAKER_WINDOW_MAX`] constant itself round-trips
        // through the shared duration codec — every value at the cap
        // renders to a clean canonical string (`"1h"`) and parses back
        // to the same `Duration`. Pin this so a future drift between
        // the cap constant and the codec's largest emitted unit
        // surfaces here. Same shape every other typed boundary pin on
        // this surface uses
        // (`policy_timeout_cap_value_round_trips_through_codec`).
        let policy = MeshPolicy {
            circuit_breaker: Some(CircuitBreaker {
                max_failures: 5,
                window: POLICY_BREAKER_WINDOW_MAX,
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&policy).unwrap();
        // The codec emits `"1h"` for the canonical 1-hour magnitude.
        assert!(
            json.contains("\"1h\""),
            "the POLICY_BREAKER_WINDOW_MAX value must render to the canonical \"1h\" form (got: {json})"
        );
        let back: MeshPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.circuit_breaker.unwrap().window,
            POLICY_BREAKER_WINDOW_MAX
        );
    }

    #[test]
    fn is_integer_millisecond_duration_predicate_tracks_codec() {
        // Pin the predicate's accepted set against the codec's
        // accepted set explicitly. The codec parses
        // `<integer><unit>` for unit ∈ {`ms`,`s`,`m`,`h`} — every
        // accepted value is an integer-millisecond multiple — so the
        // predicate must accept exactly that set. Same shape every
        // other predicate-on-the-typed-slot helper carries
        // (`is_canonical_rate_limit_window_predicate_tracks_codec`).
        // Read directly from the codec-owned predicate — the crate's
        // single source of truth every typed-`Duration` axis now routes
        // through via
        // [`crate::render::require_positive_canonical_bounded_duration`].
        use super::supervisor::duration_codec::is_integer_millisecond_duration;
        assert!(is_integer_millisecond_duration(Duration::ZERO));
        assert!(is_integer_millisecond_duration(Duration::from_millis(1)));
        assert!(is_integer_millisecond_duration(Duration::from_millis(500)));
        assert!(is_integer_millisecond_duration(Duration::from_millis(1500)));
        assert!(is_integer_millisecond_duration(Duration::from_secs(30)));
        assert!(is_integer_millisecond_duration(Duration::from_secs(3600)));
        // Non-integer-millisecond residue: rejected.
        assert!(!is_integer_millisecond_duration(Duration::from_micros(1)));
        assert!(!is_integer_millisecond_duration(Duration::from_micros(500)));
        assert!(!is_integer_millisecond_duration(Duration::from_micros(
            1500
        )));
        assert!(!is_integer_millisecond_duration(Duration::from_nanos(1)));
        assert!(!is_integer_millisecond_duration(Duration::from_nanos(
            999_999
        )));
        // The 1-ns-past-1ms boundary: rejected (no longer a clean
        // integer-millisecond multiple).
        assert!(!is_integer_millisecond_duration(Duration::from_nanos(
            1_000_001
        )));
    }

    #[test]
    fn policy_timeout_validated_value_round_trips_through_codec() {
        // The structural property the canonical-ms gate enforces:
        // every `MeshPolicy::timeout` past `AplicacaoSpec::validate`
        // round-trips losslessly through the shared `duration_codec`
        // (serialize → string → deserialize → equal value). Pin this
        // end-to-end so a future change to either side (the validate
        // gate's accepted granularity, the codec's parse/render unit
        // set) that breaks the alignment surfaces here. The
        // previous-state shape (typed slot accepts arbitrary
        // `Duration`, codec only round-trips integer-ms) would fail
        // this test for any `Duration::from_micros(1500)` timeout —
        // the validate gate now forecloses that.
        for timeout in [
            Duration::from_millis(1),
            Duration::from_millis(1500),
            Duration::from_secs(30),
            Duration::from_secs(3600),
        ] {
            let mut s = three_member_spec();
            s.politicas.timeout = Some(timeout);
            s.validate().unwrap();
            let json = serde_json::to_string(&s.politicas).unwrap();
            let back: MeshPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(
                back.timeout, s.politicas.timeout,
                "every validated :timeout must round-trip losslessly through the codec"
            );
        }
    }

    #[test]
    fn circuit_breaker_window_validated_value_round_trips_through_codec() {
        // Peer of the `:timeout` round-trip property on the breaker
        // axis.
        for window in [
            Duration::from_millis(1),
            Duration::from_millis(1500),
            Duration::from_secs(30),
            Duration::from_secs(3600),
        ] {
            let mut s = three_member_spec();
            s.politicas.circuit_breaker = Some(CircuitBreaker {
                max_failures: 5,
                window,
            });
            s.validate().unwrap();
            let json = serde_json::to_string(&s.politicas).unwrap();
            let back: MeshPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(
                back.circuit_breaker.unwrap().window,
                window,
                "every validated :circuit-breaker :window must round-trip losslessly"
            );
        }
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

    fn sharded_spec_with_key(key: &str) -> AplicacaoSpec {
        // Fixture builder for the `:placement :shard-key` shape gate
        // tests: a three-member Aplicacao on the `Sharded` strategy
        // with the supplied `:shard-key` slot. Co-locates the
        // arm-construction so every test below carries one line of
        // setup (the offending `:shard-key` value) and the assertion.
        let mut s = three_member_spec();
        s.placement.estrategia = PlacementStrategy::Sharded;
        s.placement.shard_key = Some(key.into());
        s
    }

    #[test]
    fn rejects_shard_key_with_embedded_space() {
        // The canonical paste-from-aligned-doc footgun:
        // `:shard-key "$tenant Id"` — the Akka-style entity-id
        // extractor reads the slot as a single-token reference, and an
        // embedded space breaks the token boundary at the runtime
        // hash-extractor pass with no diagnostic naming the offending
        // entry.
        let s = sharded_spec_with_key("$tenant Id");
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ShardKeyInvalid { ref shard_key, ref reason }
                    if shard_key == "$tenant Id" && reason.contains("space")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_shard_key_with_leading_space() {
        // Leading-space arm of the embedded-whitespace footgun — the
        // paste-from-aligned-doc / paste-from-CSV-cell variant where
        // the leading column-padding leaked into the slot.
        let s = sharded_spec_with_key(" $tenantId");
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ShardKeyInvalid { ref shard_key, .. }
                    if shard_key == " $tenantId"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_shard_key_with_trailing_newline() {
        // The canonical paste-from-shell-heredoc footgun — every
        // `<<EOF` heredoc terminator paste leaves a trailing newline
        // the YAML emitter then folds away inconsistently across
        // emitter implementations.
        let s = sharded_spec_with_key("$tenantId\n");
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ShardKeyInvalid { ref shard_key, ref reason }
                    if shard_key == "$tenantId\n" && reason.contains("0x0a")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_shard_key_with_embedded_tab() {
        // The paste-from-aligned-doc tab-stop variant — tabs land
        // alongside spaces in copy-paste from formatted columns.
        let s = sharded_spec_with_key("$tenant\tId");
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ShardKeyInvalid { ref shard_key, ref reason }
                    if shard_key == "$tenant\tId" && reason.contains("tab")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_shard_key_with_control_character() {
        // The paste-from-binary / paste-from-screen-cleared-terminal
        // footgun — an embedded `\x01` (SOH) byte that some YAML
        // emitters silently strip and others escape as ``,
        // breaking round-trip across emitter implementations.
        let s = sharded_spec_with_key("$tenant\u{0001}Id");
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ShardKeyInvalid { ref shard_key, ref reason }
                    if shard_key == "$tenant\u{0001}Id" && reason.contains("control")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_shard_key_with_non_ascii() {
        // The canonical un-Punycode-encoded IDN / paste-from-Unicode-doc
        // footgun — non-ASCII bytes normalize differently between the
        // caixa-mesh-side YAML emitter and the in-cluster reconciler's
        // YAML parser, the same entity ID can silently map to two
        // distinct shards on a re-render.
        let s = sharded_spec_with_key("$tenàntId");
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ShardKeyInvalid { ref shard_key, ref reason }
                    if shard_key == "$tenàntId" && reason.contains("non-ASCII")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_shard_key_too_long() {
        // Length cap pin: 64 bytes — one byte over the
        // PLACEMENT_SHARD_KEY_MAX_LEN (63) cap. The realistic shape
        // here is a paste-from-doc multi-line blob landing in
        // `:shard-key` instead of a single-token extractor expression.
        let too_long = "a".repeat(64);
        let s = sharded_spec_with_key(&too_long);
        let err = s.validate().unwrap_err();
        let AplicacaoError::ShardKeyInvalid {
            ref shard_key,
            ref reason,
        } = err
        else {
            panic!("expected ShardKeyInvalid, got {err:?}");
        };
        assert_eq!(shard_key, &too_long);
        assert!(
            reason.contains("63") && reason.contains("64"),
            "diagnostic must name the cap (63) and the actual length (64): {reason:?}"
        );
    }

    #[test]
    fn shard_key_max_length_validates() {
        // Boundary pin: 63 bytes exactly — the
        // `PLACEMENT_SHARD_KEY_MAX_LEN` cap. A future tightening (e.g.
        // dropping to 62) surfaces here as a regression, mirroring
        // `placement_cluster_max_length_validates` /
        // `placement_affinity_max_length_validates` on the peer
        // identifier-shaped slots.
        let s = sharded_spec_with_key(&"a".repeat(63));
        s.validate().unwrap();
    }

    #[test]
    fn accepts_canonical_shard_key_forms() {
        // The Akka-style entity-id extractor shapes a caixa author is
        // realistically going to write — pin every leg so a future
        // tightening that bans (e.g.) the `${...}` interpolation
        // variant or the `metadata.<field>` JSONPath form surfaces
        // here as a regression. The canonical forms span:
        //
        //   - bare property name (`tenantId`, `customerId`)
        //   - Akka `ExtractEntityId` placeholder (`$tenantId`)
        //   - JSONPath-style nested reference (`metadata.tenantId`,
        //     `$.user.id`)
        //   - interpolation-style template (`${tenant}`)
        //   - snake_case property name (`customer_id`)
        //   - kebab-case property name (`customer-id` — accepted
        //     because the slot is a printable-ASCII single-token
        //     reference, not a DNS-1123 label like
        //     `:placement :affinity` / `:clusters`)
        //   - single character (`a`, `$` — boundary)
        for form in [
            "tenantId",
            "customerId",
            "$tenantId",
            "metadata.tenantId",
            "$.user.id",
            "${tenant}",
            "customer_id",
            "customer-id",
            "a",
            "$",
        ] {
            let s = sharded_spec_with_key(form);
            s.validate().unwrap_or_else(|e| {
                panic!("canonical shard-key form {form:?} must validate, got {e:?}")
            });
        }
    }

    #[test]
    fn shard_key_empty_takes_precedence_over_invalid() {
        // Order pin: the existing `ShardedKeyEmpty` diagnostic
        // (reserved for the `Sharded` `Some("")` arm) fires before the
        // new `ShardKeyInvalid` parse-side diagnostic, so an empty
        // `:shard-key` keeps its narrower error message — the new gate
        // would also reject `""` defensively, but the empty-string arm
        // is the more self-locating diagnostic. Mirrors the
        // `placement_cluster_empty_takes_precedence_over_invalid` pin
        // on the peer identifier-shaped slot.
        let s = sharded_spec_with_key("");
        let err = s.validate().unwrap_err();
        assert_eq!(err, AplicacaoError::ShardedKeyEmpty);
    }

    #[test]
    fn shard_key_invalid_diagnostic_carries_offending_value() {
        // The diagnostic-shape pin: the error names the offending
        // `:shard-key` value verbatim so the author can grep their
        // caixa.lisp without re-running the build, and carries a
        // parser-shaped `reason:` naming the specific violation —
        // mirrors `placement_cluster_invalid_diagnostic_carries_offending_cluster`
        // on the peer identifier-shaped slot.
        let s = sharded_spec_with_key("$tenant Id");
        let err = s.validate().unwrap_err();
        let AplicacaoError::ShardKeyInvalid {
            ref shard_key,
            ref reason,
        } = err
        else {
            panic!("expected ShardKeyInvalid, got {err:?}");
        };
        assert_eq!(shard_key, "$tenant Id");
        assert!(
            !reason.is_empty(),
            "reason must name the specific violation, got empty string"
        );
    }

    #[test]
    fn shard_key_shape_fires_after_non_sharded_strategy_gate() {
        // Order pin: the `ShardKeyOnNonSharded` arm (which rejects
        // `:shard-key` carried on non-Sharded strategies) fires before
        // the shape gate, so a malformed `:shard-key` carried on (e.g.)
        // a `Replicated` strategy surfaces the more self-locating
        // strategy-mismatch diagnostic (naming the actual fix — drop
        // the slot, or switch to Sharded) rather than the shape
        // diagnostic. The strategy-mismatch arm is the more actionable
        // diagnostic: a malformed shard-key on Replicated is "you
        // shouldn't have a :shard-key here at all", not "your
        // :shard-key value is malformed".
        let mut s = three_member_spec();
        // Replicated is the default fixture strategy.
        s.placement.shard_key = Some("$tenant Id".into());
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::ShardKeyOnNonSharded {
                    estrategia: PlacementStrategy::Replicated,
                    ..
                }
            ),
            "got {err:?}"
        );
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
    fn rejects_placement_affinity_with_uppercase() {
        // The canonical "I copied the ADR's display name verbatim" typo
        // — placement hints land verbatim in K8s label-selector
        // territory, where the apiserver enforces the DNS-1123 label
        // rule (lowercase-only) on every identity-keyed admission axis.
        // Mirrors `rejects_placement_cluster_with_uppercase` on the
        // sibling slot.
        let mut s = three_member_spec();
        s.placement.affinity = Some("DataLocality".into());
        let err = s.validate().unwrap_err();
        let AplicacaoError::PlacementAffinityInvalid { affinity, reason } = err else {
            panic!("expected PlacementAffinityInvalid, got other variant");
        };
        assert_eq!(affinity, "DataLocality");
        assert!(
            reason.contains("uppercase"),
            "diagnostic must name the violation as `uppercase` (got: {reason:?})"
        );
        assert!(
            reason.contains("\"datalocality\""),
            "diagnostic must suggest the lower-cased fix verbatim (got: {reason:?})"
        );
    }

    #[test]
    fn rejects_placement_affinity_with_underscore() {
        // The canonical "I'm thinking of an env var / Python identifier"
        // leak — `_` is forbidden by every DNS-1123 label schema. Same
        // shape as `rejects_placement_cluster_with_underscore` on the
        // sibling slot.
        let mut s = three_member_spec();
        s.placement.affinity = Some("data_locality".into());
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::PlacementAffinityInvalid { ref affinity, ref reason }
                    if affinity == "data_locality" && reason.contains('_')
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_placement_affinity_with_dot() {
        // A `:placement :affinity` value is a single DNS-1123 *label*
        // (it lands as a K8s label value selector key), not a subdomain.
        // The "I want to namespace my hint with `.`" intent is expressed
        // via `-` (`data-locality-east`).
        let mut s = three_member_spec();
        s.placement.affinity = Some("data.locality".into());
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::PlacementAffinityInvalid { ref affinity, ref reason }
                    if affinity == "data.locality" && reason.contains('.')
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_placement_affinity_with_unicode() {
        // DNS-1123 is ASCII-only; IDN must be pre-encoded as Punycode
        // before it reaches K8s. The byte-by-byte ASCII validity check
        // rejects multi-byte UTF-8 sequences by the first byte that
        // fails `[a-z0-9-]`.
        let mut s = three_member_spec();
        s.placement.affinity = Some("data-localité".into());
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::PlacementAffinityInvalid { ref affinity, .. }
                    if affinity == "data-localité"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_placement_affinity_with_leading_hyphen() {
        // DNS-1123 boundary rule: labels must start with an
        // alphanumeric. Pin separately from the trailing-hyphen arm so
        // a future relaxation that only checks one boundary surfaces
        // here as a regression (parallel to
        // `rejects_placement_cluster_with_leading_hyphen`).
        let mut s = three_member_spec();
        s.placement.affinity = Some("-data-locality".into());
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::PlacementAffinityInvalid { ref affinity, ref reason }
                    if affinity == "-data-locality" && reason.contains("start and end")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_placement_affinity_with_trailing_hyphen() {
        // Symmetric arm of the DNS-1123 boundary rule. Pinned so both
        // ends are covered against a future relaxation.
        let mut s = three_member_spec();
        s.placement.affinity = Some("data-locality-".into());
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::PlacementAffinityInvalid { ref affinity, .. }
                    if affinity == "data-locality-"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_placement_affinity_with_whitespace() {
        // Whitespace is the canonical "I pasted from a sketch / doc"
        // footgun. The apiserver rejects every label-selector value
        // carrying whitespace.
        let mut s = three_member_spec();
        s.placement.affinity = Some("data locality".into());
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                AplicacaoError::PlacementAffinityInvalid { ref affinity, .. }
                    if affinity == "data locality"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_placement_affinity_too_long() {
        // 64 bytes exceeds the DNS-1123 label cap by one — the boundary
        // pin. The diagnostic names both the cap (63) and the actual
        // length so the author can shorten in one edit. Mirrors
        // `rejects_placement_cluster_too_long`.
        let mut s = three_member_spec();
        let too_long = "a".repeat(64);
        s.placement.affinity = Some(too_long.clone());
        let err = s.validate().unwrap_err();
        let AplicacaoError::PlacementAffinityInvalid { affinity, reason } = err else {
            panic!("expected PlacementAffinityInvalid");
        };
        assert_eq!(affinity, too_long);
        assert!(
            reason.contains("63") && reason.contains("64"),
            "diagnostic must name the cap (63) and the actual length (64): {reason:?}"
        );
    }

    #[test]
    fn placement_affinity_max_length_validates() {
        // 63 bytes exactly — the DNS-1123 label cap. Boundary pin so a
        // future tightening (e.g. dropping to 62) surfaces here as a
        // regression, mirroring `placement_cluster_max_length_validates`.
        let mut s = three_member_spec();
        s.placement.affinity = Some("a".repeat(63));
        s.validate().unwrap();
    }

    #[test]
    fn accepts_canonical_placement_affinity_forms() {
        // The DNS-1123 label shapes a caixa author is realistically
        // going to write for placement hints: the M3 canonical examples
        // (`data-locality`, `low-latency`, `anti-affinity`), the
        // single-token form (`affinity`), the single-character boundary
        // (`a`), the digit-start (DNS-1123 allows this, unlike
        // DNS-1035), and a regional-suffixed form. Pin every leg so a
        // future tightening that bans (e.g.) digit-start identifiers
        // surfaces here.
        for form in [
            "data-locality",
            "low-latency",
            "anti-affinity",
            "affinity",
            "a",
            "3-tier",
            "locality-east",
        ] {
            let mut s = three_member_spec();
            s.placement.affinity = Some(form.into());
            s.validate().unwrap_or_else(|e| {
                panic!("canonical affinity form {form:?} must validate, got {e:?}")
            });
        }
    }

    #[test]
    fn placement_affinity_empty_takes_precedence_over_invalid() {
        // Order pin: the existing `PlacementAffinityEmpty` diagnostic
        // (which doesn't try to parse) fires before the new
        // `PlacementAffinityInvalid` parse-side diagnostic, so an empty
        // `:affinity` keeps its narrower error message — the new gate
        // would also reject `""`, but the empty-string arm is the more
        // self-locating diagnostic. Mirrors the
        // `placement_cluster_empty_takes_precedence_over_invalid` pin.
        let mut s = three_member_spec();
        s.placement.affinity = Some(String::new());
        let err = s.validate().unwrap_err();
        assert_eq!(err, AplicacaoError::PlacementAffinityEmpty);
    }

    #[test]
    fn placement_affinity_invalid_diagnostic_carries_offending_value() {
        // The diagnostic shape pin: every rejection carries the offending
        // `affinity:` verbatim plus a parser-shaped `reason:` so the
        // author can grep their caixa.lisp for `:affinity "<hint>"` and
        // fix it in one edit. Mirrors the
        // `placement_cluster_invalid_diagnostic_carries_offending_cluster`
        // pin on the sibling slot.
        let mut s = three_member_spec();
        s.placement.affinity = Some("Data_Locality".into());
        let err = s.validate().unwrap_err();
        let AplicacaoError::PlacementAffinityInvalid { affinity, reason } = err else {
            panic!("expected PlacementAffinityInvalid");
        };
        assert_eq!(affinity, "Data_Locality");
        assert!(
            !reason.is_empty(),
            "diagnostic reason must not be empty (got: {reason:?})"
        );
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
        let payload = format!(
            r#"{{"{max_failures}":5,"{window}":"0.5m"}}"#,
            max_failures = crate::CIRCUIT_BREAKER_KEY_MAX_FAILURES,
            window = crate::CIRCUIT_BREAKER_KEY_WINDOW,
        );
        let err = serde_json::from_str::<CircuitBreaker>(&payload).unwrap_err();
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
            let payload = format!(
                r#"{{"{max_failures}":5,"{window}":"{window_lit}"}}"#,
                max_failures = crate::CIRCUIT_BREAKER_KEY_MAX_FAILURES,
                window = crate::CIRCUIT_BREAKER_KEY_WINDOW,
            );
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
    fn rate_limit_serde_rejects_leading_zero_magnitude() {
        // `"0100/s"` is digit-only, so the existing
        // non-digit-only / sign / fractional arm doesn't catch it —
        // `u32::from_str("0100")` returns `Ok(100)`, so before this
        // gate `"0100/s"` parsed to `RateLimit { 100, 1s }` and
        // round-tripped through `render` to `"100/s"` — a *different*
        // canonical string on the next emit, breaking the THEORY.md
        // Part V render-determinism contract exactly the way the
        // peer `"+100/s"` case did before the leading-`+` arm landed.
        // This is the load-bearing class the leading-zero gate closes
        // beyond what the existing digit-only / sign / fractional
        // gates cover, and the peer arm to the leading-`+` test
        // (`rate_limit_serde_rejects_leading_plus_sign`) on the same
        // canonical-form-drift axis.
        let payload = r#"{"rateLimit":"0100/s"}"#;
        let err = serde_json::from_str::<MeshPolicy>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("non-canonical leading zero"),
            "expected leading-zero diagnostic in {msg:?}"
        );
        assert!(msg.contains("\"0100\""), "missing magnitude in {msg:?}");
        assert!(
            msg.contains("THEORY.md"),
            "missing render-determinism contract citation in {msg:?}"
        );
    }

    #[test]
    fn rate_limit_serde_rejects_multi_digit_zero_magnitude() {
        // `"00/s"` is the degenerate leading-zero case — every byte
        // is `0`, the magnitude parses to `u32` = 0, and `render(0)`
        // emits `"0/s"`. Round-trip drift: `"00/s"` → 0 → `"0/s"`,
        // a *different* canonical string, same render-determinism
        // violation. The single-byte `"0/s"` itself is in the
        // accepted set (round-trips losslessly through `render`,
        // refused downstream by `PolicyRateLimitZero`); the
        // multi-byte `"00/s"` is not. Pins the boundary between the
        // accepted single-`0` and the rejected leading-zero class.
        let payload = r#"{"rateLimit":"00/s"}"#;
        let err = serde_json::from_str::<MeshPolicy>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("non-canonical leading zero"),
            "expected leading-zero diagnostic in {msg:?}"
        );
        assert!(msg.contains("\"00\""), "missing magnitude in {msg:?}");
    }

    #[test]
    fn rate_limit_serde_rejects_leading_zero_per_hour_window() {
        // Cross-window pin — the gate is window-agnostic; the
        // leading-zero class is a property of the magnitude, not the
        // unit. `"007/h"` → 7 → `"7/h"`, same drift. Mirrors the
        // peer `rate_limit_serde_rejects_leading_plus_sign` arm's
        // single-window coverage extended across the three canonical
        // windows the codec accepts.
        let payload = r#"{"rateLimit":"007/h"}"#;
        let err = serde_json::from_str::<MeshPolicy>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("non-canonical leading zero"),
            "expected leading-zero diagnostic in {msg:?}"
        );
        assert!(msg.contains("\"007\""), "missing magnitude in {msg:?}");
    }

    #[test]
    fn rate_limit_serde_rejects_leading_whitespace() {
        // `" 100/s"` — the canonical paste-from-aligned-doc /
        // paste-from-YAML-quoted-plain-scalar footgun. Before this gate
        // the top-level `s.trim()` silently ate the leading space and
        // parsed the value to `RateLimit { 100, 1s }`, which then
        // round-tripped through `render` to `"100/s"` (a *different*
        // canonical string on the next emit) — the exact
        // canonical-form-drift class the leading-`+` / leading-zero
        // arms already close, extended to the whitespace byte class.
        let payload = r#"{"rateLimit":" 100/s"}"#;
        let err = serde_json::from_str::<MeshPolicy>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("contains whitespace byte"),
            "expected whitespace diagnostic in {msg:?}"
        );
        assert!(msg.contains("0x20"), "missing offending byte in {msg:?}");
        assert!(
            msg.contains("THEORY.md"),
            "missing render-determinism contract citation in {msg:?}"
        );
    }

    #[test]
    fn rate_limit_serde_rejects_trailing_whitespace() {
        // `"100/s "` — the canonical shell-history / trailing-space
        // paste footgun. Before this gate the top-level `s.trim()`
        // silently ate the trailing space and parsed to
        // `RateLimit { 100, 1s }`, round-tripping to `"100/s"` on the
        // next emit — same canonical-form drift as the leading-space
        // sibling, closed on the same whitespace-byte arm.
        let payload = r#"{"rateLimit":"100/s "}"#;
        let err = serde_json::from_str::<MeshPolicy>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("contains whitespace byte"),
            "expected whitespace diagnostic in {msg:?}"
        );
        assert!(msg.contains("0x20"), "missing offending byte in {msg:?}");
    }

    #[test]
    fn rate_limit_serde_rejects_internal_whitespace_around_separator() {
        // `"100 / s"` — the canonical typographically-spaced author
        // shape (the same idiom every prose reference to a rate limit
        // renders as, mistakenly retained when the value is pasted
        // into a codec-shaped slot). Before this gate the per-part
        // `rate_str.trim()` / `unit.trim()` calls silently ate both
        // spaces on either side of `/` and parsed to
        // `RateLimit { 100, 1s }`, round-tripping to `"100/s"` — the
        // codec's *internal* whitespace-tolerance vector, orthogonal
        // to the leading / trailing surface but the same canonical-
        // form-drift class. Pins the arm as strictly stronger than the
        // pre-existing top-level `s.trim()` behavior: it fires on
        // whitespace anywhere in the value, not just at the string
        // boundary.
        let payload = r#"{"rateLimit":"100 / s"}"#;
        let err = serde_json::from_str::<MeshPolicy>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("contains whitespace byte"),
            "expected whitespace diagnostic in {msg:?}"
        );
        assert!(msg.contains("0x20"), "missing offending byte in {msg:?}");
    }

    #[test]
    fn rate_limit_serde_rejects_tab_byte() {
        // `"\t100/s"` — the canonical paste-from-indented-doc /
        // paste-from-YAML-block-scalar footgun where a tab byte leads
        // the magnitude. Pins that the gate covers tab (`0x09`) as
        // well as space (`0x20`) — both are `u8::is_ascii_whitespace`
        // members and both would be silently swallowed by `s.trim()`
        // pre-gate. The `is_ascii_whitespace` coverage extends beyond
        // space alone to the full ASCII-whitespace set (space `0x20`,
        // tab `0x09`, LF `0x0A`, FF `0x0C`, CR `0x0D`); this test pins
        // the tab arm as a representative of the non-space members.
        let payload = r#"{"rateLimit":"\t100/s"}"#;
        let err = serde_json::from_str::<MeshPolicy>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("contains whitespace byte"),
            "expected whitespace diagnostic in {msg:?}"
        );
        assert!(
            msg.contains("0x09"),
            "missing offending tab byte in {msg:?}"
        );
    }

    // ── canonical-form: non-ASCII Unicode `White_Space` rate-limit gate ───
    //
    // Successor to the ASCII-whitespace arm (1ad7755) on
    // `rate_limit_codec` — closes the strictly-complementary class the
    // byte-scan cannot see, through the lifted
    // [`crate::render::find_non_ascii_whitespace_char`] predicate.

    #[test]
    fn rate_limit_serde_rejects_leading_nbsp() {
        // NBSP prefix — paste-from-typography footgun. Byte-scan
        // misses, `str::trim` silently strips it, value drifts to
        // `"100/s"` on next serialize.
        let payload = "{\"rateLimit\":\"\u{00A0}100/s\"}";
        let err = serde_json::from_str::<MeshPolicy>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("non-ASCII Unicode whitespace character"),
            "expected non-ASCII whitespace diagnostic in {msg:?}"
        );
        assert!(msg.contains("U+00A0"), "missing codepoint in {msg:?}");
    }

    #[test]
    fn rate_limit_serde_rejects_internal_em_space() {
        // EM-SPACE (`\u{2003}`) between magnitude and unit — canonical
        // paste-from-typography footgun on the `<integer>/<unit>`
        // shape.
        let payload = "{\"rateLimit\":\"100\u{2003}/s\"}";
        let err = serde_json::from_str::<MeshPolicy>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("non-ASCII Unicode whitespace character"),
            "expected non-ASCII whitespace diagnostic in {msg:?}"
        );
        assert!(msg.contains("U+2003"), "missing codepoint in {msg:?}");
    }

    #[test]
    fn rate_limit_serde_accepts_ascii_only_canonical_forms_after_unicode_arm() {
        // Positive-control pin: every ASCII-only canonical form the
        // renderer emits stays accepted through the new arm.
        for lit in [r#""100/s""#, r#""5000/m""#, r#""10000/h""#] {
            let payload = format!(r#"{{"rateLimit":{lit}}}"#);
            let p: MeshPolicy = serde_json::from_str(&payload)
                .unwrap_or_else(|e| panic!("expected {lit} to parse; got {e}"));
            assert!(p.rate_limit.is_some());
        }
    }

    #[test]
    fn rate_limit_serde_accepts_single_zero_magnitude_at_codec_layer() {
        // The boundary case — `"0/s"` is the canonical form
        // `render(RateLimit { 0, 1s })` emits, so the codec accepts
        // it at the parse layer; the downstream
        // [`AplicacaoError::PolicyRateLimitZero`] gate refuses
        // `rate == 0` at the typed-validate layer above. Pins the
        // partition: the leading-zero gate at the codec layer does
        // not poach the rate-zero semantic-validation arm at the
        // typed-validate layer above (a future stricter codec must
        // not reject `"0/s"` here, or it'd collapse the diagnostic
        // partitioning that lets `PolicyRateLimitZero` name the
        // offending typed slot).
        let payload = r#"{"rateLimit":"0/s"}"#;
        let policy: MeshPolicy = serde_json::from_str(payload).unwrap_or_else(|e| {
            panic!("`\"0/s\"` must parse cleanly through rate_limit_codec: {e}")
        });
        let rl = policy.rate_limit.expect("rate_limit must be Some");
        assert_eq!(rl.rate, 0, "single-`0` magnitude must parse to rate=0");
        assert_eq!(
            rl.window,
            Duration::from_secs(1),
            "single-`0` magnitude with `s` unit must parse to window=1s"
        );
    }

    #[test]
    fn rate_limit_serde_accepts_canonical_magnitude_with_leading_one() {
        // The complementary boundary pin — every magnitude
        // `render` emits starts with `[1-9]` (or is the single byte
        // `"0"`), so the canonical-form predicate is `(len == 1) ||
        // (first byte != '0')`. Pinning the `len > 1 && first byte ==
        // '1'` case explicitly so a future tightening of the gate
        // (e.g. an over-eager "no leading digit < 5" rule, or a
        // mistakenly anchored start-of-magnitude byte check) lands
        // here before the canonical-forms-iterating test would catch
        // it.
        let payload = r#"{"rateLimit":"100/s"}"#;
        let policy: MeshPolicy = serde_json::from_str(payload)
            .unwrap_or_else(|e| panic!("canonical `\"100/s\"` must parse cleanly: {e}"));
        let rl = policy.rate_limit.expect("rate_limit must be Some");
        assert_eq!(
            rl.rate, 100,
            "canonical-100 magnitude must parse to rate=100"
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

    // ── self-membership cross-slot gate ──────────────────────────────

    #[test]
    fn validate_no_self_membership_rejects_self_named_membro() {
        // An Aplicacao whose `:membros` lists its own `:nome` is a
        // one-node lacre-closure recursion — rejected, naming the parent.
        let membros = vec![
            membro("catalog", "^0.1"),
            membro("checkout", "^0.1"),
            membro("cart", "^0.1"),
        ];
        let err = validate_no_self_membership(&membros, "checkout").unwrap_err();
        assert!(
            matches!(err, AplicacaoError::MembroIsSelfAplicacao { ref caixa } if caixa == "checkout"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_no_self_membership_accepts_distinct_membros() {
        // Positive control: distinct member names (including a member
        // that is itself an Aplicacao — recursive composition is valid,
        // MESH-COMPOSITION §V) pass the gate.
        let membros = vec![membro("catalog", "^0.1"), membro("sub-aplicacao", "^0.1")];
        validate_no_self_membership(&membros, "checkout").unwrap();
    }

    #[test]
    fn validate_no_self_membership_empty_membros_is_vacuously_ok() {
        // An empty `:membros` is rejected by `AplicacaoSpec::validate`'s
        // `NoMembros` arm (the more-fundamental "graph must have nodes"
        // gate), not by this cross-slot self-edge gate. Keeping the
        // self-membership predicate vacuously-ok on the empty input
        // matches its supervisor-axis peer
        // (`validate_no_self_supervision_empty_children_is_ok`) and
        // makes the gate composable from any future call site (an M4
        // CR materializer's per-membros validator) without re-checking
        // emptiness.
        validate_no_self_membership(&[], "checkout").unwrap();
    }

    #[test]
    fn validate_no_self_membership_diagnostic_names_offending_caixa() {
        // Pinning the Display: the self-membership diagnostic must name
        // the offending caixa verbatim + the "lists itself" framing the
        // author can grep for, so the cluster-far failure surfaces at
        // build time with one-line remediation. Same diagnostic shape
        // as the supervisor-axis `ChildSupervisesSelf` peer.
        let membros = vec![membro("orquestra", "^0.1")];
        let err = validate_no_self_membership(&membros, "orquestra").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("orquestra"),
            "diagnostic must name the offending caixa nome (got: {msg:?})"
        );
        assert!(
            msg.contains("lists itself"),
            "diagnostic must use the canonical `lists itself` framing (got: {msg:?})"
        );
    }

    #[test]
    fn default_servico_port_constant_pins_canonical_8080_literal() {
        // The canonical-constant arm — pins [`DEFAULT_SERVICO_PORT`]
        // at the verbatim `8080` literal both consumers (the
        // `Entrada::port` serde default via [`default_port`] and the
        // `caixa-mesh` `CiliumNetworkPolicy` L4-fallback at
        // `caixa-mesh/src/lib.rs:344`) read from. Peer with the
        // [`crate::DEFAULT_NAMESPACE`]-pins-`"tatara-system"`
        // discipline (a085b26) on the per-renderer canonical-K8s-axis
        // string-constant axis: a future refactor that drifts the
        // constant out from under either consumer surfaces here ahead
        // of every per-renderer's first emission. The literal value
        // matches the well-known HTTP-alt port the `pleme-computeunit`
        // library chart already emits as its `trigger.service.port`
        // default — by construction the same value the substrate
        // assumes about every Servico's in-cluster L4 listener.
        assert_eq!(
            DEFAULT_SERVICO_PORT, 8080,
            "canonical Servico port literal must remain `8080` verbatim — \
             this is the value both the `Entrada::port` serde default and the \
             caixa-mesh `CiliumNetworkPolicy` L4-fallback read from"
        );
    }

    #[test]
    fn default_port_helper_returns_canonical_servico_port_constant() {
        // The bridge-arm — pins that the [`default_port`] helper
        // [`Entrada::port`]'s `#[serde(default = "default_port")]`
        // attribute hooks routes through the lifted
        // [`DEFAULT_SERVICO_PORT`] constant, not an open-coded
        // literal. A future refactor that re-introduces the `8080`
        // literal at the helper's return site (silently re-opening
        // the drift footgun this lift closed) surfaces here ahead of
        // every author-side `(:entrada (:host … :para …))` slot
        // without an explicit `:port`. Peer with the
        // `default_namespace_re_export_points_at_caixa_core_canonical`
        // pin on the caixa-mesh-side re-export axis.
        assert_eq!(
            default_port(),
            DEFAULT_SERVICO_PORT,
            "the serde-default helper must route through the lifted constant"
        );
    }

    #[test]
    fn entrada_serde_default_port_inherits_canonical_servico_port_constant() {
        // The end-to-end pin — an author-surface `(:entrada (:host …
        // :para …))` without an explicit `:port` slot deserializes to
        // a typed [`Entrada`] carrying [`DEFAULT_SERVICO_PORT`]
        // verbatim. Routes the canonical lifted constant through both
        // the serde-default machinery (the `#[serde(default =
        // "default_port")]` attribute) and the typed-value-shape
        // contract (the resulting [`Entrada::port`] value). A future
        // refactor that drifts either axis — replacing the serde
        // hook's helper, changing the typed slot's wire shape — would
        // surface here before any per-renderer's CNP / Gateway /
        // HTTPRoute emission consumed the drifted default.
        let entrada: Entrada =
            serde_yaml::from_str("host: checkout.quero.cloud\npara: cart\n").expect("yaml parses");
        assert_eq!(
            entrada.port, DEFAULT_SERVICO_PORT,
            "the serde default must materialize as the lifted canonical Servico port"
        );
    }

    // ── drift-detection: serde-derive-to-MEMBRO_KEY_* identity ────────────

    #[test]
    fn membro_serde_keys_match_lifted_membro_key_consts() {
        // Load-bearing invariant: the two `MEMBRO_KEY_*` consts
        // ([`crate::MEMBRO_KEY_CAIXA`] / [`crate::MEMBRO_KEY_VERSAO`])
        // name the exact camelCase JSON keys the
        // `#[serde(rename_all = "camelCase")]` attribute on
        // [`Membro`] emits. Serialize a fully-populated `Membro` and pin
        // that each canonical byte-sequence appears verbatim in the
        // JSON — a future accidental `rename_all = "snake_case"` /
        // `"kebab-case"` / verbatim-field-name flip at the derive
        // attribute (any of which would silently break every downstream
        // JSON consumer that reaches for one of the two consts via
        // `Value::get(...)`) surfaces here as a build-time test failure
        // at `aplicacao.rs`, not as an apply-time
        // `.get(<stale-canonical-const>)` returning `None` far from the
        // derive-attr drift's commit. Peer with the sibling
        // `supervisor_spec_serde_keys_match_lifted_supervisor_key_consts`
        // (40cc4e5) pin on the M2 supervision-tree top-level axis —
        // same discipline the SupervisorSpec top-level lift established,
        // extended here to the M3 [`Membro`] per-`:membros` axis.
        let m = Membro {
            caixa: "catalog".into(),
            versao: "^0.1".into(),
        };
        let json = serde_json::to_string(&m).unwrap();
        for key in [crate::MEMBRO_KEY_CAIXA, crate::MEMBRO_KEY_VERSAO] {
            let quoted = format!("\"{key}\"");
            assert!(
                json.contains(&quoted),
                "serialized Membro must carry the lifted MEMBRO_KEY_* \
                 byte-sequence {quoted} verbatim in the JSON emission \
                 (got: {json})",
            );
        }
    }

    #[test]
    fn membro_key_consts_are_pairwise_distinct() {
        // Cross-axis drift-detection pin: a future collapse of the two
        // canonical [`Membro`] per-entry byte-strings onto the same
        // value (e.g. an accidental copy-paste flip of
        // [`crate::MEMBRO_KEY_VERSAO`] to also read `"caixa"`) would
        // silently reroute every downstream probe on one axis onto the
        // sibling axis's overlay entry and pass every propagation-probe
        // test that expected only the stale axis's value. Peer of the
        // sibling four-way distinct pin on the `SUPERVISOR_KEY_*` tetrad
        // (40cc4e5).
        let all = [crate::MEMBRO_KEY_CAIXA, crate::MEMBRO_KEY_VERSAO];
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(
                    a, b,
                    "MEMBRO_KEY_* consts must be pairwise-distinct \
                     canonical byte-sequences — got `{a}` == `{b}`",
                );
            }
        }
    }

    #[test]
    fn membro_key_consts_are_lower_camel_case_shape() {
        // Shape-pin: every `MEMBRO_KEY_*` const must be a
        // lowerCamelCase byte-sequence (no `snake_case` underscores, no
        // `kebab-case` hyphens, no leading colon, no `PascalCase`
        // leading capital, no whitespace / dots) — the canonical shape
        // the `#[serde(rename_all = "camelCase")]` derive produces on
        // [`Membro`]. A future flip to a non-camelCase attribute at
        // the derive surfaces both here (this test fails on the
        // stale-constant shape) and at
        // `membro_serde_keys_match_lifted_membro_key_consts` (that test
        // fails on the mismatch between const and derive). Peer with
        // `supervisor_key_consts_are_lower_camel_case_shape` (40cc4e5)
        // on the sibling `SupervisorSpec` top-level axis.
        for key in [crate::MEMBRO_KEY_CAIXA, crate::MEMBRO_KEY_VERSAO] {
            assert!(
                !key.is_empty(),
                "MEMBRO_KEY_* must be non-empty (got {key:?})"
            );
            let first = key.chars().next().unwrap();
            assert!(
                first.is_ascii_lowercase(),
                "MEMBRO_KEY_* must lead with an ASCII-lowercase byte \
                 (got {key:?}, leads with {first:?})",
            );
            assert!(
                key.chars().all(|c| c.is_ascii_alphanumeric()),
                "MEMBRO_KEY_* must be ASCII-alphanumeric only \
                 — no `_` / `-` / `:` / `.` / whitespace (got {key:?})",
            );
        }
    }

    // ── drift-detection: serde-derive-to-CONTRATO_KEY_* identity ─────────

    #[test]
    fn wit_contract_serde_keys_match_lifted_contrato_key_consts() {
        // Load-bearing invariant: the three `CONTRATO_KEY_*` consts
        // ([`crate::CONTRATO_KEY_DE`] / [`crate::CONTRATO_KEY_PARA`] /
        // [`crate::CONTRATO_KEY_WIT`]) name the exact camelCase JSON
        // keys the `#[serde(rename_all = "camelCase")]` attribute on
        // [`WitContract`] emits for the required-triad. The three
        // sibling payload-arm keys already pin under
        // [`WitTarget::HTTP_FIELD_NAME`] / `PUBSUB_FIELD_NAME` /
        // `STORE_FIELD_NAME` — pin all six alongside so a future
        // accidental `rename_all = "snake_case"` / `"kebab-case"` /
        // verbatim-field-name flip at the derive attribute (any of which
        // would silently break every downstream JSON consumer that
        // reaches for one of the six via `Value::get(...)`) surfaces
        // here as a build-time test failure at `aplicacao.rs`, not as an
        // apply-time `.get(<stale-canonical-const>)` returning `None`
        // far from the derive-attr drift's commit. Peer with the sibling
        // `membro_serde_keys_match_lifted_membro_key_consts` (ce80ca0)
        // pin on the M3 `:membros` per-entry axis — same discipline the
        // `Membro` per-entry lift established, extended here to the
        // sibling M3 `WitContract` per-`:contratos` entry axis, the last
        // M3 mesh-slot atom top-level `#[serde(rename_all = "camelCase")]`
        // axis on the Aplicacao surface without a lifted serde-key peer.
        let c = WitContract {
            de: "cart".into(),
            para: "catalog".into(),
            wit: "wasi:http/proxy".into(),
            endpoint: Some("/lookup".into()),
            subject: None,
            slot: None,
        };
        let json = serde_json::to_string(&c).unwrap();
        for key in [
            crate::CONTRATO_KEY_DE,
            crate::CONTRATO_KEY_PARA,
            crate::CONTRATO_KEY_WIT,
            WitTarget::HTTP_FIELD_NAME,
        ] {
            let quoted = format!("\"{key}\"");
            assert!(
                json.contains(&quoted),
                "serialized WitContract must carry the lifted \
                 CONTRATO_KEY_* / WitTarget::*_FIELD_NAME byte-sequence \
                 {quoted} verbatim in the JSON emission (got: {json})",
            );
        }

        // Pin the two remaining payload-arm keys by round-tripping a
        // `WitContract` under each payload-shape (pub-sub, store) — the
        // required-triad appears on every emission but the payload arms
        // only surface when their `Option<String>` field is `Some`.
        let pubsub = WitContract {
            de: "cart".into(),
            para: "events".into(),
            wit: "nats:pub-sub".into(),
            endpoint: None,
            subject: Some("orders.placed".into()),
            slot: None,
        };
        let pubsub_json = serde_json::to_string(&pubsub).unwrap();
        let pubsub_quoted = format!("\"{}\"", WitTarget::PUBSUB_FIELD_NAME);
        assert!(
            pubsub_json.contains(&pubsub_quoted),
            "serialized pub-sub WitContract must carry the lifted \
             WitTarget::PUBSUB_FIELD_NAME byte-sequence {pubsub_quoted} \
             verbatim in the JSON emission (got: {pubsub_json})",
        );
        let store = WitContract {
            de: "cart".into(),
            para: "sessions".into(),
            wit: "wasi:keyvalue/store".into(),
            endpoint: None,
            subject: None,
            slot: Some("cart/$id".into()),
        };
        let store_json = serde_json::to_string(&store).unwrap();
        let store_quoted = format!("\"{}\"", WitTarget::STORE_FIELD_NAME);
        assert!(
            store_json.contains(&store_quoted),
            "serialized store WitContract must carry the lifted \
             WitTarget::STORE_FIELD_NAME byte-sequence {store_quoted} \
             verbatim in the JSON emission (got: {store_json})",
        );
    }

    #[test]
    fn contrato_key_consts_are_pairwise_distinct() {
        // Cross-axis drift-detection pin: a future collapse of the six
        // canonical [`WitContract`] per-entry byte-strings onto the same
        // value (e.g. an accidental copy-paste flip of
        // [`crate::CONTRATO_KEY_WIT`] to also read `"de"`, or a
        // rebrand of [`WitTarget::STORE_FIELD_NAME`] to match the
        // sibling [`WitTarget::HTTP_FIELD_NAME`]) would silently reroute
        // every downstream probe on one axis onto the sibling axis's
        // overlay entry and pass every propagation-probe test that
        // expected only the stale axis's value. Peer of the sibling
        // two-way distinct pin on the `MEMBRO_KEY_*` pair (ce80ca0) —
        // widened here to the six-way axis the `WitContract`
        // required-triad + `WitTarget` payload-triad jointly cover.
        let all = [
            crate::CONTRATO_KEY_DE,
            crate::CONTRATO_KEY_PARA,
            crate::CONTRATO_KEY_WIT,
            WitTarget::HTTP_FIELD_NAME,
            WitTarget::PUBSUB_FIELD_NAME,
            WitTarget::STORE_FIELD_NAME,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(
                    a, b,
                    "CONTRATO_KEY_* / WitTarget::*_FIELD_NAME consts \
                     must be pairwise-distinct canonical byte-sequences \
                     — got `{a}` == `{b}`",
                );
            }
        }
    }

    #[test]
    fn contrato_key_consts_are_lower_camel_case_shape() {
        // Shape-pin: every `CONTRATO_KEY_*` (and every peer
        // `WitTarget::*_FIELD_NAME`) const must be a lowerCamelCase
        // byte-sequence (no `snake_case` underscores, no `kebab-case`
        // hyphens, no leading colon, no `PascalCase` leading capital, no
        // whitespace / dots) — the canonical shape the
        // `#[serde(rename_all = "camelCase")]` derive produces on
        // [`WitContract`]. A future flip to a non-camelCase attribute at
        // the derive surfaces both here (this test fails on the
        // stale-constant shape) and at
        // `wit_contract_serde_keys_match_lifted_contrato_key_consts`
        // (that test fails on the mismatch between const and derive).
        // Peer with `membro_key_consts_are_lower_camel_case_shape`
        // (ce80ca0) on the sibling `Membro` per-entry axis.
        for key in [
            crate::CONTRATO_KEY_DE,
            crate::CONTRATO_KEY_PARA,
            crate::CONTRATO_KEY_WIT,
            WitTarget::HTTP_FIELD_NAME,
            WitTarget::PUBSUB_FIELD_NAME,
            WitTarget::STORE_FIELD_NAME,
        ] {
            assert!(
                !key.is_empty(),
                "CONTRATO_KEY_* / WitTarget::*_FIELD_NAME must be \
                 non-empty (got {key:?})"
            );
            let first = key.chars().next().unwrap();
            assert!(
                first.is_ascii_lowercase(),
                "CONTRATO_KEY_* / WitTarget::*_FIELD_NAME must lead \
                 with an ASCII-lowercase byte (got {key:?}, leads with \
                 {first:?})",
            );
            assert!(
                key.chars().all(|c| c.is_ascii_alphanumeric()),
                "CONTRATO_KEY_* / WitTarget::*_FIELD_NAME must be \
                 ASCII-alphanumeric only — no `_` / `-` / `:` / `.` / \
                 whitespace (got {key:?})",
            );
        }
    }

    // ── drift-detection: serde-derive-to-ENTRADA_KEY_* identity ──────────

    #[test]
    fn entrada_serde_keys_match_lifted_entrada_key_consts() {
        // Load-bearing invariant: the four `ENTRADA_KEY_*` consts
        // ([`crate::ENTRADA_KEY_HOST`] / [`crate::ENTRADA_KEY_PARA`] /
        // [`crate::ENTRADA_KEY_PATHS`] / [`crate::ENTRADA_KEY_PORT`])
        // name the exact camelCase JSON keys the
        // `#[serde(rename_all = "camelCase")]` attribute on
        // [`Entrada`] emits. Serialize a fully-populated `Entrada` and
        // pin that each canonical byte-sequence appears verbatim in the
        // JSON — a future accidental `rename_all = "snake_case"` /
        // `"kebab-case"` / verbatim-field-name flip at the derive
        // attribute (any of which would silently break every downstream
        // JSON consumer that reaches for one of the four consts via
        // `Value::get(...)` — the [`caixa_mesh`] Gateway/HTTPRoute
        // emitter's per-Aplicacao hostname/paths/port projection, the
        // future `app-operator` reconciler's per-Aplicacao ingress
        // bind, the future `mesh.pleme.io/v1alpha1/Aplicacao` CR
        // materializer's admission-time cross-check) surfaces here as
        // a build-time test failure at `aplicacao.rs`, not as an
        // apply-time `.get(<stale-canonical-const>)` returning `None`
        // far from the derive-attr drift's commit. Peer with the
        // sibling
        // `wit_contract_serde_keys_match_lifted_contrato_key_consts`
        // (ca463a4) and
        // `membro_serde_keys_match_lifted_membro_key_consts` (ce80ca0)
        // pins on the M3 collection-slot atom axes — same discipline
        // both collection-slot lifts established, extended here to the
        // singleton `:entrada` mesh-slot atom axis, the last M3
        // typed-struct top-level `#[serde(rename_all = "camelCase")]`
        // axis on the Aplicacao surface without a lifted serde-key
        // peer.
        let e = Entrada {
            host: "checkout.quero.cloud".into(),
            para: "cart".into(),
            paths: vec!["/cart".into()],
            port: 8080,
        };
        let json = serde_json::to_string(&e).unwrap();
        for key in [
            crate::ENTRADA_KEY_HOST,
            crate::ENTRADA_KEY_PARA,
            crate::ENTRADA_KEY_PATHS,
            crate::ENTRADA_KEY_PORT,
        ] {
            let quoted = format!("\"{key}\"");
            assert!(
                json.contains(&quoted),
                "serialized Entrada must carry the lifted ENTRADA_KEY_* \
                 byte-sequence {quoted} verbatim in the JSON emission \
                 (got: {json})",
            );
        }
    }

    #[test]
    fn entrada_key_consts_are_pairwise_distinct() {
        // Cross-axis drift-detection pin: a future collapse of the four
        // canonical [`Entrada`] singleton byte-strings onto the same
        // value (e.g. an accidental copy-paste flip of
        // [`crate::ENTRADA_KEY_PARA`] to also read `"host"`) would
        // silently reroute every downstream probe on one axis onto the
        // sibling axis's overlay entry and pass every propagation-probe
        // test that expected only the stale axis's value — the
        // Gateway/HTTPRoute emitter would read the hostname string
        // where the destination-Servico name was expected (or vice
        // versa), the admission-webhook cross-check would compare the
        // wrong pair of values, and the resulting Gateway resource
        // would either be admitted with garbage or rejected at the
        // controller far from the rebrand commit's source. Peer of the
        // sibling four-way distinct pin on the `SUPERVISOR_KEY_*`
        // tetrad (40cc4e5), the two-way distinct pin on the
        // `MEMBRO_KEY_*` pair (ce80ca0), and the six-way distinct pin
        // on the `CONTRATO_KEY_*` triad + `WitTarget::*_FIELD_NAME`
        // triad (ca463a4).
        let all = [
            crate::ENTRADA_KEY_HOST,
            crate::ENTRADA_KEY_PARA,
            crate::ENTRADA_KEY_PATHS,
            crate::ENTRADA_KEY_PORT,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(
                    a, b,
                    "ENTRADA_KEY_* consts must be pairwise-distinct \
                     canonical byte-sequences — got `{a}` == `{b}`",
                );
            }
        }
    }

    #[test]
    fn entrada_key_consts_are_lower_camel_case_shape() {
        // Shape-pin: every `ENTRADA_KEY_*` const must be a
        // lowerCamelCase byte-sequence (no `snake_case` underscores, no
        // `kebab-case` hyphens, no leading colon, no `PascalCase`
        // leading capital, no whitespace / dots) — the canonical shape
        // the `#[serde(rename_all = "camelCase")]` derive produces on
        // [`Entrada`]. A future flip to a non-camelCase attribute at
        // the derive surfaces both here (this test fails on the
        // stale-constant shape) and at
        // `entrada_serde_keys_match_lifted_entrada_key_consts` (that
        // test fails on the mismatch between const and derive). Peer
        // with `membro_key_consts_are_lower_camel_case_shape` (ce80ca0)
        // and `contrato_key_consts_are_lower_camel_case_shape`
        // (ca463a4) on the sibling M3 per-`:membros` and per-`:contratos`
        // entry axes.
        for key in [
            crate::ENTRADA_KEY_HOST,
            crate::ENTRADA_KEY_PARA,
            crate::ENTRADA_KEY_PATHS,
            crate::ENTRADA_KEY_PORT,
        ] {
            assert!(
                !key.is_empty(),
                "ENTRADA_KEY_* must be non-empty (got {key:?})"
            );
            let first = key.chars().next().unwrap();
            assert!(
                first.is_ascii_lowercase(),
                "ENTRADA_KEY_* must lead with an ASCII-lowercase byte \
                 (got {key:?}, leads with {first:?})",
            );
            assert!(
                key.chars().all(|c| c.is_ascii_alphanumeric()),
                "ENTRADA_KEY_* must be ASCII-alphanumeric only \
                 — no `_` / `-` / `:` / `.` / whitespace (got {key:?})",
            );
        }
    }

    // ── drift-detection: serde-derive-to-POLITICAS_KEY_* identity ────────

    #[test]
    fn mesh_policy_serde_keys_match_lifted_politicas_key_consts() {
        // Load-bearing invariant: the five `POLITICAS_KEY_*` consts
        // ([`crate::POLITICAS_KEY_TIMEOUT`] /
        // [`crate::POLITICAS_KEY_RETRIES`] /
        // [`crate::POLITICAS_KEY_CIRCUIT_BREAKER`] /
        // [`crate::POLITICAS_KEY_MTLS_REQUIRED`] /
        // [`crate::POLITICAS_KEY_RATE_LIMIT`]) name the exact camelCase
        // JSON keys the `#[serde(rename_all = "camelCase")]` attribute
        // on [`MeshPolicy`] emits. Three of the five axes
        // (`circuit_breaker` → `circuitBreaker`, `mtls_required` →
        // `mtlsRequired`, `rate_limit` → `rateLimit`) are non-trivial
        // camelCase transforms — the derive-attribute is load-bearing
        // on those, unlike the sibling `Entrada` / `Membro` /
        // `WitContract` structs whose fields are all lowercase-single-
        // word and where the derive is a no-op on every axis.
        // Serialize a fully-populated [`MeshPolicy`] (every axis
        // `Some(…)` so `skip_serializing_if = "Option::is_none"` fires
        // on none of the five slots) and pin that each canonical
        // byte-sequence appears verbatim in the JSON — a future
        // accidental `rename_all = "snake_case"` / `"kebab-case"` /
        // verbatim-field-name flip at the derive attribute (any of
        // which would silently break every downstream JSON consumer
        // that reaches for one of the five consts via
        // `Value::get(...)` — the future M4 per-edge `:politicas`
        // overlay projection onto Cilium `L7Rules` and Gateway API
        // `HTTPRoute` backend timeouts, the future
        // `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's
        // admission-time mesh-policy cross-check, the future
        // `feira lint` per-`:politicas` bound-check gate) surfaces here
        // as a build-time test failure at `aplicacao.rs`, not as an
        // apply-time `.get(<stale-canonical-const>)` returning `None`
        // far from the derive-attr drift's commit. Peer with the
        // sibling `entrada_serde_keys_match_lifted_entrada_key_consts`
        // (a3d6162), `wit_contract_serde_keys_match_lifted_contrato_key_consts`
        // (ca463a4), and `membro_serde_keys_match_lifted_membro_key_consts`
        // (ce80ca0) pins on the M3 collection-slot / singleton-slot
        // atom axes — same discipline every M3 sibling lift
        // established, extended here to the singleton `:politicas`
        // mesh-slot atom axis, closing the last M3 typed-struct
        // top-level `#[serde(rename_all = "camelCase")]` axis on the
        // Aplicacao surface without a lifted serde-key peer.
        let p = MeshPolicy {
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
        let json = serde_json::to_string(&p).unwrap();
        for key in [
            crate::POLITICAS_KEY_TIMEOUT,
            crate::POLITICAS_KEY_RETRIES,
            crate::POLITICAS_KEY_CIRCUIT_BREAKER,
            crate::POLITICAS_KEY_MTLS_REQUIRED,
            crate::POLITICAS_KEY_RATE_LIMIT,
        ] {
            let quoted = format!("\"{key}\"");
            assert!(
                json.contains(&quoted),
                "serialized MeshPolicy must carry the lifted \
                 POLITICAS_KEY_* byte-sequence {quoted} verbatim in the \
                 JSON emission (got: {json})",
            );
        }
    }

    #[test]
    fn politicas_key_consts_are_pairwise_distinct() {
        // Cross-axis drift-detection pin: a future collapse of the five
        // canonical [`MeshPolicy`] singleton byte-strings onto the same
        // value (e.g. an accidental copy-paste flip of
        // [`crate::POLITICAS_KEY_RETRIES`] to also read `"timeout"`)
        // would silently reroute every downstream probe on one axis
        // onto the sibling axis's overlay entry and pass every
        // propagation-probe test that expected only the stale axis's
        // value — the M4 per-edge `:politicas` overlay projection would
        // read the retry-count string where the timeout duration was
        // expected (or vice versa), the CR materializer's admission
        // cross-check would compare the wrong pair of values, and the
        // resulting mesh reconciler would either bind the wrong axis
        // or reject the resource at reconcile far from the rebrand
        // commit's source. Peer of the sibling four-way distinct pin
        // on the `SUPERVISOR_KEY_*` tetrad (40cc4e5), the four-way
        // distinct pin on the `ENTRADA_KEY_*` tetrad (a3d6162), the
        // two-way distinct pin on the `MEMBRO_KEY_*` pair (ce80ca0),
        // and the six-way distinct pin on the `CONTRATO_KEY_*` triad +
        // `WitTarget::*_FIELD_NAME` triad (ca463a4).
        let all = [
            crate::POLITICAS_KEY_TIMEOUT,
            crate::POLITICAS_KEY_RETRIES,
            crate::POLITICAS_KEY_CIRCUIT_BREAKER,
            crate::POLITICAS_KEY_MTLS_REQUIRED,
            crate::POLITICAS_KEY_RATE_LIMIT,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(
                    a, b,
                    "POLITICAS_KEY_* consts must be pairwise-distinct \
                     canonical byte-sequences — got `{a}` == `{b}`",
                );
            }
        }
    }

    #[test]
    fn politicas_key_consts_are_lower_camel_case_shape() {
        // Shape-pin: every `POLITICAS_KEY_*` const must be a
        // lowerCamelCase byte-sequence (no `snake_case` underscores, no
        // `kebab-case` hyphens, no leading colon, no `PascalCase`
        // leading capital, no whitespace / dots) — the canonical shape
        // the `#[serde(rename_all = "camelCase")]` derive produces on
        // [`MeshPolicy`]. A future flip to a non-camelCase attribute
        // at the derive surfaces both here (this test fails on the
        // stale-constant shape) and at
        // `mesh_policy_serde_keys_match_lifted_politicas_key_consts`
        // (that test fails on the mismatch between const and derive).
        // Peer with `entrada_key_consts_are_lower_camel_case_shape`
        // (a3d6162), `membro_key_consts_are_lower_camel_case_shape`
        // (ce80ca0), and `contrato_key_consts_are_lower_camel_case_shape`
        // (ca463a4) on the sibling M3 typed-struct axes.
        for key in [
            crate::POLITICAS_KEY_TIMEOUT,
            crate::POLITICAS_KEY_RETRIES,
            crate::POLITICAS_KEY_CIRCUIT_BREAKER,
            crate::POLITICAS_KEY_MTLS_REQUIRED,
            crate::POLITICAS_KEY_RATE_LIMIT,
        ] {
            assert!(
                !key.is_empty(),
                "POLITICAS_KEY_* must be non-empty (got {key:?})"
            );
            let first = key.chars().next().unwrap();
            assert!(
                first.is_ascii_lowercase(),
                "POLITICAS_KEY_* must lead with an ASCII-lowercase \
                 byte (got {key:?}, leads with {first:?})",
            );
            assert!(
                key.chars().all(|c| c.is_ascii_alphanumeric()),
                "POLITICAS_KEY_* must be ASCII-alphanumeric only — \
                 no `_` / `-` / `:` / `.` / whitespace (got {key:?})",
            );
        }
    }

    // ── drift-detection: serde-derive-to-CIRCUIT_BREAKER_KEY_* identity ──

    #[test]
    fn circuit_breaker_serde_keys_match_lifted_circuit_breaker_key_consts() {
        // Load-bearing invariant: the two `CIRCUIT_BREAKER_KEY_*` consts
        // ([`crate::CIRCUIT_BREAKER_KEY_MAX_FAILURES`] /
        // [`crate::CIRCUIT_BREAKER_KEY_WINDOW`]) name the exact camelCase
        // JSON keys the `#[serde(rename_all = "camelCase")]` attribute on
        // [`CircuitBreaker`] emits inside the
        // [`crate::POLITICAS_KEY_CIRCUIT_BREAKER`] sub-block. One of the
        // two axes (`max_failures` → `maxFailures`) is a non-trivial
        // camelCase transform — the derive-attribute is load-bearing on
        // that axis, unlike the sibling `window` field where the derive
        // is a no-op. Serialize a fully-populated [`CircuitBreaker`] and
        // pin that each canonical byte-sequence appears verbatim in the
        // JSON — a future accidental `rename_all = "snake_case"` /
        // `"kebab-case"` / verbatim-field-name flip at the derive
        // attribute (any of which would silently break every downstream
        // JSON consumer that reaches for one of the two consts via
        // `Value::get(POLITICAS_KEY_CIRCUIT_BREAKER).and_then(|v|
        // v.get(CIRCUIT_BREAKER_KEY_MAX_FAILURES))` — the future M4
        // per-edge `:politicas` overlay projection onto the mesh's
        // per-backend consecutive-failure-counter tripping threshold, the
        // future `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's
        // admission-time breaker cross-check, the future `feira lint`
        // per-`:politicas :circuit-breaker` bound-check gate) surfaces
        // here as a build-time test failure at `aplicacao.rs`, not as an
        // apply-time `.get(<stale-canonical-const>)` returning `None`
        // far from the derive-attr drift's commit. Peer with the sibling
        // `mesh_policy_serde_keys_match_lifted_politicas_key_consts`
        // (b55cca7) parent-axis pin — that test pins the outer
        // sub-block key the derive on [`MeshPolicy`] emits, this test
        // pins the inner keys the derive on the payload type emits, so
        // the two together lock the whole [`MeshPolicy`] breaker-tuning
        // shape end-to-end at build time.
        let cb = CircuitBreaker {
            max_failures: 5,
            window: Duration::from_secs(60),
        };
        let json = serde_json::to_string(&cb).unwrap();
        for key in [
            crate::CIRCUIT_BREAKER_KEY_MAX_FAILURES,
            crate::CIRCUIT_BREAKER_KEY_WINDOW,
        ] {
            let quoted = format!("\"{key}\"");
            assert!(
                json.contains(&quoted),
                "serialized CircuitBreaker must carry the lifted \
                 CIRCUIT_BREAKER_KEY_* byte-sequence {quoted} verbatim \
                 in the JSON emission (got: {json})",
            );
        }
    }

    #[test]
    fn circuit_breaker_key_consts_are_pairwise_distinct() {
        // Cross-axis drift-detection pin: a future collapse of the two
        // canonical [`CircuitBreaker`] sub-block byte-strings onto the
        // same value (e.g. an accidental copy-paste flip of
        // [`crate::CIRCUIT_BREAKER_KEY_WINDOW`] to also read
        // `"maxFailures"`) would silently reroute every downstream
        // probe on one axis onto the sibling axis's overlay entry and
        // pass every propagation-probe test that expected only the
        // stale axis's value — the M4 per-edge `:politicas` overlay
        // projection would read the failure-count where the window
        // duration was expected (or vice versa), the CR materializer's
        // admission cross-check would compare the wrong pair of values,
        // and the resulting mesh reconciler would either bind the wrong
        // axis or reject the resource at reconcile far from the rebrand
        // commit's source. Peer of the sibling five-way distinct pin on
        // the `POLITICAS_KEY_*` pentad (b55cca7), the four-way distinct
        // pin on the `ENTRADA_KEY_*` tetrad (a3d6162), the two-way
        // distinct pin on the `MEMBRO_KEY_*` pair (ce80ca0), and the
        // six-way distinct pin on the `CONTRATO_KEY_*` triad +
        // `WitTarget::*_FIELD_NAME` triad (ca463a4).
        let all = [
            crate::CIRCUIT_BREAKER_KEY_MAX_FAILURES,
            crate::CIRCUIT_BREAKER_KEY_WINDOW,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(
                    a, b,
                    "CIRCUIT_BREAKER_KEY_* consts must be pairwise-distinct \
                     canonical byte-sequences — got `{a}` == `{b}`",
                );
            }
        }
    }

    #[test]
    fn circuit_breaker_key_consts_are_lower_camel_case_shape() {
        // Shape-pin: every `CIRCUIT_BREAKER_KEY_*` const must be a
        // lowerCamelCase byte-sequence (no `snake_case` underscores, no
        // `kebab-case` hyphens, no leading colon, no `PascalCase`
        // leading capital, no whitespace / dots) — the canonical shape
        // the `#[serde(rename_all = "camelCase")]` derive produces on
        // [`CircuitBreaker`]. A future flip to a non-camelCase attribute
        // at the derive surfaces both here (this test fails on the
        // stale-constant shape) and at
        // `circuit_breaker_serde_keys_match_lifted_circuit_breaker_key_consts`
        // (that test fails on the mismatch between const and derive).
        // Peer with `politicas_key_consts_are_lower_camel_case_shape`
        // (b55cca7), `entrada_key_consts_are_lower_camel_case_shape`
        // (a3d6162), `membro_key_consts_are_lower_camel_case_shape`
        // (ce80ca0), and `contrato_key_consts_are_lower_camel_case_shape`
        // (ca463a4) on the sibling M3 typed-struct axes.
        for key in [
            crate::CIRCUIT_BREAKER_KEY_MAX_FAILURES,
            crate::CIRCUIT_BREAKER_KEY_WINDOW,
        ] {
            assert!(
                !key.is_empty(),
                "CIRCUIT_BREAKER_KEY_* must be non-empty (got {key:?})"
            );
            let first = key.chars().next().unwrap();
            assert!(
                first.is_ascii_lowercase(),
                "CIRCUIT_BREAKER_KEY_* must lead with an ASCII-lowercase \
                 byte (got {key:?}, leads with {first:?})",
            );
            assert!(
                key.chars().all(|c| c.is_ascii_alphanumeric()),
                "CIRCUIT_BREAKER_KEY_* must be ASCII-alphanumeric only — \
                 no `_` / `-` / `:` / `.` / whitespace (got {key:?})",
            );
        }
    }
}
