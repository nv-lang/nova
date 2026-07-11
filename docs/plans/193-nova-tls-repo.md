<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 193 — nova-tls: вынос TLS в отдельную репу + examples + доки

**Статус:** 📋 PROPOSED 2026-07-11 (разблокирован: 03.1 закрыт, mbedTLS-бэкенд готов).
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
