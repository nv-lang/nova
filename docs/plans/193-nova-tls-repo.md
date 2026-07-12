<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 193 — nova-tls: вынос TLS в отдельную репу + examples + доки

**Статус:** 🚧 Ф.1 ✅ ЗАКРЫТА, Ф.2 BLOCKED 2026-07-12 — файловый вынос
+ dependency-wiring сделаны (nova-abi `4d8abf0b0`, nova-tls `488a16d`),
duplicate-symbol УШЁЛ (эмпирически подтверждено), conformance δ0 (95/0),
blast-radius чист (только std/http). Ф.2-гейт НЕ закрыт — STOP на 2
реальных gap'ах compiler-codegen (не мандат этого захода): (1) генерик
`[ffi] libs` не умеет lib-dir propagation/detect-and-degrade → hard
CC-FAIL «could not open mbedtls.lib» (либ нигде не установлена на машине);
(2) `embed_resolve.rs`'s `project_root` не per-package →
`E_EMBED_OUTSIDE_PROJECT` на легитимных `embed(...)` в nova-tls's
`*_test.nv` при потреблении ЛЮБЫМ внешним пакетом. См. «Ф.2» ниже —
детальный gap-отчёт, оба блокируют `std/http`, Ф.3 транзитивно тоже.
Ф.1-история (закрыта): раскладка `nova-tls` закоммичена; rt_dir/cg_include
resolver-пробел закрыт env-override'ом (`NOVA_CG_INCLUDE`/`NOVA_RT_DIR`,
см. «Ф.1 продолжение» ниже) — это НЕ было работой ext-dep-resolver-03-1
(проверено, другой домен). Ложный `D133-not-consumed` на consume-param
transfer в отдельно-пакетном контексте (см. «Ф.1 продолжение #2» ниже) —
починен (`find_repo_root_from` + `ResolvedFfiConfig::from_manifest`,
module-loader/manifest-resolver, НЕ consume-checker).
**Приоритет:** P2 (packaging, не функционал). **Консолидирует:** бывш. 116 #5 (вынос
nova-tls) + 116 #6 (examples/доки) + 195 Ф.3 (та же экстракция) — сведено в ОДИН план,
чтобы TLS-домен не был размазан по 116/195. Номер 193 заполняет дырку от консолидации
(бывш. 193 bcrypt-B → 191 Ф.B).

## Контекст
TLS-**ядро** закрыто в [Plan 116](116-std-tls-effect.md) (mbedTLS-бэкенд, полная
TLS-поверхность, https-prod-ready подтверждён — `real.nv` e2e PASS). Остался
**packaging-хвост**: вынести реальный TLS-модуль (уже на C, без Rust) в отдельную репу
`nova-tls` как «настоящий рабочий native-модуль» (эталон паттерна [Plan 195](195-native-modules-c-not-rust.md)),
монорепо тянет как внешнюю зависимость (механизм [03.1](03.1-path-git-dependencies.md)✅).

## Фазы
- **Ф.1 — standalone nova-tls репа:** новая репа `nova-tls` (сиблинг монорепы):
  `src/` (весь `.nv`-фасад std/tls — TlsStream/cert-modes/mTLS/Pinned) + `native/`
  (`tls_c_shim.c` + `tls_mozilla_roots.h` + `tls_shim.h`; mbedTLS — исходники или
  prebuilt .lib) + `nova.toml` (`[package] name = "tls"`, `[ffi]` c_shims/include_dirs/libs)
  + README + LICENSE. Module-path относительно `src/` (D78, не включает `src`). **Гейт:**
  `nova-tls` собирается+тестируется STANDALONE (clang + mbedTLS, **ноль Rust/cargo**).
- **Ф.2 — монорепо на внешний dep:** убрать `std/tls` + `nova_rt/tls_c_shim.c` из
  монорепы; `std/nova.toml` → `[dependencies] { git = "…/nova-tls", tag = "…" }` (или
  `path` для локальной разработки). std/http `real.nv` тянет TLS через dep. **Гейт:**
  монорепо собирается, std/tls-тесты зелёные ЧЕРЕЗ dep (не in-tree), conformance δ0.
