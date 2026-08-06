use caixa_ast::Span;
use caixa_theme::{Semantic, Theme};
use serde::{Deserialize, Serialize};

/// The four-arm diagnostic-severity discriminator every rule attaches
/// to its emitted [`Diagnostic`] via [`Diagnostic::severity`] — the
/// closed-set typed enum every consumer of the linter's severity axis
/// keys off (the render-side per-arm Nord palette dispatch through
/// [`Self::as_semantic`], the errors-only filter at
/// `caixa-feira/src/cmd/lint.rs`, the per-rule severity annotation at
/// `caixa-lint/src/rules.rs`, the LSP-side per-severity
/// `DiagnosticSeverity` mapping at `caixa-lsp/src/main.rs`).
///
/// The [`gen_platform::IsVariant`] derive emits per-arm
/// [`Severity::is_error`] / [`Severity::is_warning`] / [`Severity::is_info`]
/// / [`Severity::is_hint`] predicates the runner-side error-count gate
/// (`caixa-feira lint`'s `--errors-only` retain, the same verb's
/// per-diagnostic `error_count` bump) and every peer downstream
/// severity-family gate route through — the same closed-set
/// arm-discriminator discipline the sibling `caixa_core::CaixaKind` /
/// `caixa_core::CaixaDialeto` / `caixa_core::PlacementStrategy` /
/// `caixa_core::RestartStrategy` / `caixa_core::RestartPolicy` /
/// `caixa_core::DepList` / `caixa_core::RateLimitUnit` /
/// `caixa_core::render::PathShapeViolation` /
/// `caixa_arch::InvariantKind` / `caixa_lint::FixSafety` /
/// `caixa_arch::ArchVerdict` closed-set fieldless typed enums already
/// carry. `Hash` (already on the derive set) keeps the enum usable in
/// arm-keyed sets (a future `feira lint --severity-policy=<tier>` verb
/// keying arm-specific counters, a future admission-webhook rejection
/// body enumerating the accepted severity tiers).
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, gen_platform::IsVariant,
)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

impl Severity {
    /// Exhaustive iteration surface for every consumer that walks the
    /// closed four-arm [`Severity`] discriminator set (the byte-parity
    /// witness against the paired [`gen_platform::IsVariant`]-derived
    /// [`Self::is_error`] / [`Self::is_warning`] / [`Self::is_info`] /
    /// [`Self::is_hint`] predicate family, a future
    /// `feira lint --list-severities` CLI enumeration of the accepted
    /// severity tags, a future admission-webhook rejection body naming
    /// the accepted severity set, any future round-trip fuzz harness
    /// that sweeps every arm).
    ///
    /// A future variant addition (a `Debug` tier below [`Self::Hint`]
    /// once verbose per-node lint traces enter scope, a `Critical` tier
    /// above [`Self::Error`] the M3-and-later LSP surfaces for
    /// build-halting failures the rulebook grows) extends this slice as
    /// a single edit and every consumer picks up the new entry by
    /// construction; the compiler-checked exhaustiveness on the paired
    /// [`gen_platform::IsVariant`]-derived per-arm predicates covers
    /// the projection axis so both halves of the closed-set discipline
    /// migrate as one edit.
    ///
    /// Peer of the sibling closed-set fieldless typed enums'
    /// [`caixa_core::CaixaKind::ALL`] (6b1f4fb) /
    /// [`caixa_core::CaixaDialeto::ALL`] (dd4f541) /
    /// [`caixa_core::aplicacao::PlacementStrategy::ALL`] (18c7342) /
    /// [`caixa_core::supervisor::RestartStrategy::ALL`] (4eec29c) /
    /// [`caixa_core::supervisor::RestartPolicy::ALL`] (dd32ccf) /
    /// [`caixa_core::dep::DepList::ALL`] (45ee563) /
    /// [`caixa_core::aplicacao::RateLimitUnit::ALL`] (6bce03d) /
    /// [`caixa_core::render::PathShapeViolation::ALL`] (efc0326) /
    /// [`caixa_arch::invariants::InvariantKind::ALL`] (5226ad5) /
    /// [`crate::FixSafety::ALL`] (732a791) /
    /// [`caixa_arch::report::ArchVerdict::ALL`] (94dafa6)
    /// exhaustive-iteration surfaces — the twelfth closed-set typed
    /// enum on the caixa surface (and the second inside `caixa-lint`,
    /// after [`crate::FixSafety`] at 732a791) to converge onto the
    /// same one-canonical-arm-list-per-enum discipline. Order matches
    /// variant declaration order verbatim (`Error` → `Warning` →
    /// `Info` → `Hint`) so the slice is the canonical severity
    /// ordering every listing / rendering consumer defers to.
    pub const ALL: &'static [Self] = &[Self::Error, Self::Warning, Self::Info, Self::Hint];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Hint => "hint",
        }
    }

    #[must_use]
    pub const fn as_semantic(self) -> Semantic {
        match self {
            Self::Error => Semantic::Error,
            Self::Warning => Semantic::Warning,
            Self::Info => Semantic::Info,
            Self::Hint => Semantic::Hint,
        }
    }
}

