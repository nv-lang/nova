# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-emitted-names.py — у каждого C-имени, которое novac
ПЕЧАТАЕТ, есть объявленное пространство (конвенция П24).

ГРАНИЦА, КОТОРУЮ ДЕРЖИТ ПРИСТАВКА, — это ABI:
  * `Nova_...`/`nova_...` — имена ОРАКУЛА и его рантайма (точки входа std, вход
    программы, аллокатор, печать, проверяемая арифметика). Мы их НЕ придумываем:
    совпадение с ними и есть ABI, и его стережёт check-novac-mangle-fixed-point;
  * `novac_...`, `NOVAC_...`, `_novac_...` — НАШИ имена, и приставка `c` ровно
    затем, чтобы выдуманное нами не столкнулось с рантаймом.
Исключения названы поимённо: C-ключевое `void`, подстановочник `_`, метод
`equal`, `fmod`, слоты шаблона оболочки и константа рантайма NOVA_UNIT.

ПОЧЕМУ PYTHON: shell-редакция гоняла grep по каждому файлу и цикл с `sed` на
КАЖДОЕ найденное имя — 3.3с (П14).

$1 — корень; $2 — override списка файлов через пробел (шов самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-emitted-names"
QUOTED = re.compile(r'"[A-Za-z_][A-Za-z0-9_]*(?:\$\{[^}]*\})?[A-Za-z0-9_]*"')
ALLOWED_PREFIX = ("Nova_", "nova_", "novac_", "NOVAC_", "_novac_")
ALLOWED_EXACT = {"void", "_", "equal", "fmod",
                 "__NOVAC_BODY__", "__NOVAC_STRLITS__", "NOVA_UNIT"}


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    if len(a) > 2:
        files = [pathlib.Path(x) for x in a[2].split()]
    else:
        files = [root / "novac/src/sem/mangle.nv", root / "novac/src/emit_c/emit_c.nv"]

    names = set()
    for f in files:
        if not f.is_file():
            print(f"{NAME}: FAIL — нет {f}: судить нечего (класс №519)", file=sys.stderr)
            return 1
        for m in QUOTED.findall(f.read_text(encoding="utf-8", errors="replace")):
            names.add(m)

    if not names:
        print(f"{NAME}: FAIL — не нашлось ни одного имени: разбор сломался (класс №519)", file=sys.stderr)
        return 1

    bad = []
    for q in sorted(names):
        n = q.strip('"')
        if n.startswith(ALLOWED_PREFIX) or n in ALLOWED_EXACT:
            continue
        bad.append(f"  {n} — имя без объявленного пространства (Nova_/nova_ — оракул, novac_/NOVAC_/_novac_ — наше)")

    if bad:
        print(f"{NAME}: FAIL — печатаемое C-имя вне объявленных пространств (П24):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Наше имя начинается с novac_/NOVAC_/_novac_ — приставка `c` затем, чтобы", file=sys.stderr)
        print("  выдуманное нами не столкнулось с рантаймом. Имя оракула (Nova_/nova_) не", file=sys.stderr)
        print("  выдумывают: его существование стережёт check-novac-mangle-fixed-point.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: печатаемых имён: {len(names)}, все в объявленных пространствах")
    return 0


if __name__ == "__main__":
    sys.exit(main())
