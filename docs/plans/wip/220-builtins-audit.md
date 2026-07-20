<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# 220 Ф.0 — аудит билтинов: гейт окупаемости TCC-порта

**Статус:** аудит завершён 2026-07-20 (opus? нет — sonnet, разведка). База: `313ecc289`
(main). Worktree: `nova-220audit` (branch `p220-builtins-audit`), **код не менялся** — только
чтение + этот документ.

**Задача:** план [220](../220-tcc-dev-backend.md) §2.1/§3 Ф.0 — прежде чем оценивать выигрыш
TCC как dev-бэкенда, провести аудит ВСЕХ `__builtin_*`/`__atomic_*`/TLS в `nova_rt/**` и в
том, что `emit_c.rs` эмитит в generated app.c, и вынести вердикт: порт окупается / закрыть 220
/ ограничить однопоточным dev.

## 0. Ключевая методологическая находка: что реально компилирует TCC

`compiler-codegen/nova_rt/*.c` (runtime.c, driver.c, fiber_arena.c, alloc_boehm.c, net.c, fs.c,
eventloop.c, typeid.c) — после [218](../218-prebuilt-runtime-archive.md) предсобираются ОСНОВНЫМ
тулчейном (clang/gcc/msvc) в `libnova_rt.a`; TCC их НЕ компилирует, только линкует. Единственный
top-level `#include` в generated app.c — `#include "nova_rt/nova_rt.h"`
(`emit_c.rs:8344`), который транзитивно тянет ТОЛЬКО `.h`-файлы (typeid.h, array.h, effects.h,
fiber_arena.h, sync.h, fibers.h, string_builder.h, eventloop.h, nova_sched.h [→ deque.h, runq.h],
runtime.h, channels.h, sync_primitives.h, contracts.h, vtables.h, bench.h, io_console.h, os_env.h,
cast.h, alloc.h) — все они `static inline`-тяжёлые и КОМПИЛИРУЮТСЯ TCC заново в каждом app.c.

**Вывод:** реальная TCC-экспозиция = (а) `nova_rt/*.h` (компилируются TCC) + (б) всё, что
`emit_c.rs` эмитит напрямую в app.c. Билтины, живущие ТОЛЬКО в `nova_rt/*.c` (прешит в
`libnova_rt.a`) — НЕ проблема TCC-кодогена, а чисто линковочный вопрос (ABI/calling-convention
совместимость — это уже риск-пункт 3 в самом плане 220, не пункт этого аудита).

## 1. Таблица билтинов по классам

