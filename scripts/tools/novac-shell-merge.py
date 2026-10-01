#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""scripts/tools/novac-shell-merge.py -- one novac shell for every platform.

WHY (2026-10-01, plan 274, novac-gate red in CI). The shell template
novac/src/emit_c/shell.tpl.c is the oracle's emission of
novac/probe/shell_probe.nv. std carries `#cfg(target_os)` (today exactly
`host_style` in std/src/fs/path.nv), so an emission made on Windows is a
WINDOWS shell: on Linux CI it was always "stale", and worse, wrong by
behaviour -- `host_style` returned PathStyle_Windows there.

HOW. novac-regen-shell.sh emits the probe twice, NOVA_TARGET_OS=windows and
=linux, cuts the slots into each, and hands both here. The two are aligned
line by line (difflib): an equal run is kept as is; every differing run
becomes

    #ifdef _WIN32
    <windows lines>
    #else
    <linux lines>
    #endif

so the template no longer depends on the machine that generated it, and the
C compiler picks the platform when novac's output is built.

REFUSALS (exit 4, nothing is written) -- a difference the shell cannot carry
as a plain #ifdef, which goes to the Carina window as a question instead:
  - a novac slot (/*__NOVAC_...__*/) inside a differing run: the slots must
    stay in the common part, novac stamps each exactly once;
  - `main` or `nova_fn_main_impl` inside a differing run: the entry diverged
    between platforms;
  - a differing run that starts inside a macro continuation (the previous
    line ends with a backslash): an #ifdef there would cut the macro.

Usage: novac-shell-merge.py WINDOWS.c LINUX.c OUT.c
Exit: 0 written; 2 bad call; 4 refusal (reason on stderr).
A summary line on stdout names the number of platform runs and their lines.
"""
import difflib
import sys


def read_lines(path):
    with open(path, "r", encoding="utf-8", newline="") as f:
        text = f.read()
    # The oracle emits LF. A CR here means the file was touched by a CRLF
    # checkout or tool -- merging that would bake CRs into one side only.
    if "\r" in text:
        raise ValueError("%s contains CR -- emissions must be LF" % path)
    lines = text.split("\n")
    # keep the line terminator out of the comparison, restore it on write
    if lines and lines[-1] == "":
        lines.pop()
        return lines, True
    return lines, False


GUARDED = ("__NOVAC_", "nova_fn_main_impl", "int main(")


def merge(win, lin):
    """Return (out_lines, runs) or raise ValueError with the refusal."""
    sm = difflib.SequenceMatcher(None, win, lin, autojunk=False)
    out = []
    runs = []
    for tag, i1, i2, j1, j2 in sm.get_opcodes():
        if tag == "equal":
            out.extend(win[i1:i2])
            continue
        a, b = win[i1:i2], lin[j1:j2]
        for side, chunk, base in (("windows", a, i1), ("linux", b, j1)):
            for k, line in enumerate(chunk):
                for g in GUARDED:
                    if g in line:
                        raise ValueError(
                            "%s line %d differs between platforms and carries `%s` -- "
                            "a slot or the entry diverged; ask the Carina window, do not "
                            "merge: %s" % (side, base + k + 1, g, line.strip()))
        prev = out[-1] if out else ""
        if prev.rstrip().endswith("\\"):
            raise ValueError(
                "a differing run starts inside a macro continuation (windows line %d): "
                "an #ifdef there would cut the macro" % (i1 + 1))
        out.append("#ifdef _WIN32")
        out.extend(a)
        out.append("#else")
        out.extend(b)
        out.append("#endif")
        runs.append((i1 + 1, len(a), len(b)))
    return out, runs


def main(argv):
    if len(argv) != 4:
        sys.stderr.write("usage: novac-shell-merge.py WINDOWS.c LINUX.c OUT.c\n")
        return 2
    try:
        win, win_nl = read_lines(argv[1])
        lin, lin_nl = read_lines(argv[2])
        out, runs = merge(win, lin)
    except (OSError, ValueError) as e:
        sys.stderr.write("novac-shell-merge: REFUSED -- %s\n" % e)
        return 4
    with open(argv[3], "w", encoding="utf-8", newline="") as f:
        f.write("\n".join(out) + ("\n" if (win_nl or lin_nl) else ""))
    where = ", ".join("windows line %d (%d/%d lines)" % r for r in runs) or "none"
    print("novac-shell-merge ok: platform runs %d [%s]; %d lines out"
          % (len(runs), where, len(out)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