- **Ф.3 — examples + доки:** `examples/tls/echo` (TLS echo client/server), гайд
  (authoring-a-module — nova-tls как эталон), D-блоки (если нужны). Закрывает 116 #6.

## Гейты (финал)
`nova-tls` собирается standalone БЕЗ Rust (только clang + .lib); монорепо тянет его
внешним dep и std/tls-тесты (29/29) зелёные через зависимость; conformance δ0;
grep-инвариант «нет Rust-crate в TLS-пути»; examples/tls/echo PASS.

## Границы
Не меняет публичный API TLS (backend уже свопнут на mbedTLS в 116/T40). Раскладка `src/`
и общий native-модуль-паттерн — в [Plan 195](195-native-modules-c-not-rust.md); этот план =
конкретная TLS-инстанция того паттерна.

## Ф.1 блокер (2026-07-11, перезапуск после системного сбоя)

Раскладка `nova-tls` (сиблинг-репа, git корень отдельный) закоммичена
(`chore: initial nova-tls layout`, hash `7233a4d`): `src/tls/*` (13 `.nv` + `testdata/`),
`native/{tls_c_shim.c, tls_shim.h, tls_mozilla_roots.h}`, `nova.toml`, `README.md`,
`LICENSE-{MIT,APACHE}`. Сверено 1:1 против `std/tls/`: `.nv`-файлы отличаются ТОЛЬКО
module-path (`std.tls` → `tls.tls`) и обновлёнными комментариями под mbedTLS-бэкенд
(не стейл rustls-копия); `native/*` — байт-в-байт идентичны `compiler-codegen/nova_rt/`.
`std/tls/{cert_modes_test.c, handshake_test.c}` — НЕ исходники (gitignored codegen-артефакты,
`std/**/*.c` в `.gitignore` монорепы) — доизвлекать нечего, раскладка ПОЛНАЯ.

Standalone-сборка НЕ работает — упирается в архитектурный пробел компилятора
(класс **(a)**, не раскладка nova-tls):

1. `nova build src/tls/ffi.nv` без `NOVA_STD_PATH` → 7× "cannot find module 'std.net'/'std.io'/…".
   `resolve_std_path` (`compiler-codegen/src/manifest.rs:898`) резолвит `std/`
   относительно репо пакета: `(1) NOVA_STD_PATH env → (2) [workspace]/[package].std в
   nova.toml → (3) repo.join("std")`. Для standalone-пакета без `std/` рядом нужен
   `NOVA_STD_PATH=<monorepo>/std` — это ОЖИДАЕМЫЙ параметр окружения, не баг
   (README «Building standalone» дополнить упоминанием).
2. С `NOVA_STD_PATH` импорты резолвятся, но `nova build src/tls/ffi.nv` падает:
   `codegen error: cannot infer method-level type argument `U` for `Result.map` —
   appears in param `map_fn` (#0)` (без file:line — чисто codegen-уровня ошибка,
   `compiler-codegen/src/codegen/emit_c.rs:19578`). **Верифицировано НЕ дефектом
   экстракции**: идентичная ошибка воспроизводится на ОРИГИНАЛЕ
   `nova build std/tls/ffi.nv` в монорепо (тот же `nova.exe`, тот же `std/`,
   ничего не менялось). Причина: `ffi.nv`/`tls`-модуль — библиотека без `main`;
   изолированная сборка одного файла не даёт конкретной call-site, фиксирующей
   generic `U` где-то в std-коде — в полном `nova test std/tls` CU эту роль играют
   тестовые файлы. **Вывод: `nova build <file>.nv` — не тот инструмент для
   library-CU; правильный — `nova test`** (README это уже прописывает верно).