| Класс | Сайтов (TCC-компилируемые `.h` / emit_c.rs) | Сайтов (`.c`, только линковка, не TCC-риск) | TCC-поддержка | Compat-сложность/риск |
|---|---|---|---|---|
| `__builtin_*_overflow` (add/sub/mul) | 6 (`effects.h:1052-1085`, оба варианта signed/unsigned) + 2 прямых emit-сайта в `emit_c.rs` (~36806-36808, `overflowing_add/sub/mul` на 9 целочисленных типах: `nova_int`, `int8/16/32/64_t`, `nova_byte`, `uint16/32/64_t`, `nova_uint`) | 0 | **НЕТ.** Офиц. tcc-doc.html документирует только `__builtin_types_compatible_p`/`__builtin_constant_p`; overflow-семейства нет вообще. | **НИЗКИЙ-СРЕДНИЙ.** Есть ЧАСТИЧНЫЙ рабочий прецедент — `nova_msvc_compat.h:280-313` (`_nova_compat_add/sub/mul_ov_ll`, чистая переносимая арифметика без интринсиков для add/sub, `_mul128`-MSVC-интринсик для mul). НО: этот прецедент **не полный** — гейтится `sizeof(*(r))==8`, для 8/16/32-битных `int8_t/int16_t/int32_t/nova_byte/uint16_t/uint32_t` (используются emit_c.rs-сайтом НАПРЯМУЮ, не только int64!) шимма молча возвращает `overflow=1` («теоретическая защита», см. её же комментарий) — то есть даже под MSVC сегодня сомнительна корректность overflow-проверки на НЕ-64-битных типах. Полный TCC-порт потребует closing этого же пробела для всех 9 ширин/знаковостей + свой 128-битный mul (TCC вряд ли даёт `__int128`/`_mul128` — нужен inline-asm `mul`/`imul` с rdx:rax). |
| `__atomic_*` (load/store/CAS/fetch/exchange/fence) | **412** occurrences: `deque.h`(28, Chase-Lev work-stealing deque), `fibers.h`(18), `nova_sched.h`(19), `runq.h`(20), `sync.h`(17), `sync_primitives.h`(308 — std `Atomic*`-типы, механически повторяющийся паттерн), `sync_barrier.h`(1), `sync_semaphore.h`(1); + ~19 прямых emit-сайтов в `emit_c.rs` (Cell/OnceCell/Lazy: `has_value`/`state`/`poisoned`) | 84 (`alloc_boehm.c`7, `driver.c`5, `fiber_arena.c`31, `net.c`12, `runtime.c`22, `test_runq.c`7-тест) — прешиты в `libnova_rt.a`, TCC не компилирует | **ЧАСТИЧНАЯ и НЕНАДЁЖНАЯ.** stdatomic-парсинг добавлен патчами 2021; но подтверждённый баг (tinycc-devel 2022-04): скомпилированные атомики **не эмитят `lock`-префикс** → молча НЕ атомарны в многопоточном сценарии. Это худший класс отказа — не compile error, а silent race. | **ВЫСОКИЙ (если полагаться на нативную TCC-поддержку) → СРЕДНИЙ (если обойти её).** Все 14 используемых форм (`load_n/store_n/exchange_n/compare_exchange_n/fetch_add/sub/and/or/xor/nand/add_fetch/sub_fetch/thread_fence`, non-`_n` load/store) уже ПОЛНОСТЬЮ реализованы в `nova_msvc_compat.h` (176 строк) через `_Interlocked*`-интринсики на word ≤8B — никакого 128-битного DWCAS нигде не нашлось (проверено на deque.h/sync_primitives.h). Стратегия: НЕ использовать нативные TCC-атомики вообще (обойти известный баг), а написать `nova_tcc_compat.h`-аналог на GNU inline-asm (`lock cmpxchg`/`lock xadd`/`xchg`/`mfence`) — TCC's inline-asm задокументирован как зрелый (gas-синтаксис, поддержка именованных операндов GCC 3.x). Тот же «over-strong barrier always sound на x64 TSO» аргумент из шапки msvc_compat переносится буквально. Риск НЕ в «невозможности», а в том, что это САМАЯ correctness-критичная точка (Chase-Lev deque шедулера) — тот же класс кода, где недавно нашли M-187 (вечный клин) и M-211 (preempt-flag race) через целевой stress-тест, а не ревью. Новый inline-asm compat потребует ТАКОГО ЖЕ уровня stress-верификации с нуля. |
| `__builtin_expect` | 1 реальный сайт (`fibers.h:2166`, `NOVA_UNLIKELY`) | 0 | Не поддерживается (не задокументирован), НО **не важно**: TCC НЕ определяет `__GNUC__` по умолчанию (подтверждено вебпоиском) → сайт уже гейтится `#if defined(__GNUC__) \|\| defined(__clang__)` и автоматически падает в безопасный `#else (x)`-fallback. | **НОЛЬ.** Уже TCC-safe сегодня, без единой правки. |
| TLS (`_Thread_local`/`__thread`/`__declspec(thread)`) | **~20-25 отдельных extern-объявлений** в `.h`, компилируемых TCC (effects.h: 11 — `_nova_fail_top`, `error_state_native/_p`, `interrupt_top`, `current_handler_iframe`, `test_frame`, `handler_Fail`, `handler_Fail_any`, `handler_Time`, `active_finalizer_stack`, `effect_registry`; fibers.h: 5 — `active_scope`, `active_slot`, `park_unlock_fn/_arg`, `preempt_ptr`; + мелкие в fiber_arena.h/eventloop.h/net.h/fs.h/runtime.h/bench.h) **+ НЕОГРАНИЧЕННОЕ число** `_nova_handler_<Effect>` globals, эмитимых `emit_c.rs` НАПРЯМУЮ В app.c (по одному на каждый user-defined effect, `emit_c.rs:2446-2456`, `:15217-15222`) | 36 определений в `effects.c` (сами definitions — прешиты; но объявления-потребители всё равно в `.h`) | **НЕТ вообще.** Официально подтверждено (2013+ tinycc-devel тред, актуально и в mob): TCC не парсит `__thread`/`_Thread_local` как storage-class — «unsupported storage class» **hard compile error** на TCC-Windows. Громкий отказ (не тихий, как атомики) — но блокер. | **САМЫЙ ШИРОКИЙ по охвату, СРЕДНИЙ по чистой технике.** Прецедент архитектуры УЖЕ есть в репо: `fibers.h:3939-3943` — `nova_runtime_current_worker_id()` заменил raw `extern __declspec(thread)` именно потому что тот «непортируем — не включён на Linux clang без -fdeclspec». Стратегия по аналогии («errno-паттерн»): т.к. САМИ definitions TLS-переменных nova_rt живут в `.c` (прешиты обычным тулчейном, который `__thread` поддерживает нативно), TCC-у не нужно ВООБЩЕ видеть ключевое слово `__thread` — только добавить thin accessor-функции (`T* nova_tls_X_ptr(void)`) в те же `.c`, и под `__TINYC__` в заголовках сделать `#define _nova_X (*nova_tls_X_ptr())`. Работает даже под `&_nova_X` (взятие адреса, есть в registry `void** slots[N]`) — `&(*f())` эквивалентно `f()`, корректный C. Это закрывает ~20-25 nova_rt-внутренних переменных МЕХАНИЧЕСКИ. **НО** per-effect `_nova_handler_X` глобали emit_c.rs эмитит НАПРЯМУЮ в app.c (не в прешитый `.c`) — трюк «спрятать за прешитый accessor» тут не работает, т.к. переменная возникает только в конкретной user-программе. Это требует РЕАЛЬНОГО редизайна кодогена (Ф.2/Ф.3, не механика): один общий TLS-слот (одна `TlsAlloc`/`pthread_key`) вместо N независимых нативных TLS-глобалей, с полями по индексу вместо по имени. |
| Прочие `__builtin_*` | `__builtin_ctzll` — 1 сайт, но в `fiber_arena.c` (`.c`, **прешит, TCC вообще не видит**); `__builtin_readcyclecounter` — **0 реальных вызовов** (только устаревший комментарий в channels.h:871, код реально использует plain static counter — мёртвая ссылка даже в nova_msvc_compat.h); `alloca` — 2 сайта (`channels.h`1-TCC-видит, `net.c`1-прешит). Не найдено: `__builtin_unreachable/trap`, явные `__builtin_memcpy/memset/memmove`, `__builtin_clz/popcount`, `__builtin_frame_address`, `va_*`-builtins. | — | `alloca()` — де-факто стандарт, у TCC исторически есть (используется в OS-dev проектах на TCC) — низкий риск, но не верифицировано напрямую в Ф.0, отложено на Ф.1. `ctzll`/`readcyclecounter` — TCC вообще не встретит. | **НОЛЬ-НИЗКИЙ.** Ничего не блокирует. |

