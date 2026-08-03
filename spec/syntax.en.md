---
source_rev: 615e2fa7e
source_date: 2026-08-02
---

> **Informative translation; the Russian text is normative.**

# Nova — syntax

<!-- Editing rule: this document describes the language IN THE PRESENT
     TENSE — "as is". The history of changes (what was retracted,
     when and why) lives in spec/decisions/ (D-blocks with retraction
     banners) — do not move it here. Allowed exceptions:
     (a) honesty markers "not yet implemented" with a plan link;
     (b) a short "Nova has no X — use Y" for constructs familiar from other
     languages, without dates or retraction numbers. -->

## Minimal examples

```nova
// Hello world — никаких main, package, import для stdlib
print("hello")

// Чистая функция: нет эффектов, нет ошибок, детерминирована
fn double(x int) -> int => x * 2
```

## Tagged template literals — `tag\`...\``

A literal with a tag prefix is processed by the `tag` function. Returns
the type chosen by the function (not necessarily `str`):

```nova
ro j = json`{"name": "alice"}`              // -> Json
ro q = sql`SELECT * FROM users WHERE id = ${user_id}`   // -> Sql, безопасно
ro r = regex`\d+\.\d+`                       // -> Regex, raw
```

A byte blob is a separate `x"…"` literal (hex digits → `[]u8`), not a
tagged template: `ro b = x"deadbeef"` (D412; ⚠ not yet implemented —
[Plan 186](../docs/plans/186-hex-blob-embed.md)).

**Interpolation via `${expr}`** — the tag function receives the parts and
arguments **separately**, which provides safety (protection from SQL
injection):

```nova
sql`SELECT * FROM users WHERE name = ${name}`
// → sql(["SELECT * FROM users WHERE name = ", ""], [name])
// функция передаёт name как параметр, не склеивает в строку
```

