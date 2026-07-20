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
3. Написать scripts/package-release.ps1 (PS 5.1-совместимый, без && / ternary) — В РАБОТЕ.
4. Прогнать реально: zip → распаковать в чистую temp-папку → nova.exe --version →
   hello-smoke (build+test) с env vars, указывающими в распакованную папку.
5. Отметить чекбоксы Ф.2 (строки версия/win-zip) в docs/plans/221-release-v0-1.md.
6. Коммит(ы) — по имени файлов, без co-author.
