# Plan 82 Ф.1–Ф.3 — Windows fiber arena: реализация и находки

> **Статус:** ✅ **Ф.1+Ф.2+Ф.3 ЗАВЕРШЕНЫ (2026-05-22).** Windows
> fiber-стеки переведены с minicoro-default calloc на lazy-commit
> large-reserve arena с полной GC-интеграцией; arena сделана M:N-safe
> (cross-thread migration, multi-worker GC). Полный `nova test`:
> **1058 PASS / 0 FAIL / 56 SKIP** — 0 регрессий. Standalone-харнессы
> (`f1_arena_test.c`, `f1_gc_test.c`) — зелёные на MSVC + clang-cl.
>
> Ф.1 (arena) и Ф.2 (GC-интеграция) **слиты в одну поставку** — Ф.1 в
> одиночку регрессирует (см. §2). Ф.3 — M:N-safe arena (§7). Артефакты:
> `fiber_arena_win.c` (рантайм), `f1_arena_test.c` / `f1_gc_test.c`
> (харнессы).

---

## 1. Что реализовано

### Ф.1 — arena allocation + overflow detection (`fiber_arena_win.c`)

- **Lazy-commit large-reserve.** Арена — один `VirtualAlloc(MEM_RESERVE)`
  на поток (16384 слота × 8 MB = 128 GB виртуального резерва, на 64-bit
  тривиально, нулевой commit-charge). Физический commit — только под
  minicoro-header + начальное окно стека у вершины.
- **Путь A — OS-native grow** (Ф.0 decision-point): после TIB-свопа
  minicoro-asm'а ядро Windows растит коро-стек штатно через
  `PAGE_GUARD`-фолт. VEH для happy-path не нужен.
- **Раскладка слота** (§5.1): `[hard guard 16K reserved][minicoro header
  commit][reserved/grown][initial window commit + PAGE_GUARD]`. Hard
  guard — reserved (касание → AV; нулевой commit-charge), backstop от
  stack-clash. minicoro кладёт `DeallocationStack` на низ stack-секции
  → ядро поднимает `STATUS_STACK_OVERFLOW` до порчи header'а.
- **Патч `ctx.stack_limit`** (Ф.0 test a — обязателен): после
  `mco_create` `nova_fiber_post_create` (fibers.c) пишет committed-low
  в `ctx.stack_limit`; без него `__chkstk`-код крашит на MSVC.
- **VEH-диагностика overflow:** `STATUS_STACK_OVERFLOW` / arena-AV →
  «`nova: fiber stack overflow in slot N`» через `WriteFile`
  (stack-frugal — на overflow стека почти нет).
- **Decommit — послотный** при переиспользовании грязного слота
  (`nova_fiber_alloc`, dirty-ветка), не idle-batch (см. §3).
- **`FlsAlloc`/`FlsCallback`** — освобождение арены при выходе потока.

### Ф.2 — GC-интеграция (precise push, в `fiber_arena_win.c`)

- `GC_set_push_other_roots`-колбэк (`#ifdef NOVA_GC_BOEHM`): на mark-фазе
  пушит закоммиченные диапазоны КАЖДОГО живого fiber'а + native-стек
  main-thread'а.
- **Реестр живых fiber'ов = `used_bits` арены** — арена уже полный
  реестр; отдельный интрузивный список (план §5.2 (1)) не нужен для
  single-thread.

---

## 2. Главная находка: Ф.1 в одиночку РЕГРЕССИРУЕТ — §1.6 опровергнут

План разделял Ф.1 (arena) и Ф.2 (GC) на допущении §1.6: «штатный
per-thread conservative-скан Boehm + `VirtualQuery`-clamp **корректно**
покрывает running fiber на arena-стеке». Ф.0 §4(iii) проверила, что
clamp делает скан **fault-free** — но НЕ проверила **корректность
покрытия**.

