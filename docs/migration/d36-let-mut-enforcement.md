# D36 migration — `let mut` для мутирующих локалов (Plan 108.2)

> Если вы видели **`E_LOCAL_NOT_MUT`** в `nova check` / `nova build`,
> этот документ объясняет что изменилось и как мигрировать.

## TL;DR

```nova
// Было (V1 — D36 spec, но без enforcement):
let arr = [1, 2, 3]
arr.push(4)                       // компилировалось — silent mutability

// Стало (Plan 108.2 — enforcement):
let arr = [1, 2, 3]
arr.push(4)                       // ✗ E_LOCAL_NOT_MUT
let mut arr = [1, 2, 3]
arr.push(4)                       // ✓
```

## Что изменилось

[D36 amendment](../../spec/decisions/02-types.md#d36) (Plan 108.2):

- `let x = ...` — immutable binding (как раньше по spec).
- `let mut x = ...` — mutable binding (явный opt-in).
- `consume x = ...` — implicit mut (ownership transfer).

Раньше spec говорил то же самое, но компилятор пропускал `let arr = [];
arr.push(...)`.  Теперь это `E_LOCAL_NOT_MUT`.

## Migration recipes

### Recipe A — массивы

```nova
// Before
let arr = [1]
arr.push(2)

// After
let mut arr = [1]
arr.push(2)
```

### Recipe B — StringBuilder

```nova
// Before
let sb = StringBuilder.new()
sb.append("hello")

// After
consume sb = StringBuilder.new()    // consume → implicit mut
sb.append("hello")
// или
let mut sb = StringBuilder.new()
sb.append("hello")
```

### Recipe C — sync примитивы (Mutex, WaitGroup, Semaphore, etc.)

```nova
// Before
let mu = Mutex.new()
let g = mu.lock()

// After
let mut mu = Mutex.new()
let g = mu.lock()
```

Аналогично для `WaitGroup`, `Semaphore`, `RwLock`, `Once`, `Condvar`,
`Barrier`, `CountDownLatch`, `ReentrantMutex`.

### Recipe D — Field assignment

```nova
// Before
let b = Box.new(1)
b.value = 99

// After
let mut b = Box.new(1)
b.value = 99
```

### Recipe E — HashMap

```nova
// Before
let m = HashMap[str, int].new()
m.insert("k", 1)

// After
let mut m = HashMap[str, int].new()
m.insert("k", 1)
```

## Automated migration

Quick grep:

```bash
nova check 2>&1 | grep "E_LOCAL_NOT_MUT" | awk -F: '{print $1}' | sort -u
```

Затем для каждого binding'а в ошибке — добавь `mut` после `let`.

## Recipe F — Loop-var (Plan 108.3)

```nova
// Before — error
for x in arrs { x.push(99) }      // ✗ E_LOCAL_NOT_MUT

// After
for mut x in arrs { x.push(99) }  // ✓
```

## Recipe G — Pattern per-name mut (Plan 108.3)

```nova
// Before
let (a, b) = pair
a.push(1)                         // ✗ E_LOCAL_NOT_MUT

// After (per-name)
let (mut a, b) = pair             // ✓ a mutable, b immutable
a.push(1)

// Запрет — group mut
let mut (a, b) = pair             // ✗ E_PATTERN_GROUP_MUT
```

## Symmetry с Plan 108.1 (params)

| Контекст | Default | Opt-in mut |
|---|---|---|
| Param (Plan 108.1) | readonly | `fn f(mut b T)` |
| Local (Plan 108.2) | readonly (immutable) | `let mut x = ...` |
| Loop-var (Plan 108.3) | readonly | `for mut x in iter` |
| Pattern element (Plan 108.3) | readonly | `let (mut a, b) = ...` per-name |
| Field (D36 + D175) | mutable у mut-binding | `mut field` для cache, `readonly field` для freeze |

## Ссылки

- `spec/decisions/02-types.md` D36 (amended Plan 108.2 + 108.3).
- `docs/parameters.md` — обновлено: локалы тоже default readonly.
- `docs/plans/108.2-locals-readonly-default.md` — plan status.
- Plan 108.1 — symmetric для params.
