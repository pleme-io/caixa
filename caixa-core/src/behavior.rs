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
    pub fn declared_slots(&self) -> impl Iterator<Item = (&'static str, &PathBuf)> {
        [
            (":on-init", self.on_init.as_ref()),
            (":on-call", self.on_call.as_ref()),
            (":on-cast", self.on_cast.as_ref()),
            (":on-info", self.on_info.as_ref()),
            (":on-state-change", self.on_state_change.as_ref()),
            (":on-terminate", self.on_terminate.as_ref()),
        ]
        .into_iter()
        .filter_map(|(slot, opt)| opt.map(|p| (slot, p)))
    }

    /// Iterate over every declared callback path. Used by the
    /// layout checker.
    pub fn declared_paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.declared_slots().map(|(_slot, p)| p)
    }

    /// True when no callback is declared.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.declared_paths().next().is_none()
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
    /// Three invariants per slot, evaluated in declaration order
    /// (`:on-init` → `:on-call` → `:on-cast` → `:on-info` →
    /// `:on-state-change` → `:on-terminate`) so the diagnostic for
    /// multi-malformed manifests is deterministic:
    ///
    ///   - non-empty path string,
    ///   - relative path (Lunatic-style sandbox: callbacks live under
    ///     the caixa root, never in `/etc/...`),
    ///   - no `..` components (relative paths must not escape the
    ///     caixa root via parent-directory traversal).
    ///
    /// Mirrors the discipline applied to `:limits` axes
    /// (`LimitsSpec::validate`) and to the M3 mesh `:entrada :paths`
    /// invariants (`AplicacaoSpec::validate`) — every typed value
    /// carried by a slot is either absent or value-shape valid.
    pub fn validate(&self) -> Result<(), BehaviorError> {
        for (slot, path) in self.declared_slots() {
            validate_callback_path(slot, path.as_path())?;
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
        Ok(()) => Ok(()),
        Err(crate::render::PathShapeViolation::Empty) => Err(BehaviorError::EmptyPath { slot }),
        Err(crate::render::PathShapeViolation::Absolute) => Err(BehaviorError::AbsolutePath {
            slot,
            path: path.to_path_buf(),
        }),
        Err(crate::render::PathShapeViolation::ParentEscape) => Err(BehaviorError::ParentEscape {
            slot,
            path: path.to_path_buf(),
        }),
    }
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let paths: Vec<_> = b.declared_paths().cloned().collect();
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
        let paths: Vec<_> = b.declared_paths().cloned().collect();
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
            (":on-init", |p| BehaviorSpec {
                on_init: Some(p),
                ..Default::default()
            }),
            (":on-call", |p| BehaviorSpec {
                on_call: Some(p),
                ..Default::default()
            }),
            (":on-cast", |p| BehaviorSpec {
                on_cast: Some(p),
                ..Default::default()
            }),
            (":on-info", |p| BehaviorSpec {
                on_info: Some(p),
                ..Default::default()
            }),
            (":on-state-change", |p| BehaviorSpec {
                on_state_change: Some(p),
                ..Default::default()
            }),
            (":on-terminate", |p| BehaviorSpec {
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
            BehaviorError::AbsolutePath {
                slot: ":on-init",
                ..
            }
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
            BehaviorError::ParentEscape {
                slot: ":on-state-change",
                ..
            }
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
            BehaviorError::ParentEscape {
                slot: ":on-terminate",
                ..
            }
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
        assert!(matches!(err, BehaviorError::EmptyPath { slot: ":on-init" }));
    }
}
