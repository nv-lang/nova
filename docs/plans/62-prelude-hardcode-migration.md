# Plan 62: Migrate hardcoded prelude items в `std/prelude.nv`

> **Status:** proposed (2026-05-17). Quality / spec-hygiene work.

---

## Problem

`std/prelude.nv` сейчас содержит только `PRELUDE_VERSION` marker (auto-import
mechanism от Plan 35.A R27 работает, но prelude **пустой**). Все «обычно
доступные» имена hardcoded в type-checker / codegen:

```
Option, Result, Some, None, Ok, Err, Error, Never,
print, println, panic, assert, debug_assert, ...
```

Это means:
- Type-checker имеет special-case для каждого. Грязь, дублирование.
- Невозможно user-shadow / customize prelude (e.g. `import std.prelude.{Option}`).
- Spec'у D26 (prelude) надо описывать «как hardcoded» — нет канонической
  публичной декларации.
- Не AI-friendly: LLM не может посмотреть `std/prelude.nv` и understand `Option`.

## Что делаем

Постепенный migration:

1. **Перенести type declarations** в `std/prelude.nv`:
   - `export type Option[T] | None | Some(T)`
   - `export type Result[T, E] | Ok(T) | Err(E)`
   - `export type Error { msg str }` (или аналог)
   - `export type Never` (если есть в spec'е как тип)

2. **Перенести функции**:
   - `print(s str) -> ()`
   - `println(s str) -> ()`
   - `panic(msg str) -> Never`
   - `assert(cond bool) -> ()` / `debug_assert(...)`

3. **Удалить special-cases** из type-checker / codegen после переноса
   (по одному, тестируя на каждом шаге).

4. **Wire через `resolve_imports_inline`**: prelude items резолвятся
   как обычные cross-file imports (уже работает для `PRELUDE_VERSION`).

## Что НЕ делаем

- **Не трогаем `()` / `int` / `str` / `bool` / `f64` / etc.** — это built-in
  *primitives*, не prelude. Их декларации остаются в type-checker.
- **Не удаляем `Iter[T]`** — это structural protocol (D58), уже работает через
  duck-typing, не через prelude.

## Sub-tasks

### Ф.1: Option/Some/None
Перенести `Option[T] = None | Some(T)` в prelude. Test через:
- Standard library usages (`std/collections/hashmap.nv`, и т.п.) — не регрессят.
- `nova_tests/prelude/option_via_prelude.nv` — explicit test что
  `Option` доступен **только** через auto-imported prelude.

### Ф.2: Result/Ok/Err
То же для `Result[T, E] = Ok(T) | Err(E)`.

### Ф.3: print/println/panic
Перенести as external fns (без body — runtime impl остаётся в C). Удалить
special-case в emit_call для name == "print"/"println"/"panic".

### Ф.4: assert/debug_assert
Аналогично. После Plan 11 fix (`a37a4b9`) assert эмитится через comma-operator,
переезд в prelude должен сохранить тот wrapping.

### Ф.5: Error/Never
Опциональные types (если spec их фиксирует как prelude).

### Ф.6: docs/spec amendments
- `spec/decisions/03-syntax.md` D26 (prelude) — обновить с конкретным
  списком items в `std/prelude.nv`.
- `docs/idioms/prelude-customization.md` — как user может shadow prelude
  через explicit import.

## Scope / Estimate

- Type-checker / codegen: удалить ~10-15 special-cases. ~200-300 LOC removed.
- `std/prelude.nv`: добавить ~50-100 LOC (type/fn declarations).
- Tests: ~20 regression + ~5 explicit prelude visibility tests.
- Spec: 1 D-block amend.

**Total estimate:** 3-5 dev-days.

## Open questions

- **Bootstrap order**: prelude.nv импортируется в **каждый** module, включая
  стандартную библиотеку. `Option` declared в `std/prelude.nv` будет
  visible в `std/collections/hashmap.nv` (который **тоже** prelude'нут).
  Циклический import? Или prelude excluded из своего же auto-import (анти-recursion)?
- **`print` / `println` runtime backing**: external C fns. Migration просто
  переписывает Nova-side stub, runtime не меняется.
- **Backwards compat**: всё user-code продолжает работать (имена те же).

## Связь с другими планами

- **Plan 35.A R27** (закрыто): auto-import mechanism — fundament.
- **Plan 11**: assert/panic — уже emit правильно после fix'ов в `a37a4b9`.
- **Plan 45** (`nova doc`): после migration prelude items появятся в
  `nova doc std/prelude.nv` как обычный module — AI-консумеры увидят
  каноническую декларацию.

## Acceptance criteria

- ✅ `std/prelude.nv` содержит explicit declarations for all currently-hardcoded items.
- ✅ Type-checker special-cases для prelude items удалены (трасс через grep).
- ✅ Все 568+ regression tests PASS.
- ✅ User может `import std.prelude.{Option as Opt}` (custom rename).
- ✅ `nova doc std/prelude.nv` показывает полный prelude API.

## Ссылки

- `std/prelude.nv` — текущий placeholder.
- [Plan 35](35-cross-file-resolve.md) — R27 origin.
- `spec/decisions/03-syntax.md` D26 — prelude spec.
- `compiler-codegen/src/types/mod.rs` — type-checker special cases для Option/Result/...
- `compiler-codegen/src/codegen/emit_c.rs` — codegen для println/panic/assert.
