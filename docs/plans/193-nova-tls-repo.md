<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 193 — nova-tls: вынос TLS в отдельную репу + examples + доки

**Статус:** 🚧 Ф.1 IN PROGRESS, BLOCKED 2026-07-11 — раскладка `nova-tls` готова и
закоммичена, standalone `nova test` упирается в архитектурный пробел компилятора
(rt_dir/cg_include resolver жёстко на repo-root пакета, без env-override — см.
секцию «Ф.1 блокер» ниже). Ждёт ext-dep/toolchain-runtime-resolver работы.
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
