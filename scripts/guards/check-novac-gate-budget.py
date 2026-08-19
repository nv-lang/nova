# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-gate-budget.py — механизм бюджета времени на месте
и не выродился (конвенция П33).

ЗАЧЕМ. Гейт, который идёт девять минут, не гоняют — а правило, которое не
гоняют, не правило. 2026-08-19 полный прогон стоил 558с, и виноваты были НЕ
проверки: 198 процессов на чтение очереди из 33 стражей, 40 стартов
интерпретатора по 73мс, пересборка компилятора на каждом прогоне. Такая форма
возвращается молча — если цену никто не мерит. Мерит её сам гейт; этот страж
следит, чтобы механизм не сняли и не выхолостили.

ПРОВЕРЯЕТ ЧЕТЫРЕ ВЕЩИ:
  1. файл бюджета есть и разбирается: строки `<ярус> <секунды>`;
  2. гейт ЧИТАЕТ его и умеет краснеть — в тексте гейта есть и чтение файла, и
     вызов `fail` по превышению. Механизм, который можно вырезать одной
     строкой и не заметить, — это не механизм;
  3. предел масштабируется калибровкой машины (`CAL`), иначе на медленной
     машине он краснеет на здоровом дереве, а на быстрой ничего не ловит;
  4. у КАЖДОГО яруса, который гейт принимает, либо есть строка бюджета, либо
     гейт вслух говорит, что ярус не судится. Молчаливо несудимый ярус — это
     вечнозелёная дыра (класс №519).

ЧЕГО НЕ ПРОВЕРЯЕТ: разумность самих чисел. Их задаёт замер, и подпирать замер
ещё одним числом в страже значило бы гадать.

ПОЧЕМУ ЗАМЕР, А НЕ РАЗБОР ФОРМЫ. Первая редакция этого правила пробовала искать
«процесс на элемент цикла» разбором shell-синтаксиса — и на первой же пробе
отнесла к телу цикла строки, стоявшие вне его. Хрупкая эвристика о форме даёт
ложную красноту на здоровом коде; время же не обманешь: если форма плохая,
секунды вырастут, и вырастут измеримо.

$1 — корень репозитория; $2 — override пути к гейту (шов самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-gate-budget"
RE_ROW = re.compile(r"^([a-z]+)[ \t]+([0-9]+)[ \t]*$")
RE_TIERS = re.compile(r"NOVAC_TIER.*\n?.*?case[^\n]*\n\s*([a-z|]+)\)", re.M)


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    gate = pathlib.Path(a[2]) if len(a) > 2 else root / "scripts" / "gate-novac.sh"
    budget = root / "scripts" / "guards" / "novac-gate-budget.baseline"

    if not gate.is_file():
        print(f"{NAME}: FAIL — нет гейта {gate}: судить бюджет нечему", file=sys.stderr)
        return 1
    if not budget.is_file():
        print(f"{NAME}: FAIL — нет файла бюджета {budget}: время гейта не ограничено ничем",
              file=sys.stderr)
        print("  Гейт без потолка времени возвращается к девяти минутам за неделю —", file=sys.stderr)
        print("  это уже было (558с, 2026-08-19). Заведи строки `<ярус> <секунды>` замером.", file=sys.stderr)
        return 1

    rows = {}
    for line in budget.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        m = RE_ROW.match(line)
        if not m:
            print(f"{NAME}: FAIL — строка бюджета не разбирается: «{line[:60]}»", file=sys.stderr)
            print("  Форма строгая: `<ярус> <секунды>`. Неразобранная строка — это ярус,", file=sys.stderr)
            print("  который молча остался без потолка.", file=sys.stderr)
            return 1
        rows[m.group(1)] = int(m.group(2))

    if not rows:
        print(f"{NAME}: FAIL — в {budget} нет ни одной строки яруса: файл есть, потолка нет",
              file=sys.stderr)
        return 1

    text = gate.read_text(encoding="utf-8", errors="replace")

    missing = []
    if "novac-gate-budget.baseline" not in text:
        missing.append("гейт не читает файл бюджета")
    if not re.search(r"GATE_ELAPSED", text):
        missing.append("гейт не меряет собственное время (нет GATE_ELAPSED)")
    if not re.search(r"BUDGET_LIMIT=\$\(\(\s*BUDGET \* CAL\s*\)\)", text):
        missing.append("предел не масштабируется калибровкой машины (BUDGET * CAL)")
    if not re.search(r'fail "ярус \$NOVAC_TIER вышел '
                     r'за бюджет', text):
        missing.append("превышение бюджета не приводит к отказу (нет вызова fail)")
    if missing:
        print(f"{NAME}: FAIL — механизм бюджета выхолощен:", file=sys.stderr)
        for m in missing:
            print(f"  {m}", file=sys.stderr)
        print("  Механизм, который можно вырезать одной строкой и не заметить, — не механизм.", file=sys.stderr)
        print("  Верни проверку в гейт или объясни в П33, чем она заменена.", file=sys.stderr)
        return 1

    # Ярусы, которые гейт ПРИНИМАЕТ: из его же строки разбора.
    tiers = set()
    m = re.search(r"NOVAC_TIER\}?\"?\s*in\s*\n\s*([^)]*)\)", text)
    if m:
        tiers = {x.strip() for x in m.group(1).split("|") if x.strip()}
    unjudged = sorted(tiers - set(rows))
    if unjudged and "строки бюджета для него нет (не судится)" not in text:
        print(f"{NAME}: FAIL — ярус без бюджета и без честного «не судится»: "
              f"{', '.join(unjudged)}", file=sys.stderr)
        return 1

    print(f"{NAME} ok: ярусов с бюджетом {len(rows)} ({', '.join(f'{k} {v}с' for k, v in sorted(rows.items()))}), "
          f"без бюджета {len(unjudged)} (названы вслух), механизм в гейте на месте")
    return 0


if __name__ == "__main__":
    sys.exit(main())
