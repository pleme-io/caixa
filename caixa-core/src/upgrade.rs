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

impl UpgradeInstruction {
    /// Validate the instruction's typed shape. Path existence is
    /// checked separately by [`crate::layout::StandardLayout`].
    pub fn validate(&self) -> Result<(), UpgradeError> {
        match self {
            Self::LoadModule { module } | Self::SoftPurge { module } | Self::Purge { module } => {
                if module.is_empty() {
                    Err(UpgradeError::EmptyModule)
                } else {
                    Ok(())
                }
            }
            Self::StateChange { script } => {
                if script.as_os_str().is_empty() {
                    Err(UpgradeError::EmptyScript)
                } else {
                    Ok(())
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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum UpgradeError {
    #[error("upgrade-from :from must be a valid semver, got {0:?}")]
    BadFromVersion(String),
    #[error("instruction's :module is empty")]
    EmptyModule,
    #[error("instruction's :script is empty")]
    EmptyScript,
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
            UpgradeInstruction::LoadModule {
                module: "x".into(),
            },
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
        let i = UpgradeInstruction::LoadModule { module: "".into() };
        assert_eq!(i.validate().unwrap_err(), UpgradeError::EmptyModule);
    }

    #[test]
    fn validate_rejects_empty_script() {
        let i = UpgradeInstruction::StateChange {
            script: PathBuf::new(),
        };
        assert_eq!(i.validate().unwrap_err(), UpgradeError::EmptyScript);
    }

    #[test]
    fn declared_path_only_for_state_change() {
        let load = UpgradeInstruction::LoadModule {
            module: "x".into(),
        };
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
                vec![UpgradeInstruction::LoadModule {
                    module: "x".into(),
                }],
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
}
