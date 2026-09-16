# -*- coding: utf-8 -*-
"""scripts/claude-hooks/remind-session-save.py — записка интегратора обязана
обновляться РАЗ В ЧАС, и следит за этим механизм, а не память окна.

ЗАЧЕМ. 2026-09-16 владелец спросил, где и как часто сохраняется контекст сессии.
Ответ был плохим: записка `docs/dev/prompts/integrator-handoff.md` не
обновлялась СЕМНАДЦАТЬ ЧАСОВ работы — правило существовало, механизма не было.
Правило без механизма есть симптом, а не фикс (план 290 пункт 3).

ЧТО ДЕЛАЕТ. После вызова инструмента смотрит возраст записки. Старше часа —
печатает напоминание с возрастом и командой. Не блокирует НИЧЕГО: напоминание,
ставшее препятствием, выключат первым же днём, и правило исчезнет вместе с ним.

ПОЧЕМУ ВОЗРАСТ ФАЙЛА, А НЕ СВОЙ СЧЁТЧИК. Счётчик живёт в процессе окна и умирает
вместе с ним — то есть врёт ровно в том случае, ради которого всё затевалось
(сессия оборвалась). Возраст файла принадлежит ДЕРЕВУ и переживает любое окно.

ПОЧЕМУ С ОСТУДОЙ. Напоминание на каждый вызов инструмента — это шум, а шум
читают как фон. Остуда хранится в файле, а не в памяти: то же рассуждение.

ТИХИЙ ПО УМОЛЧАНИЮ: если записки нет (чужой проект, свежий клон) — молчит.
"""
import io
import json
import os
import sys
import time

# Вывод — ВСЕГДА UTF-8, а не кодировка консоли. Поймано СОБСТВЕННЫМ самотестом:
# на Windows stdout по умолчанию cp1251, и русский текст доезжал искажённым — то
# есть напоминание было бы нечитаемым ровно там, где оно нужно. Тот же класс,
# что ловит guard-shell-nonascii, и ровно та причина, по которой страж требует
# самотест у каждого хука: руками хуки не гоняют.
sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")

HANDOFF = os.path.join("docs", "dev", "prompts", "integrator-handoff.md")
MAX_AGE_SEC = 3600            # час — число владельца (план 290 п.3)
COOLDOWN_SEC = 900            # не чаще раза в 15 минут

def main():
    try:
        sys.stdin.read()
    except Exception:
        pass
    root = os.environ.get("CLAUDE_PROJECT_DIR") or os.getcwd()
    path = os.path.join(root, HANDOFF)
    if not os.path.isfile(path):
        return 0

    age = time.time() - os.path.getmtime(path)
    if age < MAX_AGE_SEC:
        return 0

    stamp = os.path.join(root, "target", ".session-save-reminder")
    try:
        if os.path.isfile(stamp) and time.time() - os.path.getmtime(stamp) < COOLDOWN_SEC:
            return 0
    except OSError:
        pass

    hours = int(age // 3600)
    mins = int((age % 3600) // 60)
    msg = (
        u"ЗАПИСКА ИНТЕГРАТОРА НЕ ОБНОВЛЯЛАСЬ %dч %02dм (предел — час).\n"
        u"  Сохрани контекст: команда /save. Она дописывает раздел состояния\n"
        u"  СВЕРХУ в %s, беря факты из дерева.\n"
        u"  Почему это не пожелание: 2026-09-16 семнадцать часов работы не были\n"
        u"  записаны нигде, кроме истории git (план 290 п.3)."
    ) % (hours, mins, HANDOFF)

    try:
        os.makedirs(os.path.dirname(stamp), exist_ok=True)
        io.open(stamp, "w", encoding="utf-8").write(str(time.time()))
    except OSError:
        pass

    print(json.dumps({
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": msg,
        }
    }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
