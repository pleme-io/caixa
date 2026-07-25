//! OTP-shaped behavior callbacks — the typed slot of `caixa.lisp`
//! that points at the `.lisp` files implementing the lifecycle.
//!
//! See `theory/INSPIRATIONS.md` §II.3 for the prior-art frame
//! (`gen_server`, `gen_statem`, `gen_event`). Authors implement the
//! callbacks; the runtime owns init / message dispatch / terminate.
//!
//! ```lisp
//! (defcaixa
//!   :nome     "my-service"
//!   :versao   "0.1.0"
//!   :kind     Servico
//!   :behavior ((:on-init         "lib/init.lisp")
//!              (:on-call         "lib/handlers.lisp")
//!              (:on-cast         "lib/handlers.lisp")
//!              (:on-info         "lib/handlers.lisp")
//!              (:on-state-change "lib/migrations.lisp")
//!              (:on-terminate    "lib/cleanup.lisp"))
//!   :servicos ("servicos/my-service.computeunit.yaml"))
//! ```
//!
//! Each slot is optional — caixas without explicit callbacks fall
//! back to the runtime defaults (no-op init, raw HTTP dispatch,
//! noop terminate). The `StandardLayout` invariant in `layout.rs`
//! verifies every declared path exists on disk before the build.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Path-to-callback bindings for an OTP-shaped Servico.
///
/// All fields optional. The wasm-engine looks up the callback by
/// kind at instance start; if absent, the runtime default is used.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BehaviorSpec {
    /// Called once before the instance accepts traffic. Analog of
    /// `gen_server:init/1`. Runs to completion or the instance
    /// fails to start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_init: Option<PathBuf>,

    /// Synchronous request/response handler. Analog of
    /// `gen_server:handle_call/3` — reply is awaited by the caller.
    /// For HTTP servicos this is the wasi:http/incoming-handler.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_call: Option<PathBuf>,

    /// Asynchronous fire-and-forget handler. Analog of
    /// `gen_server:handle_cast/2` — caller does not wait. For HTTP
    /// servicos this maps onto `Accepted: 202` shapes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_cast: Option<PathBuf>,

    /// System / out-of-band message handler. Analog of
    /// `gen_server:handle_info/2` — timeouts, downstream `nodedown`,
    /// monitor signals, scheduler ticks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_info: Option<PathBuf>,

    /// State migration callback for hot-upgrades. Analog of
    /// `gen_server:code_change/3` — receives old state + version,
    /// returns new state. Composes with the `:upgrade-from` slot
    /// declared at the Caixa root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_state_change: Option<PathBuf>,

    /// Cleanup callback before the instance shuts down. Analog of
    /// `gen_server:terminate/2`. Best-effort — runs only when the
    /// instance terminates gracefully (not on hard kill).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_terminate: Option<PathBuf>,
}

impl BehaviorSpec {
    /// Iterate over every declared callback path tagged with the
    /// kebab-case `:on-*` slot it came from. Used by the layout
    /// checker (existence) and by [`BehaviorSpec::validate`]
    /// (value-shape) so diagnostics can name the offending slot.
    ///
    /// Each per-arm kebab-case label is routed through the peer
    /// [`crate::M2_BEHAVIOR_AUTHOR_KEY_ON_*`] consts declared next to
    /// the [`crate::M2_BEHAVIOR_KEY_ON_*`] renderer-side wire-key
    /// peers, so both halves of the M2 `:behavior` sub-slot's dual
    /// axis (author-facing kebab-case label + renderer-side camelCase
    /// wire key) route through one canonical declaration per arm.
    ///
    /// Each per-arm `Option<&Path>` path-value is routed through the
    /// sibling lifted [`BehaviorSpec::on_init`] / [`BehaviorSpec::on_call`]
    /// / [`BehaviorSpec::on_cast`] / [`BehaviorSpec::on_info`] /
    /// [`BehaviorSpec::on_state_change`] / [`BehaviorSpec::on_terminate`]
    /// per-slot accessors, so the iterator's per-arm typed dispatch
    /// composes with every future accessor-side extension (a
    /// per-prior-`:versao` state-migration callback the operator pins
    /// through a future `:behavior :on-state-change-overrides` slot the
    /// `theory/ABSORPTION-ROADMAP.md` M2.5 wasm-engine callback-dispatch
    /// wire acknowledges, a per-tenant callback alias table the M4 CR
    /// materializer resolves per-CR, a per-cluster callback overlay the
    /// operator pins through a future placement-scoped slot) as a unit:
    /// the layout checker's existence sweep + the sibling
    /// [`BehaviorSpec::validate`] value-shape gate consume whichever
    /// accept-set the accessor exposes, so both halves of the diagnostic
    /// surface migrate together. Prior to this converge the six per-arm
    /// `self.on_*.as_ref()` raw-field-access sites bypassed the accessor
    /// dispatch — the accessors owned the accept-set on the read side but
    /// the iterator that both production consumers actually read
    /// projected through the raw `Option<PathBuf>` field, silently
    /// disagreeing with every accessor extension until the two-site
    /// rewrite reached both halves in lockstep.
    pub fn declared_slots(&self) -> impl Iterator<Item = (&'static str, &Path)> {
        [
            (
                crate::render::M2_BEHAVIOR_AUTHOR_KEY_ON_INIT,
                self.on_init(),
            ),
            (
                crate::render::M2_BEHAVIOR_AUTHOR_KEY_ON_CALL,
                self.on_call(),
            ),
            (
                crate::render::M2_BEHAVIOR_AUTHOR_KEY_ON_CAST,
                self.on_cast(),
            ),
            (
                crate::render::M2_BEHAVIOR_AUTHOR_KEY_ON_INFO,
                self.on_info(),
            ),
            (
                crate::render::M2_BEHAVIOR_AUTHOR_KEY_ON_STATE_CHANGE,
                self.on_state_change(),
            ),
            (
                crate::render::M2_BEHAVIOR_AUTHOR_KEY_ON_TERMINATE,
                self.on_terminate(),
            ),
        ]
        .into_iter()
        .filter_map(|(slot, opt)| opt.map(|p| (slot, p)))
    }

    /// Iterate over every declared callback path. Used by the
    /// layout checker.
    pub fn declared_paths(&self) -> impl Iterator<Item = &Path> {
        self.declared_slots().map(|(_slot, p)| p)
    }

