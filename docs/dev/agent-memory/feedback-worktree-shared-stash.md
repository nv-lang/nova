---
name: feedback-worktree-shared-stash
description: worktree делят один .git → stash/refs/reflog глобальны; не использовать git stash при конкурентных агентах
metadata: 
  node_type: memory
  type: feedback
  originSessionId: f50fe7e4-dca3-46a0-a36b-1a2474f1b7bc
---

Все git-worktree этого проекта делят **ОДИН `.git`**. Приватны для worktree только
`HEAD`, `index`, рабочее дерево. **Глобальны для всего репо:** объекты, refs/branches,
reflog, и — ключевое — **`git stash`** (стэш-стек один на весь репозиторий).

**Why:** конкурентная активность нескольких worktree/агентов пересекается через общий
`.git`. `git stash push/pop` в одном worktree задевает ВСЕ — один агент может `pop`'нуть
чужой stash → потеря изменений. (Сигнал от user 2026-06-11: Ф.2-агент в `nova-p83-gomn`
заметил параллельную worktree-активность на `plan-83-go-cmn` через общие стэши.)

**How to apply:**
- НЕ использовать `git stash` для baseline-rebuild / временного отката, когда могут
  работать другие агенты/worktree.
- Вместо stash: (a) throwaway-коммит + `git reset` назад; (b) отдельный временный
  worktree на нужном SHA (`git worktree add`); (c) сборка из конкретного commit без
  трогания рабочего дерева.
- В промпты фоновых агентов добавлять явный запрет `git stash` + альтернативу.

Связано: [[feedback-isolated-worktree]], [[feedback_worktree_cwd_clarity]],
[[feedback_no_background_agents]].
