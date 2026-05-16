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

---

## D16. Дженерики через `[T]`, не `<T>`

### Что
Параметры типа записываются в **квадратных скобках**, не угловых.

### Правило

```nova
fn sort[T](xs []T, less fn(T, T) -> bool) -> []T
type Option[T] | Some(T) | None
type HashMap[K, V] { ... }

let parsed = parse[int]("42")?
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

let inc = |x| x + 1
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
let xs [()] = [(), (), ()]       // unit как элемент массива
let r Result[(), str] = Ok(())   // unit как generic-параметр
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
let callback fn() -> int = ...
type Server { handler fn(Request) -> Response }
fn measure[T](action fn() Io -> T) Time -> (T, Duration)

// ✗ — без fn запрещено
let f () -> int = ...                      // ✗
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
let pred fn(int) -> bool = |x| x > 0
//        ^^^^^^^^^^^^^^^^^      ^^^^^^^^^^^^
//        type annotation         closure-light value

// closure-full — анонимная fn (см. D22):
let pred fn(int) -> bool = fn(x int) -> bool => x > 0   // closure-full
let pred fn(int) -> bool = fn(x int) -> bool { x > 0 }  // closure-full block
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
let inc   = |x| x + 1                              // expr-body
let zero  = || 0                                    // no params
let block = |x| { let y = x*2; y + 1 }              // block-body
let any   = |_| 0                                   // wildcard

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
let typed    = fn(x int) -> int => x * 2
let block    = fn(x int, y int) -> int { let z = x+y; z * 2 }
let with_eff = fn(req Request) Db Log -> Response { process(req) }
let void     = fn(s str) Log { Log.info(s) }
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
   let f = |x| x + 1
   f(5)                    // first use фиксирует x: int → sig: fn(int) -> int
   f(3.14)                 // ❌ ошибка: sig уже зафиксирован
   ```

Если контекст недостаточен (closure-light нигде не используется):

```nova
let f = |x| x + 1           // ❌ cannot infer signature
```

→ либо использовать `f` далее, либо переключиться на closure-full:

```nova
let f = fn(x int) -> int => x + 1
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
    let mut count = 0
    || { count = count + 1; count }
}

let f = make_counter()
let g = make_counter()
f()    // 1   ← каждый вызов make_counter создаёт свежий scope
f()    // 2
g()    // 1   ← у g свой count, не shared с f
```

Несколько closure'ов, созданных в одном scope, **разделяют** capture:

```nova
fn make_counter() -> (fn() -> int, fn(int) -> int, fn() -> int) {
    let mut count = 0
    (
        || { count = count + 1; count },
        |a| { count = count + a; count },
        || count,
    )
}

let (f1, f2, f3) = make_counter()
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
let mut count = 0
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
let find = |xs []int| {
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
let xs []int = [1, 2, 3]                // динамический
let buf [5]u8 = [0, 0, 0, 0, 0]         // фиксированный
let zeros [4]u8 = [0; 4]                // повторение через ;

let matrix [2][3]int = [[1, 2, 3], [4, 5, 6]]
matrix[i][j]                             // i: 0..2, j: 0..3 — порядок совпадает

let opt Option[int] = Some(42)           // generic не меняется
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

#### Полные слова, не сокращения

Имена методов, типов, параметров и полей — **полные слова**, не
сокращения. Приоритет — читаемость, а не количество символов.

```nova
fn StringBuilder @capacity()  -> int     // не @cap()
fn ReadBuffer    @position()  -> int     // не @pos()

fn copy_into(destination []byte) -> ()   // не dest
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
| `len` | длина коллекции (`s.len`, `arr.len`, `Vec::len`) | Rust, Go |
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

