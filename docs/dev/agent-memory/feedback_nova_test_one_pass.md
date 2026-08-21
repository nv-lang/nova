---
name: feedback-nova-test-one-pass
description: "Запускать nova test один раз — captureить и summary, и FAIL details в одном проходе. Не делать второй запуск чтобы получить детали падений."
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 1b171c1a-1ebe-41ac-8384-bdab1b0efdce
---

**Правило:** один `nova test` запуск = весь нужный output за раз.

**Why:** `nova test` занимает 5-10 минут (full suite в worktree). Запуск
два раза подряд (один для summary, второй для FAIL details) = удвоение
wall-clock. User жаловался на трату времени.

**Wrong (two passes):**
```bash
# Run 1 — get summary
nova test | grep "PASS:"
# Run 2 — get fail details
nova test | grep "FAIL"
```

**Right (one pass):**
```bash
# Capture all output, then grep both summary AND fail details
nova test 2>&1 | tee /tmp/nova_test.log
# Then post-process:
grep -E "^PASS:|^FAIL " /tmp/nova_test.log  # summary + per-test fails
```

Или сразу в одном grep'е:
```bash
nova test 2>&1 | grep -E "^PASS:|^FAIL |CC-FAIL|EXPECT|^error\["
```

**Особенно важно** при работе над Plan 70 / migrations где после каждого
изменения нужно verify — экономия 5+ мин per migration phase × десятки
сайтов = часы за сессию.

**How to apply:**
- Перед запуском `nova test` решить ЗАРАНЕЕ что нужно из output —
  summary, fail details, slowest tests, и т.д.
- Один tool call с grep cover'я все нужные паттерны
- Если нужны детали failed test'ов — `tee` сохранить полный output,
  затем post-process без повторного run.
- Background mode (`run_in_background: true`) полезен если test
  длинный — пока ждёшь, не делай ничего что инвалидирует state.
