# gcc15-rt-notes — фикс gcc15-класса ошибок в nova_rt (3 категории)

**Дата:** 2026-07-20. **Worktree:** `nova-gcc15`, ветка `p-fix-gcc15-rt`, база `main` @ `ce0ab9e00`.
**Модель:** sonnet. **Триггер:** готовый диагноз из rt-headers-волны
(`docs/plans/wip/rt-headers-notes.md` + backlog `[M-linux-mn-conformance-red]`,
2026-07-20) — после include-гигиены (`p-fix-rt-headers`) rt-архив (Plan 218)
на WSL2 gcc 15.2.0 всё ещё падал тремя НЕ-include категориями ошибок (clang
21.1.8 на идентичном коде уже собирался чисто).

## Категория 1 — struct-tag unification (44 ошибки)

**Причина.** `driver.h` (строки 25-26) форвард-декларирует ТЕГИРОВАННЫЕ типы
(`struct NovaFiberQueue;`, `struct NovaBlockingState;`), а `fibers.h` определял
их через `typedef struct { ... } NovaFiberQueue;` / `NovaBlockingState` —
**анонимный** struct. С точки зрения C это два РАЗНЫХ типа (тег vs его
отсутствие), даже когда typedef-имя совпадает. gcc 14+ (в т.ч. 15.2)
продвинул `-Wincompatible-pointer-types` из warning в error-by-default для
C — clang 21 на этом же коде не эскалирует, отсюда расхождение.

**Фикс** (`compiler-codegen/nova_rt/fibers.h`):
- `typedef struct { ... } NovaFiberQueue;` → `typedef struct NovaFiberQueue { ... } NovaFiberQueue;`
  (структура начинается на строке ~303, было обнаружено чтением файла —
  комментарий-заголовок над typedef на строке 222 относится к ДРУГОЙ структуре,
  `NovaChildError`, не путать).
- `typedef struct { ... } NovaBlockingState;` (было на строке ~3769) →
  `typedef struct NovaBlockingState { ... } NovaBlockingState;`.

Только добавление тега в определении — ABI/layout не меняется (тег не incur
дополнительной памяти), платформо-нейтрально. Никакой логики не тронуто.

## Категория 2 — `__atomic_fetch_and/or/xor` на `nova_atomic_bool*` (36 ошибок)

**Причина.** gcc 14+ отказывается компилировать RMW-битовые интринсики
(`__atomic_fetch_and/or/xor`) на операнде `_Bool*` — запрет by design
(RMW-побитовые операции на `_Bool` признаны небезопасными на уровне gcc,
байтовое представление напрямую манипулируется в обход обычной bool-семантики
0/1). `nova_atomic_bool` (`sync.h:116`) — это `typedef bool nova_atomic_bool;`.
clang 21 эту диагностику не поднимает.

**Выбор: ОПЕРАЦИИ, не тип.** Рассмотрел два варианта из задания:
(a) сменить `nova_atomic_bool` на `uint8_t` глобально — но этот тип
    используется ПОВСЕМЕСТНО в планировщике (`fibers.h` cancel_requested ×2,
    published, done; `driver.h` stop/started; `channels.h` cancelled/closed/
    reader_closed; `runtime.c` stop, `_sysmon_running`) — 15+ сайтов, каждый
    требовал бы аудита; крупный blast radius в зоне, помеченной КРИТИЧНОЙ
    (M:N-планировщик).
(b) **ВЫБРАНО**: оставить `nova_atomic_bool = bool` без изменений; переписать
    ТОЛЬКО 6 проблемных функций (`Nova_AtomicBool_method_fetch_{or,and,xor}_{bool,MemOrdering}`,
    `sync_primitives.h`) — это ЕДИНСТВЕННОЕ место во всём nova_rt, где
    выполняется битовый RMW на `nova_atomic_bool` (проверено грепом:
    `Nova_AtomicBool_method_fetch_(or|and|xor)` встречается только в
    `sync_primitives.h`, нигде не вызывается из scheduler-кода — это чистая
    stdlib-API-поверхность для `Atomic[bool].fetch_or/fetch_and/fetch_xor`
    из пользовательского Nova-кода). Все остальные `nova_atomic_bool`-сайты
    (scheduler-флаги) используют только load/store/exchange/cmpxchg — их gcc
    разрешает без изменений (тот же `Nova_AtomicBool_method_cmpxchg` рядом
    использует `__atomic_compare_exchange_n` на `_Bool` и компилируется чисто).