## 2. Отдельный риск вне `__builtin_*`/`__atomic_*`/TLS (side-finding)

`minicoro.h` (вендоренная 3rd-party coroutine-библиотека, отвечает за fiber context-switch —
самый низкоуровневый и критичный код рантайма) — уже имеет свою абстракцию `MCO_THREAD_LOCAL`
(gate по `_MSC_VER`/`__STDC_VERSION__`/GNU), но **ноль веток под TCC**, и использует
платформо-специфичный asm/`ucontext`/fcontext-подобный механизм переключения стека. Это НЕ
`__builtin_*`/`__atomic_*`/TLS в узком смысле задачи, но: (а) патчить вендоренный файл — 
дополнительная нагрузка upstream-дрейфа; (б) корректность TCC-ассемблера/линкера на этих
трамплинах не проверена данным аудитом (только чтение) — открытый вопрос для Ф.1.

## 3. Самый рискованный элемент

**`__atomic_*` в Chase-Lev deque шедулера (`deque.h`/`nova_sched.h`/`runq.h`/`fibers.h`,
412 TCC-компилируемых сайтов)** — не потому что TCC-порт технически невозможен (inline-asm
compat-путь реален и опирается на уже проверенный MSVC-прецедент), а потому что: (1) нативная
TCC-поддержка атомиков имеет ПОДТВЕРЖДЁННЫЙ баг тихой потери `lock`-префикса — единственный
безопасный путь категорически ИСКЛЮЧАЕТ использование нативных TCC-атомиков, только собственный
inline-asm-шим; (2) это ТА ЖЕ подсистема, где недавно закрыты M-187 (вечный клин под
connection-storm, opus-волна) и M-211 (preempt-flag plain race) — оба нашлись только через
целевой stress-тест, не ревью; новый inline-asm-слой потребует эквивалентной верификационной
нагрузки с нуля, на 412 сайтах критичного кода.

TLS шире по охвату (больше сайтов, плюс реальный редизайн кодогена для per-effect handlers), но
менее опасен ПО ТИПУ отказа: TCC даёт громкий compile-error на `__thread`, а не тихую гонку —
ошибку порта поймает первая же попытка собрать, а не stress-тест под нагрузкой через месяц.

## 4. Проверка альтернативы «ограничить однопоточным dev» (план §2.1 п.5)

