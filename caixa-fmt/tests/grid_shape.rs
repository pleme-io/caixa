use caixa_fmt::{FmtConfig, format_source};

fn fmt(src: &str) -> String {
    let a = format_source(src, &FmtConfig::default()).unwrap();
    let b = format_source(&a, &FmtConfig::default()).unwrap();
    assert_eq!(a, b, "not idempotent:\n{src}");
    for l in a.lines() {
        assert!(l.chars().count() <= 80, "over 80:\n{a}");
    }
    assert!(
        !a.lines().any(|l| l.ends_with(' ')),
        "trailing whitespace:\n{a}"
    );
    a
}

#[test]
fn shapes() {
    for (label, src) in [
        (
            "data table",
            r#"[("us-east-1" 3 "healthy") ("us-west-2" 12 "degraded") ("eu-west-1" 1 "healthy")]"#,
        ),
        ("matrix", r"[[1 22 333] [4444 5 66] [7 888 9]]"),
        ("floats", r#"[("cpu" 0.5) ("mem" 12.25) ("disk" 100.0)]"#),
        ("ragged -> falls back", r"[(1 2 3) (4 5) (6 7 8)]"),
        ("nested cell -> falls back", r"[(1 (a b)) (2 (c d))]"),
    ] {
        println!("--- {label} ---\n{}", fmt(src));
    }
}

#[test]
fn numbers_right_align_so_outliers_are_visible() {
    let out = fmt(r#"[("a" 3) ("b" 12) ("c" 1)]"#);
    assert!(out.contains(r#"("a"  3)"#), "3 not right-aligned:\n{out}");
    assert!(out.contains(r#"("b" 12)"#), "12 misaligned:\n{out}");
}

#[test]
fn a_ragged_table_is_not_a_grid() {
    // Different arity => no columns exist => must not pretend.
    let out = fmt(r"[(1 2 3) (4 5) (6 7 8)]");
    assert!(!out.contains("(4 5 )"), "padded a ragged row:\n{out}");
}
