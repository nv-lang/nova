# rt-headers-notes — дочинка missing-includes в nova_rt/*.h

**Дата:** 2026-07-20. **Worktree:** `nova-rtheaders`, ветка `p-fix-rt-headers`.
**Модель:** sonnet. **Триггер:** app_effect-диагностика (WSL-волна `p-fix-linux-appeffect`,
2026-07-20) — `libnova_rt`-архив (Plan 218) не собирается на свежих toolchain
(gcc 15.2 / clang 21.1), заставляя fallback на медленный per-build inline compile.
`deque.h` уже дочинен (`+#include <stdlib.h>`, коммит `a3dda126e` в main).

## Метод

1. Прочитан backlog `[M-linux-mn-conformance-red]` — известные подозреваемые:
   `typeid.h`, `sync_primitives.h`, `bench.h`.
2. Просканированы **ВСЕ** `*.h` в `compiler-codegen/nova_rt/` (31 файл, кроме
   vendored `libuv/`) — grep каждого файла на использование стандартных
   libc-символов (malloc/free/memcpy/fprintf/abort/strlen/…) сверх собственных
   `#include`. Каждое совпадение верифицировано вручную (`grep -n 'symbol('`
   с реальным call-сайтом, не комментарием/doc-строкой) — большинство «находок»
   широкого regex оказались ложными (упоминания в комментариях: «alloc.c uses
   calloc», «ceil(262144/64)» и т.п.).
3. Учтён design-паттерн этого кодбейза: некоторые заголовки (`fibers.h`,
   `effects.h`, `bench.h`) сами делают `#include "nova_rt.h"` первой строкой —
   это ПРЕДНАМЕРЕННЫЙ bootstrap-приём (guard `NOVA_RT_H`/их собственный guard
   защищает от рекурсии), они реально self-contained. Остальные заголовки
   ПОЛАГАЮТСЯ на порядок инклудов внутри `nova_rt.h`-зонтика (не баг per se —
   но НЕ self-contained при соло-компиляции, ровно как был `deque.h`).

## Найдено и исправлено (9 файлов, только `#include`-строки)

