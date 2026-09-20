---
name: www-update
description: "обновить сайт nv-lang.org: синк доки и спеки из nova, локальная сборка, пуш, зелёный деплой обоих доменов — адаптер .claude/commands/www-update.md"
type: prompt
disableModelInvocation: true
---

**Это АДАПТЕР, а не вторая копия.** Источник правды —
`.claude/commands/www-update.md`. Прочитай его целиком и следуй ему.

Подстановки под Kimi Code CLI минимальны: инструменты Bash, Read, Grep, Glob,
Edit, Write доступны; `gh` и `npm` вызываются через Bash. Единственное отличие —
никаких `ListAgents`/`SendMessage` для согласования с соседями: координация по
сайту идёт через пользователя.

Порядок сохраняется: `nova main` запушена и гейт зелёный → локальная сборка
`npm run build` в `site/` → пуш `www` во все три зеркала → зелёный деплой `.org`
→ прогон зеркала `.ru`.

$ARGUMENTS
