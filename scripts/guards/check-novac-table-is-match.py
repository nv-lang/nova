# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-table-is-match.py — таблица не пишется цепочкой
`if` (П21 п.3).

ПРАВИЛО. Три и более подряд идущих `if x == Тип.Вариант { return ... }` об
ОДНОЙ переменной — это отображение, написанное вручную. `match` показывает
отображение отображением, и ветка-остаток обязана быть либо `None` (частичность
по типу), либо отказом (`ice`/`@refuse`) — цепочка `if` не обязана ничему и
молча проваливается мимо.

СЧИТАЕТ ПОДРЯД ИДУЩИЕ строки: любая другая строка обрывает серию. Тесты
(`*_test.nv`) вне суда.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-table-is-match"
RE_ROW = re.compile(r"^if ([A-Za-z_@][A-Za-z0-9_.]*) == [A-Z][A-Za-z0-9_]*\.[A-Za-z0-9_]+ \{ return .* \}$")


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0

    files = []
    for dirpath, _dirs, names in os.walk(src):
        for nm in names:
            if nm.endswith(".nv") and not nm.endswith("_test.nv"):
                files.append(pathlib.Path(dirpath) / nm)
    files.sort(key=lambda p: str(p).replace("\\", "/"))

    if not files:
        print(f"{NAME}: FAIL — в {src} нет ни одного .nv: страж потерял мишень (класс №519)",
              file=sys.stderr)
        return 1

    bad = []
    total = 0
    # Серия НЕ обрывается на границе файла: так считал единый awk-проход, и имя
    # файла в отказе — то, в котором серия закончилась.
    run, start, curvar, rel = 0, 0, "", ""
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        for n, raw in enumerate(f.read_bytes().decode("utf-8", "replace").split("\n"), 1):
            if raw.endswith("\r"):
                raw = raw[:-1]
            line = raw.lstrip(" \t\v\f")
            m = RE_ROW.match(line)
            if m:
                v = m.group(1)
                if v == curvar:
                    run += 1
                else:
                    curvar, run, start = v, 1, n
                total += 1
                continue
            if run >= 3:
                bad.append(f"  {rel}:{start} — цепочка из {run} `if {curvar} == ...` подряд: "
                           f"это таблица, пиши match")
            run, curvar = 0, ""
    if run >= 3:
        bad.append(f"  {rel}:{start} — цепочка из {run} `if {curvar} == ...` подряд: "
                   f"это таблица, пиши match")

    if bad:
        print(f"{NAME}: FAIL — таблица написана цепочкой if (П21 п.3):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  match показывает отображение отображением; ветка-остаток обязана быть", file=sys.stderr)
        print("  либо None (частичность по типу), либо отказом (ice/@refuse).", file=sys.stderr)
        return 1

    print(f"{NAME} ok: строк-таблиц (if x == Тип.Вариант => return): {total}, "
          f"цепочек длиннее двух: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
