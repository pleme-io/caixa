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
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
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
    /// Verify the `:from` field is a valid semver.
    pub fn validate(&self) -> Result<(), UpgradeError> {
        use semver::Version;
        Version::parse(&self.from).map_err(|_| UpgradeError::BadFromVersion(self.from.clone()))?;
        // Validate each instruction's referenced paths if any.
        for instr in &self.instructions {
            instr.validate()?;
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
/// diagnostic names the malformed slot (`BadFromVersion`, `EmptyScript`,
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
        // fail here — the BadFromVersion arm above is the only gate
        // and both call `Version::parse(&self.from)`.
        let parsed = Version::parse(&entry.from).expect(
            "UpgradeFromEntry::validate must accept `:from` iff Version::parse does — keep the \
             two gates aligned",
        );
        if seen.contains(&parsed) {
            return Err(UpgradeError::DuplicateFrom {
                from: entry.from.clone(),
            });
        }
        seen.push(parsed);
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
            Self::LoadModule { .. } => ":load-module",
            Self::StateChange { .. } => ":state-change",
            Self::SoftPurge { .. } => ":soft-purge",
            Self::Purge { .. } => ":purge",
            Self::Restart => ":restart",
        }
    }

    /// Validate the instruction's typed shape. Path existence is
    /// checked separately by [`crate::layout::StandardLayout`].
    pub fn validate(&self) -> Result<(), UpgradeError> {
        match self {
            Self::LoadModule { module } | Self::SoftPurge { module } | Self::Purge { module } => {
                validate_module(self.lisp_form(), module)
            }
            Self::StateChange { script } => {
                // Delegate the three structural checks (non-empty /
                // relative / no-parent-escape) to the lifted
                // [`crate::render::is_sandboxed_relative_path`]
                // predicate — same Empty → Absolute → ParentEscape
                // arm-ordering this method previously inlined verbatim,
                // now shared with [`crate::BehaviorSpec::validate`]'s
                // per-`:on-*`-callback gate so every author-supplied path
                // on every M2 typed slot consults one gate, not two-and-
                // counting verbatim copies. Each arm wraps the tag in
                // the same `*Script` variant the original inline code
                // raised, so the diagnostic shape every caller depends
                // on (the `:state-change :script` self-locating error)
                // is preserved by construction.
                match crate::render::is_sandboxed_relative_path(script) {
                    Ok(()) => Ok(()),
                    Err(crate::render::PathShapeViolation::Empty) => Err(UpgradeError::EmptyScript),
                    Err(crate::render::PathShapeViolation::Absolute) => {
                        Err(UpgradeError::AbsoluteScript {
                            script: script.clone(),
                        })
                    }
                    Err(crate::render::PathShapeViolation::ParentEscape) => {
                        Err(UpgradeError::ParentEscapeScript {
                            script: script.clone(),
                        })
                    }
                }
            }
            Self::Restart => Ok(()),
        }
    }

    /// If the instruction references an on-disk path, return it —
    /// used by the layout checker to verify the path resolves.
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
    if module.is_empty() {
        return Err(UpgradeError::ModuleEmpty { kind });
    }
    crate::render::is_dns_1123_label(module).map_err(|reason| UpgradeError::ModuleInvalid {
        kind,
        module: module.to_string(),
        reason,
    })
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum UpgradeError {
    #[error("upgrade-from :from must be a valid semver, got {0:?}")]
    BadFromVersion(String),
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
        ":upgrade-from carries more than one `(:from {from:?})` entry — OTP appup picks at most \
         one matching block per running version (`release_handler:install_release/1` dispatches \
         on the loaded `:from` against the currently-running release), so two entries with the \
         same parsed semver are an ambiguous edge in the typed upgrade graph (the operator would \
         pick either set non-deterministically). Author one path per prior version; if two \
         distinct instruction sequences are needed, fold them into one ordered list under the \
         single matching `(:from {from:?} :instructions (…))` block."
    )]
    DuplicateFrom { from: String },
}

#[cfg(test)]
mod tests {
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
        assert!(matches!(err, UpgradeError::BadFromVersion(_)));
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
                ":load-module",
            ),
            (
                UpgradeInstruction::SoftPurge {
                    module: String::new(),
                },
                ":soft-purge",
            ),
            (
                UpgradeInstruction::Purge {
                    module: String::new(),
                },
                ":purge",
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
                ":load-module",
            ),
            (
                |m| UpgradeInstruction::SoftPurge { module: m },
                ":soft-purge",
            ),
            (|m| UpgradeInstruction::Purge { module: m }, ":purge"),
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
                kind: ":load-module"
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
    fn entry_with_chain_of_versions() {
        let entries = vec![
            entry(
                "0.1.0",
                vec![UpgradeInstruction::LoadModule { module: "x".into() }],
            ),
            entry(
                "0.1.5",
                vec![UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                }],
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
        // shape; the gate must not regress this baseline.
        let entries = vec![
            entry(
                "0.1.0",
                vec![UpgradeInstruction::LoadModule { module: "x".into() }],
            ),
            entry(
                "0.1.5",
                vec![UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                }],
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
                vec![UpgradeInstruction::SoftPurge {
                    module: "x-old".into(),
                }],
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
        // its `BadFromVersion` diagnostic, not a (less-useful)
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
            matches!(err, UpgradeError::BadFromVersion(ref s) if s == "not-a-semver"),
            "malformed `:from` on a non-duplicate entry must surface as BadFromVersion, got {err:?}"
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
                kind: ":load-module"
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
}
