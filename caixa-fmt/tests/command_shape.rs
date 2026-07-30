use caixa_fmt::{FmtConfig, format_source};

fn fmt(src: &str) -> String {
    let a = format_source(src, &FmtConfig::default()).unwrap();
    let b = format_source(&a, &FmtConfig::default()).unwrap();
    assert_eq!(a, b, "not idempotent:\n{src}");
    a
}

#[test]
fn command_shape_probe() {
    // Real corpus shapes, verbatim from the .tlisp corpus.
    for src in [
        r#"(exec-capture "curl" "-sf" "-m" "5" "-X" "PUT" "--data-binary" "@payload.json" endpoint)"#,
        r#"(exec-capture "aws" "sts" "get-caller-identity" "--profile" aws-profile)"#,
        r#"(exec-capture "kubectl" "create" "namespace" ns "--dry-run=client" "-o" "yaml")"#,
        r#"(exec-capture "mkdir" "-p" dir)"#,
    ] {
        let out = fmt(src);
        let maxw = out.lines().map(|l| l.chars().count()).max().unwrap_or(0);
        println!("--- (max col {maxw}) ---\n{out}");
        assert!(maxw <= 80, "exceeded 80 cols: {maxw}\n{out}");
    }
}

#[test]
fn a_flag_never_separates_from_its_value() {
    let out = fmt(r#"(exec-capture "curl" "-sf" "-m" "5" "-X" "PUT" endpoint)"#);
    assert!(out.contains(r#""-m" "5""#), "flag/value split:\n{out}");
    assert!(out.contains(r#""-X" "PUT""#), "flag/value split:\n{out}");
}

#[test]
fn a_short_command_still_fits_on_one_line() {
    assert_eq!(
        fmt(r#"(exec-capture "mkdir" "-p" dir)"#).trim(),
        r#"(exec-capture "mkdir" "-p" dir)"#
    );
}
