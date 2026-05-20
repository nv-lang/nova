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

### ✅ ЗАКРЫТ P1-C — Result[T,E] 5 методов: неверный return type в codegen

**Что было сломано:** `unwrap`, `unwrap_or`, `map`, `map_err`,
`unwrap_or_else` всегда возвращали `nova_int` вместо `T`/`E`.

**Фикс (Plan 72 P1-C, 2026-05-19):** `CEmitter.result_type_params:
HashMap<String, (String, String)>` — при `let v: Result[T, E] = ...`
запоминаем `(T_c, E_c)`. `infer_expr_c_type` и emit-paths для
`unwrap`/`unwrap_or`/`unwrap_or_else` используют правильный тип через
`extract_result_type_params` + `cast_from_nova_int`. Без Plan 59.

**Тесты:** `nova_tests/plan72/p1c_result_type_params_pos.nv` — 6 cases ✅.
`nova_tests/plan72/p1c_result_chain_pos.nv` — 6 cases ✅.
`nova_tests/plan72/p1c_result_inline_chain_pos.nv` — 4 cases ✅.

**Известное ограничение (P1-C followup):** `result_type_params` работает
только когда receiver — **именованная переменная** (`let r = ...; r.unwrap_or(...)`).
Inline-цепочки (`parse_bool("x").unwrap_or(false)`) попадают в ветку `else`
в `infer_expr_c_type` (emit_c.rs:20541) и получают fallback `(nova_int, nova_str)`.

Тесты с `bool` и `int` проходят случайно — в C `bool` совместим с `int`
по размеру, каст не ломает результат. Для типов-указателей (`Nova_Foo*`)
или `f64` inline-цепочка даст **неверный результат без ошибки компиляции**.

Пример опасного кода:
```nova
type Celsius { deg int }
fn parse_celsius(s str) -> Result[Celsius, str] => ...
let c = parse_celsius("100").unwrap_or(Celsius { deg: 0 })  // WRONG: каст nova_int → Nova_Celsius*
```

Фикс: расширить `infer_expr_c_type` для `ExprKind::MethodCall` — рекурсивно
выводить тип `T` из типа receiver'а когда receiver — тоже `Result`-returning call.

---

### ✅ ЗАКРЫТ P2-A — TryFrom / TryInto protocols (62.E.bis)

**Оригинальный blocker:** `Fail[E]` effect row в protocol-методе
запрещён Plan 56 Ф.2.7. Без vtable effect не пробрасывается.

**Разблокировка (migration path b, 2026-05-19):** `Result[Self, E]`
вместо `Fail[E]` effect row. Caller'ы матчат/unwrap'ят Result вместо
`?` propagation. Declarations в `std/prelude/protocols.nv`.

**Тесты:** `nova_tests/plan72/p2a_try_from_into_pos.nv` — 4 cases ✅.

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

### ✅ ЗАКРЫТ P3-B — Full vtable codegen для erased dispatch

**Что было нужно:** после P0 E7201 diagnostic — реализовать vtable
dispatch, чтобы Cases B + C из `protocol_as_value_probe.nv` работали.

**Реализация (Plan 72 P3-B, 2026-05-19):**
- `CEmitter` + 4 новых поля: `protocol_method_registry`, `protocol_var_vtable`,
  `emitted_vtable_types`, `emitted_vtable_instances`
- `emit_protocol_vtable_companion` (~120 LOC): для `let mut x Iter[int] = c`
  генерирует `NovaVtable_Iter_nova_int` struct typedef, thunk functions,
  vtable instance + companion variable `__vt_x`
- E7201 block расширен: проверяет `protocol_var_vtable` → dispatch через
  `__vt_x->method(x, args)` если vtable есть
- P0 negative fixture обновлён → positive test (P3-B supersedes E7201 для
  этого случая). Rust unit test переименован в `p0_erased_now_dispatches_via_vtable`.

**Тесты:** `nova_tests/plan72/p3b_vtable_dispatch_pos.nv` — 3 cases ✅.
`nova_tests/plan72/p0_erased_method_call_neg.nv` — теперь positive test ✅.

---

### ✅ ЗАКРЫТ P3-B followup — Function-return fat pointer

**Что было сломано:** `fn get_iter() -> Iter[int]` — функция с protocol
return type. C-функция возвращала `void*`, и `let x = get_iter()` (без
аннотации) получал `void* x = NULL`-dispatch вместо fat pointer — silent
wrong result. Работал только явный `let x Iter[int] = make_counter(3)`.

**Реализация (2026-05-20):**
- `CEmitter.current_fn_returns_protocol: Option<(String, Vec<String>)>`.
- `emit_protocol_box_typedef` вынесен из `emit_protocol_vtable_companion`:
  vtable struct + `NovaBox_*` typedef эмитятся в `generic_type_defs_buf`
  (перед fn forward decls *и* bodies — иначе forward decl ссылается на
  ещё не объявленный тип).
