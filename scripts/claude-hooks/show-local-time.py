# -*- coding: utf-8 -*-
"""scripts/claude-hooks/show-local-time.py — МЕСТНОЕ ВРЕМЯ САМО ПРИЕЗЖАЕТ В ОКНО.

Адрес: `AGENTS.md`, раздел «Время»; команды `/flow`, `/integrator`, `/carina`.

ЗАЧЕМ — замер владельца 2026-09-18, и он о провале ПРАВИЛА, а не о забывчивости.
Правило «время берётся командой `date`, а не из головы» записано в четырёх местах
и приезжает в каждое окно. Интегратор его исполнял: запускал `date`, получал
`15:25` — и писал в докладе `17:54`. То есть замер был СНЯТ и ПРОИГНОРИРОВАН:
между командой и строкой доклада окно опирается на память, и память уезжает
вперёд на часы. За смену владелец поправил это ДВАЖДЫ.

ПОЧЕМУ НЕ ПОМОГАЕТ ЕЩЁ ОДНО ПРАВИЛО. Правило требует ДЕЙСТВИЯ в нужный момент, а
момент — написание текста, где никаких команд не запускают. Механизм должен
делать факт доступным БЕЗ действия, тогда «вспомнить» становится не нужно.

ЧТО ДЕЛАЕТ. После вызова Bash подаёт в контекст одну строку с местным временем.
С ОСТУДОЙ: не чаще раза в `COOLDOWN_SEC`, иначе строка идёт после каждой команды
и превращается в шум, который перестают читать (тот же износ, что у любого
слишком частого предупреждения).

ЧЕГО НЕ ДЕЛАЕТ (сказано честно): не проверяет, ЧТО окно написало в докладе —
текст сообщений хукам недоступен. Это подача факта, а не контроль исполнения;
контроль остаётся за человеком, и именно поэтому владелец ловил ошибку дважды.

usage: вызывается Claude Code как PostToolUse-хук; вручную — для самотеста.
Самотест: scripts/claude-hooks/selftest/test-show-local-time.py
"""
import io
import json
import os
import sys
import time

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")

COOLDOWN_SEC = 120


def stamp_path(root):
    """Отметка последней подачи — в СБОРОЧНОМ каталоге дерева.

    Не в `%TEMP%`: реестр 221.1 №1153 — состояние, живущее во временном
    каталоге, система чистит молча, и тогда остуда перестаёт работать в
    сторону ШУМА (строка пойдёт после каждой команды).
    """
    return os.path.join(root, "target", ".show-local-time")


def main():
    try:
        sys.stdin.read()
    except Exception:
        pass
    # ОТМЕТКА ОСТУДЫ — В ДЕРЕВЕ СВОЕЙ СЕССИИ (реестр 221.1 №1156, второй
    # носитель класса). `CLAUDE_PROJECT_DIR` один на все окна, а окна работают
    # в разных worktree — значит отметку одного окна читало бы другое, и время
    # не подавалось бы тому, кому оно нужно. Остуда личная по смыслу, поэтому
    # и место у неё личное.
    try:
        sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
        from hook_tree import session_root
        root = session_root()
    except Exception:
        root = os.environ.get("CLAUDE_PROJECT_DIR") or os.getcwd()

    p = stamp_path(root)
    now = time.time()
    try:
        if os.path.isfile(p) and now - os.path.getmtime(p) < COOLDOWN_SEC:
            return 0
    except OSError:
        pass

    try:
        os.makedirs(os.path.dirname(p), exist_ok=True)
        io.open(p, "w", encoding="utf-8").write("%d\n" % int(now))
    except OSError:
        pass          # не смогли записать отметку — подаём строку, но не молчим

    local = time.strftime("%H:%M")
    msg = (u"ВРЕМЯ СЕЙЧАС %s (местное, снято машиной). "
           u"Доклад начинается С ЭТОГО значения, не из памяти: "
           u"замер владельца 2026-09-18 — окно запустило `date`, получило 15:25 "
           u"и написало 17:54." % local)
    print(json.dumps({
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": msg,
        }
    }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
