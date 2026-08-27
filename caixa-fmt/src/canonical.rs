//! The canonicality gate — formatted-ness as a PARSE-TIME property.
//!
//! `gofmt` and `rustfmt` are conventions: a CI job, a pre-commit hook, a
//! habit. All three are bypassable, and all three make "formatted" a fact
//! about a *repository* rather than about the *language*.
//!
//! [`parse_canonical`] makes it a fact about the language. It parses,
//! re-renders, compares bytes, and REFUSES to hand back an AST when the
//! two differ. Wired into the reader that `tatara-script` and the eval
//! path use, a non-canonical `.tlisp` does not run — not because a linter
//! objected, but because there is no code path from non-canonical source
//! to a syntax tree.
//!
//! ## What tier this actually is
//!
//! **Parse-rejected, NOT truly-unrepresentable.** A `String` holding
//! badly-formatted source is still perfectly representable; what is
//! unrepresentable is a `Vec<Node>` *derived from* one. That is the same
//! tier as a refinement type whose constructor validates, and it is the
//! honest ceiling for a property of TEXT. Claiming more would be rounding
//! up: nothing here stops a caller from using [`caixa_ast::parse`]
//! directly, and `caixa-lint`/`caixa-lsp` legitimately must, because a
//! tool that reports on unformatted code has to be able to read it.
//!
//! The invariant is therefore: **every EVALUATION path goes through this
//! gate**; the diagnostic paths deliberately do not.
//!
//! ## Ordering constraint for adoption
//!
//! Turning this on is a flag day. The moment an evaluator calls
//! `parse_canonical`, every non-canonical file in reach stops running. The
//! sequence is: land the formatter → mass-format the corpus in one commit
//! → *then* switch the evaluator over. Reversed, it bricks the corpus.

use caixa_ast::{Node, ParseError, parse};
use thiserror::Error;

use crate::config::FmtConfig;
use crate::printer::format_nodes;

/// Why a source could not be read as canonical tatara-lisp.
#[derive(Debug, Error)]
pub enum CanonicalError {
    #[error("parse: {0}")]
    Parse(#[from] ParseError),

    /// The source parses, but is not in canonical form.
    ///
    /// Carries the canonical rendering so a caller can offer the fix
    /// rather than only the complaint — an editor gate that can only say
    /// "no" is a worse tool than one that can say "no, and here it is".
    #[error(
        "not canonically formatted (line {line}): expected {expected:?}, found {found:?}. \
         Run `feira fmt` — tatara-lisp has exactly one canonical rendering, and \
         evaluation is gated on it."
    )]
    NotCanonical {
        /// 1-indexed line of the first divergence.
        line: usize,
        /// The canonical text of that line.
        expected: String,
        /// What the source had instead.
        found: String,
        /// The full canonical rendering of the whole source.
        canonical: String,
    },
}

impl CanonicalError {
    /// Substrate primitive constructor for the canonicality-gate refusal
    /// diagnostic. Folds the pre-lift open-coded six-line
    /// `Err(CanonicalError::NotCanonical { line, expected, found,
    /// canonical })` four-slot struct-literal inside
    /// [`parse_canonical_with`]'s divergence arm onto one substrate
    /// primitive so every wire-up on this variant reads through one
    /// dispatch rather than the pre-lift open-coded block.
    ///
    /// Peer with the substrate-primitive ctor discipline the sibling
    /// per-envelope error families across the workspace already carry
    /// (`ResolveError::missing_path` / `ResolveError::missing_pin`,
    /// `AplicacaoError::entrada_member_missing` /
    /// `AplicacaoError::contrato_cycle`,
    /// `SupervisorError::child_caixa_invalid`, etc.), extended here onto
    /// the last unlifted `CanonicalError` struct-literal wire-up under
    /// [`parse_canonical_with`]'s canonicality-gate refusal path.
    ///
    /// All four slots are owned pass-throughs (`line: usize` a `Copy`
    /// scalar, `expected` / `found` / `canonical` [`String`]s the caller
    /// already owns) so the ctor is byte-identical to the pre-lift form
    /// and no per-arm re-allocation lands on the ctor path — the
    /// `expected` + `found` pair threads verbatim from
    /// [`first_divergence`]'s reconstructed `(usize, String, String)`
    /// return, and `canonical` threads verbatim from the outer
    /// [`format_nodes`]-derived rendering the divergence arm already
    /// owns.
    ///
    /// Every future consumer that surfaces the same canonicality-gate
    /// refusal outside [`parse_canonical_with`] — a deferred `feira fmt
    /// --check`-style pre-flight probe reading the divergence into a
    /// structured diagnostic report, a per-file batch gate the
    /// evaluator's flag-day switchover carries once the corpus is
    /// mass-formatted, an LSP diagnostic surface projecting the
    /// `line`/`expected`/`found`/`canonical` tuple into a text-document
    /// hover — reaches the variant through one call rather than
    /// re-inlining the four-slot struct-literal in lockstep with
    /// [`parse_canonical_with`]'s wire-up.
    #[must_use]
    pub fn not_canonical(
        line: usize,
        expected: String,
        found: String,
        canonical: String,
    ) -> CanonicalError {
        CanonicalError::NotCanonical {
            line,
            expected,
            found,
            canonical,
        }
    }
}

