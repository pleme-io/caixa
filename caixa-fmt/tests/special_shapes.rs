use caixa_fmt::{FmtConfig, format_source};

fn fmt(src: &str) -> String {
    let a = format_source(src, &FmtConfig::default()).unwrap();
    let b = format_source(&a, &FmtConfig::default()).unwrap();
    assert_eq!(a, b, "not idempotent:\n{src}");
    for (i, l) in a.lines().enumerate() {
        assert!(l.chars().count() <= 80, "line {} > 80 cols:\n{a}", i + 1);
    }
    a
}

#[test]
fn shapes() {
    for src in [
        r#"(define (resolve-target name ns cluster) (let ((base (lookup name))) (string-append base "/" ns)))"#,
        r#"(if (equal? (status-of r) 0) (log-info "ok") (log-error "the command failed with a nonzero status"))"#,
        r"(cond ((null? xs) nil) ((equal? (car xs) target) (car xs)) (else (recur (cdr xs))))",
        r"(let ((first-value (compute-the-first-value)) (second-value (compute-second))) (combine first-value second-value))",
        r#"(lambda (x y z) (string-append x "-" y "-" z "-suffix-that-is-long"))"#,
    ] {
        println!("=====\n{}", fmt(src));
    }
}

#[test]
fn a_signature_is_never_shattered() {
    let out = fmt(r"(define (resolve-target name ns cluster) body)");
    assert!(
        out.lines()
            .next()
            .unwrap()
            .contains("(resolve-target name ns cluster)"),
        "signature was split across lines:\n{out}"
    );
}

#[test]
fn an_if_keeps_its_test_on_the_head_line() {
    let out = fmt(r#"(if (equal? a b) (log-info "yes") (log-info "no"))"#);
    assert!(
        out.lines().next().unwrap().starts_with("(if (equal? a b)"),
        "test was orphaned:\n{out}"
    );
}