3. `nova test src/tls` (с `NOVA_STD_PATH`) →
   `nova: FATAL libuv submodule not initialized at .../nova-tls/compiler-codegen/
   nova_rt/libuv. Plan 22 F2: libuv is mandatory.` — **это и есть блокер.**
   `detect_or_build_libuv` (`compiler-codegen/src/test_runner.rs:3444`) и
   `RepoPaths::rt_dir`/`cg_include` (`nova-cli/src/main.rs:1142-1148`:
   `rt_dir: repo.join("compiler-codegen").join("nova_rt")`) резолвятся ЖЁСТКО
   относительно корня ТЕКУЩЕГО пакета (`repo` = сам `nova-tls`), **без env-override**
   — в отличие от `NOVA_STD_PATH`/`NOVA_GC_LIB_DIR`, у которых override есть.
   Следствие: ЛЮБОЙ standalone-пакет (не только nova-tls) сейчас ОБЯЗАН иметь
   у себя копию ПОЛНОГО C-рантайма компилятора — `compiler-codegen/nova_rt/`
   (64 файла: fibers/gc/effects/alloc/net/fs/sync/…) + git submodule `libuv`
   (~468 МБ исходников, собирается on-demand через `build_libuv_lib`) — просто
   чтобы слинковать тестовый бинарь. Это дублирование внутренностей тулчейна
   внутрь leaf-пакета — архитектурно неверно для роли nova-tls как «эталона
   standalone native-модуля» и относится к недоделанной части
   ext-dep/toolchain-runtime-resolver линии (ветка nova-abi ext-dep-resolver,
   упомянутая владельцем). **Не чинил** (вне мандата разведки этого захода):
   резолвер `rt_dir`/`cg_include` должен научиться (а) читать env-override
   (симметрично `NOVA_STD_PATH`) и/или (б) находить bundled runtime рядом с
   самим `nova.exe` (toolchain-relative), прежде чем `nova-tls` сможет реально
   пройти `nova test` standalone.

**Проверено — НЕ блокер:** mbedTLS-библиотек на этой машине тоже нигде нет (ни в
`compiler-codegen/vcpkg_installed/`, ни системно — только port-описание без сборки
в `C:/vcpkg/ports/mbedtls`, install не запускался). Но это не регрессия nova-tls:
даже `nova test std/tls` В МОНОРЕПО на этой же машине красный НЕ на линковке, а на
рантайме — `RUN-FAIL … tls internal error: tls shim not built for this host`
(4 FAIL: Pinned×2, SystemRoots-NEG). Паритет с монорепо подтверждён.

**Дальше (после разблокировки rt_dir-резолвера, вне этого захода):** повторить
`nova test src/tls` с реальным mbedTLS (`vcpkg install mbedtls:x64-windows-static`
— отдельная сетевая операция, не запускалась в этой разведке).

## Ф.1 продолжение (2026-07-12): rt_dir/cg_include env-override — СДЕЛАНО, вскрыт новый блокер

**Ветка `ext-dep-resolver-03-1` (nova-abi) проверена — НЕ решает этот пробел.**
Её коммиты (`a10b2d5e9`, `9cc5472c1`, `c95cf673f`) — `ResolvedFfiConfig::merge()`
+ `resolved_dependency_roots()` в `imports.rs`/`test_runner.rs`: агрегация
`[ffi]`-секций (c_shims/include_dirs/libs) объявленных path/git-зависимостей
в бинарь ИМПОРТЁРА (§3.2 explicit dependency graph). Другой домен: там речь о
том, что native-артефакты ЗАВИСИМОСТИ линкуются в потребителя; здесь —
о том, где резолвер тулчейна ищет СОБСТВЕННЫЙ C-рантайм компилятора
(`nova_rt/` + libuv). Пересечений нет, ждать её было не нужно.

**Правка сделана** (`nova-cli/src/main.rs`, `resolve_paths()`): добавлен
`env_path_override()` + чтение `NOVA_CG_INCLUDE`/`NOVA_RT_DIR`, precedence
env → `repo`-relative default (byte-идентично прежнему хардкоду при пустом
env — 0 регрессий), симметрично `NOVA_STD_PATH`/`NOVA_GC_LIB_DIR`. Компактно
(~15 строк), собрано (`cargo build --release -p nova-cli`), verified:
- Дефолтный запуск (без env) `nova test std/tls` в монорепо — байт-идентичен
  прежнему поведению (RUN-FAIL на 4 теста, tls shim not built — паритет).
