<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 132 — Убрать bound method value `obj.@method`; разрешить field/method одного имени

> **Создан:** 2026-06-09.  **Статус:** ✅ ЗАКРЫТ 2026-06-09.
> **Эстимат:** ~1 dev-day.  **Model:** Sonnet 4.6.

---

## Что

Убрать синтаксис **bound method value** (`obj.@method` без скобок и
`obj.@method(args)` как call-via-bound).

Оставить:
- `Type.@method` — **unbound** fn-pointer (берёт self явно как первый аргумент)
- `obj.method()` — обычный вызов метода (без `@`)

Как следствие — разрешить полю и методу иметь **одно и то же имя** на одном типе,
поскольку неоднозначность устраняется без bound-форм:

| Форма | Значение |
|-------|----------|
| `@name` (в теле метода, без `()`) | поле `name` |
| `@name()` (в теле метода) | вызов метода `@name` |
| `obj.name` | поле `name` (public) |
| `obj.name()` | вызов метода `name` |
| `Type.@name` | unbound fn-pointer |

---

## Почему

`obj.@method` (bound) несёт closure-overhead (struct `{fn_ptr, self}`) и создаёт
давление на придумывание новых форм (`@@len`, `@.@len`) при коллизии поля и метода
с одним именем. Замена — лямбда `|| obj.method()` — явная и не требует специальной
семантики.

Unbound `Type.@method` не создаёт аналогичных проблем: это просто fn-pointer на
декларированную функцию, никакого self-binding.

### Практический результат

`Vec[T]` (Plan 131) может иметь `priv mut len int` и `@len() -> int` на одном
типе — это теперь легальный паттерн. `@len` в теле = поле, `@len()` = метод.

---

## Фазы

### Ф.0 — Audit (~1h)

- **Ф.0.1** Найти все `obj.@method` и `obj.@method(args)` вхождения:
  `grep -rn '\.\s*@[a-z]' --include="*.nv"` и в spec/docs `--include="*.md"`
- **Ф.0.2** Классифицировать: bound-value (без args) vs call-via-bound (с args)
- **Ф.0.3** Перечислить файлы для миграции

Ожидаемые файлы:
- `nova_tests/syntax/method_values.nv` — основные тесты bound
- `examples/ffi/sqlite_mini.nv` — `db.@exec(...)`, `db.@close()`
- `spec/decisions/03-syntax.md` — примеры bound в §method-values
- `spec/decisions/10-overloading.md` — `t.@m as fn(...)` пример

### Ф.1 — Parser/Checker: убрать bound, оставить unbound (~2h)

- **Ф.1.1** В parser: `expr.@ident` без последующего `(` → **E_BOUND_METHOD_REMOVED**
  ```
  error[E_BOUND_METHOD_REMOVED]: bound method value syntax `obj.@method` is removed.
    Use a lambda: `|| obj.method()`, or unbound `Type.@method` for fn-pointer.
  ```
- **Ф.1.2** `expr.@ident(args)` → **E_BOUND_METHOD_REMOVED** (то же сообщение, с hint
  `use obj.method(args) for a direct call`)
- **Ф.1.3** `Type.@method` (где Type — имя типа, не instance-expr) → остаётся,
  резолвится как unbound fn-pointer.
- **Ф.1.4** Checker: снять ошибку на field/method с одним именем (если она была).
  Убедиться что `@name` (без `()`) резолвится только как поле, не как метод.
- **Ф.1.5** NEG fixture: `c.@get` → E_BOUND_METHOD_REMOVED
- **Ф.1.6** NEG fixture: `c.@add(5)` → E_BOUND_METHOD_REMOVED (hint: use `c.add(5)`)
- **Ф.1.7** POS fixture: `Type.@method` (unbound) → компилируется, тип `fn(Type, ...) -> R`

**Commit:** `feat(plan132 Ф.1): remove bound method value obj.@method; E_BOUND_METHOD_REMOVED`

### Ф.2 — Migration sweep (~2h)

