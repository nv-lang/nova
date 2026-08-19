# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-edge-payload.py — у каждого ребра карты §3 объявлено
«что течёт» (274.1 §2в п.3).

ЗАЧЕМ. Ребро без контракта — дыра в карте, а не привилегия: оно разрешает «A
видит B», не отвечая, ЧТО именно A оттуда берёт.

ПРАВИЛО (перенесено из shell-редакции слово в слово, 2026-08-19):
  * таблица ищется в разделе `## 3.` архитектуры; шапка — по ИМЕНИ колонки
    «что течёт», а не по её номеру;
  * фенсед-блок (``` или ~~~) — ИЛЛЮСТРАЦИЯ, а не карта: строки внутри него не
    судятся, и код-блок ЗАКРЫВАЕТ таблицу;
  * одна строка-разделитель сразу под шапкой пропускается; пустые строки
    таблицы (одни черты и пробелы) — тоже;
  * если «что течёт» по шапке ПОСЛЕДНЯЯ колонка, ячейка берётся до конца строки
    (вертикальная черта внутри текста иначе обрезала бы контракт молча); если за
    ней есть ещё колонка — берётся РОВНО своя, иначе пустая ячейка пряталась бы
    за соседней справа;
  * дыра — пустая ячейка, один прочерк, или заглушка (tbd/todo/na/n/a/?/xxx);
    отсутствие колонки вовсе — тоже дыра.

ПОЧЕМУ PYTHON: shell-редакция гоняла awk, затем цикл с `sed`+`tr` на КАЖДУЮ
строку таблицы — 4.6с на 56 рёбрах (П14).

$1 — корень репозитория; $2 — override пути к архитектуре (шов самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-edge-payload"
FLOW = "что течёт"
STUBS = {"tbd", "todo", "na", "n/a", "?", "??", "???", "xxx"}
NBSP = " "


def norm(cell):
    # HTML-комментарий рендерится ПУСТОЙ ячейкой, значит контракта в ней нет.
    # Снимается нежадно, чтобы «<!-- a --> `Token[]` <!-- b -->» остался зелёным.
    s = re.sub(r"<!--[^>]*-->", "", cell)
    for dash in ("—", "–", "−"):
        s = s.replace(dash, "")
    s = s.replace(NBSP, "")
    s = re.sub(r"[-.,:;*_`~\"|\s]", "", s)
    return s.lower()


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    arch = pathlib.Path(a[2]) if len(a) > 2 else root / "docs" / "dev" / "novac-architecture.md"

    if not arch.is_file():
        print(f"{NAME}: FAIL — нет {arch}: карта рёбер пропала, судить «что течёт» нечем", file=sys.stderr)
        return 1

    lines = arch.read_text(encoding="utf-8", errors="replace").replace("\r", "").replace("\t", " ").split("\n")

    header_seen = False
    rows = []                      # (номер строки, miss, label, flow)
    insec = fence = intbl = False
    flowcol = headlast = 0
    needsep = False

    for n, raw in enumerate(lines, 1):
        if re.match(r"^\s*(```|~~~)", raw):
            fence = not fence
            intbl = False
            continue
        if fence:
            continue
        if re.match(r"^##\s+3\.", raw):
            insec = True
            continue
        if insec and re.match(r"^##\s", raw):
            insec = False
        if not insec:
            continue

        if not re.match(r"^\s*\|", raw):
            intbl = False
            continue

        cells = raw.split("|")
        nf = len(cells)

        if not intbl:
            if FLOW not in raw:
                continue
            flowcol = next(i for i, c in enumerate(cells, 1) if FLOW in c)
            headlast = nf if cells[nf - 1].strip() else nf - 1
            intbl, needsep, header_seen = True, True, True
            continue

        if needsep:
            needsep = False
            if re.match(r"^\s*\|[-:|\s]*$", raw):
                continue

        if not re.sub(r"[|\s]", "", raw):
            continue

        lastcol = nf if cells[nf - 1].strip() else nf - 1

        label = ""
        for i in range(2, flowcol):
            c = cells[i - 1].strip()
            if c:
                label = c if not label else label + " -> " + c
        if not label:
            label = "(ребро без имён в колонках слева)"

        flow, miss = "", True
        if lastcol >= flowcol:
            miss = False
            if flowcol >= headlast:
                flow = "|".join(cells[flowcol - 1:lastcol])
            else:
                flow = cells[flowcol - 1]

        rows.append((n, miss, label, flow.strip()))

    if not header_seen:
        print(f"{NAME}: FAIL — в {arch} не нашлось таблицы рёбер с колонкой «{FLOW}»", file=sys.stderr)
        print("  ждали раздел «## 3.» и в нём строку-шапку markdown с этой ячейкой.", file=sys.stderr)
        print("  Раздел переименовали/перенумеровали или колонку убрали — страж потерял", file=sys.stderr)
        print("  мишень, а молчать нельзя: это ровно класс №519 (вечнозелёный страж).", file=sys.stderr)
        return 1

    if not rows:
        print(f"{NAME}: FAIL — таблица рёбер §3 в {arch} найдена, но строк-рёбер под шапкой ноль:", file=sys.stderr)
        return 1

    bad = []
    for n, miss, label, flow in rows:
        hole = ""
        s = norm(flow)
        if s == "":
            hole = "колонка «что течёт» пуста (или в ней один прочерк)"
        elif s in STUBS:
            hole = f"в колонке «что течёт» заглушка «{flow}», а не контракт"
        if miss:
            hole = "в строке нет колонки «что течёт» вовсе (колонок меньше, чем в шапке)"
        if hole:
            bad.append(f"  {arch}:{n}: {label} — {hole}")

    if bad:
        print(f"{NAME}: FAIL — рёбра без объявленного «что течёт» (274.1 §2в п.3, карта §3):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Ребро без контракта — дыра в карте, а не привилегия: оно разрешает «A видит B»,", file=sys.stderr)
        print("  не отвечая, ЧТО именно A оттуда берёт. Впиши в колонку «что течёт» конкретные", file=sys.stderr)
        print("  данные ребра («Token[]», «строит GreenNode», «читает инстанцирования»), либо", file=sys.stderr)
        print("  убери строку: ребра, которое не объяснить, не существует.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: рёбер в таблице §3: {len(rows)}, все несут «что течёт», дыр: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
