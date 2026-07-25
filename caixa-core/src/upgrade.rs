//! Erlang/OTP-style appup — declarative upgrade instructions per
//! prior caixa version. Composes with the `:behavior :on-state-change`
//! callback to deliver state migration during hot upgrades.
//!
//! See `theory/INSPIRATIONS.md` §II.4 for the prior-art frame.
//!
//! ```lisp
//! (defcaixa
//!   :nome   "hello-rio"
//!   :versao "0.2.0"
//!   :upgrade-from
//!     ((:from "0.1.0"
//!       :instructions ((:load-module "hello-rio")
//!                      (:state-change "lib/migrations/v01-to-v02.lisp")
//!                      (:soft-purge "hello-rio-old")))
//!      (:from "0.1.5"
//!       :instructions ((:load-module "hello-rio")
//!                      (:soft-purge "hello-rio-old")))))
//! ```
//!
//! Each `(:from <prior>)` block declares the upgrade path *from* that
//! version *to* the current `:versao`. wasm-operator picks the
//! matching block at upgrade time, runs the instructions in order,
//! and only swaps traffic to the new instance after all instructions
//! succeed (transactional upgrade). On any failure, the current
//! version stays load-bearing — a typed atomic upgrade.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One upgrade instruction. The set mirrors OTP's appup low-level
/// instructions: enough to express every common upgrade pattern,
/// few enough that the wasm-operator can implement each
/// deterministically.
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
    gen_platform::TypedDispatcher,
    gen_platform::Discriminant,
    gen_platform::IsVariant,
)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum UpgradeInstruction {
    /// Load a new wasm module alongside the current one — the analog
    /// of OTP's `code:load_module/1`. Both versions remain in memory
    /// after this instruction; in-flight requests stay on the old
    /// version, new requests route to the new version.
    LoadModule { module: String },

    /// Run a state-migration tatara-lisp file. Receives the old state
    /// + the prior version string; returns the new state. Analog of
    /// `gen_server:code_change/3`.
    StateChange { script: PathBuf },

    /// Wait for in-flight requests on a named module to drain, then
    /// GC it — the analog of `code:soft_purge/1`. Default cooldown is
    /// 60s; longer-running requests block the upgrade.
    SoftPurge { module: String },

    /// Discard a named module immediately, without waiting for
    /// drain — the analog of `code:purge/1`. Used when we don't
    /// care about in-flight callers (cron, oneShot).
    Purge { module: String },

    /// Fall back to a full restart for this entry. Used when a typed
    /// upgrade is impossible (e.g. wasm component world incompatible).
    Restart,
}

// Fleet-wide dispatcher-catalog registration. UpgradeInstruction is
// the OTP-style hot-upgrade primitive (load_module/code_change/
// soft_purge/purge/restart) — the first NON-ADAPTER consumer of
// gen-platform's typed-dispatcher catamorphism, satisfying the ★★
// "two classes of consumer" promotion criterion from
// theory/QUIRK-APPLIER.md §V.1.
//
// Operators query via:
//   gen dispatchers --from-catalog | jq '.[] | select(.label=="caixa.upgrade-instruction")'
//
// The substrate's lib/build/shared/fleet-catalog-coverage-test.nix
// adds an assertion row for this label on the next snapshot refresh.
gen_platform::register_dispatcher!("caixa.upgrade-instruction", UpgradeInstruction);

/// One upgrade entry: the *prior* version we're upgrading from, plus
/// the instruction sequence to execute.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeFromEntry {
    /// Semver of the *prior* version. Authored as a literal string;
    /// validated lazily by [`UpgradeFromEntry::validate`].
    pub from: String,

    /// Ordered list of instructions to execute. Empty list = "no-op
    /// upgrade" (rare; usually means only documentation changed).
    #[serde(default)]
    pub instructions: Vec<UpgradeInstruction>,
}

impl UpgradeFromEntry {
    /// Prior-versao semver-2 literal this entry declares an upgrade
    /// path *from* — the string the OTP-shape `release_handler:install_release/1`
    /// analog matches the running caixa's `:versao` against at hot-
    /// upgrade dispatch time to pick this entry's `:instructions`
    /// sequence. Returned byte-for-byte from the typed slot's own
    /// `String` storage; no cloning, no re-parsing.
    ///
    /// The M2 companion of the closed M3 mesh-slot scalar-accessor
    /// family — sibling in shape to [`crate::Membro::versao_requirement`]
    /// (a40b0e3), [`crate::Membro::nome`] (4a32abf), and the
    /// [`crate::WitContract::{source, destination, world_ref}`]
    /// (7f0fd43 / 0804823) / [`crate::Entrada::{hostname, destination}`]
    /// (11f3dfe / 6db982c) `&str` accessors already routing every
    /// per-mesh-slot-atom scalar-value axis through one typed dispatch
    /// on the substrate primitive — extended here onto the first per-
    /// M2-slot scalar-value axis. Every downstream consumer of the
    /// M2 `:upgrade-from :from` axis (the [`UpgradeFromEntry::validate`]
    /// SemVer-2 parse gate, the [`validate_upgrade_from`] cross-entry
    /// duplicate-detection re-parse assertion, the
    /// [`validate_upgrade_from_against_versao`] precedence gate,
    /// the [`validate_upgrade_from_against_behavior`] state-change-
    /// callback coherence gate, every per-arm error variant carrying
    /// the offending `:from` verbatim for `feira lint` rendering)
    /// now reads through this one accessor rather than open-coding
    /// `&self.from` / `&entry.from` / `self.from.clone()` /
    /// `entry.from.clone()`.
    ///
    /// A future extension of the axis (an M4 typed `:from`-range slot
    /// composing multiple prior versions into one entry, an operator-
    /// side pre-parsed [`semver::Version`] cache the accessor could
    /// materialize behind the same `&str` return contract, a per-
    /// cluster `:placement`-scoped prior-versao overlay the
    /// `caixa-operator` reconciles ahead of dispatch) migrates as a
    /// single caixa-core edit rather than a coordinated rewrite of
    /// the four validate-side call sites + every downstream error-
    /// variant carrying `:from`.
    #[must_use]
    pub fn prior_versao(&self) -> &str {
        &self.from
    }

    /// Substrate-canonical per-`:upgrade-from :instructions`
    /// OTP-appup migration-instruction-list slice-return accessor
    /// every per-entry instructions-list reader keys off — returns
    /// the author-declared `:instructions` list verbatim as a
    /// `&[UpgradeInstruction]` slice-view over the same backing
    /// buffer the raw `self.instructions.as_slice()` field access
    /// borrows from. Non-optional: an empty slice is the load-bearing
    /// "author declared `:instructions ()`" sentinel — the
    /// `Vec<UpgradeInstruction>::default()`-produced empty tail the
    /// [`UpgradeFromEntry::instructions`] field's own docstring already
    /// names as the "no-op upgrade" shape (a metadata-only upgrade
    /// entry — the operator's `:from`-match dispatch matches the entry
    /// but runs no instructions, advancing straight to the "traffic
    /// swap" step) and every peer within-entry cross-instruction gate
    /// no-ops against without allocating a new `Vec` per gate.
    ///
    /// The `:upgrade-from :instructions` slot carries the per-`:from`
    /// OTP-appup ordered instruction list the wasm-operator's hot-
    /// upgrade dispatch materializes one per-instruction runtime
    /// primitive from — the Erlang/OTP appup's per-`{from, to,
    /// UpgradeInstructions, DowngradeInstructions}` entry's
    /// `UpgradeInstructions` list (`code:load_module/1` /
    /// `gen_server:code_change/3` / `code:soft_purge/1` /
    /// `code:purge/1` / `restart_new_emulator` — see INSPIRATIONS
    /// §II.4), projected through the tatara-lisp
    /// `:upgrade-from ((:from … :instructions …))` author surface
    /// onto a typed `Vec<UpgradeInstruction>` whose per-element
    /// variant is [`UpgradeInstruction::LoadModule`] /
    /// [`UpgradeInstruction::StateChange`] /
    /// [`UpgradeInstruction::SoftPurge`] / [`UpgradeInstruction::Purge`]
    /// / [`UpgradeInstruction::Restart`]. Every downstream consumer
    /// that fans on the per-entry instruction list keys off this
    /// slice (the [`UpgradeFromEntry::validate`] per-instruction
    /// shape-check fan-out, the seven paired within-entry cross-
    /// instruction gates [`Self::validate_restart_exclusive`] /
    /// [`Self::validate_state_change_ordering`] /
    /// [`Self::validate_purge_ordering`] /
    /// [`Self::validate_state_change_before_cleanup`] /
    /// [`Self::validate_load_singularity`] /
    /// [`Self::validate_state_change_singularity`] /
    /// [`Self::validate_cleanup_singularity`], the layout-side
    /// [`crate::layout::StandardLayout`]'s per-`:state-change`
    /// script-existence fan-out
    /// ([`crate::layout::LayoutError::MissingEntry`]'s
    /// `LAYOUT_MISSING_ENTRY_KIND_UPGRADE_SCRIPT` arm), the cross-slot
    /// [`validate_upgrade_from_against_behavior`] gate's per-entry
    /// `:state-change`-instruction detection loop, every future
    /// wasm-operator (M2.5) per-`:from`-match hot-upgrade dispatch's
    /// per-instruction runtime-primitive fan-out, every future M4
    /// `mesh.pleme.io/v1alpha1/Caixa` CR materializer's per-entry
    /// upgrade-plan admission-webhook fan-out).
    ///
    /// Prior to this lift the `.instructions` `Vec<UpgradeInstruction>`
    /// was accessed inline at nine production sites across
    /// `caixa-core/src/upgrade.rs` and `caixa-core/src/layout.rs` —
    /// the [`UpgradeFromEntry::validate`] per-instruction shape-check
    /// fan-out (`for instr in &self.instructions`), the paired
    /// [`Self::validate_restart_exclusive`] restart-count / other-kind
    /// projections + `.len()` probe (three raw-access sites in one
    /// gate), the [`Self::validate_state_change_ordering`] /
    /// [`Self::validate_purge_ordering`] /
    /// [`Self::validate_state_change_before_cleanup`] /
    /// [`Self::validate_load_singularity`] /
    /// [`Self::validate_state_change_singularity`] /
    /// [`Self::validate_cleanup_singularity`] within-entry cross-
    /// instruction gate traversal heads, the peer
    /// [`validate_upgrade_from_against_behavior`] cross-slot
    /// composition gate's `for instr in &entry.instructions`
    /// per-entry `:state-change` detection loop, and the
    /// [`crate::layout::StandardLayout`]-side
    /// `for instr in &entry.instructions` per-`:state-change`
    /// script-existence fan-out — nine open-coded field-accesses
    /// that expressed no compile-time link back to the typed slot.
    /// A future extension of the `:instructions` axis to a richer
    /// author surface (a per-cluster overlay the operator pins
    /// through a future `:upgrade-from :instructions-overrides` slot
    /// so a canary cluster runs a `(:state-change …)` before the
    /// production fleet does, a per-tenant instruction-list overlay
    /// the M4 CR materializer resolves per-CR to inject cluster-
    /// specific `(:soft-purge …)` cooldown adjustments, a promotion
    /// of the plain `Vec<UpgradeInstruction>` to a richer
    /// `{static, dynamic}` partition once virtual-actor-style
    /// dynamic-instruction composition (an operator-derived
    /// `(:load-module …)` sequence computed from the running
    /// module set at upgrade time) comes into typed scope, a
    /// per-instruction pre-condition scalar the future adaptive-
    /// upgrade engine reads to bias per-instruction retry
    /// strategy) would have had to be threaded through all nine
    /// open-coded copies in lockstep or one consumer would silently
    /// disagree with the peers on which instruction sequence a
    /// given `:upgrade-from` entry resolves to — the per-
    /// instruction shape-check reading the raw slot while the
    /// paired within-entry ordering gates read an operator-resolved
    /// slot would silently split the build-time per-entry gate
    /// cohort from the layout-side script-existence gate + the
    /// cross-slot behavior-composition gate + the runtime hot-
    /// upgrade dispatch, a nine-consumer split across the seven
    /// within-entry cross-instruction gates + the layout invariant +
    /// the cross-slot composition gate far from the source
    /// `caixa.lisp` with no field naming the instruction-sequence-
    /// drift root cause. Lifting the resolution rule to a typed
    /// method on the substrate primitive means every downstream
    /// consumer of the per-entry OTP-appup instruction-list surface
    /// reaches for exactly one typed dispatch — the resolver's
    /// accept-set migrates as a unit on any future axis addition.
    ///
    /// Fifth slice-return (`&[T]`) accessor on any M2 or M3 typed
    /// slot — sibling to the seed M2
    /// [`crate::SupervisorSpec::children`] (bc92bce) `&[ChildSpec]`
    /// accessor on the peer per-`:supervisor` static-child-list
    /// `Vec`-carry axis, the M3 [`crate::Placement::clusters`]
    /// (a6e18d7) `&[String]` accessor on the peer per-`:placement`
    /// distribution-target-list `Vec`-carry axis, the M3
    /// [`crate::AplicacaoSpec::membros`] (6c77e36) `&[Membro]`
    /// accessor on the peer per-`:membros` node-list `Vec`-carry
    /// axis, and the M3 [`crate::AplicacaoSpec::contratos`]
    /// (0dcc926) `&[WitContract]` accessor on the peer per-
    /// `:contratos` edge-list `Vec`-carry axis. This lift closes the
    /// last unlifted `Vec`-carry axis on any M2 or M3 typed slot in
    /// the substrate — the four peer axes named in the
    /// [`crate::SupervisorSpec::children`] seed docstring
    /// (`Placement::clusters`, `AplicacaoSpec::membros`,
    /// `AplicacaoSpec::contratos`, `UpgradeFromEntry::instructions`)
    /// are now all closed. The per-`UpgradeFromEntry` type carried
    /// two axes: the scalar `Copy`-return
    /// [`UpgradeFromEntry::prior_versao`] (75d27a8) on the
    /// `:from` axis, and now the slice-return
    /// [`UpgradeFromEntry::instructions`] on the peer
    /// `:instructions` axis. Named `instructions()` to match the
    /// storage field's name verbatim and the tatara-lisp
    /// author-surface term (`:instructions`) the field's own
    /// docstring already carries; the accessor's identity maps
    /// onto the canonical OTP-appup vocabulary the
    /// [`crate::upgrade`] module doc already reaches for ("runs
    /// the instructions in order"). Returns `&[UpgradeInstruction]`
    /// (not `&Vec<UpgradeInstruction>`) because every downstream
    /// consumer of the instruction list treats it as a read-only
    /// sequence — the slice-view is the narrowest borrow that
    /// supports every present + roadmapped consumer (`.iter()`,
    /// `.len()`, `.filter(...).count()`) without leaking the
    /// backing `Vec`'s grow/push/reserve surface that no consumer
    /// of the typed view reaches for (the storage-side `Vec`
    /// remains reachable through the `pub instructions` field for
    /// the mutation-carrying `Serialize`/`Deserialize` derive
    /// round-trip and per-test fixture-mutation paths).
    #[must_use]
    pub fn instructions(&self) -> &[UpgradeInstruction] {
        self.instructions.as_slice()
    }

    /// Verify the `:from` field is a valid semver, every instruction's
    /// typed shape, the within-entry `(:restart)`-exclusivity invariant
    /// (an entry containing `(:restart)` must contain exactly one
    /// `(:restart)` and nothing else — see
    /// [`Self::validate_restart_exclusive`]), the within-entry
    /// state-change-ordering invariant (every `(:state-change …)` must
    /// be preceded by a `(:load-module …)` — see
    /// [`Self::validate_state_change_ordering`]), the within-entry
    /// purge-ordering invariant (every `(:soft-purge …)` / `(:purge …)`
    /// must be preceded by a `(:load-module …)` — see
    /// [`Self::validate_purge_ordering`]), the within-entry
    /// state-change-before-cleanup ordering invariant (no
    /// `(:state-change …)` may appear after any `(:soft-purge …)` /
    /// `(:purge …)` — see
    /// [`Self::validate_state_change_before_cleanup`]), the within-
    /// entry load-singularity invariant (no module appears as the
    /// target of `(:load-module …)` more than once — see
    /// [`Self::validate_load_singularity`]), the within-entry
    /// state-change-singularity invariant (no script appears as the
    /// target of `(:state-change …)` more than once — see
    /// [`Self::validate_state_change_singularity`]), and the within-
    /// entry cleanup-singularity invariant (no module appears as the
    /// target of `(:soft-purge …)` or `(:purge …)` more than once
    /// total — see [`Self::validate_cleanup_singularity`]).
    pub fn validate(&self) -> Result<(), UpgradeError> {
        use semver::Version;
        Version::parse(self.prior_versao()).map_err(|e| UpgradeError::FromInvalid {
            from: self.prior_versao().to_string(),
            reason: e.to_string(),
        })?;
        // Per-instruction typed shape: kind-tagged `:module` /
        // `:script` value-shape gates fire here, *before* the
        // within-entry restart-exclusivity gate below — so a
        // malformed-shape diagnostic on a Module/Script-bearing
        // instruction surfaces with its narrower self-locating
        // wording (`ModuleEmpty`, `ModuleInvalid`, `EmptyScript`,
        // `AbsoluteScript`, `ParentEscapeScript`) rather than
        // collapsing two unrelated authoring errors into a single
        // exclusivity diagnostic. Same empty-first cascade discipline
        // every peer DNS-1123 / path-shape gate inside this module
        // uses (`validate_module`'s ModuleEmpty arm precedes the
        // DNS-1123 predicate; `validate` on `StateChange` consults
        // the lifted `is_sandboxed_relative_path` shape gate first).
        // Route the per-instruction shape-check fan-out through the
        // lifted [`Self::instructions`] slice-return accessor rather
        // than the raw `self.instructions` field access — first of
        // nine paired production consumers of the per-`:upgrade-from
        // :instructions` OTP-appup migration-instruction-list surface
        // that now key off exactly one typed dispatch on the substrate
        // primitive.
        for instr in self.instructions() {
            instr.validate()?;
        }
        self.validate_restart_exclusive()?;
        self.validate_state_change_ordering()?;
        self.validate_purge_ordering()?;
        self.validate_state_change_before_cleanup()?;
        self.validate_load_singularity()?;
        self.validate_state_change_singularity()?;
        self.validate_cleanup_singularity()?;
        Ok(())
    }

    /// Reject `:upgrade-from :instructions` lists that carry
    /// `(:restart)` alongside any other instruction, or that carry
    /// more than one `(:restart)`. The valid Restart-bearing shape is
    /// exactly `((:restart))` — a single `Restart` as the entry's
    /// whole instructions list.
    ///
    /// Per [`UpgradeInstruction::Restart`]'s doc comment, `(:restart)`
    /// is the *fallback* for an entry whose typed upgrade is
    /// impossible (wasm component-model world incompatibility,
    /// irreversible state shape change). The fallback is terminal by
    /// construction: the operator restarts the pod and the new version
    /// comes up fresh, so any other instructions in the same entry
    /// are dead code in both directions — either the typed sequence
    /// would have succeeded and `(:restart)` is unreached, or it
    /// wouldn't and the typed instructions are dead because the
    /// operator restarts anyway. Two canonical authoring footguns
    /// close here:
    ///
    ///   - `((:load-module …) (:state-change …) (:restart))` — the
    ///     "I'll try the typed path *then* restart anyway" footgun.
    ///     There is no coherent OTP-shaped semantic for this: if the
    ///     typed sequence succeeds, the trailing restart discards the
    ///     work that just succeeded (defeating the whole point of
    ///     declaring it); if it fails, the restart is never reached
    ///     because the entry already failed.
    ///   - `((:restart) (:restart))` — multiple `Restart` variants in
    ///     one entry. The fallback is a single semantic; repeating it
    ///     is at best redundant, at worst suggests the author thought
    ///     the second one would re-trigger after the first.
    ///
    /// Same within-entry exclusivity discipline OTP's `relup` enforces
    /// at the `restart_new_emulator | restart_emulator` instruction
    /// boundary — those instructions are terminal in the upgrade
    /// script (`systools(3)` rejects sequences that continue past
    /// them); pleme-io lifts the same shape to a build-time gate,
    /// matching the CAIXA-SDLC §III "build errors, not runtime
    /// surprises" frame.
    ///
    /// Same within-entry cross-instruction discipline the
    /// [`crate::AplicacaoSpec::validate_placement`] strategy ↔
    /// shard-key partition (934bc58) and
    /// [`validate_upgrade_from_against_versao`]'s `:from` ↔ `:versao`
    /// precedence partition (de7ab1a) apply on cross-slot axes — now
    /// extended onto the first within-list cross-instruction axis on
    /// the `:upgrade-from` typed slot.
    fn validate_restart_exclusive(&self) -> Result<(), UpgradeError> {
        // Route the paired restart-count / instructions-len / other-
        // kind projections through the lifted [`Self::instructions`]
        // slice-return accessor rather than the raw `self.instructions`
        // field access — three raw-access sites in one gate collapse
        // onto exactly one typed dispatch on the substrate primitive.
        //
        // The paired positive / negated `Self::Restart` arm-discriminator
        // predicates route through the `gen_platform::IsVariant`
        // derive-generated [`UpgradeInstruction::is_restart`] rather than
        // the raw `matches!(i, UpgradeInstruction::Restart)` /
        // `!matches!(i, UpgradeInstruction::Restart)` open-coded pattern-
        // matches — same closed-set-typed-enum arm-discriminator dispatch
        // discipline the sibling [`crate::CaixaKind`] `IsVariant` derive
        // (f5bba80) extended onto its ten `caixa.kind() == CaixaKind::X`
        // / `!= CaixaKind::X` production sites in the substrate's own
        // layout invariant verifier + typed-view projection gates,
        // extended here onto the last unlifted `matches!`-based
        // arm-discriminator axis on the [`UpgradeInstruction`] closed-set
        // typed enum. A future sixth `UpgradeInstruction` arm (an
        // adaptive-upgrade-shaped `AwaitReadiness` gate the M2.5
        // wasm-operator's hot-upgrade runtime could adopt to bracket the
        // typed instruction sequence against a per-cluster readiness
        // probe, a `Downgrade` variant OTP's `relup` acknowledges on the
        // reverse axis, a `CanaryTraffic` split-traffic variant the M4 CR
        // materializer could resolve per-CR) migrates as a single
        // enum-declaration edit — the derive auto-generates the paired
        // `.is_<new_arm>()` predicate; every consumer inherits the new
        // arm on the next re-derive, rather than the two `matches!` sites
        // here having to be threaded through in lockstep.
        let instructions = self.instructions();
        let restart_count = instructions.iter().filter(|i| i.is_restart()).count();
        if restart_count == 0 {
            return Ok(());
        }
        if restart_count == 1 && instructions.len() == 1 {
            return Ok(());
        }
        let other_kinds: Vec<&'static str> = instructions
            .iter()
            .filter(|i| !i.is_restart())
            .map(UpgradeInstruction::lisp_form)
            .collect();
        Err(UpgradeError::RestartNotExclusive {
            from: self.prior_versao().to_string(),
            restart_count,
            other_kinds,
        })
    }

