# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-no-prelude-shadow.py — novac не объявляет имя,
которое экспортирует прелюдия (П5-семейство).

ПРАВИЛО (перенесено из shell-редакции слово в слово, 2026-08-19): прелюдия
импортируется в каждый файл, поэтому своя декларация с тем же именем ТЕНИТ её и
читается как прелюдная. Собираются экспортированные имена прелюдии
(`export type X`, включая `X[T]`, и СВОБОДНЫЕ `export fn name(` — метод
`Type @name(` и `Type.name(` этим не ловятся и не должны), затем в novac/src
ищутся одноимённые декларации любого вида.

ПОЧЕМУ PYTHON: shell-редакция поднимала awk на КАЖДЫЙ файл дважды (прелюдия и
дерево) — 2.2с там, где работы на доли секунды (П14).

$1 — корень; $2 — override novac/src; $3 — override каталога прелюдии.
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-no-prelude-shadow"


def decl_ident(line, kw):
    """Имя, объявленное строкой `[export] <kw> <имя>`, и ХВОСТ после имени."""
    m = re.match(r"^(export[ \t]+)?" + kw + r"[ \t]+", line)
    if not m:
        return "", ""
    rest = line[m.end():]
    m2 = re.match(r"[A-Za-z_][A-Za-z0-9_]*", rest)
    if not m2:
        return "", ""
    return m2.group(0), rest[m2.end():]


def free_fn(tail):
    """Свободная функция: имя вплотную к `(` (или к generic-голове `[...](`)."""
    if tail.startswith("["):
        i = tail.find("]")
        if i < 0:
            return False
        tail = tail[i + 1:]
    return tail.startswith("(")


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"
    prelude = pathlib.Path(a[3]) if len(a) > 3 else root / "std" / "src" / "prelude"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0
    if not prelude.is_dir():
        print(f"{NAME} ok: судить нечего (нет {prelude})")
        return 0

    # --- имена прелюдии ---------------------------------------------------
    pre = {}
    for p in sorted(prelude.glob("*.nv")):
        if p.name.endswith("_test.nv"):
            continue
        rel = "prelude/" + p.name
        for i, raw in enumerate(p.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"), 1):
            if re.match(r"^export[ \t]+type[ \t]", raw):
                n, _ = decl_ident(raw, "type")
                if n:
                    pre.setdefault(n, ("type", f"{rel}:{i}"))
                continue
            if re.match(r"^export[ \t]+fn[ \t]", raw):
                n, tail = decl_ident(raw, "fn")
                if n and free_fn(tail):
                    pre.setdefault(n, ("fn", f"{rel}:{i}"))

    # --- тени в novac ------------------------------------------------------
    judged = [p for p in sorted(src.rglob("*.nv")) if not p.name.endswith("_test.nv")]
    bad = []
    for p in judged:
        rel = p.relative_to(src).as_posix()
        for i, raw in enumerate(p.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"), 1):
            if re.match(r"^(export[ \t]+)?type[ \t]", raw):
                n, _ = decl_ident(raw, "type")
                if n and n in pre:
                    k, w = pre[n]
                    bad.append(f"  {rel}:{i}: type {n} тенит прелюдный {k} {n} ({w})")
                continue
            if re.match(r"^(export[ \t]+)?fn[ \t]", raw):
                n, tail = decl_ident(raw, "fn")
                if n and free_fn(tail) and n in pre:
                    k, w = pre[n]
                    bad.append(f"  {rel}:{i}: fn {n} тенит прелюдный {k} {n} ({w})")

    if bad:
        print(f"{NAME}: FAIL — novac тенит имена прелюдии (прелюдия импортируется всюду):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  чинить: переименовать декларацию в novac (имя по роли в компиляторе,", file=sys.stderr)
        print("  напр. Outcome -> CompileOutcome/StepResult), либо — если нужен именно", file=sys.stderr)
        print("  прелюдный смысл — удалить свою декларацию и пользоваться прелюдной.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: имён прелюдии: {len(pre)}, файлов novac/src: {len(judged)}, теней: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
