# Plan 52: HashMap-литералы — `{field: v}` coercion + `[k: v]` литерал

> **Создан 2026-05-15**, production-ревизия 2026-05-15 (добавлено:
> сравнение Go/Rust/TS, sizing `with_capacity`, порядок вычисления,
> enforcement `K: Hashable`, recognition marker'а по canonical identity,
> разрешение пустого `{}`/`[]`, field-punning, диагностики).
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
> другой аспект D55, scope не пересекается); `Hashable` protocol (D72
> bounds, Plan 15 — enforcement готов).
>
> **Приоритет:** P1 — базовая эргономика, нужна везде. **Чисто
> аддитивно** — новый синтаксис, миграции существующего кода нет.

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

## Сравнение с Go / Rust / TS

| | Map-литерал | Пустой | Гетерогенные значения | Реализация |
|---|---|---|---|---|
| **Go** | `map[K]V{k: v}` — тип-префикс обязателен | `map[K]V{}` | только через `interface{}` (untyped) | `map` — **builtin** компилятора |
| **Rust** | **нет** литерала; `HashMap::from([(k,v)])` строит промежуточный массив; либо внешний макрос `maplit!` | `HashMap::new()` | только через enum/`Box<dyn>` | stdlib |
| **TS** | `{a: 1}` — ключ это строка `"a"`, **не** вычисляется (гоча); `Map` — `new Map([[k,v]])` | `{}` / `new Map()` | `Record<string, T>` гомогенен; объект — loose | — |
| **Nova** | `{field: v}` + `[k: v]` — две формы, тип-префикс не нужен | `[]` + ожидаемый тип | `HashMap[str, JsonValue]` через D55 sum-coercion — **типобезопасно** | stdlib (чистый Nova), литерал — сахар |

Где Nova **лучше**:

- **Две формы под два случая.** Go/Rust не имеют эргономичной
  `{}`-формы вообще; TS имеет, но с гочей «ключ-строка-не-переменная».
  Nova: `{field: v}` для статических имён, `[expr: v]` для
  выражений-ключей — без гочи (`[a: x]` вычисляет `a`).
- **Десугаринг без waste.** Rust `HashMap::from([...])` строит
  промежуточный `[](K,V)` массив. Nova десугарит **сразу** в
  `with_capacity` + `@insert` — ноль промежуточных объектов (подход
  Rust `vec![]`, но для map).
- **Пустой по контексту.** Go требует `map[K]V{}`, Rust —
  `HashMap::new()`. Nova: `[]` + ожидаемый тип.
- **Типобезопасная разнородность.** Go `interface{}`, Rust `Box<dyn>` —
  стирают типы. Nova `[«a»: 1, «b»: true]` в позиции
  `HashMap[str, JsonValue]` → D55 sum-coercion заворачивает каждое
  значение в вариант, **проверено компилятором**.
- **Не builtin.** Go `map` встроен в компилятор. Nova `HashMap` —
  обычный stdlib-тип на Nova; литерал знает только его публичный API.

Где Nova **на паритете** (и это ок): Go-builtin-map конструируется
без вызова функций — Nova десугарит в N вызовов `@insert`, которые
инлайнятся C-компилятором; при корректно заданной capacity (см. ниже)
resize при построении не происходит — тот же O(n) без амортизации.

---

## Архитектурное решение

### `[k: v]` — литерал (D108)

- **Парсинг локальный.** После `[` парсим первое выражение; следующий
  токен `:` → map-литерал, `,`/`]` → array-литерал (D27/D38). `[]`
  пустой — array-или-map, разрешается на type-check.
- **Ключи/значения — выражения.** Их позиции — D55 known-target-type
  (key→`K`, value→`V`); sum-/record-/map-coercion композируются.
- **`K: Hashable`.** Выведенный или ожидаемый `K` обязан удовлетворять
  `Hashable` (как для любого `HashMap[K,V]`). Не удовлетворяет —
  compile error с указанием, что тип ключа не хешируем.