**Эмпирически (харнесс `f1_gc_test.c` T1) §1.6 опровергнут:** объект,
удерживаемый только указателем на стеке fiber'а, запущенного на
arena-стеке, **собирается GC** (UAF) — Boehm НЕ сканирует arena-стек.

Причина: arena лежит в 32–128 GB `VirtualAlloc`-регионе далеко от
native-стека. С minicoro-default **calloc** fiber-стек (56 KB в C-heap)
случайно попадал в boehm-овский over-scan через C-heap-регион — отсюда
«работало». Arena разрывает эту случайность.

**Следствие:** Ф.1 нельзя поставить без Ф.2. §9-риск-3 плана это
предусматривал: «running fiber придётся покрывать push-колбэком». Ф.1 и
Ф.2 слиты.

---

## 3. Находка: idle-batch decommit деградирует — послотный decommit

План §5.1 предписывал decommit «батчем на idle» (`slots_active == 0` →
`VirtualFree(MEM_DECOMMIT)` по `[base, high_water*slot_size)`). На
Windows этот диапазон достигает 32–128 GB; `VirtualFree(MEM_DECOMMIT)`
проходит миллионы PTE за вызов, а при fiber-churn'е (sleep_bench — 10k
fiber'ов) idle-batch зовётся тысячи раз → деградация на порядки
(`sleep_bench` зависал >160 сек).

Это **отличие Windows от Linux**: Linux `madvise(MADV_DONTNEED)` по
NORESERVE-VMA дёшев; Windows `VirtualFree(MEM_DECOMMIT)` — нет.

**Фикс:** послотный decommit — слот декоммитится при переиспользовании
(O(slot_size), ограниченный диапазон). Освобождённый-непереиспользованный
слот держит commit-charge до reuse либо `nova_fiber_arena_compact()`.

---

## 4. Находка: `GC_push_all` не масштабируется — `GC_push_all_eager`

