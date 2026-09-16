# -*- coding: utf-8 -*-
"""scripts/guards/check-commands-no-state.py — в слэш-команде не должно быть
СОСТОЯНИЯ: команда живёт долго, состояние протухает молча.

Адрес: план 290 пункт 4 (ревизия `.claude`), реестр 221.1 — без номера: находка исправлена тем же слиянием, строка не заводилась.
Дом правила: `docs/dev/claude-commands.md`, раздел «КОМАНДА РАССЧИТАНА НА ДОЛГИЙ
ГОРИЗОНТ». Требование владельца 2026-09-16: «все команды должны быть рассчитаны
на длительный горизонт, а не на сиюминутные задачи».

ЗАЧЕМ. Команда, несущая сегодняшнее состояние, становится ВТОРЫМ ДОМОМ статуса,
а второй дом расходится с первым на первой правке. Прецедент дня заведения: в
`/carina` стоял список «Открытые сегодня: 274.5, 274.6, 274.7…» и номера строк
реестра «№1130–№1133, ещё не взято»; ревизия нашла пять таких мест в трёх
командах, включая «обычное состояние этой недели».

ЧТО СУДИТСЯ — ТОЛЬКО ЯВНЫЙ СПИСОК ОБОРОТОВ, И ЭТО НАМЕРЕННО. Отличить «замер как
обоснование» от «состояния как указания» машина не может: и то и другое —
проза. Поэтому здесь не эвристика по смыслу, а перечень конкретных ФОРМ, каждая
из которых уже ловилась в дереве. Список растёт по мере находок, а не по
фантазии: страж, ловящий воображаемое, даёт ложняки и его отключают.

ЧЕГО НЕ ПРОВЕРЯЕТ (сказано честно): что команда вообще осмысленна, что её текст
не устарел иначе (переименованный файл, снятый план), и что датированный замер в
ней уместен. Это суждения человека.

БАЗА НОЛЬ И ЭТО ЗАКОННО: все пять находок исправлены в тот же день, значит
начинать можно с чистого листа — в отличие от стражей, натравленных на историю.

$1 — корень репозитория.
"""
import glob
import io
import os
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-commands-no-state"

# Каждая форма — из РЕАЛЬНОЙ находки 2026-09-16, а не придумана.
FORMS = [
    (re.compile(u"открыт[ыои][ех]?\\s+сегодня", re.I),
     u"список открытого «сегодня» — протухнет на первой закрытой волне"),
    (re.compile(u"обычное состояние (этой|на этой) недел", re.I),
     u"«состояние этой недели» — команда правится раз в неделю"),
    (re.compile(u"на день написания команды", re.I),
     u"расклад на дату написания — читается как действующий"),
    (re.compile(u"ещё не взят[оы]|до сих пор не взят", re.I),
     u"утверждение о невзятой работе — это статус, а не правило"),
    (re.compile(u"\\*\\*\\s*\\d+\\s+коммит[а-я]*\\s+позади", re.I),
     u"счёт коммитов как факт — меняется каждым пушем"),
    (re.compile(u"сейчас красн[ыо]|красен тремя|красны сейчас", re.I),
     u"перечень текущей красноты — живёт часы"),
]

# Строки, которым форма СТАТУСА позволена: они говорят ПРО правило, а не про мир.
EXEMPT = re.compile(u"ЗАПРЕЩЕНО|протух|второй дом|пример формы|НЕ должно|не должно быть")


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    cmds = os.path.join(root, ".claude", "commands")
    if not os.path.isdir(cmds):
        print("%s: FAIL - net kataloga %s" % (NAME, cmds), file=sys.stderr)
        return 1

    bad = []
    judged = 0
    for path in sorted(glob.glob(os.path.join(cmds, "*.md"))):
        judged += 1
        name = os.path.basename(path)
        text = io.open(path, encoding="utf-8", errors="replace").read()
        for i, line in enumerate(text.splitlines(), 1):
            if EXEMPT.search(line):
                continue
            for rx, why in FORMS:
                if rx.search(line):
                    bad.append((name, i, why, line.strip()[:90]))
                    break

    print("%s: komand %d, form sostoyaniya %d (baza 0)" % (NAME, judged, len(bad)))

    if bad:
        print("%s: NARUSHENIE - v komande SOSTOYANIE, a ne pravilo:" % NAME,
              file=sys.stderr)
        for n, i, why, line in bad:
            print("    %s:%d  %s" % (n, i, why), file=sys.stderr)
            print("        %s" % line, file=sys.stderr)
        print("", file=sys.stderr)
        print("    Komanda zhivet dolgo, sostoyanie protuhaet. Vmesto stroki s",
              file=sys.stderr)
        print("    sostoyaniem stavitsya KOMANDA, kotoraya ego dobyvaet.",
              file=sys.stderr)
        print("    Proverka odnoj frazoj: pridetsya li pravit' etu stroku, kogda",
              file=sys.stderr)
        print("    zakroetsya sleduyushchaya volna? Da - znachit tam nuzhna komanda.",
              file=sys.stderr)
        print("    Dom pravila: docs/dev/claude-commands.md", file=sys.stderr)
        print("%s: FAIL" % NAME, file=sys.stderr)
        return 1

    print("%s ok: komandy nesut pravila i komandy, a ne segodnyashnee sostoyanie"
          % NAME)
    return 0


if __name__ == "__main__":
    sys.exit(main())
