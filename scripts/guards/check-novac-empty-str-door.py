# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-empty-str-door.py — пустота строки спрашивается
сравнением с `""`, а не длиной её представления (конвенция П37).
План: docs/plans/274-novac-self-hosted-compiler.md (ярус novac, волна M2b); правило — docs/dev/conventions.md П37; носители — реестр docs/plans/221.1-bug-sweep.md.

ПОЧЕМУ ЭТОТ СТРАЖ ПОЯВИЛСЯ (владелец, 2026-09-06, глядя на `lower/ir.nv`:
«пустоту строки нужно проверять сравнением с "", а не длиной байтового
представления — в конвенцию + страж + линтер»). У строки ТРИ длины (D249:
байты, кодовые точки, ширина), и `str.len()` снят именно поэтому; но вопрос
«пуста ли строка» — не вопрос о длине вообще: `s.bytes().len() == 0` и
`s.byte_len() == 0` отвечают на «сколько байтов», совпадая с пустотой лишь
случайно, и учат следующего читателя выбирать линзу там, где линза не нужна.
`s == ""` говорит ровно то, что имеется в виду. В день заведения носителей было
пять, все в коде компилятора (ir.nv, calls.nv, emit_expr.nv, emit_interp.nv,
slots.nv) — сняты тем же слиянием, поэтому это ЗАПРЕТ, не храповик: база 0.

ЧТО ЛОВИТ: строку кода (комментарии сняты) в `novac/src/**/*.nv`, включая
тесты, где `.bytes().len()` или `.byte_len()` стоит по любую сторону сравнения
(`==`, `!=`, `<`, `>`, `<=`, `>=`) с литералом `0`.
ЧЕГО НЕ ЛОВИТ: длину, спрошенную ради длины (`"${s.bytes().len()}"` в
мэнглинге — префикс длины по D134), сравнения с ненулевыми числами, пустоту
других коллекций (`v.len() == 0` у вектора — вопрос о векторе, не о строке).
Линт оракула `W_STR_EMPTY_BY_LEN` держит то же правило для всего дерева; этот
страж — для novac, потому что гейт novac стоит секунды и краснит раньше.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень репозитория; $2 — override каталога novac/src (шов самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-empty-str-door"
# `[.@]`: the lens on a named receiver (`s.byte_len()`) AND on the implicit one
# (`@byte_len()` inside a `str` method) -- the oracle lint found the second form
# in std on 2026-09-06 after the first version here saw only the dotted one.
LEN = r"(?:[.@]bytes\(\)\.len\(\)|[.@]byte_len\(\))"
CMP = r"(?:==|!=|<=|>=|<|>)"
RE_LEFT = re.compile(LEN + r"\s*" + CMP + r"\s*0\b")
RE_RIGHT = re.compile(r"\b0\s*" + CMP + r"\s*(?:[A-Za-z_@][\w.@\[\]()]*)?" + LEN)


def code_part(line):
    s = line.strip()
    if s.startswith("//"):
        return ""
    return line.split("//")[0]


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"
    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0
    files = sorted(src.rglob("*.nv"))
    if not files:
        print(f"{NAME} ok: судить нечего (в {src} файлов .nv: 0)")
        return 0
    bad = []
    for p in files:
        rel = p.relative_to(src).as_posix()
        for n, line in enumerate(p.read_text(encoding="utf-8", errors="replace").split("\n"), 1):
            code = code_part(line)
            if code and (RE_LEFT.search(code) or RE_RIGHT.search(code)):
                bad.append(f"  {rel}:{n} — {line.strip()[:110]}")
    if bad:
        print(f"{NAME}: FAIL — пустота строки спрошена длиной представления: {len(bad)} (П37)", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print('  Канон — сравнение с пустой строкой: `s == ""` / `s != ""`. Длина байтов — ответ', file=sys.stderr)
        print("  на другой вопрос (D249: у строки три длины); спрашивай её только ради длины.", file=sys.stderr)
        return 1
    print(f"{NAME} ok: файлов .nv: {len(files)}, пустот через длину представления: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
