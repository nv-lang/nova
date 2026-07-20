# [M-218-rt-archive-parallel-jobs-race] — фикс-заметки

**Статус:** ЗАКРЫТО 2026-07-20, worktree `nova-218race`, ветка `p-fix-218-archive-race`, sonnet.

## Где была гонка

`compiler-codegen/src/test_runner.rs::detect_or_build_rt_archive` (Plan 218,
докстрок над `pub fn detect_or_build_rt_archive` — было ~4931). `nova test
--jobs N` — это **N ПОТОКОВ внутри ОДНОГО процесса**
(`std::thread::scope`/`spawn_scoped`, test_runner.rs ~5983-6079), НЕ отдельные
OS-процессы, как можно было предположить из формулировки бага. Все воркеры
зовут `detect_or_build_rt_archive` конкурентно.

Гонка была в разрыве между чтением и записью:

```rust
let memo = RT_ARCHIVE_MEMO.get_or_init(...);
if let Ok(guard) = memo.lock() {
    if let Some(cached) = guard.get(&memo_key) { return cached.clone(); }
}          // <-- lock ОТПУЩЕН здесь
// окно гонки: N потоков видят memo-miss
let result = (|| { ... if lib_file.is_file() { return ...; } build_rt_archive_lib(...) })();
if let Ok(mut guard) = memo.lock() { guard.insert(memo_key, result.clone()); }
```

При холодном кэше (или просто первой сборке конкретного bucket'а — ключ
= mode/gc_kind/OS/libuv/effect-count/runtime-defines/compiler-fingerprint,
`rt_archive_key`) несколько потоков одновременно проходили и
memo-miss, и `lib_file.is_file() == false`, и ВСЕ шли в
`build_rt_archive_lib` **на ОДИН И ТОТ ЖЕ `cache_dir`**:
- общий `cache_dir/obj/` — `remove_dir_all` одного потока сносил
  объектники, которые другой поток параллельно писал/читал;
- общий `cache_dir/compile.rsp` / `cache_dir/lib.rsp` — перезаписи вперемешку;
- общий финальный `lib_file` — несколько `lib.exe`/`ar` одновременно
  писали в один путь без атомарности.

Симптом воспроизведён «живьём»: PRE-FIX прогон (`--jobs 4`, холодный кэш,
28 файлов `spec_tests/conformance/neg/f*.nv`) x10 дал
`build_triggers=4` на КАЖДОЙ итерации (все 4 воркера реально входили
в `build_rt_archive_lib`) и на итерации 8 — настоящий
`CC-FAIL spec_tests/conformance/neg/f5_uncaught_trace_throw`:
`clang: error: no such file or directory:
'...\target\rt-archive-cache\<key>\libnova_rt.lib'` — ровно тот класс
флейка, что в backlog.

## Образец — как решает соседний libuv-кэш

`detect_or_build_libuv` (test_runner.rs ~4257) **тоже** не имеет
файл-лока/atomic-rename — но ему это не нужно: он вызывается ОДИН РАЗ
за весь `nova test`-запуск, ДО того как воркер-пул стартует (main.rs/
daemon.rs зовут его последовательно перед `std::thread::scope`). Так что
он не образец атомарной публикации, а образец «не нужен лок, если нет
конкурентных вызывающих» — контраст с `detect_or_build_rt_archive`,
который зовётся ИЗНУТРИ каждого воркер-потока (per-file, в `build_command`).

Реальный образец атомарной публикации — `nova-cli/src/build_cache.rs`
(`store_c`): пишет во временный файл `{key}.{pid}.tmp`, затем
`std::fs::rename` в финальный путь; докстрока прямым текстом: «Запись
атомарна (через temp-файл + rename), чтобы параллельные сборки не
прочитали наполовину записанный файл». Этот приём переиспользован.

## Фикс (два слоя, оба в `test_runner.rs`)

