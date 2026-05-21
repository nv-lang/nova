# Plan 79: Type-checker hardening — «no silent fallback» на уровне типов

> **Создан 2026-05-21.** Выделен из re-check'а
> `[M-typecheck-missing-type-compat-checks]` (Plan 72 simplifications).
>
> **Цель:** довести принцип Plan 70 «no silent fallback» до **type-checker'а**.
> Сейчас Plan 70 закрыл silent-fallback в *кодогене*; type-checker всё ещё
> молча принимает базовые ошибки типов. Дожать так, чтобы **каждое
> выражение имело известный, проверенный тип**, любая несовместимость —
> compile-error, и **не было ни одного «skip-checking» пути** (строже TS,
> у которого `any` молча выключает проверку).

---

## Контекст: что сейчас сломано

`compiler-codegen/src/types/mod.rs` (7434 строки) проверяет имена, дубли,
эффекты, контракты — но **НЕ базовую совместимость типов**. Перепроверено
эмпирически 2026-05-21:

| Случай | Сейчас | Severity |
|--------|--------|----------|
| `fn want_bool(x bool); want_bool(42)` | компилируется И запускается | 🔴 silent miscompilation |
| `let x int = true` | компилируется тихо | 🔴 silent miscompilation |
| `fn g() -> Result[int]` (1 type-arg) | компилируется тихо | 🔴 silent miscompilation |
| `let c = Foo` (имя типа как значение) | CC-FAIL (ловит только C) | 🟡 поздняя диагностика |
| `f.nonexistent` (нет такого поля) | CC-FAIL (ловит только C) | 🟡 поздняя диагностика |

Первые три — **silent miscompilation**: ни Nova-ошибки, ни даже CC-FAIL,
программа просто работает неверно. Прямое нарушение принципа Plan 70.

Go / Rust / TS ловят **все пять** на этапе компиляции. По базовой проверке
типов Nova сейчас позади всех трёх.

---

## Принцип: типовая полнота, строже TS

1. **Каждое выражение имеет известный конкретный тип.** Нет «unknown» /
   «skip» — если тип невыводим, это **ошибка**, не тихий проход.
2. **Любая несовместимость типов — compile-error** (новые диагностики
   E73xx), а не CC-FAIL и не silent.
3. **`any` — контролируемый явный тип**, а не escape-hatch. `any`
   допустим только там, где он объявлен явно (variadic `[]any`); он
   **не «заражает»** и не отключает проверку соседних выражений (в отличие
   от TS, где `any` молча гасит ошибки). Никаких неявных untyped-путей.

Это та же дисциплина, что Plan 70 («no silent nova_int fallback»), но
на уровне type-checker'а, а не кодогена.

---

## Задачи по фазам

### Ф.1 — Assignability: arg↔param и annotation↔RHS

- Проверка совместимости типа аргумента с типом параметра на каждом
  call-site (`want_bool(42)` → E73xx «cannot use `int` as `bool`»).
- Проверка `let x T = expr` — тип `expr` должен быть совместим с `T`
  (`let x int = true` → E73xx).
- Совместимость = равенство типов + разрешённые имплицитные расширения
  (если такие есть в spec; иначе строгое равенство).

### Ф.2 — Арность type-аргументов

- `Result[T, E]` — ровно 2; `Option[T]` — 1; user-generics — по объявлению.
- `Result[int]` / `Result[A,B,C]` → E73xx «wrong number of type arguments».

### Ф.3 — Существование поля и варианта

- `record.field` — поле должно существовать в типе record'а
  (`f.nonexistent` → E73xx «no field `nonexistent` on `Foo`»).
- `Sum.Variant` — вариант должен существовать в sum-типе.
- Сейчас оба ловятся только на C-компиляции — поднять до Nova-CE.

### Ф.4 — Type-vs-value

- Имя типа в value-позиции (`let c = Foo`, `Foo + 1`) → E73xx
  «`Foo` is a type, not a value». Сейчас → CC-FAIL «undeclared identifier».

### Ф.5 — Закрытие «untyped escape» (no any-hole)

- Аудит type-checker'а: ни один путь не должен молча присваивать
  выражению «unknown» / пропускать проверку. Невыводимый тип → CE.
