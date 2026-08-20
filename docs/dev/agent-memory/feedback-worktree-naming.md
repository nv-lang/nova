---
name: feedback-worktree-naming
description: "Naming convention для постоянных worktree: nova-pNN (короткий suffix), не nova-planNN"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 149c4c1a-98b5-40ca-86c9-49e785cf7dfe
---

При создании постоянного worktree под план — использовать convention `nova-pNN` (короткий suffix), не `nova-planNN`.

**Примеры существующих worktree в проекте:** `nova-p33-contracts`, `nova-p45-doc`, `nova-p62-prelude`. Все следуют паттерну `nova-p<номер плана>-<краткий descriptor>` (descriptor обязателен — даёт контекст о чём план).

**Why:** короче, консистентно с остальной репой, легко вводить в команды.

**How to apply:**
- Worktree под Plan 62 (prelude migration) → `nova-p62-prelude/` (не `nova-plan62/`, не `nova-p62/` без descriptor).
- Worktree под Plan 70 → `nova-p70-<descriptor>/`.
- Descriptor обязателен — даёт контекст о чём план без открытия `docs/plans/`.
- Sub-worktree (если когда понадобится) — `nova-pNN-<descriptor>-<sub>/`.