    /// True when no callback is declared.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.declared_paths().next().is_none()
    }

    /// Substrate-canonical per-`:behavior` `:on-state-change`
    /// OTP-`gen_server:code_change/3`-shaped state-migration callback
    /// path scalar accessor every consumer of the Servico's hot-upgrade
    /// dispatch keys off — returns the author-declared
    /// `:behavior :on-state-change` typed callback path verbatim as an
    /// `Option<&Path>`, borrowed from the typed slot's own
    /// `Option<PathBuf>` storage. `None` when the slot is absent (the
    /// canonical "no state-migration callback declared — the caixa
    /// exposes no hot-upgrade state-fold path, so any `:upgrade-from`
    /// entry carrying a `(:state-change …)` instruction is structurally
    /// half a composition" arm the peer
    /// [`crate::validate_upgrade_from_against_behavior`] cross-slot gate
    /// keys off through this accessor).
    ///
    /// The `:behavior :on-state-change` slot carries the OTP
    /// `gen_server:code_change/3` callback contract (the module-level
    /// [`BehaviorSpec::on_state_change`] docstring pins the analog verbatim:
    /// "State migration callback for hot-upgrades. Analog of
    /// `gen_server:code_change/3` — receives old state + version, returns
    /// new state. Composes with the `:upgrade-from` slot declared at the
    /// Caixa root."). The composition it half-forms is realized in OTP by
    /// `release_handler:install_release/1`, which invokes the running
    /// `gen_server`'s `code_change/3` during the appup's `code_change` /
    /// `update, m, soft` step — the appup's instruction triggers the
    /// callback, the callback folds the prior-version state shape into
    /// the current-version shape, and the operator advances to the next
    /// instruction only after the callback returns successfully
    /// (`theory/INSPIRATIONS.md` §II.3 — OTP `gen_server` +
    /// `release_handler` state-migration wire, translated onto pleme-io's
    /// typed `:behavior` + `:upgrade-from` slot pair). caixa decomposes
    /// the same composition into two typed slots: the per-version
    /// migration logic lives in the `(:state-change "lib/migrations/…lisp")`
    /// instruction's `:script` (the `:upgrade-from` author surface,
    /// resolved through [`crate::UpgradeInstruction::StateChange`]), and
    /// the runtime hook the operator dispatches the migration through
    /// lives in the `:behavior :on-state-change` callback (the
    /// `:behavior` author surface, resolved through this accessor). The
    /// [`crate::validate_upgrade_from_against_behavior`] cross-slot gate
    /// closes the composition at validate time by refusing a Caixa that
    /// carries a `:state-change` instruction without declaring
    /// `:on-state-change` — the sole caixa-core consumer that reads the
    /// callback's `Option<&Path>` presence rather than the callback path
    /// itself.
    ///
    /// Prior to this lift the `.on_state_change` field was accessed
    /// inline at two sites — [`BehaviorSpec::declared_slots`]'s
    /// `:on-state-change` arm's `self.on_state_change.as_ref()` map into
    /// the six-tuple iterator, and the sibling
    /// [`crate::validate_upgrade_from_against_behavior`] cross-slot gate's
    /// `behavior.and_then(|b| b.on_state_change.as_ref()).is_some()`
    /// short-circuit — two open-coded field-accesses that expressed no
    /// compile-time link back to the typed slot. A future extension of
    /// the `:behavior :on-state-change` axis to a richer author surface —
    /// a per-prior-`:versao` state-migration callback the operator pins
    /// through a future `:behavior :on-state-change-overrides` slot the
    /// `theory/ABSORPTION-ROADMAP.md` M2.5 wasm-engine callback-dispatch
    /// wire acknowledges, a per-tenant migration alias table the M4 CR
    /// materializer resolves per-CR, a per-Aplicacao dynamic
    /// state-change derivation the future adaptive hot-upgrade engine
    /// computes from the sibling `:upgrade-from` instruction chain —
    /// would have had to be threaded through both open-coded copies in
    /// lockstep or the `declared_slots` iterator (the tag surface every
    /// per-slot diagnostic reads) and the
    /// `validate_upgrade_from_against_behavior` gate (the composition
    /// closure every hot-upgrade admission reads) would silently
    /// disagree on which callback a given [`BehaviorSpec`] resolves to.
    /// Lifting the resolution to a typed method on the substrate
    /// primitive means every downstream consumer of the Servico's
    /// per-`:behavior` state-migration callback surface reaches for
    /// exactly one typed dispatch — the resolver's accept-set migrates
    /// as a unit on any future axis addition.
    ///
    /// First `Option<&Path>`-return accessor on the M2 `:behavior` slot
    /// family (peer of the sibling per-`:placement`
    /// [`crate::Placement::shard_key`] 7cd2a28 /
    /// [`crate::Placement::affinity`] 74ec2d3 `Option<&str>` accessors
    /// on the M3 mesh-slot family — same "one typed dispatch on the
    /// substrate primitive, thin projections at each consumer"
    /// discipline extended onto the peer per-`:behavior`
    /// `Option<PathBuf>` optional-scalar axis; opens the "optional
    /// per-slot `Option<&Path>` scalar" projection pattern the sibling
    /// per-`:behavior` `:on-init` / `:on-call` / `:on-cast` / `:on-info`
    /// / `:on-terminate` future lifts fold on). Named
    /// `on_state_change()` to match the storage field's name; the
    /// accessor's identity name maps onto the canonical
    /// `theory/INSPIRATIONS.md` §II.3 vocabulary the slot's docstring
    /// already carries.
    #[must_use]
    pub fn on_state_change(&self) -> Option<&Path> {
        self.on_state_change.as_deref()
    }

    /// Substrate-canonical per-`:behavior` `:on-init` OTP-`gen_server:init/1`-
    /// shaped once-per-instance-start callback-path scalar accessor every
    /// consumer of the Servico's instance-start dispatch keys off — returns
    /// the author-declared `:behavior :on-init` typed callback path
    /// verbatim as an `Option<&Path>`, borrowed from the typed slot's own
    /// `Option<PathBuf>` storage. `None` when the slot is absent (the
    /// canonical "no init callback declared — the runtime falls back to
    /// the wasm-engine's no-op instance-start default" arm the runtime's
    /// callback-lookup consults at instance-start time; peer of the
    /// sibling [`BehaviorSpec::on_state_change`] `None`-arm's
    /// "no state-migration callback" semantic on the sibling axis).
    ///
    /// The `:behavior :on-init` slot carries the OTP
    /// `gen_server:init/1` callback contract (the module-level
    /// [`BehaviorSpec::on_init`] docstring pins the analog verbatim:
    /// "Called once before the instance accepts traffic. Analog of
    /// `gen_server:init/1`. Runs to completion or the instance fails to
    /// start."). Its position in the OTP lifecycle is first — the
    /// runtime instantiates the wasm process, dispatches the init
    /// callback, and only then flips the instance's readiness state so
    /// downstream traffic (`:on-call` / `:on-cast`) is accepted
    /// (`theory/INSPIRATIONS.md` §II.3 — OTP `gen_server` behavior's
    /// six-callback lifecycle, translated onto pleme-io's typed
    /// `:behavior` slot family; `theory/CAIXA-SDLC.md` §I — the
    /// author-surface pins `:on-init` as the first arm of the
    /// `:behavior` overlay every Servico may declare).
    ///
    /// Prior to this lift the `.on_init` field was accessed inline at
    /// one production site — [`BehaviorSpec::declared_slots`]'s
    /// `:on-init` arm's `self.on_init.as_ref()` map into the six-tuple
    /// iterator that both the layout checker (existence sweep at
    /// `layout.rs:900`) and the sibling `BehaviorSpec::validate`
    /// value-shape gate consume — an open-coded field-access that
    /// expressed no compile-time link back to the typed slot. A future
    /// extension of the `:behavior :on-init` axis to a richer author
    /// surface — a per-tenant init-callback override the M4 CR
    /// materializer resolves per-CR, a per-cluster instance-start
    /// callback overlay the `theory/ABSORPTION-ROADMAP.md` M2.5
    /// wasm-engine callback-dispatch wire acknowledges, a per-Aplicacao
    /// dynamic init-callback derivation the future adaptive
    /// hot-instantiation engine computes from the sibling `:limits`
    /// wasm-engine sandbox — would have had to be threaded through the
    /// open-coded field-access in `declared_slots` (the tag surface every
    /// per-slot diagnostic reads) or the `declared_slots` iterator would
    /// silently disagree on which callback a given [`BehaviorSpec`]
    /// resolves to. Lifting the resolution to a typed method on the
    /// substrate primitive means every downstream consumer of the
    /// Servico's per-`:behavior` init-callback surface reaches for
    /// exactly one typed dispatch — the resolver's accept-set migrates
    /// as a unit on any future axis addition.
    ///
    /// Second `Option<&Path>`-return accessor on the M2 `:behavior` slot
    /// family (sibling of the prior [`BehaviorSpec::on_state_change`]
    /// 9b4ecde `Option<&Path>` accessor on the peer per-`:behavior`
    /// `:on-state-change` axis — same "one typed dispatch on the
    /// substrate primitive, thin projections at each consumer"
    /// discipline extended onto the peer per-`:behavior`
    /// `Option<PathBuf>` optional-scalar axis; continues the "optional
    /// per-slot `Option<&Path>` scalar" projection pattern the sibling
    /// per-`:behavior` `:on-call` / `:on-cast` / `:on-info` /
    /// `:on-terminate` future lifts fold on). Named `on_init()` to match
    /// the storage field's name; the accessor's identity name maps onto
    /// the canonical `theory/INSPIRATIONS.md` §II.3 vocabulary the
    /// slot's docstring already carries.
    #[must_use]
    pub fn on_init(&self) -> Option<&Path> {
        self.on_init.as_deref()
    }

    /// Substrate-canonical per-`:behavior` `:on-call`
    /// OTP-`gen_server:handle_call/3`-shaped synchronous
    /// request/response callback-path scalar accessor every consumer of
    /// the Servico's synchronous-dispatch callback path keys off —
    /// returns the author-declared `:behavior :on-call` typed callback
    /// path verbatim as an `Option<&Path>`, borrowed from the typed
    /// slot's own `Option<PathBuf>` storage. `None` when the slot is
    /// absent (the canonical "no synchronous-call callback declared —
    /// the runtime falls back to the wasm-engine's raw
    /// `wasi:http/incoming-handler` default that surfaces the request to
    /// the underlying HTTP proxy world verbatim without any
    /// author-supplied reply-shape interposed" arm the M2.5 wasm-engine
    /// callback-dispatch wire consults at every synchronous incoming
    /// call; peer of the sibling [`BehaviorSpec::on_init`] /
    /// [`BehaviorSpec::on_state_change`] `None`-arm's "no
    /// instance-start / state-migration callback" semantic on the
    /// sibling axes).
    ///
    /// The `:behavior :on-call` slot carries the OTP
    /// `gen_server:handle_call/3` callback contract (the module-level
    /// [`BehaviorSpec::on_call`] docstring pins the analog verbatim:
    /// "Synchronous request/response handler. Analog of
    /// `gen_server:handle_call/3` — reply is awaited by the caller. For
    /// HTTP servicos this is the wasi:http/incoming-handler."). Its
    /// position in the OTP dispatch triad is the request/response half:
    /// the runtime routes every synchronous incoming message (every
    /// `wasi:http/incoming-handler` invocation whose caller awaits a
    /// reply, every synchronous WIT-typed peer edge whose contract
    /// carries a reply payload) through the callback; the callback runs
    /// to completion, computes the reply, and the runtime hands the
    /// reply back to the awaiting caller before flipping the process
    /// back to the mailbox-drain state (`theory/INSPIRATIONS.md` §II.3 —
    /// OTP `gen_server` behavior's six-callback lifecycle, translated
    /// onto pleme-io's typed `:behavior` slot family;
    /// `theory/CAIXA-SDLC.md` §I — the author-surface pins `:on-call` as
    /// the second arm of the `:behavior` overlay every Servico may
    /// declare, sibling to `:on-cast` / `:on-info` on the peer
    /// asynchronous-dispatch axes; `theory/RUNTIME-PATTERNS.md` §II —
    /// the synchronous-request-response pattern the runtime realizes
    /// through this callback).
    ///
    /// Prior to this lift the `.on_call` field was accessed inline at
    /// one production site — [`BehaviorSpec::declared_slots`]'s
    /// `:on-call` arm's `self.on_call.as_ref()` map into the six-tuple
    /// iterator that both the layout checker (existence sweep at
    /// `layout.rs:900`) and the sibling `BehaviorSpec::validate`
    /// value-shape gate consume — an open-coded field-access that
    /// expressed no compile-time link back to the typed slot. A future
    /// extension of the `:behavior :on-call` axis to a richer author
    /// surface — a per-tenant call-callback override the M4 CR
    /// materializer resolves per-CR, a per-cluster synchronous-dispatch
    /// callback overlay the `theory/ABSORPTION-ROADMAP.md` M2.5
    /// wasm-engine callback-dispatch wire acknowledges, a per-contrato
    /// per-`:wit`-world call-callback derivation the future adaptive
    /// dispatch engine computes from the sibling `:contratos` M3
    /// mesh-slot edges — would have had to be threaded through the
    /// open-coded field-access in `declared_slots` (the tag surface
    /// every per-slot diagnostic reads) or the `declared_slots`
    /// iterator would silently disagree on which callback a given
    /// [`BehaviorSpec`] resolves to. Lifting the resolution to a typed
    /// method on the substrate primitive means every downstream
    /// consumer of the Servico's per-`:behavior` synchronous-call
    /// callback surface reaches for exactly one typed dispatch — the
    /// resolver's accept-set migrates as a unit on any future axis
    /// addition.
    ///
    /// Third `Option<&Path>`-return accessor on the M2 `:behavior` slot
    /// family (sibling of the prior [`BehaviorSpec::on_state_change`]
    /// 9b4ecde and [`BehaviorSpec::on_init`] d66c702 `Option<&Path>`
    /// accessors on the peer per-`:behavior` `:on-state-change` /
    /// `:on-init` axes — same "one typed dispatch on the substrate
    /// primitive, thin projections at each consumer" discipline extended
    /// onto the peer per-`:behavior` `Option<PathBuf>` optional-scalar
    /// axis; continues the "optional per-slot `Option<&Path>` scalar"
    /// projection pattern the sibling per-`:behavior` `:on-cast` /
    /// `:on-info` / `:on-terminate` future lifts fold on). Named
    /// `on_call()` to match the storage field's name; the accessor's
    /// identity name maps onto the canonical
    /// `theory/INSPIRATIONS.md` §II.3 vocabulary the slot's docstring
    /// already carries.
    #[must_use]
    pub fn on_call(&self) -> Option<&Path> {
        self.on_call.as_deref()
    }

    /// Substrate-canonical per-`:behavior` `:on-cast`
    /// OTP-`gen_server:handle_cast/2`-shaped asynchronous
    /// fire-and-forget callback-path scalar accessor every consumer of
    /// the Servico's asynchronous-dispatch callback path keys off —
    /// returns the author-declared `:behavior :on-cast` typed callback
    /// path verbatim as an `Option<&Path>`, borrowed from the typed
    /// slot's own `Option<PathBuf>` storage. `None` when the slot is
    /// absent (the canonical "no asynchronous-cast callback declared —
    /// the runtime falls back to the wasm-engine's default `Accepted:
    /// 202` fire-and-forget response shape that surfaces the request to
    /// the underlying HTTP proxy world verbatim without any
    /// author-supplied post-accept-side-effect interposed" arm the M2.5
    /// wasm-engine callback-dispatch wire consults at every asynchronous
    /// incoming call; peer of the sibling [`BehaviorSpec::on_init`] /
    /// [`BehaviorSpec::on_call`] / [`BehaviorSpec::on_state_change`]
    /// `None`-arm's "no instance-start / synchronous-call /
    /// state-migration callback" semantic on the sibling axes).
    ///
    /// The `:behavior :on-cast` slot carries the OTP
    /// `gen_server:handle_cast/2` callback contract (the module-level
    /// [`BehaviorSpec::on_cast`] docstring pins the analog verbatim:
    /// "Asynchronous fire-and-forget handler. Analog of
    /// `gen_server:handle_cast/2` — caller does not wait. For HTTP
    /// servicos this maps onto `Accepted: 202` shapes."). Its position
    /// in the OTP dispatch triad is the fire-and-forget half sibling to
    /// the synchronous request/response `:on-call` half: the runtime
    /// routes every asynchronous incoming message (every
    /// `wasi:http/incoming-handler` invocation whose caller does not
    /// await a reply and whose runtime response the wasm-engine
    /// short-circuits into an `Accepted: 202` shape at accept time,
    /// every asynchronous WIT-typed peer edge whose contract carries no
    /// reply payload, every NATS `nats:pub-sub` subscriber the future
    /// M3 mesh-slot NATS bridge dispatches through the `:on-cast`
    /// callback the way the sibling `wasi:http/proxy` HTTP bridge
    /// dispatches through `:on-call`) through the callback; the callback
    /// runs to completion on the actor's own mailbox turn without any
    /// reply-shape awaiting caller, and the runtime returns to the
    /// mailbox-drain state as soon as the callback returns
    /// (`theory/INSPIRATIONS.md` §II.3 — OTP `gen_server` behavior's
    /// six-callback lifecycle, translated onto pleme-io's typed
    /// `:behavior` slot family; `theory/CAIXA-SDLC.md` §I — the
    /// author-surface pins `:on-cast` as the third arm of the
    /// `:behavior` overlay every Servico may declare, sibling to
    /// `:on-call` on the peer synchronous-dispatch axis and `:on-info`
    /// on the peer out-of-band-dispatch axis; `theory/RUNTIME-PATTERNS.md`
    /// §II — the asynchronous-fire-and-forget pattern the runtime
    /// realizes through this callback).
    ///
    /// Prior to this lift the `.on_cast` field was accessed inline at
    /// one production site — [`BehaviorSpec::declared_slots`]'s
    /// `:on-cast` arm's `self.on_cast.as_ref()` map into the six-tuple
    /// iterator that both the layout checker (existence sweep at
    /// `layout.rs:900`) and the sibling `BehaviorSpec::validate`
    /// value-shape gate consume — an open-coded field-access that
    /// expressed no compile-time link back to the typed slot. A future
    /// extension of the `:behavior :on-cast` axis to a richer author
    /// surface — a per-tenant cast-callback override the M4 CR
    /// materializer resolves per-CR, a per-cluster asynchronous-dispatch
    /// callback overlay the `theory/ABSORPTION-ROADMAP.md` M2.5
    /// wasm-engine callback-dispatch wire acknowledges, a
    /// per-`nats:pub-sub`-subject cast-callback derivation the future
    /// M3 mesh-slot NATS bridge computes from the sibling `:contratos`
    /// M3 mesh-slot edges' `:subject` axis — would have had to be
    /// threaded through the open-coded field-access in `declared_slots`
    /// (the tag surface every per-slot diagnostic reads) or the
    /// `declared_slots` iterator would silently disagree on which
    /// callback a given [`BehaviorSpec`] resolves to. Lifting the
    /// resolution to a typed method on the substrate primitive means
    /// every downstream consumer of the Servico's per-`:behavior`
    /// asynchronous-cast callback surface reaches for exactly one typed
    /// dispatch — the resolver's accept-set migrates as a unit on any
    /// future axis addition.
    ///
    /// Fourth `Option<&Path>`-return accessor on the M2 `:behavior` slot
    /// family (sibling of the prior [`BehaviorSpec::on_state_change`]
    /// 9b4ecde, [`BehaviorSpec::on_init`] d66c702, and
    /// [`BehaviorSpec::on_call`] 156ddbe `Option<&Path>` accessors on
    /// the peer per-`:behavior` `:on-state-change` / `:on-init` /
    /// `:on-call` axes — same "one typed dispatch on the substrate
    /// primitive, thin projections at each consumer" discipline extended
    /// onto the peer per-`:behavior` `Option<PathBuf>` optional-scalar
    /// axis; continues the "optional per-slot `Option<&Path>` scalar"
    /// projection pattern the sibling per-`:behavior` `:on-info` /
    /// `:on-terminate` future lifts fold on). Named `on_cast()` to match
    /// the storage field's name; the accessor's identity name maps onto
    /// the canonical `theory/INSPIRATIONS.md` §II.3 vocabulary the
    /// slot's docstring already carries.
    #[must_use]
    pub fn on_cast(&self) -> Option<&Path> {
        self.on_cast.as_deref()
    }

    /// Substrate-canonical per-`:behavior` `:on-info`
    /// OTP-`gen_server:handle_info/2`-shaped system / out-of-band
    /// message-handler callback-path scalar accessor every consumer of
    /// the Servico's out-of-band-dispatch callback path keys off —
    /// returns the author-declared `:behavior :on-info` typed callback
    /// path verbatim as an `Option<&Path>`, borrowed from the typed
    /// slot's own `Option<PathBuf>` storage. `None` when the slot is
    /// absent (the canonical "no out-of-band-info callback declared —
    /// the runtime silently drops every non-`:on-call` / non-`:on-cast`
    /// mailbox message the wasm-engine's `gen_server`-shaped dispatcher
    /// classifies as system / out-of-band (timeouts, downstream
    /// `nodedown`, monitor `DOWN` signals, scheduler ticks, wasm-engine
    /// `wasi:clocks` timer fires, adaptive-dispatch backpressure
    /// notifications the M2.5 wasm-engine callback-dispatch wire emits
    /// on peer-Servico circuit-open transitions) without any
    /// author-supplied side-effect interposed" arm the M2.5 wasm-engine
    /// callback-dispatch wire consults at every out-of-band mailbox
    /// turn; peer of the sibling [`BehaviorSpec::on_init`] /
    /// [`BehaviorSpec::on_call`] / [`BehaviorSpec::on_cast`] /
    /// [`BehaviorSpec::on_state_change`] `None`-arm's "no
    /// instance-start / synchronous-call / asynchronous-cast /
    /// state-migration callback" semantic on the sibling axes).
    ///
    /// The `:behavior :on-info` slot carries the OTP
    /// `gen_server:handle_info/2` callback contract (the module-level
    /// [`BehaviorSpec::on_info`] docstring pins the analog verbatim:
    /// "System / out-of-band message handler. Analog of
    /// `gen_server:handle_info/2` — timeouts, downstream `nodedown`,
    /// monitor signals, scheduler ticks."). Its position in the OTP
    /// dispatch triad is the out-of-band half sibling to the
    /// synchronous request/response `:on-call` and asynchronous
    /// fire-and-forget `:on-cast` halves: the runtime routes every
    /// mailbox message the `gen_server`-shaped dispatcher classifies as
    /// neither a `:on-call` synchronous request (no reply-awaiting
    /// caller) nor a `:on-cast` asynchronous WIT-typed edge (no peer
    /// Servico originated the message via a declared `:contratos`
    /// entry) through the callback; the callback runs to completion on
    /// the actor's own mailbox turn with no reply-shape awaiting caller
    /// and no peer-Servico dispatch semantics, and the runtime returns
    /// to the mailbox-drain state as soon as the callback returns
    /// (`theory/INSPIRATIONS.md` §II.3 — OTP `gen_server` behavior's
    /// six-callback lifecycle, translated onto pleme-io's typed
    /// `:behavior` slot family; `theory/CAIXA-SDLC.md` §I — the
    /// author-surface pins `:on-info` as the fourth arm of the
    /// `:behavior` overlay every Servico may declare, sibling to
    /// `:on-cast` on the peer asynchronous-dispatch axis and
    /// `:on-terminate` on the peer lifecycle-tail axis;
    /// `theory/RUNTIME-PATTERNS.md` §II — the out-of-band-info pattern
    /// the runtime realizes through this callback).
    ///
    /// Prior to this lift the `.on_info` field was accessed inline at
    /// one production site — [`BehaviorSpec::declared_slots`]'s
    /// `:on-info` arm's `self.on_info.as_ref()` map into the six-tuple
    /// iterator that both the layout checker (existence sweep at
    /// `layout.rs:900`) and the sibling `BehaviorSpec::validate`
    /// value-shape gate consume — an open-coded field-access that
    /// expressed no compile-time link back to the typed slot. A future
    /// extension of the `:behavior :on-info` axis to a richer author
    /// surface — a per-tenant info-callback override the M4 CR
    /// materializer resolves per-CR, a per-cluster
    /// out-of-band-dispatch callback overlay the
    /// `theory/ABSORPTION-ROADMAP.md` M2.5 wasm-engine
    /// callback-dispatch wire acknowledges, a per-monitor-signal
    /// callback derivation the future adaptive-dispatch engine
    /// computes from the sibling `:politicas :circuit-breaker` axis
    /// (routing peer-Servico circuit-open notifications through the
    /// info-callback the way Erlang routes `DOWN` messages through
    /// `handle_info/2`) — would have had to be threaded through the
    /// open-coded field-access in `declared_slots` (the tag surface
    /// every per-slot diagnostic reads) or the `declared_slots`
    /// iterator would silently disagree on which callback a given
    /// [`BehaviorSpec`] resolves to. Lifting the resolution to a typed
    /// method on the substrate primitive means every downstream
    /// consumer of the Servico's per-`:behavior` out-of-band-info
    /// callback surface reaches for exactly one typed dispatch — the
    /// resolver's accept-set migrates as a unit on any future axis
    /// addition.
    ///
    /// Fifth `Option<&Path>`-return accessor on the M2 `:behavior` slot
    /// family (sibling of the prior [`BehaviorSpec::on_state_change`]
    /// 9b4ecde, [`BehaviorSpec::on_init`] d66c702,
    /// [`BehaviorSpec::on_call`] 156ddbe, and [`BehaviorSpec::on_cast`]
    /// 99616ac `Option<&Path>` accessors on the peer per-`:behavior`
    /// `:on-state-change` / `:on-init` / `:on-call` / `:on-cast` axes —
    /// same "one typed dispatch on the substrate primitive, thin
    /// projections at each consumer" discipline extended onto the peer
    /// per-`:behavior` `Option<PathBuf>` optional-scalar axis;
    /// continues the "optional per-slot `Option<&Path>` scalar"
    /// projection pattern the last-remaining sibling per-`:behavior`
    /// `:on-terminate` future lift folds on). Named `on_info()` to
    /// match the storage field's name; the accessor's identity name
    /// maps onto the canonical `theory/INSPIRATIONS.md` §II.3
    /// vocabulary the slot's docstring already carries.
    #[must_use]
    pub fn on_info(&self) -> Option<&Path> {
        self.on_info.as_deref()
    }

    /// Substrate-canonical per-`:behavior` `:on-terminate`
    /// OTP-`gen_server:terminate/2`-shaped graceful-shutdown cleanup
    /// callback-path scalar accessor every consumer of the Servico's
    /// lifecycle-tail dispatch keys off — returns the author-declared
    /// `:behavior :on-terminate` typed callback path verbatim as an
    /// `Option<&Path>`, borrowed from the typed slot's own
    /// `Option<PathBuf>` storage. `None` when the slot is absent (the
    /// canonical "no terminate callback declared — the runtime tears
    /// down the wasm instance without dispatching any author-supplied
    /// cleanup side-effect, the Lunatic-per-process sandbox reclaims
    /// every wasm32 linear-memory page + fuel budget the sibling
    /// `:limits` axes accept-set caps, and every outstanding
    /// `wasi:http/incoming-handler` / `wasi:keyvalue/store` / NATS
    /// `nats:pub-sub` open handle the WIT-component-model closes the
    /// wasm process's export-side at process-tear-down time is dropped
    /// on the floor without any author-visible flush" arm the M2.5
    /// wasm-engine callback-dispatch wire consults at every graceful
    /// tear-down turn; peer of the sibling [`BehaviorSpec::on_init`] /
    /// [`BehaviorSpec::on_call`] / [`BehaviorSpec::on_cast`] /
    /// [`BehaviorSpec::on_info`] / [`BehaviorSpec::on_state_change`]
    /// `None`-arm's "no instance-start / synchronous-call /
    /// asynchronous-cast / out-of-band-info / state-migration
    /// callback" semantic on the sibling axes).
    ///
    /// The `:behavior :on-terminate` slot carries the OTP
    /// `gen_server:terminate/2` callback contract (the module-level
    /// [`BehaviorSpec::on_terminate`] docstring pins the analog verbatim:
    /// "Cleanup callback before the instance shuts down. Analog of
    /// `gen_server:terminate/2`. Best-effort — runs only when the
    /// instance terminates gracefully (not on hard kill)."). Its position
    /// in the OTP lifecycle is the lifecycle-tail complement of the
    /// `:on-init` head — the runtime instantiates the wasm process,
    /// dispatches `:on-init`, dispatches every `:on-call` / `:on-cast`
    /// / `:on-info` mailbox turn the instance accepts across its
    /// lifetime, and only at graceful tear-down time (a supervisor's
    /// `RestForOne` restart pass, a rolling wasm-engine hot-upgrade the
    /// sibling `:upgrade-from` axis's appup instructions describe, an
    /// operator-driven Aplicacao teardown the M4 CR materializer emits
    /// as a Kubernetes deletion event, a per-tenant per-`:placement`
    /// evict the future adaptive-placement engine computes on
    /// per-cluster capacity pressure) dispatches the terminate callback
    /// (`theory/INSPIRATIONS.md` §II.3 — OTP `gen_server` behavior's
    /// six-callback lifecycle, translated onto pleme-io's typed
    /// `:behavior` slot family; `theory/CAIXA-SDLC.md` §I — the
    /// author-surface pins `:on-terminate` as the sixth and final arm
    /// of the `:behavior` overlay every Servico may declare, sibling to
    /// `:on-init` on the peer lifecycle-head axis and `:on-state-change`
    /// on the peer hot-upgrade-composition axis;
    /// `theory/RUNTIME-PATTERNS.md` §II — the graceful-tear-down cleanup
    /// pattern the runtime realizes through this callback). The
    /// callback runs to completion on the actor's own mailbox turn
    /// before the runtime returns the wasm process's resources to the
    /// wasm-engine pool; the Lunatic-per-process sandbox guarantees the
    /// callback cannot exceed the sibling `:limits :wall-clock` axis's
    /// per-call cap, so a runaway cleanup path cannot wedge the
    /// tear-down (the caller receives the same
    /// `LimitsError::WallClockExceeded`-shaped runtime diagnostic the
    /// sibling `:on-*` dispatch arms surface on the peer cap-exceed
    /// path). The "best-effort — runs only when the instance terminates
    /// gracefully (not on hard kill)" clause of the module-level
    /// docstring is the OTP `terminate/2` clause verbatim: the runtime
    /// dispatches the callback on every controlled tear-down but never
    /// on `EXIT`-kill / `SIGKILL` / wasm-engine OOM eviction / fuel
    /// starvation cap-exceed.
    ///
    /// Prior to this lift the `.on_terminate` field was accessed inline
    /// at one production site — [`BehaviorSpec::declared_slots`]'s
    /// `:on-terminate` arm's `self.on_terminate.as_ref()` map into the
    /// six-tuple iterator that both the layout checker (existence sweep
    /// at `layout.rs:900`) and the sibling `BehaviorSpec::validate`
    /// value-shape gate consume — an open-coded field-access that
    /// expressed no compile-time link back to the typed slot. A future
    /// extension of the `:behavior :on-terminate` axis to a richer
    /// author surface — a per-tenant terminate-callback override the
    /// M4 CR materializer resolves per-CR, a per-cluster
    /// graceful-tear-down callback overlay the
    /// `theory/ABSORPTION-ROADMAP.md` M2.5 wasm-engine
    /// callback-dispatch wire acknowledges, a per-supervisor
    /// terminate-callback derivation the future adaptive-supervision
    /// engine computes from the sibling `:estrategia` restart-strategy
    /// axis (routing a `RestForOne` cascade's per-child terminate
    /// through the callback the way Erlang routes `terminate/2` before
    /// each `restart_child/2` retry), a per-`:upgrade-from` version
    /// migration terminate-callback the future rolling hot-upgrade
    /// engine computes from the sibling `:upgrade-from` instruction
    /// chain (dispatching the terminate callback with the outgoing
    /// version's state before the sibling `:on-state-change` callback
    /// folds it into the incoming version's shape) — would have had to
    /// be threaded through the open-coded field-access in
    /// `declared_slots` (the tag surface every per-slot diagnostic
    /// reads) or the `declared_slots` iterator would silently disagree
    /// on which callback a given [`BehaviorSpec`] resolves to. Lifting
    /// the resolution to a typed method on the substrate primitive
    /// means every downstream consumer of the Servico's per-`:behavior`
    /// graceful-tear-down callback surface reaches for exactly one
    /// typed dispatch — the resolver's accept-set migrates as a unit on
    /// any future axis addition.
    ///
    /// Sixth and final `Option<&Path>`-return accessor on the M2
    /// `:behavior` slot family (sibling of the prior
    /// [`BehaviorSpec::on_state_change`] 9b4ecde,
    /// [`BehaviorSpec::on_init`] d66c702, [`BehaviorSpec::on_call`]
    /// 156ddbe, [`BehaviorSpec::on_cast`] 99616ac, and
    /// [`BehaviorSpec::on_info`] 4846cef `Option<&Path>` accessors on
    /// the peer per-`:behavior` `:on-state-change` / `:on-init` /
    /// `:on-call` / `:on-cast` / `:on-info` axes — same "one typed
    /// dispatch on the substrate primitive, thin projections at each
    /// consumer" discipline extended onto the peer per-`:behavior`
    /// `Option<PathBuf>` optional-scalar axis; closes the last
    /// unlifted per-`:behavior` `Option<&Path>` scalar-value axis, so
    /// every arm of the six-callback OTP `gen_server` lifecycle the
    /// slot family models now routes through one typed dispatch on the
    /// substrate primitive). Named `on_terminate()` to match the
    /// storage field's name; the accessor's identity name maps onto the
    /// canonical `theory/INSPIRATIONS.md` §II.3 vocabulary the slot's
    /// docstring already carries.
    #[must_use]
    pub fn on_terminate(&self) -> Option<&Path> {
        self.on_terminate.as_deref()
    }

    /// Reject operationally-meaningless callback path values on every
    /// declared slot. Each slot remains optional — omitting a field
    /// expresses "fall back to the runtime default callback"; the bug
    /// being closed is *carrying* a foot-shaped path value, which the
    /// layout checker's `root.join(p)` would either silently treat as
    /// the project root (`PathBuf::new()`), escape the project root
    /// (absolute path replaces `root` per `Path::join` semantics), or
    /// traverse out of the root via `..` components.
    ///
    /// Four invariants per slot, evaluated in declaration order
    /// (`:on-init` → `:on-call` → `:on-cast` → `:on-info` →
    /// `:on-state-change` → `:on-terminate`) so the diagnostic for
    /// multi-malformed manifests is deterministic:
    ///
    ///   - non-empty path string,
    ///   - relative path (Lunatic-style sandbox: callbacks live under
    ///     the caixa root, never in `/etc/...`),
    ///   - no `..` components (relative paths must not escape the
    ///     caixa root via parent-directory traversal),
    ///   - terminating `.lisp` extension (the wasm-engine reads every
    ///     callback path as tatara-lisp source — a `.txt` / `.rs` /
    ///     `.lisp.bak` / no-extension shape is structurally a parser
    ///     error at instance-start time, far from the source
    ///     caixa.lisp).
    ///
    /// Mirrors the discipline applied to `:limits` axes
    /// (`LimitsSpec::validate`) and to the M3 mesh `:entrada :paths`
    /// invariants (`AplicacaoSpec::validate`) — every typed value
    /// carried by a slot is either absent or value-shape valid.
    pub fn validate(&self) -> Result<(), BehaviorError> {
        for (slot, path) in self.declared_slots() {
            validate_callback_path(slot, path)?;
        }
        Ok(())
    }
}

