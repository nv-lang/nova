#!/usr/bin/env python3
"""guard-memory.py — PreToolUse-хук Claude Code (Write): блокирует запись
feedback-заметки памяти без обязательного поля `enforcement:`.

ПОЧЕМУ. Правило поддержания плана 231.2 §3 (docs/plans/231.2-enforcement-
infra.md): «каждый новый инцидент класса „забыли правило" обязан
заканчиваться не заметкой в память, а строкой — какой МЕХАНИЗМ (хук/линт/
гейт/ratchet) делает повторение невозможным». Без принуждения это тоже
норма-пожелание: заметка-без-механизма фиксирует симптом, а не чинит его
источник (рекурсия того же принципа «норма живёт долгосрочно только в
исполняемой форме», применённая к самому себе).

ЧТО ПРОВЕРЯЕТ. Перехватывает Write, чей `file_path` матчит
`memory/feedback-*.md` (regex `_TARGET`). Если `content` НЕ содержит
подстроку `enforcement:` — блокирует (exit 2 + причина в stderr,
на латинице/транслите, т.к. это попадает в лог хука). Требуемая форма
поля — `enforcement: <механизм — хук/линт/гейт/ratchet, строка в 231.2>`
либо честное `enforcement: немашинное — <причина>`, если для инцидента
машинного стража нет и не планируется.

Файлы вне `memory/feedback-*.md` (обзоры, reference-заметки и т.п.)
пропускаются без проверки — правило касается только feedback-заметок.

ИСПОЛЬЗОВАНИЕ. Не запускается вручную — подключается декларативно через
локальный (НЕ в git) `.claude/settings.json`:
    hooks.PreToolUse[].matcher = "Write"
    → command: python scripts/claude-hooks/guard-memory.py
Хук получает JSON вызова инструмента через stdin (`tool_input.file_path`,
`tool_input.content`). Самотеста в scripts/selftest/ ПОКА НЕТ — план 231
§4в: «Write без enforcement: (блок), с полем (пропуск)» — см. таблицу
покрытия в scripts/selftest/README.md.
"""
from __future__ import annotations

import json
import re
import sys

_TARGET = re.compile(r"memory[\\/]+feedback-[^\\/]*\.md$", re.IGNORECASE)


def main() -> int:
    try:
        data = json.loads(sys.stdin.read() or "{}")
        ti = data.get("tool_input") or {}
        path = ti.get("file_path") or ""
        content = ti.get("content") or ""
    except Exception:
        return 0
    if not _TARGET.search(path):
        return 0
    if "enforcement:" in content:
        return 0
    print(
        "231.2 §3: feedback-zametka bez polya 'enforcement:'. Dobav v telo stroku "
        "'enforcement: <mekhanizm — huk/lint/gate/ratchet, stroka v 231.2>' "
        "libo 'enforcement: nemashinnoe — <prichina>'. Zametka-bez-mekhanizma = simptom, "
        "ne fiks (plan 231.2).",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    sys.exit(main())
