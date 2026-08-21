---
name: project-include-str-touch-trap
description: "include_str-ловушка: после правки .nv-снимков (string_builder/read_buffer/write_buffer/sync) ОБЯЗАТЕЛЕН touch compiler-codegen/src/codegen/external_registry.rs — ТОЧНЫЙ путь с codegen/ в середине"
metadata: 
  node_type: memory
  type: project
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
---

Файл, эмбеддящий include_str!-снимки .nv (string_builder.nv, read_buffer.nv,
write_buffer.nv, sync.nv и др.) — это
**`compiler-codegen/src/codegen/external_registry.rs`** (подпапка `codegen/`!).

Ловушка 2026-07-18: `touch compiler-codegen/src/external_registry.rs` (без
`codegen/`) МОЛЧА СОЗДАЁТ ПУСТОЙ нетрекнутый файл и не инвалидирует ничего —
cargo build проходит, симптомы фантомные. Обнаружено по `??` в git status.

Правило: после любой правки .nv-снимков —
`touch compiler-codegen/src/codegen/external_registry.rs` (полный путь), затем
пересборка `cargo build --release --manifest-path nova-cli/Cargo.toml` и
повтор таргет-смоука. Проверка, что путь верный: `git ls-files | grep
external_registry` должен показать ровно этот путь.

Связано: [[feedback-maximize-nv-sourcing]].
