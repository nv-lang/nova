#!/usr/bin/env python3
"""PreToolUse hook (Bash + PowerShell): машинное принуждение стоячих git-правил
(план 231 трек Д п.6 — правила из памяти/конвенций переезжают в перехватчик).

Блокирует (exit 2 + причина):
  - git config ... user.*        (урок: 349 коммитов под «Claude Haiku»)
  - git add -A | git add . | git add --all   (конвенция: add только по именам)
  - git stash                    (worktree делят .git — конвенция запрещает)

Fail-open по ошибкам самого хука (exit 0), fail-closed по правилам.
"""
from __future__ import annotations

import json
import re
import sys

RULES = [
    # ЗАПИСЬ user.* (со значением) — чтение `git config user.name` разрешено
    # (иначе ложные срабатывания на heredoc-текстах и проверках авторства).
    (re.compile(r"\bgit\b[^|;&\n]*\bconfig\b[^|;&\n]*\buser\.(name|email)\s+\S", re.IGNORECASE),
     "FORBIDDEN: git config user.* write — avtorstvo pravit tolko vladelets vruchnuyu "
     "(urok 2026-07-25: 349 commitov pod 'Claude Haiku' cherez obshchiy .git worktree)."),
    (re.compile(r"\bgit\b[^|;&\n]*\badd\b\s+(-A\b|--all\b|\.(\s|$))", re.IGNORECASE),
     "FORBIDDEN: git add -A/--all/. — tolko po imenam faylov (konventsiya)."),
    (re.compile(r"\bgit\b[^|;&\n]*\bstash\b", re.IGNORECASE),
     "FORBIDDEN: git stash — worktree delyat .git (konventsiya: temp-commit/reset)."),
]


_QUOTED = re.compile(r"'[^']*'|\"[^\"]*\"", re.DOTALL)
_HEREDOC = re.compile(r"<<-?\s*'?(\w+)'?.*?\n\1\b", re.DOTALL)


def main() -> int:
    try:
        data = json.loads(sys.stdin.read() or "{}")
        cmd = (data.get("tool_input") or {}).get("command") or ""
    except Exception:
        return 0
    # Матчим только ИСПОЛНЯЕМУЮ часть: литеральный текст в кавычках/heredoc
    # (коммит-сообщения, содержимое скриптов) — не команды (ложняки ×2 доказаны).
    stripped = _HEREDOC.sub(" ", cmd)
    stripped = _QUOTED.sub(" ", stripped)
    for rx, msg in RULES:
        if rx.search(stripped):
            print(msg, file=sys.stderr)
            return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