- Standalone `nova test src/tls` в `nova-tls` с `NOVA_STD_PATH`+
  `NOVA_CG_INCLUDE`+`NOVA_RT_DIR` → монорепо: FATAL libuv **ушёл** (libuv
  собрался one-time из монорепного submodule), Boehm GC **нашёлся** (через
  `cg_include`→`vcpkg_installed`), mbedTLS detection тоже прошёл без жалоб —
  дошло до РЕАЛЬНОЙ компиляции/тайпчека. Пункт 3 блокера (rt_dir/cg_include
  жёсткий resolver) — ЗАКРЫТ.

**Новый блокер (вскрылся ТОЛЬКО теперь, старый его маскировал):**
`nova test src/tls` → `CODEGEN-FAIL`: ложный `[D133-not-consumed]` на `conn`
(`TcpStream`) в `cert_modes_test.nv:26/36` — при том что `conn` легитимно
консьюмится передачей в consume-param: `TlsStream.accept(consume stream
TcpStream, …)` / `.connect(consume stream TcpStream, …)` (`server.nv:53`,
`client.nv:75`). Тип у `conn` в диагностике — буквально пустая строка
(`тип \`\``), т.е. type-inference не резолвит возврат `must_tcp(...)`
(конкретный non-generic `TcpStream`) в этом контексте — при том что
СОСЕДНИЙ `must_listener(...)`→`TcpListener` и `must_tls(...)`→`TlsStream`
(та же файла-CU, тот же паттерн) резолвятся нормально. Два контроля
исключили ложный след:
1. Self-pointing override (те же три env-переменные, но указывающие НАЗАД
   на монорепный `std/tls`, тот же `nova.exe`) — проходит чисто (RUN-FAIL,
   не CODEGEN-FAIL) → это не побочный эффект самого механизма override.
2. Удаление закэшированных `.c`-артефактов (`cert_modes_test.c`,
   `handshake_test.c`, gitignored) из монорепного `std/tls` и чистая
   пересборка — тоже проходит чисто → это не stale-cache артефакт.

Т.е. дефект специфичен именно separate-package идентичности `nova-tls`
(`[package] name = "tls"`, `module tls.tls`, `src/`-раскладка) при БАЙТ-
ИДЕНТИЧНОМ `.nv`-содержимом (не считая комментариев/module-path/`#stable`
версии) — похоже на пробел в consume-checker'е при резолве
consume-param'ов через `Type.method(consume x, …)`-синтаксис (не generic:
`must_tcp` не generic-функция) на границе leaf-пакет↔std-как-внешняя-
зависимость. НЕ пересекается с `ext-dep-resolver-03-1` (та ветка про
FFI/native-lib propagation, не про consume-checker/type-identity).
**Не чинил** — вне мандата этого захода (глубокий compiler-checker гэп,
не компактная env-правка); нужна отдельная разведка/подпункт.

## Ф.1 продолжение #2 (2026-07-12): ложный D133 — ПОЧИНЕН (module-loader, не checker)

