# [M-nova-linux-build] — прогресс (worktree p-linux-build)

**Статус:** в работе. Чекпоинт обновляется перед каждым коммитом (среда WSL
долгая, обрывы вероятны — см. feedback-workflow-agents-checkpoint-progress).

## Среда

- WSL: Ubuntu 26.04 LTS (codename resolute), ядро 6.6.87.2-microsoft-standard-WSL2.
- Уже установлено системно (apt, без sudo-действий с моей стороны):
  - `cargo 1.93.1` / `rustc 1.93.1` (built from source tarball — уже был
    настроен до начала задачи, rustup ставить не пришлось).
  - `clang 21.1.8` (Ubuntu package), `gcc 15.2.0`.
  - `cmake 4.2.3`, `make 4.4.1`.
  - `libgc-dev 1:8.2.12-1` + `libgc1` (Boehm GC) — **уже стоит**, значит
    `detect_boehm()` (`compiler-codegen/src/test_runner.rs` #[cfg(target_os = "linux")])
    должен найти `/usr/include/gc/gc.h` без доп. действий.
- `sudo -n true` → требует пароль (интерактив). Пока НЕ понадобилось: все
  нужные пакеты (clang/cmake/make/libgc-dev) уже стояли. Если что-то ещё
  понадобится — здесь будет точный список `sudo apt install ...`.
- Диск: `/` (WSL rootfs) 1007G/950G свободно.
- Репозиторий смонтирован на `/mnt/d/Sources/nv-lang/nova` (медленный I/O
  через 9p для тяжёлых операций типа `du -sh` по всему дереву, но точечные
  read/ls — быстрые, ~0.2с).
- **Worktree:** `d:/Sources/nv-lang/nova-linux` (ветка `p-linux-build`,
  создан из `main` @ 283efd569). Из WSL путь: `/mnt/d/Sources/nv-lang/nova-linux`.
  Копия для сборки (нативный ext4, cargo не любит 9p): `~/nova-linux`
  (rsync, exclude `.git`/`target`). Патчи переносятся обратно в
  `/mnt/d/Sources/nv-lang/nova-linux` вручную (Read/Edit), т.к. это
  единственная копия под git-контролем.
- `git submodule update --init compiler-codegen/nova_rt/libuv` в
  nova-linux worktree — прошёл локально (объекты уже были в
  `.git/modules/compiler-codegen/nova_rt/libuv`, сети не потребовалось).
- Полный `rsync` всего дерева в `~/nova-linux` **абортирован** — дерево
  4.7G (`spec_tests` 1.6G, `nova_tests` 1.2G, `docs` 714M(!), `compiler-codegen`
  671M — из них `nova_rt/libuv` 468M, где `libuv/test`+`libuv/docs` = 308M
  НЕ нужны сборке). Скорость копирования ~5M/мин через 9p — часы на весь
  репо. **Решение:** собирать Rust-крейты прямо с `/mnt/d/...` (точечные
  read операции быстрые, ~0.2с/файл), но `CARGO_TARGET_DIR` вынести на
  native ext4 (`~/nova-target*`) — иначе cargo пишет тысячи мелких
  промежуточных файлов через 9p. На практике первая сборка
  `compiler-codegen` заняла 3м10с — приемлемо, копировать репо не
  потребовалось вообще.

## КРИТИЧЕСКАЯ НАХОДКА: системный `rustc 1.93.1` ICE на этом коде

`cargo build --release` в `compiler-codegen/` с системным (apt/tarball)
`rustc 1.93.1 (01f6ddf75 2026-02-11)` **падает воспроизводимо** (не флак —
два независимых прогона дали идентичную панику):

```
thread 'rustc' (N) panicked at .../library/alloc/src/vec/mod.rs:2796:36:
slice index starts at 52 but ends at 51
error: the compiler unexpectedly panicked. this is a bug.
query stack during panic:
#0 [check_liveness] checking liveness of variables in
   `codegen::emit_c::<impl at src/codegen/emit_c.rs:2026:1: 2026:14>::receiver_c_type`
#1 [analysis] running analysis passes on crate `nova_codegen`
```

(второй прогон — тот же паттерн на `prepare_method_recv`, тот же файл/строка
диапазона `2026:1: 2026:14`). Это **rustc ICE в MIR-borrowck liveness-запросе**
на `compiler-codegen/src/codegen/emit_c.rs` — не в Nova-коде, баг апстрима
(шаблон `slice index starts at N but ends at N-1` — известный класс NLL
liveness-паник на сложном control-flow). GitHub CI (`ubuntu-latest`, без
явного toolchain-шага — берёт то, что предустановлено раннером) видимо
использует другую (более старую/пропатченную) версию rustc и не ловит это.

**Workaround (подтверждён):** `rustup` (устанавливается в `$HOME`, **без
sudo**) + закреплённый stable `1.85.0` (= `rust-version` MSRV из
`compiler-codegen/Cargo.toml`) — **собирается чисто**, `Finished release
profile ... in 3m 10s`, 0 ошибок (только lint-warnings). Т.е. проблема не
в Nova, а в том, что Ubuntu 26.04 (`resolute`, ещё не релизный/edge-канал)
тащит слишком свежий системный rustc с нерелизным багом.

**Рекомендация для docs/linux-build.md:** не полагаться на
дистрибутивный rustc на Linux — ставить через `rustup` с версией из
`rust-version` в Cargo.toml (сейчас 1.85) либо `stable` через rustup
(который на момент баг-фикса апстрима тоже будет ОК; просто дистрибутивный
`rustc` — не source of truth).

## Находки по коду (до прогона сборки)

- В `compiler-codegen/nova_rt/` уже есть **обе** реализации fiber-арены:
  `fiber_arena.c` (POSIX: `mman.h`, `pthread.h`, `ucontext.h`, `signal.h`)
  и `fiber_arena_win.c` (Windows). POSIX-ветка присутствует — порт арены
  делать не нужно.
- `compiler-codegen/src/test_runner.rs` уже содержит развитую
  cross-platform логику (Plan 22/27/40/44.1 era):
  - `detect_boehm()` — Linux-ветка ищет `gc.h` в `/usr/include`,
    `/usr/include/gc/`, `/usr/local/include`; `resolve_gc_or_exit()` даёт
    честный `sudo apt install libgc-dev` hint.
  - `detect_or_build_libuv()` / `build_libuv_lib()` — Linux/macOS ветка
    компилирует libuv из vendored submodule через clang (`src/unix/*.c` +
    platform subset), кэш в `target/libuv-cache/libuv.a`.
  - Множество `#[cfg(target_os = "linux")]` веток в компиляции/линковке
    (net.c/fs.c libuv-gated, brotli, system libs).
- `docker/Dockerfile` + `docker/README.md` (Plan 40 Ф.1 Этап 5, датировано
  2026-05-12) — **Linux build уже был провалидирован через Docker**:
  Ubuntu 22.04 + clang-15 + libuv1-dev (apt) + libgc-dev (apt) →
  261/261 nova_tests + 46/46 std type-check PASS. Известный gap:
  `plan40_perf_bench` падает под Docker (Boehm `GC_init`/`GC_find_limit_with_bound`
  SEGV под restricted permissions — не баг Nova). Sanitizer-сборки (TSan/ASan/UBSan)
  тогда не завелись из-за single-thread Boehm apt-пакета — нужен Boehm
  собранный `--enable-threads=posix`.
  **Эта задача не про Docker** — верифицирует то же самое напрямую в WSL
  (иначе окружение), но предыдущая валидация — сильный precedent, что
  Linux-порт в целом жизнеспособен.

## ВАЖНО: 9p I/O — нужна native-fs копия для `nova build`/`nova test`

`cargo build --release` читает исходники Rust точечно (мало файлов) —
`/mnt/d/...` работает нормально (~3-6 мин). Но `nova build`/`nova test`
(рантайм CLI) при резолве workspace-модулей **рекурсивно обходит `std/`**
и это упирается в `p9_client_rpc` (подтверждено через
`/proc/<pid>/task/*/wchan` — поток `nova-main` буквально висел в 9p RPC
несколько минут на тривиальном hello-world, запущенном с CWD на `/mnt/d`).
**Решение:** держать `nova.toml` + `std/` (+ `compiler-codegen/nova_rt`
для рантайм-C и `libuv`) на native ext4 (`~/nova-work`), собранный
`nova`-бинарь — тоже на native fs; исходники Rust-крейтов можно оставить
на `/mnt/d` (там точечные reads, не полный обход). Побочная находка:
`du` через 9p **сильно завышает** размеры (репорт «282M ./std» на
`/mnt/d`, при этом честный `rsync` + `find -type f | wc -l` показал те
же 282 файла, но **3.8M** реальных — тот же файл-каунт, на два порядка
меньше «размера». Не доверять `du` через 9p-mount.

## Шаги (обновляется по ходу)

- [x] Инвентаризация WSL.
- [x] Создание worktree `p-linux-build` + `git submodule update --init` (libuv).
- [x] `cargo build --release` (compiler-codegen + nova-cli) под WSL —
      **только под `rustup 1.85.0`** (системный `1.93.1` — ICE, см. выше).
      `nova-cli`: `Finished release profile [optimized] target(s) in 6m 47s`,
      бинарь `~/nova-target-185/release/nova` (ELF64, 15MB) запускается,
      `nova --help`/`nova 0.1.0` работают.
- [x] Runtime C: `libuv.a built (36 files)` при первой сборке — Linux-ветка
      `build_libuv_lib` (whitelist unix .c-файлов, компиляция через `cc`,
      архивация через `ar`) отработала без правок. Boehm — системный
      `libgc-dev` (`gc.h` найден по `/usr/include/gc/gc.h`), никаких
      ошибок линковки — `detect_boehm()` Linux-ветка работает как есть.
- [x] `nova build` hello-world: **PASS.** `nova build ~/nova_smoke/hello.nv`
      (CWD = `~/nova-work`, native-fs копия `nova.toml`+`std`+`nova_rt`) →
      `built: /home/craft/nova-work/hello (12.09s)` (первая сборка, включая
      one-time сборку libuv.a). Запуск `./hello` → `hello from linux nova
      build`, exit 0.
- [x] `nova test` — std/src/checksums: **PASS: 3  FAIL: 0  SKIP: 3** (crc32_test,
      adler32_test, fnv_test — все зелёные; adler32/crc32/fnv source modules
      SKIP как ожидаемо — нет test-блоков/fn main).
- [ ] docs/linux-build.md.
- [x] Бонус: TSan смоук минимального M:N — см. отдельный раздел ниже.

## Бонус: TSan смоук (spawn+supervised, минимальный M:N)

Минимальный файл (`supervised { spawn {} ; spawn {} }` + println) — вручную
пересобран `clang -fsanitize=thread` (сгенерированный `.c` из `nova build
--keep-artifacts` + все core `nova_rt/*.c` + `libuv.a` + системный `-lgc
-lpthread`, без специальных Boehm build-флагов). **Компилируется и линкуется
БЕЗ ошибок** — рантайм-C уже TSan-совместим на уровне синтаксиса/ABI.

Запуск (`TSAN_OPTIONS=halt_on_error=0`): завершается корректно
(`mn_smoke done`, exit 0), но **~55s CPU** на два пустых spawn (огромный
оверхead — ожидаемо для TSan + conservative GC на первом прогоне; не hang,
процесс жив и мониторился через `/proc/<pid>/status` — Threads: 20→17,
%CPU росло). Никаких Boehm-специфичных суппрешенов/крашей НЕ потребовалось
(в отличие от Docker README Plan 40 находки про pthread-stress под
sanitizers — возможно, разница специфично для тяжёлых стресс-тестов, не
smoke). Это НЕ проверено на устойчивость под нагрузкой — только smoke.

**TSan нашёл 2 реальных data race в первом же прогоне (заметки для Плана 211):**

1. `nova_rt/fiber_arena.c:248` / `:255` (`_arena_install_sigsegv_handler`,
   вызывается из `nova_fiber_arena_init` ← `_worker_main`,
   `runtime.c:868`) — global `_sigsegv_installed` читается/пишется двумя
   worker-потоками (T16/T17) без синхронизации. Похоже на «install-once»
   check-then-set без atomic/mutex — вероятно безобидно (idempotent
   sigaction), но формальный race.
2. `nova_rt/runq.h:131` (`nova_runq_init`, вызывается из `_materialize_pool`,
   `runtime.c:1572`) vs `runq.h:273` (`nova_runq_grab` ← `nova_runq_steal`
   ← `_worker_main:951`) — atomic read гонится с init-write БЕЗ mutex M0
   (тот же M0 что защищает `_auto_arm_if_needed`/`nova_runtime_auto_arm`,
   т.е. init происходит под mutex, но `nova_runq_grab` читает поле уже ПОСЛЕ
   выхода из-под этого mutex в другом потоке — возможная visibility gap
   между `_materialize_pool` (main thread, под M0) и первым
   `nova_runq_steal` вызовом воркера). **Это ближе к сути Плана
   211/187** (wedge при высокой concurrency nested-supervised fan-out) —
   стоит проверить, не является ли отсутствие release/acquire барьера
   здесь причиной wedge на N=3+.

Полная TSan-транскрипция — в отчёте агента (не дублирую здесь полностью,
т.к. чекпоинт не должен раздуваться; главное — 2 конкретных находки выше).
