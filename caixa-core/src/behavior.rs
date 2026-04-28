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

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
    /// Iterate over every declared callback path. Used by the
    /// layout checker.
    pub fn declared_paths(&self) -> impl Iterator<Item = &PathBuf> {
        [
            &self.on_init,
            &self.on_call,
            &self.on_cast,
            &self.on_info,
            &self.on_state_change,
            &self.on_terminate,
        ]
        .into_iter()
        .filter_map(Option::as_ref)
    }

    /// True when no callback is declared.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.declared_paths().next().is_none()
    }
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
        assert_eq!(paths, vec![PathBuf::from("a.lisp"), PathBuf::from("b.lisp")]);
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
}