/// Parse `src`, but only if it is already canonically formatted.
///
/// This is the gate. See the module docs for the tier it actually holds
/// and for the ordering constraint on switching it on.
pub fn parse_canonical(src: &str) -> Result<Vec<Node>, CanonicalError> {
    parse_canonical_with(src, &FmtConfig::default())
}

/// [`parse_canonical`] against an explicit config.
///
/// Exists for the formatter's own tests. Production callers use
/// [`parse_canonical`]: a gate whose definition of "canonical" varies per
/// caller is not a gate, and `FmtConfig` is deliberately not plumbed to
/// any CLI flag for exactly that reason.
pub fn parse_canonical_with(src: &str, cfg: &FmtConfig) -> Result<Vec<Node>, CanonicalError> {
    let nodes = parse(src)?;
    let canonical = format_nodes(&nodes, cfg);
    if canonical == src {
        return Ok(nodes);
    }
    let (line, expected, found) = first_divergence(&canonical, src);
    Err(CanonicalError::not_canonical(
        line, expected, found, canonical,
    ))
}

/// Is `src` already canonical? Cheaper to read at a call site than a
/// `matches!` over the error, and it is the shape a `--check` wants.
///
/// A source that does not PARSE is not canonical either — there is no
/// third answer, and returning `true` for unparseable input would make a
/// `--check` pass on a broken file.
#[must_use]
pub fn is_canonical(src: &str) -> bool {
    parse_canonical(src).is_ok()
}

