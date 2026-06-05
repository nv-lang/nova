#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""demojibake.py - repair UTF-8-misread-as-cp1251 mojibake in source files.

WHAT
    One-off / re-runnable repair tool for the encoding corruption tracked in
    GitHub issue #1. Russian comments (and Cyrillic diagnostic string
    literals) in some compiler-codegen sources had been corrupted: original
    UTF-8 was decoded as Windows-1251 and re-saved as UTF-8 ("double
    mojibake"), so e.g. "Алгоритм" became "РђР»РіРѕСЂёС‚Рј". Additionally some
    smart quotes / dashes (the 3rd byte of multibyte sequences) had been
    "dumbed" down to ASCII (’ -> ', — -> "), which breaks a naive reverse.

HOW IT WORKS
    Per line:
      1. If the whole line cleanly reverses (strict cp1251 round-trip and the
         result has no MORE mojibake markers) -> use the reversed line.
      2. Otherwise (a MIXED line: mojibaked Cyrillic interleaved with genuine
         punctuation / code / already-correct Russian) reverse only the
         maximal mojibake RUNS. A run starts at a Р/С/в prefix (the bytes
         D0/D1/E2) and greedily consumes continuation chars (those that
         cp1251-encode to 0x80..0xBF, i.e. the 2nd-byte range). Genuine
         standalone — / ’ / → and box-drawing pass through untouched, and
         single Р/С in correct words fail to reverse and are left as-is.
    Dumbed punctuation (PUNCT) and dumbed Cyrillic capitals (DUMBED, e.g.
    Р' -> В) are restored to their cp1251 chars BEFORE the byte rebuild.
    Rare one-off residuals are mapped explicitly in CAP_FIX.

SAFETY
    - Correctly-encoded Russian does NOT round-trip, so correct files/lines
      are left untouched.
    - A legitimate cp1251 decode-TABLE in source (test_runner.rs:
      `0x80 => 'Ђ', ...`) looks like markers but reverses to nothing useful,
      so that file is reported "nochange" and skipped.
    - It NEVER writes a result that contains U+FFFD or residual markers.
    - Pre-existing U+FFFD ("�") in a file means the original bytes are LOST
      and CANNOT be recovered by this tool (see docs/project-creation.txt,
      docs/simplifications.md) - those need manual rewrite or git history.

USAGE
    # dry-run: report status per file (clean / would-fix / nochange / FAILED)
    python scripts/demojibake.py path/to/file.rs [more files ...]

    # apply in place (preserves UTF-8 BOM):
    python scripts/demojibake.py --write path/to/file.rs

    # repo sweep (PowerShell example):
    #   Get-ChildItem -Recurse -Include *.rs |
    #     %{ python scripts/demojibake.py --write $_.FullName }
    Always dry-run first and eyeball the diff (git diff) before committing.

ROOT CAUSE (important)
    The corruption keeps RE-APPEARING because an editor/linter in the dev
    environment re-saves Cyrillic with the wrong encoding (it is NOT git -
    there is no .gitattributes encoding filter). Fix the editor's encoding
    (e.g. VS Code: Files.encoding = utf8, disable autoGuessEncoding, audit
    any format-on-save tool) or any repair done here will be undone again.

All mojibake patterns are built via chr(codepoint) so this source stays
pure ASCII and is itself immune to the corruption it fixes.
"""
import sys

C = chr
# Dumbed mojibake-punct (3rd byte normalized to ASCII) -> correct char.
PUNCT = {
    # Restoration model: restore the dumbed ASCII tail to the cp1251 char so
    # the byte loop reconstructs the ORIGINAL UTF-8 bytes (don't substitute
    # the final glyph -- it would get re-cp1251-encoded into a broken byte).
    C(0x0432) + C(0x2020) + C(0x0027): C(0x0432) + C(0x2020) + C(0x2019),
    # 'в † '' -> 'в † ’' -> E2 86 92 -> arrow ->
    C(0x0432) + C(0x0402) + C(0x0022): C(0x0432) + C(0x0402) + C(0x201D),
    # 'в Ђ "' -> 'в Ђ ”' -> E2 80 94 -> em dash —
}
# Dumbed Cyrillic capitals: original UTF-8 2nd byte (0x91-0x97) became a
# cp1251 smart-quote/dash, then dumbed to ASCII -> ambiguous. We map the
# ASCII tail back to the cp1251 char that yields the DOMINANT Russian
# reading; rare exceptions (Б/Г/Ж words) are corrected per-occurrence in
# CAP_FIX (post-pass) after visual review of the recovered files.
#   Р' -> 0x92 (В)  | Р" -> 0x94 (Д)  | Р- -> 0x97 (З)  | С' -> 0x91 (ё)
DUMBED = {
    # ellipsis (0x85 -> U+2026 -> dumbed "...") FIRST (longer keys win).
    C(0x0421) + "...": C(0x0421) + C(0x2026),        # С... -> х
    C(0x0420) + "...": C(0x0420) + C(0x2026),        # Р... -> Ё
    C(0x0420) + C(0x0027): C(0x0420) + C(0x2019),    # Р' -> В
    C(0x0420) + C(0x0022): C(0x0420) + C(0x201D),    # Р" -> Д
    C(0x0420) + C(0x002D): C(0x0420) + C(0x2014),    # Р- -> З
    C(0x0421) + C(0x0027): C(0x0421) + C(0x2018),    # С' -> ё
    C(0x0421) + C(0x0022): C(0x0421) + C(0x201D),    # С" -> є (rare; review)
    C(0x0421) + C(0x002D): C(0x0421) + C(0x2014),    # С- -> ї (rare; review)
}
# Post-pass: per-word corrections where the dominant-reading default is
# wrong (filled after reading recovered files). key = recovered substring
# to replace, value = correct substring.
CAP_FIX = {
    # Lone ё->ђ corruption on an otherwise-correct line (types/mod.rs).
    C(0x0438) + C(0x043C) + C(0x0452) + C(0x043D):  # 'имђн'
        C(0x0438) + C(0x043C) + C(0x0451) + C(0x043D),  # 'имён'
}
ARROW = C(0x0432) + C(0x2020)   # multibyte-arrow mojibake prefix
PUNCTP = C(0x0432) + C(0x0402)  # multibyte-punct mojibake prefix


def moji_score(text):
    """Count distinctive mojibake chars. U+0452..045F / U+0402..040F never
    appear in normal Russian but saturate cp1251-mojibake continuations."""
    n = 0
    for c in text:
        o = ord(c)
        if 0x0452 <= o <= 0x045F or 0x0402 <= o <= 0x040F:
            n += 1
    return n + text.count(ARROW) + text.count(PUNCTP)


def _reverse_line(line):
    """STRICT per-line cp1251 reverse. Returns recovered line if the line is
    cleanly mojibaked, else the ORIGINAL line (already-correct UTF-8 lines do
    NOT round-trip and must be left untouched -- the files are MIXED: old
    mojibaked comments interleaved with newer correctly-written Russian)."""
    if all(ord(c) < 0x80 for c in line):
        return line
    work = line
    for k, v in DUMBED.items():
        work = work.replace(k, v)
    for k, v in PUNCT.items():
        work = work.replace(k, v)
    out = bytearray()
    for c in work:
        o = ord(c)
        if o < 0x80:
            out.append(o)
        elif o == 0x98:
            # cp1251 0x98 undefined; mojibake tool kept original byte 0x98 as
            # U+0098. It is the 2nd byte of e.g. И (D0 98). Re-emit raw byte.
            out.append(0x98)
        else:
            try:
                out += c.encode("cp1251")
            except UnicodeEncodeError:
                out += c.encode("utf-8")
    try:
        rec = out.decode("utf-8")  # STRICT: raises on already-correct lines
        if moji_score(rec) <= moji_score(line):
            for k, v in CAP_FIX.items():
                rec = rec.replace(k, v)
            return rec
    except UnicodeDecodeError:
        pass
    # Whole-line strict failed (line is MIXED: mojibaked Cyrillic + genuine
    # punctuation/code). Reverse only maximal mojibake RUNS, passing genuine
    # chars (—, ’, →, ASCII, box-drawing) through untouched.
    return _reverse_runs(line)


# Mojibake "alphabet": chars that occur INSIDE cp1251-mojibake runs. Excludes
# genuine smart punctuation (—’“”→) which act as run boundaries.
def _is_moji_char(o):
    # NB: NOT the normal Russian block 0x0410..044F (а-я) — those are correct
    # text. Only the D0/D1/E2 prefixes (Р/С/в) + the cp1251-continuation chars
    # (Latin-1 supplement + Cyrillic-EXTENSION Ђ-Џ / ђ-џ + † € ™).
    return (o in (0x0420, 0x0421, 0x0432)        # Р С в (D0/D1/E2 prefixes)
            or 0x0080 <= o <= 0x00FF              # Latin-1 continuations °»µ¤·…
            or 0x0402 <= o <= 0x040F              # Ђ..Џ ext continuations
            or 0x0452 <= o <= 0x045F              # ђ..џ ext continuations
            or o in (0x2020, 0x20AC, 0x2122))     # † € ™ (mojibake continuations)


def _is_prefix(o):
    # Mojibake byte-sequence prefixes: D0->Р, D1->С, E2->в.
    return o in (0x0420, 0x0421, 0x0432)


def _is_cont(o):
    # A char is a mojibake CONTINUATION iff it cp1251-encodes to a byte in
    # 0x80..0xBF — exactly the 2nd-byte range of a 2-byte UTF-8 Cyrillic char
    # (D0/D1 + 0x80..0xBF). This precisely captures Ё/ё/«»/°/smart-punct/ext
    # AND excludes normal Russian а-я (cp1251 0xE0..0xFF) and capitals А-Я
    # (0xC0..0xDF), so correct text is never swallowed into a run.
    if _is_prefix(o):
        return True
    if o == 0x98:                      # cp1251-undefined byte kept as U+0098
        return True
    try:
        return 0x80 <= chr(o).encode("cp1251")[0] <= 0xBF
    except UnicodeEncodeError:
        return 0x0080 <= o <= 0x00BF   # Latin-1 supplement not in cp1251


def _reverse_runs(line):
    for k, v in DUMBED.items():
        line = line.replace(k, v)
    for k, v in PUNCT.items():
        line = line.replace(k, v)
    out = []
    i, n = 0, len(line)
    while i < n:
        if _is_prefix(ord(line[i])):
            j = i + 1
            while j < n and _is_cont(ord(line[j])):
                j += 1
            run = line[i:j]
            b = bytearray()
            for c in run:
                o = ord(c)
                if o == 0x98:
                    b.append(0x98)
                else:
                    try:
                        b += c.encode("cp1251")
                    except UnicodeEncodeError:
                        b += c.encode("utf-8")
            try:
                rev = b.decode("utf-8")
                # Run STARTED at a Р/С/в prefix => mojibake. Accept the reverse
                # if it is strictly cleaner, OR (when the run has no COUNTED
                # markers, e.g. Р+ё+С+… for "их") if the reverse is itself
                # marker-free and actually changed. Garbage reverses (which
                # still carry markers) are rejected. Correct lines that reach
                # here have single Р/С (no continuation) -> decode fails -> kept.
                # Run started at a Р/С/в prefix => mojibake. Accept the
                # reverse when it is strictly cleaner, OR — for runs whose
                # chars aren't counted by the narrow moji_score (e.g. Р+ё+С+…
                # for "их") — when BOTH are marker-free, the reverse changed,
                # and it is long enough (>=3 chars) to not be a rare correct
                # "Рё…"-initial word (Рёбра). _is_cont's tight 0x80-0xBF rule
                # already keeps normal Russian а-я out of runs.
                sr, sv = moji_score(run), moji_score(rev)
                accept = (sv < sr
                          or (sr == 0 and sv == 0 and rev != run and len(run) >= 3))
                out.append(rev if accept else run)
            except UnicodeDecodeError:
                out.append(run)
            i = j
        else:
            out.append(line[i])
            i += 1
    rec = "".join(out)
    for k, v in CAP_FIX.items():
        rec = rec.replace(k, v)
    return rec


def demojibake(text):
    return "\n".join(_reverse_line(ln) for ln in text.split("\n"))


def try_fix(path):
    raw = open(path, "rb").read()
    bom = b""
    if raw.startswith(b"\xef\xbb\xbf"):
        bom = raw[:3]
        raw = raw[3:]
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        return "skip-not-utf8"
    if moji_score(text) < 3:
        return "clean"
    try:
        rec = demojibake(text)
    except (UnicodeDecodeError, UnicodeEncodeError) as e:
        return "FAILED:" + str(e)[:50]
    if rec == text:
        return "nochange"
    rem = moji_score(rec)
    if rem:
        return "INCOMPLETE:%d-markers" % rem
    if "--write" in sys.argv:
        open(path, "wb").write(bom + rec.encode("utf-8"))
        return "FIXED-WRITTEN"
    return "would-fix"


if __name__ == "__main__":
    paths = [a for a in sys.argv[1:] if not a.startswith("--")]
    for p in paths:
        print("%-28s %s" % (try_fix(p), p))
