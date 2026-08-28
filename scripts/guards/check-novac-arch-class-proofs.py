# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-arch-class-proofs.py — у каждого класса проблем в
архитектуре есть все три доказательства (план 274.1 §4), И дверь каждого корня
живёт ровно в одном доме (подплан 274.4 шаг 5).

ЧАСТЬ 1 — ТРИ ДОКАЗАТЕЛЬСТВА, и ни одно не заменяет другого:
  **Верность:** почему решение убивает класс;
  **Место:** какой модуль и какие слои карты;
  **Минимальность:** что ломается при снятии КАЖДОГО инварианта.
Класс с одним доказательством — это намерение, а не решение.

Раздел «Классы проблем» обязан существовать (требование приёмки 274.1, владелец
2026-08-14): его отсутствие — красный, а не «судить нечего».

ЧАСТЬ 2 — ДОМ ДВЕРИ (274.4 шаг 5). Таблица §3а «Корни и их владельцы» называет у
каждого корня двери и ДОМ — файл или папку, где дверь объявлена. Страж требует
двух вещей разом: объявление ЕСТЬ в доме и его НЕТ ни в одном файле вне дома.
Рёбра §3 этого не ловят по построению: вторая дверь живёт ВНУТРИ законного ребра
(замер 2026-08-27 — эмиттер сам звал `fns.lookup`, а ребро `emit_c → sem`
объявлено и законно).

Имена дверей читаются КВАЛИФИЦИРОВАННО (`Тип.метод`): одноимённые методы разных
типов законны — `Interner @record` и `Checker mut @record` отвечают на разные
вопросы, и сравнение по голому имени дало бы ложное срабатывание. Ячейка, где
двери нет по природе (константы `builtins`), помечается `⚖` с причиной.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override пути к архитектуре (шов самотеста); $3 — override
директории novac/src (второй шов, для части 2).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-arch-class-proofs"
RE_SEC = re.compile(r"^## .*Классы проблем")
RE_HEAD = re.compile(r"^## ")
RE_CLASS = re.compile(r"^### К[0-9]")
RE_ROOTS = re.compile(r"^## .*Корни и их владельцы")
RE_ROW = re.compile(r"^\|")
RE_TICK = re.compile(r"`([^`]+)`")
# Дом — путь под novac/src: либо файл (`sem/channel.nv`), либо папка (`resolve/`).
RE_HOME = re.compile(r"^[a-z_]+/([a-z_]+\.nv)?$")


def door_decl_res(door):
    """Регексы объявления двери. `Тип.имя` — метод (три формы: `mut @`, `@`,
    конструктор `Тип.имя`); голое имя — свободная функция ИЛИ тип."""
    if "." in door:
        ty, nm = door.split(".", 1)
        ty, nm = re.escape(ty), re.escape(nm)
        return [re.compile(r"\bfn\s+" + ty + r"\s+mut\s+@" + nm + r"\b"),
                re.compile(r"\bfn\s+" + ty + r"\s+@" + nm + r"\b"),
                re.compile(r"\bfn\s+" + ty + r"\." + nm + r"\b")]
    nm = re.escape(door)
    return [re.compile(r"\bfn\s+" + nm + r"\s*\("),
            re.compile(r"\btype\s+" + nm + r"\b")]