План предполагает fallback: TCC только для no-spawn программ (без M:N-атомиков). Аудит это
**частично опровергает**: TLS (`_nova_fail_top`, error/interrupt-frame, `active_scope/slot`)
обслуживает БАЗОВУЮ effect-семантику Nova (raise/try/with-handler dispatch, D158 fail-frame) —
она активна в ЛЮБОЙ Nova-программе, включая тривиальный no-spawn «hello world» (fiber-модель
исполнения работает и для main() без единого `spawn`). Значит **ограничение
«однопоточный dev» снимает риск атомиков (самый опасный по типу отказа), но НЕ снимает
основную массу TLS-работы** (~20-25 nova_rt-сайтов + codegen-редизайн per-effect handlers) —
она нужна всегда. «Дешёвый fallback» из плана дешевле только по риску, не по объёму работы.

## 5. Вердикт окупаемости

Ни один из найденных блокеров не является технически непреодолимым — для всех трёх (overflow,
atomics, TLS) есть конкретный, реализуемый путь, и для двух из трёх (overflow, atomics) уже
существует ЧАСТИЧНО рабочий прецедент в этом же репо (`nova_msvc_compat.h`), доказывающий, что
макро-шим-стратегия в принципе работает на этой кодовой базе. Формально — **(а) порт окупаем**,
список работ ограничен и понятен:

1. `nova_tcc_compat.h`: overflow (9 ширин × 3 операции, включая новый inline-asm 128-битный
   `mul` — TCC вряд ли даст `__int128`) + atomics (14 форм × inline-asm, ~412 TCC-сайтов
   покрываются одним force-include header'ом по аналогии с MSVC `/FI`).
2. TLS-accessor рефакторинг nova_rt (~20-25 переменных, `.c`-side accessor + `#define`-редирект
   в `.h` под `__TINYC__`, по прецеденту `nova_runtime_current_worker_id()`).
3. Codegen-редизайн per-effect handler storage (Ф.2/Ф.3, `emit_c.rs`) — один общий TLS-блок
   вместо N нативных глобалей.
4. Полная верификация: conformance + флагман (--strict-effects) + M:N race-стресс на новом
   inline-asm atomics-слое (та же дисциплина, что уже применяется к nova_rt/mn-conventions).
5. Открытый вопрос вне списка: minicoro.h context-switch под TCC-ассемблером (не аудирован
   здесь, только чтение) — на Ф.1 может всплыть ЧЕТВЁРТЫЙ блокер, ещё не покрытый выше.

**НО** экономика — не только «возможно ли», а «стоит ли, учитывая где риск». Обе тяжёлые статьи
(atomics-inline-asm, TLS-codegen-редизайн) концентрируются именно в M:N-шедулере — подсистеме
с историей тихих concurrency-багов (M-187, M-211), которую проект уже не раз чинил ценой
целевых stress-кампаний, а не ревью. Выигрыш TCC (план §1) реализуется ТОЛЬКО после
[218](../218-prebuilt-runtime-archive.md) и ТОЛЬКО на компиляции app.c (рантайм уже прешит) —
величина выигрыша сегодня НЕ измерена (218 ещё не влит/не измерен постфактум).

**Рекомендация:** не двигаться в Ф.1 сейчас. План 220 уже P3/разведочный/высокий-риск по
собственной классификации — этот аудит подтверждает «высокий риск» предметно (не абстрактно) и
добавляет: риск сконцентрирован ровно там, где цена ошибки максимальна (шедулер), а выгода
пока не измерена (зависит от 218). Держать 220 в текущем статусе (📋 утверждён, но строго после
218) до тех пор, пока (а) 218 не влит и не даёт измеримую пост-218 картину доли app.c-компиляции
в общем dev-build-времени, и (б) владелец не решит, что цена inline-asm-atomics-слоя +
TLS-codegen-редизайна в шедулере оправдана этой измеренной величиной. Закрывать план полностью
сейчас — необязательно (блокеров технически no dead-end нет), но и открывать Ф.1 раньше
пересчёта выигрыша после 218 — преждевременно.

## 6. Хэши / модель

- База: `313ecc289` (main, "Merge branch 'p-fix-218-archive-race'").
- Worktree: `p220-builtins-audit`, только чтение — рабочее дерево совпадает с базой (без
  правок кода).
- Модель: sonnet (Claude Sonnet 5), разведка/аудит, без суб-агентов, синхронно.
- Веб-источники (TCC capability research, см. §1): bellard.org/tcc/tcc-doc.html (офиц.
  документация), tinycc-devel mailing list threads (2013 TLS feature-request; 2021 stdatomic
  patch; 2022 atomic lock-prefix bug report), TinyCC/tinycc GitHub mob-branch Changelog.