### `{field: v}` — map-coercion (D55, третий случай)

- `{...}` **всегда** парсится как анонимный record-литерал — парсинг не
  меняется. «Мапность» — type-check-time coercion.
- Применяется, когда ожидаемый тип несёт **compiler-recognized marker
  `FromFields[V]`**. Marker здесь **load-bearing для дисамбигуации**:
  без него компилятор не знает, трактовать `{debug: true}` как поля
  struct'а `HashMap` (обычная record-coercion — упадёт) или как
  строковые ключи.
- **Field-punning** работает, как в D55 record-coercion: `{debug, verbose}`
  при `debug`/`verbose` в скоупе → ключи `"debug"`/`"verbose"`,
  значения — одноимённые переменные.
- Ключ всегда `str` → `Hashable` выполняется тривиально.
- **Edge — keyword-имена.** `{type: 1}` не парсится (D83: `type` —
  keyword, не идентификатор). Для ключа `"type"` — форма `["type": 1]`.
  Это by-design граница `{}`-формы, документируется.

### Recognition marker'а — по canonical identity, не по имени

`FromFields` распознаётся по **canonical identity** типа
(`std.collections.HashMap`), **не по bare-имени** `HashMap` — иначе
shadowing (`let HashMap = ...` / локальный тип `HashMap`) ломал бы
правило. Форма: атрибут-маркер на декларации типа в `hashmap.nv`,
honored компилятором по canonical path. Bootstrap — honored только для
`std.collections.HashMap`; снятие гейта (user-типы, `OrderedMap`) —
документированное будущее расширение.

### Пустой литерал — `[]`, не `{}`

**Пустая мапа — только `[]`** (+ ожидаемый тип). `{}` остаётся
пустым block-выражением и **не** является пустым map-литералом:
парсер не должен догадываться по контексту, блок это или пустой
record. Это убирает неоднозначность, а не прячет её:

```nova
let h HashMap[str, bool] = []     // ✅ пустая мапа
let h HashMap[str, bool] = {}     // ⛔ {} — пустой блок, type error
```

`[]` уже разрешается по ожидаемому типу для массивов — расширяем на
map. Если ожидаемый тип не определяет «массив или мапа» — compile
error «cannot infer; annotate» (как для пустого массива сейчас). Дефолта
«`[]` это массив» нет.

### Порядок вычисления

- `[k1: v1, k2: v2, ...]` — пары слева направо; **внутри пары — ключ,
  потом значение**: `k1, v1, k2, v2, ...`. Side-effects в ключах/
  значениях наблюдаемы в этом порядке.
- `{f1: v1, f2: v2}` — значения в source-order (`v1`, потом `v2`);
  ключи — статические имена, не вычисляются.
- Зафиксировать в D108 (нормативно) — Rust/Go это специфицируют, мы
  тоже.

### Десугаринг — сразу в методы, без промежуточных объектов

```nova
[k1: v1, k2: v2]
// →
{
    let mut _m$hyg = HashMap[K, V].with_capacity(2)
    _m$hyg.@insert(k1, v1)
    _m$hyg.@insert(k2, v2)
    _m$hyg
}
```

- **`with_capacity` sized под N entries без resize.** Десугаринг
  передаёт **точное число пар `n`**; контракт `with_capacity(n)` —
  «`n` вставок гарантированно без rehash». Если текущий `with_capacity`
  трактует аргумент как min-bucket-count (а не min-entry-count) — Ф.6
  правит контракт/реализацию, чтобы `n` именно entries влезали. Иначе
  «преаллокация» бессмысленна — resize посреди построения съест выигрыш.
- **Гигиена.** Имя `_m$hyg` уникально; **вложенные** литералы
  (`[1: [2: "a"]]`) дают вложенные блоки с разными именами.
- **`mut` не утекает.** `mut _m$hyg` локален блоку; результат блока —
  значение `HashMap`, биндится в `let h` (mut/не-mut — по аннотации).
