# D180 migration — `consume X = …` binding syntax

> Если вы видели **`E_CONSUME_KEYWORD_MISSING`** или
> **`E_VIEW_BINDING_FORBIDDEN`** в `nova check` / `nova build`,
> этот документ объясняет что изменилось и как мигрировать.

## TL;DR

Bindings consume-обязательного значения требуют keyword `consume`:

```nova
ro g = mu.lock()       // ✗ старое — E_CONSUME_KEYWORD_MISSING
consume g = mu.lock()   // ✓ новое
```

Alias-binding consume-obligation запрещён в теле функции:

```nova
consume sb = StringBuilder.new()
ro view = sb           // ✗ E_VIEW_BINDING_FORBIDDEN
```

Для transfer ownership — используйте `consume Y = X` (move):

```nova
consume a = Token.new(1)
consume b = a           // ✓ move (a dead, b owns)
```

## Что изменилось

[D-block D180](../../spec/decisions/05-memory.md#d180) фиксирует
синтаксис binding'ов для consume-типов:

- **Rule 1**: binding consume-обязательного RHS требует keyword
  `consume X = …`.
- **Rule 2**: `let Y = consume_obligated` в теле функции запрещён
  (view-binding allowed только как function-параметр).
- **Rule 3**: `consume Y = X` где X — consume-obligation = move
  (X dead, Y owns).
- **Rule 4**: function-параметр consume-типа без `consume` keyword =
  view-borrow (bounded by callee scope).

До Plan 73.1:
- `let g = mu.lock()` компилировался — flow analyzer (D131) ловил
  отсутствие consume только при scope-exit.
- Алиас `let view = consume_var` создавал dangling reference потенциал.

После Plan 73.1:
- Syntax-уровень enforcement через keyword `consume`.
- Алиасы в body forbidden ahead-of-time (E_VIEW_BINDING_FORBIDDEN).
- D131 flow analysis работает над hardened syntactic foundation.

## Diagnostics

### `E_CONSUME_KEYWORD_MISSING`

```
error[E_CONSUME_KEYWORD_MISSING]: binding `g` держит consume-обязательную
   инстанс типа `MutexGuard` — требуется keyword `consume` (D180).
  --> file.nv:5:5
   |
 5 |     let g = mu.lock()
   |     ^^^^^^^^^^^^^^^^^
   = note: consume-обязательные значения должны быть явно ownership-bound
           через `consume X = …`. Альтернатива: передать в function-параметр
           для view-borrow.
   help: add `consume` keyword: `consume g = …`
   |
 5 |     consume g = mu.lock()
   |     ~~~~~~~
```

### `E_VIEW_BINDING_FORBIDDEN`

```
error[E_VIEW_BINDING_FORBIDDEN]: view-binding на consume-обязательную
   переменную `sb` запрещён в теле функции (D180).
  --> file.nv:7:5
   |
 7 |     let view = sb
   |     ^^^^^^^^^^^^^
   = note: views существуют ТОЛЬКО как function-параметры (D157). Для
           transfer ownership используй `consume X = …` (move); для
           view-borrow перенеси в function-параметр.
   help: use `consume sb = …` для move ownership
```

### `W_CONSUME_KEYWORD_UNNECESSARY`

```
warning[W_CONSUME_KEYWORD_UNNECESSARY]: keyword `consume` на binding `n`
   избыточен — RHS типа `int` не consume-обязателен (D180).
  --> file.nv:3:5
   |
 3 |     consume n = 42
   |     ^^^^^^^^
   help: delete `consume ` keyword
   |
 3 |     let n = 42
   |     ~~~
```

## Migration recipes

### Recipe A — обычный `let → consume` для известных consume-типов

stdlib consume-types: `MutexGuard`, `ReadGuard`, `WriteGuard`,
`Permit`, `OnceGuard`, `StringBuilder`.

```nova
// Before
ro g = mu.lock()
ro sb = StringBuilder.new()

// After
consume g = mu.lock()
consume sb = StringBuilder.new()
```

### Recipe B — `let alias = X` → consume move OR remove alias

```nova
// Before — alias inside function body
ro mu = Mutex.new()
consume g = mu.lock()
ro g2 = g          // ✗ E_VIEW_BINDING_FORBIDDEN
g2.unlock()

// After — option 1: move ownership explicitly
ro mu = Mutex.new()
consume g = mu.lock()
consume g2 = g      // ✓ move — g dead, g2 owns
g2.unlock()

// After — option 2: just rename, no alias needed
ro mu = Mutex.new()
consume g = mu.lock()
g.unlock()
```

### Recipe C — view-borrow через function parameter

```nova
// Before — alias for «read-only» peek
consume sb = StringBuilder.new()
sb.append("hi")
ro view = sb              // ✗
print(view.len())

// After — function-param view
fn print_len(s StringBuilder) -> int => s.len()
                            // ↑ view-borrow: s bounded by call

consume sb = StringBuilder.new()
sb.append("hi")
ro n = print_len(sb)      // ✓ view-borrow during call
ro v = sb.as_str()        // sb still live in caller
```

### Recipe D — fluent-chain через `-> @` методы

Fluent методы (`-> @`) возвращают receiver — chain mutators всё ещё
требует `consume` на initial binding:

```nova
consume sb = StringBuilder.with_capacity(8)
sb.append("a").append("b")     // chain — sb still live
ro s = sb.as_str()             // consume
```

Когда chain — trailing функции, M3 (Plan 73.1 V3) детектит implicit
return как consume operation:

```nova
fn build() -> StringBuilder {
    consume sb = StringBuilder.new()
    sb.append("x").append("y")  // chain as trailing — implicit return
}                               // sb consumed by implicit return ✓
```

## Automated migration

Plan 100.7 ([D165](../../spec/decisions/05-memory.md#d165)) предоставляет
`nova consume-migrate` для automated rewrite simple `let → consume`
cases.  Run:

```bash
nova consume-migrate ./src
```

Tool применяет machine-applicable suggestions из
`E_CONSUME_KEYWORD_MISSING`.  Manual review needed для:

- alias-патернов (E_VIEW_BINDING_FORBIDDEN) — recipe B/C
- `for consume X in iter` — Plan 100.2 D156 generic-bound сценарии

## Cross-module consume-types

Plan 73.1 V3 ([M-73.1-cross-module-consume-detection]) расширил
type-checker project-wide registry: consume-types declared в
`std/runtime/sync.nv` (MutexGuard etc.) распознаются в user-code:

```nova
import std.runtime.sync

ro mu = Mutex.new()
ro g = mu.lock()       // ✓ детектит — E_CONSUME_KEYWORD_MISSING
consume g = mu.lock()   // ✓ correct
```

## Acceptance

После миграции `nova check` clean — 0 `E_CONSUME_KEYWORD_MISSING` /
`E_VIEW_BINDING_FORBIDDEN`.  Property-test
`binding_migration_completeness_prop` (plan73_1) — runtime witness.

## References

- `spec/decisions/05-memory.md` — D131 (affine semantics), D180
  (binding syntax)
- `docs/consume-types.md` — user guide
- `docs/plans/73.1-consume-binding-syntax.md` — Plan 73.1 status
- `docs/plans/100.7-stdlib-migration-playbook.md` — D165 migration
  playbook
