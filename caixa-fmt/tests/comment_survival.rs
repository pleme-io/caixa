use caixa_fmt::{FmtConfig, format_source};

/// Every comment in the input must appear in the output. A formatter that
/// deletes a comment is destroying the author's intent, and this is the
/// invariant an idempotence check CANNOT catch — deleting is perfectly
/// idempotent: delete once, and it is stable forever.
fn assert_no_comment_lost(src: &str, label: &str) {
    let out = format_source(src, &FmtConfig::default()).unwrap();
    let want: Vec<&str> = src
        .lines()
        .filter_map(|l| l.split_once(';').map(|(_, c)| c.trim()))
        .filter(|c| !c.is_empty())
        .collect();
    for c in &want {
        assert!(
            out.contains(c),
            "{label}: LOST comment {c:?}\n--- in ---\n{src}\n--- out ---\n{out}"
        );
    }
}

#[test]
fn a_trailing_comment_on_the_final_form_survives() {
    assert_no_comment_lost("(define x 1) ; why x is one\n", "final-form trailing");
}

#[test]
fn a_comment_at_end_of_file_survives() {
    assert_no_comment_lost("(define x 1)\n;; a closing note\n", "eof comment");
}

#[test]
fn trailing_comments_survive_a_realistic_file() {
    assert_no_comment_lost(
        "(define a 1) ; first\n(define b 2) ; second\n(define c 3) ; third\n",
        "multiple trailing",
    );
}

#[test]
fn formatting_is_still_idempotent_with_comments() {
    let src = "(define x 1) ; why\n";
    let a = format_source(src, &FmtConfig::default()).unwrap();
    let b = format_source(&a, &FmtConfig::default()).unwrap();
    assert_eq!(a, b, "not idempotent:\n{a}");
}
