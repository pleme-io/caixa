use caixa_fmt::{FmtConfig, format_source};

/// An executable script must survive formatting AS AN EXECUTABLE: the
/// shebang has to remain the first bytes of the file, verbatim. Emitting
/// it as a `;` comment, or dropping it, silently stops the kernel from
/// running the script.
#[test]
fn a_shebang_survives_verbatim_and_stays_first() {
    let src = "#!/usr/bin/env tatara-script\n(define x 1)\n";
    let out = format_source(src, &FmtConfig::default()).unwrap();
    assert!(
        out.starts_with("#!/usr/bin/env tatara-script\n"),
        "shebang lost or moved:\n{out}"
    );
    let again = format_source(&out, &FmtConfig::default()).unwrap();
    assert_eq!(out, again, "not idempotent:\n{out}");
}

/// The escape set must match the canonical reader: an unknown escape
/// yields the character itself. `\|` is a grep alternation the corpus
/// really uses; rejecting it made a runnable file unreadable.
#[test]
fn unknown_escapes_match_the_canonical_reader() {
    let src = r#"(x "Applied\|migration\|up to date")"#;
    let out =
        format_source(src, &FmtConfig::default()).expect("must lex like the canonical reader");
    let again = format_source(&out, &FmtConfig::default()).unwrap();
    assert_eq!(out, again, "not idempotent:\n{out}");
}

/// The five corpus scripts and the one escaped-grep file must now parse.
#[test]
fn the_previously_unparseable_corpus_files_parse() {
    let root = std::path::Path::new("/Users/drzzln/code/github/pleme-io");
    for rel in [
        "hardened-images/tools/check-job-only-gates.tlisp",
        "hardened-images/tools/check-cve-claim-expiry.tlisp",
        "actions/db-migrate/run.tlisp",
        "k8s/clusters/pleme-dev/infrastructure/zot-tunnel/bootstrap-rio-zot-tunnel.tlisp",
        "nix/scripts/tlisp/ci-watch.tlisp",
        "nix/scripts/tlisp/rollout-attach.tlisp",
    ] {
        let Ok(src) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        let out = format_source(&src, &FmtConfig::default())
            .unwrap_or_else(|e| panic!("{rel} still unparseable: {e}"));
        if src.starts_with("#!") {
            assert!(out.starts_with("#!"), "{rel}: shebang lost");
        }
    }
}
