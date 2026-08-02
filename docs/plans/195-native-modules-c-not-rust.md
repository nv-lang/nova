<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 195 — `src/` layout std-миграция (общий native-модуль-паттерн → доки)

**Статус:** 🟢 ЗАВЕРШЁН 2026-07-13 — Ф.1-2 (mbedTLS-своп TLS, Rust удалён, T40) И
`src/`-раскладка std (git mv 314 файлов, компилятор переведён на манифест-derived
резолв, доки поправлены) ВЫПОЛНЕНЫ. Ф.3 (вынос nova-tls) → **[Plan 193](193-nova-tls-repo.md)**
(уже эталонная инстанция паттерна). Общий native-модуль-паттерн (.nv+.c+.lib) — в доках.
**Зависит:** [03.1](03.1-path-git-dependencies.md) (резолвер внешних зависимостей — путь A) для внешне-репового этапа.
**Общий native-модуль-паттерн** (.nv+.c+.lib, `[ffi]`, `nova-<пакет>`-нейминг D78) —
живёт в доках: [ffi-cookbook](../guide/ffi-cookbook.md) · [module-conventions](../dev/module-conventions.md) ·
[authoring-a-module](../guide/authoring-a-module.md). Этот план теперь фокусируется на
**`src/`-раскладке std-миграции** (остаток; паттерн доказан mbedTLS-волной T40).

## Архитектурное решение владельца (2026-07-10)

Тулчейн Nova = **компилятор Nova + clang** (`.nv → .c → бинарь`). Модуль обязан
собираться БЕЗ Rust/cargo. Пользователь не должен тянуть Rust-компилятор ради модуля.

**Канон native-модуля:** чистый **.nv** + опционально **.c** (компилит clang, он в
тулчейне) + опционально **.lib** (готовая библиотека — линкуется, не собирается).
Механизм — существующий **`[ffi]`** (`c_shims` → clang, `include_dirs`, `libs` → link).
**Никакого `[ffi.staticlib] build="cargo"`** как пользовательского паттерна.

## Что исправить

### 1. TLS: rustls (Rust) → C-TLS-библиотека (Вариант 1 владельца)

Текущий `compiler-codegen/tls_shim/` — Rust-staticlib (rustls+ring). Заменить на
**BearSSL** или **mbedTLS** (маленькие, встраиваемые, чистый C):
- .nv-фасад (`std/tls/*` — сохранить публичную поверхность: TlsStream, cert-modes,
  mTLS, Pinned) поверх .c-шима к C-TLS + прилинкованной .lib.
- Выбор библиотеки (sign-off): **BearSSL** (крошечная, MIT, без malloc, embeddable) vs
  **mbedTLS** (шире, Apache-2, больше фич/популярнее). Рекомендация — mbedTLS (полнее
  cert-verification/ALPN, активнее), если размер не критичен; BearSSL — если нужен
  минимальный след.
- Симметрия tests: cert-modes/mTLS/Pinned/https-разгейт (TLS-116 Ф.4-Ф.5.3) переносятся
  на C-шим — поведение то же, backend другой.

### 2. Убрать Rust-паттерн из компилятора и доков

- После миграции TLS `tls_shim`-Rust не нужен → **удалить `[ffi.staticlib]`-cargo**
  (manifest.rs) и `tls_shim/`-crate; `detect_tls` legacy-путь снять.
- Доки: `[ffi]` (.c+.lib) — единственный native-канон; убрать упоминания
  cargo/staticlib-Rust как паттерна для пользователя.

### 3. Настоящий `nova-tls` (после 03.1)

Вынести реальный tls (уже на C) в отдельную репу `nova-tls` — **настоящий рабочий
модуль**, не демка: .nv-фасад + `native/` (.c-шим + prebuilt .lib или исходники
C-TLS) + README + LICENSE. Монорепо тянет его как внешнюю зависимость (03.1).
Раскладка — по решению открытого вопроса `src/` vs плоско (ниже).

