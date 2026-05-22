# Plan 82 Ф.1 + Ф.2 — Windows fiber arena: реализация и находки

> **Статус:** ✅ **ЗАВЕРШЕНО (2026-05-22).** Windows fiber-стеки переведены
> с minicoro-default calloc на lazy-commit large-reserve arena с полной
> GC-интеграцией. Полный `nova test`: **1058 PASS / 0 FAIL / 56 SKIP** —
> 0 регрессий. Standalone-харнессы (`f1_arena_test.c`, `f1_gc_test.c`) —
> зелёные на MSVC + clang-cl.
>
> Ф.1 (arena) и Ф.2 (GC-интеграция) **слиты в одну поставку** — Ф.1 в
> одиночку регрессирует (см. §2). Артефакты: `fiber_arena_win.c`
> (рантайм), `f1_arena_test.c` / `f1_gc_test.c` (харнессы).

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

## 7. Что осталось (последующие фазы)

- **Ф.3** — cross-thread migration: arena-aware dealloc по адресу (не по
  TLS); GC-видимость мигрировавшего fiber'а.
- **Ф.5** — M:N на Windows. GC push-колбэк сейчас покрывает арену
  collector-потока + native-стек main-thread'а; под M:N нужен обход
  ВСЕХ per-thread арен + suspended worker-стеков (§5.2 (3)). Реестр на
  `used_bits` под M:N — нужна alloc-lock-дисциплина либо глобальный
  реестр. VEH-проверка арены — тоже per-thread. См.
  `simplifications.md` [M-82-gc-mn-deferred].
- **Ф.4** — полная тест-матрица §7 (обе toolchain × cooperative + M:N).
- **Ф.6** — spec D97 + закрытие 44.3.
