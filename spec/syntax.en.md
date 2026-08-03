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
