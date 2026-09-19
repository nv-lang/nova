#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""scripts/guards/check-carina-no-idle-wait.py — правило «ожидание не есть работа»
обязано стоять в команде входа окна Карины.

ПРАВИЛО (владелец, 2026-09-17, дословный вопрос: «ты можешь не ждать интегратора, а
делать др работу? запиши это в команду и самотест»). Окно, стоящее в ожидании чужого
гейта, номера или слова, тратит полчаса общей смены и не производит ничего. Очередь при
этом НЕ пуста — пуста только та её часть, которой нужна машина.

ЧТО ЭТОТ СТРАЖ СУДИТ, И ЧЕГО ОН НЕ МОЖЕТ — названо первым, а не спрятано. Он судит
НАЛИЧИЕ ПРАВИЛА В КОМАНДЕ, а не поведение окна: простой не оставляет следа в дереве, и
измерить его извне нечем. Это тот самый обмен, который у нас записан строками 1140 и
1143 (мера судит имя, а не свойство), и здесь он сделан СОЗНАТЕЛЬНО, потому что
измеримая половина всё же есть и она важная: правило должно ПЕРЕЖИВАТЬ ПЕРЕЗАПУСК
ОКНА. Память сессии его не переживает, команда — переживает; значит «правило стоит в
команде» есть проверяемое условие того, что оно вообще дойдёт до следующего окна.

ЧТО ИМЕННО ПРОВЕРЯЕТСЯ:
  A. файл команды существует (`.claude/commands/carina.md`);
  B. в нём есть ЗАГОЛОВОК правила — ключевая фраза `ОЖИДАНИЕ НЕ ЕСТЬ РАБОТА`;
  C. рядом с ним названы ОБЕ половины: когда правило срабатывает (чужой гейт, номер,
     слово владельца) и ЧТО брать вместо ожидания. Заголовок без второй половины —
     лозунг: следующее окно прочтёт «не жди» и не узнает, чем занять паузу.

ПОЧЕМУ ПРОВЕРЯЕТСЯ ФРАЗА, А НЕ ПЕРЕСКАЗ: страж, ищущий пересказ, краснеет от любой
переформулировки, и его отключают. Пиннится КЛЮЧЕВАЯ ФРАЗА и ничего длиннее — тот же
приём, что у `typeref_rules.nv` с `undeclared type name` (урок №959).

Запуск: python scripts/guards/check-carina-no-idle-wait.py [<корень>]
Коды: 0 — правило на месте; 1 — нет правила либо нет второй половины.
"""
import io
import os
import sys

NAME = "check-carina-no-idle-wait"
CMD = os.path.join(".claude", "commands", "carina.md")

HEAD = "ОЖИДАНИЕ НЕ ЕСТЬ РАБОТА"
# Вторая половина: ХОТЯ БЫ ОДНО из слов про повод ждать, и хотя бы одно про замену.
WHEN = ("гейт", "номер", "слова владельца", "оракул")
INSTEAD = ("чтения", "без прогона", "реестр", "страж")


def main(argv):
    root = argv[1] if len(argv) > 1 else "."
    path = os.path.join(root, CMD)
    if not os.path.isfile(path):
        # Дерево без команды Карины (пакетная репа, срез) — судить нечего.
        if not os.path.isdir(os.path.join(root, ".claude", "commands")):
            print("%s ok: sudit nechego (net .claude/commands)" % NAME)
            return 0
        print("%s: FAIL -- net %s, a komanda Kariny objazana nesti pravilo" % (NAME, CMD),
              file=sys.stderr)
        return 1

    text = io.open(path, encoding="utf-8", errors="replace").read()
    if HEAD not in text:
        sys.stderr.write(
            "%s: FAIL -- v %s net pravila 'ozhidanie ne est rabota'.\n"
            "  Vladelec 2026-09-17: okno ne zhdet chuzhogo verdikta, a beret rabotu bez mashiny.\n"
            "  Pravilo zhivet v KOMANDE, potomu chto pamyat sessii ne perezhivaet perezapusk.\n" % (NAME, CMD))
        return 1

    when = [w for w in WHEN if w in text]
    instead = [w for w in INSTEAD if w in text]
    if not when or not instead:
        sys.stderr.write(
            "%s: FAIL -- zagolovok pravila est, a vtoroy poloviny net.\n"
            "  povodov zhdat nazvano: %d; chem zanyat pauzu nazvano: %d\n"
            "  Zagolovok bez 'chto brat vmesto' -- lozung: sleduyushchee okno prochtet\n"
            "  'ne zhdi' i ne uznaet, chem zanyat pauzu.\n" % (NAME, len(when), len(instead)))
        return 1

    print("%s ok: pravilo na meste (povodov zhdat %d, zamen nazvano %d); "
          "strazh sudit NALICHIE pravila, ne povedenie okna" % (NAME, len(when), len(instead)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
