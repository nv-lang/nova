# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-deps.py — импорт существует, только если ребро
записано в таблице §3 архитектуры, и граф модулей ацикличен.

ДВА ПРАВИЛА:
  1. каждый относительный импорт `./x` / `../x` обязан быть строкой таблицы §3
     («из» -> «в»). Ребро добавляется ТОЛЬКО строкой таблицы с контрактом «что
     течёт» — иначе карта перестаёт описывать код;
  2. ЦИКЛОВ НЕТ. Правило было и раньше, механизма не было: §3/К4 утверждал, что
     ацикличность «следует из таблицы», а проба 2026-08-17 это опровергла —
     цикл добавляется строкой таблицы ровно так же, как честное ребро, и всё
     остаётся зелёным. Следствие из документа не механизм. Здесь обрезка с ОБЕИХ
     сторон (исток и сток), и остаток — это цикл, названный поимённо.

ВВОЗ ИЗ `std` НЕ РЕБРО: это не модуль novac. Форм ввоза четыре — `import`/`use`,
с `export` и без, отступ любой; все найдены адверсарными пробами.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории; $3 — override архитектуры.
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-deps"
RE_SEC = re.compile(r"^## 3\.")
RE_HEAD = re.compile(r"^## ")
RE_IMPORT = re.compile(r"^[ \t\v\f]*(export[ \t\v\f]+)?(import|use)[ \t\v\f]")
RE_IMPORT_STRIP = re.compile(r"^[ \t\v\f]*(export[ \t\v\f]+)?(import|use)[ \t\v\f]+")


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"
    arch = pathlib.Path(a[3]) if len(a) > 3 else root / "docs" / "dev" / "novac-architecture.md"

    if not arch.is_file():
        print(f"{NAME}: FAIL — нет {arch}", file=sys.stderr)
        return 1
    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (novac/src ещё нет)")
        return 0

    # --- разрешённые рёбра из таблицы §3 ------------------------------------
    edges = []
    inb = False
    for line in arch.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"):
        if RE_SEC.match(line):
            inb = True
            continue
        if inb and RE_HEAD.match(line):
            inb = False
        if not inb:
            continue
        f = line.split("|")
        if len(f) < 3 or "`" not in f[1]:
            continue
        frm = re.sub(r"[` ]", "", f[1])
        if frm in ("из", "---"):
            continue
        for to in f[2].replace("`", "").split(","):
            to = to.replace(" ", "")
            if to:
                edges.append((frm, to))

    if not edges:
        print(f"{NAME}: FAIL — таблица §3 не распарсилась из {arch}", file=sys.stderr)
        return 1

    known = {m for e in edges for m in e}
    edge_set = set(edges)

    files = []
    for dirpath, _dirs, names in os.walk(src):
        for nm in names:
            if nm.endswith(".nv"):
                files.append(pathlib.Path(dirpath) / nm)
    files.sort(key=lambda p: str(p).replace("\\", "/"))

    bad = []
    total = 0
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        mod = rel.split("/")[0] if "/" in rel else "main"
        if mod not in known and mod != "main":
            bad.append(f"  {rel}: модуль '{mod}' отсутствует в карте §3")
            continue
        for line in f.read_bytes().decode("utf-8", "replace").split("\n"):
            if line.endswith("\r"):
                line = line[:-1]
            if not RE_IMPORT.match(line):
                continue
            imp = RE_IMPORT_STRIP.sub("", line)
            imp = re.split(r"[ \t\v\f]", imp, maxsplit=1)[0]
            # ТОЛЬКО относительные пути: ввоз из `std` не модуль novac.
            if not re.match(r"^\.\.?/", imp):
                continue
            imp = re.sub(r"^\.\.?/", "", imp)
            imp = re.split(r"[ .{]", imp, maxsplit=1)[0]
            if not imp:
                continue
            total += 1
            if (mod, imp) not in edge_set:
                bad.append(f"  {rel}: импорт '{imp}' — ребра '{mod} -> {imp}' нет в таблице §3")

    if bad:
        print(f"{NAME}: FAIL — импорты вне таблицы рёбер (архитектура §3):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Ребро добавляется ТОЛЬКО строкой таблицы §3 с контрактом «что течёт».", file=sys.stderr)
        return 1

    # --- ацикличность: обрезка с обеих сторон, остаток — цикл ---------------
    dead = [False] * len(edges)
    changed = True
    while changed:
        changed = False
        for i, (fi, ti) in enumerate(edges):
            if dead[i]:
                continue
            out = any(not dead[j] and fj == ti for j, (fj, _tj) in enumerate(edges))
            inn = any(not dead[j] and tj == fi for j, (_fj, tj) in enumerate(edges))
            if not out or not inn:
                dead[i] = True
                changed = True
    cyc = [f"  {f} -> {t}" for i, (f, t) in enumerate(edges) if not dead[i]]
    if cyc:
        print(f"{NAME}: FAIL — в графе модулей ЦИКЛ (план §10.3):", file=sys.stderr)
        for c in cyc:
            print(c, file=sys.stderr)
        print("  Направление зависимостей — единственное архитектурное правило", file=sys.stderr)
        print("  окна: слой не смотрит вверх. Цикл снимается разрывом ребра, а не", file=sys.stderr)
        print("  строкой в таблице.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: рёбер в карте {len(edges)}, импортов проверено {total}, вне таблицы 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
