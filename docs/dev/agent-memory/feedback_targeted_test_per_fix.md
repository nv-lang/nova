---
name: feedback-targeted-test-per-fix
description: "После каждого fix запускать ТОЛЬКО targeted test (фикстуру для этого бага), не полный nova test suite. Full regression только в конце phase/plan."
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 1b171c1a-1ebe-41ac-8384-bdab1b0efdce
---

**Правило:** per-fix verify = только specific fixture для этого бага.
Full `nova test` (5-10 мин) — только в конце phase или plan-doc closure.

**Why:** Full nova test в worktree занимает 5-10 минут (761 PASS).
Запуск после каждого из 10-20 sites = 100+ минут wall-clock сжигается
на регресс-проверку которая в 99% случаев одинаковая. User flagged как
явную трату времени.

**Wrong (full test per fix):**
```bash
# Site 1 migration → cargo build → nova test (10min) → commit
# Site 2 migration → cargo build → nova test (10min) → commit
# ... 10 sites = 100min just for regression
```

**Right (targeted test per fix, full only at end):**
```bash
# Site 1 migration → cargo build → nova test plan70/f1_*.nv (5s) → commit
# Site 2 migration → cargo build → nova test plan70/f2_*.nv (5s) → commit
# ...
# Phase complete → nova test (full, 10min) → final commit + verify
```

**How to apply:**
- Для каждого site write **specific** `nova_tests/plan70/fN_*.nv` фикстура
  таргетируящая именно этот fix (positive + EXPECT_COMPILE_ERROR negative).
- После migration → cargo build + `nova test <single_fixture>` (1-5 sec).
- После всех sites одной phase → ONE full nova test для regression-guard.
- Если phase commit'ы атомарные, full test может быть после каждой
  phase (а не после каждого commit).

**Exception:** если migration trivially меняет multiple files и есть
риск чего-то задеть orthogonal — full test уместен. Honest judgment.

**Combined с feedback_nova_test_one_pass:** даже single-test run должен
делать grep с `tee` чтобы за один пробег получить summary + fail details.
