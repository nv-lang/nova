# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-no-name-hardcode.py — имя языка или std не пишется
строкой вне `builtins.nv` (П5).

ПОЧЕМУ. Имя, размазанное строковыми литералами по коду, нельзя ни переименовать,
ни найти: реестр остатка П5 существует ровно для того, чтобы список имён был
ОДИН. Здесь — константа или дверь, имя — там.

СПИСОК ИМЁН БЕРЁТСЯ ИЗ ДАННЫХ, а не зашит: все литералы-идентификаторы из
`builtins.nv` плюс поверхность прелюдии (она движется только со спекой).

КОММЕНТАРИЙ В `builtins.nv` СРЕЗАЕТСЯ ПОСИМВОЛЬНО, а не регекспом: `//` внутри
строкового литерала (`"http://..."`) резал строку и прятал за собой всё
остальное, включая нарушение (274.3/F14).

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-no-name-hardcode"
RE_IDENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
RE_LIT = re.compile(r'"[^"]*"')
PRELUDE = ["Option", "Result", "Some", "None", "Ok", "Err", "Vec", "HashMap"]


def strip_comment(line):
    """Срезает //-комментарий, но НЕ внутри строкового литерала."""
    out = []
    inq = False
    i = 0
    n = len(line)
    while i < n:
        c = line[i]
        p = line[i - 1] if i > 0 else ""
        if c == '"' and p != "\\":
            inq = not inq
        if not inq and c == "/" and line[i + 1:i + 2] == "/":
            break
        out.append(c)
        i += 1
    return "".join(out)


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0

    all_nv, judged, builtins = [], [], None
    for dirpath, _dirs, names in os.walk(src):
        for nm in names:
            if not nm.endswith(".nv"):
                continue
            p = pathlib.Path(dirpath) / nm
            all_nv.append(p)
            if nm == "builtins.nv":
                continue
            if nm.endswith("_test.nv"):
                continue
            judged.append(p)
    all_nv.sort(key=lambda p: str(p).replace("\\", "/"))
    judged.sort(key=lambda p: str(p).replace("\\", "/"))
    for p in all_nv:
        if p.name == "builtins.nv":
            builtins = p
            break

    # (1) Единственный законный дом имён: каждый литерал-идентификатор в нём.
    from_builtins = set()
    if builtins is not None:
        for line in builtins.read_bytes().decode("utf-8", "replace").split("\n"):
            for m in RE_LIT.finditer(strip_comment(line.rstrip("\r"))):
                s = m.group(0).replace('"', "")
                if RE_IDENT.match(s):
                    from_builtins.add(s)

    names = sorted(from_builtins | set(PRELUDE))
    if not names:
        print(f"{NAME}: FAIL — список имён пуст: ни builtins.nv, ни прелюдия "
              f"не дали ни одного имени", file=sys.stderr)
        return 1
    rx = re.compile('"(' + "|".join(names) + ')"')

    bad = []
    for f in judged:
        rel = str(f.relative_to(src)).replace("\\", "/")
        for n, line in enumerate(f.read_bytes().decode("utf-8", "replace").split("\n"), 1):
            if line.endswith("\r"):
                line = line[:-1]
            code = re.sub(r"//.*$", "", line)
            if rx.search(code):
                bad.append(f"  {rel}:{n}:{line}")

    if bad:
        print(f"{NAME}: FAIL — имена языка/std как строковые литералы вне builtins.nv (П5):",
              file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Имя — в novac/src/builtins/builtins.nv (единый реестр остатка П5), "
              "здесь — константа/дверь.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: файлов .nv: {len(judged)}, имён в списке: {len(names)} "
          f"(из builtins.nv: {len(from_builtins)} + прелюдия), хардкод-имён вне builtins.nv: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
