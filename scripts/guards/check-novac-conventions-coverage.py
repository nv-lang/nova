# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-conventions-coverage.py — у каждого правила
конвенции назван механизм И назван замер, его породивший.

ПРАВИЛО A (механизм). Раздел `## ПNN.` обязан назвать ЛИБО своего стража (имя
файла `check-*.sh` или `check-*.py`), ЛИБО честно объявить себя немашинным —
знаком ⚖ или словами «судится приёмкой». Правило, не сделавшее ни того ни
другого, невидимо реестру стражей ЦЕЛИКОМ: оно не попадает ни в одно из его
четырёх множеств, и никто не замечает, что его никто не проверяет.

ПРАВИЛО B (замер) — заведено 2026-08-22 по замеру НА СЕБЕ. В тот день я записал
в план правило «`Option` на границе двери, сентинел внутри обхода», выведенное
рассуждением, и пошёл приводить код в соответствие: правило оказалось неверным
через час — `?? continue` язык отвергает (D86), а `?` требует обёртки для
проброса, которой у обхода нет. Правило из рассуждения выглядит так же, как
правило из замера, и отличить их читателю нечем — поэтому раздел обязан нести
ДАТУ замера или слово «замер».

ЭТО ХРАПОВИК, А НЕ ПЛОСКОЕ ПРАВИЛО. Замер 2026-08-22: из 34 правил замер несут
26, восемь (П1..П10) написаны до этой дисциплины. Требовать замер задним числом
значило бы либо соврать датой, либо остановить работу — поэтому база 8 и
движение только вниз: новое правило приезжает с замером, старое получает его,
когда до него дойдёт волна.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override пути к конвенциям (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-conventions-coverage"
RE_RULE = re.compile(r"^## П[0-9]+\.")
RE_GUARD = re.compile(r"check-[a-z0-9-]+\.(sh|py)")
RE_MANUAL = re.compile(r"⚖|немашинн|не формализуем|формализовать .* нельзя|"
                       r"судится приёмкой|на ревью|красные на ревью")
# Замер: датой (`2026-08-22`) или словом. Слово принимается потому, что часть
# замеров лежит рядом, в плане и реестре расхождений, и дублировать дату в двух
# местах значило бы завести вторую копию, которая разойдётся (класс К4).
RE_MEASURED = re.compile(r"20[0-9]{2}-[0-9]{2}-[0-9]{2}|замер|измерен")


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    conv = pathlib.Path(a[2]) if len(a) > 2 else root / "docs" / "dev" / "novac-compiler-conventions.md"

    if not conv.is_file():
        print(f"{NAME} ok: судить нечего (нет {conv})")
        return 0

    rules = []                       # (имя, строка, покрыто, замерено)
    rule, line_no, covered, measured = "", 0, False, False
    for n, line in enumerate(conv.read_text(encoding="utf-8", errors="replace")
                             .replace("\r", "").split("\n"), 1):
        if RE_RULE.match(line):
            if rule:
                rules.append((rule, line_no, covered, measured))
            rule = re.sub(r"\..*$", "", line[3:])
            line_no, covered, measured = n, False, False
            continue
        if rule and not covered:
            if RE_GUARD.search(line) or RE_MANUAL.search(line):
                covered = True
        if rule and not measured and RE_MEASURED.search(line):
            measured = True
    if rule:
        rules.append((rule, line_no, covered, measured))

    if not rules:
        print(f"{NAME}: FAIL — в {conv} не нашлось ни одного раздела вида '## ПNN.': "
              f"страж потерял мишень (класс №519)", file=sys.stderr)
        return 1

    bad = [f"  {r} (строка {ln}) — механизм не назван" for r, ln, c, _m in rules if not c]
    if bad:
        print(f"{NAME}: FAIL — правило конвенции без названного механизма:", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Каждое правило обязано назвать ЛИБО своего стража (имя файла check-*.sh),", file=sys.stderr)
        print("  ЛИБО честно объявить себя немашинным (знак ⚖ или словами «судится приёмкой»).", file=sys.stderr)
        print("  Правило, которое не сделало ни того ни другого, невидимо реестру стражей", file=sys.stderr)
        print("  целиком — оно не попадает ни в одно из его четырёх множеств.", file=sys.stderr)
        return 1

    unmeasured = [f"  {r} (строка {ln})" for r, ln, _c, m in rules if not m]
    # Шов для самотеста (реестр №817): база — храповик по НАСТОЯЩЕМУ файлу
    # конвенций, а самотест подсовывает крошечные синтетические файлы через
    # шов $2. Без второго шва его фикстуры судились базой живого дерева и
    # краснели на обеих ветках («правил без замера стало МЕНЬШЕ: 2 при базе
    # 8») — фикстура задавала файл, но не задавала то, с чем её сравнивают.
    # Тот же класс, что №816 у стража сетки охоты.
    env_base = os.environ.get("NOVA_RULE_MEASURE_BASELINE")
    base_file = (pathlib.Path(env_base) if env_base
                 else root / "scripts" / "guards" / "novac-rule-measure.baseline")
    base = None
    if base_file.is_file():
        for bl in base_file.read_text(encoding="utf-8", errors="replace").split("\n"):
            bl = bl.strip()
            if bl and not bl.startswith("#"):
                base = int(bl)
                break
    if base is None:
        print(f"{NAME}: FAIL — нет базы {base_file.name}: храповик замеров потерял отсчёт "
              f"(класс №519)", file=sys.stderr)
        return 1
    if len(unmeasured) > base:
        print(f"{NAME}: FAIL — правил без НАЗВАННОГО ЗАМЕРА стало больше "
              f"({len(unmeasured)} при базе {base}):", file=sys.stderr)
        for b in unmeasured:
            print(b, file=sys.stderr)
        print("  Правило из рассуждения выглядит как правило из замера, и читателю", file=sys.stderr)
        print("  их нечем отличить. Замер 2026-08-22 на себе: правило про `Option`", file=sys.stderr)
        print("  и сентинел, выведенное рассуждением, оказалось неверным через час.", file=sys.stderr)
        print("  Новое правило приезжает С ДАТОЙ замера или со словом «замер».", file=sys.stderr)
        return 1
    if len(unmeasured) < base:
        print(f"{NAME}: FAIL — правил без замера стало МЕНЬШЕ ({len(unmeasured)} при базе "
              f"{base}): опусти базу тем же слиянием, иначе следующий рост до прежней "
              f"цифры пройдёт молча", file=sys.stderr)
        return 1

    guarded = sum(1 for _r, _l, c, _m in rules if c)
    measured_n = len(rules) - len(unmeasured)
    print(f"{NAME} ok: правил конвенции: {len(rules)}, у всех назван механизм ({guarded}), "
          f"с названным замером: {measured_n}, без замера: {len(unmeasured)} (== база)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
