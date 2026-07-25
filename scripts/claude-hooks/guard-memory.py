#!/usr/bin/env python3
"""PreToolUse hook (Write): принуждение правила поддержания 231.2 §3.

Новая feedback-заметка в памяти обязана нести поле `enforcement:` —
какой МЕХАНИЗМ (хук/линт/гейт/ratchet, строка в 231.2) делает повторение
инцидента невозможным, либо честное `enforcement: немашинное — <причина>`.

Заметка без поля не запишется: напоминание приходит ровно в момент
нарушения, а не из памяти интегратора (рекурсивное замыкание принципа
«норма живёт только в исполняемой форме»).
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
