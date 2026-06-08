<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 132 — Убрать bound method value `obj.@method`; разрешить field/method одного имени

> **Создан:** 2026-06-09.  **Статус:** 📋 PLANNED.
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
