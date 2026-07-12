<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 193 — nova-tls: вынос TLS в отдельную репу + examples + доки

**Статус:** 🚧 Ф.1 IN PROGRESS, BLOCKED 2026-07-12 — раскладка `nova-tls` готова и
закоммичена; rt_dir/cg_include resolver-пробел ЗАКРЫТ env-override'ом
(`NOVA_CG_INCLUDE`/`NOVA_RT_DIR`, см. «Ф.1 продолжение» ниже) — это НЕ было
работой ext-dep-resolver-03-1 (проверено, другой домен). За ним вскрылся НОВЫЙ
блокер: ложный `D133-not-consumed` на consume-param transfer в отдельно-
пакетном (не `std`) контексте — требует отдельной разведки compiler-checker'а.
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
