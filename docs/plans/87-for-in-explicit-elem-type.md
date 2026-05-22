# Plan 87 — for-in с явным типом элемента (`for x TYPE in iter`)

> **Статус:** 📋 proposed 2026-05-22, не начат
> **Приоритет:** P3 (L — тип элемента всегда выводится; аннотация —
> документирующий сахар, не функциональная необходимость)
> **Оценка:** ~0.5 dev-day
> **Зависимости:** нет (standalone)
> **Источник:** Plan 85.5 — маркер `[M-for-in-explicit-elem-type]` в
> `docs/simplifications.md`

## Зачем

`spec/syntax.md` документирует явную аннотацию типа loop-переменной в
for-in как валидный синтаксис:

```nova
for id u64 in ids { ... }       // syntax.md §«Аннотации типа»
for x int in nums { ... }       // syntax.md §«Циклы for / while / loop»
```

Bootstrap-парсер этого **не поддерживает** — после `for <ident>` сразу
ожидает `in` и падает «expected `in`, got identifier». Это spec/impl
drift: спека обещает фичу, которой в реализации нет. Все существующие
тесты (`nova_tests/syntax/for_iter*.nv`) используют inferred-форму
`for x in iter`, поэтому drift не всплывал до Plan 85.5.

Цель: реализовать `for x TYPE in iter` в парсере — привести
реализацию в соответствие со спекой.

## Scope

Парсер for-in: после loop-pattern, если следующий токен **не** `in` —
распарсить `TypeRef` по правилу «name type» (D-без двоеточия), затем
обязательный `in`.

**В scope:**
- `for x TYPE in iter` — простой ident + тип элемента.
- `for mut x TYPE in iter` — с `mut`-биндингом.
- Типы: примитивы (`int`, `u64`, `char`, ...), generic (`[]T`,
  `Option[T]`), user-типы.

**НЕ в scope** (см. Non-scope):
- `for (a, b) TYPE in` — tuple-паттерн с типом.
- Семантическая проверка, что аннотированный тип совпадает с типом
  элемента итератора.

## Фазы

### Ф.1 — Парсер (~0.25 д)
- `parse_for` (или эквивалент): после loop-ident — если следующий
  токен не `in`, распарсить `TypeRef`, затем `in`.
- AST: loop-переменная for-loop получает опциональную type-аннотацию
  (если поля нет — добавить `Option<TypeRef>`).
- Поддержать `for mut x TYPE in`.
- Codegen/type-checker: использовать аннотацию если она есть; иначе —
  существующий inference (поведение без аннотации не меняется).

### Ф.2 — Тесты (~0.15 д)
- Новый каталог `nova_tests/plan87/` — позитив: `for x int in [...]`,
  `for x TYPE in <custom Iter>`, `for mut x int in`, generic-тип
  элемента (`for o Option[int] in`).
- Негатив: `for x BadSyntax... in` или иной невалидный вход →
  `EXPECT_COMPILE_ERROR`.
- Регресс: inferred-форма `for x in iter` по-прежнему работает.

### Ф.3 — Spec / docs sync (~0.1 д)
- `spec/syntax.md` — проверить консистентность примеров.
- `docs/simplifications.md` — `[M-for-in-explicit-elem-type]` → ✅ ЗАКРЫТО.
- `docs/project-creation.txt` + `nova-private/discussion-log.md` — записи.

## Acceptance criteria

- [ ] `for x TYPE in iter` парсится и работает — примитивы, generic,
      user-типы.
- [ ] `for mut x TYPE in iter` работает.
- [ ] Inferred-форма `for x in iter` не сломана (регресс 0 FAIL).
- [ ] Полный `nova test` — 0 новых FAIL относительно baseline.
- [ ] `[M-for-in-explicit-elem-type]` закрыт в simplifications.md.

## Non-scope

- **Tuple-паттерн с типом** — `for (a, b) TYPE in`. Tuple-
  деструктуризация в for-in остаётся без поэлементных типов.
- **Type-checker enforcement** — проверка, что аннотированный тип
  равен фактическому типу элемента итератора. Если реализуется
  тривиально в Ф.1 — добавить; иначе вынести в отдельную задачу
  (аннотация без проверки = всё равно документирующий сахар, как
  было до плана для inferred-формы).
