#!/usr/bin/env python3
"""check-error-rewrap-chain.py -- an error value in a message is formatted with `{:#}`.

Registry 221.1 #1592 (docs/plans/221.1-bug-sweep.md).
`anyhow!("dependency resolution: {}", e)` keeps only the TOP message of `e`: when `e` carries a chain of causes (an `anyhow::Error`, a `.context(...)` wrapper),
`{}` prints its outermost context and drops the rest, `{:#}` prints the whole chain
`a: b: c`. Measured: on a deep NOVA_HOME on Windows git answered "Filename too long",
`run_git` put that in the error, and `nova build` printed only "checkout commit ... of a
git dependency" -- two re-wraps and the top-level printer all used `{}`.

THE RULE (one form, so nobody has to know the value's type): in `anyhow!` / `bail!` /
`eprintln!` / `format!`, a placeholder bound to an error value -- an argument named `e`,
`err`, `error`, `*_err`, `*_error`, optionally `.to_string()` -- is `{:#}` (or Debug
`{:?}`/`{:#?}`). For a value without causes `{:#}` prints exactly what `{}` does, so the
rule costs nothing where it is not needed. `format!` is judged everywhere, not only where
it visibly builds an error: a first version judged it only after `map_err(`/`Err(` on its
line and missed 21 places that put the message in a field (`error: format!(...)`,
`Stage::Cc { error: ... }`, `errors.push(format!(...))`), six of them over an
`anyhow::Error` -- the guessing cost more than the rule.

THE SECOND FORM, found by the probe after the first was fixed: `e.to_string()` is `{}` without
a macro. `GitProvider::available_versions` returned `list_versions(url).map_err(|e|
e.to_string())`, so the chain was cut there even with every `{}` turned into `{:#}`; the
probe still printed only "clone git-dependency `...`". An error value turned into a string is
`format!("{:#}", e)`. A line holding `assert` is not judged (a test reads the top message).

DOES NOT JUDGE: a value under another name (that is a reviewer's eye, not a regex);
`.context()`/`.with_context()` chains (they keep the cause; the printer then needs
`{:#}`, which this rule holds); comments.

Usage: check-error-rewrap-chain.py ROOT   (exit 1 with every offending place)
"""
import os
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

DIRS = ["nova-cli/src", "compiler-codegen/src"]
MACROS = re.compile(r"\b(anyhow!|bail!|eprintln!|format!)\s*\(")
ERRNAME = re.compile(r"^(e|err|error|[a-z0-9_]*_err|[a-z0-9_]*_error)(\.to_string\(\))?$")
PH = re.compile(r"\{\{|\}\}|\{([^{}]*)\}")
TO_STRING = re.compile(r"\b(e|err|error|[a-z0-9_]*_err|[a-z0-9_]*_error)\.to_string\(\)")


def call_text(src, start):
    """The macro's argument text from its `(` to the matching `)`, strings respected."""
    depth, i, n = 0, start, len(src)
    while i < n:
        c = src[i]
        if c == '"':
            i += 1
            while i < n and src[i] != '"':
                i += 2 if src[i] == "\\" else 1
        elif c == "'" and i + 2 < n and (src[i + 2] == "'" or src[i + 1] == "\\"):
            i = src.index("'", i + 2 if src[i + 1] != "\\" else i + 3)
        elif c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
            if depth == 0:
                return src[start + 1:i]
        i += 1
    return None


def split_args(s):
    out, depth, buf, i = [], 0, "", 0
    while i < len(s):
        c = s[i]
        if c == '"':
            j = i + 1
            while j < len(s) and s[j] != '"':
                j += 2 if s[j] == "\\" else 1
            buf += s[i:j + 1]
            i = j + 1
            continue
        if c in "([{|":
            depth += 1 if c != "|" else 0
        if c in ")]}":
            depth -= 1
        if c == "," and depth == 0:
            out.append(buf.strip())
            buf = ""
        else:
            buf += c
        i += 1
    if buf.strip():
        out.append(buf.strip())
    return out


def offenders(text):
    """(offset, placeholder, argument) for every error value printed without `{:#}`."""
    found = []
    for m in MACROS.finditer(text):
        line_start = text.rfind("\n", 0, m.start()) + 1
        before = text[line_start:m.start()]
        if before.lstrip().startswith("//"):
            continue
        body = call_text(text, m.end() - 1)
        if body is None:
            continue
        lit = re.match(r'\s*"((?:[^"\\]|\\.)*)"', body, re.S)
        if not lit:
            continue
        args = split_args(body[lit.end():].lstrip().lstrip(","))
        named = {}
        for a in args:
            k = re.match(r"^([a-z_][a-z0-9_]*)\s*=\s*(.+)$", a, re.S)
            if k:
                named[k.group(1)] = k.group(2).strip()
        positional = [a for a in args if not re.match(r"^[a-z_][a-z0-9_]*\s*=[^=]", a)]
        nxt = 0
        for ph in PH.finditer(lit.group(1)):
            spec = ph.group(1)
            if spec is None:
                continue
            name, _, fmt = spec.partition(":")
            if name == "":
                arg = positional[nxt] if nxt < len(positional) else None
                nxt += 1
            elif name.isdigit():
                arg = positional[int(name)] if int(name) < len(positional) else None
            else:
                arg = named.get(name, name)
            if arg is None or not ERRNAME.match(arg):
                continue
            if "#" in fmt or "?" in fmt:
                continue
            found.append((m.start(), "{" + spec + "}", arg, m.end() + lit.start(1) + ph.start()))
    return found


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    bad = []
    for d in DIRS:
        for dp, _, files in os.walk(os.path.join(root, d)):
            for f in files:
                if not f.endswith(".rs"):
                    continue
                p = os.path.join(dp, f)
                text = open(p, encoding="utf-8").read()
                rel = os.path.relpath(p, root).replace("\\", "/")
                for off, ph, arg, _ in offenders(text):
                    line = text.count("\n", 0, off) + 1
                    bad.append(f"{rel}:{line}: `{ph}` prints only the top of `{arg}` -- write `{{:#}}`")
                for n, ln in enumerate(text.split("\n"), 1):
                    if ln.lstrip().startswith("//") or "assert" in ln:
                        continue
                    for m in TO_STRING.finditer(ln):
                        bad.append(f"{rel}:{n}: `{m.group(0)}` keeps only the top of `{m.group(1)}` "
                                   f"-- write `format!(\"{{:#}}\", {m.group(1)})`")
    if bad:
        for b in bad:
            print(b, file=sys.stderr)
        print(f"check-error-rewrap-chain FAIL: {len(bad)} place(s) (#1592)", file=sys.stderr)
        sys.exit(1)
    print("check-error-rewrap-chain ok: every error value in a message is formatted with {:#}")


if __name__ == "__main__":
    main()
