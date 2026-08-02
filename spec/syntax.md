# Nova — синтаксис

<!-- Правило редактирования: этот документ описывает язык В НАСТОЯЩЕМ
     ВРЕМЕНИ — «как есть». История изменений (что было ретрактировано,
     когда и почему) живёт в spec/decisions/ (D-блоки с ретракционными
     баннерами) — сюда её не переносить. Допустимые исключения:
     (а) honesty-маркеры «пока не реализовано» со ссылкой на план;
     (б) короткое «в Nova нет X — используй Y» для привычных по другим
     языкам конструкций, без дат и номеров ретракций. -->

## Минимальные примеры

```nova
// Hello world — никаких main, package, import для stdlib
print("hello")

// Чистая функция: нет эффектов, нет ошибок, детерминирована
fn double(x int) -> int => x * 2
```

## Tagged template literals — `tag\`...\``

Литерал с префиксом-тегом обрабатывается функцией `tag`. Возвращает
тип, который выбирает функция (не обязательно `str`):

```nova
ro j = json`{"name": "alice"}`              // -> Json
ro q = sql`SELECT * FROM users WHERE id = ${user_id}`   // -> Sql, безопасно
ro r = regex`\d+\.\d+`                       // -> Regex, raw
```

Байтовый блоб — отдельный литерал `x"…"` (hex-цифры → `[]u8`), не
tagged template: `ro b = x"deadbeef"` (D412; ⚠ пока не реализован —
[Plan 186](../docs/plans/186-hex-blob-embed.md)).

**Интерполяция через `${expr}`** — tag-функция получает части и
аргументы **раздельно**, что обеспечивает безопасность (защита от SQL
injection):

```nova
sql`SELECT * FROM users WHERE name = ${name}`
// → sql(["SELECT * FROM users WHERE name = ", ""], [name])
// функция передаёт name как параметр, не склеивает в строку
```

