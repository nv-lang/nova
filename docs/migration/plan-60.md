# Plan 60 migration — `.len` → `.len()` (size-accessor uniformity)

> Если вы видели **`E_SIZE_ACCESSOR_FIELD`** в `nova check` / `nova build`,
> этот документ объясняет что изменилось и как мигрировать.

## TL;DR

Field-style доступ к размеру коллекции запрещён:

```nova
ro n = arr.len      // ✗ старое — error
ro n = arr.len()    // ✓ новое — добавьте ()
```

Для `.cap` — rename:
```nova
ro c = arr.cap         // ✗ старое
ro c = arr.capacity()  // ✓ новое — rename + ()
```

## Что изменилось

[D-block D117](../../spec/decisions/03-syntax.md#d117) фиксирует:

- Любой size-accessor (`len`, `capacity`, `byte_len`, `is_empty`) на
  любом типе — **только** method-call с круглыми скобками.
- `.cap` отвергнут — replaced на `.capacity()` (Rust/C++/Swift parity).

До Plan 60:
- `[]T` и `str` использовали field-style (без скобок).
- HashMap, Set, и другие user types — method-style (со скобками).
- Это была Java-style inconsistency: `arr.length` field vs `list.size()` method.

После Plan 60:
- Везде method-style. Rust паритет.

## Diagnostic

```
error[E_SIZE_ACCESSOR_FIELD]: size-like accessor `len` is method-only
                              (Plan 60 / D117)
  --> file.nv:42:23
   |
42 |     println("${vec.len}")
   |                    ^^^ help: append `()` — use `.len()` method call
```

## Auto-migration

В bootstrap workspace nova-lang есть готовый tool:

```bash
nova-cli/target/release/migrate_plan60 --apply --paths std nova_tests examples
```

Опции:
- `--dry-run` (default) — показывает что изменится, не пишет.
- `--apply` — реально записывает.
- `--md` — включить .md файлы (extract `\`\`\`nova` blocks).
- `--paths DIR1 DIR2 ...` — список директорий.

Tool использует Nova lexer, поэтому:
- Сохраняет formatting / comments / whitespace.
- Корректно distinguishes method-call от method-value.
- Не trogает локальные переменные с именем `len` / `cap` / `is_empty`.

## Manual fix-up

Tool **conservative** в одном случае: `let x = arr.len` (RHS of let)
он считает possible method-value и **не** rewrite'ит. После atomic
switch такой код становится error → fix вручную:

- Хотите int? `let x = arr.len()` (добавьте `()`).
- Хотите bound method value? `let x = arr.@len` (с `@`-prefix).

## Edge case: default parameter values

```nova
fn slice_len(xs []int, from int = 0, to int = xs.len) -> int =>
//                                            ^^^^^^^ error
```

Default value тоже под D117 — нужны скобки:

```nova
fn slice_len(xs []int, from int = 0, to int = xs.len()) -> int =>
```

## Внутренние C-поля

`arr->len` и `arr->cap` в C-runtime **остались** — это implementation
detail, не user-visible. `arr.len()` lowers в zero-cost `(arr->len)`;
никакого function-call overhead.

## Зачем

См. [D117 в spec](../../spec/decisions/03-syntax.md#d117) — полное
обоснование. Краткий ответ:
1. **Predictable cost** — скобки сигнализируют возможную дороговизну
   (`s.len()` для UTF-8 строки — O(n)).
2. **Consistency** — между built-in и user-defined collections.
3. **AI-friendly** — LLM не должен запоминать per-type form (Java
   pattern → confusion).

## Связь с другими планами

- [Plan 11](../plans/11-method-values-and-overload.md) — method-values
  (`x.@len`).
- [Plan 37](../plans/37-typecheck-semantic-parity.md) — рефайн
  diagnostic (arg-position vs non-arg) — future work.
- [Plan 45](../plans/45-nova-doc.md) — `nova doc` обновляет stdlib
  doc-comments на consistent form.
