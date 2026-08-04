---
source_rev: 07df7d2c9
source_date: 2026-08-02
---

<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Сборка Nova на Linux (native / WSL2)

[English](linux-build.md) | **Русский**

Обновлено 2026-07-21. Проверено 2026-07-20 непосредственно на WSL2
Ubuntu 26.04 (ядро `6.6.87.2-microsoft-standard-WSL2`), вне Docker.
См. также [`docker/README.md`](../../docker/README.md) для более ранней
(2026-05-12) Docker-валидации (Plan 40) — этот документ дополняет её
рецептом для bare-metal/WSL и парой подводных камней, которые изоляция
Docker скрывает.

Закрывает `[M-nova-linux-build]` (см. историю
`docs/plans/backlog-followups.md` / `docs/dev/simplifications.md`).

## TL;DR

```sh
# 1. System packages (Debian/Ubuntu; see §Packages for other distros)
sudo apt install clang cmake make libgc-dev build-essential

# 2. Rust toolchain — use rustup, NOT your distro's rustc package (see
#    §Known issue: distro rustc ICE below).
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain 1.85.0 --profile minimal
source "$HOME/.cargo/env"

# 3. Clone + submodule
git clone https://github.com/nv-lang/nova.git && cd nova
git submodule update --init compiler-codegen/nova_rt/libuv

# 4. Build
cd nova-cli && cargo build --release && cd ..
# → nova-cli/target/release/nova

# 5. Smoke test
echo 'fn main() Io -> () => println("hello")' > /tmp/hello.nv
./nova-cli/target/release/nova build /tmp/hello.nv -o /tmp/hello && /tmp/hello

# 6. Test a module
./nova-cli/target/release/nova test std/src/checksums
```

Всё вышеперечисленное проверено end-to-end (чекпоинт удалён при закрытии
волны; см. git-историю для сырого лога сессии).

## Пакеты

| Назначение | Debian/Ubuntu | Fedora/RHEL | Arch |
|---|---|---|---|
| C-тулчейн | `clang build-essential` | `clang gcc` | `clang base-devel` |
| Boehm GC | `libgc-dev` | `gc-devel` | `gc` |
| (опционально, для `std/tls`) | `libmbedtls-dev` | — | — |
| cmake/make | `cmake make` | `cmake make` | `cmake make` |

`libuv` **не** является системной зависимостью пакета — он вендорится как
git-подмодуль (`compiler-codegen/nova_rt/libuv`) и собирается из исходников
при первом использовании (см. §libuv ниже). `pkg-config` не требуется —
ничего в сборке его не использует.

`ar` (binutils) требуется для шага архивации libuv; он уже поставляется
с `build-essential` / `base-devel`.

Собственный Rust-код Nova не имеет безусловной Windows-only зависимости —
в `compiler-codegen/Cargo.toml` и `nova-cli/Cargo.toml` нет крейтов
`[target.'cfg(windows)']`. `compiler-codegen/src/test_runner.rs` уже имеет
зрелые ветви `#[cfg(target_os = "linux")]` для детекции тулчейна, детекции
Boehm (`detect_boehm`) и сборки libuv (`detect_or_build_libuv` /
`build_libuv_lib`) — это было реализовано в Plan 22/27/40/44.1 и один раз уже
провалидировано через Docker (2026-05-12). Работа этого документа — проверить,
что это всё ещё держится на реальном (не контейнерном) Linux-хосте,
и зафиксировать, что изменилось с тех пор.

## Известная проблема: дистрибутивный `rustc` может ICE-ить на `compiler-codegen`

На Ubuntu 26.04 предустановленный `rustc`/`cargo` (`1.93.1 (01f6ddf75
2026-02-11), собран из source-тарболла`) **падает с внутренней ошибкой
компилятора** при компиляции `compiler-codegen/src/codegen/emit_c.rs`:

```
thread 'rustc' panicked at .../library/alloc/src/vec/mod.rs:2796:36:
slice index starts at 52 but ends at 51
error: the compiler unexpectedly panicked. this is a bug.
query stack during panic:
#0 [check_liveness] checking liveness of variables in
   `codegen::emit_c::<impl at src/codegen/emit_c.rs:2026:1: 2026:14>::receiver_c_type`
```

Воспроизведено дважды, байт-в-байт — не флейк. Это баг апстримного rustc
в запросе `check_liveness` NLL/MIR-borrowck, спровоцированный
размером/сложностью `emit_c.rs`, а не баг Nova — GitHub CI
(`.github/workflows/nova-test-regression.yml`, `runs-on: ubuntu-latest`,
без явного шага тулчейна) **не** натыкается на это, потому что образ раннера
GH-hosted поставляет другую сборку rustc.

