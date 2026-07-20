# 221 Ф.2 A-V1+A-V2 — чекпоинт

Worktree: d:/Sources/nv-lang/nova-v01 (branch p221-version-zip, base main @ 6d078bf3f)
Модель: sonnet.

## A-V1 (версия)
- nova-cli/Cargo.toml: version = "0.1.0" — УЖЕ так на базовом коммите (не менялось этой волной).
- compiler-codegen/Cargo.toml: version = "0.1.0" — уже так.
- nova-lsp/Cargo.toml: version = "0.1.0" — уже так.
- nova-cli/src/main.rs: `#[command(name="nova", version, about=...)]` — версия берётся из
  Cargo.toml через clap `version` (без литерала). `nova --version` формат — уточняю сборкой.
- nova-lsp/src/main.rs: тоже clap `#[command(... version ...)]`, плюс своя version=env!(CARGO_PKG_VERSION)
  строка на line ~75 (доп. использование, не флаг). `--version` флаг ЕСТЬ.
- Итог по факту (до сборки): версии везде уже 0.1.0 в исходниках. Нужно cargo build + smoke, чтобы
  подтвердить `nova --version` реально печатает "nova 0.1.0" (а не другую строку/старый бинарь).

## A-V2 (zip) — план
- scripts/package-release.ps1 — ещё не написан на момент чекпоинта.
- env для сборки/тестов в воркти: NOVA_GC_LIB_DIR / NOVA_GC_INCLUDE_DIR на main repo
  (см. память project-worktree-nova-test-setup) — worktree не содержит vcpkg_installed.

## Дальше по плану
1. ✅ cargo build --release (nova-cli + nova-lsp) в воркти — ЧИСТО (только warnings).
   `nova --version` → "nova 0.1.0", `-V` тоже; `nova-lsp --version` → "nova-lsp 0.1.0".
   Бинари: nova-cli/target/release/nova.exe, nova-lsp/target/release/nova-lsp.exe.
2. std-discovery разведка (ЗАВЕРШЕНА):
   - `find_repo_root()` (nova-cli/src/main.rs:1169) ищет `nova.toml` вверх от CWD —
     это КАЖДЫЙ пользовательский проект имеет свой nova.toml (нормально, не блокер).
   - `resolve_std_path(repo)` (compiler-codegen/src/manifest.rs:1263): precedence
     env `NOVA_STD_PATH` (абсолютный путь ок) → nova.toml `std=".."` ключ → default `repo/std`.
   - `resolve_paths()` (main.rs:1227): `NOVA_CG_INCLUDE` (default repo/compiler-codegen),
     `NOVA_RT_DIR` (default repo/compiler-codegen/nova_rt) — оба ENV-override, абсолютные пути ок.
   - Генерируемый C: `#include "nova_rt/nova_rt.h"` (emit_c.rs:8344) → NOVA_CG_INCLUDE
     ОБЯЗАН содержать подпапку `nova_rt/` (структура: <install>/nova_rt/...).
   - GC (Boehm): `detect_boehm(cg_include)` (test_runner.rs:4371) — precedence env
     `NOVA_GC_LIB_DIR`(+`NOVA_GC_INCLUDE_DIR`) → `<cg_include>/vcpkg_installed/...` →
     `$VCPKG_ROOT/installed/...`. Нужны только gc.lib+atomic_ops.lib (+include/gc,
     include/atomic_ops, gc.h/atomic_ops.h верхнего уровня) — НЕ весь vcpkg_installed
     (там ещё z3, ~4.3G).
   - libuv: `detect_or_build_libuv` (test_runner.rs:4302) требует `<rt_dir>/libuv/include/uv.h`
     + `<rt_dir>/eventloop.c`; кэш `<repo_root>/target/libuv-cache/libuv.lib` — если нет,
     собирает ОДИН РАЗ (~30 сек, нужен vcvars/cl.exe) из `build_libuv_lib` — компилирует
     ТОЛЬКО `src/*.c` (нерекурсивно) + `src/win/*.c` (Windows) — НЕ весь submodule
     (468M full source; нужный поднабор include+src top+src/win ≈ 51M).
   - **Вердикт: (а) — std+nova_rt(+libuv-подмножество)+gc-подмножество кладутся в zip,
     дискавери через env vars** (НЕ hack — штатная config-поверхность,
     NOVA_STD_PATH/NOVA_CG_INCLUDE/NOVA_RT_DIR/NOVA_GC_LIB_DIR/NOVA_GC_INCLUDE_DIR).
     MSVC toolchain на машине пользователя всё равно нужен (архитектурное свойство
     C-codegen, не блокер этой волны — как и Linux-рецепт "сборка из исходников").