| Файл | Добавлено | Символ(ы), которые требовали |
|---|---|---|
| `channels.h` | `<stdio.h>`, `<stdlib.h>` | `fprintf`, `abort`, `malloc`, `free`, `getenv`, `atexit` (Time.after TLF, timer-metrics atexit hook, select_park abort) |
| `nova_sched.h` | `<stdio.h>`, `<stdlib.h>` | `fprintf`, `abort` (grow_state/get_state/gopark/park/park_with_unlock invariant-fail paths) — файл раньше не имел НИ ОДНОГО своего `#include` |
| `typeid.h` | `<string.h>`, `"alloc.h"` | `memcpy` (`nova_any_box`); `nova_alloc` (`nova_any_box`/`nova_any_from_boxed`) — см. ниже, найдено РЕАЛЬНОЙ компиляцией `typeid.c` |
| `vtables.h` | `<string.h>`, `<stdbool.h>` | `memcpy` (`_vt_nova_f64_hash`); `bool` — в standalone-fallback `#ifndef NOVA_RT_H` блоке типов (`typedef bool nova_bool;`) |
| `sync_barrier.h` | `<stdio.h>`, `<stdlib.h>` | `fprintf`, `malloc`, `free` (NovaBarrierTLFHandle, raw-malloc'd) |
| `sync_condvar.h` | `<stdio.h>`, `<stdlib.h>` | `fprintf`, `abort`, `malloc`, `free` (NovaCondvarTLFHandle) |
| `sync_countdown_latch.h` | `<stdio.h>`, `<stdlib.h>` | `fprintf`, `abort`, `malloc`, `free` (NovaCDLTLFHandle) |
| `sync_semaphore.h` | `<stdio.h>`, `<stdlib.h>` | `fprintf`, `abort`, `malloc`, `free` (NovaSemaphoreTLFHandle) |
| `plan115_ffi_test.h` | `<string.h>` | `strlen` (`nova_fn_p139_cstr_strlen`) |

Все 4 `sync_*.h`-фрагмента (barrier/condvar/countdown_latch/semaphore) реально
`#include`-ятся из `sync_primitives.h` (строки ~2166-2169, НЕ text-splice) —
до этой точки `sync_primitives.h` уже сам тянет `<stdio.h>`/`<stdlib.h>`, так
что в проде это НИКОГДА не падало; но при честной соло-проверке — тот же класс
латентного бага, что и `deque.h`.

`typeid.h`'s `nova_alloc`-геп найден НЕ грепом, а прямой компиляцией
`typeid.c` (реальный `.c` из `rt_archive_sources`, единственный, кто инклудит
`typeid.h` БЕЗ предварительного `nova_rt.h`/`alloc.h` bootstrap) — gcc 15 дал
`implicit declaration of function 'nova_alloc'` → `int`→`void*`
`-Wint-conversion` (оба error-by-default на gcc14+). Это единственный
Nova-internal (не-libc) символ в списке — включён т.к. verified реальным
archive-build breakage, не гипотеза.

Остальные 30 заголовков (включая `sync_primitives.h`, `bench.h`, `sync.h`,
`runq.h`, `runtime.h`, `driver.h`, `array.h`, `effects.h`, `fibers.h`, `net.h`,
`fs.h`, `eventloop.h`, `io_console.h`, `os_env.h`, `string_builder.h`,
`math.h`, `string.h`, `contracts.h`, `conv.h`, `cast.h`, `alloc.h`,
`nova_msvc_compat.h`) — self-contained уже, либо через собственный
bootstrap-паттерн (`#include "nova_rt.h"`), либо через полный собственный
список `#include`. `minicoro.h` — vendored single-header (как libuv),
не трогал.

## Гейты

**Windows (`cl.exe`, реальный prod toolchain для Plan-218-архива):**
- `cargo build --release` (nova-cli) — чисто, только pre-existing warnings.
- `NOVA_RT_ARCHIVE=1`, кэш очищен, `nova build` dummy-программы — архив
  собрался с нуля (`libnova_rt.lib built (13 files)`), бинарь запустился.
- `nova test spec_tests/conformance/standalone` (68 фикстур, включая
  `pos_max_fibers_concurrent`, `supervisor_parfor_test`, `supervisor_stop_test`) —
  **PASS 68 / FAIL 0** (дважды: до и после добавления `alloc.h` в `typeid.h`).

**WSL2 Ubuntu (gcc 15.2.0 / clang 21.1.8)** — тот же стенд, что диагностика
2026-07-20 (`~/nova-appeffect-wsl`), плюс отдельная копия `compiler-codegen`
(`~/rtheaders_check`) для ручной репликации `build_rt_archive_lib`'s
Unix-ветки (те же флаги: `-O0 -g -w -c -fPIC -D_GNU_SOURCE -DNOVA_GC_BOEHM
-DGC_THREADS -DNOVA_USE_LIBUV=1`):
- **clang 21.1.8: ARCHIVE_OK** — все 13 `.c` компилируются чисто, `ar rcs`
  собрал `libnova_rt.a`. Include-фиксы полностью закрывают clang-класс
  проблем backlog-заметки (implicit-declaration / C23-режим).
- **gcc 15.2.0: ARCHIVE ВСЁ ЕЩЁ FAILED** — но НЕ из-за missing-include.
  После моих фиксов `typeid.c` компилируется чисто; остаются 3 категории
  ошибок в `effects.c`/`runtime.c`/`driver.c`/`net.c`/`fs.c`/`eventloop.c`,
  ни одна не include-related:
  1. **`struct NovaFiberQueue*` vs `NovaFiberQueue*`** (44 error) — `driver.h`
     форвард-декларирует `struct NovaFiberQueue;` (тегированный incomplete
     type), а `fibers.h` определяет `typedef struct { ... } NovaFiberQueue;`
     (АНОНИМНЫЙ struct) — это ДВА разных C-типа. Аналогично
     `struct NovaBlockingState*`. Архитектурный баг структуры тэгов,
     пронизывает `fibers.h`/`driver.h`/`driver.c`/`runtime.h`.
  2. **`__atomic_fetch_and/or/xor` на `nova_atomic_bool*` (`_Bool*`)** (36 error)
     — `sync_primitives.h` `Nova_AtomicBool_method_fetch_{and,or,xor}_*` — ЭТО
     И ЕСТЬ backlog-флаг «gcc: `__atomic_fetch_and` на `_Bool*`». Не include —
     `sync_primitives.h` уже имеет `<stdio.h>`+`<stdlib.h>` целиком.
  3. **Pointer-type mismatch в тернарнике** (`const uint8_t*` vs `char*`
     string-literal) в `effects.h::nv_exit` и `bench.h::nova_bench_emit_metric`
     — backlog-флаг «pointer-type ternary». Тоже не include.
  GCC 14+ (в т.ч. 15.2 здесь) продвинул `-Wincompatible-pointer-types` из
  warning в error-by-default для C — `-w` (реальный prod-флаг) это НЕ
  подавляет (verified: `-w` один не помог, требуется отдельный
  `-Wno-error=incompatible-pointer-types`). Clang 21 на этом же коде и с теми
  же флагами не эскалирует эти диагностики — отсюда расхождение
  clang OK / gcc FAILED на идентичном источнике.

**ВЫВОД:** include-гигиена (задача этой волны) ЗАВЕРШЕНА и подтверждена на
обоих компиляторах (clang чисто, gcc — типа-ошибок больше НЕ include-класса).
Оставшиеся 3 категории gcc-ошибок — **ВНЕ ОБЪЁМА** этой волны (не
`#include`-строки, реальные type-design правки в `fibers.h`/`driver.h`
(struct tag unification), `sync_primitives.h` (`_Bool`-atomic redesign),
`effects.h`/`bench.h` (ternary cast/type fix) — по правилу задания «любая
правка сверх include = СТОП+доклад». Задокументировано в backlog отдельным
датированным пунктом для будущей волны.

**Мега-CU НЕ гонял** (по инструкции) — только `spec_tests/conformance/standalone`
(68 фикстур, отдельные compile units) на Windows.

## Хэши / артефакты

- Ветка `p-fix-rt-headers`, база `main` @ `2d9a15acc`.
- `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` → `D:\Sources\nv-lang\nova\compiler-codegen\vcpkg_installed\x64-windows-static\{lib,include}`.
- libuv submodule скопирован из main + `.git` удалён (`scripts/tools/setup_worktree_p118.sh`).
- WSL-репро: `~/rtheaders_check` (копия `compiler-codegen` с фиксами) +
  `~/rt_check.sh` (скрипт, реплицирует `build_rt_archive_lib` Unix-ветку).
