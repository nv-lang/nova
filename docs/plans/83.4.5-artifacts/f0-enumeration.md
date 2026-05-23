// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 83.4.5 Ф.0 enumeration — flip-active failure catalog

> **Создан:** 2026-05-23 (Plan 83.4.5 Ф.0 execution).
> **Артефакт:** `docs/plans/83.4.5-artifacts/f0-enumeration.md`.
> **Базис:** branch `plan-83-4-5` в worktree `D:\Sources\nv-lang\nova-p83-4-5`.

---

## 1. Методика

1. **Baseline (pre-flip)** — `nova test` без `nova_runtime_auto_arm()` (стандартное состояние main).
2. **Flip-active** — после раскомментирования `nova_runtime_auto_arm();` в
   `compiler-codegen/src/codegen/emit_c.rs:11116`. Полный rebuild
   `cargo build --release`, full `nova test`.
3. Сравнение PASS/FAIL/SKIP + enumeration NEW failures.

## 2. Сводка результатов

| Состояние | PASS | FAIL | SKIP | Δ FAIL vs baseline |
|---|---|---|---|---|
| **Baseline (pre-flip)** | 1130 | 1 (pre-existing) | 56 | — |
| **Flip-active** | 1106 | 25 (1 pre-existing + 24 NEW) | 56 | **+24** |

**Pre-existing fail** (не относится к Plan 83.4.5):
- `plan99_probe/my_map_probe` CC-FAIL — codegen-уровневая ошибка
  `initializing 'nova_int' (aka 'long long')`. Воспроизводится на main без flip.
  Out-of-scope для Plan 83.4.5.

## 3. Полный список NEW регрессий (24 теста)

### 3.1. TIMEOUT (drain hang / deadlock) — 11 случаев

| Тест | Killed после | Гипотеза |
|---|---|---|
| `concurrency/assert_in_fiber` | 72649ms | assert+abort внутри fiber — supervised drain не завершается |
| `concurrency/cancel_latency_bench` | 69002ms | cancel-wake parked не работает → drain ждёт |
| `concurrency/deep_gc` | 74750ms | deep spawn nesting + GC → hang в drain |
| `concurrency/deep_spawn` | 76113ms | deep spawn nesting → cascade cancel/cleanup hangs |
| `concurrency/gc_bench` | 71627ms | GC stress → drain hang |
| `concurrency/gc_no_leak` | 72390ms | GC leak detection — drain не финализируется |
| `concurrency/mono_spawn_closure_smoke` | 79573ms | mono-spawn closure → fiber stuck |
| `concurrency/select_timer_stress` | 76590ms | select + timer cancel cascade |
| `concurrency/sleep_bench` | 75732ms | bench-loop с sleep → drain не отпускает |
| `concurrency/supervised_cancel_stress_test` | 76016ms | **deadlock cancel cascade nested supervised** |
| `concurrency/supervised_cancel_test` | 78601ms | **same family — cancel не cascade** |

### 3.2. RUN-FAIL (assert / partial pass) — 13 случаев

| Тест | Результат | Категория |
|---|---|---|
| `concurrency/cancel_semantics_test` | partial pass | **83.4.5.1** — cancel wake parked |
| `concurrency/main_yield` | 3/x passed | **83.4.5.5** — main yield под armed runtime |
| `concurrency/mn_maxprocs_getter` | 2/3 passed | **83.4.5.6 Ф.1** — C2 ассерт (auto-armed) |
| `concurrency/mn_runtime_smoke` | 3/4 passed | **83.4.5.6 Ф.1** — C1 ассерт (auto-armed) |
| `concurrency/parallel_for` | 9/14 passed | **83.4.5.3** — ordering asserts |
| `concurrency/per_fiber_handlers` | 1+ FAIL | **83.4.5.4** — inner-with в spawn |
| `concurrency/sleep_precision_bench` | partial fail | **83.4.5.3** — tight budgets |
| `concurrency/sleep_real_clock` | 4/5 passed | **83.4.5.3** — wall-clock slack |
| `concurrency/time_handler` | partial fail | **83.4.5.4** — Time effect handler |
| `effects/fail_handler` | FAIL | **83.4.5.4** — Fail handler в supervised |
| `plan65/f10_select_cancel_propagation` | 0/1 passed | **83.4.5.1** — cancel + select close_after |
| `plan65/f11a_timer_metrics` | partial fail | **83.4.5.1** — timer_cancelled на cancel-token cleanup |
| `plan65/f7_cancel_via_token` | 0/1 passed | **83.4.5.1** — cancel закрывает pending timer |

