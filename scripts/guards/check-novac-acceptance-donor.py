# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-acceptance-donor.py — приёмка волны называет
ДОНОРА (П27-класс; замер 2026-08-22).

ЗАЧЕМ. Конвенция П27 требует донора в СООБЩЕНИИ КОММИТА, и страж
`check-novac-commit-donor` это держит. Но читают не коммиты, а план: приёмка
волны — то место, где через месяц смотрят, что волна дала. Замер 2026-08-22: из
девяти блоков приёмки донора называл ОДИН. И это не случайность конкретного дня —
про `TypeRows` я сравнил себя с донором только когда владелец спросил прямо, хотя
в коммите волны сравнение уже стояло.

Отсюда правило: вопрос «как это делает донор, и если мы отступили — какова цена»
становится ПОЛЕМ приёмки, а не воспоминанием. Восемь блоков заполнены не выдумкой
— донор каждой волны перенесён из её собственного коммита, поэтому план перестал
зависеть от `git log`, чтобы быть читаемым.

ПРАВИЛО. Каждый блок `**ПРИЁМКА…` в плане 274 обязан называть донора: слово
«донор»/`Donor` — либо ссылкой на эталон, либо честным «донор: нет» с причиной.
Правило ПЛОСКОЕ, а не храповик: в отличие от датированного замера, донора нельзя
«не помнить» — он либо есть в коммите волны, либо волна его не назвала и тогда
это дефект приёмки, а не наследство.

ЧЕГО НЕ ПРОВЕРЯЕТ: КАЧЕСТВО сравнения (эталон назван указателем или отговоркой —
судит ревью, названная слепая зона); приёмки в других планах (у семьи main свой
реестр).

$1 — корень; $2 — override плана (шов самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-acceptance-donor"

RE_ACC = re.compile(r"^\*\*ПРИЁМКА")
RE_HEAD = re.compile(r"^#### |^### |^## ")
RE_DONOR = re.compile(r"донор|Donor", re.I)


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    plan = pathlib.Path(a[2]) if len(a) > 2 else \
        root / "docs" / "plans" / "274-novac-self-hosted-compiler.md"

    if not plan.is_file():
        print(f"{NAME} ok: судить нечего (нет {plan})")
        return 0

    lines = plan.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n")

    blocks = []                      # (заголовок, строка, текст)
    cur = None
    for n, line in enumerate(lines, 1):
        if RE_ACC.match(line):
            if cur:
                blocks.append(cur)
            cur = [line[:56], n, line]
            continue
        if cur is not None:
            # приёмка кончается следующим заголовком раздела
            if RE_HEAD.match(line):
                blocks.append(cur)
                cur = None
            else:
                cur[2] += "\n" + line
    if cur:
        blocks.append(cur)

    if not blocks:
        print(f"{NAME}: FAIL — в плане нет ни одного блока '**ПРИЁМКА': страж потерял "
              f"мишень (класс №519)", file=sys.stderr)
        return 1

    bad = [f"  строка {ln}: {head}" for head, ln, text in blocks if not RE_DONOR.search(text)]

    if bad:
        print(f"{NAME}: FAIL — приёмка волны не называет донора:", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Читают не коммиты, а ПЛАН: приёмка — то место, где через месяц", file=sys.stderr)
        print("  смотрят, что волна дала. Вопрос «как это делает донор, и если мы", file=sys.stderr)
        print("  отступили — какова цена» обязан стоять здесь, а не всплывать по", file=sys.stderr)
        print("  вопросу владельца. Либо эталон назван, либо честное «донор: нет»", file=sys.stderr)
        print("  с причиной — как в сообщении коммита той же волны.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: блоков приёмки: {len(blocks)}, без названного донора: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