**`nova_tests/syntax/method_values.nv`:**
- Тесты на bound method value → переписать через лямбды или убрать
- Тест на unbound (`Counter.@add`) → оставить
- Тест `Ф.5: as fn type annotation` с `c.@add as fn(...)` → убрать (bound)

**`examples/ffi/sqlite_mini.nv`:**
- `db.@exec("...")` → `db.exec("...")`
- `db.@close()` → `db.close()`

**Spec примеры в `spec/decisions/03-syntax.md`:**
- `ro f = acc.@get` → удалить или заменить на `ro f = || acc.get()`
- `ro total = nums.fold(0, acc.@add)` → `nums.fold(0, |n| acc.add(n))`
- `buf.@write as fn(str) -> ()` → удалить (disambiguation через `as` теперь не нужна)

**`spec/decisions/10-overloading.md`:**
- `ro f = t.@m as fn(str) -> int` → пример убрать или заменить

**Commit:** `refactor(plan132 Ф.2): migrate obj.@method → direct call / lambda (spec + tests + examples)`

### Ф.3 — Разрешить field/method одного имени (~1h)

- **Ф.3.1** Если в checker есть проверка на коллизию поле/метод — убрать.
  (По текущим данным checker такой проверки НЕТ — nova check vec_owned.nv PASS).
- **Ф.3.2** Добавить POS test: тип с полем `len int` и методом `@len() -> int` — компилируется.
- **Ф.3.3** Добавить POS test: в теле метода `@len` = поле, `@len()` = метод — оба работают корректно.

**Commit:** `test(plan132 Ф.3): field/method same name — POS fixtures (unambiguous via @name vs @name())`

### Ф.4 — Spec + docs (~1h)

- **Ф.4.1** `spec/decisions/03-syntax.md` §«Bound vs unbound»: переписать.
  Убрать `acc.@get` как bound. Оставить только `Type.@method` как unbound.
  Добавить: «bound method value удалён; используй лямбду `|| obj.method()`».
- **Ф.4.2** D-block amend (тот что описывал bound method value в Plan 11).
- **Ф.4.3** `spec/decisions/03-syntax.md` §`@` semantics: добавить правило
  «`@name` без `()` в теле метода = всегда поле; `@name()` = вызов метода».
- **Ф.4.4** Добавить в spec: field и метод с одним именем — легально.
  Пример с Vec-like паттерном.
- **Ф.4.5** `docs/simplifications.md` + project-creation.txt + nova-private discussion-log.

**Commit:** `docs(plan132 Ф.4): spec — remove bound method value, document @name=field rule`

---

## Acceptance criteria

- **A-132.a** — `obj.@method` (без args) → E_BOUND_METHOD_REMOVED
- **A-132.b** — `obj.@method(args)` → E_BOUND_METHOD_REMOVED с hint `use obj.method(args)`
- **A-132.c** — `Type.@method` (unbound) → компилируется, возвращает fn-pointer
- **A-132.d** — поле `len` и метод `@len()` на одном типе → компилируется без ошибок
- **A-132.e** — `@len` в теле = поле; `@len()` = метод — оба корректны
- **A-132.f** — `nova_tests/syntax/method_values.nv` мигрирован, PASS
- **A-132.g** — 0 regressions в full `nova test`

---

## Что остаётся (не меняется)

- `Type.@method` — unbound: `Counter.@add`, `int.@neg`, `str.@len` и т.д.
- `obj.method()` — обычный вызов
- `obj.method` (без `@`) — поле (public) или E_SIZE_ACCESSOR_FIELD для size-методов (D117)

## Followups

- `[M-132-unbound-type-inference]` — `let f = Counter.@add` тип выводится как
  `fn(Counter, int) -> int`; проверить что inference работает корректно.
- `[M-132-as-disambiguation-alternative]` — если overload disambiguation через
  `as fn(...)` нужна для unbound, документировать синтаксис отдельно.

---

## Plan 132.1 — Codegen bug: `@name()` self-call when field/method share name

> **Создан:** 2026-06-09. **Статус:** ✅ ЗАКРЫТ 2026-06-09.
> **Эстимат:** ~0.5 dev-day. **Model:** Sonnet 4.6.