Первая версия push-колбэка пушила каждый fiber-стек через `GC_push_all`.
Харнесс `f1_gc_test.c` T4 показал: при **~2048 живых fiber'ах** Boehm
**намертво виснет** в колбэке. `nova test` повторял это (`sleep_bench`,
2048 fiber'ов).

Диагноз (харнесс + изоляция `NOVA_FW_GC_NOPUSH`): `GC_push_all` лишь
кладёт `(lo,hi)`-дескриптор на mark-stack (deferred). Тысячи дескрипторов
переполняют mark-stack → патологическое поведение Boehm.

**Фикс:** `GC_push_all_eager` (`gc_mark.h:301`) — сканирует диапазон
**немедленно**, без накопления дескрипторов. Документация прямо называет
его средством скана стеков. Харнесс T4: 12000 fiber'ов + GC → OK.

---

## 5. Находка: slot_count 4096 мал — 16384 для Windows

`sleep_bench` спавнит 10000 одновременных fiber'ов; calloc-baseline это
тянул (без лимита). Arena 4096 слотов → exhaustion. План §3 называет
реальный потолок «4k–16k concurrent fibers per process».

**Фикс:** `NOVA_FIBER_SLOT_COUNT` для Windows = 16384 (128 GB резерва,
тривиально на 64-bit). Linux/macOS — без изменений (4096). Безграничный
рост — arena-chaining, вне scope Plan 82 (§3).

---

## 6. Верификация

| Проверка | Результат |
|---|---|
| `f1_arena_test.c` (MSVC + clang-cl) — alloc/grow/`__chkstk`/reuse/overflow/lazy-commit | ✅ все под-тесты |
| `f1_gc_test.c` (MSVC + clang-cl) — GC изнутри коро-стека / на глубине / round-robin / 12000 fiber'ов + GC | ✅ T1–T4 |
| `nova test nova_tests/concurrency` (75 тестов, incl. `sleep_bench`, все `gc_*`) | ✅ 75 PASS / 0 FAIL |
| Полный `nova test` | ✅ **1058 PASS / 0 FAIL / 56 SKIP** — 0 регрессий |

`gc_*`-корректность (`gc_correctness`, `gc_no_leak`, `deep_gc`,
`gc_introspection`) + `supervised_*` + `channels` + `select_*` зелёные —
сильное свидетельство корректности arena + GC-интеграции.

---

## 7. Ф.3 — M:N-safe arena (cross-thread migration) — ✅ ЗАВЕРШЕНА 2026-05-22

Ф.1+Ф.2 покрывали single-thread cooperative. Под M:N (work-stealing,
несколько worker-потоков) `fiber_arena_win.c` переработан целиком:

- **Heap-арены в глобальном append-only списке.** Раньше арена жила в
  TLS-структуре (`__declspec(thread)`); под M:N этого мало — `&arena`
  становится dangling после выхода потока, а GC-колбэку и cross-thread
  dealloc нужен доступ к чужим аренам. Теперь арена — `calloc`-структура;
  TLS хранит лишь указатель. Все арены — в глобальном списке
  `_nova_fw_arena_list` (link под `_nova_fw_list_lock`; обход — lock-free,
  append-only).
- **Atomic bitmap.** `used_bits` мутируется atomically
  (`_interlockedbittestandset64` в alloc — владелец; `…reset64` в
  dealloc — ЛЮБОЙ поток); `slots_active` — `_Interlocked{Inc,Dec}rement64`.
  `dirty_bits`/`high_water` — только владелец (alloc/compact на одном
  потоке) → plain. Lock-free, без per-arena lock'а в горячем пути.
- **Address-based cross-thread dealloc (§5.3).** `nova_fiber_dealloc`
  находит арену-владельца ПО АДРЕСУ (`_nova_fw_find_arena` — обход
  списка), не по TLS. fiber, мигрировавший A→B и завершённый на B,
  освобождает слот в арене A. То же — `committed_low`, `arena_contains`,
  VEH (multi-arena).
- **Multi-arena GC-колбэк.** `_nova_fw_gc_push_other_roots` обходит ВСЕ
  арены: fiber-стеки каждого worker'а + native scheduler-стек каждого
  потока (per-arena `native_base`). Опровергает §1.6 на полную: под M:N
  GC с любого worker'а видит fiber-стеки всех остальных.
- **Worker-арена lifecycle.** `runtime.c::_worker_main` создаёт арену
  worker'а в старте (`nova_fiber_arena_init` — регистрирует native-стек
  worker'а для GC); `nova_fiber_arena_thread_exit` перед
  `GC_unregister_my_thread`; `nova_runtime_shutdown` освобождает
  worker-арены (`nova_fiber_arena_release_retired`) ПОСЛЕ join — гонок
  с обходом нет.
- **§П3 — полнота GC-root.** `_workers`-массив (`calloc`, C-heap) держит
  `w->scope` с указателями на `nova_alloc`-массивы (`fiber_ctx`/…) → без
  явного root собрался бы. `_materialize_pool` добавляет
  `GC_add_roots(_workers, …)`, `nova_runtime_shutdown` снимает.

Верификация: `nova test nova_tests/concurrency` — **75 PASS / 0 FAIL**
(вкл. все `mn_*`, `parallel_for*`, `mn_runtime_init_shutdown_cycles`);
полный `nova test` — **1058 PASS / 0 FAIL / 56 SKIP**, 0 регрессий.
`[M-82-gc-mn-deferred]` ✅ ЗАКРЫТ.

Standalone-харнесс `f1_gc_test.c` — оставлен single-thread (T1–T4):
multi-thread под-тесты упирались в harness-специфику ручного управления
Boehm-потоками (`CreateThread` + `GC_register_my_thread` без libuv-
структуры worker'а); multi-worker GC валидируется `nova test` M:N.

---

## 8. Что осталось (последующие фазы)

- **Ф.4** — production тест-матрица §7 (Nova-уровень).
- **Ф.5** — context-switch бенчмарк (M:N на Windows уже работает —
  runtime платформо-агностичен).
- **Ф.6** — spec D97 + закрытие 44.3.