**Multiline** работает естественно. Escape: `` \` ``, `\\`, `\${` —
буквальные. Остальные символы — raw (удобно для regex и SQL).

**Стандартные теги (план stdlib MVP):** `json`, `sql`, `regex`.

**Свой тег** — обычная функция:

```nova
export fn url(parts []str, args []str) -> Url => ...

ro u = url`https://api.example.com/users/${user_id}`
```

Подробно — [D48](decisions/03-syntax.md#d48).

## Интерполяция строк — `"... ${expr} ..."`

В обычном строковом литерале `"..."` (без tag-префикса) разрешена
интерполяция выражений через `${expr}`. Это **sugar** над конкатенацией
с `str.from(...)`:

```nova
ro name = "alice"
ro age  = 30
ro s = "Hello, ${name}, you are ${age}"
// = "Hello, " + str.from(name) + ", you are " + str.from(age)
```

Каждое `${expr}` рендерится через `str.from(v)` — примитивы и
prelude-типы получают его автоматически; пользовательский тип
подключается реализацией `Display` (`@display(mut w Write)`,
[D73](decisions/08-runtime.md#d73)). Буквальное `${` в строке — через
escape: `"\${name}"`.

Подробно — [D44 → «Строковые литералы и интерполяция»](decisions/03-syntax.md#d44).

## Statement separator: newline или `;`

**Перенос строки** разделяет statement'ы. **`;` опционален** —
нужен только для нескольких statement'ов на одной строке:

```nova
ro x = 1                        // newline разделяет
ro y = 2
foo(x, y)

ro a = 1; ro b = 2; foo(a, b)  // ; для одной строки
```

Newline **игнорируется** в позициях, где statement продолжается:

```nova
// 1. После висящего бинарного оператора
ro total = a +
            b +
            c

// 2. Внутри открытых () [] {}
ro user = User {
    name: "alice",
    age: 30,
}

// 3. Перед .method() (chain)
ro result = list
    .filter(|x| x > 0)
    .sum()

// 4. Перед ? (error propagation)
ro user = find_user(id)
    ?

// 5. Перед else / else if (продолжение if-выражения)
ro label =
    if s is Origin { "at-origin" }
    else if s is Circle { "circle" }
    else { "square" }
```

**Бинарные операторы — в конец строки** (Go-стиль), не в начало:

```nova
ro total = a +              ✅
            b
ro total = a
          + b                 ❌ парсится как унарный +b
```

Подробно — [D49](decisions/03-syntax.md#d49).

## Числовые литералы

```nova
// Целые
1
1_000_000_000              // разделитель `_` между цифрами
0xFF_FF_FF_FF              // hex (любой регистр)
0b1010_0001                // binary
0o755                      // octal

// Float
1.5
1_234.567_89
1e10                       // научная нотация
1.5e-3
```

**Default-типы** без контекста: `int` для целых, `f64` для float. С
аннотацией/контекстом — берётся тип контекста:

```nova
ro x u8 = 200             // 200 это u8
ro arr []f32 = [1.0, 2.0]
```

**Type-suffixes (`100u32`, `1.5f32`) не вводятся.** Для редких случаев
дисамбигуации — `as`-cast: `100 as u32`, `0xFF as u8`.

**Разделитель `_` разрешён только между цифрами**, не подряд, не в
начале/конце, не сразу после префикса (`0x_FF` ❌), не вокруг точки
или `e`. Подробно — [D44](decisions/03-syntax.md#d44).

## Аннотации типа — форма «name type», без двоеточия

В отличие от TypeScript/Rust (`name: Type`), Nova не использует `:` как
разделитель имени и типа — только пробел, `name type`:

```nova
fn save(u User, amount money) Fail Db -> ()    // параметры
ro users []User = []                            // ro
type User { id u64, name str }                   // поля типа
for id u64 in ids { ... }                        // for-loop
```

`:` в Nova вообще не используется для типов — только как **разделитель
ключ-значение** в литералах:

```nova
ro alice = User { id: 1, name: "alice" }       // record-литерал
ro cfg = { "host": "localhost", "port": 8080 } // dict-литерал
```

## Возврат: `->` обязателен, `()` опционален

```nova
fn compute(x int) -> int => x * 2    // явный тип возврата
fn log_event(e Event) Log            // -> () можно опускать
fn save(u User) Fail Db            // эффекты + dropped -> ()
```

## Closure: light `|...|` и full `fn(...)`

В Nova две формы closure ([D22](decisions/03-syntax.md#d22-closure-light--и-full-fn)):

**closure-light** — компактная untyped форма, тело bare expr или block:
```nova
ro inc   = |x| x + 1
ro zero  = || 0
ro block = |x| { ro y = x*2; y + 1 }
ro any   = |_| 0                            // wildcard

list.filter(|x| x > 0)
list.fold(0, |acc, x| acc + x)
m.get_or_insert("k", || 0)
```

`|...|` валиден **только когда контекст однозначно задаёт сигнатуру**
(параметр fn-call'а, annotated `ro`-биндинг, return-position, first-use
inference). Без контекста — переключайся на `fn(...)`.

**closure-full** — типизированная форма, идентична named fn без имени.
Тело `=> expr` или `{ block }`:
```nova
ro typed    = fn(x int) -> int => x * 2
ro block    = fn(x int, y int) -> int { ro z = x+y; z * 2 }
ro with_eff = fn(req Request) Db Log -> Response { process(req) }
```

Эффекты в closure-light **не пишутся** — они наследуются из ambient
effect-set (= эффекты enclosing-функции ∪ активные `with`-блоки).
Если тело closure'а использует эффект, недоступный в parent'е —
compile error. Closure-full объявляет эффекты явно, как named fn.

## Trailing — блок/функция-аргумент за скобками вызова

Если последний параметр функции — функционального типа, аргумент можно
вынести за `()` вызова в одну из двух форм:

**trailing-block** — для callback'ов **без параметров** (DSL):
```nova
with_timeout(2.seconds()) {
    Db.exec(sql`UPDATE counters SET v = v + 1`)
}

retry(3) {
    Net.get(url)
}
```

**trailing-fn** — для callback'ов **с параметрами**, синтаксис
идентичен closure-full без имени:
```nova
list.filter() fn(x) => x > 0
list.fold(0) fn(acc, x) { acc + x }
list.map() fn(s str) -> Result[int, ParseError] { parse(s)? }
```

**Правила:**
- `{` (для trailing-block) или `fn` (для trailing-fn) на той же
  строке, что `)`. Перенос запрещён.
- `()` обязательны (даже пустые).
- Тип последнего параметра — функциональный.
- Один trailing на вызов.
- `|...|` (closure-light) **в trailing-position запрещён** —
  передавай через args (`f(|x| body)`) или используй `fn(...)`.

`spawn` — keyword-конструкция, не функция, поэтому не подчиняется
правилу D43. Его синтаксис описан отдельно ниже.

**Когда trailing-fn vs closure-light в args:**
- `f(|x| body)` — компактнее для one-liner'ов.
- `f(args) fn(x) { ... }` — лучше для длинных тел с биндингами,
  визуально маркирует «это блок-аргумент к вызову».

Подробно — [D22](decisions/03-syntax.md#d22-closure-light--и-full-fn),
[D43](decisions/03-syntax.md#d43-trailing-block--без-params-fnp-body-с-params).

## Тело функции: `=>` для выражения, `{}` для блока

Два **взаимоисключающих** способа:

```nova
// expression-body — ровно одно выражение
fn double(x int) => x * 2                    // -> int выведен (D45)
fn classify(n int) -> str => match n {       // -> str для ясности
    0 => "zero",
    n if n > 0 => "positive",
    _ => "negative",
}

// block-body — несколько шагов; последнее выражение = значение блока
fn next_pow2(n int) -> int {                 // -> int обязателен
    if n <= 1 { return 1 }
    mut p = 1
    while p < n { p *= 2 }
    p
}
```

**Правило `-> T` — два разных уровня, не путать:**

1. **Грамматика (compile error, если нарушено).** В **block-body**
   (`{ ... }`) `-> T` **обязателен**, если тип не `()` — компилятор не
   выводит тип из блока (`return_type_c` делает inference только для
   Expr-body; для Block-body без аннотации — `()`, «Что отвергнуто» в
   [D45](decisions/03-syntax.md#d45)). В **expression-body** (`=> expr`)
   `-> T` **опционален всегда** — тип выводится из тела. `-> T`
   обязателен везде — сознательно отвергнутый вариант (шум для
   тривиальных однострок).
2. **Style-guide (линтер-warning, не compile error).** Для
   **`export`-функций** (public API) рекомендуется писать `-> T` явно,
   даже в expression-body, — линтер предупреждает, если опущено. Это
   документация и стабильность контракта, а не грамматическое
   требование: `export fn f(x int) => x * 2` без `-> int`
   **скомпилируется**, но получит lint-warning.
   Для приватных функций и tiny helpers (геттеры, предикаты,
   конструкторы) — опускать нормально, warning не выдаётся.

**Indentation не значим.** `fn f() => stmt1; stmt2` или multiline без
`{}` — ошибка. Если шагов больше одного — `{}` обязательны.

**Если `=>`-тело — record-литерал, тип называется ровно один раз** —
не TIMTOWTDI (два эквивалентных способа), а единственно верная запись
для каждого из двух состояний сигнатуры (Plan 51 Ф.2, «устраняет
единственную живую TIMTOWTDI в записи record-литералов»):

```nova
// -> T опущен → тип обязан быть в литерале
fn Duration @plus(other Duration) => Duration { nanos: @nanos + other.nanos }

// -> T присутствует → в литерале имени типа быть НЕ должно
fn Duration @plus(other Duration) -> Duration => { nanos: @nanos + other.nanos }
```

Оба варианта записывают одну и ту же функцию, но не взаимозаменяемы —
у каждого состояния сигнатуры (есть `-> T` или нет) ровно одна
допустимая форма литерала. Смешение запрещено компилятором в обе
стороны:

- `-> Duration => Duration { ... }` (тип в сигнатуре И в литерале) —
  compile error:

  ```
  error: redundant type prefix on record literal — the return type
  `-> Duration` already declares it; write `=> { ... }`
  ```

- `=> Duration { ... }` без `-> Duration` в сигнатуре, если тип нужен
  и снаружи (`export`, неочевидный inference), — линтер требует явный
  `-> T` (см. правило style-guide выше); grammar-уровня ошибки здесь
  нет, но неоднозначности тоже нет — тип всегда один источник истины.

`-> Self` резолвится к типу receiver'а — то же правило: `-> Self =>
Counter { ... }` в методе `Counter` тоже избыточно (redundant type
prefix). Sum-coercion (`-> Shape => Circle { ... }`, литерал другого
имени, чем return-тип) это правило не затрагивает — там имя литерала
обязано остаться, т.к. `Circle ≠ Shape`.

Подробно — [D40](decisions/03-syntax.md#d40), [D45](decisions/03-syntax.md#d45).

## Перегрузка операторов

Стандартные операторы автоматически вызывают методы с фиксированными
именами:

```nova
fn Duration @plus(other Duration) => Duration { nanos: @nanos + other.nanos }
fn Duration @times(n i64) => Duration { nanos: @nanos * n }

ro total = 1.hour() + 30.minutes()       // вызывает @plus
ro triple = 5.seconds() * 3              // вызывает @times
if elapsed > 1.second() { ... }           // вызывает @compare
```

| Оператор | Метод | | Оператор | Метод |
|---|---|---|---|---|
| `+` | `@plus(o)` | | `==` | `@equal(o) -> bool` |
| `-` (binary) | `@minus(o)` | | `<` | `@compare(o) -> int` |
| `-` (unary) | `@neg()` | | `<=` | `@compare(o) -> int` |
| `*` | `@times(o)` | | `>` | `@compare(o) -> int` |
| `/` | `@div(o)` | | `>=` | `@compare(o) -> int` |
| `%` | `@rem(o)` | | `!` | НЕ перегружается (строго `bool`) |
| `\|` | `@bitor(o)` | | `<<` | `@shl(n)` |
| `&` | `@bitand(o)` | | `>>` | `@shr(n)` |
| `^` | `@bitxor(o)` | | `~` | `@bitnot()` |
| `a[i]` | `@index(i)` | | `a[i]=v` | `mut @index(i, v)` |
| `a[x..y]` | `@index(r Range)` | | | |

`==`/`!=` — через `@equal` (протокол `Equal`, `!=` выводится отрицанием); `<`/`<=`/`>`/`>=` — через единый `@compare(o) -> int` (протокол `Compare`, memcmp-стиль: `< 0` / `0` / `> 0`). Индексация `a[i]` / `a[i] = v` — `@index` / `mut @index` (протоколы `Index[K, V]` / `MutIndex[K, V]`, D240); slice-индексация `a[x..y]` — та же `@index`, перегруженная по типу параметра: `x..y` (half-open, не включает `y`) лоуэрится компилятором в `Range { start: x, end: y }`, и вызывается `a.index(r Range)` — на `[]T`/`str` возвращает view без копирования (`std/collections/vec/slice.nv`, `std/runtime/string/slice.nv`). `&&`/`||` **не перегружаются** (short-circuit
семантика). **Побитовое семейство — `bit`-префикс, и `~` отдельно от `!`** (D46-амендмент 2026-07-27, план [234](../docs/plans/234-bitwise-operator-family.md)): `&`/`|`/`^` → `@bitand`/`@bitor`/`@bitxor` (прежние `@and`/`@or`/`@xor` ретрактированы — читались как ЛОГИЧЕСКИЕ, хотя логические `&&`/`||` вообще не перегружаются); `~a` → `@bitnot()` — побитовое дополнение, перегружаемое пользовательскими типами (`~x == -(x+1)` на знаковых), тогда как `!a` остаётся ЛОГИЧЕСКИМ и (D46-AMEND 2026-08-02) НЕ перегружается вовсе — только `bool`, `@not()` ретрактирован. Compound-присваивания: `+=`/`-=`/`*=`/`/=` и (D46-амендмент (C), план 234 Ф.2а) `&=`/`|=`/`^=`/`<<=`/`>>=` — десугар в `a = a <op> b`, отдельных операторных методов нет. Custom-операторы (`:+`, `<>`) не разрешены. Подробно —
[D46](decisions/03-syntax.md#d46).

## Математические операции на числовых типах

Стандартные математические функции на `f64` / `f32` / `int` объявлены
как **instance-методы** через `@`, не как static `Math.sin(...)`.
Это согласовано с D35 (методы — основной механизм для type-bound
функций) и даёт chain-friendly формулы:

```nova
ro r = (x * x + y * y).sqrt()
ro phi = im.atan2(re)
ro dist = a.hypot(b)
ro s = (theta + offset).sin()
```

**Стандартный набор на `f64` (prelude):**

| Категория | Методы |
|---|---|
| Корни и степени | `@sqrt()`, `@cbrt()`, `@pow(exp f64)` |
| Тригонометрия | `@sin()`, `@cos()`, `@tan()`, `@asin()`, `@acos()`, `@atan()` |
| `atan2` (двух-арг) | `@atan2(x f64) -> f64` (`y.atan2(x)`) |
| Гиперболические | `@sinh()`, `@cosh()`, `@tanh()` |
| Экспонента / лог | `@exp()`, `@exp2()`, `@ln()`, `@log10()`, `@log2()` |
| Норма / расстояние | `@abs()`, `@hypot(other f64)` |
| Округление | `@floor()`, `@ceil()`, `@round()`, `@trunc()` |
| Минимум / clamp | `@min(other f64)`, `@max(other f64)`, `@clamp(lo f64, hi f64)` |
| Предикаты | `@is_finite()`, `@is_nan()`, `@is_infinite()` |

На `int` набор ограничен: `@min`, `@max`, `@clamp`, `@compare`.

**Имена**, на которые стоит обратить внимание:

- **`@hypot(other)`** / **`@atan2(x)`** — двухаргументные функции,
  второй аргумент идёт как параметр; receiver — первый аргумент по
  математической конвенции (`y.atan2(x)`, `a.hypot(b)`).

**Static-функции на типе** для тех случаев, где нет естественного
receiver'а:

```nova
f64.PI                   // константа
f64.E                    // константа
f64.NAN                  // константа
f64.INFINITY             // константа
f64.try_parse(s str) -> Option[f64]
```

## Конвенции именования

| Что | Стиль | Пример |
|---|---|---|
| Типы, эффекты, протоколы, варианты sum | **PascalCase** | `User`, `HashMap`, `Db`, `Hash`, `Some` |
| Generic-параметры | **PascalCase, односимвольные** | `T`, `K`, `V`, `E` |
| Функции, методы (`@name`), параметры, поля | **snake_case** | `parse_url`, `@deposit`, `user_id`, `created_at` |
| Константы (`const`) | **SCREAMING_SNAKE_CASE** | `MAX_PAYLOAD`, `DEFAULT_TIMEOUT` |
| Модули | **snake_case** через точки | `module admin.audit`, `module std.duration` |

**Акронимы — PascalCase, не UPPERCASE.** `Db`, не `DB`. `Http`, не `HTTP`.
`Json`, не `JSON`. `Url`, не `URL`. Правило: акроним = обычное слово.

**Зарезервированные имена методов** (operator overloading, [D46](decisions/03-syntax.md#d46)):
`@plus`, `@minus`, `@times`, `@div`, `@rem`, `@neg`, `@bitand`, `@bitor`,
`@bitxor`, `@bitnot`, `@shl`, `@shr`, `@equal`, `@compare`, `@index`.
(`@not` RETRACTED 2026-08-02 — `!` больше не перегружается.)
Не использовать для других целей.

**Договорные конвенции:**
- `T.new(...)` — стандартный конструктор; `T.from(v X)` — имя-конвенция
  конструктора-конверсии ([D73](decisions/08-runtime.md#d73); это именно
  конвенция имени, protocol-механики за ней нет);
  `T.from_X(...)` — доменный конструктор когда `from(v)` не передаёт
  смысл (`from_secs`, `from_polar`, `from_imag`).
- `@to_X()` — трансформация в новое владеющее значение, когда вида
  (zero-copy) не существует в принципе (`to_str()`, `to_upper()`,
  D410). `consume @into_X()` — потребляющая передача владения
  (`into_str()`, `into_raw()`, D131). Универсального `v.into()`
  (Rust-style, тип цели из контекста) в Nova **нет** — только
  конкретные именованные методы.
- `Display`/`@display(mut w Write)` — представление в строку для
  `${expr}`-интерполяции и `str.from(v)` на пользовательском типе
  ([D73](decisions/08-runtime.md#d73)).
- `@hash()` — хеш, `@clone()` — копия, `@iter()`/`@next()` — iterator.
- **Имена ошибок** ([D30](decisions/03-syntax.md#d30)) — с типом / доменом:
  `ParseComplexError`, `ParseIntError`, `DbError`, `OverflowError`.
  Не использовать generic `ParseError`, `ValueError`, `Exception` —
  коллизии импорта, неоднозначность для AI.

Конвенция `@as_X()`, `@is_X()` **не вводится** — дублирует
существующие механизмы:
- `@as_X()` дублирует keyword `as` (D54) для дешёвых cast'ов или
  `X.from` для нетривиальных.
- `@is_X()` дублирует `v is X` (D54): для sum-типов и `any`
  оператор `is` работает напрямую (`shape is Circle`,
  `arg is int` для `arg any`). Для извлечения значения варианта
  с биндингом — `if X(n) = v` (D34).
- Приватность поля — модификатор `priv`; `_`-префикс для «приватности
  по договору» в Nova не используется (подробно —
  [«Видимость: export»](#видимость-export-для-публичных-деклараций) ниже).
- Test-имена — строки естественного языка: `test "insert and get"`,
  не `"test_insert_and_get"`.

### Зарезервированные identifier'ы

Помимо grammar-keyword'ов, Nova имеет identifier'ы со специальной
семантикой, известной компилятору. Их можно переопределить локально,
но это анти-паттерн (линтер предупреждает).

**Special types:**
- `Self` — referential type, refers к receiver-типу метода или типу,
  удовлетворяющему protocol'у ([D66](decisions/02-types.md#d66)).
  Валиден в любом type-контексте.
- `any` — top-type для runtime type-check ([D54](decisions/03-syntax.md#d54)).
- `never` — bottom-type для не-возвращающих функций.

**Prelude types:**
- `Option[T]`, `Some(v)`, `None` — sum-тип
- `Result[T, E]`, `Ok(v)`, `Err(e)` — sum-тип
- `Error` — record `{ msg str }` для `throw err`
- `RuntimeError` — sum bottom-уровневых runtime-ошибок
- `RuntimeNoneError` — unit-тип, бросается через `expr!!` на `Option` ([D85](decisions/04-effects.md#d85))
- `Effect[E]` — first-class тип handler'а эффекта
- `Display` — protocol с instance-методом `@display(mut w Write)`,
  представление в строку ([D73](decisions/08-runtime.md#d73))

**Стандартные эффекты:**
- `Fail[E]`, `Fail` — failable-эффект
- `Io`, `Net`, `Db`, `Fs`, `Time`, `Random`, `Log`, `Trace` — основные
- `Ask[T]` — Reader-style контекст
- `Alloc[R]` — аллокация в region
- `Detach` — маркер fire-and-forget задач ([D50](decisions/06-concurrency.md#d50)).
  Блокирующие вызовы и real-time — **не эффекты**, а атрибуты функции:
  `#blocking` (offload на threadpool) и `#realtime` (запрет
  parking/alloc в теле) — D172.

**Примитивные типы (lowercase, исключение из PascalCase-правила):**
- `int`, `uint`, `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`
- `f32`, `f64`
- `str`, `bool`, `char` (байт — это `u8`, отдельного типа `byte` нет)

Подробно — [D30](decisions/03-syntax.md#d30), [D46](decisions/03-syntax.md#d46), [D47](decisions/07-modules.md#d47).

## Видимость: `export` для публичных деклараций

`export` перед декларацией = публичная (видна снаружи модуля).
Без `export` = приватная (видна только внутри модуля).

Применяется единообразно к **типам**, **функциям**, **методам**,
**константам** и **протоколам**:

```nova
module account

export type Account {                    // публичный тип
    ro owner str
    balance money
    priv internal_id u64                 // field-level priv (D220):
}                                          // поле недоступно снаружи

export type Job priv {                   // priv на типе — поля module-private
    mut name str                          // by default (D281)
}

type InternalState { ... }               // приватный тип

export const ACCOUNT_MIN_BALANCE money = 0
const INTERNAL_TIMEOUT_MS int = 5_000    // без export уже module-private (D47);
                                           // `_`-префикс не нужен, тут не поле

export fn Account.new(owner str) -> Account => ...      // публичный конструктор
export fn Account @balance() => @balance                // публичный метод
fn Account @validate(amount money) => amount > 0       // приватный helper

export type Hash protocol {
    @hash() -> u64
}
```

**Поля record:** без `priv` поля `export`-типа публичны по умолчанию
(D47). Приватность — модификатор `priv` (`priv`/`priv(type)`/`priv(file)`,
D220 + D281) на **поле** (`priv internal_id u64`) или на **типе**, задавая
дефолт для всех полей (`type Job priv { ... }`) — поле физически
недоступно снаружи, компилятор это проверяет. `_`-префикс как
«приватность по договору» в Nova не используется — приватность
только compile-time, через `priv`.

**Канонический доступ к полю — одноимённые методы-свойства через
перегрузку по арности** (D84 + D117):
чтение `@x() -> T` (0 аргументов), запись `mut @x(v T) -> @`
(1 аргумент, беглая — возврат receiver'а автоматический, D409, в теле
писать `return @`/`=> @` не нужно):

```nova
// Job — тот же priv-тип, что выше. Код ниже — внутри module account:
// снаружи модуля record-литерал `Job { name: ... }` — E_PRIV_FIELD_INIT
// (module-private поле нельзя инициализировать литералом извне), нужен
// export fn Job.new(...).

fn Job @name() -> str => @name           // getter — 0 аргументов
fn Job mut @name(v str) -> @ { @name = v }    // setter — 1 аргумент, возврат @ автоматический

mut j = Job { name: "build" }
j.name()            // getter — "build"
j.name("deploy")    // setter — переприсваивает и возвращает @
    .name("test")    // fluent-chain: сеттер можно вызывать цепочкой
```

Пары `get_x`/`set_x` — **не канон** (в std таких 0). `with_x(v)` — другая
операция (копия с заменённым полем, не мутация исходного). Весь новый
код std пишется в accessor-convention парадигме.

Подробно — [D47](decisions/07-modules.md#d47), [D29](decisions/07-modules.md#d29) (модули).

## Объявление типов

| После `type Имя` идёт | Что это |
|---|---|
| `enum` | sum-type (D406; `enum` — контекстный identifier-маркер, не lexer-keyword) |
| `set` | type-set — generic-bound по членству в явном списке типов (D310; тоже контекстный) |
| `(` | tuple-структура |
| `{` | record-структура |
| `alias` | alias |
| идентификатор/тип | newtype |
| ничего | unit-тип |

```nova
// newtype — type X Y, новый тип, типизированно отличный от Y
type UserId u64
type Email str

// alias — type X alias Y, для длинных дженериков
type StringMap[V] alias HashMap[str, V]

// record (форма сразу после имени, без `=`)
type User { id u64, name str }

// позиционная структура
type Point(f64, f64)

// unit-тип
type Marker

// sum-type — обязательный маркер enum (D406)
// inline — | разделяет варианты, перед первым не нужен:
type Color enum Red | Green | Blue

// многострочный — | обязателен у КАЖДОГО варианта, включая первый:
type Shape enum
    | Circle { radius f64 }
    | Square { side f64 }
    | Triangle { a f64, b f64, c f64 }

type Result[T, E] enum Ok(T) | Err(E)
type Option[T] enum Some(T) | None
```

`enum` — маркер в грамматике типов, валиден в любой type-позиции, не
только в `type X enum ...`: параметр (`fn job(a enum A | B)`), возврат
(`fn parse() -> enum Ok(int) | Err(str)`), поле, biнding. Named-форма
(`type Foo enum A | B`) — просто объявление имени для inline
type-выражения `enum A | B`, одна грамматика.

Sum-варианты могут иметь числовые discriminants с auto-increment:

```nova
type ExitStatus enum Ok | Failure | Critical              // 0, 1, 2 (auto)
type ErrorCode enum
    | NotFound       = 404
    | Unauthorized   = 401
    | InternalError  = 500
type Bit u8 enum Off = 0 | On = 1                          // явный базовый тип
```

> ⚠ **`type X <base> enum …` (явный базовый тип) пока не реализован** —
> parser drift, см. [Plan 105](../docs/plans/105-sum-type-explicit-base.md).
> Работают только формы без базового типа (implicit `int`).

Подробно — [decisions/02-types.md → D406](decisions/02-types.md#d406-sum-type-синтаксис-enum-маркер),
ревизия [D52](decisions/02-types.md#d52).

### Варианты sum-type — те же три формы, что top-level type

Каждый вариант sum-type объявляется по тем же правилам, что top-level
объявление:

| После имени варианта | Что это | Пример |
|---|---|---|
| `( ... )` | позиционный вариант | `Some(T)`, `Ok(T)`, `Point(f64, f64)` |
| `{ ... }` | record-вариант | `Circle { radius f64 }` |
| ничего | unit-вариант | `None`, `Red`, `Origin` |

```nova
type Option[T] enum
    | Some(T)                 // позиционный — несёт значение T
    | None                    // unit — без полей, само по себе значение

type Shape enum
    | Circle { radius f64 }   // record-вариант
    | Point(f64, f64)         // позиционный
    | Origin                  // unit
```

`None` — это значение типа `Option[T]`, **не функция и не конструктор**.
Используется без скобок:

```nova
ro x = Some(42)              // позиционный — нужен аргумент
ro y = None                  // unit — без скобок
```

Подробно — [D17](decisions/02-types.md#d17).

## Создание значений и pattern matching

```nova
ro p = Point(1.0, 2.0)
ro u = User { id: 1, name: "alice" }
ro c = Circle { radius: 5.0 }
ro s = Active

// доступ к полям (D37)
println(u.name)              // record — по имени
println(p.0, p.1)            // позиционная — по индексу
ro pair = (1, "alice")
println(pair.0, pair.1)      // кортеж — то же

// создание массивов (D38)
ro xs []int = []                          // пустой, тип из annotation
ro ys = []int.new()                       // через static-метод
mut buf = []u8.new(cap: 1024)             // pre-allocation, ровно 1024 слота (D372-amend2)

// turbofish для дженериков (D38)
ro n = parse[int]("42")?                  // явный T = int
ro m = HashMap[str, int].new()            // явные K, V

// Set[T] — множество, обёртка над HashMap[T, ()] (использует use-embed, D39)
mut s = Set[int].new()
s.insert(1)                               // -> bool, false если дубликат
s.contains(1)                             // -> bool

mut t = Set[int].new()
t.insert(2)
ro union = s | t                          // union/intersect/difference — через
ro inter = s & t                          // operator overloading (D46), не методы
ro diff  = s - t

match shape {
    Circle { radius }    => 3.14159 * radius * radius
    Square { side }      => side * side
    Triangle { a, b, c } => heron(a, b, c)
}

match result {
    Ok(value)  => value
    Err(error) => default
}
```

## Pattern matching

```nova
fn classify(x) => match x {
    0          => "zero"
    n if n >= 1 && n <= 9 => "digit"
    n if n < 0 => "negative"
    _          => "big"
}
```

Каждая arm имеет форму `pattern => result`, опционально с **guard'ом**
`pattern if condition => result`. Компилятор пробует arm'ы сверху вниз,
берёт первую, где паттерн совпал И guard истинный.

**Виды паттернов:**

| Форма | Пример | Что делает |
|---|---|---|
| Литерал | `0`, `"hello"`, `true` | сравнение по значению |
| Имя (binding) | `n`, `x` | ловит любое значение, привязывает к имени |
| Wildcard | `_` | ловит любое значение, не привязывает |
| Конструктор | `Some(v)`, `Ok(value)`, `None` | разбор варианта sum-type |
| Record | `User { id, name }` | разбор record-полей |
| Tuple | `(a, b)`, `(_, value)` | разбор кортежа |
| Guard | `n if n < 0` | паттерн + дополнительное условие |

**Exhaustiveness check.** Компилятор проверяет, что match покрывает
все возможные случаи. Если нет — ошибка с указанием непокрытого
варианта. Это работает для sum-type и bool. Для общих типов
(`int`, `str`) нужен либо `_`-wildcard, либо явная проверка всех
рассматриваемых значений.

```nova
type Color enum Red | Green | Blue

fn name(c Color) -> str => match c {
    Red   => "red"
    Green => "green"
    // ОШИБКА: missing variant `Blue`
}
```

`match` — это **выражение**, возвращает значение. Все ветви должны
иметь совместимый тип (или общий supertype, либо обёрнутые в sum-type).

### Record-литералы и patterns

**Shorthand** — когда имя поля совпадает с именем переменной в scope:

```nova
ro key = "alice"
ro value = 42

ro entry = Entry { key, value }                 // shorthand обязателен (D52)
ro entry = Entry { key, value, extra: "data" }  // можно смешивать
// `Entry { key: key }` — ОШИБКА: используйте shorthand `{ key }`.
```

**Partial pattern matching** — указывать только нужные поля:

```nova
match @buckets[idx] {
    Occupied { value }     => Some(value)        // partial: key игнорируется
    Occupied { value, .. } => Some(value)        // явный .. — то же самое
    _                      => None
}
```

Обе формы валидны (`..` или без) — выбор по контексту. `..` —
сигнал «у типа есть ещё поля». Без — короче.

**Переименование при деструктуризации:**

```nova
Occupied { key: k, value }      // key переименовано в k, value совпадает
```

Подробно — [D17](decisions/02-types.md#d17).

### Циклы `for` / `while` / `loop`

```nova
for x in list { ... }            // x — immutable binding на каждой итерации
for mut x in list { ... }         // x можно мутировать в теле
for x int in nums { ... }         // явный тип элемента
for mut id u64 in ids { ... }     // mut + явный тип элемента
for (i, x) in list.iter().enumerate() { ... }   // индекс через iterator-адаптер

while cond { ... }                // условный цикл
loop { ... }                      // бесконечный, выход через break/return
```

**Явный тип элемента — `for x TYPE in iter`** — опционален и следует
универсальному правилу «name type» (как `ro x int`, `fn(x int)`,
`[T Bound]`). Аннотация **проверяется компилятором**: если `TYPE` не
совпадает с фактическим типом элемента итератора — compile error. Это
делает её *checked assertion* (фиксирует ожидание; смена типа источника
→ loud error), а не молчаливым документирующим сахаром. Go/Rust/TS
аннотацию loop-переменной не дают вовсе — Nova получает её как строгий
проверяемый superset.

Переменная в `for x in iter` — **immutable binding** (как `ro`, без
`mut`), на каждой итерации получает **новое значение**. В теле блока
переприсвоить нельзя:

```nova
for x in list {
    x = 5                         // ОШИБКА: x immutable
}

for mut x in list {
    x = transform(x)              // ок
}
```

Это согласовано с правилом D32 + D33 — все binding'и иммутабельны по
умолчанию, мутация явно через `mut`. Никакого `const` или `final`
маркера в Nova нет — иммутабельность и так дефолт.

`break` / `continue` — стандартные.

### Паттерн в условии — `if pattern = …` / `while pattern = …`

Паттерн-матч прямо в условии — короткая альтернатива `match` для
одного варианта:

```nova
// если в кеше есть — вернуть
if Some(data) = cache.get(key) {
    return data
}

// извлечение из Result
if Ok(user) = Db.find(id) {
    process(user)
} else {
    Log.warn("user not found")
}

// while с паттерном — итерация пока паттерн совпадает
while Some(line) = reader.read_line()? {
    process(line)
}

// guard-условие через && (Plan 106)
if Some(user) = lookup(id) && user.is_active {
    process(user)
}
```

> Guard-условие работает и для `while`: `while pattern = expr &&
> bool_guard { ... }`. ⚠ Несколько pattern-условий в одном `if`
> (`if Some(x) = a && Some(y) = b`) пока не реализованы — один паттерн
> плюс bool-guard.

Локальные binding'и (`data`, `user`, `line`) доступны **только в теле
блока**. После закрывающей `}` — недоступны.

Подробно — [D34](decisions/03-syntax.md#d34).

## Методы инстанса и static-функции

В Nova — **два вида функций ассоциированных с типом**, различимых по
синтаксису декларации:

```nova
// конструктор / static — через точку, без @
fn Account.new(owner str) -> Account =>
    Account { _balance: 0, owner }

// метод инстанса — через пробел и @, неявный self
fn Account @balance() -> money => @_balance

fn Account @is_solvent() -> bool => @_balance > 0

// мутирующий метод — mut перед @name
fn Account mut @deposit(amount money) {
    @_balance += amount
}
```

**Использование:**

```nova
ro acc = Account.new("alice")    // вызов constructor через точку
acc.deposit(100)                   // вызов метода — точка + скобки
ro bal = acc.balance()            // getter, обязательные скобки
```

### `@field` для доступа к полям

Внутри метода (`@method` или `mut @method`) поля self доступны через
**`@field`** — единственная форма:

```nova
fn Account @summary() -> str =>
    "${@owner}: ${@_balance}"      // = self.owner, self._balance
```

`@.field` **невалидно** — точка не используется. `@field` — единственно
верно.

`@` без поля — это **значение текущего инстанса**:

```nova
fn Account @copy() -> Account => @
fn Account @send_to(tx ChanWriter[Account]) => tx.send(@)
```

### Скобки обязательны для вызова

```nova
acc.balance()              // вызов метода
// acc.@balance            // НЕвалидно — bound method value в Nova нет
Account.@balance           // unbound method value, тип: fn(Account) -> money
|| acc.balance()           // lambda (замена bound): тип fn() -> money
Account.new                // static-функция как значение, тип: fn(str) -> Account
```

Программист и LLM мгновенно различают: вызов = со скобками, значение
= без скобок. Никаких property с побочками.

### Generic'и

```nova
fn HashMap[K, V].new() -> HashMap[K, V] => ...        // generic на типе
fn HashMap[K, V] @get(key K) -> Option[V] => ...      // тоже
fn[T] []T @map[U](f fn(T) -> U) -> []U => ...         // generic на методе [U]
```

Подробно — [D35](decisions/03-syntax.md#d35).

## Embed и delegation: `use Type` и `use name Type`

Композиция вместо наследования. `use` — это **поле + автопрокси методов**:

```nova
type Account {
    owner str
    balance money
}

fn Account mut @deposit(amount money) => @balance += amount

// embed: имя поля обязательно (D39 — alias всегда явный)
type AuditedAccount {
    use account Account
    audit_log []AuditEntry
}

fn AuditedAccount mut @withdraw(amount money) Fail[AuditError] {
    @account.deposit(-amount)               // явный вызов "родителя" через имя поля
    @audit_log.push(AuditEntry.new(amount))
}

ro aa = AuditedAccount { ... }
aa.deposit(100)                              // авто-прокси: account.deposit
aa.balance                                   // авто-прокси: account.balance
```

Имя поля **обязательно** при `use` ([D39](decisions/02-types.md#d39))
— согласовано с [D30](decisions/03-syntax.md#d30) (поля snake_case):

```nova
type Wrapper[K, V] {
    use w HashMapIter[K, V]      // имя поля = "w"
    extra int
}

fn Wrapper[K, V] @next() -> Option[Pair[K, V]] => @w.next()

// конфликт двух embed — псевдонимы обязательны
type Composite {
    use a TimerA
    use b TimerB                  // оба определяют tick() — нужны имена
}
```

**Override.** Метод того же имени на внешнем типе перекрывает прокси.
Доступ к «родительскому» — через имя поля:

```nova
fn AuditedAccount mut @deposit(amount money) {
    @account.deposit(amount)                // вызов оригинала через имя поля
    @audit_log.push(AuditEntry.new(amount))
}
```

**`use` — это не наследование.** `AuditedAccount` не подтип `Account`.
Функции `fn(Account)` принимают `Account`, не `AuditedAccount`. Структурные
интерфейсы — отдельный механизм (см. ниже).

Подробно — [D39](decisions/02-types.md#d39).

## Передача параметров

Объекты (record, sum-type, массивы) передаются **по ссылке** в managed
heap. Примитивы (`int`, `bool`, `f64`, ...) — **по значению**.
Префикс `mut` разрешает мутацию.

```nova
type Account { balance money }    // обычное поле — мутируется у mut binding'а

// без mut — иммутабельный view, мутация запрещена
fn show(acc Account) Io => println("${acc.balance}")

// с mut — мутации видны вызывающему
fn deposit(mut acc Account, amount money) {
    acc.balance += amount
}

mut my_acc = Account { balance: 100 }
deposit(my_acc, 50)
// my_acc.balance == 150  ← мутация видна

show(my_acc)
// показывает 150, my_acc не изменён
```

### Поля типа: `ro` для never-mut, `mut` для cache

```nova
type Account {
    ro id u64                // никогда не меняется (D36)
    ro owner str             // тоже
    balance money                  // мутируется у mut-binding
    closed bool                    // тоже
    mut last_cached_total money    // мутируется ВСЕГДА (для cache/lazy)
}

// group-syntax — несколько полей одного типа через запятую
type Point { x, y, z f64 }
type Color { r, g, b u8 }
```

Подробно про правила мутации полей — [D36](decisions/02-types.md#d36).

| Форма | Передача | Мутация снаружи |
|---|---|---|
| `x int` | by value | нет |
| `o Order` | managed reference | нет (immutable) |
| `mut o Order` | managed reference | да |

Для perf-критичного кода компилятор использует **escape analysis**:
не утекающие значения остаются на стеке, без аллокаций в managed
heap. Программист не пишет ничего особого. Для real-time — атрибут
`#realtime nogc` на функции ([D172 §7](decisions/06-concurrency.md#d172-realtimeblocking-sync-class-annotation-system-plan-1036);
исторически [D64](decisions/04-effects.md#d64)); блочной формы нет.
Arena-allocations через `region { }` —
проектируемая форма ([D6](decisions/05-memory.md#d6)),
⚠ в текущем компиляторе не реализована.

Подробно — [D32](decisions/02-types.md#d32).

## Опциональные параметры — через record + spread, не через defaults

Default-значений у параметров функции в Nova **нет** (намеренно — см.
[history/rejected.md](decisions/history/rejected.md)). Когда у функции
много параметров с разумными дефолтами, используется паттерн
**опции-record + spread**: комбинация record-типа с константой-дефолтом
([D52](decisions/02-types.md#d52)), record-coercion в позиции с
известным типом ([D55](decisions/02-types.md#d55)) и spread `...obj`
для override отдельных полей ([D60](decisions/03-syntax.md#d60)).

```nova
type ServerOpts {
    port     int
    host     str
    max_conn int
    timeout  Duration
}

const SERVER_DEFAULTS ServerOpts = {
    port:     8080,
    host:     "0.0.0.0",
    max_conn: 1024,
    timeout:  30.seconds(),
}

fn serve(opts ServerOpts) Net -> () => ...

// Все дефолты:
serve({ ...SERVER_DEFAULTS })

// Override одного-двух полей:
serve({ ...SERVER_DEFAULTS, port: 9000 })
serve({ ...SERVER_DEFAULTS, port: 9000, max_conn: 4096 })

// Совсем кастом:
serve({ port: 9000, host: "127.0.0.1", max_conn: 16, timeout: 5.seconds() })
```

**Преимущества над default-значениями:**

1. **Все опции видны на call-site** — программист и LLM не гадают что
   значит «остальные дефолты». `...SERVER_DEFAULTS` явно говорит
   «возьми всё остальное оттуда».
2. **Дефолты переиспользуются** — `SERVER_DEFAULTS`, `TEST_DEFAULTS`,
   `DEV_DEFAULTS` для разных сред.
3. **Refactoring безопасен** — добавил поле в record, спред-вызовы
   подхватывают новое поле; вызовы без спреда — compile error «missing
   field», программист увидит каждое место.
4. **Композиция** — несколько spread'ов: `{ ...BASE, ...OVERRIDES, port: 9000 }`.
5. **Без новой грамматики** — работает через существующие D52 + D55 + D60.

**Когда такой паттерн избыточен:**

- Функция имеет **2–3 параметра** без дефолтов — пишутся напрямую:
  `fn move(x int, y int)`.
- Дефолты семантически разные («режимы») — лучше отдельные функции
  или sum-type: `fn parse_strict(s str)`, `fn parse_lenient(s str)`.

Подробно: [D52 record](decisions/02-types.md#d52),
[D55 coercion](decisions/02-types.md#d55),
[D60 spread](decisions/03-syntax.md#d60).

## Эффекты в сигнатуре

Любое взаимодействие с внешним миром — эффект, объявляется между `)` и `->`:

```nova
fn double(x int) -> int                          // чистая
fn parse(s str) Fail -> int                    // может бросить
fn save(u User) Fail Db Log -> ()              // три эффекта
fn fetch(url str) Net Fail -> Response          // сеть + ошибки (async — ambient, не пишется)
```

**`?` и `!!`** — два постфиксных оператора для `Option`/`Result`
([D85](decisions/04-effects.md#d85)):

- `expr?` — ранний return обёртки (нужен `-> Option/Result`).
- `expr!!` — throw через `Fail[E]` (нужен `Fail[E]` в сигнатуре).

```nova
// throw-стиль через !!
fn pipeline(s str) Fail[ParseError] -> int {
    ro n = parse(s)!!
    ro doubled = n * 2
    validate(doubled)!!
    doubled
}

// return-стиль через ?
fn pipeline_r(s str) -> Result[int, ParseError] {
    ro n = parse(s)?
    ro doubled = n * 2
    validate(doubled)?
    Ok(doubled)
}
```

Подробнее — [effects.md](effects.md), [revolutionary.md](revolutionary.md).

## Контракты (опциональны)

```nova
fn withdraw(mut acc Account, amount money) Fail -> ()
    requires amount > 0
    requires acc.balance >= amount
    ensures acc.balance == old(acc.balance) - amount
=>
    acc.balance -= amount
```

Без контрактов код работает как обычно. С ними компилятор пытается
доказать статически, что не может — превращает в runtime-проверку
в debug-режиме.

## Handler'ы — литералы у `protocol`-эффектов

```nova
type Logger effect {
    log(msg str) -> ()
}

fn process(x int) Logger -> int {
    Logger.log("processing ${x}")
    x * 2
}

// handler — обычное значение через keyword `effect` (D61)
ro console = effect Logger {
    log(msg) => println("[LOG] ${msg}")
}

// применение через with
fn main() Io -> () {
    with Logger = console {
        process(42)
    }
}
```

`return value` или финальное выражение в handler-method'е продолжает
вычисление с возвращённым значением. Для досрочного выхода из всего
with-блока — `interrupt v` (D61). `resume` в Nova не существует.

## Имя эффекта в коде — три позиции

```nova
fn process() Db -> ()                // 1. позиция типа
Db.query(sql`...`)                   // 2. операция активного handler'а
ro captured = Db                    // 3. сам активный handler как значение
```

Парсер различает по позиции.

## With-блок — несколько подмен в одном

```nova
test "complex flow" {
    with Logger = collect_into(buf),
         Db = in_memory,
         Time = fixed(t0) {
        process_order(o)
    }
    assert(buf.contains("processed"))
}
```

После `with` — список «эффект = handler-выражение» через запятую,
потом **один** блок тела.

## Параллелизм — без `async/await`

```nova
fn fetch_all(ids []u64) Net Fail -> []User =>
    parallel for id in ids {
        fetch_user(id)
    }
```

Suspension в Nova — ambient runtime-инфраструктура, не эффект и не
специальная конструкция (D62). Тип возврата `[]User`, не
`Future<[]User>`. Подробно — [revolutionary.md R7](revolutionary.md).

`parallel for` — structured concurrency: ждёт всех, отменяет хвост
при ошибке.

## Capability-режим

```nova
fn run_user_script(code str) Fail -> Result =>
    forbid Net, Fs, Db {
        eval(code)
    }
```

Внутри `forbid` компилятор не пропустит вызов функции с запрещёнными
эффектами. Sandbox в типах, не в рантайме.

## Производительность — escape analysis и regions

Программист пишет обычный код:

```nova
fn hot_loop(data []f64) -> f64 =>
    data.iter().sum()  // SIMD-авто, zero-alloc через escape analysis
```

Компилятор сам решает: примитивы — в регистрах, не утекающие
объекты — на стеке, остальное — в managed heap. Никаких ссылок
вручную.

Для real-time hot path — атрибут `#realtime nogc` на функции
([D172 §7](decisions/06-concurrency.md#d172-realtimeblocking-sync-class-annotation-system-plan-1036);
исторически [D64](decisions/04-effects.md#d64)); блочной формы нет. В теле такой
функции запрещены suspend-операции и аллокации в managed heap.
Arena-allocations через `region { ... }` — проектируемая форма
([D6](decisions/05-memory.md#d6)), ⚠ в текущем компиляторе не
реализована.

## Структурные «интерфейсы» — `protocol`

Никаких `interface`/`trait`. Структурный контракт — отдельным keyword
**`protocol`**:

```nova
// именованный
type Printable protocol {
    show() -> str
}

fn log_one(x Printable) Log -> () => Log.info(x.show())

// или прямо в сигнатуре, без имени — анонимный структурный тип
fn log_one(x { show() -> str }) Log -> () => Log.info(x.show())
```

Совместимость **автоматическая** по структуре — любой тип с
подходящими методами автоматически удовлетворяет protocol'у, никаких
`impl`-блоков не нужно. `Self` валиден внутри любого type-контекста
(protocol-блок, effect-блок, instance-метод, static-метод, sum-вариант)
по [D66](decisions/02-types.md#d66):

```nova
type Hash protocol {
    @hash() -> u64
}

type Next[T] protocol {
    mut @next() -> Option[T]
}
```

`type` — для **данных** (record, sum-type, alias). `protocol` — для
**поведения** (методы как контракт). Подробно — [D42](decisions/02-types.md#d42),
[D9](decisions/01-philosophy.md#d9) / [D15](decisions/02-types.md#d15).

## Дженерики

```nova
fn map[T, U](xs []T, f T -> U) -> []U =>
    [f(x) for x in xs]

// дженерик по эффектам — функция наследует эффекты `f`
fn map_eff[T, U, E](xs []T, f (T) E -> U) E -> []U =>
    [f(x) for x in xs]
```

Параметры типа — после имени в квадратных скобках `Имя[T]`, не `<T>`.
Подробно — [D16](decisions/03-syntax.md#d16).
Массивы — `[]T` (динамический), `[N]T` (фиксированный), [D27](decisions/03-syntax.md#d27).

## Generic bounds — `[T Protocol]` или `[T TypeSet]`

Параметр-тип ограничивается через единое правило «name type» (без
двоеточия) — двумя способами: **protocol** (structural, любой тип с
подходящими методами) или **type-set** (D310, ниже — closed список
конкретных типов, membership-предикат, не структурный):

```nova
fn dedup[T Hash](xs []T) -> []T => ...
fn map[K Hash, V](m HashMap[K, V]) -> ...
fn fold[T, Acc](xs Iter[T], init Acc, f fn(Acc, T) -> Acc) -> Acc
```

Bound — это **protocol-тип** ([D53](decisions/02-types.md#d53)). Тот же
`Hash` стоит и в позиции типа значения (existential), и в bound'е
(universal через мономорфизацию):

```nova
fn dump(x Hash) -> u64 => x.hash()        // existential, dynamic dispatch
fn dump2[T Hash](x T) -> u64 => x.hash()  // universal, mono dispatch
```

**Порядок параметров — слева направо.** Имя в bound'е должно быть
объявлено раньше:

```nova
fn get[K, V, C Index[K, V]](c C, k K) -> V => c[k]   // ok: K, V объявлены первыми
fn get[C Index[K, V], K, V](c C, k K) -> V           // ОШИБКА: K, V используются до объявления
```

**Множественные bounds** — через анонимный protocol:

```nova
fn min[T protocol { @compare(other Self) -> int, @equal(other Self) -> bool }](xs []T) -> T
```

Если паттерн повторяется — выносится в именованный protocol (`type Ord
protocol { ... }`).

### Type-set — bound по членству, не по структуре

**Type-set** — четвёртая kind-форма `type` (наряду с newtype/alias/
record-tuple/`enum`, D310): именованное множество **конкретных** типов,
перечисленных явно. В отличие от protocol (любой тип с подходящими
методами удовлетворяет структурно), type-set — closed list, только
явно перечисленные члены проходят:

```nova
// inline — | разделяет члены, перед первым не нужен
type Num set int | f64

// многострочный — | обязателен у каждого члена, включая первый
type AnyNumber set
    | i8 | i16 | i32 | i64 | int
    | u8 | u16 | u32 | u64 | uint

fn[T Num] sum_two(a T, b T) -> T => a + b
```

Диспетч по первому токену после `type Name` (как `enum`/`alias`) — `set`
контекстный, не глобальный keyword. Bound из type-set ведёт себя как
protocol-bound: `[T Num]`. Композиция с протоколами — через `+`: `[T
SignedInt + Hash]` (T ∈ set И реализует Hash). **Не больше одного
type-set** в списке bound'ов (`E_MULTIPLE_TYPE_SETS`) — протоколов
можно сколько угодно.

**Члены — только конкретные типы**, перечисленные по идентичности:
newtype `type MyI8 i8` не входит в `{i8}` автоматически — нужен явный
листинг (`E_TYPE_SET_MEMBER_NOT_CONCRETE` для protocol/effect/другого
type-set как члена). **Один set не смешивает signed/unsigned целые**
(`E_TYPE_SET_MIXED_SIGNEDNESS`) — готовые `SignedInt`/`UnsignedInt` в
prelude (`std/prelude/protocols.nv`) разделены по этой оси.

Подробно — [D72](decisions/02-types.md#d72), [D310](decisions/02-types.md#d310-type-set-bounds-plan-1723).

## Конверсии: `as` и `T.from(v)`

Два способа конверсии под разные сценарии. `from` — имя-конвенция
конструктора-конверсии (не protocol-bound); универсального `v.into()`
(Rust-style, тип цели из контекста) в Nova нет:

```nova
// 1. as — compile-time, тривиальные cast'ы (D54)
ro n = 100 as u32                          // numeric
ro u = 42 as UserId                         // newtype ↔ underlying
ro code = NotFound as int                   // sum → int

// 2. T.from(v) — конвенция конструктора-конверсии, нетривиальная
//    конверсия с runtime-логикой (D73)
type Celsius f64
type Fahrenheit f64

fn Fahrenheit.from(c Celsius) -> Self =>
    Self((c as f64) * 9.0 / 5.0 + 32.0)

ro f1 = Fahrenheit.from(Celsius(100.0))    // static, единственная форма вызова

// Конверсия в строку — частный случай, тот же `from`:
ro s = str.from(42)                         // "42"
ro msg = "id=${user_id}"                    // sugar над str.from(user_id) —
                                              // для пользовательских типов
                                              // через Display/@display
```

**Когда какая форма:**

- **`T.from(v)`** — целевой тип в начале, читается «build a Fahrenheit
  from this Celsius». Единственная форма вызова конверсии — parallel
  instance-формы (`v.into()`) нет.
- Для method-chain — конкретные именованные методы `to_X()`/`into_X()`
  (см. «Договорные конвенции» выше), не generic-конверсия по типу цели.

**Граница `as` vs `T.from`:**

- `as` — bit/tag-уровень, без runtime-кода: `100 as u32`, `id as u64`.
- `T.from` — арифметика, парсинг, валидация: `Fahrenheit.from(c)`,
  `User.from(json)`.

**Граница D73 vs D55:** D55 — automatic coercion для record/sum-литералов
в позиции с известным типом (`ro u User = { id: 1, name: "x" }`).
`T.from(v)` — explicit method call для произвольных типов.

Подробно: [D54](decisions/03-syntax.md#d54), [D73](decisions/08-runtime.md#d73).

## spawn / supervised / parallel for / detach

См. [D14](decisions/06-concurrency.md#d14), [D50](decisions/06-concurrency.md#d50),
[D71](decisions/06-concurrency.md#d71).

### `spawn expr`

`spawn` — keyword-конструкция (не функция). По спеке D50 — разрешён только внутри
structured-scope (`supervised`, в т.ч. `supervised(cancel:)`, `parallel for`,
`select`; и stdlib `race`/`with_timeout` внутри своих тел); вне scope —
compile error.

Внутри scope `spawn` кладёт fiber в очередь и возвращает unit; результат
работы — через захваченные `mut`-переменные или каналы. `spawn() { body }`
с пустыми скобками **запрещён** (нет смысла; `spawn` — не функция).

```nova
supervised {
    spawn fetch_users()           // spawn + вызов функции
    spawn { compute(x) }          // spawn + inline-блок
}
```

#### Тип результата

**`spawn body` возвращает unit, всегда** (D50 + D71).
Результат body не доступен caller'у. Чтобы получить значение от
concurrent-выполнения:

```nova
// (1) прямой вызов — async прозрачный, suspension сама
ro users = fetch_users()

// (2) гомогенный fan-out — массив результатов
ro responses = parallel for url in urls { fetch(url) }

// (3) гетерогенная параллельность — mut-захваты
mut a = 0; mut b = 0
supervised {
    spawn { a = compute_a() }
    spawn { b = compute_b() }
}
```

**`spawn` вне scope = compile error**: bare `spawn` вне
structured-scope не компилируется. (Дополнительно: `spawn` всегда
возвращает unit, поэтому `ro r = spawn { ... }` бессмысленно.)

### `supervised { body }`

Structured-concurrency scope. Все `spawn` внутри ждут scope-exit перед запуском;
scheduler крутит resume по очереди (round-robin) пока все не завершатся. См.
D71 для bootstrap-семантики.

**Value-expression (Plan 173.1 Ф.1; D414 §4).** Возвращает своё
trailing-выражение, вычисленное **после join'а всех детей** (post-join —
мутации детей видны). Void-форма (без trailing) — unit. Прежняя
bootstrap-заглушка «возвращает unit, trailing отбрасывается» снята.

```nova
supervised {
    spawn handle_requests()
    spawn periodic_cleanup()
}                                  // ← ждёт пока обе fiber'ы не завершатся; unit

mut hits = 0
ro total = supervised {
    spawn { hits += fetch_a() }
    spawn { hits += fetch_b() }
    hits                           // ← значение ПОСЛЕ завершения всех детей
}
```

`Time.sleep(0)` внутри `supervised` body (на main-уровне) даёт main-flow yield
к queued fibers'ам — один full pass scheduler'а очереди.

### `parallel for x in iter { body }`

Fan-out parallel map: для каждого элемента `iter` запускается fiber с `body`,
результаты собираются в массив **в порядке завершения (completion order,
Plan 173.1 Ф.2 / D414 §4 — плотный, без дыр; порядок итерации НЕ
гарантирован; нужен порядок — `xs.sort()`)**. Тип возврата — `[]T`, где `T`
— тип `body` (ЛЮБОЙ тип: примитив, record, value-record, tuple, sum,
вложенный `[]T`), итератор — любой (Iter-protocol, без `len()`). Сбор —
через внутренний канал (Sender-клон на spawn → send из ребёнка → close на
выходе; drain-fiber внутри scope; буфер `K = min(len, 16)` back-pressure).
Десугарится в supervised-scope с channel-дренажом.
Loop-переменная захватывается **по value** (snapshot на момент spawn'а).

```nova
// Семантически: параллельный map.
ro responses []Response = parallel for url in urls { fetch(url) }

// Или с inferred return type:
fn fetch_all(urls []str) Net Fail -> []Response =>
    parallel for url in urls {
        fetch(url)
    }
```

**Не путать с обычным `for`!** `for x in iter { body }` — это **statement**
(тип `unit`), тело для side-effects:

```nova
for url in urls {
    Log.info(url)         // только side effect, ничего не возвращается
}
```

Для **sequential map** (собрать массив результатов последовательно) —
использовать `.map()`, не `for`:

```nova
ro names []str = users.map(|u| u.name)
ro names []str = users.map() fn(u) => u.name      // trailing-fn
```

Сводка:

| Форма | Тип | Семантика |
|---|---|---|
| `for x in iter { body }` | `unit` | statement, side-effects |
| `iter.map(\|x\| body)` | `[]T` | sequential map |
| `parallel for x in iter { body }` (body has trailing) | `[]T` | parallel map (fan-out) |
| `parallel for x in iter { body }` (no trailing) | `unit` | parallel side-effect loop |

⚠ Bootstrap-ограничение: array-mode работает для T ∈ {int, bool,
f64, str} и итераторов `a..b`, `a..=b`, array literal. Без trailing — старая
семантика (statement, unit). См. D71 в decisions/06-concurrency.md.

### `detach { body }`

Fire-and-forget: тело пушится на orphan-fiber (глобальный supervisor,
не локальный scope) и исполняется асинхронно — caller возвращается
мгновенно, тело переживает вызывающую функцию. Требует эффекта
`Detach` в сигнатуре (D50; иначе `[E_DETACH_REQUIRES_EFFECT]`).
Без объявления `detach` легален в теле `test`-блока (effect-root)
и под ambient-handler'ом `with Detach = …` (мокинг в тестах).

Ошибка/паника в detached-теле — **LogAndDrop**: лог в stderr, fiber
умирает чисто, процесс и остальные fiber'ы продолжают (у сироты нет
call-site — некому вернуть `Result`).

```nova
fn handle_request(req Request) Net Db Detach -> Response {
    ro resp = process(req)
    detach { write_audit(req, resp) }
    resp
}
```

### `supervised(cancel: tok) { body }`

Structured cancellation с внешним токеном. Обычный `supervised`-scope
с именованным аргументом `cancel:` ([D102](decisions/03-syntax.md#d102-именованные-аргументы-и-значения-параметров-по-умолчанию)).
`tok` — **caller-owned** значение типа `CancelToken`: создаётся
вызывающим кодом, переживает scope, может быть захвачен/передан.
`tok.cancel()` извне валит все fiber'ы scope'а — на следующем
yield-point они бросят `"scope cancelled"`.

```nova
ro tok = CancelToken.new()
supervised(cancel: tok) {
    spawn { do_thing() }
    spawn { do_other() }
}

// внешний kill-switch:
ro tok = CancelToken.new()
spawn { Time.sleep(5_000); tok.cancel() }
fetch_with_kill(urls, tok)
```

Token capabilities: `tok.cancel()`, `tok.is_cancelled()`,
`tok.bind(other)` для каскадной отмены. Один токен — один живой scope
(bind-check). Подробно — [D75](decisions/06-concurrency.md#d75-supervisedcancel-tok--структурная-отмена-с-внешним-токеном).

### `Channel[T]` и `select`

Coordination между fiber'ами через message-passing. `Channel[T]` —
typed bounded channel с blocking-семантикой. **Единственный safe
способ** разделять данные между fiber'ами в production-runtime
(альтернатива — shared `mut` — UB при preemption).

```nova
ro (tx, rx) = Channel[T].new(10)     // -> (ChanWriter[T], ChanReader[T]); cap 10 (0 = unbuffered)
tx.send(value)                       // ЗАБИРАЕТ владение `value` (consume, D79/D91-амендмент);
                                      // блокирует если буфер полон; -> bool (false = закрыт)
ro v = rx.recv()                     // Option[T]; None = closed + drained
tx.close()                            // idempotent

// drain pattern:
while Some(msg) = rx.recv() {
    process(msg)
}
```

`send`/`try_send` **забирают владение** отправленным значением — после
`tx.send(value)` переменная `value` недоступна (использование = compile
error, существующая линейная проверка D131). Причина: канал не копирует и не
изолирует буфер — общий указатель на кучу без передачи владения был бы
гонкой данных под M:N по построению (два fiber'а на разных ОС-потоках
мутируют один объект). Намеренное разделение доступа к каналу (не значения!)
— через `tx.share()` (доп. writer-handle на тот же буфер, D91), не через
повторное использование уже отправленного значения.

`select { ... }` — мультиплексирование recv-операций с опциональным
`timeout` case:

```nova
select {
    Some(msg) = rx_a => process_a(msg)
    Some(msg) = rx_b => process_b(msg)
    Some(_) = ChanReader.close_after(Duration.from_secs(5)) => default_action()
}
```

Если несколько арм'ов готовы одновременно — выбор псевдослучайный
(Fisher-Yates shuffle, D94). Select-арм — Option-паттерн на ридере:
`Some(v) = rx => …` (готов при значении) / `None = rx => …` (готов при
closed-channel). Отдельного `<-`-оператора нет.

Полная семантика (closed-channel, owner-actor pattern, отказ от
Mutex/Atomic) — [D79](decisions/06-concurrency.md#d79); `select` —
[D94](decisions/06-concurrency.md#d94).

### `Time.sleep(ms)`

Yield-point. По D62 — обычная функция, callable откуда угодно (Async ambient).
Семантика: блокирует текущий fiber на не менее чем `ms` миллисекунд.

**Реализация (Plan 22 Ф.4):** под капотом — libuv `uv_timer_t`. Fiber
паркуется через park/wake API ([D93](decisions/06-concurrency.md#d93))
до срабатывания timer-callback'а. Scheduler в это время резюмит других
fiber'ов либо идёт в `uv_run UV_RUN_ONCE` (kernel-wait, CPU idle).

| Контекст | Реализация |
|---|---|
| Внутри fiber-body (spawn) внутри supervised | park-on-`uv_timer_t` (D93) — CPU idle, реальное время |
| Вне fiber, внутри `supervised` body | drain queue пока deadline не пройдёт (Plan 22 Ф.5 → libuv-driven main) |
| Полностью вне scope | native OS sleep (Plan 22 Ф.5 → implicit main-scope, libuv) |

Cancel ([D75](decisions/06-concurrency.md#d75-supervisedcancel-tok--структурная-отмена-с-внешним-токеном)) прерывает sleep
**немедленно** через generic `stop_cb` mechanism (D93): cancel-token
закрывает таймер и wake'ает parked fiber, который throw'ает `"scope
cancelled"`. Не нужно ждать срабатывания timer'а.

`Time.sleep(0)` — fast yield (один scheduler-pass, ~µs).

## Тестирование без моков

`test "name" { body }` — тест-блок верхнего уровня. Имя — строковый
литерал (любые символы, обычно человеческое описание поведения).
Тело — обычный блок выражений; `assert(cond)` — функция из prelude
([D26](decisions/08-runtime.md#d26)), обязательно со скобками как любой
fn-call.

```nova
test "withdraw decreases balance" {
    with Db = in_memory_db([acc1, acc2]) {
        ro acc = Account.new("alice")
        acc.deposit(100)?
        acc.withdraw(30)?
        assert(acc.balance == 70)
    }
}

test "insert and get" {
    mut m = HashMap[str, int].new()
    m.insert("a", 1)
    assert(m.get("a") == Some(1))
    assert(m.get("b") == None)
}
```

Тесты собираются и запускаются только под `nova test`. В обычной сборке
тело пропускается — никаких `#[cfg(test)]`-обвязок. Эффекты подменяются
теми же `with`-блоками что и в проде, никакого mock-фреймворка.

## Panic — не эффект, ловится только runtime'ом

Деление на ноль, выход за границы массива, переполнение — это
**не эффект**, это `Panic`. Программист **не ловит panic в коде** —
panic означает смерть текущего fiber'а, runtime обрабатывает на границе:

```nova
fn mean(xs []int) -> int =>
    xs.sum() / xs.len()                  // никакого Fail[DivByZero]

fn handle(r Request) Db Log -> Response =>
    process(r)             // если panic — fiber умирает, runtime вернёт 500
```

`panic` — это смерть **fiber'а**, не процесса. В сервере падает только
текущий запрос, остальное работает. Если нужно гарантированно гасить
процесс — отдельная функция `exit(code int, msg str) -> never`
([D13](decisions/08-runtime.md#d13)).

Подробно — [revolutionary.md R11](revolutionary.md), [D13](decisions/08-runtime.md#d13).
