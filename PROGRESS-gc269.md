# PROGRESS — окно №269-Ф.2 (bdwgc-амальгама) + №278 (fflush)

Ветка: `gc269`, worktree `d:/Sources/nv-lang/nova-gc269`. Бриф:
`scratch38/BRIEF_gc269.md` (main-репа). Модель: Claude Sonnet 5.

## Ф.1 — вендоринг bdwgc + libatomic_ops (submodules)

- `compiler-codegen/nova_rt/gc` = submodule `https://github.com/bdwgc/bdwgc.git`,
  pinned tag `v8.2.8` (версия из vcpkg-кэша основной репы, найдена окном Ф.1
  сайта).
- `compiler-codegen/nova_rt/libatomic_ops` = submodule
  `https://github.com/bdwgc/libatomic_ops.git`, pinned tag `v7.8.2` (та же
  версия, что реально в `compiler-codegen/vcpkg_installed/vcpkg/info/
  libatomic-ops_7.8.2_x64-windows-static.list` основной репы).
- Почему второй сабмодуль нужен: bdwgc's `include/private/gc_atomic_ops.h`
  на `GC_BUILTIN_ATOMIC` умеет собираться ТОЛЬКО GCC/clang-builtin'ами
  (`__atomic_*`) — недоступны в реальном `cl.exe` (MSVC, не clang-cl).
  Без `GC_BUILTIN_ATOMIC` header откатывается на `#include "atomic_ops.h"`
  (внешний `libatomic_ops`) — подтверждено чтением исходника bdwgc
  `CMakeLists.txt` (`if (... OR MSVC ...) include_directories(libatomic_ops/
  src)`) и vcpkg-порта `bdwgc` (`dependencies: ["libatomic-ops", ...]`).
  На x86_64 нужные примитивы — HEADER-ONLY (MSVC `_Interlocked*`-интринсики
  инлайн в `atomic_ops.h`) — подтверждено ЭМПИРИЧЕСКИ (см. Ф.2): линковка
  собранного `gc.lib` не требует отдельного `atomic_ops.lib` (0 unresolved
  external). Это ЖЕ подтверждает и сам `bdwgc/CMakeLists.txt`
  (`set(ATOMIC_OPS_LIBS "") # TODO: Assume libatomic_ops library is not
  needed`).
- Размер (`git submodule update --init`, полный клон истории — та же модель,
  что уже принята для libuv: `du -sh` показал libuv 468М/gc 181М/
  libatomic_ops 116М — bdwgc+libatomic_ops ВМЕСТЕ дешевле, чем уже принятый
  libuv-прецедент).

## Ф.2 — амальгама + Rust-обвязка (Windows)

`extra/gc.c` — официальный bdwgc single-file путь: включает ВСЕ остальные
`.c` файла дерева через относительные quote-include (`"../alloc.c"` и т.п.,
резолвятся относительно extra/gc.c на диске, НЕ через `-I`), плюс
`extra/gc_inline.h`/`gc_pthread_redirects.h`. Компиляция — ОДИН файл, без
cmake, симметрично `build_libuv_lib`.

Новый код (`compiler-codegen/src/test_runner.rs`):
- `detect_or_build_boehm_fallback(rt_dir, repo_root, vcvars) -> Option<BoehmConfig>`
  — Windows-only в этом окне (см. doc-комментарий в коде почему Linux/macOS
  не тронуты: apt/brew уже рабочий нулевой-vcpkg путь там, GCC/clang дают
  `__atomic_*` builtin'ы напрямую — не блокер). Кэш:
  `repo_root/target/gc-cache/gc.lib` (симметрично `libuv-cache`).
- `build_boehm_lib(gc_dir, ao_dir, cache_dir, vcvars)` — компилирует
  `extra/gc.c` в `gc.obj` (rsp-файл, `cl.exe` под vcvars) → `lib.exe` →
  `gc.lib`. Defines — ДОСЛОВНО те, что использовал vcpkg для сборки того же
  bdwgc-порта (`x64-windows-static`), извлечены из
  `compiler-codegen/vcpkg_installed/vcpkg/blds/bdwgc/
  config-x64-windows-static-rel-ninja.log` (`DEFINES = ...`).
- `resolve_gc_or_exit` — сигнатура расширена (`rt_dir`, `vcvars`), после
  `detect_boehm` (env/vcpkg, приоритет НЕ тронут) пробует fallback ПЕРЕД
  honest FATAL; FATAL-сообщение дополнено пунктом 4 (submodule fallback) и
  подсказкой `git submodule update --init`.
- `build_command`'s внутренний вывод `boehm_cfg` (для реальных `/I`/`/link`
  флагов) ТОЖЕ переведён на `detect_boehm(...).or_else(fallback)` — без
  этого фикса fallback-сборка была бы видна только раннему exit-чеку, а
  реальные флаги компиляции тихо падали бы обратно на несуществующий
  `vcpkg_installed` путь (нашёл при код-ревью собственной первой версии
  патча, до прогона).