3. ✅ scripts/package-release.ps1 написан (4 коммита частями) — параметры
   (-SkipBuild/-Version/-OutDir/-SmokeTest/-VcpkgBase), сборка/копирование
   бинарей, stage-папка (std/nova_rt-подсет/gc-подсет), setup-env.ps1 +
   README-INSTALL.md + лицензии, zip+sha256, -SmokeTest блок.
4. ✅ РЕАЛЬНЫЙ прогон (-SkipBuild -SmokeTest -VcpkgBase <main-репо>) — SMOKE TEST
   PASSED после 4 найденных и исправленных багов:
   - backtick-и в PS-строках ломали парсинг (throw-msg + README here-string) +
     файл был БЕЗ UTF-8 BOM (PS5.1 без BOM → кириллица в системной codepage →
     мусорные токены). Исправлено: убраны backtick-и, файл пересохранён с BOM.
   - libuv/src/*.c копировались БЕЗ приватных заголовков (uv-common.h,
     strscpy.h, idna.h, queue.h, win/internal.h и т.п. — лежат РЯДОМ с .c в
     src/, НЕ в include/) → cl.exe "uv-common.h: No such file". Исправлено:
     Get-ChildItem -Include "*.c","*.h" (с path\* — Include игнорируется без
     wildcard/-Recurse, PS-готча).
   - SmokeTest hello.nv/nova.toml писались Set-Content -Encoding utf8 (PS5.1
     utf8 ВСЕГДА добавляет BOM) → nova-лексер "unexpected byte 'ï'".
     Исправлено: -Encoding ascii (контент чисто ASCII).
   - package name "hello-smoke" + `module hello` нарушало D78 rev-4 root-peer
     (module==package для файла в корне source root). Исправлено: name="hello".
   - Доп.: -VcpkgBase параметр (worktree без своей vcpkg_installed — нужен
     явный путь на main-репо); SmokeTest temp-папка перенесена из %TEMP% в
     dist/ (на этой машине Expand-Archive под системным Temp воспроизводимо
     "теряла" 5/12 файлов без видимой причины — не MAX_PATH; под dist/
     стабильно 12/12).
   Финал: nova --version/-V = "nova 0.1.0"; nova-lsp --version = "nova-lsp
   0.1.0"; zip = 12.4 MB (506 файлов: std 287 + nova_rt 121 + gc 90 + верхний
   уровень); SmokeTest — hello.exe собран+выполнен из ПОЛНОСТЬЮ изолированной
   папки (свой nova.toml, dot-sourced setup-env.ps1), включая one-time
   libuv+nova_rt archive автосборку (~24 сек, MSVC vcvars на этой машине).
   SHA256 zip: b76550ac6290a255c51b221e8becd2714fd9937f940e7037b66ac26ddc75065f
   (последний успешный прогон; записан также в .sha256 рядом с zip).
5. ✅ Чекбоксы Ф.2 (версия/win-zip) отмечены в docs/plans/221-release-v0-1.md
   (версия — [~] частично: nova --version подтверждён, тег на 4 репы — вне
   скоупа; win-zip — [x] полностью).
6. Коммиты — по имени файлов, без co-author (7 коммитов в этой волне).
