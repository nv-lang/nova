<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 151 — M:N runtime: GC premature-collect замыкания в `supervised{spawn{body()}}` (НЕ mono-recursion)

> **Создан:** 2026-06-13 (из Plan 149 fu #1 / marker `[M-cancellation-test-mono-recursion-overflow]`).
> **Статус:** ✅ **ЗАКРЫТ Ф.0-Ф.5 (2026-06-13).** Title-мисдиагноз исправлен (см. §1).
> **Приоритет:** P2 — un-quarantine'ит `cancellation_test`; реальный фикс мал (~67 строк runtime C).
> **Родитель:** marker `[M-cancellation-test-mono-recursion-overflow]` → переименован/закрыт как
> `[M-mn-worker-fiber-closure-call-stack-overflow]`. Затронутая подсистема: M:N runtime (`nova_rt`,
> дом Plan 82 / 83.x), **НЕ** мономорфизация (Plan 48/55 не при чём — codegen exonerated).
> **Worktree:** `nova-p151` · ветка `plan-151`.

---

## 1. Проблема — и почему title оказался МИСДИАГНОЗОМ

Title и Audits 2/3 (Plan 149) считали, что `within[T]`/`race2[T]` мономорфизируются в C с
**бесконечной рекурсией**. **Это неверно.** Plan 151 Ф.0 (5-way isolation matrix + дизассемблирование
сгенерённого C + native-диагностика) доказал:

- **Codegen ЧИСТ.** Сгенерённый `within[int]` НЕ рекурсивен: `nova_fn_…within…` зовёт
  `nova_supervised_run_cancel` + spawn-entries `_nova_spawn_0/_1`; `_nova_spawn_0` делает
  `NOVA_CLOS_CALL_vi((*_c->body))` ОДИН раз на тривиальном `nova_lambda_0_body(){return 42;}`.
  Никакой self-recursion, mono-цепочки или потерянного base-case. `mono_depth_limit` (500) НЕ
  срабатывает; codegen завершается за ~8 c.
- **Тот же бинарь PASS** под `NOVA_AUTOARM=0` и под `NOVA_MAXPROCS<=3` → это НЕ codegen и НЕ
  stack-size (64MB не помогает; AUTOARM=0 с default-стеком — помогает).
- **Generics ни при чём:** non-generic `run_int(body fn()->int)` с тем же
  `supervised{spawn{ ro r=body() }}` падает идентично.

**Реальный root-cause — GC-reachability bug в M:N рантайме.** Heap-замыкание (`NovaClos_vi`,
аллоцированное в `main`, переданное как `body` в `within`) укоренено ТОЛЬКО на **native-стеке
главного потока** (param `body` во фрейме `within`), пока main блокирован в
`nova_supervised_run_impl`. Под **≥4 worker'ах** GC срабатывает во время `_materialize_pool` —
ДО того как main создаст СВОЮ (ленивую) fiber-арену (она создаётся на первом `mco_create`). Arena
GC-колбэк `_nova_fw_gc_push_other_roots` обходит только арены из `_nova_fw_arena_list`; main-арены
там ещё нет → main-стек НЕ попадает в обход → Boehm STW НЕ видит замыкание → **premature collect**
→ блок реюзается, `closure->fn` зануляется → worker-fiber зовёт `fn()` = NULL → **RIP=0** (jump to
null), что arena-VEH рапортует обманчиво как «fiber stack overflow in slot 0».

**Подтверждающие пробы (Ф.0):** `GC_DONT_GC=1` чинит краш; `MAXPROCS<=3` pass / `>=4` fail
детерминированно (окно срабатывания GC); spawn-diag перед `mco_resume` показал `body*` валиден, но
`closure->fn == 0`; SEGV-diag показал `RIP=0`, DEP/EXEC fault, стек почти не использован (НЕ runaway,
НЕ oversized frame).

## 2. Гипотезы (Ф.0 решила) — ВСЕ codegen-гипотезы ОТВЕРГНУТЫ
- ~~(a) Потерянный base-case~~ — нет рекурсии в C.
- ~~(b) Бесконечная mono-цепочка~~ — codegen за 8 c, `mono_depth_limit` не срабатывает.
- ~~(c) Closure-param self-referential lowering~~ — `NOVA_CLOS_CALL_vi` зовётся один раз.
- ✅ **(d, реальная) M:N GC-root:** main-стек не сканируется при сборке во время материализации
  пула при ≥4 worker'ах. **Локус фикса:** `compiler-codegen/nova_rt/fiber_arena_win.c` +
  `runtime.c` (`_materialize_pool`). **НЕ** `const_fn_mono.rs` / `emit_c.rs` (codegen exonerated).

## 3. Фазы (выполнено)

### Ф.0 — GATE: re-diagnosis (вместо «audit codegen»)
- 5-way isolation + дизасм C (clean) + native-диагностика (RIP=0, closure->fn=0) + GC_DONT_GC=1 /
  MAXPROCS-sweep. Вывод: codegen exonerated; root-cause — M:N GC main-stack scan. ✅

### Ф.1 — RUNTIME fix (НЕ codegen)
- `fiber_arena_win.c`: `static void* _nova_fw_main_stack_base`; `nova_fiber_arena_set_main_stack()`
  фиксирует `NT_TIB.StackBase` главного потока; `_nova_fw_gc_push_other_roots` пушит
  `[AllocationBase, StackBase)` (committed-only) главного стека.
- `runtime.c` `_materialize_pool`: вызов `nova_fiber_arena_set_main_stack()` на main ДО создания
  worker'ов.
- `fiber_arena.c` / `fiber_arena.h`: POSIX no-op + декларация.
- НЕ тронуты `const_fn_mono.rs`, `emit_c.rs`. ✅

### Ф.2 — Un-quarantine
- Удалён `cancellation_quarantine/_fixture.toml`; `cancellation_test.nv` → `nova_tests/concurrency/`,
  module `concurrency.cancellation_test` (D78). Исправлен in-file header (был неверный mono-диагноз).
- PASS armed (NOVA_AUTOARM unset) 80/80: MAXPROCS default/2/8/16. ✅

### Ф.3 — Broad regression (concurrency-targeted, M:N — затронутая подсистема)
- Concurrency suite armed (clang): **112 PASS / 4 FAIL**, все 4 pre-existing и runtime-unrelated
  (`detach_test` E_READONLY_FIELD, `fn_array_collect_test` undefined `copy`, `sleep_real_clock`
  SLACK_MS C-bug, `condvar_wait_cancel` TIMEOUT — идентично на baseline feb64555).
- gc 2/0, generics 5/0. plan103_* sync-primitives — фейлы только pre-existing CODEGEN/CC + flaky
  suite-timeout (в изоляции pass). `plan55/f1_closure_array_gc_stress` RUN-FAIL — идентично baseline.
- **0 regressions.** ✅

### Ф.4 — Pos/neg regression-guard
- `nova_tests/concurrency/mn_closure_spawn_gcroot_test.nv` — минимальный NON-generic guard
  (`run_int(body fn()->int)` + generic `within[int]` armed). GENUINE: 8/8 fail на pre-fix runtime
  (feb64555), pass post-fix. Без `// ENV NOVA_AUTOARM=0` — обязан бежать armed. ✅

### Ф.5 — Closure
- Marker `[M-cancellation-test-mono-recursion-overflow]` → `[M-mn-worker-fiber-closure-call-stack-overflow]`,
  CLOSED. backlog-followups + README + Plan 149 quarantine-note исправлены. ✅

## 4. Критерии приёмки (met)
1. ✅ Root-cause: M:N worker-pool closure-in-spawn GC premature-collect (НЕ mono); codegen exonerated.
2. ✅ Ф.0 нативная диагностика (RIP=0, closure->fn=0, GC_DONT_GC/MAXPROCS-discriminator) ДО фикса.
3. ✅ `within[int/str/bool]` + `race2[int]` PASS @ default-стеке armed (НЕ только AUTOARM=0).
4. ✅ Минимальный non-generic guard `supervised{spawn{ro r=body();…}}` armed без краша.
5. ✅ `cancellation_test` un-quarantined + PASS armed.
6. ✅ Full concurrency + broad sample green armed; 0 regression в sync/supervised/armed-M:N.
7. ✅ НЕТ правок `const_fn_mono.rs` / `emit_c.rs`.
8. ✅ Repeat/stress: 80/80 armed (MAXPROCS default/2/8/16) + GC_FULL_FREQ=1 + 8/8 pre-fix-fail guard.

## 5. Связь
- marker `[M-mn-worker-fiber-closure-call-stack-overflow]` (этот план — его дом).
- Plan 149 fu #1 — quarantine (этот план её снимает + исправляет мисдиагноз).
- M:N runtime family: Plan 82 (Windows fiber arena), 83.x (M:N scheduler/GC roots).