    /// Reject an entry whose `(:state-change …)` is not preceded by a
    /// `(:load-module …)` in the same `:instructions` list.
    ///
    /// `StateChange` is the `gen_server:code_change/3` analog
    /// ([`UpgradeInstruction::StateChange`] doc; INSPIRATIONS §II.4):
    /// it runs the migration script that folds the *old* state into the
    /// shape the *new* code expects. In OTP, `code_change/3` is invoked
    /// in the context of the newly-loaded code — `release_handler`
    /// always loads the new module before running the advanced update
    /// that triggers the callback. caixa decomposes that into two
    /// explicit instructions (`LoadModule` brings the new version up
    /// "alongside the current one"; `StateChange` migrates the state),
    /// and the module doc pins that the operator "runs the instructions
    /// in order" and only swaps traffic after all succeed. So a
    /// `:state-change` with no preceding `:load-module` migrates state
    /// into code that was never loaded — the migration script runs while
    /// the only resident version is still the *old* one, which expects
    /// the *old* state. Two authoring footguns close here:
    ///
    ///   - `((:state-change "…"))` — the "I wrote the migration but
    ///     forgot to load the new module" footgun. The new code that
    ///     defines the new state representation (and that the migration
    ///     output is destined for) never comes up; the operator runs
    ///     the script against the old code and either no-ops or corrupts
    ///     live state.
    ///   - `((:state-change "…") (:load-module "…"))` — the
    ///     right-instructions-wrong-order footgun. Because the operator
    ///     executes in declared order, the migration runs *before* the
    ///     new code is resident, then the load brings up code expecting
    ///     already-migrated state that the just-run script produced
    ///     against the old version's shape. The canonical order is
    ///     `(:load-module …) (:state-change …) (:soft-purge …)`
    ///     (module doc example).
    ///
    /// Same within-entry cross-instruction discipline as
    /// [`Self::validate_restart_exclusive`] (the `(:restart)` terminal-
    /// exclusivity gate it runs beside): both reject an
    /// `:instructions` list whose instructions are individually
    /// well-shaped but jointly incoherent, at the typed build surface
    /// rather than as a runtime surprise. Runs *after*
    /// `validate_restart_exclusive` so a `((:state-change …)
    /// (:restart))` shape still surfaces the more-fundamental
    /// `RestartNotExclusive` (a valid `(:restart)` entry is `(:restart)`
    /// alone, so no Restart-bearing entry reaches this gate carrying a
    /// `StateChange`).
    fn validate_state_change_ordering(&self) -> Result<(), UpgradeError> {
        let mut loaded = false;
        for instr in self.instructions() {
            match instr {
                UpgradeInstruction::LoadModule { .. } => loaded = true,
                UpgradeInstruction::StateChange { script } if !loaded => {
                    return Err(UpgradeError::StateChangeWithoutPriorLoad {
                        from: self.prior_versao().to_string(),
                        script: script.clone(),
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Reject an entry whose `(:soft-purge …)` or `(:purge …)` is not
    /// preceded by a `(:load-module …)` in the same `:instructions` list.
    ///
    /// `SoftPurge` and `Purge` are the `code:soft_purge/1` /
    /// `code:purge/1` analogs (INSPIRATIONS §II.4): they remove the
    /// *old* module from memory after the new one is resident. OTP's
    /// two-phase code load is `code:load_module/1` *then*
    /// `code:soft_purge/1` — load the new version alongside the old
    /// (both in memory, new requests route to new), then purge the old
    /// after in-flight callers drain. caixa decomposes that into two
    /// explicit instructions (`LoadModule` brings the new version up
    /// "alongside the current one", per [`UpgradeInstruction::LoadModule`]
    /// doc; `SoftPurge` "waits for in-flight requests on a named module
    /// to drain, then GC it", per [`UpgradeInstruction::SoftPurge`] doc),
    /// and the module doc pins that the operator "runs the instructions
    /// in order". So a `:soft-purge` / `:purge` with no preceding
    /// `:load-module` purges old code while the only resident version is
    /// still the *same* old code, leaving the upgrade entry asking the
    /// operator to drain or discard the live module with no replacement
    /// resident. Two authoring footguns close here:
    ///
    ///   - `((:soft-purge "…"))` / `((:purge "…"))` — the "I wrote the
    ///     cleanup but forgot to load the new module" footgun. The new
    ///     code never comes up alongside; the operator either drains the
    ///     old version to nothing (`SoftPurge`) or discards it outright
    ///     mid-request (`Purge`), with no replacement to route in-flight
    ///     or future requests to.
    ///   - `((:soft-purge "…") (:load-module "…"))` /
    ///     `((:purge "…") (:load-module "…"))` — the right-instructions-
    ///     wrong-order footgun. Because the operator executes in declared
    ///     order, the cleanup runs *before* the new code is resident,
    ///     leaving a window during which neither version is available;
    ///     the canonical order is `(:load-module …) (:state-change …)
    ///     (:soft-purge …)` (module doc example).
    ///
    /// Same within-entry cross-instruction discipline as
    /// [`Self::validate_state_change_ordering`] (the `:state-change`-
    /// ordering gate it runs beside): both close the same load-before-X
    /// post-condition on the OTP appup ordering contract, now extending
    /// the typed coverage from "new code resident before its state
    /// migration runs" to "new code resident before the old code is
    /// drained or discarded" — the second half of OTP's two-phase code
    /// load. Runs *after* `validate_state_change_ordering` so an entry
    /// like `((:state-change …) (:soft-purge …))` surfaces the more-
    /// fundamental `StateChangeWithoutPriorLoad` first (both instructions
    /// are load-less, but state-change is the load-bearing semantic — the
    /// purge is meaningless either way without a preceding load, so the
    /// author should see the migration-side diagnostic first).
    fn validate_purge_ordering(&self) -> Result<(), UpgradeError> {
        let mut loaded = false;
        for instr in self.instructions() {
            match instr {
                UpgradeInstruction::LoadModule { .. } => loaded = true,
                UpgradeInstruction::SoftPurge { module } | UpgradeInstruction::Purge { module }
                    if !loaded =>
                {
                    return Err(UpgradeError::PurgeWithoutPriorLoad {
                        from: self.prior_versao().to_string(),
                        kind: instr.lisp_form(),
                        module: module.clone(),
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Reject an entry whose `(:state-change …)` appears after any
    /// `(:soft-purge …)` / `(:purge …)` in the same `:instructions`
    /// list — completing the canonical OTP appup `code:load_module/1`
    /// → `gen_server:code_change/3` → `code:soft_purge/1` ordering
    /// chain on the typed `:upgrade-from` slot.
    ///
    /// `StateChange` is the `gen_server:code_change/3` analog
    /// ([`UpgradeInstruction::StateChange`] doc; INSPIRATIONS §II.4
    /// verbatim: "State migration uses `gen_server:code_change/3` …
    /// migrate state from v0.1.0 shape to current shape"). The
    /// callback's input is the *prior* version's state shape, which
    /// only exists while the prior code is still resident — the running
    /// `gen_server` processes hold the v0.1.0 state, and the operator's
    /// dispatch invokes `code_change/3` to fold that state into the
    /// current shape. `SoftPurge` / `Purge` are the `code:soft_purge/1`
    /// / `code:purge/1` analogs ([`UpgradeInstruction::SoftPurge`] /
    /// [`UpgradeInstruction::Purge`] docs): they drain or discard the
    /// *old* module after the new one is resident. The operator runs
    /// instructions in declared order (module doc), so a cleanup ahead
    /// of a state-change discards the prior code before the migration
    /// fold runs against the state it held — the canonical OTP error
    /// mode "`code_change/3` invoked on a purged module" the
    /// `release_handler` enforces by always emitting the migration
    /// callback before the soft-purge step.
    ///
    /// `systools`-generated `.relup` files always emit `code_change`
    /// before `soft_purge` for this reason; the appup cookbook's
    /// canonical pattern (`[{load_module, m}, {update, m, soft},
    /// {soft_purge, m}]`) places the migration-triggering `update`
    /// strictly between the load and the cleanup. The caixa module
    /// doc pins the same canonical order verbatim — `(:load-module
    /// …) (:state-change …) (:soft-purge …)` — and this gate makes
    /// that ordering a structural property at build time. Three
    /// authoring footguns close here:
    ///
    ///   - `((:load-module "x") (:soft-purge "x-old") (:state-change
    ///     "lib/m.lisp"))` — the right-instructions-wrong-order
    ///     footgun on the migrate ↔ cleanup axis. Because the operator
    ///     executes in declared order, the cleanup drains the v0.1.0
    ///     module to nothing before the migration callback runs, and
    ///     the script either no-ops (no v0.1.0 state left to fold) or
    ///     crashes (`code_change/3` invoked on an unloaded version).
    ///     The canonical order is `(:load-module …) (:state-change
    ///     …) (:soft-purge …)` (module doc example).
    ///   - `((:load-module "x") (:purge "x-old") (:state-change
    ///     "lib/m.lisp"))` — same shape on the more catastrophic
    ///     `:purge` variant. The immediate-discard semantic destroys
    ///     v0.1.0 state mid-request; the trailing migration script
    ///     has nothing to fold from and the `gen_server` processes that
    ///     held v0.1.0 state were killed by the `:purge`.
    ///   - `((:load-module "x") (:soft-purge "x-old") (:state-change
    ///     "lib/m1.lisp") (:soft-purge "y-old"))` — the "migration
    ///     sandwiched between two cleanups" footgun. The first
    ///     cleanup discards v0.1.0; the migration runs against
    ///     drained state; the second cleanup is irrelevant. The first
    ///     cleanup → state-change boundary is the load-bearing defect
    ///     surfaced.
    ///
    /// Same within-entry cross-instruction discipline as
    /// [`Self::validate_state_change_ordering`] (the load → state-
    /// change ordering gate it runs after) and
    /// [`Self::validate_purge_ordering`] (the load → cleanup ordering
    /// gate it runs after): all three close one boundary of the OTP
    /// canonical sequence `code:load_module/1` →
    /// `gen_server:code_change/3` → `code:soft_purge/1`. The
    /// state-change-ordering gate closes the load → migrate boundary;
    /// the purge-ordering gate closes the load → cleanup boundary;
    /// this gate closes the migrate → cleanup boundary, completing
    /// the typed coverage of the canonical sequence. Runs *after*
    /// [`Self::validate_purge_ordering`] (and therefore after
    /// [`Self::validate_state_change_ordering`]) so an entry like
    /// `((:soft-purge "x-old") (:state-change "lib/m.lisp"))` —
    /// which violates *both* the purge-without-load gate and this
    /// state-change-after-cleanup gate — surfaces the more-
    /// fundamental `PurgeWithoutPriorLoad` first (the missing-load
    /// defect is load-bearing; once a coherent `(:load-module …)`
    /// precedes both, the migrate ↔ cleanup ordering becomes the
    /// next live defect). Runs *before* the per-instruction-class
    /// singularity gates ([`Self::validate_load_singularity`],
    /// [`Self::validate_state_change_singularity`],
    /// [`Self::validate_cleanup_singularity`]) so an entry like
    /// `((:load-module "x") (:soft-purge "x-old") (:state-change
    /// "lib/m.lisp") (:state-change "lib/m.lisp"))` — which violates
    /// *both* this ordering gate and the state-change-singularity
    /// gate — surfaces the ordering defect first; the canonical
    /// "ordering before singularity" precedence the peer
    /// `validate_state_change_ordering` / `validate_purge_ordering`
    /// gates already establish.
    ///
    /// Detection: linear scan of the instructions list with a
    /// `prior_cleanup: Option<(module, kind)>` sticky-once latch
    /// recording the first cleanup encountered; on any subsequent
    /// `StateChange` the gate fires with the script + the prior
    /// cleanup's kind/module. Diagnostic-order pin: the first
    /// colliding state-change-after-cleanup pair surfaces, not the
    /// last — mirrors every peer ordering gate's first-collision
    /// posture ([`Self::validate_state_change_ordering`] returns on
    /// the first `StateChange` without prior load,
    /// [`Self::validate_purge_ordering`] on the first cleanup
    /// without prior load).
    fn validate_state_change_before_cleanup(&self) -> Result<(), UpgradeError> {
        let mut prior_cleanup: Option<(&str, &'static str)> = None;
        for instr in self.instructions() {
            match instr {
                UpgradeInstruction::SoftPurge { module } | UpgradeInstruction::Purge { module } => {
                    if prior_cleanup.is_none() {
                        prior_cleanup = Some((module.as_str(), instr.lisp_form()));
                    }
                }
                UpgradeInstruction::StateChange { script } => {
                    if let Some((prior_module, prior_kind)) = prior_cleanup {
                        return Err(UpgradeError::StateChangeAfterCleanup {
                            from: self.prior_versao().to_string(),
                            script: script.clone(),
                            prior_cleanup_kind: prior_kind,
                            prior_cleanup_module: prior_module.to_string(),
                        });
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Reject an entry whose `:instructions` list names the same module
    /// as the target of more than one cleanup instruction (`:soft-purge`
    /// or `:purge`) in total — set-not-multiset on the (cleanup-class,
    /// module) axis, narrowed to the cleanup class.
    ///
    /// `SoftPurge` and `Purge` are the `code:soft_purge/1` /
    /// `code:purge/1` analogs (INSPIRATIONS §II.4 verbatim: "1.
    /// `code:load_module/1` — load v2 alongside v1 … 2.
    /// `code:soft_purge/1` — wait until no process is running v1, then
    /// discard. (`code:purge/1` kills v1 immediately if you don't
    /// care.)"). The author picks *one* cleanup semantic per old
    /// module — `:soft-purge` (preferred: waits for in-flight callers
    /// to drain) or `:purge` (when the drain isn't possible) — and the
    /// operator runs that one in declared order alongside any other
    /// distinct-module cleanups. systools-generated `.relup` files
    /// always emit at most one purge per module for this reason; any
    /// retry / fallback decision is the operator's job on
    /// instruction failure, not authored into the entry. Three
    /// authoring footguns close here:
    ///
    ///   - `((:load-module "x") (:soft-purge "x-old") (:soft-purge "x-old"))`
    ///     — the "I copy-pasted the cleanup line twice" footgun. The
    ///     second `:soft-purge` is a no-op (the module is already gone
    ///     after the first drain-and-discard) or undefined depending
    ///     on the operator's handling of a non-resident-module purge
    ///     request; either way the second instruction carries no
    ///     observable semantic, far from the source caixa.lisp.
    ///   - `((:load-module "x") (:soft-purge "x-old") (:purge "x-old"))`
    ///     — the "soft-then-hard fallback" footgun. The author wrote
    ///     "drain, and if drain didn't clean it up, force-discard",
    ///     but the operator runs instructions unconditionally in
    ///     declared order — the `:purge` fires whether the
    ///     `:soft-purge` already discarded the module or not, so the
    ///     fallback semantic the author imagined is missing; the
    ///     pair is incoherent (drain *and* force-discard semantics
    ///     on one module is two contradictory dispositions). The
    ///     operator's failure-handling surface is its own
    ///     responsibility: if `:soft-purge` doesn't drain within its
    ///     cooldown the operator escalates, not the author's entry.
    ///   - `((:load-module "x") (:purge "x-old") (:soft-purge "x-old"))`
    ///     — same shape on the reversed ordering. The `:purge`
    ///     discards immediately; the trailing `:soft-purge` has no
    ///     module to drain.
    ///
    /// Same within-entry exclusivity discipline as
    /// [`Self::validate_restart_exclusive`] (the `(:restart)` terminal-
    /// exclusivity gate it joins on the per-module cleanup axis): both
    /// reject an `:instructions` list whose instructions are
    /// individually well-shaped but jointly incoherent on a chosen
    /// semantic axis (restart-fallback for the whole entry there;
    /// cleanup-semantic for one module here), at the typed build
    /// surface rather than as a runtime surprise. Runs *after*
    /// [`Self::validate_purge_ordering`] (the load-before-cleanup
    /// ordering gate) so an entry like `((:soft-purge "x-old")
    /// (:soft-purge "x-old"))` surfaces the more-fundamental
    /// `PurgeWithoutPriorLoad` first (both cleanups are load-less, and
    /// the missing-load defect is the load-bearing one — the duplicate
    /// is meaningless either way without the preceding load).
    ///
    /// Same set-not-multiset discipline applied to every peer
    /// duplicate-target axis: `:children :caixa` (dbf50a9 —
    /// `SupervisorError::DuplicateChildCaixa`), `:membros :caixa`
    /// (4bb3f3d — `AplicacaoError::MembroDuplicate`), `:contratos`
    /// (5dbcfaf — `AplicacaoError::ContratoDuplicate`), `:placement
    /// :clusters` (c7c7799 — `AplicacaoError::PlacementClusterDuplicate`),
    /// `:entrada :paths` (eb3456d — `AplicacaoError::EntradaPathDuplicate`),
    /// and `:upgrade-from :from` ([`UpgradeError::DuplicateFrom`]).
    /// Each closes the same authoring footgun: a Vec authoring surface
    /// that silently accepts duplicate entries and renders the "second
    /// wins" (or "operator processes both, second is a no-op or
    /// errors") shape downstream, far from the source caixa.lisp.
    /// This gate extends the discipline onto the within-entry
    /// instruction-target axis — duplicate cleanup targets *within*
    /// one `:upgrade-from` entry — the peer of the cross-entry
    /// duplicate-`:from` axis at one level of nesting deeper.
    ///
    /// Detection: linear scan of the instructions list collecting
    /// the (module, kind) pair from every `SoftPurge` / `Purge`
    /// encountered; on the second occurrence of any module the gate
    /// fires with the prior kind and the colliding kind in declaration
    /// order. Diagnostic-order pin: the first colliding pair surfaces,
    /// not the last — mirrors
    /// [`validate_upgrade_from`]'s
    /// `validate_upgrade_from_duplicate_diagnostic_names_second_collision`
    /// posture (the first detected collision wins) and every peer
    /// duplicate gate's first-collision discipline.
    fn validate_cleanup_singularity(&self) -> Result<(), UpgradeError> {
        let mut seen: Vec<(&str, &'static str)> = Vec::new();
        for instr in self.instructions() {
            let (module, kind) = match instr {
                UpgradeInstruction::SoftPurge { module } => (
                    module.as_str(),
                    crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
                ),
                UpgradeInstruction::Purge { module } => (
                    module.as_str(),
                    crate::render::M2_UPGRADE_INSTRUCTION_KIND_PURGE,
                ),
                _ => continue,
            };
            if let Some(prior_idx) = seen.iter().position(|(m, _)| *m == module) {
                let prior_kind = seen[prior_idx].1;
                return Err(UpgradeError::DuplicateCleanup {
                    from: self.prior_versao().to_string(),
                    module: module.to_string(),
                    kinds: vec![prior_kind, kind],
                });
            }
            seen.push((module, kind));
        }
        Ok(())
    }

    /// Reject an entry whose `:instructions` list names the same module
    /// as the target of more than one `(:load-module …)` instruction —
    /// set-not-multiset on the `LoadModule` axis.
    ///
    /// `LoadModule` is the `code:load_module/1` analog (INSPIRATIONS
    /// §II.4 verbatim: "1. `code:load_module/1` — load v2 alongside v1;
    /// new code is 'current', old code is 'old'."). The instruction
    /// brings the new wasm component up resident alongside the old
    /// one so the operator can route new traffic to the new code
    /// while in-flight callers drain on the old — and the operator's
    /// dispatch table reads the module *name* (a caixa name) to bind
    /// the component, so two `(:load-module "x")` instructions in one
    /// entry ask the operator to re-bind the same component twice.
    /// `systools`-generated `.relup` files emit at most one
    /// `load_module` per module per upgrade step for this reason; the
    /// second load has no observable semantic relative to the first
    /// (the component is already resident). Three authoring footguns
    /// close here:
    ///
    ///   - `((:load-module "x") (:load-module "x"))` — the "I
    ///     copy-pasted the load line twice" footgun. The second
    ///     `:load-module` re-reads the same module name and re-binds
    ///     the same wasm component — a no-op in both directions
    ///     (no new code becomes resident; no old code is purged) —
    ///     and any cleanup / migration the author intended for a
    ///     *distinct* module is silently absent from the entry.
    ///   - `((:load-module "x") (:load-module "x") (:state-change …))`
    ///     — the "I meant to load two distinct modules" typo. The
    ///     author intended `((:load-module "x") (:load-module "y"))`
    ///     but renamed both to "x" (or copied the first line and
    ///     forgot to change the module). The migration runs against
    ///     code that's resident only on one module name, and the
    ///     second module the author imagined was being loaded never
    ///     comes up at all — far from the source caixa.lisp.
    ///   - `((:load-module "x") (:load-module "x") (:soft-purge "x-old"))`
    ///     — same shape with a trailing cleanup. The duplicate load
    ///     is dead code; the cleanup still fires correctly, masking
    ///     the load-side duplication as a silently-passing entry.
    ///
    /// Same within-entry exclusivity discipline as
    /// [`Self::validate_cleanup_singularity`] (the per-module cleanup-
    /// singularity gate this runs beside) on the sibling
    /// `LoadModule` axis: both reject an `:instructions` list whose
    /// instructions are individually well-shaped but jointly
    /// incoherent on a per-module-per-class basis (load-once for the
    /// load axis here; cleanup-once for the cleanup axis there), at
    /// the typed build surface rather than as a runtime surprise.
    /// Runs *after* [`Self::validate_purge_ordering`] (the load-
    /// before-cleanup ordering gate) so an entry like
    /// `((:state-change "m.lisp") (:load-module "x") (:load-module "x"))`
    /// surfaces the more-fundamental `StateChangeWithoutPriorLoad`
    /// first (the missing-load defect is load-bearing — the migration
    /// runs against unloaded code; the duplicate is meaningless either
    /// way without the preceding load). Runs *before*
    /// [`Self::validate_cleanup_singularity`] so an entry like
    /// `((:load-module "x") (:load-module "x") (:soft-purge "y-old")
    /// (:soft-purge "y-old"))` surfaces `DuplicateLoadModule` first —
    /// the load axis precedes the cleanup axis in the canonical OTP
    /// sequence (`code:load_module/1` then `code:soft_purge/1`) and
    /// in [`UpgradeInstruction`] declaration order (`LoadModule`
    /// before `SoftPurge`/`Purge`), so the load-side singularity is
    /// the load-bearing diagnostic when both fire.
    ///
    /// Same set-not-multiset discipline applied to every peer
    /// duplicate-target axis: `:children :caixa` (dbf50a9 —
    /// `SupervisorError::DuplicateChildCaixa`), `:membros :caixa`
    /// (4bb3f3d — `AplicacaoError::MembroDuplicate`), `:contratos`
    /// (5dbcfaf — `AplicacaoError::ContratoDuplicate`), `:placement
    /// :clusters` (c7c7799 — `AplicacaoError::PlacementClusterDuplicate`),
    /// `:entrada :paths` (eb3456d — `AplicacaoError::EntradaPathDuplicate`),
    /// `:upgrade-from :from` ([`UpgradeError::DuplicateFrom`]), and
    /// the per-module cleanup-target axis (9cedd8b —
    /// [`UpgradeError::DuplicateCleanup`]). This gate extends the
    /// discipline onto the within-entry `LoadModule` instruction-target
    /// axis — the third within-entry per-module singularity completing
    /// the load+cleanup pair across the OTP two-phase code-load
    /// contract.
    ///
    /// Detection: linear scan of the instructions list collecting the
    /// module name from every `LoadModule` encountered; on the second
    /// occurrence of any module the gate fires. Diagnostic-order pin:
    /// the first colliding occurrence surfaces, not the last — mirrors
    /// [`Self::validate_cleanup_singularity`]'s first-collision posture
    /// and every peer duplicate gate's first-collision discipline.
    fn validate_load_singularity(&self) -> Result<(), UpgradeError> {
        let mut seen: Vec<&str> = Vec::new();
        for instr in self.instructions() {
            let module = match instr {
                UpgradeInstruction::LoadModule { module } => module.as_str(),
                _ => continue,
            };
            if seen.contains(&module) {
                return Err(UpgradeError::DuplicateLoadModule {
                    from: self.prior_versao().to_string(),
                    module: module.to_string(),
                });
            }
            seen.push(module);
        }
        Ok(())
    }

    /// Reject an entry whose `:instructions` list names the same script
    /// as the target of more than one `(:state-change …)` instruction —
    /// set-not-multiset on the `StateChange` axis.
    ///
    /// `StateChange` is the `gen_server:code_change/3` analog
    /// (INSPIRATIONS §II.4: "State migration uses
    /// `gen_server:code_change/3`"). The instruction folds the *old*
    /// state into the shape the *new* code expects — a one-shot
    /// transition from one declared state representation to another.
    /// OTP's `release_handler:install_release/1` invokes `code_change/3`
    /// exactly once per upgrade per `gen_server`; `systools`-generated
    /// `.relup` files emit at most one `code_change` per `gen_server` per
    /// upgrade step for this reason. A second `(:state-change "m.lisp")`
    /// instruction targeting the same script in one entry re-runs the
    /// migration fold — at best a no-op (idempotent script masking a
    /// typo where the author intended two distinct scripts) and at
    /// worst silent state corruption (non-idempotent fold double-
    /// applied: an `add column` migration that runs twice, an
    /// `increment counter` that double-bumps, a `rename field` that
    /// renames-then-fails the second time). Three authoring footguns
    /// close here:
    ///
    ///   - `((:load-module "x") (:state-change "lib/m.lisp")
    ///     (:state-change "lib/m.lisp"))` — the "I copy-pasted the
    ///     migration line twice" footgun. The second `:state-change`
    ///     re-runs the same fold on the already-migrated state — a
    ///     no-op if the script is idempotent (dead code masking the
    ///     duplication) or state corruption if not (the migration's
    ///     pre-condition no longer holds because the post-condition is
    ///     already in place).
    ///   - `((:load-module "x") (:state-change "lib/m.lisp")
    ///     (:state-change "lib/m.lisp") (:soft-purge "x-old"))` — the
    ///     "duplicate migrate masked by trailing cleanup" footgun. The
    ///     cleanup still fires correctly, masking the migration-side
    ///     duplication as a silently-passing entry.
    ///   - `((:load-module "x") (:state-change "lib/m1.lisp")
    ///     (:state-change "lib/m1.lisp"))` — the "I meant to migrate
    ///     two distinct modules" typo. The author intended
    ///     `(:state-change "lib/m1.lisp") (:state-change "lib/m2.lisp")`
    ///     but renamed both to `m1.lisp` (or copy-pasted the first line
    ///     and forgot to change the script). The migration that should
    ///     have folded the second module's state never runs, far from
    ///     the source caixa.lisp.
    ///
    /// Same within-entry exclusivity discipline as
    /// [`Self::validate_load_singularity`] (the per-module load-
    /// singularity gate it runs after) and
    /// [`Self::validate_cleanup_singularity`] (the per-module cleanup-
    /// singularity gate it runs before) on the sibling `StateChange`
    /// axis: each rejects an `:instructions` list whose instructions
    /// are individually well-shaped but jointly incoherent on a per-
    /// instruction-class basis (load-once per module for the load
    /// axis; migrate-once per script for the migration axis here;
    /// cleanup-once per module for the cleanup axis), at the typed
    /// build surface rather than as a runtime surprise. Runs *after*
    /// [`Self::validate_load_singularity`] so an entry like
    /// `((:load-module "x") (:load-module "x") (:state-change
    /// "lib/m.lisp") (:state-change "lib/m.lisp"))` surfaces
    /// `DuplicateLoadModule` first — the load axis precedes the
    /// migration axis in the canonical OTP sequence
    /// (`code:load_module/1` then `gen_server:code_change/3`) and in
    /// [`UpgradeInstruction`] declaration order (`LoadModule` before
    /// `StateChange`), so the load-side singularity is the load-
    /// bearing diagnostic when both fire. Runs *before*
    /// [`Self::validate_cleanup_singularity`] so an entry like
    /// `((:load-module "x") (:state-change "lib/m.lisp") (:state-change
    /// "lib/m.lisp") (:soft-purge "y-old") (:soft-purge "y-old"))`
    /// surfaces `DuplicateStateChange` first — the migration axis
    /// precedes the cleanup axis in the canonical OTP sequence
    /// (`code:code_change/3` then `code:soft_purge/1`) and in
    /// [`UpgradeInstruction`] declaration order (`StateChange` before
    /// `SoftPurge`/`Purge`).
    ///
    /// Same set-not-multiset discipline applied to every peer
    /// duplicate-target axis: `:children :caixa` (dbf50a9 —
    /// `SupervisorError::DuplicateChildCaixa`), `:membros :caixa`
    /// (4bb3f3d — `AplicacaoError::MembroDuplicate`), `:contratos`
    /// (5dbcfaf — `AplicacaoError::ContratoDuplicate`), `:placement
    /// :clusters` (c7c7799 — `AplicacaoError::PlacementClusterDuplicate`),
    /// `:entrada :paths` (eb3456d — `AplicacaoError::EntradaPathDuplicate`),
    /// `:upgrade-from :from` ([`UpgradeError::DuplicateFrom`]), the
    /// per-module cleanup-target axis (9cedd8b —
    /// [`UpgradeError::DuplicateCleanup`]), and the per-module load-
    /// target axis (a503978 — [`UpgradeError::DuplicateLoadModule`]).
    /// This gate extends the discipline onto the within-entry
    /// `StateChange` instruction-target axis — the third within-entry
    /// per-instruction-class singularity, completing the OTP two-phase
    /// code-load + state-migration coverage triad
    /// (`code:load_module/1` → `gen_server:code_change/3` →
    /// `code:soft_purge/1`).
    ///
    /// Detection: linear scan of the instructions list collecting the
    /// script path from every `StateChange` encountered; on the second
    /// occurrence of any script the gate fires. Diagnostic-order pin:
    /// the first colliding occurrence surfaces, not the last — mirrors
    /// [`Self::validate_load_singularity`]'s and
    /// [`Self::validate_cleanup_singularity`]'s first-collision posture
    /// and every peer duplicate gate's first-collision discipline.
    fn validate_state_change_singularity(&self) -> Result<(), UpgradeError> {
        // Route the per-instruction `StateChange`-arm script-path
        // projection through the sibling lifted
        // [`UpgradeInstruction::declared_path`] `Option<&PathBuf>`
        // accessor rather than the raw
        // `match instr { UpgradeInstruction::StateChange { script } =>
        // script.as_path(), _ => continue }` open-coded pattern-match —
        // the third within-entry singularity gate's per-instruction
        // script-projection site now keys off exactly one typed
        // dispatch on the substrate primitive's `PathBuf`-carrying
        // axis, sibling to the four peer per-`UpgradeInstruction`
        // consumers ([`Self::validate`]'s per-`StateChange`
        // sandbox-path fan-out, the layout-side per-`StateChange`
        // script-existence fan-out at
        // `caixa-core/src/layout.rs:1017`, the cross-slot
        // [`validate_upgrade_from_against_behavior`] gate's
        // per-`StateChange` detection loop, the future wasm-operator's
        // per-`StateChange` runtime hook-dispatch) that already route
        // through `declared_path` / `declared_module`. Byte-equal
        // today (`declared_path` returns `Some(script)` iff the
        // instruction is [`UpgradeInstruction::StateChange`], per the
        // sibling `declared_path_only_for_state_change` pin), so a
        // duplicate `:state-change` script surfaces
        // `DuplicateStateChange` byte-identical to the pattern-match
        // shape. Same "one typed dispatch on the substrate primitive,
        // thin projections at each consumer" discipline the sibling
        // [`UpgradeInstruction::declared_module`] accessor established
        // (b13c4f9) on the peer `String`-carrying axis's per-variant
        // consumers, extended here onto the last unlifted
        // pattern-match on the `PathBuf`-carrying axis inside
        // `impl UpgradeFromEntry`.
        let mut seen: Vec<&std::path::Path> = Vec::new();
        for instr in self.instructions() {
            let Some(script) = instr.declared_path() else {
                continue;
            };
            let script = script.as_path();
            if seen.contains(&script) {
                return Err(UpgradeError::DuplicateStateChange {
                    from: self.prior_versao().to_string(),
                    script: script.to_path_buf(),
                });
            }
            seen.push(script);
        }
        Ok(())
    }
}

/// Validate a whole `:upgrade-from` list: per-entry typed shape via
/// [`UpgradeFromEntry::validate`] *and* the cross-entry graph-edge-set
/// invariant — at most one `(:from <prior>)` block per parsed semver.
///
/// OTP's appup picks at most one matching block to apply to the running
/// release (`release_handler:install_release/1` matches the loaded
/// `:from` against the currently-running version and executes the
/// associated instruction sequence; the wasm-operator picks the matching
/// block at upgrade time, per `upgrade.rs` module doc). Two blocks with
/// the same parsed-semver `:from` are an ambiguous edge in the typed
/// upgrade graph — the operator can pick either set deterministically,
/// but each set may carry different `LoadModule | StateChange |
/// SoftPurge | Purge | Restart` instructions, so the *chosen* path is
/// non-deterministic relative to the source caixa.lisp. The author's
/// intent is one path per prior version; the typed graph must enforce
/// that shape.
///
/// Same set-not-multiset discipline already applied to every peer
/// typed-graph axis: `:children :caixa` (dbf50a9 —
/// `SupervisorError::DuplicateChildCaixa`, `child_spec.id` is required-
/// unique per supervisor in OTP), `:membros :caixa` (4bb3f3d —
/// `AplicacaoError::MembroDuplicate`), `:contratos`
/// (5dbcfaf — `AplicacaoError::ContratoDuplicate`), `:placement
/// :clusters` (c7c7799 — `AplicacaoError::PlacementClusterDuplicate`),
/// and `:entrada :paths` (eb3456d — `AplicacaoError::EntradaPathDuplicate`).
/// Each closes the same authoring footgun: a Vec authoring surface that
/// silently accepts duplicate entries and renders the "second wins"
/// (or "operator picks arbitrarily") shape downstream, far from the
/// source caixa.lisp.
///
/// Duplicates are detected by [`semver::Version`] equality (the
/// crate's `PartialEq` compares the full identity — major.minor.patch +
/// pre-release + build metadata — so `1.0.0` and `1.0.0-rc.1` and
/// `1.0.0+build1` and `1.0.0+build2` are all distinct upgrade paths).
/// The conservative choice mirrors what the wasm-operator's
/// `:from`-match dispatch can see; collapsing build metadata to catch
/// a wider net of duplicates is a future tightening that requires
/// coordinating with the operator's match step.
///
/// Per-entry shape errors fire before the duplicate gate so the
/// diagnostic names the malformed slot (`FromInvalid`, `EmptyScript`,
/// `ModuleInvalid`, …) rather than collapsing two unrelated authoring
/// errors into a single duplicate diagnostic. Mirrors the
/// `*_invalid_fires_before_duplicate_check` order pins on every peer
/// axis ([`crate::SupervisorSpec::validate`],
/// [`crate::AplicacaoSpec::validate_membros`],
/// [`crate::AplicacaoSpec::validate_placement`]).
pub fn validate_upgrade_from(entries: &[UpgradeFromEntry]) -> Result<(), UpgradeError> {
    use semver::Version;
    let mut seen: Vec<Version> = Vec::with_capacity(entries.len());
    for entry in entries {
        entry.validate()?;
        // `entry.validate()` accepted this `:from`, so parse cannot
        // fail here — the FromInvalid arm above is the only gate
        // and both call `Version::parse(entry.prior_versao())`.
        let parsed = Version::parse(entry.prior_versao()).expect(
            "UpgradeFromEntry::validate must accept `:from` iff Version::parse does — keep the \
             two gates aligned",
        );
        if seen.contains(&parsed) {
            return Err(UpgradeError::DuplicateFrom {
                from: entry.prior_versao().to_string(),
            });
        }
        seen.push(parsed);
    }
    Ok(())
}

/// Reject `:upgrade-from` entries whose `:from` is not strictly less
/// than the caixa's current `:versao` (under SemVer-2 precedence — the
/// same ordering [`semver::Version::cmp`] implements, with build
/// metadata ignored per [SemVer §11][semver-11]).
///
/// The whole point of an `:upgrade-from :from "<prior>"` block is the
/// declarative answer to "given the wasm-operator is loading a node
/// running `<prior>`, how do I upgrade it to the *current* `:versao`?"
/// (`upgrade.rs` module doc, OTP appup `release_handler:install_release/1`
/// semantic). The operator's `:from`-match dispatch loads the
/// current `:versao` and matches the *running* version against each
/// entry's `:from`; an entry whose `:from >= :versao` is structurally
/// unreachable — the operator never runs a version greater than or
/// equal to the current `:versao` that it could then "upgrade *to*"
/// the current `:versao`. Two authoring footguns close here:
///
///   - `:from > :versao` (downgrade-shaped) — the canonical
///     "I copy-pasted from the next minor version and forgot to bump
///     `:versao`" / "I bumped `:versao` then reverted but left the
///     `:upgrade-from` entry behind" footgun. Until this gate landed
///     `(defcaixa :versao "0.1.5" :upgrade-from ((:from "0.2.0" …)))`
///     silently passed `feira build` and the wasm-operator's
///     `:from`-match dispatch would never fire on the entry — the
///     instructions sat dormant in the caixa.lisp forever, the
///     author's intent ("upgrade users coming from 0.2.0") permanently
///     unreached because they actually meant to bump `:versao`.
///
///   - `:from == :versao` (precedence-equal self-upgrade) — the
///     "I declared an upgrade from myself to myself" no-op the
///     operator's dispatch would either skip silently (no semantic
///     transition) or attempt and trivially "succeed" with no
///     observable state change. Includes the build-metadata-only
///     difference case (`:versao "0.2.0"`, `:from "0.2.0+build.1"`):
///     SemVer-2 precedence ignores build metadata so they compare
///     equal under [`semver::Version::cmp`] — the gate rejects this
///     even though [`UpgradeError::DuplicateFrom`] doesn't (the peer
///     gate uses derived `PartialEq` which keeps them distinct;
///     they're distinct dispatch keys but the same "from" version
///     for our purposes here).
///
/// Same cross-slot value-shape discipline as
/// [`crate::AplicacaoSpec::validate_placement`]'s strategy ↔ shard-key
/// partition (934bc58 — the typed partition between two declared
/// slots): one slot's value constrains the valid set of another's,
/// and the constraint is a structural property visible at validate
/// time. The validated set after this gate satisfies
/// `entry.from.parse::<Version>().unwrap() < versao.parse::<Version>().unwrap()`
/// for every entry, so the future operator-side hot-upgrade dispatch
/// step can reach for `entry.from` knowing the precedence relation
/// holds without re-deriving it from inline checks.
///
/// Silent-pass semantics on malformed inputs:
///
///   - When `versao` itself doesn't parse as semver, this gate
///     returns `Ok(())` silently — the narrower
///     [`crate::ManifestError::VersaoInvalid`] / [`UpgradeError::FromInvalid`]
///     diagnostics are the load-bearing surfaces for those failure
///     modes, and surfacing a `FromNotBeforeVersao` over an
///     unparseable `:versao` would mask the more actionable root
///     cause.
///   - Likewise, an entry whose `:from` itself doesn't parse falls
///     through to its narrower diagnostic surface
///     ([`UpgradeError::FromInvalid`]), which is expected to fire
///     via [`validate_upgrade_from`] *before* this gate runs at the
///     [`crate::LayoutInvariants`] call site.
///
/// [semver-11]: https://semver.org/#spec-item-11
pub fn validate_upgrade_from_against_versao(
    entries: &[UpgradeFromEntry],
    versao: &str,
) -> Result<(), UpgradeError> {
    use semver::Version;
    let Ok(current) = Version::parse(versao) else {
        // Malformed `:versao` is a separate gate (ManifestError::VersaoInvalid);
        // surfacing a precedence-relation diagnostic over an unparseable
        // top-level version would mask the more actionable root cause.
        return Ok(());
    };
    for entry in entries {
        // Per-entry shape — including a malformed `:from` — is gated
        // by [`validate_upgrade_from`] / [`UpgradeFromEntry::validate`]
        // upstream at the LayoutInvariants call site; an unparseable
        // `:from` here falls through silently to keep the
        // FromInvalid diagnostic load-bearing. Same fall-through
        // posture as the `versao` arm above.
        let Ok(prior) = Version::parse(entry.prior_versao()) else {
            continue;
        };
        if prior >= current {
            return Err(UpgradeError::FromNotBeforeVersao {
                from: entry.prior_versao().to_string(),
                versao: versao.to_string(),
            });
        }
    }
    Ok(())
}

/// Reject `:upgrade-from` entries whose `:instructions` list carries any
/// `(:state-change <script>)` instruction unless the caixa also declares
/// `:behavior :on-state-change` — the runtime callback the per-version
/// migration script is delivered through during hot upgrade.
///
/// The module doc on [`crate::upgrade`] pins the composition verbatim:
/// the `:upgrade-from` slot "Composes with the `:behavior :on-state-change`
/// callback to deliver state migration during hot upgrades." The peer
/// module doc on [`crate::BehaviorSpec::on_state_change`] mirrors the
/// promise from the callback side: the slot is the
/// `gen_server:code_change/3` analog — "receives old state + version,
/// returns new state. Composes with the `:upgrade-from` slot declared at
/// the Caixa root." OTP's `release_handler:install_release/1` realizes
/// the composition by invoking the running `gen_server`'s
/// `code_change/3` callback during the appup's `code_change` /
/// `update, m, soft` step — the appup's instruction triggers the
/// callback, the callback folds the prior-version state shape into the
/// current-version shape, and the operator advances to the next
/// instruction only after the callback returns successfully. caixa
/// decomposes the same composition into two typed slots: the per-version
/// migration logic lives in the `(:state-change "lib/migrations/v01-to-v02.lisp")`
/// instruction's `:script` (the `:upgrade-from` author surface), and the
/// runtime hook the operator dispatches the migration through lives in
/// the `:behavior :on-state-change` callback (the `:behavior` author
/// surface). A `:state-change` instruction declared without the callback
/// is half the composition: the per-version script the author wrote has
/// no runtime delivery path, and the operator's hot-upgrade dispatch
/// reaches for `caixa.behavior.on_state_change` at the migration step,
/// finds `None`, and either fails the upgrade mid-flight (the
/// transactional rollback the module doc names — "On any failure, the
/// current version stays load-bearing — a typed atomic upgrade") or
/// silently skips the migration depending on the operator's handling of
/// a missing callback, both far from the source caixa.lisp.
///
/// Two authoring footguns close here:
///
///   - `(:behavior ((:on-init …)))` + `(:upgrade-from ((:from "0.1.0"
///     :instructions ((:load-module "x") (:state-change "lib/m.lisp")
///     (:soft-purge "x-old")))))` — the "I declared the migration script
///     but forgot the callback" footgun. The author wrote the per-version
///     fold against the prior state shape, the typed `:upgrade-from`
///     slot validated every per-instruction shape + ordering + singularity
///     gate, and the missing callback only surfaces at upgrade time as
///     either a transactional rollback to the prior version (no progress
///     across the upgrade) or as a silently-skipped migration that leaves
///     v0.2.0 code running against unmigrated v0.1.0 state (corrupted
///     state shape).
///   - `:behavior` absent entirely + `:upgrade-from` carrying any
///     `:state-change` — the "I added the upgrade path but never declared
///     `:behavior`" footgun. `:behavior` is optional at the typed root
///     ([`crate::Caixa::behavior: Option<BehaviorSpec>`]) so the typed
///     `:upgrade-from` slot validates on its own merits, but a `Caixa`
///     with `behavior: None` and a `:state-change` instruction is the
///     same missing-callback shape — the operator's dispatch can't reach
///     a callback that doesn't exist.
///
/// Same cross-slot composition discipline as
/// [`validate_upgrade_from_against_versao`] (the `:from` ↔ `:versao`
/// precedence gate at the peer wire-up site): one slot's value
/// (`:from` < `:versao` there; `:state-change` declared here) constrains
/// the valid set of another's (the entry must be dispatchable there; the
/// callback must be declared here), and the constraint is a structural
/// property visible at validate time. The validated set after this gate
/// satisfies the documented composition: every `:state-change`
/// instruction the operator iterates at hot-upgrade time has a
/// corresponding `:on-state-change` callback declared on the same caixa,
/// so the future wasm-operator's hot-upgrade dispatch (the OTP
/// `release_handler` canonical-sequence loop) can reach for
/// `behavior.on_state_change` at the migration step knowing the
/// `Option<PathBuf>` is `Some(_)` without re-deriving the precondition
/// from inline checks.
///
/// Diagnostic-precedence:
///
///   - Runs *after* [`UpgradeFromEntry::validate`] (per-instruction
///     shape + the within-entry ordering / singularity gates) and
///     [`validate_upgrade_from`] (the cross-entry duplicate-`:from`
///     gate), so a malformed `:state-change` (`EmptyScript`,
///     `AbsoluteScript`, `ParentEscapeScript`) or an ill-ordered entry
///     (`StateChangeWithoutPriorLoad`, `StateChangeAfterCleanup`) or a
///     duplicate `:from` (`DuplicateFrom`) surfaces its narrower
///     self-locating diagnostic first — the canonical "per-instr-shape +
///     within-entry ordering + cross-entry uniqueness before
///     cross-slot composition" precedence the peer
///     `validate_upgrade_from_against_versao` gate establishes at the
///     same wire-up site. Without this precedence pin a malformed
///     `:state-change` instruction would surface this gate's
///     missing-callback diagnostic over the narrower
///     `EmptyScript` / `StateChangeWithoutPriorLoad`, masking the
///     load-bearing per-instruction defect with a cross-slot composition
///     diagnostic.
///   - Within the entries, walks the list in declaration order and
///     surfaces the *first* `:state-change` instruction encountered —
///     mirrors every peer first-collision diagnostic posture on this
///     module (`validate_state_change_ordering` returns on the first
///     `StateChange` without prior load,
///     `validate_load_singularity` returns on the second matching
///     module, etc.). A future entry's later `:state-change` doesn't
///     surface a different diagnostic — the missing callback is the same
///     defect regardless of which entry's `:state-change` exposes it.
///
/// Silent-pass semantics:
///
///   - Entries carrying no `:state-change` instruction (load-only,
///     cleanup-only, restart-only, or empty `:instructions`) leave the
///     gate vacuous — no per-version migration means no callback to
///     dispatch through, so the absence of `:on-state-change` is
///     coherent. Pins the gate's identity element on the empty-set side
///     of the composition.
///   - `behavior: None` is *not* a free pass when a `:state-change`
///     instruction is present — the same missing-callback shape as
///     `behavior: Some(_)` with `on_state_change: None`. The gate reads
///     `behavior.and_then(BehaviorSpec::on_state_change)` so both shapes
///     surface the same diagnostic.
pub fn validate_upgrade_from_against_behavior(
    entries: &[UpgradeFromEntry],
    behavior: Option<&crate::BehaviorSpec>,
) -> Result<(), UpgradeError> {
    if behavior
        .and_then(crate::BehaviorSpec::on_state_change)
        .is_some()
    {
        return Ok(());
    }
    for entry in entries {
        for instr in entry.instructions() {
            if let UpgradeInstruction::StateChange { script } = instr {
                return Err(UpgradeError::StateChangeWithoutOnStateChangeCallback {
                    from: entry.prior_versao().to_string(),
                    script: script.clone(),
                });
            }
        }
    }
    Ok(())
}

impl UpgradeInstruction {
    /// Kebab-case lisp form name for this instruction, used as the
    /// `:kind` tag in [`UpgradeError::ModuleEmpty`] /
    /// [`UpgradeError::ModuleInvalid`] diagnostics so the author can
    /// grep their caixa.lisp for `(:load-module …)` / `(:soft-purge …)`
    /// / `(:purge …)` and fix it in one edit. Mirrors the kebab-case
    /// slot tags `BehaviorError::EmptyPath` (b0c8389) and
    /// `UpgradeFromEntry`'s `:from` field already carry.
    #[must_use]
    const fn lisp_form(&self) -> &'static str {
        match self {
            Self::LoadModule { .. } => crate::render::M2_UPGRADE_INSTRUCTION_KIND_LOAD_MODULE,
            Self::StateChange { .. } => crate::render::M2_UPGRADE_INSTRUCTION_KIND_STATE_CHANGE,
            Self::SoftPurge { .. } => crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
            Self::Purge { .. } => crate::render::M2_UPGRADE_INSTRUCTION_KIND_PURGE,
            Self::Restart => crate::render::M2_UPGRADE_INSTRUCTION_KIND_RESTART,
        }
    }

    /// Validate the instruction's typed shape. Path existence is
    /// checked separately by [`crate::layout::StandardLayout`].
    ///
    /// The per-variant scalar the value-shape gates fire against is
    /// read through this method's two sibling accessors — the
    /// `String`-carrying axis via [`Self::declared_module`] (the
    /// `LoadModule` / `SoftPurge` / `Purge` variants unifying on their
    /// K8s DNS-1123-label `:module` reference) and the `PathBuf`-
    /// carrying axis via [`Self::declared_path`] (the `StateChange`
    /// variant's tatara-lisp `:script`) — rather than the per-arm
    /// `Self::LoadModule { module } | Self::SoftPurge { module } |
    /// Self::Purge { module }` pattern the module-axis previously
    /// open-coded and the per-arm `Self::StateChange { script }` the
    /// script-axis previously open-coded. Every scalar this enum
    /// carries now flows through one of the two `Option<&…>`
    /// accessors, so a future extension of either axis (a fifth
    /// module-bearing variant, an operator-side pre-parsed scalar
    /// cache the accessors materialize behind the same return
    /// contract, an M4 typed sub-slot the accessors could route
    /// alongside the existing scalar) migrates as a single edit on
    /// the accessor rather than a coordinated rewrite of every
    /// downstream value-shape gate. `Restart` (the only variant that
    /// carries neither scalar) falls through both `Option` checks and
    /// returns `Ok(())` — the terminal-fallback shape the
    /// [`Self::Restart`] variant doc pins.
    pub fn validate(&self) -> Result<(), UpgradeError> {
        if let Some(module) = self.declared_module() {
            return validate_module(self.lisp_form(), module);
        }
        if let Some(script) = self.declared_path() {
            // Delegate the four-arm cascade (empty / absolute /
            // parent-escape / non-`.lisp`-extension) to the lifted
            // [`crate::render::require_sandboxed_lisp_path`] helper —
            // same `Empty → Absolute → ParentEscape → NonLispExtension`
            // arm-ordering this method previously inlined verbatim,
            // now shared with [`crate::BehaviorSpec::validate`]'s
            // per-`:on-*`-callback gate so every author-supplied
            // tatara-lisp source path on every M2 typed slot consults
            // one gate, not two-and-counting verbatim copies of the
            // same four-arm cascade. Each closure wraps the tag in
            // the same `*Script` variant the original inline code
            // raised, so the diagnostic shape every caller depends
            // on (the `:state-change :script` self-locating error)
            // is preserved by construction. See
            // [`crate::render::require_sandboxed_lisp_path`] for the
            // smallest-scope-arm-fires-last ordering rationale.
            crate::render::require_sandboxed_lisp_path(
                script,
                || UpgradeError::EmptyScript,
                || UpgradeError::AbsoluteScript {
                    script: script.clone(),
                },
                || UpgradeError::ParentEscapeScript {
                    script: script.clone(),
                },
                || UpgradeError::NonLispExtensionScript {
                    script: script.clone(),
                },
            )?;
        }
        // `Restart` (the only variant with no `Option<&…>`-carrying
        // scalar) falls through both accessor gates and returns
        // `Ok(())` — the terminal-fallback shape.
        Ok(())
    }

    /// The `:module` scalar carried by this instruction — the
    /// K8s DNS-1123-label OTP-appup caixa-name reference every
    /// [`Self::LoadModule`] / [`Self::SoftPurge`] / [`Self::Purge`]
    /// variant declares against, and every author expects `feira lint`
    /// to name verbatim in per-instruction diagnostics. Returns `None`
    /// on [`Self::StateChange`] (which carries a `:script` — closed by
    /// the sibling [`Self::declared_path`]) and on [`Self::Restart`]
    /// (which carries no data at all, the OTP terminal-fallback
    /// shape).
    ///
    /// Sibling in shape to [`Self::declared_path`] on the second and
    /// final scalar-carrying axis of [`UpgradeInstruction`]:
    /// `declared_path` closes the `PathBuf`-carrying arm
    /// (`StateChange`); `declared_module` closes the `String`-carrying
    /// arms (`LoadModule` / `SoftPurge` / `Purge`). Every scalar the
    /// enum carries now routes through one of the two `Option<&…>`
    /// accessors — a caller that doesn't care which variant declared
    /// the scalar reads through one `if let Some(…)` rather than a
    /// per-variant pattern match. The pair is the enum-variant-
    /// unifying peer of the per-mesh-slot-atom scalar-accessor family
    /// on the M3 side ([`crate::WitContract::source`] /
    /// [`crate::WitContract::destination`] /
    /// [`crate::WitContract::world_ref`] closing `:contratos`;
    /// [`crate::Entrada::hostname`] / [`crate::Entrada::destination`]
    /// closing `:entrada`; [`crate::Membro::nome`] /
    /// [`crate::Membro::versao_requirement`] closing `:membros`) and
    /// on the M2 side ([`crate::UpgradeFromEntry::prior_versao`]
    /// closing per-entry `:from`; the [`crate::LimitsSpec`] /
    /// [`crate::BehaviorSpec`] closed families; the [`crate::ChildSpec`]
    /// closed OTP-shape supervisor family) — those peer accessors
    /// return a struct field verbatim; this pair unifies enum-
    /// variant-carried scalars into one accessor per typed axis.
    ///
    /// Byte-for-byte from the typed variant's own `String` storage;
    /// no cloning, no re-parsing. A future extension of the axis (an
    /// M4 typed sub-slot the module string is derived from, an
    /// operator-side pre-parsed caixa-name cache the accessor could
    /// materialize behind the same `&str` return contract, a fifth
    /// module-bearing OTP-appup variant the enum grows) migrates as
    /// a single caixa-core edit rather than a coordinated rewrite
    /// of every downstream module-axis consumer (currently
    /// [`Self::validate`]'s DNS-1123-label gate through
    /// [`validate_module`]; extensible to future consumers on the
    /// same axis without further per-variant match sites).
    #[must_use]
    pub fn declared_module(&self) -> Option<&str> {
        match self {
            Self::LoadModule { module } | Self::SoftPurge { module } | Self::Purge { module } => {
                Some(module.as_str())
            }
            Self::StateChange { .. } | Self::Restart => None,
        }
    }

    /// If the instruction references an on-disk path, return it —
    /// used by the layout checker to verify the path resolves.
    ///
    /// Sibling on the `PathBuf`-carrying axis to [`Self::declared_module`]
    /// on the `String`-carrying axis: `declared_path` closes the
    /// `StateChange` arm's `:script`; `declared_module` closes the
    /// `LoadModule` / `SoftPurge` / `Purge` arms' `:module`. Together
    /// they route every scalar this enum carries through one of two
    /// `Option<&…>` accessors, so [`Self::validate`]'s value-shape
    /// gates dispatch on the accessor return rather than a per-variant
    /// pattern match on the enum shape itself.
    #[must_use]
    pub fn declared_path(&self) -> Option<&PathBuf> {
        match self {
            Self::StateChange { script } => Some(script),
            _ => None,
        }
    }
}

/// Reject upgrade instruction `:module` values that aren't K8s
/// DNS-1123 labels. Thin wrapper around
/// [`crate::render::is_dns_1123_label`] that maps the shared
/// parser-shaped reason into the kind-tagged
/// [`UpgradeError::ModuleEmpty`] / [`UpgradeError::ModuleInvalid`]
/// diagnostics, so the author can grep their caixa.lisp for the
/// offending `(:<kind> <module>)` form and fix it in one edit.
///
/// The contract — the same DNS-1123 label rule the K8s apiserver
/// enforces on every `metadata.name` / Service name / label value the
/// module name lands in. Each upgrade instruction's `:module` is a
/// reference to a caixa name (the wasm-engine resolves it through the
/// same `ComputeUnit` registry the operator manages), so the value must
/// match every downstream apiserver-side schema: the per-Servico
/// `wasm.pleme.io/v1alpha1/ComputeUnit.metadata.name` the operator
/// creates, the `LABEL_PROGRAM` label value the wasm-engine matches
/// against the loaded-module table at hot-upgrade dispatch, and the
/// future `:upgrade-from`-driven `app-operator` rolling-load CR's
/// per-module reference axis. Same trajectory as `:children :caixa`
/// (31bfa43), `:membros :caixa` (3f9d7a0), and `:placement :clusters`
/// (6cbb900) onto the fourth DNS-1123-label-shaped identifier axis —
/// appup's `LoadModule | SoftPurge | Purge` `:module` references.
///
/// Empty input is rejected via the narrower [`UpgradeError::ModuleEmpty`]
/// variant before this predicate is consulted, mirroring
/// `validate_membro_caixa`'s empty-first cascade.
fn validate_module(kind: &'static str, module: &str) -> Result<(), UpgradeError> {
    // Routes through the shared
    // [`crate::render::require_valid_dns_1123_label`] gate the peer
    // name axes each land on. The `kind: &'static str` field flows
    // through both error variants so the diagnostic names which
    // per-instruction slot (`LoadModule` / `SoftPurge` / `Purge`) the
    // offending value came from.
    crate::render::require_valid_dns_1123_label(
        module,
        || UpgradeError::ModuleEmpty { kind },
        |reason| UpgradeError::ModuleInvalid {
            kind,
            module: module.to_string(),
            reason,
        },
    )
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum UpgradeError {
    #[error(
        ":upgrade-from :from {from:?} is not a valid SemVer-2 version: {reason} (the substrate \
         consumes this string as `semver::Version` — three-part `MAJOR.MINOR.PATCH` with optional \
         `-prerelease` and `+build`, the same shape every top-level `:versao` carries — across \
         every artifact derived from `:from`: the wasm-operator's `:from`-match dispatch loads \
         the running version through `semver::Version::parse` and matches it against each entry's \
         `:from`, so a malformed `:from` is structurally unreachable at dispatch time; use a \
         SemVer-2 literal like `\"0.1.0\"`, `\"0.2.0-rc.1\"`, or `\"1.0.0+build.42\"` — not a \
         git-tag-shape like `\"v0.1.0\"`, a docker-tag-shape like `\"latest\"`, a \
         requirement-shape like `\"^0.1\"`, or a four-part `\"0.1.0.0\"`)"
    )]
    FromInvalid { from: String, reason: String },
    #[error(
        "upgrade instruction `{kind}` :module is empty (every appup module reference \
         must name a caixa; use a non-empty caixa name like `\"hello-rio\"` or omit \
         the instruction entirely)"
    )]
    ModuleEmpty { kind: &'static str },
    #[error(
        "upgrade instruction `{kind}` :module {module:?} is not a valid DNS-1123 label: \
         {reason} (every appup module reference resolves to a caixa name, which lands \
         verbatim as a K8s `metadata.name` on the per-Servico ComputeUnit the operator \
         creates, the `LABEL_PROGRAM` label value the wasm-engine matches at hot-upgrade \
         dispatch, and every future `app-operator` rolling-load CR's per-module reference \
         axis; use a lowercase alphanumeric + hyphen identifier like `\"hello-rio\"` or \
         `\"cache-v2\"`)"
    )]
    ModuleInvalid {
        kind: &'static str,
        module: String,
        reason: String,
    },
    #[error("instruction's :script is empty")]
    EmptyScript,
    #[error(
        "instruction's :script {} is absolute — upgrade scripts must be relative to the caixa \
         root (Path::join would otherwise escape the project sandbox)",
        script.display()
    )]
    AbsoluteScript { script: PathBuf },
    #[error(
        "instruction's :script {} contains a `..` component — upgrade scripts must not traverse \
         above the caixa root",
        script.display()
    )]
    ParentEscapeScript { script: PathBuf },
    #[error(
        ":upgrade-from (:state-change {}) does not terminate in the `.lisp` extension — the M2.5 \
         wasm-engine instantiator reads every migration script as tatara-lisp source through \
         `tatara_lisp::read` at hot-upgrade migration time (the same downstream consumer the \
         peer `:behavior :on-*` axis routes through at instance-start time, c97815a), so any \
         other extension (`.txt`, `.rs`, `.lisp.bak`) or no-extension shape is structurally a \
         parser error far from the source caixa.lisp, with no field naming the offending \
         `(:state-change …)` instruction. Pin a relative path under the caixa root whose \
         terminating extension is lowercase-`.lisp` (e.g. `\"lib/migrations.lisp\"`, \
         `\"lib/migrations/v01-to-v02.lisp\"`).",
        script.display()
    )]
    NonLispExtensionScript { script: PathBuf },
    #[error(
        ":upgrade-from carries more than one `(:from {from:?})` entry — OTP appup picks at most \
         one matching block per running version (`release_handler:install_release/1` dispatches \
         on the loaded `:from` against the currently-running release), so two entries with the \
         same parsed semver are an ambiguous edge in the typed upgrade graph (the operator would \
         pick either set non-deterministically). Author one path per prior version; if two \
         distinct instruction sequences are needed, fold them into one ordered list under the \
         single matching `(:from {from:?} :instructions (…))` block."
    )]
    DuplicateFrom { from: String },
    #[error(
        ":upgrade-from `(:from {from:?})` is not strictly less than the caixa's current \
         `:versao {versao:?}` under SemVer-2 precedence — an upgrade block whose `:from` is \
         greater than or equal to the caixa's own version is structurally unreachable \
         (the wasm-operator's `:from`-match dispatch loads the current `:versao` and matches \
         the running version against each entry's `:from`; an entry whose `:from >= :versao` \
         is never reached because the operator never runs a version greater than or equal to \
         the current one that it could then upgrade *to* the current one). Bump the caixa's \
         `:versao` past {from:?} (the typical fix — you added the entry intending to upgrade \
         *to* a new version but forgot to bump `:versao`), drop the entry (if it's a stale \
         reference left over from a reverted `:versao` bump), or correct `:from` to a prior \
         version (if it's a typo). Pre-release values like `\"0.2.0-rc.1\"` are strictly less \
         than the corresponding release `\"0.2.0\"` under SemVer §11 precedence; build-metadata \
         values like `\"0.2.0+build.1\"` are equal to `\"0.2.0\"` under precedence and rejected \
         here as a self-upgrade no-op."
    )]
    FromNotBeforeVersao { from: String, versao: String },
    #[error(
        ":upgrade-from `(:from {from:?})` :instructions list violates the `(:restart)` \
         exclusivity invariant — an entry containing `(:restart)` must contain exactly one \
         `(:restart)` and nothing else (found {restart_count} `(:restart)` plus other \
         instruction(s): {other_kinds:?}). Per the UpgradeInstruction::Restart doc comment, \
         `(:restart)` is the fallback for an entry whose typed upgrade is impossible (wasm \
         component-model world incompatibility, irreversible state shape change), and the \
         fallback is terminal by construction (the operator restarts the pod and the new \
         version comes up fresh). Mixing the fallback with the typed sequence is dead code \
         in both directions: if the typed instructions would succeed, `(:restart)` is \
         unreached; if they wouldn't, the typed instructions are dead because the operator \
         restarts anyway. Author *either* a typed sequence (`(:load-module …) \
         (:state-change …) (:soft-purge …)`) *or* a single `((:restart))` — never both, \
         never repeated. If two distinct upgrade strategies are needed for the same prior \
         version, that is itself a typed-graph ambiguity (the operator's `:from`-match \
         dispatch picks exactly one block per running version) — keep the typed sequence; \
         the fallback restart is what the operator does on any typed-sequence failure \
         already."
    )]
    RestartNotExclusive {
        from: String,
        restart_count: usize,
        other_kinds: Vec<&'static str>,
    },
    #[error(
        ":upgrade-from `(:from {from:?})` runs `(:state-change {})` before any \
         `(:load-module …)` in its :instructions list — a state migration is the \
         gen_server:code_change/3 analog and must run in the context of the newly-loaded \
         code, but the operator executes instructions in declared order, so this migration \
         runs while the only resident version is still the prior one (which expects the \
         pre-migration state shape). Load the new module first: author the canonical \
         `(:load-module …) (:state-change {}) (:soft-purge …)` order so the new code is \
         resident before its state migration runs.",
        script.display(),
        script.display()
    )]
    StateChangeWithoutPriorLoad { from: String, script: PathBuf },
    #[error(
        ":upgrade-from `(:from {from:?})` runs `({kind} {module:?})` before any \
         `(:load-module …)` in its :instructions list — `:soft-purge` and `:purge` are the \
         code:soft_purge/1 / code:purge/1 analogs and must run after the new code is \
         resident alongside the old (OTP's two-phase code load: `code:load_module/1` \
         then `code:soft_purge/1`), but the operator executes instructions in declared \
         order, so this cleanup runs while the only resident version is still the same \
         old code (`:soft-purge` drains it to nothing; `:purge` discards it outright \
         mid-request), leaving no replacement to route in-flight or future requests \
         to. Load the new module first: author the canonical `(:load-module …) \
         (:state-change …) ({kind} {module:?})` order so the new code is resident \
         before the old code is drained or discarded."
    )]
    PurgeWithoutPriorLoad {
        from: String,
        kind: &'static str,
        module: String,
    },
    #[error(
        ":upgrade-from `(:from {from:?})` :instructions list targets module {module:?} with \
         more than one cleanup instruction ({kinds:?}) — `:soft-purge` and `:purge` are the \
         code:soft_purge/1 / code:purge/1 analogs (INSPIRATIONS §II.4: \"`code:soft_purge/1` — \
         wait until no process is running v1, then discard. (`code:purge/1` kills v1 immediately \
         if you don't care.)\"), and each module's old version is cleaned up by exactly one of \
         them: either drain-then-discard (`:soft-purge`) or immediate-discard (`:purge`), never \
         both, never repeated. systools-generated `.relup` files emit at most one purge per \
         module for this reason. A second cleanup on the same module is at best redundant (the \
         module is already gone after the first cleanup, so the second is a no-op or undefined \
         depending on the operator's handling of a non-resident-module purge request) and at \
         worst incoherent (mixing drain and discard semantics on one module suggests the author \
         wanted a fallback, but the operator runs declared instructions unconditionally — \
         fallback on cleanup failure is the operator's job, not authored into the entry). \
         Author one cleanup per module: prefer `(:soft-purge {module:?})` (waits for in-flight \
         callers to drain before GC); fall back to `(:purge {module:?})` only when the drain \
         can't complete (cron / oneShot / stuck callers). If two distinct old versions need \
         cleanup, name them distinctly (e.g. `(:soft-purge {module:?}) (:soft-purge \"…-older\")`)."
    )]
    DuplicateCleanup {
        from: String,
        module: String,
        kinds: Vec<&'static str>,
    },
    #[error(
        ":upgrade-from `(:from {from:?})` :instructions list loads module {module:?} more than \
         once — `:load-module` is the code:load_module/1 analog (INSPIRATIONS §II.4: \"1. \
         `code:load_module/1` — load v2 alongside v1; new code is 'current', old code is \
         'old'.\"), and the instruction binds the named wasm component once: the operator's \
         dispatch table reads the module name and brings up the corresponding component \
         alongside the running version. systools-generated `.relup` files emit at most one \
         `load_module` per module per upgrade step for this reason. A second `(:load-module \
         {module:?})` instruction has no observable semantic relative to the first (the \
         component is already resident) — either dead code (copy-pasted load line) or a typo \
         masking a distinct module the author intended to load alongside (renamed both to \
         {module:?} by mistake), leaving the second module silently absent from the entry. \
         Author one `(:load-module {module:?})` per old module per entry; if two distinct old \
         versions need loading alongside the running one, name them distinctly (e.g. \
         `(:load-module {module:?}) (:load-module \"…-v2\")`)."
    )]
    DuplicateLoadModule { from: String, module: String },
    #[error(
        ":upgrade-from `(:from {from:?})` :instructions list runs state migration {} more than \
         once — `:state-change` is the gen_server:code_change/3 analog (INSPIRATIONS §II.4: \
         \"State migration uses gen_server:code_change/3\"), and the script folds the prior-version \
         state shape into the current-version shape: a one-shot transition, not a step that \
         composes with itself. systools-generated `.relup` files emit at most one `code_change` \
         per gen_server per upgrade step for this reason; OTP's release_handler invokes the \
         callback exactly once. A second `(:state-change {})` instruction re-runs the same fold on \
         the already-migrated state — at best a no-op (idempotent script masking a typo where the \
         author intended two distinct migration scripts) and at worst silent state corruption \
         (non-idempotent fold double-applied: an `add column` that runs twice, an `increment \
         counter` that double-bumps, a `rename field` that renames-then-fails the second time). \
         Author one `(:state-change {})` per migration script per entry; if two distinct state \
         transitions are needed (e.g. one module's schema *and* another module's projection), \
         name them distinctly (e.g. `(:state-change {}) (:state-change \"lib/migrations/v01-to-v02-projection.lisp\")`).",
        script.display(),
        script.display(),
        script.display(),
        script.display()
    )]
    DuplicateStateChange { from: String, script: PathBuf },
    #[error(
        ":upgrade-from `(:from {from:?})` runs `(:state-change {})` after `({prior_cleanup_kind} \
         {prior_cleanup_module:?})` in its :instructions list — `:state-change` is the \
         gen_server:code_change/3 analog and folds the prior-version state shape into the \
         current shape, but the prior version's state only exists while the prior code is \
         still resident; `:soft-purge` and `:purge` are the code:soft_purge/1 / code:purge/1 \
         analogs and drain or discard that prior code. The operator executes instructions in \
         declared order, so a cleanup ahead of a state-change has already drained the prior \
         module to nothing (`:soft-purge`) or discarded it mid-request (`:purge`) by the time \
         the migration script runs, leaving the script either no-op (no prior-version state \
         left to fold) or crashing (`code_change/3` invoked on an unloaded version). The OTP \
         canonical sequence is `code:load_module/1` → `gen_server:code_change/3` → \
         `code:soft_purge/1`; the appup cookbook's recommended pattern is `[{{load_module, m}}, \
         {{update, m, soft}}, {{soft_purge, m}}]` with the migration-triggering `update` \
         strictly between load and cleanup. Author the canonical `(:load-module …) \
         (:state-change {}) ({prior_cleanup_kind} {prior_cleanup_module:?})` order so the \
         migration runs against the prior-version state before the cleanup drains it.",
        script.display(),
        script.display()
    )]
    StateChangeAfterCleanup {
        from: String,
        script: PathBuf,
        prior_cleanup_kind: &'static str,
        prior_cleanup_module: String,
    },
    #[error(
        ":upgrade-from `(:from {from:?})` declares `(:state-change {})` but the caixa does not \
         declare `:behavior :on-state-change` — the per-version migration script is the \
         gen_server:code_change/3 analog and the runtime hook it is delivered through during \
         hot upgrade is the `:on-state-change` callback. OTP's release_handler:install_release/1 \
         realizes the composition by invoking the running gen_server's code_change/3 callback \
         during the appup's `code_change` / `update, m, soft` step; caixa decomposes the same \
         composition into two typed slots, the per-version migration logic in this \
         `(:state-change …)` instruction's `:script` and the runtime dispatch hook in the \
         `:behavior :on-state-change` callback (the upgrade.rs module doc pins the composition \
         verbatim: \"Composes with the `:behavior :on-state-change` callback to deliver state \
         migration during hot upgrades\"). The missing callback leaves the per-version script \
         with no runtime delivery path: the operator's hot-upgrade dispatch reaches for the \
         callback at the migration step, finds it absent, and either fails the upgrade \
         mid-flight (the transactional rollback the module doc names — \"On any failure, the \
         current version stays load-bearing\") or silently skips the migration leaving the \
         new code running against unmigrated prior-version state. Add the callback: \
         `(:behavior ((:on-state-change \"lib/migrations.lisp\") …))` (the runtime delivery \
         path) alongside the existing `(:state-change {})` instruction (the per-version \
         script). If the upgrade truly carries no state migration, drop the `(:state-change \
         …)` instruction from the entry (a metadata-only upgrade — load + cleanup, no \
         migration — is the canonical shape).",
        script.display(),
        script.display()
    )]
    StateChangeWithoutOnStateChangeCallback { from: String, script: PathBuf },
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn entry(from: &str, instrs: Vec<UpgradeInstruction>) -> UpgradeFromEntry {
        UpgradeFromEntry {
            from: from.into(),
            instructions: instrs,
        }
    }

    #[test]
    fn round_trip_load_module() {
        let i = UpgradeInstruction::LoadModule {
            module: "hello-rio".into(),
        };
        let json = serde_json::to_string(&i).unwrap();
        assert!(json.contains("\"kind\":\"load-module\""));
        let back: UpgradeInstruction = serde_json::from_str(&json).unwrap();
        assert_eq!(i, back);
    }

    #[test]
    fn round_trip_all_variants() {
        let cases = vec![
            UpgradeInstruction::LoadModule { module: "x".into() },
            UpgradeInstruction::StateChange {
                script: PathBuf::from("lib/migrations.lisp"),
            },
            UpgradeInstruction::SoftPurge {
                module: "x-old".into(),
            },
            UpgradeInstruction::Purge {
                module: "x-old".into(),
            },
            UpgradeInstruction::Restart,
        ];
        for c in cases {
            let json = serde_json::to_string(&c).unwrap();
            let back: UpgradeInstruction = serde_json::from_str(&json).unwrap();
            assert_eq!(c, back);
        }
    }

    #[test]
    fn validate_accepts_well_formed() {
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule {
                    module: "hello-rio".into(),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/migrations/v01-to-v02.lisp"),
                },
                UpgradeInstruction::SoftPurge {
                    module: "hello-rio-old".into(),
                },
            ],
        );
        e.validate().unwrap();
    }

    #[test]
    fn validate_rejects_non_semver_from() {
        let e = entry("not-a-semver", vec![]);
        let err = e.validate().unwrap_err();
        assert!(
            matches!(err, UpgradeError::FromInvalid { ref from, .. } if from == "not-a-semver")
        );
    }

    #[test]
    fn from_invalid_diagnostic_carries_offending_from_and_reason() {
        // Diagnostic-shape pin: the error names the offending
        // `:upgrade-from :from` verbatim with a non-empty parser-shaped
        // reason, so a `feira lint` run can render the diagnostic
        // without re-parsing — the author can grep their caixa.lisp for
        // `:from "<value>"` and fix it in one edit. Mirrors the peer
        // `versao_invalid_diagnostic_carries_offending_versao` pin on
        // the sibling SemVer-2 axis (the top-level `:versao`), the
        // peer `membro_versao_invalid_diagnostic_carries_offending_value`
        // pin on `:membros :versao`, and the peer
        // `deps_invalid_diagnostic_carries_offending_value` pin on
        // `:deps :versao` — every SemVer-2-parsing slot's invalid
        // diagnostic is now structurally equivalent.
        let e = entry("v0.1.0", vec![]);
        let err = e.validate().unwrap_err();
        let UpgradeError::FromInvalid { from, reason } = err else {
            panic!("expected FromInvalid variant, got {err:?}");
        };
        assert_eq!(from, "v0.1.0");
        assert!(
            !reason.is_empty(),
            "FromInvalid `reason` must carry the parser's wording verbatim"
        );
    }

    #[test]
    fn prior_versao_returns_from_byte_equal_across_permutations() {
        // Byte-identity pin on the lifted `UpgradeFromEntry::prior_versao`
        // accessor across the SemVer-2 shape lattice every consumer
        // reaches through it — the numeric-triad canonical shape, a
        // pre-release build with a dotted identifier chain, a full-
        // metadata build, a large-magnitude triad, and the empty
        // string (which reaches this accessor unchanged before any
        // validate gate rejects it). Sibling to the peer
        // `membro_versao_requirement_returns_versao_byte_equal_across_permutations`
        // (a40b0e3) / `membro_nome_returns_caixa_byte_equal_across_permutations`
        // (4a32abf) pins on the sibling M3 mesh-slot scalar-accessor
        // family — extended here onto the first M2 slot scalar-value
        // axis. Any silent detour on the accessor (a `.to_string()`
        // + retained ownership shape, a canonicalization pass, a
        // trim-whitespace on the return path) surfaces as a byte-
        // inequality failure here rather than as a downstream error-
        // diagnostic drift.
        let cases = ["0.1.0", "0.2.0-rc.1", "1.0.0+build.42", "10.20.30", ""];
        for from in cases {
            let e = entry(from, vec![]);
            assert_eq!(
                e.prior_versao(),
                from,
                "prior_versao() must return the `:from` field byte-for-byte for {from:?}",
            );
            assert_eq!(
                e.prior_versao().len(),
                from.len(),
                "prior_versao() byte-length must equal the `:from` field's for {from:?}",
            );
        }
    }

    #[test]
    fn prior_versao_borrows_from_from_storage() {
        // Same-address pin: `UpgradeFromEntry::prior_versao` returns
        // a borrow into `self.from`'s heap allocation, never a fresh
        // owned copy. Guards against a future silent detour where
        // the accessor materializes a `Cow<'_, str>` / `String` /
        // `Rc<str>` intermediate — the return path stays zero-cost
        // even under a refactor that reshapes the storage. Sibling
        // to the peer `membro_versao_requirement_borrows_from_versao_storage`
        // (a40b0e3) / `membro_nome_borrows_from_caixa_storage`
        // (4a32abf) pins — extended onto the M2 slot's first
        // scalar-value axis.
        let e = entry("0.1.0", vec![]);
        assert!(
            std::ptr::eq(e.prior_versao().as_ptr(), e.from.as_ptr()),
            "prior_versao() must borrow from `self.from`'s storage, not allocate a fresh copy",
        );
    }

    #[test]
    fn validate_parses_prior_versao_through_lifted_accessor() {
        // Coherence pin between the accessor and the SemVer-2 parse
        // gate: every `:upgrade-from :from` value the validator
        // accepts (resp. rejects) must be identical to what
        // `Version::parse(entry.prior_versao())` accepts (resp.
        // rejects) — the two must remain in lockstep across the
        // shape lattice so `validate_upgrade_from`'s
        // `Version::parse(entry.prior_versao()).expect(...)` re-parse
        // assertion holds by construction. If a future extension of
        // `prior_versao` reshapes the return (a canonicalization
        // pass, a leading/trailing whitespace trim, an empty-to-
        // "0.0.0" fallback) it would either loosen the validator
        // (silently accepting shapes the parser rejects) or
        // tighten the parser's re-parse (silently panicking on
        // shapes the validator accepts) — this pin catches either
        // shift at caixa-core build time.
        let accepted = ["0.1.0", "0.2.0-rc.1", "1.0.0+build.42", "10.20.30"];
        for from in accepted {
            let e = entry(from, vec![]);
            e.validate().unwrap_or_else(|err| {
                panic!("validate() must accept {from:?} that Version::parse accepts, got {err:?}");
            });
            semver::Version::parse(e.prior_versao()).unwrap_or_else(|err| {
                panic!(
                    "Version::parse(prior_versao()) must accept {from:?} that validate() accepts, \
                     got {err:?}",
                );
            });
        }
        let rejected = ["", "v0.1.0", "0.1", "not-a-semver", "0.1.0.0"];
        for from in rejected {
            let e = entry(from, vec![]);
            assert!(
                matches!(e.validate(), Err(UpgradeError::FromInvalid { .. })),
                "validate() must reject {from:?} that Version::parse rejects",
            );
            assert!(
                semver::Version::parse(e.prior_versao()).is_err(),
                "Version::parse(prior_versao()) must reject {from:?} that validate() rejects",
            );
        }
    }

    #[test]
    fn validate_rejects_empty_module() {
        // Per-arm coverage: every Module-bearing variant surfaces the
        // kind-tagged `ModuleEmpty` diagnostic naming its lisp-form,
        // so the author can grep their caixa.lisp for `(:load-module
        // …)` / `(:soft-purge …)` / `(:purge …)` and fix it in one
        // edit — same self-locating shape `BehaviorError::EmptyPath`
        // (b0c8389) carries on the peer M2 typed slot.
        let cases: &[(UpgradeInstruction, &'static str)] = &[
            (
                UpgradeInstruction::LoadModule {
                    module: String::new(),
                },
                crate::render::M2_UPGRADE_INSTRUCTION_KIND_LOAD_MODULE,
            ),
            (
                UpgradeInstruction::SoftPurge {
                    module: String::new(),
                },
                crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
            ),
            (
                UpgradeInstruction::Purge {
                    module: String::new(),
                },
                crate::render::M2_UPGRADE_INSTRUCTION_KIND_PURGE,
            ),
        ];
        for (instr, expected_kind) in cases {
            assert_eq!(
                instr.validate().unwrap_err(),
                UpgradeError::ModuleEmpty {
                    kind: expected_kind
                },
                "empty :module on {instr:?} must surface as ModuleEmpty {{ kind: {expected_kind:?} }}"
            );
        }
    }

    #[test]
    fn validate_rejects_non_dns_1123_module() {
        // Every appup `:module` reference is a caixa name (the
        // wasm-engine resolves it through the same ComputeUnit
        // registry the operator manages), so the value-shape gate
        // matches the K8s apiserver-side DNS-1123 label rule. Sweep
        // the canonical authoring footguns — uppercase letters, `_`
        // separator, embedded `.`, leading/trailing `-`, an embedded
        // whitespace byte, the >63-byte UUID-shaped slug — across
        // every Module-bearing variant; each must surface as
        // `ModuleInvalid { kind, module, reason }` carrying the
        // offending value verbatim and the parser-shaped reason.
        type Build = fn(String) -> UpgradeInstruction;
        let footguns: &[&str] = &[
            "Hello-Rio",
            "hello_rio",
            "hello.rio",
            "-hello",
            "hello-",
            "hello rio",
            &"x".repeat(crate::render::DNS_1123_LABEL_MAX_LEN + 1),
        ];
        let variants: &[(Build, &'static str)] = &[
            (
                |m| UpgradeInstruction::LoadModule { module: m },
                crate::render::M2_UPGRADE_INSTRUCTION_KIND_LOAD_MODULE,
            ),
            (
                |m| UpgradeInstruction::SoftPurge { module: m },
                crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
            ),
            (
                |m| UpgradeInstruction::Purge { module: m },
                crate::render::M2_UPGRADE_INSTRUCTION_KIND_PURGE,
            ),
        ];
        for (build, expected_kind) in variants {
            for module in footguns {
                let instr = build((*module).to_string());
                let err = instr.validate().unwrap_err();
                match err {
                    UpgradeError::ModuleInvalid {
                        kind,
                        module: m,
                        reason,
                    } => {
                        assert_eq!(
                            kind, *expected_kind,
                            ":module footgun on {instr:?} must tag the lisp-form"
                        );
                        assert_eq!(
                            m, *module,
                            "ModuleInvalid must carry the offending value verbatim"
                        );
                        assert!(
                            !reason.is_empty(),
                            "ModuleInvalid reason must name the specific violation \
                             (the predicate's parser-shaped wording from \
                             `is_dns_1123_label`), got empty"
                        );
                    }
                    other => panic!("expected ModuleInvalid on {instr:?}, got {other:?}"),
                }
            }
        }
    }

    #[test]
    fn validate_accepts_canonical_module_names() {
        // Positive control: every documented authoring shape — bare
        // identifier, with hyphens, with digits, the
        // suffix-versioned alias `<nome>-old` `SoftPurge` typically
        // references — passes the gate. Drift here = a future
        // tighten that rejects any of these surfaces as a
        // test-failure at the predicate boundary, not piecemeal
        // across per-instruction call sites.
        let canonical: &[&str] = &[
            "hello-rio",
            "hello-rio-old",
            "cache",
            "cache-v2",
            "x",
            "a1",
            "0a",
            "abc-123-def",
        ];
        for module in canonical {
            UpgradeInstruction::LoadModule {
                module: (*module).to_string(),
            }
            .validate()
            .unwrap_or_else(|e| panic!("LoadModule {module:?} must pass, got {e:?}"));
            UpgradeInstruction::SoftPurge {
                module: (*module).to_string(),
            }
            .validate()
            .unwrap_or_else(|e| panic!("SoftPurge {module:?} must pass, got {e:?}"));
            UpgradeInstruction::Purge {
                module: (*module).to_string(),
            }
            .validate()
            .unwrap_or_else(|e| panic!("Purge {module:?} must pass, got {e:?}"));
        }
    }

    #[test]
    fn validate_empty_takes_precedence_over_invalid() {
        // Empty input is rejected via the narrower `ModuleEmpty`
        // diagnostic before the DNS-1123 predicate is consulted, so
        // a future tighten that adds another stage between the two
        // doesn't accidentally reorder the diagnostic precedence.
        // Mirrors the empty-first cascade on every peer DNS-1123
        // gate (`validate_membro_caixa`, `validate_placement_cluster`,
        // `SupervisorSpec::validate`'s child-name arm).
        let err = UpgradeInstruction::LoadModule {
            module: String::new(),
        }
        .validate()
        .unwrap_err();
        assert_eq!(
            err,
            UpgradeError::ModuleEmpty {
                kind: crate::render::M2_UPGRADE_INSTRUCTION_KIND_LOAD_MODULE
            }
        );
    }

    #[test]
    fn validate_rejects_empty_script() {
        let i = UpgradeInstruction::StateChange {
            script: PathBuf::new(),
        };
        assert_eq!(i.validate().unwrap_err(), UpgradeError::EmptyScript);
    }

    #[test]
    fn validate_rejects_absolute_script() {
        let i = UpgradeInstruction::StateChange {
            script: PathBuf::from("/etc/migrations.lisp"),
        };
        assert!(matches!(
            i.validate().unwrap_err(),
            UpgradeError::AbsoluteScript { .. }
        ));
    }

    #[test]
    fn validate_rejects_parent_escape_script() {
        let i = UpgradeInstruction::StateChange {
            script: PathBuf::from("../sibling/migrations.lisp"),
        };
        assert!(matches!(
            i.validate().unwrap_err(),
            UpgradeError::ParentEscapeScript { .. }
        ));
        // mid-path `..` is also caught
        let i2 = UpgradeInstruction::StateChange {
            script: PathBuf::from("lib/../../escaped.lisp"),
        };
        assert!(matches!(
            i2.validate().unwrap_err(),
            UpgradeError::ParentEscapeScript { .. }
        ));
    }

    // ── :upgrade-from :state-change :script `.lisp` extension gate ─
    // Mirrors the c97815a `BehaviorError::NonLispExtension` arm on
    // the peer `:behavior :on-*` tatara-lisp-source-path axis. Both
    // axes route through the same M2.5 wasm-engine `tatara_lisp::read`
    // consumer; the file-type contract is identical, so the per-axis
    // test grid is mirrored leg-for-leg.

    #[test]
    fn validate_rejects_no_extension_script() {
        // Fail-before-pass-after: the canonical "I declared the
        // migration script but forgot the `.lisp` extension"
        // authoring footgun (e.g. `(:state-change "lib/migrations")`).
        // The wasm-engine's `tatara_lisp::read` consumer needs a
        // file-type contract beyond the structural-shape gate; a
        // no-extension path past `is_sandboxed_relative_path` would
        // surface a parser-shaped diagnostic at hot-upgrade migration
        // time far from the source caixa.lisp.
        for relpath in ["lib/migrations", "migrations", "lib/handlers/migrate"] {
            let i = UpgradeInstruction::StateChange {
                script: PathBuf::from(relpath),
            };
            let err = i.validate().unwrap_err();
            assert!(
                matches!(&err, UpgradeError::NonLispExtensionScript { script: s }
                         if s == Path::new(relpath)),
                "no-extension script {relpath:?} must surface as NonLispExtensionScript \
                 carrying the offending path verbatim, got {err:?}"
            );
        }
    }

    #[test]
    fn validate_rejects_non_lisp_extension_script() {
        // Wrong-extension sweep across common authoring footguns: the
        // `.txt` / `.md` / `.json` / `.yaml` shapes an author might
        // drag in from the workspace tree, the `.rs` shape that an
        // IDE auto-complete might propose, the `.lisp.bak` shape an
        // editor might leave behind, and the `.lispx` near-miss that
        // a typo would produce. Each must surface as
        // `NonLispExtensionScript` carrying the offending path
        // verbatim — the wasm-engine's `tatara_lisp::read` consumer
        // rejects all of these at hot-upgrade migration time, and
        // the gate lifts that contract to validate time. Mirrors the
        // peer `BehaviorError::NonLispExtension` sweep (c97815a) on
        // the `:behavior :on-*` axis leg-for-leg — same downstream
        // consumer, same accepted set, same per-axis test grid.
        let footguns: &[&str] = &[
            "lib/migrations.rs",
            "lib/migrations.txt",
            "lib/migrations.md",
            "lib/migrations.json",
            "lib/migrations.yaml",
            "lib/migrations.toml",
            "lib/migrations.lisp.bak",
            "lib/migrations.lispx",
            "lib/migrations.lis",
        ];
        for relpath in footguns {
            let i = UpgradeInstruction::StateChange {
                script: PathBuf::from(relpath),
            };
            let err = i.validate().unwrap_err();
            assert!(
                matches!(&err, UpgradeError::NonLispExtensionScript { script: s }
                         if s == Path::new(relpath)),
                "wrong-extension script {relpath:?} must surface as NonLispExtensionScript \
                 carrying the offending path verbatim, got {err:?}"
            );
        }
    }

    #[test]
    fn validate_rejects_uppercase_lisp_extension_script() {
        // Strict lowercase: `.LISP` / `.Lisp` / `.LiSp` are
        // case-folded shapes a case-insensitive volume's existence
        // check would match the on-disk file — but the
        // canonical-form codec emits lowercase `.lisp` verbatim, so
        // a case-folded shape mismatches the round-trip-stable
        // canonical form (THEORY.md §V.2.7 render-determinism).
        // Same case-sensitive discipline the byte-size / duration
        // codecs use on unit suffixes (`MiB`, `ms`, `s`, `m`, `h`)
        // and every other shape-gate predicate in `render.rs` (label
        // / scheme / unit boundaries). Mirrors the peer
        // `BehaviorError::NonLispExtension` case-fold sweep (c97815a).
        for relpath in [
            "lib/migrations.LISP",
            "lib/migrations.Lisp",
            "lib/migrations.LiSp",
            "lib/migrations.lISP",
        ] {
            let i = UpgradeInstruction::StateChange {
                script: PathBuf::from(relpath),
            };
            let err = i.validate().unwrap_err();
            assert!(
                matches!(&err, UpgradeError::NonLispExtensionScript { script: s }
                         if s == Path::new(relpath)),
                "case-folded `.lisp` extension {relpath:?} must surface as \
                 NonLispExtensionScript (strict lowercase, canonical-form \
                 round-trip pin), got {err:?}"
            );
        }
    }

    #[test]
    fn validate_accepts_canonical_lisp_extension_scripts() {
        // Positive-control sweep across every canonical in-tree
        // authoring shape: bare filename, standard `lib/`
        // subdirectory, deeply-nested migrations subdirectory,
        // explicit current-dir-relative prefix, mid-path `./`
        // segment, multi-dot stem (the version-suffix shape
        // `lib/migrations/v.0.1.lisp` an author might use to encode
        // the migration's `:from` version into the filename). Drift
        // here = a future tightening that rejects any of these
        // surfaces as a test-failure at the per-axis validator
        // boundary, not piecemeal across renderer / layout-checker
        // call sites. Mirrors the peer `BehaviorSpec` positive-set
        // sweep (c97815a).
        let canonical: &[&str] = &[
            "lib/migrations.lisp",
            "lib/migrations/v01-to-v02.lisp",
            "migrations.lisp",
            "a.lisp",
            "./lib/migrations.lisp",
            "lib/./migrations.lisp",
            "lib/migrations/v.0.1.lisp",
        ];
        for relpath in canonical {
            UpgradeInstruction::StateChange {
                script: PathBuf::from(relpath),
            }
            .validate()
            .unwrap_or_else(|e| {
                panic!("canonical `.lisp` script {relpath:?} must pass, got {e:?}")
            });
        }
    }

    #[test]
    fn validate_sandbox_shape_takes_precedence_over_lisp_extension() {
        // Cross-arm precedence pin: a script that is *both*
        // sandbox-escaping (Empty / Absolute / ParentEscape) and
        // non-`.lisp` must surface the more-fundamental
        // sandbox-shape diagnostic first — the canonical fix
        // collapses both into "pin a relative `.lisp` path under the
        // caixa root", and the `.lisp` remediation would be
        // misleading when the offending path can never resolve under
        // the caixa root anyway. Mirrors the peer
        // `BehaviorError` cross-arm precedence (c97815a) and the
        // sibling `LimitsError`
        // (`MemoryZero` → `MemoryBelowWasm32Page` →
        // `MemoryExceedsWasm32Cap` → `MemoryNotPageMultiple`)
        // smallest-scope-arm-fires-last posture.
        let i_empty = UpgradeInstruction::StateChange {
            script: PathBuf::new(),
        };
        assert_eq!(i_empty.validate().unwrap_err(), UpgradeError::EmptyScript);
        let i_abs = UpgradeInstruction::StateChange {
            script: PathBuf::from("/etc/migrations.txt"),
        };
        assert!(
            matches!(
                i_abs.validate().unwrap_err(),
                UpgradeError::AbsoluteScript { .. }
            ),
            "absolute + non-`.lisp` must surface AbsoluteScript first"
        );
        let i_esc = UpgradeInstruction::StateChange {
            script: PathBuf::from("../sibling/migrations.rs"),
        };
        assert!(
            matches!(
                i_esc.validate().unwrap_err(),
                UpgradeError::ParentEscapeScript { .. }
            ),
            "parent-escape + non-`.lisp` must surface ParentEscapeScript first"
        );
    }

    #[test]
    fn non_lisp_extension_script_diagnostic_carries_offending_path() {
        // Diagnostic-shape pin: the surfaced error message names the
        // offending path verbatim (so the author can grep their
        // caixa.lisp for the literal value), the `.lisp` extension
        // is named in the remediation, and the downstream consumer
        // (`tatara_lisp::read` at hot-upgrade migration time) is
        // named so the author can trace the contract back to its
        // source. Same self-locating shape every per-axis variant
        // carries (`BehaviorError::NonLispExtension`, c97815a;
        // `LimitsError::MemoryNotPageMultiple`, ec266d8).
        let bad = PathBuf::from("lib/migrations.txt");
        let err = UpgradeInstruction::StateChange {
            script: bad.clone(),
        }
        .validate()
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("lib/migrations.txt"),
            "diagnostic must name the offending path verbatim, got {msg:?}"
        );
        assert!(
            msg.contains(".lisp"),
            "diagnostic must name the expected `.lisp` extension, got {msg:?}"
        );
        assert!(
            msg.contains(crate::render::M2_UPGRADE_INSTRUCTION_KIND_STATE_CHANGE),
            "diagnostic must name the offending `:state-change` instruction, got {msg:?}"
        );
        match err {
            UpgradeError::NonLispExtensionScript { script } => {
                assert_eq!(
                    script, bad,
                    "variant must carry the offending path verbatim"
                );
            }
            other => panic!("expected NonLispExtensionScript, got {other:?}"),
        }
    }

    #[test]
    fn declared_path_only_for_state_change() {
        let load = UpgradeInstruction::LoadModule { module: "x".into() };
        assert!(load.declared_path().is_none());
        let mig = UpgradeInstruction::StateChange {
            script: PathBuf::from("lib/m.lisp"),
        };
        assert_eq!(mig.declared_path(), Some(&PathBuf::from("lib/m.lisp")));
    }

    #[test]
    fn upgrade_instruction_is_restart_predicate_partitions_the_arm_set() {
        // The fail-before-pass-after pin on the `gen_platform::IsVariant`
        // derive's [`UpgradeInstruction::is_restart`] arm-discriminator
        // predicate: [`UpgradeInstruction::Restart`] is the only variant
        // that satisfies `.is_restart()`; every module-bearing arm
        // (`LoadModule` / `SoftPurge` / `Purge`) and the script-carrying
        // `StateChange` arm all return `false`. This pin makes the
        // partition invariant load-bearing at caixa-core test time so a
        // future derive regression (a hole that returns `false` for
        // `Restart` too, or a byte-collision that flips a second variant
        // to `true`) trips here rather than laundering the arm at
        // [`Self::validate_restart_exclusive`]'s paired positive /
        // negated filter sites (a hole flips restart-count to 0 →
        // vacuous OK; a collision flips restart-count > 1 → false
        // `RestartNotExclusive` on an entry the author declared without
        // any `(:restart)`). Peer of the sibling
        // [`crate::kind::tests::caixa_kind_is_variant_predicates_partition_the_arm_set`]
        // pin on the M0 `CaixaKind` axis.
        let cases: &[(UpgradeInstruction, bool)] = &[
            (UpgradeInstruction::LoadModule { module: "a".into() }, false),
            (UpgradeInstruction::SoftPurge { module: "b".into() }, false),
            (UpgradeInstruction::Purge { module: "c".into() }, false),
            (
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                false,
            ),
            (UpgradeInstruction::Restart, true),
        ];
        for (variant, expected) in cases {
            assert_eq!(
                variant.is_restart(),
                *expected,
                "UpgradeInstruction::{variant:?}.is_restart() must \
                 return {expected} (partition invariant on the \
                 IsVariant-derived arm-discriminator predicate)"
            );
        }
    }

    #[test]
    fn validate_restart_exclusive_routes_through_is_restart_predicate() {
        // Byte-identity pin on the paired positive / negated
        // `.is_restart()` filters at
        // [`Self::validate_restart_exclusive`] against the pre-lift
        // `matches!(i, UpgradeInstruction::Restart)` /
        // `!matches!(i, UpgradeInstruction::Restart)` predicates every
        // consumer of the gate previously coupled to inline. Asserts
        // the two projections agree byte-for-byte on every arm of the
        // enum, so a future derive regression that flipped either
        // predicate's arm-set would surface here at caixa-core test
        // time rather than at
        // [`Self::validate_restart_exclusive`]'s per-entry restart-
        // count / other-kinds tabulation far from the derive site.
        // Same peer-shape pin every sibling
        // `IsVariant`-derive-routed gate carries on the substrate's
        // closed-set typed-enum surface.
        let cases: Vec<UpgradeInstruction> = vec![
            UpgradeInstruction::LoadModule { module: "a".into() },
            UpgradeInstruction::SoftPurge { module: "b".into() },
            UpgradeInstruction::Purge { module: "c".into() },
            UpgradeInstruction::StateChange {
                script: PathBuf::from("lib/m.lisp"),
            },
            UpgradeInstruction::Restart,
        ];
        for instr in &cases {
            let via_predicate = instr.is_restart();
            let via_matches = matches!(instr, UpgradeInstruction::Restart);
            assert_eq!(
                via_predicate, via_matches,
                "UpgradeInstruction::{instr:?}: is_restart() must \
                 byte-equal matches!(_, UpgradeInstruction::Restart) — \
                 the pre-lift open-coded pattern and the \
                 IsVariant-derived predicate are the same axis, \
                 one typed dispatch"
            );
        }
    }

    #[test]
    fn declared_module_only_for_module_bearing_variants() {
        // Pinned partition of the `UpgradeInstruction` closed-set
        // variant space against the sibling of the peer
        // `declared_path` accessor: every OTP-appup module-bearing
        // variant (`LoadModule` / `SoftPurge` / `Purge`) surfaces its
        // `:module` string byte-for-byte through the lifted
        // `declared_module` accessor; every non-module-bearing variant
        // (`StateChange` on the peer `:script`-carrying axis;
        // `Restart` on the OTP terminal-fallback data-less axis)
        // returns `None`. Mirrors the peer
        // `declared_path_only_for_state_change` pin — the pair now
        // closes both scalar-carrying axes on the enum on one lifted
        // `Option<&…>` accessor apiece.
        let load = UpgradeInstruction::LoadModule {
            module: "hello-rio".into(),
        };
        assert_eq!(load.declared_module(), Some("hello-rio"));
        let soft = UpgradeInstruction::SoftPurge {
            module: "hello-rio-old".into(),
        };
        assert_eq!(soft.declared_module(), Some("hello-rio-old"));
        let hard = UpgradeInstruction::Purge {
            module: "hello-rio-ancient".into(),
        };
        assert_eq!(hard.declared_module(), Some("hello-rio-ancient"));
        let mig = UpgradeInstruction::StateChange {
            script: PathBuf::from("lib/m.lisp"),
        };
        assert!(mig.declared_module().is_none());
        assert!(UpgradeInstruction::Restart.declared_module().is_none());
    }

    #[test]
    fn declared_module_and_declared_path_partition_the_enum_variant_space() {
        // Byte-identity pin on the two-accessor partition: every
        // `UpgradeInstruction` variant returns `Some` from *exactly
        // one* of {`declared_module`, `declared_path`} (the two
        // module-bearing / script-carrying axes) or from *neither*
        // (the OTP terminal-fallback `Restart` shape). No variant
        // returns `Some` from both — the two axes are disjoint by
        // construction, and this pin closes the disjointness at the
        // test surface so a future variant that leaks a scalar across
        // both axes fails at build time. Mirrors the peer
        // `declared_paths_iter_covers_each_declared_slot_exactly_once`
        // discipline on the `BehaviorSpec` per-slot family.
        let cases: Vec<UpgradeInstruction> = vec![
            UpgradeInstruction::LoadModule { module: "a".into() },
            UpgradeInstruction::SoftPurge { module: "b".into() },
            UpgradeInstruction::Purge { module: "c".into() },
            UpgradeInstruction::StateChange {
                script: PathBuf::from("lib/m.lisp"),
            },
            UpgradeInstruction::Restart,
        ];
        for instr in &cases {
            let has_module = instr.declared_module().is_some();
            let has_path = instr.declared_path().is_some();
            assert!(
                !(has_module && has_path),
                "no variant may declare both a module and a path — offending: {instr:?}"
            );
            match instr {
                UpgradeInstruction::LoadModule { .. }
                | UpgradeInstruction::SoftPurge { .. }
                | UpgradeInstruction::Purge { .. } => {
                    assert!(has_module && !has_path, "module axis: {instr:?}");
                }
                UpgradeInstruction::StateChange { .. } => {
                    assert!(!has_module && has_path, "script axis: {instr:?}");
                }
                UpgradeInstruction::Restart => {
                    assert!(!has_module && !has_path, "data-less axis: {instr:?}");
                }
            }
        }
    }

    #[test]
    fn entry_with_chain_of_versions() {
        // Middle entry pairs a `:load-module` with the trailing
        // `:soft-purge` so it satisfies the within-entry purge-ordering
        // gate (`PurgeWithoutPriorLoad` rejects `:soft-purge` without a
        // preceding `:load-module`, mirroring the state-change-ordering
        // gate's `StateChangeWithoutPriorLoad`). The chain shape under
        // test is *cross-entry* `:from` values; the within-entry shape
        // is incidental — keeping it canonical (`:load-module` before
        // `:soft-purge`) leaves the chain assertion load-bearing.
        let entries = vec![
            entry(
                "0.1.0",
                vec![UpgradeInstruction::LoadModule { module: "x".into() }],
            ),
            entry(
                "0.1.5",
                vec![
                    UpgradeInstruction::LoadModule { module: "x".into() },
                    UpgradeInstruction::SoftPurge {
                        module: "x-old".into(),
                    },
                ],
            ),
            entry("0.2.0-rc.1", vec![UpgradeInstruction::Restart]),
        ];
        for e in &entries {
            e.validate().unwrap();
        }
        let json = serde_json::to_string(&entries).unwrap();
        let back: Vec<UpgradeFromEntry> = serde_json::from_str(&json).unwrap();
        assert_eq!(entries, back);
    }

    #[test]
    fn empty_instructions_list_is_valid() {
        let e = entry("0.1.0", vec![]);
        e.validate().unwrap();
    }

    #[test]
    fn json_uses_kebab_case_kind_tags() {
        let i = UpgradeInstruction::SoftPurge {
            module: "x-old".into(),
        };
        let json = serde_json::to_string(&i).unwrap();
        assert!(json.contains("\"kind\":\"soft-purge\""));
        let i2 = UpgradeInstruction::StateChange {
            script: PathBuf::from("m.lisp"),
        };
        let json2 = serde_json::to_string(&i2).unwrap();
        assert!(json2.contains("\"kind\":\"state-change\""));
    }

    // ── validate_upgrade_from: cross-entry graph-edge-set invariant ────

    #[test]
    fn validate_upgrade_from_accepts_disjoint_versions() {
        // Positive control: the canonical "chain v0.1.0 → 0.1.5 →
        // 0.2.0-rc.1" authoring shape from ABSORPTION-ROADMAP §M2.3
        // (and `entry_with_chain_of_versions` above) passes the cross-
        // entry gate. Different `:from` per entry is the intended
        // shape; the gate must not regress this baseline. Middle entry
        // pairs `:load-module` with `:soft-purge` to satisfy the
        // within-entry purge-ordering gate (see
        // `entry_with_chain_of_versions` for the same shape).
        let entries = vec![
            entry(
                "0.1.0",
                vec![UpgradeInstruction::LoadModule { module: "x".into() }],
            ),
            entry(
                "0.1.5",
                vec![
                    UpgradeInstruction::LoadModule { module: "x".into() },
                    UpgradeInstruction::SoftPurge {
                        module: "x-old".into(),
                    },
                ],
            ),
            entry("0.2.0-rc.1", vec![UpgradeInstruction::Restart]),
        ];
        validate_upgrade_from(&entries).unwrap();
    }

    #[test]
    fn validate_upgrade_from_accepts_empty_list() {
        // Absent `:upgrade-from` (the bare `feira init` shape) — the
        // gate must trivially pass an empty list. Mirrors the per-axis
        // "empty list passes" positive control on every peer typed-
        // graph gate (`validate_membros` empty list, `validate_placement`
        // requires non-empty clusters but only after a `Placement`
        // exists, etc.).
        validate_upgrade_from(&[]).unwrap();
    }

    #[test]
    fn validate_upgrade_from_rejects_duplicate_from() {
        // Fail-before-pass-after pin: two entries with the same parsed-
        // semver `:from` are an ambiguous edge in the typed upgrade
        // graph (OTP appup picks at most one matching block per running
        // version; with two matching blocks the operator picks either
        // set non-deterministically — author intent is one path per
        // prior version). Same set-not-multiset discipline as
        // `:children :caixa` (dbf50a9), `:membros :caixa` (4bb3f3d),
        // `:contratos` (5dbcfaf), `:placement :clusters` (c7c7799),
        // `:entrada :paths` (eb3456d) — now extended onto the fifth
        // typed-graph axis.
        let entries = vec![
            entry(
                "0.1.0",
                vec![UpgradeInstruction::LoadModule { module: "x".into() }],
            ),
            entry(
                "0.1.0",
                vec![
                    UpgradeInstruction::LoadModule { module: "x".into() },
                    UpgradeInstruction::SoftPurge {
                        module: "x-old".into(),
                    },
                ],
            ),
        ];
        let err = validate_upgrade_from(&entries).unwrap_err();
        assert_eq!(
            err,
            UpgradeError::DuplicateFrom {
                from: "0.1.0".into()
            },
            "two entries with `:from \"0.1.0\"` must surface as DuplicateFrom carrying the \
             offending value verbatim"
        );
    }

    #[test]
    fn validate_upgrade_from_treats_pre_release_as_distinct() {
        // Negative-of-positive: `1.0.0` and `1.0.0-rc.1` are *not*
        // equal under semver (pre-release version is part of the
        // identity), so they're distinct upgrade paths and must not
        // collide. A future tightening that collapses pre-release into
        // the release version surfaces here.
        let entries = vec![
            entry("1.0.0", vec![UpgradeInstruction::Restart]),
            entry("1.0.0-rc.1", vec![UpgradeInstruction::Restart]),
        ];
        validate_upgrade_from(&entries).unwrap();
    }

    #[test]
    fn validate_upgrade_from_treats_build_metadata_as_distinct() {
        // Conservative-by-design: [`semver::Version`]'s `PartialEq`
        // compares build metadata (it derives equality across all
        // fields including `pre` + `build`), so `1.0.0+build1` and
        // `1.0.0+build2` are *not* duplicates from the gate's
        // perspective — the operator may treat the build-metadata
        // suffix as a tiebreaker even though the semver spec says
        // build metadata is ignored for precedence
        // (https://semver.org/#spec-item-10). Pin the conservative
        // behavior here so a future switch to a build-metadata-
        // stripping comparator surfaces as a test failure first; that
        // change would require coordinating with the wasm-operator's
        // `:from`-match dispatch step, which is the load-bearing
        // semantic we'd be mirroring.
        let entries = vec![
            entry("1.0.0+build1", vec![UpgradeInstruction::Restart]),
            entry("1.0.0+build2", vec![UpgradeInstruction::Restart]),
        ];
        validate_upgrade_from(&entries).unwrap();
    }

    #[test]
    fn validate_upgrade_from_per_entry_shape_fires_before_duplicate() {
        // Order pin: a malformed `:from` on the second entry surfaces
        // its `FromInvalid` diagnostic, not a (less-useful)
        // `DuplicateFrom`. The per-entry shape pass runs *inline*
        // before the duplicate-key insert — parallel to
        // `child_versao_invalid_fires_before_duplicate_check`
        // (b38ff3a) and `membro_versao_invalid_fires_before_duplicate_check`
        // (9888b13). Without this pin a future shortcut that runs the
        // cross-entry gate first would surface a duplicate diagnostic
        // on a string that isn't even parsable as a version.
        let entries = vec![
            entry("0.1.0", vec![UpgradeInstruction::Restart]),
            entry("not-a-semver", vec![UpgradeInstruction::Restart]),
        ];
        let err = validate_upgrade_from(&entries).unwrap_err();
        assert!(
            matches!(err, UpgradeError::FromInvalid { ref from, .. } if from == "not-a-semver"),
            "malformed `:from` on a non-duplicate entry must surface as FromInvalid, got {err:?}"
        );
    }

    #[test]
    fn validate_upgrade_from_per_entry_shape_fires_before_duplicate_on_first_entry() {
        // Symmetric arm: a malformed shape on the *first* entry of a
        // duplicate pair surfaces its per-entry diagnostic too (not
        // the duplicate diagnostic that would otherwise fire on the
        // second entry). Pinned separately so a future shortcut that
        // walks the duplicate-check ahead of the per-entry pass for the
        // first entry only — easy regression to introduce — surfaces
        // here.
        let entries = vec![
            entry(
                "0.1.0",
                vec![UpgradeInstruction::LoadModule {
                    module: String::new(),
                }],
            ),
            entry("0.1.0", vec![UpgradeInstruction::Restart]),
        ];
        let err = validate_upgrade_from(&entries).unwrap_err();
        assert_eq!(
            err,
            UpgradeError::ModuleEmpty {
                kind: crate::render::M2_UPGRADE_INSTRUCTION_KIND_LOAD_MODULE
            },
            "malformed instruction on the first entry of a duplicate pair must surface its \
             per-entry diagnostic before the duplicate gate fires, got {err:?}"
        );
    }

    #[test]
    fn validate_upgrade_from_duplicate_diagnostic_names_second_collision() {
        // Diagnostic-shape pin: when three entries carry the same
        // `:from`, the gate reports the *first* collision (the second
        // entry) and stops — the third entry's duplicate is masked by
        // the first surfaced one. Mirrors
        // `validate_duplicate_child_diagnostic_names_first_collision`
        // (dbf50a9) on the supervisor axis.
        let entries = vec![
            entry("0.1.0", vec![UpgradeInstruction::Restart]),
            entry("0.1.0", vec![UpgradeInstruction::Restart]),
            entry("0.1.0", vec![UpgradeInstruction::Restart]),
        ];
        let err = validate_upgrade_from(&entries).unwrap_err();
        assert_eq!(
            err,
            UpgradeError::DuplicateFrom {
                from: "0.1.0".into()
            }
        );
    }

    #[test]
    fn validate_upgrade_from_single_entry_never_duplicates() {
        // Boundary control: a list of one entry can never produce a
        // duplicate, regardless of `:from` value (any single-element
        // set is trivially without duplicates). Pin this so a future
        // off-by-one in the seen-set insert doesn't accidentally flag
        // a single entry as duplicating itself.
        let entries = vec![entry("0.1.0", vec![UpgradeInstruction::Restart])];
        validate_upgrade_from(&entries).unwrap();
    }

    // ── validate_upgrade_from_against_versao: cross-slot precedence gate ─

    #[test]
    fn versao_gate_accepts_strict_upgrade() {
        // Positive control: the canonical "chain prior versions →
        // current" authoring shape from ABSORPTION-ROADMAP §M2.3 — each
        // `:from` strictly less than the current `:versao` under
        // SemVer-2 precedence. The gate must not regress this baseline.
        let entries = vec![
            entry("0.1.0", vec![UpgradeInstruction::Restart]),
            entry("0.1.5", vec![UpgradeInstruction::Restart]),
            entry("0.1.9", vec![UpgradeInstruction::Restart]),
        ];
        validate_upgrade_from_against_versao(&entries, "0.2.0").unwrap();
    }

    #[test]
    fn versao_gate_accepts_empty_entries() {
        // Bare `feira init` shape (no `:upgrade-from`) trivially passes;
        // the gate is a no-op when the entries list is empty. Mirrors
        // `validate_upgrade_from_accepts_empty_list` on the peer gate.
        validate_upgrade_from_against_versao(&[], "0.1.0").unwrap();
    }

    #[test]
    fn versao_gate_rejects_equal_from() {
        // Self-upgrade no-op: declaring `:from "0.2.0"` while
        // `:versao "0.2.0"` means "upgrade from myself to myself" —
        // the operator's dispatch either skips silently or
        // trivially "succeeds" with no observable state change.
        // Reject as the canonical "I forgot to bump :versao when
        // adding this entry" footgun.
        let entries = vec![entry("0.2.0", vec![UpgradeInstruction::Restart])];
        let err = validate_upgrade_from_against_versao(&entries, "0.2.0").unwrap_err();
        assert_eq!(
            err,
            UpgradeError::FromNotBeforeVersao {
                from: "0.2.0".into(),
                versao: "0.2.0".into(),
            },
            ":from == :versao under precedence must surface as FromNotBeforeVersao naming both \
             values verbatim, got {err:?}"
        );
    }

    #[test]
    fn versao_gate_rejects_downgrade_from() {
        // Downgrade-shaped: `:from "0.3.0"` while `:versao "0.2.0"`
        // means "upgrade nodes coming from 0.3.0 to 0.2.0", which
        // the operator's `:from`-match dispatch can never reach (it
        // never runs a version >= the current one). Reject as the
        // canonical "I copy-pasted from the next minor version and
        // forgot to bump :versao" footgun.
        let entries = vec![entry("0.3.0", vec![UpgradeInstruction::Restart])];
        let err = validate_upgrade_from_against_versao(&entries, "0.2.0").unwrap_err();
        assert_eq!(
            err,
            UpgradeError::FromNotBeforeVersao {
                from: "0.3.0".into(),
                versao: "0.2.0".into(),
            }
        );
    }

    #[test]
    fn versao_gate_accepts_prerelease_before_release() {
        // SemVer §11 precedence: pre-release versions are *less than*
        // the corresponding release (`0.2.0-rc.1 < 0.2.0`). Upgrading
        // FROM an RC TO the GA release is the canonical authoring
        // shape — must pass. A regression that collapses pre-release
        // into the release version (treating them as equal) surfaces
        // here as a false-positive rejection.
        let entries = vec![entry("0.2.0-rc.1", vec![UpgradeInstruction::Restart])];
        validate_upgrade_from_against_versao(&entries, "0.2.0").unwrap();
    }

    #[test]
    fn versao_gate_rejects_release_after_prerelease() {
        // Symmetric arm: with `:versao "0.2.0-rc.1"` and
        // `:from "0.2.0"`, precedence says `0.2.0 > 0.2.0-rc.1` —
        // the typical "I'm on an RC of a release that already
        // shipped" footgun. The gate names both values verbatim
        // so the author can grep for either side and fix in one
        // edit.
        let entries = vec![entry("0.2.0", vec![UpgradeInstruction::Restart])];
        let err = validate_upgrade_from_against_versao(&entries, "0.2.0-rc.1").unwrap_err();
        assert_eq!(
            err,
            UpgradeError::FromNotBeforeVersao {
                from: "0.2.0".into(),
                versao: "0.2.0-rc.1".into(),
            }
        );
    }

    #[test]
    fn versao_gate_rejects_build_metadata_only_difference() {
        // SemVer §11 explicitly excludes build metadata from
        // precedence comparison: `0.2.0+build.1` and `0.2.0` are
        // *equal* under [`semver::Version::cmp`]. From the
        // operator's `:from`-match dispatch perspective this is a
        // self-upgrade no-op (no semantic transition between the
        // two), so the gate rejects it — *unlike* the peer
        // duplicate-`:from` gate which uses derived `PartialEq` and
        // treats build-metadata variants as distinct dispatch keys.
        // The two gates' different equality notions are deliberate:
        // duplicate-check is conservative (preserves operator-side
        // tiebreaking surface), precedence-check is permissive
        // (matches operator-side dispatch semantic).
        let entries = vec![entry("0.2.0+build.1", vec![UpgradeInstruction::Restart])];
        let err = validate_upgrade_from_against_versao(&entries, "0.2.0").unwrap_err();
        assert_eq!(
            err,
            UpgradeError::FromNotBeforeVersao {
                from: "0.2.0+build.1".into(),
                versao: "0.2.0".into(),
            }
        );
    }

    #[test]
    fn versao_gate_silently_passes_on_unparseable_versao() {
        // Defensive arm: a malformed `:versao` (gated by the
        // narrower `ManifestError::VersaoInvalid` surface at the
        // load-bearing call site) must not regress into a
        // `FromNotBeforeVersao` diagnostic from this gate. Surfacing
        // the precedence error over an unparseable `:versao` would
        // mask the more actionable root cause (the author meant to
        // type `"0.2.0"`, not `"v0.2.0"`).
        let entries = vec![entry("0.1.0", vec![UpgradeInstruction::Restart])];
        validate_upgrade_from_against_versao(&entries, "not-a-semver").unwrap();
    }

    #[test]
    fn versao_gate_silently_passes_on_unparseable_from() {
        // Symmetric defensive arm: a malformed `:from` is gated by
        // [`UpgradeFromEntry::validate`] / [`validate_upgrade_from`]
        // upstream at the LayoutInvariants call site. Surfacing the
        // precedence error over an unparseable `:from` from this
        // gate alone would mask the narrower `FromInvalid`
        // diagnostic that's expected to lead — same fall-through
        // posture as the unparseable-`:versao` arm above. The
        // wiring in `LayoutInvariants::verify` runs
        // `validate_upgrade_from` *before* this gate, so in practice
        // an unparseable `:from` surfaces as `FromInvalid` first
        // and this gate is never reached on that input.
        let entries = vec![entry("not-a-semver", vec![UpgradeInstruction::Restart])];
        validate_upgrade_from_against_versao(&entries, "0.2.0").unwrap();
    }

    #[test]
    fn versao_gate_reports_first_offending_entry() {
        // Determinism pin: with multiple offending entries the gate
        // surfaces the *first* one in declaration order — same
        // posture as `validate_upgrade_from_duplicate_diagnostic_names_second_collision`
        // on the peer gate. Walks the entries in order; first
        // failing `:from >= :versao` short-circuits.
        let entries = vec![
            entry("0.1.0", vec![UpgradeInstruction::Restart]),
            entry("0.3.0", vec![UpgradeInstruction::Restart]),
            entry("0.4.0", vec![UpgradeInstruction::Restart]),
        ];
        let err = validate_upgrade_from_against_versao(&entries, "0.2.0").unwrap_err();
        assert_eq!(
            err,
            UpgradeError::FromNotBeforeVersao {
                from: "0.3.0".into(),
                versao: "0.2.0".into(),
            },
            "the first offending `:from` (0.3.0) must surface, not the later one (0.4.0)"
        );
    }

    // ── UpgradeFromEntry::validate_restart_exclusive: within-entry gate ─

    #[test]
    fn validate_rejects_restart_mixed_with_load_module() {
        // The "I'll try the typed path *then* restart anyway" footgun:
        // an instructions list with `(:restart)` plus `(:load-module …)`
        // is dead code in both directions (succeed → restart discards
        // the work that just succeeded, defeating the typed sequence's
        // whole point; fail → restart never reached because the entry
        // already failed). The gate names the offending entry's `:from`
        // verbatim plus the kebab-case lisp-form of every non-`:restart`
        // peer so the author can grep their caixa.lisp for either side
        // and fix in one edit.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule {
                    module: "hello-rio".into(),
                },
                UpgradeInstruction::Restart,
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::RestartNotExclusive {
                from: "0.1.0".into(),
                restart_count: 1,
                other_kinds: vec![crate::render::M2_UPGRADE_INSTRUCTION_KIND_LOAD_MODULE],
            },
            "restart + load-module mix must surface as RestartNotExclusive naming the \
             offending `:from` + the non-:restart kinds verbatim, got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_restart_mixed_with_full_typed_sequence() {
        // Sweep the typed-sequence universe — every non-`:restart`
        // variant alongside `:restart` — and assert every typed
        // instruction's lisp-form appears in `other_kinds` in
        // declaration order. The author should be able to grep for
        // each verbatim (`:load-module`, `:state-change`, `:soft-purge`,
        // `:purge`) and resolve in one pass. Drift in the `lisp_form`
        // mapping surfaces here.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule {
                    module: "hello-rio".into(),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                UpgradeInstruction::SoftPurge {
                    module: "hello-rio-old".into(),
                },
                UpgradeInstruction::Purge {
                    module: "hello-rio-old".into(),
                },
                UpgradeInstruction::Restart,
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::RestartNotExclusive {
                from: "0.1.0".into(),
                restart_count: 1,
                other_kinds: vec![
                    crate::render::M2_UPGRADE_INSTRUCTION_KIND_LOAD_MODULE,
                    crate::render::M2_UPGRADE_INSTRUCTION_KIND_STATE_CHANGE,
                    crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
                    crate::render::M2_UPGRADE_INSTRUCTION_KIND_PURGE,
                ],
            },
        );
    }

    #[test]
    fn validate_rejects_restart_duplicated() {
        // `((:restart) (:restart))` — multiple Restart variants in one
        // entry. The fallback is a single semantic (restart the pod;
        // the new version comes up fresh); repeating it is at best
        // redundant, at worst suggests the author thought the second
        // would re-trigger after the first. The gate reports
        // `restart_count: 2` so the diagnostic surfaces the duplication
        // mode unambiguously even when `other_kinds` is empty.
        let e = entry(
            "0.1.0",
            vec![UpgradeInstruction::Restart, UpgradeInstruction::Restart],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::RestartNotExclusive {
                from: "0.1.0".into(),
                restart_count: 2,
                other_kinds: vec![],
            },
        );
    }

    #[test]
    fn validate_accepts_sole_restart() {
        // Positive control: the canonical "this prior version's typed
        // upgrade is impossible — restart" authoring shape from the
        // UpgradeInstruction::Restart doc comment. `((:restart))` alone
        // is the entry's whole instructions list and the only valid
        // Restart-bearing shape.
        let e = entry("0.1.0", vec![UpgradeInstruction::Restart]);
        e.validate().unwrap();
    }

    #[test]
    fn validate_accepts_typed_sequence_without_restart() {
        // Positive control: the canonical typed hot-upgrade authoring
        // shape from ABSORPTION-ROADMAP §M2.3 — `:load-module` →
        // `:state-change` → `:soft-purge`. Absent `:restart` is the
        // only shape that lets the sequence run to completion under
        // the wasm-operator's `:from`-match dispatch. Drift here =
        // a future tighten that rejects any canonical typed-only shape
        // surfaces as a regression at this gate.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule {
                    module: "hello-rio".into(),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                UpgradeInstruction::SoftPurge {
                    module: "hello-rio-old".into(),
                },
            ],
        );
        e.validate().unwrap();
    }

    // ── within-entry state-change-ordering invariant ───────────────────

    #[test]
    fn validate_rejects_state_change_without_load() {
        // Fail-before-pass-after pin: a `:state-change` migrates state
        // into the newly-loaded code (gen_server:code_change/3 analog),
        // so an entry that runs it with no preceding `:load-module`
        // migrates state into code that was never loaded. The operator
        // runs instructions in declared order, so this is a build error,
        // not a runtime surprise (CAIXA-SDLC §III).
        let e = entry(
            "0.1.0",
            vec![UpgradeInstruction::StateChange {
                script: PathBuf::from("lib/m.lisp"),
            }],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::StateChangeWithoutPriorLoad {
                from: "0.1.0".into(),
                script: PathBuf::from("lib/m.lisp"),
            },
            "a `:state-change` with no preceding `:load-module` must surface as \
             StateChangeWithoutPriorLoad naming the offending entry + script verbatim"
        );
    }

    #[test]
    fn validate_rejects_state_change_before_load() {
        // Right-instructions-wrong-order: the load is present but runs
        // *after* the migration. Because the operator executes in
        // declared order, the migration runs before the new code is
        // resident — the same incoherence as the missing-load case.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                UpgradeInstruction::LoadModule {
                    module: "hello-rio".into(),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert!(
            matches!(err, UpgradeError::StateChangeWithoutPriorLoad { .. }),
            "a `:state-change` ahead of its `:load-module` must surface as \
             StateChangeWithoutPriorLoad, got {err:?}"
        );
    }

    #[test]
    fn validate_accepts_state_change_after_load() {
        // Positive control: the canonical `(:load-module …)
        // (:state-change …)` order validates. The load need not name
        // the same module the migration targets (StateChange carries a
        // script, not a module ref), so any preceding `:load-module`
        // satisfies "new code is resident before its migration runs".
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule {
                    module: "hello-rio".into(),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
            ],
        );
        e.validate().unwrap();
    }

    #[test]
    fn validate_accepts_multiple_state_changes_after_one_load() {
        // A single leading `:load-module` covers every subsequent
        // `:state-change` — the `loaded` latch stays set once the new
        // code is resident.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule {
                    module: "hello-rio".into(),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m1.lisp"),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m2.lisp"),
                },
            ],
        );
        e.validate().unwrap();
    }

    #[test]
    fn validate_state_change_ordering_fires_after_restart_exclusive() {
        // Diagnostic-precedence pin: a `((:state-change …) (:restart))`
        // shape is *both* state-change-without-load and restart-mixed.
        // The more-fundamental `RestartNotExclusive` must win (a valid
        // `(:restart)` entry is `(:restart)` alone, so no Restart-bearing
        // entry should reach the ordering gate). Guards the call order
        // in `validate` against silent reordering.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                UpgradeInstruction::Restart,
            ],
        );
        let err = e.validate().unwrap_err();
        assert!(
            matches!(err, UpgradeError::RestartNotExclusive { .. }),
            "restart-mixed must surface before the ordering gate, got {err:?}"
        );
    }

    // ── within-entry purge-ordering invariant ──────────────────────────

    #[test]
    fn validate_rejects_soft_purge_without_load() {
        // Fail-before-pass-after pin: `:soft-purge` drains the *old*
        // module after the new one is resident (OTP's two-phase code
        // load — code:load_module/1 then code:soft_purge/1), so an
        // entry that runs it with no preceding `:load-module` drains
        // the live module with no replacement. The operator runs
        // instructions in declared order, so this is a build error,
        // not a runtime surprise (CAIXA-SDLC §III).
        let e = entry(
            "0.1.0",
            vec![UpgradeInstruction::SoftPurge {
                module: "x-old".into(),
            }],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::PurgeWithoutPriorLoad {
                from: "0.1.0".into(),
                kind: crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
                module: "x-old".into(),
            },
            "a `:soft-purge` with no preceding `:load-module` must surface as \
             PurgeWithoutPriorLoad naming the offending entry + kind + module verbatim"
        );
    }

    #[test]
    fn validate_rejects_purge_without_load() {
        // Per-arm coverage: `:purge` (immediate discard, no drain) is
        // the more catastrophic peer of `:soft-purge`; same gate, same
        // shape, kind-tag differs so the author can grep their
        // caixa.lisp for the offending `(:purge …)` form.
        let e = entry(
            "0.1.0",
            vec![UpgradeInstruction::Purge {
                module: "x-old".into(),
            }],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::PurgeWithoutPriorLoad {
                from: "0.1.0".into(),
                kind: crate::render::M2_UPGRADE_INSTRUCTION_KIND_PURGE,
                module: "x-old".into(),
            },
        );
    }

    #[test]
    fn validate_rejects_soft_purge_before_load() {
        // Right-instructions-wrong-order: the load is present but runs
        // *after* the purge. Because the operator executes in declared
        // order, the cleanup drains the old code before the new code
        // is resident — same incoherence as the missing-load case,
        // leaving a window during which neither version is available.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::LoadModule { module: "x".into() },
            ],
        );
        let err = e.validate().unwrap_err();
        assert!(
            matches!(
                err,
                UpgradeError::PurgeWithoutPriorLoad {
                    kind: crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
                    ..
                }
            ),
            "a `:soft-purge` ahead of its `:load-module` must surface as \
             PurgeWithoutPriorLoad, got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_purge_before_load() {
        // Symmetric arm on the `:purge` variant — the kind tag
        // distinguishes the diagnostic so the author lands on the
        // offending form directly.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::Purge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::LoadModule { module: "x".into() },
            ],
        );
        let err = e.validate().unwrap_err();
        assert!(
            matches!(
                err,
                UpgradeError::PurgeWithoutPriorLoad {
                    kind: crate::render::M2_UPGRADE_INSTRUCTION_KIND_PURGE,
                    ..
                }
            ),
            "a `:purge` ahead of its `:load-module` must surface as \
             PurgeWithoutPriorLoad, got {err:?}"
        );
    }

    #[test]
    fn validate_accepts_soft_purge_after_load() {
        // Positive control: the canonical `(:load-module …)
        // (:soft-purge …)` order validates. The load need not name the
        // same module the purge targets — the cleanup typically targets
        // the *old* module name (e.g. `"x-old"`) and the load brings up
        // the *new* one (`"x"`); the gate only requires that *some*
        // `:load-module` precedes the purge, so the new code is resident
        // before the old one is drained.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
            ],
        );
        e.validate().unwrap();
    }

    #[test]
    fn validate_accepts_multiple_purges_after_one_load() {
        // A single leading `:load-module` covers every subsequent
        // `:soft-purge` / `:purge` — the `loaded` latch stays set once
        // the new code is resident. Same shape as
        // `validate_accepts_multiple_state_changes_after_one_load` on
        // the peer ordering gate.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::Purge {
                    module: "x-oldest".into(),
                },
            ],
        );
        e.validate().unwrap();
    }

    #[test]
    fn validate_purge_ordering_fires_after_state_change_ordering() {
        // Diagnostic-precedence pin: an entry like `((:state-change …)
        // (:soft-purge …))` is *both* state-change-without-load and
        // purge-without-load. The state-change gate must win — it's
        // the load-bearing semantic on this ordering contract, and
        // surfacing the purge diagnostic first would mask the more-
        // fundamental migration-against-stale-code defect. Guards the
        // call order in `validate` against silent reordering.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert!(
            matches!(err, UpgradeError::StateChangeWithoutPriorLoad { .. }),
            "state-change-without-load must surface before purge-without-load, got {err:?}"
        );
    }

    #[test]
    fn validate_purge_ordering_fires_after_per_instr_shape() {
        // Order pin: a malformed `:module` value on a `:soft-purge` (an
        // empty string) surfaces its narrower kind-tagged `ModuleEmpty`
        // diagnostic *before* the within-entry purge-ordering gate fires.
        // The per-instruction shape pass walks the list inline before
        // the ordering checks, so the narrower self-locating diagnostic
        // surfaces first — mirrors the empty-first cascade on every peer
        // DNS-1123 gate and the `validate_restart_exclusive_fires_after_
        // per_instr_shape` pin on the sibling ordering gate.
        let e = entry(
            "0.1.0",
            vec![UpgradeInstruction::SoftPurge {
                module: String::new(),
            }],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::ModuleEmpty {
                kind: crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE
            },
            "malformed instruction must surface its kind-tagged diagnostic before the \
             purge-ordering gate fires, got {err:?}"
        );
    }

    #[test]
    fn validate_purge_ordering_threads_through_validate_upgrade_from() {
        // The whole-list entry-point surfaces the per-entry ordering
        // error (mirrors
        // `validate_state_change_ordering_threads_through_validate_upgrade_from`):
        // the gate is reachable from the LayoutInvariants call site, not
        // only from a direct `entry.validate()`.
        let entries = vec![entry(
            "0.1.0",
            vec![UpgradeInstruction::Purge {
                module: "x-old".into(),
            }],
        )];
        let err = validate_upgrade_from(&entries).unwrap_err();
        assert!(
            matches!(
                err,
                UpgradeError::PurgeWithoutPriorLoad {
                    kind: crate::render::M2_UPGRADE_INSTRUCTION_KIND_PURGE,
                    ..
                }
            ),
            "validate_upgrade_from must thread the purge-ordering error, got {err:?}"
        );
    }

    #[test]
    fn validate_state_change_ordering_threads_through_validate_upgrade_from() {
        // The whole-list entry-point surfaces the per-entry ordering
        // error (mirrors `validate_restart_exclusive_threads_through_…`):
        // the gate is reachable from the LayoutInvariants call site, not
        // only from a direct `entry.validate()`.
        let entries = vec![entry(
            "0.1.0",
            vec![UpgradeInstruction::StateChange {
                script: PathBuf::from("lib/m.lisp"),
            }],
        )];
        let err = validate_upgrade_from(&entries).unwrap_err();
        assert!(
            matches!(err, UpgradeError::StateChangeWithoutPriorLoad { .. }),
            "validate_upgrade_from must thread the ordering error, got {err:?}"
        );
    }

    // ── within-entry cleanup-singularity invariant ─────────────────────

    #[test]
    fn validate_rejects_duplicate_soft_purge_for_same_module() {
        // Fail-before-pass-after pin: `:soft-purge` drains-then-GCs
        // its target module (code:soft_purge/1 analog); after the
        // first the module is gone, so a second `:soft-purge` of the
        // same module is at best a no-op and at worst undefined
        // (depending on the operator's handling of a non-resident-
        // module purge). Author one cleanup per module.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::DuplicateCleanup {
                from: "0.1.0".into(),
                module: "x-old".into(),
                kinds: vec![
                    crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
                    crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
                ],
            },
            "two `:soft-purge` of the same module must surface as DuplicateCleanup naming the \
             module + both kinds in declaration order, got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_duplicate_purge_for_same_module() {
        // Per-arm coverage: `:purge` (immediate discard, no drain) is
        // the more catastrophic peer of `:soft-purge`; same gate, same
        // shape, kind-tag distinguishes so the author can grep their
        // caixa.lisp for the offending `(:purge …)` form.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::Purge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::Purge {
                    module: "x-old".into(),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::DuplicateCleanup {
                from: "0.1.0".into(),
                module: "x-old".into(),
                kinds: vec![
                    crate::render::M2_UPGRADE_INSTRUCTION_KIND_PURGE,
                    crate::render::M2_UPGRADE_INSTRUCTION_KIND_PURGE,
                ],
            },
        );
    }

    #[test]
    fn validate_rejects_soft_purge_then_purge_for_same_module() {
        // Soft-then-hard footgun: the author wrote "drain, and if
        // drain doesn't clean up, force-discard", but the operator
        // runs declared instructions unconditionally — the `:purge`
        // fires whether the `:soft-purge` already discarded the
        // module or not, so the imagined fallback semantic is
        // missing. Fallback on cleanup failure is the operator's
        // job, not authored into the entry. Both kinds carry in
        // declaration order so the author can grep for either side
        // and pick one.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::Purge {
                    module: "x-old".into(),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::DuplicateCleanup {
                from: "0.1.0".into(),
                module: "x-old".into(),
                kinds: vec![
                    crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
                    crate::render::M2_UPGRADE_INSTRUCTION_KIND_PURGE,
                ],
            },
        );
    }

    #[test]
    fn validate_rejects_purge_then_soft_purge_for_same_module() {
        // Reversed-ordering arm: `:purge` discards immediately; the
        // trailing `:soft-purge` has no module to drain. The kinds
        // list reflects declaration order so the diagnostic locates
        // both forms in the source.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::Purge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::DuplicateCleanup {
                from: "0.1.0".into(),
                module: "x-old".into(),
                kinds: vec![
                    crate::render::M2_UPGRADE_INSTRUCTION_KIND_PURGE,
                    crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
                ],
            },
        );
    }

    #[test]
    fn validate_accepts_distinct_cleanup_modules() {
        // Positive control: `:soft-purge` and `:purge` on *different*
        // modules pass the gate. Mirrors
        // `validate_accepts_multiple_purges_after_one_load` — the
        // cleanup-singularity gate is keyed on (module), not on
        // (kind, module) pair, so distinct old-version names render
        // distinct cleanup targets and don't collide. Sweep both
        // same-class (two `:soft-purge` distinct modules) and cross-
        // class (`:soft-purge` then `:purge` distinct modules) so a
        // future tighten to a kind-only key (which would over-fire on
        // distinct modules) surfaces here.
        let two_soft = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::SoftPurge {
                    module: "x-older".into(),
                },
            ],
        );
        two_soft.validate().unwrap();
        let mixed = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::Purge {
                    module: "x-oldest".into(),
                },
            ],
        );
        mixed.validate().unwrap();
    }

    #[test]
    fn validate_accepts_single_cleanup_per_module() {
        // Boundary control: a list with exactly one `:soft-purge` and
        // one `:purge` (distinct modules, the canonical "drain one,
        // hard-discard the other" shape) is the gate's identity
        // element. Pin so a future off-by-one in the duplicate-detection
        // scan doesn't accidentally flag a single occurrence as
        // duplicating itself — mirrors
        // `validate_upgrade_from_single_entry_never_duplicates` on
        // the peer cross-entry duplicate axis.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::Purge {
                    module: "y-old".into(),
                },
            ],
        );
        e.validate().unwrap();
    }

    #[test]
    fn validate_cleanup_singularity_fires_after_purge_ordering() {
        // Diagnostic-precedence pin: an entry like `((:soft-purge "x")
        // (:soft-purge "x"))` is *both* purge-without-load and
        // duplicate-cleanup. The more-fundamental ordering gate must
        // win — the missing-load defect is load-bearing (the canonical
        // OTP shape requires the new code be resident before any
        // cleanup runs), and surfacing the duplicate diagnostic first
        // would mask the no-replacement-window defect the ordering
        // gate exists to close. Guards the call order in `validate`
        // against silent reordering. Same posture as
        // `validate_purge_ordering_fires_after_state_change_ordering`
        // on the sibling ordering gate.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert!(
            matches!(
                err,
                UpgradeError::PurgeWithoutPriorLoad {
                    kind: crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
                    ..
                }
            ),
            "purge-without-load must surface before duplicate-cleanup, got {err:?}"
        );
    }

    #[test]
    fn validate_cleanup_singularity_fires_after_per_instr_shape() {
        // Order pin: a malformed `:module` value on a `:soft-purge`
        // (an empty string) surfaces its narrower kind-tagged
        // `ModuleEmpty` diagnostic *before* the within-entry cleanup-
        // singularity gate fires. The per-instruction shape pass walks
        // the list inline before the singularity check, so the
        // narrower self-locating diagnostic surfaces first — mirrors
        // the empty-first cascade on every peer DNS-1123 gate and the
        // `validate_purge_ordering_fires_after_per_instr_shape` pin on
        // the sibling ordering gate.
        //
        // Two empty-string `:soft-purge` would *otherwise* duplicate
        // (both modules are the same empty string), so this pin
        // double-locks the precedence: the per-instr shape gate must
        // win on the first malformed instruction before the duplicate
        // scan even reaches the second.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: String::new(),
                },
                UpgradeInstruction::SoftPurge {
                    module: String::new(),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::ModuleEmpty {
                kind: crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE
            },
            "malformed instruction must surface its kind-tagged diagnostic before the \
             cleanup-singularity gate fires, got {err:?}"
        );
    }

    #[test]
    fn validate_cleanup_singularity_reports_first_collision() {
        // Determinism pin: with three cleanups of the same module the
        // gate reports the *first* collision (the second occurrence)
        // and stops — the third's duplicate is masked by the first
        // surfaced one. Mirrors
        // `validate_upgrade_from_duplicate_diagnostic_names_second_collision`
        // on the peer cross-entry duplicate axis.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::Purge {
                    module: "x-old".into(),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::DuplicateCleanup {
                from: "0.1.0".into(),
                module: "x-old".into(),
                kinds: vec![
                    crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
                    crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
                ],
            },
            "the first colliding pair must surface, not the later `:purge` collision"
        );
    }

    #[test]
    fn validate_cleanup_singularity_threads_through_validate_upgrade_from() {
        // The whole-list entry-point surfaces the per-entry singularity
        // error (mirrors
        // `validate_purge_ordering_threads_through_validate_upgrade_from`):
        // the gate is reachable from the LayoutInvariants call site,
        // not only from a direct `entry.validate()`.
        let entries = vec![entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::Purge {
                    module: "x-old".into(),
                },
            ],
        )];
        let err = validate_upgrade_from(&entries).unwrap_err();
        assert!(
            matches!(err, UpgradeError::DuplicateCleanup { .. }),
            "validate_upgrade_from must thread the cleanup-singularity error, got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_duplicate_load_module_for_same_module() {
        // `LoadModule` is the `code:load_module/1` analog (INSPIRATIONS
        // §II.4): each module is loaded exactly once per upgrade entry,
        // the operator's dispatch table reads the module name to bind
        // the wasm component, and a second `(:load-module "x")` re-reads
        // the same module name and re-binds the same component — a
        // no-op the second time. systools-generated `.relup` files emit
        // at most one `load_module` per module per upgrade step for
        // this reason. Author one `(:load-module "x")` per old module.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::LoadModule { module: "x".into() },
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::DuplicateLoadModule {
                from: "0.1.0".into(),
                module: "x".into(),
            },
            "two `:load-module` of the same module must surface as DuplicateLoadModule naming \
             the module, got {err:?}"
        );
    }

    #[test]
    fn validate_accepts_distinct_load_modules() {
        // Positive control: `:load-module` instructions on *different*
        // modules pass the gate. Mirrors
        // `validate_accepts_distinct_cleanup_modules` on the sibling
        // singularity axis — the load-singularity gate is keyed on
        // (module), so distinct module names render distinct load
        // targets and don't collide. Sweep both the bare two-load shape
        // and the canonical load-pair-with-cleanup shape so a future
        // tighten that over-fires on distinct loads surfaces here.
        let two_loads = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::LoadModule { module: "y".into() },
            ],
        );
        two_loads.validate().unwrap();
        let with_cleanup = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::LoadModule { module: "y".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::SoftPurge {
                    module: "y-old".into(),
                },
            ],
        );
        with_cleanup.validate().unwrap();
    }

    #[test]
    fn validate_accepts_single_load_per_module() {
        // Boundary control: a list with exactly one `:load-module`
        // followed by the canonical `:state-change` + `:soft-purge`
        // sequence (the module-doc OTP shape) is the gate's identity
        // element. Pin so a future off-by-one in the duplicate-
        // detection scan doesn't accidentally flag a single occurrence
        // as duplicating itself — mirrors
        // `validate_accepts_single_cleanup_per_module` on the sibling
        // singularity axis.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/migrations/v01-to-v02.lisp"),
                },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
            ],
        );
        e.validate().unwrap();
    }

    #[test]
    fn validate_load_singularity_fires_after_state_change_ordering() {
        // Diagnostic-precedence pin: an entry like `((:state-change
        // "m.lisp") (:load-module "x") (:load-module "x"))` is *both*
        // state-change-without-load and duplicate-load. The more-
        // fundamental ordering gate must win — the missing-load defect
        // is load-bearing (the migration runs against unloaded code),
        // and surfacing the duplicate diagnostic first would mask the
        // migrate-into-unloaded-code defect the ordering gate exists
        // to close. Guards the call order in `validate` against silent
        // reordering. Same posture as
        // `validate_cleanup_singularity_fires_after_purge_ordering`
        // on the sibling singularity gate.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::LoadModule { module: "x".into() },
            ],
        );
        let err = e.validate().unwrap_err();
        assert!(
            matches!(err, UpgradeError::StateChangeWithoutPriorLoad { .. }),
            "state-change-without-load must surface before duplicate-load, got {err:?}"
        );
    }

    #[test]
    fn validate_load_singularity_fires_after_purge_ordering() {
        // Diagnostic-precedence pin: an entry like `((:soft-purge
        // "x-old") (:load-module "x") (:load-module "x"))` is *both*
        // purge-without-load and duplicate-load. The more-fundamental
        // ordering gate must win — the missing-load defect is load-
        // bearing (the cleanup runs against no-replacement-window),
        // and surfacing the duplicate diagnostic first would mask the
        // drain-to-nothing defect the ordering gate exists to close.
        // Sibling of
        // `validate_cleanup_singularity_fires_after_purge_ordering` on
        // the load-singularity axis.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::LoadModule { module: "x".into() },
            ],
        );
        let err = e.validate().unwrap_err();
        assert!(
            matches!(
                err,
                UpgradeError::PurgeWithoutPriorLoad {
                    kind: crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
                    ..
                }
            ),
            "purge-without-load must surface before duplicate-load, got {err:?}"
        );
    }

    #[test]
    fn validate_load_singularity_fires_after_per_instr_shape() {
        // Order pin: a malformed `:module` value on a `:load-module`
        // (an empty string) surfaces its narrower kind-tagged
        // `ModuleEmpty` diagnostic *before* the within-entry load-
        // singularity gate fires. The per-instruction shape pass walks
        // the list inline before the singularity check, so the
        // narrower self-locating diagnostic surfaces first — mirrors
        // the empty-first cascade on every peer DNS-1123 gate and the
        // `validate_cleanup_singularity_fires_after_per_instr_shape`
        // pin on the sibling singularity gate.
        //
        // Two empty-string `:load-module` would *otherwise* duplicate
        // (both modules are the same empty string), so this pin
        // double-locks the precedence: the per-instr shape gate must
        // win on the first malformed instruction before the duplicate
        // scan even reaches the second.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule {
                    module: String::new(),
                },
                UpgradeInstruction::LoadModule {
                    module: String::new(),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::ModuleEmpty {
                kind: crate::render::M2_UPGRADE_INSTRUCTION_KIND_LOAD_MODULE,
            },
            "malformed instruction must surface its kind-tagged diagnostic before the \
             load-singularity gate fires, got {err:?}"
        );
    }

    #[test]
    fn validate_load_singularity_fires_before_cleanup_singularity() {
        // Diagnostic-precedence pin: an entry that violates *both*
        // singularities — duplicate load on "x" *and* duplicate cleanup
        // on "y-old" — must surface the load-side diagnostic first.
        // The load axis precedes the cleanup axis in the canonical OTP
        // sequence (`code:load_module/1` then `code:soft_purge/1`) and
        // in [`UpgradeInstruction`] declaration order (LoadModule
        // before SoftPurge/Purge), so the load-side singularity is the
        // load-bearing diagnostic when both fire — the cleanup-side
        // duplicate is meaningless either way without a coherent load.
        // Guards the call order in `validate`: `validate_load_singularity`
        // runs before `validate_cleanup_singularity`.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "y-old".into(),
                },
                UpgradeInstruction::SoftPurge {
                    module: "y-old".into(),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::DuplicateLoadModule {
                from: "0.1.0".into(),
                module: "x".into(),
            },
            "duplicate-load must surface before duplicate-cleanup, got {err:?}"
        );
    }

    #[test]
    fn validate_load_singularity_reports_first_collision() {
        // Determinism pin: with three loads of the same module the gate
        // reports the *first* collision (the second occurrence) and
        // stops — the third's duplicate is masked by the first surfaced
        // one. Mirrors
        // `validate_cleanup_singularity_reports_first_collision` on the
        // sibling singularity axis and every peer duplicate gate's
        // first-collision discipline.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::LoadModule { module: "x".into() },
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::DuplicateLoadModule {
                from: "0.1.0".into(),
                module: "x".into(),
            },
            "the first colliding occurrence must surface, not the later third-load collision"
        );
    }

    #[test]
    fn validate_load_singularity_threads_through_validate_upgrade_from() {
        // The whole-list entry-point surfaces the per-entry singularity
        // error (mirrors
        // `validate_cleanup_singularity_threads_through_validate_upgrade_from`):
        // the gate is reachable from the LayoutInvariants call site,
        // not only from a direct `entry.validate()`.
        let entries = vec![entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::LoadModule { module: "x".into() },
            ],
        )];
        let err = validate_upgrade_from(&entries).unwrap_err();
        assert!(
            matches!(err, UpgradeError::DuplicateLoadModule { .. }),
            "validate_upgrade_from must thread the load-singularity error, got {err:?}"
        );
    }

    // ── within-entry state-change-singularity invariant ────────────────

    #[test]
    fn validate_rejects_duplicate_state_change_for_same_script() {
        // `StateChange` is the `gen_server:code_change/3` analog
        // (INSPIRATIONS §II.4): the script folds the prior-version
        // state shape into the current-version shape — a one-shot
        // transition, not a step that composes with itself. OTP's
        // release_handler invokes `code_change/3` exactly once per
        // upgrade per gen_server; systools-generated `.relup` files
        // emit at most one `code_change` per gen_server per upgrade
        // step for this reason. A second `(:state-change "m.lisp")`
        // re-runs the same fold on the already-migrated state — at
        // best a no-op and at worst silent state corruption from
        // double-applied non-idempotent transforms (`add column`,
        // `increment counter`, `rename field`). Author one
        // `(:state-change "m.lisp")` per migration script per entry.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/migrations/v01-to-v02.lisp"),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/migrations/v01-to-v02.lisp"),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::DuplicateStateChange {
                from: "0.1.0".into(),
                script: PathBuf::from("lib/migrations/v01-to-v02.lisp"),
            },
            "two `:state-change` of the same script must surface as DuplicateStateChange naming \
             the script, got {err:?}"
        );
    }

    #[test]
    fn validate_accepts_distinct_state_change_scripts() {
        // Positive control: `:state-change` instructions on *different*
        // scripts pass the gate. Mirrors
        // `validate_accepts_distinct_cleanup_modules` /
        // `validate_accepts_distinct_load_modules` on the sibling
        // singularity axes — the state-change-singularity gate is keyed
        // on the script PathBuf, so distinct scripts render distinct
        // migration targets and don't collide. Sweep both the bare two-
        // migration shape and the canonical load-pair-with-cleanup shape
        // so a future tighten that over-fires on distinct scripts
        // surfaces here. This positive control is the gate-level peer of
        // `validate_accepts_multiple_state_changes_after_one_load` (the
        // ordering-gate positive control on distinct scripts), pinned
        // here independently so a future refactor that decouples the
        // gates can't accidentally drop coverage on either.
        let two_migrations = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m1.lisp"),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m2.lisp"),
                },
            ],
        );
        two_migrations.validate().unwrap();
        let with_cleanup = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m1.lisp"),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m2.lisp"),
                },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
            ],
        );
        with_cleanup.validate().unwrap();
    }

    #[test]
    fn validate_accepts_single_state_change_per_script() {
        // Boundary control: a list with exactly one `:state-change`
        // wrapped by the canonical `:load-module` + `:soft-purge`
        // sequence (the module-doc OTP shape) is the gate's identity
        // element. Pin so a future off-by-one in the duplicate-
        // detection scan doesn't accidentally flag a single occurrence
        // as duplicating itself — mirrors
        // `validate_accepts_single_load_per_module` /
        // `validate_accepts_single_cleanup_per_module` on the sibling
        // singularity axes.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/migrations/v01-to-v02.lisp"),
                },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
            ],
        );
        e.validate().unwrap();
    }

    #[test]
    fn validate_state_change_singularity_fires_after_state_change_ordering() {
        // Diagnostic-precedence pin: an entry like `((:state-change
        // "m.lisp") (:state-change "m.lisp"))` is *both* state-change-
        // without-load and duplicate-state-change. The more-fundamental
        // ordering gate must win — the missing-load defect is load-
        // bearing (the migration runs against unloaded code), and
        // surfacing the duplicate diagnostic first would mask the
        // migrate-into-unloaded-code defect the ordering gate exists to
        // close. Guards the call order in `validate` against silent
        // reordering. Same posture as
        // `validate_load_singularity_fires_after_state_change_ordering`
        // on the sibling singularity gate.
        //
        // Two same-script `:state-change` would *otherwise* duplicate
        // (both scripts collide on the very first `:state-change`-
        // without-load encountered), so this pin double-locks the
        // precedence: the ordering gate must win on the first un-loaded
        // `:state-change` before the singularity scan even reaches the
        // second.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert!(
            matches!(err, UpgradeError::StateChangeWithoutPriorLoad { .. }),
            "state-change-without-load must surface before duplicate-state-change, got {err:?}"
        );
    }

    #[test]
    fn validate_state_change_singularity_fires_after_purge_ordering() {
        // Diagnostic-precedence pin: an entry like `((:soft-purge
        // "x-old") (:load-module "x") (:state-change "m.lisp")
        // (:state-change "m.lisp"))` is *both* purge-without-load and
        // duplicate-state-change. The more-fundamental ordering gate
        // must win — the missing-load defect (a cleanup that drains the
        // only resident version to nothing) is load-bearing, and
        // surfacing the duplicate diagnostic first would mask the
        // drain-to-nothing defect the ordering gate exists to close.
        // Sibling of `validate_load_singularity_fires_after_purge_ordering`
        // on the state-change-singularity axis.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert!(
            matches!(
                err,
                UpgradeError::PurgeWithoutPriorLoad {
                    kind: crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
                    ..
                }
            ),
            "purge-without-load must surface before duplicate-state-change, got {err:?}"
        );
    }

    #[test]
    fn validate_state_change_singularity_fires_after_per_instr_shape() {
        // Order pin: a malformed `:script` value on a `:state-change`
        // (an empty path) surfaces its narrower `EmptyScript` diagnostic
        // *before* the within-entry state-change-singularity gate fires.
        // The per-instruction shape pass walks the list inline before
        // the singularity check, so the narrower self-locating
        // diagnostic surfaces first — mirrors the empty-first cascade on
        // every peer path-shape gate and the
        // `validate_load_singularity_fires_after_per_instr_shape` /
        // `validate_cleanup_singularity_fires_after_per_instr_shape`
        // pins on the sibling singularity gates.
        //
        // Two empty-path `:state-change` would *otherwise* duplicate
        // (both scripts are the same empty PathBuf), so this pin double-
        // locks the precedence: the per-instr shape gate must win on the
        // first malformed instruction before the duplicate scan even
        // reaches the second.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::new(),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::new(),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::EmptyScript,
            "malformed instruction must surface its narrower diagnostic before the \
             state-change-singularity gate fires, got {err:?}"
        );
    }

    #[test]
    fn validate_state_change_singularity_fires_after_load_singularity() {
        // Diagnostic-precedence pin: an entry that violates *both*
        // singularities — duplicate load on "x" *and* duplicate
        // state-change on "m.lisp" — must surface the load-side
        // diagnostic first. The load axis precedes the migration axis
        // in the canonical OTP sequence (`code:load_module/1` then
        // `gen_server:code_change/3`) and in [`UpgradeInstruction`]
        // declaration order (LoadModule before StateChange), so the
        // load-side singularity is the load-bearing diagnostic when
        // both fire — the migration-side duplicate is meaningless
        // either way without a coherent load. Guards the call order in
        // `validate`: `validate_load_singularity` runs before
        // `validate_state_change_singularity`.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::DuplicateLoadModule {
                from: "0.1.0".into(),
                module: "x".into(),
            },
            "duplicate-load must surface before duplicate-state-change, got {err:?}"
        );
    }

    #[test]
    fn validate_state_change_singularity_fires_before_cleanup_singularity() {
        // Diagnostic-precedence pin: an entry that violates *both*
        // singularities — duplicate state-change on "m.lisp" *and*
        // duplicate cleanup on "y-old" — must surface the migration-
        // side diagnostic first. The migration axis precedes the
        // cleanup axis in the canonical OTP sequence
        // (`gen_server:code_change/3` then `code:soft_purge/1`) and in
        // [`UpgradeInstruction`] declaration order (StateChange before
        // SoftPurge/Purge), so the migration-side singularity is the
        // load-bearing diagnostic when both fire — the cleanup-side
        // duplicate is irrelevant once the migration has corrupted
        // state by double-applying. Guards the call order in
        // `validate`: `validate_state_change_singularity` runs before
        // `validate_cleanup_singularity`.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                UpgradeInstruction::SoftPurge {
                    module: "y-old".into(),
                },
                UpgradeInstruction::SoftPurge {
                    module: "y-old".into(),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::DuplicateStateChange {
                from: "0.1.0".into(),
                script: PathBuf::from("lib/m.lisp"),
            },
            "duplicate-state-change must surface before duplicate-cleanup, got {err:?}"
        );
    }

    #[test]
    fn validate_state_change_singularity_reports_first_collision() {
        // Determinism pin: with three state-changes on the same script
        // the gate reports the *first* collision (the second
        // occurrence) and stops — the third's duplicate is masked by
        // the first surfaced one. Mirrors
        // `validate_load_singularity_reports_first_collision` /
        // `validate_cleanup_singularity_reports_first_collision` on the
        // sibling singularity axes and every peer duplicate gate's
        // first-collision discipline.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::DuplicateStateChange {
                from: "0.1.0".into(),
                script: PathBuf::from("lib/m.lisp"),
            },
            "the first colliding occurrence must surface, not the later third-migration collision"
        );
    }

    #[test]
    fn validate_state_change_singularity_threads_through_validate_upgrade_from() {
        // The whole-list entry-point surfaces the per-entry singularity
        // error (mirrors
        // `validate_load_singularity_threads_through_validate_upgrade_from`
        // / `validate_cleanup_singularity_threads_through_validate_upgrade_from`):
        // the gate is reachable from the LayoutInvariants call site,
        // not only from a direct `entry.validate()`.
        let entries = vec![entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
            ],
        )];
        let err = validate_upgrade_from(&entries).unwrap_err();
        assert!(
            matches!(err, UpgradeError::DuplicateStateChange { .. }),
            "validate_upgrade_from must thread the state-change-singularity error, got {err:?}"
        );
    }

    #[test]
    fn validate_state_change_singularity_projects_scripts_through_declared_path_accessor() {
        // Composition pin: [`UpgradeFromEntry::validate_state_change_singularity`]'s
        // per-instruction `StateChange`-arm script-path projection must
        // route through the sibling lifted
        // [`UpgradeInstruction::declared_path`] `Option<&PathBuf>`
        // accessor, not the raw
        // `match instr { UpgradeInstruction::StateChange { script } =>
        // script.as_path(), _ => continue }` open-coded pattern-match
        // the gate previously carried.
        //
        // Structurally: the gate's projection accept-set is the union
        // of every [`UpgradeInstruction`] variant for which
        // `declared_path().is_some()` — today exactly
        // [`UpgradeInstruction::StateChange`] per the sibling
        // `declared_path_only_for_state_change` pin, so a
        // duplicate-scripts input trips `DuplicateStateChange` and a
        // non-`StateChange` input (module-bearing / terminal) leaves
        // `seen` empty and the gate returns `Ok(())` byte-identical to
        // the pattern-match shape.
        //
        // Byte-equal today (`declared_path` returns `Some(script)` iff
        // `StateChange`, byte-for-byte from the variant's own storage);
        // the pin catches any future accessor extension that promotes
        // an additional variant onto the `PathBuf`-carrying axis — the
        // gate then fires on duplicate scripts from that variant too,
        // and the singularity discipline the sibling
        // `validate_load_singularity` / `validate_cleanup_singularity`
        // gates share on the `String`-carrying axis's per-variant
        // consumers extends to the promoted variant by construction.
        //
        // Peer of the sibling four per-`UpgradeInstruction` consumers
        // ([`UpgradeInstruction::validate`]'s per-`StateChange`
        // sandbox-path fan-out, the layout-side per-`StateChange`
        // script-existence fan-out at
        // `caixa-core/src/layout.rs:1017`, the cross-slot
        // [`validate_upgrade_from_against_behavior`] gate's per-
        // `StateChange` detection loop, the peer
        // [`UpgradeInstruction::declared_module`] `String`-axis
        // per-variant unifier) — this gate now shares one typed
        // dispatch on the substrate primitive's `PathBuf`-carrying
        // axis with those consumers, so a future rebrand on the axis
        // migrates as a single caixa-core edit rather than a
        // coordinated rewrite of five call sites.
        //
        // Three-arm projective coverage:
        //   (a) `StateChange` scripts project through `declared_path()`
        //       byte-equal to the raw `script.as_path()` field access;
        //   (b) a duplicate-`StateChange` input trips the gate on the
        //       second occurrence with `DuplicateStateChange` carrying
        //       the offending script verbatim;
        //   (c) a non-`StateChange`-only input (`LoadModule` /
        //       `SoftPurge` / `Purge` / `Restart`) leaves the gate
        //       vacuous with `Ok(())` — the `declared_path().is_none()`
        //       arm's `continue` fall-through pins.
        //
        // Fail-before-pass-after verified locally: swapping the
        // production `let Some(script) = instr.declared_path() else {
        // continue };` back to `let script = match instr {
        // UpgradeInstruction::StateChange { script } =>
        // script.as_path(), _ => continue, };` keeps arms (a)-(c)
        // passing but silently detaches the gate from the accessor's
        // typed dispatch — any future `declared_path` extension
        // (promotion of an additional variant onto the axis, an
        // operator-side pre-resolved-path cache the accessor
        // materializes) would then silently disagree between this
        // gate's raw pattern-match and the peer four sibling consumers
        // that route through the accessor.
        use std::path::PathBuf;

        // (a) StateChange projection byte-equal via declared_path.
        let sc = UpgradeInstruction::StateChange {
            script: PathBuf::from("lib/m.lisp"),
        };
        assert_eq!(
            sc.declared_path().map(std::path::PathBuf::as_path),
            Some(PathBuf::from("lib/m.lisp").as_path()),
            "declared_path() must project the StateChange :script byte-equal to the raw \
             field access — accessor divergence would silently detach the gate from the \
             projection every peer per-`UpgradeInstruction` consumer routes through"
        );

        // (b) Duplicate-StateChange input trips the gate.
        let dup = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
            ],
        );
        assert_eq!(
            dup.validate_state_change_singularity(),
            Err(UpgradeError::DuplicateStateChange {
                from: "0.1.0".into(),
                script: PathBuf::from("lib/m.lisp"),
            }),
            "duplicate StateChange scripts must trip the gate on the second occurrence \
             through the declared_path accessor's Some(script) arm"
        );

        // (c) Non-StateChange-only inputs leave the gate vacuous.
        for instrs in [
            vec![UpgradeInstruction::LoadModule { module: "x".into() }],
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
            ],
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::Purge {
                    module: "x-old".into(),
                },
            ],
            vec![UpgradeInstruction::Restart],
        ] {
            for instr in &instrs {
                assert!(
                    instr.declared_path().is_none(),
                    "non-StateChange variants must project None through declared_path — \
                     accessor divergence would let this gate silently fire on a duplicate \
                     module reference far from any :state-change site"
                );
            }
            let e = entry("0.1.0", instrs);
            assert_eq!(
                e.validate_state_change_singularity(),
                Ok(()),
                "the state-change-singularity gate must return Ok(()) on an entry whose \
                 instructions all project None through declared_path — the accessor's \
                 continue arm the pattern-match's `_ => continue` previously carried"
            );
        }
    }

    // ── within-entry state-change-before-cleanup ordering invariant ──

    #[test]
    fn validate_rejects_state_change_after_soft_purge() {
        // Fail-before-pass-after pin: `:state-change` is the
        // gen_server:code_change/3 analog and folds the prior-version
        // state shape into the current shape; `:soft-purge` drains the
        // prior code. The operator runs instructions in declared order,
        // so a `:soft-purge` ahead of a `:state-change` drains the
        // prior module before the migration callback runs against the
        // state it held — the canonical OTP error mode
        // "`code_change/3` invoked on a purged module" the
        // release_handler closes by always ordering the migration
        // before the cleanup.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::StateChangeAfterCleanup {
                from: "0.1.0".into(),
                script: PathBuf::from("lib/m.lisp"),
                prior_cleanup_kind: crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
                prior_cleanup_module: "x-old".into(),
            },
            "a `:state-change` after a `:soft-purge` must surface as StateChangeAfterCleanup \
             naming the offending entry + script + the prior cleanup's kind/module, got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_state_change_after_purge() {
        // Per-arm coverage: `:purge` (immediate discard, no drain) is
        // the more catastrophic peer of `:soft-purge` on the cleanup
        // axis; same gate, same shape, the `prior_cleanup_kind` field
        // distinguishes the diagnostic so the author can grep their
        // caixa.lisp for the offending `(:purge …)` form.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::Purge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::StateChangeAfterCleanup {
                from: "0.1.0".into(),
                script: PathBuf::from("lib/m.lisp"),
                prior_cleanup_kind: crate::render::M2_UPGRADE_INSTRUCTION_KIND_PURGE,
                prior_cleanup_module: "x-old".into(),
            },
            "a `:state-change` after a `:purge` must surface as StateChangeAfterCleanup with \
             `prior_cleanup_kind: \":purge\"`, got {err:?}"
        );
    }

    #[test]
    fn validate_accepts_state_change_before_cleanup() {
        // Positive control: the canonical `(:load-module …)
        // (:state-change …) (:soft-purge …)` order validates — the
        // exact shape the module doc example and `validate_accepts_
        // well_formed` already pin, restated here on the new gate's
        // identity element so a future shortcut that runs the
        // singularity gates first doesn't silently mask a regression
        // here.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
            ],
        );
        e.validate().unwrap();
    }

    #[test]
    fn validate_accepts_cleanup_without_state_change() {
        // Empty-set identity: an entry that carries no `:state-change`
        // at all has nothing to order against the cleanup, so the gate
        // passes regardless of how the cleanups are placed (after the
        // single required `:load-module`). Mirrors the
        // `validate_accepts_multiple_purges_after_one_load` positive
        // control on the peer purge-ordering gate; metadata-only
        // upgrades with cleanup-but-no-migration land here.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::Purge {
                    module: "x-oldest".into(),
                },
            ],
        );
        e.validate().unwrap();
    }

    #[test]
    fn validate_accepts_state_change_without_cleanup() {
        // Empty-set identity on the dual axis: an entry that carries no
        // cleanup at all has nothing to order against the state-change,
        // so the gate passes — additive-upgrade shapes (load new code,
        // migrate state, leave old code resident for in-flight callers
        // to drain naturally) land here.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
            ],
        );
        e.validate().unwrap();
    }

    #[test]
    fn validate_accepts_multiple_state_changes_before_cleanup() {
        // Coverage: every state-change must precede every cleanup, not
        // just the first. A chain `(load) (sc) (sc) (sp)` is the
        // canonical "two distinct migration scripts on a chained
        // upgrade" shape (one module's schema *and* another's
        // projection per the DuplicateStateChange diagnostic), and
        // it must pass when each state-change has distinct script
        // paths. Pinned here so a future shortcut that only checks
        // the first state-change doesn't silently accept a
        // `(load) (sc-1) (sp) (sc-2)` regression.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m1.lisp"),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m2.lisp"),
                },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
            ],
        );
        e.validate().unwrap();
    }

    #[test]
    fn validate_rejects_state_change_sandwiched_between_cleanups() {
        // First-cleanup-wins pin: an entry like `(load) (sp-1) (sc)
        // (sp-2)` violates the gate because the state-change runs
        // after the first cleanup. The reported `prior_cleanup_*`
        // names the *first* cleanup (the load-bearing one), not the
        // last — mirrors every peer first-collision diagnostic
        // posture on this module (`validate_state_change_ordering`,
        // `validate_purge_ordering`, `validate_load_singularity`,
        // `validate_state_change_singularity`,
        // `validate_cleanup_singularity` all report the first
        // colliding instruction, not the last).
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                UpgradeInstruction::Purge {
                    module: "y-old".into(),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::StateChangeAfterCleanup {
                from: "0.1.0".into(),
                script: PathBuf::from("lib/m.lisp"),
                prior_cleanup_kind: crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
                prior_cleanup_module: "x-old".into(),
            },
            "the first cleanup the state-change follows must surface (not the trailing one), \
             got {err:?}"
        );
    }

    #[test]
    fn validate_state_change_before_cleanup_fires_after_purge_ordering() {
        // Diagnostic-precedence pin: an entry like `((:soft-purge
        // "x-old") (:load-module "x") (:state-change "m.lisp"))` is
        // *both* purge-without-load (the cleanup runs before the
        // load) and state-change-after-cleanup (the state-change
        // runs after the cleanup). The more-fundamental ordering
        // gate must win — the missing-load defect (a cleanup that
        // drains the only resident version to nothing) is load-
        // bearing, and surfacing the state-change-after-cleanup
        // diagnostic first would mask the drain-to-nothing defect
        // the peer purge-ordering gate exists to close. Guards the
        // call order in `validate` against silent reordering. Same
        // posture as `validate_purge_ordering_fires_after_state_
        // change_ordering` on the sibling ordering gate.
        //
        // Pin specifically uses the load-after-cleanup shape (rather
        // than load-less) so the state-change-ordering gate (which
        // would otherwise fire first on a `((:soft-purge …)
        // (:state-change …))` shape with no leading load) is
        // sidestepped: with the load present after the cleanup,
        // state-change-ordering passes (its `loaded` latch is set
        // before the state-change is encountered) but purge-ordering
        // still fails (the cleanup precedes the load). That isolates
        // the precedence between purge-ordering and this gate
        // cleanly.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert!(
            matches!(
                err,
                UpgradeError::PurgeWithoutPriorLoad {
                    kind: crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
                    ..
                }
            ),
            "purge-without-load must surface before state-change-after-cleanup, got {err:?}"
        );
    }

    #[test]
    fn validate_state_change_before_cleanup_fires_after_state_change_ordering() {
        // Diagnostic-precedence pin: an entry like `((:state-change
        // "m.lisp") (:soft-purge "x-old"))` is state-change-without-
        // load (because no `:load-module` precedes the state-change)
        // but *not* state-change-after-cleanup (the state-change
        // precedes the cleanup textually). The state-change-ordering
        // gate must surface first regardless — the missing-load
        // defect on the migration axis is the load-bearing semantic
        // and surfacing a different ordering diagnostic would mask
        // the migration-against-stale-code defect. Guards the call
        // order in `validate` against silent reordering on a shape
        // that fires only the state-change-ordering gate (not this
        // one), pinning that the state-change-ordering gate wins
        // ahead of this gate's chance to look at the list.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert!(
            matches!(err, UpgradeError::StateChangeWithoutPriorLoad { .. }),
            "state-change-without-load must surface before purge-without-load (the canonical \
             validate_purge_ordering_fires_after_state_change_ordering pin), got {err:?}"
        );
    }

    #[test]
    fn validate_state_change_before_cleanup_fires_after_per_instr_shape() {
        // Order pin: a malformed `:script` value on a `:state-change`
        // (an empty path) surfaces its narrower `EmptyScript`
        // diagnostic *before* the within-entry state-change-before-
        // cleanup gate fires. The per-instruction shape pass walks
        // the list inline before the ordering check, so the narrower
        // self-locating diagnostic surfaces first — mirrors the
        // empty-first cascade on every peer path-shape gate and the
        // `validate_purge_ordering_fires_after_per_instr_shape` pin
        // on the sibling ordering gate.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::new(),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::EmptyScript,
            "malformed instruction must surface its narrower diagnostic before the \
             state-change-before-cleanup gate fires, got {err:?}"
        );
    }

    #[test]
    fn validate_state_change_before_cleanup_fires_before_state_change_singularity() {
        // Diagnostic-precedence pin: an entry like `((:load-module
        // "x") (:soft-purge "x-old") (:state-change "m.lisp")
        // (:state-change "m.lisp"))` violates *both* this ordering
        // gate (the first state-change follows the cleanup) and the
        // state-change-singularity gate (the same script appears
        // twice). The ordering gate must win — the canonical
        // "ordering before singularity" precedence the peer
        // `validate_state_change_ordering` / `validate_purge_
        // ordering` gates already establish over their own singularity
        // gates, applied uniformly across the OTP canonical-sequence
        // ordering axis here. Guards the call order in `validate`:
        // `validate_state_change_before_cleanup` runs before the
        // per-instruction-class singularity gates.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
            ],
        );
        let err = e.validate().unwrap_err();
        assert!(
            matches!(err, UpgradeError::StateChangeAfterCleanup { .. }),
            "state-change-after-cleanup must surface before duplicate-state-change, got {err:?}"
        );
    }

    #[test]
    fn validate_state_change_before_cleanup_threads_through_validate_upgrade_from() {
        // The whole-list entry-point surfaces the per-entry ordering
        // error (mirrors `validate_purge_ordering_threads_through_
        // validate_upgrade_from` and every peer wiring pin): the gate
        // is reachable from the LayoutInvariants call site, not only
        // from a direct `entry.validate()`.
        let entries = vec![entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
            ],
        )];
        let err = validate_upgrade_from(&entries).unwrap_err();
        assert!(
            matches!(err, UpgradeError::StateChangeAfterCleanup { .. }),
            "validate_upgrade_from must thread the state-change-before-cleanup error, \
             got {err:?}"
        );
    }

    #[test]
    fn validate_restart_order_independent() {
        // Position-agnostic: `(:restart)` leading or trailing the
        // mixed sequence surfaces the same RestartNotExclusive shape.
        // Mirrors OTP appup's order-insensitive
        // `restart_emulator | restart_new_emulator` terminal rule —
        // the position of the restart instruction in the script is
        // irrelevant; what matters is the script *contains* it
        // alongside other instructions at all. The gate must not
        // gain a false positive by depending on instruction ordering.
        let leading = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::Restart,
                UpgradeInstruction::LoadModule { module: "x".into() },
            ],
        );
        let trailing = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::Restart,
            ],
        );
        let middle = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "a".into() },
                UpgradeInstruction::Restart,
                UpgradeInstruction::SoftPurge {
                    module: "a-old".into(),
                },
            ],
        );
        for e in [&leading, &trailing, &middle] {
            assert!(
                matches!(
                    e.validate().unwrap_err(),
                    UpgradeError::RestartNotExclusive {
                        restart_count: 1,
                        ..
                    }
                ),
                "mixed-with-:restart entry must surface RestartNotExclusive regardless of \
                 instruction order, got {:?}",
                e.validate()
            );
        }
    }

    #[test]
    fn validate_restart_exclusive_fires_after_per_instr_shape() {
        // Order pin: a malformed `:module` value on a Module-bearing
        // instruction (an empty string) surfaces its narrower
        // kind-tagged `ModuleEmpty` diagnostic *before* the within-
        // entry restart-exclusivity gate fires. The per-instruction
        // shape pass walks the list inline before the restart-
        // exclusive check, so the narrower self-locating diagnostic
        // surfaces first — mirrors the empty-first cascade on every
        // peer DNS-1123 gate (`validate_module`,
        // `validate_membro_caixa`, `validate_placement_cluster`) and
        // the `*_invalid_fires_before_duplicate_check` arm-ordering
        // pins on every typed-graph axis. Without this pin a future
        // shortcut that runs the restart-exclusive check ahead of
        // per-instruction shape would surface a less-actionable
        // RestartNotExclusive over an instruction list that's also
        // malformed at the per-instruction layer.
        let e = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule {
                    module: String::new(),
                },
                UpgradeInstruction::Restart,
            ],
        );
        let err = e.validate().unwrap_err();
        assert_eq!(
            err,
            UpgradeError::ModuleEmpty {
                kind: crate::render::M2_UPGRADE_INSTRUCTION_KIND_LOAD_MODULE
            },
            "malformed instruction must surface its kind-tagged diagnostic before the \
             restart-exclusivity gate fires, got {err:?}"
        );
    }

    fn behavior_with_state_change_callback() -> crate::BehaviorSpec {
        // Helper for the cross-slot composition gate's pass arm: a
        // BehaviorSpec carrying just the `:on-state-change` callback,
        // the runtime hook the per-version `(:state-change "…")`
        // instruction is delivered through during hot upgrade. Mirrors
        // the canonical authoring shape pinned in the module doc.
        crate::BehaviorSpec {
            on_state_change: Some(PathBuf::from("lib/migrations.lisp")),
            ..Default::default()
        }
    }

    #[test]
    fn behavior_gate_rejects_state_change_without_any_behavior() {
        // `:upgrade-from` with a `(:state-change "lib/m.lisp")` and the
        // caixa carries no `:behavior` at all surfaces the missing-
        // callback diagnostic naming the offending entry's `:from` +
        // script. The "I added the upgrade path but never declared
        // `:behavior`" footgun: `:behavior` is optional at the typed
        // root, the typed `:upgrade-from` slot validates on its own
        // merits, and the operator's hot-upgrade dispatch reaches for
        // a callback that doesn't exist.
        let entries = vec![entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
            ],
        )];
        let err = validate_upgrade_from_against_behavior(&entries, None).unwrap_err();
        assert_eq!(
            err,
            UpgradeError::StateChangeWithoutOnStateChangeCallback {
                from: "0.1.0".into(),
                script: PathBuf::from("lib/m.lisp"),
            },
        );
    }

    #[test]
    fn behavior_gate_rejects_state_change_when_on_state_change_is_none() {
        // `:behavior` declared with *other* callbacks set
        // (`:on-init`, `:on-terminate`, etc.) but `:on-state-change`
        // None still surfaces the missing-callback diagnostic — only
        // the `:on-state-change` axis matters for this gate. The
        // "I declared `:behavior` but missed the migration callback"
        // footgun: a caixa that registers its lifecycle hooks but
        // forgets the migration delivery path leaves the
        // `:state-change` instruction with no runtime hook to
        // dispatch through.
        let entries = vec![entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
            ],
        )];
        let b = crate::BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            on_terminate: Some(PathBuf::from("lib/cleanup.lisp")),
            ..Default::default()
        };
        let err = validate_upgrade_from_against_behavior(&entries, Some(&b)).unwrap_err();
        assert_eq!(
            err,
            UpgradeError::StateChangeWithoutOnStateChangeCallback {
                from: "0.1.0".into(),
                script: PathBuf::from("lib/m.lisp"),
            },
            "only `:on-state-change` satisfies the composition; other callbacks must not mask \
             the missing migration hook"
        );
    }

    #[test]
    fn behavior_gate_accepts_state_change_with_on_state_change_callback() {
        // The canonical composition shape: a per-version
        // `(:state-change "lib/m.lisp")` instruction paired with the
        // `:behavior :on-state-change "lib/migrations.lisp"` callback
        // it is delivered through at hot-upgrade time. Pins the gate's
        // pass arm — drift here = a future tighten that rejects the
        // canonical OTP-shape composition surfaces as a regression at
        // this positive-control pin.
        let entries = vec![entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
            ],
        )];
        let b = behavior_with_state_change_callback();
        validate_upgrade_from_against_behavior(&entries, Some(&b)).unwrap();
    }

    #[test]
    fn behavior_gate_accepts_entries_without_any_state_change() {
        // Empty-set identity: entries carrying no `:state-change`
        // instruction at all (load + cleanup only — the metadata-only
        // upgrade shape the module doc names, "On any failure, the
        // current version stays load-bearing — a typed atomic
        // upgrade") leave the gate vacuous. The composition only
        // requires a callback when the per-version script exists; a
        // load + cleanup pair has no migration to deliver, so the
        // absence of `:on-state-change` is coherent.
        let entries = vec![entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
            ],
        )];
        validate_upgrade_from_against_behavior(&entries, None).unwrap();
    }

    #[test]
    fn behavior_gate_accepts_restart_only_entry() {
        // The terminal-fallback `((:restart))` shape carries no
        // `:state-change` — the operator restarts the pod and the
        // new version comes up fresh against its initial state, no
        // migration. Pinned alongside the metadata-only positive
        // control above as the second empty-state-change shape.
        let entries = vec![entry("0.1.0", vec![UpgradeInstruction::Restart])];
        validate_upgrade_from_against_behavior(&entries, None).unwrap();
    }

    #[test]
    fn behavior_gate_accepts_empty_entries_list() {
        // Empty `:upgrade-from` (a caixa with no declared upgrade
        // paths — the v0.1.0 caixa before any upgrade entries are
        // added) trivially passes the gate. Pinned so the gate
        // doesn't accidentally fire on a caixa that hasn't yet
        // declared any upgrades.
        let entries: Vec<UpgradeFromEntry> = vec![];
        validate_upgrade_from_against_behavior(&entries, None).unwrap();
    }

    #[test]
    fn behavior_gate_reports_first_state_change_in_first_entry() {
        // First-collision determinism: with multiple `:state-change`
        // instructions across multiple entries, the gate reports the
        // *first* one encountered in declaration order — the entry's
        // declaration order first, then the within-entry instruction
        // order. Mirrors every peer first-collision diagnostic posture
        // on this module (`validate_state_change_ordering`,
        // `validate_purge_ordering`, the singularity gates), so a
        // future shortcut that walks the list in reverse or returns
        // the last collision surfaces as a regression here.
        let entries = vec![
            entry(
                "0.1.0",
                vec![
                    UpgradeInstruction::LoadModule { module: "x".into() },
                    UpgradeInstruction::StateChange {
                        script: PathBuf::from("lib/m1.lisp"),
                    },
                    UpgradeInstruction::StateChange {
                        script: PathBuf::from("lib/m2.lisp"),
                    },
                ],
            ),
            entry(
                "0.1.5",
                vec![
                    UpgradeInstruction::LoadModule { module: "x".into() },
                    UpgradeInstruction::StateChange {
                        script: PathBuf::from("lib/m3.lisp"),
                    },
                ],
            ),
        ];
        let err = validate_upgrade_from_against_behavior(&entries, None).unwrap_err();
        assert_eq!(
            err,
            UpgradeError::StateChangeWithoutOnStateChangeCallback {
                from: "0.1.0".into(),
                script: PathBuf::from("lib/m1.lisp"),
            },
            "the first :state-change in the first entry must surface, not later collisions"
        );
    }

    #[test]
    fn behavior_gate_reports_second_entry_when_first_has_no_state_change() {
        // Cross-entry pin: a first entry with no `:state-change` (just
        // a load + cleanup) leaves the gate's per-entry walk continuing
        // to the second entry, where the offending instruction lives.
        // The diagnostic names the *second* entry's `:from` because
        // that's where the missing-callback shape is exposed — pinned
        // so a shortcut that bails on the first entry without a
        // `:state-change` (rather than continuing) doesn't mask the
        // defect in a later entry.
        let entries = vec![
            entry(
                "0.1.0",
                vec![
                    UpgradeInstruction::LoadModule { module: "x".into() },
                    UpgradeInstruction::SoftPurge {
                        module: "x-old".into(),
                    },
                ],
            ),
            entry(
                "0.1.5",
                vec![
                    UpgradeInstruction::LoadModule { module: "x".into() },
                    UpgradeInstruction::StateChange {
                        script: PathBuf::from("lib/m.lisp"),
                    },
                ],
            ),
        ];
        let err = validate_upgrade_from_against_behavior(&entries, None).unwrap_err();
        assert_eq!(
            err,
            UpgradeError::StateChangeWithoutOnStateChangeCallback {
                from: "0.1.5".into(),
                script: PathBuf::from("lib/m.lisp"),
            },
            "the offending entry's `:from` must surface even when an earlier entry carries no \
             :state-change"
        );
    }

    #[test]
    fn behavior_gate_does_not_fire_when_callback_is_declared_across_many_entries() {
        // Positive control: a multi-entry `:upgrade-from` (chained
        // upgrades from v0.1.0 *and* v0.1.5) where every entry carries
        // a `:state-change` passes when the callback is declared once
        // at the caixa root. The callback is a single per-caixa
        // runtime hook; one declaration covers every entry's
        // `:state-change`, mirroring OTP's
        // `release_handler:install_release/1` which dispatches every
        // appup's `code_change` instruction through the single
        // `gen_server:code_change/3` callback registered on the
        // module.
        let entries = vec![
            entry(
                "0.1.0",
                vec![
                    UpgradeInstruction::LoadModule { module: "x".into() },
                    UpgradeInstruction::StateChange {
                        script: PathBuf::from("lib/m1.lisp"),
                    },
                ],
            ),
            entry(
                "0.1.5",
                vec![
                    UpgradeInstruction::LoadModule { module: "x".into() },
                    UpgradeInstruction::StateChange {
                        script: PathBuf::from("lib/m2.lisp"),
                    },
                ],
            ),
        ];
        let b = behavior_with_state_change_callback();
        validate_upgrade_from_against_behavior(&entries, Some(&b)).unwrap();
    }

    #[test]
    fn behavior_gate_accepts_load_and_cleanup_only_when_behavior_carries_on_state_change() {
        // Symmetry pin: the gate's pass arm doesn't depend on the
        // entry actually carrying a `:state-change` — if no
        // `:state-change` is declared, the gate is vacuous regardless
        // of the callback (an `:on-state-change` declared without a
        // matching per-version script is fine, the callback is the
        // runtime default for any *future* migration the author hasn't
        // yet added). Pins that a caixa author can declare the
        // callback ahead of any migration without the gate
        // complaining.
        let entries = vec![entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
            ],
        )];
        let b = behavior_with_state_change_callback();
        validate_upgrade_from_against_behavior(&entries, Some(&b)).unwrap();
    }

    #[test]
    fn validate_restart_exclusive_threads_through_validate_upgrade_from() {
        // Wiring pin: the within-entry restart-exclusivity gate fires
        // through [`validate_upgrade_from`] (which delegates to
        // [`UpgradeFromEntry::validate`] per entry) before the cross-
        // entry duplicate-`:from` gate would have a chance to run on
        // the malformed entry. Pinned here so a future refactor that
        // walks the cross-entry gate first doesn't accidentally
        // surface a DuplicateFrom over an entry that's also malformed
        // at the within-entry restart-exclusivity layer.
        let entries = vec![
            entry(
                "0.1.0",
                vec![
                    UpgradeInstruction::LoadModule { module: "x".into() },
                    UpgradeInstruction::Restart,
                ],
            ),
            entry("0.1.0", vec![UpgradeInstruction::Restart]),
        ];
        let err = validate_upgrade_from(&entries).unwrap_err();
        assert!(
            matches!(
                err,
                UpgradeError::RestartNotExclusive {
                    restart_count: 1,
                    ..
                }
            ),
            "within-entry restart-exclusivity diagnostic must surface before the cross-entry \
             duplicate-`:from` gate fires, got {err:?}"
        );
    }

    // ── drift-detection: serde-derive-to-M2_UPGRADE_FROM_KEY_* identity ──

    #[test]
    fn upgrade_from_entry_serde_keys_match_lifted_m2_upgrade_from_key_consts() {
        // Load-bearing invariant: the two `M2_UPGRADE_FROM_KEY_*` consts
        // (`M2_UPGRADE_FROM_KEY_FROM` / `M2_UPGRADE_FROM_KEY_INSTRUCTIONS`)
        // name the exact camelCase JSON keys the `#[serde(rename_all =
        // "camelCase")]` attribute on `UpgradeFromEntry` emits, and every
        // test-side probe across the caixa-core / caixa-flux renderer
        // test fixtures navigates into each element of the rendered
        // `:upgrade-from` overlay sequence by consulting one of these two
        // `&'static str`s. Serialize a fully-populated UpgradeFromEntry
        // and pin that each canonical byte-sequence appears verbatim in
        // the JSON — a future accidental `rename_all = "snake_case"` /
        // `"kebab-case"` / verbatim-field-name flip at the derive
        // attribute (any of which would silently break every test-side
        // probe that reaches for one of the two consts) surfaces here as
        // a build-time test failure at `upgrade.rs`, not as an apply-time
        // `.get(<stale-canonical-const>)` returning `None` far from the
        // derive-attr drift's commit. Same discipline the sibling
        // `limits_spec_serde_keys_match_lifted_m2_limits_key_consts`
        // (d8b8b4f) and
        // `behavior_spec_serde_keys_match_lifted_m2_behavior_key_consts`
        // (21fe462) pins established on the peer `:limits` / `:behavior`
        // sub-slot axes: one canonical byte-string per typed sub-key
        // axis, pinned to the load-bearing serde derivation at the type
        // itself.
        let e = UpgradeFromEntry {
            from: "0.1.0".into(),
            instructions: vec![UpgradeInstruction::LoadModule {
                module: "hello-rio".into(),
            }],
        };
        let json = serde_json::to_string(&e).unwrap();
        for key in [
            crate::render::M2_UPGRADE_FROM_KEY_FROM,
            crate::render::M2_UPGRADE_FROM_KEY_INSTRUCTIONS,
        ] {
            let quoted = format!("\"{key}\"");
            assert!(
                json.contains(&quoted),
                "serialized UpgradeFromEntry must carry the lifted \
                 M2_UPGRADE_FROM_KEY_* byte-sequence {quoted} verbatim in \
                 the JSON emission (got: {json})",
            );
        }
    }

    #[test]
    fn m2_upgrade_from_key_consts_are_pairwise_distinct() {
        // Cross-axis drift-detection pin: a future collapse of the two
        // canonical sub-key byte-strings onto the same value (e.g. an
        // accidental copy-paste flip of `M2_UPGRADE_FROM_KEY_INSTRUCTIONS`
        // to also read `"from"`) would silently reroute every test-side
        // probe on one axis onto the sibling axis's per-entry field and
        // pass every propagation-probe test that expected only the stale
        // axis's value. Peer of `m2_limits_key_consts_are_pairwise_distinct`
        // (d8b8b4f) and `m2_behavior_key_consts_are_pairwise_distinct`
        // (21fe462) on the sibling `:limits` / `:behavior` sub-slot axes.
        let all = [
            crate::render::M2_UPGRADE_FROM_KEY_FROM,
            crate::render::M2_UPGRADE_FROM_KEY_INSTRUCTIONS,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(
                    a, b,
                    "M2_UPGRADE_FROM_KEY_* consts must be pairwise-distinct \
                     canonical byte-sequences — got `{a}` == `{b}`",
                );
            }
        }
    }

    #[test]
    fn upgrade_instruction_serde_tag_key_matches_lifted_m2_upgrade_instruction_key_kind_const() {
        // Load-bearing invariant on the M2 `:upgrade-from :instructions`
        // per-entry OTP-appup [`UpgradeInstruction`] enum's internally-
        // tagged variant-discriminator key axis: the
        // `M2_UPGRADE_INSTRUCTION_KEY_KIND` const names the exact tag-slot
        // JSON key the `#[serde(tag = "kind", rename_all = "kebab-case")]`
        // attribute on [`UpgradeInstruction`] emits, and every downstream
        // consumer that navigates the serialized instruction blob to
        // route by variant (the caixa-core reflection-vs-serde round-trip
        // check in `dispatcher_registration.rs` that probes
        // `v.get("kind")` against every variant's expected kebab-case
        // tag, the future M4 admission-webhook path, any wasm-operator
        // dispatch step consuming the serialized instruction blob) reads
        // through the same `&'static str`. Serialize every variant and
        // pin that the const's byte-sequence appears verbatim as the
        // tag-slot JSON key with the expected kebab-case value — a
        // future accidental `tag = "type"` / `tag = "op"` /
        // `tag = "instruction"` rebrand at the derive attribute (any of
        // which would silently break every consumer probe reaching for
        // the stale-tag-key const) surfaces here as a build-time test
        // failure at `upgrade.rs`, not as an apply-time
        // `.get(<stale-tag-key>)` returning `None` far from the derive-
        // attr drift's commit.
        //
        // Same "one canonical byte-string per typed axis" discipline the
        // sibling `upgrade_from_entry_serde_keys_match_lifted_m2_upgrade_from_key_consts`
        // pin (36ffe65) established on the peer `:upgrade-from` per-entry
        // outer-container axis — this pin extends the discipline one
        // altitude deeper onto the per-instruction *tag* axis inside
        // each element of the `:instructions` list, completing the
        // typed coverage of the `:upgrade-from :instructions` dual
        // (key = "kind" + five variant-value tags): the five
        // `M2_UPGRADE_INSTRUCTION_KIND_*` consts (56120ef) pin the
        // per-variant kebab-case *values*; this pin pins the tag *key*
        // above them.
        let samples: [(UpgradeInstruction, &'static str); 5] = [
            (
                UpgradeInstruction::LoadModule {
                    module: "hello-rio".into(),
                },
                crate::render::M2_UPGRADE_INSTRUCTION_KIND_LOAD_MODULE.trim_start_matches(':'),
            ),
            (
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/migrations/v01-to-v02.lisp"),
                },
                crate::render::M2_UPGRADE_INSTRUCTION_KIND_STATE_CHANGE.trim_start_matches(':'),
            ),
            (
                UpgradeInstruction::SoftPurge {
                    module: "hello-rio-old".into(),
                },
                crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE.trim_start_matches(':'),
            ),
            (
                UpgradeInstruction::Purge {
                    module: "hello-rio-old".into(),
                },
                crate::render::M2_UPGRADE_INSTRUCTION_KIND_PURGE.trim_start_matches(':'),
            ),
            (
                UpgradeInstruction::Restart,
                crate::render::M2_UPGRADE_INSTRUCTION_KIND_RESTART.trim_start_matches(':'),
            ),
        ];
        for (sample, expected_value) in &samples {
            let v: serde_json::Value = serde_json::to_value(sample).unwrap();
            let got = v
                .get(crate::render::M2_UPGRADE_INSTRUCTION_KEY_KIND)
                .and_then(|k| k.as_str());
            assert_eq!(
                got,
                Some(*expected_value),
                "serialized {sample:?} must carry the lifted \
                 M2_UPGRADE_INSTRUCTION_KEY_KIND byte-sequence \
                 ({:?}) verbatim as the tag-slot JSON key, holding the \
                 expected kebab-case value {expected_value:?} (got: {v})",
                crate::render::M2_UPGRADE_INSTRUCTION_KEY_KIND,
            );
        }
    }

    #[test]
    fn m2_upgrade_instruction_key_kind_const_is_lower_camel_case_shape() {
        // Shape-pin: the `M2_UPGRADE_INSTRUCTION_KEY_KIND` const must be
        // a lowerCamelCase byte-sequence (non-empty, ASCII-lowercase
        // leader, ASCII-alphanumeric only — no `snake_case` underscores,
        // no `kebab-case` hyphens, no `PascalCase` leading capital, no
        // whitespace / colons / dots) — the canonical shape a serde
        // internally-tagged discriminator key takes across every peer
        // enum in this crate. A future flip to a non-camelCase byte at
        // the const surfaces here at build time. Peer of
        // `m2_upgrade_from_key_consts_are_lower_camel_case_shape` on the
        // sibling per-entry outer-container axis.
        let key = crate::render::M2_UPGRADE_INSTRUCTION_KEY_KIND;
        assert!(
            !key.is_empty(),
            "M2_UPGRADE_INSTRUCTION_KEY_KIND must be non-empty (got {key:?})"
        );
        let first = key.chars().next().unwrap();
        assert!(
            first.is_ascii_lowercase(),
            "M2_UPGRADE_INSTRUCTION_KEY_KIND must lead with an ASCII-lowercase \
             byte (got {key:?}, leads with {first:?})",
        );
        assert!(
            key.chars().all(|c| c.is_ascii_alphanumeric()),
            "M2_UPGRADE_INSTRUCTION_KEY_KIND must be ASCII-alphanumeric only \
             — no `_` / `-` / `:` / `.` / whitespace (got {key:?})",
        );
    }

    #[test]
    fn m2_upgrade_instruction_key_kind_const_disjoint_from_variant_data_keys() {
        // Cross-axis drift-detection pin: the tag-slot key
        // `M2_UPGRADE_INSTRUCTION_KEY_KIND` (`"kind"`) must be
        // disjoint from every per-variant data-field key the
        // internally-tagged serialization also emits (`"module"` for
        // LoadModule/SoftPurge/Purge, `"script"` for StateChange). A
        // future accidental rebrand that collapses `tag = "kind"` onto
        // one of the data-field names (e.g. `tag = "module"`) would
        // silently corrupt every serialized LoadModule blob (the
        // module string and the variant tag would collide on the same
        // JSON key) and every consumer probe would either misread the
        // tag or fail to distinguish variants. Pin the disjointness at
        // build time. Same cross-axis discipline the sibling
        // `m2_upgrade_from_key_consts_are_pairwise_distinct` pin
        // (36ffe65) established on the outer container's own
        // `from`/`instructions` pair.
        let key = crate::render::M2_UPGRADE_INSTRUCTION_KEY_KIND;
        // Enumerate every per-variant data-field key across all five
        // variants of [`UpgradeInstruction`], routing through the two
        // lifted `M2_UPGRADE_INSTRUCTION_FIELD_KEY_*` byte-string consts
        // that name the same per-variant data-field JSON keys the
        // `variant_fields` reflection in
        // `caixa-core/tests/dispatcher_registration.rs` surfaces. A future
        // per-variant struct-field rebrand (`module` → `component`,
        // `script` → `path`) lands as an edit to exactly one const and
        // reaches this disjointness pin by construction — the two axes
        // (tag-slot key on one side, per-variant data-field keys on the
        // other) share one source of truth per axis.
        for data_field in [
            crate::render::M2_UPGRADE_INSTRUCTION_FIELD_KEY_MODULE,
            crate::render::M2_UPGRADE_INSTRUCTION_FIELD_KEY_SCRIPT,
        ] {
            assert_ne!(
                key, data_field,
                "M2_UPGRADE_INSTRUCTION_KEY_KIND (the serde `tag` slot) \
                 must be disjoint from every UpgradeInstruction per-variant \
                 data-field key — got tag-key {key:?} colliding with \
                 data-field {data_field:?}, which would silently corrupt \
                 the internally-tagged serialization",
            );
        }
    }

    #[test]
    fn upgrade_instruction_variant_data_field_keys_match_lifted_field_key_consts() {
        // Load-bearing invariant on the M2 `:upgrade-from :instructions`
        // per-entry OTP-appup [`UpgradeInstruction`] enum's per-variant
        // data-field JSON key axis: the two
        // `M2_UPGRADE_INSTRUCTION_FIELD_KEY_*` consts (`_MODULE`,
        // `_SCRIPT`) name the exact per-variant field JSON keys the
        // `#[serde(tag = "kind", rename_all = "kebab-case")]` attribute on
        // [`UpgradeInstruction`] emits alongside the tag-slot key from the
        // sibling [`crate::render::M2_UPGRADE_INSTRUCTION_KEY_KIND`]
        // const — the `module: String` struct-field on
        // `LoadModule`/`SoftPurge`/`Purge` and the `script: PathBuf`
        // struct-field on `StateChange` are promoted to sibling JSON keys
        // at the same nesting level as the tag by the internally-tagged
        // serialization, and every downstream consumer that navigates the
        // serialized instruction blob to reach the payload (the caixa-core
        // reflection round-trip in `dispatcher_registration.rs` that
        // consults `variant_fields`, the sibling disjointness pin below,
        // any future wasm-operator upgrade-dispatch step consuming the
        // serialized instruction blob to route the per-module load /
        // soft-purge / purge action or the per-script state-change action)
        // reads through the same `&'static str`. Serialize one Module-
        // bearing variant and one Script-bearing variant, then pin that
        // each const's byte-sequence appears verbatim in the JSON emission
        // — a future accidental struct-field rebrand (`module: String` →
        // `component: String`, `script: PathBuf` → `path: PathBuf`) at
        // either variant surfaces here as a build-time test failure at
        // `upgrade.rs`, not as an apply-time `.get(<stale-field-key>)`
        // returning `None` far from the field-name drift's commit.
        //
        // Same "one canonical byte-string per typed axis" discipline the
        // sibling `upgrade_instruction_serde_tag_key_matches_lifted_m2_upgrade_instruction_key_kind_const`
        // pin established on the peer tag-slot key axis on the same
        // enum — this pin extends the discipline onto the per-variant
        // data-field key axis, completing the `:upgrade-from :instructions`
        // variant-JSON dual (tag key + tag values + per-variant field keys)
        // fully into caixa-core.
        let module_sample = UpgradeInstruction::LoadModule {
            module: "hello-rio".into(),
        };
        let v: serde_json::Value = serde_json::to_value(&module_sample).unwrap();
        assert_eq!(
            v.get(crate::render::M2_UPGRADE_INSTRUCTION_FIELD_KEY_MODULE)
                .and_then(|k| k.as_str()),
            Some("hello-rio"),
            "serialized {module_sample:?} must carry the lifted \
             M2_UPGRADE_INSTRUCTION_FIELD_KEY_MODULE byte-sequence \
             ({:?}) verbatim as the data-field JSON key holding the \
             module string (got: {v})",
            crate::render::M2_UPGRADE_INSTRUCTION_FIELD_KEY_MODULE,
        );

        let script_sample = UpgradeInstruction::StateChange {
            script: PathBuf::from("lib/migrations/v01-to-v02.lisp"),
        };
        let v: serde_json::Value = serde_json::to_value(&script_sample).unwrap();
        assert_eq!(
            v.get(crate::render::M2_UPGRADE_INSTRUCTION_FIELD_KEY_SCRIPT)
                .and_then(|k| k.as_str()),
            Some("lib/migrations/v01-to-v02.lisp"),
            "serialized {script_sample:?} must carry the lifted \
             M2_UPGRADE_INSTRUCTION_FIELD_KEY_SCRIPT byte-sequence \
             ({:?}) verbatim as the data-field JSON key holding the \
             script path (got: {v})",
            crate::render::M2_UPGRADE_INSTRUCTION_FIELD_KEY_SCRIPT,
        );
    }

    #[test]
    fn m2_upgrade_instruction_field_key_consts_are_lower_camel_case_shape() {
        // Shape-pin: every `M2_UPGRADE_INSTRUCTION_FIELD_KEY_*` const must
        // be a lowerCamelCase byte-sequence (non-empty, ASCII-lowercase
        // leader, ASCII-alphanumeric only — no `snake_case` underscores,
        // no `kebab-case` hyphens, no `PascalCase` leading capital, no
        // whitespace / colons / dots) — the canonical shape a Rust
        // struct-field name promoted to a JSON key by serde takes on this
        // internally-tagged variant surface, matching the sibling
        // [`crate::render::M2_UPGRADE_INSTRUCTION_KEY_KIND`] tag-slot key
        // shape. A future flip to a non-camelCase byte at either const
        // (an accidental `rename_all` regime interleave, or a struct-
        // field flip like `module` → `module_name`) surfaces here at
        // build time. Peer of
        // `m2_upgrade_instruction_key_kind_const_is_lower_camel_case_shape`
        // and `m2_upgrade_from_key_consts_are_lower_camel_case_shape` on
        // the sibling wire-key axes.
        for key in [
            crate::render::M2_UPGRADE_INSTRUCTION_FIELD_KEY_MODULE,
            crate::render::M2_UPGRADE_INSTRUCTION_FIELD_KEY_SCRIPT,
        ] {
            assert!(
                !key.is_empty(),
                "M2_UPGRADE_INSTRUCTION_FIELD_KEY_* must be non-empty (got {key:?})"
            );
            let first = key.chars().next().unwrap();
            assert!(
                first.is_ascii_lowercase(),
                "M2_UPGRADE_INSTRUCTION_FIELD_KEY_* must lead with an ASCII-lowercase \
                 byte (got {key:?}, leads with {first:?})",
            );
            assert!(
                key.chars().all(|c| c.is_ascii_alphanumeric()),
                "M2_UPGRADE_INSTRUCTION_FIELD_KEY_* must be ASCII-alphanumeric only \
                 — no `_` / `-` / `:` / `.` / whitespace (got {key:?})",
            );
        }
    }

    #[test]
    fn m2_upgrade_instruction_field_key_consts_are_pairwise_distinct() {
        // Cross-axis drift-detection pin: a future collapse of the two
        // canonical per-variant data-field byte-strings onto the same
        // value (e.g. an accidental copy-paste flip of
        // `M2_UPGRADE_INSTRUCTION_FIELD_KEY_SCRIPT` to also read
        // `"module"`) would silently reroute every test-side probe on one
        // variant's payload onto the sibling variant's payload and pass
        // every propagation-probe test that expected only the stale
        // axis's value. Peer of `m2_upgrade_from_key_consts_are_pairwise_distinct`
        // on the sibling per-entry outer-container axis, and of
        // `m2_upgrade_instruction_key_kind_const_disjoint_from_variant_data_keys`
        // on the sibling tag-slot key ↔ per-variant data-field key axis.
        let all = [
            crate::render::M2_UPGRADE_INSTRUCTION_FIELD_KEY_MODULE,
            crate::render::M2_UPGRADE_INSTRUCTION_FIELD_KEY_SCRIPT,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(
                    a, b,
                    "M2_UPGRADE_INSTRUCTION_FIELD_KEY_* consts must be pairwise-distinct \
                     canonical byte-sequences — got `{a}` == `{b}`",
                );
            }
        }
    }

    #[test]
    fn m2_upgrade_from_key_consts_are_lower_camel_case_shape() {
        // Shape-pin: every `M2_UPGRADE_FROM_KEY_*` const must be a
        // lowerCamelCase byte-sequence (no `snake_case` underscores, no
        // `kebab-case` hyphens, no `PascalCase` leading capital, no
        // whitespace / colons / dots) — the canonical shape the
        // `#[serde(rename_all = "camelCase")]` derive produces on
        // `UpgradeFromEntry`. A future flip to a non-camelCase attribute
        // at the derive surfaces both here (this test fails on the
        // stale-constant shape) and at
        // `upgrade_from_entry_serde_keys_match_lifted_m2_upgrade_from_key_consts`
        // (that test fails on the mismatch between const and derive).
        // Peer of `m2_limits_key_consts_are_lower_camel_case_shape`
        // (d8b8b4f) and `m2_behavior_key_consts_are_lower_camel_case_shape`
        // (21fe462) on the sibling `:limits` / `:behavior` sub-slot axes.
        for key in [
            crate::render::M2_UPGRADE_FROM_KEY_FROM,
            crate::render::M2_UPGRADE_FROM_KEY_INSTRUCTIONS,
        ] {
            assert!(
                !key.is_empty(),
                "M2_UPGRADE_FROM_KEY_* must be non-empty (got {key:?})"
            );
            let first = key.chars().next().unwrap();
            assert!(
                first.is_ascii_lowercase(),
                "M2_UPGRADE_FROM_KEY_* must lead with an ASCII-lowercase \
                 byte (got {key:?}, leads with {first:?})",
            );
            assert!(
                key.chars().all(|c| c.is_ascii_alphanumeric()),
                "M2_UPGRADE_FROM_KEY_* must be ASCII-alphanumeric only \
                 — no `_` / `-` / `:` / `.` / whitespace (got {key:?})",
            );
        }
    }

    #[test]
    fn m2_upgrade_instruction_kind_consts_pin_canonical_kebab_case_labels() {
        // Scalar-value pin on the M2 `:upgrade-from :instructions` per-entry
        // OTP-appup variant-tag axis: the five canonical author-facing
        // kebab-case labels (`:load-module` / `:state-change` /
        // `:soft-purge` / `:purge` / `:restart`) the substrate's
        // per-variant [`UpgradeInstruction::lisp_form`] dispatch reads
        // from and every downstream consumer probes for verbatim. Same
        // scalar-value discipline the peer
        // `contrato_author_key_consts_pin_canonical_kebab_case_labels`
        // (f50c875), `m3_top_level_author_key_consts_pin_canonical_kebab_case_labels`
        // (882f498), `m2_top_level_author_key_consts_pin_canonical_kebab_case_labels`
        // (f49c8b0), and `supervisor_top_level_author_key_consts_pin_canonical_kebab_case_labels`
        // (be40492) established for the sibling M2 / M3 / Supervisor
        // top-level and sub-slot author-facing-label axes. Fail-before-
        // pass-after locally verified by mutating
        // `M2_UPGRADE_INSTRUCTION_KIND_LOAD_MODULE` to `":load"` — this
        // pin fires as expected; restoring passes.
        //
        // A future OTP-lineage per-variant rebrand (e.g.
        // `:load-module` → `:load` matching Erlang's abbreviated
        // `code:load_module` name, `:state-change` → `:code-change`
        // matching Erlang's verbatim `code_change/3` callback,
        // `:soft-purge` → `:drain` matching a hypothetical operator-side
        // vocabulary flip, `:purge` → `:discard` matching a hypothetical
        // Elixir/Phoenix hot-reload rebrand, `:restart` → `:reboot`
        // matching a supervisor-tree vocabulary alignment) lands as an
        // edit to exactly one const, and every consumer that reaches for
        // the label (the [`UpgradeInstruction::lisp_form`] dispatch, the
        // [`validate_cleanup_singularity`] per-variant `kind:` tagger,
        // every [`UpgradeError`] `kind:` / `kinds:` / `other_kinds:` /
        // `prior_cleanup_kind:` diagnostic field, the
        // [`LayoutError::UpgradeViolation`] `issue:` probe in
        // `layout.rs`) picks it up at build time rather than at runtime
        // as a downstream `kind: <stale-kebab-case>` diagnostic mismatch
        // far from the rename's commit.
        assert_eq!(
            crate::render::M2_UPGRADE_INSTRUCTION_KIND_LOAD_MODULE,
            ":load-module"
        );
        assert_eq!(
            crate::render::M2_UPGRADE_INSTRUCTION_KIND_STATE_CHANGE,
            ":state-change"
        );
        assert_eq!(
            crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
            ":soft-purge"
        );
        assert_eq!(crate::render::M2_UPGRADE_INSTRUCTION_KIND_PURGE, ":purge");
        assert_eq!(
            crate::render::M2_UPGRADE_INSTRUCTION_KIND_RESTART,
            ":restart"
        );
    }

    #[test]
    fn m2_upgrade_instruction_kind_consts_are_pairwise_distinct() {
        // Cross-arm drift-detection pin on the M2
        // [`crate::render::M2_UPGRADE_INSTRUCTION_KIND_LOAD_MODULE`] /
        // [`crate::render::M2_UPGRADE_INSTRUCTION_KIND_STATE_CHANGE`] /
        // [`crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE`] /
        // [`crate::render::M2_UPGRADE_INSTRUCTION_KIND_PURGE`] /
        // [`crate::render::M2_UPGRADE_INSTRUCTION_KIND_RESTART`]
        // closed-set OTP-appup variant-tag pentad: a future collapse
        // of two canonical variant byte-strings onto the same value
        // (an accidental copy-paste flip of
        // [`crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE`]
        // to also read `":purge"`, a per-arm rebrand that lands one
        // const without touching its paired peer) would silently
        // reroute every downstream OTP-appup dispatcher's per-
        // instruction branch onto the sibling arm's runtime
        // behavior and pass every propagation-probe test that
        // expected only the stale arm's tag — a `:soft-purge`
        // instruction (drain-then-swap: existing callers finish
        // under the old module, new callers land on the new one)
        // would come up under the `:purge` reconcile branch
        // (drop-existing: every in-flight caller terminates
        // immediately) on every hot-upgrade cycle, so a rolling
        // module swap would silently downgrade to a hard cutover
        // against its declared appup discipline, with no field
        // naming the instruction-tag drift root cause. Every
        // [`crate::UpgradeError`] diagnostic that surfaces the tag
        // ([`crate::UpgradeError::ModuleEmpty`] with `kind:` field,
        // [`crate::UpgradeError::CleanupCollision`] with `kinds:`
        // slice, [`crate::UpgradeError::CleanupPrecedes`] with
        // `prior_cleanup_kind:` field, the
        // [`crate::LayoutError::UpgradeViolation`] `issue:` probe in
        // `layout.rs`) would emit the sibling arm's stale bytes at
        // the operator's console, far from the source rebrand
        // commit. Peer of the sibling
        // [`crate::supervisor::tests::supervisor_estrategia_consts_are_pairwise_distinct`]
        // (09ffb2d) /
        // [`crate::supervisor::tests::supervisor_child_restart_consts_are_pairwise_distinct`]
        // (ccdf955) /
        // [`crate::kind::tests::caixa_kind_label_consts_are_pairwise_distinct`]
        // (d739850) distinctness pins on the sibling OTP-shape /
        // caixa-kind closed-set typed-enum discriminator axes —
        // the fifth closed-set OTP-appup / typed-enum axis to
        // converge on the same
        // "pairwise-distinct-by-construction" discipline, and the
        // canonical companion to the peer
        // [`m2_upgrade_instruction_field_key_consts_are_pairwise_distinct`]
        // (ff980bb) distinctness pin on the sibling internally-
        // tagged-JSON per-variant data-field-key axis (the tag axis
        // this pin covers vs. the data-field-key axis its peer
        // covers — two paired axes on the same
        // [`crate::UpgradeInstruction`] typed enum surface).
        //
        // Fail-before-pass-after locally verified by mutating
        // [`crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE`]
        // to also read `":purge"` — this pin fires as expected;
        // restoring passes.
        let all = [
            crate::render::M2_UPGRADE_INSTRUCTION_KIND_LOAD_MODULE,
            crate::render::M2_UPGRADE_INSTRUCTION_KIND_STATE_CHANGE,
            crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
            crate::render::M2_UPGRADE_INSTRUCTION_KIND_PURGE,
            crate::render::M2_UPGRADE_INSTRUCTION_KIND_RESTART,
        ];
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                if i != j {
                    assert_ne!(
                        a, b,
                        "M2_UPGRADE_INSTRUCTION_KIND_* consts must be pairwise \
                         distinct — got duplicate {a:?} at indices {i} and {j}",
                    );
                }
            }
        }
    }

    #[test]
    fn upgrade_instruction_lisp_form_routes_through_lifted_kind_consts() {
        // Production-through-const pin: the five per-variant labels
        // [`UpgradeInstruction::lisp_form`] returns route through the
        // lifted [`crate::render::M2_UPGRADE_INSTRUCTION_KIND_*`] consts,
        // so a future rebrand that reaches the const but not the
        // dispatch (or vice versa) surfaces here at build time rather
        // than at runtime as a downstream
        // [`UpgradeError::ModuleEmpty`] `kind: <stale-kebab-case>`
        // diagnostic drift far from the rename's commit. Mirror of the
        // peer `contrato_shape_gate_routes_through_lifted_contrato_author_key_consts`
        // (f50c875), `declared_mesh_slots_route_through_lifted_m3_author_key_consts`
        // (882f498), and `declared_servico_slots_route_through_lifted_m2_author_key_consts`
        // (f49c8b0) production-through-const pins on the sibling M3 /
        // M2 top-level slot axes.
        //
        // Fail-before-pass-after locally verified by mutating
        // `UpgradeInstruction::lisp_form`'s `Self::Purge` arm to return
        // `":purge-drift"` — this pin fires as expected; restoring
        // passes.
        let cases: &[(UpgradeInstruction, &'static str)] = &[
            (
                UpgradeInstruction::LoadModule { module: "x".into() },
                crate::render::M2_UPGRADE_INSTRUCTION_KIND_LOAD_MODULE,
            ),
            (
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/m.lisp"),
                },
                crate::render::M2_UPGRADE_INSTRUCTION_KIND_STATE_CHANGE,
            ),
            (
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE,
            ),
            (
                UpgradeInstruction::Purge {
                    module: "x-old".into(),
                },
                crate::render::M2_UPGRADE_INSTRUCTION_KIND_PURGE,
            ),
            (
                UpgradeInstruction::Restart,
                crate::render::M2_UPGRADE_INSTRUCTION_KIND_RESTART,
            ),
        ];
        for (instr, expected) in cases {
            assert_eq!(
                instr.lisp_form(),
                *expected,
                "UpgradeInstruction::lisp_form on {instr:?} must route through the lifted \
                 const (expected {expected:?})",
            );
        }
    }

    #[test]
    fn upgrade_from_entry_instructions_returns_instructions_slice_byte_equal_across_permutations() {
        // The canonical per-`:upgrade-from :instructions` OTP-appup
        // migration-instruction-list slice-shape pin:
        // [`UpgradeFromEntry::instructions`] must return the
        // `:instructions` typed `Vec<UpgradeInstruction>` verbatim as
        // a `&[UpgradeInstruction]` slice-view over the same backing
        // buffer the raw `self.instructions.as_slice()` field access
        // borrows from, byte-equal across every representative fixture
        // in the accept-set — the empty slice (the "no-op upgrade" /
        // metadata-only sentinel the [`UpgradeFromEntry::instructions`]
        // field's own docstring names), the singleton slice on every
        // variant of the [`UpgradeInstruction`] arm-space
        // (`LoadModule` / `StateChange` / `SoftPurge` / `Purge` /
        // `Restart` — the five OTP-appup runtime-primitive variants),
        // and multi-instruction cohorts (the canonical
        // `LoadModule → StateChange → SoftPurge` OTP two-phase code-
        // load + state-migration triad the module doc names as the
        // "runs the instructions in order" example).
        //
        // Pins against a future silent detour that returned
        // `&Vec<UpgradeInstruction>` (which would type-check but leak
        // the storage-side `Vec`'s grow/push/reserve surface no
        // consumer of the typed view reaches for), a fresh-allocated
        // `Vec<UpgradeInstruction>` copy (which would type-check via
        // a coercion but silently break every downstream caller that
        // relied on the slice sharing the backing buffer's identity),
        // or an out-of-order or length-drifted projection (which
        // would silently split the paired within-entry cross-
        // instruction ordering gates' inputs from the peer per-
        // instruction shape-check loop's input, one seven-gate cohort
        // silently drifting from the peer gate's actual traversal
        // input).
        //
        // Peer of the sibling
        // `aplicacao_spec_contratos_returns_contratos_slice_byte_equal_across_permutations`
        // (0dcc926) `&[WitContract]` byte-equal pin on the M3 per-
        // `:contratos` edge-list axis, extended onto the M2 per-
        // `:upgrade-from :instructions` migration-instruction-list
        // axis — the fifth `&[T]`-return byte-equal pin, closing the
        // last unlifted `Vec`-carry axis on any M2 or M3 typed slot.
        let fixtures: Vec<Vec<UpgradeInstruction>> = vec![
            Vec::new(),
            vec![UpgradeInstruction::LoadModule { module: "x".into() }],
            vec![UpgradeInstruction::StateChange {
                script: PathBuf::from("lib/m.lisp"),
            }],
            vec![UpgradeInstruction::SoftPurge {
                module: "x-old".into(),
            }],
            vec![UpgradeInstruction::Purge {
                module: "x-old".into(),
            }],
            vec![UpgradeInstruction::Restart],
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/migrations/v01-to-v02.lisp"),
                },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
            ],
        ];
        for instructions in fixtures {
            let e = UpgradeFromEntry {
                from: "0.1.0".into(),
                instructions: instructions.clone(),
            };
            assert_eq!(
                e.instructions(),
                e.instructions.as_slice(),
                "UpgradeFromEntry::instructions must project the raw \
                 `:instructions` `Vec<UpgradeInstruction>` verbatim as a \
                 `&[UpgradeInstruction]` slice-view over the same backing buffer \
                 (fixture: {instructions:?})",
            );
            assert_eq!(
                e.instructions().len(),
                instructions.len(),
                "UpgradeFromEntry::instructions length must match the raw \
                 `:instructions` `Vec<UpgradeInstruction>` length (fixture: {instructions:?})",
            );
        }
    }

    #[test]
    fn validate_reads_through_lifted_instructions_accessor() {
        // Three-consumer coherence pin on the lifted
        // [`UpgradeFromEntry::instructions`] slice-return accessor:
        // exercises three of the nine paired production consumers of
        // the per-`:upgrade-from :instructions` OTP-appup migration-
        // instruction-list surface through end-to-end validate() paths
        // that require the accessor to reach each of the fixture's
        // instructions.
        //
        // (1) The per-instruction shape-check fan-out
        // ([`UpgradeFromEntry::validate`]'s `for instr in
        // self.instructions()` loop): pass the well-formed load →
        // state-change → soft-purge triad — `validate()` must accept
        // it, which requires the accessor to project every entry so
        // each `instr.validate()` fires.
        //
        // (2) The within-entry state-change-ordering gate
        // ([`Self::validate_state_change_ordering`]): pass a
        // `((:state-change …))` singleton — `validate()` must return
        // [`UpgradeError::StateChangeWithoutPriorLoad`], which
        // requires the accessor to reach the state-change so the
        // no-prior-load probe fires.
        //
        // (3) The within-entry per-module cleanup-singularity gate
        // ([`Self::validate_cleanup_singularity`]): pass a
        // `((:load-module "x") (:soft-purge "x-old") (:soft-purge
        // "x-old"))` cohort — `validate()` must return
        // [`UpgradeError::DuplicateCleanup`], which requires the
        // accessor to iterate the whole list so the second `SoftPurge`
        // matches the first via the `seen` set.
        //
        // Peer of the sibling
        // `validate_reads_through_lifted_contratos_accessor` (0dcc926)
        // three-consumer coherence pin on the M3 per-`:contratos`
        // edge-list axis, extended onto the M2 per-`:upgrade-from
        // :instructions` migration-instruction-list axis.

        // (1) accept the well-formed OTP two-phase code-load triad
        let well_formed = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/migrations/v01-to-v02.lisp"),
                },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
            ],
        );
        assert!(
            well_formed.validate().is_ok(),
            "well-formed `LoadModule → StateChange → SoftPurge` triad must accept — \
             the per-instruction shape-check fan-out requires the accessor to reach every entry"
        );

        // (2) refuse a `((:state-change …))` singleton — the
        // state-change-without-prior-load gate must fire, which
        // requires the accessor to reach the single instruction.
        let no_prior_load = entry(
            "0.1.0",
            vec![UpgradeInstruction::StateChange {
                script: PathBuf::from("lib/m.lisp"),
            }],
        );
        match no_prior_load.validate() {
            Err(UpgradeError::StateChangeWithoutPriorLoad { .. }) => {}
            other => panic!(
                "expected StateChangeWithoutPriorLoad on a `((:state-change …))` singleton \
                 — the within-entry state-change-ordering gate must reach the single \
                 instruction through the lifted accessor; got: {other:?}"
            ),
        }

        // (3) refuse a `((:load-module "x") (:soft-purge "x-old")
        // (:soft-purge "x-old"))` cohort — the per-module cleanup-
        // singularity gate must fire on the second `SoftPurge`, which
        // requires the accessor to iterate the whole list.
        let duplicate_cleanup = entry(
            "0.1.0",
            vec![
                UpgradeInstruction::LoadModule { module: "x".into() },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
                UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                },
            ],
        );
        match duplicate_cleanup.validate() {
            Err(UpgradeError::DuplicateCleanup { module, .. }) => {
                assert_eq!(
                    module, "x-old",
                    "DuplicateCleanup must name the colliding module `x-old` — the per-module \
                     cleanup-singularity gate must iterate through the lifted accessor to \
                     match the second SoftPurge against the first via the `seen` set"
                );
            }
            other => panic!(
                "expected DuplicateCleanup on `((:load-module x) (:soft-purge x-old) \
                 (:soft-purge x-old))` — the within-entry cleanup-singularity gate must \
                 iterate the whole list through the lifted accessor; got: {other:?}"
            ),
        }

        // Path::new suppresses the unused-import warning if the
        // outer module trims `use std::path::Path;` in a future edit.
        let _ = Path::new("lib/m.lisp");
    }
}
