---
name: feedback-sync-with-main-bidirectional
description: «синканись с main» = bidirectional sync (pull main + merge worktree→main); «обновись из main» = unidirectional pull only
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 54a32126-e5da-4ff8-be99-1a1918172fee
---

«синканись с main» = **bidirectional** sync.

**Why:** user explicitly clarified 2026-06-03 that "sync" по smыслу
двунаправленная operation — pull новых main commits в worktree AND
merge worktree changes в main. Earlier sessions я interpretировал
"синканись" как только pull (unidirectional), что вызывало повторные
запросы "сделай merge".

**How to apply:**

- **«синканись с main D:\Sources\nv-lang\nova»** или **«синканись с main»**
  → bidirectional:
  1. `git fetch /d/Sources/nv-lang/nova main` в worktree
  2. `git merge FETCH_HEAD --no-edit` или `--ff-only` (pull main → worktree)
  3. `cd /d/Sources/nv-lang/nova && git merge plan-XX --no-ff -m "..."`
     (push worktree → main)
  4. Re-sync worktree fast-forward к новому main HEAD
  5. Re-build nova-cli + verify regression post-merge

- **«обновись из main»** → unidirectional (только pull main → worktree).
  Только step 1 + 2 above.

- **«оставь локально»** / **«не merge'и»** → no push к main, остаётся
  только на worktree branch для review.

**Default fallback** (ambiguous phrasing) — bidirectional. Если user
не хочет push в main, явно скажет.
