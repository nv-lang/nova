#!/usr/bin/env python3
"""guard-git.py — PreToolUse-хук Claude Code (Bash + PowerShell): блокирует
запрещённые git-команды ДО их исполнения.

ПОЧЕМУ. Правила «git config user.* правит только владелец руками», «git add
только по именам файлов», «git stash запрещён (worktree делят один .git)»
раньше жили только в тексте памяти/конвенций — соблюдение зависело от того,
вспомнит ли агент их в моменте. Измеренная цена промаха: 349 коммитов ушли
под авторством «Claude Haiku» через общий .git нескольких worktree
(инцидент 2026-07-25) — правку авторства по всей истории пришлось делать
отдельной волной. План 231 трек Д п.6 (docs/plans/231-bug-cycle-exit.md,
«правила из памяти/конвенций переезжают в перехватчик») + исполнительный
дом docs/plans/231.2-enforcement-infra.md §1.

ЧТО ПРОВЕРЯЕТ (RULES ниже; совпадение → exit 2 + причина в stderr):
  - `git config ... user.name|user.email` С ЗАПИСЬЮ значения (голое чтение
    `git config user.name` разрешено — иначе ложные срабатывания на штатных
    проверках авторства перед коммитом);
  - `git add -A` / `git add .` / `git add --all` (конвенция: добавлять
    только по именам файлов);
  - `git stash` (worktree этой репы делят один `.git` — конвенция требует
    temp-commit/reset вместо stash).

КАК (защита от ложных срабатываний). Матчится ТОЛЬКО исполняемая часть
команды: литеральный текст в `'...'`/`"..."` и содержимое heredoc
(`<<EOF ... EOF`) вырезается регэксполм (`_QUOTED`, `_HEREDOC`) ПЕРЕД
прогоном правил — иначе commit-сообщение или содержимое скрипта, где
«git add -A» упоминается как ТЕКСТ (а не выполняется), ложно блокируется.
Доказанный на практике класс регресса — см. таблицу самотестов, план 231
§4в (docs/plans/231-bug-cycle-exit.md).

Fail-open по ошибкам самого хука (не смог распарсить stdin-JSON → exit 0),
fail-closed по правилам (паттерн совпал → exit 2).

ИСПОЛЬЗОВАНИЕ. Не запускается вручную — подключается декларативно через
локальный (НЕ в git) `.claude/settings.json`:
    hooks.PreToolUse[].matcher = "Bash|PowerShell"
    → command: python scripts/claude-hooks/guard-git.py
Хук получает JSON вызова инструмента через stdin (`tool_input.command`).
Самотеста в scripts/guards/selftest/ ПОКА НЕТ — см. таблицу покрытия план 231 §4в
и scripts/guards/selftest/README.md.
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