/// First differing line between the canonical rendering and the source,
/// as `(1-indexed line, expected, found)`.
///
/// Line-granular on purpose. A byte offset is precise and unreadable; the
/// operator's next action is to look at a line.
fn first_divergence(canonical: &str, src: &str) -> (usize, String, String) {
    let mut c = canonical.lines();
    let mut s = src.lines();
    let mut n = 0usize;
    loop {
        n += 1;
        match (c.next(), s.next()) {
            (None, None) => {
                // Every line agreed: the difference is trailing-newline
                // only. Report it as the line past the end rather than
                // pretending there is no difference.
                return (
                    n,
                    String::from("<end of file>"),
                    String::from("<end of file>"),
                );
            }
            (a, b) if a != b => {
                return (
                    n,
                    a.unwrap_or("<end of file>").to_string(),
                    b.unwrap_or("<end of file>").to_string(),
                );
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_source_parses() {
        let src = "(defcaixa demo :kind :Biblioteca)\n";
        assert!(is_canonical(src), "{src:?} should already be canonical");
        assert_eq!(parse_canonical(src).unwrap().len(), 1);
    }

    /// The gate's whole reason to exist: source that parses but is not
    /// canonical must NOT yield an AST.
    #[test]
    fn non_canonical_source_is_refused_even_though_it_parses() {
        let src = "(defcaixa    demo   :kind :Biblioteca)\n";
        assert!(
            caixa_ast::parse(src).is_ok(),
            "precondition: the source parses fine"
        );
        let err = parse_canonical(src).expect_err("must be refused");
        match err {
            CanonicalError::NotCanonical {
                line, canonical, ..
            } => {
                assert_eq!(line, 1);
                assert_eq!(canonical, "(defcaixa demo :kind :Biblioteca)\n");
            }
            other => panic!("expected NotCanonical, got {other:?}"),
        }
    }

    /// The error carries the fix, not just the complaint.
    #[test]
    fn refusal_carries_the_canonical_rendering() {
        let src = "(a     b)\n";
        let Err(CanonicalError::NotCanonical { canonical, .. }) = parse_canonical(src) else {
            panic!("must be refused");
        };
        assert!(is_canonical(&canonical), "the offered fix must itself pass");
    }

    /// Unparseable input is not canonical — there is no third answer, and
    /// a `--check` that passed on a broken file would be worse than none.
    #[test]
    fn unparseable_is_not_canonical() {
        assert!(!is_canonical("(unclosed"));
        assert!(matches!(
            parse_canonical("(unclosed"),
            Err(CanonicalError::Parse(_))
        ));
    }

    /// Byte-identity pin between the substrate-primitive
    /// [`CanonicalError::not_canonical`] ctor and the pre-lift open-coded
    /// `Self::NotCanonical { .. }` struct-literal on a representative
    /// four-slot fixture (line = 3, whitespace divergence, multi-line
    /// canonical) — a silent regression that widens the ctor path,
    /// normalizes any slot, drops a `.to_string()`, or de-lifts the
    /// wire-up back to the open-coded struct-literal trips at
    /// caixa-fmt build time on both `Debug` and `Display` renders.
    #[test]
    fn not_canonical_ctor_matches_struct_literal_wrap() {
        let line = 3usize;
        let expected = "(defcaixa demo :kind :Biblioteca)".to_string();
        let found = "(defcaixa    demo   :kind :Biblioteca)".to_string();
        let canonical = "(a b)\n(c d)\n(defcaixa demo :kind :Biblioteca)\n".to_string();
        let via_struct = CanonicalError::NotCanonical {
            line,
            expected: expected.clone(),
            found: found.clone(),
            canonical: canonical.clone(),
        };
        let via_ctor = CanonicalError::not_canonical(line, expected, found, canonical);
        assert_eq!(
            format!("{via_struct:?}"),
            format!("{via_ctor:?}"),
            "not_canonical ctor Debug must byte-equal the pre-lift open-coded struct-literal"
        );
        assert_eq!(
            via_struct.to_string(),
            via_ctor.to_string(),
            "not_canonical ctor Display must byte-equal the pre-lift open-coded struct-literal"
        );
    }

    /// Boundary sweep — routes four distinct
    /// `(line, expected, found, canonical)` shapes through the ctor and
    /// pins that every slot arrives on the variant verbatim, so any
    /// wrapper-side silent trim / lowercase / truncate / dedup / re-order
    /// on the ctor path surfaces at assert time rather than at a
    /// downstream diagnostic consumer that reads the field back. Covers:
    /// the trailing-newline-only floor (`first_divergence` returns
    /// `<end of file>` sentinels), an empty-canonical degenerate,
    /// whitespace-only divergences with a multi-line canonical, and a
    /// large `line` value so any silent `u32`-narrowing on the scalar
    /// path surfaces.
    #[test]
    fn not_canonical_ctor_routes_fields_verbatim() {
        for (line, expected, found, canonical) in [
            (
                1usize,
                String::from("<end of file>"),
                String::from("<end of file>"),
                String::from("(a b)\n"),
            ),
            (7, String::new(), String::from("x"), String::new()),
            (
                12,
                String::from("(defcaixa demo :kind :Biblioteca)"),
                String::from("  (defcaixa demo :kind :Biblioteca) "),
                String::from(";; header\n(a b)\n(defcaixa demo :kind :Biblioteca)\n"),
            ),
            (
                usize::from(u16::MAX) + 5,
                String::from("(a b)"),
                String::from("(a   b)"),
                String::from("(a b)\n"),
            ),
        ] {
            let err = CanonicalError::not_canonical(
                line,
                expected.clone(),
                found.clone(),
                canonical.clone(),
            );
            let CanonicalError::NotCanonical {
                line: got_line,
                expected: got_expected,
                found: got_found,
                canonical: got_canonical,
            } = err
            else {
                panic!("ctor must construct NotCanonical, not another variant");
            };
            assert_eq!(got_line, line, "line slot must route verbatim");
            assert_eq!(got_expected, expected, "expected slot must route verbatim");
            assert_eq!(got_found, found, "found slot must route verbatim");
            assert_eq!(
                got_canonical, canonical,
                "canonical slot must route verbatim"
            );
        }
    }

    /// End-to-end wire-up pin — [`parse_canonical_with`]'s divergence arm
    /// must reach the variant through the substrate-primitive ctor, not
    /// through the pre-lift open-coded struct-literal. A silent de-lift
    /// back to the struct-literal, or a widening of the divergence arm's
    /// slot selection, trips the byte-identity assert against
    /// [`CanonicalError::not_canonical`] with the same
    /// `(line, expected, found, canonical)` tuple recomputed on the
    /// fixture — matching the peer per-envelope end-to-end wire-up-
    /// routes-through-ctor pin the sibling `contrato_cycle` /
    /// `missing_pin` ctor lifts already carry.
    #[test]
    fn parse_canonical_with_routes_through_not_canonical_ctor() {
        let src = "(defcaixa    demo   :kind :Biblioteca)\n";
        let cfg = FmtConfig::default();
        let observed = parse_canonical_with(src, &cfg).expect_err("must be refused");
        let nodes = caixa_ast::parse(src).expect("fixture parses");
        let canonical = format_nodes(&nodes, &cfg);
        let (line, expected, found) = first_divergence(&canonical, src);
        let via_ctor = CanonicalError::not_canonical(line, expected, found, canonical);
        assert_eq!(
            format!("{observed:?}"),
            format!("{via_ctor:?}"),
            "parse_canonical_with's divergence arm must reach NotCanonical through the \
             not_canonical ctor byte-equal to the recomputed same-fixture wrap"
        );
        assert_eq!(
            observed.to_string(),
            via_ctor.to_string(),
            "Display must byte-equal the ctor-side wrap"
        );
    }

    /// The gate must agree with the formatter on the whole corpus shape,
    /// or `feira fmt` would produce files the evaluator then refuses.
    #[test]
    fn formatter_output_is_always_canonical() {
        for src in [
            "(defcaixa demo :kind :Biblioteca :package { :name \"d\" } :v [ 1 2 ])",
            ";; lead\n(a b c)",
            "(defcaixa d\n  ;; inner\n  :k 1\n  ;; dangling\n  )",
            "()",
            "{}",
            "[]",
            "(a (b (c (d))))",
        ] {
            let nodes = caixa_ast::parse(src).expect("fixture parses");
            let once = format_nodes(&nodes, &FmtConfig::default());
            assert!(
                is_canonical(&once),
                "formatter emitted non-canonical output for {src:?}:\n{once}"
            );
        }
    }
}
