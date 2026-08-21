---
name: feedback-no-nova-codegen-direct
description: ЗАПРЕЩЕНО запускать nova-codegen напрямую; только nova.exe test
metadata: 
  node_type: memory
  type: feedback
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
---

Запускать `nova-codegen` (compiler-codegen бинарь) напрямую — ЗАПРЕЩЕНО.

**Why:** nova-codegen это internal инструмент, не пользовательский runner. Правильный путь — `nova.exe test`, который вызывает кодеген внутри себя.

**How to apply:** Всегда тестировать через `nova-cli/nova.exe test <path>`. Никаких `cargo run --manifest-path compiler-codegen/Cargo.toml -- test-build ...`.
