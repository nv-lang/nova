# Plan 88 — generic static-method dispatch на type-параметре

> **Статус:** 📋 proposed 2026-05-22, не начат
> **Приоритет:** P2 (M — idiom `T.from(v)` в generic-коде; пока
> обходится конверсией на call-site)
> **Оценка:** ~1–1.5 dev-day (включая обязательный аудит Ф.0)
> **Зависимости:** Plan 48 (мономорфизация) ✅ partial; Plan 54 / 63
> (mono followups) ✅ — Plan 88 продолжает эту линию
> **Источник:** Plan 85.3 — маркер `[M-generic-static-method-on-typevar]`
> в `docs/simplifications.md`

## Зачем

Вызов **static-метода на type-параметре** внутри тела generic-функции
не мономорфизируется:

```nova
fn wrap[T From[str]](s str) -> T => T.from(s)
let n = wrap[Name]("alice")     // ← undefined symbol nova_fn_T_from
```

Codegen эмитит литеральный `nova_fn_T_from` — `T` не подставляется в
obj-позиции static-вызова → undefined symbol на линковке. Аналогично
`Result[T, E]` с type-параметром в return-позиции generic-функции даёт
unsubstituted `Nova_Result_...` (CC-FAIL).

**Контраст:** instance-метод на type-параметре (`it.next()` для
`[T Iter[U]]`) мономорфизируется корректно (Plan 62
`protocol_param_generic_bound.nv`). Дыра — именно static-вызовы
`T.method(...)`.

Линия mono-followup'ов: **Plan 48** (mono core) → **Plan 54**
(return-path) → **Plan 63** (cross-module dispatch) → **Plan 88**
(static-dispatch на typevar).

Уже отмечалось как bootstrap-ограничение в Plan 62.E (`nova_tests/
plan62/tryfrom_tryinto_from_prelude.nv` — bound-функции «не
вызываются»).

## Ф.0 — Аудит кластера (~0.25 д) — ОБЯЗАТЕЛЕН ПЕРВЫМ

Static-метод на typevar — почти наверняка не единственная дыра. До
реализации зафиксировать полный кластер симптомов, чтобы план закрыл
**семейство**, а не один частный случай:

- Probe-набор static-методов на type-параметре в generic-теле:
  `T.from`, `T.try_from`, `T.new`, `T.with_capacity`, пользовательские
  `T.make`.
- `Result[T, E]` / `Option[T]` / `[]T` / generic record в return-
  позиции инстанцируемой generic-функции с unresolved `T`.
- Зафиксировать список: что CC-FAIL, что undefined-symbol, что
  silent-wrong.
- По итогам аудита уточнить фазы Ф.1–Ф.2 ниже.

## Scope

- `emit_c.rs` static-call resolution: при `obj = Ident(n)`, где `n` —
  type-параметр в активном mono-контексте (`current_type_subst`) —
  резолвить `n` в concrete Nova-тип (через `nova_type_name_from_c` или
  registry) и далее обычный static-dispatch.
- mono `Result[T, E]` и прочих generic-типов с type-параметром в
  return-позиции инстанцируемой generic-функции.

## Фазы (уточняются после Ф.0)

### Ф.0 — Аудит кластера mono-static-dispatch
См. выше — отдельная обязательная фаза.

### Ф.1 — Static-call на typevar в mono-контексте
- `emit_c.rs`: `T.from(x)` (и прочие `T.static(...)`), где
  `T ∈ current_type_subst` → подстановка `T` → concrete static-dispatch.

### Ф.2 — Generic return-тип с type-параметром
- mono `Result[T, E]` / `Option[T]` в return-позиции generic-функции.
- Объём зависит от Ф.0 — если это отдельное большое семейство,
  возможно вынести в Plan 54-lineage и оставить в Plan 88 только
  static-dispatch.

### Ф.3 — Тесты
- Новый каталог `nova_tests/plan88/` — позитив: generic-функция с
  настоящим `T.from(...)` внутри тела, реально вызывается и работает;
  негатив.
- **Снять probe-ограничения** из `nova_tests/protocols/conversion/
  generic_from_bound.nv` и `generic_try_bound.nv` — переписать на
  настоящий `T.from` / `T.try_from` внутри generic-тела (сейчас они
  тестируют только bound-parse + инстанцирование как value-тип).

### Ф.4 — Spec / docs
- `docs/simplifications.md` — `[M-generic-static-method-on-typevar]` →
  ✅ ЗАКРЫТО.
- Если Ф.0 нашёл родственные дыры — обновить соответствующие маркеры.
- `docs/project-creation.txt` + `nova-private/discussion-log.md` — записи.
- D-блок: если меняется наблюдаемая семантика — аменд D72/D73; скорее
  всего не требуется (фикс приводит реализацию в соответствие с уже
  существующими D72/D73).

## Acceptance criteria

- [ ] `fn wrap[T From[str]](s str) -> T => T.from(s)` + вызов
      `wrap[X]("...")` компилируется, линкуется, корректно работает.
- [ ] generic-функция с `Result[T, E]` return-типом мономорфизируется
      (либо обоснованный defer в Plan 54-lineage по итогам Ф.0).
- [ ] `generic_from_bound.nv` / `generic_try_bound.nv` переписаны на
      прямой `T.from` / `T.try_from` внутри generic-тела.
- [ ] Полный `nova test` — 0 новых FAIL.
- [ ] `[M-generic-static-method-on-typevar]` закрыт.

## Non-scope

- Полная type-erasure → vtable fat-pointer dispatch (Plan 72 P3-B
  territory).
- Если Ф.0 покажет, что generic record-`[]T` return-path — отдельное
  большое семейство, оно остаётся в Plan 54-lineage, не в Plan 88.