- `any`: разрешён только из явной аннотации (`[]any` и т.п.); проверить,
  что он не используется как имплицитный fallback при сбое вывода.
- Аналог Plan 70 internal-lint guard, но для type-checker-путей.

### Ф.6 — Тесты (позитивные + негативные)

- Негативные фикстуры на все Ф.1–Ф.4: arg-mismatch, annotation-mismatch,
  wrong type-arity, bad field, bad variant, type-as-value — каждый
  `EXPECT_COMPILE_ERROR`. **Разблокирует** негативы Plan 72 p1b/p2a.
- Позитивные: валидный код продолжает компилироваться без regression.
- Полный прогон `nova test nova_tests` + `std/` — 0 regress.

### Ф.7 — Spec

- D-блок: «типовая полнота» — каждое выражение типизировано, нет
  silent-fallback на уровне типов; правила assignability; статус `any`.

### Ф.8 — Follow-up: дописать негативные тесты Plan 72 (p1b/p2a)

**Обязательная задача ПОСЛЕ закрытия Ф.1–Ф.6.** На момент Plan 72
негативные тесты для подзадач **p1b** (empty-sum misuse) и **p2a**
(Result type mismatch) написать было **невозможно** — type-checker
лояльно принимал natural-negatives (NEG-NO-ERROR) либо они ловились
лишь C-компилятором (CC-FAIL). Это задокументировано в Plan 72
simplifications как `[M-typecheck-missing-type-compat-checks]`.

Plan 79 (Ф.1–Ф.4) делает эти случаи Nova-level compile-error'ами →
негативы становятся писабельны. Поэтому:

- После Ф.6 — **вернуться в `nova_tests/plan72/`** и дописать
  `p1b_empty_sum_type_neg.nv` и `p2a_try_from_into_neg.nv`
  (`EXPECT_COMPILE_ERROR` на assignability / type-arity / bad-variant).
- Снять оговорку «p1b/p2a без negative-покрытия» в Plan 72 doc и
  пометить `[M-typecheck-missing-type-compat-checks]` как RESOLVED.
- Plan 72 после этого получает полное pos+neg покрытие всех 8 подзадач.

---

## Порядок выполнения

```
Ф.1 (assignability)   — ядро, ~2-4 дня (плюс разбор fallout — см. Риски)
Ф.2 (type-arity)      — ~0.5 дня
Ф.3 (field/variant)   — ~1 день
Ф.4 (type-vs-value)   — ~0.5 дня
Ф.5 (no any-hole)     — ~1 день (аудит)
Ф.6 (тесты)           — ~1 день
Ф.7 (spec)            — ~0.5 дня
Ф.8 (follow-up: негативы Plan 72 p1b/p2a) — ~0.5 дня, ПОСЛЕ Ф.1-Ф.6
```

Рекомендуется **per-check инкрементально** (как Plan 70 мигрировал
57 sites): включить проверку → разобрать fallout в std/nova_tests →
следующая проверка. Можно landing'ить новую проверку сперва как warning,
затем промоутить в error после зачистки fallout.

---

## Риски

- **Главный риск — fallout.** Ужесточение type-checker'а вскроет
  существующий код в `std/` и `nova_tests/`, который сейчас компилируется
  только за счёт лояльности (латентные type-ошибки). Каждую вскрытую
  ошибку нужно исправить — объём заранее неизвестен, может быть
  значительным. Отсюда — инкрементальный per-check rollout + warning-first.
- **Имплицитные расширения:** если в Nova есть легальные имплицитные
  числовые расширения (`int`→`i64` и т.п. — см. spec D129), assignability
  должна их допускать, иначе ложные срабатывания. Сверить со spec.
- **`any`-сайты:** variadic `print(...items []any)` и подобные легально
  используют `any` — Ф.5 не должна их ломать.

---

## Ссылки на источники

- **Plan 70** «no silent nova_int fallback» — тот же принцип для кодогена;
  Plan 79 — его аналог для type-checker'а.
- **Plan 72** simplifications — `[M-typecheck-missing-type-compat-checks]`
  (эмпирическая перепроверка 2026-05-21, откуда выделен этот план).