- `emit_fn_forward_decl` + `emit_fn`: protocol return type → C return type
  становится `NovaBox_*`; `fn_ret_<name>` регистрируется как box-тип, так
  что `let x = get_iter()` биндится как `NovaBox_*`.
- `wrap_protocol_return`: trailing-expr / explicit `return` оборачиваются
  в `(NovaBox_*){ .data = (void*)val, .vtable = &_vt_... }`.
- Method dispatch: проверка `NovaBox_*` вынесена из-под `protocol_vars`
  gate — fn-return binding намеренно НЕ в `protocol_vars`, но dispatch
  через `.vtable` field всё равно работает.

**Тесты:** `nova_tests/plan72/p3b_fatptr_return_pos.nv` — 6 cases ✅
(3 явная аннотация `let x Iter[int] = c` + 3 protocol return type
`let x = get_iter()`). Проверено через `nova test-build` (реальный
C codegen → clang → нативный бинарник), не только интерпретатор.

---

## Итоги (2026-05-19)

| Задача | Статус | Примечания |
|--------|--------|------------|
| P0 — E7201 erased method call | ✅ ЗАКРЫТ | `protocol_vars` в `CEmitter`; E7201 в `strict_errors`; unit test в `test_runner.rs` |
| P1-A — `mut it T` parser fix | ✅ ЗАКРЫТ | `parse_param()` принимает leading `mut` для generic type-var |
| P1-B — empty-sum syntax | ✅ ЗАКРЫТ | `type X` без тела; `type X { }` тоже; пустой C typedef |
| P2-B — structural inference without turbofish | ✅ ЗАКРЫТ | Source 2e в `resolve_mono_type_args`: смотрим `next()→Option[U]` return type |
| P3-A — record-shadow Range | ✅ ЗАКРЫТ | `emit_record_lit` проверяет `record_schemas` до `sum_schema_registry` |
| P1-C — Result[T,E] 5 методов | ✅ ЗАКРЫТ | `result_type_params` в `CEmitter`; `cast_from_nova_int`; без Plan 59 |
| P2-A — TryFrom/TryInto | ✅ ЗАКРЫТ | Migration path (b): `Result[Self, E]` вместо `Fail[E]`; в `std/prelude/protocols.nv` |
| P3-B — full vtable codegen | ✅ ЗАКРЫТ | `NovaVtable_*` struct + thunks + companion `__vt_x`; P0 fixture → positive |
| P3-B followup — function-return fat pointer | ✅ ЗАКРЫТ | `fn () -> Iter[int]` возвращает `NovaBox_*`; `wrap_protocol_return` + `emit_protocol_box_typedef` |
| P3-B param — protocol-typed параметры | ✅ ЗАКРЫТ | `fn foo(x Iter[int])` → `NovaBox_*` параметр; `emit_call` боксит аргументы; `fn_protocol_params` |
| p1b/p2a/p2b — C-backend codegen fixes | ✅ ЗАКРЫТ | typedef redefinition / Result type-params для `let r = call` / `NovaOpt_X` без `*` — выявлены через `test-build` |

Тесты: `nova_tests/plan72/` — все фикстуры проходят через `test-build`
(реальный C-codegen). p3b: `p3b_vtable_dispatch_pos.nv` (3) +
`p3b_fatptr_return_pos.nv` (6) ✅.
Rust unit test: `p0_erased_now_dispatches_via_vtable` в `test_runner.rs` ✅.

> **Примечание (2026-05-20, обновлено):** изначально статусы «✅ ЗАКРЫТ»
> снимались через интерпретатор (`nova-codegen test-interp`, ранее
> `nova-codegen test`). Прогон через реальный C-codegen pipeline
> (`nova test` / `test-build` → `test_runner::run_one` → CEmitter →
> clang → нативный бинарник) вскрыл 4 C-бэкенд бага, замаскированных
> интерпретатором: `p1b` (typedef redefinition empty-sum), `p2a` (Result
> type-params для `let r = call`), `p2b` (structural inference `NovaOpt_X`
> без `*`), `p3b_vtable_dispatch` (protocol-as-**параметр**, E7201).
> **Все четыре исправлены** (commits `80edd5110f9`, `1aad24b954b`).
> Plan 72 верифицирован через C-бэкенд: full `nova test nova_tests` =
> **886 PASS / 0 FAIL / 56 SKIP** (z3 contracts), измерено 2026-05-21
> после фикса всех 4 багов + P1-C + закрытия двух P3-B honest-defers
> ([M-protocol-param-free-fn-only], [M-protocol-return-wrap-relies-on-infer]).

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