### Проблема

При вызове `@len()` внутри метода того же типа, где одновременно есть **поле** `len` и **метод** `@len()`, компилятор генерирует неверный C-символ `nova_fn__at_len` вместо правильного `Nova_Counter_method_len`.

Конкретный кейс из теста `pos_at_name_disambiguation`:
```nova
type Counter { priv mut n int, priv mut len int }
fn Counter @len() -> int => @len        // @len без () = поле ✓
fn Counter @call_len() -> int => @len() // @len() = вызов метода ← ПАДАЕТ
```

Линкер-ошибка: `undefined symbol: nova_fn__at_len`.

### Диагностика

Путь исполнения в `emit_c.rs` при emit `@len()` (= `Method { obj: SelfAccess, name: "len" }`):

1. `infer_expr_c_type(SelfAccess)` → `Nova_Counter*` (из `var_types["nova_self"]`)
2. Ветка `obj_ty.starts_with("Nova_") && ends_with('*')` → пробует `external_registry.lookup("Counter", "len")` → пусто (пользовательский метод)
3. Пробует `method_overloads.get(("Counter", "len"))` — **здесь проблема**:
   - Метод `fn Counter @len()` хранится в AST как `FnDecl { name: "len", receiver: Counter }` (без `@`)
   - Но `method_overloads` для конкретных (non-generic) non-external методов регистрируется в `emit_fn_decl` с ключом `(recv.type_name, f.name)`
   - Конкретно: при наличии поля `len` в типе, при регистрации метода происходит **коллизия** — поле занимает слот lookup'а раньше метода, либо метод регистрируется под иным ключом
4. Все ветки не нашли → финальный fallback: `free_fn_c_name("len")` → `nova_fn__at_len` (потому что внутреннее имя метода с @ → `_at_len`)

### Три пути решения

#### Вариант A — Patch emit_fn_decl: приоритет instance-метода при field-name коллизии
Когда `method_overloads.entry(("Counter", "len"))` уже содержит field-driven запись,
**перезаписать** её записью метода. Либо добавить флаг `is_field_collision = true` чтобы
call-site мог различить.

- **+** Минимальный патч (~10 строк)
- **−** Хрупко: зависит от порядка регистрации; может поломать обратный случай (обращение к полю через method_overloads)
- **−** Не устраняет корень — неправильный fallback path

#### Вариант B — Специальный путь для `Method { obj: SelfAccess, name }` (рекомендуется)
В `emit_call` добавить ветку **до** всех остальных, специфически для `obj = SelfAccess`:

```rust
if matches!(obj.kind, ExprKind::SelfAccess) {
    // current_receiver_type содержит тип self внутри метода
    if let Some(recv_type) = &self.current_receiver_type.borrow().clone() {
        let key = (recv_type.clone(), method.trim_start_matches('@').to_string());
        if let Some(sigs) = self.method_overloads.get(&key) {
            // найти instance-sig и вызвать Nova_{recv}_method_{name}(nova_self, args)
        }
    }
}
```

- **+** Явный, понятный путь для self-call
- **+** Не нарушает существующие пути
- **+** `current_receiver_type` уже хранит правильный тип self в контексте метода
- **−** Нужно убедиться что `current_receiver_type` установлен корректно в emit_fn_body

#### Вариант C — Patch infer_expr_c_type для SelfAccess + method_overloads lookup
Когда `infer_expr_c_type(SelfAccess)` возвращает `Nova_Counter*`, убедиться что дальнейший lookup `method_overloads` игнорирует field-записи и находит instance-метод. Добавить в lookup фильтр `sig.is_instance && !sig.is_field`.

- **+** Локальный патч в lookup-логике
- **−** Требует пометить поля в method_overloads (сейчас поля там не регистрируются напрямую, так что надо разобраться точнее)

### Декомпозиция

#### Ф.0 — Воспроизвести и точно локализовать (~30min)
- **Ф.0.1** Написать минимальный failing fixture:
  ```nova
  type T { priv x int }
  fn T @x() -> int => @x   // метод x() + поле x
  fn T @call_x() -> int => @x()  // вызов метода из другого метода — CC-FAIL
  ```
