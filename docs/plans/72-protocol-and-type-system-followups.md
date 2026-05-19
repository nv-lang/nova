# Plan 72: Protocol and type-system followups

> **Создан 2026-05-19.** Консолидирует все разрозненные
> proposals из Plan 48, Plan 56, Plan 62 в один читаемый список.
>
> **Цель:** не новая фича с нуля — закрытие **известных ограничений**
> текущего компилятора по протоколам, дженерикам и типовой системе,
> которые накопились как «deferred» или «followup» в нескольких планах.

---

## Контекст: что сейчас сломано

| Симптом | Источник | Приоритет |
|---------|----------|-----------|
| Method call на `void*` existential даёт неверный результат без ошибки | Plan 56 §«Plan 62 followup» | **P0** |
| `fn foo[U, T Iter[U]](mut it T)` — parse error | Plan 48 §«Plan 62 followup Gap A» | P1 |
| `type RuntimeNoneError` — не компилируется без тела | Plan 62.C.bis blocked | P1 |
| 5 методов `Result[T,E]` всегда возвращают `nova_int` (wrong type) | 62.A.bis Ф.4 blocked | P1 |
| `sum_iter(c)` — нельзя вывести `U` из bound `T Iter[U]` без turbofish | Plan 48 §«Plan 62 followup Gap B» | P2 |
| `fn foo[..., E effect {method}](x)` — effect-fn в protocol-method body | Plan 56 Ф.2.7 + 62.E.bis | P2 |
| Record-shadow codegen (`Range` init с полем `start` — конфликт) | 62.D.bis edge case | P3 |
| Full vtable codegen для erased dispatch без mono-context | Plan 56 / Plan 03 future | P3 |

---

## Задачи по приоритетам

### ✅ ЗАКРЫТ P0 — Silent miscompilation: diagnostic вместо неверного ответа

**Что сломано:**

```nova
fn IntCounter mut @next() -> Option[int] => { ... }

let mut x Iter[int] = c
let r = x.next()     // compile OK, runtime WRONG (None вместо Some)

fn foo(x Iter[int]) -> bool => { let mut xx = x; xx.next().is_some() }
foo(c)               // compile OK, runtime returns false вместо true
```

Compile проходит, тест падает с неправильным ответом — нет ни CE, ни
runtime panic. Нарушает принцип «no silent fallback» (Plan 70).

**Откуда это берётся:**

Когда тип параметра/переменной — protocol-type (`Iter[int]`), codegen
использует `void*`-erasure и НЕ знает реальный vtable. Dispatch просто
пропускается или идёт по неправильному указателю.

**Что делаем (до Plan 03 vtable codegen):**

Ввести строгий compile-error **E7201** при попытке вызова метода на
existential (erased) protocol type:

```
error E7201: method call on erased protocol type `Iter[int]`
  note: `next()` cannot be dispatched — existential type is void*,
        no vtable available
  help: use a generic bound instead:
        fn foo[U, T Iter[U]](x T) -> bool => x.next().is_some()
```

Regression marker (только binding coercion — работает):
`nova_tests/plan62/protocol_as_value_probe.nv`

**Scope:** ~1-2 dev-дня. Точки в codegen: `emit_method_call` — проверить
если `recv_type` находится в `CEmitter.protocol_types`, выдать E7201.
Аналог E7001 (Plan 70 «no silent nova_int fallback»).

**Разблокирует:** Cases B + C в `protocol_as_value_probe.nv` станут
корректными CE вместо silent wrong.

---

### ✅ ЗАКРЫТ P1-A — Parser: `mut it T` когда `T` — generic type-var

**Что сломано:**

```nova
fn sum_iter[U, T Iter[U]](mut it T) -> int => { ... }
// error: expected identifier, got `mut`
```

Парсер принимает `mut it Account` (concrete type), но не `mut it T`
(generic type-var — uppercase single letter). Баг в
`parse_fn_param_list`: rule для `mut <name> <type>` видимо отличает
«identifier-like» type от «generic-var».

**Workaround сейчас:** переименовать параметр + `let mut it = it_in` в
теле. Документировано в Plan 48 § followup.

