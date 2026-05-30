# D176 migration — параметры read-only по умолчанию (Plan 108.1)

> Если вы видели **`E_PARAM_NOT_MUT`** или **`E_PARAM_MOD_CONFLICT`** в
> `nova check` / `nova build`, этот документ объясняет что изменилось и
> как мигрировать.

## TL;DR

Параметры функций больше не позволяют вызов mut-методов без явного `mut`:

```nova
// Было (V1, Plan 108):
fn f(b []int) { b.push(1) }       // компилировалось — silent mutability

// Стало (Plan 108.1):
fn f(b []int) { b.push(1) }       // ✗ E_PARAM_NOT_MUT
fn f(mut b []int) { b.push(1) }   // ✓ explicit mut
```

## Что изменилось

[D176 amendment](../../spec/decisions/02-types.md#d176) (Plan 108.1):

- Параметр без модификатора — read-only.
- `mut` стал semantic ключом (раньше был noop в bootstrap GC).
- 3 новых сочетания запрещены parser-level: `mut`+`consume`, `mut`+`readonly`,
  `consume`+`mut` (последнее уже было запрещено по D131).

## Migration recipes

### Recipe A — Простая мутация в callee

```nova
// Before
fn append(b []int, v int) { b.push(v) }

// After
fn append(mut b []int, v int) { b.push(v) }
```

### Recipe B — StringBuilder в качестве sink

```nova
// Before
fn Point @fmt(sb StringBuilder) {
    sb.append("(")
    // ...
}

// After
fn Point @fmt(mut sb StringBuilder) {
    sb.append("(")
    // ...
}
```

### Recipe C — AtomicInt counter

```nova
// Before
fn release(counter AtomicInt) -> int {
    counter.fetch_add(1)
    0
}

// After
fn release(mut counter AtomicInt) -> int {
    counter.fetch_add(1)
    0
}
```

### Recipe D — HashMap insert/remove

```nova
// Before
fn put(m HashMap[str, int], k str, v int) { m.insert(k, v) }

// After
fn put(mut m HashMap[str, int], k str, v int) { m.insert(k, v) }
```

### Recipe E — Несколько параметров с разными правами

```nova
// Before — всё силлентно mutable
fn merge(dst []int, src []int) { for x in src { dst.push(x) } }

// After — explicit dst is mut, src is read-only
fn merge(mut dst []int, readonly src []int) { for x in src { dst.push(x) } }
```

## Автоматическая миграция

`nova consume-migrate` (Plan 100.7) пока не поддерживает Plan 108.1.
Followup: `[M-108.1-auto-migrate]`.

Manual sweep:

```bash
nova check std/ 2>&1 | grep "E_PARAM_NOT_MUT" | awk -F: '{print $1}' | sort -u
```

Затем добавь `mut` к указанным параметрам.

## Cross-module эффект

Поскольку enforcement происходит при вызове mut-метода (а не при объявлении
функции), миграция не каскадирует: caller'у не нужно ничего менять.

## Acceptance

После миграции `nova check std/` clean — 0 `E_PARAM_NOT_MUT`. Property-test
`param_mut_count_invariant_prop` (plan108_1) — runtime witness.

## Ссылки

- `spec/decisions/02-types.md` D176 (amended Plan 108.1).
- `docs/parameters.md` — user guide.
- `docs/plans/108.1-params-readonly-default.md` — plan status.
- Plan 108 — initial D175 + D176 (Plan 108.1 amends).
