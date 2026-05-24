# Plan 101.1 Ф.0 — Audit baseline (2026-05-24)

## Inventory `fn []T @method` / bare T / tuple-of-bare receiver patterns

Grep'нул `std/` + `nova_tests/` на patterns соответствующие 3 gap'ам Plan 101:

### Pattern A: `fn []T @method` (array-of-bare-typevar receiver)

**Found:** только `std/collections/vec.nv` (7 методов):

```nova
export fn []T @map[U](f fn(T) -> U) -> []U { ... }       // line 45
export fn []T @filter(pred fn(T) -> bool) -> []T { ... } // line 63
export fn []T @fold[Acc](init Acc, f fn(Acc, T) -> Acc) -> Acc { ... } // line 82
export fn []T @any(pred fn(T) -> bool) -> bool { ... }   // line 101
export fn []T @all(pred fn(T) -> bool) -> bool { ... }   // line 118
export fn []T @first() -> Option[T] => ...               // line 133
export fn []T @last() -> Option[T] => ...                // line 144
```

**Current behavior:**
- `nova check std/collections/vec.nv` → PASS (type-checker мягкий, T silently трактуется как named-type).
- `nova test` через vec.nv → CC-FAIL (codegen падает — Nova_T undefined).

### Pattern B: `fn T @method` (bare typevar receiver)

**Found:** нет use sites в std/ или nova_tests/.

Parser silently принимает (probe-фикстура из предыдущей сессии подтвердила).

### Pattern C: `fn (T, U) @method` (tuple-of-bare receivers)

**Found:** нет use sites.

## Probe фикстуры

Создаются для документирования pre-fix baseline + post-fix verification:

- `vec_map_pre.nv` — `[1,2,3].map(|x| x*2)` (current CC-FAIL, post-fix PASS)
- `vec_filter_pre.nv` — `[1,2,3,4].filter(|x| x%2 == 0)` (current CC-FAIL, post-fix PASS)
- `bare_t_no_prefix.nv` — `fn T @method` without prefix (current silent-accept → post-fix loud error)
- `tuple_no_prefix.nv` — `fn (T, U) @method` (current silent → post-fix loud error)

## GATE decision

Scope confirmed:
- vec.nv — единственный stdlib-файл с pattern A. ✅ безопасная migration.
- Нет existing user-code с bare T receiver (B/C) — добавление error-codes не ломает.
- Plan 101.1 scope ~2.5 dev-day — реалистично.

**GATE PASS.** Идём в Ф.1 parser.
