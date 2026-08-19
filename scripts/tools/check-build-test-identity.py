#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""check-build-test-identity.py - compare two generated .c files (one from
`nova build --keep-artifacts`, one from `nova test-build --keep-artifacts`)
for semantic byte-identity.

WHY (see scripts/tools/check-build-test-identity.sh for the full story):
`cmd_build` (nova-cli/src/main.rs) and `test_runner.rs::run_one`
(compiler-codegen) are two independently maintained copies of the same
codegen pipeline. Five times in the project's history one of them silently
skipped a pipeline step the other has (registry 221.1 #304 and its
main.rs-comment predecessors). The only proven way any of the five was
found: diff the generated .c of the SAME source compiled both ways.

WHAT THIS SCRIPT DOES (invoked by the .sh driver, one file pair per call)
    1. Applies a short, explicit KNOWN_EXCEPTIONS list (see below) — the
       only currently-documented legitimate divergence is one dead
       (zero-call-site, on both sides) helper function the test path
       happens to emit and the build path doesn't (compiler-codegen's
       DCE/reachability registration, not a semantic gap; found by window
       p-build304, 2026-08-04, commit ac684356f). Each exception is only
       applied when BOTH sides show zero call sites for it — if a future
       regression ever adds a real call site, the exception refuses to
       apply and the divergence surfaces normally instead of being
       silently swallowed.
    2. Canonicalizes synthetic temp identifiers (`_nv_tmp_309`,
       `_nv_if_12`, ...): compiler-codegen assigns these from a single
       monotonic counter during emission, so ONE extra/missing dead
       function anywhere earlier in the file shifts every subsequent
       number by a constant offset in the file that has it — pure
       cosmetic renumbering, not a semantic difference (documented by the
       same window). Each distinct raw token is replaced, independently
       per file, by `<prefix>_C<n>` in order of first appearance, so a
       pure renumbering collapses to identical text on both sides.
    3. Diffs the two results. Identical -> PASS. Otherwise -> FAIL, with a
       `diff -u -p` excerpt (function-name context from -p, canonical temp
       names in the body) and the deduplicated list of function signatures
       the diff touched.

USAGE
    check-build-test-identity.py <build.c> <test.c>
