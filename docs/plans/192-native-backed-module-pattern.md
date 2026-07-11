<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 192 — Native-backed модуль — СЛИТ В [Plan 195](195-native-modules-c-not-rust.md)

**Статус:** ⛔ SUPERSEDED 2026-07-11 (был дублем). Исходный 192 предлагал образец на
Rust-staticlib (`[ffi.staticlib] build="cargo"`) — **ошибочный подход** (владелец: модуль
= .nv + .c + .lib, БЕЗ Rust). Актуальный дизайн, механизм `[ffi]`, конвенция именования
`nova-<пакет>` (амендмент D78) и вынос native-модулей — **всё в [Plan 195](195-native-modules-c-not-rust.md)**.

Оставлен как редирект (на файл ссылаются ffi-cookbook/authoring-a-module/D78) — контент
не дублируется, единственный источник = Plan 195.