- Пустой (`[]` в map-позиции) → `HashMap[K, V].new()`.
- Для `{field: v}`: ключи — строковые литералы из имён полей;
  **промежуточный record не материализуется** (литерал — только синтаксис).
- **Дубликаты ключей** в `[k:v]` — last-wins (естественно из `@insert`).
  В `{field:v}` дубликаты невозможны (имена полей уникальны).
- Bootstrap: десугаринг захардкожен на `HashMap`. Точка расширения —
  протокол `FromPairs[K, V]` (`BTreeMap`, `OrderedMap`) — позже.
- `HashMap.from(arr)` остаётся обычным методом для **рантайм-массива**
  пар; литерал через него **не** идёт.

### Что компилятор должен знать

Только **имена**: `HashMap`, `with_capacity`, `@insert`, `new` — не
реализацию. `std/collections/hashmap.nv` — **чистый Nova**, как и весь
`std/collections/`. Литерал — синтаксический сахар над публичным API;
никакой части HashMap в компилятор не переезжает.

### Взаимодействие с `:` в других скобках

Три `:`-формы независимы и вкладываются без коллизий: `{field: v}` —
record/map-coercion, `(name: v)` — именованный аргумент (D102),
`[k: v]` — map-литерал. `f(opts: [1: "a"])` парсится однозначно
(named-arg `opts:`, значение `[1: "a"]`).

---

## Фазы

### Ф.0 — Spec

- D108 (map-литерал) — в `03-syntax.md`; D55 ревизия (map-coercion) — в
  `02-types.md` (готово). **Доперенести в D108:** нормативный порядок
  вычисления (см. выше).
- Acceptance: spec прочитан, cross-ref'ы D55↔D108↔Plan 52 согласованы,
  порядок вычисления зафиксирован нормативно.

### Ф.1 — AST + парсер (D108)

- AST: `ExprKind::MapLiteral { pairs: Vec<(Expr, Expr)> }`. Пустой `[]`
  остаётся существующей нодой пустого литерала коллекции.
- Парсер `parse_bracket_literal`: после `[` парсит первый expr, затем
  ветвление по токену — `:` → map-body, `,`/`]` → array-body, `]` сразу
  → пустой литерал.
- Trailing-comma разрешена в обеих формах.
- Диагностика: смешение `[a, b: c]` — actionable («map-литерал: либо
  все элементы `k: v`, либо это массив»).
- Tests: парсер-корпус (int/str/var/computed ключи, пустой, nested,
  trailing comma, mixed-error, `[k:v]` как named-arg value).

### Ф.2 — Type-checker: D108 map-литерал

- Вывод `HashMap[K, V]`: `K` из унификации ключей, `V` — значений;
  либо из ожидаемого типа.
- Key-позиция → D55 known-target-type position с ожидаемым `K`;
  value-позиция → с ожидаемым `V`. Sum-/record-/map-coercion на них
  композируются (переиспользовать механизм D55).
- **Enforce `K: Hashable`** — выведенный/ожидаемый `K` проверяется на
  bound `Hashable` (механизм Plan 15). Диагностика: «тип ключа `K` не
  реализует `Hashable`».
- Пустой `[]`: расширить существующее разрешение «пустой массив по
  ожидаемому типу» — `HashMap[K,V]` → пустая мапа; `[]T` → пустой
  массив; неоднозначно/нет типа → «cannot infer; annotate».
- Диагностика actionable: ключи не унифицируются («ключи имеют типы X
  и Y»); значения не унифицируются («…; возможно нужен
  `HashMap[K, JsonValue]`?»).

### Ф.3 — Marker `FromFields` + D55 map-coercion

- Marker `FromFields[V]` распознаётся по **canonical identity** типа
  (`std.collections.HashMap`), не по bare-имени. Форма — атрибут на
  декларации в `hashmap.nv` (Ф.6), honored по canonical path.