## 4. Категоризация по sub-планам

### 4.1. Sub-plan 83.4.5.1 (cancel wake-all + scope-tree cascade) — 7 тестов

**RUN-FAIL:**
- `concurrency/cancel_semantics_test` — cancel-wake parked
- `plan65/f7_cancel_via_token` — cancel закрывает pending timer
- `plan65/f10_select_cancel_propagation` — select+cancel close_after
- `plan65/f11a_timer_metrics` — `Time.timer_cancelled` increment

**TIMEOUT (drain hang):**
- `concurrency/supervised_cancel_test` — single supervised cancel
- `concurrency/supervised_cancel_stress_test` — nested supervised cascade
- `concurrency/cancel_latency_bench` — cancel-wake latency bench

**Root cause:** cancel НЕ wake'ает parked fibers (нет
`nova_scope_cancel_wake_all`) + cancel НЕ cascade'ится в child supervised
scope'ы (нет `nova_scope_cancel_cascade_children`).

### 4.2. Sub-plan 83.4.5.2 (detach M:N semantics) — 0 тестов

**Заметка:** `detach_test` в актуальном списке fail НЕ присутствует — либо
прошёл, либо изменился вместе с runtime'ом. **Подтвердить через targeted-run**
после Ф.0; если pass — sub-plan 83.4.5.2 можно понизить в приоритете
(detach spec D50 amend остаётся как documentation work).

### 4.3. Sub-plan 83.4.5.3 (test-suite cleanup) — 6 тестов

**RUN-FAIL (assert):**
- `concurrency/parallel_for` (9/14) — ordering asserts `log == 121212`
  (round-robin ожидание). Под armed runtime worker-deque order ≠ cooperative.
- `concurrency/sleep_precision_bench` — `sleep(0)` zero-overhead yield <5ms
- `concurrency/sleep_real_clock` (4/5) — wall-clock slack узок
- `concurrency/mn_runtime_smoke` (3/4) — C1 ассерт `!runtime.is_initialized()`
- `concurrency/mn_maxprocs_getter` (2/3) — C2 ассерт same

**TIMEOUT (long-running bench под M:N):**
- `concurrency/sleep_bench` — bench-loop становится slow под scheduler jitter

**Root cause:** test-side assertions ожидают
- (a) cooperative round-robin ordering (parallel_for),
- (b) tight wall-clock budgets без M:N jitter (sleep_*),
- (c) bootstrap `!runtime.is_initialized()` до auto-armed (mn_*).

### 4.4. Sub-plan 83.4.5.4 (handler-scoping nested) — 3 теста

**RUN-FAIL:**
- `concurrency/per_fiber_handlers` — `inner with в spawn перекрывает outer
  для своего fiber — outer_seen == 111`. **Подтверждённая hypothesis:**
  spawn-time TLS snapshot НЕ наследует parent handler-state.
- `concurrency/time_handler` — Time effect handler under nested with
- `effects/fail_handler` — Fail handler ловит throw из supervised
  (cross-mco-boundary interrupt) — `invocations == 1`

**Root cause:** spawn-time `fiber_effect_snapshot[i]` init — пустой,
не наследует parent's TLS handler-state. Под armed runtime spawned fiber
видит EMPTY handlers → `raise X` без active inner-with НЕ находит handler.