def check_root_doors(lines, src, bad):
    """ЧАСТЬ 2: у каждой двери таблицы §3а объявление есть в доме и нет вне его."""
    if not src.is_dir():
        return 0
    files = []
    for dirpath, _dirs, names in os.walk(src):
        for nm in names:
            if nm.endswith(".nv") and not nm.endswith("_test.nv"):
                files.append(pathlib.Path(dirpath) / nm)
    files.sort(key=lambda q: str(q).replace("\\", "/"))
    texts = [(str(f.relative_to(src)).replace("\\", "/"),
              f.read_text(encoding="utf-8", errors="replace")) for f in files]

    n_doors = 0
    in_sec = False
    for line in lines:
        if RE_ROOTS.match(line):
            in_sec = True
            continue
        if in_sec and RE_HEAD.match(line):
            in_sec = False
        if not in_sec or not RE_ROW.match(line):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 5 or cells[0].startswith("---") or "корень" in cells[0]:
            continue
        doors_cell, home_cell = cells[2], cells[3]
        if "⚖" in doors_cell:
            continue
        home = RE_TICK.findall(home_cell)
        if not home or not RE_HOME.match(home[0]):
            bad.append(f"  {cells[0]} — колонка «дом двери» пуста или не путь под novac/src: {home_cell}")
            continue
        home = home[0]
        for door in RE_TICK.findall(doors_cell):
            n_doors += 1
            res = door_decl_res(door)
            here, elsewhere = 0, []
            for rel, text in texts:
                hit = any(r.search(text) for r in res)
                if not hit:
                    continue
                if rel == home or rel.startswith(home):
                    here += 1
                else:
                    elsewhere.append(rel)
            if here == 0:
                bad.append(f"  дверь `{door}` не объявлена в своём доме `{home}` "
                           f"(корень: {cells[0]})")
            for rel in elsewhere:
                bad.append(f"  дверь `{door}` объявлена ВНЕ дома `{home}`: {rel} "
                           f"— вторая дверь к корню «{cells[0]}»")
    return n_doors


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    doc = pathlib.Path(a[2]) if len(a) > 2 else root / "docs" / "dev" / "novac-architecture.md"
    src = pathlib.Path(a[3]) if len(a) > 3 else root / "novac" / "src"

    if not doc.is_file():
        print(f"{NAME}: FAIL — нет {doc}", file=sys.stderr)
        return 1

    lines = doc.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n")

    if not any(RE_SEC.match(l) for l in lines):
        print(f"{NAME}: FAIL — в архитектуре нет раздела «Классы проблем»", file=sys.stderr)
        print("  Требование приёмки 274.1: раздел обязан существовать (владелец 2026-08-14).",
              file=sys.stderr)
        return 1

    bad = []
    n_classes = 0
    in_sec = False
    cls, v, m, mi = "", False, False, False

    def check():
        if not cls:
            return
        if not (v and m and mi):
            miss = ("" if v else " Верность") + ("" if m else " Место") + \
                   ("" if mi else " Минимальность")
            bad.append(f"  {cls} — не хватает:{miss}")

    for line in lines:
        if RE_SEC.match(line):
            in_sec = True
            continue
        if in_sec and RE_HEAD.match(line):
            in_sec = False
        if in_sec and RE_CLASS.match(line):
            check()
            cls, v, m, mi = line, False, False, False
            n_classes += 1
            continue
        if in_sec and cls:
            if "**Верность:**" in line:
                v = True
            if "**Место:**" in line:
                m = True
            if "**Минимальность:**" in line:
                mi = True
    check()

    if bad:
        print(f"{NAME}: FAIL — классы без полного набора доказательств:", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Каждому классу: **Верность:** (почему решение убивает класс),", file=sys.stderr)
        print("  **Место:** (модуль/слои карты), **Минимальность:** (что ломается", file=sys.stderr)
        print("  при снятии каждого инварианта). План 274.1 §4.", file=sys.stderr)
        return 1

    # --- ЧАСТЬ 2: дом двери (274.4 шаг 5) -----------------------------------
    dbad = []
    n_doors = check_root_doors(lines, src, dbad)
    if dbad:
        print(f"{NAME}: FAIL — дверь корня живёт не там, где сказано в таблице §3а:",
              file=sys.stderr)
        for b in dbad:
            print(b, file=sys.stderr)
        print("  У корня ОДИН дом двери. Переезд двери — правка таблицы §3а тем же", file=sys.stderr)
        print("  слиянием; вторая дверь к одному корню — находка, а не деталь.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: классов {n_classes}, у каждого все три доказательства; "
          f"дверей корней {n_doors}, каждая в своём доме")
    return 0


if __name__ == "__main__":
    sys.exit(main())
