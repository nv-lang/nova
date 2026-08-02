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

## Ф.3 — реальная сборка выявила 2 доп. разрыва (найдены прогоном, не чтением)

Ручная амальгама-компиляция была необходимым, но НЕ достаточным условием —
реальный `nova build hello.nv` end-to-end вскрыл ДВА мест, где обвязка
не была fallback-aware:

1. `boehm_cfg` вычислялся `detect_boehm`-ONLY (без `.or_else(fallback)`) ЕЩЁ
   в ДВУХ местах помимо раннего `resolve_gc_or_exit`-чека:
   `build_command`'s собственный `boehm_cfg` (реальные `/I`/`/link` флаги),
   `compile_multi_tu_to_exe`'s собственный `boehm_cfg` (Plan 209 Ф.2
   multi-TU путь), И `detect_or_build_rt_archive`'s собственный `boehm_cfg`
   (Plan 218 prebuilt `libnova_rt.lib`). Без синхронизации ВСЕХ четырёх —
   fallback-сборка отработала бы в раннем чеке, но реальные флаги
   компиляции тихо падали бы обратно на несуществующий vcpkg-путь. Все
   четыре теперь используют identical `detect_boehm(...).or_else(||
   detect_or_build_boehm_fallback(...))`.
2. `atomic_ops.lib` линковался БЕЗУСЛОВНО в ТРЁХ местах (MSVC `/link`-фаза
   `build_command`, Clang Windows-ветка `build_command`, Clang Windows-ветка
   `compile_multi_tu_to_exe`) — фикс: `.is_file()`-guard на каждом (vcpkg-
   путь не меняется, fallback больше не падает «cannot open atomic_ops.lib»
   за файл, который ему не нужен).
3. `nova_rt` использует ОБЕ конвенции инклюда — `<gc.h>` (большинство
   файлов) И `<gc/gc.h>`/`<gc/gc_mark.h>` (fiber_arena.c/fiber_arena_win.c
   only) — vcpkg-порт кладёт заголовки ОБОИМИ способами (подтверждено
   листингом `vcpkg_installed/.../include/gc/gc.h`), сырое дерево
   сабмодуля bdwgc — только плоско. Фикс: `populate_boehm_include_dir`
   копирует нужные `.h` в `cache_dir/include/` (плоско) И
   `cache_dir/include/gc/` (namespaced) — НЕ в сам сабмодуль (чтобы не
   грязнить его working tree).

Оба класса разрывов найдены ТОЛЬКО прогоном реального `nova build
hello.nv` в чистом клоне (не код-ревью, не чтением кода заранее) —
итеративно: compile → link-error → fix → recompile → следующая ошибка,
3 раунда до первого чистого PASS.

## Приёмка (гейты из брифа)

