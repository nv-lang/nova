---
name: reference-nova-int-intptr-not-i64
description: "int=nova_int=intptr_t (address-sized, Go intgo) — НЕ int64_t; i64=int64_t; РАЗНЫЕ C-типы/mangle хоть на 64-бит совпадают по ширине"
metadata: 
  node_type: memory
  type: reference
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
---

`int` в Nova = `nova_int` = **`typedef intptr_t`** (знаковое **address-sized**, модель Go
C-эры `intgo`, Plan 133) — **НЕ `int64_t`**. См. `compiler-codegen/nova_rt/nova_rt.h:17`.

- `i64` = `int64_t` (всегда ровно 64 бита); `uint` = `nova_uint` = `uintptr_t`; `u64` = `uint64_t`.
- На bootstrap-таргете (x86_64) `int`/`i64` СОВПАДАЮТ по ширине/значению, **НО это РАЗНЫЕ C-типы**:
  `primitive_name_to_c` (emit_c.rs) даёт `int→nova_int`, `i64→int64_t` → **разный mangle**
  (`NovaOpt_nova_int` ≠ `NovaOpt_int64_t`, `Map[int,V]` ≠ `Map[i64,V]`). «int ≡ i64» = совпадение
  ШИРИНЫ, НЕ тождество типов (аналогия Go `int`≠`int64`, Rust `isize`≠`i64`).
- **НЕ предполагать int == i64/int64** (рекуррентная ошибка владелец указывал ≥2×). Это часть
  named-priority §0: i64/char НЕ схлопывать в nova_int ([[feedback-plan172-whole-not-half]]).
- Spec: D129 в `spec/decisions/02-types.md` — заголовок/тело «alias i64 / typedef int64_t /
  mangle идентичен» **УСТАРЕЛИ** (Plan 70.4, до Plan 133); добавлен крупный ⚠️ AMEND-callout
  (commit 4b0222c3). char = codepoint = `nova_char` ≠ int (D327/D128).