**Workaround (проверено):** установи тулчейн через `rustup` вместо того,
чтобы полагаться на дистрибутивный пакет — `rustup` устанавливается в `$HOME`,
**`sudo` не нужен** и сосуществует с системным `rustc` (rustup предупредит
о предустановленном; это безвредно, просто не ставь `~/.cargo/bin` впереди
`/usr/bin` в `PATH`, если хочешь продолжать использовать дистрибутивный для
чего-то ещё):

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain 1.85.0 --profile minimal
~/.cargo/bin/cargo build --release --manifest-path nova-cli/Cargo.toml
```

`1.85.0` был выбран потому, что это объявленный `rust-version` (MSRV)
`compiler-codegen` — собрался чисто (`Finished release profile [optimized]
target(s) in 6m 47s` для `nova-cli`, `3m 10s` для `compiler-codegen` отдельно).
Любой стабильный релиз из rustup, близкий к этому, должен работать; ломается
конкретно *дистрибутивно-запатченная* сборка, а не язык/edition. Этот
репозиторий **не** поставляет пин `rust-toolchain.toml` (повлиял бы и на
Windows-воркфлоу) — если это укусит CI или другого контрибьютора, пересмотреть
вопрос о его добавлении.

## Boehm GC

`detect_boehm()` (Linux-ветвь) ищет `gc.h` по адресам `/usr/include/gc.h`,
`/usr/include/gc/gc.h`, `/usr/local/include/gc.h`, в таком порядке, иначе
падает с подсказкой `sudo apt install libgc-dev`. Подтверждено: пакет
`libgc-dev` Ubuntu 26.04 (`1:8.2.12-1`) кладёт заголовок в
`/usr/include/gc/gc.h`, а shared-библиотеку в стандартный multiarch-путь —
переопределения `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` для стокового
apt-инсталла не нужны. Линковка — обычный `-lgc` (+ `-lpthread` на Linux) —
статическая/вендоренная сборка Boehm на Linux не нужна (в отличие от Windows,
использующей триплет `x64-windows-static` из vcpkg).

## libuv

`nova build`/`nova test` собирают libuv **из исходников** при первом
использовании (в этой кодовой базе нет системно-пакетного пути для libuv
на Linux — Plan 22 решил вендорить его единообразно на всех платформах,
а не мешать `libuv1-dev` на Linux со сборкой-из-исходников на Windows).
Linux-ветвь `build_libuv_lib` (`compiler-codegen/src/test_runner.rs`)
компилирует фиксированный whitelist из `src/*.c` + `src/unix/{async,core,dl,fs,...
,linux,procfs-exepath,proctitle,random-getrandom,random-sysctl-linux,
no-fsevents}.c` через `cc` (уважает `$CC`) и архивирует их `ar` в
`<repo>/target/libuv-cache/libuv.a`. Подтверждено, работает как есть:
`nova: libuv.a built (36 files)`, затем кэшируется (~мгновенно на
последующих сборках).

## Арена файберов — POSIX-реализация уже существует

`compiler-codegen/nova_rt/fiber_arena.c` (POSIX: `mman.h`, `pthread.h`,
`ucontext.h`, `signal.h`, обработчик `SIGSEGV` для диагностики переполнения
стека) — полная, отдельная реализация от `fiber_arena_win.c` (охраняется
`#if defined(_WIN32) && NOVA_FIBER_ARENA_ENABLED`, компилируется практически
в ничто на Linux). **Работ по портированию не потребовалось** — это была
единственная часть задачи, которая стала бы жёсткой остановкой, если бы
отсутствовала; её нет.

## Gotcha: WSL2 `/mnt/<drive>` (9p) годится для точечных чтений, плох для обходов
## каталогов — скопируй Nova *workspace* на нативную ФС перед `nova build`/`test`

Если твой Nova-чекаут лежит на Windows-диске, смонтированном в WSL2 через
`/mnt/c`/`/mnt/d` (протокол 9p), проявляются два очень разных перф-профиля:

- **`cargo build`** (компиляция Rust-крейтов) читает умеренное число отдельных
  `.rs`-файлов — нормально прямо на `/mnt/...` (release-сборка
  `compiler-codegen` заняла ~3 мин в любом случае). Всё равно указывай
  `CARGO_TARGET_DIR` на нативную ext4 (например `$HOME/...`) —
  инкрементальная сборка cargo пишет тысячи мелких объектных/метаданных
  файлов, а это *медленно* через 9p.
- **`nova build`/`nova test`** резолвят Nova-workspace (`nova.toml` + `std/`)
  рекурсивным обходом каталогов. На `/mnt/...` эта цепочка вызовов видимо
  блокируется в канале ожидания ядра `p9_client_rpc` (проверялось через
  `/proc/<pid>/task/*/wchan`) на **минуты** на тривиальном hello-world,
  потому что каждый листинг каталога — это 9p-круговая поездка.
  **Фикс:** скопируй `nova.toml` + `std/` (+ `compiler-codegen/nova_rt/`,
  для рантайм-C + урезанного libuv `src`+`include`, пропустив `libuv/test`+
  `libuv/docs` — ~300 MB, которые тебе не нужны) на нативный путь и запускай
  `nova build`/`nova test` с ним как `$CWD`. Пересборки после этого:
  однозначные секунды.

Отступление: `du` **через 9p-mount дико завышает размер** — показал `282M`
для `std/`, в то время как `rsync`-копия тех же самых 282 файлов (счётчик
файлов совпал точно) легла в `3.8M` на нативной ext4. Не доверяй числам `du`,
собранным через `/mnt/...`; вместо этого доверяй `find -type f | wc -l`
(счётчику файлов), если нужно прозондировать копию.

Это артефакт WSL2/9p, а не баг Nova — нативный Linux-бокс (без
Windows-диск-маунта в цепочке) вообще не должен это видеть, и GitHub CI тоже
не видит.

## Проверено (базлайн 2026-07-16, критичные проблемы решены 2026-07-20)

Первичная Linux-валидация сборки (2026-07-16) выявила три детерминированных
платформенно-специфичных проблемы, все из которых с тех пор исправлены
(2026-07-20, волна followup Plan 208–220). Conformance-gate (`nova-gate.yml`)
теперь проходит чисто на Linux:

- **link-order регрессия** (порядок архивирования Unix-линкера): исправлено
  в `test_runner.rs` — `libuv.a` теперь ставится после `.o`-файлов
  и рантайм-архива в команде линковки.
- **gc-sections dead-code проблема** (`nova_bench_*` символы): исправлено через
  `-ffunction-sections`/`-fdata-sections` на Unix в `build_rt_archive_lib`.
- **cbrt ULP непереносимость**: исправлено в тесте
  `d109_primitive_methods_f64_f32_math.nv` — точное равенство заменено
  на epsilon-сравнение.

| Шаг | Результат |
|---|---|
| `cargo build --release` (compiler-codegen) | PASS (rustup 1.85.0), 3m10s |
| `cargo build --release` (nova-cli) | PASS (rustup 1.85.0), 6m47s, бинарь запускается |
| libuv build-from-source | PASS, `libuv.a built (36 files)` |
| Boehm GC детекция/линковка | PASS, системный `libgc-dev`, без overrides |
| `nova build` hello-world | PASS, `built: .../hello (12.09s)`, запустился, корректный stdout |
| `nova test std/src/checksums` | PASS: 3 FAIL: 0 SKIP: 3 |
| Conformance gate (`spec_tests/conformance`) | PASS (после фиксов 2026-07-20) |
| TSan smoke (spawn+supervised, ручной `clang -fsanitize=thread`) | Компилируется+линкуется чисто, доходит до конца, **найдено 2 реальных гонки данных** — чекпоинт удалён при закрытии волны, см. git-историю и закрывающий отчёт задачи для Plan 211 |

## Известные пробелы (вне рамок здесь, найдены через существующий CI)

`.github/workflows/nova-test-regression.yml` документирует предсуществующие
падения `nova test std` на Linux (по состоянию на 2026-07-16), отличные от
conformance-gate и вне рамок этого документа:
`std/src/concurrency/retry_test` (ошибка компиляции C — несоответствие типа
struct-return, похоже на баг mono/codegen, не очевидно Linux-специфично), два
RUN-FAIL краша переполнения стека файбера (`std/src/fs/concurrent_stat_test`,
`std/src/net/addr`), integer-overflow RUN-FAIL в `std/src/identifiers/ulid_test`
и обычная ошибка компиляции `.nv`-исходника в `std/src/time/civil/civil_arith_test`
(выведенный из эксплуатации API `str.len()`, D249 — похоже на предсуществующий
баг исходника, не связан с платформой). Кто бы ни взял полный `nova test std`
на Linux, должен отслеживать их отдельно.

## Сборка nova-lsp

Языковой сервер Nova доступен в `nova-lsp/` внутри репозитория:

```sh
cd nova-lsp
cargo build --release
# → target/release/nova-lsp (executable available in nova-lsp/target/release)
```

Не требуется дополнительных системных зависимостей сверх стандартного сетапа
сборки Nova (Rust-тулчейн, C-компилятор, Boehm GC). LSP-бинарь можно
использовать как бэкенд языкового сервера для совместимых редакторов (VSCode,
Neovim и т.д.).

## TSan / sanitizer-сборки

Не часть стандартной сборки (в `test_runner.rs` нет флага `--sanitizer`);
`docker/Dockerfile` из Plan 40 гоняет sanitizer-сборки, используя `clang`
напрямую с sanitizer-флагами вне обычного пути компиляции `nova`-CLI —
TSan-smoke этого документа делал то же самое (вручную перекомпилировал
сгенерированный CLI `.c` + `nova_rt/*.c` + `libuv.a` с `-fsanitize=thread`,
чекпоинт удалён при закрытии волны, см. git-историю). Для минимального
smoke-теста с 2 spawn и стоковым системным `libgc` (без специальных
Boehm-флагов сборки) файл подавлений не понадобился — более тяжёлые
стресс-тесты всё ещё могут наткнуться на взаимодействие Boehm/TSan,
документированное в `docker/README.md` (митигация `THREAD_LOCAL_ALLOC=0
PARALLEL_MARK=0`, нужна для Boehm-сборок `--enable-threads=posix`
под sanitizer-ами). `[M-tsan-race-detector]` и `[M-83.11-f2-arm-tsan]`
(оба гейтились на закрытие этого документа) теперь могут продолжаться.