### 4.5. Sub-plan 83.4.5.5 (main_yield armed runtime) — 1 тест

**RUN-FAIL:**
- `concurrency/main_yield` (3/x passed) — main `Time.sleep(0)` под armed
  runtime консумит worker async-events → race с supervised_run drain.

**Root cause:** `nova_fiber_yield` на main thread (Plan 83.4.3 B4) вызывает
`uv_run(loop, UV_RUN_NOWAIT)`, который под armed runtime читает worker
`_main_wake` async events → events «съедены» yield'ом, drain'овский
outer `uv_run` блокируется бесконечно.

### 4.6. Sub-plan 83.4.5.6 (flip re-activation + closure) — C1+C2 ассерты + 4 TIMEOUTs

**C1+C2 (test-assertion under armed runtime auto-init):**
- `concurrency/mn_runtime_smoke` (test 1) — assertion `!is_initialized()` →
  поменять на `is_initialized()` (auto-armed).
- `concurrency/mn_maxprocs_getter` (test 1) — same.

**Edge TIMEOUTs требуют root-cause investigation (могут быть симптомами
83.4.5.1 cancel cascade — но в not-strictly-cancel-tests):**
- `concurrency/assert_in_fiber` — assertion внутри fiber → abort path
- `concurrency/deep_spawn` — deep-spawn nesting → ctx_pins / scope-stack
- `concurrency/deep_gc` — deep spawn + GC root scanning
- `concurrency/gc_bench`, `concurrency/gc_no_leak` — GC + drain interaction
- `concurrency/mono_spawn_closure_smoke` — mono-spawn closure
- `concurrency/select_timer_stress` — select + timer cancel cascade

**Гипотеза:** многие TIMEOUTs — следствия missing cancel-wake-all
(parked fiber никогда не пробуждается → drain hangs). Должны
самофиксы после 83.4.5.1.

## 5. Acceptance критерий Ф.0

✅ Полный enumeration NEW failures под flip-active build:
- 11 TIMEOUTs + 13 RUN-FAILs = **24 NEW regressions** (vs **18 предсказанных** в umbrella).
- Каждый fail привязан к одному из 6 sub-planов.

✅ `83.4.5-artifacts/f0-enumeration.md` создан (этот файл).

✅ REVERT `nova_runtime_auto_arm();` обратно в comment-form (ожидается
после commit'а артефакта; для Ф.0 closure отдельный коммит-revert).

## 6. Следующие шаги (Ф.1-Ф.6 sub-planов)

Порядок (sequential для безопасности; параллелизация worktree'ями
возможна для 83.4.5.1+3+4+5 — все independent):

1. **83.4.5.3 (test-suite cleanup)** — самый низкий риск (test-side only).
   Закрывает 6 тестов одним PR. Может быть выполнен ПЕРВЫМ.
2. **83.4.5.5 (main yield)** — узкий fix `nova_fiber_yield`, 1 тест.
3. **83.4.5.1 (cancel wake-all + cascade)** — runtime work, 4 RUN-FAIL +
   3 TIMEOUT. **Likely фиксит и часть других TIMEOUTs** (deep_*, select_*,
   gc_*).
4. **83.4.5.4 (handler-scoping nested)** — runtime work, 3 теста.
5. **83.4.5.2 (detach M:N)** — детальный re-investigation; если
   `detach_test` теперь PASS, sub-plan может быть понижен до docs-only
   (D50 amend).
6. **83.4.5.6 (flip activation + closure)** — GATED на 83.4.5.1+3+4+5.

## 7. Артефакт-link для умbrella

Этот файл и сводка прикреплены к [Plan 83.4.5](../83.4.5-mn-drain-edge-cases.md)
как референс для Ф.1-Ф.6.

---

**Cтатус Ф.0:** ✅ ЗАВЕРШЕНО (2026-05-23).