**Реализация:** load+CAS retry loop — тот же идиом, что уже используется в
этом файле чуть выше для `fetch_max`/`fetch_min` на `AtomicI64`/`AtomicI32`/…
(`int mo = nova_mo_c(ord); T cur = load(RELAXED); while (!cas(&value,&cur,new,true,mo,RELAXED)) {}`).
Семантика идентична прямому интринсику: для bool∈{0,1} битовый OR/AND/XOR
== логический OR/AND/XOR; CAS-цикл возвращает значение, наблюдённое
непосредственно перед успешной записью — то же, что вернул бы
`__atomic_fetch_*`. `_bool`-суффиксные версии (default ordering) используют
`__ATOMIC_SEQ_CST` success + `__ATOMIC_RELAXED` failure — точно тот же
паттерн, что уже применяется в SeqCst-варианте `fetch_max`/`fetch_min` рядом.
Никакой scheduler-логики/порядка не тронуто — только 6 stdlib-методов.

## Категория 3 — pointer-mismatch ternary (`const uint8_t*` vs `char*`-литерал)

**Причина.** `nova_str.ptr` — `const uint8_t*` (ABI, `vtables.h:44`). Голый
строковый литерал (`""`, `"?"`) в C имеет тип `char*` (НЕ `const char*`, в
отличие от C++). Тернарник `cond ? msg.ptr : ""` смешивает эти два типа —
gcc 14+ это тоже ошибка (`-Wincompatible-pointer-types`).

**Фикс** (точечные касты, без изменения поведения):
- `compiler-codegen/nova_rt/effects.h::nv_exit` — `msg.len > 0 ? msg.ptr : ""`
  → `msg.len > 0 ? msg.ptr : (const uint8_t*)""`.
- `compiler-codegen/nova_rt/bench.h::nova_bench_emit_metric` — два тернарника
  (`name.ptr ? name.ptr : "?"`, `unit.ptr ? unit.ptr : ""`) → те же касты
  `(const uint8_t*)"?"` / `(const uint8_t*)""`.

## Гейты

**WSL2 Ubuntu (gcc 15.2.0 / clang 21.1.8)** — `~/rtheaders_check/compiler-codegen`
(та же реплика `build_rt_archive_lib`'s Unix-ветки, `~/rt_check.sh`), nova_rt
обновлён rsync'ом фиксов из worktree (без vendored `libuv/`, кроме
`libuv/include`):
- **gcc 15.2.0: ARCHIVE_OK** — все 13 `.c` компилируются ЧИСТО (пустой
  stderr на каждом), `ar rcs` собрал `libnova_rt.a`.
- **clang 21.1.8: ARCHIVE_OK** — все 13 `.c` компилируются ЧИСТО (не сломан).

**Windows (`nova-cli`, worktree `nova-gcc15`)**:
- `cargo build --release` (nova-cli) — чисто, только pre-existing warnings.
  Finished в 4m33s.
- `NOVA_RT_ARCHIVE=1`, кэш пуст → dummy-программа: `libnova_rt.lib built
  (13 files)` с нуля, бинарь собрался и запустился (`hi`).
- `nova test spec_tests/conformance/standalone` — **PASS 70 / FAIL 0**
  (полный набор, вкл. `pos_max_fibers_concurrent`, `supervisor_parfor_test`,
  `supervisor_stop_test`).
- `pos_max_fibers_concurrent` + `supervisor_stop_test` — **5× подряд, все
  PASS** (планировщик/атомики не сломаны категорией 2).
- Флагман (`examples/flagship/aggregator/src/main.nv`, `--strict-effects`) —
  собрался (62.98s, только pre-existing warnings), запущен на
  `AGGREGATOR_PORT=8199`, `curl http://127.0.0.1:8199/` → `HTTP 200` с HTML
  фронтенда ("Nova · Concurrent Aggregator"). Процесс завершён вручную
  (`taskkill`).
- Мега-CU НЕ гонял (по инструкции).

## Затронутые файлы (только эти 4, ничего в запрещённой зоне)

- `compiler-codegen/nova_rt/fibers.h` — тег `NovaFiberQueue`/`NovaBlockingState`
  (категория 1).
- `compiler-codegen/nova_rt/sync_primitives.h` — 6 fetch_or/and/xor методов
  `Nova_AtomicBool` переписаны на CAS-retry (категория 2).
- `compiler-codegen/nova_rt/effects.h` — `nv_exit` ternary-cast (категория 3).
- `compiler-codegen/nova_rt/bench.h` — `nova_bench_emit_metric` ternary-cast
  ×2 (категория 3).

`fiber_arena.c`/`fiber_arena_win.c`/`alloc_boehm.c` (Boehm-зона) НЕ тронуты —
только читались (rsync их синхронизировал вместе с остальными `.c`, но diff
по ним нулевой, компилировались уже без ошибок ДО этой волны).

## Вывод

Все 3 категории gcc15-класса ошибок из backlog `[M-linux-mn-conformance-red]`
закрыты. rt-архив (Plan 218) теперь собирается чисто на ОБОИХ toolchain'ах
WSL2 (gcc 15.2.0 и clang 21.1.8) — устраняет необходимость fallback на
per-build inline compile на этой машине. Windows-гейт (prod path,
`nova-cli`) подтверждает, что планировщик/атомики не сломаны: 70/0 conformance
+ 5× concurrency-стресс + флагман под `--strict-effects` живой.
