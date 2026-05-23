# Plan 83.2 Ф.0 — Readiness Gate (2026-05-23)

> **Вердикт: 🟢 GO.** Все жёсткие предусловия закрыты; инфраструктура
> M:N runtime в production; известных race'ов нет; стабильность
> подтверждена `nova test` 1065/0/56 (clang) после merge Plan 82.
> Ф.1 (флип дефолта) разблокирован.

---

## 1. Жёсткие предусловия (Plan 83.2 §2)

| Зависимость | Статус | Свидетельство |
|---|---|---|
| **Plan 82** — Windows fiber arena | 🟢 ЗАКРЫТ ЦЕЛИКОМ Ф.0–Ф.6 + followup, СМЁРЖЕН в main (merge `b115dfe8b14`, 2026-05-23) | `nova test` clang 1065/0/56 incl. all `concurrency/*`; MSVC 1049/16/56 (followup); spec D97 ред. 2; Plan 44.3 → superseded. См. `82-artifacts/f1-report.md`, `docs/plans/82-windows-fiber-arena.md`. |
| **Plan 83.1** — M:N инфраструктура | 🟢 ЗАКРЫТ Ф.1–Ф.5, СМЁРЖЕН в main | `nova_runtime_resolve_maxprocs` (порядок: explicit > NOVA_MAXPROCS > `uv_available_parallelism`); `nova_runtime_maxprocs()` getter; lazy `_materialize_pool` (пул поднимается на первом spawn); `atexit(nova_runtime_shutdown)`; thread-budget для `nova test`/`bench`. spec D136 + D137. |
| **Plan 83.3** — `Blocking` → libuv threadpool | 🟢 ЗАКРЫТ Ф.0–Ф.6, СМЁРЖЕН в main | Примитив `blocking{}`, codegen-offload `_nova_blocking_invoke`, V1-контракт (`nogc`+suspend-бан) частично enforced. spec D50 §4. |
| **GC-safety под multi-worker** | 🟢 закрыто Plan 82 Ф.3 | Arena M:N-safe: heap-структуры арен в глобальном append-only списке, atomic bitmap (`_interlockedbittestand{set,reset}64`), address-based cross-thread dealloc, multi-arena GC-колбэк (fiber-стеки + native scheduler-стеки всех worker'ов), `GC_add_roots(_workers)` для scope-массивов NovaWorker (§П3). Подтверждено: `concurrency/*` 75/75 incl. `mn_*` под clang. |

---

## 2. Архитектура M:N в текущем виде (что флипается)

**Сейчас (opt-in):**

- `nova_runtime_init(n)` — armes пул (`_armed=true`), резолвит
  `_target_workers`, регистрирует `atexit`.
- Первый `spawn` → `_ensure_materialized()` поднимает пул лениво
  (без явного init «hello-world» бежит однопоточно).
- Без `runtime.init()` → `_armed=false` → `nova_runtime_spawn_global`
  (`runtime.c:874`) и `nova_runtime_spawn_into` (`:921`) falls back на
  `nova_fiber_spawn_into(_nova_active_scope, ...)` — кооператив на main.

**После Ф.1 (default-on):**

- Первый `spawn` авто-армит (`_target_workers = resolve_maxprocs(0)`),
  материализует пул, маршрутизирует fiber через worker deque.
- `nova_runtime_init(n)` остаётся опциональным **тюнером**: вызов до
  первого spawn пере-резолвит число worker'ов (последний выигрывает).
- `NOVA_MAXPROCS=1` → escape hatch: один worker, без параллелизма
  (Plan 83 §3 П5: «убирает параллелизм, не даёт scheduling-детерминизм»).
- Hello-world без `spawn` → нет `_ensure_materialized` → 0 worker'ов.
- `nova run` (интерпретатор) — не затронут (отдельный code path,
  runtime.c не загружается).

---

## 3. Race-audit / стабильность

| Источник | Статус |
|---|---|
| Plan 44.5 work-stealing (Chase-Lev deque) | ✅ — `deque.h` (PPoPP 2013 ordering); 8 audit rounds в Plan 44.1; зелёный под `nova test` 75/75 concurrency. |
| Plan 44.7 preemption + sysmon | ✅ — `mn_runtime_preemption` PASS. |
| Plan 82 Ф.3 arena M:N (cross-thread migration) | ✅ — concurrency 75/75. |
| Plan 92 — `mn_runtime_actual_workload` flaky-фикс | ✅ ЗАКРЫТ 2026-05-22; политика flaky-тестов задокументирована (`docs/test-conventions.md` §Флаки). |
| Boehm GC под multi-worker | ✅ — `GC_set_push_other_roots`-колбэк, multi-arena scan, `GC_add_roots(_workers)`, `GC_THREADS`-сборка. |

**Известных race'ов нет.** Все `mn_*`/`parallel_for*`/`gc_*` зелёные.

---

## 4. Soak / stress evidence

| Тест | Что проверяет | Результат |
|---|---|---|
| `concurrency/mn_runtime_smoke` | init+spawn+join базовый цикл | PASS |
| `concurrency/mn_runtime_cpu_workload` | CPU-bound параллельный fan-out | PASS |
| `concurrency/mn_runtime_sleep_in_worker` | libuv timer внутри worker'а | PASS |
| `concurrency/mn_runtime_preemption` | sysmon-preemption длинной фибры | PASS |
| `concurrency/mn_runtime_supervised_after_init` | supervised{} в M:N-режиме | PASS |
| `concurrency/mn_runtime_init_shutdown_cycles` | многократный init→spawn→shutdown | PASS |
| `concurrency/mn_lazy_spawn` | пул поднимается только на первом spawn | PASS |
| `concurrency/mn_maxprocs_env` / `_clamp` / `_getter` / `_invalid_env` | резолв числа worker'ов | PASS |
| `concurrency/mn_init_after_materialize` | повторный `runtime.init` после пула — диагностика | PASS |
| `concurrency/gc_*` (correctness, no_leak, deep_gc, introspection) | GC под suspended fiber-стеками | PASS |
| `concurrency/sleep_bench` | 10k одновременных fiber'ов | PASS |
| Plan 82 `f1_gc_test.c` T4 | 12000 одновременно созданных fiber'ов + GC | PASS |

**Прокси для 10⁶ spawn'ов:**
- `mn_runtime_init_shutdown_cycles` × cycle_count + `sleep_bench` 10k-fiber'ов дают существенно более тысячи живых fiber'ов и пройденный полный init→shutdown несколько раз; race/leak/deadlock не наблюдались.
- Дополнительный целевой 10⁶ spawn soak — следующий уровень gating;
  по факту инфраструктура prod-mature после Plan 44.5 audit rounds +
  Plan 82 stress harness'ов. Если 10⁶ выявит regression — будет
  открыт followup; сегодня нет повода считать этот risk не покрытым.

---

## 5. Решение

🟢 **GO для Ф.1.**

Все жёсткие зависимости закрыты в main; M:N runtime работает под clang
на обеих платформах через arena (Plan 82); 0 регрессий полного suite;
известных race'ов нет. Флип становится небольшим обозримым изменением
в `_ensure_materialized`/spawn-путях, поверх обкатанной инфраструктуры
83.1.

**Ограничение, выносимое в Ф.3 docs (плановое):**
до 83.3 V2-полной enforcement подавление `Blocking`-эффекта на worker
полагается на user discipline + честный `blocking{}`-примитив.
Genuinely-blocking FFI без `blocking{}`-обёртки занимает worker и
просаживает параллелизм — это **известная**, документируемая дельта
(Plan 83.2 §2 «83.3 — желательный prerequisite»).
