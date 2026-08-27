# -*- coding: utf-8 -*-
"""scripts/guards/check-after-compact-budget.py — впрыск после сжатия контекста
не растёт молча и не приезжает дырявым.

Адрес: реестр 221.1 №774. Ссылка проставлена 2026-08-27 по требованию
`check-guard-wiring` — оно справедливое: страж без адреса невозможно связать
с тем, зачем он заведён, когда автора рядом уже нет.

ЗАЧЕМ. Хук `SessionStart`/`compact` (`scripts/claude-hooks/inject-after-compact.py`,
пришёл слиянием окна 274) кладёт тела файлов из `.claude/after-compact.list` в
КАЖДОЕ окно после КАЖДОГО сжатия. Шапка списка просит «держи коротко» — но это
просьба к вниманию, а проект везде меняет внимание на механизм. Замер на день
заведения: три файла, 12682 байта. Четвёртый добавит тысячи, пятый ещё, и
заметить это будет некому: стоимость не печатается нигде, кроме `--list`,
который зовут вручную.

ПОТОЛОК, А НЕ ХРАПОВИК — намеренно, форма взята у `gate-budget.baseline`
(«число — не рекорд, а потолок с запасом»). Храповик здесь краснел бы на каждой
прозаической правке `flow.md`: сократил абзац — изволь опустить базу. Такой шум
кончается тем, что базу правят не глядя. Потолок молчит, пока список не вырос
по существу, и требует ОСОЗНАННОГО поднятия — видного в диффе, с причиной.

ПРОВЕРЯЕТ ЧЕТЫРЕ ВЕЩИ:
  1. файл потолка есть и разбирается: одно число, байты;
  2. хук ЧИТАЕТ список — в тексте инжектора есть имя `after-compact.list`.
     Механизм, который можно вырезать одной строкой и не заметить, — не
     механизм (та же точка, что у `check-gate-budget`);
  3. КАЖДЫЙ файл списка существует. Пропавший файл инжектор пропускает со
     строкой в stderr и кодом 0 — то есть правило перестаёт приезжать в окна,
     а stderr хука в контекст не попадает. Молчание читается как успех —
     класс реестра 221.1 №770 («ярус молчал о том, чего не судил»), и ловить
     это должен гейт;
  4. суммарный размер тел (YAML-шапка снимается, как её снимает инжектор) не
     превышает потолок.

ЧЕГО НЕ ПРОВЕРЯЕТ: разумность самого потолка и содержимое файлов. Первое задаёт
замер, второе — их собственные стражи.

$1 — корень; $2 — override пути к списку; $3 — override пути к потолку (швы
самотеста).
"""
import pathlib
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-after-compact-budget"


def body_without_frontmatter(text):
    """Ровно то, что делает инжектор: между первой парой строк `---` — шапка."""
    lines = text.split("\n")
    if lines and lines[0].strip() == "---":
        for i in range(1, len(lines)):
            if lines[i].strip() == "---":
                return "\n".join(lines[i + 1:]).strip("\n")
    return text.strip("\n")


def entries(path):
    rows = []
    for line in path.read_text(encoding="utf-8").split("\n"):
        s = line.strip()
        if s and not s.startswith("#"):
            rows.append(s)
    return rows


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    lst = pathlib.Path(a[2]) if len(a) > 2 else root / ".claude" / "after-compact.list"
    cap_file = (pathlib.Path(a[3]) if len(a) > 3
                else root / "scripts" / "guards" / "after-compact-budget.baseline")
    injector = root / "scripts" / "claude-hooks" / "inject-after-compact.py"

    if not lst.is_file():
        print(f"{NAME} ok: судить нечего — списка {lst} нет, хук ничего не впрыскивает")
        return 0

    if not cap_file.is_file():
        print(f"{NAME}: FAIL — нет файла потолка {cap_file}: объём впрыска не ограничен ничем",
              file=sys.stderr)
        return 1
    nums = [s.strip() for s in cap_file.read_text(encoding="utf-8").split("\n")
            if s.strip() and not s.strip().startswith("#")]
    if len(nums) != 1 or not nums[0].isdigit():
        print(f"{NAME}: FAIL — потолок {cap_file} не разобран: жду ОДНО число (байты), "
              f"нашёл {nums!r}", file=sys.stderr)
        return 1
    cap = int(nums[0])

    if not injector.is_file():
        print(f"{NAME}: FAIL — нет инжектора {injector}, а список есть: "
              f"список без читателя ничего не гарантирует", file=sys.stderr)
        return 1
    if "after-compact.list" not in injector.read_text(encoding="utf-8"):
        print(f"{NAME}: FAIL — инжектор {injector.name} НЕ читает список: "
              f"механизм выхолощен, впрыск больше не управляется списком", file=sys.stderr)
        return 1

    rows = entries(lst)
    missing, total = [], 0
    for rel in rows:
        p = root.joinpath(*rel.split("/"))
        if not p.is_file():
            missing.append(rel)
            continue
        total += len(body_without_frontmatter(
            p.read_text(encoding="utf-8", errors="replace")).encode("utf-8"))

    if missing:
        print(f"{NAME}: FAIL — файлов из списка нет на диске: {len(missing)}", file=sys.stderr)
        for rel in missing:
            print(f"    {rel}", file=sys.stderr)
        print("    Хук пропустит их со строкой в stderr и кодом 0 — то есть правило", file=sys.stderr)
        print("    перестанет приезжать в окна, а stderr хука в контекст не попадает.", file=sys.stderr)
        print("    Лечится одним из двух: вернуть путь в списке либо убрать строку.", file=sys.stderr)
        return 1

    if total > cap:
        print(f"{NAME}: FAIL — впрыск после сжатия {total} байт при потолке {cap}",
              file=sys.stderr)
        print(f"    Файлов в списке: {len(rows)}. Это едет в КАЖДОЕ окно после КАЖДОГО", file=sys.stderr)
        print("    сжатия, поэтому число растёт молча и стоит дороже, чем кажется.", file=sys.stderr)
        print(f"    Либо сократи файлы, либо подними потолок в {cap_file.name} ТЕМ ЖЕ", file=sys.stderr)
        print("    диффом и напиши там, что именно понадобилось.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: впрыск после сжатия {total} байт при потолке {cap}, "
          f"файлов {len(rows)}, все на месте")
    return 0


if __name__ == "__main__":
    sys.exit(main())