Exit: 0 identical (modulo whitelisted exceptions), 1 real divergence found,
2 usage/IO error.
"""
import re
import subprocess
import sys
import tempfile
import os

# Поток вердикта — с LF: python на Windows иначе печатает CRLF там, где shell
# печатал LF, и вывод молча расходится с shell-редакцией (правило
# check-guard-honesty, заведено 2026-08-19).
sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

# Each entry: the exact C symbol name to look for, and why it's a legitimate
# divergence. Keep this list SHORT — every entry here is content the tool
# will NOT flag, so a padded list quietly turns the tool into "everything is
# fine". Add an entry only with a documented, understood root cause.
KNOWN_EXCEPTIONS = [
    {
        "symbol": "nova_fn_7runtime7fmt_buf7scratch",
        "reason": (
            "window p-build304 (2026-08-04, commit ac684356f): "
            "test-build emits this helper, build does not; ZERO call "
            "sites in EITHER output (verified: `grep -c` both sides) -> "
            "dead code, a DCE/reachability-registration difference in "
            "compiler-codegen, not a semantic gap. Exception only applies "
            "while both sides stay at zero call sites (checked below on "
            "every run)."
        ),
    },
]

TOKEN_RE = re.compile(r"_nv_[A-Za-z_]+_[0-9]+")


def _decl_and_def_re(symbol):
    # Anchored to the WHOLE line (modulo surrounding whitespace) — a real
    # call site embedded in a larger statement (e.g.
    # `int x = 1 + symbol(1);`) also ends with `);` as a bare suffix, which
    # a naive `line.endswith(");")` check would misclassify as "just the
    # forward declaration" and silently exclude from the call-site count.
    # Anchoring the return-type/`static` prefix through to end-of-line
    # rules that false positive out.
    esc = re.escape(symbol)
    decl = re.compile(r"^\s*[A-Za-z_][A-Za-z0-9_ *]*\b" + esc + r"\s*\([^;{}]*\)\s*;\s*$")
    defn = re.compile(r"^\s*[A-Za-z_][A-Za-z0-9_ *]*\b" + esc + r"\s*\([^;{}]*\)\s*\{\s*$")
    return decl, defn


def count_call_sites(lines, symbol):
    """Lines that call `symbol(` but are not its own forward-decl/def line."""
    decl_re, defn_re = _decl_and_def_re(symbol)
    n = 0
    for line in lines:
        if symbol + "(" not in line:
            continue
        if decl_re.match(line) or defn_re.match(line):
            continue  # forward declaration or definition header
        n += 1
    return n


def strip_function(lines, symbol):
    """Remove the forward declaration line and the full definition block
    (from the `... symbol(...) {` line down to the next line that is
    exactly `}`) for `symbol`. No-op if `symbol` is not present."""
    decl_re, defn_re = _decl_and_def_re(symbol)
    out = []
    i = 0
    n = len(lines)
    removed = 0
    while i < n:
        line = lines[i]
        if decl_re.match(line):
            i += 1
            removed += 1
            continue
        if defn_re.match(line):
            i += 1
            removed += 1
            while i < n and lines[i].rstrip("\n") != "}":
                i += 1
                removed += 1
            if i < n:
                i += 1  # the closing '}' line itself
                removed += 1
            if i < n and lines[i].strip() == "":
                i += 1  # trailing blank separator line
                removed += 1
            continue
        out.append(line)
        i += 1
    return out, removed


def apply_known_exceptions(a_lines, b_lines, log):
    for exc in KNOWN_EXCEPTIONS:
        sym = exc["symbol"]
        a_calls = count_call_sites(a_lines, sym)
        b_calls = count_call_sites(b_lines, sym)
        if a_calls != 0 or b_calls != 0:
            log.append(
                f"[EXCEPTION SKIPPED] {sym}: call sites a={a_calls} "
                f"b={b_calls} (expected 0/0) -> exception NOT applied, "
                f"any divergence here is reported as real"
            )
            continue
        a_lines, removed_a = strip_function(a_lines, sym)
        b_lines, removed_b = strip_function(b_lines, sym)
        if removed_a or removed_b:
            log.append(
                f"[EXCEPTION APPLIED] {sym}: {exc['reason']} "
                f"(stripped {removed_a} line(s) from A side, "
                f"{removed_b} line(s) from B side)"
            )
        else:
            log.append(
                f"[EXCEPTION UNUSED] {sym}: not present in either file "
                f"this run (fine — nothing to strip)"
            )
    return a_lines, b_lines


def canonicalize(lines):
    mapping = {}
    counter = [0]

    def repl(m):
        tok = m.group(0)
        canon = mapping.get(tok)
        if canon is None:
            prefix = re.sub(r"_[0-9]+$", "", tok)
            canon = f"{prefix}_C{counter[0]}"
            mapping[tok] = canon
            counter[0] += 1
        return canon

    text = "".join(lines)
    return TOKEN_RE.sub(repl, text)


def function_names_touched(diff_text):
    names = []
    seen = set()
    for line in diff_text.splitlines():
        if not line.startswith("@@"):
            continue
        m = re.match(r"^@@ -\d+(?:,\d+)? \+\d+(?:,\d+)? @@ ?(.*)$", line)
        if not m:
            continue
        ctx = m.group(1).strip()
        if ctx and ctx not in seen:
            seen.add(ctx)
            names.append(ctx)
    return names


def main(argv):
    if len(argv) != 3:
        print(f"usage: {argv[0]} <build.c> <test.c>", file=sys.stderr)
        return 2
    a_path, b_path = argv[1], argv[2]
    try:
        with open(a_path, encoding="utf-8") as f:
            a_lines = f.readlines()
        with open(b_path, encoding="utf-8") as f:
            b_lines = f.readlines()
    except OSError as e:
        print(f"error: cannot read input: {e}", file=sys.stderr)
        return 2

    log = []
    a_lines, b_lines = apply_known_exceptions(a_lines, b_lines, log)
    for entry in log:
        print(entry)

    a_norm = canonicalize(a_lines)
    b_norm = canonicalize(b_lines)

    if a_norm == b_norm:
        print("IDENTICAL (modulo applied exceptions above, if any)")
        return 0

    # Real divergence: shell out to `diff -u -p` on the canonicalized text
    # so we get GNU diff's function-context heuristic (-p) for free. Temp
    # identifiers in the excerpt are canonical (_nv_tmp_C7, ...), not the
    # original numbers — that's intentional: it isolates STRUCTURAL
    # differences from cosmetic renumbering, at the cost of the excerpt not
    # being directly patchable.
    with tempfile.NamedTemporaryFile(
        "w", suffix=".c", delete=False, encoding="utf-8"
    ) as fa, tempfile.NamedTemporaryFile(
        "w", suffix=".c", delete=False, encoding="utf-8"
    ) as fb:
        fa.write(a_norm)
        fb.write(b_norm)
        fa_path, fb_path = fa.name, fb.name
    try:
        proc = subprocess.run(
            ["diff", "-u", "-p", fa_path, fb_path],
            capture_output=True,
            text=True,
        )
        diff_text = proc.stdout
    finally:
        os.unlink(fa_path)
        os.unlink(fb_path)

    names = function_names_touched(diff_text)
    print("DIVERGENT (real difference after exceptions + canonicalization)")
    print(f"functions touched by the diff ({len(names)}):")
    for n in names:
        print(f"  - {n}")
    print()
    print("first lines of the diff (canonical temp names, not the raw ones):")
    excerpt = diff_text.splitlines()[:60]
    for line in excerpt:
        print(line)
    if len(diff_text.splitlines()) > 60:
        print(f"... ({len(diff_text.splitlines()) - 60} more diff line(s) truncated)")
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
