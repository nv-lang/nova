# Plan 52: HashMap-литералы — `{field: v}` coercion + `[k: v]` литерал

> **Создан 2026-05-15**, production-ревизия 2026-05-15 (v2 — полный
> аудит: баг `with_capacity` entry-vs-bucket, ревизия D55 §5 `{}`,
> `@insert` discard, ordering фаз, NaN-footgun, lint дубликатов,
> диагностика keyword-ключей, C-гигиена имени temp'а, Go eval-order,
> TS `Map`/`{}` дуальность, D55 positions-inconsistency, scope Ф.3a).
>
> **СТАТУС:** ✅ **ЗАКРЫТ 2026-05-16** (Ф.0–Ф.8). AST `MapLit { pairs,
> inferred_key, inferred_value }`, parser `[k:v]` + `#from_fields`
> attribute, type-checker `MapLitCtx` + `annotate_map_literals` mutable
> pass (выводит K/V для turbofish-десугаринга), codegen+interp через
> AST-desugar (общий `desugar.rs`). Stdlib `hashmap.nv`: `with_capacity`
> entry-based (баг для n=4/7/8/13 закрыт), `#from_fields` маркер. Plan 52
> Ф.7 production-fix: десугаринг эмитит `HashMap[K, V].with_capacity(n)`
> с turbofish (через inferred K/V из annotate pass), иначе мономорфизация
> инстанциирует `HashMap[void*, void*]` → segfault. Тесты: 2 negative
> (keyword-field, mixed forms) + 3 positive (HashMap[str,int],
> [int,str], [int,int] — basic + trailing comma + expressions-as-keys
> + no-rehash). 0 регрессий vs main baseline.
>
> **Реализует:** [D108](../../spec/decisions/03-syntax.md#d108-map-литерал-k-v)
> (map-литерал `[k: v]`) + ревизию [D55](../../spec/decisions/02-types.md#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы)
> (map-coercion `{field: v}` → `HashMap[str, V]`).
>
> **Зависит от:** `std/collections/hashmap.nv` (есть: `new`,
> `with_capacity`, `@insert`); `Hashable` protocol (D72 bounds, Plan 15
> — enforcement готов).
>
> ⚠️ **Критическая зависимость — D55 argument-position coercion.** D55
> coercion реализована для `let x T = …` / `const` / return-выражения,
> но для **caller-стороны `fn f(x T)`** в таблице D55 помечена ⛔ «ещё
> нет». Flagship-пример Plan 52 — `JsonValue.object({name: "alice"})` —
> это **argument position**. Plan 52 **включает** её реализацию в scope
> (Ф.3a). [Plan 51](51-d55-record-literal-unification.md) — про другой
> аспект D55 (тип пишется один раз), scope не пересекается.
>
> ⚠️ **Критическая зависимость — баг `with_capacity`.** Текущая
> реализация `hashmap.nv` трактует аргумент как минимальный bucket-count,
> а не entry-count. `with_capacity(4)` → 4 бакета, threshold = 3 →
> 4-й `@insert` в десугаренном литерале вызывает rehash. Ф.6
> **обязан** починить реализацию **до** Ф.4 (codegen). Подробный
> расчёт — раздел «Десугаринг».
>
> **Приоритет:** P1 — базовая эргономика, нужна везде. **Чисто
> аддитивно** — новый синтаксис, миграций существующего кода нет.

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

| | Map-литерал | Пустой | Порядок вычисления | Гетерогенные значения | Реализация |
|---|---|---|---|---|---|
| **Go** | `map[K]V{k: v}` — тип-префикс обязателен; внутри composite-литерала тип элемента сокращается (но только на 1 уровень) | `map[K]V{}` | **не специфицирован** в Go spec — UB по порядку; только результат детерминирован | только через `any` (untyped, нет compile-time гарантий) | `map` — **builtin** компилятора |
| **Rust** | **нет** литерала; `HashMap::from([(k,v)])` строит промежуточный `Vec<(K,V)>` на куче; внешний макрос `maplit!` не в stdlib; `HashMap::from_iter(...)` — тоже промежуточный итератор | `HashMap::new()` | n/a | `Box<dyn Trait>` / enum — стирание типов | stdlib; `Entry` API для upsert |
| **TS** | `{a: 1}` — ключ это строка **`"a"`**, не переменная `a` (главная гоча); `{[expr]: v}` — computed key, отдельный синтаксис; `Map` — `new Map([[k,v]])` с промежуточным массивом | `{}` / `new Map()` | object: left-to-right (ECMA spec); `Map`: left-to-right | `Record<string, T>` гомогенен; object-literal — loose | — |
| **Nova** | `{field: v}` + `[k: v]` — два случая, тип-префикс не нужен, **`[a: x]` — `a` это выражение** (нет гочи) | `[]` + ожидаемый тип | **нормативно зафиксирован**: пары слева направо, внутри пары — ключ, потом значение | `HashMap[str, JsonValue]` через D55 sum-coercion — **типобезопасно** | stdlib (чистый Nova); literal — сахар |

Где Nova **лучше**:

- **Нет гочи «ключ-строка-не-переменная».** TS: `{a: x}` — ключ
  `"a"`, `a` не вычисляется (вынуждает писать `{[a]: x}`). Nova:
  `[a: x]` — `a` это выражение, всегда вычисляется.
- **Порядок вычисления специфицирован нормативно.** Go spec говорит
  «порядок вычисления map-literal expressions не специфицирован» — Nova
  фиксирует left-to-right key-then-value в D108.
- **Две формы под два случая.** Go/Rust не дают эргономичной `{}`-формы
  вовсе; TS нет отдельной Map-формы без гочи. Nova: `{field: v}` для
  статических имён, `[expr: v]` для выражений.
- **Десугаринг без waste.** Rust `HashMap::from([...])` → промежуточный
  `Vec` на куче. Nova десугарит **сразу** в `with_capacity` + `@insert`.
- **Типобезопасная разнородность.** TS: object-literal с `Record<string, T>`
  гомогенен; разнородность через `any` теряет гарантии. Nova:
  `[«a»: 1, «b»: true]` в `HashMap[str, JsonValue]` — D55 sum-coercion
  оборачивает каждое значение в вариант, **проверено компилятором**.
- **Не builtin.** Go `map` встроен в компилятор. Nova `HashMap` —
  обычный stdlib-тип на Nova; литерал знает только его публичный API.

Где Nova **на паритете** с Go: capacity-based prealloc → zero resize при
правильном `with_capacity` (тот же O(n) без амортизации). Rust `Entry`
API (`entry().or_insert(0) += 1`) — Nova закрывает через `get_or_insert`.

---

## Архитектурное решение

### `[k: v]` — литерал (D108)

- **Парсинг локальный, без type-directed.** После `[` парсим первое
  выражение; следующий токен `:` → map-литерал, дальше пары `expr : expr`.
  Токен `,`/`]` → array-литерал. `[]` пустой — array-или-map, разрешается
  на type-check по ожидаемому типу (ровно как пустой массив).
- **Ключи/значения — выражения.** Их позиции — D55 known-target-type
  (key→`K`, value→`V`); sum-/record-/map-coercion на них композируются.
- **`K: Hashable`.** Выведенный или ожидаемый `K` проверяется на bound
  `Hashable` (механизм Plan 15). Диагностика: «тип ключа `K` не реализует
  `Hashable`».

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
- **Дубликаты ключей невозможны** — имена полей record-литерала уникальны
  by construction.

### Recognition marker'а — по canonical identity, не по имени

`FromFields` распознаётся по **canonical identity** типа
(`std.collections.HashMap`), **не по bare-имени** — иначе shadowing
(`let HashMap = ...` / локальный тип `HashMap`) ломал бы правило.
Форма: атрибут-маркер на декларации типа в `hashmap.nv`, honored
компилятором по canonical path. Bootstrap — honored только для
`std.collections.HashMap`; снятие гейта (user-типы, `OrderedMap`) —
документированное будущее расширение.

### Пустой литерал — `[]`, не `{}`

**Пустая мапа — только `[]`** (+ ожидаемый тип). `{}` — пустой
block-expression с типом `unit` — **никогда не является** пустой
map-positional coercion, даже если ожидаемый тип `HashMap[str, V]`:

```nova
let h HashMap[str, bool] = []     // ✅ пустая мапа (type из контекста)
let h HashMap[str, bool] = {}     // ⛔ {} — пустой блок, тип unit ≠ HashMap
```

**Это ревизия D55 §5** (которая ошибочно допускала `{}` → empty map):
там было написано `{}` в map-позиции → `HashMap[str, V].new()`, но это
требовало type-directed parsing блока — Nova не делает этого. Ф.0
явно удаляет это правило из D55, заменяя ссылкой на `[]`.

`[]` уже разрешается по ожидаемому типу для массивов — расширяем на
map. Если ожидаемый тип не определяет однозначно «массив или мапа» —
compile error «cannot infer; annotate» (как для пустого массива сейчас).
Дефолта «`[]` это массив» нет — тип определяет контекст.

### Порядок вычисления

**Нормативно зафиксирован в D108** (это улучшение над Go, который
не специфицирует порядок map-literal expressions):

- `[k1: v1, k2: v2, ...]` — пары слева направо; внутри пары — сначала
  ключ, потом значение: `k1, v1, k2, v2, ...`. Side-effects в ключах/
  значениях наблюдаемы в этом порядке.
- `{f1: v1, f2: v2}` — значения в source-order (`v1`, потом `v2`);
  ключи — статические имена, не вычисляются.

Порядок **observable в тестах** — Ф.7 включает тест с side-effects
в ключах/значениях и проверяет точный порядок через массив-лог.

### Десугаринг — сразу в методы, без промежуточных объектов

```nova
[k1: v1, k2: v2]
// →
{
    let mut _m0 = HashMap[K, V].with_capacity(2)
    let _ = _m0.@insert(k1, v1)
    let _ = _m0.@insert(k2, v2)
    _m0
}
```

**`with_capacity(n)` — контракт: n вставок без rehash.**

Текущая реализация `with_capacity(min_capacity)` вычисляет:
```
cap = next_pow2(max(4, min_capacity))
threshold = floor(cap * 0.75)
```
С `with_capacity(4)`: cap=4, threshold=3. `@maybe_grow` вызывается в
начале каждого `@insert`; при 4-м вызове `used(3) >= threshold(3)` →
**rehash происходит** — нарушение контракта.

Чтобы n вставок гарантированно не вызывали rehash, нужно:
`cap * load_factor >= n`, т.е. `cap >= ceil(n / load_factor)`.
С `load_factor = 0.75`: `cap >= ceil(4n/3)`.

| n | Текущий cap=next_pow2(n) | threshold=floor(cap×0.75) | Rehash? | Нужный cap=next_pow2(⌈4n/3⌉) |
|---|---|---|---|---|
| 2 | 4 | 3 | нет | 4 |
| 3 | 4 | 3 | нет | 4 |
| **4** | **4** | **3** | **ДА (4>3)** | **8** |
| 5 | 8 | 6 | нет | 8 |
| 6 | 8 | 6 | нет | 8 |
| **7** | **8** | **6** | **ДА (7>6)** | **16** |
| **8** | **8** | **6** | **ДА (8>6)** | **16** |
| 12 | 16 | 12 | нет | 16 |
| **13** | **16** | **12** | **ДА** | **32** |

Баг проявляется для ~25% значений n (верхняя четверть каждого
степень-двойки диапазона). **Ф.6 обязан** исправить `with_capacity`:
```
cap = next_pow2(max(4, ceil(min_capacity / load_factor)))
```
Это делает аргумент честным entry-count (как в Rust `HashMap::with_capacity`).
Хэш-таблица получает слегка больше бакетов — цена negligible,
correctness гарантирована.

**Примечание по `@insert` return.** `@insert` возвращает `Option[V]`
(старое значение). В десугаринге возврат всегда отбрасывается через
`let _ = ...` — защита от возможного будущего lint «discarded non-unit».

**Примечание по GC-safety.** `_m0` — C-локальная переменная на стеке.
Boehm conservative GC сканирует стек + регистры (через `setjmp` трюк),
поэтому `_m0` остаётся корнем GC во время каждого `@insert`, который
может вызвать rehash+alloc. Codegen обязан хранить `_m0` как отдельную
стековую переменную (не только в регистре без spill'а перед call'ом).

**Гигиена имён.** Temp-переменная `_m0` при вложенных литералах:
каждый вложенный блок получает свой счётчик `_m0`, `_m1`, `_m2` ...
(или уникальный scope-prefix). Имя не содержит `$` — это расширение
C, не стандарт ISO C11.

**Для `{field: v}`**: ключи — строковые литералы из имён полей;
промежуточный record не материализуется.

**Дубликаты ключей** в `[k:v]` — last-wins (natural из `@insert`).
В `{field:v}` дубликаты невозможны (имена полей уникальны).

**Пустой** → `HashMap[K, V].new()`.

Bootstrap: десугаринг захардкожен на `HashMap`. Точка расширения —
протокол `FromPairs[K, V]` (`BTreeMap`, `OrderedMap`) — позже.
`HashMap.from(arr)` остаётся обычным методом для **рантайм-массива** пар.

### NaN как ключ — документированный footgun

Если `K = f64` или `K = f32` и компилятор разрешает `f64: Hashable`,
то `[f64.NAN: "x"]` — синтаксически корректный литерал. Но NaN имеет
IEEE 754-семантику: `NaN != NaN`, поэтому вставленный NaN-ключ
**невозможно найти** через `@get(f64.NAN)` (eq-сравнение вернёт false).

Это известный footgun: Rust решил радикально (`f64` не реализует `Hash +
Eq` в stdlib, нужен `OrderedFloat`), Go и TS — документируют, не
предотвращают. Nova документирует в D108 и выдаёт **предупреждение**
если NaN-константа попадает в ключевую позицию:

```
WARNING: NaN as map key — inserted key can never be found (NaN != NaN)
```

Реализация в Ф.2 (type-checker): если ключевое выражение — константа
`f64.NAN` или `f32.NAN`, emit warning. Runtime-проверку не вводим (дорого
для non-NaN случаев).

### Диагностика: keyword как поле в `{...}`

`{type: 1}` — parse error, т.к. `type` — keyword (D83). Это
**by-design граница** `{}`-формы. Без специальной диагностики ошибка
парсера будет непонятной («expected `:`, got keyword»). Ф.1 добавляет
recovery-проверку: если parser видит keyword в field-position `{...}`,
эмитит:

```
ERROR: keyword `type` cannot be used as field name in map-coercion literal
HELP: use map-literal syntax: ["type": value]
```

Ключевые слова-кандидаты: `type`, `fn`, `let`, `return`, `if`, `match`,
`for`, `while`, `import`, `export`, `module`, `use`, `with`, `spawn` и
другие из grammar. Исчерпывающий список в parser.

### Диагностика: компиляционно-известные дубликаты ключей

`[1: "a", 1: "b"]` — семантически last-wins, но **lint-предупреждение**
если оба ключа — одинаковые compile-time константы (int/str/bool literal):

```
WARNING: duplicate key `1` in map literal — second entry overwrites first
```

Это улучшение над Go (где `go vet` тоже предупреждает о duplicate map
keys), TS (где tsc предупреждает). Реализация в Ф.2: после type-check
ключей, если два ключа — константные выражения с одним значением.
Только constants (literals, `const`-переменные), не arbitrary expressions.

### D55 positions — scope Ф.3a и элементы коллекций

D55 таблица позиций содержит **несоответствие**: строка `[]T` помечена
⛔, но в тексте D55 приводится пример `save_all([{id:1, name:"a"},...])` как
работающий. Это **ошибка spec**. Ф.0 явно фиксирует: пример некорректен
для текущего bootstrap'а, добавляется честная note.

Ф.3a реализует **только** caller-side argument-position:
`fn f(x T)` → `f({...})` и `f([k:v])`. Это достаточно для всех
flagship-примеров Plan 52. Позиции элементов коллекций (`[]T` элементы)
и match-arm results остаются ⛔ — выходят за scope этого плана.

### Взаимодействие с `:` в других скобках

Три `:`-формы независимы и вкладываются без коллизий: `{field: v}` —
record/map-coercion, `(name: v)` — именованный аргумент (D102),
`[k: v]` — map-литерал. `f(opts: [1: "a"])` парсится однозначно
(named-arg `opts:`, значение `[1: "a"]`).

### `const` / comptime — не поддерживается в bootstrap

Map-литерал **только в `let`-позициях**, не `const`. `const` —
compile-time-вычисляемый (D33), а построение `HashMap` требует runtime
heap-alloc + хеширования. `const TABLE HashMap[int,str] = [1: "a"]` —
compile error с подсказкой «используй `let` / lazy-init». Снятие
ограничения — будущее (требует comptime-heap).

### Spread в map-литерале — не поддерживается в bootstrap

`[...m, k: v]` / `{...defaults, key: v}` (merge мап через D60-spread) —
**вне scope**. D60-spread определён для массивов и record'ов; семантика
merge для мап (порядок, дубликаты) — отдельная фича. В bootstrap
spread в `[k:v]` / map-coerced `{...}` — compile error.

### Большие литералы

Литерал из N пар → N statement'ов `@insert`. C-компилятор переваривает;
при корректной capacity resize не происходит. Спец-обработки не
вводятся — если когда-то всплывут гигантские литералы (>10k пар),
оптимизация отдельной задачей.

---

## Фазы и зависимости

**Порядок выполнения с зависимостями:**
```
Ф.0 (spec) → Ф.6 (stdlib fix) → Ф.1 (parser) → Ф.2+Ф.3+Ф.3a (type-checker)
           → Ф.4 (codegen) → Ф.5 (interp) → Ф.7 (tests) → Ф.8 (docs)
```

Ф.6 идёт **перед** Ф.4: codegen-тесты требуют корректного
`with_capacity` в stdlib. Ф.2/Ф.3/Ф.3a можно делать в одном коммите
(все — type-checker). Ф.5 параллелен Ф.4 (разные компоненты).

---

### Ф.0 — Spec (делать первым)

**Изменения в spec:**

1. **D108** (`03-syntax.md`) — добавить нормативный порядок вычисления
   (`k1, v1, k2, v2, ...`; ключ перед значением внутри пары; `{f:v}`
   только значения в source-order).
2. **D55** (`02-types.md`) — три правки:
   - Удалить правило §5 («пустой `{}` в map-позиции → `HashMap[str, V].new()`»);
     заменить ссылкой на `[]` как единственную форму пустой мап-позиции.
   - Добавить честную note к примеру `save_all([{id:1,name:"a"},...])`:
     «пример некорректен для bootstrap'а (позиция `[]T` помечена ⛔);
     будет работать после расширения Ф.3a на коллекции».
   - Обновить таблицу позиций: строка `fn f(x T)` → ⛔ изменить на ✅
     после Ф.3a.
3. **D108** — добавить NaN-footgun note для float-ключей.
4. **D108** — зафиксировать last-wins семантику дубликатов + lint-warning
   для compile-time-known дубликатов.

Acceptance: все cross-ref'ы D55↔D108↔Plan 52 согласованы.

---

### Ф.1 — AST + парсер (D108)

- AST: `ExprKind::MapLiteral { pairs: Vec<(Expr, Expr)> }`. Пустой `[]`
  остаётся существующей нодой пустого литерала коллекции.
- Парсер `parse_bracket_literal`: после `[` парсит первый expr, затем
  ветвление по токену — `:` → map-body, `,`/`]` → array-body, `]` сразу
  → пустой литерал.
- Trailing-comma разрешена в обеих формах.
- **Диагностика «keyword как поле»**: если в `{...}` field-position
  встречается keyword → `ERROR: keyword \`X\` cannot be used as field name; HELP: use [\`"X"\`: value]`.
- **Диагностика «смешение форм»**: `[a, b: c]` — actionable: «map-literal:
  либо все элементы `k: v`, либо это массив; нельзя смешивать».
- **Error recovery**: битая пара (`[1: ]`, `[1 2]`) — синхронизация по
  `,`/`]`, парсинг продолжается, без каскада ошибок.
- Tests: парсер-корпус (int/str/var/computed ключи, пустой, nested,
  trailing comma, mixed-error, recovery, `[k:v]` как named-arg value,
  keyword-field в `{...}`).

---

### Ф.2 — Type-checker: D108 map-литерал + lint

- Вывод `HashMap[K, V]`: `K` из унификации ключей, `V` — значений;
  либо из ожидаемого типа.
- Key-позиция → D55 known-target-type position с ожидаемым `K`;
  value-позиция → с ожидаемым `V`. Sum-/record-/map-coercion на них
  композируются (переиспользовать механизм D55).
- **Enforce `K: Hashable`** — выведенный/ожидаемый `K` проверяется на
  bound `Hashable` (механизм Plan 15). Диагностика: «тип ключа `K` не
  реализует `Hashable`».
- **Пустой `[]`**: расширить разрешение «пустой литерал по ожидаемому
  типу» — `HashMap[K,V]` → пустая мапа; `[]T` → пустой массив;
  неоднозначно/нет типа → «cannot infer; annotate».
- **NaN-warning**: если ключевое выражение — константа `f64.NAN` или
  `f32.NAN`, emit warning (Ф.0 §3).
- **Duplicate-key lint**: после type-check ключей — если два ключа
  являются одинаковыми compile-time константами → warning (Ф.0 §4).
- Диагностика actionable:
  - ключи не унифицируются: «ключи имеют типы `X` и `Y`»;
  - значения не унифицируются: «...; возможно нужен `HashMap[K, JsonValue]`?»;
  - `K` не `Hashable`: «тип ключа `K` не реализует `Hashable`».

---

### Ф.3 — Marker `FromFields` + D55 map-coercion

- Marker `FromFields[V]` распознаётся по **canonical identity** типа
  (`std.collections.HashMap`), не по bare-имени. Форма — атрибут на
  декларации в `hashmap.nv` (Ф.6), honored по canonical path.
- Type-checker: анонимный record-литерал `{...}` в позиции, ожидающей
  тип с `FromFields[V]` →
  - все значения полей унифицируются в `V` (с D55-coercion на каждом);
  - имена полей → строковые ключи; field-punning поддержан;
  - результат — map-coercion, **не** record-coercion.
- **Пустой `{}`** в map-position: type error (`unit ≠ HashMap`) —
  не empty-map coercion (D55 §5 удалено в Ф.0).
- Диагностика: значения не гомогенны → actionable.

---

### Ф.3a — D55 argument-position coercion

Критическая зависимость. D55 coercion на caller-стороне `fn f(x T)`
помечена ⛔ — без неё `JsonValue.object({...})`, `configure([1: "a"])` и
вообще «передать литерал в функцию» не работают.

- В type-checker'е: при проверке вызова, для каждого аргумента, чей
  declared-тип параметра известен, аргумент-литерал получает этот тип
  как known-target-type position → sum-/record-/map-coercion применяется
  (тот же механизм, что для `let x T = …`).
- Покрыть: обычные функции, `@`-методы, конструкторы, named-аргументы
  (D102): `f(opts: {debug: true})` при `fn f(opts HashMap[str, bool])`.
- **Scope**: только `fn f(x T)` caller-side. Позиции `[]T`-элементов
  и match-arm results остаются ⛔.
- Это расширение D55, не новая подсистема — переиспользует существующий
  coercion-проход, меняется только set активируемых позиций.
- Обновить таблицу позиций D55: строка `fn f(x T)` — ⛔ → ✅.
- Tests: record-coercion, sum-coercion и map-coercion **в позиции
  аргумента** — positive (включая named-arg) + negative.

---

### Ф.6 — Stdlib (делать до Ф.4)

**Порядок: Ф.6 перед Ф.4.** Codegen-тесты требуют корректного
`with_capacity`.

- **Исправить `with_capacity` контракт и реализацию** (критический баг):
  изменить вычисление capacity на entry-based:
  ```nova
  let cap = next_pow2(max(4, ceil_div(min_capacity * 4, 3)))
  // ceil_div(a, b) = (a + b - 1) / b — целочисленное деление с округлением вверх
  ```
  Где `4/3 = ceil(1 / 0.75)`. Это гарантирует `min_capacity` вставок
  без rehash. Совместимо с Rust-семантикой `HashMap::with_capacity(n)`.
- `@insert(K, V) -> Option[V]` — убедиться, что сигнатура соответствует
  ожидаемой десугаринг-формой `let _ = ...`.
- `HashMap` несёт marker-атрибут `FromFields[V]` (форма по Ф.3).
- `@capacity()` — убедиться, что метод публичен (нужен для тестов
  no-resize верификации).
- **Тест контракта** в `hashmap.nv`: `with_capacity(n)` + `n` insert'ов
  → `@capacity()` не изменилась. Проверить для граничных значений:
  n = 4, 7, 8, 13, 16 (все бывшие баги).

---

### Ф.4 — Codegen: десугаринг (обе формы)

- `emit_c.rs`: `MapLiteral` и map-coerced record-литерал эмитят **один
  и тот же** block-expression: `with_capacity(n)` + n×`let _ = @insert(...)`.
- `with_capacity` получает **точное `n`** (после Ф.6 это entry-count).
- **Имена temp-переменных**: `_m0`, `_m1`, ... — без `$`, валидный ISO C.
  Counter per-scope или per-depth.
- **Гигиена вложенных литералов**: `[1: [10: "x"]]` → внешний блок
  использует `_m0`, внутренний — `_m1`. Ни одно имя не shadowed.
- Пустой (`[]` в map-позиции) → `.new()`. `{field:v}` — ключи
  строковыми C-литералами из имён полей.
- **Ноль промежуточных объектов** — проверяется по сгенерированному C
  (нет `malloc`-вызовов кроме `with_capacity` и rehash-free insert'ов).
- **GC-safety**: `_m0` — стековая переменная (не register-only без
  spill); codegen обязан генерировать `NovaValue _m0 = ...` как
  отдельную C-переменную, а не временное выражение.
- Покрыть все позиции D55: `let`-аннотация, аргумент функции (Ф.3a),
  return, элемент другого литерала, named-arg value.

---

### Ф.5 — Treewalk-интерпретатор (`nova run`)

- `interp/mod.rs`: `MapLiteral` и map-coerced record-литерал — строить
  `HashMap` теми же вставками, тот же порядок вычисления.
- `nova run` не остаётся позади codegen-пути.
- Tests: `nova run` на всех кейсах Ф.7 даёт результат, идентичный
  codegen.

---

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
- `JsonValue.object({name: "alice", age: 30.0})` — реальная мотивация
  (требует Ф.3a);
- `configure({width: 80, height: 25})` — map-coercion в arg-position;
- вложенный литерал `[1: [10: "x"]]` — гигиена имён;
- map-литерал как аргумент / return / элемент массива / named-arg value;
- **порядок вычисления observable**: ключи/значения с side-effect
  (push в массив-лог) → проверить точный порядок `k1,v1,k2,v2`;
- дубликат ключа в `[k:v]` — last-wins observable;
- **no-resize**: `with_capacity(n)` + n insert'ов → `@capacity()`
  не изменилась; проверить для n = 4, 7, 8, 13 (бывшие баги);
- `nova run` на тех же кейсах (Ф.5).

`nova_tests/map_literals/negative_*` — `EXPECT_COMPILE_ERROR` /
`EXPECT_COMPILE_WARNING`:

- гетерогенные значения без общего `V` (`[1: "a", 2: 3]`);
- `{1: "a"}` — `1` не имя поля → parse error;
- `[]` без выводимого типа → «cannot infer; annotate»;
- `let h HashMap[str,V] = {}` — `{}` это блок, type error;
- `{field: v}` в позиции struct'а **без** `FromFields` — обычная
  record-coercion / ошибка полей, не мапа;
- ключ нехешируемого типа → compile error;
- `[a, b: c]` — смешение массива и пар → actionable error;
- ключи не унифицируются → actionable error;
- `{type: 1}` → keyword-field diagnostic + HELP-подсказка;
- `[1: "a", 1: "b"]` → duplicate-key warning;
- `[f64.NAN: "x"]` → NaN-key warning.

---

### Ф.8 — Spec sync + docs

- D108 / D55 — проверить, что все изменения Ф.0 в файлах.
- `docs/project-creation.txt` — запись о реализации (фазы, файлы,
  регрессия).
- `docs/simplifications.md` — bootstrap-ограничения как `[M*]`:
  - `FromFields` marker honored только для `std.collections.HashMap`;
  - `FromPairs[K,V]` для `[k:v]` под другие map-типы — не реализован;
  - D55 `[]T` element positions — ещё ⛔;
  - `const` map-literal — не поддерживается;
  - spread в map-literal — не поддерживается.
- Запись в discussion-log private-репы.

---

## Что НЕ входит

- **Протокол `FromPairs[K, V]`** (расширяемость `[k:v]` на `BTreeMap`,
  `OrderedMap`) — bootstrap хардкодит `HashMap`.
- **User-объявляемый `FromFields[V]`** — bootstrap honored только для
  `std.collections.HashMap`.
- **Map-литерал на `{}`** (`{1: "a"}`) — parse error by design.
- **`HashMap` как compiler builtin** — остаётся stdlib-типом.
- **D55 `[]T` element-position coercion** — за scope Ф.3a.
- **Tuple-coercion в `[k:v]`** — не вводится.
- **Map-comprehensions** (`[k: v for ...]`) — отдельная возможная фича.
- **Spread в map-literal** (`[...m, k: v]`) — отложено.
- **`const` map-literal** — требует comptime-heap.
- **Entry API** (`entry().or_insert()`) — не часть литерала; есть
  `get_or_insert` для upsert-паттернов.

---

## Size estimate

| Компонент | LOC |
|---|---|
| AST + парсер D108 + keyword-diagnostic (Ф.1) | ~150 |
| Type-checker map-литерал + Hashable + NaN-warn + dup-lint (Ф.2) | ~230 |
| Marker (canonical-identity) + map-coercion D55 (Ф.3) | ~180 |
| D55 argument-position coercion (Ф.3a) — все типы вызовов | ~260 |
| Codegen десугаринг — обе формы, C-гигиена (Ф.4) | ~220 |
| Treewalk interp (Ф.5) | ~110 |
| Stdlib: with_capacity fix + marker + capacity contract test (Ф.6) | ~80 |
| Тесты (Ф.7) | ~480 |
| Spec sync + docs (Ф.8) | ~40 |
| **Итого** | **~1750** |

Рост vs v1 (~1340): Ф.3a шире (все виды вызовов + named-args), Ф.6
содержит реальный fix + contract тесты, Ф.7 расширен (NaN, dup-lint,
no-resize, keyword-diagnostic).

---

## Acceptance criteria

- [ ] `[k: v]` парсится локально; `[a, b]` остаётся массивом, `[k:v]` —
      мапой; `[]` разрешается по ожидаемому типу (array vs map), иначе
      «cannot infer; annotate».
- [ ] Ключи/значения `[k:v]` — D55 known-target-type positions;
      sum-/record-/map-coercion на них композируются.
- [ ] `K: Hashable` enforced для `[k:v]`; нехешируемый ключ — compile error.
- [ ] `{field: v}` коэрсится в `HashMap[str, V]` через marker `FromFields`,
      распознаваемый по **canonical identity**; field-punning работает;
      НЕ ломает обычную record-coercion для не-map struct'ов.
- [ ] `{}` пустой — всегда блок, никогда пустая мапа (D55 §5 удалено).
      Пустая мапа — `[]` + ожидаемый тип.
- [ ] Порядок вычисления (`k1,v1,k2,v2`) зафиксирован нормативно в D108
      и **observable в тестах** через side-effects.
- [ ] `with_capacity(n)` гарантирует n вставок без rehash (entry-based);
      `@capacity()` не меняется при построении n-элементного литерала для
      **всех** n: 4, 7, 8, 13, 16 (бывшие баги).
- [ ] Обе формы десугарятся в `with_capacity(n)` + `@insert` block-expr;
      **ноль промежуточных объектов** (проверено по сгенерированному C).
- [ ] Temp-переменные имеют вид `_m0`, `_m1`, ... — valid ISO C,
      без `$`; вложенные литералы не конфликтуют.
- [ ] `let _ = @insert(...)` — всегда явный discard возврата.
- [ ] `JsonValue.object({name: "alice", age: 30.0})` компилируется и
      работает (D55 argument-position coercion + sum-coercion, Ф.3a).
- [ ] Named-arg map-coercion `f(opts: {debug: true})` работает (Ф.3a).
- [ ] `{type: 1}` → keyword-field diagnostic с HELP «use `["type": v]`».
- [ ] `[1: "a", 1: "b"]` → duplicate-key warning (не error).
- [ ] `[f64.NAN: "x"]` → NaN-key warning.
- [ ] D55 `{}` example в §1165 удалён / исправлен в spec.
- [ ] D55 `save_all([{...}])` example помечен «не работает в bootstrap»
      с honest-note.
- [ ] `nova run` (treewalk) даёт тот же результат, что codegen — на
      всех кейсах Ф.7.
- [ ] `nova check` (без codegen) корректно типизирует обе формы.
- [ ] Все positive + negative тесты Ф.7 PASS.
- [ ] Полная регрессия `nova test` без новых FAIL (release-сборка).
- [ ] Каждая фаза — отдельный commit; Ф.6 коммит до Ф.4 коммита.

---

## Связь

- [D108](../../spec/decisions/03-syntax.md#d108-map-литерал-k-v) —
  map-литерал `[k: v]`.
- [D55](../../spec/decisions/02-types.md#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы)
  — map-coercion (третий случай) + key/value positions + ревизия §5.
- [D27](../../spec/decisions/03-syntax.md#d27-синтаксис-массивов-t-префикс-nt-фиксированные)
  / [D38](../../spec/decisions/03-syntax.md#d38-создание-массивов-и-turbofish-для-дженериков)
  — array-литерал на `[]`, делит скобки с map-литералом.
- [Plan 15](15-generic-bounds-enforcement.md) — `Hashable` bound
  enforcement, переиспользуется в Ф.2.
- [Plan 51](51-d55-record-literal-unification.md) — другой аспект D55
  (тип пишется один раз); scope не пересекается.
- `std/collections/hashmap.nv` — целевой тип, несёт marker, требует
  fix `with_capacity`.
