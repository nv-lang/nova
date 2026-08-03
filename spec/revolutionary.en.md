---
source_rev: dcdf639fa
source_date: 2026-05-31
---

> **Informative translation; the Russian text is normative.**

# Nova — revolutionary features

This document describes the features that make Nova not "just another good
language", but a language with a unique claim. All of them follow from one
central idea (see [decisions/01-philosophy.md#d10](decisions/01-philosophy.md#d10)):

> **Everything is an effect. A handler is a first-class function. Killer use-case —
> AI-first programming.**

---

## R1. Algebraic effects + handlers

### Idea

Network, disk, time, randomness, logging, errors, mutation — all of these are
effects. An effect is declared via `effect`, has operations, and a
**handler** intercepts the operations and decides what to do with them.

This is a generalization of `try/catch`, `async/await`, dependency injection,
and mocks into one thing: test mocks, transaction wrappers, retry,
distributed tracing — everything is written through the same handler
mechanism, not through four different libraries.

### Basic syntax

```nova
// объявление эффекта
type Logger effect {
    log(msg str) -> ()
}

// функция, использующая эффект
fn process(x int) Logger -> int {
    Logger.log("processing ${x}")
    x * 2
}

// handler — обычное значение через `handler` keyword
ro console = effect Logger {
    log(msg) => println("[LOG] ${msg}")
}

// применение handler'а
fn main() Io -> () =>
    with Logger = console {
        process(42)   // напечатает [LOG] processing 42
    }
```

`return value` (or a final expression) in a handler method — the resumption
of the computation with the returned value. To complete the whole
`with`-block early, `interrupt v` is used (that is how `Fail` works).

**The special case of `Fail[E]`.** The `Fail[E].fail` operation has the
return type `never` — there is nothing to return to the `throw` point. So a
`Fail[E]` handler has only two outcomes: `interrupt v` (complete the
with-block) or a fresh `throw` (rethrow further). The "return value" form
is forbidden for `Fail`.

Roles in error handling:

- **`throw err`** — language syntax, raises an error. After
  `throw` control never returns to that point.
- **`Fail[E]`** — the effect contract for catching and handling the
  error. An effect has no fields, only operation signatures.
- **a `Fail[E]` handler** — what catches the error. It has no fields
  of its own, but it captures variables from the environment (like an
  ordinary closure).

### What follows from this automatically

**Testing without mocks:**

```nova
test "process logs correctly" {
    mut buf = []
    ro collect = effect Logger {
        log(msg) { buf.push(msg); return () }
    }
    with Logger = collect {
        process(42)
    }
    assert(buf == ["processing 42"])
}
```

No mock library. No DI framework. This is **just a handler**.

**Transactions:**

```nova
type Db effect {
    query(q Sql) -> []DbRow
    exec(q Sql)  -> ()
}

fn transactional(real Effect[Db]) -> Effect[Db] => effect Db {
    query(q) => return real.query(q)
    exec(q)  { staged.push(q); return () }
}

with Db = transactional(real_db) {
    transfer(1, 2, 100)
    transfer(2, 3, 50)
}  // обе операции в одной транзакции, при ошибке — откат
```

A transaction is a handler. Nested transactions are nested handlers.

**Capability security:**

```nova
fn untrusted_plugin(input str) Logger -> str {
    // плагин может только логировать; Net/Db/Fs недоступны
    Logger.log("plugin called")
    input.reverse()
}
```

If the plugin tries to use `Net.get`, the **compiler will not let it
through** — the `Net` effect is absent from the signature. This is capability
security in types, not in the runtime.