**1. Сузить окно гонки — держать ОДИН guard через всю
check→build→memoize последовательность** (`detect_or_build_rt_archive`):
раньше лок брался дважды (проверка, потом вставка) с разрывом между
ними; теперь один `MutexGuard` держится от начала до конца функции.
Первый поток, дошедший до данного (или, в редком случае — до ЛЮБОГО)
bucket'а, реально строит архив; все остальные потоки блокируются на
мьютексе и либо сразу видят уже заполненный memo (тот же bucket —
типичный случай: большинство файлов в одном `nova test`-прогоне делят
один bucket, см. докстрока `rt_archive_key`), либо строят СВОЙ другой
bucket сразу после (редкий случай). Побочный эффект: конкурентные
воркеры больше НЕ дублируют компиляцию архива — `build_triggers=1`
вместо `build_triggers=4` на холодном кэше (см. гейты ниже).

**2. Атомарная публикация на диск — defense-in-depth**
(`build_rt_archive_lib`): весь вывод одной попытки сборки (obj-файлы,
`.rsp`, сама слинкованная библиотека) пишется в УНИКАЛЬНЫЙ scratch-каталог
`cache_dir/.build-<pid>-<counter>-<nanos>` (`unique_build_tag()`), а не в
фиксированный общий `cache_dir/obj`. Финальный `lib_file` публикуется
ОДНИМ `std::fs::rename` в самом конце — читатель либо не видит файла,
либо видит полностью готовый, никогда частично записанный. Если
`rename` падает, но `lib_file` уже существует (другой ОС-процесс —
единственный случай, который лок из пункта 1 НЕ покрывает, — успел
опубликовать раньше), это трактуется как успех (контент
адресуется тем же bucket-ключом, т.е. байт-идентичен). Scratch-каталог
подчищается best-effort в конце независимо от исхода.

Мьютекс из пункта 1 закрывает РЕАЛЬНО НАБЛЮДАЕМУЮ гонку (потоки одного
процесса — `nova test --jobs N`). Rename из пункта 2 — защита на случай
отдельных `nova`-процессов, которые этот мьютекс не видит (например,
параллельный `nova build` в другом терминале на тот же `target/`) —
не тот механизм, что дал репродюсящийся баг, но тот же класс проблемы.

## Гейты

- **Repro (PRE-FIX, подтверждение диагноза):** `--jobs 4`, чистый кэш
  перед каждым запуском, x10, батч `spec_tests/conformance/neg/f*.nv`
  (28 файлов) — `build_triggers=4` на каждой итерации, FAIL получен на
  итерации 8 (`CC-FAIL f5_uncaught_trace_throw`, отсутствующий
  `libnova_rt.lib`).
- **Repro (POST-FIX):** тот же прогон x10 — `f1_parse_message_positive`
  PASS на всех 10, `build_triggers=1` на каждой итерации (одна реальная
  сборка, не N дублей), `PASS: 26 FAIL: 0 SKIP: 2` стабильно.
- **std/checksums+collections, `--jobs 4` x5** (чистый кэш перед каждым):
  `PASS: 16 FAIL: 0 SKIP: 9` стабильно все 5 раз.
- **Standalone-CU FAIL:0:** `spec_tests/conformance/neg` целиком (406
  файлов) с `--full --jobs 4`, тёплый кэш: `PASS: 405 FAIL: 0 SKIP: 3`
  (3 skip — легитимные «no test blocks and no fn main()», не регрессия).
- **218-выигрыш сохранён:** на тёплом кэше НЕ появляется сообщение
  `libnova_rt archive not built...` (fast-path `lib_file.is_file()` —
  подтверждено grep'ом по выводу); сравнение `NOVA_RT_ARCHIVE=1`
  (default, архив тёплый) vs `NOVA_RT_ARCHIVE=0` (pre-218 inline-путь)
  на одном файле — архивный путь стабильно быстрее (~3.2с vs ~3.5-3.8с
  на лёгком тестовом файле, включая полный процесс discovery+
  typecheck+link+run, не только саму компиляцию nova_rt).
- Мега-CU (агрегированный `spec_tests/conformance` одним CU) — НЕ
  гонялся (вне зоны/директивы задачи).

## Зона правки

Только `compiler-codegen/src/test_runner.rs`:
`detect_or_build_rt_archive` (сужение критической секции) +
`build_rt_archive_lib` (уникальный scratch-каталог + atomic-rename
публикация) + новая приватная `unique_build_tag()`. Ничего в
`types/mod.rs`, `emit_c.rs`, `lints.rs`, `nova-cli/src/**` не тронуто.
