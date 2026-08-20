---
name: feedback-sync-with-path
description: "«синканись с X PATH» = доведи свою работу до PATH (merge в репо по этому пути), НЕ fetch/pull/report"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 983d5d85-0ec0-4e36-bebd-16d93a47786a
---

«**Синканись с майн D:\path**» в словаре пользователя означает **«приведи main в этой директории в синхронное состояние со своей работой = смержи свою ветку в main по этому пути»**, а НЕ «git fetch + report state».

**Why:** 2026-06-03 misread эту команду **4 раза подряд** в одной сессии. Каждый раз делал read-only check состояния вместо merge. Пользователь в итоге не видел файла `docs/research/08-*.md` в `D:\Sources\nv-lang\nova`, потому что моя работа была закоммичена в worktree, но не в main репо.

**How to apply:**

1. **Путь в конце команды = destination, не cwd.** Если в команде указан путь к репо/директории — это **куда положить результат**, не «где запустить git».
2. **Конкретно для «синканись с майн PATH»:** проверить `git log` в PATH, и если моих коммитов нет — `git merge` ветки в main по этому пути (fast-forward если возможно). Read-only report — НЕ то, что просят.
3. **Repeat-misread signal.** Если пользователь повторяет одну и ту же фразу два раза подряд и результат не тот — это означает что я неправильно понял intent. **Остановиться и переспросить, а не делать третий раз то же самое.** Не повторять misread N раз надеясь что в этот раз получится правильно.

**Related:**
- [[feedback-isolated-worktree]] — рабочие изолированные worktree'и; merge в main делается по explicit команде «синканись».
- [[feedback-commit-per-task]] — коммит per task; merge в main = отдельная операция после коммитов.
