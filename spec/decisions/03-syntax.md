# Syntax — синтаксис, литералы, операторы, методы

Решения этой группы фиксируют поверхностный синтаксис Nova: формы
объявлений, стрелки, литералы, методы, операторы. Семантика типов и
эффектов — в [02-types.md](02-types.md) и [04-effects.md](04-effects.md);
здесь — только запись.

| # | Решение |
|---|---|
| [D16](#d16-дженерики-через-t-не-t) | Дженерики через `[T]`, не `<T>` |
| [D19](#d19-match-arms-через--не--) | Match-arms через `=>`, не `->` |
| [D20](#d20--вместо-void-и-сводка-стрелок) | `()` вместо `void` + сводка стрелок |
| [D22](#d22-closure-light--и-full-fn) | Closure: light `\|...\|` и full `fn(...)` |
| [D23](#d23-return--только-для-раннего-выхода) | `return` — только для раннего выхода |
| [D27](#d27-синтаксис-массивов-t-префикс-nt-фиксированные) | Синтаксис массивов: `[]T` префикс, `[N]T` фиксированные |
| [D30](#d30-стиль-именования) | Стиль именования |
| [D33](#d33-const-vs-let--compile-time-vs-runtime) | `const` vs `let` — compile-time vs runtime |
| [D34](#d34-pattern-bind-в-ifwhile-conditions--unified-grammar-с-match-arms) | Pattern-bind в `if`/`while` conditions (без `let` — ретракция Plan 114) |
| [D35](#d35-методы-инстанса-через--self-отменён) | Методы инстанса через `@`, `self` отменён |
| [D37](#d37-доступ-к-полям-name-для-record-n-для-позиционных-и-кортежей) | Доступ к полям: `.name` для record, `.N` для позиционных |
| [D38](#d38-создание-массивов-и-turbofish-для-дженериков) | Создание массивов и turbofish для дженериков |
| [D40](#d40-тело-функции--для-одного-выражения--для-блока) | Тело функции: `=>` для одного выражения, `{}` для блока |
| [D43](#d43-trailing-block--без-params-fnp-body-с-params) | Trailing: `{ block }` без params, `fn(p) body` с params |
| [D44](#d44-числовые-литералы) | Числовые литералы |
| [D45](#d45-inferred-return-type-для-expression-body) | Inferred return type для expression-body |
| [D46](#d46-перегрузка-операторов-через--методы) | Перегрузка операторов через `@`-методы |
| [D48](#d48-tagged-template-literals) | Tagged template literals |
| [D49](#d49-statement-separator-и-парсинг-выражений) | Statement separator и парсинг выражений |
| [D54](#d54-операторы-as-и-is) | Операторы `as` (compile-time cast) и `is` (runtime type-check для `any`) |
| [D58](#d58-range-литерал-iterator-protocol-for-in-implicit-iter) | Range-литерал `a..b`, `Iter[T]` protocol, `for x in c` implicit iter |
| [D59](#d59-array-tuple-и-позиционные-partial-patterns) | Array, tuple и позиционные partial patterns (`[]`, `[r]`, `[_, ..]`, `Cons(..)`) |
| [D60](#d60-spread-в-литералах-arr-record) | Spread `...x` в литералах: массив `[1, ...arr, 2]` и record `{ ...obj, field: v }` |
| [D69](#d69-variadic-параметры-через-items-t) | Variadic-параметры через `...items []T` |
| [D83](#d83-keywords-строго-запрещены-как-identifierы) | Keywords строго запрещены как identifier'ы (закрывает Q-keywords-as-fields) |
| [D88](#d88-default-значения-generic-параметров) | Default-значения generic-параметров: `[T = int]`, `[T Bound = Default]` |
| [D90](#d90-defer-и-errdefer--scope-level-cleanup-statement) | `defer` и `errdefer` — scope-level cleanup statement |
| [D102](#d102-именованные-аргументы-и-значения-параметров-по-умолчанию) | Именованные аргументы `f(name: val)` и значения параметров по умолчанию `fn f(x int = 0)`; параметр с дефолтом — keyword-only |
| [D108](#d108-map-литерал-k-v) | Map-литерал `[k: v]` — конструирование `HashMap[K, V]` (D104-D107 зарезервированы Plan 45) |
| [D126](#d126-external-type--opaque-типы-без-body) | `external type X[Generics]` — opaque типы с runtime backing, без body (D109-D125 заняты другими планами) |
| [D238](#d238-indexk-v-protocol---akey-magic) | `Index[K, V]` protocol: `@index(key K) -> V` — magic для `a[key]`; Range overload = slice mechanism |
| [D239](#d239-t-сахар-над-vect) | `[]T` как сахар над `Vec[T]` — type alias, literal desugaring, migration |
| [D240](#d240-mutindexk-v-protocol---akey--val-magic) | `MutIndex[K, V]` protocol: `mut @index(key K, val V)` — magic для `a[key] = val` |
| [D241](#d241-канонический-порядок-модификаторов-type-декларации-scope-adjacency) | Канонический порядок type-модификаторов (scope-adjacency): `value priv` канон, order-independence запрещён |
| [D262](#d262-slice-op-surface-на-t-view-модели-без-нового-типа) | Slice-op surface (`split_at`/`first_n`/`last_n`/`as_slice`/chunks/windows) на `[]T`-view модели — БЕЗ нового `Slice`-типа |

---

## D16. Дженерики через `[T]`, не `<T>`

### Что
Параметры типа записываются в **квадратных скобках**, не угловых.

### Правило

```nova
fn sort[T](xs []T, less fn(T, T) -> bool) -> []T
type Option[T] | Some(T) | None
type HashMap[K, V] { ... }

ro parsed = parse[int]("42")?
```

`[T]` — это **generic-применение** к именованному типу или функции
(`Имя[T]`). Само по себе `[T]` массивом **не является** — для массивов
есть `[]T` ([D27](#d27-синтаксис-массивов-t-префикс-nt-фиксированные)).

Грамматика однозначна:
- `Имя[T]` после идентификатора — generic-применение.
- `[]T`, `[N]T` без имени слева — конструкция массива.
- `arr[i]` в позиции выражения — индексация.

### Почему

1. **Парсер однозначен** — после имени `[` всегда генерик; `<T>`
   создаёт известную ambiguity (`sort<int>(xs)` — генерик или
   сравнение?).
2. **Турбофиш не нужен** — `parse[int]("42")` работает напрямую
   ([D38](#d38-создание-массивов-и-turbofish-для-дженериков)).
3. **Скорость компиляции** — нет backtracking, важно для AI-first,
   где LLM прогоняет компилятор много раз.
4. **Прецедент** — Go и Scala 3 пришли к тому же по тем же причинам.

### Что отвергнуто

- **`<T>` (Rust/TS/Java/C#)** — парсер-ambiguity, требует turbofish
  `::<>` или backtracking; `>>` парсится как сдвиг.
- **Контекстный парсинг с backtracking** — медленнее, ошибки
  непонятнее.

### Связь
- [D27](#d27-синтаксис-массивов-t-префикс-nt-фиксированные) — `[]T`
  как тип массива, разделение с `[T]`.
- [D38](#d38-создание-массивов-и-turbofish-для-дженериков) —
  явная передача параметров через `Имя[T]`, без `::`.
- [02-types.md](02-types.md) — generic-параметры в декларации типов.

### Эволюция
В ранних черновиках `[T]` означал и «массив», и «генерик». [D27](#d27-синтаксис-массивов-t-префикс-nt-фиксированные)
расщепил: `[]T` для массива, `[T]` только в позиции generic-применения.

---

## D19. Match-arms через `=>`, не `->`

### Что
В `match` разделитель «образец → результат» — **`=>`**, не `->`. Match-arm
имеет две формы тела: `pattern => expr` (одно выражение) или
`pattern => { block }` (блок). Match-arm — **исключение** из общего
правила [D40](#d40-тело-функции--для-одного-выражения--для-блока)
«`=>` и `{}` не сочетаются».

### Правило

`->` — для **типов и сигнатур**:

```nova
fn f(x int) -> int                       // тип возврата
type Handler alias fn(Request) -> Response // функциональный тип через alias
```

`=>` — для **тела и разветвлений**:

```nova
match shape {
    Circle { r } => 3.14 * r * r
    Square { s } => s * s
}

ro inc = |x| x + 1
fn double(x int) -> int => x * 2
```

Match-arm с блоком — **через `=>` и `{}`** (Rust-стиль):

```nova
match entry {
    Empty => insert_new(idx, key, value)        // одно выражение
    Occupied { value: old } => {                // блок через => { ... }
        @entries[idx] = Occupied { key, value }
        return Some(old)
    }
    Tombstone => {
        @tombstones -= 1
        @entries[idx] = Occupied { key, value }
        return None
    }
}
```

Грамматика:

```
match-expr = 'match' expr '{' { match-arm } '}'
match-arm  = pattern [ guard ] '=>' arm-body
arm-body   = expression | block
guard      = 'if' expr
block      = '{' { statement } [ expression ] '}'
```

«Параметры → тело» и «образец → результат» — одна семантика «дай мне
это, я отдам тебе то», везде один символ `=>`.

### Почему

1. **Разделение ролей.** `->` декларативно (тип), `=>` вычислительно
   (выражение). Глаз видит границу.
2. **Прецедент.** C#, F#, Scala 3, Rust унифицируют `=>` для лямбд и
   match-arms.
3. **AI-first.** Один символ — одна роль, меньше путаницы у LLM.
4. **`=>` всегда в match-arm.** Без `=>` parser не отличал бы блок-arm
   от guarded-arm `pattern if cond => expr` или от вложенного блока
   внутри сложного pattern'а. `=>` остаётся гарантированным маркером
   «начало результата».

### Что отвергнуто

- **`->` для match-arms (Rust до 1.0, OCaml/Haskell)** — перегрузка
  с типом возврата.
- **`:` (Python)** — конфликт с record-литералами.
- **`then`** — лишнее ключевое слово ради того же эффекта.
- **Блок-arm без `=>`** (`pattern { block }`). Без `=>` теряется
  единый маркер «начало результата»; парсер хуже различает arm с
  блоком от arm с guarded-pattern и от нестед-блока в сложном
  pattern'е.

### Связь
- [D20](#d20--вместо-void-и-сводка-стрелок) — сводная таблица стрелок.
- [D22](#d22-closure-light--и-full-fn) — closure-light `|x|` без `=>`,
  closure-full `fn(...)` подчиняется D40 как named fn.
- [D40](#d40-тело-функции--для-одного-выражения--для-блока) — общий
  закон «`=>` и `{}` не сочетаются» и match-arm как **единственное
  исключение**.

### Эволюция
Старые примеры `match ... -> result` обновлены на `=>`.

---

## D20. `()` вместо `void`, сводка стрелок, function type syntax

### Что
Тип «без значения» — `()` (unit), не `void`. Плюс сводная таблица
стрелок (каждая роль закреплена за одним символом) и **обязательный
`fn`-keyword** для function type везде.

### Правило

```nova
fn cleanup() Io -> ()           // явно
fn cleanup() Io                  // -> () можно опустить
ro xs [()] = [(), (), ()]       // unit как элемент массива
ro r Result[(), str] = Ok(())   // unit как generic-параметр
```

Сводка символов:

| Символ | Роль |
|---|---|
| `->` | тип возврата, функциональный тип |
| `=>` | тело функции (именованной или анонимной), match-arm |
| `=`  | присваивание (`let x = 5`) |

Один символ — одна роль.

#### Function type — всегда с `fn` префиксом

Function type записывается **только** через `fn(args) Effects? -> Ret`.
Бесколонная форма `(args) -> Ret` **запрещена** во всех контекстах.

```nova
// ✓ — function type везде с fn
fn sort[T](xs []T, less fn(T, T) -> bool) -> []T
type Handler alias fn(Request) -> Response
ro callback fn() -> int = ...
type Server { handler fn(Request) -> Response }
fn measure[T](action fn() Io -> T) Time -> (T, Duration)

// ✗ — без fn запрещено
ro f () -> int = ...                      // ✗
type Handler alias (Request) -> Response   // ✗
fn sort[T](xs []T, less (T, T) -> bool)    // ✗
type Server { handler (Request) -> Response }  // ✗
```

**Где конкретно `fn` нужен:**

| Контекст | Синтаксис |
|---|---|
| Type alias | `type H alias fn(Args) -> Ret` |
| Параметр функции | `fn f(g fn(Args) -> Ret) -> ...` |
| Let-annotation | `let f fn(Args) -> Ret = ...` |
| Поле record | `type X { cb fn(Args) -> Ret }` |
| Generic-bound | `[T fn(Args) -> Ret]` (если применимо) |
| Возврат функции | `fn make() -> fn(int) -> int` |

#### Почему `fn` обязателен

1. **Парсер однозначен.** Без `fn` парсер видит `(int) -> bool` и должен
   делать lookahead чтобы различить:
   - Group expression (parens around expression) в выражении.
   - Tuple type `(int)` в позиции типа (хотя одно-element tuple
     обычно не пишется в Nova).
   - Function type начало.

   `fn` ставит явный признак «дальше function type» — парсер не
   ошибается.

2. **AI-friendly.** LLM, генерирующая код, не путает функциональный
   тип с tuple/grouping. Один синтаксис для function type, один путь.

3. **Согласованность с named-fn.** `fn name(args) -> Ret => body` —
   именованная функция начинается с `fn`. Function type
   `fn(args) -> Ret` — то же начало. Это **одна и та же** концепция
   «function thing» — `fn` это её префикс.

4. **D9 «один путь».** Не два варианта (alias-form vs other-form).
   Везде одинаково.

5. **Прецеденты.** Rust (`fn(i32) -> bool`), Go (`func(int) bool`) —
   оба требуют function-type keyword. TypeScript/Kotlin/Swift не
   требуют, потому что у них grammar не имеет `(x)` group-expr
   ambiguity (разные приоритеты parsing). Nova с её парсером ближе
   к Rust/Go.

#### Не путать с closure

**Function type** (тип) — `fn(int) -> bool`.
**Closure value** (выражение) — `|x| x > 0` (light) или `fn(x int) -> bool => x > 0` (full).

```nova
// Тип: fn(int) -> bool
ro pred fn(int) -> bool = |x| x > 0
//        ^^^^^^^^^^^^^^^^^      ^^^^^^^^^^^^
//        type annotation         closure-light value

// closure-full — анонимная fn (см. D22):
ro pred fn(int) -> bool = fn(x int) -> bool => x > 0   // closure-full
ro pred fn(int) -> bool = fn(x int) -> bool { x > 0 }  // closure-full block
```

`fn` встречается в трёх ролях, различимых по контексту:
- **Декларация** — `fn name(...) ...` (top-level statement-position).
- **Тип** — `fn(int) -> bool` (в type-annotation position).
- **Closure-full** — `fn(x int) -> bool => body` (в expression-position).

См. [D22](#d22-closure-light--и-full-fn) для closure-light vs full.

### Почему

1. **`()` — обычный тип.** Может быть generic-параметром, элементом
   массива, полем. `void` в C/Java — особый случай с дырами.
2. **Двухсимвольное разделение** яснее «всё через `->`» (Rust) или
   «всё через `=>`»: глаз видит границу «тип / выражение».
3. **Прецедент.** Rust/Haskell/OCaml/Swift/Kotlin — `()`/`Unit` как
   нормальный тип. Дыра `void` — известная боль во всех языках, где
   её оставили.

### Что отвергнуто

- **`void`** — не может быть generic-параметром (`Result[void, E]`),
  требует обходных путей.
- **Везде один символ** (`->` или `=>`) — перегрузка, теряется
  визуальная граница.
- **Третий символ** (`~>`, `:>`) — экзотика без выигрыша.

### Связь
- [D19](#d19-match-arms-через--не--) — match-arm через `=>`.
- [D22](#d22-closure-light--и-full-fn)
  — пересмотр: `=` больше не используется для тел функций.

### Эволюция
Ранее `=` отделял тело именованной функции (`fn f() = expr`). [D22](#d22-closure-light--и-full-fn)
перенёс эту роль на `=>`, чтобы убрать дублирующий синтаксис. `=`
теперь — только присваивание.

---

## D22. Closure: light `|...|` и full `fn(...)`

### Что
В Nova две взаимодополняющие формы closure:

1. **closure-light** — `|params| body` — компактная untyped форма.
   Без типов параметров, без `-> T`, без эффектов. Тело — bare
   expression ИЛИ block.
2. **closure-full** — `fn(params T) Effects -> Type body` —
   типизированная форма, идентичная named fn без имени. Тело —
   `=> expr` или `{ block }`, как у named fn ([D40](#d40-тело-функции--для-одного-выражения--для-блока)).

Эти формы **не пересекаются**: как только нужен хоть один тип
параметра, return-type или эффект — переключаемся на `fn(...)`.
`|...|` — **только** untyped.

Тело именованной функции остаётся как было: `=> expr` или `{ block }`
(D40). `=` — только для `let`.

### Правило

#### closure-light

```nova
ro inc   = |x| x + 1                              // expr-body
ro zero  = || 0                                    // no params
ro block = |x| { ro y = x*2; y + 1 }              // block-body
ro any   = |_| 0                                   // wildcard

list.filter(|x| x > 0)                              // closure-arg
list.fold(0, |acc, x| acc + x)                      // multiple params
list.map(|_| 42)                                    // ignore element
spawn(|| compute())                                  // no-arg closure-arg
```

Грамматика:

```
closure-light = '|' params? '|' (expression | block)
params        = identifier { ',' identifier }
identifier    = name | '_'
```

В closure-light **запрещено**:

```nova
|x int| x + 1            // ❌ типы параметров — переключайся на fn(x int)
|x| -> int { ... }       // ❌ return-type — переключайся на fn(x) -> int
|x| Db -> R { ... }      // ❌ эффекты — переключайся на fn(x) Db -> R
|x| => x + 1             // ❌ нет `=>` в closure-light, body начинается сразу
```

#### closure-full

```nova
ro typed    = fn(x int) -> int => x * 2
ro block    = fn(x int, y int) -> int { ro z = x+y; z * 2 }
ro with_eff = fn(req Request) Db Log -> Response { process(req) }
ro void     = fn(s str) Log { Log.info(s) }
```

Грамматика идентична named fn без имени:

```
closure-full = 'fn' '(' params ')' [ effects ] [ '->' type ] body
body         = '=>' expression | block
params       = param { ',' param }
param        = identifier type            // тип обязателен
```

#### Inference и context-sensitivity

closure-light валиден **только когда контекст однозначно задаёт
сигнатуру**. Источники контекста:

1. **Параметр fn-call'а**: `list.filter(|x| x > 0)` — sig из `filter`'а.
2. **Annotated let**: `let f fn(int) -> int = |x| x + 1`.
3. **Return-position**: `fn make() -> fn(int) -> int => |x| x + 1`.
4. **Tuple-position при typed return**: `(|x| ...)` если parent
   объявил `-> (fn(int) -> int, ...)`.
5. **First-use inference** (Rust-семантика):
   ```nova
   ro f = |x| x + 1
   f(5)                    // first use фиксирует x: int → sig: fn(int) -> int
   f(3.14)                 // ❌ ошибка: sig уже зафиксирован
   ```

Если контекст недостаточен (closure-light нигде не используется):

```nova
ro f = |x| x + 1           // ❌ cannot infer signature
```

→ либо использовать `f` далее, либо переключиться на closure-full:

```nova
ro f = fn(x int) -> int => x + 1
```

#### Эффекты

closure-light **никогда не пишет эффекты** в сигнатуре. Эффекты,
реально используемые в теле closure-light, должны:
- быть подмножеством contextual-sig'а, И
- покрываться **ambient effect-set** в точке создания closure'а
  (= эффекты enclosing-функции ∪ активные `with`-блоки).

```nova
fn process(users []User) Db -> []Result =>
    users.map(|u| Db.find(u.id))                   // Db: ✅ есть в parent

fn pure(xs []int) -> int =>
    xs.fold(0, |acc, x| acc + x)                   // эффектов нет — ✅

fn no_db(users []User) -> []Result =>              // Db в parent НЕТ
    users.map(|u| Db.find(u.id))                   // ❌ Db не доступен
```

closure-full эффекты пишет явно — она «полная» по сигнатуре:

```nova
fn make_handler() -> fn(Request) Db -> Response =>
    fn(req) Db -> Response { process(req) }
```

Эффекты на named fn остаются обязательными — D62/R1 «эффекты всегда
видны в сигнатуре» не ослабляется. Inference применим только к
closure-light, потому что closure-light не пересекает границу модуля.

#### Captures

Closure захватывает свободные переменные **по ссылке через scope**.
Никаких `move` / `&mut` / lifetime — это не нужно благодаря
managed-heap ([D32](02-types.md#d32), [D62](04-effects.md#d62)).

- **Примитивы** (`int`, `bool`, `f64`, …) — copy-by-value.
- **Объекты** (record, sum-type, array) — managed-reference,
  shared с enclosing scope.
- **`let mut` переменные** — closure модифицирует **тот же slot**;
  изменения видны снаружи и между вызовами closure'а.
- **Escape** — если closure уезжает за пределы создавшей fn,
  захваченные переменные автоматически живут в managed-heap.

```nova
fn make_counter() -> fn() -> int {
    mut count = 0
    || { count = count + 1; count }
}

ro f = make_counter()
ro g = make_counter()
f()    // 1   ← каждый вызов make_counter создаёт свежий scope
f()    // 2
g()    // 1   ← у g свой count, не shared с f
```

Несколько closure'ов, созданных в одном scope, **разделяют** capture:

```nova
fn make_counter() -> (fn() -> int, fn(int) -> int, fn() -> int) {
    mut count = 0
    (
        || { count = count + 1; count },
        |a| { count = count + a; count },
        || count,
    )
}

ro (f1, f2, f3) = make_counter()
f1()    // 1   ← все три closure'а share один count
f1()    // 2
f2(5)   // 7
f3()    // 7
```

#### Free-variable resolution

Свободные переменные резолвятся через **lexical scoping** на момент
**создания** closure'а. Параметр одного closure'а **не виден** в теле
другого:

```nova
mut count = 0
(|a| count += a, || a)                              // ❌ `a` undefined в `|| a`
//                  ^
//                  parameter of previous closure, not in scope here
```

#### Body-type matching

Тип тела closure (выводимый или явный) должен совпадать с ожидаемым
return-type из contextual sig:

```nova
fn make() -> (fn() -> int, fn(int) -> int) =>
    (|| 0, |a| count += a)
//          ^^^^^^^^^^^^^ ❌ `count += a` returns `()`, sig expects `int`
//                          fix: |a| { count += a; count }
```

#### `return` в closure-light

`return` в `|x| { ... }` выходит **из самого closure**, не из
enclosing fn. Это согласовано с D43 (`return` в trailing-block выходит
из блока):

```nova
ro find = |xs []int| {
    for x in xs {
        if x > 100 { return Some(x) }                // выход ИЗ closure
    }
    None
}
```

#### Wildcard `_` в параметрах

`_` валиден как имя параметра в closure-light, closure-full и named fn —
«параметр обязателен по арности, не используется в теле»
(расширение [D59](#d59-array-tuple-и-позиционные-partial-patterns)):

```nova
list.map(|_| 42)
fn handle(req Request, _meta Meta) Db -> Response { ... }
fn(_x int, y int) -> int => y * 2
```

### Почему

1. **Освобождение `=>`.** В Nova `=>` — маркер тела (named fn,
   handler-method) и match-arm. Использование `=>` в лямбдах создавало
   перегрузку и запрещало блок-форму. Closure-light с `|...|` убирает
   перегрузку: `=>` остаётся только для тела/arm.
2. **Two-level: light vs full.** Untyped one-liner'ы (`filter`, `map`,
   `fold`) получают компактный синтаксис. Typed/effect-aware closures
   пишутся полной формой `fn(...)`, идентичной named fn — нет
   специальной грамматики anonymous-typed.
3. **Парсер коммитится за один токен.** `|...|` в expression-position
   решается мгновенно (binary `|` без LHS невозможен). Старый
   `(params) =>` требовал unbounded look-ahead.
4. **Trailing и closure ортогональны.** closure-light **только** в
   expression-position. Trailing — через `fn(...)` или zero-param `{}`
   ([D43](#d43-trailing-block--без-params-fnp-body-с-params)). Парсер не путает.
5. **Anonymous fn возвращается.** D22-old запрещала `fn(...)` без
   имени; новая D22 разрешает её как closure-full.
6. **Блок-форма для closure-light.** `|x| { stmts; expr }` теперь
   разрешено — старая D22 явно запрещала `=> { block }`, что заставляло
   выносить любую closure с `let` в named fn.
7. **Captures без `move`/lifetime.** Managed-heap ([D32](02-types.md#d32))
   делает escape автоматическим.

### Что отвергнуто

- **`(x) => expr`** (D22-old) — перегружает `=>`, требует unbounded
  look-ahead, не имеет блок-формы.
- **`x => expr`** без скобок (JS-style) — не решает look-ahead для
  multi-param случая, оставляет `=>` перегруженным.
- **`fn(...)` без типов** (overlap с `|...|`) — две взаимозаменяемых
  формы создают выбор без правила. Граница «типы есть → `fn`, нет → `|...|`»
  чёткая.
- **Effect inference на named fn** — отказ от R1 «эффекты всегда видны
  в сигнатуре». Inference допустим только для closure-light.
- **`move`-keyword / lifetime-маркеры** — managed-heap автоматизирует
  escape.
- **Implicit `it`** — нелокальный reasoning, плохо для AI.
- **Trailing closure через `|x|`** — `func(args) |x| body` создавал
  ambiguity с binary `|`. Trailing с params — только через `fn(...)`,
  см. [D43](#d43-trailing-block--без-params-fnp-body-с-params).
- **`=> { block }` для closure-light** — closure-light не использует
  `=>` вообще. Тело всегда либо bare expression, либо block.

### Связь
- [D19](#d19-match-arms-через--не--), [D20](#d20--вместо-void-и-сводка-стрелок)
  — `=>` остаётся в match-arm как маркер «начало результата».
- [D40](#d40-тело-функции--для-одного-выражения--для-блока) — правило
  «`=>` и `{}` не сочетаются» применяется к named fn, closure-full,
  handler-method. closure-light имеет отдельную грамматику.
- [D43](#d43-trailing-block--без-params-fnp-body-с-params) — trailing с params
  через `fn(...)`, без params — `{ block }`. `|...|` в trailing-position
  запрещён.
- [04-effects.md → D31](04-effects.md#d31) — handler-method, как fn,
  имеет две формы тела.
- [D62](04-effects.md#d62) — closure-light наследует ambient
  effect-set.
- [02-types.md → D32](02-types.md#d32) — captures через managed-heap.

### Эволюция
Пересмотр D20: `=` исключён из «тел функций», его роль принял `=>`.

Ревизия (2026-05-1): «лямбда строго `(params) => expr`, без блок-формы».

Ревизия (2026-05-10): полная замена `(params) =>` на two-level
closure: `|x|` (light, untyped) + `fn(...)` (full, typed). Триггер —
семантический перегруз `=>`, look-ahead в парсере, запрет блок-формы
лямбды, унификация с trailing-block. Anonymous-fn запрет (D22-old)
снимается — `fn(...)` без имени = closure-full. Block-форма closure
возвращается. Migration: ~30 примеров в spec/, патч parser/interp,
план — [docs/plans/19-closure-and-error-ops.md](../../docs/plans/19-closure-and-error-ops.md).

---

## D23. `return` — только для раннего выхода

### Что
`return` есть, но используется **исключительно** для guard-clauses /
ранних выходов. Последнее выражение тела — автоматически результат.

`return` — это **statement**, поэтому он встречается только в **блок-форме**
тела (`fn name(...) { ... }`). В `=>`-теле (где должно быть ровно одно
выражение, [D40](#d40-тело-функции--для-одного-выражения--для-блока))
guard-clauses через `return` не пишутся: либо вся функция выражается
одним `match`/`if` (тогда `=>`-тело подходит), либо нужны guard'ы — и
тогда блок-форма.

### Правило

Разрешено:

```nova
// блок-форма с guard'ами
fn classify(x int) -> str {
    if x < 0  { return "negative" }
    if x == 0 { return "zero" }
    "big"                              // последнее выражение = результат
}

fn process(req Request) Db Fail -> Response {
    if req.method == "GET" { return next(req) }
    do_work(req)
}

// =>-тело: одно выражение, return не нужен
fn classify(x int) -> str => match x {
    n if n < 0  => "negative"
    0           => "zero"
    _           => "big"
}
```

Запрещено линтом (избыточно):

```nova
fn double(x int) -> int => return x * 2     // лишний return; и =>-тело
                                            // вообще не допускает statement'ов
fn classify(x int) -> str {
    if x < 0 { return "n" } else { return "p" }   // обе ветки return
}
```

Если все ветви заканчиваются `return` — переписать через `match`/`if`
как выражение и использовать `=>`-тело.

Запрещено грамматически:

```nova
// =>-тело допускает ровно одно выражение, а не цепочку statement'ов
fn classify(x int) -> str =>
    if x < 0  { return "negative" }      // ← statement, не expression
    if x == 0 { return "zero" }
    "big"
```

Семантика:
- `return` в closure-light (`|x| body`) — выходит **из самого closure**,
  не из enclosing fn ([D22](#d22-closure-light--и-full-fn)). Аналогично
  `return` в trailing-block.
- `return` в closure-full (`fn(...) body`) — выходит из closure
  (точно как named fn).
- `return` в match-arm — match-arm тоже строго `pattern => expr`
  ([D40](#d40-тело-функции--для-одного-выражения--для-блока)),
  поэтому `return` в arm тоже отсутствует. Если в arm нужен
  ранний выход — match вынесен в блок-форму fn, и `return`
  стоит после match'а.
- `return` в `with`-блоке (block-body) — выходит из enclosing-функции.
- `return` в trailing-block ([D43](#d43-trailing-block--без-params-fnp-body-с-params)) —
  выходит из самого блока (это блок, не лямбда), не из enclosing fn.

### Почему

1. **Guard-clauses естественно пишутся** в блок-форме — middleware,
   валидация, ранние выходы.
2. **AI-first.** LLM рефлекторно генерит `return` — полный запрет
   требовал бы переучивания.
3. **Один стиль на функцию.** Линт против избыточного `return` в
   последней позиции.
4. **Прецедент.** Rust идиоматически использует `return` только для
   ранних выходов.
5. **`=>` строго одно выражение.** Раньше D23 разрешал чередование
   guard-`if {return}` + финальное выражение в `=>`-теле. Это
   нарушает «`=>` = одно выражение» ([D40](#d40-тело-функции--для-одного-выражения--для-блока));
   убрано — guard'ы только в блок-форме.

### Что отвергнуто

- **Полное отсутствие `return` (OCaml/Haskell)** — заставляет вкладывать
  `if/else` глубже.
- **`break`/`done`** — нестандартно, без выгоды.
- **`return` обязателен (Go/Java)** — противоречит «функция = выражение».
- **Guard-цепочки в `=>`-теле** (как было в старой D23). Конфликтовало
  с D40 — `=>`-тело это одно выражение, statement-цепочки требуют
  блок-формы.

### Связь
- [D22](#d22-closure-light--и-full-fn) — `return` в closure-light
  и closure-full выходит из самого closure, не из enclosing fn.
- [D19](#d19-match-arms-через--не--) — match-arm строго
  `pattern => expr` или `pattern => { block }`; `return` в arm
  выходит из enclosing fn (т.к. arm не функция).
- [D40](#d40-тело-функции--для-одного-выражения--для-блока) — `=>`
  и `{}` не сочетаются; guard-цепочки требуют блок-формы.

### Эволюция
Ревизия (2026-05): убраны примеры guard-clauses в `=>`-теле fn.
Раньше D23 допускал `fn classify(x) -> str => if x<0 {return "n"} ... "big"`
— цепочка statement'ов после `=>`. Это противоречило D40 («`=>` =
ровно одно выражение»). Теперь правило единое: guard'ы только в
блок-форме `fn name(...) { ... }`.

---

## D27. Синтаксис массивов: `[]T` префикс, `[N]T` фиксированные

> **Amended (Plan 138 D239, 2026-06-10):** `[]T` теперь синтаксический
> псевдоним `Vec[T]` (D239). Синтаксис записи не меняется, семантика —
> меняется: `[]T ≡ Vec[T]` на уровне типов.
>
> `arr[i]` для типов реализующих `Index[K, V]` (D238) — десугаринг в
> `arr.index(i)` вместо compiler built-in. `[]T` / `Vec[T]` реализует
> `Index[int, T]` и `Index[Range, Vec[T]]` (zero-copy slicing).

> **Amended ([M-fixed-array-value-semantics], 2026-07-10): `[N]T` — value-класс.**
> Layout-обещание этого блока («`[N]T` — N подряд, без указателя») теперь
> **нормативная семантика хранения И копирования**, а не только раскладка:
>
> 1. **Inline-хранение.** `[N]T` живёт по месту — в стек-кадре у локала,
>    внутри объекта у поля record'а (C: `T name[N];`). Никакого heap-указателя
>    и len/cap-заголовка: `N` — компайл-тайм константа типа. (До амендмента
>    имплементация кучевала `[N]T` наравне с `[]T` — это ретрактировано.)
> 2. **Копирующая семантика.** Присваивание/передача/возврат следуют
>    value-правилам (D216 §V3.1, таблица value-типов — `[N]T` включён):
>    `mut b = a` — независимая копия (мутация `b` не видна в `a`); параметр
>    по умолчанию — ro-представление (копия либо скрытая ссылка на выбор
>    компилятора, ненаблюдаемо — D326 Р10); `mut x [N]T` — in-out ссылка
>    (мутации видны вызывающему, единый канон D326 Р3/Р10); `-> [N]T` —
>    возврат по значению.
> 3. **Элементы могут быть кучевыми.** `[N]str` / `[N]*T` / `[N]Vec[T]` —
>    контейнер остаётся inline value; элементы-указатели сканируются GC
>    по месту (consecutive-стек и heap-объекты Boehm сканирует
>    консервативно — пейлоады удерживаются на протяжении жизни массива).
> 4. **Bounds-check.** `arr[i]` / `arr[i] = v` паникуют при OOB по
>    компайл-тайм `N` (сообщение как у `[]T`: «array: index i out of
>    bounds for length N»); элизия на доказанных in-range сайтах —
>    та же политика D257, что у `[]T`.
> 5. **Литералы.** `[e0, …, eN-1]` в `[N]T`-позиции обязан иметь ровно `N`
>    элементов; `...spread` в `[N]T`-литерале запрещён (N фиксирован).
>    Оба нарушения — ошибка компиляции.
>
> `[]T`-часть блока (Vec-канон D239) не затронута. Открывает
> `SocketAddr { image [20]u8 }` без аллокаций (174.5-связка).
> Тесты: `nova_tests/fixed_array/` (pos+neg+panics), GC-удержание —
> `gc_forced_collect.nv`.

> **Amended ([D440](#d440-fixedarray-len-и-ptr-аксессоры-plan-200-п19-2026-07-21),
> 2026-07-21): `[N]T @len()` / `@ptr()` accessors.** `[N]T` получает
> зеркало Vec/str-поверхности — `arr.len()` (компайл-тайм `N`) и
> `arr.ptr()`/`mut`-перегрузка `-> *mut T` (адрес первого элемента).
> Компилятор-синтез (НЕ `.nv`-декларация — method-level const-generic
> `N` в языке отсутствует). Подробности — D440.

> **Amended (221.1 №24, 2026-07-23, ОКНО-4, владелец: путь (б)):
> `[](T1, T2, ...)` — `[]T`-алиас как ТИП в EXPRESSION-позиции, когда
> `T` сам требует скобок (кортежный тип-элемент).** До амендмента `[]`
> в expression-позиции ВСЕГДА парсился как пустой литерал (Vec/array-или-
> map, D38), а следующая `(...)` — как ВЫЗОВ этого литерала: `[](str,
> str).new()` давало мутную `[E7330] str is a record type, not a value`
> (компилятор ошибочно диагностировал bare-имена типов `str` как
> value-аргументы вызова, вместо распознавания намерения пользователя —
> «тип `(str, str)` как элемент Vec»). Канон требовал многословной формы
> `Vec[(str, str)].new()`.
>
> Вызов пустого литерала как функции — ВСЕГДА ошибка (значение, а не
> функция); эта грамматическая позиция была семантически СВОБОДНА для
> переиспользования. Правило: если непосредственно за пустым `[]` в
> expression-позиции следует СКОБОЧНАЯ тип-форма (`(T1, T2, ...)` —
> кортежный тип, тот же `parse_type()`, что и в любой другой тип-позиции)
> И следом идёт ГЕНУИННОЕ продолжение типа-как-значения (`.` или `{`) —
> парсер трактует ВСЮ конструкцию `[](T1, …)` как `Vec[(T1, …)]`
> (идентичная `TurboFish`-форма, что и явно написанный `Vec[(T1, …)]` —
> переиспользует 100% существующей static-dispatch машинерии, никакого
> нового codegen/typecheck-пути). Lookahead — спекулятивный с rollback
> (симметрично `try_parse_turbofish_args`): если скобочная форма НЕ
> парсится как тип, или после неё нет `.`/`{`, парсер откатывается к
> СТАРОМУ разбору (пустой литерал + вызов) — оригинальная диагностика на
> ДЕЙСТВИТЕЛЬНО некорректном вызове (`ro y = [](str, str)` без
> продолжения) сохраняется байт-в-байт (`E7330`, как и раньше).
>
> Простая (не кортежная) форма `[]T.new()` (bare `Ident` сразу после
> `[]` + `.`) — существующий D38-механизм (`Path(["__array", T])`),
> амендментом НЕ затронута/не переписана — обе формы (`[]T.new()` и
> `[](T1, T2).new()`) канон, независимые пути.
>
> Канонизация существующих `Vec[(`-сайтов (если такие найдутся в других
> пакетах) — ВНЕ объёма этого амендмента (отдельная волна по решению
> владельца). Фикстуры: `spec_tests/conformance/d27_array_type_expr_
> position.nv` (pos — tuple-форма + простой-T регресс + push/index +
> Vec[(...)]-baseline), `spec_tests/conformance/neg/array_type_expr_
> position_neg.nv` (neg — E7330 сохранена для генуинно некорректного
> вызова).

### Что
Массивы записываются **префиксом** (Go-стиль): `[]T` динамический,
`[N]T` фиксированный, `[N1][N2]T` многомерный — порядок размеров
**совпадает с порядком индексации**.

### Правило

```nova
ro xs []int = [1, 2, 3]                // динамический
ro buf [5]u8 = [0, 0, 0, 0, 0]         // фиксированный
ro zeros [4]u8 = [0; 4]                // повторение через ;

ro matrix [2][3]int = [[1, 2, 3], [4, 5, 6]]
matrix[i][j]                             // i: 0..2, j: 0..3 — порядок совпадает

ro opt Option[int] = Some(42)           // generic не меняется
```

Парсер по позиции:
- В позиции типа без имени слева — массив (`[]T`, `[5]T`).
- В позиции типа после имени — generic (`Option[T]`).
- В позиции выражения — индексация (`arr[i]`).

Layout: `[N]T` — N подряд, без указателя. `[]T` — `{ ptr, len, cap }`,
24 байта на 64-bit. `[N1][N2]T` — плоский row-major. `[][]T` —
jagged (массив указателей на массивы).

### Почему

1. **Соответствие индексации** — `[2][3]int` ↔ `arr[i][j]`. В Rust
   `[[T; 3]; 2]` порядок обратный; программисты ошибаются.
2. **Парсер однозначен** — `[` различается по позиции в грамматике.
3. **Чтение слева направо** — «массив 2×3 целых».
4. **Generic не страдает** — `Option[T]` остаётся.
5. **Прецедент Go.**

### Что отвергнуто

- **Java `T[]` / `int[2][3]`** — парсер сложнее, конфликт с `Option[T]`.
- **Rust `[T]` / `[[T; N]; M]`** — обратный порядок размеров, конфликт
  «массив vs generic» одного символа.
- **`[T; N]` для одномерного** — `;` читается странно в многомерных,
  нет соответствия индексации.

### Связь
- [D16](#d16-дженерики-через-t-не-t) — `[T]` теперь только generic-применение.
- [D38](#d38-создание-массивов-и-turbofish-для-дженериков) — static-методы
  на типе массива (`[]T.with_capacity(n)`).
- [02-types.md](02-types.md) — sum/record не конфликтуют по грамматике.

### Эволюция
Старо: `[T]` динамический, `[T; N]` фиксированный — конфликт с
generic. Перешли на Go-style; ~50 мест в документах исправлено.

---

## D440. FixedArray len и ptr аксессоры (Plan 200 П19, 2026-07-21)

**Source:** [Plan 200](../../docs/plans/200-std-improvements.md) Пункт 19. **Status:** ✅ ACTIVE.
**Связь:** [D27](#d27-синтаксис-массивов-t-префикс-nt-фиксированные) (`[N]T` value-класс, амендмент выше),
[D238](#d238-indexk-v-protocol--akey-magic) (`arr[i]` — та же структурная FixedArray-магия),
[D299](02-types.md#d299--asslicet-protocol-contiguous-buffer-abstraction-plan-1531-2026-06-17)
(`@ptr()`/`@len()` пара — Vec/str-прецедент, зеркалируемый здесь).

### Что

`[N]T` (fixed-array value-тип) получает два ресивер-метода, зеркало
Vec/str-поверхности:

```nova
fn [N]T @len() -> int    // компайл-тайм N, O(1), без рантайм-обвязки
fn [N]T @ptr() -> *T     // адрес первого элемента; ro-ресивер
fn [N]T mut @ptr() -> *mut T  // та же форма, mut-ресивер (перегрузка по мутабельности)
```

### Правило

- `arr.len()` — целочисленная константа `N` из типа ресивера `arr`
  (`[N]T`). Не читает никакого рантайм-поля (у `[N]T` нет len/cap
  заголовка, D27-амендмент — inline `T data[N]`).
- `arr.ptr()` — указатель на первый элемент, `*T` (ro, D246 `*T ≡ *ro T`
  default) когда `arr` связан `ro`, `*mut T` когда `arr` связан `mut`
  (перегрузка по мутабельности ресивера — тот же паттерн, что у
  `Vec[T] @ptr()` / `Vec[T] mut @ptr()`, `std/collections/vec/access.nv:262/270`).
  Значение указателя **safe** получить без `unsafe`; разыменование
  (`p.read()`/`p.write(v)`/`p[i]`) требует `unsafe { }` — контракт
  идентичен `Vec[T] @ptr()` / `str @ptr()`.
- **Lifetime (доккомент, НЕ enforcement):** для стекового `[N]T` (локал,
  не-`priv`-поле record'а на куче) вид, полученный через `@ptr()`, не
  переживает скоуп владельца — тот же неформальный контракт, что у
  `Vec[T] @ptr()`'s «pointer invalidated by reallocating mutation»
  (там — realloc, здесь — конец стек-фрейма). Компилятор это НЕ
  проверяет (в языке нет lifetime-анализа/borrow-checker'а); документируется
  как обязательство вызывающего, зеркало существующей практики.
- `@len()`/`@ptr()` — НЕ `unsafe fn` (сама операция safe); `unsafe`
  нужен только на стороне вызывающего для разыменования результата
  `@ptr()`.

### Почему

**Компилятор-синтез, не `.nv`-декларация.** Method-level const-generic
`N` в языке отсутствует — ресивер-форма `fn [N]T @m() { ... }` с `N`
как ссылающимся на РАЗМЕР типа не парсится (проверено пробой: `fn [4]u8
@probe() -> int => 4` → `error: expected identifier, got int literal`
на `4` внутри `[4]`). Написать один `.nv`-текст на ВСЕ `N` невозможно;
писать N деклараций (одну на каждый использованный размер) —
дублирование, D9. Компилятор синтезирует обе операции структурно из
`TypeRef::FixedArray(N, elem)` — тем же механизмом, каким уже резолвится
`arr[i]` (D238-семья, [M-fixed-array-value-semantics] амендмент к D27
выше): чекер извлекает `(N, elem)` напрямую из типа ресивера
(НЕ через `method_table`, никакой `.nv`-сигнатуры не существует), кодоген
распознаёт тот же `_NovaFixArr_<N>_..._<elem>` C-struct-паттерн, что уже
использует `arr[i]`-чтение, и эмитит `N`-литерал / адрес `data[0]`
напрямую. Один резолв-путь для всей FixedArray-поверхности (§3
one-window) — `@len`/`@ptr` не заводят отдельную name-keyed
диагностическую/диспетчерскую ветку в стороне от `@index`.

**Мотив (Plan 200 П19, три реальных упора одной недели):** (1)
pad-оптимизация не могла собраться — `RawMem.copy` из `fill_bytes [4]u8`
не имел указателя-источника; (2) потребители `encode_utf8` писали байты
циклом push вместо копии среза; (3) generic-fallback `@display`
примитива — стек-буфер `[32]u8` был невыразим без каста в холодной
ветке. Обход существовал (`&arr as *T`, прецедент `from_bits`), но это
каст-жест на каждом сайте вместо именованной двери — `@ptr()`/`@len()`
дают ту же дверь, что уже есть у `Vec[T]`/`str`.

### Что отвергнуто

- **`.nv`-декларация с method-level const-generic `N`.** Языка-уровня
  const-generic'ов нет (см. «Почему») — потребовало бы нового
  синтаксиса ради двух accessor'ов; непропорционально.
- **N отдельных `.nv`-деклараций (по одной на каждый встреченный
  размер).** Дублирование без предела — каждый новый `[7]u8`/`[64]u8` в
  проекте требовал бы своей копии тела. Нарушает D9.
- **Enforcement lifetime-контракта у `@ptr()`.** У языка нет
  borrow-checker'а/lifetime-анализа (осознанный компромисс всего
  проекта, не локальное решение здесь) — контракт документируется, как
  у `Vec[T] @ptr()`, а не проверяется статически.

### Реализовано в

`compiler-codegen/src/types/mod.rs` (`peel_fixed_array` +
`fixed_array_accessor_return`, вызываются из `infer_expr_type`'s Call-арма
и `infer_method_call_channel_type` — та же пара продюсеров, что и любой
другой instance-call); `compiler-codegen/src/codegen/emit_c.rs`
(`emit_call`'s `Member`-арм, синтез через существующую
`parse_mono_fixed_array_name` — ту же функцию, что и `arr[i]`-чтение).
Тесты: `spec_tests/conformance/` (standalone) — pos (`.len()`, `.ptr()` +
`RawMem.copy` round-trip, mut-перегрузка запись), neg (bare `arr.len` без
скобок — обычная диагностика, не новая форма). D440 NEW.

---

## D30. Стиль именования

### Что
Один стиль на весь язык: PascalCase для типов и протоколов, snake_case
для функций/полей/локальных, SCREAMING_SNAKE_CASE для констант.
Акронимы — **PascalCase** без исключений.

### Правило

| Что | Стиль | Пример |
|---|---|---|
| Типы, варианты sum-type, эффекты, протоколы | PascalCase | `User`, `HashMap`, `Some`, `Db`, `Hash` |
| Generic-параметры | PascalCase, односимвольные | `T`, `K`, `V`, `E` |
| Функции, методы, поля, параметры, локальные | snake_case | `parse_url`, `@deposit`, `user_id` |
| Константы | SCREAMING_SNAKE_CASE | `MAX_PAYLOAD`, `DEFAULT_TIMEOUT` |
| Модули | snake_case через точки | `module admin.audit` |

Акронимы **PascalCase**, не UPPERCASE:

```nova
type Db effect { ... }          // не DB (эффект — protocol)
type Io effect { ... }          // не IO
type Url str                 // не URL (newtype над str)
type Http effect { ... }        // не HTTP
type JsonValue { ... }       // не JSON (record)
type SqlBuilder { ... }      // не SQL (record с полями)
```

Договорные конвенции имён методов:

| Имя | Когда |
|---|---|
| `T.new(...)` | стандартный конструктор (вкл. из компонентов) |
| `T.of(a, b, c)` | вариадик-литерал элементов коллекции (D259; НЕ пустой — пустой это `new()`) |
| `T.from(v X)` | конструктор-конверсия из X — **имя-конвенция**, не протокол (D73 ретрактирован 2026-07-06) |
| `T.from_X(...)` | **доменный** конструктор (`from_secs`, `from_polar`, `from_imag`) — когда `from(v)` не передаёт смысл |
| `@is_X()` | bool-предикат |
| голое существительное (`bytes()`, `chars()`, `slice()`) | O(1) вид/линза, заём (D410); копия — `.clone()`/`.collect()` на месте вызова |
| `@to_X()` | трансформация в новое владеющее значение (`to_str`, `to_upper`) — вида не существует в принципе (D410) |
| `consume @into_X()` | потребляющая передача владения (`into_str`, `into_raw`) — D131; универсального `v.into()` НЕТ (D73-ретракция) |
| `@hash()`, `@clone()`, `@iter()`, `@next()` | стандартные методы |

Оси семантические (вид / трансформация / потребление) — следуй им; `as_`-префикс упразднён (D410).

**`try_*` / failable pair convention** (D30 §2, Plan 108):

Когда операция может завершиться с ошибкой, определяются **две формы**:

| Форма | Сигнатура | Семантика |
|---|---|---|
| `try_op(...)` | `-> Result[T, E]` | возвращает результат без эффектов; вызывающий сам обрабатывает ошибку |
| `op(...)` | `Fail[E] -> T` | unwrap-обёртка через `!!`; кидает `E` через эффект при провале |

Правило реализации: **`op` реализуется как Nova-body через `try_op`**:

```nova
// Примитив — только эта функция знает как читать байт:
export external fn ReadBuffer mut @try_read_byte() -> Result[u8, ReadBufferError]

// Обёртка — один лайнер на Nova, без дублирования C-логики:
export fn ReadBuffer mut @read_byte() Fail[ReadBufferError] -> u8 => @try_read_byte()!!
```

**Зачем `try_*` первичен:**
- C-логика живёт в одном месте (`try_*`), `*` = тонкая обёртка
- Нет дублирования кода ошибок между парами
- Вызывающий выбирает стиль: `op()` (throw-style) или `try_op()` (result-style)

Применяется везде: `ReadBuffer`, `WriteBuffer`, I/O, парсинг, преобразования типов.

#### Полные слова, не сокращения

Имена методов, типов, параметров и полей — **полные слова**, не
сокращения. Приоритет — читаемость, а не количество символов.

```nova
fn ReadBuffer    @position()  -> int     // не @pos()

fn copy_into(destination []u8) -> ()   // не dest
fn parse(input str) -> Result[T, E]      // не buf, не val
```

**Запрещены ad-hoc сокращения** (mainstream-precedent): `pos`,
`dest`, `src`, `buf`, `val`, `tmp`, `cnt`, `idx` (кроме mainstream-исключений
ниже), `arr`, `len` (кроме mainstream-исключения), `msg` (кроме `Error.msg`
field — закреплено D26), `cfg`, `ctx`.

**Mainstream-исключения** (Rust/Go/Swift convention — слишком устоявшиеся
формы, чтобы менять):

| Сокращение | Где разрешено | Прецеденты |
|---|---|---|
| `len` | длина коллекции (`s.len()`, `arr.len()`; method-only по [D117](#d117-size-like-accessors-require-call-syntax)) | Rust, Go |
| `cap` | capacity-свойство коллекции (`arr.cap()`, `mut arr.cap(n)`; read/write property-pair, D117 AMEND 2026-07-06) — дублирующий `capacity()` РЕТРАКТИРОВАН [M-unwrap-twins-retraction] 2026-07-07 | Go |
| `iter` | итератор (`coll.iter()`, `Iterator`) | Rust |
| `idx` | index — **только в локальных переменных** (`for idx in ...`) | Rust convention |

Ровно четыре исключения, никаких других. Остальные — full word:
`length` если не коллекция-`len`, `capacity` если не свойство-`cap`,
`iterator` если не protocol-имя, `index` если параметр или поле.

**Operator-overloading имена** ([D46](#d46-перегрузка-операторов-через--методы))
— `@plus`, `@rem`, `@neg`, `@shl`, ... — **фиксированы** и **не подчиняются
правилу полных слов**. Это исторически зацементированная convention из
Rust/C++/Swift; менять `@plus` → `@addition` бессмысленно.

**Acronyms** работают по правилу выше (PascalCase в типах, snake_case
в методах: `JsonParser`, `parse_json`). К full-word правилу не относятся.

**Зачем строго:**

1. **AI-friendly.** LLM не должна угадывать когда `pos` это `position`,
   а когда `posix`. Один canonical full word — однозначность.
2. **Code review consistency.** Reviewer видит `dest` и спрашивает «destination
   or destruct?» — лишний cycle. Full word убирает класс багов.
3. **Прецедент Swift API Guidelines.** Swift строго запрещает abbreviations,
   и это даёт API surface, которую читать как естественный язык.

#### Leading underscore: «параметр / биндинг намеренно не используется»

**Конвенция** (Plan 110.7.3.a, 2026-06-01): локальные биндинги и
параметры с префиксом `_` явно сигналят compiler'у «эта переменная
объявлена для интерфейсной совместимости, но не нужна телу». Это
**подавляет** `W_UNUSED_PARAM` / `W_UNUSED_LOCAL` warning'и без
необходимости комментариев.

| Применение | Пример | Семантика |
|---|---|---|
| Unused parameter | `fn @cleanup(_outcome ScopeOutcome) -> ()` | param required by protocol, тело его не читает |
| Unused let-binding | `ro _ = expensive_compute()` | side-effect важен, value irrelevant |
| Unused pattern binding | `match v { Some(_x) => 0, None => 1 }` | wildcard с именем для diagnostic, не reading |
| Discard tuple element | `ro (a, _b) = pair()` | первый нужен, второй — нет |

**Правило компилятора:**
* Имя начинается с `_` (включая чистый `_`) → unused-warning suppressed.
* Любое другое имя → warning fires если binding не читается.
* `_` (одиночное подчёркивание) — традиционная «throwaway» форма; допустимо
  использовать многократно в одном scope (каждое — fresh binding).

**Prior art:**
* Rust: `let _x = compute()` — same convention.
* Swift: `_` parameter labels — call-site suppression.
* Go: `_` blank identifier — same purpose, syntax level.
* Python: `_var` — informal convention, no enforcement.

**Запрещено**: `_` префикс на **public exports** — это signals «private
to module», и leading-underscore tied к unused-suppression cleanly
разделимо только для local / private bindings.

#### Типы ошибок: `Parse<TypeName>Error`, `<Operation><Domain>Error`

Имена ошибок в публичных API должны включать **тип / домен** который
породил ошибку, а не быть generic-словом:

| Стиль | Пример | Прецедент |
|---|---|---|
| `Parse<TypeName>Error` | `ParseIntError`, `ParseComplexError`, `ParseUrlError` | Rust `std`, `num-complex` |
| `<Domain>Error` | `DbError`, `HttpError`, `RepoError` | стандартный backend-стиль |
| `<Operation>Error` | `OverflowError`, `TransferError` | для конкретной операции, не типа |

**Не использовать generic-имена:**

| Плохо | Почему | Лучше |
|---|---|---|
| `ParseError` | коллизии: URL/JSON/datetime/complex/... | `ParseUrlError`, `ParseComplexError`, ... |
| `Error` (как пользовательский тип) | конфликт с prelude `Error` (D65) | конкретное имя |
| `Exception`, `Failure` | пустые слова без домена | по операции / домену |
| `ValueError`, `TypeError` | заимствование из Python — слишком общо | по операции / домену |

**Вариантам внутри sum-типа** доменный префикс не нужен — они уже
живут в namespace своего типа:

```nova
type ParseComplexError enum InvalidFormat | NotANumber

throw InvalidFormat                          // имя варианта без префикса
throw ParseComplexError.InvalidFormat        // полная форма (если ambiguous)
```

Это согласовано с D65 lookup'ом: `throw InvalidFormat` находит
активный `Fail[ParseComplexError]` handler по типу варианта.

Видимость полей record/tuple — через `priv` keyword (Plan 124, D220);
default = public. Convention `_prefix` для conventionally-private
полей **отменена 2026-06-02** в пользу compile-time `priv`
([07-modules.md → D47 amend](07-modules.md#d47)). Для функций/методов
видимость через `export` / приватно ([07-modules.md → D47](07-modules.md#d47)).

Зарезервированные имена для operator overloading: `@plus`, `@minus`,
`@times`, `@div`, `@rem`, `@neg`, `@or`, `@and`, `@xor`, `@shl`,
`@shr`, `@eq`, `@lt`, `@le`, `@gt`, `@ge`, `@not`, `@get`, `@set` —
[D46](#d46-перегрузка-операторов-через--методы).

Test-имена — строки естественного языка: `test "insert and get" { ... }`.

#### Длина имени типа: минимум 2 символа (D30 §naming, Plan 167)

**Запрещены однобуквенные имена типов** (`type T`, `type S`, `type K`, ...).

Таблица в D30 закрепляет `T`, `K`, `V`, `E`, ... как конвенцию для
generic-параметров (`fn[T] ...`). Если объявить `type T { ... }`, компилятор
не может отличить «named type T» от «typevar T» в generic context —
это источник `E_PREFIX_SHADOWS_NAMED_TYPE`. Запрет однобуквенных имён
делает пространства имён непересекающимися.

```nova
type T { ... }       // ✗ E_TYPE_NAME_TOO_SHORT — переименуй в TVal, TNode, ...
type S { ... }       // ✗ E_TYPE_NAME_TOO_SHORT
type Db effect { }   // ✓ — 2 символа
type Ok { }          // ✓ — 2 символа
```

**Error:** `E_TYPE_NAME_TOO_SHORT` (Plan 167, D30 §naming).

### Почему

1. **Одно правило без исключений** для акронимов — программисту и LLM
   не помнить «2 буквы UPPER, 3+ Pascal».
2. **Composability** — `HttpClient`, `JsonParser` читаются без
   «плотностей» из заглавных. Сравни `HTTPClient`, `JSONParser`.
3. **AI-friendly.** LLM плохо угадывает «сколько букв в акрониме» —
   единое правило.
4. **Прецедент.** Swift API Guidelines, современный .NET, Rust.

### Что отвергнуто

- **Java/C# до 2010-х (UPPERCASE для коротких акронимов)** — каша
  на стыке (`parseXMLForJSONFromHTTPResponse`).
- **snake_case для всего (Python)** — типы и значения визуально не
  отличаются.
- **camelCase для функций (Java/JS)** — `to_str` читается лучше
  `toStr`; границы слов чётче.

### Связь
- [07-modules.md → D47](07-modules.md#d47) — `export` / приватно;
  стиль не зависит от видимости.
- [D33](#d33-const-vs-let--compile-time-vs-runtime) — `SCREAMING_SNAKE_CASE`
  для `const`.
- [D46](#d46-перегрузка-операторов-через--методы) — зарезервированные
  имена.

---

## D33. Три оси immutability — `ro`/`mut`/`consume` + `const` + per-field freeze

> **Plan 114 rewrite (2026-05-31)**: эта секция полностью переписана.
> Старая формулировка («`const` vs `let` — compile-time vs runtime»)
> декларировала три оси, но **одна была fake**: ось «`const` = compile-time,
> `let` = runtime» не соответствовала реальности после Plan 14 Ф.2 (расширил
> `const` на non-constexpr RHS через lazy-init `nova_const_<name>()` static
> getter). Plan 114 Ф.9 narrow'ит `const` обратно до strict constexpr-only,
> делая ось «hard compile-time guarantee» правдивой; Ф.10 generalize'ит
> `const` на 3 позиции; `let` retracted. Полный дизайн см. [D184](#d184).

### Что

Nova V2 имеет **три ортогональные оси, все три реальные**:

| Конструкция | Что фиксирует | Позиции | Решает |
|---|---|---|---|
| `ro x` / `mut x` / `consume x` | binding mutability + ownership | module-level (только `ro`) + scope (все три) | можно ли переприсвоить переменную; кто owns |
| `const X = …` | **hard compile-time guarantee** (strict constexpr) | module-level + scope-local + record-field (associated const) | известно ли значение при компиляции; compile-error если не |
| `ro field T` / `mut field T` / `field ro T` | per-field freeze | внутри `type X { … }` | можно ли мутировать конкретное поле в record'е |

### Правило

```nova
// Ось 1: binding mutability + ownership
ro x = 5                            // immutable binding
mut counter = 0                     // mutable binding
counter = counter + 1               // OK — mut
consume sb = StringBuilder.new()    // owned binding (Plan 73.1)

// Ось 2: const — hard compile-time guarantee
const MAX_PAYLOAD = 4096            // ✓ literal
const TIMEOUT_SEC = 60 * 5          // ✓ constexpr arithmetic
const ORIGIN Point = { x: 0.0, y: 0.0 }  // ✓ constexpr record-literal

const COMPUTED = make_point(7, 14)  // ✗ E_CONST_NOT_CONSTEXPR
                                    //   hint: «use `ro` for lazy-init»

// Module-level non-constexpr → ro (заменяет старый let X = …)
ro NOW = Time.now()
ro COMPUTED Point = make_point(7.0, 14.0)

// Ось 3: per-field freeze
type Account {
    ro id u64                       // never-mut, даже у `mut acc`
    balance int                     // default — mut если binding mut
    mut log_count int               // always-mut, даже у `ro acc`
}
```

`const` требует (strict constexpr-only — Plan 114 Ф.9):
- Литералы любого primitive-типа.
- Арифметика/bitwise/comparison над constexpr операндами.
- Record-литерал из constexpr-полей.
- Sum-type конструктор из constexpr args.
- Ссылка на другой `const`.
- Вызов `const fn` с constexpr args (Plan 114 Ф.11; см. [D199](#d199)).

**Не** runtime call, **не** effect, **не** allocation. Для lazy-init
runtime-value используется `ro X = …` на module-level (заменяет старый
`let X = …` host).

### Strict module-level partition

На **module-level** между `const` и `ro` — обязательное разделение по
constexpr-eligibility:

| RHS | Keyword |
|---|---|
| Constexpr-eligible (literal, arithmetic, record-литерал, `const fn` call) | `const X = …` (E_RO_FOR_CONSTEXPR_PREFER_CONST иначе) |
| Non-constexpr (runtime call, effect, allocation) | `ro X = …` (E_CONST_NOT_CONSTEXPR иначе) |

Compiler определяет «constexpr» точно — user не выбирает между `const`/`ro`,
выбирает RHS, keyword следует. Codemod auto-converts в обе стороны.

**Scope-level — без strict-правила.** Внутри fn body `ro x = 5` и `const
x = 5` оба валидны; разница только в гарантиях (`const` = строго constexpr
+ inlined; `ro` = runtime immutable binding).

**Статус реализации (обе стороны — implemented).** Обратное направление
(`E_CONST_NOT_CONSTEXPR` на не-constexpr `const X`) реализовано в Plan 114.4
Ф.1. Прямое направление (`E_RO_FOR_CONSTEXPR_PREFER_CONST` на constexpr-eligible
module-level `ro X`) реализовано в Plan 148 Ф.3 (аменд D199 ниже). Оба
направления используют **один и тот же** предикат constexpr-eligibility
(`check_const_constexpr_ex`), поэтому не могут разойтись в определении
«constexpr». Прямое правило применяется только к **module-level** binding с
**одним именованным** binder'ом (`ro NAME = …`; UPPER_CASE binder парсится как
single-segment unit-variant pattern и трактуется как имя); destructuring
(`ro (a, b) = …`) и scope-local `ro` — не затрагиваются.

### Почему

1. **Compile-time гарантия (восстановлена).** `const` теперь делает то что
   обещает — hard constexpr. После Plan 14 Ф.2 это было размыто; Plan 114
   Ф.9 narrow'ит обратно.
2. **Размеры массивов.** `[N]T` ([D27](#d27-синтаксис-массивов-t-префикс-nt-фиксированные))
   требуют `const N` (теперь N может быть scope-local — Plan 114 Ф.10).
3. **Associated constants.** `type T { const X = … }` — namespace-bound
   constexpr (Java `static final`, Rust `impl T { const X }`, Kotlin
   `companion const val`). См. [D200](02-types.md#d200).
4. **AI-first.** LLM, видя `const X = compute(...)` → compile error
   E_CONST_NOT_CONSTEXPR, получает явный сигнал «используй `ro`».

### Что отвергнуто

- **`let`/`let mut`** — retracted в Plan 114 (D184). Сейчас replaced
  тройкой ro/mut/consume.
- **`:=` (Go)** — дублирует binding declaration; источник shadowing-багов.
- **`final` (Java)** — лишнее ключевое слово.
- **Без разделения** — массивы `[N]T` потребуют литералов всюду;
  comptime станет несовместимым.

### Сравнение с mainstream — см. [D184](#d184) §«Сравнение с mainstream»

### Связь
- [D27](#d27-синтаксис-массивов-t-префикс-nt-фиксированные) — `const N` для `[N]T` (Plan 114 Ф.10: any visible scope).
- [D30](#d30-стиль-именования) — `SCREAMING_SNAKE_CASE` для `const`.
- [D32](02-types.md#d32) — default immutable (`ro` ось 1).
- [D36](02-types.md#d36), [D175](02-types.md#d175), [D176](02-types.md#d176) — `ro`/`mut` field, ось 3.
- [D102](#d102) — default-param values reference `const`.
- [D184](#d184) — master keyword refresh decision (Plan 114).
- [D199](#d199) — `const fn` comptime evaluable.
- [D200](02-types.md#d200) — associated constants.
- [07-modules.md](07-modules.md) — `export const` экспортирует.

---

## D33-LEGACY (archived). `const` vs `let` — compile-time vs runtime

> ⚠ Эта секция — historical record для legacy-codebase reference.
> Plan 114 retracted `let` keyword (D184); design rewritten выше.

### Что (archived)
`const` — для **compile-time констант**, известных при компиляции.
`let` — для **runtime значений** (immutable binding); `let mut` —
mutable. Это два разных ключевых слова, не сахар.

### Правило

```nova
// const — compile-time
const MAX_PAYLOAD = 4096
const TIMEOUT_SEC = 60 * 5            // арифметика над литералами
const GREETING = "hello"

// let — runtime
ro now = Time.now()
ro user = Db.find(user_id) ?? throw UserNotFound(user_id)

// let mut
mut counter = 0
counter += 1
```

`const` требует:
- Compile-time computable: литералы, арифметика, конструкторы
  record/sum-type из const-значений.
- **Не** runtime-вызовы, эффекты, ссылки на не-const.

`const fn` (compile-time функции) — ✅ реализовано в Plan 114.4.2 (D199):
функция с `const`-params + `-> const T` return вычисляется компилятором,
call sites заменяются литералом в AST. См. [D199](#d199-const-fn--comptime-evaluable-functions).
`const NOW = Time.now()` остаётся ошибкой (Time.now() — runtime call,
не const fn).

`const` живёт в data-segment (zero-cost). `let`-объекты — в managed
heap (или на стеке через escape analysis).

### Почему

1. **Compile-time гарантия.** `const` — программист уверен, нет
   runtime-зависимостей.
2. **Размеры массивов.** `[N]T` ([D27](#d27-синтаксис-массивов-t-префикс-nt-фиксированные))
   требуют `const N` для имени.
3. **`const` явно** говорит «в data-segment», не нужно угадывать.
4. **AI-first.** LLM, видя `const X = compute(...)` → compile error,
   получает явный сигнал «используй `let`».

### Что отвергнуто

- **`:=` (Go)** — дублирует `let`; источник shadowing-багов в Go.
- **`final` (Java)** — лишнее ключевое слово рядом с `let`.
- **Без разделения** — массивы `[N]T` потребуют литералов всюду;
  comptime станет несовместимым.

### Сравнение с `ro` / `mut` field — три оси immutability

Nova имеет **три разных** keyword'а связанных с immutability — `let`,
`const`, `ro`/`mut` field. Они **не конкурируют**, потому что
работают на **разных уровнях** программы:

| Конструкция | Что фиксирует | Где живёт | Решает |
|---|---|---|---|
| `let x` / `let mut x` | binding | в функции / scope | можно ли переприсвоить переменную |
| `const X = ...` | compile-time placement | top-level или scope | известно ли значение при компиляции |
| `ro field T` | поле record'а never-mut | внутри `type X { ... }` ([D36](02-types.md#d36)) | можно ли мутировать поле даже у `let mut` binding'а |
| `mut field T` | поле record'а always-mut | внутри `type X { ... }` ([D36](02-types.md#d36)) | можно ли мутировать поле даже у `let` binding'а |

#### `let` / `let mut` — про **binding**

```nova
ro x = 5             // binding x не переприсваивается
mut y = 0         // binding y переприсваивается
y = y + 1
```

Default immutable ([D32](02-types.md#d32)) — `let` без префикса всегда
immutable. `let mut` — явный opt-in в mutable, аналогично Rust
`let mut`, Swift `var`, Kotlin `var`. Программист видит `let mut` —
знает что переменная меняется.

#### `const` — про **compile-time**

```nova
const MAX = 4096                  // compile-time, в data-segment
ro limit = compute_limit()        // runtime, в heap/stack
```

Оба immutable. **Разница** — `const` накладывает требование
compile-time computability (литералы + арифметика над ними +
const-record'ы). `let` принимает любое runtime-выражение.

`const` нужен для:
- Размеров фиксированных массивов: `[N]T` ([D27](#d27)) требует `const N`.
- Compile-time оптимизаций (свёртка, размещение в data-segment).
- Семантической декларации «это всегда константа», не «immutable
  до scope-exit».

#### `ro` / `mut` field — про **поле record'а**

```nova
type Account {
    ro id u64        // поле never-mut, даже у `let mut acc`
    balance money          // поле default — mut если binding mut
    mut log_count int      // поле always-mut, даже у `let acc`
}

mut acc = Account { id: 1, balance: 100, log_count: 0 }
acc.balance = 200          // OK   — поле default + binding mut
acc.id = 999               // ERR  — id ro
acc.log_count += 1         // OK   — log_count mut
```

`ro` / `mut` per-field — это **freeze/unfreeze** конкретного
поля относительно дефолта. Они **не пересекаются** с `let`/`let mut`:
binding управляет «можно ли модифицировать **переменную**», поле
управляет «можно ли модифицировать **конкретное поле в записи**».

Пример где они комбинируются:

| binding | field declaration | можно `acc.field = ...` |
|---|---|---|
| `let acc` | `field T` (default) | ❌ — binding immutable |
| `let acc` | `mut field T` | ✅ — поле always-mut |
| `let acc` | `ro field T` | ❌ |
| `let mut acc` | `field T` (default) | ✅ |
| `let mut acc` | `mut field T` | ✅ |
| `let mut acc` | `ro field T` | ❌ — ro сильнее |

#### Почему **три**, а не одно

Альтернативы и почему они хуже:

1. **Только `let`/`let mut` без `const`** — массивы `[N]T` требовали
   бы compile-time выводимости из `let N = 5`. Компилятор должен
   проводить escape-analysis на каждый `let`, чтобы понять
   const-eligible. Программист не видит явно «это compile-time»,
   а получает компилятор-error при первом нарушении. AI-unfriendly.

2. **Только `let`/`let mut` без `ro`/`mut field`** — потеря
   per-field freeze. Альтернатива — newtype wrappers (`type AccountId(u64)`
   для каждого immutable поля), что ведёт к verbose-коду и потере
   ergonomics (`acc.id.value()` вместо `acc.id`). Cell/RefCell-style
   wrappers (как в Rust) ещё хуже для AI-кодинга.

3. **Только `const`/`ro`** (без `let`/`let mut`) — теряем
   обычные mutable переменные в функциях. Можно через field record'а
   (тип-обёртку `Counter { mut value int }`), но это противоестественно
   для локальных счётчиков.

Это **три разные оси ответственности**, каждая решает свою задачу:
- `let`/`let mut` — **binding mutability** (можно ли переприсвоить).
- `const` — **compile-time vs runtime placement**.
- `ro`/`mut` field — **per-field freeze в record'е**.

### Связь
- [D27](#d27-синтаксис-массивов-t-префикс-nt-фиксированные) — `const`
  для размеров фиксированных массивов.
- [D30](#d30-стиль-именования) — `SCREAMING_SNAKE_CASE` для `const`.
- [D32](02-types.md#d32) — default immutable bindings; `mut` для
  переменных и параметров.
- [D36](02-types.md#d36) — `ro`/`mut` модификаторы полей
  record'а; per-field freeze.
- [07-modules.md](07-modules.md) — `export const` экспортирует.

---

## D34. Pattern-bind в `if`/`while` conditions — unified grammar с match arms

> Status: active (Rust 1:1, 2026-05-27); amended Plan 114 D184 (2026-05-31):
> drop outer `let` keyword; identifier-pattern требует `ro`/`mut`;
> constructor/destructure pattern bare = immutable, `mut` inside;
> `consume` запрещён в conditions; outer-`mut` запрещён.
> AMEND (2026-07-25, реестр 221.1 №95): парсер-энфорс ретракции —
> `if let`/`while let` (Rust-style outer `let`) до этого момента
> тихо принимались парсером наравне с каноном («две двери»); теперь
> дают `E_IF_LET_RETRACTED` со span на `let`. Пишите паттерн напрямую:
> `if Some(x) = expr { ... }` / `while Some(x) = expr { ... }`.
> AMEND (2026-07-25, реестр 221.1 №106, [M-ro-launder-pattern-bind-not-
> enforced]): «bare bindings inside pattern default immutable» (правило 1
> выше) была ДЕКЛАРАЦИЕЙ без энфорса на cross-binding — паттерн-биндинги
> (match arm / `if Some(x) =`) не были заведены в L1 launder-таблицу
> (D246 §ORACLE G), так что `Ok(b0) => { mut b = b0 }` на кучевом payload
> отмывало ro-заморозку МОЛЧА. Чекер теперь заводит bare
> pattern-bind = L1-ro (ту же launder-проверку, что явный `ro x = ...`),
> `mut x` внутри паттерна = mut — см. D246 §ORACLE G AMEND ниже.

### Что

Синтаксис `if pattern = expr { ... }` и `while pattern = expr { ... }`
— pattern matching прямо в условии с локальным binding в scope блока.
Guard-условие через `&&` (Plan 106, D34 amend 2026-06-17).

Pattern grammar **унифицирована** с match-arm patterns: те же правила
(`mut` inside `Some(mut x)`, bare = immutable) работают в обеих позициях.

### Правило

```nova
// Constructor / destructure pattern — bare bindings default immutable
if Some(user) = cache.get(key) { process(user) }
if Some(mut buf) = pool.try_take() { buf.fill(0) }    // mut inside pattern
if (a, b) = pair { use(a, b) }
if { name, age } = user_opt { greet(name, age) }

while Some(item) = queue.pop() { handle(item) }
while Some(mut line) = reader.read_line() { line.trim_in_place() }

// Identifier pattern — REQUIRES `ro` or `mut` (footgun protection)
if ro user = compute_user() { use(user) }              // ✓ explicit ro
if mut counter = init() { counter += 1; … }            // ✓ explicit mut
if user = compute_user() { ... }                       // ✗ E_AMBIGUOUS_IDENT_PATTERN

// Guard через && (Plan 106, D34 amend 2026-06-17)
if Some(x) = cache.get(key) && x > 5 { use(x) }
if ro Some(user) = db.find(id) && user.is_active { process(user) }
while Some(item) = queue.pop() && item.valid { handle(item) }

// else-if
if Some(a) = lookup_a() {
    use(a)
} else if Some(b) = lookup_b() {
    use(b)               // a НЕ доступна
}
```

**Правила:**

1. **Constructor / destructure pattern** — bare bindings inside pattern
   default immutable. `mut` explicit когда нужно (`Some(mut x)`,
   `(mut a, b)`). Consistent с match arms.
2. **Identifier pattern** (`if NAME = expr`) — **обязательно `ro` или
   `mut`**. Иначе `E_AMBIGUOUS_IDENT_PATTERN` (footgun protection:
   bare `if x = compute()` визуально неотличимо от assignment).
3. **`consume` запрещён** в conditions — `E_CONSUME_IN_CONDITION`.
4. **Outer `mut` удалён** — `if mut Some(x)` → use `if Some(mut x)`
   (mut moves inside pattern). Единое правило с match.
5. **Guard-выражение**: после scrutinee допустимо `&&` + bool-expr;
   биндинги паттерна видны в guard. Аналог Rust let-chains (RFC 2497),
   выбрано `&&` вместо запятой (Swift) за очевидность семантики и
   знакомость Rust-аудитории.
6. **`else if`** — корректно для всех форм.

Грамматика:

```
if-expr    := "if" if-cond block ("else" (if-expr | block))?
while-expr := "while" if-cond block
if-cond    := cond-pattern "=" expr ("&&" expr)?  // guard: && expr после scrutinee
            | expr
cond-pattern := ("ro" | "mut") IDENT type_opt
              | constructor-pattern         // Some(...) / None / etc.
              | tuple-pattern               // (a, b)
              | record-pattern              // { name, age }
```

Скоуп: связанные имена доступны **в guard-выражении и в теле блока**.

`?` работает: `if Some(user) = Db.find(id)? { ... }` пробрасывает ошибку
наверх; внутрь блока заходим только при успехе.

### Почему

1. **«Получить и использовать если есть»** без полного `match`-блока.
2. **Unified grammar с match arms** — единое правило (`Some(mut x)`),
   а не два разных (Rust `if let Some(mut x)` vs match `Some(mut x)`).
3. **Footgun protection** — identifier-pattern требует keyword'а
   (Plan 114 D184 §«identifier-pattern protection»).
4. **Условные циклы** — итерация пока паттерн совпадает.
5. **Guard `&&`** — «получить и проверить» без вложенного `if`.

### Что отвергнуто

- **Go-стиль `;`-разделитель** — нарушает D17 «один разделитель — запятая».
- **`:=` оператор** — shadowing-проблемы Go.
- **Smart-cast (Kotlin)** — магия в типе, AI-first против.
- **`if let` (Rust-style outer `let`)** — Plan 114 retracted в пользу
  unified pattern grammar с match arms.
- **Outer `mut` в pattern position** (`if mut Some(x)`) — Plan 114
  retracted; mut goes inside pattern (`if Some(mut x)`).
- **Запятая-chain (Swift-стиль)** — отвергнута в пользу `&&` (Plan 106);
  amend 2026-06-17: добавлен `&&` guard; запятая-chain отвергнута.

### Связь
- [D33](#d33-три-оси-immutability--romutconsume--const--per-field-freeze) — `ro`/`mut` binding mutability.
- [D184](#d184) — master keyword refresh (Plan 114).
- [02-types.md → D17](02-types.md#d17) — pattern matching в `match` (shared grammar).
- [Plan 106](../../docs/plans/106-if-let-chains.md) — chain syntax.

---

## D35. Методы инстанса через `@`, `self` отменён

### Что
Методы инстанса объявляются как `fn Type @method(...)` с **неявным
self**. Поля self — через `@field`. Мутирующий метод —
`fn Type mut @method(...)`. Конструкторы и static — через точку
`fn Type.name(...)`. Ключевое слово `self` отменено.

### Правило

```nova
type Account {
    ro owner str
    _balance money
}

// конструктор / static — через точку, без @
fn Account.new(owner str) -> Account =>
    Account { _balance: 0, owner }

// метод инстанса — через пробел и @, неявный self
fn Account @balance() -> money => @_balance
fn Account @summary() -> str => "${@owner}: ${@_balance}"

// мутирующий — mut перед @name
fn Account mut @deposit(amount money) =>
    @_balance += amount
```

Грамматика:

```
free-fn          := identifier "(" params ")" effects? ("->" type)? "=>" body
static-method    := Type "." identifier "(" params ")" ...
instance-method  := Type ("mut")? "@" identifier "(" params ")" ...
```

После имени типа: `.` → static, `@` или `mut @` → instance.

#### Receiver — любой тип, включая примитивы

Receiver-тип может быть **любым именованным типом**: record, sum, newtype,
unit-тип, protocol — **и встроенный примитив** (`int`, `str`, `bool`,
`f64`, `u8`, ...). Это естественное следствие того, что в Nova
примитивы — обычные типы (D30, D32), просто с lowercase-именами и
особым представлением в runtime.

```nova
// Static method on a primitive — `str` is a regular type.
fn str.from(i int) -> Self => /* ... */

// Instance method on a primitive — used via `value.method()`.
fn int @to_hex() -> str => /* ... */
fn f64 @round() -> int => /* ... */

ro s = str.from(42)            // static via D35
ro h = (255).to_hex()          // instance, parens around literal
ro r = 3.7.round()             // chained on numeric literal
```

Применение: `From[X]` для `str` (D73) — основной механизм
строковой конверсии. Также `int.parse(s str)`, `bool.from(n int)`
и другие фабрики, не требующие отдельного wrapper-типа.

Ограничения: примитивы — **закрытые** типы, программист не может
добавить **новые поля** (нет `type str { ... }` для существующего
`str`). Только методы. Это согласовано с тем, что `extension functions`
в Nova не вводятся (D46): метод определяется один раз в модуле,
владеющем типом-receiver. Для примитивов это **stdlib**: `fn int.method`
определяется только в stdlib-модулях, пользовательский код может
определять методы только на собственных типах.

В теле метода `@field` — единственная форма доступа к self-полю.
`@.field` невалидно. `@` без поля — значение текущего инстанса
(аналог `self`):

```nova
fn Account @copy() -> Account => @
fn Account @send(ch Channel[Account]) => ch.send(@)
```

Вызов методов — **скобки обязательны**:

```nova
ro acc = Account.new("alice")
acc.deposit(100)
ro bal = acc.balance()         // getter, обязательные ()
```

Unbound method value и lambda (bound method value удалён в Plan 132):

```nova
// Unbound: fn-pointer, self передаётся явно.
ro g = Account.@balance         // unbound: fn(Account) -> money
ro v = g(acc)                   // 100

// Closure захватывает receiver — замена bound-форме.
ro f = || acc.balance()         // lambda: fn() -> money
ro v2 = f()                     // 100
```

> **Правило `@name` в теле метода:**
> `@name` без `()` = поле `name`; `@name()` = вызов метода `name`
> (включая cross-method вызов: `@len()` из `@call_len()` корректно эмитирует
> `Nova_T_method_len(nova_self, ...)`).
> Поле и метод с одним именем на одном типе — легально (нет коллизии).
>
> **REMOVED (Plan 132, 2026-06-09):** `obj.@method` (bound method value).
> Замена: лямбда `|| obj.method()` или unbound `Type.@method`.

Generic'и: `[T]` после имени типа (`fn Vec[T] @len()`) и/или после
`@name` (`fn Vec[T] @map[U](f T -> U)`).

### Почему

1. **Минимум строк.** `fn Account.deposit(mut self, ...)` →
   `fn Account mut @deposit(...)` экономит 6-9 символов на метод.
2. **Один смысл `@` — «принадлежит self».** В сигнатуре `@method`,
   в теле `@field`.
3. **Чёткое разделение.** Точка = static (`Account.new`), `@` =
   instance. Программист и LLM видят роль из синтаксиса.
4. **Скобки обязательны** — `acc.balance()` явно вызов, не поле.
   Property-механизмы (C#/Kotlin) делают это невидимым.

### Что отвергнуто

- **`fn Type.method(self, ...)`** — повторяющийся `self` в каждом
  методе и каждом обращении к полю.
- **Property** (`property balance { get; set }`) — невидимое
  «поле или вызов?»; известный источник путаницы в C#.
- **`@` как параметр** (`fn deposit(mut @, ...)`) — `@` приобретает
  два смысла.
- **`fn mut @Type.method`** — `mut` на типе vs на binding'е, разные
  смыслы.
- **`fn Type new(...)` без точки** — расходится с namespace path.

### Связь
- [D32](02-types.md#d32) (если есть) / [05-memory.md](05-memory.md) — `mut`
  семантика mutable-binding'а.
- [D37](#d37-доступ-к-полям-name-для-record-n-для-позиционных-и-кортежей)
  — `@field` / `@N` для self.
- [D38](#d38-создание-массивов-и-turbofish-для-дженериков) — generic
  на типе и методе.
- [D46](#d46-перегрузка-операторов-через--методы) — operator overloading
  через `@`-методы.
- [01-philosophy.md → D1](01-philosophy.md#d1) — методы как часть
  парадигмы `protocols + data`.

### Перегрузка методов

Полная семантика перегрузки методов (по типу аргумента, arity,
mangling, bootstrap-status, ambiguity, disambiguation) — в
[D84](10-overloading.md#d84). Здесь лишь напоминание: метод может
быть перегружен несколькими сигнатурами на одном receiver-типе, резолв
выполняется по статическим типам аргументов.

### Method values (Plan 11 Ф.4)

Методы — first-class values: можно сохранить в переменную, передать
в HOF, вернуть из функции. Три формы:

```nova
type Account { balance int }
fn Account.new(b int) -> Self => Self { balance: b }
fn Account @get() -> int => @balance
fn Account @add(n int) -> int => @balance + n

ro acc = Account.new(42)

// 1. Closure capturing receiver (replaces bound method value removed in Plan 132).
ro f = || acc.get()                     // тип: fn() -> int
ro g = fn(n int) -> int => acc.add(n)  // тип: fn(int) -> int
ro v = f()               // 42
ro r = g(10)             // 52

// 2. Unbound method value: self передаётся явно как первый аргумент.
//    Тип: fn(Receiver, <params>) -> R
ro h = Account.@add      // тип: fn(Account, int) -> int
ro r2 = h(acc, 10)       // 52

// 3. Static method value: обычная свободная функция.
//    Тип: fn(<params>) -> R
ro mk = Account.new      // тип: fn(int) -> Self
ro acc2 = mk(7)
```

#### Семантика

- **Lambda capturing receiver** — lambda closes over the receiver variable.
  Subsequent calls use the captured variable.
- **Unbound** — fn pointer без env'а. Caller обязан передать receiver
  как первый аргумент.
- **Static** — fn pointer без receiver'а вообще.

#### Использование в HOF

```nova
ro nums = [1, 2, 3]
ro negated = nums.map(int.@neg)          // unbound: применяет @neg к каждому
ro total = nums.fold(0, |n| acc.add(n))  // closure-light: добавляет каждый num к acc
```

#### Disambiguation для overloaded methods

Если у метода несколько overload'ов, используется lambda с явными типами аргументов:

```nova
fn Buffer mut @write(s str) -> ()
fn Buffer mut @write(b []u8) -> ()

ro buf = Buffer.new()
ro f1 = fn(s str) -> () => buf.write(s)      // выбор по типу аргумента
ro f2 = fn(b []u8) -> () => buf.write(b)
```

Тип аргумента в lambda однозначно определяет overload.

#### C-runtime представление

Lambda (closure) и unbound — оба используют generic `NovaClosBase` layout:
```c
typedef struct { void* fn; void* env; } NovaClosBase;
```

`fn` указывает на сгенерированный wrapper, `env` — указатель на
struct с captured receiver (для lambda) или dummy struct (для unbound).
Call-site: cast `fn` к нужной сигнатуре, передача `env` + args.

Static method values — bare fn pointer (без env'а) — но в bootstrap
для единообразия тоже оборачиваются в NovaClosBase.

> **Note:** Bound method value (`obj.@method`) removed in Plan 132.
> Lambda `|| obj.method()` is the explicit replacement.

### Self в expression position (D66 расширение, Plan 11 Ф.4.5)

`Self` ранее работал только в **type position** (return type, parameter
type). Plan 11 Ф.4.5 добавляет **expression position**:

```nova
type Account { balance int }

fn Account.with_initial(amount int) -> Self =>
    Self { balance: amount }                  // record literal

fn Account.new() -> Self =>
    Self.with_initial(0)                      // call current type's static
```

Резолюция: `Self` в expression context резолвится в имя текущего
receiver-типа из метода (тот же `current_receiver_type` что для
type-position). Полезно для default → parameterized constructor
chain'ов и DRY.

Прецеденты: Rust `impl Foo { fn make() -> Self { Self::new(2) } }`,
Swift `Self.method()`. D66 расширяется этим Plan'ом 11.

---

## D37. Доступ к полям: `.name` для record, `.N` для позиционных и кортежей

### Что
Доступ к полю / элементу — через точку:
- `obj.name` — поле record по имени;
- `obj.0`, `obj.1` — поле позиционной структуры или кортежа по
  индексу (0-based);
- `@name`, `@0`, `@1` — то же внутри методов инстанса для self.

### Правило

```nova
// record — доступ по имени
ro u = User { id: 1, name: "alice" }
println(u.name)

// позиционная структура — по индексу
type Point(f64, f64)
ro p = Point(1.0, 2.0)
println(p.0)             // 1.0
println(p.1)             // 2.0

// кортежи — то же
ro pair = (1, "alice")
println(pair.0)
println(pair.1)
```

Внутри методов:

```nova
fn Point @magnitude() -> f64 =>
    math.sqrt(@0 * @0 + @1 * @1)

fn Account @summary() -> str =>
    "${@owner}: ${@balance}"
```

Mutation работает по правилам [05-memory.md](05-memory.md) (mut binding +
поле без `ro`):

```nova
mut p = Point(1.0, 2.0)
p.0 = 5.0                // ок
```

Pattern matching как альтернатива:

```nova
match p {
    Point(x, y) => x + y
}
ro (x, y) = p           // деструктуризация ПОЗИЦИОННОЙ формы
```

**Правка 2026-09-04 (решение владельца).** Здесь стояло
`ro Point(x, y) = p`, и эта форма ЗАПРЕЩЕНА: разбор на части не называет тип.
Позиционная форма разбирается как `ro (x, y) = p`, именованная — как
`ro {x, y} = p` (D411). Пример был неверен и в другом смысле: компилятор его
отказывает («refutable pattern in `let`: `Point` is a variant-pattern»), то
есть спека показывала как законное то, чего нет, — замерено окном 274
(`docs/plans/repro/p274-6-v32-step0/f2_paren_ctor_bind/`). Имя типа перед
скобкой остаётся только в `match`, где оно и различает варианты.

Парсер: `.N` после идентификатора или `)` — field access. После
числового литерала точка — только decimal. `1.foo` — ошибка.

### Почему

1. **Точечный доступ** для одного поля без полной деструктуризации.
2. **`.0`/`.1`** — стандарт Rust/Swift, AI-friendly.
3. **Compile-time** проверка границ (в отличие от runtime `obj[i]`).

### Что отвергнуто

- **Только pattern matching** — многословно для простого доступа.
- **Аксессоры (`fst`/`snd`)** — не масштабируются для 3+ кортежей.
- **`obj[0]` (TS array-style)** — конфликт с runtime-индексацией
  массивов.

### Связь
- [02-types.md → D17](02-types.md#d17) — позиционные структуры
  (`type Point(f64, f64)`) объявляются через `()`.
- [D35](#d35-методы-инстанса-через--self-отменён) — `@name` / `@N`
  внутри методов.

---

## D38. Создание массивов и turbofish для дженериков

### Что
Пустые массивы — литералом с annotation или static-методом на типе
массива (`[]T.with_capacity(n)`). Когда inference не справляется —
**turbofish** через те же `[T]` после имени, без Rust'овского `::`.

### Правило

Создание массивов:

```nova
// 1) литерал + annotation
mut buckets []Slot[K, V] = []
ro xs []int = [1, 2, 3]

// 2) inference из контекста
fn first(xs []int) -> Option[int] => ...
ro result = first([])           // [] выводится из аргумента

// 3) static-методы
ro buckets = []Slot[K, V].with_capacity(cap)
ro empty = []int.new()
ro zeros = []u8.filled(0, 1024)
```

Turbofish — те же `[T]`, без `::`:

```nova
fn parse[T](s str) -> Result[T, ParseError] => ...
ro n = parse[int]("42")?            // в Result-возвращающей функции

ro c = Cache[str, int].new()
ro buckets = []Slot[K, V].with_capacity(16)
ro result = m.@get[int]("key")
```

Грамматика — generic-application:

```
generic-application := identifier "[" type ("," type)* "]"
```

Работает для функций, static-методов, конструкторов, instance-методов.

### Почему

1. **Парсер однозначен** ([D16](#d16-дженерики-через-t-не-t)) — `::`
   не нужен. Rust сами признают `::<>` ошибкой дизайна.
2. **Static-методы на типе массива** — тип явный, pre-allocation
   доступна.
3. **Один синтаксис `[T]`** — везде, без специальных операторов.

### Что отвергнуто

- **Rust `::<T>`** — нужен только из-за `<T>`-ambiguity, у Nova её нет.
- **Глобальный `make[T](n)` (Go)** — не вписывается.
- ~~**`Vec[T].new()`** — `[]T` это встроенный синтаксис, не отдельный
  тип `Vec`.~~ **ОТКАЗ СНЯТ АМЕНДМЕНТОМ 2026-08-30 (реестр №840): он был
  вытеснен двумя ACTIVE-блоками и никто этого не записал.**
  [D239](02-types.md#d239) (ACTIVE, Plan 138, 2026-06-10) объявляет `[]T`
  СИНТАКСИЧЕСКИМ ПСЕВДОНИМОМ `Vec[T]` — то есть посылка этого отказа («не
  отдельный тип `Vec`») перестала быть верной; [D232](02-types.md#d232)
  (ACTIVE) даёт `Vec[T].new()` первой строкой таблицы Construction.
  Компилятор согласен с ними, а не с этим отказом: `Vec[int].new()`
  собирается и работает (замер сборкой и запуском 2026-08-30).

  **Почему это записано, а не подправлено молча.** Отвергнутое, ставшее
  нормой, — худший вид расхождения: читатель D38 находит запрет, читатель
  D232 находит канон, и оба правы по своему тексту. Нашло это окно 274 при
  подготовке волны и НЕ стало решать само, сославшись на порядок старшинства
  в `AGENTS.md`, — правильно: разрешение противоречия между источниками не
  его работа. Здесь оно разрешено не вкусом, а тем, что обе стороны
  (позднейшие ACTIVE-блоки и компилятор) уже говорят одно, а этот отказ
  остался единственным несогласным.

### Связь
- [D16](#d16-дженерики-через-t-не-t) — generic через `[T]`.
- [D27](#d27-синтаксис-массивов-t-префикс-nt-фиксированные) — `[]T`
  как тип; static-методы на нём.
- [D35](#d35-методы-инстанса-через--self-отменён) — `Type.method` для
  static.

### Эволюция
[D16](#d16-дженерики-через-t-не-t) уточнён: `[T]` сам по себе не
является типом — только generic-применение к именованной сущности.

**Bootstrap (2026-05-07):** turbofish реализован в codegen-парсере.
Активируется в expression-position через peek-disambiguation: после
`Ident[T1, T2, ...]` смотрим post-`]` token; если это `(` (call),
`.IDENT(` (method-call) или `?` (Try) — это turbofish-узел
(`ExprKind::TurboFish { base, type_args }`); иначе — обычный
Index-доступ. Параллельно с этим, multi-arg внутри `[...]` —
однозначно turbofish (Index не имеет comma). Bootstrap-codegen
прозрачно делегирует TurboFish в `base` (monomorphization идёт по
call-site / receiver-type), но AST сохраняет `type_args` для будущих
этапов inference. Тесты — `nova_tests/types/generics.nv`.

**Plan 98 (закрыт 2026-05-23):** type-argument inference расширена на
generic-параметризованные типы в позиции param. До Plan 98
`infer_type_param_binding` (`emit_c.rs`) выводил `T` только из голого
`T` и `[]T` — `Option[T]` / `Result[T,E]` / пользовательские
`Box[T]`/`HashMap[K,V]` молча игнорировались → каждый generic-helper,
принимающий generic-тип, **требовал turbofish** (`check[int](a)`
вместо естественного `check(a)`). Хуже Rust/Go/TS, где это базовая
unification. Plan 98 конвертировал функцию из associated `fn` в метод
`&self` + добавил три рекурсивные ветки: `Option[T]` (recovery из
`NovaOpt_<sani>` через `novaopt_value_types`), `Result[T,E]`
(`novares_ok_err`), user-generic (через `generic_type_instance_info`).
**Граница (known limitation):** `[]Option[T]` / `[]Result[T,E]`
(массив generic-элементов) пока **НЕ** выводится — codegen эрейзит
element type в `receiver_type_c_ident` (`NovaArray_nova_int*` для
не-примитивов), теряя generic-инфу до inference; отдельный gap, не
scope Plan 98. Тесты — `nova_tests/plan98/`.

### Built-in API для `[]T` (Plan 17 Ф.1, закрывает Q-array-api)

`[]T` — встроенный тип, **не** запись stdlib (`Vec[T]` нет). Граница
между **built-in API** (компилятор знает напрямую) и **stdlib
extensions** (методы добавлены через `fn []T @method` по D35) —
зафиксирована ниже.

**Built-in API — известно компилятору:**

| Категория | API | Семантика |
|---|---|---|
| длина | `xs.len()`, `xs.is_empty()` | `len()` — method-call, zero-cost lowering в `arr->len` (O(1)); `is_empty()` ≡ `len() == 0` ([D117](#d117-size-like-accessors-require-call-syntax)) |
| capacity | `xs.cap()`, `mut xs.cap(n)` | размер выделенного storage'а; `len() ≤ cap()`. Канон `cap()` (D117 AMEND 2026-07-06); дублирующий `.capacity()` РЕТРАКТИРОВАН [M-unwrap-twins-retraction] 2026-07-07 |
| доступ | `xs[i]`, `xs.get(i)` | `[i]` — panic при out-of-bounds (D13); `get(i)` → `Option[T]` |
| мутация | `mut xs.push(v)`, `mut xs.pop() -> Option[T]` | `push` grow при `len() == cap()` |
| итерация | `xs.iter() -> Iter[T]`, `for x in xs { ... }` | `for` — sugar над `.iter().next()` (D58) |
| создание | `[]T.new(cap int = 0)`, `[]T.filled(v T, n int)` | static-функции на типе; `with_capacity` удалён (D372); pre-alloc = `.new(cap)` точный (D372-amend2), chain `.new().cap(n)` тоже легален |

`xs.cap()` — присутствует, но **не часть стабильного API** для
прикладного кода (detail of representation D32). Использование — для
оптимизации pre-allocation; при изменениях representation может
исчезнуть.

**Field-access form (`xs.len`, `xs.cap`, `xs.is_empty` без скобок)** —
запрещена ([D117](#d117-size-like-accessors-require-call-syntax)).
Compiler выдаёт `E_SIZE_ACCESSOR_FIELD`. Для bare `.cap` — diagnostic
подсказывает append `()` (canonical form — `.cap()`).

**Stdlib extensions** (`std/collections/vec.nv` через D35) — то, что
пишется как обычный пользовательский метод:

| Метод | Что делает |
|---|---|
| `xs.map[U](f fn(T) -> U) -> []U` | каждый элемент через `f` |
| `xs.filter(pred fn(T) -> bool) -> []T` | оставить совпадения |
| `xs.fold[Acc](init Acc, f fn(Acc, T) -> Acc) -> Acc` | свёртка слева |
| `xs.any(pred)`, `xs.all(pred)` | bool-предикаты |
| `xs.first()`, `xs.last()` | `Option[T]` head/tail |

Расширяется по необходимости (`contains`, `index_of`, `reverse`,
`sort`, `zip`, `take`, `drop`, `unique`, `enumerate` — добавляются по
запросу use-case'ов; формальный D-block не нужен, любой `fn []T
@method` валиден по D35).

**Слайсинг `xs[a..b]`** — реализовано Plan 96 (см. [D144](02-types.md#d144)).
Поддержаны 5 форм Range: `a..b`, `a..=b`, `a..`, `..b`, `..` (Rust
`RangeBounds` parity). Возвращает sub-slice view (`cap == len`, push →
realloc → silent detach). OOB → `panic` (D13).

**Embed `use []T`** — допустим по D39 (имя поля обязательно):

```nova
type Holder[T] {
    use data []T
    extra str
}
ro h = Holder[int] { data: [1, 2, 3], extra: "info" }
ro n = h.len()           // прокси к data.len() (D117 method-only)
h.push(42)                // прокси к data.push
```

Подробно — Plan 17 Ф.1, [Q-array-api](../open-questions.md#q-array-api)
(closed), [02-types.md → D39](02-types.md#d39) (use-delegation).

---

## D40. Тело функции: `=>` для одного выражения, `{}` для блока

### Что
Два **взаимоисключающих** способа задать тело именованной функции:
`=> expr` (ровно одно выражение) или `{ stmt; ...; expr }` (блок).
Общий закон: **`=>` и `{}` не сочетаются**. Распространяется на `fn`
(named и closure-full), handler-method.

**Closure-light (`|x| body`)** — отдельная грамматика
([D22](#d22-closure-light--и-full-fn)): тело — bare expression ИЛИ
block, **без `=>`**. D40 к ней не применяется.

**Единственное исключение — match-arm** ([D19](#d19-match-arms-через--не--)):
arm может быть `pattern => expr` или `pattern => { block }` (Rust-стиль).
Причина исключения — `=>` гарантирован как маркер «начало результата»
после pattern'а с возможным `if`-guard'ом, поэтому терять его в блок-форме
нельзя.

Indentation **не значим**.

### Правило

```
fn-decl       = 'fn' name '(' params ')' [effects] ['->' type] body
closure-full  = 'fn'      '(' params ')' [effects] ['->' type] body
body          = '=>' expression | block
block         = '{' { statement } [ expression ] '}'
closure-light = '|' params? '|' (expression | block)              // без =>
match-arm     = pattern [ guard ] '=>' ( expression | block )     // исключение
```

Везде, где есть `=>` (named fn, closure-full, handler-method), после
него идёт **ровно одно выражение**. Ни `fn f() => { ... }`, ни
`fn f() { => x }`, ни `fn(x) => { stmt; expr }` — запрещены.
Closure-light `=>` вообще не использует.

Симметрия по контекстам:

| Контекст                       | `=> expr` | `{ block }` | `=> { block }` |
|--------------------------------|-----------|-------------|----------------|
| `fn name(...)` (named fn)      | ✅         | ✅           | ❌              |
| `fn(...)` (closure-full)       | ✅         | ✅           | ❌              |
| `\|...\|` (closure-light)        | ❌ (нет `=>`) | ✅       | —              |
| Match-arm                      | ✅         | —           | ✅ ([D19](#d19-match-arms-через--не--)) |
| Handler-method                 | ✅         | ✅ (без `=>`) | ❌            |

Если нужно несколько statement'ов:
- для `fn` (named) и closure-full — блок-форма `{ stmt; ...; expr }`;
- для closure-light — block-форма прямо в `|x| { stmt; expr }`
  ([D22](#d22-closure-light--и-full-fn));
- для match-arm — `pattern => { stmt; expr }` ([D19](#d19-match-arms-через--не--));
- для handler-method — блок-форма без `=>`: `op(p) { stmt; expr }`
  ([04-effects.md → D31](04-effects.md#d31)).

```nova
// expression-body
fn double(x int) -> int => x * 2
fn HashMap[K, V].new() -> HashMap[K, V] =>
    HashMap[K, V].with_capacity(16)        // одно выражение, перенесённое

// block-body
fn next_pow2(n int) -> int {
    if n <= 1 { return 1 }
    mut p = 1
    while p < n { p *= 2 }
    p
}
```

Многострочный `match`/`if` — это **одно выражение**, поэтому `=> match {...}`
и `=> if {...} else {...}` остаются легальными:

```nova
fn classify(n int) -> str => match n {
    0           => "zero"
    n if n > 0  => "positive"
    _           => "negative"
}
fn abs(x int) -> int => if x < 0 { -x } else { x }
```

Граница: появилось ли что-то **кроме самого выражения** (statement,
`let`, `return`, `for`, `while`)? Тогда нужен `{ block }`.

```nova
// НЕ ОК — `let` это statement, `=>` ожидает одно выражение
fn area(r f64) -> f64 =>
    ro pi = 3.14
    pi * r * r

// ОК — блок-форма
fn area(r f64) -> f64 {
    ro pi = 3.14
    pi * r * r
}
```

### Почему

- **Один общий закон.** `=>` означает «ровно одно выражение после»
  для лямбд, тела `fn`, handler-method. Match-arm — единственное
  исключение, оправданное необходимостью гарантированного маркера
  «начало результата» после pattern'а с возможным `if`-guard'ом
  ([D19](#d19-match-arms-через--не--)).
- **Indentation-significant грамматика** ломает copy-paste, плохо
  переживает auto-format (Python-стиль отвергнут).
- **Парсер сложнее** при значимых отступах.
- **AI-инструменты** часто переформатируют код — невидимая разница
  становится багом.
- **Явные `{}`** — ноль двусмысленности для форматера, линтера, LSP.
- **Граница `fn` vs лямбда видна по форме.** Блок-тело может иметь
  только `fn name(...) { ... }`, [trailing-block](#d43-trailing-block--без-params-fnp-body-с-params)
  и [handler-method](04-effects.md#d31). Лямбда — никогда.

### Что отвергнуто

- **`=> indented-block`** (F#/OCaml/Python-стиль) — indentation-significant.
- **Только `{}`** для всех тел — теряется компактная expression-body.
- **`{}` после `=>`** (Kotlin/JS-стиль `(x) => { ... }`) — два маркера
  для одного, размывает границу «выражение vs блок».
- **Сочетание `=>` и `{}` для лямбд при запрете для `fn`** —
  непоследовательно: общий закон должен работать одинаково для всех
  «безымянных» и «именованных» функций. Match-arm имеет особую
  природу (всегда требует `=>` как маркер) и потому делает исключение.

### Связь
- [D22](#d22-closure-light--и-full-fn) — closure-light `|x|` имеет
  отдельную грамматику (bare expr или block, без `=>`); closure-full
  `fn(...)` подчиняется D40 как named fn.
- [D19](#d19-match-arms-через--не--) — match-arm: `pattern => expr`
  или `pattern => { block }` (единственное исключение из правила
  «`=>` и `{}` не сочетаются»).
- [D23](#d23-return--только-для-раннего-выхода) — guard-clauses
  через `return` требуют блок-формы.
- [D43](#d43-trailing-block--без-params-fnp-body-с-params) — trailing-block (без
  params) — `f(args) { block }`; trailing-fn (с params) — `f(args) fn(p) body`.
- [04-effects.md → D31](04-effects.md#d31) — handler-method
  имеет две формы (`=> expr` или `{ block }`), как `fn`.
- [D45](#d45-inferred-return-type-для-expression-body) — inference
  работает только на expression-body.
- [D49](#d49-statement-separator-и-парсинг-выражений) — `{}` правит
  newline-разделители.

### Эволюция
Ревизия (2026-05-10): правило «`=>` и `{}` не сочетаются» больше не
применяется к closure-light (`|x|`), у которой своя грамматика без `=>`.
Изначально правило покрывало «лямбды» как единый класс; после
перехода на two-level closure ([D22](#d22-closure-light--и-full-fn))
«лямбды» расщепились на closure-light (отдельная грамматика) и
closure-full (`fn(...)`, подчиняется D40 как named fn).

---

## D43. Trailing: `{ block }` без params, `fn(p) body` с params

### Что
Если последний параметр функции — функционального типа, аргумент-функция
может быть вынесен **за `()` вызова** в одну из двух форм:

- **trailing-block** — `f(args) { block }` — для callback'ов **без
  параметров** (DSL-форма: `with_timeout`, `retry`, `transaction`).
- **trailing-fn** — `f(args) fn(params) body` — для callback'ов
  **с параметрами**. Синтаксис идентичен closure-full
  ([D22](#d22-closure-light--и-full-fn)) без имени.

Скобки `()` вызова всегда обязательны; trailing-форма должна начинаться
на той же строке, что `)`.

`|...|` (closure-light) **в trailing-position запрещён** — для
callback'ов с params используется `fn(...)`, иначе ambiguity с
binary `|`. Closure-light с параметрами передаётся через args:
`f(|x| body)`.

### Правило

```nova
// trailing-block — без параметров (DSL)
with_timeout(2.seconds) {
    Db.exec(sql`UPDATE counters SET v = v + 1`)
}

retry(3) {
    Net.get(url)
}

transaction(db) { ... }

// trailing-fn — с параметрами; обе формы тела
list.filter() fn(x) => x > 0                            // expr-body
list.fold(0) fn(acc, x) { acc + x }                      // block-body
list.map() fn(s str) Fail -> int { parse(s)? }           // typed + effects

// closure-light — в args, не в trailing
list.filter(|x| x > 0)
list.fold(0, |acc, x| acc + x)
```

Грамматика:

```
call           = primary '(' args ')' [ trailing ]
trailing       = trailing-block | trailing-fn
trailing-block = '{' block-body '}'
trailing-fn    = 'fn' '(' params ')' [ effects ] [ '->' type ] body
body           = '=>' expression | block
block-body     = { statement } [ expression ]
```

Trailing-fn идентична closure-full ([D22](#d22-closure-light--и-full-fn)).
Параметры пишутся как у named fn — `(x int, y int)`, типы опциональны
если выводятся из ожидаемой сигнатуры callee.

Правила:
1. **`()` обязательны** — trailing должен следовать сразу после `)`.
2. **На той же строке** — для trailing-block `{` сразу после `)`;
   для trailing-fn `fn` сразу после `)`. Перенос строки между ними
   запрещён.
3. **Тип последнего параметра — функциональный.** Иначе type error.
4. **Один trailing на вызов.**
5. **`|...|` (closure-light) в trailing-position запрещён** — пишется
   `fn(...)` или передаётся через args вызова.
6. **Trailing-block — без параметров.** Если callback требует параметры
   — использовать trailing-fn (`fn(p) ...`) или закрытие в args.
7. **Implicit `it` запрещён** — параметр всегда именован.
8. **Method chain** — те же правила: `list.filter() fn(x) => x > 0`.

> **`spawn` — исключение.** `spawn` — keyword-конструкция, не вызов
> функции, поэтому не подчиняется D43. Его синтаксис: `spawn expr`,
> где `expr` — любое выражение: вызов функции (`spawn foo()`), блок
> (`spawn { body }`), и т.д. `spawn() { body }` — **запрещено**
> (пустые скобки без смысла вводят в заблуждение).

Дисамбигуация с record-литералом:

```nova
ro u = User { name: "alice" }                  // record (имя типа, без ())
fn_call(arg) { name: "alice" }                  // trailing-block (после `)`)
fn_call(arg) fn(x) => x.value                    // trailing-fn
fn_call(arg, User { name: "a" })                // record внутри args
```

Многие language primitives становятся обычными функциями stdlib:

```nova
fn with_timeout[T](dur Duration, body fn() -> T) Fail -> T
fn transaction[T](mut db Db, body fn() Db Fail -> T) Db Fail -> T
fn retry[T](attempts int, body fn() Fail -> T) Fail -> T
```

Keyword-блоки **остаются** (без `()`): `with X = h { ... }`,
`parallel for x in xs { ... }`, `region { ... }`, `match`/`if`/`for`/`while`.
Различие с trailing — наличие `()`.

### Почему

1. **`()` обязательны** — локальный парсер без type-directed parsing.
   Kotlin/Swift вынуждены смотреть на тип, чтобы различить trailing
   и record-литерал.
2. **trailing-fn = closure-full без имени.** Симметрия — программист
   учит одну грамматику параметров. Парсер коммитится за `fn`-keyword
   после `)`, никаких ambiguity.
3. **Closure-light не в trailing.** `func() |x| body` создавал
   ambiguity с binary `|` в expression-position. Запрет даёт парсеру
   мгновенный ответ: `|...|` → closure-light в args; `fn(...)` после
   `)` → trailing-fn; `{...}` после `)` → trailing-block.
4. **Trailing-block — DSL-ниша.** Для `with_timeout`/`retry`/`transaction`
   нет параметров callback'а, и `{ block }` визуально маркирует
   «здесь начинается тело DSL'а».
5. **Не closure-литерал внутри `()`.** Closure-light с params
   передаётся через args (`f(|x| ...)`), trailing — для последнего
   функционального параметра. Программист выбирает по форме (длина
   тела, наличие `let`'ов).

### Что отвергнуто

- **Опциональные `()`** (Kotlin) — нет локального способа развести
  с record-литералами.
- **`()` опционально в method chain** — лишнее исключение.
- **Implicit `it`** — нелокальный reasoning.
- **`do { body }` keyword** — лишнее ключевое слово.
- **Indentation-significant** — конфликт с [D40](#d40-тело-функции--для-одного-выражения--для-блока).
- **Trailing-block = лямбда** (до 2026-05) — переклассифицировано в
  самостоятельную грамматику.
- **Trailing-block с параметрами через `{ x => body }`** (до 2026-05-10) —
  заменено на trailing-fn (`fn(x) ...`) для симметрии с closure-full.
- **Trailing closure через `|x|`** — `func(args) |x| body` создавал
  ambiguity с binary `|` в expression-position; `fn(...)` решает за
  один токен.

### Связь
- [D22](#d22-closure-light--и-full-fn) — closure-light в args через
  `|x|`; trailing-fn идентична closure-full без имени.
- [D40](#d40-тело-функции--для-одного-выражения--для-блока) —
  trailing-fn body подчиняется правилу `=>` ↔ `{}` как named fn;
  trailing-block — block-only (без `=>`).
- [04-effects.md](04-effects.md) — handler-блоки `with X = h { ... }`
  — keyword-блок, не trailing.
- [06-concurrency.md](06-concurrency.md) — `parallel for`, `supervised`,
  `race`, `select` — keyword-блоки.

### Эволюция
Ревизия (2026-05): переименование «trailing-lambda» → «trailing-block».
Раньше форма `f(args) { params => body }` называлась лямбдой и
конфликтовала с правилом «лямбда = одно выражение». Тогда же
переклассифицировано в самостоятельную грамматику.

Ревизия (2026-05-10): trailing расщеплён на **trailing-block** (без
params, для DSL) и **trailing-fn** (с params, через `fn(...)`). Старая
форма `f(args) { x => body }` отменена. Триггер — переход closure
на two-level (`|x|` + `fn(...)`, [D22](#d22-closure-light--и-full-fn));
старая форма с `=>` внутри `{}` после `)` создавала путаницу с новым
правилом «`=>` не используется в closure-light». Симметрия trailing-fn
↔ closure-full даёт парсеру и программисту одно правило вместо двух.
Migration: ~10 примеров trailing с params в spec/.

---

## D44. Числовые литералы

### Что
Полный набор числовых форм; `_` как разделитель между цифрами;
default — `int` для целых, `f64` для дробных. **Type-suffixes
(`100u32`, `1.5f32`) отвергнуты** — type через annotation или `as`-cast.

### Правило

```nova
// целые: десятичные / hex / binary / octal
1
1_000_000_000
0xFF             0xFF_FF_FF_FF
0b1010_0001
0o755

// float
1.5              1_234.567_89
1e10             1.5e-3            1_000.5e6

// type через cast или аннотацию
ro x i32 = 100
100 as u8
0xFF as u32
```

Default-типы: `int` (= `i64` на bootstrap per [D129](02-types.md#d129);
future-arch may become platform-pointer-width signed) для целого,
`f64` для дробного. Контекст (annotation, тип параметра, тип поля)
переопределяет с **hard compile-time range-check** (см. [D227](#d227)
для range-policy, no narrow-fallback, negative-в-unsigned правил):

```nova
ro x u8 = 200             // 200 это u8
fn write(b u8) -> () => ...
write(0xFF)                // 0xFF это u8
ro arr []f32 = [1.0, 2.0]            // annotation-контекст: литералы → f32
ro v = Vec[f32].from([1.0, 2.0])     // param-тип `from(arr []f32)` → литералы → f32
ro w = Vec[f32].of(1.0, 2.0)         // вариадик `of(...args []f32)` → то же
```

> **Impl-note (Plan 154.1, 2026-06-14):** приведение числового литерала к
> элементному float-типу из контекста для **array-литералов** (`[]f32`/`Vec[f32]`)
> было codegen-дырой — литералы строили `[]f64`, чьи биты реинтерпретировались как
> f32 → мусор. Исправлено: `try_emit_typed_vec_literal` берёт float-hint когда ВСЕ
> элементы — числовые литералы (`FloatLit`/`IntLit`); turbofish-static арг-array-
> литерал эмитится с param-C-типом. Гард: только всё-литеральные массивы. Переменная
> f64 в `[]f32`-контексте (`Vec[f32].from([f64_var])`) — НЕ сужается молча: это неявное
> сужение f64→f32, запрещённое для не-литералов (как `as`-cast narrowing, [D54](#d54)) →
> громкая ошибка `E_ARRAY_ELEM_NARROW` (нужен явный `as f32` на элементе). Маркер
> `[M-154.1-f32-literal-coercion]` ✅ (pos+neg тесты `plan154_1`).

Разделитель `_` — **только между цифрами**. Запрещено: в начале
(`_1`), в конце (`1_`), подряд (`1__0`), сразу после префикса
(`0x_FF`), вокруг точки (`1_.5`), вокруг `e` (`1_e10`).

Regex:

```
decimal-int = [0-9] (_? [0-9])*
hex-int     = "0x" [0-9a-fA-F] (_? [0-9a-fA-F])*
binary-int  = "0b" [01] (_? [01])*
octal-int   = "0o" [0-7] (_? [0-7])*
float       = decimal-int "." decimal-int (("e"|"E") ("+"|"-")? decimal-int)?
            | decimal-int ("e"|"E") ("+"|"-")? decimal-int
```

### Почему

1. **Без suffixes — меньше шума.** `100u32`, `0xFFu8`, `1.5f32` хуже
   `100 as u32`. `let x u32 = 100` уже работает через inference.
2. **Тренд новых языков** (Swift, Go, Zig) — без суффиксов.
3. **AI-friendly** — меньше форм записи.
4. **`int` платформенно** — компромисс между Rust (фиксированный) и
   Python (bigint).
5. **`_` строгий regex** запрещает мусор (`1__0`, `_1`).

### Что отвергнуто

- **Type-suffixes (`100u32`, `1.5f32`)** — шум, дублирование с
  annotation, прецедент новых языков против.
- **Свободные `_`** — хочется без `1__0` и `_1`.
- **`'` как разделитель (C++14)** — экзотический выбор, `_` стандарт.

### Связь
- [D27](#d27-синтаксис-массивов-t-префикс-nt-фиксированные) — литералы
  длин массивов берут тип `int`.
- [D33](#d33-const-vs-let--compile-time-vs-runtime) — литералы в `const`.
- [D40](#d40-тело-функции--для-одного-выражения--для-блока) — литералы
  в expression-body.

### Строковые литералы и интерполяция `${expr}`

Строковый литерал `"..."` хранит **UTF-8 байты** (тип `str`). Внутри
литерала разрешена **интерполяция** через `${expr}` (D-string-interp,
закрыт в Plan 17 Ф.1):

```nova
ro name = "alice"
ro age  = 30
ro s = "Hello, ${name}, you are ${age}"   // → "Hello, alice, you are 30"
```

**Семантика — sugar над `+` и `str.from(...)`** (D73 [Into]). Литерал
с N интерполяциями развёртывается в N+1 литеральных частей и N
выражений:

```nova
"a${x}b${y}c"
// = "a" + str.from(x) + "b" + str.from(y) + "c"
```

Каждое выражение `${expr}` должно иметь тип, удовлетворяющий
`Into[str]` (через D73 это автоматически верно для `int`, `f64`,
`bool`, `str`, `char`, `Option[T]` где `T: Into[str]`, и любых
user-типов с реализованным `From[Self] for str` или `Into[str]`).

**Escape для буквального `${`** — обратный слэш: `"price: \${value}"`
печатает `${value}` без интерполяции.

**Multi-line.**

> **Амендмент (владелец, 2026-08-29): сырой newline внутри `"..."` РЕТРАКТИРОВАН**
> — решение «принимаем исследования», по
> [2026-08-29-string-literals-proposal.md](../../docs/dev/research/2026-08-29-string-literals-proposal.md),
> нормативно оформлено в D467 §4. До этой даты абзац разрешал «сырой newline
> между `"..."`», и оракул этому следовал. Причина ретракции — диагностика:
> при разрешённом сыром переносе забытая закрывающая кавычка молча съедает
> следующие строки, и ошибка указывает в место, не имеющее отношения к
> опечатке. **Hard error `E_STR_RAW_NEWLINE`.** Сообщение называет обе замены.
> **Канон.** Длинный текст без переводов строк — продолжение строки `\` перед
> переносом (D467 §3). Настоящий многострочный текст — backtick (D467 §5–§7).
> `\n` внутри литерала работает как работал.

Пустое выражение `"${}"` — **compile error**.

```nova
// Что разрешено
ro v = "x = ${1 + 2}"             // sub-expression — ok
ro v = "user = ${user.name()}"    // method call — ok
ro v = "${a}${b}"                 // соседние интерполяции — ok
ro v = "literal \${name}"         // escape — буквальное "${name}"

// Что НЕ работает
ro v = "${}"                      // ✗ пустое выражение
ro v = "${ro x = 1; x}"          // ✗ statement, не выражение
```

**Bootstrap status (2026-05-08):** ✅ реализовано в lexer/parser/codegen
(Plan 17 Ф.4):

- **Lexer** видит `\$` как escape — сохраняет sentinel-байт `\x01$`
  (SOH+`$`), чтобы парсер мог отличить literal-`${` от
  interpolation-`${`.
- **Parser** разворачивает `TokenKind::Str(s)` в expression-position в
  `ExprKind::InterpolatedStr { parts: Vec<InterpStrPart> }`. Каждое
  `${expr}` парсится через sub-Lexer + sub-Parser; balanced `{}`
  внутри expr поддерживается. Пустое `${}` — compile error.
- **Codegen** эмитит цепочку StringBuilder с pre-size estimate:
  `Nova_StringBuilder_static_with_capacity(N)` →
  `Nova_StringBuilder_method_append_str(...)` per fragment →
  `Nova_StringBuilder_method_into(sb)`. Одна аллокация на итоговый
  buffer; нет O(N²) от цепочки `+`. Per-fragment dispatch по типу:
  `nova_str` pass-through, `nova_bool` → `nova_bool_to_str`,
  `nova_f64` → `nova_f64_to_str`, `CharLit` → `nova_char_to_str`
  (UTF-8 encode), user-тип с `@into() -> str` (D73) — `Nova_T_method_into`,
  fallback `nova_int_to_str`.
- **Interp** (для тестов и `nova run`) — обычная конкатенация через
  `format!("{}", value)`.
- **Const-инициализатор**: интерполяция запрещена (требует runtime
  StringBuilder); compile error «not allowed in const initialiser».

Тесты — `nova_tests/types/string_interpolation.nv` (13 тестов, все
PASS): int / negative int / str / bool / f64 / char-литерал /
multi-interpolation / expression в `${}` / escape `\${` / большие
строки через StringBuilder.

В `tag\`...\``-литералах ([D48](#d48-tagged-template-literals)) tag-функция
получает части и аргументы раздельно — для них интерполяция работает
по той же грамматике `${expr}`, но обработка идёт user-функцией.

**Связь:** [D48](#d48-tagged-template-literals) (tagged templates —
raw-строки `tag\`...\`` без интерполяции по такой же грамматике
`${expr}`, но обработка зависит от tag-функции),
[08-runtime.md → D73](08-runtime.md#d73) (`str.from` через
`From`/`Into`), [08-runtime.md → D26](08-runtime.md#d26) (`str` тип
+ конкатенация).

### Format spec extension (Plan 91.14 D229)

`${expr:SPEC}` — Plan 91.14 D229 extension к base interp-string grammar. V1 supports `:?` (debug format via Debug.@debug). См. [D229](02-types.md#d229-Debug-protocol--format-spec-expr) для подробного описания + 3 новых error codes (E_FORMAT_SPEC_UNKNOWN / E_FORMAT_SPEC_EMPTY / E_FORMAT_SPEC_TRAILING). Future extensions (`:hex`, `:pad-N`, `:.3`) — [M-91.14-format-dsl-extensions] followup.

### D44-амендмент (2026-07-25, Plan 102, реестр 221.1 №102): вложенные строки/кавычки внутри `${...}` — легальны (PEP 701/JS-путь)

**Проблема (было).** Строковый сканер лексера был одномерным: искал первую
неэкранированную `"`, НЕ отслеживая `${}`-глубину. Разбиение на интерполяции
(`desugar_string_interpolation`, `parser/mod.rs`) было постфактум-проходом
поверх уже вырезанной строки, с наивным `{`/`}`-счётчиком (тоже
string-неосведомлённым). Итог: `"hello, ${req.param("name")}"` и ТИПОВОЙ
`"${m["key"]}"` (индекс, D238 `@index`) ловили «unterminated interpolation»
или мусор — внутренняя `"` (внутри аргумента метода / индекс-ключа) закрывала
ВНЕШНЮЮ строку раньше времени.

**Норма (стало).** `${...}` — это Nova-**выражение**, а не строковое
содержимое. Внутри него легально встречаются:
- вложенные строковые литералы, включая индекс со строковым ключом
  (`"${m["key"]}"`) и метод со строковым аргументом
  (`"${req.param("name")}"`) — БЕЗ экранирования внутренних кавычек;
- вложенная интерполяция внутри такой вложенной строки
  (`"${f("x ${y} z")}"`), рекурсивно, любой глубины;
- `{`/`}` record-литералов и блоков внутри выражения (`${ if c { a } else { b } }`).

Escape `\${` (буквальный `${` без интерполяции) — без изменений.
Пустое `${}` — по-прежнему compile error.

**Реализация.** Единый string/brace-aware сканер `scan_interpolation_body`
(`compiler-codegen/src/lexer/mod.rs`) — стек режимов (string-mode /
expr-mode с brace-depth), находит истинную парную `}` для `${`, проходя
сквозь произвольно вложенные строки/скобки/интерполяции. Используется ДВАЖДЫ
одним и тем же кодом (не два дубля):
1. Лексером (`lex_string`) — чтобы найти истинный конец строкового литерала,
   содержащего `${...}` с вложенными строками (иначе внутренняя `"` обрывала
   бы строку раньше времени, старый баг). Диапазон `${...}` копируется в
   токен СЫРЫМ (без escape-декодирования) — это Nova-исходник, а не
   строковое содержимое; его decode/re-lex делает шаг 2.
2. Парсером (`desugar_string_interpolation`) — для сплита уже вырезанной
   строки на литерал/expr-части (заменяет старый наивный `{`/`}`-счётчик).

Диагностика реально незакрытой интерполяции (`"abc ${x` — EOF без парной
`}`) теперь ловится на **лексер**-уровне, с точным span'ом на `${`, начавший
интерполяцию: `unterminated interpolation (started here): ... `${` has no
matching `}` before end of string/file`.

**Связь:** база — этот же D44 (`### Строковые литералы и интерполяция
${expr}`), [D238](#d238) (`Index[K,V]` — `a[key]` → `a.@index(key)`),
[D258](#d258-формат-спеки-в-интерполяции--rust-style-mini-language) (формат-спек
после `:` — не затронут этим амендментом). Позитивные фикстуры —
`spec_tests/conformance/interp_nested_strings.nv`; негативная —
`spec_tests/conformance/neg/interp_unterminated_neg.nv`.
См. также [docs/guide/strings.md](../../docs/guide/strings.md#nested-strings-inside--plan-102-d44-amendment).

---

## D45. Inferred return type для expression-body

### Что
В **expression-body** (`=> expr`) тип возврата `-> T` **опционален** —
выводится из тела. В **block-body** (`{ ... }`) `-> T` обязателен,
если тип не unit.

### Правило

```nova
// expression-body — -> T опционален
fn double(x int) => x * 2                          // -> int выведен
fn Duration @as_nanos() => @nanos                  // -> i64 выведен
fn Duration @is_zero() => @nanos == 0              // -> bool выведен
fn HashMap[K, V] @len() => @count                  // -> int выведен

// block-body — -> T обязателен
fn next_pow2(n int) -> int {
    if n <= 1 { return 1 }
    mut p = 1
    while p < n { p *= 2 }
    p
}

fn process() {                                     // -> () можно опускать
    Log.info("hello")
}
```

Inference локальный (по одной функции, одному выражению), не Hindley-Milner:
- литерал → его тип; `@field` → тип поля;
- вызов → тип возврата вызываемого; record-литерал `T { ... }` → `T`;
- match/if-else → unification веток.

Style-guide:
- **`export` функции** — писать `-> T` явно (линтер предупреждает).
- **Сложные match'и** — писать явно.
- **Generic-функции** — связь параметра с возвратом полезно видеть.
- **Простые геттеры/предикаты/конструкторы** — опускать.

### Почему

1. **Compact form для тривиальных методов** — getters, predicates.
2. **Локальный inference** — дёшев, прозрачен, не масштабирует на
   весь модуль.
3. **Граница совпадает с D40** — где `=>`, там и inference; где `{}`,
   там типы обязательны.
4. **Прецедент Kotlin.**

### Что отвергнуто

- **Inference в block-body** — теряется явный контракт; диф большой
  функции мог бы молча менять тип возврата.
- **Полный inference (Haskell)** — public API теряет явный контракт.
- **`-> T` обязателен везде** — шум для тривиальных одностроек.

### Связь
- [D40](#d40-тело-функции--для-одного-выражения--для-блока) — граница
  применимости.
- [D20](#d20--вместо-void-и-сводка-стрелок) — `-> ()` опускается всегда.
- [07-modules.md → D47](07-modules.md#d47) — `export` функции и линтер.

### Реализация (Plan 55 Ф.3, 2026-05-16)

Bootstrap-codegen (`compiler-codegen/src/codegen/emit_c.rs::return_type_c`)
реализует **только** Expr-body inference (FnBody::Expr) — Block-body
без аннотации → `nova_unit` (как раньше; см. «Что отвергнуто» выше).

Inference при registration call-site signatures (free fn + method)
делегируется в `return_type_c`. Это гарантирует что caller'ы видят
правильный return type **до** emit_fn собственно body.

Edge-case: если body Expr возвращает `void*` или unknown — fallback
на `nova_unit` (safety).

---

## D46. Перегрузка операторов через `@`-методы

> **AMEND (решение владельца 2026-08-02, окно p-op-channel): `@not` RETRACTED —
> `!` сужен до `bool` СТРОГО, больше не перегружаемый оператор.**
>
> `!a` БОЛЬШЕ НЕ диспетчится на пользовательский метод `@not()`. `!` остаётся
> ЕДИНСТВЕННО логическим отрицанием на `bool` — `!x`, где `x` — не `bool`
> (числовой/строковый/пользовательский record/sum-тип), теперь ошибка чекера
> `[E_UNARY_OPERAND_TYPE]` («требуется `bool`»), а не dispatch-попытка на
> `@not()`. Селектор `not` убран из `UNOP_TABLE`
> (`compiler-codegen/src/codegen/operator_dispatch.rs`) и из D46-таблицы ниже.
>
> **Почему:** `@not` был единственным унарным оператором, для которого «тип
> результата = тип операнда» — не тождество (Bool→UserType), и единственным,
> для которого не было НИ ОДНОГО реального потребителя в `std`/`examples`
> (грепом на момент решения: только 2 conformance-фикстуры, обе — carrier-тесты
> самого `@not`, не реальный код). Побитовое дополнение уже получило отдельный
> оператор (`~`/`@bitnot`, AMEND 2026-07-27 ниже) — по тому же прецеденту `!`
> оставлен строго логическим, без второй перегружаемой роли. Устраняет
> асимметрию с `&&`/`||` (никогда не перегружались, Правило 2) и снимает
> необходимость операторному чекер-каналу (план 196, окно p-op-channel)
> резолвить единственный унарный оператор БЕЗ обратного тождества типа.
>
> **Фикстуры-носители снесены** (владелец подтвердил): бывшие
> `spec_tests/conformance/d46_operator_overload_at_methods.nv` (секция
> `D46BoolBox`/`@not`-тесты) и `m128_neg_not_operator_only_pos.nv`
> (`M128Neg@not`-DCE-репро) — обе существовали только ради `@not`. Негативная
> фикстура `!x` на не-`bool` — `spec_tests/conformance/neg/`.

> **AMEND (решение владельца 2026-07-27): побитовое семейство переименовано в
> `bit`-префикс + введён оператор `~`.**
>
> **(A) `@and`/`@or`/`@xor` → `@bitand`/`@bitor`/`@bitxor`** (RETRACT прежних имён).
> В Nova `&`/`|` — ЧИСТО побитовые: логические `&&`/`||` не перегружаются вовсе
> (Правило 2 ниже), поэтому имена `and`/`or`, читающиеся в прозе и в большинстве
> языков как ЛОГИЧЕСКИЕ, вводили в заблуждение. Эталон архитектуры — Rust —
> префиксует ровно ради этого различения (`BitAnd::bitand`, `BitOr::bitor`,
> `BitXor::bitxor`). Момент выбран по цене: реализаций прежних имён в `std` было
> **ноль** (первый потребитель — `std/math/int128.nv`), позже переименование
> стоило бы миграции.
>
> **(B) Введён `~a` → `@bitnot()` — побитовое дополнение, ПЕРЕГРУЖАЕМОЕ**
> (решение владельца: доступно и пользовательским типам, не только примитивам).
> До этого побитового НЕ не существовало КАК ОПЕРАТОРА: `!` в Nova — логическое
> отрицание (на `bool` работает, на целых нет), а `~` отсутствовал. Единственным
> путём был ширино-зависимый обход `x ^ 0xFFFF…` — **9 сайтов в `std`**
> (`checksums/crc32.nv:74,103`, `crypto/md5.nv:196` — там прямо комментарий
> `// ~B`, то есть автор думает «`~B`», а пишет маску). Обход опасен: при смене
> ширины типа (`u32`→`u64`) маска МОЛЧА перестаёт быть дополнением, компилятор
> этого не ловит. `~x` ширино-независим по построению.
>
> Семантика `~` на знаковых — дополнение до двух: `~x == -(x + 1)`. Переполнения
> не бывает (trap неприменим). Приоритет — унарный, как `!` и унарный `-`.
>
> **`!a` → `@not()` НЕ меняется и остаётся ЛОГИЧЕСКИМ** — именно поэтому
> побитовому дополнению нужен собственный оператор, а не перегрузка `!`
> (путь Rust, где `!` служит обеим ролям, нам закрыт: `!` уже логический).
> **[RETRACTED 2026-08-02 — см. AMEND выше]:** `@not()` как перегружаемая цель
> для `!` впоследствии полностью убрана (`!` сужен до `bool` строго); в момент
> ЭТОГО амендмента (2026-07-27) `@not` ещё существовал, текст сохранён как
> историческая запись решения "не давать `~` роль `!`".
>
> Реализация — план [234](../../docs/plans/234-bitwise-operator-family.md)
> (компилятор: переименование диспетчера + парсер/лоуэринг `~`; миграция
> 9 обход-сайтов). Маркер `[M-bitwise-operator-method-naming]`.

### Что
Стандартные операторы автоматически вызывают instance-методы с
**фиксированными именами**. Если у типа есть метод нужного имени —
оператор работает. Custom-операторы запрещены.

### Правило

```nova
fn Duration @plus(other Duration) -> Duration =>
    Duration { nanos: @nanos + other.nanos }

fn Duration @times(n i64) -> Duration =>
    Duration { nanos: @nanos * n }

ro total = 1.hour() + 30.minutes()       // вызывает @plus
ro triple = 5.seconds() * 3              // вызывает @times
```

> **(C) Compound-присваивания `&=` `|=` `^=` `<<=` `>>=`** (решение владельца
> 2026-07-27, «мы просто забыли про них»): арифметические `+=`/`-=`/`*=`/`/=`
> существовали, битовых не было. Семантика — десугар в `a = a <op> b`;
> отдельного операторного метода НЕТ: пользовательские типы диспетчеризуются
> теми же `@bitand`/`@bitor`/`@bitxor`/`@shl`/`@shr`. Реализация — план 234 Ф.2а.
>
> **(F) `%` определён и на ПЛАВАЮЩИХ** (2026-08-17, реестр №707): `7.5 % 2.0`
> — законное выражение. Вопрос поставлен окном 274, обнаружившим расхождение:
> `nova check` принимал, `nova build` падал ошибкой clang «invalid operands to
> binary expression» — текст ЧУЖОГО компилятора утекал пользователю вместо
> диагностики Nova (в C оператор `%` определён только для целых). **Почему
> законен, а не запрещён:** таблица приоритетов перечисляет `%` без
> ограничения по типам; D46 даёт `a % b -> @rem(b)` перегружаемым, то есть
> операция мыслится общей; C (`fmod`) и Rust (`%` на float) её имеют,
> запрещает только Go. Запрет сузил бы язык ради обхода дефекта кодогена.
> **Семантика — как у целых и как в C/Rust: знак остатка берётся от ДЕЛИМОГО**
> (`-7.5 % 2.0 == -1.5`), а не евклидов. Лоуэринг: `fmod` для `f64`, `fmodf`
> для `f32` (не `fmod`, иначе двойное округление); составная форма `x %= y`
> десугарится в `x = fmod(x, y)` — то же обещание, что (E) даёт для целых.
> Деления на ноль здесь НЕТ как ошибки: IEEE-754 даёт NaN, паника целых
> (D427) к плавающим не применяется — как и у `/`.
>
> **(E) Compound-присваивание `%=`** (решение владельца 2026-08-16): последний
> оператор уровня 1 таблицы приоритетов, которого не было в языке. Найдено окном
> 274 при сверке novac с оракулом: из одиннадцати операторов строки «уровень 1»
> (`=` `+=` `-=` `*=` `/=` `%=` `&=` `|=` `^=` `<<=` `>>=`) десять принимались, а
> `%=` давал `unexpected \`=\` in expression`. **Это не был осознанный пропуск и не
> опечатка таблицы:** амендмент (C) выше добавлял битовое семейство со словами
> владельца «мы просто забыли про них» — и `%=` забыли в тот же раз, а D427
> (2026-07-16) зафиксировал следствие как факт языка («в языке НЕТ `%=`, только
> `Div`-вариант `AssignOp`»), не заметив, что таблица приоритетов требует его.
> Таблица была права, язык — нет. Семантика — как у (C): десугар в `a = a % b`,
> отдельного операторного метода НЕТ, пользовательские типы идут тем же `@rem`.
> Guard достался бесплатно: D427 уже даёт `nova_<T>_checked_rem` для каждого члена
> `Ints` с тем же условием, что `checked_div` (`b == 0` → паника «division by
> zero»; signed `MIN % -1` → «division overflow», потому что x86 `idiv` фолтит на
> overflow ЧАСТНОГО независимо от истинного остатка). Проверено на `int`, `i64`,
> `u8`, элементе вектора (`v[i] %= k`) и на нуле (паника, не SIGFPE).
>
> **(D) `~` определён ТОЛЬКО для целочисленных** (решение владельца 2026-07-27):
> `~true`/`~1.5`/`~"str"` — ошибка чекера (на `bool` логическое отрицание — `!`);
> для пользовательских типов — через объявленный `@bitnot()`. Эмиссия на узких
> ширинах учитывает C integer promotion: беззнаковые узкие — `x ^ MASK` ширины
> (нулевое продвижение делает результат корректным без каста), знаковые узкие и
> все широкие — голый `~` (план 234, таблица эмиссии).

Mapping:

| Оператор | Метод | Возврат |
|---|---|---|
| `a + b` | `@plus(b)` | свободный |
| `a - b` | `@minus(b)` | свободный |
| `-a` | `@neg()` | обычно `Self` |
| `a * b` | `@times(b)` | свободный |
| `a / b` | `@div(b)` | свободный |
| `a % b` | `@rem(b)` | свободный |
| `a \| b`, `a & b`, `a ^ b` | `@bitor` / `@bitand` / `@bitxor` | свободный |
| `~a` | `@bitnot()` | обычно `Self` |
| `a << n`, `a >> n` | `@shl` / `@shr` | свободный |
| `a == b`, `a != b` | `@equal(b)` через протокол `Equal` (`!=` выводится) | `bool` |
| `a < b`, `<=`, `>`, `>=` | `@compare(b)` через протокол `Compare` (< 0, > 0, == 0) | `bool` |
| `!a` | НЕ перегружаемый — **строго логическое** отрицание на `bool` (RETRACTED 2026-08-02, см. AMEND) | `bool` |
| `a[i]` (read), `a[i] = v` | `@get(i)` / `@set(i, v)` | свободный / `()` |

Правила:
1. **Только методы инстанса** — привязка к первому операнду.
2. **`&&`, `||` не перегружаются** — short-circuit предсказуем.
3. **`!=` выводится из `@equal`** — отдельно объявлять не надо.
4. **Custom-операторы запрещены** (`:+`, `>>=` и т.п.) — фиксированный
   набор символов.
5. **Никаких protocol/trait** — структурное соответствие по имени.
5а. **`!a` НЕ перегружается** (RETRACTED 2026-08-02) — единственный
    невырожденный унарный оператор, оставленный строго встроенным на `bool`,
    тем же принципом, что Правило 2 для `&&`/`||`.
6. **Type coercion нет** — `Duration + 30` ошибка, нужен `Duration + 30.seconds()`.
7. **Overloading методов по типу аргумента разрешён**, если сигнатуры
   различимы:

```nova
fn Vector @times(s f64) -> Vector =>     // умножение на скаляр
    Vector { x: @x * s, y: @y * s }

fn Vector @times(other Vector) -> f64 => // dot product
    @x * other.x + @y * other.y
```

> **Примечание (Plan 91.8b, 2026-06-17):** `@eq`/`@lt`/`@le`/`@gt`/`@ge` — УДАЛЕНЫ как operator-dispatch имена.
> Используй `Equal.@equal` / `Compare.@compare`. Подробнее: [D363](#d363-operator-dispatch-via-protocols--замена-magic-methods-plan-918b).

> **Амендмент (владелец, 2026-07-21): исключение для `str` — `a + b`
> запрещён.** Строка `| a + b | @plus(b) | свободный |` таблицы выше
> перестаёт применяться к `str`+`+` конкретно: `nova_str + nova_str` —
> **hard error `E_STR_CONCAT_PLUS`**, а не dispatch на `@plus`. `str`
> определяет `@plus` (std/src/runtime/string/transform.nv, делегирует
> `@concat`), но операторный `+` больше не диспатчится на него ни для
> какого типа операнда — единственное такое исключение в языке. Канон —
> string-интерполяция (`"${a}${b}"`) или явный `.concat(...)`/
> `StringBuilder` в цикле. Подробности и мотивация — амендмент
> [D216 §1](02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo)
> (str-блок, рядом с `@concat`/operator-lowering option (b)).

### Почему

1. **Просто и предсказуемо** — структурное matching по имени, без
   trait-механики.
2. **Закрытый набор операторов** — Scala-style символьные методы
   (`:+`, `<>`) известны как источник нечитаемости.
3. **`&&`/`||` фиксированы** — short-circuit семантика.
4. **Прецедент Kotlin** — фиксированные имена методов.

### Что отвергнуто

- **Через `protocol/trait`** (Rust `impl Add`, Swift) — избыточно.
- **Custom-операторы (Scala/C++)** — нечитаемый код.
- **Свободные функции (`fn plus(a, b)`) для операторов** —
  unification-ambiguity при резолве `a + b`. Overloading свободных
  функций по типам аргументов сам по себе разрешён
  ([D84](10-overloading.md#d84)), но привязка операторов к
  receiver-методам (`@plus`/`@times`) однозначнее: компилятор знает,
  где искать реализацию.
- **Перегрузка `&&`/`||`** — нарушает short-circuit.
- **Auto-derive `@equal`/`@compare`** — отдельный механизм, не часть D46 (см. D363).

### Связь
- [D35](#d35-методы-инстанса-через--self-отменён) — те же `@`-методы.
- [D45](#d45-inferred-return-type-для-expression-body) — методы
  операторов имеют inferred return при expression-body.
- [02-types.md](02-types.md) — отсутствие trait/impl.

### Эволюция
Закрывает Q16 (bitflags): `type Permission(int)` с `@or`/`@and`/`@not`
для `|`/`&`/`!`.

---

## D363. Operator dispatch via protocols — замена magic methods (Plan 91.8b)

> Status: active (2026-06-17, Plan 91.8b).

### Что

Операторы сравнения диспатчятся через протоколы:

| Оператор | Диспатч | Требование |
|---|---|---|
| `a == b` | `a.@equal(b)` | тип реализует `Equal` |
| `a != b` | `!a.@equal(b)` | тип реализует `Equal` |
| `a < b` | `a.@compare(b) < 0` | тип реализует `Compare` |
| `a <= b` | `a.@compare(b) <= 0` | тип реализует `Compare` |
| `a > b` | `a.@compare(b) > 0` | тип реализует `Compare` |
| `a >= b` | `a.@compare(b) >= 0` | тип реализует `Compare` |

Примитивы (`int`, `bool`, `f64`, `char`) — встроенный C-диспатч, без вызова метода.

### Что удалено

`@eq`, `@lt`, `@le`, `@gt`, `@ge` как operator-dispatch имена — удалены из компилятора (Plan 91.8b).

### Связь
- [D46](#d46-перегрузка-операторов-через--методы) — operator overloading base
- [D109](02-types.md#d109) — Equal/Compare протоколы
- [Plan 91.8b](../../docs/plans/91.8b-operator-dispatch-protocols.md)

---

## D48. Tagged template literals

### Что
Литералы вида `` tag`raw_text` `` — синтаксический сахар над вызовом
функции `tag`, получающей сегменты текста и интерполированные значения
**раздельно**.

### Правило

```nova
ro j = json`{"name": "alice"}`
ro q = sql`SELECT * FROM users WHERE id = ${user_id}`
ro h = html`<div>${escape(name)}</div>`
ro r = regex`\d{3}-\d{4}`
ro b = bytes`deadbeef`
```

Грамматика:

```
tagged-template = identifier '`' template-body '`'
template-body   = ( raw-char | escape-seq | interpolation )*
escape-seq      = '\\' ( '`' | '\\' | '$' )
interpolation   = '${' expression '}'
```

**Амендмент 2026-08-30 (D467 §6, реестр №830).** Здесь стояло
`'\\' ( '`' | '\\' | '${' | 'n' | 't' | ... )` — набор с МНОГОТОЧИЕМ, и это
неверно дважды. Во-первых, D467 §6 закрыл набор до РОВНО ТРЁХ (`` \` ``, `\\`,
`\$`), а `\n`/`\t` из него убраны: backtick многострочный, перевод строки
набирается клавишей, а раскрытие ломало пути (`` `C:\new\table` ``) и regex.
Во-вторых, третий escape записан здесь как `'${'` — три символа, — тогда как
реализация и D467 говорят `\$`: слеш экранирует ОДИН доллар, а `{` дальше идёт
текстом.

**Почему амендмент отдельной строкой, а не тихой правкой:** 2026-08-29 D467
получил амендмент к разделу «Что отвергнуто» этого же блока (снятие
«Implicit tag»), а ЭТОТ блок грамматики остался нетронутым — и сутки читатель
D48 находил нормой одно, читатель D467 другое. Нашла это охота по оракулу
(план 278 Ф.7), а не вычитка: агент честно отказался выбирать между двумя
местами спеки и подал их как противоречие, что и есть его работа.

Desugar:

```nova
sql`SELECT * FROM users WHERE id = ${user_id} AND name = ${name}`
// эквивалентно
sql(
    ["SELECT * FROM users WHERE id = ", " AND name = ", ""],
    [user_id, name]
)
```

Tag-функция получает `parts []str` (сегменты, длина = `args.len() + 1`)
и `args []T`. Сигнатура:

```nova
fn tag_name(parts []str, args []T) -> ResultType => ...
```

Стандартные теги stdlib MVP: `json`, `sql`, `regex`, `bytes`. `html`,
`css`, `graphql` — user-space.

Compile-time validation через `@comptime` — для тегов без интерполяций
(пустой `args`); если функция помечена, литерал проверяется при
компиляции (некорректный JSON → compile error). В MVP `@comptime`
откладывается на v2.

Multiline и raw escapes естественны:

```nova
ro r = regex`\d+\.\d+`               // не нужно дважды экранировать
ro q = sql`
    SELECT id, name
    FROM users
    WHERE created_at > ${cutoff}
`
```

### Почему

1. **Типобезопасная интерполяция** — главное преимущество. Tag
   получает raw parts и args отдельно, сама эскейпит / передаёт
   через prepared statement (защита от SQL injection).
2. **User-defined теги** — обычные функции, любое имя.
3. **Compile-time валидация** через `@comptime` — JSON/regex/SQL без
   runtime-парсинга.
4. **Прецедент JavaScript** по синтаксису, Scala/Rust по compile-time.

### Что отвергнуто

- **`s"..."` / `r"..."` (Scala)** — ограничивает имя одним символом,
  нет user-defined.
- **`tag.raw("...") + tag.interp("...", args)`** — слишком многословно.
- **Macros (Rust `sql!`)** — требует механизма макросов.
- **Implicit tag** — ~~ambiguity со строками~~.
  **Амендмент (владелец, 2026-08-29): отказ СНЯТ, голая форма законна** —
  решение «принимаем исследования», нормативно — D467 §5. Названная здесь
  двусмысленность снята не тегом, а ДЕЛИМИТЕРОМ: `"…"` и `` `…` `` — два разных
  литерала, и ни один не превращается в другой от появления идентификатора
  слева. Настоящая двусмысленность была в обратном: пока голая форма жила вне
  нормы, тело backtick зависело от НАЛИЧИЯ тега (с тегом — parts/args, без —
  сырая строка), и `` `a\nb ${x}` `` менял смысл от одного имени слева.
  Теперь тело разбирается всегда; тег решает только сборку.

### Связь
- [D33](#d33-const-vs-let--compile-time-vs-runtime) — `@comptime`-теги
  без интерполяций могут быть `const`.
- [D27](#d27-синтаксис-массивов-t-префикс-nt-фиксированные) — `parts`
  и `args` — обычные `[]T`.
- [D40](#d40-тело-функции--для-одного-выражения--для-блока) — tag-функции
  обычные.
- [09-tooling.md → D24](09-tooling.md#d24) — `requires` для валидации
  parts/args.

---

## D49. Statement separator и парсинг выражений

### Что
**Перенос строки** — основной разделитель statement'ов. **`;`** —
опциональный, нужен только при нескольких statement'ах на одной строке.

### Правило

```nova
ro x = 1                        // newline разделяет
ro y = 2
foo(x, y)

ro a = 1; ro b = 2; foo(a, b)  // ; для одной строки (редко)
```

Лексер игнорирует NEWLINE, если statement очевидно продолжается:

1. **После висящего бинарного оператора** в конце предыдущей строки:
   ```nova
   ro total = a +
               b +
               c
   ```
2. **Внутри открытых `(`, `[`, `{`** — newlines игнорируются.
3. **Перед `.`** (method chain) и **перед `?`** (error propagation):
   ```nova
   ro r = list
       .filter(|x| x > 0)
       .map(|x| x * 2)
       .sum()
   ```
4. **После `,`** в списках.
5. **Перед `else` / `else if`** — продолжение `if`-выражения:
   ```nova
   ro label =
       if s is Origin { "at-origin" }
       else if s is Circle { "circle" }
       else { "square" }
   ```
   Без этого правила multi-line `if/else` приходится писать через
   повторное присваивание `let mut x = default; if ... { x = ... }`.

6. **Перед `||` / `&&` / `or` / `and`** — продолжение boolean expression:
   ```nova
   fn is_alnum(c char) -> bool {
       (c >= '0' && c <= '9')
       || (c >= 'A' && c <= 'Z')
       || (c >= 'a' && c <= 'z')
   }
   ```
   Это исключение из общего правила «бинарные операторы — в конце
   предыдущей строки» (Go-стиль). `||` и `&&` часто пишут leading'ом
   для читаемости; обе формы допустимы. Реализовано через look-ahead
   в `parse_or` / `parse_and`.

**Бинарные операторы — в конце предыдущей строки** (Go-стиль) для
большинства операторов (`+`, `-`, `*`, и т.п.). Исключения
зафиксированы в правилах 5 и 6 выше: `else`/`else if` и
`||`/`&&`/`or`/`and` — leading-форма допустима. `+` в начале новой
строки воспринимается как унарный.

#### Compound-assignment

Compound-операторы — синтаксический сахар:

| Оператор | Десахар |
|---|---|
| `a += e` | `a = a + e` |
| `a -= e` | `a = a - e` |
| `a *= e` | `a = a * e` |
| `a /= e` | `a = a / e` |

**Target обязан быть lvalue** — одна из трёх форм:

```nova
// 1) Локальная mut-переменная
mut n = 0
n += 1                              // ✅

// 2) @field на self в методе (D35)
fn Counter mut @inc() -> () {
    @value += 1                     // ✅
}

// 3) Element массива/индексируемой коллекции
mut xs = [10, 20, 30]
xs[0] += 5                          // ✅
```

Compound-assign — это **statement**, не expression. После `=>` в
match-arm или в expression-body функции его нельзя писать без
обёртки в `{ ... }`:

```nova
match c {
    Some('\n') => { @line += 1; @col = 1 }     // ✅ блок
    Some(_)    => { @col += 1 }                 // ✅ блок
    None       => ()
}

// ❌ парсер не поймёт `+=` в expression-position arm:
// Some(_) => @col += 1
```

Правая часть compound-assign — обычное выражение (любое допустимое в
RHS обычного `=`). Type-check соответствует базовому оператору:
`a += e` валидно ⇔ `a + e` валидно и его тип присваиваем `a`.

Перегрузка через `@plus`/`@minus`/`@times`/`@div` ([D46](#d46-перегрузка-операторов))
работает прозрачно — compound на user-типе с `@plus` десахарится в
`a = a.plus(e)`.

Edge cases:

```nova
ro x = foo
(arg)                        // ❌ два statement'а: foo и (arg)

ro x = foo(arg)             // ✅ одна строка
ro x = foo(                 // ✅ открытая ( игнорирует newline
    arg
)
```

Trailing-block: `)` и `{` на одной строке ([D43](#d43-trailing-block--без-params-fnp-body-с-params)).

Match-arms — `,` или `\n` оба разделяют:

```nova
match x {
    Some(v) => v * 2          // newline разделяет
    None    => 0
}

match x {
    Some(v) => v * 2,         // запятые тоже работают
    None    => 0,
}
```

Пустые `;` запрещены — всегда баг.

Иерархия приоритетов (от низкого к высокому):

| Уровень | Операторы | Ассоциативность |
|---|---|---|
| 1  | `=`, `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `\|=`, `^=`, `<<=`, `>>=` | right |
| 2  | `..`, `..=` (range) | non-associative |
| 3  | `\|\|` | left |
| 4  | `&&` | left |
| 5  | `==`, `!=` | left |
| 6  | `<`, `<=`, `>`, `>=` | left |
| 7  | `\|` (bitwise or) | left |
| 8  | `^` (bitwise xor) | left |
| 9  | `&` (bitwise and) | left |
| 10 | `<<`, `>>` | left |
| 11 | `??` (coalescing) | **right** |
| 12 | `+`, `-` (binary) | left |
| 13 | `*`, `/`, `%` | left |
| 14 | `as` (cast) | left |
| 15 | `!`, `-` (unary) | right |
| 16 | `?`, `!!`, `()`, `[]`, `.` | left |

**Строка `??` и её место ЗАМЕРЕНЫ, а не назначены (2026-08-30, реестр №841).**
До этого дня таблица не называла ни `??`, ни `!!` вовсе — то есть два действующих
оператора языка не имели записанного приоритета; пробел нашло окно 274 при
подготовке волны и не стало закрывать его догадкой. Замеры СБОРКОЙ И ЗАПУСКОМ:

| Проба | Результат | Что значит |
|---|---|---|
| `a ?? 1 + 10`, `a = None` | `11` | разобрано как `a ?? (1 + 10)` — аддитивные КРЕПЧЕ |
| `a ?? 1 << 2`, `a = Some(8)` | `32` | разобрано как `(a ?? 1) << 2` — сдвиги СЛАБЕЕ |
| `b ?? 0 < 5`, `b = Some(7)` | `false` | разобрано как `(b ?? 0) < 5` — сравнения СЛАБЕЕ |
| `a ?? b ?? 3` | собирается, даёт `2` | ПРАВОассоциативен: при левой `(a ?? b)` было бы `int ?? Option` |

Отсюда место: строго МЕЖДУ сдвигами и аддитивными. Нумерация ниже сдвинута
на единицу; проверено грепом, что номера уровней нигде не цитируются (0 совпадений
по `spec/` и `docs/dev/`), то есть они ярлыки порядка, а не идентификаторы.
`!!` дописан к постфиксному уровню по уже записанной норме
([04-effects.md](04-effects.md): «имеет высший приоритет, тот же уровень, что `?`») — это не
новое решение, а перенос существующего в таблицу, где его ищут.

**ВНИМАНИЕ: эта таблица описывает РАЗБОР, а не типизацию.** Тем же
замером обнаружено, что `??` ВООБЩЕ НЕ ПРОВЕРЯЕТ типы операндов:
`ro n = 5` и `n ?? 99` даёт **99**, то есть не-Option слева молча считается `None`
и написанное значение теряется. Это дефект РЕАЛИЗАЦИИ, а не норма:
реестр №839, блокер тега. Норма по-прежнему [D86](#d86): слева обязан быть
`Option`/`Result`, справа — ЗНАЧЕНИЕ нагрузки.

Грамматика (упрощённо):

```
program       = statement*
block         = '{' statement* '}'
statement     = ( decl | expr ) statement-end
statement-end = ';' | NEWLINE | look-ahead '}'

postfix-expr  = primary ( '.' name | '[' expr ']' | '(' args ')' | '?' )*
primary       = literal | identifier | '(' expr ')' | block | if | match | ...
```

#### Block-expressions в statement-позиции (Plan 148 Ф.2 clarification)

`statement = ( decl | expr ) statement-end` — в statement-позиции `expr`
парсится **полной** `postfix-expr` грамматикой, без отдельной
«block-statement» ветки. Это значит, что **block-формы выражений**
(`{ ... }`, `if`, `match`, `unsafe { ... }`) в начале statement'а
принимают постфикс (`.method()` / `[i]` / `.field`) **напрямую, без
обёртки в `(…)`**:

```nova
fn Vec[T Display] @display(mut sb StringBuilder) -> () {
    sb.append("Vec[")
    for i in 0..@len {
        if i > 0 { sb.append(", ") }
        unsafe { @data[i] }.display(sb)   // ✅ постфикс на unsafe-блоке
    }                                      //    без `(unsafe { … })`
    sb.append("]")
}

ro d = if cond { mk_a() } else { mk_b() }.doubled()  // ✅ постфикс на if
ro n = match tag { 1 => mk(99), _ => mk(0) }.field    // ✅ постфикс на match
```

Граница с **bare `{ ... }` блоком**: если блок открывает statement и за
ним **нет** постфикса, он остаётся обычным side-effecting
expression-statement (его значение, если не trailing, отбрасывается) —
никакого специального «block vs expression» форка в парсере нет; разбор
единообразен. Постфикс привязывается ровно тогда, когда он синтаксически
присутствует сразу после `}` (с учётом newline-tolerance правила 3 для
ведущего `.`). Ранее `unsafe { … }`-форма документировалась как
требующая `(…)` для постфикса — это устранено (закрывает backlog-маркер
`[M-138-unsafe-block-postfix-stmt]`).

### Почему

1. **Современный тренд** (Go/Kotlin/Swift/TS): newline-разделитель,
   меньше шума.
2. **Простые правила вместо JS ASI** — JavaScript ASI известный
   источник багов (`return\n{...}` возвращает undefined). Nova
   строит на «висящий оператор», «незакрытая скобка», «.method/?».
3. **Бинарный оператор в конце** — Go-практика, иначе унарный
   парсинг ломает выражение.

### Что отвергнуто

- **Обязательный `;` (Rust/C)** — лишний шум.
- **Indentation-significant блоки** — конфликт с [D40](#d40-тело-функции--для-одного-выражения--для-блока).
- **JS ASI с edge cases** — известный источник багов.
- **Перенос оператора в начало строки** — унарный/бинарный конфликт.

### Связь
- [D40](#d40-тело-функции--для-одного-выражения--для-блока) — внутри
  `{}` newlines разделяют statement'ы.
- [D43](#d43-trailing-block--без-params-fnp-body-с-params) — `)` и `{` на одной
  строке как частный случай.
- [D45](#d45-inferred-return-type-для-expression-body) — последнее
  выражение блока становится возвратом через newline-разделитель.
- [04-effects.md](04-effects.md) — handler-литералы используют те же
  правила внутри `{...}`.

---

## D54. Операторы `as` и `is`

### Что
Два оператора с разной семантикой:

- **`as`** — **compile-time конвертация** значения между совместимыми
  типами (numeric cast, newtype ↔ underlying, sum → int).
  Возвращает значение целевого типа. Если конвертация невозможна по
  правилам типов — ошибка компиляции.
- **`is`** — **runtime type-check** для значений типа `any`. Возвращает
  `bool`. Также используется как pattern в `match` и `if` для
  биндинга и smart cast'а.

`as` — про **«сделай этим типом»** (статически). `is` — про
**«проверь, какой это тип сейчас»** (runtime).

### Правило

#### `as` — compile-time конвертация

`as` работает в позиции выражения: `<expr> as <type>`. Возвращает
значение целевого типа.

**Numeric cast** (см. [D44](#d44-числовые-литералы)):

```nova
ro n = 100 as u32           // литерал → u32
ro big = 0xFF_FF as u16
ro x = 1.5 as i32           // f64 → i32 (truncate)
ro y = some_int as f64       // int → f64
```

#### Семантика narrowing-конверсий

Поведение `as` при потере точности зависит от пары source→target.
В отличие от C (где out-of-range float→int это UB), Nova даёт
**defined behavior** на любом входе:

| From → To | Семантика | Пример |
|---|---|---|
| `iN → iM` (M < N) | wraparound (modulo 2^M) | `0x1_FFFF as i16 == -1` |
| `iN → uM` | bit-pattern truncate | `-1i32 as u16 == 65535` |
| `uN → uM` (M < N) | wraparound | `0x1_FFFF as u16 == 0xFFFF` |
| `uN → iM` | bit-pattern, signed reinterpret | `0xFFFFu16 as i16 == -1` |
| `f64 → f32` | IEEE rounding | `1.1 as f32 ≈ 1.1` (с потерей) |
| **`f → iN`** | **saturation + NaN→0** | `70000.5 as i16 == 32767` |
| **`f → uN`** | **saturation + NaN→0 + neg→0** | `-1.0 as u16 == 0` |
| `iN → f` | exact (или nearest IEEE) | `123 as f64 == 123.0` |
| newtype ↔ underlying | identity | `42 as UserId` reuses bits |

> **Амендмент 2026-09-04 (решение владельца; исследование
> `docs/dev/research/2026-09-04-numeric-widening.md` §5).** Две правки к таблице выше.
>
> **1. Четыре целочисленные строки — ОДНА операция.** «wraparound (modulo 2^M)», «bit-pattern
> truncate», «signed reinterpret» — три названия одного действия: взять младшие M бит источника и
> прочитать их как значение целевого типа. `0x1_FFFF as i16 == -1` и `-1i32 as u16 == 65535` —
> одно и то же правило с разных сторон. Так у Rust `as`, Go, Java, C для беззнаковых. Следствие для
> читателя: `u64 as u8`, `i32 as u16` **не требуют** `& 0xFF`/`& 0xFFFF` руками — `as` и есть эта
> маска (проба: `0x1234u64 as u8 == 52`, `70000i32 as u16 == 4464`). Исключений по имени типа нет:
> `int as uint` — та же операция (D130 Q2 снят тем же днём).
>
> **2. Пример `i16.try_from(f)?` ниже — мёртвая ссылка.** D77 ретрагирован 2026-07-06, статики
> `i16.try_from` не существовало никогда (проба: чекер `ok`, кодоген `E_UNKNOWN_STATIC_METHOD`).
> Проверяемая форма для ЦЕЛЫХ — `@to_i8()…@to_uint()` (D430, blanket по `Ints`):
> `(300 as u32).to_u8() == Err(AboveMax)`. Для float → целое проверяемой формы **нет**;
> её заведение и семантика (усечение-и-проверка против «точно представимо») — строка реестра
> «D54 отсылает к `i16.try_from`», решение владельца. До этого единственная форма — `as` с
> насыщением.

**Float → integer — saturation, не UB.** Out-of-range, NaN, ±Infinity
дают defined значение, не зависящее от платформы:

- Out-of-range positive → `INT_MAX` / `UINT_MAX`.
- Out-of-range negative → `INT_MIN` / `0` (для unsigned).
- NaN → `0`.
- `+Infinity` → `INT_MAX` / `UINT_MAX`.
- `-Infinity` → `INT_MIN` / `0`.

**Если нужна проверка** out-of-range — для ЦЕЛЫХ есть `@to_*` (D430),
для float проверяемой формы нет; см. амендмент 2026-09-05 ниже. Прежняя
отсылка к `TryFrom` (D77) СНЯТА: D77 ретрагирован 2026-07-06.

```nova
ro n = f as i16                // saturation, infallible
ro n = i16.try_from(f)?         // МЁРТВАЯ ССЫЛКА — см. амендмент 2026-09-04 выше; для целых: (x).to_i16()
```

`as` остаётся **pure** (без Fail-эффекта). Форма, возвращающая ошибку,
— это `@to_*` (D430) для целых; для float её нет.

**Прецеденты.** Saturation для float→int согласован с **Rust 1.45+**
(RFC #2484 «sealed casts») — прямой аналог. C/C++ дают UB, Nova
улучшает. Swift делает trap (panic), нет pure `as` — Nova выбирает
saturation для совместимости с D54 «as это pure». Java делает
IEEE round + wraparound (defined, но не saturation).

**Newtype ↔ underlying** (см. [02-types.md → D52](02-types.md#d52)):

```nova
type UserId u64

ro u UserId = 42 as UserId   // u64 → UserId
ro n u64 = u as u64           // UserId → u64
```

**Sum → int** (для sum'ов с числовыми discriminants, [D52](02-types.md#d52)):

```nova
type ErrorCode enum NotFound = 404 | InternalError = 500
ro code = NotFound as int    // 404
```

**Запрещено:**

- **`any → T`** (`x as int` где `x any`) — нет статической конвертации.
  Используйте `is`-pattern или `try_as[T]()` (см. ниже).
- **Произвольные типы без явного правила** (`User as Account`) —
  ошибка компиляции.
- **int → Sum через `as`** — type-небезопасно (число может не
  попасть в варианты). Только через pattern match (см. D52).

#### Запрещённые `as`-cast'ы для char/u8/bool

Prune `as`-cast'ов где seemingly-numeric mapping выражает unsafe
семантику. Программист должен использовать `try_from`/конверсию на
источнике (с range-check'ом/парсингом) или explicit comparison:

| Запрещено через `as` | Альтернатива |
|---|---|
| `int as char`, `iN/uN as char` | **формы нет** — см. амендмент 2026-09-05 ниже |
| `char as u8` | `u8.try_from(c)?` (fails если codepoint > 0xFF) |
| `int/u8/f64/etc as bool` | `n != 0` (или `n != 0.0`) |
| `str as int/i32/f64/bool/char` | `s.to_int()?`/`s.to_f64()?`/`s.to_bool()?`/`s.to_char()?` (parse, метод на источнике) |
| `int/f64/bool/char as str` | `v.to_str()` (метод на ИСТОЧНИКЕ, D430/Display) |

> **AMEND (2026-08-01, owner decision, p-tryfrom) — `str as T` parse-строка
> ретрактирована.** Прежняя форма `T.try_from(s)?` для `str as
> int/i32/f64/bool/char` противоречила канону Plan 174.1 + D77 R3
> («конверсия — метод на ИСТОЧНИКЕ», тот же принцип, что уже применён к
> `TimeZone.try_from` → `s.to_timezone()`, [D321
> R6](04-effects.md#d321)). Интегратор дважды спотыкался об это внутреннее
> противоречие спеки (таблица предписывала `T.try_from(s)?`, а фактический
> канон/реализация — `s.to_*()`). Канон: `s.to_int()`/`s.to_u64()`/
> `s.to_f64()`/`s.to_bool()`/`s.to_char()` (`std/runtime/string/parse.nv`,
> Plan 174.1) — каждый возвращает `Result[T, E]`, `?`/`.ok()`/`match`
> работают идентично прежней `try_from`-форме. `T.try_from(s str)` для
> примитивов (`int`/`i64`/`u64`/`f64`/`bool`/`char`) был чистым
> compiler-интринзиком без единой `.nv`-декларации — эмит-ветка удалена
> (`emit_c.rs`), вызов теперь честная ошибка компиляции
> `[E_UNKNOWN_STATIC_METHOD]`. Числовые/char narrowing-конверсии
> (`char.try_from(n int)`, `u8.try_from(c char)`) — **отдельная, легальная**
> ветка `try_from`, НЕ затронута этим амендментом (см. блок ниже).

> **АМЕНДМЕНТ 2026-09-05 (решение владельца; исследование
> `docs/dev/research/2026-09-04-numeric-widening.md` §5.6).** Последняя фраза выше снята.
> «Отдельной, легальной ветки `try_from` для числовых/char конверсий» **нет**, и не было в
> момент написания: `char.try_from(n int)` ретрагирован 2026-07-09 в пользу `n.to_char()`
> (`char.nv:40`, «одно имя = одна дверь»), а `u8.try_from(c char)` оставался единственным
> статиком-на-цели среди числовых конверсий, с нулём вызовов. Канон записывается целиком,
> двумя половинами, чтобы у следующего читателя не было соблазна вывести третью:
>
> 1. **Конверсия между конкретными типами — метод на ИСТОЧНИКЕ:** `x.to_T()`
>    (инфаллибельная), `x.to_T()` (проверяемая, `Result[T, RangeError]`, D430).
>    `s.to_int()`, `n.to_char()`, `c.to_u8()`, `f.to_i16()`, `(300u32).to_u8()`.
> 2. **Статика на ЦЕЛИ законна ровно в двух случаях:** конструктор `Self` в протоколе, где цель
>    известна только как параметр типа и метода на источнике быть не может
>    (`Ordinal.from_ordinal(i int) -> Self`, D143 — `int` не знает, какой `I` из него хотят);
>    и фаллибельный конструктор рядом с инфаллибельным `from` (`JsonValue.try_from(s str)`,
>    D77 R3 — имя-конвенция конструктора, не конверсии).
>
> `u8.try_from(c char)` не подпадает ни под одну половину — типы конкретны, `Self` нет, сиблинга
> `from` нет — и заменён на `char @to_u8() -> Result[u8, RangeError]` (`std/runtime/char.nv`);
> `TryFromCharError` снят вовсе (владелец 2026-09-05: без deprecated — до тега стабильной поверхности ещё
> нет, и мёртвый тип в ней не нужен). Протоколы `From[T]`/`Into`/`TryFrom`/`TryInto` ретрагированы D73/D77
> 2026-07-06 и в канон не входят.
>
> **Тем же днём — `int → char`: одна дверь, одна ошибка (решение владельца 2026-09-05).**
> Поправка к амендменту интегратора выше: проверяемая конверсия `int → char` **есть** —
> `int @to_char() -> Result[char, …]` (`std/runtime/char.nv:43`), просто её ошибка носила
> имя от снятого конструктора (`CharFromError` ← `char.from` ← `CharTryFromError` ←
> `char.try_from`), а рядом жило второе имя на тот же факт (`InvalidCodepoint` у
> `str.try_from_codepoint`) и три копии проверки диапазона (`char.nv`, `fmt_buf/core.nv` ×2,
> `json.nv` со своей арифметикой суррогатных пар). Норма:
>
> - `char` — Unicode scalar value: число в `[0, 0x10FFFF]` вне зарезервированного блока
>   `[0xD800, 0xDFFF]` (эти 2048 номеров Unicode отдал UTF-16 под запись символов двумя
>   16-битными единицами; символов с такими номерами нет, в UTF-8 они не кодируются).
> - **Единственная дверь** из числа в `char` — `int @to_char() -> Result[char, CharError]`;
>   `type CharError enum AboveMax | BelowMin | Invalid` (выше `0x10FFFF` / отрицательное /
>   внутри диапазона, но зарезервировано). Тот же словарь, что у `RangeError` (D430 R7) и
>   `ParseIntError` (D178, амендмент того же дня).
> - Сняты: `CharFromError`, `InvalidCodepoint`, `str.try_from_codepoint(cp int)` (статика на
>   цели между конкретными типами, ноль вызовов; путь — `cp.to_char()?.to_str()`); экспорт
>   `str.from_codepoint` (непроверяющий кодировщик, невалидный номер → молча пустая строка).
>   `json.nv` для `\uXXXX` использует `decode_surrogate_pair` из `utf16.nv` и `to_char()`.
>   Проверки в `fmt_buf` остаются как страж после `unsafe { n as char }`, не как вторая дверь.
> - Открытое расхождение, не решено здесь: D327 объявляет тип кодовой точки `u32`, а
>   `to_char()` принимает `int`.

> **Амендмент 2026-09-05 (интегратор; приёмка строки реестра №931(а) — «проверяется
> ВСЯ таблица narrowing-конверсий на отсылки к формам, которых нет»).** Таблица выше
> проверена целиком, каждая названная форма — запуском `nova build`, а не чтением.
> Найдено ТРИ мёртвых отсылки сверх той, что сняли 2026-09-04.
>
> **1. ВСТРОЕННОГО `str.from(v)` для примитивов не существует**
> (`[E_UNKNOWN_STATIC_METHOD]`, проба `str.from(42)`). Строка таблицы исправлена на
> `v.to_str()` — и это не выбор из двух, а тот же канон «конверсия — метод на
> ИСТОЧНИКЕ», которым амендмент 2026-08-01 уже заменил `T.try_from(s)` на `s.to_*()`.
> Проба: `(42).to_str()`, `(1.5).to_str()`, `true.to_str()` собираются и печатают
> `42`, `1.5`, `true`.
>
> **ПОПРАВКА 2026-09-05, В ТОТ ЖЕ ДЕНЬ (интегратор; расхождение назвало окно 274).**
> Первая редакция этого пункта говорила «`str.from(v)` не существует» без
> уточнения — и это было ШИРЕ замера. Меряли `str.from(42)`, то есть ВСТРОЕННУЮ
> статику для примитива; ПОЛЬЗОВАТЕЛЬСКАЯ перегрузка `fn str.from(T) -> str` —
> отдельная и живая форма: проба `type Coin { v int }` +
> `fn str.from(c Coin) -> str => "coin:${c.v}"` собирается и печатает `coin:7`.
> Именно её описывает [02-types.md](02-types.md) в блоке Display/fmt
> (`sb.append(str.from(@))`, `fn str.from(MyType) -> str`), и именно её план 274.7
> перечисляет живым путём Display. Ни то, ни другое не противоречит этой таблице:
> таблица про запрещённый `as`-cast ПРИМИТИВА в `str`, и её альтернатива —
> `v.to_str()`. Записано, потому что широкая формулировка уже успела прочитаться как
> «`str.from` снят вообще», а это неверно и стоило бы кому-то удаления рабочего кода.
>
> **2. `char.try_from(n int)` не существует** (`[E_UNKNOWN_STATIC_METHOD]`), хотя
> таблица посылала к нему из ЗАПРЕЩЁННОГО каста, текст ниже повторял его для
> переменных, а пример реализации приводил его тело — как существующее. Это худшее
> место для мёртвой ссылки: читатель приходит сюда из ошибки компиляции.
>
> **ПРОВЕРЯЕМОЙ КОНВЕРСИИ `int` → `char` СЕГОДНЯ НЕТ ВООБЩЕ.** Что есть:
> `str.try_from_codepoint(cp int) -> Result[str, InvalidCodepoint]` (`std`,
> `runtime/string/core.nv:354`; снята 2026-09-05, см. амендмент ниже) — но она даёт `str`, а не `char`; исключение для
> compile-time литерала (`65 as char`, проверка в чекере); `unsafe { n as char }` с
> проверкой диапазона руками — ровно то, чем пользуется сам `std`.
> Значит операция ЗАПРЕЩЕНА, а её названная альтернатива отсутствует.
>
> **ИМЯ РЕШЕНО, И РЕШЕНО НЕ СЕГОДНЯ — ПОПРАВКА К ЭТОМУ ЖЕ АМЕНДМЕНТУ, 2026-09-05.**
> Первая редакция этого пункта объявляла выбор имени открытым («`char.try_from(n)`
> против `n.to_char()`, за владельцем»). Это было НЕВЕРНО: ответ уже стоял в спеке, и
> я его не нашёл, потому что искал среди открытых решений, а не в соседнем амендменте.
> **Амендмент 2026-08-01** (выше в этом же D-блоке) прямо выделяет исключение:
> «Числовые/char narrowing-конверсии (`char.try_from(n int)`, `u8.try_from(c char)`) —
> ОТДЕЛЬНАЯ, ЛЕГАЛЬНАЯ ветка `try_from`, НЕ затронута этим амендментом». То есть канон
> «конверсия — метод на источнике» введён для РАЗБОРА ИЗ СТРОКИ (`s.to_int()`), и та же
> правка сознательно оставила числовое/char сужение на форме `T.try_from(источник)`.
> Спека этим именем и пользуется: `@to_ascii_uppercase`/`@to_ascii_lowercase` описаны
> через `char.try_from(@ as int ±32)`.
>
> **И ветка не гипотетическая — половина её работает.** Проба 2026-09-05:
> `u8.try_from('A')` собирается и исполняется; не существует ровно `char.try_from(n int)`.
> Значит это НЕ открытый вопрос выбора, а НЕРЕАЛИЗОВАННАЯ форма с уже назначенным
> именем — и в строке реестра она стоит в половине (б) как работа по std, а не как
> развилка для владельца. До реализации таблица честно говорит «формы нет».
>
> **3. Отсылки к D77 в прозе вокруг таблицы сняты.** Амендмент 2026-09-04 поправил
> строку кода `i16.try_from(f)?`, но не прозу над ней («используйте `TryFrom`») и не
> фразу под ней («throw-форма доступна через D77»), а D77 ретрагирован 2026-07-06.
> Поправка одной строки не закрывает класс — ровно то, чего оговорка №931 и требовала.
**Исключение для char-литералов:** `'A' as int`, `'A' as u8`
разрешены — программист видит codepoint буквально на
write-time, range-check не нужен.

**Исключение для int-литералов → char:** `0x41 as char`, `65 as char`
разрешены, если литерал — compile-time-known integer в валидном
Unicode-диапазоне `U+0..=U+10FFFF` исключая surrogate range
`U+D800..=U+DFFF`. Range-check выполняется статически в checker'е,
runtime `Fail` не нужен. Off-range литерал — compile error с указанием
конкретного codepoint (не generic suggestion). Для **переменных** типа
`int` правило прежнее — cast запрещён, а проверяемой формы нет (амендмент
2026-09-05 выше). Введено в Plan 14
Ф.7 (2026-05-09).

**Исключение для `unsafe { }` блоков:** внутри `unsafe { }` запрещённые
`as`-cast'ы для переменных разрешены без range-check. Программист берёт
ответственность за корректность значения. Основное применение — реализация
`int @to_char()` (единственная дверь с 2026-09-05; `str.try_from_codepoint` снята) — где range-check уже
выполнен явно перед cast'ом. Пример ниже написан в форме `char.try_from`,
которой НЕ СУЩЕСТВУЕТ (амендмент 2026-09-05 выше) — он показывает форму
тела, а не вызываемое имя:

```nova
export fn char.try_from(cp int) -> Result[char, str] {
    if cp < 0 || cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF) {
        Err("char.try_from: invalid codepoint")
    } else {
        Ok(unsafe { cp as char })
    }
}
```

**Прецеденты.** Rust требует `char::from_u32(n)` (Result), не `n as
char`. Swift `Character.init(extendedGraphemeClusterLiteral)` — нет
прямого `n as Character`. Kotlin `n.toChar()` существует но deprecated
для unsafe usage. Java `(char)n` — narrow с silent overflow (UB-class).
Nova выбирает Rust-стиль strict.

**Bool-restrictions** — то же из Rust/Swift/Kotlin: `if cond` требует
bool, `n as bool` — explicit ошибка с suggestion. Это закрывает
известный bug-class C/JavaScript/Python.

#### Strict `if cond: bool` / `while cond: bool`

`if cond { ... }`, `while cond { ... }`, `cond1 && cond2`,
`cond1 || cond2` — **cond обязан быть `bool`**. C-стиль truthy-int
(`if a` где `a: int`) запрещён.

```nova
ro n int = 5
if n { ... }          // ❌ compile error: cond must be bool
if n != 0 { ... }     // ✅ explicit comparison
```

**Прецеденты.** Rust/Swift/Kotlin/Go (если игнорировать nil-check
shortcut) — все требуют bool. Python/C/JavaScript разрешают truthy —
известный bug-class.

#### `is` — runtime type-check

`is` работает в **двух сценариях**:

1. **`any → T`** — type-check для значений top-type'а `any`.
   Возвращает `bool` (или используется как pattern в match).
2. **`Sum → Variant`** — variant-check для sum-значений: «является
   ли это значение конкретным вариантом sum-типа?» (revision v2).

На остальных «обычных» типах (record без вариантов, primitives,
аносу́ты) `is` — ошибка компиляции: тип известен статически, проверка
бессмысленна.

##### Сценарий 1: `any is T`

**Boolean-выражение:**

```nova
fn dump(x any) Io -> () =>
    if x is int { println("got int") }
    if x is str { println("got str") }
```

**Pattern в `match`:**

```nova
match arg {
    n is int  => process_int(n)         // биндинг + smart cast
    s is str  => process_str(s)
    is bool   => println("bool")        // без биндинга
    _         => throw UnsupportedType
}
```

Pattern-форма: `<binding> is <type>` или `is <type>` (без биндинга).

**Smart cast в `if`:**

```nova
fn process(x any) -> str =>
    if x is str {
        x.upper()              // x здесь имеет тип str автоматически
    } else if x is int {
        str.from(x)             // x здесь int (D73)
    } else {
        "unknown"
    }
```

После `if x is T { ... }` внутри блока компилятор автоматически
уточняет тип переменной до `T` (Kotlin smart cast). Работает если
переменная **не переприсваивается** в блоке.

##### Сценарий 2: `<sum> is <Variant>`

`is` работает на любом sum-значении, проверяя соответствие конкретному
варианту:

```nova
type Shape enum Circle { radius f64 } | Square { side f64 } | Origin

ro s Shape = Circle { radius: 1.0 }

if s is Circle { println("circular") }       // ✅ true
if s is Square { println("squarish") }        // ✅ false
if s is Origin { println("at origin") }       // ✅ unit-вариант

// Также для prelude sum-типов:
ro r Result[int, str] = Ok(42)
if r is Ok    { println("happy path") }      // ✅
if r is Err   { handle_error() }              // ✅

ro opt Option[User] = Some(u)
if opt is Some { ... }
if opt is None { ... }
```

**Без биндинга** — `is` это просто `bool`. Для извлечения значения
из варианта используется pattern-bind `if` (D34), который комбинирует check
и binding в одном выражении:

```nova
// Без биндинга — только yes/no:
if r is Ok { println("ok") }

// С биндингом — pattern-bind if:
if Ok(n) = r { use(n) }
```

Это даёт чёткое разделение:
- **`is`** = «yes/no» (короткий guard).
- **`if Pattern = expr`** = «yes + extract» (binding form, D34 — без `let`).

Поэтому `is` **не поддерживает binding-форму** на sum-типах —
`r is Ok(n)` ошибка, нужно `if Ok(n) = r`. Это согласовано
с D9 «один очевидный путь»: одна форма для одной задачи.

**Реализация:** компилятор знает теги вариантов и эмитит
runtime-проверку tag'а sum-struct'а (`shape->tag == NOVA_TAG_Shape_Circle`).
Стоимость — одно сравнение integer'ов.

**На не-sum / не-`any` — ошибка компиляции:**

```nova
type User { id u64 }
fn process(x User) -> () =>
    if x is int { ... }       // ОШИБКА: User — record, не sum и не any
```

#### Методы на `any` для extraction (комплементарные `is`)

Для pattern-bind-`if`-стиля и работы через эффект `Fail`:

```nova
// Опциональный cast — Option[T]
fn any.try_as[T](x any) -> Option[T] =>
    // runtime-проверка тэга, Some если совпал, None иначе

// Cast через Fail — для строгих случаев
fn any.as[T](x any) Fail[TypeMismatch] -> T =>
    // throw TypeMismatch если тег не совпал
```

Использование:

```nova
// pattern-bind if
if Some(n) = arg.try_as[int]() {
    process_int(n)
}

// ?-стиль
ro n int = arg.as[int]?
```

**Три инструмента под разные сценарии:**

| Способ | Когда применять |
|---|---|
| `match { is T => ... }` | несколько вариантов, exhaustive обработка |
| `if Some(n) = x.try_as[T]()` | один-два типа, mostly happy path |
| `let n = x.as[T]?` | один тип, ожидается этот тип; несовпадение — ошибка |

### Почему

#### Раздельные `as` и `is` — два разных вопроса

`as` — **«как сделать значение типа `T`»** (compile-time, статически
решаемая задача). `is` — **«какой тип у значения сейчас»** (runtime,
нужен для top-type extraction).

В языках, использующих **один оператор** для обоих (Swift `as`/`as?`/`as!`,
C++ `static_cast`/`dynamic_cast`), программист путается. В Nova
разделение явное — два keyword'а с непересекающимися ролями.

#### `is` для `any` и sum-типов — без overhead на остальных типах

`is` работает там, где **runtime-tag уже есть структурно**:

1. **`any`-значения** содержат tag дискриминирующий конкретный тип
   (boxing-цена для top-type — обязательная).
2. **Sum-типы** содержат tag дискриминирующий вариант (это часть
   layout'а sum-struct'а — `tag + payload`).

Для record/primitives/protocol — tag'а нет, и `is` ошибка компиляции:
тип уже известен статически, проверка бессмысленна.

В Kotlin/C# `is T` работает **на любом типе** через RTTI (Runtime
Type Information) — каждое значение несёт type-tag. Это глобальный
overhead. Nova избегает этого: `is` использует **существующие** теги
(any-boxing, sum-discriminant), не добавляет новых. Поэтому стоимость
`is` localized.

**Sum-вариант check vs `match`**:

```nova
// Короткая форма для yes/no:
if shape is Circle { return "round" }

// Полная форма с biding'ом. Варианты выше объявлены RECORD-формой
// (`Circle { radius f64 }`), поэтому и разбираются ФИГУРНО:
if Circle { radius } = shape { use(radius) }

// Exhaustive обработка:
match shape {
    Circle { radius } => ...
    Square { side }   => ...
    Origin            => ...
}
```

> **АМЕНДМЕНТ 2026-08-17 (аудит самосогласованности, раздел 4, пункт 5;
> реестр 221.1 №719).** Здесь стоял ПОЗИЦИОННЫЙ разбор — `Circle(r)`, —
> хотя двумя абзацами выше те же варианты объявлены record-формой
> `Circle { radius f64 }`. Нормативна фигурная форма: таблица форм
> `spec/syntax.ru.md` («`( ... )` — позиционный вариант, `{ ... }` —
> record-вариант») и канонический разбор того же `Shape` в
> [02-types.md](02-types.md). 
>
> Это не опечатка без последствий: форма приехала ИЗ ЭТОГО ПРИМЕРА в
> компилятор — чекер её принимал, а кодоген падал `no member named '_0'`,
> то есть ошибка чужого компилятора в лицо пользователю. Отказ
> `E_RECORD_VARIANT_POSITIONAL_PATTERN` заведён той же волной, и заведён в
> ЕДИНОМ обходе образцов — иначе `if Circle(r) = s` остался бы живым мимо
> проверки в `match`. Починить компилятор и оставить пример значило бы
> продолжать учить неверной форме.

Каждая форма для своего сценария: `is` — guard, pattern-bind `if` — guard +
extract, `match` — exhaustive multi-way.

#### Smart cast — стандартная эргономика

`if x is T { x.method_of_T() }` без явного re-binding — фича Kotlin,
TypeScript narrowing, C# pattern matching, Swift binding-pattern. Все
сообщества **любят** smart cast, и этого не избегают.

#### Прецеденты ключевых слов

- **`as`**: Rust, Swift, C#, Kotlin, TS — для cast (numeric и иначе).
  Nova берёт это значение.
- **`is`**: C# (`x is T`), Kotlin (`x is T`), TS (`typeof`/`instanceof`,
  но не `is` — `is` в TS это type predicate). F# использует `:?`,
  что менее красиво. Nova берёт C#/Kotlin-стиль.

### Что отвергнуто

- **Один оператор для cast и type-check** (Swift `as?`/`as!`).
  Усложняет mental model, путает пользователя.
- **`is T` для любого типа без tag'а** (Kotlin-style RTTI). Требует
  runtime-tag на всех значениях — глобальный overhead. Nova ограничена
  типами, у которых tag **уже есть структурно** (`any`-boxing,
  sum-discriminant). Для record/primitives — compile error.
- **`is Variant(binding)` с биндингом на sum-типах.** Дублирует
  `if Variant(binding) = expr` (D34). Чтобы избежать двух форм
  для одной задачи — `is` без binding, pattern-bind `if` с binding.
- **`x.is[int]()` метод** вместо оператора. Менее читаемо в условиях
  (`if x.is[int]()`-запись хуже `if x is int`). Operator проще.
- **`as` для `any → T`** без runtime-проверки. Type-небезопасно
  (программист может написать `x as int` для `x any` без гарантии).
  Используйте `is` или `try_as[T]`.
- **Implicit cast** между типами без `as`. Все конвертации явные.
- **Flow-sensitive narrowing на `!is`** в MVP. Для `if !(x is T)
  { return }` после блока `x` **не** уточняется автоматически. Можно
  расширить позже.

### Цена

1. **Два keyword'а** в синтаксисе языка вместо одного. `is` ранее
   не использовался — теперь зарезервирован.
2. **Runtime-tag** для `any`-значений — стоимость в реализации
   (memory overhead на boxing).
3. **Smart cast** требует поддержки в type-checker — переменная имеет
   разный тип в разных ветках одной функции. Усложняет реализацию.
4. **`try_as[T]()` и `as[T]?`** — два метода stdlib на `any` поверх
   оператора `is`. Нужно зафиксировать в prelude (D26).

### Связь
- [02-types.md → D52](02-types.md#d52) — newtype, sum, discriminants —
  типы, для которых `as` определён.
- [02-types.md → D53](02-types.md#d53) — `any` как пустой
  protocol-тип, для которого работает `is`.
- [D44](#d44-числовые-литералы) — numeric `as`-cast (`100 as u32`)
  как частный случай D54.
- [D34](#d34-pattern-bind-в-ifwhile-conditions--unified-grammar-с-match-arms) —
  `if Some(n) = x.try_as[T]()` использует pattern-bind-форму (D34).
- [D19](#d19-match-arms-через--не--) — `=>` в match-arms,
  `is`-pattern наследует ту же стрелку.
- [08-runtime.md → D26](08-runtime.md#d26) — `try_as` и `as` методы
  на `any` в prelude.

### Открытые вопросы
- **Flow-sensitive narrowing на `!is`** — можно ли после `if !(x is
  T) { return }` уточнять тип в продолжении функции? Отложено.
- **`is` для protocol-types** (runtime structural check) — дорого,
  не входит в MVP.
- **`is` для error/cancel-detection в `Result[T, E]`.** `r is Err`
  работает (variant check), но иногда хочется проверить конкретный
  payload — `r is Err(NotFound)`. Сейчас это не поддерживается
  (binding запрещён), нужно `if Err(NotFound) = r`.

### Эволюция

**v1:** `is` работал только для `any`-значений. Sum-варианты
проверялись через `match` или pattern-bind `if` — короткой `is`-формы не было.
Это вынуждало писать convention `@is_circle()` методы для часто
проверяемых вариантов, что засоряет API типов.

**v2 (текущая, 2026-05-06):** `is` расширен на sum-варианты —
`shape is Circle` работает. Cтоимость localized: tag для sum уже
есть в layout'е, никакого нового runtime-overhead'а. Биндинг-форма
**не** добавлена — это работа pattern-bind `if` (D34); чёткое разделение
ролей: `is` = yes/no, pattern-bind `if` = yes + extract.

Это убрало нужду в `@is_X` convention'ах из syntax.md.

### Эволюция
До D54 `as` использовался без формального D-решения (упоминался в
D44, D52). D54 фиксирует семантику явно: `as` — compile-time
конвертация; `is` — runtime type-check. Закрывает Q-any-extract
(извлечение типа из `any`-значения).

### Реализация `is`/`try_as` на `any` (Plan 174.3, 2026-07-04)

**v1 `any`-downcast** (был спроектирован, но не реализован в codegen —
`any` не имел рабочего value-представления) теперь реализован поверх
type_id-реестра Plan 61.

**Runtime-представление `any`** (D53). `any` — тип-стёртый `void*`,
указывающий на heap-boxed `NovaAny`:

```c
typedef struct { NovaTypeId type_id; const char* name; } NovaTypeInfo;
typedef struct { const NovaTypeInfo* info; void* data; } NovaAny;
```

`info` — per-type статический `NovaTypeInfo` (несёт `type_id` из реестра
Plan 61 + имя для Display/диагностик); `data` — отдельная GC-allocation с
копией payload. И box, и payload сканируются консервативным GC. `any`
остаётся `void*` в ABI (нулевой blast-radius на erased-void*-код:
`print`/`println`, generic-параметры).

**Операции:**

- **upcast `T → any`** (boxing): `nova_any_box(&NOVA_TYPEINFO_<T>, &v, sizeof(T))`.
  Явный `v as any` **и** неявный по D53-supertype — аргумент к `any`-параметру,
  `ro x any = <concrete>`, `fn … -> any => <concrete>` — боксируются автоматически.
- **`x is T`** (`x: any`): `nova_any_is(x, NOVA_TID_<T>)` — сравнение `type_id`.
- **`x.try_as[T]() -> Option[T]`**: при совпадении `type_id` — `Some(*(T*)nova_any_data(x))`,
  иначе `None`.
- **flow-narrowing** `if x is T { … }`: внутри блока `x` уточняется до `T`
  (Kotlin smart-cast) — payload-deref, aliased через `#define`.

**Диагностика `[E_IS_NON_ANY]`** (чекер): `is` на операнде, чей тип статически
известен как **не-`any` и не-sum** (record / примитив) — ошибка компиляции
(проверка бессмысленна). Чистый span+код, не CC-FAIL.

**Отложено** (followup `[M-174.3-*]`): форма `x.as[T]?` (Fail-downcast —
парсер не принимает `.as` member, `as` — ключевое слово); `match { n is T => … }`
pattern-форма (реализована операторная/`if`-форма + `try_as`); гетерогенные
`[]any` + Display (Ф.3).

---

## D58. Range-литерал, `Iter[T]` protocol, `for x in c` implicit iter

> **Amend (Plan 138):** правило `for x in c` изменено — `iter()` всегда первым.
> Итераторы обязаны реализовывать `iter() -> Self` (trivial).
>
> **Amend (Plan 152.1 / D250, 2026-06-13):** `for c in s` (`s: str`) → `char` через
> `str @iter() => @as_chars()` (`CharsIter`, codepoint-итератор). Default-единица
> итерации строки — codepoint (как Go `for range`, Swift `for c in s`). Codegen:
> str's C-тип `nova_str` (lowercase, lang-item) маппится в for-in на ключ "str" →
> Case 2 синтезирует `s.iter()`. Байтовая итерация — явно `for b in s.as_bytes()`.

### Что
Три связанных правила, объединённых одним D-блоком, потому что они
взаимно поддерживают друг друга:

1. **`a..b` и `a..=b` — литералы Range** в любой expression-позиции
   (не только в `for`). **Open-ended формы `a..`, `..b`, `..=b`, `..`** —
   расширение Plan 96 ([D144](02-types.md#d144)): **только** в slice-
   context (`arr[range]`). В materialize / for-loop / quantifier /
   parallel-for — compile-error (нужна bounded форма).
2. **`Iter[T]`** — структурный protocol в prelude (D26):
   `protocol { mut next() -> Option[T] }`. Любой тип с таким методом
   — итератор.
3. **`for x in c`** — всегда через `iter()`. Десугаринг:
   `{ mut _it = c.iter(); loop { match _it.next() { Some(x) => body, None => break } } }`.
   Итераторы реализуют `iter() -> Self` (trivial) — единая точка входа.

### Правило

#### Range-литералы

```nova
ro r1 = 0..5             // Range { start: 0, end: 5 }   half-open [0, 5)
ro r2 = 0..=5            // Range { start: 0, end: 6 }   нормализован компилятором

ro r Range = 1..10       // в ro-binding'е работает
fn count(r Range) -> int => r.end - r.start
count(0..100)              // в позиции аргумента работает

ro ranges []Range = [0..5, 10..20, 100..200]   // в массиве
```

`a..b` — синтаксический сахар, разворачивается компилятором в
`Range { start: a, end: b }`. `a..=b` → `Range { start: a, end: b+1 }`
(нормализуется, `inclusive` не хранится).

Конструкторов `Range.exclusive`/`Range.inclusive` нет — лишний API.

**Range — value record** (Plan 138 Ф.0.3):

```nova
export type Range value {
    ro start int
    ro end   int      // half-open: end НЕ включён
}
```

Stack-allocated, 16 bytes. Методы: `@iter()`, `@contains(x)`, `@len()`, `@is_empty()`.

#### `Next[T]` + `Iter[I]` протоколы (D241+D242)

Конвенция: имя протокола = магический метод.

```nova
// Итератор — умеет выдавать следующий элемент.
export type Next[T] protocol {
    mut @next() -> Option[T]
}

// Источник итератора — умеет превращаться в итератор.
// I — конкретный тип итератора (реализует Next[T] для некоторого T).
export type Iter[I] protocol {
    @iter() -> I
}
```

Итераторы реализуют **оба** протокола: `Next[T]` + `Iter[Self]` (trivial `=> self`).
Коллекции реализуют только `Iter[SomeIter]`.

```nova
// Итераторы:
fn RangeIter mut @next() -> Option[int] => ...
fn RangeIter @iter() -> RangeIter => self        // Iter[RangeIter]

fn VecIter[T] mut @next() -> Option[T] => ...
fn VecIter[T] @iter() -> VecIter[T] => self      // Iter[VecIter[T]]

// Коллекции — только @iter():
fn Range @iter() -> RangeIter => ...             // Iter[RangeIter]
fn Vec[T] @iter() -> VecIter[T] => ...          // Iter[VecIter[T]]
```

Generic bound для «принять любой iterable»:

```nova
fn collect[C Iter[I], I Next[T], T](c C) -> Vec[T] {
    mut result = Vec.new()
    for x in c { result.push(x) }
    result
}
```

#### `for x in c` — implicit iter

`for-loop` принимает **любое выражение справа от `in`**, разворачиваясь
по правилу:

```
for x in c { body }
```

десугарится компилятором **двухфазно** (как Rust `IntoIterator` + `Iterator`):

```
Фаза 1: mut _it = c.iter()          // всегда вызвать iter() один раз
Фаза 2: loop { match _it.next() { Some(x) => body, None => break } }
```

Правило:
1. Если `c` имеет `iter()` — вызвать, получить итератор, перейти к фазе 2.
2. Если `c` не имеет `iter()` но имеет `next()` — fallback: использовать
   напрямую (backward compat; рекомендуется добавить `iter() -> Self`).
3. Иначе — ошибка компиляции.

Итераторы реализуют `iter() -> Self` — поэтому `for x in iter_obj` тоже
работает через единый путь: `iter_obj.iter()` возвращает тот же объект,
фаза 2 вызывает `next()`. Нет бесконечного цикла: компилятор вызывает
`iter()` ровно один раз (фаза 1), дальше только `next()`.

```nova
ro v []int = [1, 2, 3]
for x in v { ... }                   // v.iter() → VecIter → next()

ro r = 0..5
for x in r { ... }                   // r.iter() → RangeIter → next()
for x in 0..5 { ... }                // то же

mut it = v.iter()
for x in it { ... }                  // it.iter() → self → next() (trivial)
```

### Почему

1. **Range как expression — естественно.** В for-loop `0..n` уже
   работает. Расширение на любую expression-позицию устраняет
   асимметрию: «range можно в for, но не в let». Прецедент Rust,
   F#, Haskell, Scala.
2. **`Iter[T]` как protocol — fits structural typing.** Никакого
   специального механизма, обычный protocol с одним методом.
   Прецедент Rust `Iterator`-trait, OCaml `Seq.t`, Python `__iter__`.
3. **`for x in c` без `.iter()` — стандарт mainstream.** Kotlin,
   Swift, Python, C#, Rust (через `IntoIterator`) — везде sugar.
   Только Go требует `range`-keyword.
4. **AI-friendly.** `for x in c` короче, чем `for x in c.iter()`.
   Меньше boilerplate, меньше ошибок «забыл `.iter()`».

### Что отвергнуто

- **Range только в for-loop** (текущая ситуация до D58). Ограничивает
  использование — нельзя передать range как аргумент, сохранить в
  переменную.
- **`Range` как примитив языка** (без Range-типа в stdlib). Полезно,
  но изоляция от системы типов хуже — нельзя добавить методы,
  написать функцию, принимающую Range.
- **`for x in c` строгое — только Iter[T]** (без implicit `iter()`
  сахара). Программист пишет `for x in v.iter()` каждый раз,
  избыточно.
- **`for-in` через специальный keyword (Go `range`).** Лишний
  синтаксис, нет преимущества над implicit iter через protocol.

### Цена

1. **Range type в prelude.** Расширение D26 (prelude растёт).
2. **`a..b` как expression.** Парсер должен понимать `a..b` в
   любой expression-позиции, не только в for. Лёгкая правка
   грамматики.
3. **`for-in`-сахар.** Компилятор делает desugaring `for x in c`
   → выбор `c.iter()` vs использование `c` напрямую. Простое
   правило, но требует type-resolution.
4. **`Iter[T]` имя.** Короткое, но конфликтует с потенциальными
   user-defined type'ами `Iter`. Согласовано с [D30](#d30) (типы
   PascalCase).

### Связь
- [02-types.md → D42](02-types.md#d42), [D53](02-types.md#d53)
  — `Iter[T]` как обычный protocol через структурную типизацию.
- [D38](#d38-создание-массивов-и-turbofish-для-дженериков) — `0..n` как
  range-выражение в существующем синтаксисе for-loop.
- [08-runtime.md → D26](08-runtime.md#d26) — `Iter[T]`, `Range`,
  `RangeIter` в prelude.
- [02-types.md → D355](02-types.md#d355--blanket-protocol-receiver-methods-plan-161-2026-06-15) — blanket methods на `Next[T]` implementors: `fn[I Next[T]] I @m` диспетчируется на любой `C` impl `Next[T]`; ≤1 impl инвариант (D355 §4, ex-D282). Cross-ref: D241+D242 (Plan 161 Ф.4).

### Открытые вопросы
- **Reverse range** (`5..0` или `(0..5).reverse()`) — что значит
  range с `start > end`? Пустой? Идущий назад? — открытый
  Q-range-extras.
- **`(0..5).step(n)`** — step-итерация. Q-range-extras.
- **`collect[Out]()` generic-collection-construction** — требует
  bound'ов (Q-bounds) и static-method-protocol. Q-collect-mechanism.
- **Type-as-value** (передача типа как значения, `xs.collect([]int)`)
  — отдельный вопрос Q-type-as-value.
- ~~**`@`-префикс в protocol-методах**~~ —
  ✅ **RESOLVED** Plan 108.4 (2026-06-09). `@` обязателен перед instance-методами
  в protocol declarations. `ro`/`mut`/`consume` prefix перед `@` для receiver
  mutability. Default = `ro`. Type-checker enforces impl match. См. [D209](04-effects.md#d209--protocol-method--syntax--receiver-mutability-plan-1084-2026-06-09).
- ~~**Static-метод в protocol через `.method()`-префикс**~~ —
  ✅ **RESOLVED** Plan 97 (2026-05-23). Leading-точка `.method(args) -> Ret`
  в `protocol {}` теле помечает метод **статическим** (симметрично D35
  `fn Type.name`); реализация ожидается через `fn Type.method(...)`.
  `From`/`TryFrom` обновлены под новый синтаксис (`.from(t T) -> Self`/
  `.try_from(t T) -> Result[Self,E]`). Hard-enforcement static↔instance
  mismatch — followup.

> ⚠️ **D58 AMENDED by Plan 108.4 (2026-06-09)** — Protocol declaration of
> `Iter[T]` / `Iterable[T]` now uses `mut @next() -> Option[T]` (explicit `@`
> + `mut` receiver). The `@`-prefix is required for all protocol instance-methods
> (D209). Use-site structural conformance (for-in loop, `[T Iterable[U]]` bound)
> checks receiver_mut: a type that declares `@next()` (ro) does NOT satisfy
> `Iterable[T]`. Bootstrap parser limitation comment in `std/prelude/collections.nv`
> has been removed. All existing implementers (`Counter`, `RangeIter`, `VecIter`,
> etc.) already used `mut @next()` — no implementer changes required.

---

## D59. Array, tuple и позиционные partial patterns

### Что
Pattern matching на массивах (`[]T`), кортежах (`(A, B)`) и
позиционных конструкторах sum (`Cons(T, T')`). Покрывает разрозненные
фичи, которые **уже использовались в examples** (`[]`, `[r]`,
`[_, ..]`, `Cons(..)`), но не были формально зафиксированы.

`..` (rest-pattern) — единый маркер «остальные элементы игнорируются»
во всех трёх контекстах: record (`{ field, .. }` — D17/D52),
позиционные конструкторы (`Cons(..)`, `Click(x, ..)`), массивы
(`[head, ..]`, `[.., last]`, `[a, .., z]`).

### Правило

#### Array patterns

```nova
match xs {
    []           => "empty"                  // пустой массив
    [x]          => "one: ${x}"               // ровно 1 элемент, bind в x
    [a, b]       => "two: ${a}, ${b}"          // ровно 2
    [a, b, c]    => "three: ..."                // ровно 3
    [head, ..]   => "first: ${head}"            // ≥1, bind первого
    [.., last]   => "last: ${last}"             // ≥1, bind последнего
    [a, .., z]   => "first/last: ${a}, ${z}"   // ≥2, bind первого+последнего
    [_, ..]      => "non-empty"                  // ≥1, без bind
    [_, _, third]=> "exactly third"              // ровно 3, bind третьего
    _            => "other"                       // wildcard
}
```

**Правила:**

1. **Ровные позиции** (`[a, b]`, `[a, b, c]`) — соответствуют точной
   длине.
2. **`..` rest-pattern** — означает «0 или больше элементов».
   Допустим в позициях:
   - `[items, ..]` — head + остальное.
   - `[.., items]` — остальное + last.
   - `[a, .., z]` — head + middle (игнорируется) + last.
3. **`..items` с биндингом** — biind остатка как массива:
   ```nova
   match xs {
       [head, ..rest] => process(head, rest)    // rest : []T
       [.., last]     => last                     // без bind остального
   }
   ```
4. **`_` placeholder** — игнорировать один элемент, точно как в record.
5. **Не более одного `..` в массиве-pattern** — иначе ambiguous
   (Rust то же правило).

#### Tuple patterns

```nova
ro p = (1, "alice", true)

match p {
    (1, _, true)        => "first variant"
    (n, name, _)        => "n=${n}, name=${name}"
    _                   => "other"
}

ro (a, b, c) = (1, 2, 3)                  // destructuring ro
ro (x, _, z) = (1, 2, 3)                   // ignore middle
```

**Правила:**

1. **Tuple-pattern** соответствует точно — длина фиксирована типом.
2. **`..` в tuple запрещён** (длина известна на этапе типизации,
   `..` не нужен).
3. Деструктуризация в `let` через tuple-pattern — поддерживается.

#### Positional sum-variant partial-pattern

```nova
type LinkedList[T] | Empty | Cons(T, LinkedList[T])

match list {
    Empty       => "nil"
    Cons(h, _)  => "head only"                  // явный _ для tail
    Cons(..)    => "non-empty"                   // partial: оба поля игнорируются
    Cons(h, ..) => "head: ${h}"                  // bind первого, остальное ..
}

type Event enum Click(int, int) | Move(int, int, int) | Idle

match event {
    Idle             => "idle"
    Click(..)        => "click"
    Move(x, ..)      => "move at x=${x}"
    Move(.., z)      => "move with z=${z}"
    _                => "other"
}
```

**Правила:**

1. **`..` в позиционном конструкторе** работает так же, как в
   массиве: head/tail/middle-rest.
2. **Один `..` на конструктор**.
3. Согласовано с D17/D52 partial-pattern для record-форм.

### Почему

1. **Используется в examples.** `effect-density/repository.nv`,
   `orm_demo.nv`, `stdlib_linkedlist.nv` уже **активно** применяют
   `[]`, `[r]`, `[_, ..]`, `Cons(..)`. Без формализации парсер не
   знает грамматику, LLM не знает правила, code review не имеет
   опоры.
2. **Прецедент Rust.** Array/tuple/sum-positional patterns в Rust
   имеют точно такой синтаксис (`[]`, `[head, ..]`, `[.., tail]`,
   `Variant(..)`). Программисты с Rust-фоном узнают мгновенно.
3. **Единый `..` для всех partial-форм.** Record (D17/D52),
   позиционный sum, массив — везде `..` означает «остальное
   игнорируется». Один концепт.
4. **Tuple destructuring в `let`** — стандартная фича современных
   языков (Rust/Swift/Kotlin/Python).

### Что отвергнуто

- **`Cons(_, _)` как единственная форма** для позиционного sum.
  Шумно для конструкторов с 3+ полями (`Move(_, _, _)`). С `..`
  → `Move(..)`.
- **Cons-list pattern (`head :: tail`)** для массивов, как в
  Scala/OCaml. Nova не имеет cons-семантики массивов — `[]T` это
  slice, не linked list. Используем bracket-syntax.
- **Multiple `..` в одном pattern** (`[a, .., b, .., c]`).
  Ambiguous — какое `..` сколько элементов берёт? Запрещено.
- **`..` в tuple-pattern.** Длина tuple фиксирована, `..` не несёт
  информации. Запрещено для строгости.
- **Slice-binding `[head, ..rest]`** с типом `rest : []T` — частично
  отложено. **Bind через `..items`** (без значения по умолчанию)
  поддерживается. Расширения вроде `[a, b, ..rest, c, d]` (rest в
  середине с bind) — не в MVP.

### Цена

1. **Парсер усложняется** — три новых формы pattern (array, tuple,
   positional-rest). Стандартное расширение, прецедент Rust.
2. **Exhaustiveness check для массивов сложнее.** Длина
   динамическая, компилятор не может проверить «все случаи покрыты»
   как для sum-вариантов. **Wildcard `_` обязателен** в array-match,
   если не покрыты все возможные длины (которых бесконечно). Это
   как в Rust.
3. **`..items` slice-binding** требует runtime-аллокации сегмента
   массива (`rest : []T`). В zero-copy случае — `rest` это slice
   (start, len). Согласовано с [D32](02-types.md#d32) (slice-семантика).

### Связь
- [D17](02-types.md#d17), [D52](02-types.md#d52) — partial-pattern
  `..` для record-форм. D59 расширяет на массивы и позиционные
  конструкторы.
- [D27](#d27-синтаксис-массивов-t-префикс-nt-фиксированные) — `[]T`
  как тип, на котором работают array-patterns.
- [D34](#d34-pattern-bind-в-ifwhile-conditions--unified-grammar-с-match-arms) —
  pattern-bind в условиях; array/tuple-patterns доступны и в
  pattern-bind `if`/`while` (D34).
- Закрывает Q-positional-partial-pattern.

### Открытые вопросы
- **`[a, b, ..rest, c]`** — rest в середине с bind. Не в MVP.
- **Slice-bind на массиве с `[]int.alloc(...)` vs zero-copy slice**
  — деталь runtime, не дизайн.
- **String-as-array patterns** (`match s { "hello" => ..., _ =>
  ... }` — strings как массивы char) — отдельный вопрос
  Q-string-patterns.

---

## D60. Spread `...x` в литералах: массив и record

### Что
Оператор `...` (три точки) внутри array- и record-литералов
**вставляет элементы/поля** из существующего значения. Двойственная
к D59 partial-pattern: D59 **разбирает**, D60 **строит**.

```nova
ro arr1 = [1, 2, 3]
ro arr2 = [0, ...arr1, 4]                  // [0, 1, 2, 3, 4]

ro user1 = User { id: 1, name: "alice", email: "a@x.com" }
ro user2 = { ...user1, name: "bob" }        // copy + override name
```

### Правило

#### Array spread

```nova
ro a = [1, 2, 3]
ro b = [4, 5]

ro c = [...a, ...b]                         // [1, 2, 3, 4, 5]
ro d = [0, ...a, ...b, 6]                    // [0, 1, 2, 3, 4, 5, 6]
ro e = [...a]                                // копия (не reference)
```

**Правила:**

1. **Источник `...src`** должен быть `[]T`, где `T` совпадает с типом
   элементов целевого массива.
2. **Несколько spread'ов** в одном литерале разрешены: `[...a, ...b,
   ...c]`.
3. **Смешивание spread и обычных элементов** — в любом порядке:
   `[1, ...a, 2, ...b, 3]`.
4. **Стоимость:** O(total length) — концептуально concatenation.
   Компилятор может оптимизировать (пред-аллокация по сумме длин).

#### Record spread

```nova
type User { id u64, name str, email str, role str }

ro alice User = { id: 1, name: "alice", email: "a@x.com", role: "user" }

// Override одного поля:
ro alice2 = { ...alice, name: "ALICE" }

// Override нескольких:
ro admin_alice = { ...alice, role: "admin", email: "admin@x.com" }

// Все поля из spread — то же значение:
ro copy = { ...alice }                       // эквивалентно alice (но новый record)
```

**Правила:**

1. **Источник `...src`** должен быть **того же типа**, что и target
   (или иметь совпадающее множество полей).
2. **Override:** явные `field: value` после `...src` **перезаписывают**
   значения из spread. Порядок в литерале — left-to-right.
   ```nova
   ro r = { ...src, name: "new", ...override, id: 99 }
   //           ↑       ↑          ↑           ↑
   //  src.все   override("name")  override.все  override("id"=99)
   ```
3. **Все required-поля должны быть покрыты** — компилятор проверяет.
   Если spread + явные не дают полного покрытия — ошибка.
4. **Один spread** на record-литерал в MVP. `{ ...a, ...b }` —
   отложено (нужны правила приоритета).
5. **Тип источника:** в MVP — **строго тот же тип**, что target. В
   будущем — может быть подтип/совпадение по полям (требует
   structural-subtyping, Q-anonymous-union).

#### Совместимость с D52 literal coercion

```nova
type User { id u64, name str }

ro u User = { id: 1, name: "alice" }              // D52 record-coercion
ro u2 User = { ...u, name: "bob" }                 // D60 spread + D52 coercion
ro u3 User = { ...u }                              // полный copy через spread
```

В позиции с явным целевым типом spread работает с D52-coercion: имя
типа подразумевается из аннотации.

#### Совместимость с D17/D52 field punning

```nova
ro name = "bob"
ro u User = { ...other, name }                     // shorthand + spread
```

Field punning ([D52](02-types.md#d52)) работает после spread — если
имя поля совпадает с переменной в scope, shorthand обязателен.

### Почему

1. **Immutable update.** В функциональном стиле (доминирующем в
   Nova: `mut` через эффект, GC по умолчанию) immutable-обновление
   record — частая операция. Без spread:
   ```nova
   ro u2 = User { id: u.id, name: "bob", email: u.email, role: u.role }
   ```
   С spread: `{ ...u, name: "bob" }`. **Краткость + защита от
   ошибок** (если в `User` добавилось поле, программист **не должен**
   обновлять каждый use-site).

2. **Concatenation массивов.** `[head, ...rest]` — элегантнее
   `[head].concat(rest)` или ручного цикла.

3. **Прецедент TypeScript.** `...spread` массово используется в
   современном TS/JS. Программисты знают.

4. **Симметрия с D59 partial-pattern.** D59 разбирает значение через
   `..`, D60 строит через `...`. Концептуально — две стороны одной
   медали. Разные токены (`..` vs `...`) убирают синтаксическую
   путаницу.

5. **AI-friendly.** LLM генерирует `{ ...other, name: "bob" }` —
   очевидное намерение, нет boilerplate.

### Что отвергнуто

- **`..` (две точки)** для spread (Rust struct-update style). Конфликт
  с range-литералом ([D58](#d58-range-литерал-iterator-protocol-for-in-implicit-iter))
  и rest-pattern ([D59](#d59-array-tuple-и-позиционные-partial-patterns)).
  Парсер мог бы различать по контексту, но **`...` (три точки)**
  однозначен и согласован с TS-прецедентом.
- **`*arr`/`**obj`** (Python-style). Два разных оператора для
  array vs record — лишнее. Один `...` для всего.
- **`{ src with name = "bob" }`** (OCaml-style `with`-keyword).
  Новый keyword, менее знакомый, не симметричен с array-spread.
- **Multiple record-spread `{ ...a, ...b }`** в MVP. Семантика
  «правый перезаписывает» интуитивна, но требует продумать
  edge-cases (что если поле есть в обоих и target требует один тип
  — компилятор должен проверить). Отложено до measured-need.
- **Spread в pattern-position** (`match xs { [1, ...rest, 5] => ... }`).
  D59 уже даёт `[head, ..rest]` через две точки — отдельный
  механизм для destructuring. `...` остаётся **только для
  construction**.
- **Spread с подтипом.** В MVP target и source строго одного типа.
  Расширение — Q-spread-subtype.

### Цена

1. **Парсер расширяется** — `...expr` в array/record литералах.
   Стандартное расширение, прецедент TS.
2. **Type-checker** проверяет покрытие required-полей при spread в
   record. Не сложнее, чем уже есть для D55 literal coercion.
3. **Runtime cost array-spread** — O(total length). Программист
   знает (концептуально concat).
4. **Runtime cost record-spread** — O(field count) копирование
   полей. Минимально, по аналогии с обычным record-литералом.

### Связь
- [D52](02-types.md#d52) — record-coercion. D60 расширяет: spread в
  позиции с явным типом тоже coerce'ится.
- [D17/D52 field punning](02-types.md#d17) — `{ ...src, name }`
  shorthand работает после spread.
- [D58](#d58-range-литерал-iterator-protocol-for-in-implicit-iter) —
  `..` (две точки) для range. D60 использует `...` (три точки) для
  spread — разные токены, нет конфликта.
- [D59](#d59-array-tuple-и-позиционные-partial-patterns) —
  partial-pattern `..` в **destructuring**. D60 — spread `...` в
  **construction**. Двойственные операции, разные синтаксисы.
- [D27](#d27-синтаксис-массивов-t-префикс-nt-фиксированные) — `[]T`
  как тип, на котором работает array-spread.

### Открытые вопросы
- **Multiple record-spread** (`{ ...a, ...b, ... }`) — отложено.
- **Spread с подтипом/совпадением полей** — Q-spread-subtype.
- **Spread в tagged template literal args** — нет в MVP, не нужен.
- **Tuple-spread** (`(1, ...t, 5)`) — длина tuple фиксирована типом,
  spread даёт компилятору всю информацию. Не вводится в MVP за
  ненадобностью.

---

## D69. Variadic-параметры через `...items []T`

### Что
Последний параметр функции может быть помечен префиксом `...` —
параметр объявляет, что **на call site** его можно вызвать одним из
двух способов:

1. **Через spread** существующего массива: `f(...arr)`.
2. **Через отдельные элементы**: `f(a, b, c)` — компилятор соберёт
   их в `[]T`.

Тип параметра — обычный `[]T`. Внутри функции `items` это `[]T`,
никакой специальной семантики.

### Правило

#### Декларация

```nova
fn print[T](...items []T) Io -> () {
    for x in items {       // items: []T внутри функции
        Io.write(str.from(x))
    }
}

fn fmt(template str, ...args []str) -> str {
    // template — обычный параметр; args — variadic []str
    ...
}
```

Грамматика:

```
param = [ '...' ] name type
```

`...` допустим **только перед последним параметром**. Тип после `...`
обязан быть `[]T` (или `[]Type` любой формы) — не element type.

#### Call site

```nova
// Способ 1: spread массива
ro names = ["alice", "bob"]
print(...names)            // эквивалентно print("alice", "bob")

// Способ 2: отдельные элементы
print("alice", "bob")      // компилятор собирает в ["alice", "bob"]

// Микс — spread в любой позиции после обычных аргументов
print("prefix", ...names, "suffix")
//      ↑          ↑          ↑
//      обычный    spread     обычный
//      → результат: ["prefix", "alice", "bob", "suffix"]
```

Spread на call site можно использовать **только** для variadic-параметра.
Для обычного `items []T` параметра spread не разрешён —
программист передаёт массив явно: `f(["a", "b"])`.

### Семантика

- `...items []T` в декларации — это **синтаксический marker**, не
  новый тип. Тип `items` это `[]T`.
- На call site spread `...arr` разворачивает `arr: []T` в позиционные
  аргументы.
- Без spread'а: компилятор собирает все аргументы в `[]T` неявно
  (compile-time, zero overhead).
- **Только последний** параметр может быть variadic — упрощает
  парсинг и неоднозначности.
- **Type checking**: каждый аргумент проверяется против element type
  `T`; spread-выражение должно иметь тип `[]T`.

#### Generic-variadic

```nova
fn first[T](...items []T) -> Option[T] {
    if items.len() == 0 { None } else { Some(items[0]) }
}

first(1, 2, 3)             // T = int
first("a", "b")            // T = str
first(...["x", "y"])       // T = str через spread
```

`T` выводится из элементов или spread-массива.

#### Heterogeneous-variadic через `any`

Когда нужен `print("count=", 42, " items")` (разные типы):

```nova
fn print(...items []any) Io -> ()
```

`any` — top-type из [D54](#d54). Каждый элемент конвертируется в
строку через `str.from(v)` ([D73](08-runtime.md#d73)). Это разрешает
`print` принимать смешанные типы без T-параметра.

### Что НЕ делается

- **Variadic не последним параметром** (`fn f(...xs []int, last str)`).
  Усложняет грамматику без выгоды; в крайнем случае программист
  переставляет параметры.
- **Несколько variadic-параметров** — нет смысла.
- **Keyword args** (Python `**kwargs`) — отдельная фича, не нужна
  для variadic use-case.
- **Postfix-синтаксис как в Go** (`items ...string`). Префикс `...`
  единый для всех spread'ов в Nova ([D60](#d60-spread-в-литералах-arr-record)
  для массивов, D69 для variadic) — symmetric.
- **Element-type как в Go** (`...items T`). Декларация показала бы
  «items: T» с magic-преобразованием в []T. Nova предпочитает
  явный array-type без скрытой обёртки.

### Почему

1. **D60 symmetry.** В литералах массивов уже используется prefix
   `...arr` для spread. Variadic-call-spread `f(...arr)` — та же форма.
2. **D40 «один способ».** Нет «двух типов в одной декларации»
   (element vs array как в Go). Тип параметра = `[]T`, конец.
3. **TypeScript-прецедент.** Самый популярный variadic-синтаксис в
   современных языках, LLM знает.
4. **AI-friendly.** Сигнатура `(...items []T)` сразу показывает:
   - `...` → variadic;
   - `[]T` → точный тип параметра;
   - element type выводится естественно.
5. **Минимальные изменения грамматики.** Парсер уже распознаёт `...`
   в spread-литералах (D60). Расширение на параметры функции —
   маленькое дополнение.

### Что отвергнуто

- **Без variadic вообще** (всегда явный `f([a, b, c])`). Отвергнуто:
  частые отладочные `print(...)` стали бы шумнее. Variadic — конкретное
  улучшение DX.
- **Macro-style** (`println!`-как-в-Rust). Отвергнуто: у Nova нет
  macro-системы; добавлять её только ради variadic — overkill.
- **Variadic через Java-style autoboxing** (`Object...`). Отвергнуто:
  no implicit boxing в Nova; используем `any` явно.

### Связь

- [D60](#d60-spread-в-литералах-arr-record) — spread `...arr` в литералах
  массивов и record'ов; D69 распространяет на параметры функций.
- [D54](#d54-операторы-as-и-is) — `any` для heterogeneous-variadic.
- [D27](#d27-синтаксис-массивов-t-префикс-nt-фиксированные) — `[]T`
  как тип параметра.
- [08-runtime.md → D26](08-runtime.md#d26) — `print`/`println` теперь
  имеют сигнатуру `fn print(...items []any) Io -> ()`.

### Эволюция

Bootstrap-stdlib изначально имел `print` как Native-функцию принимающую
переменное число аргументов (Rust-side `&[Value]`), но **в спеке** D26
определял `fn print(s str)` — fixed arity 1. Это был drift между
implementation и spec.

D69 фиксирует variadic как полноценную фичу языка и приводит сигнатуру
`print` к `fn print(...items []any) Io -> ()`.

---

## D83. Keywords строго запрещены как identifier'ы

### Что
Зарезервированные слова языка (`fn`, `type`, `let`, `mut`, `if`, `for`,
`while`, `in`, `match`, `use`, `import`, `export`, и др.) **не могут**
использоваться как имена переменных, полей, параметров, типов,
методов, импортов или любых других user-defined identifier'ов.
**Никаких escape-механизмов** не предусмотрено.

Закрывает [Q-keywords-as-fields](../open-questions.md#q-keywords-as-fields)
вариантом 1 (строгий запрет).

### Правило

#### Полный список зарезервированных слов

**Декларации:** `module`, `import`, `use`, `export`, `external`, `fn`,
`type`, `protocol`, `effect`, `handler`, `alias`.

**Bindings:** `let`, `const`, `mut`, `ro`.

**Control flow:** `if`, `else`, `match`, `for`, `while`, `loop`, `in`,
`return`, `break`, `continue`.

**Effects/concurrency:** `with`, `throw`, `interrupt`, `forbid`,
`realtime`, `spawn`, `supervised`, `parallel`, `detach`, `blocking`,
`select`.

**Cleanup** (D90): `defer`, `errdefer`.

**Operators (как слова):** `as`, `is`, `and`, `or`, `not`.

**Литералы:** `true`, `false`.

**Test:** `test`.

**Special:** `Self` (D66), `_` (wildcard / discard).

#### Что запрещено

```nova
// все следующие — compile error «expected identifier, got keyword `X`»

ro if = 5                          // ✗
mut while = 0                   // ✗

type Queue[T] {
    in []T                          // ✗ — «expected identifier, got `in`»
}

fn process(match int) -> int =>     // ✗ — параметр не может быть `match`
    match * 2

fn export() -> int                  // ✗ — `export` зарезервировано

import std.use                      // ✗ — `use` в module path
```

#### Что разрешено

**Зарезервированные identifier'ы** (D26 prelude — `Self`, `any`,
`never`, `Option`, `Some`, `None`, `Result`, `Ok`, `Err`, `Error`,
`int`, `f64`, etc.) — это **обычные имена** в prelude scope, не
keyword'ы. Программист может **переопределить локально** (см.
[overview.md](../overview.md) «Зарезервированные identifier'ы»),
но это анти-паттерн (lint выдаёт warning).

```nova
ro int_array []int = [1, 2, 3]    // ✓ — `int_array` обычный identifier
fn shadow() {
    ro int = "string"              // ⚠️ shadow's prelude name (warning, не error)
    println(int)
}
```

#### Контекстуальные keywords — отвергнуто

Альтернатива из Swift/C# (`async`, `var`, `dynamic` контекстные —
keyword только в специфичных позициях, иначе обычные identifier'ы)
**не принимается** в Nova. Все keyword'ы — глобально зарезервированы.

#### Escape-механизм (`r#identifier`, `` `identifier` ``) — отвергнуто

Альтернативы:
- **Rust-style** `r#fn` — raw identifier через `r#` префикс.
- **C#-style** `@class` — verbatim identifier.
- **Swift/Kotlin** `` `class` `` — backticks.

В Nova **сейчас не предусмотрены**. Программист переименовывает
поле/переменную если оно конфликтует с keyword.

Когда **может** появиться: если накопится боль FFI с C-библиотеками
у которых функция называется `match`, или ORM/JSON-данные с keyword-
полями. До v1.0 — не вводим, после v1.0 — отдельный D-decision
(вероятно `r#identifier` Rust-style).

**Backtick'и `` `...` ``** в Nova **уже заняты** для tagged template
literals (D48 raw strings) — Swift-style `` `identifier` `` создаст
конфликт.

### Почему

1. **Простота парсера.** Один-проход рекурсивного спуска, никакого
   lookahead'а для разрешения «keyword vs identifier».

2. **AI-friendly.** LLM **никогда не путается** между keyword и
   identifier. Никаких escape-форм для запоминания.

3. **Читаемость.** Программист видит `if` — control flow. Видит
   `class` — class. Никаких `if` как имени переменной.

4. **Прецедент мейнстрима.** Java, Go, C, Python — все строго
   запрещают. Default ожидание программиста.

5. **Future-proof по версии.** Без escape — добавление нового
   keyword'а это явный breaking change, программист видит compile
   error и переименовывает (как Rust 2018/2021 editions).

### Что отвергнуто

- **Контекстуальные keywords** (Swift/C# style). Сложнее парсер,
  AI-unfriendly. Прецедент Swift: contextual keywords постепенно
  становятся глобальными.

- **`r#identifier` (Rust-style).** Полезен для FFI, но не приоритет
  в bootstrap'е. Можно добавить позже без breaking change.

- **`@identifier` (C#-style).** В Nova `@` занято (D35 self-method/field).

- **`` `identifier` `` (Swift/Kotlin).** Backtick'и заняты для raw
  strings (D48). Конфликт.

- **Только-в-полях ослабление** (например `mut in []T` разрешено
  поскольку `in` контекстный для `for x in iter`). Отвергнуто —
  специальное правило для одного keyword'а нарушает D9.

### Связь

- [Q-keywords-as-fields](../open-questions.md#q-keywords-as-fields) —
  закрывается этим D-decision.
- [D29](07-modules.md#d29) — module/import grammar.
- [D30](#d30) — naming convention. D83 — жёсткое правило поверх D30.
- [D48](#d48-tagged-template-literals) — backtick'и заняты.
- [D26](08-runtime.md#d26) — prelude names — это identifier'ы, не
  keyword'ы.

### Цена

- **Sweep `std/collections/queue.nv`** — поле `in []T` переименовать
  в `input` или `inputs`.
- **Будущая FFI работа** будет требовать обёртки если C-функция
  называется так же как Nova-keyword. Не блокер.

### Эволюция

До D83 вопрос был open в Q-keywords-as-fields с тремя вариантами.
D83 закрывает вопрос окончательно — Java/Go/C/Python style строгий
запрет, без escape.

Если когда-либо в будущем (v1.0+) накопится FFI-боль — отдельный
D-decision вводящий `r#identifier` Rust-style. До v1.0 — строгий
запрет без escape.

---

## D88. Default-значения generic-параметров

### Что
Generic-параметры могут иметь **default-значение** через `[T = Default]`
или с bound'ом `[T Bound = Default]`. Default используется когда
компилятор не может вывести параметр из аргументов и программист не
указал его явно.

Закрывает [Q-default-generic](../open-questions.md#q-default-generic).
Триггер принятия — [D87](04-effects.md#d87) (`Effect[E, IRT = never]`).

### Правило

#### Базовый синтаксис

```nova
type Complex[T = f64] {
    re T
    im T
}

// Старые вызовы продолжают работать без [T]:
ro z = Complex.from(2.0)             // T выводится как f64 (из default + arg)
ro z Complex = Complex.new(1.0, 2.0)  // тип Complex без скобок ≡ Complex[f64]

// Новые — с явным параметром:
ro z32 Complex[f32] = Complex.new(1.0_f32, 2.0_f32)
```

#### С bound'ом

```nova
fn run[T Numeric = int](a T) -> T => a + 1

run(5)                          // T = int (вывод из аргумента)
run(5.0)                        // T = f64 (вывод из аргумента)
run[i64](5)                     // T = i64 (явно)
```

Грамматика для одного параметра: `name [bound] [= default]`.

#### Семантика

| Случай | Что происходит |
|---|---|
| Аргументы дают информацию о `T` | Inference побеждает default |
| Аргументов нет / `T` не выводится / нет явной аннотации | Используется default |
| Программист указал `[T_value]` явно | Default игнорируется |

```nova
fn first[T = int](xs []T) -> Option[T] { ... }

first([1, 2, 3])                // T = int (вывод из []int)
first[]([])                     // ERROR: empty array, T не выводится
                                //        default не применяется (тип элемента
                                //        не из argument-type)
first[str]([])                  // T = str (явно)
```

#### Несколько параметров

Параметры с default'ом **должны идти после** обязательных:

```nova
type HashMap[K, V, S = DefaultHasher] { ... }       // ✅
type Bad[T = f64, U] { ... }                         // ❌ обязательный после default'а
```

Все default'ы могут быть опущены частично:
```nova
ro m HashMap[str, int] = ...                        // S = DefaultHasher
ro m HashMap[str, int, FxHasher] = ...              // S явно
```

#### Default — это тип, не выражение

```nova
type X[T = f64] { ... }              // ✅ default = тип
type Y[N = 10] { ... }               // ❌ const-generic — отдельная фича, не входит
```

В D88 default — **только тип**. Const-generic (значения как параметры
типа) — отдельная задача, не покрывается.

#### Default через bound

```nova
type Sorted[T Ord = int] { ... }        // T должен реализовать Ord; если не указан — int

fn sort[T Ord = int](xs []T) -> []T => ...
```

Default-тип **должен** удовлетворять bound'у — компилятор проверяет
это при объявлении.

### Почему

1. **Backward-compat.** Добавление generic к существующему типу/функции
   = breaking change без default'ов. С default'ами — ноль ломаний:
   ```nova
   // Раньше:
   type Complex { re f64, im f64 }

   // Теперь generic, но старый код работает:
   type Complex[T = f64] { re T, im T }
   ro z = Complex.from(2.0)            // ← без правок
   ```
2. **Default — не выбор для программиста.** Это сокращённая запись,
   не два пути с разной семантикой. Нарушения D9 «один очевидный путь»
   нет — программист либо не пишет параметр (получает default), либо
   пишет (получает явное значение).
3. **Прецеденты:** Rust (`Vec<T, A: Allocator = Global>`),
   C++ (`template<typename T = int>`), TypeScript (`Foo<T = string>`).
4. **Realistic consumer.** [D87](04-effects.md#d87) `Effect[E, IRT = never]` —
   главный практический use-case в Nova prelude.

### Что отвергнуто

- **`[T default int]`** keyword-форма — длиннее, без выгоды.
- **Const-generic в default'е** (`[N = 10]`) — отдельная фича,
  отложена.
- **Forward-references** в default'е (`[T = SelfType]`) — запрет: тип
  должен быть уже объявлен в момент парсинга generic-списка.
- **Default-параметры функции** (`fn f(x int = 0)`) — отдельная задача
  и **отвергнута** ([history/rejected.md](history/rejected.md)) в пользу
  опции-record + spread. D88 касается **только** generic-параметров типа.

### Связь

- [D16](#d16-дженерики-через-t-не-t) — синтаксис `[T]`.
- [D72](02-types.md#d72) — generic bounds (`[T Hash]`); D88
  расширяет до `[T Hash = SomeDefault]`.
- [D52](02-types.md#d52) — newtype/alias; D88 дополняет alias-механику
  (alias для конкретной инстанции, default — для самой частой).
- [D87](04-effects.md#d87) — `Effect[E, IRT = never]` главный consumer.

### Эволюция

Зафиксировано 2026-05-10. Раньше — открытый вопрос
[Q-default-generic](../open-questions.md#q-default-generic), помечен
DEFERRED до появления реального consumer'а. Триггер — D87
параметризация `Handler` interrupt-типом.

Migration: ~10 примеров `Effect[E]` в spec/, где требуется
`Effect[E, IRT]` для interrupt-делающих handler'ов. См.
[D87 миграция](04-effects.md#d87).

---

## D90. `defer` и `errdefer` — scope-level cleanup statement

> **Закрывает** [Q20 «Нужен ли defer?»](../open-questions.md#q20).
>
> **⚠️ Амендмент (D189 ретракт + D314 defer-kernel):** `errdefer` (и `okdefer`/`defer |result|`)
> **РЕТРАКТНУТЫ** ([D189](#d189-прямое-удаление-okdefer--errdefer--defer-result)); парсер отвергает их
> `[D189-removed-*]`. Замена — **outcome-несущая форма `defer(o ScopeOutcome) { … }`** ([D314](#d314-единое-ядро-cleanup--defer-как-примитив-defer-kernel),
> Plan 173 Ф.2): тело получает исход `Success | Failure(reason) | Panic(msg)`; `errdefer{b}` ≡
> `defer(o){ match o { Failure(_)|Panic(_) => b, Success => () } }`. Bounded lookahead различает
> `defer(o ScopeOutcome){…}` от `defer (expr)`; `defer(o T)` с `T ≠ ScopeOutcome` → `[E_DEFER_OUTCOME_TYPE]`,
> лишние токены → `[E_DEFER_OUTCOME_ARITY]`. Секции ниже про `errdefer` — **historical**. Плейн `defer`
> (п.1) без изменений (byte-identical).

### Что
Два keyword-statement'а для **отложенного выполнения** при выходе из
текущего scope'а:

1. **`defer <body>`** — выполнить `<body>` при **любом** exit'е из
   enclosing scope (normal flow, `return`, `throw`, `interrupt`,
   panic).
2. **`errdefer <body>`** — выполнить `<body>` **только** при exit'е
   через ошибку (`throw`/`panic`). При normal exit или `return`
   `errdefer` **не** выполняется.

Назначение — детерминированный cleanup (close, unlock, rollback)
в языке без RAII-destructor'ов (D6 managed heap — нет detrministic
destruction; см. цена [D6](05-memory.md#d6)).

### Правило

#### Грамматика

```
statement = ...
          | 'defer'    body
          | 'errdefer' body

body = expression
     | block             // { stmt1; stmt2; ... }
```

`body` — обычное выражение или block. Никаких params, никаких
`=>` — это **statement**, не closure.

#### Примеры

**Простой `defer`:**

```nova
fn read_config(path str) Fs Fail -> Config {
    ro file = Fs.open(path)
    defer file.close()                  // выполнится на exit из fn
    ro raw = file.read_all()
    Config.parse(raw)
}
```

**Block-form:**

```nova
fn process() Db Log -> () {
    defer {
        Log.info("done processing")
        Metrics.record_completion()
    }
    Db.exec(...)
}
```

**Несколько `defer` — LIFO (последний defer'нутый — первый выполнится):**

```nova
fn nested() Fs -> () {
    defer println("3")          // выполнится последним
    defer println("2")
    defer println("1")          // выполнится первым
    // exit prints: 1, 2, 3
}
```

**Scope-level (не function-level):**

```nova
fn process() Fs Log -> () {
    ro log_file = Fs.open("app.log")
    defer log_file.close()              // выход из fn

    if condition {
        ro temp = Fs.create_temp()
        defer temp.cleanup()            // выход из if-блока
        write_to(temp)
    }   // <- здесь выполняется temp.cleanup()

    // <- здесь выполняется log_file.close() при exit из fn
}
```

**`errdefer` — откат при ошибке:**

```nova
fn create_user(data UserData) Fail[Db] Db -> User {
    ro user = Db.insert_user(data)
    errdefer Db.delete_user(user.id)    // откат если что-то дальше упадёт

    ro profile = Db.insert_profile(user, data)
    errdefer Db.delete_profile(profile.id)

    Db.send_welcome(user.email)         // если throw — оба delete сработают
                                         // в LIFO порядке (delete_profile, потом delete_user)

    user                                 // normal exit — errdefer'ы НЕ выполняются
}
```

**Комбинированно — `defer` + `errdefer`:**

```nova
fn transaction() Fail Db -> Receipt {
    Db.begin()
    defer Log.info("transaction finished")    // ВСЕГДА
    errdefer Db.rollback()                     // только при throw

    ro r = do_work()
    Db.commit()
    r
}
// normal exit: Db.commit() → Log.info(...)
// throw exit:  Db.rollback() → Log.info(...)
```

#### Семантика

**1. Scope-level.** `defer`/`errdefer` привязаны к **enclosing
block** (function body, `if`/`else` branch, `for` body, `with`-block,
`supervised`-body, etc.). Выполняются при exit'е именно этого scope'а.

**2. LIFO order.** Несколько `defer`'ов выполняются в обратном
порядке регистрации (последний `defer` — первый выполняется).

**3. Eager argument evaluation.** Аргументы `defer`-выражения
вычисляются **в момент `defer`**, тело — откладывается:

```nova
ro i = 5
defer println(i)            // i = 5 захвачено сейчас
ro i_new = 100             // другая переменная (immutable)
// exit prints: 5
```

Для **mut**-переменной с теми же captures-правилами:

```nova
mut counter = 0
defer println(counter)      // counter — захвачен по reference (как closure)
counter = 42
// exit prints: 42
```

Это симметрично closure-семантике D32 (managed heap, mut-captures
through reference).

**4. Defer body — Fail-allowed с composition** _(amended by [D158](#d158),
Plan 100.4.1, 2026-05-23)._ Тело `defer`/`errdefer` **может** иметь
`Fail[E]`-эффект; cleanup-failure композируется с propagating error через
Plan 49 multi-error infrastructure. Enclosing fn-sig **обязан** declare
`Fail[E']` с совместимым `E ⊆ E'`.

```nova
fn process() Fail[WorkErr] -> () {
    consume tx = begin()
    defer { tx.commit() }                       // ✅ Fail[CommitErr] body
    do_work()?                                   // throws WorkErr
    // composite: { primary: WorkErr, suppressed: [CommitErr] }
}
```

Если defer body имеет `Fail[E]`, но enclosing fn-sig не declares Fail —
**compile error** `D158-defer-fail-not-in-sig`. Это force'ит explicit
visibility cleanup-fail в API.

Backward-compat: handler-wrap pattern **продолжает работать** как
opt-in shorthand для silent-suppress:

```nova
defer {
    with Fail = handler {
        fail(e) { Log.error("cleanup failed: ${e}"); interrupt () }
    } {
        risky_cleanup()                          // Fail caught в inner with
    }
}
```

Подробно — composition rules, MultiError API, diagnostic format —
[D158](#d158).

**Historical (pre-D158, Plan 20 Ред. 1):** body было **infallible** —
любой `Fail[E]` в defer body выдавал compile error. Programmer обязан
был ручной handler-wrap. D158 (Plan 100.4.1) снял это ограничение,
сохранив **compile-time visibility** через required fn-sig `Fail[E']`
declaration. Скрытого поглощения ошибок по-прежнему нет: cleanup-fail
видна either как composite-error caller'у, либо через explicit handler-
wrap внутри defer.

**5. Defer body — suspend allowed** _(amended by [D159](#d159), Plan 100.4.2,
2026-05-23)._ В теле `defer`/`errdefer` **разрешены** suspend-операции:
`Time.sleep`, `Net.*`, `Fs.*`, `Db.*`, `Channel.recv` — для production
graceful cleanup (socket close с FIN+ACK, DB drain, async commit).

**Запрещены** только AST-level concurrency constructs: `spawn`,
`parallel for`, `supervised`, `detach`, `blocking` — они leak supervised
hierarchy (новый fiber переживает scope cleanup'а). Это compile error
E (D159-spawn-in-defer).

**Cancel-safe semantics** (D159): runtime обеспечивает что cleanup
completes-then-propagates cancel. Programmer должен использовать
`Time.timeout(d) { ... }` (Plan 22) для bounded cleanup.

**Historical (pre-D159, Plan 20 Ред. 1):** body было **no-suspend** —
любая suspend operation в defer выдавала compile error. Programmer
обязан был ручной `with Time.timeout` обёртка. D159 (Plan 100.4.2)
снял ограничение для production-grade async cleanup.

**6. Top-level `return` / `break` / `continue` / `interrupt` в defer-body —
запрещены (Вариант 3 — Plan 20 Ф.3 revised).** Нельзя hijack scope-exit
окружающей функции/цикла через defer — defer **сам** часть exit-процесса.

Локальный control разрешён, **только** внутри вложенных конструкций:

- `return` — разрешён внутри **nested fn-литерала** в defer body
  (`return` локален к этому fn-литералу, не к enclosing fn).
- `break` / `continue` — разрешены внутри **nested loop** (for/while/loop)
  в defer body (локальны к этому loop'у, не к enclosing).
- `interrupt` — **всегда** запрещён на любом уровне (hijack scope-exit
  с-effect-block'а; не failable cleanup).
- `throw` / `?` / `!!` — **разрешены** _(D158, Plan 100.4.1)_ если
  enclosing fn-sig объявляет `Fail[E]`; cleanup-fail композируется через
  Plan 49 multi-error (см. пункт 4 и [D158](#d158)).

```nova
defer {
    for x in items {
        if x.bad { break }          // ✅ local break в nested loop
    }
    return 0                         // ❌ top-level return — hijack scope exit
}

defer {
    ro cleanup_fn = || {
        if early_done { return }     // ✅ local return в nested fn-literal
        do_more()
    }
    cleanup_fn()
}
```

Type-check: `DeferBodyCtx { loop_depth, fn_depth }` инкрементируется
при заходе в nested loop/fn-literal; проверка > 0 на каждом
return/break/continue.

**7. `errdefer` запускается на:**
- `throw err` (любой `Fail[E]`).
- `panic(msg)` — пока fiber не умер.
- `interrupt v` — **нет**, это normal control flow (с точки зрения
  errdefer scope'а — exit «успешный»).
- `exit(code, msg)` — **нет**, exit гасит процесс без cleanup'ов
  (D13).

**8. `defer` запускается на:**
- Normal exit (последнее выражение block'а вычислено).
- `return`.
- `throw err`.
- `panic(msg)` — пока fiber не умер.
- `interrupt v` — да (exit scope'а, неважно как).
- `exit(code, msg)` — **нет** (D13: exit без cleanup'ов).

### Почему

#### Зачем нужен defer в Nova

В Nova **нет deterministic destructor'ов** ([D6](05-memory.md#d6):
managed heap + GC). RAII Rust/C++ невозможен. Без `defer` resource
cleanup (file.close, unlock, rollback) пишется через **handler-блоки**
с copy-pasted error-paths:

```nova
// Без defer — verbose:
fn create_user(data UserData) Fail Db -> User {
    ro user = Db.insert_user(data)
    mut profile_id Option[int] = None
    with Fail = effect Fail {
        fail(e) {
            if Some(pid) = profile_id { Db.delete_profile(pid) }
            Db.delete_user(user.id)
            throw e
        }
    } {
        ro profile = Db.insert_profile(user, data)
        profile_id = Some(profile.id)
        Db.send_welcome(user.email)
    }
    user
}
```

Десятки строк boilerplate. С `defer`/`errdefer` — 6 строк
(см. пример выше). Это **значительная** экономия.

#### Прецеденты

| Язык | Конструкция | Scope-level? | errdefer? |
|---|---|---|---|
| Go | `defer expr` | function-level | нет |
| Swift | `defer { body }` | scope-level | нет |
| Zig | `defer expr; errdefer expr` | scope-level | **да** |
| D | `scope(exit/success/failure) expr` | scope-level | да + extra |

Nova берёт **Zig-style**: scope-level + `errdefer`. Не function-level
(Go), потому что Nova имеет вложенные scope'ы с богатой семантикой
(`if`, `for`, `with`, `supervised`) — function-level
ограничивал бы. Не D-style `scope(success)` — редко нужно, можно
писать обычным кодом перед exit'ом.

#### Почему scope-level, не function-level

Function-level (Go) накапливает все defer'ы в стеке функции:
```go
func f() {
    if cond {
        temp := create()
        defer temp.cleanup()        // выполнится в КОНЦЕ func, не на exit if
    }
    long_running_work()              // temp висит всё это время
}
```

В Nova scope-level позволяет **локальный** cleanup, что часто
естественнее.

#### Почему eager argument evaluation

Если бы аргументы вычислялись lazy:
```nova
mut i = 0
defer println(i)
i = 42
// exit: print 42 (хотел печатать 0?)
```

Это **regular** для closure-семантики, но **сюрприз** для programmer'а
ожидающего «defer фиксирует значение тогда же».

Eager arguments + lazy closures (через captures) — баланс. Это путь
Go (которому 15 лет программистской практики симпатизируют).

#### Почему failable body + composition (а не infallible — historical)

_Plan 20 Ред. 1 (2026-05-11) выбрал infallible body. **D158 (Plan 100.4.1,
2026-05-23) revised** к failable + composition. Аргументы._

Допустим, defer-body может падать:
```nova
fn process() Fail[CommitErr] -> () {
    consume tx = begin()
    defer { tx.commit() }           // commit may fail
    do_work()?                       // throws WorkErr
    // exit: WorkErr propagating → defer fires → commit throws CommitErr → ???
}
```

Языки решают по-разному:
- **Rust:** panic-in-Drop = `abort()` процесса. Безопасно, но programming
  совершенно непрактичен — `tx.rollback()` который может fail = abort.
- **Go:** defer возвращает error через named return — manual handling,
  легко пропустить. На практике все игнорируют.
- **TS (ES2024) / Java:** `Symbol.dispose` / `close()` throws → composite
  `SuppressedError` / `addSuppressed()` chain. Структурированно, caller
  видит весь chain.

**Nova D158 выбрал TS/Java-подход:** composition через `MultiError` chain.
Plan 49 multi-error infrastructure уже даёт kinded throws + typed payload;
D158 добавляет `nv_compose_suppressed` для chain append'а и MultiError
prelude type для caller-side inspection.

**Visibility сохранена через fn-sig:** enclosing fn-sig обязан declare
`Fail[E']` где `E ⊆ E'` для defer body. Без этого — compile error
`D158-defer-fail-not-in-sig`. Это сильнее Go/TS (которые не enforce'ят
visibility в сигнатуре), сравнимо с Java checked exceptions, но без
их verbosity — `Fail[E]` уже часть base effect-system.

**Backward-compat:** handler-wrap pattern сохраняется как opt-in
shorthand для silent suppress (см. пункт 4 example).

#### Почему suspend allowed (а не no-suspend — historical)

_Plan 20 Ред. 1 (2026-05-11) запретил suspend в defer body argument'ируя
«cleanup быстрый». **D159 (Plan 100.4.2, 2026-05-23) revised**: production
cleanup ОБЯЗАН suspend — graceful socket close с FIN+ACK, DB drain через
`Channel.recv`, async transaction commit. Без suspend programmer вынужден
делать leak-y fire-and-forget cleanup._

**D159 решение:** suspend allowed, но:
- `spawn` / `parallel for` / `supervised` / `detach` / `blocking` — запрещены
  (leak supervised hierarchy: новый fiber переживает scope cleanup'а).
- Programmer отвечает за bounded cleanup через `Time.timeout(d) { ... }`
  (Plan 22 sleep-libuv-integration).
- Runtime обеспечивает **cancel-safe semantics**: cleanup completes
  before cancel-propagation (production-grade — Plan 100.4.2 followup
  `[M-100.4.2-cancel-shielding]` для full runtime enforcement; в bootstrap
  defer runs after throw, cancel-as-throw тоже triggers cleanup).

### Что отвергнуто

- **Function-level defer (Go-style)** — слабее scope-level, ограничивает
  локальный cleanup.
- **`successdefer`** (D `scope(success)`) — редкий case, обычный код
  перед exit покрывает.
- **`defer` без `errdefer`** — `errdefer` критичен для transactions,
  без него boilerplate тот же что и без `defer`. Включаем сразу.
- **Lazy argument evaluation** — surprise factor, eager — стандарт
  Go/Swift/Zig/D.
- **Failable defer body banned-as-such** — first revision (Plan 20)
  запретила Fail в defer body absolutely. **Revised D158 (Plan 100.4.1):**
  failable body разрешён с composition через Plan 49 multi-error chain
  (`MultiError`); fn-sig обязан declare `Fail[E']`. См. пункт 4.
- **`defer return X`** — нельзя hijack exit-значение через defer.
- **`recover` (Go)** — поглощение panic из defer. Сложная семантика,
  не нужно в Nova (panic — смерть fiber'а, D13).

### Связь

- [D6](05-memory.md#d6) — managed heap без RAII, мотивирует
  потребность в `defer`.
- [D13](08-runtime.md#d13) — `panic` / `exit` семантика. `defer`
  выполняется при panic пока fiber жив; **не** выполняется при
  `exit` (D13: exit гасит процесс без cleanup'ов).
- [D22](#d22-closure-light-и-full-fn) — closure семантика; defer
  использует те же mut-capture правила.
- [D32](02-types.md#d32) — managed-heap captures, base для defer
  captures.
- [D85](04-effects.md#d85) — `?`/`!!`; в теле defer запрещены (требуют
  `Fail`, defer body infallible).
- [D91](06-concurrency.md#d91) — Channel revision; defer `tx.close()`
  — main use-case для defer в concurrency.
- [Q20](../open-questions.md#q20) — закрыто этим D-блоком.

### Bootstrap-status

- ✅ **Реализовано** (Plan 20, 2026-05-11). Все 7 фаз закрыты:
  - Ф.1 Лексер: keyword'ы `defer`/`errdefer` (commit 75673d7).
  - Ф.2 Парсер + AST: `Stmt::Defer { body }`, `Stmt::ErrDefer { body }`
    (commit 380b457).
  - Ф.3 Type-checker constraints (revised: **Вариант 3, local control
    разрешён**, commit fdb53be + 3faf9f0):
    * `throw`/`?`/`!!`/`interrupt`/suspend-effects — всегда запрещены.
    * `return`/`break`/`continue` — запрещены **только на top-level
      defer body**; внутри nested fn-литерала/loop — разрешены.
  - Ф.4 Codegen: per-scope DeferScope с активационными флагами;
    NovaFailFrame setjmp wrapper для errdefer throw-path с
    longjmp re-throw; integration во все emit_block_* paths;
    early-exit cleanup для return/break/continue
    (commits 94151c3 + b058968).
  - Ф.5 Interp: per-scope defer-stack, LIFO invocation, errdefer
    skip non-error exit (commit c96f7f3).
  - Ф.6 Positive-тесты: defer_basic.nv, errdefer_basic.nv,
    errdefer_throw.nv (interrupt handler).
  - Ф.7 Spec uplift: текущий блок.
  - **Ф.8 Production-grade hardening** (2026-05-11, commits e04ca85d
    + 61af5af4 + 007bb9ba + d913aa08 + 33c1e050):
    * (1) Type-check enforcement D61 §1430-1434: handler-method для
      эффект-операции с return type `never` ОБЯЗАН закончиться
      `interrupt`/`throw`/`panic`/`exit`. Static analysis в
      `check_handler_never_ops` + helpers (`expr_diverges`,
      `block_diverges`). Покрывает Fail.fail + user-defined effects
      с never-методами.
    * (2) Defer/errdefer на interrupt-path: codegen эмитит local
      NovaInterruptFrame setjmp wrapper аналогично fail-frame.
      На interrupt — invoke только `defer` (skip `errdefer` —
      это handled exit), pop interrupt-frame, re-interrupt с
      тем же value.
    * (3) Loop/branch body defer integration: while/loop/while-let/
      for-in-array/for-in-iter/else-branch/match-arm — все эмитят
      defer scope (раньше только for-range body был покрыт).
    * (4) D65 правило 3 (re-throw): NovaVtable_Fail.prev = outer
      handler; Nova_Fail_fail на время handler-body invocation
      swap'ает _nova_handler_Fail = current->prev, восстанавливает
      после. Throw в handler-body dispatch'ится на outer (skip
      current frame — нет infinite recursion).

  Ф.8 positive-тесты:
    * `syntax/defer_in_blocks.nv` (9 кейсов) — defer внутри
      while/loop/for-in-array body, else-branch, match-arm-block,
      nested defer scopes (LIFO между inner/outer).
    * `syntax/errdefer_rethrow.nv` (3 кейса) — re-throw из inner
      handler → outer (1-level и 3-level); errdefer + outer interrupt
      → errdefer корректно skip.
    * `syntax/defer_on_interrupt.nv` (4 кейса) — defer fires на
      interrupt-path; errdefer skip; defer+errdefer combo; LIFO для
      multiple defer'ов.

  Ф.8 negative-тест:
    * `negative_capability/fail_handler_no_exit_rejected.nv` —
      handler `fail()` без exit-control → compile error.

  Все 12 positive + 6 negative defer-relevant тестов PASS.
  10/10 effects + 17/17 concurrency без регрессий после Ф.8.

#### Известные ограничения

- **Suspend (Db/Net/Fs/Time/spawn) в defer body** — compile error
  (Ф.3). Это spec-compliant strict ограничение, не gap.
- **`exit(code, msg)`** не запускает defer'ы (D13: exit гасит процесс
  без cleanup'ов) — by design.
- **Cleanup на `panic(msg)`** — для bootstrap'а purposefully простой:
  если fiber жив, defer тоже срабатывает через fail-frame
  longjmp-path (panic dispatch'ится через nova_throw).

---

## D102. Именованные аргументы и значения параметров по умолчанию

> **Status:** active (spec). Базовая реализация — [Plan 46](../../docs/plans/46-named-parameters.md)
> (закрыт). Ревизия «дефолт → keyword-only» (2026-05-15) — [Plan 50](../../docs/plans/50-default-keyword-only.md).
> **2026-06-01 D199 amend:** default-value expression может вызывать
> `const fn` (D199) — call-site replaced литералом во время компиляции,
> default остаётся `Expr::IntLit/StrLit/...` после Plan 114.4.2 Ф.3.
> Plan 114.4.2 fixture `const_fn_used_in_const_ok.nv` — proof-of-concept
> с module-level `const`; default param сценарий — followup-сценарий
> (parser + checker уже совместимы).

### Что

Параметр функции может иметь **значение по умолчанию**; на месте
вызова аргумент может передаваться **по имени**. Ключевое правило:
**параметр с дефолтом передаётся только по имени**, позиционно — нельзя.

```nova
fn connect(host str, port int = 8080, tls bool = false) -> Conn

connect("localhost")                       // ок — обязательный позиционно
connect("localhost", port: 9000)           // ок — дефолтный по имени
connect("localhost", tls: true, port: 80)  // ок — именованные переставимы
connect("localhost", 9000)                 // ОШИБКА — port с дефолтом, только по имени
connect("localhost", 9000, true)           // ОШИБКА — нечитаемые позиционные флаги
```

Ментальная модель одной строкой: **обязательный параметр —
позиционно, опциональный — по имени.**

Это общая фича языка, не спецсинтаксис. `supervised(cancel: tok)`
([D75](06-concurrency.md#d75)) — обычный именованный аргумент.

### Правило — объявление

```nova
fn f(required int, opt int = 0, flag bool = false)
//   ^^^^^^^^      ^^^^^^^^^^^   ^^^^^^^^^^^^^^^^^
//   без дефолта   с дефолтом    с дефолтом
```

1. **Параметры с дефолтом идут после параметров без дефолта.**
   `fn f(x int = 0, y int)` — compile error.
2. **Default-выражение вычисляется на месте вызова**, каждый вызов
   заново (не Python-style def-time). Может ссылаться на
   **предшествующие** параметры и module-level `const`:
   ```nova
   fn slice(xs []int, from int = 0, to int = xs.len())
   ```
3. **Variadic-параметр** ([D69](#d69-variadic-параметры-через-items-t))
   остаётся последним и **не может иметь дефолта** (его дефолт —
   пустой пакет). Параметры до variadic могут иметь дефолты.

### Правило — вызов

```nova
// fn f(required int, opt int = 0, flag bool = false)

f(1)                       // opt, flag опущены → дефолты
f(1, opt: 5)               // дефолтный по имени
f(1, flag: true, opt: 5)   // именованные переставимы
f(required: 1, opt: 5)     // ОШИБКА — обязательный только позиционно
f(1, 5)                    // ОШИБКА — opt с дефолтом, позиционно нельзя
f(opt: 5, 1)               // ОШИБКА — позиционный после именованного
```

1. **Параметр с дефолтом — keyword-only.** Передаётся только по имени;
   позиционно — compile error. (Исключение — trailing-форма для
   последнего функционального параметра, см. «Взаимодействие».)
2. **Параметр без дефолта — позиционный.** Связывается только
   позиционно; по имени — compile error. **Правка 2026-09-04 (решение
   владельца).** Раньше здесь стояло «позиционно **или** по имени», и
   это делало правило несимметричным: дефолтный — только по имени,
   обязательный — как угодно. Теперь пара читается одной фразой:
   **обязательный — позиционно, дефолтный — по имени**, и место
   аргумента само говорит, какой он. Побочно снимается противоречие с
   амендментом D215 (2026-08-05), который уже исходил из этого чтения
   и ссылался на D102 как на своё основание.
3. **Позиционные аргументы идут первыми**, связываются слева направо.
   Именованный аргумент **не может предшествовать** позиционному —
   `f(opt: 5, 1)` — compile error.
4. **Именованные аргументы переставимы** между собой.
5. **Каждый параметр связывается ровно один раз.** Передать параметр
   и позиционно, и по имени — compile error (`f(1, required: 2)`).
6. **Параметр с дефолтом можно опустить;** параметр без дефолта —
   обязателен и передаётся позиционно (правка 2026-09-04, см. п. 2).
7. Имя в `name: expr` — это **имя параметра callee**, не выражение.

### Грамматика

```
param        = ident type [ '=' expr ]
params       = param { ',' param } [ ',' '...' ident '[]' type ]
call-args    = [ pos-args ] [ ',' named-args ] | named-args
pos-args     = expr { ',' expr }
named-args   = named-arg { ',' named-arg }
named-arg    = ident ':' expr
```

Внутри `(...)` вызова `ident ':' expr` всегда **именованный аргумент**
— коллизии с record-литералом нет (record-литерал — `Имя { ... }` в
фигурных скобках, [D43](#d43-trailing-block--без-params-fnp-body-с-params)).
`f(User { name: "a" })` — позиционный аргумент-record.

### Взаимодействие

- **D43 trailing-block / trailing-fn.** Trailing-форма связывается с
  **последним** функциональным параметром. Trailing-форма синтаксически
  отлична от позиционного аргумента в `(...)`, поэтому **остаётся
  допустимой даже если этот параметр имеет дефолт** — это не
  «позиционный аргумент дефолтного параметра». Передать тот же параметр
  *и* trailing-формой, *и* именованным аргументом нельзя (правило 5,
  «связан дважды»).
- **D69 variadic.** Именованные аргументы — только для параметров
  **до** variadic. После `...items` именованных аргументов нет.
- **Overloading отсутствует** — в Nova нет перегрузки функций, поэтому
  разрешение «какой параметр» однозначно по имени, без type-directed
  resolution.
- **`@`-методы / protocol-методы** — именованные аргументы работают
  одинаково для свободных функций и методов.

### Почему

1. **Нечитаемые флаги — compile error, а не «нежелательно».**
   `connect("h", false, true)` — позиционные `bool`/`int`-флаги
   нечитаемы и это классическая ошибка LLM-генерации. Правило «дефолт →
   keyword-only» превращает её из стиль-замечания в ошибку компиляции.
   Для AI-first языка перевод целого класса багов в compile error —
   прямо по миссии.
2. **Одно правило, обучаемая граница.** «Обязательный — позиционно,
   опциональный — по имени». Не нужно решать на каждом вызове, называть
   или нет; не нужна система двух имён, как в Swift (`_` + label).
   Опциональные параметры — это как раз те, чей порядок не запоминается.
3. **Убирает builder/option-struct boilerplate** для простых случаев
   «функция с несколькими опциональными настройками».
4. **Включает `supervised(cancel: tok)`** — синтаксис structured
   concurrency ([D75](06-concurrency.md#d75)) опирается на эту фичу.
5. **Call-site evaluation дефолтов** — нет Python-гочи с разделяемым
   mutable-дефолтом.

### Что отвергнуто

- **Spread аргументов в вызов** — `f(...record)` (record → именованные)
  и `f(..array)` (массив → позиционные). Причины: два разных оператора
  несогласованны; `...` уже занят variadic ([D69](#d69-variadic-параметры-через-items-t))
  и spread-в-литералах ([D60](#d60-spread-в-литералах-arr-record));
  позиционный spread тихо ломается при перестановке параметров callee;
  call-site становится непрозрачным. Бандл связанных параметров
  выражается option-struct'ом (`fn f(host str, opts Opts = Opts{})`)
  или именованными аргументами.
- **Python-style def-time вычисление дефолта** — mutable-default гоча.
- **Все параметры обязательно-именованные** (Swift-style, имя
  обязательно на call-site для каждого параметра) — лишняя церемония
  для унарных и math-функций (`abs(x: -5)`, `add(left: a, right: b)`),
  и делает имя **каждого** параметра жёстким API. Keyword-only
  применяется **только** к параметрам с дефолтом — обязательные
  остаются позиционными.
- **Исключение «если дефолтный параметр один — разрешить позиционно»** —
  отвергнуто: количество дефолтов не показатель риска (один `bool`-флаг
  так же нечитаем, как один из трёх); добавление второго дефолтного
  параметра тихо ломало бы существующие позиционные вызовы
  (рефакторинг-ловушка); теряется простота «одного правила».
- **Per-параметр opt-in в позиционность** (Swift `_`) — добавляет
  сложность на декларации; пока не нужно. Если math-функции начнут
  раздражать многословием — вернуться к этому отдельным решением.
- **Позиционный аргумент после именованного** — неоднозначно, запрещён.

### Эволюция

Ревизия (2026-05-15): добавлено правило **«параметр с дефолтом —
keyword-only на месте вызова»**. Раньше дефолтный параметр можно было
передать и позиционно. Триггер — позиционные `bool`/`int`-флаги
(`connect("h", false, true)`) остаются нечитаемыми и частой ошибкой
LLM-генерации даже при наличии именованных аргументов; правило делает
их compile error. Рассмотрены и отвергнуты: обязательные имена для
**всех** параметров (Swift-style) и исключение для «одного дефолта»
(см. «Что отвергнуто»). Реализация ревизии — [Plan 50](../../docs/plans/50-default-keyword-only.md);
существующие call-site'ы из Plan 46 с позиционными дефолтными
аргументами требуют миграции.

### Связь

- [D69](#d69-variadic-параметры-через-items-t) — variadic-параметры;
  variadic несовместим с дефолтом, остаётся последним.
- [D60](#d60-spread-в-литералах-arr-record) — spread `...x` в литералах;
  spread-в-вызов (отвергнут здесь) — другая операция.
- [D43](#d43-trailing-block--без-params-fnp-body-с-params) — trailing
  closure связывается с последним функциональным параметром.
- [D75](06-concurrency.md#d75) — `supervised(cancel: tok)` использует
  именованный аргумент; ревизия D75 зависит от D102.
- [Plan 46](../../docs/plans/46-named-parameters.md) — базовая реализация
  (named args + дефолты), закрыт.
- [Plan 50](../../docs/plans/50-default-keyword-only.md) — реализация
  ревизии «дефолт → keyword-only».

---

## D108. Map-литерал `[k: v]`

> **Status:** active (spec). Реализация — [Plan 52](../../docs/plans/52-hashmap-literals.md).
> (Номера D104-D107 зарезервированы [Plan 45](../../docs/plans/45-nova-doc.md).)

### Что

Map-литерал `[k: v, ...]` конструирует `HashMap[K, V]`. Ключи и
значения — **выражения**, вычисляются в рантайме.

```nova
ro m HashMap[int, str]  = [1: "a", 2: "b"]
ro m = [1: "a", 2: "b"]                       // K, V выводятся из литерала
ro a = 10
ro m HashMap[int, str]  = [a: "x", a + 1: "y"]   // ключи — выражения
ro m HashMap[str, bool] = ["has space": true]    // не-идентификаторный str-ключ
ro empty HashMap[int, str] = []               // пустой — тип из контекста
```

Дополняет map-coercion `{field: v}` ([02-types.md → D55](02-types.md#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы)):
- **`{...}`** — ключи это **статические имена-идентификаторы** →
  `HashMap[str, V]`.
- **`[k: v]`** — ключи это **выражения** (int, переменная,
  не-идентификаторная строка, computed) → `HashMap[K, V]`.

### Правило — синтаксис и парсинг

```
collection-literal = '[' ( map-body | array-body | (empty) ) ']'
map-body           = expr ':' expr { ',' expr ':' expr } [ ',' ]
array-body         = expr { ',' expr } [ ',' ]              // D27/D38
```

Парсинг **локальный, без type-directed**:
1. После `[` парсим первое выражение.
2. Следующий токен `:` → это map-литерал, дальше пары `expr : expr`.
3. Следующий токен `,` или `]` → это array-литерал (D27/D38).
4. `[]` (пусто) → array-или-map, разрешается **на type-check** по
   ожидаемому типу — ровно как уже работает пустой массив (D38).

Внутри `[...]` слева от `:` — **выражение**, не имя. Коллизии нет: в
`[]` вообще нет понятия «имя поля» (в отличие от record-литерала
`{}`). Первый `:` вне вложенных `()`/`[]`/`{}` — разделитель пары.

### Правило — типы и coercion

- Тип литерала — `HashMap[K, V]`; `K`/`V` выводятся из ключей/значений
  либо из ожидаемого типа.
- Key-позиция — D55 «known-target-type position» с ожидаемым типом
  `K`; value-позиция — с ожидаемым `V`. Значит sum-/record-/map-coercion
  ([D55](02-types.md#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы))
  композируются на ключах и значениях:
  ```nova
  ro m HashMap[str, JsonValue] = ["name": "alice", "age": 30.0]
  // значения: "alice" → Str(...), 30.0 → Num(...)
  ```
- Все ключи унифицируются в один `K`, все значения — в один `V`.

### Правило — порядок вычисления

Порядок вычисления **зафиксирован нормативно** — это улучшение над Go,
spec которого оставляет порядок вычисления map-literal expressions
неспецифицированным:

- `[k1: v1, k2: v2, ...]` — пары вычисляются **слева направо**; внутри
  каждой пары — **сначала ключ, потом значение**. Итоговый порядок
  side-effect'ов: `k1, v1, k2, v2, ...`.
- Этот порядок **observable** — побочные эффекты в ключах/значениях
  наблюдаемы именно в нём.

### Правило — порядок итерации

`HashMap` создаваемый литералом — **unordered**, как Go и Rust. Порядок
итерации не специфицирован и **может рандомизироваться** между запусками
программы (Go-стиль, защищает от случайной зависимости от порядка) либо
быть устойчивым в пределах процесса (Rust-стиль, per-instance random
seed). Конкретная политика — деталь реализации stdlib и может меняться
в будущем (например, при переходе на swisstable-implementation).

Это **намеренное проектное решение** — без него users пишут fragile
тесты («первый элемент в map это X»), которые ломаются при изменении
load-factor или hash-seed. Если требуется **детерминированный порядок** —
используйте `OrderedMap` (insertion-order, отдельный тип через
`FromPairs` протокол, Plan 52.1) или явный sort после `.entries()`.

Сравнение:
- **Go**: random per-iteration (агрессивно ломает reliance) — мы можем
  выбрать то же
- **Rust**: random per-instance (стабилен в пределах HashMap, но между
  HashMap'ами разный)
- **TS `Map`**: preserves insertion (но это другая структура — мы для
  этого даём `OrderedMap`)

### Правило — десугаринг

Map-литерал десугарится **сразу в вызовы методов**, без промежуточного
массива пар:

```nova
[k1: v1, k2: v2]
// →
{
    mut _m0 = HashMap[K, V].with_capacity(2)
    ro _ = _m0.insert(k1, v1)
    ro _ = _m0.insert(k2, v2)
    _m0
}
```

- Пустой (`[]` в map-позиции) → `HashMap[K, V].new()`.
- Ноль промежуточных объектов на куче — только сам `HashMap` (подход
  Rust `vec![]`: преаллокация + вставки).
- `with_capacity(n)` несёт контракт «`n` вставок без rehash» — аргумент
  это entry-count, не bucket-count (см. [Plan 52](../../docs/plans/52-hashmap-literals.md)).
- `@insert` возвращает `Option[V]` (старое значение); в десугаринге
  возврат всегда явно отбрасывается через `let _ = ...`.
- Temp-переменная — `_m0`, `_m1`, ... (per-scope счётчик): valid ISO C11,
  без `$`; вложенные литералы (`[1: [10: "x"]]`) не конфликтуют именами.
- **Дубликаты ключей** — last-wins, естественно из семантики `@insert`.
  Если **два ключа — одинаковые compile-time константы** (int/str/bool
  literal или `const`), компилятор выдаёт **lint-предупреждение**
  «duplicate key — second entry overwrites first» (паритет с `go vet`
  и `tsc`). Произвольные выражения не проверяются.
- **Plan 52 Ф.23 — расширяемость через `#from_pairs` attribute.**
  Десугаринг по умолчанию вызывает `HashMap`, но если expected type
  помечен `#from_pairs`, target меняется на этот тип. User-типы
  получают support литерала добавив `#from_pairs` + **протокол**:
  - `static with_capacity(n int) -> Self` — предаллоцировать под `n` записей;
  - `mut @insert_new(key K, val V) -> ()` — вставить новую запись (без
    возврата; вызывается только если перед парой не было spread);
  - `mut @insert(key K, val V) -> Option[V]` — вставить с override
    (возвращает старое значение; вызывается для пар **после** spread
    и в самом spread-цикле — т.к. ключ мог уже присутствовать).
  Итого: `#from_pairs` обязан реализовывать оба метода.
  Полный `FromPairs[K, V]` протокол (с bound-check через Plan 15) — future
  generalization, не в bootstrap.
- `HashMap.from(arr)` остаётся как обычный метод для **рантайм-массива**
  пар; литерал через него **не** идёт.

### Правило — NaN как ключ (документированный footgun)

Если `K` — float (`f64`/`f32`) и реализует `Hash`, то `[f64.NAN: "x"]`
синтаксически валиден. Но по IEEE 754 `NaN != NaN`, поэтому вставленный
NaN-ключ **невозможно найти** обратно — `@get(f64.NAN)` всегда вернёт
`None`. Rust решает радикально (`f64` не реализует `Hash + Eq`); Go и TS
документируют, но не предотвращают. Nova документирует и **предупреждает**:
если ключевое выражение — константа `f64.NAN` / `f32.NAN`, компилятор
эмитит warning «NaN as map key — inserted key can never be found». Runtime-
проверку не вводим (дорого для non-NaN случаев).

### Почему `[]`, а не `{}`

`{...}` — это record-литерал ([D17](#d17-объявление-типов-единый-синтаксис-без-)/[D55](02-types.md#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы)).
`{ ident: x }` неустранимо неоднозначен: `ident` — имя поля record'а
или выражение-ключ? Различить можно только type-directed parsing
(Nova отвергает, [D43](#d43-trailing-block--без-params-fnp-body-с-params))
или JS-гочей (`{a:1}` — ключ это строка `"a"`, не переменная). Внутри
`[...]` понятия «имя поля» нет — `[a: x]` однозначно: `a` — выражение.
Прецедент — Swift (словари на `[]`, не `{}`).

`{field: v}` всё равно даёт str-keyed map — через map-coercion (D55),
для подмножества «ключи это статические идентификаторы». Это не
TIMTOWTDI: `{}` и `[]` покрывают **разные** случаи (имя vs выражение).

### Что отвергнуто

- **Map-литерал на `{}`** (`{1: "a"}`, `{[expr]: v}`) — `1` не имя
  поля, `{}` пришлось бы парсить тремя способами (блок / record / map)
  с различием по «идентификатор ключ или нет», что молча меняет
  семантику (`{x: v}` record vs `{x(): v}` map). Фрагильно.
- **Десугаринг через `HashMap.from([(k,v),...])`** — строит
  промежуточный `[](K,V)` массив + tuple'ы на куче только ради
  инициализации. Десугарим сразу в `with_capacity` + `@insert`.
- **`[:]` как токен пустой мапы** (Swift-style) — лишний спецтокен;
  `[]` + ожидаемый тип уже однозначно даёт пустую мапу.
- **Map-литерал как compiler builtin** — `HashMap` остаётся stdlib-типом
  на Nova; литерал — чистый сахар, компилятор знает только имена
  `HashMap` / `with_capacity` / `@insert`, не реализацию.

### Связь

- [D27](#d27-синтаксис-массивов-t-префикс-nt-фиксированные) /
  [D38](#d38-создание-массивов-и-turbofish-для-дженериков) — array-литерал
  на `[]`; map-литерал делит с ним скобки, разводится по `:`.
- [D55](02-types.md#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы)
  — map-coercion (`{field: v}`); key/value-позиции литерала — D55
  known-target-type positions.
- [D17](#d17-объявление-типов-единый-синтаксис-без-) — record-литерал
  `{...}`, с которым `[]` намеренно не конфликтует.
- [Plan 52](../../docs/plans/52-hashmap-literals.md) — реализация
  D108 + ревизии D55 (map-coercion).

### Spread в map-литерале (Plan 55 followup, 2026-05-16)

`...m` внутри map-литерала разворачивает другую map того же типа:

```nova
ro defaults HashMap[str, int] = ["a": 1, "b": 2]
ro m HashMap[str, int] = [...defaults, "c": 3]      // {a:1, b:2, c:3}
ro m HashMap[str, int] = [...defaults, "a": 100]    // {a:100, b:2} (override)
ro m HashMap[str, int] = [...a, ...b]               // merge two maps
```

Семантика «right-most wins»: при duplicate keys позже встретившаяся
запись побеждает (как JS object spread, Python `{**a, **b}`).

Парсер использует **lookahead** для disambiguation: `[...x, y, z]`
рассматривается как array, `[...x, k: v]` — как map. Edge case
`[...x]` (только spread без pairs) — type-directed: если expected тип
помечен `#from_pairs` (HashMap), интерпретируется как map.

**Status (bootstrap):** parser + desugar + annotator готовы; codegen
для `[...src]` с **non-empty** src блокирован orthogonal
[M-mono-tuple-element-types] (Plan 56 scope). Эффективно работает
spread пустых map'ов + pair-only литералов.

### Mono invariants (Plan 55 Ф.4, 2026-05-16)

Codegen (`emit_c.rs`) при monomorphization сохраняет следующие
invariants:

1. **`current_fn_return_ty` save/restore** в `emit_fn` через
   `mem::replace` + restore в конце. Это предотвращает leak prior
   return type в recursive emit (mono'd transitively'd deps).
2. **Protocol-method return-type whitelist** — для well-known
   protocol methods (`eq`/`ne`/`lt`/`le`/`gt`/`ge`/`is_*` → `bool`;
   `hash` → `int`) infer возвращает stable тип до fallback на
   `fn_ret_<m>` lookup (который может содержать stale из другой fn).
3. **Placeholder mono skip** — `register_mono_method_instance` +
   `drain_generic_type_worklist` отвергают type_subst содержащий
   `Nova_<G>*` placeholders (G ∈ fn.generics). Это предотвращает
   broken erased generic emit для recursive generic calls
   (e.g. `HashMap[K,V].with_capacity` внутри `HashMap.clone()` body).
4. **`current_type_subst` save/restore** в local scope — каждая
   recursive mono call имеет свой subst stack, не leak глобально.
5. **Pattern::Record bindings** — `collect_pattern_inner_bindings`
   для record-form variant patterns (`Slot.Occupied { key: k }`)
   использует `record_variant_field_types` map с lookup mono'd
   sum_name first, fallback на base. Это предотвращает leak stale
   var_types между mono'd instances.

Полное описание — [Plan 55](../../docs/plans/55-codegen-followups-from-plan-54.md)
Ф.0-Ф.6.


---

## D104. Синтаксис doc-comment'ов — `///` outer, `//!` inner

> **Status:** active (spec). Реализация — [Plan 45](../../docs/plans/45-nova-doc.md) Ф.1.
>
> **Cross-refs:** [D101](07-modules.md#d101-doc-module-attr) (`#doc "..."` module-attr сосуществует с `//!`); [D105](09-tooling.md#d105-doc-attributes) (`#doc(...)` типизированные атрибуты делят namespace `#doc`); [D106](09-tooling.md#d106-doc-test-semantics) (code-блоки внутри doc-comment'ов).

### Что

Два префикса doc-comment'ов:

- `///` — **внешний doc-comment** (outer): привязывается к **следующей**
  декларации (function, type, constant, effect, handler, protocol) — и,
  с rev-2 (амендмент ниже), к следующему **полю записи** и следующему
  **варианту суммы**.
- `//!` — **внутренний doc-comment** (inner): привязывается к
  окружающему модулю/файлу. Допустим **только в начале файла** (после
  строки `module X` и любых строк `import`), до первой декларации.

Голое `//` остаётся обычным комментарием (doc-token не эмитится).

```nova
//! Краткое описание модуля.
//!
//! Подробное описание того, что предоставляет модуль, включая
//! примеры, охватывающие несколько items.

module std.example

import std.io

/// Возвращает модуль числа `x`.
///
/// # Examples
///
/// ```nova
/// assert(abs(-5) == 5)
/// ```
fn abs(x int) -> int =>
    if x < 0 { -x } else { x }
```

### Правила

1. **Outer (`///`)** — привязывается к **следующей** декларации в
   порядке исходника. Подряд идущие `///` строки сливаются в один
   doc-блок. Пустая `///` строка не разрывает блок (становится пустой
   строкой в content); не-doc строка завершает блок.

2. **Inner (`//!`)** — допустим **только в начале модуля**: после
   строки `module <path>` и любых `import` statement'ов, но **до**
   первой декларации item'а. Подряд идущие `//!` строки сливаются.

3. **`////` (четыре или больше слэшей)** — обычный комментарий, **не**
   doc-comment. Это копирует поведение rustdoc и предотвращает
   случайное doc-promotion для идиомы section-divider'ов (`////
   SECTION`).

4. **Multi-line merging** — подряд идущие `///` (или `//!`) строки без
   разделяющих blank-строк или других токенов конкатенируются с
   `\n`-разделителями. С каждой строки снимается префикс `///` (или
   `//!`) плюс ровно один опциональный ведущий пробел:

   ```nova
   /// Первая строка.
   ///
   /// Третья строка (после пустой doc-строки).
   ```

   даёт content `"Первая строка.\n\nТретья строка (после пустой doc-строки)."`.

5. **Indentation stripping** — когда doc-блок занимает несколько строк,
   **общий leading whitespace** (после префикса `///`/`//!` + одного
   опционального пробела) снимается единообразно с каждой непустой
   строки. Это нормализует индентацию markdown:

   ```nova
       /// Indented doc:
       ///   inner detail
   ```

   даёт `"Indented doc:\n  inner detail"` (четырёхпробельный внешний
   отступ снят равномерно; двухпробельный внутренний — сохранён).

6. **Doc не допускается на `module`, `import`, `let` на module-scope,
   `test`-блоке.** Документация уровня модуля — через `//!` (inner doc)
   или через `#doc "..."` module-attr (D101); у `test`-блока doc-
   convention нет (если нужен комментарий — обычный `//`).

7. **Пустой doc-блок (`///` за которым blank line или `///\n`)** —
   warning, обрабатывается как отсутствие документации. Style guide
   запрещает пустые doc-блоки кроме явных случаев `#hide_doc` (D105).

> **Amended (D104 rev-2, владелец 2026-08-15, поднято окном плана 274 —
> novac документирует каждый тип и поле, а язык отвергал `///` на поле):
> поля записей и варианты сумм — носители outer doc.**
>
> 1. **Носители п.1 расширены:** outer `///` привязывается также к следующему
>    **полю записи** и следующему **варианту суммы**. Форма — как у деклараций
>    (Rust): `///` **строкой выше** элемента, привязка «к следующему элементу».
> 2. **Вторая форма — trailing doc (решение владельца расширено 2026-08-15):**
>    outer `///`, стоящий на ТОЙ ЖЕ строке ПОСЛЕ объявления поля или варианта,
>    привязывается к этому полю/варианту — для коротких описаний:
>
>    ```nova
>    type Span {
>        lo int /// start offset, inclusive
>        hi int /// end offset, exclusive
>    }
>    type DefKind enum
>        | DefType    /// a type declaration
>        | DefVariant /// a sum variant
>    ```
>
>    Обе формы законны. **Обе формы на одном элементе** (строка выше И хвост) —
>    ошибка `E_DOC_BOTH_FORMS`, не молчаливая конкатенация. Trailing-форма —
>    ТОЛЬКО для полей и вариантов; на декларациях (`fn`/`type`/…) хвостовой
>    `///` не вводится: это ошибка `E_DOC_TRAILING_ON_DECL` (до rev-2 такой `///`
>    молча уезжал doc'ом к СЛЕДУЮЩЕЙ декларации — та же молчаливая
>    перепривязка, которую запрещает `E_DOC_BOTH_FORMS`).
> 3. **Слияние (п.4) и снятие индентации (п.5)** действуют для полей и вариантов
>    без изменений. `////` — по-прежнему не doc (п.3).
> 4. **Висячий `///` перед `}` записи** (без поля за ним) — warning как в п.7,
>    не ошибка. У многострочной суммы закрывающего токена нет: `///` после
>    последнего варианта — не висячий, а outer doc **следующей декларации**
>    (привязка «к следующему элементу» без изменений).
> 5. **П.6 (где doc не допускается)** — без изменений.
> 6. **Потребители:** LSP hover/outline поля и варианта показывают их doc — из
>    любой из двух форм — как у декларации; `nova doc` рендерит doc
>    поля/варианта в таблице типа.
>
> ```nova
> /// A half-open byte range.
> type Span {
>     /// Start offset, inclusive.
>     lo int
>     /// End offset, exclusive.
>     hi int
> }
>
> /// What a name is.
> type DefKind enum
>     /// A type declaration.
>     | DefType
>     /// A sum variant.
>     | DefVariant
> ```

### Position rules — примеры

```nova
//! ok: в начале модуля, после module + imports.

module foo

import bar

//! WARNING: //! после первого item — отбрасывается с warning'ом.

/// ok: outer doc на item ниже.
fn baz() -> int => 1

/// orphan outer doc — warning: за ним нет item'а.
```

```nova
fn outer() -> int {
    //! ERROR: //! внутри тела функции недопустим.

    /// ERROR: outer doc на let-statement не поддерживается.
    ro x = 1
    x
}
```

### Кодировка и escapes

- Content doc-comment'а — **сырой текст** (CommonMark markdown слой
  применяется позже, в [D106](09-tooling.md#d106-doc-test-semantics) /
  Plan 45 Ф.5).
- На уровне лексера escape-последовательности не интерпретируются.
  Backslash'ы, backtick'и и пр. — часть raw content.
- Только UTF-8. BOM в начале файла снимается перед doc-recognition.
- Trailing whitespace на каждой строке сохраняется (от него зависит
  markdown line-break семантика).

### Грамматика на уровне лексера

```
doc-outer-line  = "///" [content-char ...] NEWLINE
doc-inner-line  = "//!" [content-char ...] NEWLINE
doc-block-outer = doc-outer-line { doc-outer-line }
doc-block-inner = doc-inner-line { doc-inner-line }
content-char    = любой символ, кроме NEWLINE; при этом строка НЕ
                  ДОЛЖНА начинаться с `/` сразу после префикса (т.е.
                  `////` — обычный комментарий, а не doc-prefix + лишний
                  слэш).
```

### Сосуществование с `#doc "..."` (D101)

[D101](07-modules.md#d101-doc-module-attr) определяет атрибут
**module-level** `#doc "..."`, который может стоять перед строкой
`module X` в `_module.nv` и пропагируется на все peer-файлы. Это
**комплементарно** к `//!`:

- `#doc "..."` — для коротких summary модуля, особенно в
  folder-module'ах с `_module.nv`.
- `//!` — для длинной документации модуля в одном каноническом файле,
  включая markdown-тело и `# Examples`-секции.

Модуль **может иметь оба** одновременно. Если оба присутствуют:
- Текст `#doc` становится module summary (первое предложение).
- Тело `//!` добавляется как module description.

`nova doc` склеивает их; конфликта нет, но lint `redundant-module-doc`
предупреждает, если оба содержат идентичный текст.

### Почему

1. **`///` + `//!`** — копирует rustdoc-конвенцию, знакомую широкому
   developer-сообществу. Заимствование устоявшейся конвенции снижает
   friction для новичков и AI-ассистентов.
2. **`////` отвергнут как doc** — сохраняет идиому
   headings-as-comment'ы (`//// SECTION`) без случайного
   doc-promotion. rustdoc сделал этот выбор; мы повторяем.
3. **Никаких `/** ... */`-style блочных doc-comment'ов** — в Nova
   вообще нет блочных комментариев (только `//` line по существующей
   языковой конвенции). Добавлять блочные doc-comment'ы только ради
   документации — вводить новый синтаксис комментариев для одной цели.
4. **Английский как рекомендованная convention** — для широкого
   охвата и AI/LLM-consumption Plan 45 §11.5 рекомендует писать
   doc-content на английском. Однако технически lexer/codegen не
   ограничивают язык — content treated как opaque UTF-8 text, и при
   необходимости разработчик/команда выбирает язык под свою аудиторию.

### Что отвергнуто

- **`///` для inner-doc через position next-line** — неоднозначно
  с привязкой к следующему item'у. Отвергнуто; `//!` однозначно inner.
- **`//* ... */`-блочные doc-comment'ы** — добавляет вариант
  синтаксиса комментариев для одной цели; line-форма покрывает все
  случаи одним правилом.
- **Авто-promotion `//` обычных комментариев в doc, когда они
  предшествуют exported item** — неявно и неожиданно. Doc-promotion
  обязан быть явным (`///`).
- **Doc на `import`** — import'ы не часть public API surface, в
  output'е не рендерятся.

### Связь

- [D101](07-modules.md#d101-doc-module-attr) — module-level `#doc`
  attribute; правила сосуществования выше.
- [D105](09-tooling.md#d105-doc-attributes) — типизированные doc-
  атрибуты, включая `#doc(summary = "...")`.
- [D106](09-tooling.md#d106-doc-test-semantics) — code-блоки внутри
  doc-comment'ов являются doc-test'ами.
- [D107](09-tooling.md#d107-json-output-schema-v1) — JSON output
  включает сырой doc-content плюс распарсенную структуру.
- [Plan 45](../../docs/plans/45-nova-doc.md) — реализация; §11.5
  style guide.

---

## D117. Size-like accessors require call syntax

> **Status:** active (spec). Реализация — [Plan 60](../../docs/plans/60-len-access-uniformity.md).
> (Номера D112–D116 заняты другими планами 33.x.)

> **AMEND (2026-07-06, решение владельца) — методы-свойства как ОБЩИЙ канон доступа
> к полям.** Правило D117 обобщается со «size-подобных» на ВСЕ поля публичной
> поверхности типа: канонический доступ — **одноимённые методы-свойства через
> перегрузку по арности (D84)**: чтение `@x() -> T`, запись `mut @x(v T) -> @`
> (беглая; возврат приёмника автоматический — D409). Пары `get_x`/`set_x` —
> НЕ канон (в std их 0 — норма уже соблюдена). Прецедент пары: `@cap()`/`@cap(n)`
> (vec/core.nv, исходный D117-амендмент про write-setter). `with_x(v)` — ДРУГАЯ
> операция (копия с заменой поля, не мутация; см. nv-coding-style «Именование»).
> Весь новый код std пишется в этой парадигме.
>
> **AMEND-2 (2026-07-06, решение владельца) — сеттер возвращает `@`.** Метод
> установки свойства **по умолчанию объявляется `-> @`** (беглая форма): это
> бесплатно (возврат приёмника автоматический — D409, в теле ни строки) и любая
> последовательность установок становится цепочкой. `-> ()` у сеттера —
> отступление, допустимое только с обоснованием; установка, которая может
> отказать (валидация), — не сеттер-свойство, а операция с `Result`/эффектом.

### Что

Для **любого** типа `T` методы, возвращающие размер/cardinality/
capacity (`len`, `cap`, `byte_len`, `is_empty`, плюс будущие
`count`, `size` если они появятся как built-in convention), вызываются
**только** через method-call с круглыми скобками: `t.method()`.

Запись `t.method` (без скобок, без `@`) — это обращение к **полю**
(если поле public), либо compile error если поле не существует.
**Bound method value (`t.@method`) удалён в Plan 132** — используй
лямбду `|| t.method()` или unbound `Type.@method`.
В подавляющем большинстве случаев голое `v.len` — user error
(забытые скобки).

### Правило

```nova
ro v = [1, 2, 3]
ro n = v.len()        // ✓ корректно
ro m = v.len          // ✗ error E_SIZE_ACCESSOR_FIELD
ro z = v.is_empty()   // ✓
ro c = v.cap()        // ✓ канон (D117 AMEND 2026-07-06 — Go-style короткое имя;
                       //     `.capacity()` был дублирующим alias, РЕТРАКТИРОВАН
                       //     [M-unwrap-twins-retraction] 2026-07-07)
```

Что попадает под D117 (по conventional имени):

| Имя | Где |
|---|---|
| `len` | любая коллекция |
| `cap` | любая коллекция (включая `[]T`, `HashMap`, `StringBuilder`, `WriteBuffer`, etc.) |
| `byte_len` | строкоподобная поверхность (`str`, `StringBuilder`) — длина в байтах UTF-8 |
| `is_empty` | любая коллекция |
| `count`, `size` | если когда-нибудь добавятся как built-in convention |

Имя `cap` — канон (D117 AMEND 2026-07-06). Прежний дублирующий
`capacity` accessor РЕТРАКТИРОВАН на всех носителях
([M-unwrap-twins-retraction], 2026-07-07) — diagnostic при попытке
field-access `t.cap` (без скобок) подсказывает rename на `.cap()`.

### Diagnostic при нарушении

```
error[E_SIZE_ACCESSOR_FIELD]: size-like accessor `len` is method-only
                              (Plan 60 / D117)
  --> file.nv:42:23
   |
42 |     println("${vec.len}")
   |                    ^^^ help: append `()` — use `.len()` method call
   |
   = note: bare `.len` without `()` — missing parens (bound method value
           syntax was removed in Plan 132; use `.len()` to call)
```

Для bare `.cap` (без скобок):
```
   = help: append `()` — use `.cap()` method call (D117 AMEND 2026-07-06)
```

### Почему

1. **Predictable cost.** Nova сознательно отвергает TS/Swift-style
   computed properties (без скобок) — это спрятало бы O(n) операции
   за field-syntax (например, `s.len` для UTF-8 string требует
   codepoint count, O(n)). Скобки везде = «здесь происходит
   вычисление, возможно дорогое».
2. **Consistency.** Без D117 — built-in коллекции (`[]T`, `str`) дают
   `.len` field-style, а user-defined (`HashMap`, `Set`) — `.len()`
   method-style. Это паттерн Java (`arr.length` field vs
   `list.size()` method), worst-of-both: программист и LLM не могут
   запомнить «для какого типа какая форма».
3. **AI-friendly.** D117 — explicit spec'ed contract. LLM, читающий
   spec, имеет однозначный сигнал. Rust имеет тот же result, но
   через implicit convention (rustc не выдаёт error если вы определите
   публичное поле `len` — Nova выдаёт).
4. **Internal C-поля сохранены.** `arr->len`/`arr->cap` в C-runtime
   остаются — это implementation detail. `arr.len()` lowers в zero-cost
   `(arr->len)`; никакого function-call overhead.

### Соответствие state-of-the-art

| Language | Array size | String size | Map size | Inconsistency? |
|---|---|---|---|---|
| **Rust** | `vec.len()` method | `s.len()` method | `map.len()` method | none |
| **Go** | `len(slice)` builtin | `len(s)` builtin | `len(m)` builtin | none (но top-level fn) |
| **TS** | `arr.length` property | `s.length` property | `map.size` property | none (но field) |
| **Swift** | `arr.count` property | `s.count` property | `dict.count` property | none (но field) |
| **Java** | `arr.length` field | `s.length()` method | `m.size()` method | **inconsistent** |
| **Python** | `len(arr)` builtin | `len(s)` builtin | `len(m)` builtin | none |
| **Nova** | `arr.len()`/`arr.cap()` method | `s.len()`/`s.byte_len()` method | `map.len()`/`map.cap()` method | none (D117) |

Nova = Rust паритет, **+ explicit D-block** (Rust полагается на
convention без compiler enforcement).

### Что отвергнуто

- **Field-style для всех типов** — невыразимо для user-types
  (encapsulation: HashMap внутри `_count` + invariant'ы).
- **TS/Swift-style property (no parens)** — противоречит [D14
  «скобки обязательны для вызова»](#d14-syntax-стандартный-skin) и
  главное — спрячет O(n) операции за field-syntax.
- **`len(x)` builtin (Go-style)** — global-function-namespace
  конфликт с user-types; не работает с method-chaining
  `vec.map(f).len()`.
- ~~**`cap()` (Go naming)** — отвергнуто; полное слово `capacity()`~~
  **СУПЕРСЕДЕД D117 AMEND (2026-07-06)**: `cap()` стал каноном —
  precedent-пара read/write свойства `@cap()`/`@cap(n)` (см. AMEND-блок
  выше); `capacity()` был оставлен как alias, затем сам ретрактирован
  как дубль ([M-unwrap-twins-retraction], 2026-07-07). Историческая
  причина отказа (D29 explicitness) уступила единообразию
  read/write-пары одним именем.
- **Allow bare `.len` как warning, не error** — отвергнуто для
  bootstrap; method-value form требует явного intent (Plan 11
  syntax).

### Amend (Plan 139.2 Ф.1): self-field carve-out для declaring type-method

`E_SIZE_ACCESSOR_FIELD` нацелен на **внешних caller'ов** (`s.len`,
`vec.len` без скобок — забытые скобки / попытка обойти O(n)-cost
сигнал). Метод **самого типа** имеет полное право читать своё backing-
поле напрямую — это implementation detail (см. «Почему» §4: внутренние
C-поля сохранены). Для generic-типов (`Vec[T] @len() => @len`) это уже
работало de-facto: при эмите generic-шаблона `nova_self`-тип ещё не
concrete, поэтому guard не срабатывал.

Когда `str` стал **concrete** value-record lang-item (Plan 139.1,
`type str value priv { ptr *u8, len int }`), его собственный
`@len`/`@byte_len` Nova-body (`=> @len`) начал спотыкаться о D117:
`infer_expr_c_type(SelfAccess) == "nova_str"` + `name == "len"`. Carve-
out: bare `@len` field-read разрешён iff **(a)** obj — `SelfAccess`,
**(b)** enclosing receiver — `str`, **(c)** `name` — реально
объявленное поле `len` (НЕ `is_empty`/`cap`/`byte_len`/`capacity` —
они не поля str, остаются method-only даже на `@`). Внешний `s.len`
(obj — `Ident`, не `SelfAccess`) по-прежнему `E_SIZE_ACCESSOR_FIELD`
(regression-проверено: `plan60/f3_str_field_rejected`,
`plan139/neg_t0_str_len_field` — PASS как negatives).

Это разблокировало миграцию `str @len`/`@byte_at` external-C →
Nova-body (Plan 139.2 Ф.1): `@len() => @len` (O(1) byte-len, бывший
`nova_str_byte_len`), `@byte_at(i)` = bounds-check + `unsafe { @ptr[i] }`
(бывший `nova_str_byte_at`, идентичная OOB-паника).

### Связь

- [D32](02-types.md#d32) — array layout `(ptr, len, cap)`; D117
  скрывает эти поля от user-language.
- [D26](08-runtime.md#d26) — prelude API; D117 добавляет методы
  `[]T.len()`, `[]T.cap()`, `[]T.is_empty()`, `str.is_empty()`
  в список prelude-API.
- [D38](#d38-создание-массивов-и-turbofish-для-дженериков) — built-in
  API для `[]T`; D117 amend'ит таблицу (раздел "Built-in API").
- [Plan 11](../../docs/plans/11-method-values-and-overload.md) —
  method-value semantics; bound form (`x.@len`) removed in Plan 132;
  unbound `Type.@method` and lambda remain.
- [Plan 37](../../docs/plans/37-typecheck-semantic-parity.md) — refine
  arg-position vs non-arg method-value disambiguation (post-Plan 60
  follow-up).
- [Plan 45](../../docs/plans/45-nova-doc.md) — stdlib doc-comments
  обновлены на consistent `.len()` form.
- [Plan 56](../../docs/plans/56-vtable-dispatch-erased-generics.md) —
  bound-K vtable dispatch для size-accessors на erased generics.

---

## D126. `external type` — opaque типы без body

> **Status:** 🔴 **RETRACTED 2026-06-01 (Plan 91.12 V2)** для plain form
> (`external type X` без `consume`).
>
> **Retract rationale:**
>
> Все 5 stdlib D126 типов мигрированы на более чистые альтернативы:
>
> | Тип | Migration | Pattern |
> |---|---|---|
> | `StringBuilder` | Plan 109 (D179) | Pure Nova consume record `{ mut buf []u8 }` |
> | `WriteBuffer` | Plan 91.12 V1 | Pure Nova record `{ mut buf []u8 }` (consume @into) |
> | `ReadBuffer` | Plan 91.12 V1 | Pure Nova cursor `{ ro data, mut pos }` |
> | `OnceCell[T]` | Plan 91.12 V2 | Tuple-newtype `type OnceCell[T](ptr)` (Plan 115 D214) |
> | `Lazy[T]` | Plan 91.12 V2 | Tuple-newtype `type Lazy[T](ptr)` (Plan 115 D214) |
> | `Condvar` | Plan 91.12 V2 | Tuple-newtype `type Condvar(ptr)` (Plan 115 D214) |
>
> Plain `external type X` declarations в любом модуле теперь — hard error
> [E_EXTERNAL_TYPE_RETRACTED]. Type-checker emit'ит diagnostic с migration
> hint:
>
> ```
> [E_EXTERNAL_TYPE_RETRACTED] `external type` (D126) retracted by Plan 91.12 V2
> (2026-06-01). Replace `external type X` with `type X(ptr)` (tuple-newtype
> opaque-handle pattern, Plan 115 D214). C runtime backing preserved через
> `external fn` методы — ABI unchanged.
> Migration guide: docs/dev/migration/d126-to-tuple-newtype.md.
> For FFI opaque consume-types оставайся на `external type X consume` (D163,
> supported).
> ```
>
> **D163 (FFI opaque consume-types) сохраняется:** `external type X consume`
> остаётся allowed для FFI resource handles (`File consume`, `Mutex
> consume`, etc) — это by-design ([D163](#d163-external-type-with-consume)),
> Plan 100.5. Только plain (non-consume) form retracted.
>
> **Историческая справка:** D126 был bridge bootstrap для opaque runtime
> types (Plan 62.D.bis, 2026-05-18). Через год эксплуатации стало ясно,
> что эта форма не нужна:
> - Для пользовательских FFI handles → D214 `type X(ptr)` tuple-newtype
>   (Plan 115, 2026-06-01) даёт лучший type-safety + zero-overhead
>   opaque-pointer wrap.
> - Для stdlib runtime-backed generic types → тот же D214 паттерн +
>   compiler special-case routing к existing emit_*_instance helpers
>   (Plan 91.12 V2 §«codegen routing»).
> - Для FFI resource handles с auto-cleanup → D163 `external type X
>   consume` (Plan 100.5) — уже отдельная форма с правильной семантикой.
>
> **Реализация:**
> - Plan 62.D.bis (2026-05-18) — D126 introduce.
> - Plan 109 D179 (~2026-05-28) — StringBuilder pure Nova migration.
> - Plan 91.12 V1 (2026-06-01) — WriteBuffer/ReadBuffer pure Nova.
> - Plan 91.12 V2 (2026-06-01) — OnceCell/Lazy/Condvar tuple-newtype +
>   formal D126 retract notice (this §).
>
> Cross-ref: [D214 — `ptr` type + tuple-newtype opaque-handle](02-types.md#d214-ptr-opaque-pointer-type--tuple-ffi-returns--opaque-handle-pattern)
> (Plan 115), [D163 — FFI consume integration](02-types.md#d163-ffi-consume-integration--type-driven-без-отдельного-keywordа)
> (Plan 100.5).

---

> **Legacy reference (для historical clarity — больше не применяется к
> новому коду; type-checker emit'ит E_EXTERNAL_TYPE_RETRACTED):**

### Что

`external type X [Generics]` — модификатор type-декларации, означающий
что **тип реализован в runtime (C-коде `nova_rt/`)**, а Nova-уровневая
декларация даёт только имя + optional generic параметры. Тело
(variants/fields/protocol/effect/alias/newtype) **отсутствует** —
type «opaque». Аналог [D82](08-runtime.md#d82-external-fn--функции-с-runtime-implementation)
`external fn`, но для типов.

`external` применяется к **типам** через D126; к **функциям** — через
[D82](08-runtime.md#d82-external-fn--функции-с-runtime-implementation).
Один и тот же keyword, два валидных позиционирования
(`external fn ...` / `external type ...`).

### Правило

#### Грамматика

```
type-decl = ['export'] ['external'] 'type' name [generic-params] [body]
```

Порядок modifiers строгий: `export` первым, `external` вторым. Body у
`external type` **должен отсутствовать** (никаких `{ ... }`,
`| variant`, `effect { ... }`, `protocol { ... }`, `alias TYPE`, или
newtype `TYPE`), иначе compile error «external type cannot have a body».

#### Примеры

```nova
// Public external (built-in, Plan 62.D.bis в std/prelude/collections.nv)
export external type StringBuilder
export external type WriteBuffer
export external type ReadBuffer

// Generic external (future Channel use-case)
export external type Channel[T]

// Two-param generic external (future Region use-case)
export external type Region[T, Capability]

// Private external (внутри runtime module'а)
external type Nova_intrinsic_buffer
```

#### Связь с D26 prelude

Built-in opaque-типы из [D26](08-runtime.md#d26-базовая-stdlib-и-prelude)
(`StringBuilder`, `WriteBuffer`, `ReadBuffer`) объявляются через
`external type` в `std/prelude/collections.nv` (Plan 62.D.bis,
2026-05-18). Раньше (Plan 04) типы были «known-by-name» без formal
declaration; D126 даёт canonical source-of-truth + `nova doc` surface +
eligible для type-annotations / cross-file resolve.

```nova
// std/prelude/collections.nv
module std.prelude.collections

export external type StringBuilder
export external type WriteBuffer
export external type ReadBuffer
// + Iter[T] protocol (D58)
```

Methods на opaque-типах объявляются **отдельно** через `external fn`
([D82](08-runtime.md#d82-external-fn--функции-с-runtime-implementation))
в `std/runtime/<type>.nv`:

```nova
// std/runtime/string_builder.nv
module std.runtime.string_builder

export external fn StringBuilder.new() -> Self
export external fn StringBuilder mut @append(s str) -> Self
export external fn StringBuilder @into() -> str
// ... 11 more methods
```

Связь декларация ↔ methods — по receiver-type name (`StringBuilder`).
Нет syntactic block'а, объединяющего type-decl с methods (по
[D52](02-types.md#d52) это правильно — methods orthogonal к
declarations, free-fn-style).

#### Связь с D5/D47 видимостью

`export external type` — публичный: имя видно из других модулей.
`external type` без `export` — модуль-private. Те же правила, что для
обычных type-декл. `external` ортогонален `export`.

#### Связь с D52 kind-tokens

[D52](02-types.md#d52) фиксирует kind-tokens `type` / `protocol` /
`effect`. D126 **не** добавляет нового kind-token'а — `external` это
**модификатор** перед `type` (mirror D82 для `fn`), не отдельный kind.

В AST это кодируется через `TypeDeclKind::Opaque` (новый variant,
Plan 62.D.bis Ф.1), параллельный existing `Record` / `Sum` / `Effect` /
`Protocol` / `Alias` / `Newtype`. С точки зрения user'а — `external
type X` это specialised type-declaration формы, не отдельный kind.

#### Связь с будущим FFI

`external type` — для типов, реализованных **в Nova-runtime**
(`nova_rt/*.h`/`.c`). Для типов, импортируемых из **сторонних
C-библиотек** (libuv handles, OS-libs), будет отдельный keyword
`extern("C") type` (Q-ffi, не реализуется сейчас). Семантика разная:

| Keyword | Реализация | C-name | Разрешён программисту |
|---|---|---|---|
| `external type` | Nova-runtime (`nova_rt/`) | `Nova_<Name>*` mangled | **нет** (только в `std.runtime.*` / `std.prelude.*`) |
| `extern("C") type` (TBD) | сторонний C/lib | as-is | да (FFI) |

#### Restriction: только `std.*`-whitelist

Программистский Nova-код **не пишет** `external type`. Этот keyword —
**экспозиционный**: только модули в `std.runtime.*` и `std.prelude.*`
имеют право его использовать. Компилятор **отклоняет** `external type`
в любом другом namespace'е:

```
error: `external type` is only allowed in `std.runtime.*` / `std.prelude.*`
       modules (this module is `myapp.foo`); for FFI to external C libraries
       a future `extern("C") type` keyword will be added (Q-ffi)
```

Whitelist реализуется через `manifest::is_stdlib_runtime_module ||
is_prelude_self_module` (тот же check что для `external fn` per D82).

#### Mangling и codegen

`external type X` **не эмитит** struct definition в C output —
определение живёт в runtime header (`nova_rt/<x>.h`):

```c
// nova_rt/string_builder.h
typedef struct {
    char*  data;
    size_t len;
    size_t cap;
} Nova_StringBuilder;
```

Codegen reference на `external type X` использует mangling `Nova_X*`
(pointer, opaque). Это идентично mangling user-defined record-типов
(`type Foo { ... }` → `Nova_Foo*`), что обеспечивает consistency.

| Nova-form | C-name |
|---|---|
| `let sb StringBuilder = ...` | `Nova_StringBuilder* sb = ...` |
| `fn f(sb StringBuilder)` | `void f(Nova_StringBuilder* sb)` |
| `external type Channel[T]` | `Nova_Channel*` (T erased в bootstrap) |

`emit_type_decl` **skip'ает** emission для `TypeDeclKind::Opaque`.
Forward-declarations (`typedef struct Nova_X Nova_X;`) skip'аются
через `BUILTIN_RUNTIME_TYPES` skip-list — runtime header сам
предоставляет.

#### Validation

Аналогично [D82](08-runtime.md#d82-external-fn--функции-с-runtime-implementation),
компилятор validate'ит что декларированный `external type` **реально
существует** в runtime (через `BUILTIN_RUNTIME_TYPES` list + at-emit-
time check). Если user добавит `external type FooBar`, но
`nova_rt/foo_bar.h` отсутствует → C-toolchain ошибётся при линковке
с `undefined reference to Nova_FooBar` при первом методе.

Полная Nova-side validation (компилятор знает все runtime-implemented
типы и заранее ошибётся «type 'FooBar' not implemented in runtime»)
— требует **registry runtime types**, который сейчас живёт неявно в
`BUILTIN_RUNTIME_TYPES`. Q-codegen-runtime-types-registry — отдельная
задача аналогично D82 builtins.nv validation; bootstrap relies на
list maintenance.

### Почему

#### Зачем нужен `external type`

1. **Source-of-truth для `nova doc`.** Программист (и AI) видит
   формальную декларацию типа в одном месте — `nova doc
   std.prelude.collections` покажет StringBuilder/WriteBuffer/
   ReadBuffer как canonical API. Раньше (Plan 04, до Plan 62.D.bis)
   типы существовали только как bare-name строки в D26 spec'е — не
   visible в tooling.

2. **Eligibility для cross-file resolve.** После formal declaration
   типы участвуют в R26+R27 resolve (Plan 35). User-код может писать
   `import std.prelude.collections.{StringBuilder}` или полагаться
   на auto-import через prelude.

3. **D29 W_PRELUDE_SHADOW работает.** User declaration `type
   StringBuilder { ... }` теперь генерирует warning (mirror Plan
   62.A behavior для Option/Result). Раньше silent shadow.

4. **Symmetry с D82 `external fn`.** Если методы opaque-типа
   объявляются через `external fn`, сам тип должен иметь parallel
   form. Без D126 semantic asymmetry: methods are first-class, type
   itself isn't.

5. **Future-proof для opaque user-types** (Channel, Region, mmap'ed
   buffers). Когда возникнет use-case, mechanism уже есть — нужно
   только relax whitelist (или ввести `extern("C") type` для FFI).

#### Почему не `opaque type`

- Один keyword (`external`) для двух concepts (`fn` и `type`) —
  снижает cognitive load. Прецедент: OCaml `external`, Dart
  `external`, Kotlin `external` — все используют один keyword для
  функций и (когда уместно) типов.
- `opaque` подразумевает abstraction-from-user-code, а semantic
  нужный здесь — **implementation-elsewhere**. `external` точнее
  семантически.

#### Почему не `#external` attribute

- Per [D82](08-runtime.md#d82-external-fn--функции-с-runtime-implementation)
  уже decision: «Атрибуты в Nova зарезервированы для тестов /
  dev-tools (Q-attributes). Modifier-форма единообразна с `export`/
  `mut`». D126 follows тот же principle.
- `#external` дублировал бы syntax: `#external type X` vs `external
  type X`. Choose one — modifier form for consistency.

#### Почему restrict scope

- Bootstrap MVP — программист **не должен** объявлять opaque types
  произвольно. Runtime backing — это **compiler-versioned artefact**,
  не user-extensible (см. D82 same argument). User-extensibility
  опасна: declaration без runtime impl приведёт к undefined-reference
  C errors.
- Future relaxation требует либо:
  - **Plugin mechanism** (compiler plugin defines runtime — too heavy
    для bootstrap).
  - **FFI keyword `extern("C") type`** (Q-ffi) — для внешних libs,
    не Nova-runtime.

### Что отвергнуто

- **Bare-name `type X`** (no modifier, no body) — parser ambiguity с
  newtype branch (`type X SomeType`).
- **`opaque type X`** — separate keyword без явного gain.
- **`#external` attribute** — modifier consistency lost.
- **`type X { _ runtime }` body** — magic body, parsing complexity.
- **Auto-discovery по runtime header presence** — magic, debugging
  nightmare. Explicit `external` лучше.
- **Включить methods в декларацию типа** (`external type X { fn
  @method ... }`): per [D52](02-types.md#d52) + Plan 11, methods
  orthogonal к type-decl (free-fn-style). Не ломаем consistency
  только для opaque types.

### Связь

- [D5 / D47](07-modules.md#d47) — `export` modifier; `external` —
  ортогональный второй modifier.
- [D26](08-runtime.md#d26-базовая-stdlib-и-prelude) — prelude содержит
  StringBuilder/WriteBuffer/ReadBuffer; декларации типов — через D126
  в `std/prelude/collections.nv`.
- [D30](#d30) — naming convention; `external` — full word.
- [D52](02-types.md#d52) — kind-tokens (`type`/`effect`/`protocol`);
  D126 **не** добавляет нового kind-token'а — `external` это modifier.
- [D82](08-runtime.md#d82-external-fn--функции-с-runtime-implementation)
  — `external fn`; D126 — type-analog того же принципа. Один keyword
  `external`, два valid позиционирования.

### Эволюция

До Plan 62.D.bis (2026-05-18) типы StringBuilder/WriteBuffer/ReadBuffer
существовали как «known-by-name» (D26 prose-only), без formal Nova-
side declaration. D82 (2026-05-08, Plan 04) явно отложил `external
type` как «not yet — built-in only».

Plan 62 main (2026-05-18) выявил это как последний «known-by-name»
hole в D26 visible prelude (все остальные items мигрированы 62.A–
62.F). Plan 62.D.bis закрывает.

D126 numbering — выбран чтобы продолжить chronology D124/D125 (Plan
62.F.bis, 2026-05-18); ставит этот D-block в `03-syntax.md` (syntax-
extension), отдельно от runtime-side D82 в `08-runtime.md`.

### Bootstrap status (2026-05-18)

- ✅ Lexer: `KwExternal` token уже существует (Plan 04 Этап 2).
- ✅ Parser: relax `external` check на `KwType` (Plan 62.D.bis Ф.1).
- ✅ AST: `TypeDeclKind::Opaque` variant добавлен (Plan 62.D.bis Ф.1).
- ✅ Type-checker: whitelist enforcement (`std.runtime.*` /
  `std.prelude.*`) — Plan 62.D.bis Ф.1.
- ✅ Codegen: skip `emit_type_decl` для Opaque kind (Plan 62.D.bis Ф.1).
- ✅ `std/prelude/collections.nv`: добавлены 3 declarations (Plan
  62.D.bis Ф.2).
- ✅ `std/prelude.nv` facade: re-export (Plan 62.D.bis Ф.2).
- ⏳ Validation: формальная registry runtime-types — deferred
  (Q-codegen-runtime-types-registry), bootstrap полагается на
  `BUILTIN_RUNTIME_TYPES` list maintenance.

---

## D132. `-> @` — fluent-return (метод возвращает receiver)

> **Plan 77.** Принято 2026-05-21 (вариант B обсуждения Plan 73).

### Что

Тип возврата `-> @` означает: метод возвращает **сам receiver**.
Тип результата — receiver-тип (эквивалент `Self`), плюс гарантия, что
возвращается именно тот объект, на котором метод вызван.

```nova
fn StringBuilder mut @append(s str) -> @      // вернёт сам StringBuilder
fn Counter mut @bump() -> @ { @n = @n + 1; @ }
```

### Зачем — `Self` отвечает «какой тип», `@` отвечает «какой объект»

`Self` ([D66](02-types.md#d66)) — referential **тип**: «тот же тип, что
у receiver'а». Метод `@m() -> Self` может вернуть и **новый** объект
того же типа (`@clone() -> Self` — копия). Builder-/fluent-методам
нужно строго «**тот же объект**» — для chaining (`sb.append("a")
.append("b")`) и для проверяемых инвариантов.

`-> @` даёт это явно: `@` в позиции return-type — value-level двойник
type-level `Self`, консистентно с `@` = receiver везде в Nova.

### Правила

- **Только instance-метод.** `-> @` требует `@`-receiver'а; на
  static-методе (`Type.method`) и свободной функции — parse error.
- **Тело обязано вернуть `@`.** Non-external метод с `-> @`: тело
  завершается выражением `@`. Иначе compile error — иначе гарантия
  `-> @` была бы ложной.
- **`external fn ... -> @`** — C-реализация по контракту runtime'а
  возвращает receiver (напр. `Nova_StringBuilder_method_append` →
  `return b`).
- Тип результата для type-checker / codegen — receiver-тип (как `Self`).

### Что это разблокирует

- **Sound builder-chain alias** в consume-checker ([D131](05-memory.md#d131)):
  `let sb2 = sb.append("x")` — раз `append` объявлен `-> @`, `sb2`
  гарантированно алиас `sb`; use-after-consume через chain ловится.
- Самодокументируемые fluent-API — fluent виден из сигнатуры
  (важно для AI-first: локальность контекста).

### Сравнение

Rust выражает «возвращает receiver» через `&mut self -> &mut Self`
(заём) либо `self -> Self` (move) — точно, но ценой borrow-checker /
ownership-модели. Go сознательно отказался от builder-chaining
(`b.WriteString(...)` отдельными statement'ами). TS `this`-тип — как
наш `Self`, «тот же тип», без гарантии объекта. `-> @` даёт
Rust-уровень точности **без borrows / lifetimes** — поверх GC.

### Поправка (Plan 91 Ф.2.6, 2026-05-28) — wrapper-метод и инверсная проверка

**Правило 1 (уточнение).** Тело `-> @` метода обязано завершаться
выражением, которое **статически гарантированно** возвращает receiver:

- Bare `@` — всегда OK.
- Вызов другого метода **того же типа**, объявленного `-> @`, на `@`
  (`@write()`, `@append(s)`) — OK, поскольку он гарантированно вернёт
  сам receiver.
- `if/else`, где **все** ветки удовлетворяют условиям выше — OK.
- Всё прочее — compile error (D132).

```nova
fn Buf mut @write() -> @ { @n = @n + 1; @ }   // ✅ bare @
fn Buf mut @push()  -> @ => @write()           // ✅ делегирует в -> @ метод
```

**Правило 2 (инверсное, новое).** Если метод объявлен `-> Self`, но
**все пути тела** статически возвращают receiver (`@` или вызов `-> @`
метода), это compile error:

```
error[E_FLUENT_SELF]: метод `step` объявлен `-> Self`, но все пути
возвращают сам receiver (`@`). Используйте `-> @`.
```

Рационал: `-> Self` и `-> @` — разные семантики. `-> @` = «возвращает
тот же объект» (гарантия aliasing). `-> Self` = «возвращает значение
того же типа» (может быть копия/новый). Объявить `-> Self` там, где
тело делает только `-> @` семантику — это дезинформация для
type-checker'а (нарушает consume-aliasing D131).

### Связь

- [D131](05-memory.md#d131) — `consume`; главный потребитель `-> @`
  (builder-chain alias).
- [D66](02-types.md#d66) — `Self` (referential тип); `-> @` — его
  value-level уточнение «именно receiver».
- [D35](#d35) — методы инстанса.

---

## D143. Static-метод в `protocol {}` через leading-точку

> **Plan 97.** Принято 2026-05-23. Закрывает `Q-static-method-protocol`
> (был в [D58](#d58) разделе открытых вопросов).

### Что

В теле `protocol {}` метод объявленный с **leading-точкой**
(`.method(args) -> Ret`) — **статический** (D35: ожидаемая реализация
`fn Type.method(...)`); метод без префикса (`method(args) -> Ret`) —
**instance** (ожидаемая реализация `fn Type @method(...)`).

> **Поправка примера 2026-09-05 (исследование
> `docs/dev/research/2026-09-04-numeric-widening.md`).** Пример ниже приводил `type From[T]
> protocol { .from(t T) -> Self }` — протокол `From[T]` **ретрагирован** D73 2026-07-06 вместе с
> `Into`/`TryFrom`/`TryInto`, в std его нет; и `hash() -> u64` без `@` — форма, которую чекер
> отвергает (`E_PROTO_METHOD_NEEDS_AT`: instance-метод в теле протокола пишется `@hash()`).
> Правило D143 от этого не меняется; пример заменён на живой: `Ordinal` из
> `2026-09-04-typed-index-vec.md` (статика `.from_ordinal` — конструктор `Self` из представления,
> ровно тот случай, когда метод на источнике невозможен, потому что цель — параметр типа) и
> `Hash` в форме `@hash()`.

```nova
type Ordinal protocol {
    @ordinal() -> int              // instance — value.ordinal()
    .from_ordinal(i int) -> Self   // static — Type.from_ordinal(i)
}

type Hash protocol {
    @hash() -> u64                 // instance — value.hash()
}

type Builder[T] protocol {
    .new() -> Self            // static — Type.new()
    @push(item T) -> @        // instance, mutating, fluent return (D132)
}
```

### Правило

#### Синтаксическое различение

```
protocol-method := [ "#pure" ] [ "." | "@" ]? ident generics? "(" params? ")" effects? ret? contracts?
```

- **`.name(...)`** — static-метод (симметрично D35 `fn Type.name`).
- **`@name(...)`** — instance-метод (явный маркер, симметрично D35
  `fn Type @name`). Bare-имя `name(...)` остаётся **instance** по
  умолчанию (backwards-compat).
- Static + instance с одинаковым именем в одном протоколе —
  **запрещены** (parse error «duplicate method `foo` in protocol»).

#### Matching типа против протокола

При проверке «тип `T` удовлетворяет protocol `P`»:

- Для `is_static = true` метода `P` — ищется `fn T.method(...)`
  (D35 static-форма, регистрируется компилятором среди статиков).
- Для `is_static = false` (instance) — ищется `fn T @method(...)`
  (D35 instance-форма, регистрируется среди методов receiver-типа).

Несовпадение static/instance — **compile error** «type T does not
satisfy P: method `foo` declared `.foo` (static) but T provides
instance `@foo`» (либо обратное). Это hardening аналогичный Plan 79;
вводится постепенно — на момент Plan 97 Ф.1 matching остаётся
структурно ленивым (см. [`[M-protocol-static-enforcement-deferred]`](../../docs/dev/simplifications.md)).

#### Backwards-compat

Все существующие протоколы (`Iter`/`Hash`/`Equal`/`Compare`/
`Display`/`Into`/`TryInto`) написаны bare → остаются instance без
изменений. Меняются только `From`/`TryFrom` в `std/prelude/protocols.nv`
(их методы `.from`/`.try_from` — static).

### Почему

- **D35 симметрия**: реализация `fn Type.name(...)` — статический
  метод (точка); реализация `fn Type @name(...)` — instance (`@`).
  Декларация в протоколе должна **те же маркеры** использовать; без
  этого `From.from(t T) -> Self` неотличимо от instance, что
  противоречит D35.
- **Самодокументированность прелюдии**: `From[T] protocol { .from(t T)
  -> Self }` сразу читается «статический фабричный метод»; без точки —
  неоднозначно.
- **Spec hint**: `D58` раздел открытых вопросов уже предложил именно
  `.method()`-префикс (см. `Q-static-method-protocol` до резолва);
  Plan 97 этот hint реализует.
- **Bare = instance** (а не «требовать `@` явно») — backwards-compat:
  существующие протоколы не переписываются. Явный `@`-префикс —
  Q-open `Q-protocol-method-prefix` (followup, не блокер).

### Что отвергнуто

- **`static method(...)` keyword** — отвергнут (нет `static` в Nova,
  противоречит D35 «точка для static»).
- **`[static]` атрибут** — несимметричен D35 и громоздок.
- **Инференция static из «возвращает Self без self-параметра»** —
  фрагильно (`fn into() -> U` тоже без явного self-параметра,
  но instance).
- **`@method` обязательный для instance** — отвергнут ради backwards-
  compat. Может вернуться как optional symmetry-маркер (Q-open).

### Связь

- [D35](#d35) — реализация: static через `.`, instance через `@`.
  D143 — декларация в протоколе через те же маркеры.
- [D58](#d58) — раздел открытых вопросов; `Q-static-method-protocol`
  закрывается этим D-блоком.
- [D53](02-types.md#d53) — protocol declaration (контейнер для D143).
- [D142](02-types.md#d142) — symmetry effect/protocol declaration ↔
  literal (соседний D-блок Plan 97).
- [D77](08-runtime.md#d77) — `From`/`TryFrom` 4-way auto-derive
  (главные потребители static в протоколах).
- [Plan 97](../../docs/plans/97-protocol-effect-syntax-symmetry.md) Ф.1 —
  имплементация (parser + AST `is_static`).

---

## D158. Failable cleanup body — `Fail` effect разрешён в defer/errdefer

> **Plan 100.4.1.** Принято 2026-05-23 (proposed; implementation pending).
> **Amend [D90](#d90) §4** — снимает ограничение «defer body INFALLIBLE».
>
> **⚠️ Historical-нота (Plan 173 Ф.5.4):** заголовок/примеры ниже упоминают
> `errdefer` — он РЕТРАКТНУТ (see [D189](#d189-прямое-удаление-okdefer--errdefer--defer-result) /
> [D314](#d314-единое-ядро-cleanup--defer-как-примитив-defer-kernel) / [D188](#d188-cleanupe-protocol--consume-x--expr-body-scope-block));
> composition-семантика D158 живёт в defer-kernel: failable тело у плейн
> `defer`/`defer(o)`/`@cleanup`.
>
> **АМЕНДМЕНТ 2026-07-06 (Plan 173 Ф.4, решение владельца — модель Б).**
> Ранняя редакция D158 (ниже, в §«Правила composition» и §«MultiError API»)
> заворачивала обе ошибки в конверт `Err(MultiError { primary, suppressed })`
> и отдавала его как единый `Fail[MultiError]`. **Пересмотрено:** primary
> отдаётся ловящему **КАК ЕСТЬ** (типизированная ловля `with Fail[Primary]`
> работает, эффект НЕ становится `Fail[MultiError]`); подавленные лежат «в
> кармане» и достаются свободным аксессором `suppressed() -> []any` ПОСЛЕ
> ловли. Нормативная модель доставки — §«[Модель доставки — вариант Б]
> (#d158-модель-доставки--вариант-б)» ниже; конверт-модель отвергнута
> (§«Что отвергнуто»). Материализация подавленных реализована end-to-end
> (Plan 173 Ф.4 #6/#7): runtime suppressed-chain `NovaErrorChain` →
> `suppressed()` через `nova_failframe_suppressed_count`/`_at`.

### Что

`defer { ... }` и `errdefer { ... }` body теперь может содержать `Fail`-
effect (вызов failable consume-метода / любой Fail-action). Cleanup-fail
**композируется** с propagating error через [D85](04-effects.md#d85) /
[D118](04-effects.md#d118) multi-error infrastructure: каждая ошибка
сохраняется в chain (primary + suppressed), caller получает composite
через `MultiError`.

```nova
fn process() Fail[Err] -> () {
    consume tx = begin()
    defer { tx.commit() }                       // commit may fail — теперь валидно
    do_work()                                    // may throw Err1
    // Если do_work fails:
    //   1. unwinding starts
    //   2. defer fires — tx.commit() fails Err2
    //   3. composite: { primary: Err1, suppressed: [Err2] }
    //   4. caller получает composite через Fail[Err]
}
```

### Зачем

D90 §4 (Plan 20) запретил `Fail`-effect в defer-body как защита от
тихого поглощения ошибок. Это работало для simple cleanup (log,
mutex.unlock), но **блокирует production resource-management**:

- `Transaction.commit()` / `.rollback()` — failable (network drop,
  deadlock, constraint violation).
- `File.close()` — может fail (disk error).
- `Socket.shutdown()` — может fail.
- `Connection.disconnect()` — может fail.

Без D158 каждый такой cleanup — 6-строчный handler-wrap, что не
production-grade ergonomics. D158 force'ит explicit Fail в fn-sig
(compile-time visibility), а composition handles runtime.

### Изменение D90 §4

```
БЫЛО:  defer body не должно иметь Fail effect; обернуть в handler.
СТАЛО: defer body может иметь Fail effect; ошибка композируется через
       Plan 49 multi-error. Enclosing fn-sig ОБЯЗАН declare Fail[E].
```

### Правила composition (3 сценария)

**A. Defer-fail на normal exit:**
```nova
fn process() Fail[Err] -> () {
    consume tx = begin()
    defer { tx.commit() }                       // may fail
    do_work()                                    // success
}
// Exit: defer fires; commit может throw — caller получает Fail.
```

**B. Defer-fail во время error-propagation:**
```nova
fn process() Fail[Err] -> () {
    consume tx = begin()
    defer { tx.commit() }
    do_work()!!                                  // throws Err1 (D85: в Fail-fn — `!!`, не `?`)
    // defer fires during unwinding:
    //   tx.commit() fails CommitErr → composite
    //   { primary: Err1, suppressed: [CommitErr] }
}
```

**C. Multiple defers, each can fail** — детально в [D161](#d161)
(Plan 100.4.4 multi-defer accumulation).

### Модель доставки — вариант Б

> **Нормативно (амендмент 2026-07-06, решение владельца — Plan 173 Ф.4 §6/#1).**
> Перекрывает §«Правила composition» и §«MultiError API» выше в части
> *доставки* композита ловящему (правила *накопления* цепочки — в силе).

Cleanup-fail во время propagation **НЕ подменяет** primary и **НЕ меняет
эффект**:

- **primary — единственный носитель эффекта.** Тело бросило `Primary` →
  наружу летит `Primary`. `with Fail[Primary]` ловит; `e` = `Primary`
  (`type_id`/payload сохранены, `e is Primary` истинно); эффект остаётся
  `Fail[Primary]`, **не** превращается в `Fail[MultiError]`.
- **подавленные — вне эффекта, «в кармане».** Каждый cleanup-fail во время
  unwind'а копится в thread-local suppressed-chain (`NovaErrorChain` на
  fail-frame, LIFO); в момент ловли снимок цепочки оседает в кармане
  (`_nova_last_error`). Ловящий достаёт их **после** перехвата свободным
  аксессором `suppressed() -> []any`.
- если упал **только** cleanup (тело успешно) — cleanup-ошибка становится
  primary (обычный `Fail`), карман пуст.

```nova
fn process() Fail[WorkErr] -> () {
    consume tx = begin()
    defer { tx.commit() }        // может кинуть CommitErr
    do_work()?                   // кидает WorkErr (primary)
}

// Ловим primary типизированно; подавленные — из кармана ПОСЛЕ ловли:
mut recovered = default_result()
with Fail[WorkErr] = effect Fail[WorkErr] {
    fail(e) { interrupt log_and_default(e) }   // e: WorkErr — типизированно
} {
    process()
}
for s in suppressed() {          // == [CommitErr]
    if s is CommitErr { Log.warn("commit failed during rollback") }
}
```

**`suppressed() -> []any`** — свободный аксессор (`std/prelude/runtime.nv`):
возвращает подавленные последней пойманной ошибки в **хронологическом порядке
аварий** (первая-упавшая первой); каждый элемент — `any`, сужается через
`is T` / `.try_as[T]()` ([D54](02-types.md#d54) / [D53](02-types.md#d53)).
Пустой массив = cleanup не падал. Карман **сбрасывается на каждый свежий
`throw`** и заполняется в момент ловли, поэтому не течёт между несвязанными
ловлями (инвариант Plan 173 Ф.4 #7).

**Тип `MultiError`** (`std/prelude/errors.nv`, методы `@primary`/`@suppressed`/
`@walk`/`@find_first_panic`) остаётся как **явная value-обёртка** для кода,
который хочет собрать primary+suppressed в одно значение (лог-агрегация,
walk, panic-detection). Он **НЕ** конверт эффекта и НЕ строится компилятором
автоматически — это опциональная надстройка над `suppressed()`.

### Что отвергнуто

**Конверт-модель `Err(MultiError { primary, suppressed })` как `Fail[MultiError]`
— ОТВЕРГНУТА** (владелец, 2026-07-06). Причина: заворачивание primary в
`MultiError` ломает типизированную ловлю — `with Fail[Primary]` перестал бы
ловить (эффект стал бы `Fail[MultiError]`), а весь смысл typed-errors в том,
что root cause ловится по своему типу. Композит-как-значение остаётся доступен
через явный тип `MultiError` для тех, кому нужна агрегация, но по умолчанию
эффект несёт **только** primary, а подавленные — вне канала эффекта.

*(Историческая иллюстрация отвергнутой формы — деструктуринг
`Err(MultiError { primary, suppressed })` в §«MultiError API» выше — читать
как «НЕ так»; актуальная форма — §«Модель доставки — вариант Б».)*

### Compile-time visibility — fn-sig обязан Fail[E]

```nova
fn process() -> () {                            // ❌ нет Fail[E]
    defer { tx.commit() }                       // ❌ Fail[CommitErr] body
}
// E (D158-defer-fail-not-in-sig): add `Fail[CommitErr]` к fn-sig.
```

Force'ит explicit visibility в API.

### Diagnostic format

```
error: composite error during scope exit
  primary error:
    Err1 ("operation failed")  at do_work (process.nv:12)
  suppressed during defer LIFO (in order of firing):
    [1] CommitErr ("network timeout")  at tx.commit() in defer (process.nv:14)
    [2] Err3 ("disk full")  at tx1.commit() in defer (process.nv:13)
```

### Сравнение

| Capability | Go | Rust | TS (ES2024) | Java | Nova D158 |
|---|---|---|---|---|---|
| Cleanup body может fail | ✅ (return err) | ❌ panic-in-Drop = abort | ✅ Symbol.dispose throws | ✅ AutoCloseable.close throws | ✅ **Plan 49 composition** |
| Error composition при cleanup-fail-mid-error | ⚠️ manual | ❌ abort | ✅ SuppressedError chain | ✅ addSuppressed | ✅ **MultiError tree** |
| Visibility в сигнатуре | ⚠️ method-by-method | n/a | ⚠️ TS types | ⚠️ throws-list | ✅ **`Fail[E]` effect** |

Nova **matches Java/TS** на composition; **превосходит Rust** (no
double-panic-abort) + Go (нет manual `defer` error-handling).

### Backward-compat

Existing handler-wrap код продолжает работать. D158 — расширение
capabilities, не breaking change.

### Связь

- [D90](#d90) §4 — amend'аем.
- [D85](04-effects.md#d85), [D118](04-effects.md#d118) — composition
  infrastructure.
- [D131](05-memory.md#d131), [D133](02-types.md#d133) — consume foundation.
- [D159](#d159), [D160](#d160), [D161](#d161), [D162](#d162) — sibling
  sub-sub-plans Plan 100.4 family.

---

## D159. Async/suspend в cleanup body — cancel-safe

> **Plan 100.4.2.** Принято 2026-05-23 (proposed). **Amend [D90](#d90)
> §5** — снимает «no-suspend».

### Что

`defer`/`errdefer` body теперь может содержать suspend-операции
(`Time.sleep`, `Channel.recv`, `Net.*`, `Fs.*`). **Cancel-safe
semantics**: cleanup completes-then-cancel-propagates (runtime shield'ит
cleanup от cancel signal до его завершения).

```nova
fn process() -> () {
    consume socket = open_socket()
    defer { socket.graceful_close() }           // includes Net.* — теперь валидно
    do_io()
}
// Exit + pending cancel:
//   1. graceful_close может suspend (FIN+ACK).
//   2. cleanup completes (shielded).
//   3. cancel propagates AFTER cleanup.
```

### Запрещено

`spawn` / `parallel for` в defer body — error E (D159-spawn-in-defer).
Создание новых fiber'ов в cleanup → leak supervised hierarchy.

### Изменение D90 §5

```
БЫЛО:  defer body NO-SUSPEND (Time.sleep, Channel.recv, Net.* запрещены).
СТАЛО: suspend разрешён; cancel-safe (cleanup completes-then-propagates);
       spawn/parallel for остаются запрещены.
```

### `Time.timeout` для bounded cleanup

```nova
defer {
    with Time.timeout(5_s) {
        socket.graceful_close()                 // если >5s — abort
    }
}
```

(Полная реализация Plan 22 libuv async — already ✅.)

### Сравнение

| Capability | Rust | TS | Kotlin | Nova D159 |
|---|---|---|---|---|
| Async cleanup body | ⏳ Rust 2024+ work-in-progress | ✅ await using | ✅ coroutine use{} | ✅ **defer body suspend** |
| Cancel-safe (cleanup completes first) | ⚠️ manual shielded | ✅ AbortSignal | ✅ `withContext(NonCancellable)` | ✅ **shield-by-default** |

### Связь

- [D90](#d90) §5 — amend'аем.
- [D158](#d158) — failable cleanup (parallel).
- [D85](04-effects.md#d85) — cancel-routing foundation.
- [Plan 22](../../docs/plans/22-sleep-libuv-integration.md) ✅ — async
  foundation.

---

## D160. `okdefer` + reason-aware `defer |result|`

> **Plan 100.4.3.** Принято 2026-05-23 (proposed). **Статус: RETRACTED**
> by D189 (Plan 110.5.7 hard cutover, 2026-05-31). **Каноническая замена
> (Plan 173 Ф.2, see [D314](#d314-единое-ядро-cleanup--defer-как-примитив-defer-kernel)/[D188](#d188-cleanupe-protocol--consume-x--expr-body-scope-block)/[D189](#d189-прямое-удаление-okdefer--errdefer--defer-result)):**
> `okdefer{b}` ≡ `defer(o){ match o { Success => b, _ => () } }`;
> reason-aware `defer |result|` ≡ `defer(o ScopeOutcome) { … }` (типизированный
> биндинг исхода); для ресурсов — `consume X = ... { body }` с `match outcome`
> в `@cleanup` (D188). Всё тело ниже — **historical**.
>
> Новые scope-level statements; complement к D90 defer/errdefer family
> (historical).

### Что

Два новых construct'а:

1. **`okdefer { ... }`** — complement к `errdefer`. Выполняется только
   на **success-path** (normal exit / `return expr`); skipped при
   throw/panic/interrupt. Симметризует defer-family.

2. **`defer |result| { ... }`** — reason-aware форма. Body имеет доступ
   к exit-reason через pattern `result` (`Ok(value)` / `Err(e)` / `Panic(m)`).

### Использование

```nova
consume tx = begin()
errdefer { tx.rollback() }                      // error path → rollback
okdefer  { tx.commit() }                        // success path → commit
do_work()?
// На обоих paths tx covered — exhaustive coverage.
```

```nova
defer |result| {
    match result {
        Ok(value) => Log.info("success: ${value}"),
        Err(e)    => Log.error("failed: ${e}"),
        Panic(m)  => Log.fatal("panic: ${m}"),
    }
}
```

### Триггерные правила

| Exit-path | `defer` | `errdefer` | `okdefer` |
|---|---|---|---|
| Normal end-of-scope | ✅ | ❌ | ✅ |
| `return expr` (без error) | ✅ | ❌ | ✅ |
| `throw err` / `expr?` / `expr!!` | ✅ | ✅ | ❌ |
| `panic(msg)` | ✅ | ✅ | ❌ |
| `interrupt v` (после D162 amend) | ✅ | ✅ | ❌ |
| `exit(code)` | ❌ | ❌ | ❌ |

okdefer + errdefer — **exhaustive** (один и только один срабатывает
при non-exit() exit'е).

### Exit-path определяется в start, НЕ retro-fires

Если `okdefer { tx.commit() }` запустился (success-path) и `commit()`
fail'ит — exit-path **остаётся success**. errdefer того же scope'а
**НЕ fires** ретро-активно. Failure okdefer'а propagates через D158/
D161 multi-error composition (composite { primary: cleanup-fail }).

```nova
consume tx = begin()
errdefer { tx.rollback() }
okdefer { tx.commit() }
do_work()
// normal exit → exit-path = SUCCESS
// okdefer fires → commit fails Err1
// errdefer SKIPPED (success exit-path не retro-changes на error)
// Err1 propagates через D158 composition
```

**Почему так:** (1) tx уже Consumed через commit (failed or not) — rollback
on Consumed = error; (2) commit-failure не означает «rollback safe»
(may have partial DB state); (3) Предсказуемая семантика: exit-path
fixed at start.

Если programmer хочет «rollback-if-commit-fails»:

```nova
okdefer {
    with Fail = handler {
        fail(e) {
            tx.rollback()?
            throw e
        }
    } {
        tx.commit()
    }
}
```

### Mixed LIFO

```nova
defer A
okdefer B
errdefer C
okdefer D
defer E
```

- Normal exit LIFO: `E → D → B → A` (defer + okdefer; errdefer skipped).
- Error exit LIFO: `E → C → A` (defer + errdefer; okdefer skipped).

### Сравнение

**Unique среди GC-языков** — никто не имеет success-only cleanup
distinction:

| Capability | Go | Rust | TS | Kotlin | Nova D160 |
|---|---|---|---|---|---|
| Success-only cleanup | ❌ | ❌ | ❌ | ❌ | ✅ `okdefer` |
| Reason-aware cleanup | ❌ | ❌ | ❌ | ⚠️ try-finally manual | ✅ `defer \|result\|` |
| Symmetric defer family | ❌ | ❌ | ❌ | ❌ | ✅ defer + errdefer + okdefer |

### Связь

- [D90](#d90) — defer/errdefer foundation.
- [D158](#d158) — failable body может Fail в okdefer тоже.
- [D159](#d159) — suspend body тоже.
- [D162](#d162) — consume-integration uses okdefer для commit-on-success.

---

## D161. Multi-defer LIFO error accumulation + panic-in-defer composition

> **Plan 100.4.4.** Принято 2026-05-23 (proposed). Extends [D158](#d158)
> composition на multi-defer + panic. **Amend [D90](#d90) §«panic»**.
>
> **⚠️ Historical (Plan 173 Ф.5.4):** примеры ниже используют ретрактнутый
> `errdefer` — see [D314](#d314-единое-ядро-cleanup--defer-как-примитив-defer-kernel)/[D188](#d188-cleanupe-protocol--consume-x--expr-body-scope-block)/[D189](#d189-прямое-удаление-okdefer--errdefer--defer-result).
> САМА семантика (LIFO продолжает после partial failure; panic-in-defer
> композируется, panic-dominance) ЖИВА — реализована defer-kernel'ом
> (D314 §4a; emit_c leave/fail run-sites) и покрыта suppressed-pocket
> моделью Б (D158-амендмент).

### Что

1. **Multi-defer LIFO continues после partial failure.** Если defer N
   fail'ит → defer N-1 still runs (все N attempted; errors accumulate
   в Plan 49 multi-error chain). **Превосходит Rust** уверенно (no
   abort + all cleanups attempted).
2. **Panic в defer body композируется** с propagating через Plan 49
   multi-error — **нет Rust-style double-panic-abort**.

### LIFO с partial failure

```nova
fn process() Fail[MultiErr] -> () {
    defer A_runs                                // fail E_a
    defer B_runs                                // fail E_b
    defer C_runs                                // success
    body                                         // fail E_main
}
// Exit semantics:
//   1. body throws E_main
//   2. C_runs — success; no contribution
//   3. B_runs — fails E_b; suppressed
//   4. A_runs — fails E_a; suppressed (LIFO continues!)
//   5. caller получает MultiError {
//        primary: E_main,
//        suppressed: [E_b, E_a]                // LIFO order: first to fail = first
//      }
```

### Panic-in-defer composition

```nova
fn process() Fail[Err] -> () {
    defer { panic("cleanup broken") }
    do_fails()?                                  // throws Err1
}
// Exit:
//   1. body throws Err1
//   2. unwinding starts
//   3. defer fires — panic("cleanup broken")
//   4. panic composes с Err1 → composite { primary: Err1, suppressed: [Panic("cleanup broken")] }
//   5. propagation continues with composed error
```

**Никаких abort'ов.** Plan 49 multi-error already supports panic-as-
throw; D161 расширяет composition на panic.

### Defer-stack runtime structure

```
for entry in stack.reverse() {
    let result = run_defer_body(entry)
    match result {
        Ok(())   => continue
        Err(e)   => { propagating = compose(propagating, e); continue }
        Panic(m) => { propagating = compose(propagating, Panic(m)); continue }
    }
}
throw propagating
```

LIFO walk **completes** даже при ошибках. Rust does NOT do this.

### Diagnostic — chain visibility

```
error: composite error during scope exit
  primary error:
    Err1 ("operation failed")  at do_work (process.nv:12)
  suppressed during defer LIFO:
    [1] Err_B ("cleanup B failed")  at B_cleanup() in defer (process.nv:10)
    [2] Err_A ("cleanup A failed")  at A_cleanup() in defer (process.nv:8)
    [3] Panic("cleanup C broken")  at panic() in defer (process.nv:11)
```

### Сравнение

| Capability | Go | Rust | TS | Kotlin | Java | Nova D161 |
|---|---|---|---|---|---|---|
| Multi-cleanup LIFO continues после partial fail | ⚠️ defer continues errors lost | ❌ first-panic-abort | ✅ SuppressedError | ⚠️ partial | ✅ addSuppressed | ✅ **Plan 49 multi-error** |
| Panic в cleanup body | ✅ recover() | ❌ **double-panic-abort** | ⚠️ SuppressedError | ⚠️ try-catch | ⚠️ silent if not addSuppressed | ✅ **composition + no abort** |
| All N cleanups attempted | ⚠️ depends | ❌ first-Drop-only-tries | ⚠️ depends | ✅ try-finally chain | ✅ try-with-resources | ✅ **guaranteed** |

Nova **превосходит Rust уверенно** (no double-panic-abort + all
cleanups attempted) + matches TS/Java на composition + превосходит
на visibility (effect-typed).

### Связь

- [D90](#d90) §«panic» — amend'аем.
- [D158](#d158) — failable cleanup foundation.
- [D85](04-effects.md#d85) — multi-error composition.
- [D162](#d162) — consume-integration uses D161 для multi-consume failures.

---

## D162. Consume-integration final — check_consume + defer-family + cancel

> **Plan 100.4.5.** Принято 2026-05-23 (proposed). **Amend [D90](#d90)
> §7** (`interrupt` triggers errdefer). Финал Plan 100.4 umbrella.
>
> **⚠️ Historical (Plan 173 Ф.5.4):** таблица ниже перечисляет ретрактнутые
> `errdefer`/`okdefer` — see [D314](#d314-единое-ядро-cleanup--defer-как-примитив-defer-kernel)/[D188](#d188-cleanupe-protocol--consume-x--expr-body-scope-block)/[D189](#d189-прямое-удаление-okdefer--errdefer--defer-result).
> Актуальное покрытие consume-обязательств: плейн `defer` (все пути) и
> `defer(o ScopeOutcome)` c match по исходу; D133-quick-fix предлагает
> `defer`/`@cleanup` (Plan 173 Ф.1 #2).

### Что

`check_consume` pass (D133) распознаёт `defer`/`errdefer`/`okdefer`
как покрывающие consume-vars на соответствующих exit-paths:

| Statement | Покрывает consume на path'е |
|---|---|
| `defer { tx.commit() }` | **все exit-paths** (success, error, panic, interrupt) |
| `errdefer { tx.rollback() }` | **error-paths** (throw, panic, interrupt — amend D90 §7) |
| `okdefer { tx.commit() }` | **success-path** (normal exit, return) |

### Amend D90 §7

```
БЫЛО:  errdefer triggers on throw + panic; NOT on interrupt.
СТАЛО: errdefer triggers on throw + panic + INTERRUPT (за исключением exit()).
```

Логика: errdefer = «exit без normal completion». throw/panic/interrupt
— все «abnormal» exits относительно success-path. Backward-compat
impact — handler-flow user-code: errdefer'ы now fire on interrupt.
Plan 100.4.5 Ф.0 GATE audit'ит existing fixtures.

### Multiple defers на одну consume-var

```nova
consume tx = begin()
errdefer { tx.rollback() }                      // error path
okdefer  { tx.commit() }                        // success path
do_work()?
// tx covered: error (errdefer) + success (okdefer) = exhaustive
```

### Double-cover — error

```nova
consume tx = begin()
okdefer { tx.commit() }
tx.commit()                                      // ❌ E (D162-double-cover):
                                                 //    okdefer already commits.
```

### Partial coverage — error

```nova
consume tx = begin()
errdefer { tx.rollback() }
do_work()?
// ❌ E (D162-not-consumed-on-path): success path tx Live.
// Suggest: добавить `okdefer { tx.commit() }` или explicit `tx.commit()`.
```

### Exit-path fixed at start (НЕ retro-fire)

См. **D160 §«Exit-path определяется в start, НЕ retro-fires»** —
если okdefer fail'ит на success-path, errdefer **не** fires
дополнительно. Failure composes через D158/D161 multi-error
composition. Exit-path определяется в начале unwinding'а и не
меняется по ходу defer-execution.

### Supervised cancel + consume cleanup

```nova
supervised(cancel: tok) {
    spawn {
        consume tx = begin()
        errdefer { tx.rollback() }              // покрывает cancel-path после D90 §7 amend
        long_op()                                // may cancel
        tx.commit()
    }
}
// На cancel: errdefer fires → tx.rollback() runs (cancel-shielded по D159);
// rollback completes; fiber dies; supervised continues unwinding.
```

### Async-await preservation

```nova
fn process() Fail Async -> () {
    consume tx = begin()
    errdefer { tx.rollback() }
    await long_async_op()                       // suspend; may cancel
    tx.commit()
}
// Pre-await:  tx Live, errdefer registered.
// Post-await: tx still Live.
// Cancel-during-await: errdefer fires → tx Consumed via rollback.
```

### Canonical Transaction lifecycle

```nova
fn process_order(data Data) Fail[OrderErr] Db -> Receipt {
    consume tx = Db.begin()
    errdefer { tx.rollback()? }                 // failable rollback (D158)
    okdefer  { tx.commit()?   }                 // failable commit (D158)
    ro order = Db.insert(data)?
    ro receipt = Db.notify(order)?
    return receipt                               // okdefer fires → commit
}
// Error: errdefer fires → rollback (composite если rollback fails)
// Success: okdefer fires → commit (throw если commit fails)
```

### Связь

- [D90](#d90) §7 — amend'аем.
- [D131](05-memory.md#d131), [D133](02-types.md#d133) — consume foundation.
- [D158](#d158), [D159](#d159), [D160](#d160), [D161](#d161) —
  precondition (Plan 100.4 family).
- [Plan 49](../../docs/plans/49-cancel-throw-routing.md) — cancel-routing.
- [Plan 47](../../docs/plans/47-supervised-cancel.md) — supervised.

---

## D184. Keyword refresh: `ro`/`mut`/`consume` bindings, `const` narrowed + generalized, no `let`, `readonly` → `ro`

> 📌 **Полная модель мутабельности — [D246](02-types.md#d246-три-оси-мутабельности-l1-binding--l2-view--l3-pointee)** (3 оси: L1 binding / L2 content-view / L3 pointee). Этот D-блок описывает только **синтаксис binding-ключевых слов (`ro`/`mut`/`consume`) и переименование `readonly` → `ro`**. Для семантики, дефолтов и error-кодов читай D246.

> 🔗 **Amend — [D347](#d347-same-scope-re-binding-romutconsume-x---повторно-тип-может-меняться)** (Plan 181): повторное `ro`/`mut`/`consume x = …` в ОДНОМ scope легально (new binding; тип может меняться). Правила R1–R7 — см. D347.

> **Статус:** 🆕 draft (Plan 114 Ф.0; финализируется в Ф.8).

### Что

Plan 114 фиксирует **единую keyword-поверхность** для четырёх ортогональных
осей immutability в Nova V2:

| Ось | Keyword'ы | Позиции |
|---|---|---|
| **Binding mutability + ownership** | `ro` (immutable), `mut` (mutable), `consume` (owned) | scope; `ro` также module-level |
| **Hard compile-time guarantee** | `const` (strict constexpr) | module-level + scope-local + record-field (associated const) |
| **Per-field freeze** | `ro field T` / `mut field T` / `field ro T` | внутри `type X { … }` |
| **Comptime evaluable functions** | `const fn` (с `const` params и `-> const T` return) | top-level fn-declaration |

`let` retracted. `readonly` retracted (rename → `ro`). `const` keyword
сохранён, semantics narrowed (strict constexpr-only) + generalized (работает
в трёх позициях с единой semantics).

### Правило: binding statements

```nova
ro x = 5                            // immutable binding
mut counter = 0                     // mutable binding
consume sb = StringBuilder.new()    // owned binding (Plan 73.1)

ro x int = 5                        // с явным типом (Plan 70 prefix-form)
ro (a, b) = pair                    // destructuring tuple — оба immutable
mut (lo, hi) = bounds               // destructuring tuple — оба mutable
ro { name, age } = user             // destructuring record
```

- `ro` / `mut` — statement-leading keyword'ы в любой statement-позиции
  (top of fn body, top of block, body for/while/match arm).
- `=` обязателен. Bare `ro x` / `mut x` без init = `E_BINDING_REQUIRES_INIT`.
- Destructure-pattern: leading keyword распространяется на все имена.
  Per-element granularity (`(ro a, mut b)`) не вводится — destructure +
  reassign если нужна асимметрия.
- `mut X = expr` на module-level → `E_MUT_AT_MODULE_LEVEL`.
- `consume X = expr` на module-level → `E_CONSUME_AT_MODULE_LEVEL`.
- `ro X = expr` валиден на module-level (заменяет старый `let X = …` host
  для non-constexpr lazy-init).

### Правило: pattern-bind в условиях

`if Pat = expr` / `while Pat = expr` без outer keyword. Pattern grammar
**унифицирована** с match arm:

```nova
// Constructor / destructure pattern — bare bindings default immutable
if Some(user) = cache.get(key) { use(user) }
if Some(mut buf) = pool.try_take() { buf.fill(0) }    // mut inside pattern
if (a, b) = pair { use(a, b) }
if { name, age } = user_opt { greet(name, age) }

while Some(item) = queue.pop() { handle(item) }
while Some(mut line) = reader.read_line() { line.trim_in_place() }

// Identifier pattern — REQUIRES `ro`/`mut` (footgun protection)
if ro user = compute() { use(user) }
if mut counter = init() { counter += 1; … }
if user = compute() { … }                              // E_AMBIGUOUS_IDENT_PATTERN

// Chains (Plan 106)
if Some(user) = lookup(id), user.is_active {
    process(user)
}

// else if
if Some(a) = lookup_a() {
    use(a)
} else if Some(b) = lookup_b() {
    use(b)
}
```

- **Constructor / destructure pattern**: bare bindings default immutable;
  `mut` explicit inside (`Some(mut x)`, `(mut a, b)`).
- **Identifier pattern** (`if NAME = expr`): обязательно `ro`/`mut` —
  иначе `E_AMBIGUOUS_IDENT_PATTERN` (визуально неотличимо от assignment).
- **`consume` запрещён** в conditions — `E_CONSUME_IN_CONDITION`.
- **`mut` outside pattern удалён**: `if mut Some(buf) = e` → use
  `if Some(mut buf) = e` (`E_OUTER_MUT_IN_CONDITION`).
- Chains (Plan 106) переиспользуют тот же `if_cond`.
- Pattern grammar shared между match arm и if/while condition.

### Правило: `readonly` → `ro` (keyword rename, все позиции)

| Позиция | Было | Стало |
|---|---|---|
| Field default-immutable | `readonly id u64` | `ro id u64` |
| Field type-modifier (mutable ref, ro content) | `field readonly T` | `field ro T` |
| Field always-mut, ro content | `mut field readonly T` | `mut field ro T` |
| Param explicit ro (synonym default) | `fn f(readonly b T)` | `fn f(ro b T)` |
| Return-type | `-> readonly []u8` | `-> ro []u8` |
| Binding type-position | `ro view readonly []u8 = …` | `ro view ro []u8 = …` |

Error codes сохраняются (stable API): `E_READONLY_FIELD`, `E_READONLY_CONTENT`,
`E_READONLY_COERCE`, `E_PARAM_NOT_MUT`. Terminology в текстах diagnostic'ов
обновляется (`ro` вместо `readonly`).

`ro view ro []u8` — **не tautology**: первое `ro` фиксирует «нельзя
`view = …`» (binding), второе — «нельзя `view[0] = …`» (content).

### Правило: `const` narrow → strict constexpr-only (Ф.9)

`const X = expr` принимает **только** constexpr-eligible RHS:
- Литералы любого primitive-типа.
- Арифметика/bitwise/comparison над constexpr операндами.
- Record-литерал из constexpr-полей.
- Sum-type конструктор из constexpr args.
- Ссылка на другой `const` (любая позиция).
- Вызов `const fn` с constexpr args (Ф.11).

**Не** runtime call, **не** effect, **не** allocation, **не** ссылка на
runtime `ro`.

Errors:
- `E_CONST_NOT_CONSTEXPR` — RHS не constexpr-eligible.
- `E_CONST_REFERS_NON_CONSTEXPR` — RHS ссылается на runtime binding.
- `E_CONST_EFFECT_IN_INIT` — effect call в RHS.

```nova
// ✓ constexpr
const MAX_PAYLOAD = 4096
const TIMEOUT_SEC = 60 * 5
const GREETING = "hello"
const ORIGIN Point = { x: 0.0, y: 0.0 }

// Lazy-init non-constexpr → теперь `ro`
ro COMPUTED Point = make_point(7.0, 14.0)
ro NOW = Time.now()

// ✗ E_CONST_NOT_CONSTEXPR
const COMPUTED Point = make_point(7.0, 14.0)
```

**Strict module-level partition.** На **module-level** между `const` и `ro`
не выбор, а обязательное разделение по constexpr-eligibility:

```nova
ro MAX = 4096                              // ✗ E_RO_FOR_CONSTEXPR_PREFER_CONST
const COMPUTED = make_point(7, 14)         // ✗ E_CONST_NOT_CONSTEXPR
```

Scope-level — без strict-правила (`ro x = 5` и `const x = 5` оба валидны,
разница в гарантиях).

#### Amend (Plan 173.3, 2026-07-10): конкуренция в module-level инициализаторах запрещена

Module-level `ro`/`const`-инициализаторы исполняются в `nova_consts_init()`
ДО арминга M:N-воркеров — конкуренция в них (`spawn`/`supervised`/`detach`/
`parallel for`/`select`/блокирующие канальные `.send()`/`.recv()`/вызов fn с
эффектом `Detach`) отвергается чекером: **`[E_CONST_INIT_CONCURRENCY]`**.
Обычные runtime/extern-вызовы в `ro`-init остаются легальными. Норматив и
мотивация — [D415 §6](06-concurrency.md#d415-data-race-freedom--share-атрибут-capture-check-consume-в-spawn-plan-1733)
([M-const-init-concurrency-gate]).

#### Amend (Plan 148 Ф.3, 2026-06-12): forward direction implemented

Прямое направление partition `E_RO_FOR_CONSTEXPR_PREFER_CONST` (раньше
spec-only — маркер `[M-114.4-strict-partition]`) теперь enforced в checker'е
(`types/mod.rs::check_ro_module_partition`, вызывается из
`TypeCheckCtx::check_module` по `Item::Let`-арму). Зеркало
`E_CONST_NOT_CONSTEXPR`: оба используют один предикат
`check_const_constexpr_ex`, что гарантирует согласованность определения
«constexpr».

Семантика (точная):
- Срабатывает **только** на module-level `ro` binding (post-D184 `Item::Let`
  на module-level всегда происходит из `ro` — `let`/`mut`/`consume` отвергнуты
  парсером); внутри fn body / block (scope-local) правило не применяется.
- Срабатывает **только** на single-name binder: `Pattern::Ident` либо
  single-segment unit `Pattern::Variant` (UPPER_CASE `ro MAX = …` парсится как
  `Variant{path:["MAX"],kind:Unit}` — в позиции `ro X = …` это всегда свежее
  имя, не match). Destructuring (`ro (a, b) = …`, record/array pattern) и
  multi-segment / sub-pattern variant — оставлены как есть (у `const` нет
  destructuring-формы).
- `ghost ro X = …` — spec-only binding, не затрагивается.
- Срабатывает **только** когда весь RHS constexpr-eligible (тот же предикат,
  что и `E_CONST_NOT_CONSTEXPR`). Runtime call / effect / allocation / ссылка на
  другой `ro` → RHS не constexpr → `ro` корректен, ошибки нет.

```nova
ro MAX int = 4096                          // ✗ E_RO_FOR_CONSTEXPR_PREFER_CONST → const MAX
ro DERIVED int = BASE + 24                 // ✗ (BASE const, арифметика над const → constexpr)
ro ORIGIN Point = { x: 0.0, y: 0.0 }       // ✗ (record-литерал из constexpr полей)
ro COMPUTED int = make()                   // ✓ (runtime call — `ro` корректен)
fn f() { ro x = 4096 }                     // ✓ (scope-local — partition не применяется)
```

> Codegen module-level **runtime** `ro X = expr()` (non-constexpr) пока не
> lowered (pre-existing gap, не часть этого правила); checker корректно его
> принимает (не флагает), что и проверяется на уровне `check`.

### Правило: `const` generalization (Ф.10)

`const` валиден в **трёх позициях** с единой semantics (strict constexpr):

```nova
// 1. Module-level (как сегодня)
const MAX = 4096

// 2. Scope-local (внутри fn body / block)
fn parse_header(data ro []u8) -> Header {
    const HEADER_SIZE = 16
    ro buf [HEADER_SIZE]u8 = ...
    ...
}

// 3. Record-field — associated constant
type Config {
    const VERSION int = 2                   // не в instance layout
    const MAX_PEERS int = 1024
    name str                                 // instance field
    timeout Duration
}

Config.VERSION                              // ✓ 2 (namespace access)
ro c = Config { name: "alice", timeout: SECOND }
c.VERSION                                   // ✗ E_CONST_INSTANCE_ACCESS
```

Sum-type assoc const и generic-type assoc const (T-independent +
T-dependent с per-monomorphization codegen) — детали в [D200](02-types.md#d200).

Modifier-conflicts:
- `mut const` / `const mut` → `E_CONST_MUT_CONFLICT`.
- `ro const` / `const ro` → `E_CONST_RO_REDUNDANT`.
- `consume const` → `E_CONST_CONSUME_CONFLICT`.
- `export const` — ✓ (module-level и record-field).

### Правило: `const fn` (Ф.11)

```nova
fn calc(const a int, const b char) -> const int {
    const c = b as int
    a + c * 10
}

const RESULT = calc(5, 'A')                 // ✓ comptime → 655
ro buf [calc(2, '0')]u8 = ...               // ✓ array size 482
```

V1: all-or-nothing const params/return; body subset (literals, arithmetic,
`as`-casts, const-references, local `const`, final expression, calls на
другие `const fn`); no if/match/loop/mut/consume/effect/alloc/recursion/
generic в V1. Детали — [D199](#d199).

### Правило: Return-type defaults + `@`-inheritance (D176 amend)

Асимметрия с параметрами **намеренная**:
- Param default = `ro` (defensive — callee без права мутации).
- Return default = mutable (permissive — caller owns).

```nova
fn make_buf(n int) -> []u8                  // -> mutable []u8 by default
fn read_view(s str) -> ro []u8              // explicit ro в возврате
```

**Pointer-returns (D216 amend, Plan 138.5).** Для возвращаемого
**указателя** return-mut-default ставит мутируемость **pointee** (target),
не самого указателя: `-> *T` (= `-> *ro T`) возвращает указатель на
**read-only** target; чтобы вернуть writable target — explicit `-> *mut T`.
Никакого «outer pointer-mut в типе» нет (retracted). Перепривязываемость
самого результата (`p = other_ptr`) — это **bind-site** (`ro`/`mut`, D36),
а не часть возвращаемого типа.

```nova
fn alloc_cell() -> *mut int                 // writable target (pointee mut)
fn peek_head(buf []u8) -> *u8               // ≡ -> *ro u8 — ro target
ro p *mut int = alloc_cell()                // p фиксирован (binding ro), target writable
mut q *u8 = peek_head(buf)                  // q reassignable (binding mut), target ro
```

Это снимает прежнюю двусмысленность «двух mut» в return-позиции
(см. [D216 §V2.6/§V3.3](02-types.md#d216-v2-amend-2026-06-04--universal-right-binding-rule-для-type-level-modifiers--unsafe-t-first-class)):
в типе — только pointee-mut (постфикс), reassignability — только binding.

**`-> @` (self-return, D181)** наследует мутируемость от receiver:

| Receiver | Return `-> @` |
|---|---|
| `fn T @method() -> @` (implicit ro receiver) | `ro @` |
| `fn T mut @method() -> @` | mut `@` |
| `fn T consume @method() -> @` | `E_CONSUME_RECEIVER_RETURNS_AT` |

`-> @` без receiver-method context → `E_AT_RETURN_OUTSIDE_METHOD`.

### Grammar (precise diff)

```ebnf
// Старое (retracted)
binding_stmt   ::= "let" "mut"? IDENT type_opt "=" expr
                 | "consume" IDENT type_opt "=" expr
if_let_stmt    ::= "if" "let" pattern "=" expr block ("else" else_branch)?
while_let_stmt ::= "while" "let" pattern "=" expr block
field_decl     ::= ("readonly" | "mut")? "field"? IDENT type
type_modifier  ::= "readonly" type
param_decl     ::= ("mut" | "readonly" | "consume")? IDENT type

// Новое
binding_stmt   ::= ("ro" | "mut" | "consume") bind_lhs "=" expr
                 | const_decl
bind_lhs       ::= IDENT type_opt
                 | "(" bind_lhs ("," bind_lhs)* ")"
                 | "{" IDENT ("," IDENT)* "}"

const_decl     ::= "export"? "const" IDENT type_opt "=" expr

module_item    ::= ...
                 | "export"? "ro" IDENT type_opt "=" expr
                 | const_decl

if_stmt        ::= "if" if_cond ("," if_cond)* block ("else" else_branch)?
while_stmt     ::= "while" if_cond block
if_cond        ::= cond_pattern "=" expr
                 | bool_expr
cond_pattern   ::= ("ro" | "mut") IDENT type_opt
                 | constructor_pattern
                 | tuple_pattern
                 | record_pattern
constructor_pattern ::= TYPE_PATH "(" pattern_arg ("," pattern_arg)* ")"
                      | TYPE_PATH
pattern_arg    ::= "mut"? IDENT type_opt

field_decl     ::= ("ro" | "mut")? "field"? IDENT type
                 | "mut" "field"? IDENT "ro" type
                 | "field"? IDENT "ro" type
                 | const_decl
type_modifier  ::= "ro" type
param_decl     ::= ("mut" | "ro" | "consume" | "const")? IDENT type
fn_return      ::= "->" "const"? type
```

**Tokenizer изменения:** новый keyword token `KW_RO`; `KW_LET`/`KW_READONLY`
сохранены как recognized-but-deprecated (parser отвергает с
`E_KW_REMOVED_LET` / `E_KW_REMOVED_READONLY`).

### Сравнение с mainstream

| Язык | Immutable | Mutable | Pattern-bind-in-cond | Strength |
|---|---|---|---|---|
| Go | `const X = …` (comp-time) / нет immutable runtime | `var X` / `X :=` | `if v, ok := m[k]; ok` | walrus `:=` cond compact |
| Rust | `let x = …` | `let mut x = …` | `if let Some(x) = e` | `let` повсюду, mut явный |
| TypeScript | `const x = …` | `let x = …` | `if (e !== null) const x = e` | const/let несимметрия |
| Kotlin | `val x = …` | `var x = …` | `if (e is X) /* smart cast */` | symmetric pair |
| Java | `final var x = …` | `var x = …` | `if (e instanceof X x)` | `final` verbose |
| Swift | `let x = …` | `var x = …` | `if case let .some(x) = e` | symmetric pair |
| **Nova V1** (was) | `let x = …` | `let mut x = …` | `if let Some(x) = e` | Rust-clone |
| **Nova V2** | **`ro x = …`** | **`mut x = …`** | **`if ro x = …`** / **`if Some(x) = e`** | symmetric `ro`/`mut`/`consume` triad + ortho `const` + pattern-grammar unification |

### Acceptance

См. Plan 114 [A1-A16](../../docs/plans/114-keyword-refresh-ro-mut-no-let.md#acceptance-criteria).

### Связь

- [D27](#d27) — `[N]T` size, small wording-update «`const N` from any visible scope».
- [D30](#d30) — naming convention SCREAMING_SNAKE_CASE для `const`.
- [D32](02-types.md#d32) — default immutable amend.
- [D33](#d33) — three immutability axes (rewritten Ф.8).
- [D34](#d34) — `if`/`while` pattern grammar amend.
- [D36](02-types.md#d36) — field modifiers amend.
- [D102](#d102) — default-param-values reference `const` (compat).
- [D175](02-types.md#d175) — `ro field` full freeze (rename).
- [D176](02-types.md#d176) — `ro T` type-modifier + return defaults + `@`-inheritance (rename + Plan 114 раздел).
- [D180](05-memory.md#d180) — `consume` binding (cross-ref).
- [D199](#d199-const-fn--comptime-evaluable-functions) — `const fn` comptime evaluable functions (Plan 114.4 Ф.3).
- [D200](02-types.md#d200) — associated constants (Plan 114.4 Ф.2).
- [D201](#d201-cancel_safe--attestation-на-ffi-safety-inside-cleanup) — `#cancel_safe` FFI attestation (Plan 110.7.3.a).
- [Plan 114](../../docs/plans/114-keyword-refresh-ro-mut-no-let.md) — master plan.

---

## D199. `const fn` — comptime evaluable functions

> **Plan 114.4.2** (extracted from Plan 114.4 Ф.3 safety hatch).
> **Status:** ✅ **ACTIVE** (2026-06-01) — V1 implementation landed:
> parser (const params + `-> const T` + all-or-nothing + modifier-conflicts
> + effect-list/generic/external reject) + body checker (whitelist +
> 7 error codes + call-graph cycle detection) + comptime evaluator
> subsystem (env-based interp + memoization + overflow/div-zero) + AST
> rewriter (call-site → literal replacement + codegen drop) +
> 22 fixtures (8 NEG parser + 6 POS + 8 NEG checker/eval/external).
> См. [Plan 114.4.2](../../docs/plans/114.4.2-const-fn.md) closure.

### Что

`const fn` — функция, **вычисляемая компилятором** во время компиляции.
Параметры с модификатором `const` требуют constexpr-eligible args на call
site; `-> const T` return type гарантирует constexpr-eligible результат.
Компилятор evaluate'ит body во время компиляции и inline'ит результат
литералом на каждый call site.

```nova
fn calc(const a int, const b char) -> const int {
    const c = b as int
    a + c * 10
}

const RESULT = calc(5, 'A')              // ✓ comptime → 655
ro buf [calc(2, '0')]u8 = ...            // ✓ array size 482
fn open(n int = calc(3, ' ')) { ... }    // ✓ default param 323
```

### Правила V1

1. **All-or-nothing** — если хоть один param объявлен `const` ИЛИ return
   `-> const T`, то ВСЕ params обязаны быть `const` И return обязан быть
   `const`. Mixed → `E_CONST_FN_PARTIAL_CONSTNESS`.

2. **Allowed body** (V1 subset):
   - Литералы и арифметика над const.
   - `as`-casts между primitive-типами.
   - Ссылки на const-параметры и local `const`-bindings.
   - Локальные `const c = expr` declarations.
   - Final expression (последний statement — expression).
   - Вызовы других `const fn` с constexpr args.

3. **Forbidden body** (V1):
   - `if`/`else`/`match`/`for`/`while`/`loop` → `E_CONST_FN_CONTROL_FLOW`.
   - `mut`/`consume` bindings → `E_CONST_FN_MUT_BINDING`.
   - Effects (calls на non-const fn, runtime calls) →
     `E_CONST_FN_EFFECT_IN_BODY`.
   - Allocations → `E_CONST_FN_ALLOCATION`.
   - Generic type params → `E_CONST_FN_GENERIC`.
   - Recursion → `E_CONST_FN_RECURSION` (V1).

4. **Effect-list запрещён в declaration**: `fn calc(const a int) Log
   -> const int { … }` → `E_CONST_FN_EFFECT_IN_SIGNATURE`.

5. **Call-site rules**: все args обязаны быть constexpr-evaluable.
   `E_CONST_FN_NON_CONST_ARG` иначе. Result inline'ится литералом.

6. **First-class запрещено в V1**: `ro f = calc` → `E_CONST_FN_FIRST_CLASS`.

7. **Codegen.** `const fn` НЕ emit'ится в C-output. Все call sites
   replaced литералом. Dead `const fn` — silently dropped.

8. **Modifier-conflicts**: `mut const a int` → `E_CONST_PARAM_MOD_CONFLICT`.

### Comptime evaluator

Environment-based interpreter:
- **Param env + local const env** — отдельные scope'ы.
- **Sequential statements** — выполнение по одному.
- **Final expression** — последнее выражение возвращает результат.
- **Recursion-limit V1=1** — no recursion (checker rejects).
- **Memoization**: `(fn_id, arg_tuple) → result` cache per compilation.

Errors на evaluator-side:
- `E_CONST_FN_EVAL_OVERFLOW` — arithmetic overflow.
- `E_CONST_FN_DIV_ZERO` — division by zero.

Расширяет existing Plan 14 Ф.2 constexpr-engine на fn-вызовы.

### Сравнение с mainstream

| Язык | Синтаксис | Body subset | Mixed const/runtime |
|---|---|---|---|
| Rust | `const fn factorial(n: u32) -> u32` | Subset; recursion OK | Нет |
| C++ | `constexpr fn factorial(int n)` | Subset; recursion OK | Нет |
| Zig | `fn factorial(comptime n: u32) u32` | Full Zig | **Yes** |
| **Nova V1** | `fn factorial(const n int) -> const int { … }` | V1 subset | Нет в V1 |

### Cross-ref

- [D184](#d184) — Plan 114 master keyword refresh.
- [D33](#d33-три-оси-immutability--romutconsume--const--per-field-freeze) — three immutability axes.
- [D102](#d102) — default-param-values.
- [D200](02-types.md#d200) — assoc const.
- [D27](#d27-синтаксис-массивов-t-префикс-nt-фиксированные) — `[N]T` arrays.

### Acceptance

См. Plan 114.4.2 A14-A18 (T3 series), 22/22 fixtures PASS на release nova-cli.

### Implementation notes (2026-06-01)

- **Parser** (`compiler-codegen/src/parser/mod.rs`): `Param.is_const: bool`
  + `FnDecl.return_is_const: bool` AST extensions. All-or-nothing check
  + modifier-conflicts + effect-list / generic / external reject
  выполняются в `parse_fn` после params + return parsing.
- **Body checker** (`compiler-codegen/src/types/mod.rs::check_const_fn_decl`):
  whitelist (literal / arith / as-cast / Ident param/local / direct
  const-fn-call), blacklist (7 error codes). Stmt::Const внутри body
  binds local const env. Call-graph DFS (WHITE/GRAY/BLACK) detects
  direct + mutual recursion.
- **Comptime evaluator** (`compiler-codegen/src/const_fn_eval.rs`):
  `ConstValue` enum (Int/Float/Bool/Str/Char/Unit с manual Hash via
  `to_bits` для floats) + `ConstFnEvaluator` (OwnedEvaluator variant
  для rewrite borrow flow). Memoization `(fn_name, args) → result` per
  compilation. Checked arithmetic — overflow / div-zero explicit errors.
- **AST rewriter** (`rewrite_const_fn_calls`): walks все expressions
  (включая Match arms / loops / closures / spawn / supervised / etc.),
  заменяет Call(const_fn) на literal Expr. После walk — retain'ит
  filter из items + peer_files.items_here removing const fn declarations.
- **Pipeline placement**: после `types::check_module`, до
  `annotate_map_literals` / `desugar_module`. Single pass через
  module → fail-fast на первой error.

### V2 extensions (Plan 114.4.3, 2026-06-01)

V2 значительно расширяет const fn surface, закрывая 5 followup markers
из Plan 114.4.2 followup chain.

**Ф.1 — control flow в body:**
- `if`/`else`/`else if` expressions allowed; cond + branches validated.
- `match` expressions allowed; arms with literal patterns + wildcard +
  ident-bind + Or-alternation (V2.0 subset; record/sum patterns →
  followup `[M-114.4.3-pattern-record-sum]`).
- `for`/`while`/`loop`/`if let` остаются rejected V2.0 (followup
  `[M-114.4.3-loops]`); workaround: use recursion.
- New errors: `E_CONST_FN_MATCH_EXHAUSTIVE` (uncovered scrutinee
  value), `E_CONST_FN_PATTERN_NOT_SUPPORTED`.

**Ф.2 — recursion:**
- Direct AND mutual recursion allowed.
- Evaluator depth limit: 256 (V1: 64). Reaching → new error
  `E_CONST_FN_EVAL_DEPTH_EXCEEDED`.
- Memoization (V1) provides O(n) Fibonacci behavior.
- V1 cycle detection retained but downgraded to informational (no error).

**Ф.3 — mixed-args:**
- Allow ANY mix const/runtime params + const/runtime return.
- New fn classification:
  * **Fully-const fn**: all params const + const return. V1 behavior —
    evaluator inlines + dropped из codegen.
  * **Mixed fn**: any const surface, не fully-const. Stays в codegen
    как обычная runtime fn; const params validated at call-sites
    (must receive constexpr literal).
- Effect-list reject relaxed: fully-const only (mixed fn body может
  effects).
- Marker `[M-114.4.2-runtime-return]` покрыт (частный случай:
  all-const params + runtime return).

**Ф.4 — generic const fn:**
- `fn[T] foo(const a int) -> const int { ... }` allowed для
  T-independent body.
- T reflection (size_of[T], T.field) rejected V2.0 — followup
  `[M-114.4.3-t-reflection]` для V3.
- Per-T monomorphization happens через standard generic pipeline.

**Ф.5 — first-class const fn alias:**
- `const ALIAS = const_fn_name` allowed.
- `ALIAS(args)` resolves через alias map в rewriter.
- Alias-to-alias chains supported (depth-10 iterative pass).
- Alias `const` decls dropped из codegen (no runtime storage).
- Out-of-scope V2.0: `ro f = const_fn` runtime binding silent allow
  (enforcement → followup `[M-114.4.3-runtime-let-enforcement]`);
  HOF pass fails at link-time с unfriendly error (followup
  `[M-114.4.3-friendly-hof-error]`).

### V3 extensions (Plan 114.4.4, 2026-06-01)

V3 расширяет const fn surface с usability + control flow:

**Ф.1 (`#fn_eval_max_depth(N)` attribute):**

Per-fn override const fn evaluator recursion depth limit.

*Syntax:*
```nova
#fn_eval_max_depth(N)
fn deep_recursive(const x int) -> const int =>
    if x <= 0 { 0 } else { x + deep_recursive(x - 1) }
```

- `N` — int literal в диапазоне `1..=65535` (parser-enforced;
  out-of-range → compile error).
- Default (без attribute) — **256** (constant `MAX_EVAL_DEPTH`
  в `compiler-codegen/src/const_fn_eval.rs`).
- При reach limit → `E_CONST_FN_EVAL_DEPTH_EXCEEDED` с error message
  упоминающим attribute как способ override.

*Семантика:*
- Override применяется на eval_call_inner — depth check выполняется
  ПЕРЕД memoization lookup (защита от runaway recursion даже когда
  memo cache effective).
- Attribute lookup происходит per-call-site через `FnDecl.fn_eval_max_depth:
  Option<u32>` — каждая const fn имеет собственный override.
- Memoization работает независимо: повторные calls с identical args
  кэшируются вне зависимости от depth limit.

*Use cases:*
- Deep recursion: `#fn_eval_max_depth(1024)` для factorial / fibonacci
  с большими аргументами (caveat below).
- Conservative limit: `#fn_eval_max_depth(10)` для protect against
  user-induced infinite recursion в API-design.

*Caveat:* Rust thread call stack (~8 MB) limits actual deep recursion
независимо от override. Practical limit `N <= ~150-200` без stack
overflow в evaluator. Real production deep recursion — followup
`[M-114.4.4-iterative-evaluator]` V4 (rewrite evaluator on iterative
form с explicit stack).

*Cross-ref:* `MAX_LOOP_ITERATIONS` (Ф.3 loops, default 10_000) —
аналогичный guard для iteration-based termination; configurable
attribute `#fn_eval_max_iterations(N)` — V4 followup.

**Ф.2 (friendly UX errors):**
- `ro f = const_fn` runtime binding → `E_CONST_FN_FIRST_CLASS` с
  actionable suggestions (alias OR lambda wrap).
- `map(arr, const_fn)` runtime HOF → `E_CONST_FN_FIRST_CLASS_RUNTIME_HOF`
  с suggestion `|args| const_fn(args)`.
- Walker validate_const_fn_runtime_uses runs after rewriter.

**Ф.3 (loops в body):**
- `for x in 0..n { body }` allowed — literal Range iter only V3.0
  (followup `[M-114.4.4-for-iter-array]` для array iter).
- `while cond { body }` allowed.
- `loop { body }` allowed (must `break` to exit).
- `break`/`continue` working — propagate through nested if/blocks.
- `mut` let bindings allowed (для accumulator pattern).
- `assignment` allowed.
- `MAX_LOOP_ITERATIONS = 10_000` — anti-infinite-loop guard. New error
  `E_CONST_FN_EVAL_ITERATIONS_EXCEEDED`.

**Extracted V3 followups (Plan 114.4.4.1-5):**
- Plan 114.4.4.1 — record/sum patterns в match (V2.1; ConstValue
  extension needed).
- Plan 114.4.4.2 — t-reflection (size_of[T]/align_of[T]; type layout
  integration).
- Plan 114.4.4.3 — runtime HOF (trampoline ABI design).
- Plan 114.4.4.4 — closure-returning const fn.
- Plan 114.4.4.5 — true per-const-arg monomorphization.

### V4 extensions (Plan 114.4.4 finish session, 2026-06-02)

**Ф.4 — Record/sum/tuple patterns в match:**

Pattern V2.0 subset → V4 расширен с structured destructuring:
- `(a, b)` — tuple pattern.
- `Variant`, `Variant(p1, p2)`, `Cons(h, ..)` — sum-type destructuring.
- `{ field: pat }` или `{ field }` (shorthand) — record destructuring.
- `pat as name` — binding pattern.

ConstValue extended с `Tuple(Vec<ConstValue>)`, `Variant(String,
Vec<ConstValue>)`, `Record(Vec<(String, ConstValue)>)` variants.

Variant constructor recognition heuristic: `Call(Ident(Name), args)` где
`Name` starts с uppercase letter и НЕ const fn → constructed as
`ConstValue::Variant`. Lowercase Idents trigger const fn lookup.

**Ф.5 — Type reflection (size_of/align_of):**

Built-in intrinsics evaluating at compile time:
```nova
const SIZE_INT = size_of[int]()        // 8
const SIZE_BOOL = size_of[bool]()       // 1
const ALIGN_F64 = align_of[f64]()      // 8

fn buf_size(const count int) -> const int =>
    count * size_of[int]()              // works в const fn body
```

V4.0 surface (primitive types only — hardcoded per default 64-bit ABI):
- `int` / `i64` / `u64` / `f64` → 8 bytes.
- `i32` / `u32` / `f32` → 4 bytes.
- `i16` / `u16` → 2 bytes.
- `i8` / `u8` / `bool` → 1 byte.
- `char` → 4 bytes (u32 codepoint per Plan Q-char-literals).
- `str` → 16 bytes (pointer + length per Plan 26 prelude).

Records / sum-types / generic types → V4 followup
`[M-114.4.4-record-reflection]` (Plan 114.4.4.2 covers it).

`size_of`/`align_of` recognized as built-in identifiers в name resolution
(special-cased в `is_known`); replaced литералом в rewriter pass до
codegen.

### V4.1 extensions (Plan 114.4.4 V4.1 session, 2026-06-02)

**Mono-specialization — per-const-arg true monomorphization (Plan 114.4.4.5):**

V3 baseline (Plan 114.4.3 Ф.3): mixed const fn (e.g. `fn scale(const
factor int, x int) -> int`) compiled as regular runtime fn — const
param `factor` purely informational. V4.1 lands true monomorphization:

```nova
fn scale(const factor int, x int) -> int => x * factor

ro a = scale(3, x)    // generates fn `scale__cst_0(x) => x * 3`
ro b = scale(3, y)    // reuses `scale__cst_0` (same const arg)
ro c = scale(10, z)   // generates fn `scale__cst_1(x) => x * 10`
```

*Semantics:*
- Each unique (mixed_fn_name, const_args_tuple) tuple → отдельная
  specialized C fn `<orig>__cst_<idx>` где const params substituted
  с literal values в body, dropped из signature.
- Per-compilation cache deduplicates: identical (fn, const_args)
  reuses spec name.
- Mixed fn original AST kept as template (used для cloning); codegen
  emits both original (mostly dead) + all specializations.

*Implementation:* New module `compiler-codegen/src/const_fn_mono.rs`.
Pipeline placement: AFTER `rewrite_const_fn_calls` (fully-const fns
already dropped + size_of replaced). Mono pass walks all call sites,
generates spec FnDecls с literal substitution в body, rewrites Call
sites с new spec names + drops const args.

*Use cases:* Performance optimization для hot loops с known const
parameters (loop unrolling, branch elimination, SSE-like vectorization).
Const generics-style API design (Rust analog).

### V4.2 extensions (Plan 114.4.4.3 V4.2 session, 2026-06-02)

**Runtime trampoline для first-class const fn use (Plan 114.4.4.3):**

V3 baseline (Plan 114.4.4 Ф.2): const fn name использованное в non-callee
position (`ro f = const_fn`, `apply(const_fn, x)`) → friendly errors
`E_CONST_FN_FIRST_CLASS` / `E_CONST_FN_FIRST_CLASS_RUNTIME_HOF` с
suggestion обернуть в lambda. V4.2 supersedes этот ограничивающий
поведение автоматической генерацией runtime trampoline:

```nova
fn double(const x int) -> const int => x * 2
fn apply(f fn(int) -> int, x int) -> int => f(x)

test "HOF use" {
    ro r = apply(double, 5)   // V3: error. V4.2: compiler emits
                              // `double__trampoline(int x) -> int { x * 2 }`
                              // и переписывает `double` → `double__trampoline`.
    assert(r == 10)
}
```

*Semantics:*
- Для каждого fully-const fn `f`, используемого в non-callee position
  (Call.args, Let.value, Assign.value, Return.value), компилятор
  генерирует runtime trampoline fn с именем `<f>__trampoline`.
- Trampoline — клон оригинала с demoted modifiers: `const` параметры
  становятся обычными runtime, `-> const T` → `-> T`. Body unchanged
  (т.к. body fully-const fn состоит из выражений валидных и в runtime).
- Транзитивный вызов: если body trampoline вызывает другой fully-const
  fn, тот fn тоже добавляется в trampoline-set, и call внутри body
  переписывается на `<other>__trampoline`. Fixed-point reachability.
- `size_of[T]()` / `align_of[T]()` intrinsics в trampoline body
  substituted с literal Int при генерации body (V4.0 primitive limit).
- Alias resolution: `const ALIAS = const_fn; apply(ALIAS, x)` →
  alias resolved к `const_fn`, который trampolines.
- Original fully-const fn декларация всё ещё dropped из codegen pass
  (call sites уже inlined литералами); trampoline имеет распознаваемый
  суффикс и переживает retain step.

*Implementation:* New module `compiler-codegen/src/const_fn_trampoline.rs`.
Pipeline placement: внутри `rewrite_const_fn_calls`, ПОСЛЕ main walker
(inlining + intrinsic eval), ДО validate + retain.

*V4.2 limitations:*
- Generic const fn rejected (`E_CONST_FN_TRAMPOLINE_GENERIC`) — trampoline
  body нужны concrete types для intrinsic substitution. Followup
  `[M-114.4.4-trampoline-generics]`.
- Closure literals в body — Plan 114.4.4.4 territory.

### V4.3 extensions (Plan 114.4.4.4 V4.3 session, 2026-06-02)

**Closure-returning const fn — comptime closure specialization (Plan 114.4.4.4):**

V3 baseline (Plan 114.4.4 Ф.1): closure literals в const fn body отвергались
с `E_CONST_FN_EFFECT_IN_BODY`. V4.3 разрешает специальную форму — const
fn чьё body — single closure literal:

```nova
fn make_adder(const n int) -> const fn(int) -> int =>
    |x| x + n   // captures const param n

test {
    ro adder5 = make_adder(5)   // ⇒ Ident("make_adder__closure_0")
    assert(adder5(3) == 8)      // calls specialized fn (x int) -> int { x + 5 }

    ro m3 = make_mul(3)         // distinct spec __closure_1
    ro m3_again = make_mul(3)   // reuses __closure_1 (memoized)
}
```

*Semantics:*
- Detect: const fn whose body is `FnBody::Expr(Lambda | ClosureLight | ClosureFull)`.
- At each Call site `host_fn(LITERAL_ARGS)`:
  - Memoize per (host_fn_name, const_args_tuple); identical args reuse spec.
  - Generate specialized top-level fn `<host>__closure_<idx>` where:
    - Params = closure params (типы выведены: Lambda/ClosureFull explicit
      annotations OR host fn's `-> const fn(T1, ..) -> R` declaration для
      ClosureLight untyped).
    - Body = closure body с host's const params substituted с literal values.
    - Return type = closure's explicit annotation OR host's declared `R`.
  - Replace Call expression с `Ident(spec_name)` — Nova принимает bare
    fn name как fn pointer.

*Body validation extension:* `check_const_fn_decl` детектирует closure-at-top-level
case и расширяет scope (host const params + closure params) при validation
closure body — body validated по обычным V1 rules (literals/arithmetic/control
flow/calls к другим const fn) с extended ident scope.

*Implementation:* New module `compiler-codegen/src/const_fn_closure.rs`.
Pipeline placement: внутри `rewrite_const_fn_calls` **ДО** main walker
(чтобы calls к closure-returning fns были already rewritten к Idents и
walker не пытался eval_call их через interpreter, который не умеет
closure ConstValue).

*V4.3 limitations:*
- First-class use of closure-returning fn name (`ro f = make_adder` без
  immediate call) rejected: each call produces distinct specialized
  closure → no single trampoline. Friendly error
  `E_CONST_FN_CLOSURE_FIRST_CLASS`.
- Untyped closure `|x| body` requires host's `-> const fn(T) -> R`
  declaration для type inference (Lambda/ClosureFull с explicit types
  работают независимо).
- Closure param arity must match host's `fn(..)` declaration arity
  (`E_CONST_FN_CLOSURE_ARITY`).
- Generic closure-returning const fn — V2 followup
  `[M-114.4.4-closure-generic]`.

### V3+V4+V4.1+V4.2+V4.3 acceptance — A27-A35 (landed)

| # | Критерий | Verification |
|---|---|---|
| A27 | `#fn_eval_max_depth(N)` override работает | Ф.1 fixtures |
| A28 | Runtime-let `ro f = const_fn` через trampoline | runtime_hof_let_binding_ok (V4.2) |
| A29 | HOF passing через trampoline | runtime_hof_arg_ok (V4.2) |
| A30 | Loops + mut/assign/break/continue работают | Ф.3 fixtures |
| A31 | Record/sum/tuple patterns в match destructure | Ф.4 fixtures |
| A32 | `size_of[T]()` / `align_of[T]()` для primitives | Ф.5 fixtures |
| A33 | Mixed const fn per-arg monomorphization | mono_specialization_ok fixture |
| A34 | Const fn first-class use через runtime trampoline | runtime_hof_*_ok fixtures (5 шт.) |
| A35 | Closure-returning const fn — comptime specialization | closure_from_const_fn_*_ok fixtures (4 шт.) |

### V4.4 — rename `sizeof` → `size_of` для Rust-style consistency (2026-06-02)

**Status:** ✅ LANDED.

Plan 114.4.4 Ф.5 V4 originally shipped `sizeof[T]()` (без подчёркивания) +
`align_of[T]()` (с подчёркиванием) — inconsistent. Following Rust's
`std::mem::size_of::<T>()` / `align_of::<T>()` naming, оба intrinsic
теперь имеют consistent `<verb>_of` form:
- ❌ `sizeof[T]()` (V4.0/V4.4 Ф.1) → ✅ `size_of[T]()` (V4.4 rename).
- ✅ `align_of[T]()` (unchanged).

**Migration:** Bootstrap-stage breaking change — no deprecation shim
shipped (Nova не имеет внешних пользователей yet). All fixtures
+ spec/docs updated atomically с rename. Future code must use
`size_of[T]()`.

**Implementation:** Single-token rename в parser/types/eval/trampoline
recognition tables. Fixture files renamed `sizeof_*.nv` → `size_of_*.nv`.

**User-facing docs:** см. [docs/guide/size-of-align-of.md](../../docs/guide/size-of-align-of.md) —
concept doc с детальным explanation: что возвращает, зачем нужно
(CPU memory alignment), layout semantics composite types, padding
edge cases, Rust comparison, V4.4 limitations.

### V4.4 extensions (Plan 114.4.4 V4.4 session, 2026-06-02)

**Ф.1 — size_of/align_of для composite types (closes [M-114.4.4-trampoline-record-reflection]):**

`type_size_or_align` расширен от primitives-only до композитных типов
БЕЗ TypeDecl lookup:
- Tuples `(T1, T2, ..)` — C struct-style layout с natural alignment + tail-pad.
- FixedArray `[N]T` — `N * size_of(T)`, element alignment.
- Array `[]T` — slice ABI = 16 bytes (pointer + length), align 8.
- Unit `()` — size 0, align 1.
- Readonly `ro T` — same layout as T.
- Primitive table теперь (size, align) tuples — `str = (16, 8)` для slice ABI.

Still V2 (требует TypeDecl access):
- Named user-defined records/sum-types → followup `[M-114.4.4-trampoline-named-types]`.
- Generic instantiations (`Option[int]` etc) — same followup.

**Ф.2 — closure-returning const fn captures outer const locals (closes [M-114.4.4-closure-captures-outer]):**

Расширяет V4.3 closure-from-const-fn для body форма
`FnBody::Block { stmts: all Stmt::Const, trailing: closure_literal }`:

```nova
fn make_thing(const base int) -> const fn(int) -> int {
    const offset = base + 10
    fn(x int) -> int => x + offset
}
ro t5 = make_thing(5)   // spec body: x + 15 (offset evaluated)
```

При специализации: каждый outer const eval'ится в порядке declaration
с running subst map (host params + prior outer consts); результаты
добавляются в map, потом specialize closure body с full subst.

**Parser note:** `|x| body` после `const x = ...` Stmt парсится как
binary OR. V4.4 Ф.2 workaround — explicit `fn(x T) -> R => body`
syntax (ClosureFull). Followup `[M-114.4.4-closure-light-after-const-stmt-parser]`.

**Ф.3 — generic const fn first-class use (DEFERRED, design pending):**

Goal: `apply_i(id, 7)` где `fn id[T](const x T) -> const T => x` —
should generate `id__trampoline_int` per generic instantiation.

Blocker: requires one of:
1. Parser support for TurboFish-as-fn-value (`id[int]` без postfix continuation —
   currently parsed как Index expression).
2. HOF context type inference — look up callee's expected fn-type and
   infer generic type args. Requires fn signature registry access
   at trampoline pass time (cross-cutting design).
3. Explicit type annotation: `ro f fn(int) -> int = id` — requires
   type-checker integration to read annotation at trampoline pass time.

V4.4 Ф.3 не shipped — design question (1/2/3) ждёт user input.
Tracked: `[M-114.4.4-trampoline-generics]`.

**Ф.4 — generic closure-returning const fn (DEFERRED, same blocker as Ф.3):**

Same design challenge — generic instantiation requires same parser/type-inference
infrastructure as Ф.3. Will be unblocked together. Tracked:
`[M-114.4.4-closure-generic]`.

### V3+V4+V4.1+V4.2+V4.3+V4.4 acceptance — A36-A37 added

| # | Критерий | Verification |
|---|---|---|
| ... A27-A35 ... | (см. выше) | (см. выше) |
| A36 | size_of/align_of для tuples/arrays/Unit/Readonly | size_of_tuple_ok / size_of_fixed_array_ok / size_of_nested_composite_ok (V4.4 Ф.1) |
| A37 | Closure-returning const fn с outer const captures | closure_outer_const_ok / closure_outer_chained_ok (V4.4 Ф.2) |
| A38 | Padding/alignment edge cases для tuples (layout semantics) | size_of_padding_ok (V4.4 Ф.1 docs) — 7 edge cases: inner/tail-pad, big gaps, no-pad uniform, multi-step alignment, unit, mixed sizes |
| A39 | Generic const fn first-class via HOF inference | trampoline_generic_inferred_ok / _two_types_ok / _no_context_neg (V4.5 Ф.3) |
| A40 | Generic closure-returning const fn via TurboFish | closure_generic_ok / closure_generic_const_arg_ok / _no_turbofish_neg (V4.5 Ф.4) |
| A41 | size_of/align_of для user records + sums | size_of_record_ok / size_of_sum_ok (V4.6 M1) |
| A42 | ClosureLight `\|x\|` после Stmt::Const parses correctly | closure_outer_pipe_ok (V4.6 M2) |
| A43 | Closure-generic HOF inference without TurboFish | closure_generic_hof_inferred_ok (V4.6 M3) |
| A44 | Composite concrete types (Tuple/Array) в generic instantiation | trampoline_generic_tuple_concrete_ok / closure_generic_tuple_turbofish_ok (V4.6 M4) |

### V4.6 extensions (Plan 114.4.4 V4.6 final-followups session, 2026-06-03)

**M1 — size_of/align_of для user-defined records + sums (closes [M-114.4.4-trampoline-named-types]):**

V4.4 Ф.1 расширил layout computation до composite ABI types но всё ещё
ограничивался primitive Named types. M1 finalizes к user-defined Named:

- **Records** `type T { f1, f2 }` — C struct layout, natural alignment +
  tail-pad to max-field-align.
- **NamedTuple** `type T(f1, f2)` (Plan 120 forward-compat) — same as Record.
- **Sum-types** `type Color | Red | Green | Blue` — tag (i32 = 4 bytes,
  align 4) + max-variant payload, align = max(4, max_payload_align).
- **Variants:** Unit (size 0), Tuple (sum + padding), Record (struct layout).
- **Newtype** / **Alias** — transparent.
- **Opaque/Effect/Protocol** — None (no concrete layout).

Implementation via TypeDecl registry: `type_size_or_align_resolved` accepts
registry, backward-compat wrapper uses thread-local via `TypeDeclRegistryGuard`
(RAII install/clear). Registry built at start of `rewrite_const_fn_calls`.

**M2 — Parser disambiguation `|x|` after Stmt::Const (closes [M-114.4.4-closure-light-after-const-stmt-parser]):**

V4.4 Ф.2 documented parser ambiguity workaround. M2 implements lookahead
disambiguation в `parse_bit_or`:

```nova
fn make_thing(const base int) -> const fn(int) -> int {
    const offset = base + 10
    |x| x + offset   // V4.6 M2: parses correctly as ClosureLight (was binary OR)
}
```

`looks_like_closure_light_params` lookahead helper peeks pattern
`|ident<,ident>*|`. If detected, bit-or continuation breaks; ClosureLight
starts as new expression.

**M3 — Closure-generic HOF inference (closes [M-114.4.4-closure-generic-hof-inference]):**

V4.5 Ф.4 required explicit TurboFish at call site. M3 extends к HOF context
where TurboFish becomes optional:

```nova
fn make_id[T]() -> const fn(T) -> T => fn(x T) -> T => x
fn apply_i(f fn(int) -> int, x int) -> int => f(x)
test {
    ro r = apply_i(make_id(), 7)   // T=int inferred from apply_i's f param
}
```

`GenericClosureHofCtx` pre-pass walks module mutably, at each Call site
checks args; for bare `Call(generic_closure_fn, [], None)` без TurboFish,
matches host's return-type Func against expected param Func via
`unify_simple`. Wraps Ident в TurboFish; existing rewriter handles rest.

**M4 — Composite concrete types (closes [M-114.4.4-trampoline-complex-concrete]):**

V4.5 ограничивался simple Named concrete types (`int`, `str`). M4 расширяет
к composite TypeRef в both trampoline and closure paths:

- Tuple `(int, int)` → mangle `tup2_int_int`
- Array `[]int` → `arr_int`
- FixedArray `[4]int` → `fixarr4_int`
- Named-with-generics `Option[int]` → `Option_int`
- Unit `()` → `unit`
- Readonly `ro T` → `ro_<m>`
- Func type → `fn<n>_<params>_ret_<ret>`

`GenericInst.concrete` теперь `Vec<TypeRef>`. Hash/Eq via `mangled_args()`
stable serialization. `mangle_type_ref` pub helper exported для cross-module
use. Closure spec value: `(spec_name, Vec<TypeRef>)` — TypeRefs retained
для substitution.

### 🎯 Plan 114.4.4 family COMPLETE — V3-V4.6 (всё закрыто)

- ✅ V3 (Ф.1-Ф.5) + V4 (Ф.1-Ф.5) — base.
- ✅ V4.1 mono-specialization (Plan 114.4.4.5).
- ✅ V4.2 runtime trampoline (Plan 114.4.4.3).
- ✅ V4.3 closure-from-const-fn (Plan 114.4.4.4).
- ✅ V4.4 Ф.1+Ф.2 composite-sizeof + outer-captures (Plan 114.4.4.6).
- ✅ V4.5 Ф.3+Ф.4 generic trampoline + generic closure.
- ✅ V4.6 M1+M2+M3+M4 (Plan 114.4.4.7).

**Cumulative:** 15 markers `[M-114.4.4-*]` все закрыты, A1-A44 acceptance,
70/70 fixtures PASS на release nova-cli.

### V4.5 extensions (Plan 114.4.4 V4.5 followups session, 2026-06-02)

**Ф.3 — Generic const fn first-class через HOF type inference (closes [M-114.4.4-trampoline-generics]):**

V4.2 baseline отвергал generic const fn как first-class value. V4.5 Ф.3
снимает ограничение: при passing generic const fn как arg в HOF callee,
компилятор infers concrete generic types из callee's expected param fn-type:

```nova
fn id[T](const x T) -> const T => x
fn apply_i(f fn(int) -> int, x int) -> int => f(x)

test {
    ro r = apply_i(id, 7)   // inferred T=int → id__trampoline_int generated
    assert(r == 7)
}
```

*Semantics:*
- Build fn signature registry от non-const fns в модуле.
- На каждом Call.args site проверить: arg = Ident matching generic const
  fn AND callee's expected param[i].ty = TypeRef::Func.
- Unification: structural recursion (Named/Tuple/Array/FixedArray/Func/
  Unit/Readonly), bind generic params к concrete types.
- Generate specialized monomorph FnDecl с substituted TypeRef positions
  + dropped generics + demoted const modifiers.
- Mangled name: `<orig>__trampoline_<T1>_<T2>...`
- In-place Ident rewrite at each Call.args site (multiple instantiations
  same name not disambiguable from later rewriter pass).

*V4.5 Ф.3 limitations:*
- Inference только в Call.args position. Let/Assign/Return positions
  require type annotation infrastructure (V2 followup).
- Concrete TurboFish args должны быть simple Named (V2 followup для
  nested generics).
- Multiple generic params supported (independent inference per position).

**Ф.4 — Generic closure-returning const fn via TurboFish (closes [M-114.4.4-closure-generic]):**

```nova
fn make_id[T]() -> const fn(T) -> T => fn(x T) -> T => x

test {
    ro id_i = make_id[int]()     // make_id__closure_int_0
    ro id_b = make_id[bool]()    // make_id__closure_bool_1 — distinct
}
```

*Semantics:*
- Detect closure-returning const fns с non-empty generics.
- Call rewriter accepts BOTH `Ident(name)(args)` AND `Ident(name)[T1,..](args)`
  (TurboFish form).
- Spec key extended: `(name, const_args, type_args_mangle)`.
- generate_closure_spec substitutes generic types в host return signature,
  closure params, closure body via shared `subst_type_ref_pub` helper
  (exposed from const_fn_trampoline.rs).
- Mangled name: `<orig>__closure_<T1>_<T2>_<idx>`.

*V4.5 Ф.4 limitations:*
- TurboFish required at call site (no HOF inference для closure-returning).
  Followup `[M-114.4.4-closure-generic-hof-inference]`.
- Concrete TurboFish args must be simple Named.
- TurboFish arity must match host's generics.

🎯 **Plan 114.4.4 family complete (umbrella + all V4.x sub-plans):**
- ✅ V3 (Ф.1-Ф.5) + V4 + V4.1 mono + V4.2 trampoline + V4.3 closure +
  V4.4 Ф.1 composite-sizeof + V4.4 Ф.2 outer-captures + V4.5 Ф.3
  generic-trampoline + V4.5 Ф.4 generic-closure.
- All `[M-114.4.4-*]` markers closed (11 cumulative).
- A1-A40 acceptance criteria.

🎯 **Plan 114.4.4 family extended status:**
- ✅ V3/V4/V4.1/V4.2/V4.3 phases (А1-А35) — landed earlier sessions.
- ✅ V4.4 Ф.1 + Ф.2 (А36-А37) — этот session.
- 🟡 V4.4 Ф.3/Ф.4 generics — design-blocked, tracked в `[M-114.4.4-trampoline-generics]` +
  `[M-114.4.4-closure-generic]`.

### V2 acceptance — A19-A26

| # | Критерий | Verification |
|---|---|---|
| A19 | `if`/`match` в body парсятся + evaluate + inline literal | T4.1 positives (3 fixtures) |
| A20 | Loops reject с pointer на V2.1 followup | T4.1 negatives |
| A21 | Recursion (direct + mutual) с depth-limit + memo | T4.2 (4 fixtures) |
| A22 | Termination — runtime depth-limit enforcement | T4.2 negative |
| A23 | Mixed const+runtime params + monomorphization | T4.3 positives |
| A24 | Mixed signature constraints enforced | T4.3 negative |
| A25 | Generic const fn (T-independent) + per-T mono | T4.4 |
| A26 | First-class alias resolution + drop из codegen | T4.5 |

См. Plan 114.4.3 closure для full T4 series listing.

---

## D409. `-> @`: автоматический возврат приёмника (2026-07-06)

> **Решение владельца.** Амендит [D181](02-types.md#d181-array-methods----fluent-mut-chain--slice-syntax)
> (беглые цепочки `-> @`) и D132 (ретракция требования «тело обязано
> завершаться `@`»). Синтаксис тел методов. ✅ IMPLEMENTED 2026-07-06.

### Что

Метод с возвратом `-> @` НЕ пишет возврат приёмника — он единственно
автоматический:

- отсутствие хвостового выражения в теле → неявный `return @`;
- голый `return` (без значения) в таком теле → `return @`;
- хвостовое выражение, само не являющееся приёмником (например, вызов
  НЕ-fluent метода), — discard'ится как statement, затем неявный `return @`;
- **explicit-формы запрещены** (амендмент владельца, усиление изначальной
  редакции: обратная совместимость снята, все существующие тела мигрированы):
  явный `@` в хвосте, `return @`, `=> @` → `E_EXPLICIT_SELF_RETURN`.
  Исключение — делегация в другой `-> @` метод (`=> @write()` /
  `return @other_fluent()`): это не self-return, а обычный вызов.

```nova
export fn Vec[T] mut @reserve(additional int) -> @ {
    ro needed = @len + additional
    if needed <= @cap { return }   \ = return @
    _grow(needed)
}                                   \ конец тела = return @
```

### Правило

1. Работает ТОЛЬКО для формы возврата `-> @`. Обычные `-> T` — без изменений.
2. Вернуть в `-> @`-методе что-либо, кроме приёмника, — ошибка (как и раньше).
3. Explicit `@`/`return @`/`=> @` в `-> @`-теле → `E_EXPLICIT_SELF_RETURN`.
4. Чекер: тело/ветки без значения — легальны (`check_fluent_return`);
   лоуэринг (`self_return_lower.rs`, AST-pass ПОСЛЕ check) подставляет возврат
   приёмника на каждом выходе без явного значения — codegen не меняется,
   переиспользуется существующая эмиссия manual-формы.

### Почему

`-> @` уже однозначно фиксирует, ЧТО возвращается, — тело должно лишь сделать
работу. Ручной `@`/`return @` — шум и источник рассинхрона (забытый `@` в одной
из веток). После D409 класс «забыл вернуть приёмник» невозможен по построению;
запрет explicit-форм убирает двоестилие (единственный способ написать —
правильный).

---

## D347. Same-scope re-binding (`ro`/`mut`/`consume x = ...` повторно; тип может меняться)

> **Plan 181.** Амендит [D184](#d184) (binding-грамматика). Cross-ref: D34 (отличие от
> отвергнутого `:=`), D90 §3 (defer eager), D131/D133/D180 (consume), D22 (замыкания),
> D217 (field-cache порядок), D274 (interp), D297 (LSP rename — followup).

**Что.** Повторное объявление того же имени в ОДНОМ scope полной binding-формой
(`ro`/`mut`/`consume x = ...`) — легально. Каждый rebind = **новая переменная**: свой
тип (может отличаться), своя мутабельность. Старое значение недоступно по имени ниже по
тексту (живёт до конца scope только для уже созданных захватов). Статус-кво до Plan 181
был дырой: фронтенд принимал (shadowing-типизация), бэкенд эмитил оба под ОДНИМ C-именем →
clang `redefinition` (ошибка в `.c`, не в `.nv`).

```nova
ro input = read_line()
ro input = input.trim()           \ тот же тип — pipeline (R5-тихо)
ro input = parse_request(input)?  \ str → Request; str-версия НЕДОСТУПНА ниже (R1)
mut work = work                   \ «разморозка» ro→mut (Rust: let mut x = x)
```

**Правило (R1–R7).**
- **R1 Явность.** Rebind — только полной формой `ro`/`mut`/`consume x = ...`. Голое `x = v`
  остаётся мутацией существующего `mut`-биндинга (грамматика D184 не меняется). Тип и
  мутабельность новой переменной независимы от старой.
- **R2 Consume-звучность (hard error `E_REBIND_LIVE_CONSUME`).** Затенение биндинга с
  непотреблённым consume-обязательством (D131/D133/D180) **в ТОМ ЖЕ scope** — ошибка на
  месте rebind («потребите/переименуйте»). Nova-эксклюзив: Rust-футган «затенённый guard
  живёт до конца scope» становится compile error (guard'ы Nova = consume-типы, D174). Ловит
  тихую **same-scope** double-consume-shadow утечку (B2). **Граница:** *nested-scope*
  блок-затенение того же класса (`consume tx=…; { consume tx=… } tx.commit()` — и в
  теле if/for/match/while-let) R2 НЕ ловит: alpha-rename по R7 не уникализирует cross-scope,
  `rebind_shadows` для него пуст, а consume-obligations ключуются по имени → один commit
  гасит оба. Это **pre-existing** дыра consume-чекера (идентична на baseline до Plan 181,
  НЕ регрессия), см. `[M-consume-nested-scope-shadow-leak]` — территория D131/D133.
- **R3 Нерекурсивность.** RHS rebind'а видит **старый** биндинг: `ro x = x + 1` читает
  прежний `x`. Haskell-грабли (`let x = f x` = `<<loop>>`) исключены.
- **R4 Захваты на момент создания.** Замыкание/`defer` видят биндинг, живой в точке
  создания замыкания / регистрации defer (D22 env-снапшот, D90 §3 eager). Rebind ниже по
  тексту НЕ меняет захваченное.
- **R5 Lint `W_SHADOW_UNRELATED`** (warn): новое значение не использует старое И старый
  биндинг ещё жив. Pipeline `ro x = f(x)` — тихо. Подавление `#allow(shadow)`.
- **R6 Параметры.** Затенять параметр можно (`fn f(x int) { ro x = x + 1 }`).
- **R7 Cross-scope без изменений.** Блочное затенение работает как прежде.

**Почему.** (1) Pipeline-идиома (unwrap/transform/validate под одним именем) убирает
`input2`/`input3`-мусор; давление в сторону immutable `ro`. (2) Прецедент: Rust / OCaml /
F# (в функциях) / Elixir — rebind идиоматичен десятилетиями; ФП-норма. (3) Реализация
дешёвая — один alpha-renaming pass после parse уникализирует 2-й+ same-scope биндинг в
`x__s1`, `x__s2`, …, original-имя в side-channel для диагностик; весь остальной компилятор
(consume-checker, codegen, замыкания, verify) видит уже уникальные имена. Pass попутно
чинит 3 смежных бага: false-positive D133 при rebind ПОТРЕБЛЁННого consume; тихую
double-consume утечку (obligations по имени); расхождение чекер↔codegen на `ro x = x + 1`.

**Отвергнуто.** (a) **Source-SSA / `:=` (Go-модель)** — D34 отверг `:=` за «shadowing-баги
Go»: у `:=` дефект — *СЛУЧАЙНОЕ* затенение из-за смешанного decl/assign-оператора + cross-
scope протечки (класс `err`-багов). У D347 иначе: *ЯВНОЕ* `ro`/`mut`/`consume` +
same-scope (старое имя недоступно ниже) + R2/R5 guard'ы. Инверсия мейнстрима (Java/C#/TS
запретили *безопасное* same-scope, разрешили *опасное* cross-scope): Nova — same-scope
свободно с правилами, cross-scope как есть. (b) **Запрет (`E_DUPLICATE_LOCAL`)** —
рассмотрен как fallback; отвергнут: фича обоснована, а alpha-pass дёшев и чинит B1/B2/B3
попутно. (c) **Erlang/Zig-строгость** («имя = одна вещь») — порождает `State1/State2`-класс
багов, отвергнута. (d) **flow-narrowing вместо rebind** (TS/Kotlin/Swift) — покрывает
«тот же объект, уточнённый тип», но НЕ «трансформация с потерей старого» (`ro s = parse(s)`).

**Таблица проб (эмпирика 2026-07-02, 11 проб) — целевое поведение после Plan 181.**

| Проба | Форма | Поведение |
|---|---|---|
| p01 | `ro x=1; ro x=2` same type | POS |
| p02 | `ro x=1; ro x="s"` смена типа | POS (str-версия ниже) |
| p03/p04 | `mut x=1; ro x=2` / `ro x=1; mut x=x` | POS (разморозка) |
| p05 | вложенный блок `{ ro x=2 }` | POS (inner≠outer, R7) |
| p06 | closure + rebind | POS runtime (f()==старое, R4) |
| p07 | `for i { ro x=i }` | POS (fresh на итерацию) |
| p08 | `consume sb=…; sb.append(); ro sb=5` | NEG `E_REBIND_LIVE_CONSUME` (R2) |
| p08b | consume ПОТРЕБЛЁН `into_str()` → `ro sb=5` | POS (B1 fixed) |
| p08c | `consume tx=…; consume tx=…` (**same** scope) | NEG `E_REBIND_LIVE_CONSUME` (B2 same-scope leak; nested-scope — pre-existing gap) |
| p09 | `fn f(x){ ro x=x+1 }` param-shadow | POS (R6) |
| p10 | `ro x=1; ro x=x+1` | POS x==2 (R3, B3 fixed) |

**Связь / границы.** Amend D184 (binding), D90 §3 (defer), D131/D133/D180 (R2), D22/D90
(R4), D34 (отличие от `:=`). Alpha-pass врезан ДО канала resolved_types (172.1 M0). LSP
rename (D297 V1) переименует оба одноимённых биндинга — pre-existing долг (уже сломан для
nested shadow), честный фикс = V2 symbol table. Destructuring-rebind (`ro (a,b)=…`
повторно) — tuple-дубли уникализируются, R5-lint на них V2. Rebind САМОГО pattern-bound
имени внутри matching-ветки (`if Some(u)={ ro u=… }`) — вне scope (pre-existing
checker-overflow), см. `[M-181-pattern-var-rebind]`. Nested-scope double-consume-shadow
утечка (R2 ловит только same-scope) — pre-existing gap consume-чекера, см.
`[M-consume-nested-scope-shadow-leak]`.

---

## D188. `Cleanup[E]` protocol + `consume X = expr { body }` scope-block

> **Plan 110.** Принято 2026-05-31. **Статус: ACTIVE** (Plan 110.1+110.2
> +110.4+110.5 landed 2026-06-01; Plan 110.9 V1.1 partial 2026-06-03 —
> M-110.9.2/110.9.4/110.9.5 closed, M-110.9.1/110.9.3 deferred). Radical
> simplification cleanup-семейства: один keyword `consume` + один protocol
> `Cleanup[E]` покрывают ~95% cleanup use-cases, оставляя `defer { }`
> для оставшихся 5%. Amends [D90](#d90), [D158](#d158), [D161](#d161),
> [D162](#d162). Retracts [D160](#d160).
>
> **Plan 110.9 V1.1 partial closure (2026-06-03):**
> - ✅ M-110.9.4: `W_FFI_CANCEL_UNSAFE` lint enforcement (was parser-only).
> - ✅ M-110.9.5: `cleanup` strict signature check (return Unit + effects
>   only Fail[E] + no generics; D188-malformed-on-exit extended).
> - ✅ M-110.9.2: WithExitTimeout Level 1 per-type protocol — codegen
>   emits `Nova_<T>_method_exit_timeout_ms(binding)` lookup before
>   Application/hardcoded fallback (D192 Level 1 enabled).
> - 🟡 M-110.9.1 (typed throw codegen), M-110.9.3 (finalizer LIFO) —
>   deferred to focused dev-session.
>
> **Plan 110.9 V1.1 ✅ COMPLETE (2026-06-03):**
> - ✅ M-110.9.1: typed CleanupTimeoutError throw codegen (1377c611a57).
>   Static `_nova_throw_cleanup_timeout_impl` emitted, fn pointer assigned
>   в main; runtime `nv_shield_check_deadline` always takes typed path.
> - ✅ M-110.9.3: Application register_finalizer LIFO runtime (86028e74aac).
>   Three-layer: NovaFinalizerStack TLS + with-Application prologue/epilogue +
>   method dispatcher intercept. D195 R1+R2+R8 integrated.
>
> **Plan 110.9.5.a ✅ closed (2026-06-05):**
> - Removed pragmatic accept-both в `is_unit_tr` (types/mod.rs:2256).
>   Recon (Workflow `wf_ccdccc85-007`) confirmed parser ALREADY emits
>   canonical `TypeRef::Unit(Span)` для `()` (parser/mod.rs:5031); accept-both
>   was defensive code for non-existent variance. Now strict: only
>   `TypeRef::Unit` and `TypeRef::Readonly(Unit)`. Aligns с peer
>   `is_unit_or_none()` at line 9137.
>
> **Plan 110 — M-110-deadline-fire-fixture ✅ closed (2026-06-05):**
> - E2E fixture `deadline_fire_e2e_v1_1.nv` — verifies full pipeline:
>   Level 1 `exit_timeout_ms()` (1ms) + cleanup Time.sleep(50) +
>   Fail[CleanupTimeoutError] propagation. Unblocked после 110.9.2+110.9.1
>   landed earlier. Compile-success verifies all 6 codegen splices.
>
> 🎯 **Plan 110 family полностью завершена.** 9 sub-plans + 12 D-blocks
> ACTIVE + 76 fixtures PASS (74 baseline + 2 new V1.1.a closures).
>
> **Амендмент 2026-07-13 (решение владельца, Plan 201, ветка
> d188-consume-block):** вторая форма scope-block'а — **re-consume block**
> `consume X { body }` (без `= expr`) — re-consume СУЩЕСТВУЮЩЕГО
> owned-биндинга; блок — ВЫРАЖЕНИЕ. Внутри блока `X` — ro-view + методы;
> единственная легальная форма выноса владения — tail-значение `X` или
> `return X` (голый `X`; на этом пути cleanup ДИЗАРМИТСЯ); прочие формы
> выноса → `E_CONSUME_BLOCK_MOVE_OUT`; alias-копия — `stream.share()`;
> give/release-дизарм-примитив ОТВЕРГНУТ. См. §«Re-consume форма
> `consume X { body }`» ниже.
>
> **Амендмент v3 (2026-07-13, решение владельца «хочу», Plan 201, ветка
> d188-v3):** «однократный вынос через выражение» — tail/return EXPR (не
> только голый `X`) санкционирован, если `X` встречается РОВНО ОДИН раз,
> АРГУМЕНТОМ consume-параметра (`return Ok(TlsStream.wrap(stream,
> session))` — без промежуточного `mut session_out`/`ro s` танца);
> замыкание внутри EXPR, захватывающее `X` — ошибка; дизарм — в момент
> передачи владения (вход в consuming-вызов), всё что фейлит ДО этого в
> EXPR — cleanup ещё сработает. Композиция с multi-var-формой — по-
> переменно на каждом уровне вложенного desugar'а. См. §«Амендмент v3 —
> "однократный вынос через выражение"» ниже.

### Что

Вводятся два связанных языковых элемента:

1. **`Cleanup[E]` protocol** — контракт ресурсов, требующих cleanup.
2. **`consume X = expr { body }`** — scope-block, гарантирующий exactly-once
   вызов `cleanup` при выходе из `body` (success, throw, panic, cancel).
   У scope-block'а есть и вторая форма — `consume X { body }` (re-consume
   существующего owned-биндинга, амендмент 2026-07-13) — а также
   multi-var сахар `consume A, B, C { body }` над её вложением
   (амендмент D188-multivar, Plan 174).

```nova
type Cleanup[E] protocol {
    cleanup(outcome ScopeOutcome) Fail[E] -> ()
}

type ScopeOutcome
    | Success
    | Failure(any)
    | Panic(str)
```

- **`E`** — тип ошибок, которые `cleanup` ресурса сам может throw
  (commit failure, flush failure). Если ресурс infallible — `E = Never`
  (см. [D194](#d194)).
- **`ScopeOutcome`** — type-erased (Python `__exit__` pattern): ресурс
  не знает body error type. `Failure(any)` хранит throw/cancel-payload как
  существенно динамическое значение; route'ит через `if err is T`
  (D85 auto-narrowing).
- `Panic(str)` отдельно — это **bug**, не recoverable error.

### Syntax

```nova
consume IDENT = EXPR { BODY }              // binding-форма
consume IDENT { BODY }                     // re-consume форма (амендмент 2026-07-13)
consume IDENT (, IDENT)* { BODY }          // multi-var re-consume (амендмент D188-multivar, Plan 174)
```

- Parser lookahead решает между формами:
  - `consume X = expr { body }` — scope-block (этот D188; `{` после EXPR).
  - `consume X = expr` — raw linear binding (D180; для builder/transfer).
  - `consume X { body }` — re-consume block существующего owned-биндинга
    (амендмент 2026-07-13; см. §«Re-consume форма»). Дизамбиг: `IDENT`
    (со строчной буквы) + `{` на той же строке БЕЗ `=`. `Type { … }`
    (с заглавной) остаётся record-destructure pattern'ом raw-формы.
  - `consume A, B, C { body }` — multi-var re-consume (амендмент
    D188-multivar, Plan 174; см. §«Multi-var re-consume форма»).
    Дизамбиг: `IDENT` (со строчной буквы) + `,` на той же строке.
    Список идентов — ТОЛЬКО re-consume форма; смешение со `=`
    (`consume A, B = expr { … }`) — парс-ошибка (список идентов не
    поддерживает инициализатор).
- `IDENT` — single name. Destructure (`consume (a, b) = ...`) не разрешается
  для scope-block (один resource = один cleanup).
- `EXPR` должен statically resolve к типу `Cleanup[E]` для некоторого `E`
  (см. [D196](#d196)).
- `BODY` — block; `IDENT` доступен внутри как `ro` binding (нельзя reassign,
  можно вызывать методы, mutating через interior mutability разрешено).

### Desugaring

`consume tx = init() { body }` развёрнуто codegen'ом эквивалентно:

```nova
{
    ro _tx = init()                                  // R1: throws → exit
    ro _timeout = nv_resolve_exit_timeout(_tx)       // R4 (D192 3-level)
    ro _outcome = nv_run_body_capturing { body }     // captures Success/Failure(e)/Panic(m)
    nv_enter_cancel_shield(deadline: _timeout)       // R3
    match _outcome {
        Success      => _tx.cleanup(Success)
        Failure(e)   => { _tx.cleanup(Failure(e)); throw e }
        Panic(m)     => { _tx.cleanup(Panic(m)); nv_panic(m) }   // Plan 173 Ф.5 #9: реальный примитив (nv_resume_panic — фикция, ретирована)
    }
    nv_leave_cancel_shield()
}
```

Где `nv_run_body_capturing { body }` — codegen-emitted construct:
- normal exit → `Success`
- `throw e` / `?` propagation / cancel-as-throw (D90 §7 amend) → `Failure(e)`
- `panic(m)` → `Panic(m)`
- `exit(code)` — НЕ captures; process exit'ы напрямую (handler.cleanup не runs).

### Re-consume форма `consume X { body }` (амендмент 2026-07-13, Plan 201)

Вторая форма scope-block'а — БЕЗ инициализатора: re-consume уже
существующего owned-биндинга (consume-параметра функции или owned-локала
`consume X = …`, [D180](05-memory.md#d180-consume-binding-syntax-plan-731)).
Даёт те же гарантии, что и binding-форма: `cleanup` exactly-once на всех
cleanup-путях выхода (R2), cancel-shield (R3), LIFO-композиция (R5);
после блока `X` недоступен (consumed, D131).

**Блок — ВЫРАЖЕНИЕ**: `ro s = consume X { …; <tail> }` — значение блока
= tail-выражение. Приёмник (`ro`/`mut`/`consume s = …`) объявляется в
объемлющем scope после блока.

```nova
fn serve(consume stream TcpStream) Net Fail[NetError] -> () {
    consume stream {                          // re-consume owned-параметра
        ro banner = stream.read_bytes(64)?   // throw → cleanup(Failure) → закрыт
        stream.write_all("hello".bytes())?
    }
    // stream consumed — использование после блока = compile error (D131)
}
```

#### Правила формы

1. **Owned-требование.** `X` — существующий owned-биндинг: consume-параметр
   или `consume X = …` локал. Не-owned (`ro`/`mut` локал, view-параметр,
   параметр без `consume`) → ошибка **`E_CONSUME_BLOCK_NOT_OWNED`**.
2. **Cleanup[E]-требование** — как у binding-формы: тип `X` обязан
   реализовывать `Cleanup[E]`; нет `cleanup`-метода →
   **`E_D188_NOT_CLEANUP`** (у binding-формы исторический код той же
   ошибки — `D188-not-consumable`).
3. **Вынос владения — tail/return самого `X`, либо (амендмент v3 ниже)
   `X` РОВНО ОДИН раз в consume-позиции внутри tail/return EXPR.**
   Легальные формы выноса — tail-значение `X` (при биндинге-
   приёмнике) или `return X` (ГОЛЫЙ `X`), а также — амендмент v3,
   2026-07-13, Plan 201, §«Амендмент v3» ниже — tail/return **EXPR**, где
   `X` встречается ровно один раз в consume-позиции: аргументом
   consume-параметра (`return Ctor(x, other)`,
   `return Ok(TlsStream.wrap(stream, session))`) или — M-178-амендмент
   v3 п.2 — инициализатором consume-поля record-литерала
   (`return Ok(TlsStream { tcp: stream, session })`).
   На этих путях cleanup блока **дизармится**, владение уходит результату
   блока / из функции — приёмник получает обязательство (D133). Все
   ОСТАЛЬНЫЕ пути выхода (конец тела, throw, panic, cancel, break/continue,
   `return <не-X>` с нуля/>1 вхождений X не в consume-позиции) — cleanup
   exactly-once (R2 как есть).
4. **Внутри блока `X` — ro-view + вызовы методов.** Прочие формы выноса —
   передача `X` в consume-параметр ВНЕ санкционированной tail/return-
   позиции (п.3 / амендмент v3), вызов consume-метода (включая
   `X.close()`), присваивание `X` в consume-поле / любое место
   (`place = X`, `T { field: X }` — вне санкционированной tail/return-
   позиции п.3), повторный `consume X { }`, реассайн
   `X = …` — ошибка компиляции **`E_CONSUME_BLOCK_MOVE_OUT`** с текстом:
   «вынос владения из consume-блока — только tail/return самого `X` либо
   ровно одно вхождение `X` аргументом consume-параметра в tail/return
   EXPR; либо возьмите `@share()`-копию». Ручной `X.cleanup(...)` —
   по-прежнему `D188-r2-manual-on-exit`.
5. **Alias-копия — `@share()`.** Второй владелец того же ресурса — только
   явной share-копией со СВОИМ consume-обязательством: `TcpStream
   @share()` — alias одного TCP-потока с refcount-close («закрывает
   последний»; НЕ независимая копия — см. std/src/net/tcp.nv и
   переименование `ChanWriter.clone()` → `share()`).

#### Канонический TLS-паттерн (заменяет ручные `stream.close()`)

v2 (голый tail-вынос + wrap СНАРУЖИ блока):

```nova
ro s = consume stream {
    // … handshake по ro-view `stream`; любая ошибка/throw →
    // cleanup(Failure) закрывает stream exactly-once …
    pump_handshake(stream, session)?
    stream                                  // tail-вынос: cleanup дизармлен,
}                                           // владение уехало в `s`
Ok(TlsStream.wrap(s, session))              // s → consume-параметр wrap
```

v3 (амендмент ниже, 2026-07-13, Plan 201) — `wrap(...)` ПРЯМО в `return`
внутри арма, без промежуточного mut-локала/tail-выноса:

```nova
consume stream {
    ro c = match build_client_cfg(config) {
        Err(e) => { return Err(e) }             // cleanup закрывает stream
        Ok(cc) => cc
    }
    ro session = /* … tls_client_new … */
    match pump_handshake(stream, session) {
        Ok(_)  => { return Ok(TlsStream.wrap(stream, session)) }  // дизарм
        Err(e) => { return Err(e) }             // cleanup закрывает stream
    }
}
```

### Multi-var re-consume форма `consume A, B, C { body }` (амендмент D188-multivar, Plan 174)

Список из ≥ 2 идентов через запятую — **ЧИСТЫЙ САХАР** над вложением
re-consume-форм, разворачиваемый парсером ДО type-check/codegen:

```nova
consume A, B, C { body }
// ≡
consume A {
    consume B {
        consume C { body }
    }
}
```

Cleanup срабатывает в **LIFO-порядке** (`C`, потом `B`, потом `A`) — не
отдельная механика, а прямое следствие вложенности desugar'а (внутренний
scope закрывается первым, при выходе из него — внешний). Каждый идент —
независимый re-consume-блок: **все правила формы** (§«Правила формы»
выше — owned-требование `E_CONSUME_BLOCK_NOT_OWNED`, `Cleanup[E]`-
требование `E_D188_NOT_CLEANUP`, guard/`E_CONSUME_BLOCK_MOVE_OUT`)
действуют на каждый идент **независимо** — checker и codegen не знают
про multi-форму вообще, видят только цепочку вложенных
`consume IDENT { … }`.

**Tail/return-вынос — ПОИМЕНОВАННЫЙ, дизармит СВОЙ cleanup.** Поскольку
только САМЫЙ внутренний идент (`C` в примере) непосредственно граничит
с реальным телом `body`, только голый `C` в tail/`return`-позиции
дизармит cleanup `C` (как у single-var формы). Голый `return A` или
`return B` из глубины `body` дизармит cleanup соответствующего
идента точно так же, как если бы вложенность была написана вручную —
guard/дренаж работает по имени, не по глубине вложенности. Прочие
иденты на этом пути (не упомянутые в tail/`return`) cleanup'ятся как
обычно.

**Только re-consume-список — без инициализаторов.** Форма существует
ТОЛЬКО для re-consume уже существующих owned-биндингов; смешение со
связывающей (binding) формой запрещено:

```nova
consume A, B = expr { … }   // ПАРС-ОШИБКА: список идентов не поддерживает `=`
```

Для инициализации нового owned-ресурса ВНУТРИ multi-var блока — обычный
`consume X = expr { … }` вложенным statement'ом внутри `body` (или
раздельные `consume`-блоки).

#### Взаимодействие с `spawn` / `parallel for` (173.1 by-value капчур)

Захват owned linear-значения (consume-типа) в тело
`spawn`/`parallel for`/`detach` — ошибка
**`E_LINEAR_CAPTURE_IN_FIBER`**: by-value копия обёртки в N файберов =
N алиасов одного ресурса БЕЗ учёта владения → double-close/интерференция.
Канон многофиберного доступа — `@share()`-копия per-fiber
(`ro w = tx.share(); spawn { …w… }`) либо явный move
`spawn consume c { … }` (D415 §4). `consume X { … }`-блок ЦЕЛИКОМ внутри
тела итерации (свой ресурс на итерацию) — легален, без особенностей.

#### Desugaring / реализация

Тот же D188-механизм, что и у binding-формы, с init = сам биндинг `X`
(эквивалент `consume X = X { body }` с уже готовым ресурсом). R1
(partial-construction) тривиален — init не может throw. Tail/return-вынос
= сброс arm-флага cleanup'а НА ЭТОМ пути непосредственно перед выносом
(все прочие пути видят флаг взведённым). Compile-time ловля move-out —
расширение D131 flow-анализа; формы, которые D131 не распознаёт —
bootstrap-граница: ловля расширится вместе с D131 (double-close на
TcpStream дополнительно смягчён rc-close механикой `@share()`).

Cross-ref: `spawn consume c { body }` (D415 §4 already-bound form) — тот
же re-consume-синтаксис, но cleanup привязан к выходу ДОЧЕРНЕГО фибра,
а move-out-запрет D415 не вводил (осознанное расхождение: тело spawn —
владеющий scope дочернего фибра).

#### Rejected — `give X` / `release X` (дизарм-примитив)

Рассматривался вариант «отдать `X` наружу произвольным путём, отключив
cleanup блока явным примитивом» (give / release / disarm; обсуждены Rust
`ScopeGuard::into_inner`, Swift `consume`, Zig `errdefer` — последний у
нас ретрактирован D189). **ОТВЕРГНУТО** (решение владельца 2026-07-13):
свободный дизарм ослабляет главную гарантию формы — безусловный
exactly-once cleanup — и возвращает класс багов «забыл закрыть на одном
из путей». Вынос владения разрешён ровно в СИНТАКСИЧЕСКИ видимых формах
(tail `X` / `return X` — п.3 выше; плюс амендмент v3 ниже — ровно одно
вхождение `X` аргументом consume-параметра в tail/return EXPR), где
приёмник обязательства очевиден checker'у; всё прочее — `@share()`-копия
со своим обязательством.

### Амендмент v3 — «однократный вынос через выражение» (2026-07-13, Plan 201)

> **Мотивация (владелец):** канонический TLS-паттерн (§«Канонический
> TLS-паттерн» выше) вынуждал танец `mut session_out` + `ro s = consume
> stream { … }` + `Ok(TlsStream.wrap(s, session_out))` СНАРУЖИ блока —
> потому что v2 разрешал выносить наружу только ГОЛЫЙ `X`, а `wrap(...)`
> нужно вызвать С ним. Амендмент узко расширяет вынос: `X` можно передать
> ПРЯМО В consume-параметр вызова, стоящего в tail/return-позиции — без
> танца с промежуточным mut-локалом.

**Грамматика НЕ меняется** (см. §Syntax выше) — меняется только правило
выноса владения (п.3 §«Правила формы»).

1. **Ровно одно вхождение.** В tail-выражении блока (при result-приёмнике)
   или в `return EXPR` guarded-биндинг `X` должен встречаться **РОВНО
   ОДИН РАЗ** во всём EXPR (на любой глубине вложенности вызовов —
   `Ok(TlsStream.wrap(stream, session))` содержит `stream` один раз, хотя
   и внутри двух вложенных вызовов). Два и более вхождения (в т.ч. если
   ОБА — в consume-позиции разных вызовов) — нарушение.
2. **Вхождение — в consume-позиции для ДИЗАРМА; не-consume-позиция —
   обычное использование (исправлено 2026-07-13, тот же день, до первого
   релиза амендмента — regression в `tcp_share_test.nv`).** Единственное
   вхождение `X` дизармит cleanup, ТОЛЬКО если оно — АРГУМЕНТ параметра,
   помеченного `consume`, у вызова (свободная fn, метод `obj.method(...)`,
   статический `Type.method(...)` / `module.fn(...)`), **либо — амендмент
   того же дня (2026-07-13, fix `[M-178-consume-field-ctor-from-var]`, см.
   [D133](02-types.md#d133) §«Что считается consume») — инициализатор
   `consume`-поля record-литерала** (`consume s { MyRec{res: s, tag: 1} }`,
   `return Ok(TlsStream { tcp: stream, session })`; форма литерала любая —
   типизированный / record-вариант суммы / анонимный с типом из контекста /
   D52-punning): конструирование литерала забирает владение так же, как
   consuming-вызов. Не-consume-поле литерала — НЕ consume-позиция (обычное
   использование, см. ниже). Частный случай —
   EXPR = голый `X` (п.3 §«Правила формы» — как в v2, БЕЗ изменений).
   Вхождение(я) `X` **исключительно** в НЕ-consume-позиции — RECEIVER
   (`X.method()`, включая `X.share()`) или view-/mut-АРГУМЕНТ
   (`mo3_peek(X)`) — это **обычное использование**, ровно как то же самое
   использование внутри тела блока СТЕЙТМЕНТОМ (п.4 §«Правила формы»):
   **НЕ ошибка**, cleanup срабатывает штатно на выходе из блока, дизарма
   НЕТ (значение блока — обычно копия/производное, НЕ сам `X`). Пример:
   `consume s { s.share() }` — `share()` берёт `s` RECEIVER'ом (ro-view),
   создаёт независимую shared-копию; копия уезжает значением блока,
   cleanup блока штатно закрывает долю оригинала `s`. Смешение — вынос
   (consume-позиция) ПЛЮС ещё вхождение того же `X` в том же EXPR
   (consume-позиция или нет) — остаётся нарушением (см. п.5).
3. **Замыкание, захватывающее `X`, внутри EXPR — ошибка.** `return
   Ok(spawn { … stream … })` или любой `|…| … X …` / `fn(…) … X …`
   ВНУТРИ tail/return EXPR, чьё тело ссылается на `X` (в любой форме,
   не только consume-позиции) — `E_CONSUME_BLOCK_MOVE_OUT`: время жизни
   замыкания не синтаксически видно checker'у (может пережить сам вызов),
   несовместимо с «приёмник обязательства очевиден checker'у» (см.
   §Rejected выше).
4. **Точка дизарма — момент передачи владения** (вход в consuming-вызов,
   т.е. после того, как ВСЕ аргументы ЭТОГО вызова вычислены — Nova's
   порядок вычисления аргументов слева направо). Всё, что в EXPR
   вычисляется/падает/throw'ит **до** этого момента — cleanup блока ещё
   взведён и сработает как обычно (R2). Пример: `return Ok(wrap(stream,
   risky()))` — если `risky()` вычисляется до входа в `wrap` (по AST-
   порядку аргументов `wrap`) и throw'ит — `stream` ещё НЕ передан
   `wrap`, cleanup закрывает его; успешный `risky()` → вход в `wrap` →
   дизарм → `wrap` теперь владеет `stream`. Для record-литеральной
   consume-позиции (п.2, M-178-амендмент) точка дизарма — конструирование
   литерала: само конструирование после вычисления полей упасть не может,
   поэтому окно риска схлопывается в field-выражения — throw в другом
   поле ДО конструирования оставляет cleanup взведённым.
5. **Нарушения — тот же `E_CONSUME_BLOCK_MOVE_OUT`, уточнённый текст.**
   Оставшиеся два failure-режима (смешение — consume-вхождение ПЛЮС ещё
   вхождение того же `X` в том же EXPR; захватывающее замыкание) остаются
   ОДНИМ кодом ошибки `E_CONSUME_BLOCK_MOVE_OUT` — текст диагностики
   уточняет причину. Единственное вхождение НЕ в consume-позиции —
   БОЛЬШЕ НЕ failure-режим (см. п.2, исправлено 2026-07-13) — обычное
   использование.
   Легальный `pump_handshake(stream, session)` КАК STATEMENT внутри тела
   (НЕ в tail/return-позиции) — НЕ затронут: это ro-view-borrow
   (view-параметр `stream` у `pump_handshake`), разрешённый п.4 §«Правила
   формы» независимо от этого амендмента; амендмент v3 сужен ИСКЛЮЧИТЕЛЬНО
   до tail/return EXPR.
6. **Композиция с multi-var-формой.** `consume A, B { return f(A, B) }`
   десугарится (см. §«Multi-var re-consume форма» выше) в `consume A {
   consume B { return f(A, B) } }` — правило применяется **по-переменно
   на каждом уровне вложенного desugar'а независимо**: у `return f(A, B)`
   ДВА активных guard'а (`A` — внешний, `B` — внутренний) одновременно;
   каждый решается САМ ПО СЕБЕ (ровно одно вхождение своего имени, в
   consume-позиции) — оба могут дизармиться этим ОДНИМ `return`, если оба
   — прямые consume-аргументы `f`. Дизарм — индивидуальный (свой
   `_defer_<bid>_0_active = 0` на каждый уровень вложения); ошибка одного
   guard'а (напр. `B` встречается дважды) не блокирует дизарм другого
   (`A`), если тот сам по себе санкционирован.

### Правила (R1-R6)

#### R1 — Partial construction safety

Если `init()` throws **до** scope-entry — `cleanup` не вызывается. Codegen
эмитит установку `_outcome`/shield **только после** успешного завершения
`init()`. Пример: если `db.begin()` throws, никакого `tx.cleanup(...)` не
будет (некому).

Это согласовано с [D195](04-effects.md#d195) §boot-order для `Application`
handler'а.

#### R2 — Exactly-once

`cleanup` для данной consume-binding **гарантированно вызывается ровно один
раз** на любом exit-path (включая `return`, `throw`, `panic`, cancel).
Реализуется runtime-счётчиком `_consume_count` *(Plan 173 Ф.5 #8, реализовано:
скрытое поле `int _consume_ccount` на heap-record инстансе + scope-локальный
дубль-гард в desugared-коде)*: пролог generated `Nova_<T>_consume_cleanup` —
единственный chokepoint всех путей вызова — проверяет и инкрементирует счётчик;
при ≥ 2 — runtime panic `D188-on-exit-double-invocation`.

Double-invocation invariant нарушается только если programmer вручную
зовёт `tx.cleanup(...)` — прямые/алиасные вызовы из body ловятся compile-time
(`D188-r2-manual-on-exit`, включая `let y = tx; y.cleanup(...)` и
receiver-подвыражения); вызов, пронесённый через границу функции
(view-параметр) или FFI/reflection, ловится runtime-счётчиком (linear types
prevent double-consume в большинстве случаев, но обход возможен). Extern
"nova" cleanup'ы (MutexGuard и пр., D194 hot-path) счётчиком не оснащаются —
их структуры рукописные в nova_rt.

#### R3 — Cancel-shield by default (completes-by-default — §3a Plan 173, D192-ретракт)

Внутри cleanup-path (`tx.cleanup(...)`) cancel-доставка автоматически
маскируется **до завершения cleanup** — cleanup ВСЕГДА добегает
(completes-by-default, пересмотр владельца 2026-06-26 §3a Plan 173;
[D192](#d192) РЕТРАКТИРОВАН — force-прерывания cleanup'а не существует).
Это **default behavior**; opt-out не предоставляется (Rust scopeguard /
C++23 lessons показывают что opt-in cancel-shield большинство забывает).

Cancel остаётся pending в `fiber->cancel_pending`; доставляется после
`nv_leave_cancel_shield()`. Если cleanup превысил watchdog-порог
(3-level resolution D192-ретракт: WithExitTimeout → Application → default
5000ms) — на ближайшей suspend-точке печатается one-shot stderr-ВАРН
«fiber stuck in cleanup», и cleanup продолжает бежать; превышение
структурно наблюдаемо в `ResourceTrace.on_resource_exit`
(`duration_ms`/`overrun` — D185 amend). `CleanupTimeoutError` УДАЛЁН.

**R3b amend (2026-06-08, V2 refinement) [M-83.11-cancel-token-bound-race-2k]
proper fix:** Plan 83.11 §11.6 ctx_pins[] **ARRAY** (not the token itself)
must be allocated uncollectable. Под high fiber-count (≥2k spawns в
supervised(cancel:)) ctx_pins[] array growth (16→32→64→...→1024+) triggers
many GC cycles. Pre-fix nova_alloc'd array lost root coverage под heavy
allocation pressure — Boehm conservative scanner could miss the pointer-
chain `stack-scope → ctx_pins → tokens` during Mark phase, reclaiming
tokens. Result: token memory reused for SpawnCtx, struct overlap at
offset 8 (_nova_worker_slot vs bound_scope) → panic «token already bound
to a live scope» on bind.

**Fix shape:**
- `nova_scope_pin_ctx` allocates `ctx_pins[]` via `nova_alloc_uncollectable`
  (fibers.h:639).
- On array growth, OLD array `nova_free_uncollectable`d (tokens already
  copied to NEW array — no leak).
- NovaCancelToken stays `nova_alloc` (collectable) — array always alive
  keeps tokens reachable through standard Boehm pointer-chain scan.

**Pre-V2 workaround (2026-06-05, now superseded):** initially `nova_cancel_token_new`
itself switched к `nova_alloc_uncollectable` — что fixed crash но создавало
per-token leak. V2 V refinement moves uncollectable-ness к ctx_pins array
which is per-scope (~16-1024 entries × 8 bytes), and token stays collectable.

**V3 refinement (2026-06-08) [M-83.11-ctx-pins-scope-cleanup] fix:**
Cleanup hook added в `nova_supervised_run_impl` (~line 1907) после
`nova_cancel_token_unbind`, перед всеми re-throw paths. Frees ctx_pins[]
array via `nova_free_uncollectable` + resets ctx_pins/count/cap fields.
Runs on all exit paths (normal, error, interrupt, CANCEL). Token остаётся
reachable через caller's stack ref (Boehm scans stack roots) даже после
free — array нужен был только для cross-worker pointer-chain reachability
which ends at scope exit. SpawnCtx entries уже cleaned up через their own
nova_spawn_pool_release lifecycle.

**V3 trade-off:** zero per-scope leak (was 128B-8KB tail в V2). Cleanup
overhead = single nova_free_uncollectable + 3 field resets — negligible.

**R3c amend (2026-06-05) [M-83.11-nested-supervised-cascade-drain-hang]
fix:** `nova_cancel_token_bind` deferred-cancel propagation (вызывается
когда `cancel_requested=true` уже выставлен до bind) MUST run полную
cancel-hook chain — не только `nova_sched_cancel_all_pending(q)`. Pre-fix
bug: cascade-cancel race (outer fiber вызывает `outer_tok.cancel()` BEFORE
main reaches outer's bind, поскольку cascade triggered by inner supervised
которое blocks main thread first) — cancel cascades через `linked[]`
к inner_tok который УЖЕ bound, но outer's deferred propagation на bind
only wakes pending-slot fibers, missing armed M:N worker-parked fibers +
ASYNC slots + driver-armed timers. Outer scope worker fibers stay parked
→ supervised_run hangs до watchdog. Fix: bind деferred-cancel calls full
chain — `nova_sched_cancel_all_pending` + `nova_scope_cancel_wake_all` +
`nova_runtime_cancel_worker_fibers` + `_nova_cancel_via_driver` (same
sequence as `nova_cancel_token_cancel_reason`).
composition.

**R3d amend (2026-07-23) [M-cancel-loop-accept-swallowed-residual] — shield
NARROWED to the cleanup phase only (221.1 №15, ОКНО-3, владелец-решение (б)):**
the codegen IMPLEMENTATION (`emit_c.rs`/`fibers.h`'s `nv_consume_enter_shield`/
`nv_consume_leave_shield`) had drifted from R3's own letter above (which
already scoped the mask to «cleanup-path», not the whole scope) — it armed
the cancel-mask at RESOURCE-CAPTURE time (consume-scope/auto-cleanup entry)
and held it until scope exit, i.e. over the ENTIRE body between capture and
cleanup dispatch, not just the cleanup call itself. Any suspend/yield point
inside the body of a live `consume`-bound resource (including a pure
CPU-bound busy/retry loop that never touches the resource — e.g. an
`accept()` retry loop under `supervised(timeout:)` holding an unrelated
`consume`d listener) therefore never observed cancellation until the
resource's cleanup eventually ran — an indefinite hang for any loop whose
own exit condition depends on that same cancellation. Fix: `nv_consume_
enter_shield` is called ONLY immediately before the `Nova_<T>_consume_
cleanup` dispatch itself (inside the single dispatcher shared by all four
run-sites — FAIL/LEAVE/EARLY/INTERRUPT); `nv_consume_leave_shield` keeps
running right after, unchanged. The body's own suspend/yield points are now
cancellable normally; only the cleanup call proper is shielded — this IS
what R3's own prose already says («внутри cleanup-path... маскируется»), the
codegen simply hadn't matched it. Regression:
`spec_tests/conformance/standalone/m2217_15b_cancel_consume_shield_narrowed.nv`
(RED on the pre-fix binary — timed out; GREEN after) + the full
`d432_auto_cleanup_hybrid_c.nv` suite (9 cases: exactly-once, return-disarm,
explicit-close disarm, MaybeConsumed branches, LIFO order, loop-body,
throw-path, spawn-closure, break/continue boundary) re-verified unaffected.

**R3a amend (2026-06-05) [M-110.x-cleanup-shield-deadline-underflow]
codegen-layout invariant fix (supervised(cancel:) path):**

Cancel-shield state — `_nova_cancel_mask_count` + `_nova_cancel_deadline_ns`
fields — lives в `NovaSpawnCtxBase` (runtime fibers.h). Codegen-emitted
per-fiber `NovaSpawnCtx_<id>` (spawn) и `NovaDetachCtx_<id>` (detach)
структуры **MUST** включать ВСЕ base fields в правильном порядке (включая
позже добавленные `_nova_pool_size` (Plan 83.6), `_nova_cancel_mask_count`
(Plan 110.2.1.a), `_nova_cancel_deadline_ns` (Plan 110.2.2.a)).

**Why:** runtime cast'ает `mco_get_user_data(co) → NovaSpawnCtxBase*` и
читает поля по фиксированному offset. Если codegen struct emit'ит ТОЛЬКО
первые N полей, allocation размер = `sizeof(codegen_ctx) < sizeof(NovaSpawnCtxBase)`.
Runtime читает за пределами allocation → Boehm GC adjacent memory bytes
(garbage) → mask=garbage > 0 → `nv_shield_check_deadline` enters slow path
→ deadline=garbage → bogus watchdog-варн (исторически: bogus
`CleanupTimeoutError` с inflated over_ms — `720M+ ms over budget` в
6-second tests, i64 underflow symptom; throw-механизм ретрактирован
D192-ретрактом, Plan 173 Ф.5 п.2).

**Pre-fix bug history:** codegen comment на `emit_c.rs:6253` явно
утверждал «`_nova_fiber_state` MUST be last in NovaSpawnCtxBase prefix» —
stale assertion. Runtime добавил `_nova_pool_size` (Plan 83.6) +
`_nova_cancel_mask_count` + `_nova_cancel_deadline_ns` (Plan 110.2),
codegen не обновился. Layout mismatch latent до supervised(cancel:) +
sleep + cancel pattern — там fiber yields в check_deadline под обычным
обстоятельством, реально читая garbage.

**Acceptance invariant:** sizeof(codegen NovaSpawnCtx_<id>) >=
sizeof(NovaSpawnCtxBase). Test enforcement (TODO Plan 110 V2):
runtime-side static_assert + codegen sanity check.

**Cross-references:** [D196](#d196) R4 (consume{} prev_deadline
save/restore — different bug); [D196](#d196) R4b (cleanup exception
safety — different bug); [Plan 110 plan-doc](../../docs/plans/110-scoped-resources-radical-simplification.md).

#### R4 — Watchdog-threshold resolution at scope-entry (D192-ретракт: порог варна, не прерывания)

Watchdog-порог (ex-`exit_timeout`) определяется **один раз** при
scope-entry через 3-level fallback (см. [D192](#d192) — RETRACTED как
force-механизм; resolution сохранена как источник ПОРОГА варна):

1. `WithExitTimeout` impl ресурса (если есть);
2. Активный `Application` effect handler (см. [D195](04-effects.md#d195));
3. Hardcoded fallback `Duration.seconds(5)`.

Кэшируется в локалке `_timeout`; армится `nv_cleanup_watchdog_arm` ТОЛЬКО
вокруг cleanup-вызова («fiber застрял в cleanup» — body порогом не
ограничивается). Сохранение в локалке предотвращает race с асинхронным
изменением handler'а во время body execution. Превышение порога =
one-shot stderr-варн + `overrun=true` в ResourceTrace exit-событии
(D185 amend); прерывания НЕТ.

#### R5 — LIFO composition

Вложенные `consume {}` блоки выходят в LIFO порядке (наружный позже
внутреннего). Если outer throws, inner.cleanup уже завершён. Если inner
throws, outer.cleanup получает `Failure(inner_err)` в outcome.

Mixed `consume {}` + `defer { }` LIFO — единый scope-stack per-fiber:

```nova
consume a = A.new() {
    defer { cleanup_b() }
    consume c = C.new() {
        defer { cleanup_d() }
        body
    }
    // exit: cleanup_d → c.cleanup → cleanup_b
}
// exit: a.cleanup
```

#### R6 — Memory ordering

Acquire-release semantics между body и `cleanup`:
- Все writes в `body` happen-before `cleanup` reads (release on exit,
  acquire on entry).
- Согласовано с [D167](06-concurrency.md#d167) memory ordering.
- Reason: cleanup может flush/commit видимое состояние; должен видеть
  финальную семантику ресурса.

### Typed error dispatch в `cleanup`

Resource решает что делать с body error через D85 auto-narrowing:

```nova
fn Transaction consume @cleanup(outcome ScopeOutcome) Fail[DbError] -> () {
    match outcome {
        Success => @commit()!!
        Failure(err) => {
            if err is DbError.Deadlock {
                @retry_friendly_rollback()!!    // err narrow'нут до DbError.Deadlock
            } else if err is DbError {
                @rollback_with_log(err.msg)!!
            } else {
                @rollback()!!                    // generic non-DB failure
            }
        }
        Panic(_) => @rollback_emergency()
    }
}
```

Никакого `outcome.failure_as[T]()` helper'а не предоставляется — `is`-
narrowing достаточен и идиоматичен (rejected alternative в [D190](#d190)).

### Generic constraint

```nova
fn use_any[T Cleanup[E]](r T) Fail[E] -> () {
    consume binding = r {
        // binding : T
    }
}
```

Generic bound `[T Cleanup[E]]` следует синтаксису [D72](#d72). E может
быть concrete (`Cleanup[IoError]`) или generic param (`[T Cleanup[E]]`
с обоими свободными). **Never special case**: если `E = Never` в bound,
type-checker автоматически снимает требование `Fail[E]` у caller'а
(см. [D194](#d194)).

### Что заменяется

| Старая форма | Новая форма |
|---|---|
| `consume tx = begin(); errdefer { rollback }; okdefer { commit }` | `consume tx = begin() { body }` (Transaction impl Cleanup) |
| `defer \|result\| match { ... }` | `consume X = ... { }` или `with ResourceTrace = h { ... }` (D185) |
| `consume X = ...; defer { X.close() }` | `consume X = ... { body }` (если X impl Cleanup) |

См. [D189](#d189) для прямого удаления.

### Сравнение

| Capability | Java | Kotlin | Swift | C++23 | Rust | Go | TS | Python | **Nova D188** |
|---|---|---|---|---|---|---|---|---|---|
| Cancel-shield by default | ❌ | ⚠️ opt-in | ⚠️ opt-in | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |
| Exactly-once invariant | ⚠️ | ⚠️ | ✅ | ⚠️ | ⚠️ | ❌ | ❌ | ⚠️ | ✅ runtime |
| Partial-construction spec'd | ⚠️ stacked-using bug | ⚠️ | ⚠️ | ⚠️ | ⚠️ | n/a | ⚠️ | ⚠️ | ✅ R1 |
| Single keyword | ✅ try-w-r | ✅ .use | ❌ | ❌ | ❌ | ❌ | ⚠️ using | ✅ with | ✅ `consume{}` |

### Связь

- [D72](#d72) — generic bounds syntax.
- [D85](04-effects.md#d85) — `is` auto-narrowing.
- [D90](#d90) §7 — amend: cancel as `Failure(CancelError)`.
- [D158](#d158) — amend: ErrorKind discrimination retracted.
- [D160](#d160) — **retract** в пользу D188.
- [D161](#d161) — amend: MultiError без ErrorKind.
- [D162](#d162) — amend: scope-block exhaustive by construction.
- [D167](06-concurrency.md#d167) — memory ordering basis.
- [D180](05-memory.md#d180) — raw `consume X = ...` (без block).
- [D185](04-effects.md#d185) — Cleanup effect (telemetry).
- [D189](#d189) — okdefer/errdefer/defer\|r\| removal.
- [D190](#d190) — rejected alternatives.
- [D191](#d191) — async cleanup.
- [D192](#d192) — exit-timeout taxonomy + 3-level resolution.
- [D193](#d193) — MultiError + cycle-safety.
- [D194](#d194) — `Cleanup[Never]`.
- [D195](04-effects.md#d195) — Application nesting.
- [D196](#d196) — init type constraints.
- [D197](#d197) — cleanup re-entrance.
- [D198](#d198) — realtime + cleanup interaction.
- [Plan 110](../../docs/plans/110-scoped-resources-radical-simplification.md).

---

## D189. Прямое удаление `okdefer` + `errdefer` + `defer |result|`

> **Plan 110 Ф.9.** Принято 2026-05-31. **Статус: ACTIVE** (Plan 110.5.7
> hard cutover landed; parser rejects retracted forms с D189-removed-*
> errors). Fixture migration via deletion + behavior coverage preserved
> в plan110/ (Plan 110.5.5).

### Что удаляется

| Construct | Origin | Replacement |
|---|---|---|
| `errdefer { ... }` | D90 §2 | Move logic в `Cleanup.cleanup` через `match outcome { Failure(_) => ... }` или `defer + flag` для escape hatch |
| `okdefer { ... }` | D160 | `match outcome { Success => ... }` |
| `defer \|result\| { ... }` | D160 | `match outcome { ... }` в `cleanup` |
| `DeferResult[T, E]` type | D160 | Заменён на `ScopeOutcome` (D188) |
| `DeferWithResult` AST node | D160 | Удалён |

### Parse errors (после Ф.5 удаления)

После Ф.5 парсер не принимает старые формы — каждая выдаёт parse error с
suggestion на новую форму:

| Token | Error code | Hint |
|---|---|---|
| `okdefer` | `D189-removed-okdefer` | Use `consume X = ... { ... }` with `match outcome { Success => ... }` |
| `errdefer` | `D189-removed-errdefer` | Use `consume X = ... { ... }` with `match outcome { Failure(_) => ... }` |
| `defer \|...\| { ... }` | `D189-removed-defer-result` | Use `consume X = ... { ... }` |

### Auto-fix mappings

`nova fix --simplify-cleanup` применяется один раз перед удалением парсер-
поддержки (Plan 110 Ф.9.2):

1. **Pattern: linear `consume + errdefer + okdefer`**

   ```nova
   // before:
   consume tx = db.begin()
   errdefer { tx.rollback() }
   okdefer  { tx.commit()?  }
   do_work()?

   // after:
   consume tx = db.begin() {
       do_work()?
   }
   // (предполагается Transaction impl Cleanup: cleanup Success → commit, Failure → rollback)
   ```

2. **Pattern: `errdefer` без ресурса (cleanup state)**

   ```nova
   // before:
   errdefer { log.warn("operation failed") }
   risky()?

   // after:
   mut done = false
   defer { if !done { log.warn("operation failed") } }
   risky()?
   done = true
   ```

3. **Pattern: `defer |result|`**

   ```nova
   // before:
   defer |result| {
       match result {
           Ok(_)    => Log.info("ok")
           Err(e)   => Log.error("fail: ${e}")
           Panic(m) => Log.fatal("panic: ${m}")
       }
   }

   // after (using Cleanup effect — D185):
   with ResourceTrace = LogHandler.new(label: "operation") {
       body
   }
   ```

### Rationale (почему hard cutover без migration window)

1. Nova ещё pre-0.1; breaking change acceptable per project-philosophy.
2. Кода на `.nv` мало (стдлиб + tests + examples; ~десятки fixture'ов).
3. Auto-fix tool покрывает 100% паттернов (3 правила выше).
4. Dual-syntax fallback запутывал бы users во время transition (Rust
   scope-bracket lessons).
5. Spec maintenance cost dual-форм > value migration window.

### Связь

- [D90](#d90) §«errdefer» — retract.
- [D160](#d160) — retract целиком.
- [D158](#d158) — amend.
- [D161](#d161) — amend.
- [D162](#d162) — amend.
- [D188](#d188) — replacement.
- [Plan 110 Ф.9](../../docs/plans/110-scoped-resources-radical-simplification.md#ф9-migration).

---

## D190. Rejected alternative cleanup designs

> **Plan 110.** Принято 2026-05-31. **Статус: ACTIVE** (pure documentation
> of rejected design choices; no impl required). Документирует rejected
> design choices для будущих ревизоров с rationale почему именно
> `Cleanup[E]` + `consume {}`.

### Drop-trait (Rust-style)

```rust
impl Drop for File { fn drop(&mut self) { self.close(); } }
```

**Отвергнуто** потому что:
- Implicit cleanup невидим в call-site — code review загромождён.
- Async-Drop unresolved (Rust open RFC с 2019); Nova first-class async.
- `drop()` нельзя throw — Rust решает через `abort`, Nova хочет typed
  propagation.
- Order-of-drop magic (struct field order); programmer ошибки скрыты.

### Priority-defer (`defer priority=10 { ... }`)

**Отвергнуто**: LIFO достаточен для всех known patterns. Priority вводит
новый axis сложности без killer use-case (ни один индустриальный язык
не имеет).

### `module_finalizer { ... }` keyword

**Отвергнуто**: добавление primitive для редкого паттерна. Достижимо через
`Cleanup[Application]` idiom (см. [D195](04-effects.md#d195)).

### Two-method protocol (`on_success` / `on_failure`)

```nova
type Cleanup[E] protocol {
    on_success() Fail[E] -> ()
    on_failure(err any) Fail[E] -> ()
}
```

**Отвергнуто**:
- Resource'ы которые делают одинаковое cleanup в обоих случаях (Mutex,
  File) — дублирование кода.
- Panic-handling требует третий метод → 3 method protocol → readability
  страдает.
- Single `cleanup(outcome)` с match — лучше структурирован, легче
  generic'и пишутся.

### Generic `ScopeOutcome[E]`

```nova
type ScopeOutcome[E]
    | Success
    | Failure(E)
    | Panic(str)
```

**Отвергнуто**: resource не знает body error type (transactionResource не
знает что body может throw `OrderError`). Type-erased `Failure(any)` —
canonical Python `__exit__` pattern. Routing через `if err is T` (D85).

### Отдельный `Cancelled` variant

```nova
type ScopeOutcome enum Success | Failure(any) | Cancelled(any) | Panic(str)
```

**Отвергнуто**: ни один из benchmark-языков (Java/Kotlin/Swift/C++/Rust)
не выделяет cancel отдельно. Cancel — special case throw'а; semantics
identical для resource cleanup. См. [D90 §7](#d90) amend.

### `using` / `scoped` keyword

```nova
using tx = db.begin() { ... }
scoped tx = db.begin() { ... }
```

**Отвергнуто**: re-use existing `consume` keyword снижает keyword count
на 1. `consume` уже описывает "linear, owned, single-use" semantics —
scope-block — естественное расширение.

### `outcome.failure_as[T]() -> Option[T]` helper

```nova
match outcome {
    Failure(_) => {
        if dbErr = outcome.failure_as[DbError]() {
            ...
        }
    }
}
```

**Отвергнуто**: D85 `is` auto-narrowing уже даёт smart-cast; helper —
дублирование. Kotlin smart-cast precedent.

### Связь

- [D188](#d188) — accepted design.
- [Plan 110 §«Rejected»](../../docs/plans/110-scoped-resources-radical-simplification.md).

---

## D191. Async cleanup — `suspend` в `cleanup` body

> **Plan 110 Ф.3.** Принято 2026-05-31. **Статус: ACTIVE** (Plan 110.2.1.a
> +110.2.2.a landed 2026-06-01). Расширяет [D159](#d159) async
> cleanup на `Cleanup.cleanup`.

### Что разрешено

`cleanup` body может содержать `suspend`-операции:
- `Time.sleep(d)` — для retry с backoff.
- `Net.*` — для grace-close TCP socket.
- `Db.*` — для commit/rollback с round-trip.
- `await fut` — для произвольного `Future[T]`.

### Что запрещено

- `spawn { ... }` — fire-and-forget fiber внутри cleanup нарушает exactly-
  once и cancel propagation (D159 правило).
- `parallel { ... }` — concurrent cleanup-tasks непредсказуемы.
- `supervised { ... }` — supervisor-frame inside cleanup-frame double-nested
  cancel routing (Plan 83.10 lessons).

Эти запреты эмитятся checker'ом как `E_CLEANUP_FORBIDDEN_OPERATION` с
suggestion переписать как sequential `await`.

### Cancel-shield пробрасывается через suspend (D192-ретракт, Plan 173 Ф.5 п.2)

Внутри `cleanup` cancel доставка отложена **до завершения cleanup**
(completes-by-default, R3 [D188](#d188)). На каждом suspend-point runtime
проверяет watchdog-порог:

```nova
fn TcpStream consume @cleanup(outcome ScopeOutcome) Fail[IoError] -> () {
    @send_eof()?                    // suspend ok; cancel masked
    @wait_for_ack(timeout: 1.s())?  // suspend ok; watchdog check
    @close()?                       // suspend ok; cancel masked
    // если cumulative time > порога → one-shot stderr-ВАРН здесь (не throw)
}
```

### Threshold exceedance (ex-«Timeout exceedance»)

Если cumulative cleanup-suspend-time превысил `_timeout`-порог (computed at
scope-entry per [D192](#d192) 3-level resolution — RETRACTED как
force-механизм, сохранён как источник порога):

1. Ближайшая suspend-точка печатает one-shot stderr-варн
   «fiber stuck in cleanup» — БЕЗ прерывания; cleanup добегает.
2. Превышение структурно наблюдаемо: `ResourceTrace.on_resource_exit`
   несёт `duration_ms` + `overrun=true` (D185 amend).
3. Cancel доставка снимается (shield off) по завершении cleanup, cancel
   re-raises после exit.

### Realtime context

В `#realtime` fn (D172) — `_timeout = Duration.zero` (D198). Любой suspend
в `cleanup` запрещён checker'ом через D172 правила (parking ops not
allowed in `#realtime`). Это compile error, не runtime.

### Связь

- [D90](#d90) §7 — cancel/throw routing.
- [D158](#d158) — failable cleanup base.
- [D159](#d159) — async cleanup base; этот D191 — amend для Cleanup.
- [D172](04-effects.md#d172) — `#realtime` parking ban.
- [D188](#d188) §R3 — cancel-shield.
- [D192](#d192) — 3-level timeout resolution.
- [Plan 100.4.2](../../docs/plans/100.4.2-async-suspend-cleanup.md).

---

## D192. `exit_timeout` taxonomy + 3-level resolution

> **Plan 110 Ф.3.** Принято 2026-05-31. **Статус: ⛔ RETRACTED как force-механизм
> (пересмотр владельца 2026-06-26 §3a Plan 173; применён Plan 173 Ф.5 п.2, 2026-07-10).**
> **Принцип: «defer всегда добегает»** — НИКАКОГО прерывания cleanup'а по таймауту
> не существует (ни один из 7 языков-планки — Go/Rust/TS/Kotlin/Java/Zig/Swift —
> не force-прерывает cleanup; прерывание вернуло бы partial-cleanup-порчу).
> Что РЕТРАКТИРОВАНО: тип `CleanupTimeoutError` (удалён из prelude), typed-throw
> механизм (`_nova_throw_cleanup_timeout_fn` + string-fallback), «инжект ошибки в
> suspend cleanup'а» (D188 R3-старая редакция).
> Что СОХРАНЕНО: 3-level resolution ниже — как источник **ПОРОГА watchdog-варна**:
> превышение порога на suspend-точке внутри cleanup = one-shot stderr-варн
> «fiber stuck in cleanup» (cleanup продолжает бежать) + `duration_ms`/`overrun`
> в `ResourceTrace.on_resource_exit` (D185 amend). Bounded-shutdown задаётся
> дедлайном НА SCOPE — `supervised(deadline:/timeout:)` (D408), не на cleanup.
> Текст ниже — исторический (читать «deadline» как «порог варна»).

### Taxonomy Duration значений

| Value | Семантика | Diagnostic |
|---|---|---|
| `Duration.zero` | Sync-only cleanup; любой suspend → runtime error | `D192-zero-timeout-suspend` (runtime) |
| `Duration.positive(d)` | Normal timeout; deadline = now + d | — |
| `Duration.MAX` | Без timeout; cleanup ждёт неограниченно | `D192-infinite-timeout-warn` (compile warn) |
| `Duration.negative` | Invalid; runtime panic при resolve | `D192-negative-timeout` (runtime panic) |

`Duration.zero` использует `#realtime` (D198). Realtime-context
автоматически устанавливает zero без 3-level resolution.

### 3-level resolution (от ближайшего к дальнему)

При входе в `consume X = ... { body }` runtime resolves timeout один раз:

#### Level 1 — `WithExitTimeout` impl

```nova
type WithExitTimeout protocol {
    exit_timeout() -> Duration
}

fn Transaction @exit_timeout() -> Duration => 30.s()
```

- Structural match — ресурс не обязан явно объявлять impl, достаточно
  иметь метод правильной signature.
- Если method присутствует — runtime зовёт его при scope-entry, result
  кэшируется в локалке.
- НЕ часть Cleanup protocol (опционально); Mutex/Sem/Lock не нужны.

#### Level 2 — `Application` effect handler

```nova
with Application = Application.handler(default_exit_timeout: 10.s()) {
    run_server()                            // все consume{} получают 10s
}
```

- Если активен `Application` effect handler и ресурс НЕ имеет
  `WithExitTimeout` impl — runtime вызывает `Application.default_exit_timeout()`.
- См. [D195](04-effects.md#d195).
- Nested Application handlers: inner handler побеждает (effect-stack
  semantics).

#### Level 3 — hardcoded fallback

`Duration.seconds(5)` если ни Level 1 ни Level 2 не сработали. Конечная
safety net.

### Implementation: единая runtime функция

```c
// runtime emit:
nv_duration_t nv_resolve_exit_timeout(nv_typeid_t type, void* obj) {
    // 1) check WithExitTimeout via vtable lookup
    if (nv_type_has_method(type, "exit_timeout")) {
        return nv_call_method_exit_timeout(type, obj);
    }
    // 2) check Application effect
    nv_handler_t* app = nv_effect_lookup("Application");
    if (app) {
        return nv_call_application_default_exit_timeout(app);
    }
    // 3) hardcoded
    return nv_duration_seconds(5);
}
```

Codegen вызывает `nv_resolve_exit_timeout` один раз per scope, результат
кэшируется в локалке. Преимущества vs per-callsite codegen:
- меньше binary size;
- единая точка модификации;
- проще для inlining в VM/JIT.

### Per-instance конфигурация — library pattern

```nova
fn Db.connect(url str, exit_timeout Duration = 30.s()) -> Db => ...
fn Db @begin() -> Transaction => Transaction {
    exit_timeout_value: @config.exit_timeout, ...
}
fn Transaction @exit_timeout() -> Duration => @exit_timeout_value
```

Когда `Db.connect(url, exit_timeout: 60.s())` — все транзакции через этот
Db унаследуют 60s, потому что `Transaction.exit_timeout()` структурно
satisfies `WithExitTimeout`. **Это library pattern, не language feature.**

### Что НЕ делаем

- ❌ Нет `exit_timeout()` в `Cleanup` — оптимизация для infallible
  cleanup (`MutexGuard`).
- ❌ Нет scope-level override через `with X = Y { }` — этот syntax только
  для effect-handlers.
- ❌ Нет global mutable setting через прямой setter — конфиг через
  `Application` effect handler.

### Связь

- [D188](#d188) §R4 — resolution at scope-entry.
- [D194](#d194) — `Cleanup[Never]` hot-path opt.
- [D195](04-effects.md#d195) — `Application` effect.
- [D198](#d198) — realtime bypass.
- [Plan 110 Ф.3](../../docs/plans/110-scoped-resources-radical-simplification.md#ф3-cancel-shield).

---

## D193. `MultiError` — iteration + cycle-safety + depth-limit

> **Plan 110 Ф.6.** Принято 2026-05-31. **Статус: ACTIVE** (MultiError API
> + cycle-safety + depth-limit 256 landed 2026-05-31). Refactor [D158](#d158) / [D161](#d161)
> MultiError API + добавление cycle-safety из Java JDK-8287921 lesson.

### Structure

```nova
type MultiError {
    ro primary    any
    ro suppressed []any
}
```

`primary` — первая ошибка цепочки (chronologically first failure).
`suppressed` — последующие ошибки, добавленные через `compose`.

`any` (не `Error`) — потому что MultiError может composit'ить `CancelError`,
`TimeoutError`, `DbError`, panic-string и пр. Type-erased.

### API

```nova
fn MultiError @primary() -> any => @primary
fn MultiError @suppressed() -> []any => @suppressed

fn MultiError @walk() -> Iter[any] {
    // returns: primary, then suppressed in LIFO order
}

fn MultiError @fmt_chain() -> str {
    // formatted: "primary: X\n  suppressed: Y\n  suppressed: Z"
}

fn MultiError @find_first_panic() -> Option[str] {
    // первый panic-string в chain (primary or suppressed); None если none
}
```

### Cycle-safety (Java JDK-8287921 lesson)

В Java HotSpot обнаружили deadlock когда `Throwable.addSuppressed(this)` —
self-suppression создавала self-reference cycle. Nova избегает через
identity-check в compose operation:

```c
void nv_compose_error(nv_multi_err_t* m, void* secondary) {
    // identity check: ignore self
    if (secondary == m->primary) return;
    // dedup: ignore if already in suppressed
    for (size_t i = 0; i < m->suppressed_len; i++) {
        if (m->suppressed[i] == secondary) return;
    }
    nv_multi_err_push(m, secondary);
}
```

### Depth limit

Runtime invariant: `suppressed.len <= 256`. Если cleanup-cascade глубже —
очередная compose добавляет sentinel entry `MultiErrorTruncated { depth }`
и дальше silently ignores дальнейшие composes.

256 выбран как:
- порядок MAX_DEFER_DEPTH (D193);
- достаточно для всех reasonable cleanup-cascades (10 levels nesting × 25
  resources per level);
- protects from O(N²) compose-чейнов с pathological recursion.

### Concrete error types в prelude

```nova
type CancelError { reason str }
type MultiErrorTruncated { depth int }
```

Эти типы emerge из D90 §7 amend (CancelError). NB: `CleanupTimeoutError`
БЫЛ в этом списке (D192 deadline enforcement) — УДАЛЁН D192-ретрактом
(Plan 173 Ф.5 п.2): превышение cleanup-порога = watchdog-варн +
`duration_ms`/`overrun` в ResourceTrace exit-событии, не ошибка.

### Что удаляется

- `ErrorKind` enum (D158) — типизация через прямой `if err is T`.
- `DeferResult[T, E]` (D160) — заменён `ScopeOutcome`.
- Raw `MultiError.suppressed.push(...)` — должен идти через `nv_compose_error`
  с cycle-check (compile error D193-direct-mutation).

### Сравнение

| Capability | Java | Kotlin | Swift | C++23 | Rust | TS | **Nova D193** |
|---|---|---|---|---|---|---|---|
| Iterable walk | ✅ getSuppressed | ✅ | ✅ | ⚠️ stdexception_ptr | n/a | ✅ AggregateError | ✅ `walk()` |
| Cycle-safety spec'd | ⚠️ JDK-8287921 bug | ⚠️ inherit | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ✅ identity-check |
| Depth-limit explicit | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ 256 |
| Fast panic finder | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ `find_first_panic` |

### Связь

- [D90](#d90) §7 — CancelError.
- [D158](#d158) — amend: ErrorKind retracted.
- [D161](#d161) — amend: MultiError structure.
- [D188](#d188) §R3 — cancel-shield exit composition.
- [D191](#d191) — async cleanup compose path.
- [D192](#d192) — CleanupTimeoutError.

---

## D194. `Cleanup[Never]` — infallible cleanup + hot-path elision

> **Plan 110.** Принято 2026-05-31. **Статус: ACTIVE — с оговоркой (Plan 173 Ф.2.D194,
> 2026-07-03).** Реализована **caller-relaxation** (type-checker снимает требование `Fail[E]`
> для `Cleanup[Never]`-биндинга — см. §Caller relaxation, живо). §perf **hot-path elision**
> (§Hot-path optimization ниже) **НЕ реализована** — см. врезку в конце секции. Special-case
> для resource'ов которые гарантированно не fail в cleanup (Mutex, Sem, Lock).

### Что

Resource-типы которые **гарантированно не fail в cleanup** используют
`Cleanup[Never]`:

```nova
fn MutexGuard consume @cleanup(outcome ScopeOutcome) -> () => @release()
//                                                       ^^^^ no Fail[E]
```

`Fail[Never]` — empty effect set, эквивалентно «не throws».

### Caller relaxation

Type-checker special-case: если binding имеет тип `Cleanup[Never]`,
требование `Fail[E]` у caller'а **снимается**:

```nova
fn use_mutex() -> () {                // нет Fail[E]
    consume _l = mu.acquire() {        // MutexGuard: Cleanup[Never] — ОК
        do_work()
    }
}
```

Это аналог Rust `Result<T, !>` или Haskell `IO ()` без `bracket throws`.
Делает API для locks/permits эргономичным.

### Generic Never special case

```nova
fn use_any[T Cleanup[Never]](r T) -> () {     // нет Fail[E]
    consume binding = r { do_work() }
}
```

Generic с `[T Cleanup[Never]]` тоже не требует `Fail[E]` у caller'а
(см. [D188](#d188) generic constraint).

### Hot-path optimization (D194 §perf) — ⚠ НЕ РЕАЛИЗОВАНА (аспирационно)

> **⚠ Статус (Plan 173 Ф.2.D194, 2026-07-03): эта оптимизация НЕ реализована в codegen.**
> Де-риск Ф.2 (2 агента независимо + firsthand-разбор) установил: единственная эмитящая
> `ConsumeScope`-ветка (`emit_c.rs`, ~19816-20101) эмитит ПОЛНЫЙ frame-bearing путь
> **БЕЗУСЛОВНО** для ВСЕХ resource-типов (вкл. `Cleanup[Never]`): shield-пара
> (`nv_consume_enter_shield`/`nv_consume_leave_shield`), 3-level timeout, ≥2 setjmp-кадра
> (body + on_exit), полный outcome. Effect-row-inspection для Never-элизии в codegen = 0.
> Прежний текст «Disasm-verified в T2.9» — **артефакт отсутствует** (`docs/plans/artifacts/
> 173-disasm-baseline/`), премиса дрейфанула. **Ф.2 (defer-kernel unification) acceptance =
> PARITY** (lowered consume/defer(o) НЕ увеличивает кадры/shield/outcome vs дорефакторный
> frame-bearing вывод), а НЕ «сохранить существующую элизию» (её нет). Генуинная §perf-элизия —
> отдельный deferred followup **`[M-173-d194-perf-elision]`** (перф-оптимизация вне периметра
> unification; требует own disasm-guard artifact).

Целевая (будущая) форма — codegen detect'ит case когда:
1. Binding имеет тип `Cleanup[Never]` И
2. Type не satisfies `WithExitTimeout` (нет custom `exit_timeout()` method).

Тогда codegen мог бы **eliminate**:
- Cancel-shield setup/teardown (на throw на cleanup-path → нет MultiError compose).
- Timeout resolution (5s hardcoded не нужен — release инстант).
- `outcome` construction (Mutex не различает Success/Failure/Panic).

```c
// regular case (СЕЙЧАС — для ВСЕХ типов, вкл. Never):
nv_consume_enter_shield(timeout);
... body (fail-frame) ...
nv_call_on_exit(tx, outcome);   // on_exit-frame
nv_consume_leave_shield(prev);

// elided case (Cleanup[Never] + no WithExitTimeout) — ЦЕЛЬ [M-173-d194-perf-elision], НЕ реализовано:
... body ...
nv_call_release(mu);
```

**Critical для hot-paths**: lock contention, high-frequency permits, fast
mutex/atomic patterns — мотивация для будущего `[M-173-d194-perf-elision]`.

### Когда elision НЕ применяется

- `Cleanup[Never]` + `WithExitTimeout` impl → timeout resolution не
  нужен (5s default), но shield нужен для potential `await exit_timeout()`
  call.
- `Cleanup[E]` где `E != Never` → cleanup может throw, shield нужен.
- Body содержит cancel-points даже если `E = Never` — shield нужен для
  cancel-routing.

### Связь

- [D188](#d188) — Cleanup base.
- [D192](#d192) — WithExitTimeout protocol.
- [Plan 110 Ф.2.8](../../docs/plans/110-scoped-resources-radical-simplification.md#ф2-codegen).

---

## D196. Init type constraints для `consume X = expr { body }`

> **Plan 110 Ф.2.9.** Принято 2026-05-31. **Статус: ACTIVE** (Plan
> 110.1.2 + 110.1.3 + refine landed; forms 1-3, 5 implemented в
> type-checker; form 4 method-chain deep recursion partial). Определяет
> какие expression'ы могут служить init для consume scope-block.

### Правило

`expr` после `=` должен statically resolve к типу implementing
`Cleanup[E]` для какого-то `E`. Type-checker проверяет в Ф.1.5 (после
type inference body init expression'а).

### Acceptable init forms

#### 1. Прямой Cleanup

```nova
consume tx = db.begin() { ... }     // db.begin() : Transaction (Cleanup[DbError])
```

#### 2. Result/Option unwrap через `?` / `!!`

```nova
consume tx = db.try_begin()? { ... }    // try_begin() : Result[Transaction, DbError]
                                        // после `?` → Transaction
```

```nova
consume tx = maybe_tx()!! { ... }       // maybe_tx() : Option[Transaction]
                                        // после `!!` → Transaction
```

#### 3. Conditional (if/match)

```nova
consume tx = if cond { open_a() } else { open_b() } { ... }
```

- Обе ветки должны возвращать совместимый Cleanup type.
- Если a и b возвращают разные Cleanup типы → `D196-divergent-consumable`.

#### 4. Method chain

```nova
consume tx = db.with_config(cfg).begin() { ... }
```

Финальный return type должен быть Cleanup.

### Rejected init forms

#### Wrapped без unwrap

```nova
consume tx = maybe_tx() { ... }     // maybe_tx() : Option[Transaction]
// → D196-wrapped-init-needs-unwrap
```

Suggestion: «use `consume tx = maybe_tx()!! { ... }` или distinguish None
сначала через `if Some(tx) = maybe_tx() { consume tx = tx { ... } }`».

#### Non-Cleanup

```nova
consume x = 42 { ... }     // int не Cleanup
// → D188-not-consumable
```

### Memory ordering для acquisition

`init` evaluation полностью завершается **до** scope-entry (acquire
semantics). `nv_consume_enter` имеет implicit memory fence перед
`nv_run_body_capturing`. Cleanup видит финальное состояние ресурса
(см. [D188](#d188) §R6).

### Связь

- [D85](04-effects.md#d85) — `?`/`!!` operators.
- [D86](04-effects.md#d86) — `??` coalesce.
- [D188](#d188) — Cleanup base.
- [D194](#d194) — Never special case.
- [D196](#d196).

---

## D197. Cleanup re-entrance — nested `consume {}` inside `cleanup`

> **Plan 110 Ф.2.12.** Принято 2026-05-31. **Статус: ACTIVE** (Plan
> 110.1.8 landed; codegen handles re-entrance correctly through nested
> scope-blocks). Разрешает вложенные consume
> scope-block'и внутри `cleanup`.

### Правило

`cleanup` body **может содержать** вложенные `consume {}` блоки:

```nova
fn Connection consume @cleanup(outcome ScopeOutcome) Fail[IoError] -> () {
    // closing the connection requires acquiring lock
    consume _l = @cleanup_mutex.acquire() {
        @do_close()?
    }
}
```

### Семантика

1. **Outer cancel-shield остаётся активен** на время всей outer `cleanup`
   body. Inner consume{} наследует масштабы shield (nested mask).

2. **Inner consume{} создаёт свой shield** с своим timeout. Cancel
   доставка остаётся **глобально pending** до выхода **outer cleanup**.

   **R4 amend (2026-06-05) [M-110.x-cleanup-shield-deadline-underflow] fix:**
   Inner shield's deadline **shadows** outer's. На inner leave outer's
   deadline **должна быть restored** (через prev_deadline save+restore
   pattern в `nv_consume_enter_shield` returning prev / `nv_consume_leave_shield`
   accepting prev arg). Pre-fix bug: `nv_consume_leave_shield()` cleared
   deadline только when mask reached 0 — left inner's stale (shorter)
   deadline visible к outer body, producing inflated/bogus
   CleanupTimeoutError fires (appeared as i64 underflow в reports).
   Codegen threads prev_deadline через local var per consume-block.

   **R4b amend (2026-06-05) [M-110.x-on-exit-throws-leaks-shield] fix:**
   `nv_consume_leave_shield(prev_deadline)` **MUST execute on every exit
   path** from a consume-scope — success, body throw, body panic, cleanup
   throw, cleanup panic. Codegen wraps the cleanup C-call в own
   `NovaFailFrame` setjmp; the catch branch sets `on_exit_threw` kind
   (0=ok, 1=throw, 2=panic). After the wrap, `nv_consume_leave_shield`
   is called **unconditionally** before any re-throw/re-panic decision.
   Pre-fix bug: cleanup C-call ran outside any setjmp; if cleanup body
   threw, longjmp routed to caller's frame, skipping leave_shield → mask
   stuck (cancel deferral leaks into caller fiber) + deadline never
   restored (same shadow bug as R4 but triggered via exception path).
   Re-throw composition follows D193 R5 + D197 R3:
   - body panic OR cleanup panic → `nv_panic` propagates (body's panic
     wins if both panic).
   - body throw + cleanup throw → body=primary, cleanup composed как
     suppressed через `nv_compose_suppressed(&body_frame, on_exit_*)` →
     `nova_rethrow_with_suppressed(&body_frame)`.
   - only body throw → `nova_rethrow_with_suppressed(&body_frame)`.
   - only cleanup throw → `nova_rethrow_with_suppressed(&on_exit_frame)`.
   - both clean → fall through.
   ResourceTrace observability hook (`Nova_ResourceTrace_on_resource_exit`) fires only
   когда cleanup succeeded — preserves existing post-fix behavior.

3. **Inner `cleanup` ошибки compose в локальный MultiError**. Если он
   throws — outer `cleanup` получает это в propagation:
   - outer.cleanup started → outcome = Failure(orig)
   - inner.cleanup failed → MultiError { primary: orig, suppressed: [inner_err] }
   - outer cleanup completes; outer body re-throws с composed.

4. **Depth limit 256** (same as MultiError D193). При превышении — runtime
   error `D197-cleanup-reentrance-depth-exceeded` composes в MultiError;
   cleanup продолжает разворачиваться с этой ошибкой как «...truncated»
   entry.

5. **Запрещено**: re-entrance с тем же ресурсом — linear types prevent
   (D131 use-after-consume). Программер пытающийся `consume X = X { ... }`
   внутри `X.cleanup` получает compile error до reaching this rule.

### Use case

Connection close требующий lock acquisition; Database flush требующий
internal transaction; HTTP keep-alive close требующий buffer flush. Все
эти паттерны — composable resources с inner cleanup.

### Связь

- [D131](05-memory.md#d131) — linear types.
- [D188](#d188) §R5 — LIFO composition base.
- [D192](#d192) — timeout per-scope.
- [D193](#d193) — MultiError + depth limit.
- [Plan 110 Ф.2.12](../../docs/plans/110-scoped-resources-radical-simplification.md#ф2-codegen).

---

## D198. `#realtime` + cleanup-timeout interaction

> **Plan 110 Ф.3.6.** Принято 2026-05-31. **Статус: ACTIVE** (codegen
> `in_realtime` check emits 0-timeout, 2026-05-31). Cross-ref [D172](04-effects.md#d172).

### Семантика `#realtime` (recap)

`#realtime` на функции — **гарантия callee**:
- Внутри `#realtime` fn body: можно вызывать только другие `#realtime` fns
  или `#realtime`-annotated primitive operations.
- Parking ops, allocations, GC pauses запрещены.
- **Никаких ограничений на caller**: обычная fn свободно может вызвать
  `#realtime` fn. Атрибут описывает свойство callee.

См. [D172](04-effects.md#d172) полную семантику attribute.

### Правило для cleanup

Codegen смотрит на **enclosing function** где находится `consume {}`:

```nova
// в обычной fn:
fn foo() Fail[E] -> () {
    consume r = expr { body }
    // codegen: let _timeout = nv_resolve_exit_timeout(r);  // WithExitTimeout / App / 5s
}

// в #realtime fn:
#realtime
fn bar() -> () {
    consume r = expr { body }
    // codegen: let _timeout = Duration.zero();             // hardcoded
}
```

`#realtime` контекст **полностью bypass'ит 3-level resolution** —
runtime functions для timeout lookup не вызываются вовсе.

### Следствия (автоматические из правила #realtime)

1. **`cleanup` метод ресурса должен быть `#realtime`**, иначе compile
   error внутри `bar` body (нельзя вызвать non-`#realtime` fn из
   `#realtime`). Это значит resource-тип используемый в realtime-
   context уже спроектирован для него (`MutexGuard.release`, atomic
   ops).

2. **`WithExitTimeout` impl ресурса не вызывается** — потому что
   `nv_resolve_exit_timeout` не вызывается.

3. **`Application` effect не запрашивается** — same reason.

4. **Suspend в `cleanup` невозможен** — D172 body restriction (parking
   ban), не через нашу new проверку.

### Что НЕ делаем

- ❌ Compile-time heuristic «попытается ли Application override» — не нужно;
  правило `#realtime` body уже всё ограничивает.
- ❌ Runtime fallback к Application в realtime — codegen эмитит zero
  напрямую.
- ❌ Дополнительные constraints на caller — не нужно, атрибут это callee
  promise.

### Diagnostic

`D198-realtime-application-override`: warning если статически detect'имо
что `#realtime` fn внутри `with Application = handler(default_exit_timeout:
non-zero)` scope'е. Application timeout будет ignored — warn user. Это
heuristic detection (не точная analysis); warning, не error.

### Связь

- [D172](04-effects.md#d172) — `#realtime` attribute model.
- [D188](#d188) §R4 — timeout resolution baseline.
- [D191](#d191) — async cleanup parking restrictions.
- [D192](#d192) — 3-level resolution bypassed.
- [D194](#d194) — `Cleanup[Never]` typical realtime pattern.
- [Plan 103.6](../../docs/plans/103.6-realtime-blocking-integration.md).
- [Plan 113](../../docs/plans/113-realtime-blocking-attribute-only.md).

---

## D201. `#cancel_safe` — attestation на FFI safety inside cleanup

> **Plan 110.7.3.a.** Принято 2026-06-01. Cross-ref
> [D188](#d188) §R3 (cancel-shield), [D192](#d192) (exit_timeout).

### Что

`#cancel_safe` — fn-level attribute который аттестует, что функция
**безопасна для вызова из `Cleanup.cleanup` body** под активным
cancel-shield'ом (D188 R3).

```nova
#cancel_safe
external fn sqlite3_close(handle int) -> int

#cancel_safe
fn local_cleanup_helper(state State) -> () { ... }
```

### Зачем

Когда `consume X = expr { body }` выходит, runtime поднимает
cancel-shield (mask_count++). Внешние cancel'ы откладываются до
`leave_shield`. Если `cleanup` вызывает C-функцию, которая:

1. **Блокируется неограниченно** (например, classic POSIX `read(fd)`
   на TTY-устройстве без `O_NONBLOCK`) — fiber виснет на C-стэке,
   shield deadline сгорит впустую.
2. **Не идемпотентна при повторе** — если cancel в итоге сработает
   и unwind рестартанёт cleanup, partial-effect C-state может оставить
   garbage.
3. **Требует Nova fail-frame state** — например читает `_nova_fail_top`
   — это нестабильно через FFI boundary.

`#cancel_safe` — обещание разработчика, что вызываемая функция отвечает
**трём требованиям**:

1. **Bounded completion time.** Функция завершится за разумное время даже
   под cancel-shield'ом — то есть **не может зависеть от внешнего cancel
   для пробуждения / завершения** (внешний cancel игнорируется shield'ом).
   Конкретно:
   * Никаких `read()` / `recv()` / `poll()` без timeout'а на файл-дескрипторах
     которые могут никогда не получить данные.
   * Никаких `pthread_cond_wait` / event-loop wait'ов без timeout'а.
   * Никаких busy-loop'ов которые ожидают «снаружи что-то изменится».
   *Антипаттерн:* C-функция «жди новой записи в очереди пока не отменят» —
   под shield'ом отмена не прилетит → fiber висит до exit_timeout.
   *Хороший паттерн:* C-функция делает свою работу sync'но (`fclose`,
   `sqlite3_close`, `free`) и возвращается.

2. **Idempotent для cleanup семантики.** D188 R2 «exactly-once»
   гарантия требует чтобы partial-effect cleanup был safe для рестарта
   (multi-cancel / multi-throw scenarios — компилятор не дублирует, но
   если повтор в коде → должен быть OK).

3. **Не зависит от Nova fail-frame TLS state.** Внутри Nova fail-frame
   chain (`_nova_fail_top` TLS pointer) — это runtime mechanism Nova для
   throw routing. C-код **не должен**:
   * Читать `_nova_fail_top` / `_nova_active_scope` / другие internal
     TLS-переменные runtime'а.
   * Вызывать `nova_throw_*` / `nova_fail_push/pop` напрямую.
   * Полагаться на Nova handler stack или ScopeOutcome.
   *Причина:* shield'ом fail-frame в strange mid-unwinding state; C-кода
   таких допущений делать не должен. C-код возвращает int код ошибки —
   Nova-обёртка (caller) сама конвертирует в throw если надо.

### Lint

При вызове FFI fn БЕЗ `#cancel_safe` из `cleanup` body — компилятор
выдаёт **`W_FFI_CANCEL_UNSAFE`** warning с suggestion:
* Добавить `#cancel_safe` к декларации FFI fn если действительно safe.
* Обернуть call в sync-only wrapper если cancel-safety не гарантирована.

Внутри тела обычной Nova fn (не FFI) — `#cancel_safe` не требуется;
весь Nova-код cancel-safe by construction (cancel routed через
nova_throw_cancel + fail-frame).

### Что НЕ делает `#cancel_safe`

* Не меняет codegen вызова — это статическая аттестация.
* Не отключает cancel-shield (это всегда активно under ConsumeScope).
* Не предоставляет runtime check на cancel-safety — только compile-time
  warn'ит на отсутствующую аттестацию.

### Связь

- [D188](#d188) §R3 — cancel-shield механизм.
- [D192](#d192) — exit_timeout taxonomy.
- [Plan 110.7](../../docs/plans/110.7-ffi-consumable.md) — FFI
  Cleanup integration.
- [Plan 100.5](../../docs/plans/100.5-ffi-external-integration.md) —
  general FFI rules.
- Followup [M-110.7.3-w-ffi-cancel-unsafe-lint] — runtime lint
  enforcement (currently parser stores attribute, lint check pending).

---

## D227. Numeric literal inference — default `int`, context coercion, hard range-check

> **Принято 2026-06-03.** Уточняет и амендит [D44](#d44) §«default-типы»;
> закрывает Rust-style narrow-fallback question. Связан с
> [D129](02-types.md#d129) (`int = i64`) и [D226](02-types.md#d226)
> (signed indexing).

### Что

Целочисленный литерал без context-аннотации имеет тип `int`
([D129](02-types.md#d129) alias `i64`). В позиции с явным числовым
типом (annotation, parameter type, field type, generic constraint)
литерал **coerce**'ится в этот тип с **compile-time range-check**.
Rust-style fallback «default to `i32` when value fits» **отвергнут**.

### Правило

1. **Default без context.** Голый литерал `42`, `0xFF`, `1_000_000_000`
   в позиции без typed target → `int` (= `i64` per D129).

    ```nova
    ro x = 42                 // x: int
    ro y = 0xFF               // y: int (= 255)
    ro z = 3_000_000_000      // z: int (out of i32 range — fine, int = i64)
    ```

2. **Context coercion.** В позиции с явным numeric target литерал
   принимает этот тип без cast:

    ```nova
    ro a i32 = 100            // a: i32
    ro b u8  = 0xFF           // b: u8
    fn write(b u8) -> () => ...
    write(200)                // 200: u8 (coerce'ится из context)
    ro arr []f32 = [1.0, 2.0] // элементы: f32
    ```

   Working positions: `let X T = …`, function-call argument, field
   initializer, return statement в fn с явным return-type, generic
   instantiation argument когда тип фиксирован, array/record literal в
   typed context (см. [D55](02-types.md#d55)).

3. **Range-check на compile time.** Если литерал не помещается в
   target type — **hard compile error**, не silent truncation:

    ```nova
    ro a i32 = 3_000_000_000   // ✗ E_LIT_OUT_OF_RANGE: 3000000000 > i32.MAX (2147483647)
    ro b u8  = 300             // ✗ E_LIT_OUT_OF_RANGE: 300 > u8.MAX (255)
    ro c u8  = -1              // ✗ E_LIT_OUT_OF_RANGE: -1 < u8.MIN (0)
    ro d i32 = 100 + 50        // ok если оба операнда — литералы, evaluator проверяет sum
    ```

   Это **отличие от Rust**, где `let a: i32 = 3_000_000_000` тоже
   error, **но** `let x = 3_000_000_000` → silent `i64` (а в Nova →
   `int` per Rule 1). И отличие от C/Java, где silent truncation.

4. **Без type-suffix.** [D44](#d44) подтверждается: `100u32`, `1.5f32`
   и прочие suffix-формы остаются rejected. Тип выбирается annotation'ом
   или `as`-cast:

    ```nova
    100u32        // ✗ syntax error (D44)
    100 as u32    // ok
    let x u32 = 100   // ok (Rule 2)
    ```

5. **Floating-point параллельно.** Дробный литерал без context → `f64`
   ([D44](#d44)); с context → `f32` или `f64` с range-check (overflow
   при exponent overflow тоже compile error).

6. **Negative literal в unsigned context.** `-1`, `-200` etc. в позиции
   **любого** unsigned-типа — `u8`/`u16`/`u32`/`u64` **И wide `uint`** —
   **hard error** (`E_LIT_OUT_OF_RANGE`, напр. `-1 < uint.MIN (0)`), не wrap.
   Для wrap-семантики используется `(-1) as u32` (D54 saturation rules apply).

   > **Amend 2026-06-20 (Plan 172.1, `[M-172.1-U5.2-d227-neg-uint]`).** Уточнение
   > для wide-default `uint`. Правило «widest int» (Rule 1: `int`/`uint` несут
   > любой литерал без **верхнего** range-check) относится ТОЛЬКО к верхней
   > границе. **Нижняя граница (floor 0) для unsigned проверяется ВСЕГДА**, включая
   > `uint`: отрицательный литерал в `uint` — sign-domain ошибка, как и в `u64`. До
   > 2026-06-20 impl имел дыру — `uint = -1` молча принимался (wrap → `u64.MAX`),
   > тогда как `u64 = -1` корректно ругался. Fix (impl→spec conformance) ключится на
   > общем свойстве `signed == false`, не на имени `uint` (§3 общий механизм).
   > Регресс — `detect172/d227` (pos: положительный `uint`/large-hex OK; neg:
   > `uint = -1`/`-5` → `E_LIT_OUT_OF_RANGE`).

7. **Pointer-typed positions.** Литералы **не coerce'ятся в pointer
   type** автоматически (в отличие от numeric Rule 2). Требуется
   явный `as`-cast:

    ```nova
    ro p *() = 0                  // ✗ E_LIT_PTR_NO_COERCE
    ro p *() = 0 as *()           // ok — explicit cast (Plan 134 / D214)
    ro h *() = 0x1000 as *()      // ok — opaque handle from integer
    ro q *u8  = some_ptr          // ok — pointer-to-pointer, no literal
    ```

   (`ptr` в type position удалён Plan 134, 2026-06-09 — используй `*()`.)

   В pointer arithmetic offset — обычный `int` per Rule 1, scaled
   by `sizeof(T)` runtime ([D216](02-types.md#d216) §6):

    ```nova
    unsafe {
        ro p2 = some_ptr + 1      // 1: int (Rule 1), p2: *unsafe T
        ro p3 = some_ptr + offset // offset: int
    }
    ```

   `null ptr` literal **retracted** (Plan 118 A23, 2026-06-02
   [D214 amend](02-types.md#d214); `ptr` removed Plan 134 2026-06-09) —
   стандартный паттерн: `(0 as *())`. Для nullable pointers —
   `Option[*T]` (NPO codegen per [D216](02-types.md#d216) §7), `None`
   это constructor, не литерал.

### Почему

**Industry baseline (2026-06).**

| Язык | Default `42` | Coerce в context? | Range-check на compile? | Suffix? |
|---|---|---|---|---|
| Rust | `i32` (fallback) | Нет — strict | Да (на typed binding) | Да (`42i64`) |
| Swift | `Int` (word) | Да | Да | Нет |
| Go | untyped const → `int` | Да | Да | Нет |
| Java | `int` (i32) | Implicit widen | Да (overflow → error при literal) | Да (`42L`) |
| Kotlin | `Int` (i32) | Implicit widen | Да | Да (`42L`) |
| C# | `int` (i32) | Implicit widen | Да (`unchecked` opt-in) | Да (`42L`) |
| Zig | `comptime_int` (∞) | Yes (coerce at use) | Yes | Нет |
| Nim | `int` (word) | Yes | Yes | Нет |
| **Nova** | **`int` (= i64)** | **Yes** | **Yes — hard error** | **Нет** |

Nova model = **Zig/Swift гибрид** (wide default + context coerce) **без**
Rust-style narrow-fallback и **без** Java/C# silent widening / suffix.

**Конкретные обоснования:**

1. **Никакого `as i32` шума.** `arr[i]` где `i` индекс — работает без
   cast потому что `int = i64` = natural index type per [D226](02-types.md#d226).
   Rust `arr[x as usize]` — постоянная ceremony — заслужено критикуется.

2. **Predictability.** «`42` это всегда тот же тип» — правило, которое
   человек / LLM может удержать без edge cases. Rust `42` → `i32` если
   default, → `usize` если в `Vec::with_capacity(42)`, → `u8` если field —
   три разных типа по 5 правилам fallback. Slop.

3. **Compile-error overflow > silent wrap.** `ro b u8 = 300` ошибка на
   3 декларации раньше, чем `b` использован — surface area ошибки
   минимальна. Plan 33.8 runtime `int` overflow → panic; literal
   overflow надо catch'ить на компиляции, не runtime.

4. **AI-first ([D10](01-philosophy.md#d10)).** LLM пишет числа без
   suffix'а. Default = «общий int», context coerce'ит — LLM не должен
   гадать ширину.

5. **Generic instantiation hygiene.** `Vec.new()` потом `push(42)` →
   `Vec[int]`. Без narrow-fallback `Vec[i32]` инстанциация не возникает
   «случайно» — каждое `[i32]` появляется только из явной аннотации.
   Меньше mangled symbols в binary.

6. **Refactor safety.** Изменить `42` на `3_000_000_000` — `int`
   спокойно держит. В Rust меняет тип `i32` → `i64` со cascade-effect
   на все use sites.

### Что отвергнуто

1. **Rust-style narrow-fallback (`42` → `i32` если влезает).** Создаёт
   3 разных типа для одного литерала в зависимости от контекста; cast-hell
   с usize-индексами (у нас был бы `int`-индексами); refactor-hostile.
   Главный design-debt Rust в области numeric ergonomics.

2. **Default = `i32` (Java/Kotlin/C# stance).** Раскрыто в research
   2026-06-03: overflow на 2.1B в современном коде вероятен (file sizes,
   timestamps, counters, hashes), Plan 33.8 panic'ает, D226 индексы
   ломаются. Java team регретит, ловить ту же грабельку — без выгоды.

3. **Bigint-default (Python stance).** Performance regression vs
   `i64`; runtime arbitrary-precision support увеличивает runtime
   complexity. Plan 33.7 BitVec/sized integers покрывает domain, где
   точная ширина нужна. Bigint — future stdlib type `BigInt`, не
   primitive default.

4. **Type-suffix (`42i64`, `100u8`).** Подтверждено [D44](#d44).
   Annotation `let x i64 = 42` уже работает; suffix дублирует.

5. **Silent truncation на overflow.** C/Java behaviour. Hidden bug
   source; не AI-friendly; не aligned с Plan 33.8 soundness philosophy.

### Связь

- [D44](#d44) — базовый numeric literal grammar (этот D227 amend'ит
  default-types раздел).
- [D54](#d54) — `as`-cast saturation / narrowing semantics (runtime
  conversion path).
- [D55](02-types.md#d55) — literal coercion в typed positions (parallel
  rule for record/sum constructors).
- [D129](02-types.md#d129) — `int = i64` alias (bootstrap invariant).
- [D130](02-types.md#d130) — `uint = u64` alias.
- [D226](02-types.md#d226) — signed indexing convention (`int` для
  len/capacity/index, дополняется правилом «литерал в индексной
  позиции = int by Rule 1»). См. также [D226 §7](02-types.md#d226)
  «Pointer interactions» — matrix offset/diff/FFI типов.
- [D214](02-types.md#d214) — opaque pointer type (`ptr` removed Plan 134;
  use `*()`) + null retract → `(0 as *())` (motivates Rule 7).
- [D216](02-types.md#d216) — `*T` typed pointer family + arithmetic
  semantics (motivates Rule 7 «offset = int per Rule 1» pattern).
- [Plan 33.8](../../docs/plans/33.8-verifier-soundness.md) Ф.1 — runtime
  `int` overflow → panic (motivates compile-time literal range-check).

### Эволюция

- **D44** (2025-Q4): зафиксировал «default int, context переопределяет»
  + reject suffix. Не специфицировал: range-check policy, narrow-fallback
  policy, behaviour negative-в-unsigned.
- **D129** (2026-05-19): `int = i64` alias на bootstrap. D44 line
  «`int` платформенно-зависимая ширина» становится drift.
- **D226** (2026-06-03): signed indexing convention; внутренне
  предполагает Rule 1 («голый литерал = int»), но не формализует.
- **D227** (2026-06-03, этот блок): закрывает три пробела —
  no narrow-fallback, hard range-check, negative-в-unsigned;
  amend D44 default-type line (см. inline amend в D44 выше).

### Acceptance criteria

- [x] D227 spec block формализует 4 правила + industry baseline +
  rejected alternatives
- [x] D44 §«default-типы» amend cross-refs D129 + D227
- [x] Compiler error code `E_LIT_OUT_OF_RANGE` — landed Plan 142 Ф.1
  (commit `d6b209b8e63`). Emitted при context-coercion целочисленного
  литерала к sized-int в `types/mod.rs` (`assignable()` IntLit-арм +
  `Unary{Neg, IntLit}`-арм для negative-в-unsigned Rule 6); сообщение
  `[E_LIT_OUT_OF_RANGE] <val> > <T>.MAX (<max>)` / `< <T>.MIN (<min>)`.
- [x] Test corpus — landed Plan 142 Ф.1 в **`nova_tests/plan142/`**
  (не `nova_tests/types/literal_range_*` как изначально именовалось в
  этом блоке): 8 NEG (`neg_u8_300`, `neg_u8_minus1`, `neg_i32_3b`,
  `neg_u16_70000`, `neg_i8_200`, `neg_u8_hex_1ff`, `neg_u32_4b`,
  `neg_arg_u8` — call-arg path) + 2 POS (`pos_boundaries` — все 8 sized
  MIN/MAX exactly in-range, включая boundary `127`/`-128` для i8;
  `pos_wide_int` — Rule 1 default `int`). 10/0 PASS на релизном `nova`.

### Открытые вопросы (scoped)

- **Alias / newtype над sized-int.** `assignable()` range-check'ит
  только **прямой** Named sized-int (+ `Readonly`/`Mut`/`Unsafe`
  wrappers). Литерал в позиции alias'а / newtype над sized-int
  (напр. `type Age = u8; ro a Age = 300`) **не** проверяется — чтобы
  не печатать неверное имя типа в диагностике (требуется резолв через
  `self.types`, недоступный из free-fn coercion-сайта). Скоуп для
  будущего sub-plan если alias-coverage понадобится.
- **Float range-check (Rule 5).** Compile-time overflow дробного
  литерала при coercion к `f32` (exponent overflow) **не реализован** —
  Plan 142 Ф.1 scope был integer-only (plan §43 «все 8 sized-int»;
  floats не перечислены). Rule 5 остаётся spec-only до отдельного
  enforcement-плана.

---

**D236 — Tuple destructuring assignment**

Syntax:
```nova
(lhs_0, lhs_1, ..., lhs_N) = (rhs_0, rhs_1, ..., rhs_N)
```

Semantics:
- RHS is evaluated completely before any LHS assignment begins.
- Each lhs_i must be a mutable lvalue: mut local binding, @field, arr[i], or chain.
- LHS and RHS element counts must match exactly.
- Consume types on lhs are banned in V1 ([M-136-consume-tuple-assign]).

Codegen V1 — conservative tmp:
1. Build lhs_names = set of root ident names from all lhs elements.
2. For each rhs_i that reads any ident in lhs_names: emit C tmp variable.
3. Assign lhs_i from tmp (or direct rhs_i if independent).

Example: (a, b) = (b, a) generates:
```c
{ nova_int _nv_ta_0 = b; nova_int _nv_ta_1 = a; a = _nv_ta_0; b = _nv_ta_1; }
```

Codegen V2 (Plan 136.1 -- ACTIVE): pure permutation (all rhs[i] lvalue-equal to some
lhs[j], mapping bijective) -> cycle-decomposition, 1 tmp/cycle. Otherwise: V1 fallback.

Error codes:
- E_TUPLE_ASSIGN_ARITY_MISMATCH  lhs.len() != rhs.len()
- E_TUPLE_ASSIGN_LHS_NOT_MUT     lhs element is not a mutable lvalue
- E_TUPLE_ASSIGN_CONSUME_TYPE    lhs element has consume type (V1 ban)

Acceptance criteria:
- A1: (a, b) = (b, a) compiles and executes correctly
- A2: (a, b, c) = (b, c, a) rotate works correctly
- A3: independent rhs (no lhs overlap) — 0 tmp in C
- A4: E_TUPLE_ASSIGN_ARITY_MISMATCH fires on arity mismatch
- A5: E_TUPLE_ASSIGN_LHS_NOT_MUT fires on ro-binding lhs
- A6: 0 regressions in existing tests

Followup markers:
- [M-136-cycle-decomp] V2 codegen: cycle-decomposition
- [M-136-nested-tuple-lhs] nested tuple lhs ((a,b),c) = ...
- [M-136-consume-tuple-assign] consume types in tuple-assign

Related: Plan 53 (let destructuring), Plan 59 (tuple mono), Plan 120 (named tuples), D215
Added: 2026-06-09  Status: ACTIVE

---

## D238. `Index[K, V]` protocol — `a[key]` magic

> **AMENDED (Plan 152.1 / D249, 2026-06-13): RETRACT `str | int | char`.** `str` НЕ
> реализует `Index[int, char]` — `s[i]` (int) → `E_STR_NO_INT_INDEX` (codepoint-index
> O(n) под видом O(1)). `str` реализует ТОЛЬКО `Index[Range, str]` (byte-range slice,
> contract-bounds, `slice.nv`). Элементный доступ — через линзы `as_bytes()[i]` /
> `for c in as_chars()` (positional `as_chars().nth(i)` retracted — D260-амендмент).
> См. [D249/D250](#d249).

### Что

`@index(key K) -> V` — протокол для `a[key]` доступа на user-типах.
`K` — тип ключа (int для массивов, Range для slicing, str для maps).
`V` — тип возвращаемого значения.

### Семантика

`a[key]` десугаринг в `a.index(key)` для любого типа реализующего
`Index[K, V]`. Паникует при invalid key (OOB, missing key etc.).

Builtin пути (`[]T`, `str`, `NovaArray_*`) продолжают работать через
compiler built-in dispatch. User-типы реализуют через `@index` метод.

### Примеры

```nova
// Vec[T] implements Index[int, T]:
assert(v[i])          // → v.index(i)

// Vec[T] implements Index[Range, Vec[T]]:
assert(v[2..5].len() == 3)  // → v.index(Range { start: 2, end: 5 })

// Custom map implements Index[K, V]:
type MyMap[K, V] { ... }
fn MyMap[K, V] @index(key K) -> V { ... }
assert(mymap["hello"])  // → mymap.index("hello")
```

### Стандартные реализации

| Тип | K | V |
|---|---|---|
| `Vec[T]` | `int` | `T` |
| `Vec[T]` | `Range` | `Vec[T]` (zero-copy view) |
| `str` | `int` | `char` (panic OOB) |
| `str` | `Range` | `str` (byte-range view, panic OOB) |

### Slicing: `Index[Range]`

`v[a..b]` и `v[..]` — **механизм среза (slice)** в Nova. Нет
отдельного slice-протокола: Range — просто другой тип ключа `K` в `Index[K, V]`.

Компилятор опускает `a[key]` в `a.index(key)`. Для `a[range]` ключ —
значение `Range`. Пример:

```nova
mut v []int = [10, 20, 30, 40, 50]   // []int = Vec[int] (D239)

v[1..4]       // → v.index(Range { start: 1, end: 4 })   // [20, 30, 40]
v[1..4][0]    // → [20]   (цепочка: сначала срез, потом скалярный index)
v.get(1..4)   // → Option[Vec[int]]  (safe-версия, None если OOB)
```

**Open-ended bounds** (`v[1..]`, `v[..3]`, `v[..]`) — компилятор подставляет
`0` или `arr.len()` перед вызовом `@index`, так что пользовательские
реализации всегда получают **полностью заполненный `Range`**:

```nova
v[2..]   // → v.index(Range { start: 2, end: v.len() })
v[..3]   // → v.index(Range { start: 0, end: 3 })
v[..]    // → v.index(Range { start: 0, end: v.len() })  // полная копия/view
```

Open-ended формы допустимы **только** в index-context (`a[range]`).
В for-loop / материализации — compile-error (нужна bounded форма), см. D58.

**Zero-copy semantics** (для `Vec[T]`): `v[a..b]` возвращает новый
`Vec[T]`-заголовок (pointer + len + cap), указывающий внутрь того же
GC-tracked буфера. Мутации view (`push`) вызывают realloc → silent detach
от родителя. Backing-буфер защищён `GC_set_all_interior_pointers`.

`str` реализует `Index[Range, str]` — slice по байтам (panic при
разрезании посередине кодпоинта не предусмотрен в V1; планируется D239+).

### Почему

- Устраняет compiler magic для `a[i]` — пользователь может реализовать
  indexing для своего типа (Map, Matrix, etc.)
- Нет отдельного `Slice`-протокола — Range как K достаточен; перегрузка
  по типу ключа обеспечивает и скалярный, и range доступ
- Выравнивает с Python `__getitem__`, C++ `operator[]`, Rust `Index`
- Совместно с `MutIndex[K, V]` (D240) покрывает полный read/write API

### Что отвергнуто

- **`@get(key)` вместо `@index(key)`** — имя `get` занято `Option`-возвращающей
  версией (safe access). `@index` — panic-версия, явно иная семантика.
- **`[T Index[K, V]]` обязательный bound** — структурная типизация достаточна,
  никаких `impl`-блоков.
- **Отдельный `Slice[T]` протокол** — излишен: `Index[Range, Vec[T]]`
  выражает то же самое через обычный перегруженный `@index`.

### Связь

- D58 — `for x in c` implicit iter (аналогичный sugar-паттерн); open-ended bounds в for-loop запрещены
- D239 — `[]T` как сахар над `Vec[T]`; `v[a..b]` возможен потому что `[]T = Vec[T]` реализует `Index[Range, Vec[T]]`
- D240 — `MutIndex[K, V]` write magic
- Plan 138 — `[]T` sugar over `Vec[T]` + Index protocol

Added: 2026-06-10  Status: ACTIVE

---

## D240. `MutIndex[K, V]` protocol — `a[key] = val` magic

### Что

`mut @index(key K, val V)` — протокол для `a[key] = val` записи.
Read-only типы реализуют только `Index[K, V]`.
Мутабельные типы реализуют оба.

### Семантика

`a[key] = val` десугаринг в `a.@index(key, val)` (write-overload `@index`
с `mut`-receiver) для типов реализующих `MutIndex[K, V]`. Паникует при invalid
key (OOB, etc.). Запись-`@index` отличается от чтения-`@index` арностью
(2 аргумента vs 1) и `mut`-receiver'ом.

### Примеры

```nova
// Vec[T] implements MutIndex[int, T]:
mut v []int = [1, 2, 3]
v[0] = 99   // → v.@index(0, 99)
assert(v[0] == 99)
```

### Стандартные реализации

| Тип | K | V |
|---|---|---|
| `Vec[T]` | `int` | `T` |

### Followup

- `[M-138-index-set]` — compiler dispatch для `a[key] = val` через
  write-overload `mut @index(key, val)` (пока LHS assignment через builtin path —
  codegen инлайнит `v[i] = val` напрямую, см. emit_c.rs `Stmt::Assign` +
  `ExprKind::Index`; протокол-метод declared для конформанса MutIndex D240)

### Связь

- D238 — `Index[K, V]` read magic (парный протокол)
- Plan 138 — `[]T` sugar over `Vec[T]` + Index protocol

Added: 2026-06-10  Status: ACTIVE

---

## D239-зеркало — `[]T` как сахар над `Vec[T]` (канон: 02-types.md#d239)

> ⚠ **ДУБЛЬ-ЗЕРКАЛО (помечено 2026-07-03):** канонический блок D239 — [02-types.md#d239](02-types.md#d239-t--синтаксический-псевдоним-vect) (полная метадата/typed-storage/§amend). Этот блок — краткое зеркало (Plan 138); при расхождении канон = 02-types.

### Что

`[]T` — **синтаксический псевдоним** `Vec[T]`. После миграции (Plan 138 Ф.5)
компилятор разворачивает любое `[]T` в `Vec[T]` на уровне type resolution.

```nova
// Эти объявления эквивалентны:
mut a []int = [1, 2, 3]
mut b Vec[int] = [1, 2, 3]

// Литерал desugars в Vec[T]:
[1, 2, 3]  →  Vec[int].new(3); push 1; push 2; push 3
//   вместимость = числу элементов (канон: 02-types.md#d239,
//   Amended 2026-08-27). Здесь стоял `with_capacity(3)` — снят амендментом
//   vec-sweep 2026-07-06, а зеркало осталось на ретрактированном имени.

// []T.new() = Vec[T].new()
mut v []int = []int.new()
```

### Зачем

До D239 `[]T` — встроенный C-макро тип (`NovaArray_T`), `Vec[T]` — чистая
Nova-реализация с тем же layout (`{ data *T, len int, cap int }`, 24 байта).
Объединение:

- Закрывает typed-storage gap: `[]Option[int]`, `[]Record` получают
  правильное typed хранение вместо int64-erasure
- Убирает дублирование логики между `[]T` и `Vec[T]`
- `v[a..b]` работает потому что `Vec[T]` реализует `Index[Range, Vec[T]]` (D238)
- Все методы `Vec[T]` (`push`, `pop`, `len`, `get`, `iter` и т.д.)
  доступны через `[]T`-синтаксис

### Статус

> **Plan 138 Ф.1-Ф.4 (2026-06-10):** D239 частично активен — `[]T → Vec[T]` flip
> работает в единицах компиляции, которые явно импортируют `Vec` (т.е. имеют
> `Vec`-шаблон в `generic_type_templates`). Примитивные единицы без `Vec`-импорта
> продолжают использовать `NovaArray_T` C-backing.
>
> **Plan 138.2 [M-138.1-vec-in-prelude]:** сделать `Vec` частью prelude —
> тогда D239 включится для всех единиц и `NovaArray_T` можно будет убрать.

### Правила

- `[]T` = `Vec[T]` на уровне типов: `fn f(a []int)` принимает `Vec[int]`
- Литерал `[e1, e2, ...]` в позиции `[]T` строит `Vec[T]`
- Spread-литерал `[...xs]` → итерация с push (D60)
- `[]T.new()` и `Vec[T].new()` — одно и то же

### Связь

- D238 — `Index[K, V]`; `v[a..b]` через `Index[Range, Vec[T]]`
- D240 — `MutIndex[K, V]`; `v[i] = x` через `MutIndex[int, T]`
- D27 — синтаксис `[]T` (формат записи сохраняется, семантика меняется)
- D144 (02-types.md) — migration path: «`[]T` may become sugar over `Vec[T]`» — закрывается D239
- Plan 138 — полный план миграции

Added: 2026-06-10  Status: ACTIVE (partial — gated on Vec-in-prelude)

---

## D241. Канонический порядок модификаторов type-декларации (scope-adjacency)

### Что

Модификаторы в объявлении типа (`export type NAME <mods> { fields }`) имеют
**единственный канонический порядок** — по **scope** (область действия), от широкой к
узкой слева направо. Несколько эквивалентных порядков (order-independence) **запрещены**
(«one canonical syntax» — Nova не допускает разных написаний одного и того же).

```nova
export type Point value priv { x f64, y f64 }   // ✓ канон
type Point priv value { ... }                    // ✗ E_MODIFIER_ORDER → fix-it «value priv»
```

| Позиция | Модификатор | Scope |
|---|---|---|
| до `type` | `export` | видимость **типа** в модуле (широчайший) |
| после имени, левее | `value` | аллокация/представление **всего типа** (stack vs heap) |
| после имени, вплотную к `{` | `priv` | дефолт видимости **полей** в `{…}` |
| `{ … }` | — | сами поля |

### Зачем

**Scope-adjacency**: каждый модификатор стоит рядом с тем, что определяет.
- `priv` задаёт дефолт видимости полей внутри `{…}` → квалифицирует блок `{…}` → вплотную к `{`.
- `value` — свойство всего типа (представление/копирование) → type-level → левее.
- `export` — видимость самого типа → широчайший scope → перед `type`.

Чтение монотонно сужает scope: **модуль → тип → поля → сами поля**.

Выбор `value priv` (не `priv value`) обоснован **по существу** (scope-adjacency), а не
«так уже принято»: `priv` примыкает к полям, которыми управляет; `value` остаётся на уровне
типа. Контраргумент «видимость первой» (как `pub`/`priv` в member-декларациях) не побеждает:
видимость **типа** уже выражена `export` слева, а type-level `priv` — это **дефолт полей**,
иная сущность, принадлежащая блоку `{…}`.

Правило обобщается на **все** будущие type-модификаторы: сортировать по scope (type-level →
field-default), не вводя произвольных синонимичных порядков.

### Статус

> **РЕШЕНО 2026-06-11; ENFORCEMENT РЕАЛИЗОВАН 2026-06-12 (Plan 148 Ф.1,
> `[M-138-canonical-modifier-order]` CLOSED).**
> Order-independence (намеренный артефакт Plan 124.8) RETIRED. Parser
> (`parser/mod.rs::parse_type_decl`, modifier-loop) присваивает каждому
> модификатору canonical rank, проверяет монотонное возрастание rank'ов в
> порядке появления и при инверсии эмитит `E_MODIFIER_ORDER` с
> machine-applicable fix-it (переписывает modifier-регион в канон).
> `nova_tests/plan124_8/modifier_order_independence_ok.nv` ФЛИПНУТ в negative
> (`EXPECT_COMPILE_ERROR E_MODIFIER_ORDER`); positive-канон + neg-инверсии в
> `nova_tests/plan148/mo_*`.

### Правила

- Канон: `export` (до `type`) → type-level mods (`value`) → type-level ownership
  (`consume`) → field-default mods (`priv`) → `{ fields }`.
- **Canonical ranks** (по scope, широкий→узкий): `value`=0 (представление/аллокация
  всего типа) → `consume`=1 (must-consume обязательство всего типа) → `priv`=2
  (дефолт видимости полей). `export` — отдельным keyword'ом *до* `type`, в rank-набор
  не входит (всегда левее имени).
- Out-of-canon (любая инверсия rank'ов в порядке появления, напр. `priv value`,
  `priv consume`, `consume value`) → `E_MODIFIER_ORDER` с fix-it (переписать
  modifier-регион в rank-отсортированный канон).
- 0 или 1 модификатор — всегда канон (нечего переставлять); проверка фактически
  применяется к ≥2 модификаторам.
- Правило по scope обобщается на любые новые модификаторы: новому модификатору
  присваивается rank по его scope (type-level → field-default) — он автоматически
  попадает в проверку монотонности, без введения синонимичных порядков.

### Связь

- type-level `priv {}` flip (02-types.md, Plan 124) — field-default visibility (`priv` модификатор)
- value-records (Plan 124.8) — `value` модификатор (stack-аллокация)
- D239 — `[]T ≡ Vec[T]`; Plan 138 family
- D124 / D220 (02-types.md) — `priv` field-default visibility (rank 2 модификатор);
  D241 фиксирует его позицию в каноне (amend D124's order-independence allowance).
- Plan 124.8 — `value` модификатор + (ретированный) order-independence test
- `[M-138-canonical-modifier-order]` — enforcement followup → CLOSED Plan 148 Ф.1

Added: 2026-06-11  Status: IMPLEMENTED 2026-06-12 (Plan 148 Ф.1; enforcement в parser,
[M-138-canonical-modifier-order] closed)

---

## D262. Slice-op surface на `[]T`-view модели — без нового типа

> **AMEND (2026-07-06, D410):** `@as_slice()` / `mut @as_slice()` переименованы в
> `@slice()` / `mut @slice()` — префикс `as_` упразднён (см. [D410](#d410-ретракция-as_to_-близнецов-голые-имена-виды-копия-на-месте-вызова-2026-07-06)).
> Ниже — исторический текст с прежним именем.

> **Минорное решение (Plan 153.4).** Фиксирует, что весь slice-операционный
> поверхностный API (`split_at`/`split_first`/`split_last`/`first_n`/`last_n`/
> `as_slice` + ленивые `chunks`/`windows`) строится **на уже принятой**
> `[]T`-view модели D238/D239 + Plan 96 (D-single-type, D-cap-len) и **НЕ
> вводит** отдельный `Slice[T]`-тип. Новых типов/протоколов нет — это
> подтверждение едино-типной модели + конкретная сигнатурная поверхность.

### Что

Slice-операции (разбиение, префикс/суффикс, whole-view) на `Vec[T]` возвращают
**view того же типа** `[]T ≡ Vec[T]` (`{ data: @data + start, len, cap: len }`,
`cap == len`), указывающий внутрь того же GC-tracked буфера. Нет `Slice[T]`,
нет `&[T]`, нет borrow-lifetime — view это просто `Vec`-заголовок (D238 уже
зафиксировал это для `v[a..b]`; D262 распространяет на named-методы).

| Метод | Сигнатура | Контракт |
|---|---|---|
| `@split_at(i)` | `-> (Self, Self)` | `requires 0 <= i <= len` (OOB → panic, НЕ clamp) |
| `@split_first()` | `-> Option[(T, Self)]` | пусто → `None` |
| `@split_last()` | `-> Option[(T, Self)]` | пусто → `None` |
| `@first_n(n)` | `-> Self` | **CLAMP** (`n>len`→весь, `n<=0`→пусто) |
| `@last_n(n)` | `-> Self` | **CLAMP** (как `first_n`) |
| `@as_slice()` | `-> Self` | ro whole-view (`Vec`-side аналог `str.as_bytes()`) |
| `mut @as_slice()` | `-> mut Self` | write-through whole-view (recv-mut overload) |
| `@chunks(n)`/`@chunks_exact(n)`/`@rchunks(n)`/`@windows(n)` | `-> BoxIter[Self]` (LAZY, yield `[]T`-view'ы) | `requires n > 0`; в `std/collections/vec_lazy.nv` (Plan 153.4-B, ✅ IMPLEMENTED) |

### Семантика

- **Same-type, zero-copy.** Каждый view алиасит родительский буфер; никакой
  внешней аллокации. Owning-копия — `clone()`/`to_vec()`, никогда не view.
- **`split_at` — контракт (panic), `first_n`/`last_n` — clamp.** `split_at`
  обязан держать инвариант `len(left) + len(right) == len`; silent clamp скрыл
  бы баг вызывающего и сломал инвариант → `requires`-violation panic. У `first_n`/
  `last_n` семантика «взять до N» — clamp естественен (зеркало Rust `[..n.min(len)]`),
  не сюрпризит «не больше n».
- **Mut-view через receiver-mut overload.** `mut @as_slice()` выбирается на
  `mut`-bound получателе и даёт write-through view; имя — `as_slice` (overload по
  получателю, как `@as_ptr`/`mut @as_ptr`), **НЕ** `as_mut_slice` (D247 / Plan 135
  accessor-конвенция).
- **Detach-on-resize (D238/Plan 96, Go-модель GC-safe).** View имеет `cap == len`,
  поэтому первая *реаллоцирующая* мутация (`push`/`reserve`/`insert` при `cap==len`)
  уезжает в свежий буфер и view **молча детачится** — родительский backing не
  перезаписывается (Go shared-backing footgun устранён без borrow-checker).
  До точки detach mut-view пишет сквозь в родителя. Точка detach предсказуема
  через точную ёмкость (`with_capacity`/`@cap(n)`, 153.1 — без pow2-округления).

### Почему

- **Никакого нового типа.** `[]T ≡ Vec[T]` (D239) + `Index[Range, Vec[T]]` (D238)
  уже выражают slice; named slice-методы — лишь эргономика поверх той же модели.
  Отдельный `Slice[T]` потребовал бы borrow-lifetime инфраструктуры, которой в
  Nova нет и не нужно (detach-on-resize заменяет borrow-checker).
- **Eager без внешней аллокации для view-заголовков, lazy для chunks/windows.**
  `split_at`/`first_n`/… возвращают view'ы БЕЗ аллокации (просто заголовки), их
  отдаём eager. `chunks`/`windows` в eager-форме аллоцировали бы `[][]T`-Vec — это
  расходится с ленивым каноном (Q-iterator-laziness), поэтому они **ленивые**
  итераторы `-> BoxIter[Self]` поверх инфры Plan 153.2 (Plan 153.4-B, реализованы
  в `std/collections/vec_lazy.nv`): `collect()` материализует `[][]T` только по
  требованию, `chunks(n).map/fold/count/for_each` — без внешней аллокации вовсе.

### Что отвергнуто

- **Отдельный `Slice[T]` / `&[T]` тип** — избыточен на single-type модели Plan 96;
  view это `Vec`-заголовок с `cap == len`.
- **`as_mut_slice` как отдельное имя** — мут-view = receiver-mut overload `@as_slice`
  (D247/Plan 135), одно имя, диспатч по mut-получателю.
- **Eager `chunks`/`windows` → `[][]T`** — аллоцировал бы внешний Vec, расходится
  с ленивым каноном; реализованы ленивыми `-> BoxIter[Self]` (Plan 153.4-B).

### Связь

- D238 — `Index[K, V]`; `v[a..b]` через `Index[Range, Vec[T]]` (та же view-модель)
- D239 — `[]T ≡ Vec[T]` (нет отдельного slice-типа)
- D247 / Plan 135 — accessor-конвенция: мут-вариант = receiver-mut overload (`mut @as_ptr`)
- Plan 96 — D-single-type (`[]T`-view того же типа) + D-cap-len + detach-on-resize
- Plan 153.4-A — eager view-заголовки (`std/collections/vec/views.nv`)
- Plan 153.4-B — ленивые `chunks`/`chunks_exact`/`rchunks`/`windows` `-> BoxIter[Self]`
  (`std/collections/vec_lazy.nv`, поверх Plan 153.2); `[M-153.4-chunks-windows-lazy]` ✅ CLOSED
- D260 / Plan 153.2 — `BoxIter[T]` ленивая итератор-инфра (база для chunks/windows)

Added: 2026-06-14  Status: ✅ IMPLEMENTED ЦЕЛИКОМ (Plan 153.4-A eager-views
`std/collections/vec/views.nv`, 2026-06-14; Plan 153.4-B lazy chunks/chunks_exact/
rchunks/windows `std/collections/vec_lazy.nv`, 2026-06-15)

---

## D249. Строковая координатная модель: линзы вместо плоских методов

Added: 2026-06-13  Status: IMPLEMENTED 2026-06-13 (Plan 152.1 Ф.1/Ф.4)

`str` хранится как UTF-8 `(ptr *ro u8, len int)` (Plan 139). Доступ — через
**линзы-представления**, не плоские методы; единицы координат — **байтовые**.

1. **`str` НЕ индексируется целым** — `s[i]` (int) → **`E_STR_NO_INT_INDEX`**
   (checker, `types/mod.rs` Index-арм). Fix-it: байт `s.as_bytes()[i]` (O(1)),
   codepoint через `for c in s.as_chars()` / `.indices()` (O(n); positional
   `as_chars().nth(i)` retracted — D260-амендмент, same O(n)-as-O(1) footgun),
   срез `s[a..b]`. Codepoint-index на UTF-8 =
   O(n)-ложь под видом O(1) → запрещён (прецедент Rust/Swift).
2. **`str[a..b]`** — единственный `str[..]`: **byte-range** zero-copy view
   (`Index[Range,str]`, `slice.nv`), O(1). Контракт `requires 0<=start &&
   start<=end && end<=byte_len()` (140.2-элидируемый) + рантайм-паника при рассечении
   codepoint-границы (R-UTF8). Дискриминация int-vs-Range — по типу индекса (Range-var
   тоже допускается). Codegen лоуэрит `s[a..b]` inline в `(nova_str){.ptr=…+from,.len}`.
3. **`@as_bytes() -> ro []u8`** — байтовый слой бесплатно (`Vec[u8]`-view, D239):
   `[i]`/`len()`/итерация O(1). `@byte_at(i)` ретайрнут → `as_bytes()[i]`.
4. **`@as_chars() -> CharsIter`** (D250) — codepoint-слой (поток, O(n)). Плоские
   `@char_at`/`@char_len`/`@get(int)` ретайрнуты → `for c in as_chars()` / `.count()`
   (positional `as_chars().nth(i)` — сам тоже ретрактирован, D260-амендмент).
5. **`@find`/`@rfind` → байт-offset** (композируется с `s[k..]` за O(1)).
6. **`for c in s` → `char`** (= `s.as_chars()`, D58 amend).
7. **Бэар `@len()` ретайрнут** → **`E_STR_NO_LEN`**. `@byte_len()` (O(1), читает
   priv-поле `@len`) — единственный length-метод на `str`; codepoint-длина —
   `as_chars().count()`. Поле `len` (storage) остаётся. Ретайрнутые с targeted
   `E_STR_NO_LEN`+fix-it (checker, НЕ-self receiver; self-`@len` field-read изъят).
8. **Инвариант R-UTF8** (D26 AMEND): значение `str` всегда валидный UTF-8 →
   `as_chars()`-decode тотален.

**Отвергнуто:** `str[i]→u8` (Go) / `str[i]→char` (Python, прежняя «школа B»);
индексация строки провоцирует `for i in 0..len`-антипаттерн O(n²). Дефолт-grapheme
(Swift) — Unicode-data в библиотеку (Plan 152.4). Q-string-indexing/Q-string-len —
закрыты этим (open-questions.md). Полный план — [Plan 152.1](../../docs/plans/152.1-coordinate-model-lenses.md).

---

## D250. `CharsIter` (codepoint-итератор) + конвенция `as_`/`to_`

Added: 2026-06-13  Status: IMPLEMENTED 2026-06-13 (Plan 152.1 Ф.3)

`as_chars()` — **decoding lens** (codepoint'ы вычисляются на проходе, не лежат
массивом), поэтому честный примитив — **итератор-поток**, не коллекция (View с
`at`/`len` → `for i in 0..len{at(i)}` = O(n²) footgun, нарушает «no hidden O(n)»).
Прецедент: Rust `str::chars()`.

```nova
type CharsIter value priv { buf str, pos int }   // borrows str; GC видит ptr через buf
```

| Метод | Смысл | Сложность |
|---|---|---|
| `mut @next() -> Option[char]` | декод codepoint в `buf[pos]`, `pos += step` (`Next[char]`) | аморт. O(1) |
| `@iter() -> CharsIter => @` | `Iter` (self-iterator, D58 `for c in s`) | O(1) |
| `@count() -> int` | число codepoint'ов от `pos` (терминатор) | O(n) |
| ~~`@nth(i int)`~~ | **⛔ ретрактирован (2026-07-06, решение владельца)** — это воссозданный запрещённый `s[i]` по codepoint (`E_STR_NO_INT_INDEX`): не-mut приёмник сканировал с нуля каждый вызов → O(n²) в циклах. Целевая итерация: `for-in` + `enumerate`/`skip`; миграция `[M-d73-d77-retraction-migration]`-волной | — |
| `@is_empty() -> bool` | `pos >= buf.byte_len()` | O(1) |

- **НЕ реализует `Index[int,char]`**, нет позиционных `at`/`len` (вариант C). На `str`
  нет `char_len`/`char_at` — это и есть минимализм. `CharsIter[i]` → CC-FAIL (не индексируем).
- **`str @as_chars() -> CharsIter`** (ленивый, borrows). **`str @iter() => @as_chars()`** →
  `for c in s` декодит codepoint'ы. codepoint-count = `as_chars().count()`; N-й элемент —
  через целевую итерацию (`for`/`.indices()`), не позиционный доступ (`.nth(i)` ретрактирован).
- **Конвенция:** `as_<repr>()` = линза/итератор-вью (borrows, zero-copy); `to_<repr>()` =
  owned копия (alloc). `as_bytes`/`as_chars` vs `to_bytes`/`to_chars`.
- **GC/lifetime:** `CharsIter.buf` (str) держит `ptr` → conservative GC видит буфер
  живым; `str` иммутабелен (R8).
- **Codegen-foundation (Plan 152.1 Ф.3):** value-record итератор — первый общий
  value-record с методами; потребовал фиксов value-record ABI в `emit_c.rs` (for-in
  `NovaValue_*` strip + by-pointer `&it` для `mut @next`; bare-`@` self deref для
  self-return; `nova_type_name_from_c`/recv-dispatch + `prepare_method_recv`) — заодно
  закрыл класс `[M-138.2-vec-self-return]` для value-records.

---

## D251. Полный str-surface (паритет Go/Rust/TS/Kotlin/Java)

Added: 2026-06-13  Status: IMPLEMENTED 2026-06-13 (Plan 152.2 Ф.1-Ф.3)

Прод-поверхность `str` поверх координатной модели D249/D250. **Все позиции/длины —
байтовые** (или явный char через линзу); поиск/нарезка — byte-offset; нарезка/trim/
strip/split возвращают **zero-copy** sub-views (`@[a..b]`).

- **split-семейство** (search.nv, byte-scan + zero-copy views): `@split(sep)`,
  `@splitn(n, sep)` / `@rsplitn(n, sep)` (≤ n частей, последняя = остаток; reverse для
  rsplit\*), `@rsplit(sep)` (справа, обратный порядок), `@split_once(sep)` /
  `@rsplit_once(sep)` → `Option[(str, str)]`, `@split_terminator(sep)` (без пустого
  хвоста), `@split_whitespace()` (ASCII-WS runs; non-ASCII → `[M-152-ws-unicode]`,
  152.4), `@lines()` (`\n`/`\r\n`, без терминатора).

  > **Амендмент 2026-07-11 (str.split → ленивый):** sep-семейство `@split`/`@splitn`/
  > `@split_terminator`/`@rsplit`/`@rsplitn` возвращают **ленивый итератор**
  > `SplitIter`/`RSplitIter` (семья `vec_iter`, zero-cost, паритет с Rust `str::split`)
  > вместо эагерного `[]str`: сегменты — те же zero-copy str-виды, выдаются по одному;
  > массив — `.collect()`. `@split_once`/`@rsplit_once` (→ `Option[(str,str)]`) и
  > `@split_whitespace`/`@lines` (эагерный `[]str`) — без изменений. Попутно исправлены
  > 2 codegen-бага (non-generic `Next[T]` в `@collect`); открытый хвост —
  > `[M-splititer-filter-closure-ctx-gap]`.

  > **Амендмент 2026-07-13 (196.5 Stage-D волна-3): `@until` / `@after`** — тотальная
  > пара к `@split_once`: `@until(sep) -> str` — подстрока ДО первого вхождения `sep`
  > (вся строка, если `sep` не найден); `@after(sep) -> str` — подстрока ПОСЛЕ первого
  > вхождения (`""`, если не найден). В отличие от `@split_once` (`None` при промахе)
  > обе тотальны — «нет разделителя» = «разбиение не произошло». Byte-offset,
  > zero-copy views (search.nv). Мотивация: examples/real_world/orm_decorators.nv
  > использовал их с заведения; компилятор падал P67-паникой (тест авторитетен —
  > метод добавлен в std, пример не переписан).
- **trim/strip** (transform.nv, **zero-copy** views): `@trim` / `@trim_start` /
  `@trim_end` (ASCII-WS ≤0x20; Unicode-WS — Phase B); `@trim_matches(c char)` /
  `@trim_start_matches` / `@trim_end_matches` (codepoint-pattern, multibyte-safe через
  `str.from(c)`); `@strip_prefix(p)` / `@strip_suffix(s)` → `Option[str]`.
- **search-доп** (search.nv): `@match_indices(needle) -> []int` (byte-offset'ы),
  `@matches(needle) -> []str` (views); `@is_char_boundary(idx) -> bool` (core.nv, O(1),
  Rust-parity — для безопасной нарезки без паники).
- **transform** (transform.nv): `@replace` (все), `@replacen(from, to, n)` (первые n),
  `@repeat`, `@pad_left` / `@pad_right` / `@pad_center` — **ширина в CODEPOINT'ах**
  (`as_chars().count()`, не байты: `"é".pad_left(3,'·') == "··é"`).
- **slice/owned-линзы:** `s[a..b]` (panic) / `@get(a..b) -> Option[str]` (None при
  OOB/не-границе) — byte-range, zero-copy (D249/slice.nv); `@to_bytes() -> []u8` /
  `@to_chars() -> []char` (owned, alloc).

Аллокации (для не-zero-copy путей: replace/repeat/pad) — через `_buffer`/`StringBuilder`
(Plan 152.0). codepoint-семантика поиска/split (прежняя школа B) переведена на байт.
Полный план — [Plan 152.2](../../docs/plans/152.2-string-surface-parity.md).

---

## D252. `char`-тип API — классификация / case / digit

Added: 2026-06-13  Status: 152.3a (ASCII) IMPLEMENTED 2026-06-13 (Plan 152.3a); 152.3b (Unicode) IMPLEMENTED 2026-06-15 (Plan 152.3b)

`char` (u32 codepoint) получает прод-API уровня Rust `char` / Java `Character` / Go
`unicode`. **Двухуровнево:** ASCII-core (в ядре, без таблиц) + Unicode-aware (делегат
в `std/unicode`, Phase B).

### 152.3a — ASCII-core (Фаза A, IMPLEMENTED)

В `std/runtime/defaults.nv` (рядом с `char @compare` — Nova-body методы на builtin
`char` эмитятся только из disk-loaded prelude-модуля, НЕ из embedded `char.nv`,
который парсится лишь для checker'а). Все предикаты — через char-сравнения (codepoint
order), ноль Unicode-таблиц; семантика == Rust `char::is_ascii_*`/`to_digit`:

- `@is_ascii` / `@is_ascii_digit` / `@is_ascii_alphabetic` / `@is_ascii_alphanumeric` /
  `@is_ascii_whitespace` (space\t\n\r\x0C, без \x0B) / `@is_ascii_uppercase` /
  `@is_ascii_lowercase` / `@is_ascii_hexdigit`.
- `@to_ascii_uppercase` / `@to_ascii_lowercase` (a-z↔A-Z через `char.try_from(@ as int ±32)`).
- `@to_digit(radix) -> Option[int]` (2..=36; None вне диапазона/не-цифра/значение≥radix).
- `@len_utf8() -> int` (1-4); `@encode_utf8() -> str` (= `str.from(@)`).

Ad-hoc `is_digit`/`is_hexdigit` (json.nv) консолидированы на эти методы.

### 152.3b — Unicode-aware (Фаза B, IMPLEMENTED — делегат в std.unicode)

**Opt-in** char-методы (доступны ТОЛЬКО под `import std.unicode`, как `str
@as_graphemes` — за размер таблиц платит импортирующий, НЕ prelude). Реализованы в
[`std/unicode/category.nv`](../../std/unicode/category.nv) char-receiver-обёртками над
code-point-предикатами того же модуля; 1:1 с UCD 16.0, семантика == Rust `char`:

- `@is_alphabetic` / `@is_numeric` / `@is_alphanumeric` / `@is_whitespace` /
  `@is_uppercase` / `@is_lowercase` / `@is_control` — бинарные предикаты.
- `@general_category() -> GeneralCategory` — UCD General_Category (TR44 Table 12),
  все 30 значений (`Lu|Ll|Lt|Lm|Lo|Mn|Mc|Me|Nd|Nl|No|Pc|Pd|Ps|Pe|Pi|Pf|Po|Sm|Sc|Sk|
  So|Zs|Zl|Zp|Cc|Cf|Cs|Co|Cn`); `Cn` (not assigned) — дефолт для code point'а вне
  `UnicodeData.txt`. Объявлен как `export type GeneralCategory` в `category.nv`.
- `@to_uppercase() -> str` / `@to_lowercase() -> str` — полное per-code-point
  case-mapping, возвращают **`str`** (НЕ один `char`): результат может быть
  multi-code-point (ß→`"SS"`, ﬁ→`"FI"`, İ→`"i"`+◌̇). `CharsView` НЕ существует; возврат
  `str` — финальное решение (см. ниже). Final_Sigma — string-level контекст-правило,
  поэтому одиночная Σ (U+03A3) lowercase'ится в σ (non-final form) — корректный
  context-free ответ. Переиспользует `upper_one`/`lower_one` из `case.nv` (152.4.4).

**Делегация (под капотом).** Предикаты делегируют в `std.unicode` code-point-функции
(`general_category`/`is_alphabetic`/`is_numeric`/…), которые читают новую таблицу
[`std/unicode/category_data.nv`](../../std/unicode/category_data.nv): General_Category
(`UnicodeData.txt` поле 2) + бинарные `Alphabetic` (DerivedCoreProperties.txt) и
`White_Space` (PropList.txt) из UCD 16.0. `is_numeric` выводится из GC (Nd|Nl|No, как
Rust), отдельной таблицы не требует. Таблица сгенерирована build-time-инструментом
`nova-codegen unicode` (range-таблицы binary-search, как `word_data.nv`); `--check` —
CI-guard. Char-методы остаются **вне prelude**: программа без `import std.unicode` их
не видит (pin'ит negative-фикстура).

**AMEND (Plan 159 Ф.4 / Plan 169.2.1, D308, 2026-06-19):** char-@методы фактически
доступны **без явного `import std.unicode`** — резолвер инжектит `std.unicode` в
entry-группу, увидев *method-call*-селектор `expr.is_alphabetic()` (тег `@method:`),
а Plan 159 Ф.1 DCE срезает неиспользуемые таблицы. **Free-функции** же
(`general_category(0x41)` и т.п.) остаются строго opt-in под `import std.unicode`
(`@method:`-тег различает форму вызова) — это и есть то, что pin'ит negative-фикстура
[`plan152_3/neg/n_char_unicode_opt_in.nv`](../../nova_tests/plan152_3/neg/n_char_unicode_opt_in.nv).
Plan 162 Ф.4 временно хостил @методы в `prelude.core` (через `core.nv import
std.unicode`); Plan 169.2.1 [D308](07-modules.md#d308) вернул их в `category.nv` и
восстановил resolver-инъекцию, чтобы `core` не тянул unicode (фикс частичного
`#prelude(core, …)`). См. [D308](07-modules.md#d308).

**`@to_uppercase()` возврат `str`, НЕ итератор.** Возможные формы возврата были `str`
(материализованный) vs `CharsView`/итератор (как Rust `char::to_uppercase()` →
`ToUppercase: Iterator<Item=char>`). Решение — **`str`**: (1) `CharsView` для char-case
в Nova не существует и не нужен (расширение 1–3 cp — крошечное), (2) симметрия со
string-level `to_uppercase(s) -> str` (case.nv), (3) `str` напрямую конкатенируется/
сравнивается без сбора. См. [Plan 152.3](../../docs/plans/152.3-char-type-api.md).

---

## D253. `std/unicode` — нормализация (UAX #15) + grapheme/word/sentence-сегментация (UAX #29) + case folding/mapping/title-casing

Added: 2026-06-14  Status: 152.4.1+152.4.2 (нормализация) + 152.4.3 (graphemes) + 152.4.4 (case folding/mapping) + 152.4.5 (word-сегментация + title-casing) + 152.4.6 (sentence-сегментация) IMPLEMENTED 2026-06-14; collation = D254 (152.5b DUCET, тоже IMPLEMENTED 2026-06-14)

Полная Unicode-нормализация — отдельный **opt-in** модуль `std/unicode` (модуль
`std.unicode`, folder-module), импортируется явно; НЕ prelude (за размер таблиц
платит импортирующий, criterion A6). Ядро (152.1–152.3a: байты/codepoint/ASCII)
остаётся prelude-доступным и собирается без `std/unicode`.

### Что в ядре vs библиотеке
Ядро: байты, codepoint decode/encode, ASCII case/classification, byte-`Compare`.
`std/unicode`: нормализация + grapheme/word/sentence-сегментация + case
folding/mapping/title-casing (здесь, реализовано); collation (152.5b) — Phase B.

### API (152.4.2, реализовано)
Free-function форма (D253): **`normalize_nfc/nfd/nfkc/nfkd(s str) -> str`**.
- **NFD** = canonical decomposition + canonical ordering (стабильная сортировка
  combining-марок по CCC, starter'ы — барьеры).
- **NFC** = NFD + canonical composition (blocking-rule UAX #15).
- **NFKD/NFKC** = compatibility decomposition вместо canonical.
Hangul декомпозируется/композируется **алгоритмически** (UAX #15 §Hangul, L+V и
LV+T), не по таблицам.

### Данные (152.4.1, Q-unicode-data)
Таблицы генерируются build-time-инструментом **`nova-codegen unicode`** из UCD
(`UnicodeData.txt`, `CompositionExclusions.txt`, `DerivedNormalizationProps.txt`) в
`std/unicode/norm_data.nv` — компактные `;`-строки (NFD/NFKD full decomp, CCC,
canonical composition), пин к `UNICODE_VERSION` (16.0). Парсятся **лениво** в
`HashMap` при первом вызове (module-level `ro` lazy-static, D199). Composition-ключ —
упакованный int `(a<<21)|b` (tuple-key HashMap — codegen-gap). Без ICU/ОС;
`--check` — CI-guard. Прецедент: Rust `unicode-*` (codegen), Go `maketables`.

### API grapheme-сегментации (152.4.3, реализовано)
Третья линза `str.@as_graphemes() -> GraphemesIter` (симметрична
`as_bytes`/`as_chars`): extended grapheme clusters (UAX #29) — то, что человек
видит как один символ (é = e+◌́; 🇺🇸 = 2 RI; 👨‍👩‍👧 = ZWJ-emoji — каждый 1 grapheme).
`GraphemesIter` (`value priv {buf,pos}`, как `CharsIter`) реализует `Next[str]`
(`mut next() -> Option[str]`, срез-кластер) + `iter`/`count`/`is_empty`; decoding
lens (поток, O(n) `count`, нет позиционного кэша — I2). Правила GB1-GB13 **+ GB9c**
(Indic Conjunct Break, U15.1) над Grapheme_Cluster_Break + Extended_Pictographic +
Indic_Conjunct_Break (range-таблицы binary-search, `grapheme_data.nv` из
GraphemeBreakProperty.txt + emoji-data.txt + DerivedCoreProperties.txt). Hangul
L/V/T — через GCB-категории.

### API case folding + case mapping (152.4.4, реализовано)
Free-function форма: **`fold_case(s) -> str`**, **`to_uppercase(s) -> str`**,
**`to_lowercase(s) -> str`** — все **locale-independent** (D253: без локали).
- **`fold_case`** — полное case folding (CaseFolding.txt status C+F) для caseless
  matching (Σ/σ/ς→σ, ß→"ss", K-Kelvin→k). Идемпотентно. НЕ нормализация —
  для полного caseless-сравнения канонически-эквивалентного текста сначала
  normalize (UAX #15), затем fold.
- **`to_uppercase`/`to_lowercase`** — полное Unicode case mapping, multi-codepoint
  (ß→SS, ﬁ→FI, İ→"i"+◌̇). SpecialCasing.txt unconditional + UnicodeData simple.
- **Final_Sigma** — единственное context-правило языко-нейтрального подмножества:
  Σ→ς в конце слова (preceded-by-cased & not-followed-by-cased, скип
  Case_Ignorable), иначе σ. Реализовано в `case.nv` через `Cased`/`Case_Ignorable`
  range-таблицы.
- **Исключено (по дизайну, не упрощение):** locale tailoring (tr/az/lt
  SpecialCasing + Turkic fold status T); title-casing (нужны UAX #29 word
  boundaries — `[M-152-word-boundaries]`).

### Данные case (152.4.4)
`std/unicode/case_data.nv` (`nova-codegen unicode` из CaseFolding.txt +
SpecialCasing.txt + UnicodeData.txt[12,13,14] + DerivedCoreProperties.txt
Cased/Case_Ignorable): FOLD/LOWER/UPPER/TITLE maps (`cp:m1,m2;..`) +
CASED/CASE_IGNORABLE ranges. Ленивые `ro` HashMap/range-таблицы.

### API word-сегментация + title-casing (152.4.5, реализовано)
Четвёртая линза `str.@as_words() -> WordsIter`: UAX #29 word boundaries (WB1-WB16) —
итерирует ВСЕ сегменты (слова, пробелы, пунктуация), как видит человек. **O(1) на
создание** (forward state-machine, lazy — границы не материализуются; conditional
lookahead для WB6/7b/12 только когда нужен). `WordsIter` (`value priv
{buf, pos, n, prev_imm_cat, prev_eff_cat, prev_prev_eff_cat, ri_run}`) реализует
`Next[str]` + `iter`/`count`/`is_empty`. Данные: `std/unicode/word_data.nv`
(WB-категории range-таблица из WordBreakProperty.txt;
Extended_Pictographic для WB3c переиспользуется из grapheme_data.nv).
**`to_titlecase(s) -> str`** (locale-independent): титулкейс первой cased-буквы каждого
слова (UAX #29) + lowercase остального (с Final_Sigma). Использует TITLE-маппинг
(не upper — ǆ→ǅ, не Ǆ).

### API sentence-сегментация (152.4.6, реализовано)
Пятая линза `str.@as_sentences() -> SentencesIter`: UAX #29 sentence boundaries
(SB1-SB11 + SB998) — итерирует сегменты-предложения (предложение + хвостовой
whitespace/терминатор). **O(1) на создание** (forward state-machine, lazy; SB8
forward-lookahead ограничен следующим blocker/Lower per-char, O(1) amortised).
`SentencesIter` (`value priv {buf, pos, n, prev_imm, term, phase, peb, ppeb, sb8_lower}`)
реализует `Next[str]` + `iter`/`count` (remaining, позиционный)/
`is_empty`. **Важно:** sentence-правила по умолчанию НЕ ломают (SB998 = ×) —
противоположно grapheme/word (по умолчанию ломают). Данные:
`std/unicode/sentence_data.nv` (SB-категории range-таблица из
SentenceBreakProperty.txt: 14 категорий CR/LF/Extend/Sep/Format/Sp/Lower/Upper/
OLetter/Numeric/ATerm/SContinue/STerm/Close).
- **SB6** ATerm × Numeric (`3.4` — одно предложение); **SB7** (Upper|Lower) ATerm ×
  Upper (`U.S.A`); **SB8** ATerm Close* Sp* × (¬…)* Lower (`resp. leaders` — строчная
  буква после `.` подавляет разрыв ⇒ одно предложение); **SB8a** (STerm|ATerm) Close*
  Sp* × (SContinue|STerm|ATerm); **SB9-SB11** — терминатор + Close*/Sp* + опц. ParaSep
  ⇒ разрыв.
- **По дизайну (не упрощение):** дефолтный UAX #29 без словаря аббревиатур —
  `Mr. Smith` ломается после `Mr.` (заглавная `S` ⇒ SB8 не срабатывает, SB11 ломает),
  как в эталонном `SentenceBreakTest.txt`. Это поведение UAX #29, не баг.

### Conformance
- UAX #15: `NormalizationTest.txt` — фикстура
  `normalization_conformance.nv` (полный 19965/19965 verified out-of-band; коммит — сэмпл).
- UAX #29: `GraphemeBreakTest.txt` — фикстура `grapheme_conformance.nv`
  (**полный 1093/1093**, content-checked).
- Case (152.4.4): `case_conformance.nv` — breadth по всем mapped codepoints
  (**полный 2981/2981 verified out-of-band**; коммит — uniform-spread 1500). UCD-derived
  expected → проверяет рантайм-**парсер+lookup+emission**, НЕ **выборку** (self-referential).
  Выборку (no-locale: Turkic-excl, field-index, 3-cp, Final_Sigma+case-ignorable, title≠upper)
  пиннит **independent hand-oracle** в `case.nv`/`words.nv`.
- Word (152.4.5): `word_conformance.nv` — UAX #29 из `WordBreakTest.txt` (**полный
  1826/1826 content-checked verified out-of-band**; коммит — uniform-spread 1500).
  **Independent** UCD oracle (boundaries заданы в тест-файле, не выводятся из генератора).
- Sentence (152.4.6): `sentence_conformance.nv` — UAX #29 из `SentenceBreakTest.txt`
  (**полный 512/512 content-checked**; коммит = весь тест-файл, т.к. < лимита 1500).
  **Independent** UCD oracle (boundaries заданы в тест-файле). Все —
  `nova-codegen unicode --emit-conformance`.

См. [Plan 152.4](../../docs/plans/152.4-std-unicode.md), Q-unicode-data
(open-questions.md). Phase B остаток: collation CLDR-tailoring + `eq_ignore_case`
(`[M-152-collation-tailoring]`); DUCET-collation (152.5b, D254) IMPLEMENTED.
`[M-152-case-fold]` + `[M-152-word-boundaries]` + `[M-152-sentence-boundaries]` ЗАКРЫТЫ.
AMEND D250 §«линзы» — `as_graphemes` 3-я, `as_words` 4-я, `as_sentences` 5-я линза.

**AMEND D253 V2 (Plan 91.18, 2026-06-19):**
- `*View` полностью переименованы в `*Iter`: `GraphemesView`→`GraphemesIter`,
  `WordsView`→`WordsIter`, `SentencesView`→`SentencesIter`. Везде единый суффикс Iter.
- `WordsIter` + `SentencesIter` стали ленивыми forward state-machine (O(1) create).
  `as_words()` / `as_sentences()` — O(1) конструкторы.
- `str @to_nfc/nfd/nfkc/nfkd()` добавлены как методы (тонкие делегаты);
  free-fn `normalize_*` остаются internal helpers.
- `str @fold_case()` / `@to_upper()` / `@to_lower()` / `@to_title()` добавлены как
  методы (под `import std.unicode`); bare `to_upper`/`to_lower` = Unicode (таблицы),
  `to_ascii_upper`/`to_ascii_lower` = ASCII-only (всегда доступны).
- `CharIndicesIter` добавлен в chars.nv: `s.as_chars().indices() -> CharIndicesIter`,
  `@next() -> Option[(int, char)]` — byte-offset + char.

---

## D254. Модель сравнения строк — byte-`Ord` дефолт + явный collation

Added: 2026-06-13  Status: 152.5a (core) IMPLEMENTED 2026-06-13; 152.5b DUCET-collation (UCA/UTS #10, Shifted + S2.1) IMPLEMENTED 2026-06-14; остаётся eq_ignore_case + CLDR-tailoring (`[M-152-collation-tailoring]`)

Дефолтное сравнение `str` — **byte-lexicographic** (быстрое, детерминированное,
locale-НЕзависимое, как Rust/Go). Locale-aware collation — отдельный явный opt-in
слой; `str` никогда не делает collation молча.

### 152.5a — Ядро (Фаза A, IMPLEMENTED)
- **`@compare(other) -> int`** (`Compare`/`Equal`/`Hash`) — byte-lexicographic (D178):
  memcmp-style на `@as_bytes()` + length-tiebreak. Дефолт `Ord`; сортировка
  детерминирована, locale-независима.
- **`@eq_ignore_ascii_case(other) -> bool`** (str + char) — ASCII case-insensitive eq
  (только A-Z/a-z складываются, БЕЗ Unicode-таблиц; длины должны совпадать). Rust
  `str/char::eq_ignore_ascii_case`. Не-ASCII (`é`/`É`) — байт-различны (не фолдятся).

### 152.5b — Unicode collation (IMPLEMENTED 2026-06-14) + locale (Phase B)
- **DUCET collation (UCA/UTS #10) — `std/unicode/collate.nv`, opt-in:**
  `collate_compare(a,b)->int`, `collate_sort_key(s)->Vec[u32]`, `collate_eq`,
  `Collator.order`/`Collator.key`/`Collator.same` (bodyless namespace — DUCET stateless,
  amend 91.18 2026-06-19: было instance `Collator.root().@order`). Алгоритм: NFD → collation elements
  (longest-match contractions **+ S2.1 discontiguous** + implicit-веса) → multi-level
  sort key (**Shifted** variable-weighting) → лексикографическое сравнение.
  `collate_data.nv` из DUCET `allkeys.txt` (`nova-codegen unicode`). Conformance:
  `CollationTest_SHIFTED.txt` (independent oracle, 50000/50000 spread out-of-band;
  коммит 1500; полный — slow-lane Plan 156). НЕ prelude — `str` Ord остаётся byte-lex.
- **eq_ignore_case** (тонкий враппер над `fold_case`) + **CLDR locale-tailoring** —
  остаются Phase B → `[M-152-collation-tailoring]`. Аналог: Rust `unicode-collation`
  DUCET-mode / ICU root collator (без tailoring).

**AMEND D254 V2 (Plan 91.18, 2026-06-19):**
- `collate_sort_key` возвращает `Vec[u32]` (не `[]int` как было в doc — код всегда
  был правильным, исправлена только документация).
- `Collator.strength` удалён как dead field (писался только в `root()`, никогда не
  читался). `Collator` хранит `_tag int` type-private placeholder для future
  tailoring. `STRENGTH_QUATERNARY` константа удалена.
- **split("") policy (отменяет Plan 91.15 Ф.4):** пустой sep/паттерн = определённый
  край (не паника) во всём search/split/replace surface. `"hi".split("")==["hi"]`,
  `find("")==Some(0)`, `rfind("")==Some(2)`, `contains("")==true`. Единое правило
  «пустой паттерн — определённый результат» заменяет «пустой паттерн → паника».
- Collation intentionally uses free-fn + Collator (not str @compare/@equal) — D254
  design: `str` никогда не делает collation молча; asymmetry is by design.

### D-R4 ✅ DONE (2026-06-13) — декомиссия str-operator C-lowering
`==`/`!=`/`<`/`<=`/`>`/`>=`/`+` для `str` теперь синтезируются из Nova-body методов
(`emit_c.rs` nova_str BinOp arm): `==`/`!=` → `Nova_str_method_eq`, `<`/`<=`/`>`/`>=`
→ `Nova_str_method_compare(l,r) OP 0`, `+` → `Nova_str_method_concat`. Хардкод C
`nova_str_lt`/`le`/`gt`/`ge` в operator-lowering снят; реестр str (`runtime_registry.rs`
+ `str_method_to_rt`) теперь содержит **только `@hash`** (SipHash crypto-seed,
неустраним). Method-form `s.eq(t)`/`s.lt(t)` falls through к тем же Nova-body.
- **Nova-body на RawMem-примитивах:** `@eq` = length-first + `RawMem.compare` (memcmp);
  `@compare` = `RawMem.compare` + length-tiebreak; `@concat` = `with_capacity` + 2×
  `Vec.append` (memcpy) + steal. Те же примитивы, что и в снятых C-функциях.
- **Бенч-паритет подтверждён:** 5M сравнений + 200k concat — before (C-операторы,
  ae465fce) 23.5–28.2s vs after (Nova-body) 22.6–29.2s; дельта в пределах
  compile-шума (after-лучший быстрее before-лучшего) → регрессии нет.
- **Остаток C (codegen-внутренние, НЕ операторы):** `nova_str_eq` — структурная eq
  полей в `emit_field_eq` (HashMap-ключи/record-`==`); `nova_str_concat` — cancel-marker
  ScopeOutcome + concat-аккумулятор. Это codegen-примитивы, не user-facing str-операторы;
  reroute создал бы method-emission reachability-связь в concurrency/collections codegen
  без пользовательской выгоды.
- **0 регрессий** (str/plan137/plan152_*/plan91_13/plan131/plan138 + reachability-проба
  чистых операторов). Закрывает `[M-139.1-operator-lowered-methods]`.

См. [Plan 152.5](../../docs/plans/152.5-comparison-collation.md), Q-string-collation (open-questions.md).

---

## D255. UTF-16 / code-point interop

Added: 2026-06-13  Status: IMPLEMENTED 2026-06-13 (Plan 152.6)

Для FFI/JSON/протоколов (Windows API, JS-interop, JSON `\uXXXX`) нужны конверсии в/из
UTF-16 и доступ к сырым int-codepoint'ам. Дополняет байтовый слой
(`from_bytes_*`/`to_bytes`/`as_bytes`) и codepoint-линзу (`as_chars`). Размещено в
`std/encoding/utf16` (модуль `encoding.utf16`) — **импортируется явно**, НЕ prelude
(FFI/протокол-концерн). `str`-методы — тонкие делегаты в этот модуль (резолвятся через
dot-call при `import std.encoding.utf16`).

### API
- **`str @encode_utf16() -> []u16`** — UTF-16 code units. BMP-codepoint → один `u16`;
  supplementary (`cp > 0xFFFF`) → surrogate-пара (два `u16`). Аналог Rust
  `str::encode_utf16`.
- **`str.from_utf16(units []u16) -> Result[str, Utf16Error]`** — checked decode.
  Валидирует surrogate-пары; lone high/low или усечённая пара → `Err`. На выходе —
  валидный UTF-8 (R-UTF8, D26 AMEND). Двухпроходно: валидация в `[]int`, затем
  построение через `StringBuilder` (consume один раз на успешном пути).
- **`str @code_points() -> []int`** — сырые int-codepoint'ы (без `char`-обёртки), для
  низкоуровневых протоколов/таблиц. Значения == `as_chars()` как int.
- **`Utf16Error`** | `LoneHighSurrogate(int)` | `LoneLowSurrogate(int)` |
  `TruncatedSurrogatePair(int)` — хранит проблемный code unit.
- **surrogate-помощники** (свободные fn, контракты Plan 140): `is_high_surrogate`,
  `is_low_surrogate`, `decode_surrogate_pair(hi, lo)`
  (`requires is_high_surrogate(hi) && is_low_surrogate(lo)`,
  `ensures result ∈ [0x10000, 0x10FFFF]`).

Roundtrip-инвариант: `from_utf16(s.encode_utf16()) == Ok(s)` на ASCII/BMP/supplementary.

См. [Plan 152.6](../../docs/plans/152.6-utf16-encoding-interop.md).

---

## D258. Формат-спеки в интерполяции — Rust-style mini-language

Added: 2026-06-14  Status: B1 (format mini-language) IMPLEMENTED 2026-06-14 (Plan 152.7-B); B2 (Write-sink обобщение) — отложено в 152.7.1 (breaking). AMEND D229/D44/D183.

Расширяет интерполяцию `${expr:SPEC}` (база — D229, Plan 91.14, поддерживала только
`:?` Debug) до полного **Rust-style** mini-language. Синтаксис `${...}` не меняется.

### Грамматика спека (после `:`)
`[[fill]align][sign][#][0][width][.precision][type]`
- **align** `<` (left) / `^` (center) / `>` (right); опц. **fill**-char перед align
  (`${x:*^10}` → центрирование с заполнением `*`). Дефолт-выравнивание: числа вправо,
  прочее влево.
- **sign** `+` — всегда печатать знак для чисел (`${n:+}`).
- **`#`** alternate — radix-префикс для целых (`0x`/`0o`/`0b`). Префикс **всегда
  строчный** даже для `:#X` → `0xFF` (как Rust; `upper` влияет только на цифры).
- **`0`** zero-pad — нули между знаком/префиксом и цифрами (sign-aware: `${n:08x}`,
  `${n:#08x}` → `0x0000ff`).
- **width** — мин. ширина поля (в Unicode-скаляр-ширине).
- **`.precision`** — для f64 = число знаков после точки; для str = усечение.
- **type** — `x`/`X` (hex), `b` (binary), `o` (octal), `?` (Debug, из D229). Дефолт —
  Display.

Семантика выровнена с Rust `format!`. Прецедент: Rust mini-language, Python f-string.

### Диагностика (compile-time)
- `E_FORMAT_SPEC_UNKNOWN` — неизвестный type-char (с fix-it: список поддерживаемых).
- `E_BAD_FORMAT_SPEC` — структурно невалидный спек (напр. `.` без цифр precision).
- `E_FORMAT_SPEC_EMPTY` / `E_FORMAT_SPEC_TRAILING` — из D229 (пустой спек / мусор).
Невалидный спек — **всегда compile-error**, никогда не молчаливый pass.

### Реализация
AST `FormatSpec` (`compiler-codegen/src/ast/format_spec.rs`, расширен от Plan 91.14) +
парсер (`parser/mod.rs`) + codegen (`emit_c.rs`) → runtime-хелперы
`compiler-codegen/nova_rt/conv.h` (`nova_fmt_int_body`/`_radix_body`/`_pad`/
`_radix_prefix`/`_f64_body` + sign/prefix). Целые — `nova_int` (64-bit), negative hex =
two's-complement (`${-1:x}` → `ffffffffffffffff`, как Rust).

### ~~Отложено~~ B2 — ✅ ЗАКРЫТО (2026-06-16, [D374](02-types.md#d374-write-sink-протокол--декаплинг-displaydebug-от-stringbuilder-plan-15271))
Обобщение `@display(mut sb StringBuilder)` → `@display(mut w Write)` выполнено Plan 152.7.1:
sink-протокол `Write`, сигнатуры Display/Debug переломлены (breaking, сделан).

См. [Plan 152.7](../../docs/plans/152.7-interpolation-formatting.md),
[D229](02-types.md#d229-Debug-protocol--format-spec-expr).

---


---

## D314. Единое ядро cleanup — `defer` как примитив (defer-kernel)

> **Plan 173 Ф.2 (defer-kernel unification). Статус:** 🔨 SPEC-FIRST (2026-07-04, написан ДО codegen;
> реализация — под-атомы Ф.2 (де-риск-карта: чекпоинт волны удалён при закрытии, см. git-историю)).
> **Закрывает** несведённость трёх cleanup-поверхностей (`defer` / `Consumable.on_exit`+`consume{}` /
> `with Fail[E]`) из двух непримирённых эпох (Plan 173 §1). **Модель:** MODEL 1 «`defer` — ядро»
> (sign-off 2026-06-20). **Нормативы:** [§3a](../../docs/plans/173-error-system-unify-harden.md)
> (completes-by-default, D192-ретракт) + §3b (no-restart-default). **Амендит:** [D90](#d90-defer-и-errdefer--scope-level-cleanup-statement),
> [D188](#d188-cleanupe-protocol--consume-x--expr-body-scope-block), [D189](#d189-прямое-удаление-okdefer--errdefer--defer-result),
> [D194](#d194-consumablenever--infallible-cleanup--hot-path-elision), [D185](../04-effects.md#d185)
> (эффект `Cleanup`→`ResourceTrace`). **Хаб:** docs/dev/idioms/error-and-cleanup-model.md (rewrite Ф.2.E).

### Что

Nova имел ТРИ несведённые cleanup-поверхности. D314 сводит их к ОДНОМУ примитиву — `defer` — с
опциональным outcome-биндингом; протокол `Cleanup[E]` (ex-`Consumable`) / `consume` / `@cleanup`
(ex-`@on_exit`) низводятся до **сахара** над outcome-defer; весь unwind-ре-диспатч идёт через ОДНУ
runtime-точку (`nova_scope_exit`).

### Правило

**1. `defer body` (форма без изменений)** — безусловный cleanup на ЛЮБОМ exit из enclosing scope
(normal / `return` / `throw` / `panic` / `interrupt`), кроме `exit(N)`. LIFO. Добегание — §3a
(completes-by-default: щит элидится для sync-тел, cancel замаскирован для suspend-тел).

**2. `defer(o ScopeOutcome) { … }` (НОВОЕ)** — outcome-несущий block-defer; тело получает исход
`ScopeOutcome` ([core.nv](../../std/prelude/core.nv)). Отображение exit-path → outcome:

| Exit-path | `ScopeOutcome` |
|---|---|
| normal end-of-scope / `return v` | `Success` |
| `throw e` / typed-throw | `Failure(reason)` |
| `cancel` (structural) | `Failure("cancel: " + reason)` (D90 §7 marker) |
| `panic(msg)` | `Panic(msg)` |
| `interrupt v` | `Failure(reason)` (спека core.nv:130; выравнивает impl — consume-монолит СЕЙЧАС обходил on_exit на interrupt, defer-frame его оборачивает) |
| `break` / `continue` (loop-body exit, [D432](02-types.md#d432) §6 Plan 217) | `Success` — не несут throw/panic-исход; тело итерации завершается штатно, cleanup бежит с тем же исходом, что normal end-of-scope |

Субсумирует три ретрактнутые формы (D189): `errdefer{…}` ≡ `defer(o){ match o { Failure(_)|Panic(_) => …, Success => () } }`;
`okdefer{…}` ≡ `defer(o){ match o { Success => …, _ => () } }`; `defer |r| {…}` ≡ эта форма.
**Zig-парность:** `errdefer |e| {…}` ≡ `defer(o){ match o { Failure(e) => use(e), _ => () } }` — payload
`e` типизирован; ветка `Panic` различима И ВЫПОЛНЯЕТСЯ (Zig defer при panic не бежит). **AST:** опц.
поле на `Stmt::Defer` (не новый вариант — low blast). **Parser:** bounded lookahead `defer (IDENT
ScopeOutcome)` с fallback на `parse_expr` для `defer (expr)`; double-binding / non-`ScopeOutcome` тип → диаг.

**3. `consume X = e { body }` → САХАР** над outcome-defer:
`{ ro X = e; defer(o) { X.@cleanup(o) }; body }`. Протокол `Cleanup[E]` (ex-`Consumable`) остаётся как
protocol-сахар (тип инкапсулирует cleanup через `@cleanup(o ScopeOutcome)`, ex-`@on_exit`). Разница
`consume` vs bare `defer(o)` — **must-consume (D133) + exactly-once (D188 R2) + partial-init (D188 R1)
+ ResourceTrace-события**, НЕ «добежит/не добежит» (§3a). Consume-специфичная policy (cancel-shield
поверх cleanup-фазы ТОЛЬКО — **R3d амендмент**, ранее ошибочно «body+cleanup»; 3-level timeout,
ResourceTrace enter/exit) re-home'ится на **consume-flavored defer-entry**, не теряется при десугаре.

**4. Централизованный ре-диспатч (`nova_scope_exit`)** — ОДИН runtime-helper
`nova_scope_exit(NovaFailFrame* primary, NovaScopeExitPolicy policy)`, `policy ∈ {CATCH, TRANSPARENT}`;
читает `primary->error_kind`. **РЕАЛИЗОВАН (Ф.2.C, effects.h `static inline` рядом с триадой).** Таблица
**приведена к факту** (прежняя `PANIC→nv_panic` / `CANCEL→nova_throw_cancel_reason` устарела — оба сайта
фактически несли suppressed-chain через `nova_rethrow_with_suppressed`, а голый `nv_panic` его теряет;
§5-согласование выбрало канон, СОХРАНЯЮЩИЙ chain + reason):

| `error_kind` | Транспорт |
|---|---|
| `PANIC` | `nova_rethrow_with_suppressed(primary)` — kind=PANIC, suppressed-chain СОХРАНЁН (матчит post-B3-merge defer-kernel; голый `nv_panic` терял бы chain) |
| `CANCEL` | `nova_rethrow_with_suppressed(primary)` — `error_reason_ptr` И chain СОХРАНЕНЫ (rethrow копирует `frame->error_reason_ptr`; эмпирически подтверждено §5, унифицирует с PANIC) |
| `USER` / `USER_TYPED` | `CATCH` → return-to-caller (handler отработал; вызывающий ставит result=default; **with-Fail**); `TRANSPARENT` → `nova_rethrow_with_suppressed` (**defer/consume**) |
| `Success` | no-op (sentinel; недостижим — frame инспектируется только после throw, у `NovaThrowKind` нет success-значения) |

Класс бага «кадр забыл kind» (дефект #1) исчезает **по построению** — единственная точка политики.
**IN** (роутятся через helper): with-Fail terminal (CATCH; site-пролог `pop(fail)+restore(handlers)+
pop(interrupt)` ПЕРЕД helper, флаг `caught` заменил per-kind dispatch), defer-kernel FAIL run-site
(TRANSPARENT), consume-flavored FAIL run-site (TRANSPARENT; consume физически слит в defer-kernel —
B3-merge, отдельного consume-терминала НЕТ). **OUT** (report/log-семьи, НЕ throw-транспорт; читают
`error_kind` как ПАРАМЕТР, не как policy-диспатч): fiber-report spawn-worker (`nova_fiber_report_*_kinded`
— форвардит kind в parent-scope queue; ре-диспатч на main flow через `supervised_run` first_error_kind),
detach LogAndDrop (D50 — log-to-stderr, не throw), test-frame. **Compose ОСТАЁТСЯ в codegen** (helper —
только single-frame transport): `nv_compose_suppressed`/suppressed-chain build (D158), panic-dominance
`emit_fail_cleanup_compose` (§4a), defer normal-exit cleanup-fail hand-rolled `longjmp` (переносит
compose-state `comp_msg/comp_kind/...` — НЕ single frame, поэтому helper к нему НЕ применим), а также
outcome-материализация `ScopeOutcome` из frame (`assign_scope_outcome_from_frame` — строит user-visible
`o`, не transport). grep-guard (пройден): `error_kind ==` только внутри `nova_scope_exit` +
санкционированных compose/outcome-сайтов.

**4a. Единое правило composition — panic-dominance (аменд Ф.2, 2026-07-04).** Поскольку `consume` — САХАР
над `defer(o)` (§3), их compose ОБЯЗАН совпадать (иначе десугар лжёт). Единая таблица (тело упало И
cleanup упал), выводимая из [D13](08-runtime.md#d13) (panic = abort-class баг, строго СРОЧНЕЕ recoverable
throw) + [D158](#d158)/[D161](#d161) (throw-composition) + [D196](#d196) R3:

| body \ cleanup | cleanup Success | cleanup **throw** (USER/CANCEL) | cleanup **PANIC** |
|---|---|---|---|
| **Success** | — | cleanup throw = primary | cleanup panic propagates |
| **throw** | body throw = primary | body=primary, cleanup=suppressed ([D158]) | **cleanup PANIC доминирует** (nv_panic, body-throw подавлен) |
| **panic** ([D196] R3) | body panic доминирует | body panic доминирует, cleanup подавлен | body panic доминирует |

**Ключевой пункт (устраняет расхождение impl):** `body-throw + cleanup-PANIC` → **panic доминирует**
(D13: баг важнее recoverable-ошибки). Монолит `consume` это УЖЕ делал; defer-kernel же ошибочно клал
cleanup-panic в suppressed-chain как обычный throw (chain-compose не различал PANIC-kind) — это
**непреднамеренный гэп defer**, НЕ намеренное поведение. Унификация: defer-kernel тоже даёт
cleanup-PANIC доминировать. Это делает десугар `consume`→`defer(o)` **behavior-preserving** (обе
поверхности — одно правило) и разблокирует физическое слияние (B3-merge). Blast-radius: только
двойной-fault `body-throw + cleanup-panic` (редкий; cleanup-panic сам = баг) — мигрируется в том же
изменении (§7).

**5. Hot-path (D194 амендмент — ПРЕМИСА ИСПРАВЛЕНА, §3.5):** прежняя формулировка «`Consumable[Never]`
СЕЙЧАС элидит shield/timeout/outcome (disasm-verified T2.9)» **не соответствует коду** — ConsumeScope
эмитит полный frame-bearing путь безусловно (§perf-элизия НЕ реализована, Plan 110:695). Поэтому Ф.2
acceptance = **PARITY** (lowered consume/defer(o) даёт disasm ≡ текущему; НЕ регрессировать). Генуинная
§perf-элизия (ключ «sync-тело + cleanup `Fail[Never]` → прямой вызов без кадра») — ОТДЕЛЬНЫЙ followup
`[M-173-d194-perf-elision]`, вне периметра unification. D194-спека приводится к факту (без «disasm-verified»
без живого артефакта). Disasm-guard targets (byte-identical): `Mutex`/`Semaphore`/atomic guards.

### Renames (нормативны; порядок ОБЯЗАТЕЛЕН — коллизия имён)

Протокол `Cleanup[E]` и эффект `Cleanup` НЕ сосуществуют в prelude (один type-name namespace) →
duplicate-def ломает ВЕСЬ prelude. Порядок: **(1) эффект `Cleanup`→`ResourceTrace`** (D185; ops
`on_resource_enter(label)` / `on_resource_exit(label, outcome)`, `timeout` из enter УБРАН) — ПЕРВЫМ,
освобождает имя. **(2) протокол `Consumable[E]`→`Cleanup[E]`** + **(3) метод `@on_exit`→`@cleanup(o
ScopeOutcome)`** — вместе (после 1). ⚠ `CleanupTimeoutError` (errors.nv) — ДРУГОЕ имя (D192-scope, Ф.5),
НЕ трогать sed'ом.

### Миграция (errdefer/okdefer → defer(o))

| Старое (ретракт D189) | Новое (D314) |
|---|---|
| `errdefer { rollback() }` | `defer(o){ match o { Failure(_) \| Panic(_) => rollback(), Success => () } }` |
| `okdefer { commit() }` | `defer(o){ match o { Success => commit(), _ => () } }` |
| `errdefer \|e\| { log(e) }` | `defer(o){ match o { Failure(e) => log(e), _ => () } }` |
| `defer \|r\| { … }` | `defer(o){ … }` |

### НЕ-цели

- call-site try-маркер (видимость эффекта — на уровне сигнатуры); HOF-эффект-полиморфизм (Swift
  `rethrows`) — вне периметра; `ScopeOutcome.Failure(any)` типизированный payload — **✅ реализован Ф.4 #5
  (2026-07-06)**: cleanup/`defer(o)` восстанавливает причину через `if err is T` (D54/174.3), cancel — как
  `Failure(CancelError{reason})`; идентичность ошибки на interrupt-path переживает разрушенный stack-fail-frame
  через thread-local snapshot `_nova_last_error`. Генуинная §perf-элизия — followup
  `[M-173-d194-perf-elision]`. Идиома `Fail[E]→Result`: `with Fail[E] = |e| interrupt Err(e) { Ok(body) }`
  (std-сахар D314 не вводит).

## D410. Ретракция `as_*`/`to_*`-близнецов: голые имена-виды, копия на месте вызова (2026-07-06)

> **Status:** accepted (решение владельца, 2026-07-06). Реализация — миграционная волна
> `[M-d410-as-to-migration]` (~380 мест) после волны []T/cap/http-props.

### Что

Словарь префикса `as_` **упраздняется**. Оси именования конверсий/доступов:

| Форма | Смысл | Примеры |
|---|---|---|
| голое существительное | O(1) вид/линза: заём, zero-copy, лениво | `bytes()`, `chars()`, `graphemes()`, `words()`, `sentences()`, `slice()`, `ptr()` |
| `.clone()` / `.collect()` на месте вызова | явная копия/материализация вида | `s.bytes().clone()`, `s.chars().collect()` |
| `to_*` | **трансформация** в новое владеющее значение — операция, у которой вида не существует в принципе | `to_upper()`, `to_lower()`, `to_str()`, `to_ascii_upper()` |
| `into_*` | потребляющий финализатор (ось владения, без изменений) | `into_str()`, `into_raw()` |

**Удаляются близнецы-копии** (их смысл — `вид + clone`): `to_bytes` (≡ `bytes().clone()`),
`to_chars` (≡ `chars().collect()`). **Переименовываются виды**: `as_bytes`→`bytes`,
`as_chars`→`chars`, `as_graphemes`→`graphemes`, `as_words`→`words`, `as_sentences`→`sentences`, `as_slice`→`slice`
(вкл. recv-mut перегрузку `mut @slice()`, D262-амендмент), `as_ptr`→`ptr` (ложится в канон
методов-свойств D117: `ptr` — читатель поля). `to_upper`/`to_str`-семейство и `into_*` — без
изменений.

### Почему

1. **Правило «два имени — выбери сам» не самообеспечивается**: в std `to_bytes` — 124 вызова
   против 67 у `as_bytes` — тяжёлая копия доминирует там, где хватило бы вида. Удаление
   близнецов делает неправильное **невыразимым** (дух нулевой толерантности, §4а
   compiler-conventions).
2. **`as_chars` был нашей девиацией от Rust-первоисточника**: в Rust C-CONV ленивые итераторы —
   голые существительные (`chars()`, `lines()`, `bytes()`), `as_` только у видов-реинтерпретаций.
3. **`[]T`-вид Nova и random-access, и итерируем напрямую** (D238/D239) — один `bytes()`
   покрывает раздельные в Rust `as_bytes` (вид), `bytes` (итератор) и `to_bytes` (через
   `.clone()`).
4. **Плата за копию видна в точке вызова** (`.clone()` читается глазами), а не спрятана в имени.

Прецеденты: Swift (голые виды `.utf8`, копия конструктором `Array(s.utf8)`) — целевая модель;
Rust C-CONV — источник старого правила (итераторы там уже без `as_`); C#/Kotlin
(`AsSpan`/`ToArray`, `asSequence`/`toList`) — таксономия, от которой уходим благодаря
`[]T`-модели. Правило §1 nv-coding-style переписано синхронно.

> **AMEND (2026-07-14, Plan 174.2):** канонический вход «скаляр/значение → строка» —
> ОДИН путь, `@to_str()`, через bare-T blanket (D145 форма, зеркалит
> `fn[T] T @identity() -> T => @`):
> ```nova
> fn[T] T @to_str() -> str => "${@}"
> ```
> («строка из значения = подставить значение в интерполяцию»). Статический
> `str.from(scalar)` (был bootstrap-путём D35, см. историю D70/D73/D77 выше)
> **ретрактирован** — `.to_str()` чейнится, `str.from(x)` нет, держать оба
> нарушало бы D9 (один канонический путь). Специализация — по D84 «concrete
> побеждает generic» + [D285](10-overloading.md#d285--receiver-compatibility-rule--blanket-dispatch-priority-plan-164-ф3):
> конкретный `T @to_str()` (напр. `[]u8 @to_str() -> Result[str, Utf8Error]` —
> UTF-8 decode, другая семантика/arity, НЕ `"${bytes}"`; `char @to_str()`)
> всегда побеждает blanket по receiver-типу. Рекурсия исключена: `"${@}"` для
> примитива лоуэрится напрямую в Display-хелпер (emit_c.rs
> `emit_interpolated_str` primitive dispatch: `nova_int_to_str`/`nova_f64_to_str`/…),
> НЕ через `.to_str()` повторно; для не-примитива — через `Display.@display`.
> Детали миграции — `[M-d410-str-from-retraction]`, `docs/plans/wip/174.2-scalar-to-str-notes.md`.

> **AMEND (2026-07-16, Plan 200 Step 2) — хвост `std/time/duration.nv` закрыт.**
> Последний оставшийся `as_*`-остров (`Duration`/`Timestamp`/`Monotonic` — не входил
> в исходную ~380-site волну `[M-d410-as-to-migration]`, т.к. `time.duration`
> компилировался до волны) мигрирован: `@as_nanos/micros/millis/secs/mins/hours/
> days()` → голые виды `@nanos/micros/millis/seconds/minutes/hours/days()`;
> `@as_secs_f64/millis_f64()` → `@seconds_f64/millis_f64()`; Timestamp
> `@as_unix_secs/millis/nanos()` → `@unix_seconds/unix_millis/unix_nanos()`
> (полное слово `seconds`, не `secs` — согласовано с общим правилом «единица
> полным словом»); Monotonic `@as_nanos()` → `@nanos()`. Одновременно ретрактированы
> per-width **конструкторы** `Duration.from_*`/`Timestamp.from_unix_*` и bare-fluent
> `int/f64 @seconds()` и т.п. (не виды, а трансформации `T → Duration`) — заменены
> единым `to_*`-бланкетом над `Ints` + `f64 @to_seconds()` (см. D317 амендмент
> ниже, `[M-primitive-receiver-bounded-blanket-dispatch]`-зависимость закрыта
> 196.8/196.9). Грепы `@as_` = 0 в `std/time/duration.nv`.

## D411. Record-деструктуризация в биндингах `ro`/`mut` (2026-07-07)

> **Status:** ✅ IMPLEMENTED 2026-07-07 (решение владельца, предложено на живом коде —
> паре `ro line = @line; ro col = @col` в json-лексере). Парсер переиспользует
> match'евскую грамматику record-паттернов без изменений (`parse_pattern` уже вызывался
> из `parse_ro_mut_binding`); irrefutable-проверка (Plan 53) и codegen-биндинг полей
> (`emit_record_destructure`) тоже уже существовали в конвейере — реализация D411 добавила
> НОВОЕ: `[E_REFUTABLE_BINDING]` код-тег на существующую refutability-диагностику,
> `[E_RECORD_PATTERN_NEEDS_REST]` — новая проверка «частичный список полей без `..`» (только
> для `ro`/`mut`-биндингов, не match/for), conformance-тесты, json-лексер как потребитель.
> `[M-d411-record-binding-destructuring]` ЗАКРЫТ.

### Что

Record-паттерн (уже существующий в `match`-ветках) разрешается в irrefutable-позиции
биндинга — симметрично уже принятой кортежной деструктуризации (`ro (a, b) = ...`):

```nova
ro {line, col, ..} = @            \ снапшот части полей приёмника (`..` обязателен)
ro {line, col} = tp               \ перечислены ВСЕ поля — `..` не нужен
mut {pos, ..} = lex               \ mut-вариант: каждое введённое имя — mut-биндинг
ro {line: l0, col: c0, ..} = @    \ переименование — та же match-грамматика
```

### Правило

- **Грамматика паттерна — ровно match'евская** (один паттерн-язык, ноль особых случаев):
  shorthand-поля, переименование `поле: имя`, `..` ОБЯЗАТЕЛЕН при частичном списке полей
  (честный сигнал «беру не всё»), без `..` — только полный список.
- **Позиция**: после `ro`/`mut` токен `{` однозначен; конфликтов с блоком (не бывает слева
  от `=`) и D55-коэрцией литералов (та — справа) нет.
- **Irrefutability**: паттерн обязан быть неопровержимым — записи и named-tuple да; варианты
  сумм/литералы — НЕТ (`[E_REFUTABLE_BINDING]`, для них `match`/pattern-bind `if`).
- **Семантика — pattern-native, БЕЗ отдельного AST-десугара**: `ro {a, b, ..} = e`
  концептуально ≡ `ro _tmp = e; ro a = _tmp.a; ro b = _tmp.b` (`e` вычисляется один раз;
  копия для value-полей, разделяемая ссылка для кучевых), НО реализовано не переписыванием
  AST в серию синтетических биндингов, а тем, что `Pattern::Record` остаётся первоклассным
  узлом `LetDecl.pattern` через весь конвейер: чекер понимает его напрямую
  (irrefutability/priv/`..`-правило — все три ходят по pattern, не по десугар-форме),
  кодоген (`emit_record_destructure`) сам эмитит `T tmp = <e>;` РОВНО один раз, затем
  биндит поля через `pattern_bind_typed`. Архитектурное решение (не с нуля для D411 —
  унаследовано от Plan 53's `let`-record-destructure, который эту инфраструктуру уже
  построил до retraction `let`→`ro`/`mut`): нет нужды выдумывать hygienic tmp-имена на
  уровне Nova-исходника, когда C-кодоген уже даёт свежий `tmp` на своём уровне.
- Вложенные паттерны — как в match (рекурсия по вложенным `Pattern::Record`-полям).
  Ограничение реализации: record-паттерн, вложенный ВНУТРИ tuple-элемента биндинга
  (`ro (a, {x, ..}) = pair`), не проходит `..`-правило (нет type-resolution на уровне
  tuple-элемента в текущем чекер-пути) — редкий кейс, вне примеров D411 (json-лексер,
  `TokenWithPos`-снапшоты), при необходимости — отдельный follow-up.

### Почему

Замыкает матрицу: паттерны записей уже есть (match), биндинги уже принимают паттерны
(кортежи) — отсутствовала одна клетка. Частый мотив «снапшот полей» (лексер, диспатчеры)
схлопывается в строку без новой концепции в языке.

## D412. Hex-блоб литерал `x"…"` → `[]u8` и интринсик `embed("path")` (2026-07-07)

**Статус: ✅ РЕАЛИЗОВАН** (Plan 186, 2026-07-09: лексер/AST/парсер/embed/эмиссия; фикстуры d412_hex_blob_embed.nv + d412_embed_fixture.bin; статус-строка обновлена 2026-07-26 по находке владельца на сайте — «Реализация в очереди» висела после исполнения). Принят владельцем 2026-07-07 («добро п.1, 2»); прежний хвост
(маркер [M-hex-blob-embed-d412] в docs/plans/backlog-followups.md).

### Решение

1. **Hex-строковый литерал** (форма D-стиля): `x"486900FF_486900FF"` — компайл-тайм
   последовательность байт, тип `[]u8`.
   - Внутри кавычек допустимы hex-цифры и разделители: `_`, пробел, перенос строки —
     разделители игнорируются (группировка на глаз читателя).
   - **Нечётное число цифр = ошибка компиляции** (E_HEX_BLOB_ODD). Не-hex символ =
     ошибка (E_HEX_BLOB_CHAR). Пустой `x""` легален → пустой `[]u8`.
   - Это НЕ числовой литерал: ведущие нули значимы (`x"00FF"` = 2 байта), порядок
     байт = порядок записи (эндианность машины не участвует).

2. **`embed("relative/path")`** — компайл-тайм интринсик: содержимое файла как `[]u8`.
   - Аргумент — ТОЛЬКО строковый литерал (путь известен на компиляции).
   - Путь разрешается **относительно файла-исходника** (.nv), где стоит вызов —
     модель Rust `include_bytes!`; выход из дерева проекта наружу (`..` выше корня) —
     ошибка. Файл не найден = ошибка компиляции (E_EMBED_NOT_FOUND).
   - Встроенный файл — зависимость сборки: участвует в fingerprint пересборки.
   - Интринсик класса C по природе (файловый ввод на этапе компиляции — как
     `include_bytes!`/`#embed`/`@embedFile`), конвенции §3 не противоречит.

### Материализация: указатель в статические данные

Оба источника эмитятся в C как `static const uint8_t nova_blob_<n>[] = {…}` в
секции статических данных (.rodata) — прецедент: str-литералы уже интернируются в
`static const nova_str` (одна копия на CU, см. interned_str_literals в emit_c).

- **`ro`-биндинг** (`ro img []u8 = x"…"` / `= embed("…")`): **нулевая копия** —
  `[]u8`-вид с `data` → статический блоб, `len == cap == N` (модель D262-видов).
  Мутирующие методы на ro-биндинге отсекает чекер — записи в .rodata невыразимы.
- **`mut`-биндинг / consume в мутацию**: материализуется **копией в GC-кучу** в
  точке биндинга (память = обычный Vec-буфер, дальше живёт как любой `[]u8`).
- **GC**: Boehm-консервативный сканер указатель вне своей кучи игнорирует —
  статический блоб не собирается и не двигается; interior-указатели легальны.
- Рост (`push` после копии) идёт обычным Vec-grow (alloc-new + memcpy) — статическая
  память никогда не realloc'ится.

### Почему не голый `0x486900FF…`

Числовой литерал стирает ведущие нулевые байты (`0x00FF` = 255), не определяет
нечётное число цифр, тянет эндианность-двусмысленность (число vs блоб — противо-
положные порядки байт в памяти) и делает вывод типа `ro x = 0x48` контекстно-
зависимым. Кавычная форма снимает все четыре проблемы лексически.

Прецеденты: D (`x"48 69"` в языке), Rust (`b"…"` + `include_bytes!`), C23 (`#embed`),
Go (`//go:embed`), Zig (`@embedFile`).

---

### D412-амендмент (2026-07-16, Plan 210): `embed_dir("dir")` — встраивание папки

**Статус: ПРИНЯТО** (владелец 2026-07-15, вариант A; материализация Option R′). Расширяет
п.2 (`embed`) на целую директорию. Маркер [M-embed-dir]. Дизайн/survey:
[research 2026-07-15](../../docs/dev/research/2026-07-15-embed-dir-proposal.md),
[план 210](../../docs/plans/210-embed-dir.md).

3. **`embed_dir("relative/dir")`** — компайл-тайм интринсик: содержимое ВСЕЙ папки
   (рекурсивно) как иммутабельный `EmbeddedDir`.
   - Аргумент — ТОЛЬКО строковый литерал (`E_EMBED_ARG_NOT_STR_LITERAL`, reuse). Путь
     резолвится относительно .nv-файла вызова (модель `embed`); граница — package-root
     вызова (Plan 193 gap-2). Выход наружу (папка ИЛИ любой обойдённый файл внутри неё) —
     `E_EMBED_OUTSIDE_PROJECT` (reuse).
   - Папка не найдена — `E_EMBED_DIR_NOT_FOUND`; путь ведёт на файл — `E_EMBED_NOT_A_DIR`
     (используй `embed`). Пустая папка → пустой `EmbeddedDir` (легально, но
     `W_EMBED_DIR_EMPTY` если пуста ПОСЛЕ dot/symlink-скипа — «навёлся не туда»).
   - **Симметрия:** `embed("папка")` (одиночный `embed` на директорию) — `E_EMBED_IS_A_DIR`
     (используй `embed_dir`).
   - **Backslash в аргументе:** `\` в строковом литерале пути (у `embed` И `embed_dir`) —
     `E_EMBED_PATH_BACKSLASH` (непортируемый исходник; путь пишется POSIX-стилем `/`
     независимо от ОС компиляции).
   - **Обход:** рекурсивный. **Скрытые** записи (имя начинается с `.`) — пропускаются;
     правило касается записей ВНУТРИ обхода, не самого аргумента — `embed_dir(".assets")`
     (корень назван явно) встраивается. **Символические ссылки** (файлы и папки) — НЕ
     следуются, пропускаются с предупреждением `W_EMBED_DIR_SYMLINK_SKIPPED` (защита от
     escape через линк и от циклов).
   - **Размер/шум:** мягкое предупреждение `W_EMBED_DIR_LARGE` при суммарном размере
     встроенных файлов > **16 MiB** или > 4096 файлов (совет, не ошибка). Порог ниже, чем
     изначально обсуждавшиеся 64 MiB: hex-блоб рендерится текстом (`0x%02X,` на байт,
     ×5.3 расширение) — 64 MiB payload дал бы ~340 МБ `.c`-файла, что практичнее
     ограничить раньше (см. Option E ниже — будущий выход из текстового рендеринга).
   - **Не-ASCII имя файла** → `W_EMBED_DIR_NON_ASCII_PATH` — непортируемый байтовый ключ:
     macOS хранит имена файлов в NFD, Windows/Linux обычно в NFC → один и тот же
     репозиторий на разных ОС даёт РАЗНЫЕ байтовые ключи (и разный `.c`). Компилятор НЕ
     нормализует — ключ = байты имени как в файловой системе на момент сборки.
   - **Имя `embed_dir` зарезервировано** (паритет `embed`): распознаётся резолвером ДО
     type-check в free-Ident-форме вызова (`embed_dir(...)`); пользовательская функция с
     этим именем не поддерживается. METHOD-позиция (`x.embed_dir(...)`) интринсиком НЕ
     является и не перехватывается.
   - **Ключ** записи = путь относительно embed-корня, разделитель POSIX `/`,
     case-sensitive, без ведущего `./`; **байты имени как есть, без Unicode-нормализации**
     (см. non-ASCII warning выше). `@get` не нормализует свой аргумент
     (`get("./x")` → `None`, `..` не упрощается). **Записи отсортированы** по ключу
     (UTF-8 байтовый порядок; порядок эквивалентен `str.compare`, D178) и **уникальны**
     (совпадающий путь дважды — невозможен по построению обхода ФС) — предпосылка
     корректности бинарного поиска.
   - **Fingerprint:** все обойдённые файлы (после dot/symlink-скипа) — зависимости
     сборки; add/rm/change любого из них инвалидирует кэш (`build_cache::compute_c_key`).
   - **CRLF/line-endings:** байты встраиваются КАК ЕСТЬ из рабочей копии на диске (без
     нормализации переносов строк). На Windows-чекауте с `autocrlf=true` текстовые
     ассеты (например, `.html`/`.css`) байтово ОТЛИЧАЮТСЯ от Linux-чекаута того же
     коммита → разный `.c` / разный fingerprint между ОС для одного и того же
     содержимого репозитория. Для кросс-ОС воспроизводимости — `-text` (или явный `eol`)
     в `.gitattributes` на asset-папки, встраиваемые через `embed`/`embed_dir`.
   - Glob-фильтр, mime/Content-Type, сжатие вшитых ассетов — вне объёма (future).

**Тип `EmbeddedDir` (prelude, `std/prelude/embed.nv`):**
- `EmbeddedEntry { path str, data []u8 }` — путь + байты (нулевая копия, вид над
  `.rodata`); публично конструируем (сам по себе инвариантов не несёт).
- `EmbeddedDir { priv entries []EmbeddedEntry }` — записи ОТСОРТИРОВАНЫ по `path` и
  уникальны; поле `priv` (D281) — инвариант охраняет ЕДИНСТВЕННЫЙ публичный конструктор
  `EmbeddedDir.new(entries)`: O(N) verify-sorted-unique (нарушение → panic, не тихий
  промах поиска) + защитная мелкая копия входа (пост-конструкционная мутация вызывающим
  исходного вектора не ломает инвариант — сами `EmbeddedEntry` иммутабельны).
- API: `@get(path str) -> Option[[]u8]` (бинарный поиск, O(log N)), `@paths() -> []str`
  (отсортированы), `@len() -> int`, `@has(path str) -> bool`, `@entries() -> ro
  []EmbeddedEntry` (явный property-read `priv`-поля, прецедент `Vec @ptr()`; `ro`-возврат
  — read-only view, L2, прецедент `str @bytes() -> ro []u8` — мутация результата — ошибка
  компиляции, инвариант не разъедается через выходной алиас). Иммутабелен целиком (нет
  мутирующих методов).
- **Иммутабельность data-view (унаследованный D412-хазард):** `data`/`@get(...)`-
  результат — вид над `.rodata`, НЕ копия. `mut`-биндинг результата + in-place запись
  (`d[0] = 5`) — неопределённое поведение уровня чекера/рантайма (D412-копия-при-
  mut-биндинге ловит только биндинг блоб-ЛИТЕРАЛА напрямую, не значения, вернувшегося из
  функции/метода) — см. floating-маркер `[M-d412-blob-view-mut-write]` (backlog, P3,
  home D412; общий для одиночного `embed` и `embed_dir`, вне объёма Plan 210). Для
  мутации содержимого — явная копия (`.clone()`).

**Материализация (Option R′):** `embed_dir("dir")` переписывается компилятором (пасс
`embed_resolve`, ДО type-check, зеркало `try_replace_embed`) в вызов `EmbeddedDir.new([
EmbeddedEntry{path: "…", data: x"…"}, …])` (список отсортирован резолвером по POSIX-байтам
пути; verify конструктора пробегает даром — уже отсортировано). Каждый `data` —
hex-блоб (п.1 этого D-блока) → zero-copy вид над `static const uint8_t nova_blob_<h>[]`
(`.rodata`); пути — interned static-строки. Таблица `entries` (`[]EmbeddedEntry` —
заголовки + view-указатели) строится один раз при вычислении выражения — O(N) мелких
GC-alloc, пренебрежимо против payload'ов. Совет: биндить `embed_dir(...)` **один раз** (в
`main()`, а не на уровне модуля — до закрытия `[M-codegen-emission-nondeterminism]`(c)
static-init topological order — и не в горячем пути; повторный вызов пересобирает
таблицу). Никаких новых codegen-примитивов: payload'ы наследуют модель материализации
D412 (`emit_c.rs` не менялся — синтезированный `Call`/`RecordLit`/`ArrayLit` типизируется
и эмитится штатным конвейером).

**Future (вне объёма Plan 210):** Option N — полностью-static таблица `entries` (тоже в
`.rodata`, zero-heap целиком) — codegen-оптимизация поверх готового API, если профиль
когда-нибудь покажет значимость one-time сборки таблицы (нереалистично для типичных N).
Option E — C23 `#embed` вместо текстового hex-рендеринга (`0x%02X,`) для payload'ов
устраняет ×5.3 раздутие `.c` и порог `W_EMBED_DIR_LARGE`; требует clang ≥19 (доступен
через WSL-clang; native windows-clang / MSVC — hex-fallback остаётся). Glob-фильтр,
mime-роутинг (`std/http`-хелпер), сжатие вшитых ассетов, dev-режим (чтение с диска в
debug, rust-embed-стиль — ОТКЛОНЁН явно: ломает «один бинарь» + вводит эффект), LSP/hover
со списком встроенных путей — все future, отдельным заходом.

**Прецеденты:** Go `//go:embed dir` + `embed.FS` (эталон: рекурсия, сортировка, бинарный
поиск, исключение скрытых, POSIX-пути, case-sensitive), Rust `rust-embed`/`include_dir!`
(`.get(path) -> Option`, debug=disk режим НЕ заимствован).

**См. также:** [D323-амендмент](04-effects.md#d323) (`spec/decisions/04-effects.md`,
Plan 210 Ф.6б) — `ReadFs`: `EmbeddedDir` конформит
read-only VFS-протоколу `ReadFs` через extension-методы (D287) в `std.fs`, наравне с
`DirFs` (root-scoped вид на реальную ФС) — единый generic-код «dev с диска / prod
embedded».

---

> **AMEND (Plan 210 Ф.6а, 2026-07-17): NFC-нормализация путей `embed_dir`.**
> Меняет пункт «Ключ» выше — тезис «байты имени как есть, без Unicode-нормализации»
> относился к исходной V1-редакции `embed_dir` и **больше не верен**: каждый
> относительный POSIX-путь записи нормализуется в **NFC** (Unicode Normalization
> Form C) при обходе — восстанавливает воспроизводимость бинаря между ОС (macOS
> обычно отдаёт имена файлов в NFD, Windows/Linux — в NFC; один и тот же
> git-чекаут раньше давал РАЗНЫЕ байтовые ключи, и разный `.c`, на разных ОС для
> одного и того же содержимого репозитория).
> - **Коллизия форм** — два РАЗНЫХ исходных имени в одной папке, чьи NFC-формы
>   совпадают (пример: файл `café.txt` с предкомпонованным `é` = U+00E9 РЯДОМ с
>   файлом `café.txt`, где `é` записан как `e` + U+0301 COMBINING ACUTE ACCENT —
>   два разных имени на уровне ФС, одна и та же NFC-форма) — жёсткая ошибка
>   компиляции **`E_EMBED_DIR_NFC_COLLISION`** (не тихая перезапись одной записи
>   другой в отсортированной таблице — то было бы хуже молчаливого варнинга).
> - **`W_EMBED_DIR_NON_ASCII_PATH` остаётся** (не-ASCII имя всё ещё стоит
>   внимания автора), но текст предупреждения обновлён: файл ВСТРАИВАЕТСЯ под
>   NFC-ключом (а не «как есть»); коллизию форм ловит отдельная жёсткая ошибка
>   выше, не этот варнинг.
> - **`@get`/строковые литералы путей** не меняются: обычные UTF-8 `str`.
>   Поскольку ключ таблицы теперь канонический NFC, а подавляющее большинство
>   текстовых редакторов сохраняют `.nv`-исходники в NFC, `get("café.txt")`
>   (NFC-литерал в исходнике) находит запись независимо от того, в какой форме
>   ФС физически хранила имя файла на диске (NFC или NFD) на момент сборки.
> - **Одиночный `embed(...)` НЕ затронут** — нормализация касается только
>   таблицы путей `embed_dir`; байты СОДЕРЖИМОГО файла (`data`) у обоих
>   интринсиков остаются байт-в-байт как на диске (CRLF/line-endings — см. выше,
>   без изменений).
> - **Материализация — zero новых Cargo-зависимостей.** Нормализация
>   переиспользует уже сгенерированные Unicode 16.0 таблицы
>   `std/src/unicode/norm_data.nv` (Plan 152.4.1, `NFD_DATA`/`CCC_DATA`/
>   `COMP_DATA`) через Rust-порт canonical decompose → canonical order →
>   canonical compose (UAX #15) в `compiler-codegen/src/nfc.rs` — тот же
>   алгоритм, что `std.unicode.normalize_nfc`/`str @to_nfc()` (Plan 152.4.2,
>   D253), не крейт `unicode-normalization` (добавил бы ~762 КБ исходников /
>   ~128 КБ сжатый `.crate` для NFD+NFKD+CCC+quick-check+stream-safe данных —
>   отклонено порогом «не тащить новый крейт >200 КБ без owner-go», исследование
>   в [план 210 §6а](../../docs/plans/210-embed-dir.md)).

---

> **AMEND (Plan 210 Ф.7, 2026-07-17): Go-паритет+ — `glob`/`hidden` именованные
> аргументы `embed_dir`, новый интринсик `embed_str`.** Owner-go «впиши в план
> и реализуй» (2026-07-17). Расширяет п.3 (`embed_dir`) двумя именованными
> аргументами и добавляет п.4 (`embed_str`, сестра `embed`). Язык-меняющее —
> амендмент едет в ТОМ ЖЕ слиянии, что код (`compiler-codegen/src/embed_resolve.rs`).
> Карта/фикстуры/вердикты — [план 210 §Ф.7](../../docs/plans/210-embed-dir.md#ф7--go-паритет-globembed_strhiddenmerge).
>
> 3а. **`embed_dir("dir", glob: "pattern")`** — именованный аргумент,
>    ТОЛЬКО строковый литерал (не-литерал → `E_EMBED_ARG_NOT_STR_LITERAL`,
>    та же семья, что не-литеральный путь). Фильтрует результат обхода ПОСЛЕ
>    dot/symlink-skip и ПОСЛЕ NFC-нормализации (Ф.6а) — маска сверяется с
>    ФИНАЛЬНЫМ POSIX-ключом записи. Простой собственный матчер (без новых
>    зависимостей): `*` — ноль+ символов, НЕ пересекает `/`; `**` — ноль+
>    символов, ПЕРЕСЕКАЕТ `/` (расхождение с Go `//go:embed`, который `**` не
>    поддерживает — выбрано прямым заданием владельца); `?` — ровно один
>    символ, не `/`; любой другой символ — буквальный (включая явный `/` для
>    сегментации маски, `"nested/*"`). Известное упрощение: `"**/x"` НЕ даёт
>    bash-подобного «нулевого каталога» — требует буквальный `/` в тексте,
>    не матчит `x` в корне (используйте `"**x"` без слэша перед маской для
>    матча на любой глубине включая корень). Пустой результат ПОСЛЕ фильтра
>    → `W_EMBED_DIR_EMPTY` (reuse, тот же код, что dot/symlink-опустошение).
> 3б. **`embed_dir("dir", hidden: true)`** — именованный аргумент, ТОЛЬКО
>    bool-литерал. Отключает dot-skip ДЛЯ ЗАПИСЕЙ ВНУТРИ ОБХОДА (симлинки
>    скипаются БЕЗУСЛОВНО — `hidden` на это не влияет). Дефолт `false` —
>    поведение не меняется (byte-identical с V1 `embed_dir`).
> 3в. **`glob` и `hidden` независимы:** `hidden` решает, что ПОПАДАЕТ в обход;
>    `glob` фильтрует уже собранный результат — dot-skipped записи `glob` не
>    может «вернуть», если `hidden` не включён рядом.
> 3г. **Неизвестное имя именованного аргумента / дублирующий `glob:`/`hidden:`
>    / лишний позиционный аргумент / spread после пути** — `E_EMBED_DIR_BAD_ARG`
>    (новый код; одна общая ошибка на эту малую семью form-нарушений, симметрично
>    тому, как `E_EMBED_ARG_NOT_STR_LITERAL` уже покрывает несколько сценариев
>    одним кодом).
> 4. **`embed_str("relative/path")`** — компайл-тайм интринсик-сестра `embed`:
>    содержимое файла как `str` вместо `[]u8`. Симметрична `embed` буквально
>    во всём, КРОМЕ типа результата и одной дополнительной проверки:
>    - аргумент — ТОЛЬКО строковый литерал (reuse `E_EMBED_ARG_NOT_STR_LITERAL`);
>    - путь не найден → reuse `E_EMBED_NOT_FOUND`; путь — папка →
>      reuse `E_EMBED_IS_A_DIR` (используй `embed_dir`); выход за project
>      root → reuse `E_EMBED_OUTSIDE_PROJECT`; `\` в пути → reuse
>      `E_EMBED_PATH_BACKSLASH` — ВСЕ диагностики `embed`/`embed_str` идут
>      через один и тот же код на одинаковых входах (полная симметрия,
>      задание явно требовало);
>    - содержимое файла ОБЯЗАНО быть валидным UTF-8 (`str`-инвариант) — иначе
>      **новый** `E_EMBED_NOT_UTF8` с offset'ом ПЕРВОГО невалидного байта
>      (`Utf8Error::valid_up_to()`, точный byte-offset без ручной реализации).
>    - Материализация: результат — интернированный `str`-литерал (та же
>      инфраструктура, что и любой рукописный `"…"`, `intern_str_literal`) —
>      **0 новых codegen-примитивов**, тот же принцип «переписать в узел,
>      которым компилятор уже умеет» что и весь план 210.
> 5. **`EmbeddedDir @merge(other EmbeddedDir) -> EmbeddedDir`** (prelude,
>    `std/prelude/embed.nv`) — АДДИТИВНО, языка НЕ меняет (чистая Nova-body
>    функция, не интринсик). Упомянуто здесь для полноты API-поверхности:
>    отсортированная склейка O(N+M) двух `EmbeddedDir` (шаг mergesort);
>    совпадающий путь в обоих входах → `panic` (тот же контракт, что
>    `EmbeddedDir.new` на нарушении sorted-инварианта — конфликт двух
>    встроенных деревьев с одинаковым путём есть программная ошибка автора).
>
> **Имена `embed_str` зарезервированы**, как и `embed`/`embed_dir` (паритет —
> резолвится до type-check в free-Ident-форме; METHOD-позиция не перехватывается).

---

## D452. Разделитель по природе конструкции: перевод строки, `;` и `,` (2026-08-10)

> **Status:** accepted (решение владельца, 2026-08-10). Реализация — [план 264](../../docs/plans/264-separator-rule.md),
> две волны: парсер + спека + фикстуры + миграция живых деревьев — **до тега v0.1**;
> линт-правило и хвост `nova_tests` — отдельным слиянием (линт по `nova_tests` уже
> в гейте и покраснел бы на немигрированном хвосте).

### Что

Разделитель определяется **природой конструкции**, а не местом:

| что разделяем | смысл | многострочно | на одной строке |
|---|---|---|---|
| стейтменты | последовательность, «потом» | перевод строки | `;` |
| арма́ `match` | альтернативы, «или» | перевод строки | `,` |
| поля записи, аргументы, импорты, элементы | список, «и ещё» | `,` | `,` |

Правило **выводимо, а не запоминаемо**: спроси «последовательность это или
список» — разделитель следует. `;` между армами запрещён: он обещает
последовательность там, где её нет (арма́ взаимоисключающи, выполнится ровно
одна).

**Ретрактируются четыре формы** (все четыре парсер принимал до этого решения):

| форма | где | мест в `std/` |
|---|---|---|
| разделение пробелом (разделителя нет) | арма́ `match` | 119 |
| разделение пробелом | стейтменты на одной строке | 0 в корпусе, но принималось |
| `;` между армами | `match` | 19 |
| запятая в многострочных армах, включая хвостовую | `match` | 269 |

### Почему

1. **Пропущенный разделитель молча даёт другую программу.** Это главный довод,
   и он не эстетический: `Ok(k) => k Err(_) => -1` при потерянной запятой
   разбирается во что-то другое и **компилируется**. Форма, в которой опечатка
   не отличима от намерения, — не удобство, а ловушка. Тот же дух, что §4а
   compiler-conventions: неправильное должно быть невыразимым.
2. **Разделитель обязан нести смысл конструкции.** `;` читается как «потом»,
   `,` — как «и ещё». Арма́ `match` взаимоисключающи: между ними «или», и
   обещать последовательность точкой с запятой — врать читателю.
3. **Обязательная запятая у Rust, Zig и C#-switch — вынужденная**, а не
   выбранная: у этих языков перевод строки не значим, и без запятой список не
   разделить. Nova перевод строки уже назначила разделителем (см.
   [«Statement separator»](../syntax.md)), то есть принадлежит семье
   Swift/Kotlin/Go, где отдельный разделитель нужен **только внутри строки**.
   Копировать чужое вынужденное решение, не имея его причины, — карго-культ.
4. **Граница ретракции проведена по одному признаку: запятая остаётся там, где
   перевод строки НЕ разделяет.** Внутри скобок и квадратных скобок перевод
   строки — это законное ПРОДОЛЖЕНИЕ выражения (правило переноса, см.
   `syntax.md`), поэтому в многострочном списке аргументов, полей записи,
   импортов и элементов массива запятая обязана остаться: без неё границу
   элемента не отличить от переноса длинной строки. У арм `match` такой
   двусмысленности нет — каждая арма самоограничена формой `паттерн =>`, и
   перевод строки разделяет её однозначно. **Поэтому таблица выше не «два
   правила», а одно: разделитель нужен ровно там, где иначе граница
   неоднозначна.**
5. **Прецедент ретракции близнецов — [D410](#d410)**: два способа записать одно
   не самообеспечиваются, выбор между ними тратит внимание и разъезжается по
   корпусу (там `to_bytes` 124 против `as_bytes` 67 — тяжёлая копия победила
   «по привычке»). Здесь то же самое в четырёх экземплярах сразу.

### Что снимается сознательно, а не по недосмотру

**Хвостовая запятая в многострочном списке — это диффовая гигиена**, а не
второй способ разделять: добавил арму — не тронул предыдущую строку, и в
истории видна одна добавленная строка вместо двух изменённых. Мы это удобство
**снимаем сознательно**, платя им за одно правило вместо четырёх. Записано
здесь именно затем, чтобы через полгода никто не «вернул как удобнее», решив,
что довод просто не заметили: его заметили и взвесили.

Область платы ограничена армами `match`. В списках аргументов, полей и
элементов хвостовая запятая **остаётся законной** — там запятая и так
обязательна (см. «Почему» §4), и хвостовая ничего не удваивает.

### Прецеденты

* **Swift, Kotlin, Go** — перевод строки значим, разделитель внутри строки:
  целевая семья, к которой Nova уже принадлежит по правилу переноса.
* **Rust, Zig, C# (switch-выражения)** — обязательная запятая между армами:
  форма, от которой уходим, вместе с причиной, по которой она у них появилась.
* **[D410](#d410)** — прецедент ретракции близнецов по тому же доводу.
* **[D38](#d38)** — сахар литерала массива; трогать его запятые запрещено
  («Почему» §4).

## D461. `_`-имя нельзя использовать; линейное значение нельзя выбросить (2026-08-15)

> **Status:** accepted (решение владельца, 2026-08-15 — разбор вокруг реестра
> 221.1 №667/№673). Реализация — реестр №674 (чекер + фикстуры), **до тега
> v0.1**: правило (Б) закрывает тихую утечку ресурса.
>
> **Cross-refs:** [D133](02-types.md#d133) и [D432](02-types.md#d432) §2/§9
> (линейность и авто-`@cleanup`), [D30](#d30) §naming (соглашения имён).

### Что

Два правила, оба — **ошибка компилятора**, не предупреждение.

**(А) Имя с ведущим `_` нельзя использовать — для ЛЮБОГО типа.**
`_x`, `_s`, `_at_x`: связать можно (это и есть способ сказать «значение мне не
нужно»), сослаться в коде — нельзя.

```nova
match maybe(5) {
    Some(_x) => _x + 1      // ОШИБКА: `_x` объявлено как неиспользуемое
    None     => 0
}
ro _y = 7
_y * 2                      // ОШИБКА, там же
```

**(Б) Линейное (`consume`) значение нельзя выбросить `_`-паттерном.**

```nova
match open() {
    Ok(_)  => ()            // ОШИБКА: линейный payload выброшен
    Err(_) => ()            // ok: `()`-payload не линеен
}

match open() {
    Ok(consume s) => s.close()   // единственная законная форма
    Err(_)        => ()
}
```

Из (А) и (Б) вместе следует, что для линейного payload **форма
`Ok(consume _s)` невозможна по построению**: использовать имя нельзя, значит
потребить его нечем, значит обязательство D133 невыполнимо. Компилятор обязан
сказать это прямо, а не оставлять программисту тупик.

### Почему

1. **Соглашение, не подкреплённое машиной, — не соглашение.** До этого решения
   `_`-префикс не проверялся нигде (кроме пропусков в verify-конвейере):
   `_x + 1` компилировался молча. Имя, объявленное «намеренно неиспользуемым» и
   тут же использованное, вводит читателя в заблуждение ровно там, где он
   доверяет соглашению.
2. **Линейность нельзя обходить синтаксисом.** `Ok(_)` — самая короткая запись,
   какой можно выбросить ресурс: значение никуда не связано, авто-`@cleanup`
   для него не эмитится (D432 §2 — он существует только для bare-биндингов), и
   компилятор до этого решения не говорил ни слова. Это тот же класс, что №667
   (арм-биндинг без потребления), но **без единого ключевого слова**, и потому
   опаснее: утечка выглядит как «я явно сказал, что мне не нужно».
3. **Отвергнутая альтернатива — освободить `_`-префикс от обязательства**
   (первая рекомендация интегратора, снята им же после вопроса владельца «а как
   тогда потребить?»): префикс говорит про ИСПОЛЬЗОВАНИЕ, а линейный ресурс —
   про ОСВОБОЖДЕНИЕ. «Не использовать» ресурс нельзя: его нужно отдать. Освободить
   префикс значило бы разрешить связанному ресурсу утечь молча — тот самый №667 с
   новым поводом.

### Зарезервированные префиксы — исключение из (А) (амендмент 2026-08-16)

Правило (А) говорит про **человеческое соглашение об именовании**, поэтому оно НЕ
распространяется на имена, которые пишет не человек, а машина:

* `_nova_*` — служебные имена рантайма и кодогена;
* `_at_*` — кэши полей (`@x`), которые вводит проход кодогена;
* `__*` — генерация (сериализация, derive, wire-таблицы).

**Почему это часть решения, а не деталь реализации:** без этой оговорки правило
отвергает собственный вывод компилятора — измерено 2026-08-15: первая редакция
поймала `_nova_decr_old`, `_at_x` и `__nv_wire_keys` в четырёх фикстурах конформанса.

**Поправка к «Радиусу» ниже:** утверждение, что фикстура
`prop_collision_avoidance_ok` «теряет предмет по построению», **неверно** и снято:
с этой оговоркой пользовательский локал `_at_x` остаётся законным, столкновение с
кэшами кодогена по-прежнему возможно, и фикстура сохраняет смысл. Она остаётся
на месте; негативные фикстуры на само правило заведены отдельно.

### Радиус (замерен 2026-08-15, а не оценён)

По 2597 файлам `.nv` корпуса (`std/`, `examples/`, `spec_tests/`, `novac/`) и
пакетам (`polaris`, `http`, `socks`, `tls`): **7 использований `_`-имён в 5
файлах, все — в `spec_tests/`, ноль в `std/`, примерах и пакетах.** Правило
вводится дёшево. Фикстура `prop_collision_avoidance_ok` (пользовательский локал
`_at_x` против компиляторных `_at_`-кэшей) теряет предмет по построению —
столкновение исчезает, раз пользовательское `_`-имя нельзя использовать; её
место занимает негативная фикстура на само правило (А).

### Приёмка

* neg: `Some(_x) => _x + 1` (обычный тип) — названная ошибка правила (А);
* neg: `ro _y = 7; _y * 2` — та же ошибка в let-форме;
* neg: `match open() { Ok(_) => () }` на `consume`-типе — ошибка правила (Б);
* pos: `Ok(consume s) => s.close()` и `Err(_)` на нелинейном payload —
  компилируются;
* корпус: `nova check` по `std/`, примерам и пакетам — без новых отказов
  (радиус выше).

---

## D467. Строковые литералы: продолжение строки, полный набор escape, тело backtick (2026-08-29)

**Решение владельца 2026-08-29** («принимаем исследования и планы должны быть
написаны по ним»), по исследованию
[2026-08-29-string-literals-proposal.md](../../docs/dev/research/2026-08-29-string-literals-proposal.md);
исполнение — план [277](../../docs/plans/277-string-literals.md).

Блок **закрывает открытый вопрос** «список поддерживаемых escapes для обычных
`"..."`-литералов» (`spec/open-questions.ru.md`) и **ретрактирует две нормы**:
санкцию сырого перевода строки в D44 §«Multi-line» и отказ от implicit tag в
D48 §«Что отвергнуто». Обе ретракции оформлены амендментами в своих блоках —
здесь они только названы, чтобы у правила был один дом.

### §1. Три формы на три задачи

| Задача | Форма |
|---|---|
| Длинный текст без переводов строк | `"...\` + перенос строки (§3) |
| Многострочный текст, шаблоны, DSL | `` `...` `` — с тегом или без (§5) |
| Большой файл целиком | `embed("...")` — D412 |

### §2. Полный набор escape в `"..."` — ЗАКРЫТЫЙ список

Внутри `"..."` распознаются ровно эти последовательности, и никакие другие:

| Escape | Значение |
|---|---|
| `\n` | перевод строки (U+000A) |
| `\t` | табуляция (U+0009) |
| `\r` | возврат каретки (U+000D) |
| `\\` | обратный слэш |
| `\"` | кавычка |
| `\0` | нулевой байт (U+0000) |
| `\$` | буквальный `$` — подавляет интерполяцию `${…}` (D44) |
| `\xNN` | кодовая точка U+00NN по двум hex-цифрам, 00..FF (см. амендмент ниже) |
| `\u{H…}` | кодовая точка Unicode; суррогаты и > U+10FFFF отвергаются |
| `\` + перенос строки | продолжение строки, §3 |

Любой другой символ после `\` — **ошибка** `unknown escape`, а не «слэш
насквозь». Список закрыт намеренно: открытый набор означает, что добавление
формы завтра меняет смысл текста, написанного сегодня.

**АМЕНДМЕНТ 2026-08-30 к `\xNN` (реестр №831), и он снимает МОЮ ЖЕ ошибку в
этом блоке.** Здесь стояло «**байт** по двум hex-цифрам» и абзац «список не
выдуман, а замерен … первые девять форм уже реализованы ровно так». Второе
утверждение НЕВЕРНО, и это показал замер сборкой (охота по оракулу, план 278
Ф.7): `"\xC3\xA9"` даёт `byte_len` = **4** и печатает `Ã©`, а не двухбайтную
`é`. То есть реализация кладёт для 128..255 КОДОВУЮ ТОЧКУ Latin-1, а не байт —
и прямо признаёт это комментарием в `lexer/mod.rs:865-868`.

**Нормой становится КОДОВАЯ ТОЧКА, а не байт, и выбор здесь не вкусовой.**
`str` в Nova — только UTF-8 (D104), а байтовое чтение `\xNN` позволило бы
записать `"\xFF"` и получить строку, которая не является валидным UTF-8, —
то есть один escape ломал бы инвариант типа. Для сырых байтов в языке есть
своя дверь, и она названа в том же комментарии реализации: `Buffer`/`[]byte`.
Правило одной фразой: **`\xNN` — короткая запись `\u{NN}` для первых 256
кодовых точек, и ничего больше.**

**Почему это записано амендментом, а не тихой правкой таблицы:** ошибка была
не в слове, а в СПОСОБЕ — норма 2026-08-29 объявила себя «замеренной», не
будучи замеренной сборкой; `check` на такой пробе молчит, потому что дефект в
ЗНАЧЕНИИ, а не в типах. Урок общий и стоит дороже самой строки: «замерено» о
поведении означает ЗАПУСК, а не чтение кода.

### §3. Продолжение строки: `\` перед переносом

`\`, стоящий непосредственно перед переводом строки, съедает и сам перенос, и
ведущие пробелы следующей строки. Литерал остаётся ОДНИМ токеном; `${…}`
работает без изменений.

```nova
panic("ChanReader.tick_every is not yet implemented — see Plan 66 \
       (docs/plans/66-timer-wheel-and-tick-every.md). Use \
       ChanReader.close_after for one-shot timers in Plan 65.")
```

**Зачем именно так.** Перенос ЯВНЫЙ, поэтому забытая закрывающая кавычка
по-прежнему обрывается на конце строки, и диагностика не уезжает на сто строк
вперёд — то самое свойство, ради которого эта форма живёт в rustc и Roslyn.

### §4. Сырой перевод строки в `"..."` — ошибка `E_STR_RAW_NEWLINE`

Перевод строки, не экранированный по §3, внутри `"..."` **запрещён**.
Сообщение обязано называть ОБЕ замены: `\` для абзаца без переводов строк,
backtick — для настоящего многострочного текста.

**Это РЕТРАКЦИЯ, а не уточнение:** D44 §«Multi-line» до сегодня разрешал
«сырой newline между `"..."`». Причина ретракции — не вкус, а диагностика:
незакрытая кавычка при разрешённом сыром переносе превращается в ошибку,
указывающую на место, не имеющее отношения к опечатке.

### §5. Backtick без тега — законная форма; тело всегда разбирается

Тело backtick **всегда** режется на `parts` и `args`. Тег решает, как собрать
обратно; **тега нет — обычная склейка через Display**, то есть ровно то, что
делает `"${a}${b}"`.

```nova
sql`a=${i} and b=${j}`   →  sql(["a=", " and b=", ""], [i, j])
   `a=${i} and b=${j}`   →  те же куски и значения, склеенные подряд
```

**Это РЕТРАКЦИЯ отказа D48** («Implicit tag — ambiguity со строками»).
Названная там двусмысленность снята тем, что формы РАЗЛИЧАЮТСЯ ДЕЛИМИТЕРОМ, а
не наличием тега: `"…"` и `` `…` `` — два разных литерала, и ни один не
превращается в другой от появления или пропажи идентификатора слева. До этого
решения тело зависело от наличия тега, из-за чего `` `a\nb ${x}` `` менял смысл
от одного идентификатора слева — вот это и было настоящей двусмысленностью.

### §6. Набор escape в backtick — ровно три

``\` ``, `\\`, `\$` — и всё. Любой другой символ после `\` проходит
насквозь ДВУМЯ символами.

**Почему именно три:** без них нельзя записать сам backtick, сам слэш и
буквальный `${`. `\n` и `\t` не нужны — backtick многострочный, перевод
строки набирается клавишей — и вредны: `` `C:\new\table` `` иначе даёт
перевод строки и табуляцию вместо пути. Правило одной фразой: **слеш экранирует
только то, что иначе не записать; всё остальное — текст.** Именно это делает
`` regex`\d{3}-\d{4}` `` рабочим без двойного экранирования.

### §7. Гигиена тела backtick: отступ и CRLF

Для БЛОЧНОЙ формы (открывающий backtick, сразу перевод строки):

1. перевод строки сразу после открывающего backtick — не содержимое;
2. снимается общий отступ: минимум по непустым строкам **и по строке
   закрывающего backtick** (правило Java text blocks — сдвинул закрывающий
   backtick левее, получил более широкое поле);
3. хвост симметричен голове (AMEND 2026-08-30, вопрос владельца: в первой
   редакции хвост подразумевался, а подразумеваемое = неопределённое):
   перевод строки ПОСЛЕДНЕЙ содержательной строки остаётся, строка
   закрывающего backtick содержимого не даёт — блочный литерал кончается
   на `\n` (правило Java text blocks; Swift, наоборот, хвостовой перевод
   съедает — выбрано ОДНО из двух явно). Не нужен хвостовой перевод —
   закрывающий backtick ставится сразу за последним символом, это уже
   однострочное окончание;
4. `\r\n` в теле нормализуется в `\n`.

Однострочная форма (`` `abc` ``) не меняется.

Пункт 4 — про СИНТАКСИС, и он не спорит с D412: там байты файла
встраиваются как есть, потому что файл — данные; здесь переносы строк
исходника — форма записи программы, и значение константы не должно зависеть от
того, как файл выкачали.

Алгоритм п.2 не изобретается: он уже описан и реализован для `///` — см.
D104 п.5.

### §8. Что отвергнуто

- **`"""`, heredoc, Zig-style `\\`-префикс** — второй многострочный делимитер
  там, где backtick уже якорь D48; у `\\`-блока к тому же нельзя написать тег.
- **Неявная склейка соседних литералов** (C, Python, D) — класс тихих ошибок с
  забытой запятой; в Nova хуже, потому что `ro a = "abc"` и `"def"` следующей
  строкой уже парсятся молча как два statement'а.
- **`+` для строк** — ретрактирован 2026-07-21 (`E_STR_CONCAT_PLUS`).

### Связь
- D44 — база строкового литерала и интерполяция; §4 ретрактирует его
  разрешение сырого переноса.
- [D48](#d48-tagged-template-literals) — tagged template; §5 ретрактирует его отказ от implicit tag,
  §6 закрывает набор escape, который там задан с многоточием.
- D104 — алгоритм снятия отступа (п.5) и слияния строк (п.4).
- D412 — `embed` для целых файлов; §7 п.3 разграничен с ним явно.
