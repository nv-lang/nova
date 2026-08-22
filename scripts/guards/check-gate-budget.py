# -*- coding: utf-8 -*-
"""scripts/guards/check-gate-budget.py — механизм бюджета времени на месте
и не выродился (конвенция гейтов и стражей, Г1..Г7).

ЗАЧЕМ. Гейт, который идёт двадцать три минуты, не гоняют — а правило, которое
не гоняют, не правило. Замер 2026-08-21 по шагам: 67 шагов, 1401 секунда, из
них самотесты 365с, `conformance --full` 224с, мега-CU 210с. Ответ на это —
ярусы (Г4): дешёвый ярус зовётся машиной на каждый пуш. Но ярус остаётся
дешёвым, только пока кто-то мерит его цену. Мерит её сам гейт; этот страж
следит, чтобы механизм не сняли и не выхолостили.

ФОРМА ВЗЯТА У ОКНА 274 (`scripts/gate-novac.sh` в ветке `p274-novac`) и
перенацелена на главный гейт дерева: переменная яруса здесь `NOVA_GATE_TIER`,
судимый файл — `scripts/gate.sh`. Изобретать второй механизм под ту же задачу
значило бы завести две базы, которые разойдутся молча.

ПРОВЕРЯЕТ ЧЕТЫРЕ ВЕЩИ:
  1. файл бюджета есть и разбирается: строки `<ярус> <секунды>`;
  2. гейт ЧИТАЕТ его и умеет краснеть — в тексте гейта есть и чтение файла, и
     вызов `fail` по превышению. Механизм, который можно вырезать одной
     строкой и не заметить, — это не механизм;
  3. предел масштабируется калибровкой машины (`CAL`), иначе на медленной
     машине он краснеет на здоровом дереве, а на быстрой не ловит ничего;
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

NAME = "check-gate-budget"
RE_ROW = re.compile(r"^([a-z]+)[ \t]+([0-9]+)[ \t]*$")


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    gate = pathlib.Path(a[2]) if len(a) > 2 else root / "scripts" / "gate.sh"
    budget = root / "scripts" / "guards" / "gate-budget.baseline"

    if not gate.is_file():
        print(f"{NAME}: FAIL — нет гейта {gate}: судить бюджет нечему", file=sys.stderr)
        return 1
    if not budget.is_file():
        print(f"{NAME}: FAIL — нет файла бюджета {budget}: время гейта не ограничено ничем",
              file=sys.stderr)
        print("  Гейт без потолка времени возвращается к двадцати трём минутам за неделю —", file=sys.stderr)
        print("  это уже было (1401с, 2026-08-21). Заведи строки `<ярус> <секунды>` замером.", file=sys.stderr)
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
    if "gate-budget.baseline" not in text:
        missing.append("гейт не читает файл бюджета")
    if not re.search(r"GATE_ELAPSED", text):
        missing.append("гейт не меряет собственное время (нет GATE_ELAPSED)")
    if not re.search(r"BUDGET_LIMIT=\$\(\(\s*BUDGET \* CAL\s*\)\)", text):
        missing.append("предел не масштабируется калибровкой машины (BUDGET * CAL)")
    if not re.search(r'fail "ярус \$NOVA_GATE_TIER вышел за бюджет', text):
        missing.append("превышение бюджета не приводит к отказу (нет вызова fail)")
    if missing:
        print(f"{NAME}: FAIL — механизм бюджета выхолощен:", file=sys.stderr)
        for m in missing:
            print(f"  {m}", file=sys.stderr)
        print("  Механизм, который можно вырезать одной строкой и не заметить, — не механизм.", file=sys.stderr)
        print("  Верни проверку в гейт или объясни в Г1..Г7, чем она заменена.", file=sys.stderr)
        return 1

    # Ярусы, которые гейт ПРИНИМАЕТ: из его же строки разбора.
    tiers = set()
    m = re.search(r"NOVA_GATE_TIER\}?\"?\s*in\s*\n\s*([^)]*)\)", text)
    if m:
        tiers = {x.strip() for x in m.group(1).split("|") if x.strip()}
    if not tiers:
        print(f"{NAME}: FAIL — в {gate} не найден разбор NOVA_GATE_TIER: ярусов нет вовсе, "
              f"либо страж перестал их видеть (страж ни о чём — класс №519)", file=sys.stderr)
        return 1
    unjudged = sorted(tiers - set(rows))
    if unjudged and "строки бюджета для него нет (не судится)" not in text:
        print(f"{NAME}: FAIL — ярус без бюджета и без честного «не судится»: "
              f"{', '.join(unjudged)}", file=sys.stderr)
        return 1

    print(f"{NAME} ok: ярусов {len(tiers)}, с бюджетом {len(rows)} "
          f"({', '.join(f'{k} {v}с' for k, v in sorted(rows.items()))}), "
          f"без бюджета {len(unjudged)} (названы вслух), механизм в гейте на месте")
    return 0


if __name__ == "__main__":
    sys.exit(main())
