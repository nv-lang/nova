---
name: feedback-agents-must-not-touch-git-config
description: "Агентам ЗАПРЕЩЕНО git config: worktree делят .git — правка травит авторство ВСЕЙ репы (338 коммитов Claude Haiku)"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
  modified: 2026-07-25T20:39:12.422Z
---

Инцидент 2026-07-25 (вопрос владельца «почему claude committed» по GitHub): haiku-агент 22.07 сделал `git config user.name/email` БЕЗ --global в своём worktree — конфиг у worktree ОБЩИЙ (.git один), 338 коммитов main ушли под «Claude Haiku <claude@anthropic.com>» вместо владельца.

**Why:** авторство коммитов — владельца (соло-проект, публичная история); отравленный shared-конфиг незаметен локально и всплывает только на GitHub.

**How to apply:**
- В КАЖДЫЙ бриф агента: «git config НЕ ТРОГАТЬ (ни user.*, ни прочее)».
- Интегратор: периодически `git log --format=%an -1` перед push; после инцидента канон-конфиг nova = `Evgeniy Golovin <unitcraft@nv-lang.org>` (локально; глобальный — inbox.ru).
- Переписывание истории (force-push) — только по явному слову владельца.

Связано: [[feedback-no-claude-coauthor]], [[user-identity]], [[feedback-worktree-shared-stash]] (тот же класс: worktree делят .git).