**Гейт 1 — чистая установка (главный):** реальный `git clone --recursive`
(из этого worktree, ASCII-путь `D:\Sources\nv-lang\_gc269_clean_test\` —
Temp-путь с кириллицей в имени пользователя вскрыл НЕСВЯЗАННЫЙ
pre-existing баг в `build_libuv_lib`'s rsp-файле, нет BOM, вне scope этого
окна, задокументирован ниже отдельно) → `cargo build --release`
(nova-cli, 2m19s-3m05s по прогонам) → `nova build hello.nv -o hello.exe`
БЕЗ `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR`/`VCPKG_ROOT` в env — дословный
хвост финального зелёного прогона:

```
nova: Boehm GC (gc.lib) not found via $NOVA_GC_LIB_DIR/vcpkg — building from vendored bdwgc submodule (one-time, ~10 sec)...
nova: gc.lib built (bdwgc extra/gc.c amalgamation)
nova: gc.lib built from vendored bdwgc source (D:\Sources\nv-lang\_gc269_clean_test\target\gc-cache\gc.lib)
nova: libnova_rt archive not built for this config, building (one-time, ~5-7 sec)...
nova: libnova_rt.lib built (13 files)
nova: libnova_rt archive built (D:\Sources\nv-lang\_gc269_clean_test\target\rt-archive-cache\cf22875f5521d114\libnova_rt.lib)
built: hello.exe (17.05s)
BUILD_EXIT=0
=== running hello.exe ===
Hello, Nova!
RUN_EXIT=0
```

Повторный прогон (cache-hit, без пересборки GC): `build cache hit —
reusing generated C`, `built: hello2.exe (3.40s)`, вывод `Hello, Nova!`
байт-в-байт. ВЕРДИКТ: **PASS**.

**Гейт 2 — vcpkg-путь не сломан:** worktree `nova-gc269` (без своего
`vcpkg_installed`) с `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR`, указывающими
на РЕАЛЬНЫЙ vcpkg главной репы (`d:/Sources/nv-lang/nova/compiler-codegen/
vcpkg_installed/x64-windows-static/{lib,include}`) → `nova build hello.nv`
— fallback НЕ триггерится (env-приоритет №1 не тронут), собирается,
печатает «Hello, Nova!». `nova test spec_tests/conformance/append_self.nv
spec_tests/conformance/slice_gc_alive.nv` (тот же vcpkg env) — дословный
хвост:

```
Toolchain: clang, mode=Dev, jobs=16, paths=[...append_self.nv, ...slice_gc_alive.nv]
PASS           spec_tests/conformance/append_self
PASS           spec_tests/conformance/slice_gc_alive
===== SUMMARY =====
PASS: 2  FAIL: 0
```

ВЕРДИКТ: **PASS**.

**Гейт 3 — ratchet/чекер:** этим окном НЕ трогается ни один checker-канал
файл (`types/mod.rs` и т.п.) — только `test_runner.rs` (build-driver),
`alloc.c`/`alloc_boehm.c` (rt), `.gitmodules`/новые сабмодули, README.md.
Ratchet структурно не может вырасти. ВЕРДИКТ: **PASS (by construction)**.

**Гейт 4 — cargo чистый:** `cargo build --release` (`compiler-codegen` +
`nova-cli`) — 0 ошибок, ТОЛЬКО pre-existing warnings (dead-code и т.п. в
файлах, которых это окно не трогало — `field_cache.rs`, `types/mod.rs`,
`crosscheck.rs`, `main.rs`); grep подтвердил 0 warnings, указывающих на
`test_runner.rs`. Отдельно проверено: `cargo test --release --lib
test_runner` падает stack-overflow на `doc::test_runner::tests::
compile_fail_passes_when_fails` — ПОДТВЕРЖДЕНО pre-existing (идентичный
краш байт-в-байт на НЕМОДИФИЦИРОВАННОМ `main`, тот же тест, тот же
STATUS_STACK_OVERFLOW) — НЕ регрессия этого окна.

**Мега-CU/флагман (--strict-effects):** по брифу — работа интегратора при
приёмке, этим окном не прогонялся.

## Побочные находки (НЕ в scope этого окна, задокументированы, не тронуты)

- `build_libuv_lib`'s rsp-writer (test_runner.rs) не имеет UTF-8 BOM —
  ломается на путях с кириллицей (тот класс бага, что `link_prep.rs`'s
  `build_vendor_ffi_lib` уже чинил себе BOM'ом, но `build_libuv_lib` не
  получил тот же фикс). Проявляется ТОЛЬКО когда фактический путь резолва
  содержит не-ASCII байты (напр. `C:\Users\<кириллица>\...`) — этот
  worktree/сессия сама под таким профилем, поэтому обнаружено. НЕ чинится
  этим окном (rt/build, но другая функция/другой баг-класс, вне brief'а
  269/278) — оставлено как находка для отдельного номера/окна.
- `test_mt*.c`/`do_*.bat` синтетические скретч-тесты (сегфолт на
  ЛЮБОМ `CreateThread`/`_beginthreadex`-построенном потоке ПОСЛЕ
  `GC_unregister_my_thread`, воспроизводится байт-в-байт И на
  production vcpkg `gc.lib`) — баг МОЕГО тест-харнесса, не бага в
  амальгама-сборке ни в проде; не отслеживается отдельным номером
  (не воспроизводится в реальном `nova_rt`, который уже использует
  корректный протокол потоков).