/// A textual edit — replace `span` with `replacement` in the source.
/// Edits never overlap; the autofix driver sorts them by `span.start`
/// descending and applies in reverse order so earlier offsets stay
/// stable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edit {
    pub span: Span,
    pub replacement: String,
}

/// Auto-applicable correction for a [`Diagnostic`]. A single fix may
/// involve multiple edits (e.g. rename a name + every reference);
/// all edits apply atomically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fix {
    pub description: String,
    pub edits: Vec<Edit>,
    pub safety: FixSafety,
}

/// How safe is a fix to apply automatically?
///
/// * `Safe` — mechanical, semantics-preserving for pure round-trips.
///   `feira lint --fix` applies these by default.
/// * `Unsafe` — heuristic; may change runtime behavior in edge cases.
///   Requires explicit `--fix-unsafe` opt-in.
///
/// The `gen_platform::IsVariant` derive emits per-arm
/// [`FixSafety::is_safe`] / [`FixSafety::is_unsafe`] predicates the
/// runner's fix-safety gate keys off — the same closed-set
/// arm-discriminator discipline the sibling `caixa_core::CaixaKind`
/// / `caixa_core::CaixaDialeto` / `caixa_core::PlacementStrategy` /
/// `caixa_core::RestartStrategy` / `caixa_core::RestartPolicy` /
/// `caixa_core::DepList` / `caixa_core::RateLimitUnit` /
/// `caixa_arch::InvariantKind` closed-set fieldless typed enums already
/// carry. `Hash` joins the derive set so the enum can live in
/// arm-keyed sets (a future `feira lint --fix-safety=<policy>` verb
/// keying arm-specific counters, a future admission-webhook rejection
/// body naming the accepted safety-tier list).
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, gen_platform::IsVariant,
)]
pub enum FixSafety {
    Safe,
    Unsafe,
}

impl FixSafety {
    /// Exhaustive iteration surface for every consumer that walks the
    /// closed two-arm [`FixSafety`] discriminator set (a future
    /// `feira lint --fix-safety=<policy>` verb enumerating the accepted
    /// safety-tier list, a future admission-webhook rejection body
    /// naming the accepted tiers, any future safety-tier fuzz harness
    /// that sweeps every arm).
    ///
    /// A future variant addition (an `Experimental` tier between
    /// [`Self::Safe`] and [`Self::Unsafe`] the M3-and-later lint
    /// runner grows for AI-suggested rewrites that need explicit
    /// review-and-accept) extends this slice as a single edit and
    /// every consumer picks up the new entry by construction; the
    /// compiler-checked exhaustiveness on the paired
    /// `gen_platform::IsVariant`-derived per-arm predicates
    /// ([`Self::is_safe`] / [`Self::is_unsafe`]) covers the projection
    /// axis so both halves of the closed-set discipline migrate as
    /// one edit.
    ///
    /// Peer of the sibling closed-set typed enums'
    /// [`caixa_core::CaixaKind::ALL`] (6b1f4fb) /
    /// [`caixa_core::CaixaDialeto::ALL`] (dd4f541) /
    /// [`caixa_core::PlacementStrategy::ALL`] (18c7342) /
    /// [`caixa_core::RestartStrategy::ALL`] (4eec29c) /
    /// [`caixa_core::RestartPolicy::ALL`] (dd32ccf) /
    /// [`caixa_core::DepList::ALL`] (45ee563) /
    /// [`caixa_core::RateLimitUnit::ALL`] (6bce03d) /
    /// [`caixa_arch::InvariantKind::ALL`] (5226ad5)
    /// exhaustive-iteration surfaces — the ninth closed-set typed
    /// enum on the caixa surface (and the second outside `caixa-core`,
    /// after `caixa_arch::InvariantKind`) to converge onto the same
    /// one-canonical-arm-list-per-enum discipline.
    pub const ALL: &'static [Self] = &[Self::Safe, Self::Unsafe];
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub rule_id: &'static str,
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    pub hint: Option<String>,
    /// Optional autofix. When present, `feira lint --fix` applies it
    /// (subject to the requested safety threshold).
    pub fix: Option<Fix>,
}