## Раскладка пакета — РЕШЕНО: `src/` (владелец 2026-07-10)

Канон: манифест (`nova.toml`) + README + LICENSE + `native/` в корне, весь `.nv` — в
**`src/`** (как Rust/npm/Python/Zig). Отменяет плоское решение 2026-05-22. Механизм в
манифесте уже был (`[lib] src`, депрекнут но «legacy уважается») — переоживить/канонизировать.
Module-path — относительно `src/` (D78, не включает `src`). Ради согласованности `std`
перевести на `src/` — отдельный механический под-план (большой git mv, module-path не
меняется). **Остаётся один sub-sign-off:** C-TLS-библиотека — BearSSL vs mbedTLS (рек. mbedTLS).

## Фазы

- **Ф.0** sign-off: C-TLS-библиотека (BearSSL/mbedTLS) + раскладка (src/ vs плоско).
- **Ф.1** ✅ .c-шим к C-TLS + .nv-фасад std/tls (T40, mbedTLS 3.6.2, поверхность сохранена, 29/29).
- **Ф.2** ✅ удалён Rust tls_shim + `[ffi.staticlib]`-cargo + legacy detect_tls; доки на `[ffi]` (T40).
- **Ф.3** (вынос nova-tls) — **ВЫНЕСЕНА в [Plan 193](193-nova-tls-repo.md)** (2026-07-11, чтобы TLS-домен не дублировался с 116). Здесь остаётся ОБЩИЙ паттерн; nova-tls (193) = его эталонная инстанция.
- **Ф.4** ✅ (2026-07-13) `src/`-раскладка std: `git mv` всего содержимого `std/` (314 файлов,
  кроме `nova.toml`) → `std/src/**`, module-path не изменился. `std/nova.toml`: `[lib] src = "src"`.
  Компилятор: `resolve_std_path` (`manifest.rs`) теперь читает `[lib] src` найденного std-корня и
  возвращает SOURCE ROOT (не package root) — покрывает ~20 call-сайтов (imports.rs, nova-cli,
  nova-lsp) без правки каждого. `cmd_unicode`/`cmd_emit_runtime_stubs`/`cmd_regen_runtime`/
  `should_skip_path_full` переведены на манифест-derived путь вместо хардкода `std/...`.
  `include_str!` в `external_registry.rs` (компилятор собирает СЕБЯ — манифест читать некогда) —
  единственный легальный литеральный хардкод `std/src/...`, помечен doc-комментарием. Доки
  (`docs/promts/*`, `test-conventions.md`, `module-conventions.md`, `ffi-cookbook.md`) и спека
  (D78-амендмент, `spec/decisions/07-modules.md`) поправлены. Новая форма команды: `nova test
  std/src/<домен>` (старая `std/<домен>` больше не существует на диске).

## Гейты

- ✅ `cargo build` (compiler-codegen + nova-cli + nova-lsp) — чист (0 errors, только pre-existing warnings).
- ✅ `spec_tests/conformance` (один CU, `--jobs 4`) — **97 PASS / 0 FAIL**.
- ✅ Таргетные std-тесты (`nova test std/src/collections std/src/data`) — **16 PASS / 0 FAIL** (9 SKIP — lib-файлы без test-блоков, норма).
- ✅ `nova check std` — работает (126 PASS type-check; 21 FAIL — ВСЕ под `neg/`/`*_neg` — by-design invalid fixtures, ожидаемо не проходят `check` вне test-harness).
- std/tls тесты зелёные на C-backend (cert-modes/mTLS/Pinned/https) — из Ф.1/Ф.2 (T40); grep-инвариант
  «нет Rust-crate в native-пути модулей»; nova-tls собирается standalone БЕЗ Rust (только clang + .lib).

## Границы

Не меняет публичный API tls (backend-своп) и std (module-path 1:1 сохранён). http — тем же
паттерном после tls.
