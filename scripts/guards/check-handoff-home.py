# -*- coding: utf-8 -*-
"""scripts/guards/check-handoff-home.py — передача остановки лежит в РОЛЕВОЙ
ЗАПИСКЕ, а не в игнорируемом каталоге.

Адрес: план 290 п.3 (сохранение сессии), команды `/stop` и `/save` — дом правила; слово владельца 2026-09-18.

ЗАЧЕМ. `/stop` требовал писать передачу в `docs/.sessions/handoff-<id>.md`, а
этот каталог стоит в `.gitignore`. Написанное туда выполняет ровно ОДНУ из двух
задач передачи — пережить перезапуск СЕССИИ — и проваливает вторую: пережить
перезапуск МАШИНЫ и стать видимым владельцу и соседним окнам. Замер на день
правки: в каталоге лежали три файла, два — от окон, чья работа давно влита, и ни
один не видел никто, кроме написавшего его окна. Нашёл владелец, а не проверка.

ЧТО СУДИТСЯ — ДВЕ СТОРОНЫ, и по отдельности ни одна не достаточна.

  1. ФАКТ. В `docs/.sessions/` нет файлов `handoff-*.md`, ИЗМЕНЁННЫХ после даты
     правила. Это и есть предмет: если окно снова напишет передачу туда, файл
     появится, сколько бы правильных слов ни стояло в командах. Старые файлы —
     история, они не судятся: у них дата раньше правила.

  2. НАПИСАНИЕ. В `/stop` назван правильный дом (ролевая записка), и он назван
     ПРЕДПИСАНИЕМ, а не только в объяснении «почему не туда». Без этой половины
     страж молчал бы на команде, из которой правило кто-то вынул, — пока очередное
     окно не остановится и не запишет передачу мимо.

ПОЧЕМУ НЕ ГРЕП «`.sessions` В КОМАНДАХ ЗАПРЕЩЁН». Потому что упоминать его
командам НАДО: летопись правки объясняет, почему прежний адрес снят, и запрет на
слово стёр бы объяснение вместе с ошибкой. Судится не слово, а ПОВЕДЕНИЕ — файл в
каталоге — и наличие предписания.

ЧЕГО НЕ ПРОВЕРЯЕТ (сказано честно): что передача написана ХОРОШО, что в ней пять
частей и что написанное в ней правда; что окно вообще остановилось по `/stop`.
Это судит человек. Метки разделов судит соседний страж `check-handoff-labels`.

usage: python scripts/guards/check-handoff-home.py [КОРЕНЬ]
Самотест: scripts/guards/selftest/test-check-handoff-home.sh
"""
import io
import os
import sys

# Вывод стража читают из MSYS-консоли в cp866: без этого русский текст
# приезжает мусором, и отказ становится нечитаемым ровно тогда, когда он нужен.
sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-handoff-home"

# Дата правки владельца; файлы старше неё — история, они не судятся.
#
# ЧИСЛО ПОЛУЧЕНО МАШИНОЙ, а не в уме: первая версия несла 1758153600, что есть
# 18 сентября 2025 ГОДА, — и страж покраснел на двух файлах, которые ровно на
# год старше правила. Ошибка в год не выглядит ошибкой: число правдоподобно, и
# поймал его только прогон.
RULE_EPOCH = 1789689600  # calendar.timegm((2026, 9, 18, 0, 0, 0, 0, 0, 0))

# Предписание, которое обязано стоять в /stop. Ищется по СМЫСЛОВОМУ ядру, а не
# по всей фразе: текст команды правится, и страж, привязанный к целому абзацу,
# покраснел бы на первой же редактуре.
STOP_MUST_SAY = "РОЛЕВУЮ ЗАПИСКУ"


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."

    stale = []
    sdir = os.path.join(root, "docs", ".sessions")
    if os.path.isdir(sdir):
        for name in sorted(os.listdir(sdir)):
            if not (name.startswith("handoff-") and name.endswith(".md")):
                continue
            p = os.path.join(sdir, name)
            try:
                mt = os.path.getmtime(p)
            except OSError:
                continue
            if mt > RULE_EPOCH:
                stale.append(name)

    stop_path = os.path.join(root, ".claude", "commands", "stop.md")
    says = False
    if os.path.isfile(stop_path):
        text = io.open(stop_path, encoding="utf-8", errors="replace").read()
        says = STOP_MUST_SAY in text

    bad = False

    if stale:
        print("%s: передача написана в ИГНОРИРУЕМЫЙ каталог — файлов %d"
              % (NAME, len(stale)), file=sys.stderr)
        for n in stale:
            print("    docs/.sessions/%s" % n, file=sys.stderr)
        print("    Этот каталог под .gitignore: написанное туда не переживает",
              file=sys.stderr)
        print("    ни клон, ни перезапуск машины и не видно никому, кроме",
              file=sys.stderr)
        print("    самого окна. Перенеси раздел в свою ролевую записку",
              file=sys.stderr)
        print("    (docs/dev/prompts/*-handoff.md), метка `## 0-ГГГГ-ММ-ДД`,",
              file=sys.stderr)
        print("    в заголовке слово ОСТАНОВКА. Дом правила — /stop, /save.",
              file=sys.stderr)
        bad = True

    if not says:
        print("%s: /stop больше НЕ ПРЕДПИСЫВАЕТ писать в ролевую записку"
              % NAME, file=sys.stderr)
        print("    Искали в .claude/commands/stop.md слова «%s»."
              % STOP_MUST_SAY, file=sys.stderr)
        print("    Правило, вынутое из команды, замечается только тогда,",
              file=sys.stderr)
        print("    когда следующее окно уже записало передачу мимо.",
              file=sys.stderr)
        bad = True

    if bad:
        print("%s: FAIL" % NAME, file=sys.stderr)
        return 1

    print("%s ok: свежих передач в игнорируемом каталоге нет, "
          "и /stop называет ролевую записку" % NAME)
    return 0


if __name__ == "__main__":
    sys.exit(main())