- **Ф.0.2** Добавить debug-print в emit_c: что возвращает `infer_expr_c_type(SelfAccess)` и что находит `method_overloads.get(...)` в данном контексте
- **Ф.0.3** Убедиться в диагнозе: выяснить точно, какой ключ используется при регистрации метода в `method_overloads` и есть ли там запись

#### Ф.1 — Реализовать Вариант B (~1h)
- **Ф.1.1** В `emit_call` (ветка `ExprKind::Member { obj, method, args }`), добавить **первую** проверку:
  ```
  if obj == SelfAccess → lookup method в method_overloads по (current_receiver_type, method_stripped) → emit Nova_{recv}_method_{name}(nova_self, args)
  ```
- **Ф.1.2** Убедиться что `current_receiver_type` содержит правильный тип в теле каждого метода (проверить `emit_fn_body` / `emit_fn_decl`)
- **Ф.1.3** Если `method_overloads` не имеет записи (не-generic non-external method) → fallback на `Nova_{recv_c}_method_{name}` через `var_types["nova_self"]` тип

#### Ф.2 — Тесты (~30min)
- **Ф.2.1** NEG fixture не нужен (это POS-сценарий)
- **Ф.2.2** Восстановить полный `pos_at_name_disambiguation.nv` с `@call_len()` и `@double_len()` — теперь должен PASS
- **Ф.2.3** Убедиться что `pos_field_method_same_name` всё ещё PASS
- **Ф.2.4** Regression: `nova test plan132/` — все 5 PASS

#### Ф.3 — Обновить план и acceptance criteria (~15min)
- Обновить A-132.e: добавить что `@name()` из другого метода того же типа корректно
- Закрыть этот sub-план

### Acceptance criteria для 132.1

- **A-132.1.a** — `@name()` self-call когда поле и метод имеют одно имя → корректный C-символ `Nova_T_method_name`
- **A-132.1.b** — `pos_at_name_disambiguation.nv` с `@call_len()` и `@double_len()` → PASS
- **A-132.1.c** — 0 regressions в `nova test plan132/`

### Итог 132.1
- Ф.0: Root cause confirmed — Method{obj:SelfAccess} fallthrough to free_fn_c_name
- Ф.1: Variant B implemented — SelfAccess early path in emit_call (~15 LOC)
- Ф.2: pos_at_name_disambiguation fully restored; all plan132 fixtures PASS
- Ф.3: spec/docs/plan updated

---

## Итог

Plan 132 закрыт полностью (2026-06-09).

- **Ф.1:** E_BOUND_METHOD_REMOVED добавлен в parser/checker. `Type.@method` (unbound) оставлен. Коллизия field/method снята на уровне checker.
- **Ф.2:** Миграция: spec, nova_tests/syntax/method_values.nv, examples/ffi/sqlite_mini.nv — перемигрированы на лямбды/прямые вызовы.
- **Ф.3:** POS fixtures: `pos_field_method_same_name.nv` + `pos_at_name_disambiguation.nv` — оба PASS.
- **Ф.4:** Spec обновлён: D35 «Bound vs unbound» переписан, C-runtime раздел исправлен, D117 диагностика скорректирована. docs/simplifications.md, docs/project-creation.txt, nova-private/discussion-log.md, этот файл.

Acceptance criteria:
- A-132.a ✅ `obj.@method` (без args) → E_BOUND_METHOD_REMOVED
- A-132.b ✅ `obj.@method(args)` → E_BOUND_METHOD_REMOVED с hint
- A-132.c ✅ `Type.@method` (unbound) → компилируется, fn-pointer
- A-132.d ✅ поле `len` и метод `@len()` на одном типе → PASS
- A-132.e ✅ `@len` в теле = поле; `@len()` = метод (включая cross-method: `@len()` из `@call_len()` → корректный C-символ `Nova_T_method_len`)
- A-132.f ✅ nova_tests/syntax/method_values.nv — PASS
- A-132.g ✅ 0 regressions