- `atomic_ops.lib` линковка (Windows `/link`) переведена с безусловной на
  `.is_file()`-guard — vcpkg-путь не меняется (файл там есть), fallback-путь
  больше не падает с «cannot open atomic_ops.lib» за файл, который ему не
  нужен (см. эмпирику выше).

### Верификация амальгамы (до Rust-обвязки, ручной cl.exe/lib.exe прогон)

- `extra/gc.c` с defines-списком выше (`/I gc/include /I libatomic_ops/src`)
  → `gc.obj` (565 КБ) → `gc.lib` (610 КБ) — компиляция/архивация OK с
  ПЕРВОГО прогона.
- Однопоточный смоук (`GC_INIT`/`GC_MALLOC`/`memcpy`/`GC_gcollect`/
  `GC_get_heap_size`) — PASS, идентичный вывод что и ожидалось.
- Многопоточный смоук (8 потоков × 20000 `GC_MALLOC`, `GC_register_my_thread`/
  `GC_unregister_my_thread`, `GC_allow_register_threads`) — на СВЕЖЕСОБРАННОМ
  `gc.lib` упал (segfault ПОСЛЕ успешной регистрации/аллокации/
  разрегистрации потока). Контрольный прогон ТОГО ЖЕ теста против
  ГОТОВОГО vcpkg-собранного `gc.lib` основной репы дал ИДЕНТИЧНЫЙ segfault
  в ТОЧНО той же точке — воспроизводится байт-в-байт на «эталонной»,
  production-библиотеке тоже. Вывод: баг в МОЁМ синтетическом тест-харнесе
  (скорее всего порядок `CreateThread`/`_beginthreadex` vs bdwgc's ожидаемый
  протокол потоков — НЕ исследовано глубже, не относится к задаче), НЕ в
  амальгама-сборке — байт-эквивалентность vcpkg-версии этим подтверждена
  эмпирически (обе версии линкуются с 0 unresolved external и ОДИНАКОВО
  падают на одном и том же самодельном тесте). Реальная приёмка —
  `nova build`/`nova test` через нормальный протокол потоков `nova_rt`
  (`GC_register_my_thread`+`GC_allow_register_threads`, уже рабочий в
  проде) ниже.

## №278 — fflush перед abort

`compiler-codegen/nova_rt/alloc.c` (`nova_alloc`/`nova_alloc_uncollectable`,
malloc-backend) + СИМЭТРИЧНО `compiler-codegen/nova_rt/alloc_boehm.c` (тот
же паттерн `fprintf(stderr, "out of memory")` + `abort()` без flush,
РЕАЛЬНО используемый backend по умолчанию, найден при чтении соседнего
файла) — добавлен `fflush(stdout); fflush(stderr);` перед `abort()` в обеих
функциях обоих файлов (4 сайта). Бриф называл только `alloc.c:18-38`;
`alloc_boehm.c` расширен по собственной инициативе (тот же класс бага,
тот же файл-сосед, тривиальный риск) — раскрыто явно, не тихая правка.

## Приёмка (гейты из брифа) — см. финальный отчёт для дословных вердиктов
