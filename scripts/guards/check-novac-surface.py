# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-surface.py — публичная поверхность модулей под
храповиком В ОБЕ СТОРОНЫ (274 §10.4).

ПРАВИЛО (перенесено из shell-редакции слово в слово, 2026-08-19): число строк
`^export ` на модуль сверяется с базой `novac-surface.baseline`. Рост —
сознательное решение (подними базу тем же слиянием и назови, зачем имя нужно
наружу); СУЖЕНИЕ тоже красное — иначе следующий рост до прежней цифры пройдёт
молча. Модуль без строки в базе и строка без модуля — тоже расхождение.

ПОЧЕМУ PYTHON: shell-редакция поднимала `tr` и `grep` на КАЖДЫЙ файл плюс
`join`/`awk`/`sort` — 2.8с там, где работы на доли секунды (П14).

$1 — корень; $2 — override novac/src; $3 — override файла базы (швы самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-surface"


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"
    base_path = pathlib.Path(a[3]) if len(a) > 3 else root / "scripts" / "guards" / "novac-surface.baseline"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0
    if not base_path.is_file():
        print(f"{NAME}: FAIL — нет базы {base_path}: храповик поверхности сверять не с чем", file=sys.stderr)
        return 1

    # --- факт: экспортов на модуль ---------------------------------------
    fact = {}
    for p in sorted(src.rglob("*.nv")):
        if p.name.endswith("_test.nv"):
            continue
        rel = p.relative_to(src).as_posix()
        mod = rel.split("/")[0] if "/" in rel else "main"
        n = sum(1 for l in p.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n")
                if l.startswith("export "))
        fact[mod] = fact.get(mod, 0) + n

    # --- база -------------------------------------------------------------
    base = {}
    for l in base_path.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"):
        l = re.sub(r"#.*", "", l)
        parts = l.split()
        if len(parts) == 2 and parts[1].isdigit():
            base[parts[0]] = int(parts[1])

    if not base:
        print(f"{NAME}: FAIL — база {base_path} пуста или не разобралась: сверять нечем", file=sys.stderr)
        return 1

    grew, shrank, nobase, nomod = [], [], [], []
    for mod in sorted(set(fact) | set(base)):
        f, b = fact.get(mod), base.get(mod)
        if f is not None and b is not None:
            if f > b:
                grew.append(f"  {mod}: экспортов {f}, база {b} — рост без поднятия базы")
            elif f < b:
                shrank.append(f"  {mod}: экспортов {f}, база {b} — база протухла, опусти её")
        elif b is None:
            nobase.append(f"  {mod}: модуль есть в коде ({f} экспортов), строки в базе нет")
        else:
            nomod.append(f"  {mod}: строка в базе ({b}), а модуля в коде нет")

    if grew or shrank or nobase or nomod:
        print(f"{NAME}: FAIL — публичная поверхность разошлась с базой (274 §10.4):", file=sys.stderr)
        for group in (grew, shrank, nobase, nomod):
            for l in group:
                print(l, file=sys.stderr)
        print("  Рост поверхности — сознательное решение: подними базу тем же слиянием и", file=sys.stderr)
        print("  назови в сообщении, зачем новое имя нужно наружу. Сужение тоже красное:", file=sys.stderr)
        print("  иначе следующий рост до прежней цифры пройдёт молча.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: модулей {len(fact)}, экспортов всего {sum(fact.values())}, все на базе")
    return 0


if __name__ == "__main__":
    sys.exit(main())
