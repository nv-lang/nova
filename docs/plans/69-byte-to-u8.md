# Plan 69 — Remove `byte` type alias, canonicalise `u8`

## Status: ✅ ЗАКРЫТ 2026-05-22

## Problem

Nova has both `byte` and `u8` as names for the same 8-bit unsigned integer type.
Having two names creates ambiguity: identical values can be written two ways, style
inconsistency creeps into the stdlib and tests, and documentation is unclear about
which form is idiomatic.

## Decision

* Remove `byte` as a named primitive type.
* `u8` is the sole canonical name for the 8-bit unsigned integer.
* `[]u8` is the canonical type for binary data (byte slices).
* The tagged-template tag `bytes` (D48: `` bytes`deadbeef` ``) is a **function name**,
  not a type — it is **unaffected** by this change.
* Word "byte" appearing in prose comments describing byte counts / bit widths
  is English text — also **unaffected**.

## Spec change

Add D125 in `spec/decisions/02-types.md`:

> **D125** — `byte` is removed as a built-in type alias. Use `u8` everywhere.
> Binary data slices use `[]u8`. Existing code using `byte` must migrate to `u8`.

## Scope

| Location | Occurrences |
|---|---|
| `spec/decisions/` | ~65 |
| `std/` | ~212 |
| `nova_tests/` | ~92 |
| `examples/` | ~1 |

## Steps

1. [x] Create this plan document.
2. [x] Add D125 to `spec/decisions/02-types.md`.
3. [x] Replace `byte` → `u8` (тип) в `spec/decisions/` — per-occurrence,
   проза и тег `bytes` не тронуты.
4. [x] Replace `byte` → `u8` в `std/` (4 файла: `prelude/collections.nv`,
   `runtime/read_buffer.nv`, `runtime/string.nv`, `runtime/write_buffer.nv`).
5. [x] Replace `byte` → `u8` в `nova_tests/`. Фикстуры
   `plan70_4/f5_byte_u8_alias_pos.nv` и `f8_byte_u8_interop_pos.nv`
   **удалены** — они тестировали `byte`↔`u8`-алиасинг, который Plan 69
   как раз убирает.
6. [x] `examples/` — вхождений типа `byte` нет (0).
7. [x] Убрать `byte` из builtin-типов компилятора — `types/mod.rs`,
   `parser/mod.rs`, `codegen/emit_c.rs`. C-typedef `nova_byte` сохранён
   (внутреннее имя codegen для `u8`, не пользовательская поверхность).
8. [x] Build + `nova test` — зелёный.
9. [x] Commit.

## Реализация (2026-05-22)

`byte` больше не распознаётся компилятором как тип — единственное имя
8-битного беззнакового целого — `u8`, срез — `[]u8`. Прогон `nova test`
после удаления `byte` из компилятора служит enforcement'ом: любой
пропущенный `byte`-as-type → ошибка компиляции. Тег `bytes` (D48) и
слово «byte» в прозе комментариев — не затронуты (по D125).
