---
name: project-nova-test-vs-test-build
description: nova test = C-codegen pipeline; nova-codegen test-interp = интерпретатор (переименовано из nova-codegen test)
metadata: 
  node_type: memory
  type: project
  originSessionId: 4c25eee9-b442-4e6d-bdf3-3fd2c374b7bc
---

`nova test` (nova-cli) использует **C-codegen pipeline** (`test_runner::run_one`
→ CEmitter → .c → clang/msvc/gcc → нативный бинарник). Это production path.

`nova-codegen test-interp <file.nv>` — интерпретатор (`nova_codegen::interp::Interpreter`,
`cmd_test` в `compiler-codegen/src/main.rs`). Переименовано из `nova-codegen test`
(коммит efe62ccd, 2026-05-20) чтобы устранить коллизию имён.

**Why:** баги C-кодогена (неверный return type, silent wrong dispatch,
typedef redefinition) не видны через интерпретатор. Пример: в plan72 4 из 8
фикстур проходили `nova-codegen test-interp`, но падали на `nova test-build`.

**How to apply:** при работе над `emit_c.rs` / C-кодогеном проверять через
`nova test` или `nova test-build`, а не `nova-codegen test-interp`.
`nova-codegen test-interp` годится только для быстрой проверки Nova-синтаксиса.

**УРОК 2026-07-17 (стоп-волна-фантом):** `cargo build --release` в
`compiler-codegen/` пересобирает ТОЛЬКО библиотеку nova-codegen; бинарь
`nova-cli/target/release/nova.exe` этим НЕ обновляется (отдельный target).
Перед любым смоуком: `cargo build --release --manifest-path nova-cli/Cargo.toml`
из корня. Проверка свежести: mtime nova.exe против времени слияния.
И: пуш ВСЕГДА отдельной командой ПОСЛЕ зелёного смоука, не в одной цепочке.

**ЛОВУШКА include_str! (2026-07-17, slice-mono волна):** external_registry.rs
эмбеддит снимки .nv (read/write_buffer, string_builder, sync) через include_str!;
cargo incremental НЕ инвалидирует надёжно при правке только .nv → фантомные
разные ошибки на одном диффе. Перед rebuild после правки этих .nv — ОБЯЗАТЕЛЬНО
`touch compiler-codegen/src/codegen/external_registry.rs`.