**Scope:** ~1-2h. Правка в `compiler-codegen/src/parser/mod.rs` —
расширить `parse_fn_param_list` чтобы `mut` prefix работал независимо
от того конкретный тип или type-var.

**Разблокирует:** более читаемые generic-bound функции без лишнего rebind.

---

### ✅ ЗАКРЫТ P1-B — Empty-sum syntax для `type Never` и `type RuntimeNoneError`

**Что сломано:**

```nova
type Never           // parse error — пустое тело без вариантов
type RuntimeNoneError  // same
```

Сейчас парсер ожидает хотя бы один вариант после `type Name`. Но
`Never` (bottom type, 0 вариантов) и `RuntimeNoneError` (ошибка без
полезной нагрузки — тоже 1-вариантный empty-payload) нужны как типы без
конструкторов или с пустым телом.

**Что делаем:**

Разрешить `type X` без тела — пустой sum-type (0 вариантов). Codegen:
`typedef int64_t Nova_X;` или просто `void Nova_X` (ABI-compat со
`never`). `type X { }` тоже допустимо (явное пустое тело).

Это разблокирует:
- `type Never` формально в prelude (сейчас только built-in keyword)
- `type RuntimeNoneError` в `std/prelude/errors.nv` (Plan 62.C.bis)
- Любой enum-like «marker» тип без данных

**Scope:** ~2-4h. Парсер + codegen (trivial emit) + spec D-блок
(«empty sum type»). Может войти в независимый Plan.

---

### P1-C — Result[T,E] 5 методов: неверный return type в codegen

**Что сломано:** `unwrap`, `unwrap_or`, `map`, `map_err`,
`unwrap_or_else` всегда возвращают `nova_int` вместо `T`/`E`.

**Корень:** `type_of_method_call_c` hardcode'ит `nova_int` для generic
return-type когда `T` ещё не substituted. Требует Plan 59 Ф.7.5
(sum-type mono extension).

**Что сейчас:** в Plan 62.A.bis Ф.4 — defer до Plan 59. Зависимость
явная.

**Зависимость:** Plan 59 (`59-tuple-monomorphization.md`) уже закрыт как
infrastructure; Ф.7.5 — extension для sum-type methods return-type
inference. Это отдельный 1-2 dev-дня sprint в Plan 59 followup или
standalone sub-plan.

---

### P2-A — TryFrom / TryInto protocols (62.E.bis)

**Что заблокировано:**

```nova
protocol TryFrom[T] {
    fn from(v T) -> Result[Self, RuntimeError]
}
```

Protocol-method с effect return-type (`Result` с runtime error) требует
Plan 56 Ф.2.7 — effects-in-protocol-method-body enforcement в codegen.
Без этого codegen не знает как routing'ить effect через vtable-like path.

**Scope:** связан с P0 (vtable dispatch) — после P0 diagnostic, Ф.2.7
из Plan 56 можно реализовать отдельно (~2-3 dev-дня). Разблокирует
62.E.bis полностью.

---

### ✅ ЗАКРЫТ P2-B — Structural inference `[U, T Iter[U]]` без turbofish

**Что сломано:**

```nova
// Работает (turbofish):
let count = sum_iter[int, IntCounter](c)

// Не работает (должно работать как в Rust):
let count = sum_iter(c)
// error: cannot infer type argument `U` for generic function `sum_iter`
```

Компилятор не идёт по пути: аргумент `c: IntCounter` → bound `T Iter[U]`
→ метод `@next() -> Option[U]` → `U = int`.

**Scope:** medium-deep, ~4-8h. Расширение `resolve_type_args` в
`compiler-codegen/src/codegen/emit_c.rs` (или type-inference pass) —
при неразрешённом type-var `U` проходить bound-chain и пробовать
вывести из concrete arg type.

**Regression marker:** `nova_tests/plan62/protocol_param_generic_bound.nv`
с turbofish — работает. Без турбофиша — failcase, пока deferred.

---

### ✅ ЗАКРЫТ P3-A — Record-shadow codegen edge case

