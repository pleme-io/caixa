use caixa_ast::NodeKind;
use caixa_fmt::{FmtConfig, format_source};

/// Writing a value down and reading it back must be the IDENTITY — same
/// bits and same node kind. Anything less means the document is not
/// homoiconic: the text is a lossy picture of the data rather than the
/// data itself.
fn assert_round_trips(v: f64, label: &str) {
    let out = format_source(&format!("(x {v:?})"), &FmtConfig::default())
        .unwrap_or_else(|e| panic!("{label}: emitted text does not re-lex: {e}"));
    let nodes = caixa_ast::parse(&out).unwrap_or_else(|e| panic!("{label}: reparse failed: {e}"));
    let Some(items) = nodes[0].kind.as_list() else {
        panic!("{label}: shape changed")
    };
    match &items[1].kind {
        NodeKind::Float(b) => assert_eq!(
            b.to_bits(),
            v.to_bits(),
            "{label}: value changed — wrote {v:e}, read back {b:e}\n{out}"
        ),
        other => panic!("{label}: a float read back as {other:?} — kind changed\n{out}"),
    }
}

#[test]
fn extremes_survive_the_round_trip() {
    for (label, v) in [
        ("1e300", 1e300f64),
        ("1e-300", 1e-300f64),
        ("min_positive", f64::MIN_POSITIVE),
        ("max", f64::MAX),
        ("1e100", 1e100f64),
    ] {
        assert_round_trips(v, label);
    }
}

#[test]
fn ordinary_values_are_unchanged_and_still_readable() {
    for (label, v) in [
        ("0.1+0.2", 0.1 + 0.2),
        ("1/3", 1.0 / 3.0),
        ("2^53+1", 9007199254740993.0),
        ("2.5", 2.5f64),
        ("integral", 2.0f64),
        ("neg zero", -0.0f64),
    ] {
        assert_round_trips(v, label);
    }
    // Plain decimal is preferred at equal-or-better length: a config file
    // full of `1e0` instead of `1.0` would be a regression in readability.
    let out = format_source("(x 2.0)", &FmtConfig::default()).unwrap();
    assert!(out.contains("2.0"), "readable form lost:\n{out}");
}

#[test]
fn extremes_are_compact_not_three_hundred_characters() {
    let out = format_source(&format!("(x {:?})", 1e-300f64), &FmtConfig::default()).unwrap();
    assert!(
        out.trim().len() < 20,
        "still expanding to a wall of digits:\n{out}"
    );
}
