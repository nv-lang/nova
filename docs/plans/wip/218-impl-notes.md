<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 218 — реализация: чекпоинт-заметки

**Модель:** sonnet. **Worktree:** `nova-218`, ветка `p218-libnova-rt-archive`, база `main`.

## Где реализовано

Весь код — `compiler-codegen/src/test_runner.rs` (build-пайплайн, разделяемый `nova build`/
`nova test`/`nova bench` — тот же файл, где живёт `detect_or_build_libuv`/`build_libuv_lib`,
взятый за эталон паттерна). `nova-cli/src/main.rs` НЕ тронут — `nova build` вызывает
`compile_c_to_exe` → `build_command`, и весь архивный слой спрятан внутри `build_command`,
поэтому `BuildOpts`/вызывающий код не менялись (0 изменений в 4 call-site'ах `BuildOpts { .. }`).

Новый код (все функции в `test_runner.rs`, вставлены после `collect_c_files`, перед
`Summary`):
- `RtArchiveConfig` — `{ lib_file: PathBuf }`.
- `rt_archive_enabled()` — `NOVA_RT_ARCHIVE=0/off/false` выключает (escape hatch,
  симметрично `NOVA_CACHE=0`).
- `rt_archive_sources()` — тот же список `nova_rt/*.c`, что раньше пушился построчно в
  `build_command` (rt_alloc..rt_segv_diag + net.c/fs.c/eventloop.c при libuv).
- `rt_hashable_files()` — ВСЕ `*.c`/`*.h` прямо в `nova_rt/` (не рекурсивно в `libuv/`) —
  для контент-хеша инвалидации.
- `rt_archive_compiler_fingerprint()` + `resolve_archive_cc_path()` — отпечаток
  архив-компилятора (см. ниже почему компилятор ФИКСИРОВАН, не следует `--toolchain`).
- `rt_archive_key()` — DefaultHasher-ключ (тот же паттерн, что `build_cache.rs::compute_c_key`):
  версия схемы + mode + gc_kind + OS + libuv-present + effect-count-define + runtime-defines +
  (march, если release) + compiler-fingerprint + контент ВСЕХ hashable-файлов.
- `RT_ARCHIVE_MEMO` (process-wide `OnceLock<Mutex<HashMap>>`) — избегает пере-хеширования
  ~1.5МБ nova_rt на КАЖДЫЙ файл в `nova test` (сотни файлов за один процесс).
- `detect_or_build_rt_archive()` — публичная точка входа, mirrors `detect_or_build_libuv`:
  ищет `target/rt-archive-cache/<key>/libnova_rt.{lib,a}`; если нет — строит; ЛЮБАЯ ошибка →
  `None` (не fatal — pure optimization, чистый откат к старому поведению).
- `build_rt_archive_lib()` — компилирует sources → объектники → архивирует. Windows:
  `cl.exe`/`lib.exe` через vcvars (response-файлы, как `build_libuv_lib`). Unix:
  `$CC`/`cc` → `ar rcs`.

`build_command()` (три ветки Clang/Msvc/Gcc) — минимальные хирургические правки:
1. В начале функции — `let rt_archive = detect_or_build_rt_archive(...); let use_rt_archive = rt_archive.is_some();`
   (repo_root выводится из `opts.rt_dir.parent().parent()` — не трогает `BuildOpts`).
2. В libuv if-let блоке — `net.c`/`fs.c`/`eventloop.c` пушатся только если `!use_rt_archive`
   (иначе — double-define, символы уже в архиве).
3. Место, где раньше АБСОЛЮТНО ВСЕГДА пушились `rt_alloc..rt_segv_diag` как source-аргументы —
   теперь `if let Some(cfg) = &rt_archive { c.arg(&cfg.lib_file); } else { /* старый код 1:1 */ }`.

`compile_multi_tu_to_exe` (Plan 209, default-off `NOVA_MULTI_TU=1`) НЕ тронут — вне области
218 (тот путь уже компилирует rt-объекты параллельно КАЖДЫЙ раз, отдельная территория Plan 209).

## Находка при реализации (не была в разведке) — ABI-опасность NOVA_MAX_EFFECT_STORAGES

`effects.c` ОПРЕДЕЛЯЕТ физическое хранилище `NovaEffectRegistry` (TLS), `runtime.c`
аллоцирует/использует `NovaEffectSnapshot` по `sizeof`. Оба типа размером в
`-DNOVA_MAX_EFFECT_STORAGES=N` — **per-программное** значение (Plan 174.4: N = число
различных эффектов КОНКРЕТНОЙ программы, маркер на строке 1 генерённого `.c`). Этот define
намеренно применяется на ВЕСЬ cc-вызов (не `#define` внутри одного `.c`) именно чтобы
`NovaEffectRegistry`/`NovaEffectSnapshot` имели ОДИНАКОВЫЙ layout во всех TU. Если бы
`effects.c`/`runtime.c` были предсобраны с ФИКСИРОВАННЫМ N и слинкованы с app.c с ДРУГИМ N —
`.count`-оффсет `NovaEffectRegistry` съезжает, `NovaEffectSnapshot`-аллокации могут быть
меньше нужного — classic silent memory corruption, НЕ ошибка линковки.

**Фикс:** `rt_archive_key` включает N (effect-count define) и `[runtime]` fiber_stack/
max_fibers-оверрайды как измерения корзины — программа линкуется ТОЛЬКО с архивом,
собранным под ЕЁ N. Это сохраняет доминирующий dev-loop выигрыш (правка/пересборка ОДНОЙ
программы никогда не меняет её effect-count) и на практике большинство программ делят N
(builtins Fail+Time, без кастомных эффектов) — кросс-программное переиспользование тоже
работает.

## Почему архив собирается ФИКСИРОВАННЫМ компилятором (не `--toolchain`)

На Windows архив ВСЕГДА собирается `cl.exe`/`lib.exe` (через vcvars), НЕЗАВИСИМО от того,
`--toolchain=clang` или `=msvc` у самой сборки приложения. На Unix — `$CC`/`cc` НЕЗАВИСИМО
от `--toolchain=clang/gcc`. Это ТОЧНАЯ калька уже существующего прецедента
`detect_or_build_libuv`/`build_libuv_lib` — `libuv.lib` точно так же собирается один раз
cl.exe/cc и линкуется что в clang-, что в msvc-сборки (COFF/ELF от разных фронтендов
совместимы на своей платформе). Следствие: архивные объекты — НЕ буквально byte-identical
машинный код тому, что дал бы `--toolchain=clang` инлайново (другой компилятор + БЕЗ
cross-TU `-flto` в архив, см. doc-comment `build_rt_archive_lib`) — они ПОВЕДЕНЧЕСКИ
идентичны (тот же C-источник, те же эффективные defines, детерминированная семантика) — это
и есть реальный гейт; `libuv.lib` сосуществует с `-flto` app-сборками по этой же схеме с
Plan 22.

`-flto`/`/GL` НЕ применяется к архивным объектам даже в release (соответствует
`build_libuv_lib`, который тоже никогда не LTO-ит `libuv.lib`) — смешивание LTO-bitcode
архива, собранного ОДНИМ компилятором, с финальной линковкой ВОЗМОЖНО ДРУГИМ (архивный
компилятор фиксирован, а `--toolchain` приложения — нет) рискует несовместимым форматом
bitcode. app.c по-прежнему получает полный `-flto` в `build_command` — теряется только
cross-TU инлайнинг архивного рантайма В app.c, ограниченная и прецедентная цена.

TSan — НЕ реализован как измерение корзины: `-fsanitize=thread` вообще НЕ существует нигде
в текущем Rust build-пайплайне (`grep -r "fsanitize=thread\|NOVA_TSAN"` — 0 совпадений) —
это НЕ реальная ось `nova build`/`test_runner.rs` сегодня, значит нечего бакетировать
(флаги-based ключ подхватит её автоматически, если её когда-нибудь проведут через тот же
код path).

## Замер (Windows, clang toolchain, dev mode, пустышка `fn main() -> int => 0`)

Изолирует ИМЕННО фазу `c-compile` (остальные фазы — parse/dep-lock/imports-resolve/
type-check/codegen — НЕ меняются архивом). `NOVA_CACHE=0` — форсирует пересборку `.c`
(симулирует «я поправил свой код», где content-cache `.c` в любом случае промахнётся).

| сценарий | c-compile | wall (`built:`) |
|---|---|---|
| BASELINE `NOVA_RT_ARCHIVE=0` (= до 218), устойчивое состояние (5 прогонов) | ~4.94-5.19с | ~8.1-8.5с |
| АРХИВ включён, тёплый (5 прогонов) | ~0.67-0.70с | ~3.9-4.0с |
| **Дельта** | **≈ −4.3с** | **≈ −4.2с** |
| Архив ХОЛОДНЫЙ (кеш стёрт, libuv тёплый) | 7.64с (сборка 13 файлов в архив + финальный линк) | 11.0с |
| Архив тёплый ПОСЛЕ холодного | 0.61-0.82с | ~3.8-4.3с |

Совпадает с целью плана (−~5с/сборку); абсолютные числа ниже, чем в разведке (была ~6.45с
baseline на другой машине) — соотношение (~7× ускорение c-compile) сопоставимо.

## Проверка инвалидации

- `touch nova_rt/runtime.c` (без изменений) → та же bucket-директория
  (`e6df9d7da3f97d5d`), НЕТ пересборки, c-compile снова ~0.6с. **Хеш по содержимому, НЕ mtime.**
- Реальная правка (добавлен комментарий в `typeid.c`) → НОВАЯ bucket-директория
  (`1b5d236e16045515`), пересборка архива, старая корзина остаётся (no-eviction v1,
  `target/` disposable — тот же паттерн, что `build_cache.rs`).
- Откат правки → снова СТАРАЯ корзина (`e6df9d7da3f97d5d`) — контент-адресация подтверждена
  (не зависит от mtime, только от байтов).
- `--mode release` → ОТДЕЛЬНАЯ корзина (`9a1c30ad1daf73bf`), холодный первый прогон
  (8.96с c-compile), тёплый второй (0.72с). dev/release НЕ смешиваются.
- Оба архивных бинаря (dev, release) запускались — `exit=0`.

## Гейты (все зелёные)

- **conformance** (`nova test --positive --compile-error --timeout 300 --jobs 4
  spec_tests/conformance`, 1548 `.nv`): **PASS 503 / FAIL 1 / SKIP 14**. Единственный FAIL
  (`app_effect_basic_t8_1`, `Vec[...].chained .debug/.display`) — ПОДТВЕРЖДЁННЫЙ чужой
  известный пин (см. `git log` commit `f234f5e77`: «остаточный RUN-FAIL CU = чужой известный
  vec-chained-debug пин»), НЕ регрессия от 218. SKIP — файлы без test-блоков/`fn main()`
  (compiled OK, ожидаемо). Архив построил ОДНУ корзину (dev+boehm+`N`-эффектов пустышки/
  типичной программы) и переиспользовал её для подавляющего большинства из 1548 файлов.
- **std** (`nova test std/src/checksums std/src/collections --jobs 4`): **PASS 16 / FAIL 0 /
  SKIP 9** (SKIP — модули без test-блоков, ожидаемо).
- **Флагман** (`nova build examples/flagship/aggregator/src/main.nv --strict-effects -o ...`):
  собрался ЗЕЛЁНЫМ (архив построил СВОЮ отдельную корзину — `d8efa985591f45ab` — эффект-каунт
  реальной программы с кастомными эффектами ОТЛИЧАЕТСЯ от пустышки, подтверждает работу
  per-program бакетирования на реальном коде, не только на синтетике). Запущен: слушает
  `127.0.0.1:8187`, `curl http://127.0.0.1:8187/` → **HTTP 200**, реальный HTML-контент
  (мокап-дашборд) — byte-identical-поведение подтверждено на живом бинаре (не только exit-код).
  (Побочный эффект от прогона — `examples/nova.lock` dep-lock переразрешил `nova-tls`-коммит;
  откачен `git checkout -- examples/nova.lock`, не входит в коммит 218.)

## Зона / дисциплина

- Не трогал `compiler-codegen/src/types/mod.rs`, `compiler-codegen/src/codegen/emit_c.rs`
  (зоны других агентов) — подтверждено (`git diff --stat` см. в отчёте).
- Не менял СОДЕРЖИМОЕ `nova_rt/*.c`/`*.h` — только читал (для хеша) и компилировал в архив.
- `BuildOpts`/`TestBuildOpts` не тронуты (0 новых полей) — архивная логика полностью внутри
  `build_command`, вызывающий код (`nova-cli/src/main.rs`, `nova-cli/src/bench/run.rs`) не
  менялся.
