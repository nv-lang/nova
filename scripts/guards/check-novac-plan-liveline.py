# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-plan-liveline.py — живая строка плана не отстаёт
от кода.

ПОЧЕМУ. План 274 требует от этой строки обновления ТЕМ ЖЕ слиянием, что и код.
Устаревшая строка не молчит — она говорит НЕВЕРНОЕ, и по ней принимают решения.

СУДЯТСЯ ТОЛЬКО СТРОКИ, КОТОРЫЕ САМИ НАЗЫВАЮТ СЕБЯ ЖИВЫМИ, и дата ищется в ТОЙ
ЖЕ строке, где стоит слово. Первая редакция брала любую дату документа и
зазеленела на дате из соседнего раздела, пока живая строка стояла
четырёхдневной давности: страж не промолчал — он сказал «свежо».

Судятся ВСЕ живые строки: каждая описывает свой этап, и отставшая врёт про свой.
Этап закрыт — строка перестаёт называться живой, тогда и не судится.

ПОЧЕМУ PYTHON: shell-редакция и так звала python ради разницы дат — теперь без
посредника (П14).

$1 — корень; $2 — override даты кода (шов самотеста).
"""
import datetime
import pathlib
import re
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-plan-liveline"
LAG = 2
RE_DATE = re.compile(r"на ([0-9]{4}-[0-9]{2}-[0-9]{2})")


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    plan = root / "docs" / "plans" / "274-novac-self-hosted-compiler.md"

    if not plan.is_file():
        print(f"{NAME} ok: судить нечего (нет {plan})")
        return 0

    if len(a) > 2 and a[2]:
        code_date = a[2]
    else:
        code_date = subprocess.run(
            ["git", "-C", str(root), "log", "-1", "--format=%ad", "--date=short", "--", "novac/src"],
            capture_output=True).stdout.decode("utf-8", "replace").strip()
    if not code_date:
        print(f"{NAME} ok: судить нечего (нет истории git по novac/src)")
        return 0

    live = [(n, l) for n, l in
            enumerate(plan.read_text(encoding="utf-8", errors="replace")
                      .replace("\r", "").split("\n"), 1)
            if "живая строка" in l.lower()]
    if not live:
        print(f"{NAME}: FAIL — в плане нет ни одной живой строки", file=sys.stderr)
        print("  Правило «обновляется тем же слиянием, что и код» живёт в этих строках;", file=sys.stderr)
        print("  без них состояние работ читать неоткуда.", file=sys.stderr)
        return 1

    dates = sorted({m for _n, l in live for m in RE_DATE.findall(l)})
    if not dates:
        print(f"{NAME}: FAIL — у живой строки нет маркера даты (`на ГГГГ-ММ-ДД`)", file=sys.stderr)
        for n, l in live[:3]:
            print(f"{n}:{l}", file=sys.stderr)
        print("  Без даты «живая» — это слово, а не свойство: проверить нечем.", file=sys.stderr)
        return 1

    # Живая строка БЕЗ своего маркера даты не судится ничем: `dates` собирает
    # все даты скопом, и одна датированная строка «прикрывает» остальные.
    # Дока стража утверждает «Судятся ВСЕ живые строки», а код это утверждение
    # не держал — правка 2026-09-08. ХРАПОВИК, а не жёсткий отказ: на день
    # правки такая строка ОДНА (план 274, шапка с заглавной буквы), и её правит
    # окно 274 своей волной; краснеть на чужой незакоммиченной работе нельзя.
    undated = [n for n, l in live if not RE_DATE.search(l)]
    base_undated = 0
    bp = root / "scripts" / "guards" / "novac-liveline.baseline"
    if bp.is_file():
        for bl in bp.read_text(encoding="utf-8", errors="replace").split("\n"):
            if bl.startswith("undated="):
                try:
                    base_undated = int(bl.split("=", 1)[1].strip())
                except ValueError:
                    pass
                break
    if len(undated) > base_undated:
        print(f"{NAME}: FAIL — живых строк без маркера даты {len(undated)} (база {base_undated})",
              file=sys.stderr)
        for n in undated[:5]:
            print(f"  {plan}:{n}", file=sys.stderr)
        print("  Без даты «живая» — это слово, а не свойство: проверить нечем.", file=sys.stderr)
        print("  Допиши в ТУ ЖЕ строку маркер вида `на ГГГГ-ММ-ДД`.", file=sys.stderr)
        return 1
    if len(undated) < base_undated:
        print(f"{NAME}: долг СНИЗИЛСЯ ({len(undated)} < базы {base_undated}) — опусти базу в {bp}")

    # ИМЯ ЧЕСТНОЕ (правка 2026-09-08): `sorted()[0]` — самая СТАРАЯ дата, и это
    # намеренно: судится самая отставшая строка. Переменная звалась `newest`, а
    # сообщение ниже — «самый свежий маркер», хотя показывало самый отставший.
    lagging = dates[0]
    try:
        diff = (datetime.date.fromisoformat(code_date) - datetime.date.fromisoformat(lagging)).days
    except ValueError:
        print(f"{NAME}: FAIL — не удалось сравнить даты ({code_date} vs {lagging})", file=sys.stderr)
        return 1

    if diff > LAG:
        print(f"{NAME}: FAIL — живая строка плана отстала от кода на {diff} дней (предел {LAG})",
              file=sys.stderr)
        print(f"  последний коммит в novac/src: {code_date}", file=sys.stderr)
        print(f"  самый ОТСТАВШИЙ маркер в плане: {lagging}", file=sys.stderr)
        print("  План 274 требует от этой строки обновления ТЕМ ЖЕ слиянием, что и код.", file=sys.stderr)
        print("  Устаревшая строка не молчит — она говорит неверное, и по ней принимают", file=sys.stderr)
        print("  решения. Обновить текст и дату маркера в этом же слиянии.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: живая строка плана свежа (самый отставший маркер {lagging}, код {code_date}, "
          f"отставание {diff} дн. при пределе {LAG})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
