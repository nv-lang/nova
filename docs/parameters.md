# Параметры функций в Nova

> User-facing guide по модификаторам параметров и их семантике.

## TL;DR

Параметры функций — **read-only по умолчанию**.  Хочешь менять — пиши `mut`.

```nova
fn append(mut b []int, v int) { b.push(v) }   // ✓ mutates
fn count(b []int) -> int => b.len()           // ✓ read-only (default)
fn count(readonly b []int) -> int => b.len()  // ✓ readonly (synonym default)
fn drain(consume b []int) { ... }             // ✓ ownership transfer
```

## Модификаторы

| Модификатор | Что разрешено в callee | Передача в caller'е |
|---|---|---|
| (нет) — default | чтение, итерация, non-mut методы | borrow (caller owns) |
| `mut` | + mut-методы (`.push`, `.append`, и т.п.), index-assign | borrow (caller owns) |
| `readonly` | то же что и default — synonym | borrow (caller owns) |
| `consume` | всё (owned), включая mut-методы | move (caller-binding мёртв) |

## Правила сочетания

- `mut` + `consume` — ✗ `E_PARAM_MOD_CONFLICT` (consume уже подразумевает mut)
- `mut` + `readonly` — ✗ `E_PARAM_MOD_CONFLICT` (взаимоисключают)
- `readonly` + `consume` — ✗ `E_PARAM_MOD_CONFLICT` (readonly запрещает мутацию, consume требует владения)

## Когда использовать что

### `mut` — нужно изменить и вернуть caller'у изменённое

```nova
fn append_world(mut sb StringBuilder) { sb.append(" world") }

let sb = StringBuilder.from("hello")
append_world(sb)
let s = sb.as_str()                  // "hello world" — мутация видна
```

### default или `readonly` — только читать (с производством результата)

```nova
fn sum(b []int) -> int {
    let mut total = 0
    for x in b { total = total + x }
    total
}
```

Используй `readonly` явно, когда хочешь подчеркнуть гарантию в API
(особенно для FFI/документации):

```nova
export fn hash(readonly bytes []u8) -> u64 => ...
```

### `consume` — забираешь ownership

```nova
fn finalize(consume sb StringBuilder) -> str => sb.as_str()

consume sb = StringBuilder.from("x")
let s = finalize(sb)                  // sb dead after this
```

## Диагностики

| Код | Когда |
|---|---|
| `E_PARAM_NOT_MUT` | вызов mut-метода на параметре без `mut` |
| `E_PARAM_MOD_CONFLICT` | взаимоисключающие модификаторы |
| `E_READONLY_COERCE` | передача `readonly T` в `T` параметр (где `T` ожидает не-readonly) |

Все с machine-applicable suggestions.

## Coercion (subtyping) для параметров

Поскольку `T` в позиции параметра **уже readonly** (Plan 108.1 default),
большинство комбинаций — тождество.  Единственное нарушение:
`readonly → mut`.

| caller-type → callee-param | OK? |
|---|---|
| `T` → `T` (param default readonly) | ✓ (сужение) |
| `T` → `readonly T` (param explicit readonly) | ✓ (synonym default) |
| `T` → `mut T` (param explicit mut) | ✓ (caller разрешает mut) |
| `readonly T` → `T` (param default readonly) | ✓ — оба readonly |
| `readonly T` → `readonly T` | ✓ |
| `readonly T` → `mut T` (param explicit mut) | ✗ `E_READONLY_COERCE` |
| `mut T` → `T` (param default readonly) | ✓ (сужение) |
| `mut T` → `mut T` | ✓ |

## Receiver methods (методы)

Receiver mutability задаётся отдельно от обычных параметров:

```nova
fn StringBuilder @len() -> int               // read-only receiver
fn StringBuilder mut @append(s str) -> @     // mut receiver
fn StringBuilder consume @as_str() -> str    // consume receiver
```

## Локальные let-bindings (Plan 108.2)

Внутри тела функции локальные binding'и подчиняются тому же правилу,
что и параметры: **без `mut` — read-only**.

```nova
let arr = []
arr.push(1)                       // ✗ E_LOCAL_NOT_MUT
let mut arr = []
arr.push(1)                       // ✓
```

`consume X = ...` неявно подразумевает `mut` (как `consume` param).

## Loop-var и pattern (Plan 108.3)

### `for mut x in iter`

Переменная цикла по умолчанию read-only.  Opt-in `mut`:

```nova
for x in arrs { x.push(1) }       // ✗ E_LOCAL_NOT_MUT
for mut x in arrs { x.push(1) }   // ✓
```

`for consume x in iter` — implicit mut (ownership transfer).

### Pattern per-name mut

При destructure `mut` ставится **на каждое имя отдельно** (Rust-style):

```nova
let (a, b) = pair                  // оба immutable
let (mut a, b) = pair              // a mutable, b immutable
let (a, mut b) = pair              // a immutable, b mutable
let (mut a, mut b) = pair          // оба mutable
```

**Запрет group-mut** — `let mut (a, b) = ...` parser-level отвергается
(`E_PATTERN_GROUP_MUT`): `mut` keyword относится к одному имени,
не к pattern целиком.

## Ссылки

- `spec/decisions/02-types.md` D176 — formal spec params.
- `spec/decisions/02-types.md` D36 + amend Plan 108.2/108.3 — formal spec locals + loop-var + pattern.
- `docs/migration/d176-param-readonly-default.md` — params migration guide.
- `docs/migration/d36-let-mut-enforcement.md` — locals migration guide.
- D131 (Plan 73) — consume affine semantics.
- D157 (Plan 100.3) — view-borrow для consume-типов.
