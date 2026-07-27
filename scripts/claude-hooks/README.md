<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# scripts/claude-hooks/ — хуки Claude Code

## Что это

Python-скрипты, подключённые как **PreToolUse-хуки Claude Code** — они
перехватывают вызов инструмента (Bash/PowerShell/Write) **ДО его
исполнения** и могут заблокировать его (`exit 2` + причина в stderr) или
пропустить (`exit 0`). Это агентская, а не git-часть машинного принуждения
норм плана [231](../../docs/plans/231-bug-cycle-exit.md) трек Д
(исполнительный дом — [231.2](../../docs/plans/231.2-enforcement-infra.md) §1):
правила, которые раньше жили только в тексте памяти/CLAUDE.md и зависели
от того, вспомнит ли агент их в моменте.

| Хук | Событие | Блокирует |
|---|---|---|
| [`guard-git.py`](guard-git.py) | `PreToolUse`, matcher `Bash\|PowerShell` | запись `git config user.name/email`, `git add -A`/`.`/`--all`, `git stash` |
| [`guard-memory.py`](guard-memory.py) | `PreToolUse`, matcher `Write` | запись `memory/feedback-*.md` без поля `enforcement:` |

## Как подключены

Через локальный (**НЕ закоммичен в git** — машинно-специфичные абсолютные
пути) `.claude/settings.json` в корне репы:

```json
{
  "hooks": {
    "PreToolUse": [
      { "matcher": "Bash|PowerShell",
        "hooks": [{ "type": "command", "command": "python \".../scripts/claude-hooks/guard-git.py\"" }] },
      { "matcher": "Write",
        "hooks": [{ "type": "command", "command": "python \".../scripts/claude-hooks/guard-memory.py\"" }] }
    ]
  }
}
```

Каждый хук получает JSON вызываемого инструмента через **stdin**
(`tool_input.command` для Bash/PowerShell, `tool_input.file_path`/
`tool_input.content` для Write) и печатает причину блокировки в stderr.

## Как проверить руками

```sh
echo '{"tool_input":{"command":"git add -A"}}' | python scripts/claude-hooks/guard-git.py; echo "exit=$?"
echo '{"tool_input":{"command":"git status"}}'  | python scripts/claude-hooks/guard-git.py; echo "exit=$?"
echo '{"tool_input":{"file_path":"memory/feedback-x.md","content":"без поля"}}' \
    | python scripts/claude-hooks/guard-memory.py; echo "exit=$?"
```

Первый должен вернуть `exit=2` (блок), второй и третий (без `enforcement:`
третий как раз ДОЛЖЕН заблокироваться — если нужен пропускающий пример,
добавьте `"enforcement: ..."` в `content`) — см. точную семантику в шапках
самих файлов.

На 2026-07-27 у этих двух хуков **нет** отдельного регресс-самотеста в
`scripts/selftest/` (см. таблицу покрытия в
[`scripts/selftest/README.md`](../selftest/README.md) и план 231 §4в) —
их поведение верифицировано только руками при вводе (2026-07-26).

## Чем отличаются от `scripts/githooks/`

Эти хуки ловят **ДО исполнения** самой команды инструментом (Bash-вызов
`git add -A` заблокирован раньше, чем оболочка вообще его увидит) и
работают только внутри сессии Claude Code. [`scripts/githooks/`](../githooks/)
— это обычные git-хуки (`core.hooksPath`), которые ловят **на `git commit`**
независимо от того, кто и как готовил staged-изменения (агент, владелец
руками, любой другой инструмент) — более поздний, но и более
универсальный рубеж.
