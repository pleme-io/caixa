#!/usr/bin/env python3
"""clippy_ratchet — make `cargo clippy` a gate this repo can actually adopt.

WHY THIS EXISTS, AND WHY IT IS NOT JUST `-D warnings`
-----------------------------------------------------
`.github/workflows.disabled/ci.yml` carried `cargo clippy --workspace
--all-targets -- -D warnings` and was parked on 2026-04-22. GitHub only ever
reads `.github/workflows/`, so that gate never ran once. Three months of
commits landed against a 22-crate workspace with nothing checking; when this
gate was wired on 2026-07-27 the workspace carried 805 clippy warnings. (That
figure is a dated measurement, not a live one — `tools/clippy-baseline.txt` is
the current authority, and the whole point of the ratchet is that the number
only ever goes down.)

Restoring `-D warnings` verbatim would produce a job that is red on its first
run and on every run after, which is the same as no gate at all — a permanently
red check gets ignored within a week. The two ways out that DO NOT work:

  * dropping the clippy job          -> the defect we are removing
  * `continue-on-error: true`        -> a gate that reports nothing, i.e. the
                                        exact 96-day failure mode, re-created

So: a baseline ratchet. Every warning present today is enumerated in
`tools/clippy-baseline.txt` as accepted debt and does not fail the build. A
warning that is NEW — a lint firing in a file where it did not fire before, or
firing MORE times in a file than before — fails. The gate is green on day one,
it can only ever shrink, and it is honest about what it is not checking.

WHAT IS NOT BASELINED, EVER
---------------------------
Diagnostics at level `error` — clippy's deny-by-default `correctness` group,
and any hard rustc error — always fail, and are never written to the baseline.
They cannot be: a deny-level lint ABORTS the crate, so clippy stops emitting
diagnostics for everything downstream of it. That is not hypothetical here.
`clippy::approx_constant` in `caixa-ast/src/lexer.rs` was doing exactly that;
until it was fixed at source (commit that added this file), `cargo clippy` on
this workspace could not complete, and the 746 warnings it managed to print
before dying were an undercount of the real 805.

THE BASELINE KEY, AND ITS ONE HONEST LIMITATION
-----------------------------------------------
A key is `(lint, file)` with a COUNT — deliberately NOT `file:line:col`.
Line/column keys look more precise and are a trap: inserting a line at the top
of a file shifts every warning below it, so an unrelated edit reports the whole
file as new violations, the gate cries wolf, and someone turns it off. `(lint,
file, count)` is invariant under line motion and still strictly monotonic.

The limitation, stated plainly rather than papered over: within one file, if
you FIX one instance of a lint and INTRODUCE a different instance of the SAME
lint in the SAME file in the SAME commit, the count is unchanged and the gate
stays green. That is a real blind spot, accepted knowingly — it is the price of
not having a line-shift false-positive storm, and the gate still catches every
new lint kind, every new file, and every net increase.

USAGE
    python3 tools/clippy_ratchet.py                     # gate (exit 1 on new)
    python3 tools/clippy_ratchet.py --update-baseline   # accept current as debt

Exit 0 clean, 1 on new violations or any error-level diagnostic.
"""
from __future__ import annotations

import argparse
import collections
import json
import pathlib
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parent.parent
BASELINE = REPO / "tools" / "clippy-baseline.txt"

CLIPPY_CMD = [
    "cargo", "clippy",
    "--workspace", "--all-targets", "--all-features",
    "--message-format=json",
]


def in_workspace(file_name: str) -> bool:
    """Only this repo's own sources are ratcheted.

    Diagnostics from git/registry dependencies (tatara, iac-forge, gen, sui)
    are not ours to fix and would churn the baseline every time an upstream
    pin moves. cargo reports workspace files as paths relative to the
    workspace root; dependency files come through as absolute store/registry
    paths.
    """
    if file_name.startswith("/"):
        return False
    return not any(
        seg in file_name for seg in ("registry/src", "git/checkouts", "target/")
    )


def run_clippy() -> tuple[list[dict], int]:
    """Invoke clippy and return (diagnostics, returncode).

    stdout goes to a real file rather than through a pipe: a pipeline's exit
    status is the RIGHT-hand command's, and reading the left side's status
    wrong is precisely how a gate ends up reporting nothing. subprocess with
    an explicit file handle keeps `returncode` unambiguous.
    """
    with tempfile.TemporaryFile("w+") as out:
        proc = subprocess.run(CLIPPY_CMD, cwd=REPO, stdout=out, text=True, check=False)
        out.seek(0)
        raw = out.read()

    diags: list[dict] = []
    for line in raw.splitlines():
        try:
            m = json.loads(line)
        except json.JSONDecodeError:
            continue
        if m.get("reason") != "compiler-message":
            continue
        msg = m.get("message") or {}
        if msg.get("level") not in ("warning", "error"):
            continue
        primary = [s for s in msg.get("spans", []) if s.get("is_primary")]
        if not primary:
            # Trailing "N warnings emitted" summaries carry no span. They are
            # counts of the diagnostics above, not diagnostics themselves.
            continue
        if not in_workspace(primary[0]["file_name"]):
            continue
        code = (msg.get("code") or {}).get("code") or "(uncoded)"
        diags.append({
            "level": msg["level"],
            "code": code,
            "file": primary[0]["file_name"],
            "line": primary[0]["line_start"],
            "text": msg.get("message", ""),
        })
    return diags, proc.returncode


