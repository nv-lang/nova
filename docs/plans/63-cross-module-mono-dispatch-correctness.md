# Plan 63: cross-module + mono dispatch correctness

> **Status:** proposed (2026-05-17). Discovered во время Sprint F.36 regression testing
> (`nova_tests/plan11_followup/`). Family of related compiler issues — все из-за
> того что compiler tracks N (signature/typearg/overload) во-внутри-module, но
> теряет N при пересечении module boundary.

---

## Bugs covered

### Bug A: Return type inference cross-method для new external methods

**Repro:** [`nova_tests/plan11_followup/f11_stringbuilder_new_methods_positive.nv`](../../nova_tests/plan11_followup/f11_stringbuilder_new_methods_positive.nv)

```nova
let mut sb = StringBuilder.new()
sb.append("abc")
let snap = sb.peek()      // ← snap declared в C как nova_int, не nova_str
```

Generated C:
```c
nova_int snap = Nova_StringBuilder_method_peek(sb);   // wrong type!
```

Workaround в test: `let snap str = sb.peek()` (explicit annotation).

**Root cause:** call-site type inference для method returning `str`, **registered
через external_registry** (нашими auto-gen stubs), не возвращает корректный
return type. Fallback на `nova_int`. Symbol resolution OK (правильный
`Nova_StringBuilder_method_peek` called), но return type inference broken.

### Bug B: Variadic signature lost cross-module via alias-import

**Repro:** [`nova_tests/plan11_followup/f13_path_join_normalize_positive.nv`](../../nova_tests/plan11_followup/f13_path_join_normalize_positive.nv)

```nova
import std.path.path as p
p.Path.join("a", "b", "c")    // CODEGEN-FAIL: assert не variadic
```

Через `import std.path.path.{Path}` (direct import name) — works.

**Root cause:** при `import X as p`, compiler регистрирует alias `p` но
variadic flag fn'а **теряется** при path resolution `p.Path.join(...)`. Compiler
trying spread lowering но parent context (assert(...)) думает spread не нужен.

### Bug C: Generic type anonymous record literal — Nova_T_p placeholder leak

**Repro:** Generic factory pattern в внешнем модуле:
```nova
export type Box[T] { v T }
export fn Box[T].of(value T) -> Box[T] => { v: value }   // anonymous record
```

Generated C для caller `Box[int].of(42)`:
```c
Nova_Box____Nova_T_p* m = Nova_Box____nova_int_static_of(42);
//        ^^^^^^^^^^ unresolved T placeholder leaks
```

**Root cause:** anonymous record literal в return position generic static fn'а
не triggered mono instance enrollment для T → emit'ит type placeholder Nova_T_p
вместо substituted type. Mono path для record literal не handled.

### Bug D: D73 cross-module overload dispatch misroute

**Repro:** [`nova_tests/plan11_followup/f10_hashmap_self_restored_positive.nv`](../../nova_tests/plan11_followup/f10_hashmap_self_restored_positive.nv)
(workaround: revert на explicit type)

```nova
HashMap[str, int].from([("a", 1)])
```

При `-> Self` return type в HashMap.from declaration:
```c
Nova_str_static_from(...)         // ← wrong! str.from selected
// или Nova_StringBuilder_static_from(...) в другом запуске
```

При `-> HashMap[K, V]` (explicit) — works.

**Root cause:** при `parts = ["HashMap", "from"]` lookup в `method_overloads`
не finds key, fallback к single-key `method_receivers["from"]` который **last-wins**
(picks first registered .from from str / StringBuilder / WriteBuffer).
Cross-module overload resolution не учитывает typeargs `[str, int]` в parts[0].

## Что НЕ делаем

- **Drop variadic / Drop generics / Drop overloading** — это core spec features.
- **Per-method registration в external_registry** для каждого нового метода — too verbose.

## Решение (4 sub-fixes)

### Fix A: Return type carrier через external_registry

`ExternalRegistry` уже хранит `return_c_type`. Compile site lookup при method
call'е должен use registered return_c_type вместо fallback на `nova_int`.

Файл: `compiler-codegen/src/codegen/emit_c.rs` метод call inference path.
Найти место где member-call inference выбирает nova_int default; instead
look up `external_registry.by_key[(recv, method)].return_c_type`.

Scope: ~50-100 LOC + 5-10 tests.

### Fix B: Variadic info via cross-module signature carrier

При `import X as p`, alias resolution должен пройти `variadic_last` flag
через. Сейчас `user_fn_variadic` set локально для каждого module, alias
import не updates external module's user_fn_variadic.

Fix: при resolving `p.Path.join` lookup variadic info через imported module's
sig table, не локальный.

Scope: ~100-150 LOC + 5-10 tests.

### Fix C: Mono enrollment для anonymous record literal в generic return

При emit anonymous record literal в return position generic fn'а с
known `current_fn_return_ty` ending с mono'd template name (e.g.
`Nova_Box____nova_int*`):
- Trigger mono instance enrollment для template's struct definition.
- Use substituted struct name (`Nova_Box____nova_int`) вместо placeholder
  `Nova_Box____Nova_T_p`.

Scope: ~80-150 LOC + 5-10 tests.

### Fix D: Typeargs-aware overload dispatch

При static method call `Type[T1, T2].method(args)`:
- Build key `(Type, T1, T2, method)` для overload lookup.
- Не fallback на single-key `method_receivers[method]` если parts[0]
  matches **any** registered receiver type.

Файл: `emit_c.rs` line ~12326-12352 (Path branch для Type.method).

Scope: ~150-250 LOC + 10-15 tests.

## Total estimate

3-5 dev-days для всех 4 sub-fixes. По одному за commit.

## Acceptance criteria

- ✅ `let snap = sb.peek()` infers snap as `str` (no explicit annotation).
- ✅ `import X as p; p.VariadicFn(a, b, c)` lowers корректно.
- ✅ `Box[int].of(42)` (generic factory) generates correct mono'd C
  без Nova_T_p leakage.
- ✅ `HashMap[str, int].from(pairs)` dispatches к HashMap.from, не str.from.
- ✅ Existing 568+ regression tests не регрессят.
- ✅ Все 15 tests в `nova_tests/plan11_followup/` PASS (без workaround'ов).

## Связь с другими планами

- **Plan 11 Follow-up** (закрыт): нашёл первые 2 bugs (Self + assert-in-expr).
  Plan 63 — продолжение того же regression-testing pass'а.
- **Plan 35** (cross-file resolve): фундамент cross-module resolution.
- **Plan 48 / 55 / 56** (mono / closures / vtable): mono enrollment infrastructure
  где Fix C должен интегрироваться.
- **Plan 60** (.len() uniformity): косвенно — тот же сорт «cross-module API
  resolution» проблемы.
- **Plan 61** (typed errors): отдельная архитектурная проблема, не пересекается.

## Ссылки

- `nova_tests/plan11_followup/` — full regression suite (15 tests, 13 PASS, 2 deferred).
- Sprint F.36 closure: `docs/plans/45-nova-doc.md` (Ф.36 status table).
- `compiler-codegen/src/codegen/emit_c.rs` — dispatch sites.
- `compiler-codegen/src/codegen/external_registry.rs` — type info storage.