Разведка следующего захода подтвердила гипотезу «leaf-пакет не сворачивает
sibling-файлы папки в один модуль»: instrumented-debug на `codegen_to_c`
(`test_runner.rs:2910`) показал для standalone `nova-tls`
`find_repo_root_from(.../src/tls/cert_modes_test.nv) = None`, а для
монорепного `std/tls` — `Some(<repo>)`, `module.items=923 peer_files=83`.
Когда `find_repo_root_from` возвращает `None`, весь блок
`resolve_imports_inline_ex` + `collect_all_signatures` (`test_runner.rs`
~2956-2964) **пропускается целиком** — модуль остаётся ровно как распарсен
из ОДНОГО entry-файла (sibling-файлы той же папки НЕ сворачиваются, cross-
module сигнатуры не собираются), из-за чего вызовы вроде `must_tcp()`
(определён в sibling'е `handshake_test.nv`) резолвятся в error/empty-тип, а
не в отсутствующую функцию — отсюда специфика диагноза («тип `conn`
буквально пустая строка», а не «unknown function»). Корень — **НЕ**
consume-checker (`types/mod.rs`), а `find_repo_root_from`
(`compiler-codegen/src/test_runner.rs:2870`), общий helper module-loader'а.

**Баг 1 (корневой): `find_repo_root_from` `?`-пропагация discard'ит
`last_toml_dir` на границе файловой системы.**
```rust
let parent = dir.parent()?.to_path_buf();   // BUG: `?` возвращает None из
if parent == dir {                          //      ФУНКЦИИ, как только
    return last_toml_dir;                   //      dir.parent() == None —
}                                            //      строки ниже НИКОГДА
dir = parent;                                //      не выполняются (dead
                                              //      code: parent() никогда
                                              //      не возвращает Some(dir)).
```
`Path::parent()` возвращает `None` НА корне ФС (`D:\`, `/`), а не
`Some(dir)` — поэтому проверка `parent == dir` недостижима, и `?` тихо
пропагирует `None` из **всей функции**, отбрасывая уже найденный
`last_toml_dir` (свой, БЕЗ `[workspace]`-маркера, `nova.toml` — ровно
случай `nova-tls`, у которого нет `[workspace]` предка, т.к. это sibling-
репа, не nested в монорепу). Для `std/tls` баг НЕ проявляется: walk-up от
`std/tls/*.nv` находит `nova-199/nova.toml` с `[workspace]`-маркером и
возвращает `Some` РАНЬШЕ, чем доходит до сломанного fs-root хвоста —
поэтому монорепо всегда было «случайно защищено» этим же самым багом.
Идентичный (уже ПРАВИЛЬНЫЙ) паттерн уже существовал рядом — fallback-ветка
`find_repo_root()` в `nova-cli/src/main.rs:1106-1125` использует `match
dir.parent() { Some(p) if p != dir => …, _ => return last_toml_dir }` —
собственно эталон, по которому и сделан фикс.
**Фикс:** `compiler-codegen/src/test_runner.rs::find_repo_root_from` —
заменить `?`-пропагацию на `match dir.parent() { Some(p) => dir = p, None
=> return last_toml_dir }` (зеркально `find_repo_root()`).

**Баг 2 (сопутствующий, вскрылся ПОСЛЕ фикса бага 1): `[ffi]` пути
резолвились от `source_root`, а не от директории `nova.toml`.** После
фикса бага 1 `nova-tls` продвинулась дальше D133 до `CC-FAIL: no such file
… src\native\tls_c_shim.c` — `ResolvedFfiConfig::from_manifest`
(`test_runner.rs:726`) джойнил `c_shims`/`include_dirs` от
`Manifest::source_root`, а не от директории `nova.toml`, вопреки
документированному контракту `[ffi]`-секции («Все paths относительные к
директории nova.toml», `manifest.rs:108/149`). Для пакетов с
`[lib] src = "."` (default, ВСЕ существующие manifest'ы в монорепо —
`nova_tests/plan03_1/*`, `plan115`, etc.) `source_root == nova.toml`-
директория, поэтому баг был невидим; `nova-tls` (`[lib] src = "src"`,
legacy-но-honored D78 back-compat) — первый реальный пакет, различающий
их. **Фикс:** добавлено поле `Manifest::manifest_dir` (директория
`nova.toml`, отдельно от `source_root`); `ResolvedFfiConfig::from_manifest`
джойнит от него.

**Гейты (оба фикса):**
- Standalone `nova-tls test src/tls --filter cert_modes_test` (env-override
  `NOVA_STD_PATH`/`NOVA_CG_INCLUDE`/`NOVA_RT_DIR`/`NOVA_GC_LIB_DIR` →
  монорепо) — `D133-not-consumed` **ушёл**; `CC-FAIL: no such file` (баг 2)
  тоже ушёл после фикса; дошло до `CC-FAIL: duplicate symbol` (см. ниже,
  Ф.2-территория, НЕ regression от этих фиксов).
- `spec_tests/conformance --full` (single CU, включая
  `d133_not_consumed_neg`/`d133_branch_maybe_consumed_neg`/
  `d133_field_marker_missing_neg`) — **95 PASS / 0 FAIL** — D133-checker
  сам не ослаблен, реальные not-consumed продолжают ловиться.
- `nova test std` (default sample) — см. результат прогона в коммите/CI.

**Третий блокер (НЕ чинил, вне мандата — ожидаемо разрешится Ф.2):** после
обоих фиксов линковка standalone `nova-tls` падает на `lld-link: duplicate
symbol: tls_client_cfg_new / tls_server_cfg_new / tls_cfg_verify_system` —
`tls_c_shim.c` линкуется ДВАЖДЫ: один раз через `nova-tls`'s собственный
`[ffi] c_shims = ["native/tls_c_shim.c"]`, второй раз через toolchain'ный
auto-link монорепного `nova_rt/tls_c_shim.c` (детектится по вызову
`tls_*`-символов в сгенерированном C, `test_runner::c_file_uses_tls`,
Plan D337) — т.к. `NOVA_RT_DIR` в этом заходе указывает НАЗАД на монорепу
(Ф.2 «монорепо → external dep на nova-tls» ещё не сделана, поэтому в
монорепе всё ещё живёт СВОЙ `std/tls`+`nova_rt/tls_c_shim.c` параллельно с
`nova-tls`'s копией). Не архитектурный баг — артефакт временного
env-override-моста поверх ещё-не-мигрированного монорепного `std/tls`;
разрешится сам после Ф.2 (монорепо перестанет иметь собственную копию
шима). Реальный mbedTLS-линк (библиотек на машине нет) — отдельно
непроверяем, как и раньше.

## Ф.2 (2026-07-12): монорепо → external dep — механика СДЕЛАНА,
## duplicate-symbol УШЁЛ, но GATE НЕ закрыт — STOP на 2 реальных gap'ах

**Сделано** (nova-abi commit `4d8abf0b0`, nova-tls commit `488a16d`):
- `std/tls/` (13 `.nv` + `testdata/`) и `compiler-codegen/nova_rt/{tls_c_shim.c,
  tls_shim.h, tls_mozilla_roots.h}` удалены из монорепы.
- Built-in D337-style TLS auto-link спецкейс (`MbedtlsConfig`,
  `detect_mbedtls`, `c_file_uses_tls`, `uses_tls`-ветки во всех 3
  toolchain'ах Clang/Msvc/Gcc) убран из `test_runner.rs` — эта машинерия
  целиком заменяется nova-tls's собственным `[ffi]` (сделано в Ф.1).
  `nova_rt.h` больше не `#include "tls_shim.h"` (файла нет).
- `std/nova.toml`: `[dependencies] tls = { path = "../../nova-tls" }`
  (path — **два** `..`, т.к. relative к директории `std/nova.toml`, НЕ к
  workspace-корню: `std/` → `nova-abi/` → `nv-lang/` → `nova-tls/`; сначала
  ошибся с одним `..`, поймал через `nova check` — «expected:
  …\std\..\nova-tls»).
- `std/http/error.nv` + `std/http/transport/real.nv`:
  `import std.tls.{...}` → `import tls.tls.{...}`.
- nova-tls's `nova.toml`: `[ffi] c_shims` дополнен `native/tls_shim.h`
  (force-include прототипов во ВСЕ TU — иначе caller'ские CU видят
  `tls_client_cfg_new`/`tls_server_cfg_new` только как implicit-int decl →
  усечение 64-битного handle; раньше это гарантировал монорепный
  `nova_rt.h`'s безусловный `#include`, теперь эта роль — на nova-tls
  самой). `.gitignore` (codegen-артефакты + `target/`).

**Эмпирически подтверждено — duplicate-symbol ушёл:** standalone
`nova-tls test src/tls --filter cert_modes_test` (тот же env-override
мост на монорепный toolchain: `NOVA_STD_PATH`/`NOVA_CG_INCLUDE`/
`NOVA_RT_DIR`/`NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR`) — было `lld-link:
duplicate symbol`, стало `lld-link: error: could not open 'mbedtls.lib':
no such file or directory` (× mbedtls/mbedx509/mbedcrypto). Класс ошибки
СМЕНИЛСЯ — двойная линковка шима исчезла, ровно то, что обещала Ф.2.
Toolchain на этой машине — **Clang** (не MSVC; `cl.exe` не на `PATH`,
`C:\Program Files\LLVM\bin\clang.exe` найден).

**Regression-check (Rust-уровня правки test_runner.rs/nova_rt.h не задели
остальной язык):**
- `spec_tests/conformance --full --timeout 300 --jobs 4` (env
  `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` → монорепо `nova`) —
  **95 PASS / 0 FAIL, δ0**. Conformance не трогает `std.tls` вовсе
  (`m178_variant_ctor_crosssum_option_collision.nv` явно «reproduces the
  bug WITHOUT std.tls»), поэтому это чистая проверка «ничего не
  сломалось в остальном компиляторе».
- `nova check std` (весь пакет) — 117 PASS / 29 FAIL. Blast-radius
  подтверждён ТОЧНО std/http-доменом: 22 из 29 FAIL — заведомо-красные
  `*_neg.nv` фикстуры (`std/encoding/serde_neg`, `std/fs/neg`,
  `std/io/neg`, `std/net/neg`, `std/time/civil/neg`, `std/http/neg/*`) —
  это НЕГАТИВНЫЕ фикстуры, `nova check` (в отличие от `nova test`) не
  знает про `EXPECT_FAIL`-конвенцию и репортит их «ошибку типизации» как
  raw FAIL — так было ДО этого захода тоже, не регрессия. Оставшиеся 7 —
  реальные новые FAIL, все внутри `std/http` (`body.nv`, `client/
  client.nv`, `serdejson/serdejson.nv`, `server/mux_test.nv`,
  `servernet/{servernet.nv,rt/handle_connection_smoke.nv}`,
  `transport/real.nv`) — все транзитивно тянут `http.error` → `tls.tls`.
  Никакой другой домен (net/fs/io/time/encoding — их ПОЗИТИВНЫЕ файлы)
  не задет.

**STOP — 2 реальных gap'а 03.1/D412-механизма, НЕ костылил (по мандату):**

1. **`[ffi] libs` — нет lib-dir propagation, нет detect-and-degrade.**
   Генерик `[ffi]`-конвейер (`ResolvedFfiConfig`, `test_runner.rs`) для
   `libs = [...]` эмитит ТОЛЬКО голые `-l<name>` (Clang/Gcc) /
   `<name>.lib` (MSVC) — нет эквивалента `include_dirs`'ного `-I` для
   search-directory (`-L`/`/LIBPATH`), и нет detect-if-present-else-stub
   семантики, которую имели retired built-in спецкейсы (Boehm/brotli/
   старый mbedtls: находили vcpkg lib_dir explicit'но, ИЛИ тихо
   компилировали Q11-стаб при отсутствии — никогда hard link error).
   На Windows нет системного «default lib search path» аналога `/usr/lib`
   — единственный способ найти non-standard-path `.lib` это либо `LIB`
   env var (vcvars-snapshot, `env_clear()`-изолирован в MSVC-ветке — НЕ
   видит внешний `NOVA_MBEDTLS_LIB_DIR`/`NOVA_GC_LIB_DIR`), либo explicit
   `-L`/`/LIBPATH`, которого генерик-`[ffi]` не умеет декларировать.
   Итог: `mbedtls.lib`/`mbedx509.lib`/`mbedcrypto.lib` нигде не установлены
   на этой машине (не в `compiler-codegen/vcpkg_installed/`, не в
   `VCPKG_ROOT`, не системно) → **hard CC-FAIL** для ЛЮБОГО потребителя
   `tls` (не только TLS-тестов — самого факта `[dependencies] tls`
   достаточно). Раньше (built-in спецкейс) это же отсутствие mbedTLS
   давало graceful Q11-stub (RUN-FAIL на 4 теста, остальное PASS) — Ф.2
   потеряла эту деградацию, т.к. переехала на генерик-конвейер, у
   которого её никогда не было (не регрессия Ф.2, а первое реальное
   упражнение генерик-`[ffi] libs` с non-default-path нативной либой).
   Установка mbedTLS через vcpkg разблокировала бы ЭТУ конкретную
   проверку, но не саму архитектурную дыру (не трогал
   `compiler-codegen/vcpkg.json` — не мой мандат, отдельное владельческое
   решение: registry-путь для non-standard lib search paths в `[ffi]`).

2. **`embed_resolve.rs::resolve_embeds` — `project_root` не per-package,
   не 03.1-aware.** `project_root = find_repo_root_from(entry_path)`
   считается ОДИН раз от entry-файла CU (`test_runner.rs:2830`) и
   применяется как единственная граница для ВСЕХ peer-файлов слитого
   модуля, включая peer-файлы из ВНЕШНЕЙ `[dependencies]`-зависимости.
   `nova-tls`'s `*_test.nv` (folder=module, co-equal с `ffi.nv`/
   `stream.nv` — тот же `module tls.tls`) легитимно вызывают
   `embed("testdata/....pem")`, разрешаемое ОТНОСИТЕЛЬНО ИХ СОБСТВЕННОЙ
   директории (это `base_dir` — корректно, per-file) — но
   `E_EMBED_OUTSIDE_PROJECT`-проверка сверяет результат с ЕДИНСТВЕННЫМ
   `canon_root` (директория `nova-abi`, entry-файла `std/http/error.nv`),
   которая физически не включает `nova-tls/` (сиблинг-репа) → **любой**
   потребитель `tls.tls` получает `E_EMBED_OUTSIDE_PROJECT`. Блокирует
   даже голый `nova check` (не только codegen/link) — обнаружено ПОСЛЕ
   исправления off-by-one в path (`../../nova-tls`). Не пересекается с
   03.1's `lookup_dependency`/`resolved_dependency_roots` (та машинерия
   резолвит модули/FFI корректно) — отдельный, доселе неупражнённый шов
   между D412 (Plan 186, embed) и 03.1 (Plan 03.1, package deps): D412
   никогда не видел peer-файл из ДРУГОГО пакета с `embed(...)` внутри,
   пока конвенция «тесты рядом с модулем» (`feedback-module-tests-
   beside-module`) не встретилась с folder=module-схлопыванием через
   границу пакета.

**Вывод:** механика Ф.2 (файловый вынос + dependency-wiring + import fix +
retirement built-in auto-link) — корректна и подтверждена (duplicate-
symbol ушёл, δ0 conformance, чистый blast-radius). Оставшиеся 2 gap'а —
в `compiler-codegen` (генерик `[ffi]` и `embed_resolve.rs`), вне мандата
этого захода («НЕ трогай compiler-codegen кроме удаления tls_c_shim.c»);
не костылил vcpkg.json / lib_dirs / project_root logic. Ф.2-гейт
(«std/tls-тесты зелёные ЧЕРЕЗ dep», «монорепо собирается» на уровне
`std`-пакета) — **не закрыт**; нужен отдельный заход на любой из двух
gap'ов (или оба) с owner sign-off — это архитектурные решения
(lib_dirs-в-`[ffi]` формат / detect-and-degrade семантика / per-package
embed project-root), не локальные фиксы. Ф.3 (examples/tls/echo, гайд)
блокирована тем же — echo-пример не соберётся, пока gap #1 открыт.
