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
| [D34](#d34-if-let-и-while-let-для-pattern-matching-в-условии) | `if let` и `while let` для pattern matching в условии |
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

## D30. Стиль именования

### Что
Один стиль на весь язык: PascalCase для типов и протоколов, snake_case
для функций/полей/локальных, SCREAMING_SNAKE_CASE для констант.
Акронимы — **PascalCase** без исключений.

### Правило

| Что | Стиль | Пример |
|---|---|---|
| Типы, варианты sum-type, эффекты, протоколы | PascalCase | `User`, `HashMap`, `Some`, `Db`, `Hashable` |
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
| `T.new(...)` | стандартный конструктор |
| `T.from(v X)` | general-purpose конверсия из X через [D73](../08-runtime.md#d73) `From[X]` |
| `T.from_X(...)` | **доменный** конструктор (`from_secs`, `from_polar`, `from_imag`) — когда `from(v)` не передаёт смысл |
| `v.into()` | парная форма для `T.from` через [D73](../08-runtime.md#d73) `Into[T]` |
| `@is_X()` | bool-предикат |
| `@as_X()` | дешёвая конверсия (без аллокации) |
| `@to_X()` | возможно дорогая конверсия |
| `@hash()`, `@clone()`, `@iter()`, `@next()` | стандартные методы |

`is_`/`as_`/`to_` — семантическая разница, следуй ей.

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
fn StringBuilder @capacity()  -> int     // не @cap()
fn ReadBuffer    @position()  -> int     // не @pos()

fn copy_into(destination []u8) -> ()   // не dest
fn parse(input str) -> Result[T, E]      // не buf, не val
```

**Запрещены ad-hoc сокращения** (mainstream-precedent): `pos`, `cap`,
`dest`, `src`, `buf`, `val`, `tmp`, `cnt`, `idx` (кроме mainstream-исключений
ниже), `arr`, `len` (кроме mainstream-исключения), `msg` (кроме `Error.msg`
field — закреплено D26), `cfg`, `ctx`.

**Mainstream-исключения** (Rust/Go/Swift convention — слишком устоявшиеся
формы, чтобы менять):

| Сокращение | Где разрешено | Прецеденты |
|---|---|---|
| `len` | длина коллекции (`s.len()`, `arr.len()`; method-only по [D117](#d117-size-like-accessors-require-call-syntax)) | Rust, Go |
| `iter` | итератор (`coll.iter()`, `Iterator`) | Rust |
| `idx` | index — **только в локальных переменных** (`for idx in ...`) | Rust convention |

Ровно три исключения, никаких других. Остальные — full word:
`length` если не коллекция-`len`, `iterator` если не protocol-имя,
`index` если параметр или поле.

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
| Unused parameter | `fn @on_exit(_outcome ScopeOutcome) -> ()` | param required by protocol, тело его не читает |
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
type ParseComplexError | InvalidFormat | NotANumber

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

### Сравнение с `readonly` / `mut` field — три оси immutability

Nova имеет **три разных** keyword'а связанных с immutability — `let`,
`const`, `readonly`/`mut` field. Они **не конкурируют**, потому что
работают на **разных уровнях** программы:

| Конструкция | Что фиксирует | Где живёт | Решает |
|---|---|---|---|
| `let x` / `let mut x` | binding | в функции / scope | можно ли переприсвоить переменную |
| `const X = ...` | compile-time placement | top-level или scope | известно ли значение при компиляции |
| `readonly field T` | поле record'а never-mut | внутри `type X { ... }` ([D36](02-types.md#d36)) | можно ли мутировать поле даже у `let mut` binding'а |
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

#### `readonly` / `mut` field — про **поле record'а**

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

`readonly` / `mut` per-field — это **freeze/unfreeze** конкретного
поля относительно дефолта. Они **не пересекаются** с `let`/`let mut`:
binding управляет «можно ли модифицировать **переменную**», поле
управляет «можно ли модифицировать **конкретное поле в записи**».

Пример где они комбинируются:

| binding | field declaration | можно `acc.field = ...` |
|---|---|---|
| `let acc` | `field T` (default) | ❌ — binding immutable |
| `let acc` | `mut field T` | ✅ — поле always-mut |
| `let acc` | `readonly field T` | ❌ |
| `let mut acc` | `field T` (default) | ✅ |
| `let mut acc` | `mut field T` | ✅ |
| `let mut acc` | `readonly field T` | ❌ — readonly сильнее |

#### Почему **три**, а не одно

Альтернативы и почему они хуже:

1. **Только `let`/`let mut` без `const`** — массивы `[N]T` требовали
   бы compile-time выводимости из `let N = 5`. Компилятор должен
   проводить escape-analysis на каждый `let`, чтобы понять
   const-eligible. Программист не видит явно «это compile-time»,
   а получает компилятор-error при первом нарушении. AI-unfriendly.

2. **Только `let`/`let mut` без `readonly`/`mut field`** — потеря
   per-field freeze. Альтернатива — newtype wrappers (`type AccountId(u64)`
   для каждого immutable поля), что ведёт к verbose-коду и потере
   ergonomics (`acc.id.value()` вместо `acc.id`). Cell/RefCell-style
   wrappers (как в Rust) ещё хуже для AI-кодинга.

3. **Только `const`/`readonly`** (без `let`/`let mut`) — теряем
   обычные mutable переменные в функциях. Можно через field record'а
   (тип-обёртку `Counter { mut value int }`), но это противоестественно
   для локальных счётчиков.

Это **три разные оси ответственности**, каждая решает свою задачу:
- `let`/`let mut` — **binding mutability** (можно ли переприсвоить).
- `const` — **compile-time vs runtime placement**.
- `readonly`/`mut` field — **per-field freeze в record'е**.

### Связь
- [D27](#d27-синтаксис-массивов-t-префикс-nt-фиксированные) — `const`
  для размеров фиксированных массивов.
- [D30](#d30-стиль-именования) — `SCREAMING_SNAKE_CASE` для `const`.
- [D32](02-types.md#d32) — default immutable bindings; `mut` для
  переменных и параметров.
- [D36](02-types.md#d36) — `readonly`/`mut` модификаторы полей
  record'а; per-field freeze.
- [07-modules.md](07-modules.md) — `export const` экспортирует.

---

## D34. Pattern-bind в `if`/`while` conditions — unified grammar с match arms

> Status: active (Rust 1:1, 2026-05-27); amended Plan 114 D184 (2026-05-31):
> drop outer `let` keyword; identifier-pattern требует `ro`/`mut`;
> constructor/destructure pattern bare = immutable, `mut` inside;
> `consume` запрещён в conditions; outer-`mut` запрещён.

### Что

Синтаксис `if pattern = expr { ... }` и `while pattern = expr { ... }`
— pattern matching прямо в условии с локальным binding в scope блока.
Несколько условий через запятую (Plan 106).

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

// Chains (Plan 106)
if Some(user) = lookup(id), user.is_active {
    process(user)
}

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
5. **Chains** (Plan 106) переиспользуют тот же `if_cond`.
6. **`else if`** — корректно для всех форм.

Грамматика:

```
if-expr      := "if" if-cond ("," if-cond)* block ("else" (if-expr | block))?
while-expr   := "while" if-cond ("," if-cond)* block
if-cond      := cond-pattern "=" expr | expr
cond-pattern := ("ro" | "mut") IDENT type_opt
              | constructor-pattern         // Some(...) / None / etc.
              | tuple-pattern               // (a, b)
              | record-pattern              // { name, age }
```

Скоуп: связанные имена доступны **только в теле блока**.

`?` работает: `if Some(user) = Db.find(id)? { ... }` пробрасывает ошибку
наверх; внутрь блока заходим только при успехе.

### Почему

1. **«Получить и использовать если есть»** без полного `match`-блока.
2. **Unified grammar с match arms** — единое правило (`Some(mut x)`),
   а не два разных (Rust `if let Some(mut x)` vs match `Some(mut x)`).
3. **Footgun protection** — identifier-pattern требует keyword'а
   (Plan 114 D184 §«identifier-pattern protection»).
4. **Условные циклы** — итерация пока паттерн совпадает.

### Что отвергнуто

- **Go-стиль `;`-разделитель** — нарушает D17 «один разделитель — запятая».
- **`:=` оператор** — shadowing-проблемы Go.
- **Smart-cast (Kotlin)** — магия в типе, AI-first против.
- **`if let` (Rust-style outer `let`)** — Plan 114 retracted в пользу
  unified pattern grammar с match arms.
- **Outer `mut` в pattern position** (`if mut Some(x)`) — Plan 114
  retracted; mut goes inside pattern (`if Some(mut x)`).

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

Bound vs unbound:

```nova
ro f = acc.balance              // bound: fn() -> money
ro g = Account.@balance         // unbound: fn(Account) -> money
```

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

// 1. Bound method value: захватывает obj как self.
//    Тип: fn(<remaining-params>) -> R
ro f = acc.@get          // тип: fn() -> int
ro g = acc.@add          // тип: fn(int) -> int
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

- **Bound** копирует / захватывает receiver внутрь closure-структуры.
  Subsequent calls используют captured self.
- **Unbound** — fn pointer без env'а. Caller обязан передать receiver
  как первый аргумент.
- **Static** — fn pointer без receiver'а вообще.

#### Использование в HOF

```nova
ro nums = [1, 2, 3]
ro negated = nums.map(int.@neg)    // unbound: применяет @neg к каждому
ro total = nums.fold(0, acc.@add)  // bound: добавляет каждый num к acc
```

#### Disambiguation для overloaded methods (Ф.5)

Если у метода несколько overload'ов, нужна type annotation:

```nova
fn Buffer mut @write(s str) -> ()
fn Buffer mut @write(b []u8) -> ()

ro buf = Buffer.new()
ro f1 = buf.@write as fn(str) -> ()      // выбор по annotation
ro f2 = buf.@write as fn([]u8) -> ()
```

Без annotation — compile error «ambiguous method value». Annotation
либо на cast (`as fn(...)`), либо на let-binding type
(`let f fn(str) -> () = buf.@write` — также работает).

#### C-runtime представление

Bound и unbound — оба используют generic `NovaClosBase` layout:
```c
typedef struct { void* fn; void* env; } NovaClosBase;
```

`fn` указывает на сгенерированный wrapper, `env` — указатель на
struct с captured receiver (для bound) или dummy struct (для unbound).
Call-site: cast `fn` к нужной сигнатуре, передача `env` + args.

Static method values — bare fn pointer (без env'а) — но в bootstrap
для единообразия тоже оборачиваются в NovaClosBase.

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
поле без `readonly`):

```nova
mut p = Point(1.0, 2.0)
p.0 = 5.0                // ок
```

Pattern matching как альтернатива:

```nova
match p {
    Point(x, y) => x + y
}
ro Point(x, y) = p      // деструктуризация
```

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
- **`Vec[T].new()`** — `[]T` это встроенный синтаксис, не отдельный
  тип `Vec`.

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
| capacity | `xs.capacity()` | размер выделенного storage'а; `len() ≤ capacity()`. Renamed from `.cap` (Plan 60 / D117 — Rust/C++/Swift naming) |
| доступ | `xs[i]`, `xs.get(i)` | `[i]` — panic при out-of-bounds (D13); `get(i)` → `Option[T]` |
| мутация | `mut xs.push(v)`, `mut xs.pop() -> Option[T]` | `push` grow при `len() == capacity()` |
| итерация | `xs.iter() -> Iter[T]`, `for x in xs { ... }` | `for` — sugar над `.iter().next()` (D58) |
| создание | `[]T.new()`, `[]T.with_capacity(n)`, `[]T.filled(v T, n int)` | static-функции на типе |

`xs.capacity()` — присутствует, но **не часть стабильного API** для
прикладного кода (detail of representation D32). Использование — для
оптимизации pre-allocation; при изменениях representation может
исчезнуть.

**Field-access form (`xs.len`, `xs.cap`, `xs.is_empty` без скобок)** —
запрещена ([D117](#d117-size-like-accessors-require-call-syntax)).
Compiler выдаёт `E_SIZE_ACCESSOR_FIELD`. Для legacy `.cap` —
diagnostic подсказывает rename `.capacity()`.

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
fn transaction[T](db mut Db, body fn() Db Fail -> T) Db Fail -> T
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

Default-типы: `int` (платформенно-зависимая ширина) для целого,
`f64` для дробного. Контекст (annotation, тип параметра, тип поля)
переопределяет:

```nova
ro x u8 = 200             // 200 это u8
fn write(b u8) -> () => ...
write(0xFF)                // 0xFF это u8
ro arr []f32 = [1.0, 2.0]
```

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

**Multi-line** работает через обычные newlines в литерале (`\n` или
сырой newline между `"..."`); tag-форма (D48) для raw-строк отдельная.

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

Mapping:

| Оператор | Метод | Возврат |
|---|---|---|
| `a + b` | `@plus(b)` | свободный |
| `a - b` | `@minus(b)` | свободный |
| `-a` | `@neg()` | обычно `Self` |
| `a * b` | `@times(b)` | свободный |
| `a / b` | `@div(b)` | свободный |
| `a % b` | `@rem(b)` | свободный |
| `a \| b`, `a & b`, `a ^ b` | `@or` / `@and` / `@xor` | свободный |
| `a << n`, `a >> n` | `@shl` / `@shr` | свободный |
| `a == b`, `a != b` | `@eq(b)` (`!=` выводится) | `bool` |
| `a < b`, `<=`, `>`, `>=` | `@lt` / `@le` / `@gt` / `@ge` | `bool` |
| `!a` | `@not()` | обычно `bool` или `Self` |
| `a[i]` (read), `a[i] = v` | `@get(i)` / `@set(i, v)` | свободный / `()` |

Правила:
1. **Только методы инстанса** — привязка к первому операнду.
2. **`&&`, `||` не перегружаются** — short-circuit предсказуем.
3. **`!=` выводится из `@eq`** — отдельно объявлять не надо.
4. **Custom-операторы запрещены** (`:+`, `>>=` и т.п.) — фиксированный
   набор символов.
5. **Никаких protocol/trait** — структурное соответствие по имени.
6. **Type coercion нет** — `Duration + 30` ошибка, нужен `Duration + 30.seconds()`.
7. **Overloading методов по типу аргумента разрешён**, если сигнатуры
   различимы:

```nova
fn Vector @times(s f64) -> Vector =>     // умножение на скаляр
    Vector { x: @x * s, y: @y * s }

fn Vector @times(other Vector) -> f64 => // dot product
    @x * other.x + @y * other.y
```

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
- **Auto-derive `@eq`/`@lt`** — отдельный механизм, не часть D46.

### Связь
- [D35](#d35-методы-инстанса-через--self-отменён) — те же `@`-методы.
- [D45](#d45-inferred-return-type-для-expression-body) — методы
  операторов имеют inferred return при expression-body.
- [02-types.md](02-types.md) — отсутствие trait/impl.

### Эволюция
Закрывает Q16 (bitflags): `type Permission(int)` с `@or`/`@and`/`@not`
для `|`/`&`/`!`.

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
escape-seq      = '\\' ( '`' | '\\' | '${' | 'n' | 't' | ... )
interpolation   = '${' expression '}'
```

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
- **Implicit tag** — ambiguity со строками.

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
| 11 | `+`, `-` (binary) | left |
| 12 | `*`, `/`, `%` | left |
| 13 | `as` (cast) | left |
| 14 | `!`, `-` (unary) | right |
| 15 | `?`, `()`, `[]`, `.` | left |

Грамматика (упрощённо):

```
program       = statement*
block         = '{' statement* '}'
statement     = ( decl | expr ) statement-end
statement-end = ';' | NEWLINE | look-ahead '}'

postfix-expr  = primary ( '.' name | '[' expr ']' | '(' args ')' | '?' )*
primary       = literal | identifier | '(' expr ')' | block | if | match | ...
```

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

**Float → integer — saturation, не UB.** Out-of-range, NaN, ±Infinity
дают defined значение, не зависящее от платформы:

- Out-of-range positive → `INT_MAX` / `UINT_MAX`.
- Out-of-range negative → `INT_MIN` / `0` (для unsigned).
- NaN → `0`.
- `+Infinity` → `INT_MAX` / `UINT_MAX`.
- `-Infinity` → `INT_MIN` / `0`.

**Если нужна проверка** out-of-range — используйте
[`TryFrom`](08-runtime.md#d77):

```nova
ro n = f as i16                // saturation, infallible
ro n = i16.try_from(f)?         // throws Fail[OutOfRangeError]
```

`as` остаётся **pure** (без Fail-эффекта). Throw-форма доступна через
D77 как explicit choice.

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
type ErrorCode | NotFound = 404 | InternalError = 500
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

Рrune `as`-cast'ов где seemingly-numeric mappingвыражает unsafe
семантику. Программист должен использовать `try_from` (с
range-check'ом) или explicit comparison:

| Запрещено через `as` | Альтернатива |
|---|---|
| `int as char`, `iN/uN as char` | `char.try_from(n)?` (range 0..0x10FFFF, не surrogate) |
| `char as u8` | `u8.try_from(c)?` (fails если codepoint > 0xFF) |
| `int/u8/f64/etc as bool` | `n != 0` (или `n != 0.0`) |
| `str as int/i32/f64/bool/char` | `T.try_from(s)?` (parse) |
| `int/f64/bool/char as str` | `str.from(v)` (format) |

**Исключение для char-литералов:** `'A' as int`, `'A' as u8`
разрешены — программист видит codepoint буквально на
write-time, range-check не нужен.

**Исключение для int-литералов → char:** `0x41 as char`, `65 as char`
разрешены, если литерал — compile-time-known integer в валидном
Unicode-диапазоне `U+0..=U+10FFFF` исключая surrogate range
`U+D800..=U+DFFF`. Range-check выполняется статически в checker'е,
runtime `Fail` не нужен. Off-range литерал — compile error с указанием
конкретного codepoint (не generic suggestion). Для **переменных** типа
`int` правило прежнее — нужен `char.try_from(n)?`. Введено в Plan 14
Ф.7 (2026-05-09).

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
type Shape | Circle { radius f64 } | Square { side f64 } | Origin

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
из варианта используется `if let` (D34), который комбинирует check
и binding в одном выражении:

```nova
// Без биндинга — только yes/no:
if r is Ok { println("ok") }

// С биндингом — if let:
if Ok(n) = r { use(n) }
```

Это даёт чёткое разделение:
- **`is`** = «yes/no» (короткий guard).
- **`if let`** = «yes + extract» (binding form).

Поэтому `is` **не поддерживает binding-форму** на sum-типах —
`r is Ok(n)` ошибка, нужно `if let Ok(n) = r`. Это согласовано
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

Для `if let`-стиля и работы через эффект `Fail`:

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
// if let
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
| `if let Some(n) = x.try_as[T]()` | один-два типа, mostly happy path |
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

// Полная форма с biding'ом:
if Circle(r) = shape { use(r) }

// Exhaustive обработка:
match shape {
    Circle(r)  => ...
    Square(s)  => ...
    Origin     => ...
}
```

Каждая форма для своего сценария: `is` — guard, `if let` — guard +
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
  `if let Variant(binding) = expr` (D34). Чтобы избежать двух форм
  для одной задачи — `is` без binding, `if let` с binding.
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
- [D34](#d34-if-let-и-while-let-для-pattern-matching-в-условии) —
  `if let Some(n) = x.try_as[T]()` использует `if let`-форму.
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
  (binding запрещён), нужно `if let Err(NotFound) = r`.

### Эволюция

**v1:** `is` работал только для `any`-значений. Sum-варианты
проверялись через `match` или `if let` — короткой `is`-формы не было.
Это вынуждало писать convention `@is_circle()` методы для часто
проверяемых вариантов, что засоряет API типов.

**v2 (текущая, 2026-05-06):** `is` расширен на sum-варианты —
`shape is Circle` работает. Cтоимость localized: tag для sum уже
есть в layout'е, никакого нового runtime-overhead'а. Биндинг-форма
**не** добавлена — это работа `if let` (D34); чёткое разделение
ролей: `is` = yes/no, `if let` = yes + extract.

Это убрало нужду в `@is_X` convention'ах из syntax.md.

### Эволюция
До D54 `as` использовался без формального D-решения (упоминался в
D44, D52). D54 фиксирует семантику явно: `as` — compile-time
конвертация; `is` — runtime type-check. Закрывает Q-any-extract
(извлечение типа из `any`-значения).

---

## D58. Range-литерал, `Iter[T]` protocol, `for x in c` implicit iter

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
3. **`for x in c` без `.iter()`** — implicit-iter. Если `c` уже
   итератор, используется напрямую; если есть метод `iter()`,
   компилятор подставляет вызов.

### Правило

#### Range-литералы

```nova
ro r1 = 0..5             // Range { start: 0, end: 5, inclusive: false }
ro r2 = 0..=5            // Range { start: 0, end: 5, inclusive: true }

ro r Range = 1..10       // в ro-binding'е работает
fn count(r Range) -> int => r.end - r.start
count(0..100)              // в позиции аргумента работает

ro ranges []Range = [0..5, 10..20, 100..200]   // в массиве
```

`a..b` — синтаксический сахар, разворачивается компилятором в
`Range { start: a, end: b, inclusive: false }`. `a..=b` →
`inclusive: true`.

**Range — обычный тип** ([08-runtime.md → D26](08-runtime.md#d26) prelude):

```nova
type Range {
    ro start int
    ro end int
    ro inclusive bool
}
```

Имеет методы `@iter()`, `@contains(x)`, `@len()`, `@is_empty()`.
Подробно — `examples/stdlib_range.nv`.

#### `Iter[T]` protocol

```nova
type Iter[T] protocol {
    mut next() -> Option[T]
}
```

Любой тип с структурно-совместимым методом `mut next() -> Option[T]`
— итератор по [D42](02-types.md#d42)/[D53](02-types.md#d53).

Примеры реализаций (структурно автоматические):

```nova
type RangeIter { ... }
fn RangeIter mut @next() -> Option[int] => ...      // Iter[int]

type VecIter[T] { ... }
fn VecIter[T] mut @next() -> Option[T] => ...        // Iter[T]

type LinesIter { ... }
fn LinesIter mut @next() -> Option[str] => ...       // Iter[str]
```

В сигнатурах функций можно использовать как параметр:

```nova
fn count_items[T](it Iter[T]) -> int {
    mut n = 0
    for _ in it { n += 1 }
    n
}
```

Структурная типизация — никаких `impl Iter for ...`-блоков, любой
`mut next() -> Option[T]` подходит.

#### `for x in c` — implicit iter

`for-loop` принимает **любое выражение справа от `in`**, разворачиваясь
по правилу:

```
for x in c { body }
```

компилируется как:

1. Если `c` имеет `mut next() -> Option[T]` — используется напрямую
   как итератор.
2. Иначе если `c` имеет `iter() -> Iter[T]` — компилятор вставляет
   `c.iter()`.
3. Иначе — ошибка компиляции.

Это означает, что **программист пишет `for x in c`** для коллекций
(используется `c.iter()` под капотом), и **то же самое для
итераторов** напрямую (без двойного `.iter()`).

```nova
ro v []int = [1, 2, 3]
for x in v { ... }                   // []T.iter() автоматически

ro r = 0..5
for x in r { ... }                   // Range.iter() автоматически
for x in 0..5 { ... }                // тот же

ro it = v.iter()
for x in it { ... }                  // it уже Iter[T], без двойного iter()
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

### Открытые вопросы
- **Reverse range** (`5..0` или `(0..5).reverse()`) — что значит
  range с `start > end`? Пустой? Идущий назад? — открытый
  Q-range-extras.
- **`(0..5).step(n)`** — step-итерация. Q-range-extras.
- **`collect[Out]()` generic-collection-construction** — требует
  bound'ов (Q-bounds) и static-method-protocol. Q-collect-mechanism.
- **Type-as-value** (передача типа как значения, `xs.collect([]int)`)
  — отдельный вопрос Q-type-as-value.
- **`@`-префикс в protocol-методах** (симметрия с реализацией) —
  Q-protocol-method-prefix.
- ~~**Static-метод в protocol через `.method()`-префикс**~~ —
  ✅ **RESOLVED** Plan 97 (2026-05-23). Leading-точка `.method(args) -> Ret`
  в `protocol {}` теле помечает метод **статическим** (симметрично D35
  `fn Type.name`); реализация ожидается через `fn Type.method(...)`.
  Bare-имя `method(args)` остаётся **instance** (backwards-compat: все
  существующие протоколы `Iter`/`Hashable`/`Equatable`/`Comparable`/
  `Display`/`Into`/`TryInto` без изменений). `From`/`TryFrom` обновлены
  под новый синтаксис (`.from(t T) -> Self`/`.try_from(t T) ->
  Result[Self,E]`). Hard-enforcement static↔instance mismatch — followup.

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

type Event | Click(int, int) | Move(int, int, int) | Idle

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
- [D34](#d34-if-let-и-while-let-для-pattern-matching-в-условии) —
  pattern-bind в условиях; array/tuple-patterns доступны и в
  `if let`/`while let`.
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

**Bindings:** `let`, `const`, `mut`, `readonly`.

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
- [D72](02-types.md#d72) — generic bounds (`[T Hashable]`); D88
  расширяет до `[T Hashable = SomeDefault]`.
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
fn process() Fail[CommitErr] -> () {
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
f(required: 1, opt: 5)     // обязательный тоже можно по имени
f(1, 5)                    // ОШИБКА — opt с дефолтом, позиционно нельзя
f(opt: 5, 1)               // ОШИБКА — позиционный после именованного
```

1. **Параметр с дефолтом — keyword-only.** Передаётся только по имени;
   позиционно — compile error. (Исключение — trailing-форма для
   последнего функционального параметра, см. «Взаимодействие».)
2. **Параметр без дефолта** связывается позиционно **или** по имени.
3. **Позиционные аргументы идут первыми**, связываются слева направо.
   Именованный аргумент **не может предшествовать** позиционному —
   `f(opt: 5, 1)` — compile error.
4. **Именованные аргументы переставимы** между собой.
5. **Каждый параметр связывается ровно один раз.** Передать параметр
   и позиционно, и по имени — compile error (`f(1, required: 2)`).
6. **Параметр с дефолтом можно опустить;** параметр без дефолта —
   обязателен (позиционно или по имени).
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
  получают support литерала добавив `#from_pairs` + статический
  `with_capacity(int) -> Self` + `mut @insert_new(K, V)`. Полный
  `FromPairs[K, V]` протокол (с bound-check через Plan 15) — future
  generalization, не в bootstrap.
- `HashMap.from(arr)` остаётся как обычный метод для **рантайм-массива**
  пар; литерал через него **не** идёт.

### Правило — NaN как ключ (документированный footgun)

Если `K` — float (`f64`/`f32`) и реализует `Hashable`, то `[f64.NAN: "x"]`
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
  декларации (function, type, constant, effect, handler, protocol).
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

### Что

Для **любого** типа `T` методы, возвращающие размер/cardinality/
capacity (`len`, `capacity`, `byte_len`, `is_empty`, плюс будущие
`count`, `size` если они появятся как built-in convention), вызываются
**только** через method-call с круглыми скобками: `t.method()`.

Запись `t.method` (без скобок) — это **bound method value** типа
`fn() -> T`, и компилятор отдельно её обрабатывает (D-block method-
values, [Plan 11](../../docs/plans/11-method-values-and-overload.md)).
В подавляющем большинстве случаев это user error.

### Правило

```nova
ro v = [1, 2, 3]
ro n = v.len()        // ✓ корректно
ro m = v.len          // ✗ error E_SIZE_ACCESSOR_FIELD
ro z = v.is_empty()   // ✓
ro c = v.capacity()   // ✓ (renamed from .cap — Rust/C++/Swift naming)
```

Что попадает под D117 (по conventional имени):

| Имя | Где |
|---|---|
| `len` | любая коллекция |
| `capacity` | любая коллекция (включая `[]T`, `HashMap`, `Set`, etc.) |
| `byte_len` | `str` (длина в байтах UTF-8) |
| `is_empty` | любая коллекция |
| `count`, `size` | если когда-нибудь добавятся как built-in convention |

Имя `cap` — **legacy alias** для `capacity`; diagnostic при попытке
field-access `t.cap` подсказывает rename на `.capacity()`.

### Diagnostic при нарушении

```
error[E_SIZE_ACCESSOR_FIELD]: size-like accessor `len` is method-only
                              (Plan 60 / D117)
  --> file.nv:42:23
   |
42 |     println("${vec.len}")
   |                    ^^^ help: append `()` — use `.len()` method call
   |
   = note: bare `.len` is bound method value `fn() -> int`,
           rarely intended in argument position
```

Для `.cap`:
```
   = help: rename to `.capacity()` (Rust/C++/Swift naming; D117)
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
| **Nova** | `arr.len()` method | `s.len()` method | `map.len()` method | none (D117) |

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
- **`cap()` (Go naming)** — отвергнуто; для редко используемого
  accessor'а Nova выбирает полное слово `capacity()` (Rust/C++/Swift
  parity), [D29 «явность над краткостью»](#d29-один-способ-делать-одно).
- **Allow bare `.len` как warning, не error** — отвергнуто для
  bootstrap; method-value form требует явного intent (Plan 11
  syntax).

### Связь

- [D32](02-types.md#d32) — array layout `(ptr, len, cap)`; D117
  скрывает эти поля от user-language.
- [D26](08-runtime.md#d26) — prelude API; D117 добавляет методы
  `[]T.len()`, `[]T.capacity()`, `[]T.is_empty()`, `str.is_empty()`
  в список prelude-API.
- [D38](#d38-создание-массивов-и-turbofish-для-дженериков) — built-in
  API для `[]T`; D117 amend'ит таблицу (раздел "Built-in API").
- [Plan 11](../../docs/plans/11-method-values-and-overload.md) —
  method-value semantics (`let f = x.@len` legitimate; bare `x.len`
  error).
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
> Migration guide: docs/migration/d126-to-tuple-newtype.md.
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

```nova
type From[T] protocol {
    .from(t T) -> Self        // static — Type.from(v)
}

type Hashable protocol {
    hash() -> u64             // instance — value.hash()
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
структурно ленивым (см. [`[M-protocol-static-enforcement-deferred]`](../../docs/simplifications.md)).

#### Backwards-compat

Все существующие протоколы (`Iter`/`Hashable`/`Equatable`/`Comparable`/
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
    do_work()?                                   // throws Err1
    // defer fires during unwinding:
    //   tx.commit() fails CommitErr → composite
    //   { primary: Err1, suppressed: [CommitErr] }
}
```

**C. Multiple defers, each can fail** — детально в [D161](#d161)
(Plan 100.4.4 multi-defer accumulation).

### `MultiError` API

```nova
type MultiError {
    primary: Err,
    suppressed: []Err,                          // в порядке firing (LIFO)
}

fn MultiError @primary() -> Err
fn MultiError @suppressed() -> []Err
fn MultiError @fmt_chain() -> str
```

Caller inspect:
```nova
match process() {
    Ok(_) => Log.info("done"),
    Err(MultiError { primary, suppressed }) => {
        Log.error("primary: ${primary}")
        for s in suppressed { Log.error("  suppressed: ${s}") }
    }
}
```

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
> by D189 (Plan 110.5.7 hard cutover, 2026-05-31). Replaced by
> `consume X = ... { body }` scope-block с `match outcome { Success/
> Failure(_)/Panic(_) }` в `on_exit` method (D188).
>
> Новые scope-level statements; complement к D90 defer/errdefer family.

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

**User-facing docs:** см. [docs/size-of-align-of.md](../../docs/size-of-align-of.md) —
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
- Readonly `readonly T` — same layout as T.
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

## D188. `Consumable[E]` protocol + `consume X = expr { body }` scope-block

> **Plan 110.** Принято 2026-05-31. **Статус: ACTIVE** (Plan 110.1+110.2
> +110.4+110.5 landed 2026-06-01). Radical simplification
> cleanup-семейства: один keyword `consume` + один protocol `Consumable[E]`
> покрывают ~95% cleanup use-cases, оставляя `defer { }` для оставшихся 5%.
> Amends [D90](#d90), [D158](#d158), [D161](#d161), [D162](#d162).
> Retracts [D160](#d160).

### Что

Вводятся два связанных языковых элемента:

1. **`Consumable[E]` protocol** — контракт ресурсов, требующих cleanup.
2. **`consume X = expr { body }`** — scope-block, гарантирующий exactly-once
   вызов `on_exit` при выходе из `body` (success, throw, panic, cancel).

```nova
type Consumable[E] protocol {
    on_exit(outcome ScopeOutcome) Fail[E] -> ()
}

type ScopeOutcome
    | Success
    | Failure(any)
    | Panic(str)
```

- **`E`** — тип ошибок, которые `on_exit` ресурса сам может throw
  (commit failure, flush failure). Если ресурс infallible — `E = Never`
  (см. [D194](#d194)).
- **`ScopeOutcome`** — type-erased (Python `__exit__` pattern): ресурс
  не знает body error type. `Failure(any)` хранит throw/cancel-payload как
  существенно динамическое значение; route'ит через `if err is T`
  (D85 auto-narrowing).
- `Panic(str)` отдельно — это **bug**, не recoverable error.

### Syntax

```nova
consume IDENT = EXPR { BODY }
```

- Parser lookahead на `{` после `EXPR` решает между:
  - `consume X = expr { body }` — scope-block (этот D188).
  - `consume X = expr` — raw linear binding (D180; для builder/transfer).
- `IDENT` — single name. Destructure (`consume (a, b) = ...`) не разрешается
  для scope-block (один resource = один cleanup).
- `EXPR` должен statically resolve к типу `Consumable[E]` для некоторого `E`
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
        Success      => _tx.on_exit(Success)
        Failure(e)   => { _tx.on_exit(Failure(e)); throw e }
        Panic(m)     => { _tx.on_exit(Panic(m)); nv_resume_panic(m) }
    }
    nv_leave_cancel_shield()
}
```

Где `nv_run_body_capturing { body }` — codegen-emitted construct:
- normal exit → `Success`
- `throw e` / `?` propagation / cancel-as-throw (D90 §7 amend) → `Failure(e)`
- `panic(m)` → `Panic(m)`
- `exit(code)` — НЕ captures; process exit'ы напрямую (handler.on_exit не runs).

### Правила (R1-R6)

#### R1 — Partial construction safety

Если `init()` throws **до** scope-entry — `on_exit` не вызывается. Codegen
эмитит установку `_outcome`/shield **только после** успешного завершения
`init()`. Пример: если `db.begin()` throws, никакого `tx.on_exit(...)` не
будет (некому).

Это согласовано с [D195](04-effects.md#d195) §boot-order для `Application`
handler'а.

#### R2 — Exactly-once

`on_exit` для данной consume-binding **гарантированно вызывается ровно один
раз** на любом exit-path (включая `return`, `throw`, `panic`, cancel).
Реализуется через runtime counter `_consume_count` в desugared coode: codegen
+ runtime инкрементируют при invocation, runtime panic'ит при ≥ 2.

Double-invocation invariant нарушается только если programmer вручную
зовёт `tx.on_exit(...)` из body — это runtime error
`D188-on-exit-double-invocation` (linear types prevent double-consume в
большинстве случаев, но FFI/reflection обход возможен).

#### R3 — Cancel-shield by default

Внутри cleanup-path (`tx.on_exit(...)`) cancel-доставка автоматически
маскируется до завершения cleanup или превышения `exit_timeout`
(см. [D192](#d192)). Это **default behavior**; opt-out не предоставляется
(Rust scopeguard / C++23 lessons показывают что opt-in cancel-shield
большинство забывает).

Cancel остаётся pending в `fiber->cancel_pending`; доставляется после
`nv_leave_cancel_shield()`. Если cleanup body превысил timeout — текущий
suspend получает `CleanupTimeoutError`, дальше propagates через D161
composition.

#### R4 — Timeout resolution at scope-entry

`exit_timeout` определяется **один раз** при scope-entry через 3-level
fallback (см. [D192](#d192)):

1. `WithExitTimeout` impl ресурса (если есть);
2. Активный `Application` effect handler (см. [D195](04-effects.md#d195));
3. Hardcoded fallback `Duration.seconds(5)`.

Кэшируется в локалке `_timeout` для use в `nv_enter_cancel_shield`.
Сохранение в локалке предотвращает race с асинхронным изменением handler'а
во время body execution.

#### R5 — LIFO composition

Вложенные `consume {}` блоки выходят в LIFO порядке (наружный позже
внутреннего). Если outer throws, inner.on_exit уже завершён. Если inner
throws, outer.on_exit получает `Failure(inner_err)` в outcome.

Mixed `consume {}` + `defer { }` LIFO — единый scope-stack per-fiber:

```nova
consume a = A.new() {
    defer { cleanup_b() }
    consume c = C.new() {
        defer { cleanup_d() }
        body
    }
    // exit: cleanup_d → c.on_exit → cleanup_b
}
// exit: a.on_exit
```

#### R6 — Memory ordering

Acquire-release semantics между body и `on_exit`:
- Все writes в `body` happen-before `on_exit` reads (release on exit,
  acquire on entry).
- Согласовано с [D167](06-concurrency.md#d167) memory ordering.
- Reason: cleanup может flush/commit видимое состояние; должен видеть
  финальную семантику ресурса.

### Typed error dispatch в `on_exit`

Resource решает что делать с body error через D85 auto-narrowing:

```nova
fn Transaction consume @on_exit(outcome ScopeOutcome) Fail[DbError] -> () {
    match outcome {
        Success => @commit()?
        Failure(err) => {
            if err is DbError.Deadlock {
                @retry_friendly_rollback()?     // err narrow'нут до DbError.Deadlock
            } else if err is DbError {
                @rollback_with_log(err.msg)?
            } else {
                @rollback()?                     // generic non-DB failure
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
fn use_any[T Consumable[E]](r T) Fail[E] -> () {
    consume binding = r {
        // binding : T
    }
}
```

Generic bound `[T Consumable[E]]` следует синтаксису [D72](#d72). E может
быть concrete (`Consumable[IoError]`) или generic param (`[T Consumable[E]]`
с обоими свободными). **Never special case**: если `E = Never` в bound,
type-checker автоматически снимает требование `Fail[E]` у caller'а
(см. [D194](#d194)).

### Что заменяется

| Старая форма | Новая форма |
|---|---|
| `consume tx = begin(); errdefer { rollback }; okdefer { commit }` | `consume tx = begin() { body }` (Transaction impl Consumable) |
| `defer \|result\| match { ... }` | `consume X = ... { }` или `with Cleanup = h { ... }` (D185) |
| `consume X = ...; defer { X.close() }` | `consume X = ... { body }` (если X impl Consumable) |

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
- [D194](#d194) — `Consumable[Never]`.
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
| `errdefer { ... }` | D90 §2 | Move logic в `Consumable.on_exit` через `match outcome { Failure(_) => ... }` или `defer + flag` для escape hatch |
| `okdefer { ... }` | D160 | `match outcome { Success => ... }` |
| `defer \|result\| { ... }` | D160 | `match outcome { ... }` в `on_exit` |
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
   // (предполагается Transaction impl Consumable: on_exit Success → commit, Failure → rollback)
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
   with Cleanup = LogHandler.new(label: "operation") {
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
> `Consumable[E]` + `consume {}`.

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
`Consumable[Application]` idiom (см. [D195](04-effects.md#d195)).

### Two-method protocol (`on_success` / `on_failure`)

```nova
type Consumable[E] protocol {
    on_success() Fail[E] -> ()
    on_failure(err any) Fail[E] -> ()
}
```

**Отвергнуто**:
- Resource'ы которые делают одинаковое cleanup в обоих случаях (Mutex,
  File) — дублирование кода.
- Panic-handling требует третий метод → 3 method protocol → readability
  страдает.
- Single `on_exit(outcome)` с match — лучше структурирован, легче
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
type ScopeOutcome | Success | Failure(any) | Cancelled(any) | Panic(str)
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

## D191. Async cleanup — `suspend` в `on_exit` body

> **Plan 110 Ф.3.** Принято 2026-05-31. **Статус: ACTIVE** (Plan 110.2.1.a
> +110.2.2.a landed 2026-06-01). Расширяет [D159](#d159) async
> cleanup на `Consumable.on_exit`.

### Что разрешено

`on_exit` body может содержать `suspend`-операции:
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

### Cancel-shield пробрасывается через suspend

Внутри `on_exit` cancel доставка отложена до `exit_timeout` (R3 [D188](#d188)).
На каждом suspend-point runtime проверяет deadline:

```nova
fn TcpStream consume @on_exit(outcome ScopeOutcome) Fail[IoError] -> () {
    @send_eof()?                    // suspend ok; cancel masked
    @wait_for_ack(timeout: 1.s())?  // suspend ok; deadline check
    @close()?                       // suspend ok; cancel masked
    // если cumulative time > exit_timeout → CleanupTimeoutError throws здесь
}
```

### Timeout exceedance

Если cumulative cleanup-suspend-time превысил `_timeout` (computed at
scope-entry per [D192](#d192) 3-level resolution):

1. Текущий active suspend получает `CleanupTimeoutError`.
2. Эта ошибка propagates через `on_exit`'s normal error path (`?`/`!!`).
3. Если `on_exit` throws — `MultiError.suppressed.push(CleanupTimeoutError)`
   composed с primary error.
4. Cancel доставка снимается (shield off), cancel re-raises после exit.

### Realtime context

В `#realtime` fn (D172) — `_timeout = Duration.zero` (D198). Любой suspend
в `on_exit` запрещён checker'ом через D172 правила (parking ops not
allowed in `#realtime`). Это compile error, не runtime.

### Связь

- [D90](#d90) §7 — cancel/throw routing.
- [D158](#d158) — failable cleanup base.
- [D159](#d159) — async cleanup base; этот D191 — amend для Consumable.
- [D172](04-effects.md#d172) — `#realtime` parking ban.
- [D188](#d188) §R3 — cancel-shield.
- [D192](#d192) — 3-level timeout resolution.
- [Plan 100.4.2](../../docs/plans/100.4.2-async-suspend-cleanup.md).

---

## D192. `exit_timeout` taxonomy + 3-level resolution

> **Plan 110 Ф.3.** Принято 2026-05-31. **Статус: ACTIVE** (Plan 110.2.3
> + 110.4.6.a Level-2 Application landed 2026-06-01). Определяет как ресурс получает
> свой cleanup deadline.

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
- НЕ часть Consumable protocol (опционально); Mutex/Sem/Lock не нужны.

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

- ❌ Нет `exit_timeout()` в `Consumable` — оптимизация для infallible
  cleanup (`MutexGuard`).
- ❌ Нет scope-level override через `with X = Y { }` — этот syntax только
  для effect-handlers.
- ❌ Нет global mutable setting через прямой setter — конфиг через
  `Application` effect handler.

### Связь

- [D188](#d188) §R4 — resolution at scope-entry.
- [D194](#d194) — `Consumable[Never]` hot-path opt.
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
`CleanupTimeoutError`, `DbError`, panic-string и пр. Type-erased.

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
type CleanupTimeoutError { duration Duration }
type MultiErrorTruncated { depth int }
```

Эти типы emerge из D90 §7 amend (CancelError) и D192 deadline enforcement
(CleanupTimeoutError).

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

## D194. `Consumable[Never]` — infallible cleanup + hot-path elision

> **Plan 110.** Принято 2026-05-31. **Статус: ACTIVE** (codegen recognizes
> `Consumable[Never]` для hot-path elision, 2026-05-31). Special-case для resource'ов которые
> гарантированно не fail в cleanup (Mutex, Sem, Lock).

### Что

Resource-типы которые **гарантированно не fail в cleanup** используют
`Consumable[Never]`:

```nova
fn MutexGuard consume @on_exit(outcome ScopeOutcome) -> () => @release()
//                                                       ^^^^ no Fail[E]
```

`Fail[Never]` — empty effect set, эквивалентно «не throws».

### Caller relaxation

Type-checker special-case: если binding имеет тип `Consumable[Never]`,
требование `Fail[E]` у caller'а **снимается**:

```nova
fn use_mutex() -> () {                // нет Fail[E]
    consume _l = mu.acquire() {        // MutexGuard: Consumable[Never] — ОК
        do_work()
    }
}
```

Это аналог Rust `Result<T, !>` или Haskell `IO ()` без `bracket throws`.
Делает API для locks/permits эргономичным.

### Generic Never special case

```nova
fn use_any[T Consumable[Never]](r T) -> () {     // нет Fail[E]
    consume binding = r { do_work() }
}
```

Generic с `[T Consumable[Never]]` тоже не требует `Fail[E]` у caller'а
(см. [D188](#d188) generic constraint).

### Hot-path optimization (D194 §perf)

Codegen detect'ит case когда:
1. Binding имеет тип `Consumable[Never]` И
2. Type не satisfies `WithExitTimeout` (нет custom `exit_timeout()` method).

В этом случае codegen **eliminates**:
- Cancel-shield setup/teardown (на throw на cleanup-path → нет MultiError compose).
- Timeout resolution (5s hardcoded не нужен — release инстант).
- `outcome` construction (Mutex не различает Success/Failure/Panic).

```c
// regular case:
nv_consume_enter(timeout);
... body ...
nv_call_on_exit(tx, outcome);
nv_consume_leave();

// elided case (Consumable[Never] + no WithExitTimeout):
... body ...
nv_call_release(mu);
```

**Critical для hot-paths**: lock contention, high-frequency permits, fast
mutex/atomic patterns. Disasm-verified в T2.9.

### Когда elision НЕ применяется

- `Consumable[Never]` + `WithExitTimeout` impl → timeout resolution не
  нужен (5s default), но shield нужен для potential `await exit_timeout()`
  call.
- `Consumable[E]` где `E != Never` → cleanup может throw, shield нужен.
- Body содержит cancel-points даже если `E = Never` — shield нужен для
  cancel-routing.

### Связь

- [D188](#d188) — Consumable base.
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
`Consumable[E]` для какого-то `E`. Type-checker проверяет в Ф.1.5 (после
type inference body init expression'а).

### Acceptable init forms

#### 1. Прямой Consumable

```nova
consume tx = db.begin() { ... }     // db.begin() : Transaction (Consumable[DbError])
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

- Обе ветки должны возвращать совместимый Consumable type.
- Если a и b возвращают разные Consumable типы → `D196-divergent-consumable`.

#### 4. Method chain

```nova
consume tx = db.with_config(cfg).begin() { ... }
```

Финальный return type должен быть Consumable.

### Rejected init forms

#### Wrapped без unwrap

```nova
consume tx = maybe_tx() { ... }     // maybe_tx() : Option[Transaction]
// → D196-wrapped-init-needs-unwrap
```

Suggestion: «use `consume tx = maybe_tx()!! { ... }` или distinguish None
сначала через `if Some(tx) = maybe_tx() { consume tx = tx { ... } }`».

#### Non-Consumable

```nova
consume x = 42 { ... }     // int не Consumable
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
- [D188](#d188) — Consumable base.
- [D194](#d194) — Never special case.
- [D196](#d196).

---

## D197. Cleanup re-entrance — nested `consume {}` inside `on_exit`

> **Plan 110 Ф.2.12.** Принято 2026-05-31. **Статус: ACTIVE** (Plan
> 110.1.8 landed; codegen handles re-entrance correctly through nested
> scope-blocks). Разрешает вложенные consume
> scope-block'и внутри `on_exit`.

### Правило

`on_exit` body **может содержать** вложенные `consume {}` блоки:

```nova
fn Connection consume @on_exit(outcome ScopeOutcome) Fail[IoError] -> () {
    // closing the connection requires acquiring lock
    consume _l = @cleanup_mutex.acquire() {
        @do_close()?
    }
}
```

### Семантика

1. **Outer cancel-shield остаётся активен** на время всей outer `on_exit`
   body. Inner consume{} наследует масштабы shield (nested mask).

2. **Inner consume{} создаёт свой shield** с своим timeout. Cancel
   доставка остаётся **глобально pending** до выхода **outer cleanup**.

3. **Inner `on_exit` ошибки compose в локальный MultiError**. Если он
   throws — outer `on_exit` получает это в propagation:
   - outer.on_exit started → outcome = Failure(orig)
   - inner.on_exit failed → MultiError { primary: orig, suppressed: [inner_err] }
   - outer cleanup completes; outer body re-throws с composed.

4. **Depth limit 256** (same as MultiError D193). При превышении — runtime
   error `D197-cleanup-reentrance-depth-exceeded` composes в MultiError;
   cleanup продолжает разворачиваться с этой ошибкой как «...truncated»
   entry.

5. **Запрещено**: re-entrance с тем же ресурсом — linear types prevent
   (D131 use-after-consume). Программер пытающийся `consume X = X { ... }`
   внутри `X.on_exit` получает compile error до reaching this rule.

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

1. **`on_exit` метод ресурса должен быть `#realtime`**, иначе compile
   error внутри `bar` body (нельзя вызвать non-`#realtime` fn из
   `#realtime`). Это значит resource-тип используемый в realtime-
   context уже спроектирован для него (`MutexGuard.release`, atomic
   ops).

2. **`WithExitTimeout` impl ресурса не вызывается** — потому что
   `nv_resolve_exit_timeout` не вызывается.

3. **`Application` effect не запрашивается** — same reason.

4. **Suspend в `on_exit` невозможен** — D172 body restriction (parking
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
- [D194](#d194) — `Consumable[Never]` typical realtime pattern.
- [Plan 103.6](../../docs/plans/103.6-realtime-blocking-integration.md).
- [Plan 113](../../docs/plans/113-realtime-blocking-attribute-only.md).

---

## D201. `#cancel_safe` — attestation на FFI safety inside cleanup

> **Plan 110.7.3.a.** Принято 2026-06-01. Cross-ref
> [D188](#d188) §R3 (cancel-shield), [D192](#d192) (exit_timeout).

### Что

`#cancel_safe` — fn-level attribute который аттестует, что функция
**безопасна для вызова из `Consumable.on_exit` body** под активным
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
`leave_shield`. Если `on_exit` вызывает C-функцию, которая:

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

При вызове FFI fn БЕЗ `#cancel_safe` из `on_exit` body — компилятор
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
  Consumable integration.
- [Plan 100.5](../../docs/plans/100.5-ffi-external-integration.md) —
  general FFI rules.
- Followup [M-110.7.3-w-ffi-cancel-unsafe-lint] — runtime lint
  enforcement (currently parser stores attribute, lint check pending).