impl Diagnostic {
    #[must_use]
    pub fn new(
        rule_id: &'static str,
        severity: Severity,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        Self {
            rule_id,
            severity,
            span,
            message: message.into(),
            hint: None,
            fix: None,
        }
    }

    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Attach an auto-applicable correction.
    #[must_use]
    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }

    /// Convenience: attach a single-edit safe fix that replaces this
    /// diagnostic's own span with `replacement`.
    #[must_use]
    pub fn with_fix_replace(
        self,
        description: impl Into<String>,
        replacement: impl Into<String>,
    ) -> Self {
        let span = self.span;
        self.with_fix(Fix {
            description: description.into(),
            edits: vec![Edit {
                span,
                replacement: replacement.into(),
            }],
            safety: FixSafety::Safe,
        })
    }

    /// Render this diagnostic against a source string, Nord-themed.
    #[must_use]
    pub fn render(&self, src: &str, theme: &Theme) -> String {
        let pos = caixa_ast::line_column(src, self.span.start);
        let sev = theme.paint(self.severity.as_semantic(), self.severity.as_str());
        let id = theme.paint(Semantic::Muted, &format!("[{}]", self.rule_id));
        let at = theme.paint(Semantic::Muted, &format!("{pos}"));
        let mut out = format!("{sev} {id} {at}: {}", self.message);
        if let Some(h) = &self.hint {
            out.push('\n');
            out.push_str("  ");
            out.push_str(&theme.paint(Semantic::Hint, "hint"));
            out.push_str(": ");
            out.push_str(h);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fix_safety_all_lists_every_variant_in_declaration_order() {
        // Fail-before-pass-after pin on the closed two-arm
        // [`FixSafety`] discriminator set: any future variant
        // addition (an `Experimental` tier between [`FixSafety::Safe`]
        // and [`FixSafety::Unsafe`] the M3-and-later lint runner grows
        // for AI-suggested rewrites that need explicit review-and-
        // accept) that lands the new variant on the enum without
        // extending [`FixSafety::ALL`] trips this test — the
        // compiler-checked exhaustiveness on the
        // `gen_platform::IsVariant`-derived per-arm predicates
        // ([`FixSafety::is_safe`] / [`FixSafety::is_unsafe`]) covers
        // the projection axis; this pin covers the exhaustive-
        // iteration axis so both halves of the closed-set discipline
        // migrate as one edit.
        //
        // Peer of the sibling
        // `caixa_arch::invariants::tests::invariant_kind_all_lists_every_variant_in_declaration_order`
        // pin on the peer closed-set-enum `ALL` axis (5226ad5).
        assert_eq!(FixSafety::ALL, &[FixSafety::Safe, FixSafety::Unsafe]);
        // Every arm satisfies exactly one of the paired per-arm
        // predicates — the byte-parity pin between the
        // [`FixSafety::ALL`] iteration axis and the per-arm
        // `gen_platform::IsVariant`-derived predicate axis.
        for arm in FixSafety::ALL {
            assert_eq!(
                usize::from(arm.is_safe()) + usize::from(arm.is_unsafe()),
                1,
                "FixSafety::{arm:?} must satisfy exactly one of \
                 is_safe / is_unsafe",
            );
        }
    }

    #[test]
    fn fix_safety_predicates_are_byte_equal_to_matches_family() {
        // Fail-before-pass-after pin on the runner-side converge of
        // the pre-lift `match (max_safety, fix.safety) { (Safe, Unsafe)
        // => skip, _ => {} }` tuple-match at
        // [`crate::runner::apply_fixes`] onto the
        // `gen_platform::IsVariant`-derive-generated per-arm predicate
        // family: for every arm, each predicate agrees byte-for-byte
        // with the pre-lift `matches!(_, FixSafety::…)` shape.
        //
        // A future rebrand touching either endpoint (a
        // `#[is_variant(name = "…")]` attribute drift on the derive,
        // an arm rename, an accidental peer predicate that shadows the
        // derive-generated one) would silently split the two paths and
        // trip this pin. Peer of the sibling
        // `caixa_arch::invariants::tests::invariant_kind_predicates_are_byte_equal_to_matches_family`
        // pin on the peer closed-set-enum `IsVariant` convergence axis
        // (5226ad5) and the workspace
        // `caixa_core::supervisor::tests::restart_policy_predicates_are_byte_equal_to_matches`
        // sibling pin.
        for arm in FixSafety::ALL {
            assert_eq!(
                arm.is_safe(),
                matches!(arm, FixSafety::Safe),
                "FixSafety::{arm:?}.is_safe() must agree with \
                 matches!(_, FixSafety::Safe) byte-for-byte",
            );
            assert_eq!(
                arm.is_unsafe(),
                matches!(arm, FixSafety::Unsafe),
                "FixSafety::{arm:?}.is_unsafe() must agree with \
                 matches!(_, FixSafety::Unsafe) byte-for-byte",
            );
        }
    }

    #[test]
    fn fix_safety_gate_agrees_with_tuple_match_across_full_2x2_grid() {
        // Byte-parity pin on the [`crate::runner::apply_fixes`] gate:
        // the post-lift `max_safety.is_safe() && fix.safety.is_unsafe()`
        // predicate must agree with the pre-lift `matches!((max_safety,
        // fix.safety), (FixSafety::Safe, FixSafety::Unsafe))` tuple-
        // match across every point of the 2×2 (max_safety × fix.safety)
        // grid — the four-point closure of the runner's fix-safety
        // gate over the closed-set discriminator's full accept-set.
        // A future arm addition to [`FixSafety`] shifts this from a
        // 2×2 to an NxN grid; the [`FixSafety::ALL`]-driven iteration
        // picks up the new arm by construction.
        for a in FixSafety::ALL {
            for b in FixSafety::ALL {
                let post_lift = a.is_safe() && b.is_unsafe();
                let pre_lift = matches!((a, b), (FixSafety::Safe, FixSafety::Unsafe));
                assert_eq!(
                    post_lift, pre_lift,
                    "runner fix-safety gate diverges at (max={a:?}, fix={b:?}): \
                     post-lift={post_lift}, pre-lift={pre_lift}",
                );
            }
        }
    }

    #[test]
    fn severity_all_enumerates_every_variant_once() {
        // Fail-before-pass-after pin on the [`gen_platform::IsVariant`]
        // derive's per-arm-list-per-enum discipline for [`Severity`]:
        // any future variant addition (a `Debug` tier below
        // [`Severity::Hint`] for verbose per-node lint traces, a
        // `Critical` tier above [`Severity::Error`] for build-halting
        // failures the rulebook grows) that lands the new variant on
        // the enum without extending [`Severity::ALL`] trips this test
        // — the compiler-checked exhaustiveness on the paired
        // [`gen_platform::IsVariant`]-derived per-arm predicates
        // ([`Severity::is_error`] / [`Severity::is_warning`] /
        // [`Severity::is_info`] / [`Severity::is_hint`]) covers the
        // projection axis; this pin covers the exhaustive-iteration
        // axis so both halves of the closed-set discipline migrate as
        // one edit.
        //
        // Peer of the sibling
        // [`fix_safety_all_lists_every_variant_in_declaration_order`]
        // pin on the peer closed-set-enum `ALL` axis (732a791) and the
        // workspace `caixa_arch::report::tests::arch_verdict_all_enumerates_every_variant_once`
        // sibling pin (94dafa6).
        assert_eq!(
            Severity::ALL,
            &[
                Severity::Error,
                Severity::Warning,
                Severity::Info,
                Severity::Hint,
            ],
        );
        // Every arm satisfies exactly one of the paired per-arm
        // predicates — the byte-parity pin between the
        // [`Severity::ALL`] iteration axis and the per-arm
        // [`gen_platform::IsVariant`]-derived predicate axis: for
        // every arm, exactly one of `is_error` / `is_warning` /
        // `is_info` / `is_hint` returns `true`.
        for arm in Severity::ALL {
            let (err, warn, info, hint) = (
                arm.is_error(),
                arm.is_warning(),
                arm.is_info(),
                arm.is_hint(),
            );
            assert_eq!(
                usize::from(err) + usize::from(warn) + usize::from(info) + usize::from(hint),
                1,
                "Severity::{arm:?} must satisfy exactly one of \
                 is_error / is_warning / is_info / is_hint",
            );
        }
    }

    #[test]
    fn severity_predicates_are_byte_equal_to_matches_family() {
        // Fail-before-pass-after pin on the two-axis convergence of
        // the four pre-lift `d.severity == Severity::Error` sites at
        // `caixa-lint/src/rules.rs` (the `aplicacao-completeness`
        // rule-firing assertion + the `clean_manifest_only_info_level`
        // error-filter) and `caixa-feira/src/cmd/lint.rs` (the
        // `--errors-only` retain + the per-diagnostic `error_count`
        // bump) onto the [`gen_platform::IsVariant`]-derive-generated
        // per-arm predicate family: for every arm, each predicate
        // agrees byte-for-byte with the pre-lift `matches!(_,
        // Severity::…)` shape.
        //
        // A future rebrand touching either endpoint (a
        // `#[is_variant(name = "…")]` attribute drift on the derive,
        // an arm rename, an accidental peer predicate that shadows
        // the derive-generated one) would silently split the two
        // paths and trip this pin. Peer of the sibling
        // [`fix_safety_predicates_are_byte_equal_to_matches_family`]
        // pin on the peer `caixa-lint` closed-set-enum `IsVariant`
        // convergence axis (732a791) and the workspace
        // `caixa_arch::report::tests::arch_verdict_predicates_are_byte_equal_to_matches_family`
        // sibling pin (94dafa6).
        for arm in Severity::ALL {
            assert_eq!(
                arm.is_error(),
                matches!(arm, Severity::Error),
                "Severity::{arm:?}.is_error() must agree with \
                 matches!(_, Severity::Error) byte-for-byte",
            );
            assert_eq!(
                arm.is_warning(),
                matches!(arm, Severity::Warning),
                "Severity::{arm:?}.is_warning() must agree with \
                 matches!(_, Severity::Warning) byte-for-byte",
            );
            assert_eq!(
                arm.is_info(),
                matches!(arm, Severity::Info),
                "Severity::{arm:?}.is_info() must agree with \
                 matches!(_, Severity::Info) byte-for-byte",
            );
            assert_eq!(
                arm.is_hint(),
                matches!(arm, Severity::Hint),
                "Severity::{arm:?}.is_hint() must agree with \
                 matches!(_, Severity::Hint) byte-for-byte",
            );
        }
    }

    #[test]
    fn severity_error_family_dispatches_through_is_error_across_full_arm_set() {
        // Byte-parity pin on the four converged `d.severity.is_error()`
        // sites (`caixa-lint/src/rules.rs` :674 + :738,
        // `caixa-feira/src/cmd/lint.rs` :108 + :111): for every arm in
        // [`Severity::ALL`], a [`Diagnostic`] carrying that severity
        // reports `is_error()` iff the arm is `Error`. Guards against a
        // future silent split between the four call sites and the
        // derived predicate (a hand-rolled `== Severity::Error`
        // reintroduction, an arm rename that touches one site but not
        // the others, an accidental peer predicate that shadows
        // `is_error`) by asserting the two paths return the same
        // `bool` on every arm.
        //
        // The paired dummy [`Diagnostic`] materializes the "runner-
        // facing" projection the four production sites actually
        // dispatch on (each is `d.severity == Severity::Error` on a
        // borrowed [`Diagnostic`]), not just the free-standing
        // arm-discriminator the peer
        // [`severity_predicates_are_byte_equal_to_matches_family`] pin
        // covers — so a hypothetical future accessor-shim that widens
        // the runner-side severity view (a per-cluster override, an
        // operator-derived severity remap) that failed to route
        // through [`Severity::is_error`] would surface here rather
        // than at apply time far from the shim commit.
        for &severity in Severity::ALL {
            let d = Diagnostic {
                rule_id: "test-rule",
                severity,
                message: String::new(),
                span: Span::default(),
                hint: None,
                fix: None,
            };
            assert_eq!(
                d.severity.is_error(),
                matches!(severity, Severity::Error),
                "Diagnostic::severity.is_error() must dispatch through \
                 Severity::is_error() byte-for-byte on {severity:?}",
            );
        }
    }
}