- Type-checker: анонимный record-литерал `{...}` в позиции, ожидающей
  тип с `FromFields[V]` →
  - все значения полей унифицируются в `V` (с D55-coercion на каждом);
  - имена полей → строковые ключи; field-punning поддержан;
  - результат — map-coercion, **не** record-coercion (не матчить против
    полей struct'а).
- Диагностика: значения не гомогенны (и не приводятся к общему `V`) —
  actionable.

### Ф.4 — Codegen: десугаринг (обе формы)

- `emit_c.rs`: `MapLiteral` и map-coerced record-литерал эмитят **один
  и тот же** block-expression: `with_capacity(n)` + n×`@insert`, return
  `_m`.
- `with_capacity` получает **точное `n`**; см. контракт в Ф.6.
- Гигиена имени temp + поддержка вложенных литералов.
- Пустой → `.new()`. `{field:v}` — ключи строковыми литералами.
- **Никаких промежуточных объектов** — проверяется по сгенерированному C.
- Покрыть все позиции D55: `let`-аннотация, аргумент функции, return,
  элемент другого литерала, и т.д.

### Ф.5 — Treewalk-интерпретатор (`nova run`)

- `interp/mod.rs`: `MapLiteral` и map-coerced record-литерал — строить
  `HashMap` теми же вставками, тот же порядок вычисления.
- `nova run` не остаётся позади codegen-пути (production-grade).
- Tests: `nova run` на всех кейсах Ф.7 даёт результат, идентичный
  codegen.

### Ф.6 — Stdlib

- `std/collections/hashmap.nv`: `HashMap` несёт marker-атрибут
  `FromFields[V]` (форма — по Ф.3).
- **Верифицировать/зафиксировать контракт `with_capacity`:**
  `with_capacity(n)` обязан гарантировать `n` вставок **без rehash**.
  Если текущая семантика — min-bucket-count, привести к
  min-entry-count (или десугаринг считает headroom — но лучше
  починить контракт один раз). Тест: `with_capacity(n)` + `n` insert'ов
  → `@capacity` не менялась.
- `@insert(K, V) -> Option[V]`, `new` — проверить точное соответствие
  десугарингу.

### Ф.7 — Тесты

`nova_tests/map_literals/` — positive:
- `[1: "a", 2: "b"]` — int-ключи, вывод типа;
- `[a: "x", a+1: "y"]` — переменная и выражение в ключе;
- `["has space": v]` — не-идентификаторная строка-ключ;
- `[]` пустой в map-позиции (let-аннотация, аргумент функции, return);
- `{debug: true, verbose: false}` → `HashMap[str, bool]`;
- field-punning `{debug, verbose}`;
- композиция с sum-coercion: `["name": "alice", "age": 30.0]` и
  `{name: "alice", age: 30.0}` → `HashMap[str, JsonValue]`;
- `JsonValue.object({name: "alice", age: 30.0})` — реальная мотивация;
- вложенный литерал `[1: [10: "x"]]` — гигиена;
- map-литерал как аргумент / return / элемент массива / named-arg value;
- **порядок вычисления** observable (ключи/значения с side-effect →
  массив-лог `k1,v1,k2,v2`);
- дубликат ключа в `[k:v]` — last-wins (observable);
- `with_capacity` корректность — no resize на построении (через
  `@capacity` introspection);
- `nova run` на тех же кейсах (Ф.5).

`nova_tests/.../negative_*` — `EXPECT_COMPILE_ERROR`:
- гетерогенные значения без общего `V` (`[1: "a", 2: 3]`);
- `{1: "a"}` — `1` не имя поля → parse error;
- `[]` без выводимого типа;
- `let h HashMap[str,V] = {}` — `{}` это блок, не пустая мапа;
- `{field: v}` в позиции struct'а **без** `FromFields` — обычная
  record-coercion / ошибка полей, не мапа;
- ключ нехешируемого типа (`K` не `Hashable`);
- `[a, b: c]` — смешение массива и пар;
- ключи не унифицируются.

### Ф.8 — Spec sync + docs

- D108 / D55 — готовы (Ф.0).
- `docs/project-creation.txt` — запись о реализации (фазы, файлы,
  регрессия).
- `docs/simplifications.md` — bootstrap-ограничения как `[M*]`:
  marker `FromFields` honored только для `std.collections.HashMap`;
  протокол `FromPairs[K,V]` для `[k:v]` под другие map-типы — не
  реализован.
- Запись в discussion-log private-репы.

---

## Что НЕ входит

- **Протокол `FromPairs[K, V]`** (расширяемость `[k:v]` на `BTreeMap`,
  `OrderedMap`) — bootstrap хардкодит `HashMap`. Отдельная задача.
- **User-объявляемый / user-типы для `FromFields[V]`** — bootstrap
  honored только для `std.collections.HashMap`.
- **Map-литерал на `{}`** (`{1: "a"}`) и **пустая мапа через `{}`** —
  отвергнуты в D108 / разрешены в пользу `[]`.
- **`HashMap` как compiler builtin** — остаётся stdlib-типом; литерал —
  чистый сахар.
- **Tuple-coercion / multi-param coercion** — D55 territory, не здесь.
- **Map-comprehensions** (`[k: v for ...]`) — отдельная возможная
  фича, не в scope.

---

## Size estimate

| Компонент | LOC |
|---|---|
| AST + парсер D108 (Ф.1) | ~130 |
| Type-checker map-литерал + `Hashable` enforce (Ф.2) | ~210 |
| Marker (canonical-identity) + map-coercion D55 (Ф.3) | ~180 |
| Codegen десугаринг — обе формы, sized capacity (Ф.4) | ~210 |
| Treewalk interp (Ф.5) | ~110 |
| Stdlib marker + `with_capacity` контракт (Ф.6) | ~50 |
| Тесты (Ф.7) | ~420 |
| Spec sync + docs (Ф.8) | ~30 |
| **Итого** | **~1340** |

---

## Acceptance criteria

- [ ] `[k: v]` парсится локально; `[a, b]` остаётся массивом, `[k:v]` —
      мапой; `[]` разрешается по ожидаемому типу (array vs map), иначе
      «cannot infer».
- [ ] Ключи/значения `[k:v]` — D55 known-target-type positions;
      sum-/record-/map-coercion на них композируются.
- [ ] `K: Hashable` enforced для `[k:v]`; нехешируемый ключ — compile
      error.
- [ ] `{field: v}` коэрсится в `HashMap[str, V]` через marker
      `FromFields`, распознаваемый по **canonical identity** (не
      bare-имени); НЕ ломает обычную record-coercion для не-map
      struct'ов; field-punning работает.
- [ ] `{}` пустой остаётся блоком — НЕ пустой мапой; пустая мапа — `[]`.
- [ ] Порядок вычисления (`k1,v1,k2,v2`) зафиксирован нормативно в D108
      и observable в тестах.
- [ ] Обе формы десугарятся в `with_capacity(n)` + `@insert` block-expr;
      `with_capacity(n)` гарантирует `n` вставок без resize; **ноль
      промежуточных объектов** (проверено по сгенерированному C и через
      `@capacity`).
- [ ] Вложенные литералы — корректная гигиена.
- [ ] `JsonValue.object({name: "alice", age: 30.0})` компилируется и
      работает (композиция map-coercion + sum-coercion).
- [ ] `nova run` (treewalk) даёт тот же результат, что codegen — на
      всех кейсах Ф.7.
- [ ] `nova check` (без codegen) корректно типизирует обе формы.
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
- [Plan 15](15-generic-bounds-enforcement.md) — `Hashable` bound
  enforcement, переиспользуется в Ф.2.
- [Plan 51](51-d55-record-literal-unification.md) — другой аспект D55
  (тип пишется один раз); scope не пересекается.
- `std/collections/hashmap.nv` — целевой тип, несёт marker.
