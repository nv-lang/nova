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