`_prefix` — **только для полей record** (по конвенции, означает
«используй методы, не прямой доступ»). Для функций/методов `_prefix`
не используется — есть только `export` / приватно ([07-modules.md → D47](07-modules.md#d47)).

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

## D33. `const` vs `let` — compile-time vs runtime

### Что
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
let now = Time.now()
let user = Db.find(user_id) ?? throw UserNotFound(user_id)

// let mut
let mut counter = 0
counter += 1
```

`const` требует:
- Compile-time computable: литералы, арифметика, конструкторы
  record/sum-type из const-значений.
- **Не** runtime-вызовы, эффекты, ссылки на не-const.

`const fn` (compile-time функции) — отложено до Q7 (comptime).
До этого `const NOW = Time.now()` — ошибка.

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
let x = 5             // binding x не переприсваивается
let mut y = 0         // binding y переприсваивается
y = y + 1
```

Default immutable ([D32](02-types.md#d32)) — `let` без префикса всегда
immutable. `let mut` — явный opt-in в mutable, аналогично Rust
`let mut`, Swift `var`, Kotlin `var`. Программист видит `let mut` —
знает что переменная меняется.

#### `const` — про **compile-time**

```nova
const MAX = 4096                  // compile-time, в data-segment
let limit = compute_limit()        // runtime, в heap/stack
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
    readonly id u64        // поле never-mut, даже у `let mut acc`
    balance money          // поле default — mut если binding mut
    mut log_count int      // поле always-mut, даже у `let acc`
}

let mut acc = Account { id: 1, balance: 100, log_count: 0 }
acc.balance = 200          // OK   — поле default + binding mut
acc.id = 999               // ERR  — id readonly
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

## D34. `if let` и `while let` для pattern matching в условии

### Что
Синтаксис `if let pattern = expr { ... }` и `while let pattern = expr { ... }`
— pattern matching прямо в условии с локальным binding в скоупе блока.
Несколько условий через запятую.

### Правило

```nova
if let Some(user) = cache.get(key) {
    process(user)
}

if let Ok(user) = Db.find(id) {
    process(user)
} else {
    Log.warn("user not found")
}

while let Some(item) = queue.pop() {
    process(item)
}

// несколько условий через запятую
if let Some(user) = lookup(id), user.is_active {
    process(user)
}

// else if let
if let Some(a) = lookup_a() {
    use(a)
} else if let Some(b) = lookup_b() {
    use(b)               // a НЕ доступна
}
```

Грамматика:

```
if-expr    := "if" if-cond ("," if-cond)* block ("else" (if-expr | block))?
while-expr := "while" if-cond ("," if-cond)* block
if-cond    := "let" pattern "=" expr | expr
```

Скоуп: связанные `let`-имена доступны **только в теле блока**.

`?` работает: `if let user = Db.find(id)? { ... }` пробрасывает
ошибку наверх; внутрь блока заходим только при успехе.

### Почему

1. **«Получить и использовать если есть»** без полного `match`-блока.
2. **Эквивалент Go `if v, err := f(); err == nil`** со скоупом
   переменной = тело if.
3. **Условные циклы** — итерация пока паттерн совпадает.
4. **Прецедент.** Rust 1:1.

### Что отвергнуто

- **Go-стиль `;`-разделитель** — нарушает D17 «один разделитель —
  запятая».
- **`:=` оператор** — shadowing-проблемы Go.
- **Smart-cast (Kotlin)** — магия в типе, AI-first против.
- **Без `let`** (`if Some(x) = ...`) — парсер не отличит от
  сравнения.

### Связь
- [D33](#d33-const-vs-let--compile-time-vs-runtime) — `let` для
  runtime binding с локальным скоупом.
- [02-types.md → D17](02-types.md#d17) — pattern matching в `match`.

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
    readonly owner str
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
`f64`, `u8`, ..., `byte`). Это естественное следствие того, что в Nova
примитивы — обычные типы (D30, D32), просто с lowercase-именами и
особым представлением в runtime.

```nova
// Static method on a primitive — `str` is a regular type.
fn str.from(i int) -> Self => /* ... */

// Instance method on a primitive — used via `value.method()`.
fn int @to_hex() -> str => /* ... */
fn f64 @round() -> int => /* ... */

let s = str.from(42)            // static via D35
let h = (255).@to_hex()          // instance, parens around literal
let r = 3.7.@round()             // chained on numeric literal
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
let acc = Account.new("alice")
acc.deposit(100)
let bal = acc.balance()         // getter, обязательные ()
```

Bound vs unbound:

```nova
let f = acc.balance              // bound: fn() -> money
let g = Account.@balance         // unbound: fn(Account) -> money
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

let acc = Account.new(42)

// 1. Bound method value: захватывает obj как self.
//    Тип: fn(<remaining-params>) -> R
let f = acc.@get          // тип: fn() -> int
let g = acc.@add          // тип: fn(int) -> int
let v = f()               // 42
let r = g(10)             // 52

// 2. Unbound method value: self передаётся явно как первый аргумент.
//    Тип: fn(Receiver, <params>) -> R
let h = Account.@add      // тип: fn(Account, int) -> int
let r2 = h(acc, 10)       // 52

// 3. Static method value: обычная свободная функция.
//    Тип: fn(<params>) -> R
let mk = Account.new      // тип: fn(int) -> Self
let acc2 = mk(7)
```

#### Семантика

- **Bound** копирует / захватывает receiver внутрь closure-структуры.
  Subsequent calls используют captured self.
- **Unbound** — fn pointer без env'а. Caller обязан передать receiver
  как первый аргумент.
- **Static** — fn pointer без receiver'а вообще.

#### Использование в HOF

```nova
let nums = [1, 2, 3]
let negated = nums.map(int.@neg)    // unbound: применяет @neg к каждому
let total = nums.fold(0, acc.@add)  // bound: добавляет каждый num к acc
```

#### Disambiguation для overloaded methods (Ф.5)

Если у метода несколько overload'ов, нужна type annotation:

```nova
fn Buffer mut @write(s str) -> ()
fn Buffer mut @write(b []byte) -> ()

let buf = Buffer.new()
let f1 = buf.@write as fn(str) -> ()      // выбор по annotation
let f2 = buf.@write as fn([]byte) -> ()
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
let u = User { id: 1, name: "alice" }
println(u.name)

// позиционная структура — по индексу
type Point(f64, f64)
let p = Point(1.0, 2.0)
println(p.0)             // 1.0
println(p.1)             // 2.0

// кортежи — то же
let pair = (1, "alice")
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
let mut p = Point(1.0, 2.0)
p.0 = 5.0                // ок
```

Pattern matching как альтернатива:

```nova
match p {
    Point(x, y) => x + y
}
let Point(x, y) = p      // деструктуризация
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
let mut buckets []Slot[K, V] = []
let xs []int = [1, 2, 3]

// 2) inference из контекста
fn first(xs []int) -> Option[int] => ...
let result = first([])           // [] выводится из аргумента

// 3) static-методы
let buckets = []Slot[K, V].with_capacity(cap)
let empty = []int.new()
let zeros = []u8.filled(0, 1024)
```

Turbofish — те же `[T]`, без `::`:

```nova
fn parse[T](s str) -> Result[T, ParseError] => ...
let n = parse[int]("42")?            // в Result-возвращающей функции

let c = Cache[str, int].new()
let buckets = []Slot[K, V].with_capacity(16)
let result = m.@get[int]("key")
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

### Built-in API для `[]T` (Plan 17 Ф.1, закрывает Q-array-api)

`[]T` — встроенный тип, **не** запись stdlib (`Vec[T]` нет). Граница
между **built-in API** (компилятор знает напрямую) и **stdlib
extensions** (методы добавлены через `fn []T @method` по D35) —
зафиксирована ниже.

**Built-in API — известно компилятору:**

| Категория | API | Семантика |
|---|---|---|
| длина | `xs.len`, `xs.is_empty` | `len` — поле без скобок (О(1)); `is_empty` ≡ `len == 0` |
| capacity | `xs.cap` | размер выделенного storage'а; `len ≤ cap` |
| доступ | `xs[i]`, `xs.get(i)` | `[i]` — panic при out-of-bounds (D13); `get(i)` → `Option[T]` |
| мутация | `mut xs.push(v)`, `mut xs.pop() -> Option[T]` | `push` grow при `len == cap` |
| итерация | `xs.iter() -> Iter[T]`, `for x in xs { ... }` | `for` — sugar над `.iter().next()` (D58) |
| создание | `[]T.new()`, `[]T.with_capacity(n)`, `[]T.filled(v T, n int)` | static-функции на типе |

`xs.cap` — присутствует, но **не часть стабильного API** для
прикладного кода (deтail of representation D32). Использование — для
оптимизации pre-allocation; при изменениях representation может
исчезнуть.

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

**Слайсинг `xs[a..b]`** — отложен (Q-array-slicing). Сейчас у `[]T`
нет range-индексирования.

**Embed `use []T`** — допустим по D39 (имя поля обязательно):

```nova
type Holder[T] {
    use data []T
    extra str
}
let h = Holder[int] { data: [1, 2, 3], extra: "info" }
let n = h.len             // прокси к data.len
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
    let mut p = 1
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
    let pi = 3.14
    pi * r * r

// ОК — блок-форма
fn area(r f64) -> f64 {
    let pi = 3.14
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
let u = User { name: "alice" }                  // record (имя типа, без ())
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
let x i32 = 100
100 as u8
0xFF as u32
```

Default-типы: `int` (платформенно-зависимая ширина) для целого,
`f64` для дробного. Контекст (annotation, тип параметра, тип поля)
переопределяет:

```nova
let x u8 = 200             // 200 это u8
fn write(b u8) -> () => ...
write(0xFF)                // 0xFF это u8
let arr []f32 = [1.0, 2.0]
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
let name = "alice"
let age  = 30
let s = "Hello, ${name}, you are ${age}"   // → "Hello, alice, you are 30"
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
let v = "x = ${1 + 2}"             // sub-expression — ok
let v = "user = ${user.name()}"    // method call — ok
let v = "${a}${b}"                 // соседние интерполяции — ok
let v = "literal \${name}"         // escape — буквальное "${name}"

// Что НЕ работает
let v = "${}"                      // ✗ пустое выражение
let v = "${let x = 1; x}"          // ✗ statement, не выражение
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
    let mut p = 1
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

let total = 1.hour() + 30.minutes()       // вызывает @plus
let triple = 5.seconds() * 3              // вызывает @times
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
let j = json`{"name": "alice"}`
let q = sql`SELECT * FROM users WHERE id = ${user_id}`
let h = html`<div>${escape(name)}</div>`
let r = regex`\d{3}-\d{4}`
let b = bytes`deadbeef`
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

Tag-функция получает `parts []str` (сегменты, длина = `args.len + 1`)
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
let r = regex`\d+\.\d+`               // не нужно дважды экранировать
let q = sql`
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
let x = 1                        // newline разделяет
let y = 2
foo(x, y)

let a = 1; let b = 2; foo(a, b)  // ; для одной строки (редко)
```

Лексер игнорирует NEWLINE, если statement очевидно продолжается:

1. **После висящего бинарного оператора** в конце предыдущей строки:
   ```nova
   let total = a +
               b +
               c
   ```
2. **Внутри открытых `(`, `[`, `{`** — newlines игнорируются.
3. **Перед `.`** (method chain) и **перед `?`** (error propagation):
   ```nova
   let r = list
       .filter(|x| x > 0)
       .map(|x| x * 2)
       .sum()
   ```
4. **После `,`** в списках.
5. **Перед `else` / `else if`** — продолжение `if`-выражения:
   ```nova
   let label =
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
let mut n = 0
n += 1                              // ✅

// 2) @field на self в методе (D35)
fn Counter mut @inc() -> () {
    @value += 1                     // ✅
}

// 3) Element массива/индексируемой коллекции
let mut xs = [10, 20, 30]
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
`a = a.@plus(e)`.

Edge cases:

```nova
let x = foo
(arg)                        // ❌ два statement'а: foo и (arg)

let x = foo(arg)             // ✅ одна строка
let x = foo(                 // ✅ открытая ( игнорирует newline
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
let n = 100 as u32           // литерал → u32
let big = 0xFF_FF as u16
let x = 1.5 as i32           // f64 → i32 (truncate)
let y = some_int as f64       // int → f64
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
let n = f as i16                // saturation, infallible
let n = i16.try_from(f)?         // throws Fail[OutOfRangeError]
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

let u UserId = 42 as UserId   // u64 → UserId
let n u64 = u as u64           // UserId → u64
```

**Sum → int** (для sum'ов с числовыми discriminants, [D52](02-types.md#d52)):

```nova
type ErrorCode | NotFound = 404 | InternalError = 500
let code = NotFound as int    // 404
```

**Запрещено:**

- **`any → T`** (`x as int` где `x any`) — нет статической конвертации.
  Используйте `is`-pattern или `try_as[T]()` (см. ниже).
- **Произвольные типы без явного правила** (`User as Account`) —
  ошибка компиляции.
- **int → Sum через `as`** — type-небезопасно (число может не
  попасть в варианты). Только через pattern match (см. D52).

#### Запрещённые `as`-cast'ы для char/byte/bool

Рrune `as`-cast'ов где seemingly-numeric mappingвыражает unsafe
семантику. Программист должен использовать `try_from` (с
range-check'ом) или explicit comparison:

| Запрещено через `as` | Альтернатива |
|---|---|
| `int as char`, `iN/uN as char` | `char.try_from(n)?` (range 0..0x10FFFF, не surrogate) |
| `char as byte` | `byte.try_from(c)?` (fails если codepoint > 0xFF) |
| `int/byte/f64/etc as bool` | `n != 0` (или `n != 0.0`) |
| `str as int/i32/f64/bool/char` | `T.try_from(s)?` (parse) |
| `int/f64/bool/char as str` | `str.from(v)` (format) |

**Исключение для char-литералов:** `'A' as byte`, `'A' as int`,
`'A' as u8` разрешены — программист видит codepoint буквально на
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
let n int = 5
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

let s Shape = Circle { radius: 1.0 }

if s is Circle { println("circular") }       // ✅ true
if s is Square { println("squarish") }        // ✅ false
if s is Origin { println("at origin") }       // ✅ unit-вариант

// Также для prelude sum-типов:
let r Result[int, str] = Ok(42)
if r is Ok    { println("happy path") }      // ✅
if r is Err   { handle_error() }              // ✅

let opt Option[User] = Some(u)
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
if let Ok(n) = r { use(n) }
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
if let Some(n) = arg.try_as[int]() {
    process_int(n)
}

// ?-стиль
let n int = arg.as[int]?
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
if let Circle(r) = shape { use(r) }

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
   (не только в `for`).
2. **`Iter[T]`** — структурный protocol в prelude (D26):
   `protocol { mut next() -> Option[T] }`. Любой тип с таким методом
   — итератор.
3. **`for x in c` без `.iter()`** — implicit-iter. Если `c` уже
   итератор, используется напрямую; если есть метод `iter()`,
   компилятор подставляет вызов.

### Правило

#### Range-литералы

```nova
let r1 = 0..5             // Range { start: 0, end: 5, inclusive: false }
let r2 = 0..=5            // Range { start: 0, end: 5, inclusive: true }

let r Range = 1..10       // в let-binding'е работает
fn count(r Range) -> int => r.end - r.start
count(0..100)              // в позиции аргумента работает

let ranges []Range = [0..5, 10..20, 100..200]   // в массиве
```

`a..b` — синтаксический сахар, разворачивается компилятором в
`Range { start: a, end: b, inclusive: false }`. `a..=b` →
`inclusive: true`.

**Range — обычный тип** ([08-runtime.md → D26](08-runtime.md#d26) prelude):

```nova
type Range {
    readonly start int
    readonly end int
    readonly inclusive bool
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
    let mut n = 0
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
let v []int = [1, 2, 3]
for x in v { ... }                   // []T.iter() автоматически

let r = 0..5
for x in r { ... }                   // Range.iter() автоматически
for x in 0..5 { ... }                // тот же

let it = v.iter()
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
- **Static-метод в protocol через `.method()`-префикс** —
  Q-static-method-protocol.

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
let p = (1, "alice", true)

match p {
    (1, _, true)        => "first variant"
    (n, name, _)        => "n=${n}, name=${name}"
    _                   => "other"
}

let (a, b, c) = (1, 2, 3)                  // destructuring let
let (x, _, z) = (1, 2, 3)                   // ignore middle
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
let arr1 = [1, 2, 3]
let arr2 = [0, ...arr1, 4]                  // [0, 1, 2, 3, 4]

let user1 = User { id: 1, name: "alice", email: "a@x.com" }
let user2 = { ...user1, name: "bob" }        // copy + override name
```

### Правило

#### Array spread

```nova
let a = [1, 2, 3]
let b = [4, 5]

let c = [...a, ...b]                         // [1, 2, 3, 4, 5]
let d = [0, ...a, ...b, 6]                    // [0, 1, 2, 3, 4, 5, 6]
let e = [...a]                                // копия (не reference)
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

let alice User = { id: 1, name: "alice", email: "a@x.com", role: "user" }

// Override одного поля:
let alice2 = { ...alice, name: "ALICE" }

// Override нескольких:
let admin_alice = { ...alice, role: "admin", email: "admin@x.com" }

// Все поля из spread — то же значение:
let copy = { ...alice }                       // эквивалентно alice (но новый record)
```

**Правила:**

1. **Источник `...src`** должен быть **того же типа**, что и target
   (или иметь совпадающее множество полей).
2. **Override:** явные `field: value` после `...src` **перезаписывают**
   значения из spread. Порядок в литерале — left-to-right.
   ```nova
   let r = { ...src, name: "new", ...override, id: 99 }
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

let u User = { id: 1, name: "alice" }              // D52 record-coercion
let u2 User = { ...u, name: "bob" }                 // D60 spread + D52 coercion
let u3 User = { ...u }                              // полный copy через spread
```

В позиции с явным целевым типом spread работает с D52-coercion: имя
типа подразумевается из аннотации.

#### Совместимость с D17/D52 field punning

```nova
let name = "bob"
let u User = { ...other, name }                     // shorthand + spread
```

Field punning ([D52](02-types.md#d52)) работает после spread — если
имя поля совпадает с переменной в scope, shorthand обязателен.

### Почему

1. **Immutable update.** В функциональном стиле (доминирующем в
   Nova: `mut` через эффект, GC по умолчанию) immutable-обновление
   record — частая операция. Без spread:
   ```nova
   let u2 = User { id: u.id, name: "bob", email: u.email, role: u.role }
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
let names = ["alice", "bob"]
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
    if items.len == 0 { None } else { Some(items[0]) }
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

let if = 5                          // ✗
let mut while = 0                   // ✗

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
`Never`, `Option`, `Some`, `None`, `Result`, `Ok`, `Err`, `Error`,
`int`, `f64`, etc.) — это **обычные имена** в prelude scope, не
keyword'ы. Программист может **переопределить локально** (см.
[overview.md](../overview.md) «Зарезервированные identifier'ы»),
но это анти-паттерн (lint выдаёт warning).

```nova
let int_array []int = [1, 2, 3]    // ✓ — `int_array` обычный identifier
fn shadow() {
    let int = "string"              // ⚠️ shadow's prelude name (warning, не error)
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
Триггер принятия — [D87](04-effects.md#d87) (`Handler[E, IRT = Never]`).

### Правило

#### Базовый синтаксис

```nova
type Complex[T = f64] {
    re T
    im T
}

// Старые вызовы продолжают работать без [T]:
let z = Complex.from(2.0)             // T выводится как f64 (из default + arg)
let z Complex = Complex.new(1.0, 2.0)  // тип Complex без скобок ≡ Complex[f64]

// Новые — с явным параметром:
let z32 Complex[f32] = Complex.new(1.0_f32, 2.0_f32)
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
let m HashMap[str, int] = ...                        // S = DefaultHasher
let m HashMap[str, int, FxHasher] = ...              // S явно
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
   let z = Complex.from(2.0)            // ← без правок
   ```
2. **Default — не выбор для программиста.** Это сокращённая запись,
   не два пути с разной семантикой. Нарушения D9 «один очевидный путь»
   нет — программист либо не пишет параметр (получает default), либо
   пишет (получает явное значение).
3. **Прецеденты:** Rust (`Vec<T, A: Allocator = Global>`),
   C++ (`template<typename T = int>`), TypeScript (`Foo<T = string>`).
4. **Realistic consumer.** [D87](04-effects.md#d87) `Handler[E, IRT = Never]` —
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
- [D87](04-effects.md#d87) — `Handler[E, IRT = Never]` главный consumer.

### Эволюция

Зафиксировано 2026-05-10. Раньше — открытый вопрос
[Q-default-generic](../open-questions.md#q-default-generic), помечен
DEFERRED до появления реального consumer'а. Триггер — D87
параметризация `Handler` interrupt-типом.

Migration: ~10 примеров `Handler[E]` в spec/, где требуется
`Handler[E, IRT]` для interrupt-делающих handler'ов. См.
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
    let file = Fs.open(path)
    defer file.close()                  // выполнится на exit из fn
    let raw = file.read_all()
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
    let log_file = Fs.open("app.log")
    defer log_file.close()              // выход из fn

    if condition {
        let temp = Fs.create_temp()
        defer temp.cleanup()            // выход из if-блока
        write_to(temp)
    }   // <- здесь выполняется temp.cleanup()

    // <- здесь выполняется log_file.close() при exit из fn
}
```

**`errdefer` — откат при ошибке:**

```nova
fn create_user(data UserData) Fail[Db] Db -> User {
    let user = Db.insert_user(data)
    errdefer Db.delete_user(user.id)    // откат если что-то дальше упадёт

    let profile = Db.insert_profile(user, data)
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

    let r = do_work()
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
let i = 5
defer println(i)            // i = 5 захвачено сейчас
let i_new = 100             // другая переменная (immutable)
// exit prints: 5
```

Для **mut**-переменной с теми же captures-правилами:

```nova
let mut counter = 0
defer println(counter)      // counter — захвачен по reference (как closure)
counter = 42
// exit prints: 42
```

Это симметрично closure-семантике D32 (managed heap, mut-captures
through reference).

**4. Defer body — infallible.** Тело `defer`/`errdefer` **не должно**
иметь `Fail`-эффект:

```nova
defer file.close()                              // ✅ если close infallible
defer parse_config()?                            // ❌ ? requires Fail
```

Если cleanup может упасть — программист **обязан** suppress'ить:

```nova
defer {
    with Fail = handler {
        fail(e) { Log.error("cleanup failed: ${e}"); interrupt () }
    } {
        risky_cleanup()
    }
}
```

Это сознательно — language **запрещает** скрытое поглощение ошибок.
Если упало — программист видит в коде.

**5. Defer body — no-suspend.** В теле `defer`/`errdefer` **запрещены**
suspend-операции: `Time.sleep`, `Net.*`, `Fs.*` (если читают/пишут
большие объёмы), `Channel.recv` (blocking-форма), `parallel for`,
`spawn`. Это compile error.

Причина — cleanup должен быть **быстрым**. Suspend в defer делает
exit-семантику scope'а непредсказуемой.

Sync-операции с эффектами (`Db.exec` для быстрого SQL, `Log.info`)
— разрешены.

**6. Top-level `return` / `throw` / `break` / `continue` в defer-body —
запрещены (Вариант 3 — Plan 20 Ф.3 revised).** Нельзя hijack scope-exit
окружающей функции/цикла через defer — defer **сам** часть exit-процесса.

Локальный control разрешён, **только** внутри вложенных конструкций:

- `return` — разрешён внутри **nested fn-литерала** в defer body
  (`return` локален к этому fn-литералу, не к enclosing fn).
- `break` / `continue` — разрешены внутри **nested loop** (for/while/loop)
  в defer body (локальны к этому loop'у, не к enclosing).
- `throw` / `?` / `!!` / `interrupt` — **всегда** запрещены на любом
  уровне (defer body должен оставаться infallible — пункт 4).

```nova
defer {
    for x in items {
        if x.bad { break }          // ✅ local break в nested loop
    }
    return 0                         // ❌ top-level return — hijack scope exit
}

defer {
    let cleanup_fn = || {
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
    let user = Db.insert_user(data)
    let mut profile_id Option[int] = None
    with Fail = handler Fail {
        fail(e) {
            if let Some(pid) = profile_id { Db.delete_profile(pid) }
            Db.delete_user(user.id)
            throw e
        }
    } {
        let profile = Db.insert_profile(user, data)
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
let mut i = 0
defer println(i)
i = 42
// exit: print 42 (хотел печатать 0?)
```

Это **regular** для closure-семантики, но **сюрприз** для programmer'а
ожидающего «defer фиксирует значение тогда же».

Eager arguments + lazy closures (через captures) — баланс. Это путь
Go (которому 15 лет программистской практики симпатизируют).

#### Почему infallible body

Допустим, defer-body может падать:
```nova
fn process() Fail -> () {
    defer file.close()              // что если close бросает?
    throw OrderError
}
// exit: throw OrderError → cleanup → file.close throws → ???
```

Double-throw — невозможно представить корректно. Языки решают
по-разному (suppress, abort, accumulate exceptions). Все плохо.

Nova **запрещает** failable cleanup — программист обязан handle
явно. Это согласовано с D40 «один очевидный путь».

#### Почему no-suspend

`defer Time.sleep(1.second())` в exit-path означает scope живёт
лишнюю секунду. Это catastrophic для structured concurrency: parent
scope ждёт defer'ов всех детей, scheduling непредсказуем.

Принцип «cleanup быстрый» защищает от accidental hangs.

### Что отвергнуто

- **Function-level defer (Go-style)** — слабее scope-level, ограничивает
  локальный cleanup.
- **`successdefer`** (D `scope(success)`) — редкий case, обычный код
  перед exit покрывает.
- **`defer` без `errdefer`** — `errdefer` критичен для transactions,
  без него boilerplate тот же что и без `defer`. Включаем сразу.
- **Lazy argument evaluation** — surprise factor, eager — стандарт
  Go/Swift/Zig/D.
- **Failable defer body** — double-throw невозможно сделать корректно.
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
      эффект-операции с return type `Never` ОБЯЗАН закончиться
      `interrupt`/`throw`/`panic`/`exit`. Static analysis в
      `check_handler_never_ops` + helpers (`expr_diverges`,
      `block_diverges`). Покрывает Fail.fail + user-defined effects
      с Never-методами.
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
let m HashMap[int, str]  = [1: "a", 2: "b"]
let m = [1: "a", 2: "b"]                       // K, V выводятся из литерала
let a = 10
let m HashMap[int, str]  = [a: "x", a + 1: "y"]   // ключи — выражения
let m HashMap[str, bool] = ["has space": true]    // не-идентификаторный str-ключ
let empty HashMap[int, str] = []               // пустой — тип из контекста
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
  let m HashMap[str, JsonValue] = ["name": "alice", "age": 30.0]
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
    let mut _m0 = HashMap[K, V].with_capacity(2)
    let _ = _m0.@insert(k1, v1)
    let _ = _m0.@insert(k2, v2)
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
    let x = 1
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
