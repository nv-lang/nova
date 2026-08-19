# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-conventions-coverage.py — у каждого правила
конвенции назван механизм.

ПРАВИЛО. Раздел `## ПNN.` обязан назвать ЛИБО своего стража (имя файла
`check-*.sh` или `check-*.py`), ЛИБО честно объявить себя немашинным — знаком ⚖
или словами «судится приёмкой». Правило, не сделавшее ни того ни другого,
невидимо реестру стражей ЦЕЛИКОМ: оно не попадает ни в одно из его четырёх
множеств, и никто не замечает, что его никто не проверяет.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override пути к конвенциям (шов самотеста).
"""
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


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    conv = pathlib.Path(a[2]) if len(a) > 2 else root / "docs" / "dev" / "novac-compiler-conventions.md"

    if not conv.is_file():
        print(f"{NAME} ok: судить нечего (нет {conv})")
        return 0

    rules = []                       # (имя, строка, покрыто)
    rule, line_no, covered = "", 0, False
    for n, line in enumerate(conv.read_text(encoding="utf-8", errors="replace")
                             .replace("\r", "").split("\n"), 1):
        if RE_RULE.match(line):
            if rule:
                rules.append((rule, line_no, covered))
            rule = re.sub(r"\..*$", "", line[3:])
            line_no, covered = n, False
            continue
        if rule and not covered:
            if RE_GUARD.search(line) or RE_MANUAL.search(line):
                covered = True
    if rule:
        rules.append((rule, line_no, covered))

    if not rules:
        print(f"{NAME}: FAIL — в {conv} не нашлось ни одного раздела вида '## ПNN.': "
              f"страж потерял мишень (класс №519)", file=sys.stderr)
        return 1

    bad = [f"  {r} (строка {ln}) — механизм не назван" for r, ln, c in rules if not c]
    if bad:
        print(f"{NAME}: FAIL — правило конвенции без названного механизма:", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Каждое правило обязано назвать ЛИБО своего стража (имя файла check-*.sh),", file=sys.stderr)
        print("  ЛИБО честно объявить себя немашинным (знак ⚖ или словами «судится приёмкой»).", file=sys.stderr)
        print("  Правило, которое не сделало ни того ни другого, невидимо реестру стражей", file=sys.stderr)
        print("  целиком — оно не попадает ни в одно из его четырёх множеств.", file=sys.stderr)
        return 1

    guarded = sum(1 for _r, _l, c in rules if c)
    print(f"{NAME} ok: правил конвенции: {len(rules)}, у всех назван механизм ({guarded}), "
          f"без механизма: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
