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
1. cargo build --release (nova-cli + nova-lsp) в воркти, подтвердить --version.
2. Написать scripts/package-release.ps1 (PS 5.1-совместимый, без && / ternary).
3. Прогнать реально: zip → распаковать в чистую temp-папку → nova.exe --version →
   hello-smoke (build+test) → выяснить std-discovery (NOVA_STD_PATH? relative-to-exe?).
4. По итогам std-discovery: либо включить std+nova_rt в zip и подтвердить работу из чистой
   папки, либо задокументировать честный блокер [M-release-std-discovery].
5. Отметить чекбоксы Ф.2 (строки версия/win-zip) в docs/plans/221-release-v0-1.md.
6. Коммит(ы) — по имени файлов, без co-author.