fn validate_callback_path(slot: &'static str, path: &Path) -> Result<(), BehaviorError> {
    // Delegate the three structural checks (non-empty / relative /
    // no-parent-escape) to the lifted [`crate::render::is_sandboxed_relative_path`]
    // predicate — same Empty → Absolute → ParentEscape arm-ordering
    // this function previously inlined verbatim, now shared with
    // [`crate::UpgradeInstruction::StateChange`]'s `:script` arm so
    // every author-supplied path on every M2 typed slot consults one
    // gate, not two-and-counting verbatim copies. Each arm wraps the
    // tag in the same `*Path` / `*Escape` variant the original inline
    // code raised, so the diagnostic shape every caller depends on
    // (the per-slot diagnostic naming `:behavior :on-init`, etc.) is
    // preserved by construction.
    match crate::render::is_sandboxed_relative_path(path) {
        Ok(()) => {}
        Err(crate::render::PathShapeViolation::Empty) => {
            return Err(BehaviorError::EmptyPath { slot });
        }
        Err(crate::render::PathShapeViolation::Absolute) => {
            return Err(BehaviorError::AbsolutePath {
                slot,
                path: path.to_path_buf(),
            });
        }
        Err(crate::render::PathShapeViolation::ParentEscape) => {
            return Err(BehaviorError::ParentEscape {
                slot,
                path: path.to_path_buf(),
            });
        }
    }
    // File-type contract on the `:behavior :on-*` axis: every typed
    // callback path is consumed by the M2.5 wasm-engine instantiator
    // (the `ABSORPTION-ROADMAP` names the runtime substrate) as a
    // tatara-lisp source file the engine reads through
    // `tatara_lisp::read` at instance-start time. The author surface
    // already documents the `.lisp` extension as the canonical shape
    // (the in-module example and every in-tree test fixture use
    // `lib/<name>.lisp`), and the layout's `:kind Biblioteca` default
    // (`lib/<nome>.lisp` at `layout.rs:793`) plus the `:bibliotecas` /
    // `:exe` directory invariants all assume the same `.lisp` extension
    // verbatim; until this gate landed the typed slot accepted any
    // path that passed the structural-shape checks (empty / absolute /
    // parent-escape), so a programmatic struct literal
    // (`BehaviorSpec { on_init: Some(PathBuf::from("lib/init.txt")), .. }`
    // or `Some(PathBuf::from("lib/init.rs"))` or the equivalent
    // author-surface `(:behavior (:on-init "lib/init.lisp.bak"))` /
    // `(:on-init "lib/init")` / any "I dragged the wrong file from the
    // workspace tree" typo landing in the slot) round-tripped cleanly
    // through serde and the per-callback CSE invariant (no value the
    // wasm-engine can't honor as tatara-lisp source) was a runtime,
    // not build-time, contract. The wasm-engine's `tatara_lisp::read`
    // failed at instance-start time with a parser-shaped diagnostic
    // far from the source caixa.lisp, with no field naming the
    // offending `:on-*` slot — the canonical "declared-but-unloadable
    // callback" footgun the sibling `:behavior` path-shape arms close
    // on the peer "sandbox-escaping" / "empty" / "parent-escape"
    // shapes, and the peer `LimitsError::MemoryNotPageMultiple`
    // (ec266d8) closes on the `:limits :memory` axis's "engine-
    // quantization-granularity-mismatch" shape: both are "the consumer
    // substrate's accepted set is narrower than the structural-shape
    // gate alone" lifts, surfacing the engine's load-bearing
    // type-contract at validate time rather than at apply time.
    //
    // Strict `.lisp` (lowercase, case-sensitive): the byte-size and
    // duration codecs are case-sensitive on unit suffixes (`MiB`,
    // `ms`, `s`, `m`, `h`) and every layout invariant compares
    // extensions via `PathBuf` equality (case-sensitive on every
    // filesystem the substrate targets — Linux/macOS volume defaults,
    // every container image filesystem), so a strict `.lisp` shape
    // matches the downstream accepted set without case-folding drift.
    // An uppercase `.LISP` shape that the layout's existence check
    // would (case-insensitively, on case-insensitive volumes) match
    // the on-disk file would still mismatch the canonical-form codec
    // emits, breaking the THEORY.md §V.2.7 render-determinism
    // contract every typed slot carries. Same canonical-form
    // discipline `is_canonical_rate_limit_window` (808017c) applies
    // to the rate-limit window axis and every shape-gate predicate
    // in `render.rs` (case-sensitive on unit / scheme / label
    // boundaries).
    //
    // The path-shape arms strictly precede this extension arm so a
    // path that is *both* sandbox-escaping and non-`.lisp` surfaces
    // the more fundamental sandbox-shape diagnostic first (the
    // `.lisp` remediation would be misleading when the offending
    // path can never be loaded under the caixa root anyway — the
    // canonical fix collapses both into "pin a relative `.lisp` path
    // under the caixa root"). Same posture every peer
    // zero-then-shape-then-cap chain uses on this surface
    // (`MemoryZero` → `MemoryBelowWasm32Page` →
    // `MemoryExceedsWasm32Cap` → `MemoryNotPageMultiple`, the
    // smallest-scope arm fires last).
    if !crate::render::is_lisp_extension(path) {
        return Err(BehaviorError::NonLispExtension {
            slot,
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BehaviorError {
    #[error(
        ":behavior {slot} path is empty (omit the slot to fall back to the runtime default \
         callback; do not declare an empty path)"
    )]
    EmptyPath { slot: &'static str },
    #[error(
        ":behavior {slot} path {} is absolute — callbacks must be relative to the caixa root, \
         since the layout checker's `root.join(p)` would otherwise escape the project sandbox \
         (Path::join replaces the base with an absolute right-hand side)",
        path.display()
    )]
    AbsolutePath { slot: &'static str, path: PathBuf },
    #[error(
        ":behavior {slot} path {} contains a `..` component — callbacks must not traverse \
         above the caixa root",
        path.display()
    )]
    ParentEscape { slot: &'static str, path: PathBuf },
    #[error(
        ":behavior {slot} path {} does not terminate in the `.lisp` extension — the M2.5 \
         wasm-engine instantiator reads every callback path as tatara-lisp source through \
         `tatara_lisp::read` at instance-start time, so any other extension (`.txt`, `.rs`, \
         `.lisp.bak`) or no-extension shape is structurally a parser error far from the \
         source caixa.lisp, with no field naming the offending `:on-*` slot. Pin a \
         relative path under the caixa root whose terminating extension is \
         lowercase-`.lisp` (e.g. `\"lib/init.lisp\"`, `\"lib/handlers.lisp\"`, \
         `\"lib/migrations/v01-to-v02.lisp\"`) or omit the slot to fall back to the \
         runtime default callback",
        path.display()
    )]
    NonLispExtension { slot: &'static str, path: PathBuf },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{
        M2_BEHAVIOR_AUTHOR_KEY_ON_CALL, M2_BEHAVIOR_AUTHOR_KEY_ON_CAST,
        M2_BEHAVIOR_AUTHOR_KEY_ON_INFO, M2_BEHAVIOR_AUTHOR_KEY_ON_INIT,
        M2_BEHAVIOR_AUTHOR_KEY_ON_STATE_CHANGE, M2_BEHAVIOR_AUTHOR_KEY_ON_TERMINATE,
    };

    #[test]
    fn empty_behavior_round_trip() {
        let b = BehaviorSpec::default();
        assert!(b.is_empty());
        let json = serde_json::to_string(&b).unwrap();
        assert_eq!(json, "{}");
        let back: BehaviorSpec = serde_json::from_str("{}").unwrap();
        assert_eq!(back, b);
    }

    #[test]
    fn full_behavior_round_trip_through_json() {
        let b = BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            on_call: Some(PathBuf::from("lib/handlers.lisp")),
            on_cast: Some(PathBuf::from("lib/handlers.lisp")),
            on_info: Some(PathBuf::from("lib/handlers.lisp")),
            on_state_change: Some(PathBuf::from("lib/migrations.lisp")),
            on_terminate: Some(PathBuf::from("lib/cleanup.lisp")),
        };
        let json = serde_json::to_string(&b).unwrap();
        let back: BehaviorSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(b, back);
    }

    #[test]
    fn partial_behavior_keeps_explicit_fields() {
        let b = BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            on_call: Some(PathBuf::from("lib/handlers.lisp")),
            ..Default::default()
        };
        assert!(!b.is_empty());
        let paths: Vec<PathBuf> = b.declared_paths().map(Path::to_path_buf).collect();
        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&PathBuf::from("lib/init.lisp")));
        assert!(paths.contains(&PathBuf::from("lib/handlers.lisp")));
    }

    #[test]
    fn declared_paths_skips_none() {
        let b = BehaviorSpec {
            on_init: Some(PathBuf::from("a.lisp")),
            on_terminate: Some(PathBuf::from("b.lisp")),
            ..Default::default()
        };
        let paths: Vec<PathBuf> = b.declared_paths().map(Path::to_path_buf).collect();
        assert_eq!(
            paths,
            vec![PathBuf::from("a.lisp"), PathBuf::from("b.lisp")]
        );
    }

    #[test]
    fn json_keys_are_camelcase() {
        let b = BehaviorSpec {
            on_init: Some(PathBuf::from("init.lisp")),
            on_state_change: Some(PathBuf::from("mig.lisp")),
            ..Default::default()
        };
        let json = serde_json::to_string(&b).unwrap();
        assert!(json.contains("\"onInit\""));
        assert!(json.contains("\"onStateChange\""));
        assert!(!json.contains("\"on_init\""));
    }

    #[test]
    fn deserialize_accepts_camelcase() {
        let json = r#"{"onInit":"a.lisp","onTerminate":"b.lisp"}"#;
        let b: BehaviorSpec = serde_json::from_str(json).unwrap();
        assert_eq!(b.on_init, Some(PathBuf::from("a.lisp")));
        assert_eq!(b.on_terminate, Some(PathBuf::from("b.lisp")));
    }

    // ── drift-detection: serde-derive-to-M2_BEHAVIOR_KEY_ON_* identity ────

    #[test]
    fn behavior_spec_serde_keys_match_lifted_m2_behavior_key_consts() {
        // Load-bearing invariant: the six `M2_BEHAVIOR_KEY_ON_*` consts
        // (`M2_BEHAVIOR_KEY_ON_INIT` / `M2_BEHAVIOR_KEY_ON_CALL` /
        // `M2_BEHAVIOR_KEY_ON_CAST` / `M2_BEHAVIOR_KEY_ON_INFO` /
        // `M2_BEHAVIOR_KEY_ON_STATE_CHANGE` /
        // `M2_BEHAVIOR_KEY_ON_TERMINATE`) name the exact camelCase JSON
        // keys the `#[serde(rename_all = "camelCase")]` attribute on
        // `BehaviorSpec` emits, and every test-side probe across the
        // caixa-core / caixa-flux / caixa-helm renderer test fixtures
        // navigates into the rendered `:behavior` overlay sub-block by
        // consulting one of these six `&'static str`s. Serialize a
        // fully-populated BehaviorSpec and pin that each canonical
        // byte-sequence appears verbatim in the JSON — a future
        // accidental `rename_all = "snake_case"` / `"kebab-case"` /
        // verbatim-field-name flip at the derive attribute (any of
        // which would silently break every test-side probe that reaches
        // for one of the six consts) surfaces here as a build-time test
        // failure at `behavior.rs`, not as an apply-time
        // `.get(<stale-canonical-const>)` returning `None` far from the
        // derive-attr drift's commit. Same discipline the sibling
        // `limits_spec_serde_keys_match_lifted_m2_limits_key_consts`
        // pin (d8b8b4f) established on the peer `:limits` sub-slot
        // axis: one canonical byte-string per typed sub-key axis,
        // pinned to the load-bearing serde derivation at the type
        // itself.
        let b = BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            on_call: Some(PathBuf::from("lib/handlers.lisp")),
            on_cast: Some(PathBuf::from("lib/handlers.lisp")),
            on_info: Some(PathBuf::from("lib/handlers.lisp")),
            on_state_change: Some(PathBuf::from("lib/migrations.lisp")),
            on_terminate: Some(PathBuf::from("lib/cleanup.lisp")),
        };
        let json = serde_json::to_string(&b).unwrap();
        for key in [
            crate::render::M2_BEHAVIOR_KEY_ON_INIT,
            crate::render::M2_BEHAVIOR_KEY_ON_CALL,
            crate::render::M2_BEHAVIOR_KEY_ON_CAST,
            crate::render::M2_BEHAVIOR_KEY_ON_INFO,
            crate::render::M2_BEHAVIOR_KEY_ON_STATE_CHANGE,
            crate::render::M2_BEHAVIOR_KEY_ON_TERMINATE,
        ] {
            let quoted = format!("\"{key}\"");
            assert!(
                json.contains(&quoted),
                "serialized BehaviorSpec must carry the lifted \
                 M2_BEHAVIOR_KEY_ON_* byte-sequence {quoted} verbatim \
                 in the JSON emission (got: {json})",
            );
        }
    }

    #[test]
    fn m2_behavior_key_consts_are_pairwise_distinct() {
        // Cross-axis drift-detection pin: a future collapse of two
        // canonical sub-key byte-strings onto the same value (e.g. an
        // accidental copy-paste flip of `M2_BEHAVIOR_KEY_ON_CAST` to
        // also read `"onCall"`) would silently reroute every test-side
        // probe on one axis onto the sibling axis's overlay entry and
        // pass every propagation-probe test that expected only the
        // stale axis's value. Peer of `m2_limits_key_consts_are_
        // pairwise_distinct` (d8b8b4f) on the sibling `:limits`
        // sub-slot axis.
        let all = [
            crate::render::M2_BEHAVIOR_KEY_ON_INIT,
            crate::render::M2_BEHAVIOR_KEY_ON_CALL,
            crate::render::M2_BEHAVIOR_KEY_ON_CAST,
            crate::render::M2_BEHAVIOR_KEY_ON_INFO,
            crate::render::M2_BEHAVIOR_KEY_ON_STATE_CHANGE,
            crate::render::M2_BEHAVIOR_KEY_ON_TERMINATE,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(
                    a, b,
                    "M2_BEHAVIOR_KEY_ON_* consts must be pairwise-distinct \
                     canonical byte-sequences — got `{a}` == `{b}`",
                );
            }
        }
    }

    #[test]
    fn m2_behavior_key_consts_are_lower_camel_case_shape() {
        // Shape-pin: every `M2_BEHAVIOR_KEY_ON_*` const must be a
        // lowerCamelCase byte-sequence (no `snake_case` underscores,
        // no `kebab-case` hyphens, no `PascalCase` leading capital, no
        // whitespace / colons / dots) — the canonical shape the
        // `#[serde(rename_all = "camelCase")]` derive produces on
        // `BehaviorSpec`. A future flip to a non-camelCase attribute
        // at the derive surfaces both here (this test fails on the
        // stale-constant shape) and at
        // `behavior_spec_serde_keys_match_lifted_m2_behavior_key_consts`
        // (that test fails on the mismatch between const and derive).
        // Peer of `m2_limits_key_consts_are_lower_camel_case_shape`
        // (d8b8b4f) on the sibling `:limits` sub-slot axis.
        for key in [
            crate::render::M2_BEHAVIOR_KEY_ON_INIT,
            crate::render::M2_BEHAVIOR_KEY_ON_CALL,
            crate::render::M2_BEHAVIOR_KEY_ON_CAST,
            crate::render::M2_BEHAVIOR_KEY_ON_INFO,
            crate::render::M2_BEHAVIOR_KEY_ON_STATE_CHANGE,
            crate::render::M2_BEHAVIOR_KEY_ON_TERMINATE,
        ] {
            assert!(
                !key.is_empty(),
                "M2_BEHAVIOR_KEY_ON_* must be non-empty (got {key:?})"
            );
            let first = key.chars().next().unwrap();
            assert!(
                first.is_ascii_lowercase(),
                "M2_BEHAVIOR_KEY_ON_* must lead with an ASCII-lowercase \
                 byte (got {key:?}, leads with {first:?})",
            );
            assert!(
                key.chars().all(|c| c.is_ascii_alphanumeric()),
                "M2_BEHAVIOR_KEY_ON_* must be ASCII-alphanumeric only \
                 — no `_` / `-` / `:` / `.` / whitespace (got {key:?})",
            );
        }
    }

    #[test]
    fn deserialize_omits_unknown_fields_via_default() {
        // Forward-compatible: a future caixa.lisp with extra fields
        // round-trips without losing the known ones.
        let json = r#"{"onInit":"a.lisp"}"#;
        let b: BehaviorSpec = serde_json::from_str(json).unwrap();
        assert_eq!(b.on_init, Some(PathBuf::from("a.lisp")));
        assert!(b.on_call.is_none());
    }

    // ── value-shape invariants on declared callback paths ──────────

    #[test]
    fn validate_default_is_ok() {
        BehaviorSpec::default().validate().unwrap();
    }

    #[test]
    fn validate_every_slot_relative_is_ok() {
        let b = BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            on_call: Some(PathBuf::from("lib/handlers.lisp")),
            on_cast: Some(PathBuf::from("lib/handlers.lisp")),
            on_info: Some(PathBuf::from("lib/handlers.lisp")),
            on_state_change: Some(PathBuf::from("lib/migrations.lisp")),
            on_terminate: Some(PathBuf::from("lib/cleanup.lisp")),
        };
        b.validate().unwrap();
    }

    #[test]
    fn validate_rejects_empty_path_per_slot() {
        let cases: [(&'static str, fn(PathBuf) -> BehaviorSpec); 6] = [
            (M2_BEHAVIOR_AUTHOR_KEY_ON_INIT, |p| BehaviorSpec {
                on_init: Some(p),
                ..Default::default()
            }),
            (M2_BEHAVIOR_AUTHOR_KEY_ON_CALL, |p| BehaviorSpec {
                on_call: Some(p),
                ..Default::default()
            }),
            (M2_BEHAVIOR_AUTHOR_KEY_ON_CAST, |p| BehaviorSpec {
                on_cast: Some(p),
                ..Default::default()
            }),
            (M2_BEHAVIOR_AUTHOR_KEY_ON_INFO, |p| BehaviorSpec {
                on_info: Some(p),
                ..Default::default()
            }),
            (M2_BEHAVIOR_AUTHOR_KEY_ON_STATE_CHANGE, |p| BehaviorSpec {
                on_state_change: Some(p),
                ..Default::default()
            }),
            (M2_BEHAVIOR_AUTHOR_KEY_ON_TERMINATE, |p| BehaviorSpec {
                on_terminate: Some(p),
                ..Default::default()
            }),
        ];
        for (expected_slot, build) in cases {
            let err = build(PathBuf::new()).validate().unwrap_err();
            assert!(
                matches!(err, BehaviorError::EmptyPath { slot } if slot == expected_slot),
                "slot {expected_slot}: got {err:?}",
            );
        }
    }

    #[test]
    fn validate_rejects_absolute_path() {
        let b = BehaviorSpec {
            on_init: Some(PathBuf::from("/etc/passwd")),
            ..Default::default()
        };
        let err = b.validate().unwrap_err();
        assert!(matches!(
            err,
            BehaviorError::AbsolutePath { slot, .. } if slot == M2_BEHAVIOR_AUTHOR_KEY_ON_INIT
        ));
    }

    #[test]
    fn validate_rejects_parent_escape() {
        let b = BehaviorSpec {
            on_state_change: Some(PathBuf::from("../sibling/migrations.lisp")),
            ..Default::default()
        };
        let err = b.validate().unwrap_err();
        assert!(matches!(
            err,
            BehaviorError::ParentEscape { slot, .. } if slot == M2_BEHAVIOR_AUTHOR_KEY_ON_STATE_CHANGE
        ));
    }

    #[test]
    fn validate_rejects_parent_escape_mid_path() {
        // `lib/../../escaped.lisp` is still a parent-traversal — must
        // be caught regardless of where the `..` component sits.
        let b = BehaviorSpec {
            on_terminate: Some(PathBuf::from("lib/../../escaped.lisp")),
            ..Default::default()
        };
        let err = b.validate().unwrap_err();
        assert!(matches!(
            err,
            BehaviorError::ParentEscape { slot, .. } if slot == M2_BEHAVIOR_AUTHOR_KEY_ON_TERMINATE
        ));
    }

    #[test]
    fn validate_diagnostic_order_is_deterministic() {
        // Multiple bad slots — the first declared (`:on-init`) wins
        // so authors see a stable, single-slot diagnostic.
        let b = BehaviorSpec {
            on_init: Some(PathBuf::new()),
            on_call: Some(PathBuf::from("/etc/passwd")),
            on_terminate: Some(PathBuf::from("../escape.lisp")),
            ..Default::default()
        };
        let err = b.validate().unwrap_err();
        assert!(matches!(
            err,
            BehaviorError::EmptyPath { slot } if slot == M2_BEHAVIOR_AUTHOR_KEY_ON_INIT
        ));
    }

    // ── `.lisp`-extension gate on every `:behavior :on-*` axis ─────

    #[test]
    fn validate_rejects_non_lisp_extension_per_slot() {
        // Loop the same offending non-`.lisp` path through every M2
        // typed `:behavior` slot — the diagnostic must name the
        // offending slot, not collapse to a generic "bad extension"
        // shape. Same per-slot diagnostic posture every peer
        // `BehaviorError` arm carries (`EmptyPath`, `AbsolutePath`,
        // `ParentEscape`).
        let cases: [(&'static str, fn(PathBuf) -> BehaviorSpec); 6] = [
            (M2_BEHAVIOR_AUTHOR_KEY_ON_INIT, |p| BehaviorSpec {
                on_init: Some(p),
                ..Default::default()
            }),
            (M2_BEHAVIOR_AUTHOR_KEY_ON_CALL, |p| BehaviorSpec {
                on_call: Some(p),
                ..Default::default()
            }),
            (M2_BEHAVIOR_AUTHOR_KEY_ON_CAST, |p| BehaviorSpec {
                on_cast: Some(p),
                ..Default::default()
            }),
            (M2_BEHAVIOR_AUTHOR_KEY_ON_INFO, |p| BehaviorSpec {
                on_info: Some(p),
                ..Default::default()
            }),
            (M2_BEHAVIOR_AUTHOR_KEY_ON_STATE_CHANGE, |p| BehaviorSpec {
                on_state_change: Some(p),
                ..Default::default()
            }),
            (M2_BEHAVIOR_AUTHOR_KEY_ON_TERMINATE, |p| BehaviorSpec {
                on_terminate: Some(p),
                ..Default::default()
            }),
        ];
        let path = PathBuf::from("lib/init.txt");
        for (expected_slot, build) in cases {
            let err = build(path.clone()).validate().unwrap_err();
            assert!(
                matches!(&err, BehaviorError::NonLispExtension { slot, path: p }
                    if *slot == expected_slot && p == &path),
                "slot {expected_slot}: got {err:?}",
            );
        }
    }

    #[test]
    fn validate_rejects_no_extension() {
        // The no-extension shape — author dropped the suffix entirely
        // (`lib/init` instead of `lib/init.lisp`). `Path::extension`
        // returns None, so the typed check distinguishes this from
        // the wrong-extension shape and from a leading-dot file like
        // `.lisp` (which also has no `Path::extension`).
        let cases = [
            PathBuf::from("lib/init"),
            PathBuf::from("lib/handlers"),
            PathBuf::from("init"),
        ];
        for path in cases {
            let b = BehaviorSpec {
                on_init: Some(path.clone()),
                ..Default::default()
            };
            let err = b.validate().unwrap_err();
            assert!(
                matches!(&err, BehaviorError::NonLispExtension { slot, path: p }
                    if *slot == M2_BEHAVIOR_AUTHOR_KEY_ON_INIT && p == &path),
                "no-extension path {path:?}: got {err:?}",
            );
        }
    }

    #[test]
    fn validate_rejects_wrong_extension() {
        // Common authoring footguns — files that pass every prior
        // path-shape gate but that the wasm-engine's tatara-lisp
        // reader cannot consume as source.
        let cases = [
            PathBuf::from("lib/init.rs"),
            PathBuf::from("lib/init.txt"),
            PathBuf::from("lib/init.md"),
            PathBuf::from("lib/init.json"),
            PathBuf::from("lib/init.yaml"),
            PathBuf::from("lib/init.lisp.bak"),
            PathBuf::from("lib/init.lispx"),
        ];
        for path in cases {
            let b = BehaviorSpec {
                on_call: Some(path.clone()),
                ..Default::default()
            };
            let err = b.validate().unwrap_err();
            assert!(
                matches!(&err, BehaviorError::NonLispExtension { slot, path: p }
                    if *slot == M2_BEHAVIOR_AUTHOR_KEY_ON_CALL && p == &path),
                "wrong-extension path {path:?}: got {err:?}",
            );
        }
    }

    #[test]
    fn validate_rejects_uppercase_lisp_extension() {
        // The strict-lowercase posture matches every other shape
        // predicate in `render.rs` — the byte-size codec is
        // case-sensitive on `MiB`, the duration codec on `ms` / `s` /
        // `m` / `h`, every DNS-1123 label is lowercase-only — so the
        // accepted set for `.lisp` does not silently fold to `.LISP` /
        // `.Lisp` / `.LiSp` even on case-insensitive volumes. A path
        // whose existence check would match the on-disk file via
        // case-insensitive lookup would still mismatch the canonical
        // form the codec emits, breaking the round-trip-stability
        // contract.
        let cases = [
            PathBuf::from("lib/init.LISP"),
            PathBuf::from("lib/init.Lisp"),
            PathBuf::from("lib/init.LiSp"),
        ];
        for path in cases {
            let b = BehaviorSpec {
                on_init: Some(path.clone()),
                ..Default::default()
            };
            let err = b.validate().unwrap_err();
            assert!(
                matches!(&err, BehaviorError::NonLispExtension { slot, path: p }
                    if *slot == M2_BEHAVIOR_AUTHOR_KEY_ON_INIT && p == &path),
                "uppercase `.lisp` {path:?}: got {err:?}",
            );
        }
    }

    #[test]
    fn validate_accepts_canonical_lisp_paths() {
        // Positive control: every shape the in-tree fixtures and
        // module-doc examples use must pass the gate. Pins the
        // accepted set so a future tightening doesn't accidentally
        // reject the canonical authoring shape.
        let cases = [
            PathBuf::from("lib/init.lisp"),
            PathBuf::from("lib/handlers.lisp"),
            PathBuf::from("lib/migrations/v01-to-v02.lisp"),
            PathBuf::from("init.lisp"),
            PathBuf::from("a.lisp"),
            PathBuf::from("./lib/init.lisp"),
            PathBuf::from("lib/./handlers.lisp"),
            PathBuf::from("lib/migrations/v.0.1.lisp"),
        ];
        for path in cases {
            let b = BehaviorSpec {
                on_init: Some(path.clone()),
                ..Default::default()
            };
            b.validate()
                .unwrap_or_else(|e| panic!("canonical `.lisp` path {path:?} must pass: {e:?}"));
        }
    }

    #[test]
    fn validate_path_shape_precedes_extension_arm() {
        // Cross-arm ordering: a path that is *both* path-shape invalid
        // (empty / absolute / parent-escape) and non-`.lisp` surfaces
        // the more fundamental sandbox-shape diagnostic first — the
        // `.lisp` remediation is misleading when the offending path
        // can never resolve under the caixa root anyway. Mirrors the
        // `MemoryZero` → `MemoryBelowWasm32Page` → `MemoryExceedsWasm32Cap`
        // → `MemoryNotPageMultiple` smallest-scope-last cascade on the
        // peer `:limits :memory` axis.

        // Empty + non-`.lisp` → empty wins (the empty case has no
        // extension to begin with).
        let b = BehaviorSpec {
            on_init: Some(PathBuf::new()),
            ..Default::default()
        };
        assert!(matches!(
            b.validate().unwrap_err(),
            BehaviorError::EmptyPath { slot } if slot == M2_BEHAVIOR_AUTHOR_KEY_ON_INIT
        ));

        // Absolute + non-`.lisp` → absolute wins.
        let b = BehaviorSpec {
            on_init: Some(PathBuf::from("/etc/init.txt")),
            ..Default::default()
        };
        assert!(matches!(
            b.validate().unwrap_err(),
            BehaviorError::AbsolutePath { slot, .. } if slot == M2_BEHAVIOR_AUTHOR_KEY_ON_INIT
        ));

        // Parent-escape + non-`.lisp` → parent-escape wins.
        let b = BehaviorSpec {
            on_init: Some(PathBuf::from("../sibling/init.txt")),
            ..Default::default()
        };
        assert!(matches!(
            b.validate().unwrap_err(),
            BehaviorError::ParentEscape { slot, .. } if slot == M2_BEHAVIOR_AUTHOR_KEY_ON_INIT
        ));
    }

    #[test]
    fn validate_extension_diagnostic_names_offending_slot_and_path() {
        // Self-locating diagnostic pin: the surfaced error names the
        // exact `:on-*` slot the author wrote and the exact offending
        // path verbatim, so the author can grep their caixa.lisp for
        // the named slot / path and fix it in one edit. Same shape
        // every peer per-slot `:behavior` arm exposes.
        let path = PathBuf::from("lib/handlers.rs");
        let b = BehaviorSpec {
            on_cast: Some(path.clone()),
            ..Default::default()
        };
        let err = b.validate().unwrap_err();
        let rendered = err.to_string();
        assert!(
            rendered.contains(M2_BEHAVIOR_AUTHOR_KEY_ON_CAST),
            "diagnostic must name the `:on-cast` slot: {rendered}"
        );
        assert!(
            rendered.contains("lib/handlers.rs"),
            "diagnostic must carry the offending path verbatim: {rendered}"
        );
        assert!(
            rendered.contains(".lisp"),
            "diagnostic must name the expected `.lisp` extension: {rendered}"
        );
    }

    #[test]
    fn validate_extension_arm_fires_across_multi_malformed_manifest_in_slot_order() {
        // Multi-malformed manifest, all four slots carrying a
        // non-`.lisp` extension — the first declared slot
        // (`:on-init`) wins, mirroring the prior
        // `validate_diagnostic_order_is_deterministic` pin on the
        // path-shape arms.
        let b = BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.rs")),
            on_call: Some(PathBuf::from("lib/handlers.txt")),
            on_state_change: Some(PathBuf::from("lib/migrations.md")),
            ..Default::default()
        };
        let err = b.validate().unwrap_err();
        assert!(matches!(
            &err,
            BehaviorError::NonLispExtension { slot, path }
                if *slot == M2_BEHAVIOR_AUTHOR_KEY_ON_INIT
                    && path == &PathBuf::from("lib/init.rs")
        ));
    }

    #[test]
    fn m2_behavior_author_key_consts_pin_canonical_kebab_case_labels() {
        // Scalar-value pin: the six author-facing kebab-case labels the
        // `(defcaixa … :behavior (:on-* …))` surface admits, one arm per
        // sub-slot. Mirrors the peer scalar-value pin the sibling
        // renderer-side [`crate::M2_BEHAVIOR_KEY_ON_*`] camelCase consts
        // carry (21fe462), so both halves of the M2 `:behavior` sub-slot
        // dual axis (author-facing kebab-case label + renderer-side
        // camelCase wire key) route through one canonical per-arm
        // declaration. A future rebrand (`:on-init` → `:on-start`
        // matching Akka's per-actor preStart naming, `:on-state-change`
        // → `:on-code-change` matching Erlang's verbatim `code_change/3`
        // name) lands as an edit to exactly one const, and every
        // consumer that reaches for the label picks it up at build time
        // rather than at runtime as a downstream mismatch.
        assert_eq!(M2_BEHAVIOR_AUTHOR_KEY_ON_INIT, ":on-init");
        assert_eq!(M2_BEHAVIOR_AUTHOR_KEY_ON_CALL, ":on-call");
        assert_eq!(M2_BEHAVIOR_AUTHOR_KEY_ON_CAST, ":on-cast");
        assert_eq!(M2_BEHAVIOR_AUTHOR_KEY_ON_INFO, ":on-info");
        assert_eq!(M2_BEHAVIOR_AUTHOR_KEY_ON_STATE_CHANGE, ":on-state-change");
        assert_eq!(M2_BEHAVIOR_AUTHOR_KEY_ON_TERMINATE, ":on-terminate");
    }

    #[test]
    fn declared_slots_labels_route_through_lifted_author_key_consts() {
        // Production-through-const pin: the six per-arm labels
        // [`BehaviorSpec::declared_slots`] threads through as the
        // `(slot, path)` iterator's first component route through the
        // lifted [`crate::M2_BEHAVIOR_AUTHOR_KEY_ON_*`] consts, in
        // declaration order. A future re-order or drift at the tagger
        // (a rename that reaches the tagger but not the const, or vice
        // versa) surfaces here at build time rather than at runtime as
        // a diagnostic naming a stale kebab-case slot far from the
        // rename's commit.
        let b = BehaviorSpec {
            on_init: Some(PathBuf::from("a.lisp")),
            on_call: Some(PathBuf::from("b.lisp")),
            on_cast: Some(PathBuf::from("c.lisp")),
            on_info: Some(PathBuf::from("d.lisp")),
            on_state_change: Some(PathBuf::from("e.lisp")),
            on_terminate: Some(PathBuf::from("f.lisp")),
        };
        let labels: Vec<&'static str> = b.declared_slots().map(|(s, _)| s).collect();
        assert_eq!(
            labels,
            vec![
                M2_BEHAVIOR_AUTHOR_KEY_ON_INIT,
                M2_BEHAVIOR_AUTHOR_KEY_ON_CALL,
                M2_BEHAVIOR_AUTHOR_KEY_ON_CAST,
                M2_BEHAVIOR_AUTHOR_KEY_ON_INFO,
                M2_BEHAVIOR_AUTHOR_KEY_ON_STATE_CHANGE,
                M2_BEHAVIOR_AUTHOR_KEY_ON_TERMINATE,
            ]
        );
    }

    #[test]
    fn declared_slots_paths_route_through_lifted_on_star_accessors() {
        // Production-through-accessor pin: the six per-arm
        // `Option<&Path>` path-values [`BehaviorSpec::declared_slots`]
        // threads through as the `(slot, path)` iterator's second
        // component route through the lifted per-slot
        // [`BehaviorSpec::on_init`] / [`BehaviorSpec::on_call`] /
        // [`BehaviorSpec::on_cast`] / [`BehaviorSpec::on_info`] /
        // [`BehaviorSpec::on_state_change`] / [`BehaviorSpec::on_terminate`]
        // accessors, so every future accessor-side extension of an
        // `:on-*` slot (a per-prior-`:versao` state-migration callback
        // the operator pins through a future `:behavior
        // :on-state-change-overrides` slot the
        // `theory/ABSORPTION-ROADMAP.md` M2.5 wasm-engine
        // callback-dispatch wire acknowledges, a per-tenant callback
        // alias table the M4 CR materializer resolves per-CR, a
        // per-cluster callback overlay the operator pins through a
        // future placement-scoped slot) reaches both production
        // consumers of the iterator (the layout checker's existence
        // sweep in `layout.rs` + the sibling [`BehaviorSpec::validate`]
        // value-shape gate) by construction, without a coordinated
        // rewrite of the iterator's six raw-field-access sites and the
        // six accessor bodies in lockstep. Peer of the sibling
        // `declared_slots_labels_route_through_lifted_author_key_consts`
        // pin on the tag-surface axis — same "one typed dispatch on the
        // substrate primitive, thin projections at each consumer"
        // discipline extended onto the peer per-arm `Option<&Path>`
        // path-value axis.
        //
        // Byte-equal today (each `on_*()` accessor is a thin
        // `.as_deref()` on the raw `Option<PathBuf>` field); the pin
        // catches any future accessor-side extension whose iterator
        // read regresses to the raw field.
        let b = BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            on_call: Some(PathBuf::from("lib/rpc/call.lisp")),
            on_cast: Some(PathBuf::from("lib/rpc/cast.lisp")),
            on_info: Some(PathBuf::from("lib/rpc/info.lisp")),
            on_state_change: Some(PathBuf::from("lib/migrations.lisp")),
            on_terminate: Some(PathBuf::from("lib/cleanup.lisp")),
        };
        let entries: Vec<(&'static str, &Path)> = b.declared_slots().collect();
        assert_eq!(
            entries,
            vec![
                (M2_BEHAVIOR_AUTHOR_KEY_ON_INIT, b.on_init().unwrap()),
                (M2_BEHAVIOR_AUTHOR_KEY_ON_CALL, b.on_call().unwrap()),
                (M2_BEHAVIOR_AUTHOR_KEY_ON_CAST, b.on_cast().unwrap()),
                (M2_BEHAVIOR_AUTHOR_KEY_ON_INFO, b.on_info().unwrap()),
                (
                    M2_BEHAVIOR_AUTHOR_KEY_ON_STATE_CHANGE,
                    b.on_state_change().unwrap(),
                ),
                (
                    M2_BEHAVIOR_AUTHOR_KEY_ON_TERMINATE,
                    b.on_terminate().unwrap(),
                ),
            ],
            "declared_slots must route each of its six per-arm \
             Option<&Path> path-values through the sibling lifted \
             BehaviorSpec::on_* accessor for its slot, so future \
             accessor-side extensions reach both the layout checker \
             + validate gate by construction (got {entries:?})",
        );
    }

    #[test]
    fn declared_paths_routes_through_lifted_on_star_accessors() {
        // Sibling of `declared_slots_paths_route_through_lifted_
        // on_star_accessors` on the peer path-only projection axis:
        // [`BehaviorSpec::declared_paths`] must project each declared
        // callback path through the lifted per-slot
        // [`BehaviorSpec::on_*`] accessor, so the layout checker's
        // `for p in b.declared_paths() { root.join(p) }` on-disk
        // existence sweep at `layout.rs:901` reaches every future
        // accessor-side extension without a coordinated rewrite of the
        // sibling `declared_slots` internal iterator + the accessor
        // bodies in lockstep.
        let b = BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            on_call: Some(PathBuf::from("lib/rpc/call.lisp")),
            on_cast: Some(PathBuf::from("lib/rpc/cast.lisp")),
            on_info: Some(PathBuf::from("lib/rpc/info.lisp")),
            on_state_change: Some(PathBuf::from("lib/migrations.lisp")),
            on_terminate: Some(PathBuf::from("lib/cleanup.lisp")),
        };
        let paths: Vec<&Path> = b.declared_paths().collect();
        assert_eq!(
            paths,
            vec![
                b.on_init().unwrap(),
                b.on_call().unwrap(),
                b.on_cast().unwrap(),
                b.on_info().unwrap(),
                b.on_state_change().unwrap(),
                b.on_terminate().unwrap(),
            ],
            "declared_paths must project each callback path through \
             the sibling lifted BehaviorSpec::on_* accessor for its \
             slot (got {paths:?})",
        );
    }

    // ── per-`:behavior :on-state-change` accessor pins ─────────────────────

    #[test]
    fn behavior_on_state_change_returns_option_path_verbatim_across_permutations() {
        // Canonical per-`:behavior` `:on-state-change` OTP-`code_change/3`-
        // shaped callback-path scalar pin: [`BehaviorSpec::on_state_change`]
        // must return the `:behavior :on-state-change` typed `PathBuf`
        // verbatim as an `Option<&Path>`, borrowed from the raw
        // `Option<PathBuf>` field access across the three canonical
        // shape-arms — `None` (no callback declared — the caixa exposes
        // no hot-upgrade state-fold path), `Some("lib/migrations.lisp")`
        // (the canonical single-file shape the module-doc example uses),
        // `Some("lib/migrations/v01-to-v02.lisp")` (the per-version
        // sub-directory shape the `theory/ABSORPTION-ROADMAP.md` M2.5
        // wasm-engine callback-dispatch wire acknowledges).
        //
        // Peer of the sibling per-`:placement` [`crate::Placement::shard_key`]
        // (7cd2a28) / [`crate::Placement::affinity`] (74ec2d3)
        // `Option<&str>` accessor pin on the sibling `Option<Str>`-return
        // axis, extended to the peer per-`:behavior` typed-`PathBuf`
        // optional-scalar shape — first `Option<&Path>`-return accessor
        // on the M2 `:behavior` slot family. Pins against a future silent
        // detour that re-derived the callback path from a peer axis (an
        // accidental `.on_call`-collapse that assumed the two
        // `Option<PathBuf>` axes carry the same value), a `None` →
        // `Some(empty)` collapse (the canonical
        // `Option<PathBuf>` → `PathBuf::new()` footgun the
        // [`BehaviorError::EmptyPath`] validate arm guards on the peer
        // path-shape axis), or a per-arm variant swap that landed on one
        // consumer without the other.
        for path in [
            None,
            Some(PathBuf::from("lib/migrations.lisp")),
            Some(PathBuf::from("lib/migrations/v01-to-v02.lisp")),
        ] {
            let b = BehaviorSpec {
                on_state_change: path.clone(),
                ..BehaviorSpec::default()
            };
            assert_eq!(
                b.on_state_change(),
                path.as_deref(),
                "BehaviorSpec::on_state_change must return the \
                 :behavior :on-state-change PathBuf verbatim as \
                 Option<&Path> (got {:?}, expected {:?})",
                b.on_state_change(),
                path.as_deref(),
            );
            assert_eq!(
                b.on_state_change(),
                b.on_state_change.as_deref(),
                "BehaviorSpec::on_state_change must byte-equal the \
                 raw .on_state_change.as_deref() field access across \
                 every value in the accept-set",
            );
        }
    }

    #[test]
    fn behavior_on_state_change_is_independent_of_peer_on_star_axes() {
        // Cross-axis independence pin: flipping only the
        // `:on-state-change` axis flips [`BehaviorSpec::on_state_change`]
        // independently of every peer `:on-*` axis
        // (`:on-init` / `:on-call` / `:on-cast` / `:on-info` /
        // `:on-terminate`). A future silent detour that re-derived the
        // callback path from a peer axis (an accidental `.on_call`-
        // collapse, a "state-change falls back to on-info" default that
        // would silently rebind the callback dispatch to the wrong
        // slot) surfaces here as a build-time test failure.
        //
        // Mirrors the sibling `limits_is_empty_memory_arm_routes_through_accessor`
        // (620c067) cross-axis pin on the peer M2 `:limits` slot
        // family — each accessor-lift closes exactly one axis and
        // leaves every peer axis unshifted.
        let base = BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            on_call: Some(PathBuf::from("lib/handlers.lisp")),
            on_cast: Some(PathBuf::from("lib/handlers.lisp")),
            on_info: Some(PathBuf::from("lib/handlers.lisp")),
            on_terminate: Some(PathBuf::from("lib/cleanup.lisp")),
            ..BehaviorSpec::default()
        };
        assert_eq!(base.on_state_change(), None);
        let with = BehaviorSpec {
            on_state_change: Some(PathBuf::from("lib/migrations.lisp")),
            ..base.clone()
        };
        assert_eq!(
            with.on_state_change(),
            Some(PathBuf::from("lib/migrations.lisp").as_path()),
            "BehaviorSpec::on_state_change must project the \
             :on-state-change axis independently of every peer :on-* \
             axis (got {:?})",
            with.on_state_change(),
        );
    }

    #[test]
    fn validate_upgrade_from_against_behavior_routes_through_on_state_change_accessor() {
        // Production-through-const pin: the sole caixa-core consumer of
        // the accessor's `Option<&Path>` presence — the
        // [`crate::validate_upgrade_from_against_behavior`] cross-slot
        // composition gate — must route through
        // [`BehaviorSpec::on_state_change`] rather than the raw
        // `.on_state_change` field, so the gate's short-circuit and
        // every future accessor-side extension (a per-prior-`:versao`
        // callback override, a per-tenant migration alias table) land
        // as one edit at the accessor rather than as a coordinated
        // two-site rewrite of the gate + accessor.
        //
        // Peer of the sibling `declared_slots_labels_route_through_
        // lifted_author_key_consts` (production-through-const pin on
        // the label surface) and the sibling
        // `limits_is_empty_memory_arm_routes_through_accessor`
        // (620c067) pin on the peer M2 `:limits` slot family.
        //
        // Positive control: a `:behavior :on-state-change` callback
        // declared + a `:upgrade-from` entry carrying a
        // `(:state-change …)` instruction admits, because the accessor
        // returns `Some(&Path)` and the gate's short-circuit fires.
        let entries = vec![crate::UpgradeFromEntry {
            from: "0.1.0".to_string(),
            instructions: vec![
                crate::UpgradeInstruction::LoadModule {
                    module: "codec".to_string(),
                },
                crate::UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/migrations/v01-to-v02.lisp"),
                },
            ],
        }];
        let b = BehaviorSpec {
            on_state_change: Some(PathBuf::from("lib/migrations.lisp")),
            ..BehaviorSpec::default()
        };
        assert!(b.on_state_change().is_some());
        crate::validate_upgrade_from_against_behavior(&entries, Some(&b))
            .expect("callback declared → gate admits");

        // Negative control: dropping only the accessor's slot to `None`
        // (with the same `:upgrade-from` entry) flips the gate to
        // refusal — the accessor's `None` return is the sole predicate
        // the short-circuit reads.
        let b_no_cb = BehaviorSpec::default();
        assert_eq!(b_no_cb.on_state_change(), None);
        let err = crate::validate_upgrade_from_against_behavior(&entries, Some(&b_no_cb))
            .expect_err(":state-change instruction without callback → refuse");
        assert!(matches!(
            err,
            crate::UpgradeError::StateChangeWithoutOnStateChangeCallback { .. }
        ));

        // `behavior: None` is the same refusal shape — the accessor
        // isn't reached, but `Option::and_then` on `None` short-circuits
        // to `None`, so the diagnostic is identical.
        let err = crate::validate_upgrade_from_against_behavior(&entries, None)
            .expect_err("behavior absent + :state-change instruction → refuse");
        assert!(matches!(
            err,
            crate::UpgradeError::StateChangeWithoutOnStateChangeCallback { .. }
        ));
    }

    // ── per-`:behavior :on-init` accessor pins ─────────────────────

    #[test]
    fn behavior_on_init_returns_option_path_verbatim_across_permutations() {
        // Canonical per-`:behavior` `:on-init` OTP-`init/1`-shaped
        // callback-path scalar pin: [`BehaviorSpec::on_init`] must
        // return the `:behavior :on-init` typed `PathBuf` verbatim as
        // an `Option<&Path>`, borrowed from the raw `Option<PathBuf>`
        // field access across the three canonical shape-arms — `None`
        // (no callback declared — the runtime falls back to the
        // wasm-engine's no-op instance-start default), `Some("lib/init.lisp")`
        // (the canonical single-file shape the module-doc example
        // uses), `Some("lib/lifecycle/init.lisp")` (the per-lifecycle
        // sub-directory shape the `theory/ABSORPTION-ROADMAP.md` M2.5
        // wasm-engine callback-dispatch wire acknowledges).
        //
        // Peer of the sibling per-`:behavior` [`BehaviorSpec::on_state_change`]
        // (9b4ecde) `Option<&Path>` accessor pin on the sibling
        // `Option<PathBuf>`-return axis — second `Option<&Path>`-return
        // accessor on the M2 `:behavior` slot family. Pins against a
        // future silent detour that re-derived the callback path from a
        // peer axis (an accidental `.on_call`-collapse that assumed the
        // two `Option<PathBuf>` axes carry the same value), a `None` →
        // `Some(empty)` collapse (the canonical
        // `Option<PathBuf>` → `PathBuf::new()` footgun the
        // [`BehaviorError::EmptyPath`] validate arm guards on the peer
        // path-shape axis), or a per-arm variant swap that landed on
        // one consumer without the other.
        for path in [
            None,
            Some(PathBuf::from("lib/init.lisp")),
            Some(PathBuf::from("lib/lifecycle/init.lisp")),
        ] {
            let b = BehaviorSpec {
                on_init: path.clone(),
                ..BehaviorSpec::default()
            };
            assert_eq!(
                b.on_init(),
                path.as_deref(),
                "BehaviorSpec::on_init must return the \
                 :behavior :on-init PathBuf verbatim as \
                 Option<&Path> (got {:?}, expected {:?})",
                b.on_init(),
                path.as_deref(),
            );
            assert_eq!(
                b.on_init(),
                b.on_init.as_deref(),
                "BehaviorSpec::on_init must byte-equal the \
                 raw .on_init.as_deref() field access across \
                 every value in the accept-set",
            );
        }
    }

    #[test]
    fn behavior_on_init_is_independent_of_peer_on_star_axes() {
        // Cross-axis independence pin: flipping only the `:on-init`
        // axis flips [`BehaviorSpec::on_init`] independently of every
        // peer `:on-*` axis (`:on-call` / `:on-cast` / `:on-info` /
        // `:on-state-change` / `:on-terminate`). A future silent detour
        // that re-derived the callback path from a peer axis (an
        // accidental `.on_call`-collapse, a "init falls back to
        // state-change" default that would silently rebind the callback
        // dispatch to the wrong slot) surfaces here as a build-time
        // test failure.
        //
        // Peer of the sibling
        // `behavior_on_state_change_is_independent_of_peer_on_star_axes`
        // (9b4ecde) cross-axis pin on the sibling
        // `:on-state-change` axis — each accessor-lift closes exactly
        // one axis and leaves every peer axis unshifted.
        let base = BehaviorSpec {
            on_call: Some(PathBuf::from("lib/handlers.lisp")),
            on_cast: Some(PathBuf::from("lib/handlers.lisp")),
            on_info: Some(PathBuf::from("lib/handlers.lisp")),
            on_state_change: Some(PathBuf::from("lib/migrations.lisp")),
            on_terminate: Some(PathBuf::from("lib/cleanup.lisp")),
            ..BehaviorSpec::default()
        };
        assert_eq!(base.on_init(), None);
        let with = BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            ..base.clone()
        };
        assert_eq!(
            with.on_init(),
            Some(PathBuf::from("lib/init.lisp").as_path()),
            "BehaviorSpec::on_init must project the \
             :on-init axis independently of every peer :on-* \
             axis (got {:?})",
            with.on_init(),
        );
    }

    // ── per-`:behavior :on-call` accessor pins ─────────────────────

    #[test]
    fn behavior_on_call_returns_option_path_verbatim_across_permutations() {
        // Canonical per-`:behavior` `:on-call`
        // OTP-`gen_server:handle_call/3`-shaped synchronous
        // request/response callback-path scalar pin:
        // [`BehaviorSpec::on_call`] must return the `:behavior :on-call`
        // typed `PathBuf` verbatim as an `Option<&Path>`, borrowed from
        // the raw `Option<PathBuf>` field access across the three
        // canonical shape-arms — `None` (no callback declared — the
        // runtime falls back to the wasm-engine's raw
        // `wasi:http/incoming-handler` default), `Some("lib/handlers.lisp")`
        // (the canonical single-file shape the module-doc example
        // uses), `Some("lib/rpc/call.lisp")` (the per-dispatch
        // sub-directory shape the `theory/ABSORPTION-ROADMAP.md` M2.5
        // wasm-engine callback-dispatch wire acknowledges).
        //
        // Peer of the sibling per-`:behavior`
        // [`BehaviorSpec::on_state_change`] (9b4ecde) /
        // [`BehaviorSpec::on_init`] (d66c702) `Option<&Path>` accessor
        // pins on the sibling `Option<PathBuf>`-return axes — third
        // `Option<&Path>`-return accessor on the M2 `:behavior` slot
        // family. Pins against a future silent detour that re-derived
        // the callback path from a peer axis (an accidental
        // `.on_cast`-collapse that assumed the two `Option<PathBuf>`
        // axes carry the same value — a plausible slip because the
        // module-doc example shares one `lib/handlers.lisp` file
        // between `:on-call` / `:on-cast` / `:on-info` on the pattern
        // that the tatara-lisp dispatch inside the file discriminates
        // on the callback-kind atom), a `None` → `Some(empty)` collapse
        // (the canonical `Option<PathBuf>` → `PathBuf::new()` footgun
        // the [`BehaviorError::EmptyPath`] validate arm guards on the
        // peer path-shape axis), or a per-arm variant swap that landed
        // on one consumer without the other.
        for path in [
            None,
            Some(PathBuf::from("lib/handlers.lisp")),
            Some(PathBuf::from("lib/rpc/call.lisp")),
        ] {
            let b = BehaviorSpec {
                on_call: path.clone(),
                ..BehaviorSpec::default()
            };
            assert_eq!(
                b.on_call(),
                path.as_deref(),
                "BehaviorSpec::on_call must return the \
                 :behavior :on-call PathBuf verbatim as \
                 Option<&Path> (got {:?}, expected {:?})",
                b.on_call(),
                path.as_deref(),
            );
            assert_eq!(
                b.on_call(),
                b.on_call.as_deref(),
                "BehaviorSpec::on_call must byte-equal the \
                 raw .on_call.as_deref() field access across \
                 every value in the accept-set",
            );
        }
    }

    #[test]
    fn behavior_on_call_is_independent_of_peer_on_star_axes() {
        // Cross-axis independence pin: flipping only the `:on-call`
        // axis flips [`BehaviorSpec::on_call`] independently of every
        // peer `:on-*` axis (`:on-init` / `:on-cast` / `:on-info` /
        // `:on-state-change` / `:on-terminate`). A future silent detour
        // that re-derived the callback path from a peer axis (an
        // accidental `.on_cast`-collapse — the sibling asynchronous
        // fire-and-forget arm on the peer OTP dispatch triad, a
        // plausible confusion because both callbacks share the
        // `handle_*/2|3` OTP shape — that would silently rebind the
        // synchronous request/response dispatch to the sibling
        // fire-and-forget slot's callback) surfaces here as a
        // build-time test failure.
        //
        // Peer of the sibling
        // `behavior_on_state_change_is_independent_of_peer_on_star_axes`
        // (9b4ecde) /
        // `behavior_on_init_is_independent_of_peer_on_star_axes`
        // (d66c702) cross-axis pins on the sibling `:on-state-change` /
        // `:on-init` axes — each accessor-lift closes exactly one axis
        // and leaves every peer axis unshifted.
        let base = BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            on_cast: Some(PathBuf::from("lib/handlers.lisp")),
            on_info: Some(PathBuf::from("lib/handlers.lisp")),
            on_state_change: Some(PathBuf::from("lib/migrations.lisp")),
            on_terminate: Some(PathBuf::from("lib/cleanup.lisp")),
            ..BehaviorSpec::default()
        };
        assert_eq!(base.on_call(), None);
        let with = BehaviorSpec {
            on_call: Some(PathBuf::from("lib/handlers.lisp")),
            ..base.clone()
        };
        assert_eq!(
            with.on_call(),
            Some(PathBuf::from("lib/handlers.lisp").as_path()),
            "BehaviorSpec::on_call must project the \
             :on-call axis independently of every peer :on-* \
             axis (got {:?})",
            with.on_call(),
        );
    }

    // ── per-`:behavior :on-cast` accessor pins ─────────────────────

    #[test]
    fn behavior_on_cast_returns_option_path_verbatim_across_permutations() {
        // Canonical per-`:behavior` `:on-cast`
        // OTP-`gen_server:handle_cast/2`-shaped asynchronous
        // fire-and-forget callback-path scalar pin:
        // [`BehaviorSpec::on_cast`] must return the `:behavior :on-cast`
        // typed `PathBuf` verbatim as an `Option<&Path>`, borrowed from
        // the raw `Option<PathBuf>` field access across the three
        // canonical shape-arms — `None` (no callback declared — the
        // runtime falls back to the wasm-engine's default `Accepted:
        // 202` fire-and-forget shape), `Some("lib/handlers.lisp")` (the
        // canonical single-file shape the module-doc example uses,
        // shared with `:on-call` / `:on-info` on the pattern that the
        // tatara-lisp dispatch inside the file discriminates on the
        // callback-kind atom), `Some("lib/rpc/cast.lisp")` (the
        // per-dispatch sub-directory shape the
        // `theory/ABSORPTION-ROADMAP.md` M2.5 wasm-engine
        // callback-dispatch wire acknowledges).
        //
        // Peer of the sibling per-`:behavior`
        // [`BehaviorSpec::on_state_change`] (9b4ecde) /
        // [`BehaviorSpec::on_init`] (d66c702) /
        // [`BehaviorSpec::on_call`] (156ddbe) `Option<&Path>` accessor
        // pins on the sibling `Option<PathBuf>`-return axes — fourth
        // `Option<&Path>`-return accessor on the M2 `:behavior` slot
        // family. Pins against a future silent detour that re-derived
        // the callback path from a peer axis (an accidental
        // `.on_call`-collapse that assumed the two `Option<PathBuf>`
        // axes carry the same value — a plausible slip because both
        // callbacks share the `handle_*/2|3` OTP shape and the
        // module-doc example shares one `lib/handlers.lisp` file
        // between `:on-call` / `:on-cast` / `:on-info`), a `None` →
        // `Some(empty)` collapse (the canonical `Option<PathBuf>` →
        // `PathBuf::new()` footgun the [`BehaviorError::EmptyPath`]
        // validate arm guards on the peer path-shape axis), or a
        // per-arm variant swap that landed on one consumer without the
        // other.
        for path in [
            None,
            Some(PathBuf::from("lib/handlers.lisp")),
            Some(PathBuf::from("lib/rpc/cast.lisp")),
        ] {
            let b = BehaviorSpec {
                on_cast: path.clone(),
                ..BehaviorSpec::default()
            };
            assert_eq!(
                b.on_cast(),
                path.as_deref(),
                "BehaviorSpec::on_cast must return the \
                 :behavior :on-cast PathBuf verbatim as \
                 Option<&Path> (got {:?}, expected {:?})",
                b.on_cast(),
                path.as_deref(),
            );
            assert_eq!(
                b.on_cast(),
                b.on_cast.as_deref(),
                "BehaviorSpec::on_cast must byte-equal the \
                 raw .on_cast.as_deref() field access across \
                 every value in the accept-set",
            );
        }
    }

    #[test]
    fn behavior_on_cast_is_independent_of_peer_on_star_axes() {
        // Cross-axis independence pin: flipping only the `:on-cast`
        // axis flips [`BehaviorSpec::on_cast`] independently of every
        // peer `:on-*` axis (`:on-init` / `:on-call` / `:on-info` /
        // `:on-state-change` / `:on-terminate`). A future silent detour
        // that re-derived the callback path from a peer axis (an
        // accidental `.on_call`-collapse — the sibling synchronous
        // request/response arm on the peer OTP dispatch triad, a
        // plausible confusion because both callbacks share the
        // `handle_*/2|3` OTP shape and the module-doc example shares
        // one `lib/handlers.lisp` file between the two — that would
        // silently rebind the asynchronous fire-and-forget dispatch to
        // the sibling synchronous request/response slot's callback)
        // surfaces here as a build-time test failure.
        //
        // Peer of the sibling
        // `behavior_on_state_change_is_independent_of_peer_on_star_axes`
        // (9b4ecde) /
        // `behavior_on_init_is_independent_of_peer_on_star_axes`
        // (d66c702) /
        // `behavior_on_call_is_independent_of_peer_on_star_axes`
        // (156ddbe) cross-axis pins on the sibling `:on-state-change` /
        // `:on-init` / `:on-call` axes — each accessor-lift closes
        // exactly one axis and leaves every peer axis unshifted.
        let base = BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            on_call: Some(PathBuf::from("lib/handlers.lisp")),
            on_info: Some(PathBuf::from("lib/handlers.lisp")),
            on_state_change: Some(PathBuf::from("lib/migrations.lisp")),
            on_terminate: Some(PathBuf::from("lib/cleanup.lisp")),
            ..BehaviorSpec::default()
        };
        assert_eq!(base.on_cast(), None);
        let with = BehaviorSpec {
            on_cast: Some(PathBuf::from("lib/handlers.lisp")),
            ..base.clone()
        };
        assert_eq!(
            with.on_cast(),
            Some(PathBuf::from("lib/handlers.lisp").as_path()),
            "BehaviorSpec::on_cast must project the \
             :on-cast axis independently of every peer :on-* \
             axis (got {:?})",
            with.on_cast(),
        );
    }

    // ── per-`:behavior :on-info` accessor pins ─────────────────────

    #[test]
    fn behavior_on_info_returns_option_path_verbatim_across_permutations() {
        // Canonical per-`:behavior` `:on-info`
        // OTP-`gen_server:handle_info/2`-shaped system / out-of-band
        // message-handler callback-path scalar pin:
        // [`BehaviorSpec::on_info`] must return the `:behavior :on-info`
        // typed `PathBuf` verbatim as an `Option<&Path>`, borrowed from
        // the raw `Option<PathBuf>` field access across the three
        // canonical shape-arms — `None` (no callback declared — the
        // runtime silently drops every out-of-band mailbox message),
        // `Some("lib/handlers.lisp")` (the canonical single-file shape
        // the module-doc example uses, shared with `:on-call` /
        // `:on-cast` on the pattern that the tatara-lisp dispatch
        // inside the file discriminates on the callback-kind atom),
        // `Some("lib/rpc/info.lisp")` (the per-dispatch sub-directory
        // shape the `theory/ABSORPTION-ROADMAP.md` M2.5 wasm-engine
        // callback-dispatch wire acknowledges).
        //
        // Peer of the sibling per-`:behavior`
        // [`BehaviorSpec::on_state_change`] (9b4ecde) /
        // [`BehaviorSpec::on_init`] (d66c702) /
        // [`BehaviorSpec::on_call`] (156ddbe) /
        // [`BehaviorSpec::on_cast`] (99616ac) `Option<&Path>` accessor
        // pins on the sibling `Option<PathBuf>`-return axes — fifth
        // `Option<&Path>`-return accessor on the M2 `:behavior` slot
        // family. Pins against a future silent detour that re-derived
        // the callback path from a peer axis (an accidental
        // `.on_cast`-collapse that assumed the two `Option<PathBuf>`
        // axes carry the same value — a plausible slip because the
        // module-doc example shares one `lib/handlers.lisp` file
        // between `:on-call` / `:on-cast` / `:on-info` on the pattern
        // that the tatara-lisp dispatch inside the file discriminates
        // on the callback-kind atom), a `None` → `Some(empty)` collapse
        // (the canonical `Option<PathBuf>` → `PathBuf::new()` footgun
        // the [`BehaviorError::EmptyPath`] validate arm guards on the
        // peer path-shape axis), or a per-arm variant swap that landed
        // on one consumer without the other.
        for path in [
            None,
            Some(PathBuf::from("lib/handlers.lisp")),
            Some(PathBuf::from("lib/rpc/info.lisp")),
        ] {
            let b = BehaviorSpec {
                on_info: path.clone(),
                ..BehaviorSpec::default()
            };
            assert_eq!(
                b.on_info(),
                path.as_deref(),
                "BehaviorSpec::on_info must return the \
                 :behavior :on-info PathBuf verbatim as \
                 Option<&Path> (got {:?}, expected {:?})",
                b.on_info(),
                path.as_deref(),
            );
            assert_eq!(
                b.on_info(),
                b.on_info.as_deref(),
                "BehaviorSpec::on_info must byte-equal the \
                 raw .on_info.as_deref() field access across \
                 every value in the accept-set",
            );
        }
    }

    #[test]
    fn behavior_on_info_is_independent_of_peer_on_star_axes() {
        // Cross-axis independence pin: flipping only the `:on-info`
        // axis flips [`BehaviorSpec::on_info`] independently of every
        // peer `:on-*` axis (`:on-init` / `:on-call` / `:on-cast` /
        // `:on-state-change` / `:on-terminate`). A future silent detour
        // that re-derived the callback path from a peer axis (an
        // accidental `.on_cast`-collapse — the sibling asynchronous
        // fire-and-forget arm on the peer OTP dispatch triad, a
        // plausible confusion because the module-doc example shares one
        // `lib/handlers.lisp` file between `:on-call` / `:on-cast` /
        // `:on-info` — that would silently rebind the out-of-band-info
        // dispatch to the sibling asynchronous fire-and-forget slot's
        // callback) surfaces here as a build-time test failure.
        //
        // Peer of the sibling
        // `behavior_on_state_change_is_independent_of_peer_on_star_axes`
        // (9b4ecde) /
        // `behavior_on_init_is_independent_of_peer_on_star_axes`
        // (d66c702) /
        // `behavior_on_call_is_independent_of_peer_on_star_axes`
        // (156ddbe) /
        // `behavior_on_cast_is_independent_of_peer_on_star_axes`
        // (99616ac) cross-axis pins on the sibling `:on-state-change` /
        // `:on-init` / `:on-call` / `:on-cast` axes — each
        // accessor-lift closes exactly one axis and leaves every peer
        // axis unshifted.
        let base = BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            on_call: Some(PathBuf::from("lib/handlers.lisp")),
            on_cast: Some(PathBuf::from("lib/handlers.lisp")),
            on_state_change: Some(PathBuf::from("lib/migrations.lisp")),
            on_terminate: Some(PathBuf::from("lib/cleanup.lisp")),
            ..BehaviorSpec::default()
        };
        assert_eq!(base.on_info(), None);
        let with = BehaviorSpec {
            on_info: Some(PathBuf::from("lib/handlers.lisp")),
            ..base.clone()
        };
        assert_eq!(
            with.on_info(),
            Some(PathBuf::from("lib/handlers.lisp").as_path()),
            "BehaviorSpec::on_info must project the \
             :on-info axis independently of every peer :on-* \
             axis (got {:?})",
            with.on_info(),
        );
    }

    // ── per-`:behavior :on-terminate` accessor pins ────────────────

    #[test]
    fn behavior_on_terminate_returns_option_path_verbatim_across_permutations() {
        // Canonical per-`:behavior` `:on-terminate`
        // OTP-`gen_server:terminate/2`-shaped graceful-shutdown cleanup
        // callback-path scalar pin: [`BehaviorSpec::on_terminate`] must
        // return the `:behavior :on-terminate` typed `PathBuf` verbatim
        // as an `Option<&Path>`, borrowed from the raw `Option<PathBuf>`
        // field access across the three canonical shape-arms — `None`
        // (no callback declared — the runtime tears down the wasm
        // instance without dispatching any author-supplied cleanup
        // side-effect), `Some("lib/cleanup.lisp")` (the canonical
        // single-file shape the module-doc example uses, the
        // author-surface pins `lib/cleanup.lisp` as the reference
        // `:on-terminate` value), `Some("lib/lifecycle/terminate.lisp")`
        // (the per-lifecycle-arm sub-directory shape the
        // `theory/ABSORPTION-ROADMAP.md` M2.5 wasm-engine
        // callback-dispatch wire acknowledges).
        //
        // Peer of the sibling per-`:behavior`
        // [`BehaviorSpec::on_state_change`] (9b4ecde) /
        // [`BehaviorSpec::on_init`] (d66c702) /
        // [`BehaviorSpec::on_call`] (156ddbe) /
        // [`BehaviorSpec::on_cast`] (99616ac) /
        // [`BehaviorSpec::on_info`] (4846cef) `Option<&Path>` accessor
        // pins on the sibling `Option<PathBuf>`-return axes — sixth and
        // final `Option<&Path>`-return accessor on the M2 `:behavior`
        // slot family, closes the last unlifted per-`:behavior`
        // `Option<&Path>` scalar-value axis. Pins against a future
        // silent detour that re-derived the callback path from a peer
        // axis (an accidental `.on_init`-collapse that assumed the two
        // lifecycle-arm `Option<PathBuf>` axes carry the same value —
        // a plausible slip because both are the lifecycle-head /
        // lifecycle-tail bookends of the OTP `gen_server` lifecycle, so
        // the "run once per instance" semantics rhyme across the two
        // arms), a `None` → `Some(empty)` collapse (the canonical
        // `Option<PathBuf>` → `PathBuf::new()` footgun the
        // [`BehaviorError::EmptyPath`] validate arm guards on the peer
        // path-shape axis), or a per-arm variant swap that landed on
        // one consumer without the other.
        for path in [
            None,
            Some(PathBuf::from("lib/cleanup.lisp")),
            Some(PathBuf::from("lib/lifecycle/terminate.lisp")),
        ] {
            let b = BehaviorSpec {
                on_terminate: path.clone(),
                ..BehaviorSpec::default()
            };
            assert_eq!(
                b.on_terminate(),
                path.as_deref(),
                "BehaviorSpec::on_terminate must return the \
                 :behavior :on-terminate PathBuf verbatim as \
                 Option<&Path> (got {:?}, expected {:?})",
                b.on_terminate(),
                path.as_deref(),
            );
            assert_eq!(
                b.on_terminate(),
                b.on_terminate.as_deref(),
                "BehaviorSpec::on_terminate must byte-equal the \
                 raw .on_terminate.as_deref() field access across \
                 every value in the accept-set",
            );
        }
    }

    #[test]
    fn behavior_on_terminate_is_independent_of_peer_on_star_axes() {
        // Cross-axis independence pin: flipping only the `:on-terminate`
        // axis flips [`BehaviorSpec::on_terminate`] independently of
        // every peer `:on-*` axis (`:on-init` / `:on-call` / `:on-cast`
        // / `:on-info` / `:on-state-change`). A future silent detour
        // that re-derived the callback path from a peer axis (an
        // accidental `.on_init`-collapse — the sibling lifecycle-head
        // arm on the peer OTP dispatch lifecycle, a plausible confusion
        // because both are the lifecycle-bookend arms that run
        // once-per-instance rather than per-mailbox-turn — that would
        // silently rebind the graceful-tear-down dispatch to the
        // sibling instance-start slot's callback) surfaces here as a
        // build-time test failure.
        //
        // Peer of the sibling
        // `behavior_on_state_change_is_independent_of_peer_on_star_axes`
        // (9b4ecde) /
        // `behavior_on_init_is_independent_of_peer_on_star_axes`
        // (d66c702) /
        // `behavior_on_call_is_independent_of_peer_on_star_axes`
        // (156ddbe) /
        // `behavior_on_cast_is_independent_of_peer_on_star_axes`
        // (99616ac) /
        // `behavior_on_info_is_independent_of_peer_on_star_axes`
        // (4846cef) cross-axis pins on the sibling `:on-state-change` /
        // `:on-init` / `:on-call` / `:on-cast` / `:on-info` axes —
        // each accessor-lift closes exactly one axis and leaves every
        // peer axis unshifted, so the six-callback OTP `gen_server`
        // lifecycle the slot family models routes through one typed
        // dispatch per arm on the substrate primitive.
        let base = BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            on_call: Some(PathBuf::from("lib/handlers.lisp")),
            on_cast: Some(PathBuf::from("lib/handlers.lisp")),
            on_info: Some(PathBuf::from("lib/handlers.lisp")),
            on_state_change: Some(PathBuf::from("lib/migrations.lisp")),
            ..BehaviorSpec::default()
        };
        assert_eq!(base.on_terminate(), None);
        let with = BehaviorSpec {
            on_terminate: Some(PathBuf::from("lib/cleanup.lisp")),
            ..base.clone()
        };
        assert_eq!(
            with.on_terminate(),
            Some(PathBuf::from("lib/cleanup.lisp").as_path()),
            "BehaviorSpec::on_terminate must project the \
             :on-terminate axis independently of every peer :on-* \
             axis (got {:?})",
            with.on_terminate(),
        );
    }
}
