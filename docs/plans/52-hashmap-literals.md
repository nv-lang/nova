# Plan 52: HashMap-литералы — `{field: v}` coercion + `[k: v]` литерал

> **Создан 2026-05-15.** Production-grade, без упрощений.
>
> **СТАТУС:** план, не начат.
>
> **Реализует:** [D108](../../spec/decisions/03-syntax.md#d108-map-литерал-k-v)
> (map-литерал `[k: v]`) + ревизию [D55](../../spec/decisions/02-types.md#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы)
> (map-coercion `{field: v}` → `HashMap[str, V]`).
>
> **Зависит от:** `std/collections/hashmap.nv` (есть: `new`,
> `with_capacity`, `@insert`); D55 sum-/record-coercion (есть, частично
> реализован — [Plan 51](51-d55-record-literal-unification.md) про
> другой аспект D55, scope не пересекается).
>
> **Приоритет:** P1 — базовая эргономика, нужна везде.

---

## Зачем

Сейчас HashMap конструируется только через `HashMap[K,V].new()` /
`.with_capacity()` / `.from([(k,v),...])` — многословно. Две
комплементарные литеральные формы закрывают это:

| Форма | Когда | Тип |
|---|---|---|
| `{field: v}` | ключи — статические имена-идентификаторы | `HashMap[str, V]` |
| `[k: v]` | ключи — выражения (int, переменная, не-id строка, computed) | `HashMap[K, V]` |

Обе — coercion к ожидаемому типу, консистентно с D55. Граница чёткая:
`{}` — ключ это **имя**, `[]` — ключ это **выражение**. Не TIMTOWTDI —
разные случаи.

---

## Архитектурное решение

### `[k: v]` — литерал (D108)

- Парсинг **локальный**: после `[` первое выражение; `:` → map,
  `,`/`]` → array (D27/D38). `[]` пустой — array-или-map, разрешается
  на type-check по ожидаемому типу (как пустой массив уже сейчас).
- Ключи/значения — выражения; их позиции — D55 known-target-type
  (key→`K`, value→`V`), sum-/record-/map-coercion композируются.
- **Десугаринг — сразу в методы**, без промежуточного массива пар:
  block-expression `with_capacity(n)` + n×`@insert`. Пустой → `.new()`.
  Подход Rust `vec![]` (преаллокация + вставки).

### `{field: v}` — map-coercion (D55, третий случай)

- `{...}` **всегда** парсится как анонимный record-литерал — парсинг не
  меняется. «Мапность» — type-check-time coercion.
- Применяется, когда ожидаемый тип несёт **compiler-recognized marker
  `FromFields[V]`**. Marker здесь **load-bearing для дисамбигуации**:
  без него компилятор не знает, трактовать `{debug: true}` как поля
  struct'а `HashMap` (обычная record-coercion — упадёт) или как
  строковые ключи. Bootstrap — marker захардкожен для `HashMap`.
- Десугаринг — тот же block-expression, имена полей → строковые ключи,
  **промежуточный record не материализуется**.

### Что компилятор должен знать

Только **имена**: `HashMap`, `with_capacity`, `@insert`, `new` — не
реализацию. HashMap остаётся stdlib-типом на Nova ([feedback_third_party_libs](../../memory/feedback_third_party_libs.md)
не при чём — это наш код, но принцип «тонкий компилятор» тот же).

`std/collections/hashmap.nv` — **чистый Nova**, без `.c`-сайдкара (в
отличие от `deque.c` / `lru.c` / `queue.c` рядом). Литерал не меняет
этого: HashMap остаётся обычным Nova-кодом, литерал — синтаксический
сахар над его публичным API. Никакой части HashMap в компилятор не
переезжает.

---

## Фазы

### Ф.0 — Spec

- D108 (map-литерал) — уже в `03-syntax.md` (готово).
- D55 ревизия (map-coercion, третий случай) — уже в `02-types.md`
  (готово).
- Acceptance: spec прочитан, cross-ref'ы D55↔D108↔Plan 52 согласованы.

### Ф.1 — AST + парсер (D108)

- AST: `ExprKind::MapLiteral { pairs: Vec<(Expr, Expr)> }`. Пустой `[]`
  остаётся существующей нодой пустого литерала коллекции (array-или-map).
- Парсер `parse_bracket_literal`: после `[` парсит первый expr, затем
  ветвление по токену — `:` → map-body, `,`/`]` → array-body (как
  сейчас), `]` сразу → пустой литерал.
- Trailing-comma разрешена в обеих формах.
- Диагностика: смешение `[a, b: c]` (часть пар, часть нет) —
  понятная ошибка.
- Tests: парсер-корпус (int/str/var/computed ключи, пустой, nested,
  trailing comma, mixed-error).

### Ф.2 — Type-checker: D108 map-литерал

- Вывод `HashMap[K, V]`: `K` из унификации ключей, `V` — значений;
  либо из ожидаемого типа.
- Key-позиция → D55 known-target-type position с ожидаемым `K`;
  value-позиция → с ожидаемым `V`. Sum-/record-/map-coercion на них
  композируются (переиспользовать механизм D55).
- Пустой `[]`: расширить существующее разрешение «пустой массив по
  ожидаемому типу» — если ожидаемый тип `HashMap[K,V]`, `[]` это
  пустая мапа; если `[]T` — пустой массив; иначе «cannot infer».
- Диагностика: ключи не унифицируются; значения не унифицируются;
  `[]` без выводимого типа.

### Ф.3 — Marker `FromFields` + D55 map-coercion

- Compiler-recognized marker `FromFields[V]`. Bootstrap: захардкожен
  список типов-носителей = `{ HashMap }`. (Полноценный
  user-объявляемый marker-протокол — точка расширения, отдельная
  задача.)
- Type-checker: анонимный record-литерал `{...}` в позиции, ожидающей
  тип с `FromFields[V]` →
  - все значения полей унифицируются в `V` (с D55-coercion на каждом);
  - имена полей — валидные идентификаторы (by parse уже так);
  - результат — map-coercion, НЕ record-coercion (не матчить против
    полей struct'а).
- Пустой `{}` в такой позиции → пустая мапа.
- Диагностика: значения не гомогенны (и не приводятся к общему `V`).

### Ф.4 — Codegen: десугаринг (обе формы)

- `emit_c.rs`: `MapLiteral` и map-coerced record-литерал эмитят **один
  и тот же** block-expression:
  ```
  { let mut _m<hyg> = HashMap[K,V].with_capacity(n);
    _m.@insert(k_i, v_i)...; _m }
  ```
  - Гигиеничное имя temp-переменной.
  - Пустой → `HashMap[K,V].new()`.
  - Для `{field: v}`: ключи — строковые литералы из имён полей.
  - **Никаких промежуточных объектов** (ни массива пар, ни record'а).
  - `@insert` возвращает `Option[V]` — игнорируется.
- Покрыть все позиции: `let`-аннотация, аргумент функции, return,
  элемент другого литерала, и т.д. (там же, где работает D55).

### Ф.5 — Treewalk-интерпретатор (`nova run`)

- `interp/mod.rs`: `MapLiteral` — построить `HashMap` теми же
  вставками; map-coerced record-литерал — аналогично.
- Не оставлять `nova run` позади codegen-пути (production-grade —
  без «вторичный путь, потом»).
- Tests: `nova run` на map-литерале и `{field:v}`-coercion даёт
  корректный результат.

### Ф.6 — Stdlib

- `std/collections/hashmap.nv`: `HashMap` несёт marker `FromFields[V]`
  (форма объявления — по решению Ф.3).
- Верифицировать сигнатуры под контракт десугаринга: `with_capacity`,
  `@insert(K, V) -> Option[V]`, `new` — есть; проверить точное
  соответствие.

### Ф.7 — Тесты

`nova_tests/map_literals/` — positive:
- `[1: "a", 2: "b"]` — int-ключи, вывод типа;
- `[a: "x", a+1: "y"]` — переменная и выражение в ключе;
- `["has space": v]` — не-идентификаторная строка-ключ;
- `[]` пустой в map-позиции (let-аннотация, аргумент);
- `{debug: true, verbose: false}` → `HashMap[str, bool]`;
- `{}` пустой в map-позиции;
- композиция с sum-coercion: `["name": "alice", "age": 30.0]` →
  `HashMap[str, JsonValue]`; `{name: "alice", age: 30.0]}` тоже;
- `JsonValue.object({name: "alice", age: 30.0})` — реальная мотивация;
- map-литерал как аргумент функции / return / элемент массива;
- observable: дубликат ключа в `[k:v]` — last-wins;
- `nova run` на тех же кейсах (Ф.5).

`nova_tests/.../negative_*` — `EXPECT_COMPILE_ERROR`:
- гетерогенные значения без общего `V` (`[1: "a", 2: 3]`);
- `{1: "a"}` — `1` не имя поля → parse error (не map на `{}`);
- `[]` без выводимого типа;
- `{field: v}` в позиции struct'а **без** `FromFields` marker — не
  превращается в мапу (обычная record-coercion / ошибка полей);
- ключи не унифицируются.

### Ф.8 — Spec sync + docs

- D108 / D55 — готовы (Ф.0).
- `docs/project-creation.txt` — запись о реализации.
- `docs/simplifications.md` — bootstrap-ограничения как `[M*]`
  (в частности: marker `FromFields` захардкожен для `HashMap`, не
  user-объявляемый; протокол `FromPairs[K,V]` для `[k:v]` под другие
  map-типы — не реализован).
- Запись в discussion-log private-репы.

---

## Что НЕ входит

- **Протокол `FromPairs[K, V]`** (расширяемость `[k:v]` на `BTreeMap`,
  `OrderedMap`) — bootstrap хардкодит `HashMap`. Отдельная задача.
- **User-объявляемый marker `FromFields[V]`** — bootstrap хардкодит
  `HashMap`. Полноценный marker-протокол — позже.
- **Map-литерал на `{}`** (`{1: "a"}`) — отвергнут в D108.
- **`HashMap` как compiler builtin** — остаётся stdlib-типом; литерал —
  чистый сахар.
- **Tuple-coercion / multi-param coercion** — D55 territory, не здесь.

---

## Size estimate

| Компонент | LOC |
|---|---|
| AST + парсер D108 (Ф.1) | ~120 |
| Type-checker map-литерал (Ф.2) | ~180 |
| Marker + map-coercion D55 (Ф.3) | ~160 |
| Codegen десугаринг — обе формы (Ф.4) | ~200 |
| Treewalk interp (Ф.5) | ~100 |
| Stdlib marker (Ф.6) | ~20 |
| Тесты (Ф.7) | ~350 |
| Spec sync + docs (Ф.8) | ~30 |
| **Итого** | **~1160** |

---

## Acceptance criteria

- [ ] `[k: v]` парсится локально; `[a, b]` остаётся массивом, `[k:v]` —
      мапой; `[]` разрешается по ожидаемому типу (array vs map).
- [ ] Ключи/значения `[k:v]` — D55 known-target-type positions;
      sum-/record-/map-coercion на них композируются.
- [ ] `{field: v}` коэрсится в `HashMap[str, V]` через marker
      `FromFields` — и НЕ ломает обычную record-coercion для не-map
      struct'ов.
- [ ] Обе формы десугарятся в `with_capacity` + `@insert` block-expr —
      **ноль промежуточных объектов** (проверено по сгенерированному C).
- [ ] `JsonValue.object({name: "alice", age: 30.0})` компилируется и
      работает (композиция map-coercion + sum-coercion).
- [ ] `nova run` (treewalk) даёт тот же результат, что codegen — на
      всех кейсах Ф.7.
- [ ] Все positive + negative тесты Ф.7 PASS.
- [ ] Полная регрессия `nova test` без новых FAIL (release-сборка).
- [ ] Каждая фаза — отдельный commit.

---

## Связь

- [D108](../../spec/decisions/03-syntax.md#d108-map-литерал-k-v) —
  map-литерал `[k: v]`.
- [D55](../../spec/decisions/02-types.md#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы)
  — map-coercion (третий случай) + key/value positions.
- [D27](../../spec/decisions/03-syntax.md#d27-синтаксис-массивов-t-префикс-nt-фиксированные)
  / [D38](../../spec/decisions/03-syntax.md#d38-создание-массивов-и-turbofish-для-дженериков)
  — array-литерал на `[]`, делит скобки с map-литералом.
- [Plan 51](51-d55-record-literal-unification.md) — другой аспект D55
  (тип пишется один раз); scope не пересекается.
- `std/collections/hashmap.nv` — целевой тип, несёт marker.
