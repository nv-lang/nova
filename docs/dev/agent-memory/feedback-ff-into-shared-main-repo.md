---
name: feedback-ff-into-shared-main-repo
description: "FF в main из общей рабочей копии может попасть в ЧУЖУЮ checked-out ветку concurrent-агента — проверять HEAD, использовать git branch -f main"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 1e103df6-8d3d-4ee9-80ac-d67a5be852c1
---

При bidirectional-sync (worktree-ветка → main через fast-forward) рабочая копия `d:/Sources/nv-lang/nova` может стоять НЕ на `main`, а на feature-ветке concurrent-агента (напр. `plan-140-overflow-elide`, созданной из main). Тогда `git merge --ff-only <branch>` форвардит **checked-out ветку**, а не main → хайджек чужой ветки.

**Why:** другие агенты держат свои ветки checked-out в общем main-репо; `git merge` всегда оперирует текущим HEAD, не `main`.

**How to apply:**
1. Перед любым merge в общем main-репо: `git rev-parse --abbrev-ref HEAD` — убедиться что это `main`.
2. Если main НЕ checked out — двигать ref без checkout: проверить `git merge-base --is-ancestor <old-main> <commit>` затем `git branch -f main <commit>` (FF-safe, не трогает рабочий каталог чужой ветки).
3. Если уже хайджекнул чужую ветку — `git reflog show <branch>` найти прежний HEAD, `git reset --hard <prev>` (предварительно убедившись `git status` чист, иначе потеряешь чужие незакоммиченные изменения).

Связано: [[feedback-worktree-shared-stash]], [[feedback-isolated-worktree]], [[feedback-sync-with-main-bidirectional]].