**Что есть:** `Range` struct имеет поле `start` — codegen при
`let r = Range { start: 0, end: 10 }` иногда конфликтует с
W_PRELUDE_SHADOW (D125). Фикс — в `emit_record_lit` проверять что поле
не совпадает с prelude-shadow lookup. ~30 LOC, corner case.

---

### P3-B — Full vtable codegen для erased dispatch

**Что нужно:** когда P0 diagnostic введён (`E7201` на erased method
call), следующий шаг — реально **сделать erased dispatch рабочим**
через vtable. Это Plan 56 Ф.1/Ф.2/Ф.3 architecture полностью — уже
задизайнено в плане.

**Зависимость:** Plan 03 (multi-crate ecosystem) — не нужен для
single-crate. Т.е. vtable codegen реально можно сделать раньше без
Plan 03 — это отдельный sprint (~5-10 dev-дней).

**Что даёт:** Cases B + C из `protocol_as_value_probe.nv` перестают
быть E7201 и становятся рабочими. `fn foo(x Iter[int])` с method-call
наконец работает.

---

## Итоги (2026-05-19)

| Задача | Статус | Примечания |
|--------|--------|------------|
| P0 — E7201 erased method call | ✅ ЗАКРЫТ | `protocol_vars` в `CEmitter`; E7201 в `strict_errors`; unit test в `test_runner.rs` |
| P1-A — `mut it T` parser fix | ✅ ЗАКРЫТ | `parse_param()` принимает leading `mut` для generic type-var |
| P1-B — empty-sum syntax | ✅ ЗАКРЫТ | `type X` без тела; `type X { }` тоже; пустой C typedef |
| P2-B — structural inference without turbofish | ✅ ЗАКРЫТ | Source 2e в `resolve_mono_type_args`: смотрим `next()→Option[U]` return type |
| P3-A — record-shadow Range | ✅ ЗАКРЫТ | `emit_record_lit` проверяет `record_schemas` до `sum_schema_registry` |
| P1-C — Result[T,E] 5 методов | ⏳ DEFERRED | Зависит Plan 59 Ф.7.5 |
| P2-A — TryFrom/TryInto | ⏳ DEFERRED | Зависит Plan 56 Ф.2.7 |
| P3-B — full vtable codegen | ⏳ DEFERRED | Plan 56 Ф.1–Ф.3, отдельный sprint |

Тесты: `nova_tests/plan72/` — 5 fixtures, все 11 test cases ✅.

---

## Порядок выполнения

```
P0 (silent miscompilation → E7201 diagnostic)
  └─ независимый, начинать первым, ~1-2 дня

P1-A (mut it T parser fix) — ~2h, независимый
P1-B (empty sum syntax) — ~3-4h, независимый  
P1-C (Result 5 методов) — зависит Plan 59 Ф.7.5

P2-A (TryFrom/TryInto) — зависит Plan 56 Ф.2.7
P2-B (structural inference) — независимый, ~4-8h

P3-A (record-shadow edge) — ~30 LOC, любое время
P3-B (full vtable codegen) — после P0, ~5-10 дней
```

---

## Ссылки на источники

- **Plan 48** §«Plan 62 followup — protocol-as-parameter mono ergonomics»
  → Gap A (P1-A) + Gap B (P2-B)
- **Plan 56** §«Plan 62 followup — silent miscompilation на
  protocol-as-value» → P0 + P3-B
- **Plan 56** §«Ф.2.7 effects-in-protocol-methods» → P2-A (TryFrom)
- **Plan 62.A.bis** §«Ф.4 deferred» → P1-C (Result методы)
- **Plan 62.C.bis** (blocked) → P1-B (empty sum syntax)
- **Plan 62.E.bis** (blocked) → P2-A
- **Plan 62.D.bis** edge case note → P3-A

Regression tests (не трогать — negative smoke):
- `nova_tests/plan62/protocol_as_value_probe.nv` (P0)
- `nova_tests/plan62/protocol_param_generic_bound.nv` (P2-B)
- `nova_tests/plan62/protocol_param_erasure.nv` (P0)