def tally(diags: list[dict]) -> collections.Counter:
    """Count unique (lint, file) pairs.

    Deduped on (file, line, code, text) first: every source line is checked
    twice under --all-targets (once for the lib, once for the lib's test
    harness), so a raw count double-reports.
    """
    seen: set[tuple] = set()
    counts: collections.Counter = collections.Counter()
    for d in diags:
        ident = (d["file"], d["line"], d["code"], d["text"])
        if ident in seen:
            continue
        seen.add(ident)
        counts[(d["code"], d["file"])] += 1
    return counts


def read_baseline() -> collections.Counter:
    counts: collections.Counter = collections.Counter()
    if not BASELINE.exists():
        return counts
    for line in BASELINE.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        n, lint, file_name = line.split("\t", 2)
        counts[(lint, file_name)] = int(n)
    return counts


HEADER = """\
# clippy debt, enumerated. Generated by `tools/clippy_ratchet.py
# --update-baseline`; never hand-edited.
#
# Format: <count>\\t<lint>\\t<file>. Each row is REAL, TRACKED debt that
# predates the gate being wired (the ci.yml carrying `-D warnings` sat in
# .github/workflows.disabled for 96 days, where GitHub never reads it, so
# none of this was ever caught).
#
# The goal is an empty file. A row shrinks only by actually fixing the
# warnings, never by relaxing a lint level to make it fit. Rows that drop to
# zero are reported by the gate and must be removed with --update-baseline,
# so the ratchet cannot silently re-permit fixed debt.
"""


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--update-baseline", action="store_true",
                    help="record current warnings as accepted debt")
    args = ap.parse_args()

    diags, rc = run_clippy()

    errors = [d for d in diags if d["level"] == "error"]
    if errors:
        # Deny-level lints abort the crate and blind the gate to everything
        # downstream, so they are fixed at source and never baselined.
        print(f"clippy_ratchet: {len(errors)} ERROR-level diagnostic(s) — "
              f"these are never baselined, fix at source:\n")
        for e in errors:
            print(f"  - {e['file']}:{e['line']}: [{e['code']}] {e['text']}")
        return 1

    if rc != 0:
        print(f"clippy_ratchet: cargo clippy exited {rc} with no error-level "
              f"diagnostic — the build itself failed. Not a lint problem.")
        return 1

    found = tally(diags)

    if args.update_baseline:
        rows = "".join(
            f"{n}\t{lint}\t{file_name}\n"
            for (lint, file_name), n in sorted(found.items(), key=lambda kv: (kv[0][1], kv[0][0]))
        )
        BASELINE.write_text(HEADER + rows)
        print(f"clippy_ratchet: baseline updated — {sum(found.values())} warning(s) "
              f"across {len(found)} (lint, file) pair(s) accepted as debt")
        return 0

    known = read_baseline()

    new: list[str] = []
    for pair, n in sorted(found.items()):
        lint, file_name = pair
        was = known.get(pair, 0)
        if n > was:
            new.append(f"{file_name}: [{lint}] {n} occurrence(s), baseline allows {was}")

    shrunk = [
        (pair, known[pair], found.get(pair, 0))
        for pair in known
        if found.get(pair, 0) < known[pair]
    ]
    if shrunk:
        print(f"clippy_ratchet: {len(shrunk)} baselined row(s) shrank — "
              f"run --update-baseline to lock the win in:")
        for (lint, file_name), was, now in sorted(shrunk):
            print(f"    fixed: {file_name} [{lint}] {was} -> {now}")

    total = sum(found.values())
    if not new:
        print(f"clippy_ratchet: clean — {total} warning(s) across "
              f"{len(found)} (lint, file) pair(s), all baselined as debt")
        return 0

    print(f"\nclippy_ratchet: {len(new)} NEW violation(s); "
          f"{total} total warning(s), the rest are baselined\n")
    for x in new:
        print(f"  - {x}")
    print("\nFix them, or if they are genuinely pre-existing debt you just "
          "uncovered, run:\n    python3 tools/clippy_ratchet.py --update-baseline")
    return 1


if __name__ == "__main__":
    sys.exit(main())
