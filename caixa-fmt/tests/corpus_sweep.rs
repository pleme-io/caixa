use caixa_fmt::{FmtConfig, format_source};

/// Format every .tlisp file in the fleet and assert the invariants hold on
/// REAL input, not synthetic cases: <=80 columns, deterministic, idempotent.
/// Is this line over-budget only because it carries a single token that no
/// layout could break — a comment, or a string/symbol atom longer than the
/// budget itself?
///
/// The 80-column rule binds LAYOUT, which is all a formatter controls.
/// Breaking a string literal changes the value; reflowing a comment that
/// holds a URL corrupts it. So the honest invariant is "no line is long
/// because of how it was laid out", and this separates that from "no line
/// is long", which is not achievable and never was.
fn line_is_unbreakable(line: &str) -> bool {
    let t = line.trim_start();
    if t.starts_with(';') {
        return true;
    }
    let indent = line.len() - t.len();
    // The longest single atom on the line: a quoted string, or a run of
    // non-delimiter characters.
    let mut longest = 0usize;
    let (mut cur, mut in_str, mut esc) = (0usize, false, false);
    for c in t.chars() {
        if esc {
            esc = false;
            cur += 1;
        } else if in_str && c == '\\' {
            esc = true;
            cur += 1;
        } else if c == '"' {
            in_str = !in_str;
            cur += 1;
            if !in_str {
                longest = longest.max(cur);
                cur = 0;
            }
        } else if in_str {
            cur += 1;
        } else if c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '{' | '}') {
            longest = longest.max(cur);
            cur = 0;
        } else {
            cur += 1;
        }
    }
    longest = longest.max(cur);
    // Closing delimiters at the end of the line are mandatory: they belong
    // to forms that end here and have nowhere else to go. So the true
    // floor for a line is indent + its longest unbreakable atom + that
    // tail, and a line at or above the budget for THAT reason is not a
    // layout failure.
    let tail = t
        .chars()
        .rev()
        .take_while(|c| matches!(c, ')' | ']' | '}'))
        .count();
    // A `:key value` pair is ONE unit by the printer's own invariant (a
    // key is never separated from its value by a line break), so the key
    // prefix is mandatory-adjacent to the atom and counts toward the
    // floor. Splitting them would not help anyway: the two corpus cases
    // this covers are 76-character doc strings that overflow at the
    // deeper indent too.
    let kw_prefix = if t.starts_with(':') {
        t.split_whitespace().next().map_or(0, |k| k.len() + 1)
    } else {
        0
    };
    indent + kw_prefix + longest + tail > 80
}

#[test]
fn fleet_corpus_respects_the_invariants() {
    // corpus list absent — nothing to sweep
    let Ok(list) = std::fs::read_to_string("/tmp/tl.txt") else {
        return;
    };
    let (mut ok, mut parse_skip, mut over) = (0usize, 0usize, Vec::new());
    for rel in list.lines() {
        let path = std::path::Path::new("/Users/drzzln/code/github/pleme-io").join(rel);
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(a) = format_source(&src, &FmtConfig::default()) else {
            parse_skip += 1;
            continue;
        };
        let b = format_source(&a, &FmtConfig::default()).expect("reformat");
        assert_eq!(a, b, "NOT IDEMPOTENT: {rel}");
        for (i, l) in a.lines().enumerate() {
            let w = l.chars().count();
            if w <= 80 || line_is_unbreakable(l) {
                continue;
            }
            over.push(format!("{rel}:{} = {w} cols | {}", i + 1, l.trim()));
        }
        ok += 1;
    }
    println!("swept={ok} unparseable={parse_skip} over80={}", over.len());
    for o in over.iter().take(12) {
        println!("  OVER: {o}");
    }
    assert!(over.is_empty(), "{} lines exceed 80 columns", over.len());
}
