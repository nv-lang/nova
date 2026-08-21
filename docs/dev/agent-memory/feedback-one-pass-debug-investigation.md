---
name: feedback-one-pass-debug-investigation
description: При debug-инвестигации не делать summary-then-detail в два запуска; собрать всё нужное в одном запуске
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 149c4c1a-98b5-40ca-86c9-49e785cf7dfe
---

При расследовании конкретного бага в test suite — **не делать два запуска** (сначала `nova test` → выяснил какой fail → потом второй прогон чтобы получить детали этого fail). Делать всё в одном запуске.

**Why:** один прогон `nova test` ≈ 60-100 секунд. Два прогона = 2-3 минуты простоя плюс пользователь видит «agent зависает». Большинство этого времени — повторная компиляция тестов, которая уже была сделана. Пользователь явно указал на этот anti-pattern: «ты тратишь очень много времени, чтобы сначала вытащить SUMMARY, а вторым заходом искать конкретный баг».

**Anti-pattern:**
```
1. ./nova test                                  # 90s — только SUMMARY, видим "FAIL: closure_rev"
2. ./nova test --filter closure_rev --verbose   # ещё 30s — теперь получили детали
3. analyze details, fix bug
```

**Correct pattern (one-pass debug):**
```
./nova test 2>&1 | tee /tmp/test-run.log
# Сразу извлекаем и summary, и FAIL details в одном проходе через output

# Если test-runner поддерживает structured output:
./nova test --verbose --keep-tmp 2>&1 | tee /tmp/test-run.log
# --verbose печатает details для каждого FAIL прямо в runtime

# Затем grep по логу:
grep -B 2 -A 20 "FAIL" /tmp/test-run.log
```

**Alternative — known target ahead of time:**

Если уже знаем какие именно тесты надо проверить (например, из плана или предыдущего отчёта — «4 тестовых файла должны fail'нуть»):
```
# НЕ запускать полный nova test для «убедиться»
# Сразу targeted с verbose details
./nova test --filter "closure_rev|p48_closure|cancel_semantics|for_in_range_iter" --verbose 2>&1 | tee /tmp/probe.log
```

**Алгоритм debug-инвестигации (готовый чек-лист):**

1. **Перед расследованием**: какие именно файлы/симптомы ищем? (из плана, prior agent report, expected failures list)
2. **Если знаем targets** → `nova test --filter "<regex>" --verbose 2>&1 | tee log` — один запуск с деталями.
3. **Если не знаем targets** → `nova test --verbose 2>&1 | tee log`, потом `grep -B 2 -A 20 "FAIL\|ERROR" log` для всех details сразу.
4. **Codegen failure** → диагностические данные обычно в stderr; передавать `2>&1` обязательно.
5. **Если test runner не печатает детали в verbose** — добавить `--keep-tmp` или `--output-c` (если есть), читать generated artifacts сразу из log.
6. **НЕ делать пробный прогон без --verbose** «чтобы посмотреть что упало» — это удвоение времени.

**How to apply:**
- Перед запуском `nova test` всегда задать вопрос: «какие детали мне понадобятся когда что-то упадёт?». Включить флаги для них сразу.
- `tee /tmp/file.log` чтобы output можно было grep'ать после, не запуская заново.
- Если суммарной строки PASS/FAIL нет в `--verbose` режиме — добавить `| tail -50` сохранением stdout/stderr целиком.
- Применять одинаково к `cargo test`: `cargo test --release -- --nocapture 2>&1 | tee log` — `--nocapture` сразу даёт `println!` debug-вывод.
- Передавать тот же алгоритм sub-agent'ам в задачах — явно в инструкции «один запуск с --verbose, не делать summary-then-detail».