**Multiline** works naturally. Escapes: `` \` ``, `\\`, `\${` — literal.
The rest of the characters — raw (convenient for regex and SQL).

**Standard tags (stdlib MVP plan):** `json`, `sql`, `regex`.

**Your own tag** — an ordinary function:

```nova
export fn url(parts []str, args []str) -> Url => ...

ro u = url`https://api.example.com/users/${user_id}`
```

Details — [D48](decisions/03-syntax.md#d48).

## String interpolation — `"... ${expr} ..."`

In an ordinary string literal `"..."` (without a tag prefix) expression
interpolation via `${expr}` is allowed. This is **sugar** over concatenation
with `str.from(...)`:

```nova
ro name = "alice"
ro age  = 30
ro s = "Hello, ${name}, you are ${age}"
// = "Hello, " + str.from(name) + ", you are " + str.from(age)
```

Each `${expr}` is rendered via `str.from(v)` — primitives and
prelude types get it automatically; a user type
hooks in by implementing `Display` (`@display(mut w Write)`,
[D73](decisions/08-runtime.md#d73)). A literal `${` in a string — via
escape: `"\${name}"`.

Details — [D44 → "Строковые литералы и интерполяция"](decisions/03-syntax.md#d44).

## Statement separator: newline or `;`

A **newline** separates statements. **`;` is optional** —
needed only for several statements on one line:

```nova
ro x = 1                        // newline разделяет
ro y = 2
foo(x, y)

ro a = 1; ro b = 2; foo(a, b)  // ; для одной строки
```

A newline is **ignored** in positions where the statement continues:

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

**Binary operators — at the end of the line** (Go-style), not at the start:

```nova
ro total = a +              ✅
            b
ro total = a
          + b                 ❌ парсится как унарный +b
```

Details — [D49](decisions/03-syntax.md#d49).

## Numeric literals

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

**Default types** without context: `int` for integers, `f64` for floats. With
an annotation/context — the context type is used:

```nova
ro x u8 = 200             // 200 это u8
ro arr []f32 = [1.0, 2.0]
```

**Type-suffixes (`100u32`, `1.5f32`) are not introduced.** For rare
disambiguation cases — an `as`-cast: `100 as u32`, `0xFF as u8`.

**The `_` separator is allowed only between digits**, not consecutively, not
at the start/end, not right after a prefix (`0x_FF` ❌), not around the dot
or `e`. Details — [D44](decisions/03-syntax.md#d44).

## Type annotations — the "name type" form, without a colon

Unlike TypeScript/Rust (`name: Type`), Nova does not use `:` as the
name/type separator — only a space, `name type`:

```nova
fn save(u User, amount money) Fail Db -> ()    // параметры
ro users []User = []                            // ro
type User { id u64, name str }                   // поля типа
for id u64 in ids { ... }                        // for-loop
```

`:` is not used for types in Nova at all — only as a **key-value separator**
in literals:

```nova
ro alice = User { id: 1, name: "alice" }       // record-литерал
ro cfg = { "host": "localhost", "port": 8080 } // dict-литерал
```

## Return: `->` is mandatory, `()` is optional

```nova
fn compute(x int) -> int => x * 2    // явный тип возврата
fn log_event(e Event) Log            // -> () можно опускать
fn save(u User) Fail Db            // эффекты + dropped -> ()
```

## Closure: light `|...|` and full `fn(...)`

Nova has two closure forms ([D22](decisions/03-syntax.md#d22-closure-light--и-full-fn)):

**closure-light** — a compact untyped form, the body is a bare expr or block:
```nova
ro inc   = |x| x + 1
ro zero  = || 0
ro block = |x| { ro y = x*2; y + 1 }
ro any   = |_| 0                            // wildcard

list.filter(|x| x > 0)
list.fold(0, |acc, x| acc + x)
m.get_or_insert("k", || 0)
```

`|...|` is valid **only when the context unambiguously determines the
signature** (a fn-call parameter, an annotated `ro` binding, a return
position, first-use inference). Without context — switch to `fn(...)`.

**closure-full** — a typed form, identical to a named fn without a name.
The body `=> expr` or `{ block }`:
```nova
ro typed    = fn(x int) -> int => x * 2
ro block    = fn(x int, y int) -> int { ro z = x+y; z * 2 }
ro with_eff = fn(req Request) Db Log -> Response { process(req) }
```

Effects in closure-light are **not written** — they are inherited from the
ambient effect set (= the enclosing function's effects ∪ active
`with`-blocks). If a closure body uses an effect unavailable in the parent —
compile error. closure-full declares effects explicitly, like a named fn.

## Trailing — a block/function argument after the call parentheses

If the last parameter of a function is of functional type, the argument can
be moved out of the call's `()` into one of two forms:

**trailing-block** — for callbacks **without parameters** (DSL):
```nova
with_timeout(2.seconds()) {
    Db.exec(sql`UPDATE counters SET v = v + 1`)
}

retry(3) {
    Net.get(url)
}
```

**trailing-fn** — for callbacks **with parameters**, syntax
identical to closure-full without a name:
```nova
list.filter() fn(x) => x > 0
list.fold(0) fn(acc, x) { acc + x }
list.map() fn(s str) -> Result[int, ParseError] { parse(s)? }
```

**Rules:**
- `{` (for trailing-block) or `fn` (for trailing-fn) on the same
  line as `)`. A line break is forbidden.
- `()` are mandatory (even empty).
- The last parameter's type is functional.
- One trailing per call.
- `|...|` (closure-light) **in a trailing position is forbidden** —
  pass it via args (`f(|x| body)`) or use `fn(...)`.

`spawn` is a keyword construct, not a function, so it does not obey
the D43 rule. Its syntax is described separately below.

**When trailing-fn vs closure-light in args:**
- `f(|x| body)` — more compact for one-liners.
- `f(args) fn(x) { ... }` — better for long bodies with bindings;
  visually marks "this is a block argument to the call".

Details — [D22](decisions/03-syntax.md#d22-closure-light--и-full-fn),
[D43](decisions/03-syntax.md#d43-trailing-block--без-params-fnp-body-с-params).

## Function body: `=>` for an expression, `{}` for a block

Two **mutually exclusive** ways:

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

**The `-> T` rule — two different levels, don't confuse:**

1. **Grammar (a compile error if violated).** In a **block-body**
   (`{ ... }`) `-> T` is **mandatory**, if the type is not `()` — the compiler
   does not infer the type from the block (`return_type_c` does inference only
   for an Expr body; for a Block body without an annotation — `()`, see "What
   was rejected" in [D45](decisions/03-syntax.md#d45)). In an
   **expression-body** (`=> expr`) `-> T` is **always optional** — the type
   is inferred from the body. `-> T` mandatory everywhere — a consciously
   rejected option (noise for trivial one-liners).
2. **Style-guide (a linter warning, not a compile error).** For
   **`export` functions** (public API) it is recommended to write `-> T`
   explicitly, even in an expression-body — the linter warns if omitted. That
   is documentation and contract stability, not a grammar
   requirement: `export fn f(x int) => x * 2` without `-> int`
   **compiles**, but gets a lint warning.
   For private functions and tiny helpers (getters, predicates,
   constructors) — omitting is fine, no warning is emitted.

**Indentation is not significant.** `fn f() => stmt1; stmt2` or a multiline
without `{}` — an error. If there is more than one step — `{}` is mandatory.

**If the `=>` body is a record literal, the type is named exactly once** —
not TIMTOWTDI (two equivalent ways), but the only correct spelling
for each of the two states of the signature (Plan 51 Ф.2, "removes
the only live TIMTOWTDI in the spelling of record literals"):

```nova
// -> T опущен → тип обязан быть в литерале
fn Duration @plus(other Duration) => Duration { nanos: @nanos + other.nanos }

// -> T присутствует → в литерале имени типа быть НЕ должно
fn Duration @plus(other Duration) -> Duration => { nanos: @nanos + other.nanos }
```

Both variants write the same function, but are not interchangeable — each
signature state (with or without `-> T`) has exactly one
allowed literal form. Mixing is forbidden by the compiler in both
directions:

- `-> Duration => Duration { ... }` (the type in the signature AND in the
  literal) — a compile error:

  ```
  error: redundant type prefix on record literal — the return type
  `-> Duration` already declares it; write `=> { ... }`
  ```

- `=> Duration { ... }` without `-> Duration` in the signature, if the type
  is needed also outside (`export`, non-obvious inference) — the linter
  requires an explicit `-> T` (see the style-guide rule above); there is no
  grammar-level error here, but there is no ambiguity either — the type is
  always the single source of truth.

`-> Self` resolves to the receiver's type — the same rule: `-> Self =>
Counter { ... }` in a `Counter` method is also redundant (redundant type
prefix). Sum-coercion (`-> Shape => Circle { ... }`, a literal of a different
name than the return type) is not affected by this rule — there the literal's
name must remain, because `Circle ≠ Shape`.

Details — [D40](decisions/03-syntax.md#d40), [D45](decisions/03-syntax.md#d45).

## Operator overloading

Standard operators automatically call methods with fixed names:

```nova
fn Duration @plus(other Duration) => Duration { nanos: @nanos + other.nanos }
fn Duration @times(n i64) => Duration { nanos: @nanos * n }

ro total = 1.hour() + 30.minutes()       // вызывает @plus
ro triple = 5.seconds() * 3              // вызывает @times
if elapsed > 1.second() { ... }           // вызывает @compare
```

| Operator | Method | | Operator | Method |
|---|---|---|---|
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

`==`/`!=` — via `@equal` (the `Equal` protocol, `!=` is derived by negation); `<`/`<=`/`>`/`>=` — via the single `@compare(o) -> int` (the `Compare` protocol, memcmp-style: `< 0` / `0` / `> 0`). Indexing `a[i]` / `a[i] = v` — `@index` / `mut @index` (the `Index[K, V]` / `MutIndex[K, V]` protocols, D240); slice indexing `a[x..y]` — the same `@index`, overloaded by parameter type: `x..y` (half-open, does not include `y`) is lowered by the compiler into `Range { start: x, end: y }`, and `a.index(r Range)` is called — on `[]T`/`str` it returns a view without copying (`std/collections/vec/slice.nv`, `std/runtime/string/slice.nv`). `&&`/`||` are **not overloadable** (short-circuit
semantics). **The bitwise family — a `bit` prefix, and `~` separate from `!`** (D46-amendment 2026-07-27, plan [234](../docs/plans/234-bitwise-operator-family.md)): `&`/`|`/`^` → `@bitand`/`@bitor`/`@bitxor` (the former `@and`/`@or`/`@xor` are retracted — they read as LOGICAL, though the logical `&&`/`||` are not overloadable at all); `~a` → `@bitnot()` — bitwise complement, overloadable by user types (`~x == -(x+1)` on signed), whereas `!a` stays LOGICAL and (D46-AMEND 2026-08-02) is not overloadable at all — only `bool`, `@not()` is retracted. Compound assignments: `+=`/`-=`/`*=`/`/=` and (D46-amendment (C), plan 234 Ф.2а) `&=`/`|=`/`^=`/`<<=`/`>>=` — desugar into `a = a <op> b`, no separate operator methods. Custom operators (`:+`, `<>`) are not allowed. Details —
[D46](decisions/03-syntax.md#d46).

## Mathematical operations on numeric types

Standard mathematical functions on `f64` / `f32` / `int` are declared
as **instance methods** via `@`, not as static `Math.sin(...)`.
This is consistent with D35 (methods are the main mechanism for type-bound
functions) and gives chain-friendly formulas:

```nova
ro r = (x * x + y * y).sqrt()
ro phi = im.atan2(re)
ro dist = a.hypot(b)
ro s = (theta + offset).sin()
```

**The standard set on `f64` (prelude):**

| Category | Methods |
|---|---|
| Roots and powers | `@sqrt()`, `@cbrt()`, `@pow(exp f64)` |
| Trigonometry | `@sin()`, `@cos()`, `@tan()`, `@asin()`, `@acos()`, `@atan()` |
| `atan2` (two-arg) | `@atan2(x f64) -> f64` (`y.atan2(x)`) |
| Hyperbolic | `@sinh()`, `@cosh()`, `@tanh()` |
| Exponential / log | `@exp()`, `@exp2()`, `@ln()`, `@log10()`, `@log2()` |
| Norm / distance | `@abs()`, `@hypot(other f64)` |
| Rounding | `@floor()`, `@ceil()`, `@round()`, `@trunc()` |
| Min / clamp | `@min(other f64)`, `@max(other f64)`, `@clamp(lo f64, hi f64)` |
| Predicates | `@is_finite()`, `@is_nan()`, `@is_infinite()` |

On `int` the set is limited: `@min`, `@max`, `@clamp`, `@compare`.

**Names worth noting:**

- **`@hypot(other)`** / **`@atan2(x)`** — two-argument functions;
  the second argument comes as a parameter; the receiver is the first
  argument by mathematical convention (`y.atan2(x)`, `a.hypot(b)`).

**Static functions on the type** for cases with no natural receiver:

```nova
f64.PI                   // константа
f64.E                    // константа
f64.NAN                  // константа
f64.INFINITY             // константа
f64.try_parse(s str) -> Option[f64]
```

## Naming conventions

| What | Style | Example |
|---|---|---|
| Types, effects, protocols, sum variants | **PascalCase** | `User`, `HashMap`, `Db`, `Hash`, `Some` |
| Generic parameters | **PascalCase, single-character** | `T`, `K`, `V`, `E` |
| Functions, methods (`@name`), parameters, fields | **snake_case** | `parse_url`, `@deposit`, `user_id`, `created_at` |
| Constants (`const`) | **SCREAMING_SNAKE_CASE** | `MAX_PAYLOAD`, `DEFAULT_TIMEOUT` |
| Modules | **snake_case** via dots | `module admin.audit`, `module std.duration` |

**Acronyms — PascalCase, not UPPERCASE.** `Db`, not `DB`. `Http`, not `HTTP`.
`Json`, not `JSON`. `Url`, not `URL`. Rule: an acronym is an ordinary word.

**Reserved method names** (operator overloading, [D46](decisions/03-syntax.md#d46)):
`@plus`, `@minus`, `@times`, `@div`, `@rem`, `@neg`, `@bitand`, `@bitor`,
`@bitxor`, `@bitnot`, `@shl`, `@shr`, `@equal`, `@compare`, `@index`.
(`@not` RETRACTED 2026-08-02 — `!` is no longer overloadable.)
Do not use them for other purposes.

**Contract conventions:**
- `T.new(...)` — the standard constructor; `T.from(v X)` — the name
  convention of the constructor-conversion ([D73](decisions/08-runtime.md#d73); this is exactly a
  naming convention, no protocol mechanics behind it);
  `T.from_X(...)` — a domain constructor when `from(v)` does not convey
  the meaning (`from_secs`, `from_polar`, `from_imag`).
- `@to_X()` — transformation into a new owning value, when a view
  (zero-copy) does not exist in principle (`to_str()`, `to_upper()`,
  D410). `consume @into_X()` — a consuming ownership transfer
  (`into_str()`, `into_raw()`, D131). A universal `v.into()`
  (Rust-style, target type from context) does **not** exist in Nova — only
  concrete named methods.
- `Display`/`@display(mut w Write)` — string representation for
  `${expr}` interpolation and `str.from(v)` on a user type
  ([D73](decisions/08-runtime.md#d73)).
- `@hash()` — hash, `@clone()` — copy, `@iter()`/`@next()` — iterator.
- **Error names** ([D30](decisions/03-syntax.md#d30)) — with a type / domain:
  `ParseComplexError`, `ParseIntError`, `DbError`, `OverflowError`.
  Do not use generic `ParseError`, `ValueError`, `Exception` —
  import collisions, ambiguity for AI.

The `@as_X()`, `@is_X()` convention is **not introduced** — it duplicates
existing mechanisms:
- `@as_X()` duplicates the `as` keyword (D54) for cheap casts or
  `X.from` for nontrivial ones.
- `@is_X()` duplicates `v is X` (D54): for sum types and `any`
  the `is` operator works directly (`shape is Circle`,
  `arg is int` for `arg any`). To extract the variant value
  with a binding — `if X(n) = v` (D34).
- Field privacy — the `priv` modifier; the `_`-prefix for "privacy by
  contract" is not used in Nova (details —
  ["Visibility: export"](#видимость-export-для-публичных-деклараций) below).
- Test names — natural-language strings: `test "insert and get"`,
  not `"test_insert_and_get"`.

### Reserved identifiers

Besides the grammar keywords, Nova has identifiers with special
semantics known to the compiler. They can be locally overridden,
but that is an anti-pattern (the linter warns).

**Special types:**
- `Self` — referential type, refers to the receiver type of a method or the
  type satisfying a protocol ([D66](decisions/02-types.md#d66)).
  Valid in any type context.
- `any` — the top type for runtime type-check ([D54](decisions/03-syntax.md#d54)).
- `never` — the bottom type for non-returning functions.

**Prelude types:**
- `Option[T]`, `Some(v)`, `None` — sum type
- `Result[T, E]`, `Ok(v)`, `Err(e)` — sum type
- `Error` — the record `{ msg str }` for `throw err`
- `RuntimeError` — sum of bottom-level runtime errors
- `RuntimeNoneError` — unit type, thrown via `expr!!` on `Option` ([D85](decisions/04-effects.md#d85))
- `Effect[E]` — first-class type of an effect handler
- `Display` — protocol with the instance method `@display(mut w Write)`,
  string representation ([D73](decisions/08-runtime.md#d73))

**Standard effects:**
- `Fail[E]`, `Fail` — the failable effect
- `Io`, `Net`, `Db`, `Fs`, `Time`, `Random`, `Log`, `Trace` — the main ones
- `Ask[T]` — Reader-style context
- `Alloc[R]` — allocation in a region
- `Detach` — the marker of fire-and-forget tasks ([D50](decisions/06-concurrency.md#d50)).
  Blocking calls and real-time — **not effects**, but function attributes:
  `#blocking` (offload to a threadpool) and `#realtime` (forbid
  parking/alloc in the body) — D172.

**Primitive types (lowercase, an exception to the PascalCase rule):**
- `int`, `uint`, `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`
- `f32`, `f64`
- `str`, `bool`, `char` (a byte is `u8`, there is no separate `byte` type)

Details — [D30](decisions/03-syntax.md#d30), [D46](decisions/03-syntax.md#d46), [D47](decisions/07-modules.md#d47).

## Visibility: `export` for public declarations

`export` before a declaration = public (visible outside the module).
Without `export` = private (visible only inside the module).

Applied uniformly to **types**, **functions**, **methods**,
**constants**, and **protocols**:

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

**Record fields:** without `priv`, fields of an `export` type are public by
default (D47). Privacy — the `priv` modifier (`priv`/`priv(type)`/`priv(file)`,
D220 + D281) on a **field** (`priv internal_id u64`) or on a **type**, setting
the default for all fields (`type Job priv { ... }`) — a field is physically
unavailable outside, the compiler checks it. The `_`-prefix as
"privacy by contract" is not used in Nova — privacy
only compile-time, via `priv`.

**Canonical field access — same-name property methods via
arity-based overloading** (D84 + D117):
read `@x() -> T` (0 arguments), write `mut @x(v T) -> @`
(1 argument, fluent — receiver return automatic, D409, no need to write
`return @`/`=> @` in the body):

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

`get_x`/`set_x` pairs — **not the canon** (there are 0 of them in std).
`with_x(v)` — a different operation (a copy with a replaced field, not
mutating the original). All new std code is written in the
accessor-convention paradigm.

Details — [D47](decisions/07-modules.md#d47), [D29](decisions/07-modules.md#d29) (modules).

## Type declarations

| After `type Name` comes | What it is |
|---|---|
| `enum` | sum-type (D406; `enum` is a contextual identifier marker, not a lexer keyword) |
| `set` | type-set — a generic bound by membership in an explicit list of types (D310; also contextual) |
| `(` | tuple structure |
| `{` | record structure |
| `alias` | alias |
| identifier/type | newtype |
| nothing | unit type |

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

`enum` — a marker in the type grammar, valid in any type position, not
only in `type X enum ...`: a parameter (`fn job(a enum A | B)`), a return
(`fn parse() -> enum Ok(int) | Err(str)`), a field, a binding. The named form
(`type Foo enum A | B`) — just declaring a name for the inline
type expression `enum A | B`, one grammar.

Sum variants can have numeric discriminants with auto-increment:

```nova
type ExitStatus enum Ok | Failure | Critical              // 0, 1, 2 (auto)
type ErrorCode enum
    | NotFound       = 404
    | Unauthorized   = 401
    | InternalError  = 500
type Bit u8 enum Off = 0 | On = 1                          // явный базовый тип
```

> ⚠ **`type X <base> enum …` (explicit base type) not yet implemented** —
> parser drift, see [Plan 105](../docs/plans/105-sum-type-explicit-base.md).
> Only the forms without a base type work (implicit `int`).

Details — [decisions/02-types.md → D406](decisions/02-types.md#d406-sum-type-синтаксис-enum-маркер),
revision [D52](decisions/02-types.md#d52).

### Sum-type variants — the same three forms as a top-level type

Each sum-type variant is declared by the same rules as a top-level
declaration:

| After the variant name | What it is | Example |
|---|---|---|
| `( ... )` | positional variant | `Some(T)`, `Ok(T)`, `Point(f64, f64)` |
| `{ ... }` | record variant | `Circle { radius f64 }` |
| nothing | unit variant | `None`, `Red`, `Origin` |

```nova
type Option[T] enum
    | Some(T)                 // позиционный — несёт значение T
    | None                    // unit — без полей, само по себе значение

type Shape enum
    | Circle { radius f64 }   // record-вариант
    | Point(f64, f64)         // позиционный
    | Origin                  // unit
```

`None` is a value of type `Option[T]`, **not a function and not a constructor**.
Used without parentheses:

```nova
ro x = Some(42)              // позиционный — нужен аргумент
ro y = None                  // unit — без скобок
```

Details — [D17](decisions/02-types.md#d17).

## Creating values and pattern matching

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

Each arm has the form `pattern => result`, optionally with a **guard**
`pattern if condition => result`. The compiler tries arms top-down,
takes the first one where the pattern matched AND the guard is true.

**Kinds of patterns:**

| Form | Example | What it does |
|---|---|---|
| Literal | `0`, `"hello"`, `true` | comparison by value |
| Name (binding) | `n`, `x` | catches any value, binds it to a name |
| Wildcard | `_` | catches any value, binds nothing |
| Constructor | `Some(v)`, `Ok(value)`, `None` | destructures a sum-type variant |
| Record | `User { id, name }` | destructures record fields |
| Tuple | `(a, b)`, `(_, value)` | destructures a tuple |
| Guard | `n if n < 0` | a pattern + an extra condition |

**Exhaustiveness check.** The compiler checks that the match covers all
possible cases. If not — an error naming the uncovered variant. This works
for sum types and bool. For general types (`int`, `str`) you need either a
`_`-wildcard or an explicit check of all considered values.

```nova
type Color enum Red | Green | Blue

fn name(c Color) -> str => match c {
    Red   => "red"
    Green => "green"
    // ОШИБКА: missing variant `Blue`
}
```

`match` is an **expression**, returns a value. All arms must have
a compatible type (or a common supertype, or wrapped in a sum type).

### Record literals and patterns

**Shorthand** — when the field name matches a variable name in scope:

```nova
ro key = "alice"
ro value = 42

ro entry = Entry { key, value }                 // shorthand обязателен (D52)
ro entry = Entry { key, value, extra: "data" }  // можно смешивать
// `Entry { key: key }` — ОШИБКА: используйте shorthand `{ key }`.
```

**Partial pattern matching** — specifying only the needed fields:

```nova
match @buckets[idx] {
    Occupied { value }     => Some(value)        // partial: key игнорируется
    Occupied { value, .. } => Some(value)        // явный .. — то же самое
    _                      => None
}
```

Both forms are valid (`..` or without) — a choice by context. `..` —
a signal "the type has more fields". Without — shorter.

**Renaming on destructuring:**

```nova
Occupied { key: k, value }      // key переименовано в k, value совпадает
```

Details — [D17](decisions/02-types.md#d17).

### `for` / `while` / `loop` loops

```nova
for x in list { ... }            // x — immutable binding на каждой итерации
for mut x in list { ... }         // x можно мутировать в теле
for x int in nums { ... }         // явный тип элемента
for mut id u64 in ids { ... }     // mut + явный тип элемента
for (i, x) in list.iter().enumerate() { ... }   // индекс через iterator-адаптер

while cond { ... }                // условный цикл
loop { ... }                      // бесконечный, выход через break/return
```

**An explicit element type — `for x TYPE in iter`** — is optional and
follows the universal "name type" rule (like `ro x int`, `fn(x int)`,
`[T Bound]`). The annotation is **checked by the compiler**: if `TYPE` does
not match the iterator's actual element type — a compile error. That makes it
a *checked assertion* (pins the expectation; a change of the source type →
a loud error), not a silent documenting sugar. Go/Rust/TS
do not give a loop-variable annotation at all — Nova has it as a strict,
checkable superset.

A variable in `for x in iter` — an **immutable binding** (like `ro`, no
`mut`), receiving a **new value** on each iteration. It cannot be
reassigned in the block body:

```nova
for x in list {
    x = 5                         // ОШИБКА: x immutable
}

for mut x in list {
    x = transform(x)              // ок
}
```

This is consistent with the D32 + D33 rule — all bindings are immutable by
default, mutation explicitly via `mut`. There is no `const` or `final`
marker in Nova — immutability is already the default.

`break` / `continue` — standard.

### A pattern in a condition — `if pattern = …` / `while pattern = …`

A pattern match right in the condition — a short alternative to `match` for
a single variant:

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

> The guard condition works for `while` too: `while pattern = expr &&
> bool_guard { ... }`. ⚠ Several pattern conditions in one `if`
> (`if Some(x) = a && Some(y) = b`) are not yet implemented — one pattern
> plus a bool-guard.

Local bindings (`data`, `user`, `line`) are available **only in the block
body**. After the closing `}` — unavailable.

Details — [D34](decisions/03-syntax.md#d34).

## Instance methods and static functions

Nova has **two kinds of functions associated with a type**, distinguishable
by the declaration syntax:

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

**Usage:**

```nova
ro acc = Account.new("alice")    // вызов constructor через точку
acc.deposit(100)                   // вызов метода — точка + скобки
ro bal = acc.balance()            // getter, обязательные скобки
```

### `@field` for field access

Inside a method (`@method` or `mut @method`), self's fields are accessible
via **`@field`** — the only form:

```nova
fn Account @summary() -> str =>
    "${@owner}: ${@_balance}"      // = self.owner, self._balance
```

`@.field` is **invalid** — a dot is not used. `@field` — the only
correct form.

`@` without a field — the **value of the current instance**:

```nova
fn Account @copy() -> Account => @
fn Account @send_to(tx ChanWriter[Account]) => tx.send(@)
```

### Parentheses are mandatory for calls

```nova
acc.balance()              // вызов метода
// acc.@balance            // НЕвалидно — bound method value в Nova нет
Account.@balance           // unbound method value, тип: fn(Account) -> money
|| acc.balance()           // lambda (замена bound): тип fn() -> money
Account.new                // static-функция как значение, тип: fn(str) -> Account
```

The programmer and the LLM instantly distinguish: a call = with
parentheses, a value = without. No properties with side effects.

### Generics

```nova
fn HashMap[K, V].new() -> HashMap[K, V] => ...        // generic на типе
fn HashMap[K, V] @get(key K) -> Option[V] => ...      // тоже
fn[T] []T @map[U](f fn(T) -> U) -> []U => ...         // generic на методе [U]
```

Details — [D35](decisions/03-syntax.md#d35).

## Embed and delegation: `use Type` and `use name Type`

Composition instead of inheritance. `use` is a **field + auto-proxy of
methods**:

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

The field name is **mandatory** with `use` ([D39](decisions/02-types.md#d39))
— consistent with [D30](decisions/03-syntax.md#d30) (fields snake_case):

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

**Override.** A method of the same name on the outer type shadows the proxy.
Access to the "parent" — via the field name:

```nova
fn AuditedAccount mut @deposit(amount money) {
    @account.deposit(amount)                // вызов оригинала через имя поля
    @audit_log.push(AuditEntry.new(amount))
}
```

**`use` is not inheritance.** `AuditedAccount` is not a subtype of `Account`.
Functions `fn(Account)` take `Account`, not `AuditedAccount`. Structural
interfaces are a separate mechanism (see below).

Details — [D39](decisions/02-types.md#d39).

## Parameter passing

Objects (record, sum-type, arrays) are passed **by reference** into the managed
heap. Primitives (`int`, `bool`, `f64`, ...) — **by value**.

The `mut` prefix allows mutation.

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

### Field kinds: `ro` for never-mut, `mut` for cache

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

Details about field mutation rules — [D36](decisions/02-types.md#d36).

| Form | Passing | External mutation |
|---|---|---|
| `x int` | by value | no |
| `o Order` | managed reference | no (immutable) |
| `mut o Order` | managed reference | yes |

For perf-critical code the compiler uses **escape analysis**:
non-escaping values stay on the stack, without managed-heap allocations.
The programmer writes nothing special. For real-time — the attribute
`#realtime nogc` on a function ([D172 §7](decisions/06-concurrency.md#d172-realtimeblocking-sync-class-annotation-system-plan-1036);
historically [D64](decisions/04-effects.md#d64)); no block form. Arena
allocations via `region { }` — a
designed form ([D6](decisions/05-memory.md#d6)),
⚠ not implemented in the current compiler.

Details — [D32](decisions/02-types.md#d32).

## Optional parameters — via record + spread, not defaults

Functions in Nova have **no default parameter values** (deliberately — see
[history/rejected.md](decisions/history/rejected.md)). When a function has
many parameters with reasonable defaults, the **options-record + spread**
pattern is used: a combination of a record type with a default constant
([D52](decisions/02-types.md#d52)), record-coercion in a position with a
known type ([D55](decisions/02-types.md#d55)) and spread `...obj`
to override individual fields ([D60](decisions/03-syntax.md#d60)).

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

**Advantages over default values:**

1. **All options are visible at the call site** — the programmer and the
   LLM do not guess what "the rest of the defaults" means. `...SERVER_DEFAULTS`
   explicitly says "take everything else from there".
2. **Defaults are reused** — `SERVER_DEFAULTS`, `TEST_DEFAULTS`,
   `DEV_DEFAULTS` for different environments.
3. **Refactoring is safe** — added a field to the record, spread calls
   pick up the new field; calls without spread — a compile error "missing
   field", the programmer sees every place.
4. **Composition** — several spreads: `{ ...BASE, ...OVERRIDES, port: 9000 }`.
5. **No new grammar** — works via existing D52 + D55 + D60.

**When such a pattern is redundant:**

- A function has **2–3 parameters** without defaults — written directly:
  `fn move(x int, y int)`.
- The defaults are semantically different ("modes") — better separate
  functions or a sum-type: `fn parse_strict(s str)`, `fn parse_lenient(s str)`.

Details: [D52 record](decisions/02-types.md#d52),
[D55 coercion](decisions/02-types.md#d55),
[D60 spread](decisions/03-syntax.md#d60).

## Effects in the signature

Any interaction with the outside world is an effect, declared between `)` and `->`:

```nova
fn double(x int) -> int                          // чистая
fn parse(s str) Fail -> int                    // может бросить
fn save(u User) Fail Db Log -> ()              // три эффекта
fn fetch(url str) Net Fail -> Response          // сеть + ошибки (async — ambient, не пишется)
```

**`?` and `!!`** — two postfix operators for `Option`/`Result`
([D85](decisions/04-effects.md#d85)):

- `expr?` — an early return of the wrapper (needs `-> Option/Result`).
- `expr!!` — throw via `Fail[E]` (needs `Fail[E]` in the signature).

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

Details — [effects.md](effects.md), [revolutionary.md](revolutionary.md).

## Contracts (optional)

```nova
fn withdraw(mut acc Account, amount money) Fail -> ()
    requires amount > 0
    requires acc.balance >= amount
    ensures acc.balance == old(acc.balance) - amount
=>
    acc.balance -= amount
```

Without contracts the code works as usual. With them the compiler tries
to prove statically; what it cannot — turns into a runtime check in
debug mode.

## Handlers — literals for `protocol`-effects

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

`return value` or the final expression in a handler-method continues
the computation with the returned value. For an early exit from the whole
with-block — `interrupt v` (D61). `resume` does not exist in Nova.

## The effect name in code — three positions

```nova
fn process() Db -> ()                // 1. позиция типа
Db.query(sql`...`)                   // 2. операция активного handler'а
ro captured = Db                    // 3. сам активный handler как значение
```

The parser distinguishes by position.

## With-block — several substitutions in one

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

After `with` — a comma-separated list of "effect = handler-expression",
then **one** body block.

## Concurrency — without `async/await`

```nova
fn fetch_all(ids []u64) Net Fail -> []User =>
    parallel for id in ids {
        fetch_user(id)
    }
```

Suspension in Nova is ambient runtime infrastructure, not an effect and not a
special construct (D62). The return type is `[]User`, not
`Future<[]User>`. Details — [revolutionary.md R7](revolutionary.md).

`parallel for` — structured concurrency: waits for all, cancels the tail
on error.
