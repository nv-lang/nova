---
name: feedback-test-fix-per-file-loop
description: при исправлении массовых compile-errors после крупного refactor'а — per-file loop (nova check FILE → fix → re-check), а не full regression в loop
metadata:
  node_type: memory
  type: feedback
  originSessionId: b9c3879a-c2ff-443a-a53d-549da6c4f9a3
---

**Правило:** Когда массовая миграция (Plan 108.x sed sweeps, рефакторинги и
т.п.) даёт десятки compile-error'ов в разных файлах, **не запускай full
`nova test` в loop** для сходимости.  Вместо этого используй per-file loop:

**Алгоритм:**

1. Запусти полную регрессию **один раз** → получи список файлов с ошибками.
2. Для каждого файла:
   - `nova check FILE` (или `nova test FILE`).
   - Compiler показывает ВСЕ ошибки в этом файле (не только первую — depends).
   - Исправь все обнаруженные.
   - Повтори check на этом же файле, пока чисто.
3. После того как все файлы по-отдельности чистые → один финальный full
   regression для верификации + concurrency-flakiness check.

**Why:**

- **Why per-file, не full:** компилятор может stop'нуться на первой ошибке
  per-file → каждый full-regression раунд показывает только «head» ошибок.
  Per-file loop позволяет file-level convergence без межфайловых артефактов.
- **Why не sed-based:** sed regex'ы не покрывают все формы записи (type
  annotation `let X T =`, multi-let line `; let Y =`, indentation,
  edge cases).  Иt каждый раунд sed expose new patterns.  Per-file
  precise fix всегда корректен.
- **Why один финальный full regression:** проверить cross-file dependencies
  + concurrency tests (flaky vs deterministic).

**Когда НЕ применять:**

- Изменения 1-2 файлов → fix direct.
- Compiler change без migrations → full regression ок.

**Пример trigger'ов:**

- Plan 108.x (D36 amend) — массовая `let X = ...` → `let mut X = ...`.
- Plan 73.1 V3 — массовая `let X` → `consume X = ...` для consume-types.
- Future bulk renames через sed.

**Эффективность:**

- Full regression ~10-20 мин wall-time.
- Per-file check ~5-15 сек.
- 8 раундов full regression = ~1.5-2.5 часа vs ~10-30 мин per-file loop.

См. также: [[feedback-one-pass-fix]], [[feedback_nova_test_one_pass]].
