// SPDX-License-Identifier: MIT OR Apache-2.0
# Async cleanup — graceful drain через defer + suspend

> Practical guide для [D159](../../spec/decisions/03-syntax.md#d159)
> (Plan 100.4.2). Когда cleanup нужен I/O (graceful socket close,
> DB connection drain, etc.).

## Когда нужен async cleanup

✅ **Используй async cleanup для:**
- Graceful socket close (FIN+ACK exchange — Net.* в defer).
- DB connection drain (pending queries — Channel.recv в defer).
- Async transaction commit/abort (network round-trip).
- Buffered writer flush до close.

❌ **НЕ нужен async cleanup для:**
- Mutex.unlock (instant).
- Mem alloc release (instant).
- Counter decrement, simple log.

## Базовый pattern

```nova
fn process() Fs Time -> () {
    consume conn = open_connection()?
    defer { conn.graceful_drain() }             // suspend-able (D159)
    do_io(conn)?
}
```

D159 снимает D90 §5 «no-suspend» — `Time.sleep`, `Channel.recv`,
`Net.*`, `Fs.*` теперь допустимы в defer body.

## Cancel-safe semantics

Если outer scope cancelled mid-cleanup, **cleanup completes ПЕРВЫМ**,
cancel propagates AFTER:

```nova
supervised(cancel: tok) {
    spawn {
        consume conn = open_db()?
        defer { conn.drain(timeout: 5_s) }      // shielded от cancel
        long_op()
        // outer cancels:
        //   1. spawned fiber unwinding
        //   2. defer fires: conn.drain() — shielded
        //   3. drain completes (или timeout)
        //   4. cancel propagates наверх
    }
}
```

Аналог Kotlin `withContext(NonCancellable) { cleanup() }` — но
автоматически.

## `Time.timeout` для bounded cleanup

```nova
defer {
    with Time.timeout(5_s) {
        socket.graceful_close()                 // если >5s — abort cleanup
    }
}
```

Programmer обязан bound long cleanups через `Time.timeout`. Иначе —
infinite hang risk.

## Что нельзя в defer body

❌ **`spawn`** — error D159-spawn-in-defer. Новый fiber в cleanup утечёт
supervised scope.

❌ **`parallel for`** — same reason.

❌ **`return` / `throw` / `break` top-level** (D90 §6 unchanged).

✅ **Допустимо:**
- `Time.sleep(ms)`
- `Channel.recv()` / `select`
- `Net.*` (TCP/UDP read/write)
- `Fs.*` (file I/O)
- consume-методы с suspend в body.

## Async cleanup + failable composition (D158 + D159 combined)

```nova
fn process() Fail Time Net -> () {
    consume sock = open_socket()?
    defer {
        with Time.timeout(5_s) {
            sock.graceful_close()?              // failable + async
        }
    }
    do_network_io()?
    // На любой exit-path: cleanup runs (suspend OK, shielded);
    // если cleanup fails — composes через Plan 49 multi-error.
}
```

Combines D158 (failable body) + D159 (async/suspend) — production-grade
graceful shutdown.

## Сравнение

| Capability | Rust | TS (ES2024) | Kotlin | Nova D159 |
|---|---|---|---|---|
| Async cleanup body | ⏳ Rust 2024+ WIP | ✅ `await using` | ✅ coroutine `use{}` | ✅ `defer` body suspend |
| Cancel-safe shielding | ⚠️ manual shielded | ✅ AbortSignal | ✅ `withContext(NonCancellable)` | ✅ **shield-by-default** |
| Timeout bounded cleanup | ⚠️ manual | ⚠️ AbortSignal.timeout | ✅ `withTimeout` | ✅ `Time.timeout` integration |

Nova matches Kotlin coroutine + TS await using. Шилдинг автоматический
— не требует explicit keyword'а.

## Связь

- [D159](../../spec/decisions/03-syntax.md#d159) — async/suspend в
  cleanup body.
- [D90](../../spec/decisions/03-syntax.md#d90) §5 — amend (no-suspend
  → suspend allowed).
- [D158](../../spec/decisions/03-syntax.md#d158) — failable cleanup
  body; combines с D159.
- [D85](../../spec/decisions/04-effects.md#d85) — Plan 49 cancel-routing.
- [Plan 22](../plans/22-sleep-libuv-integration.md) ✅ — async/libuv
  foundation.
- [cleanup-on-failure idiom](cleanup-on-failure.md) — broader defer
  family patterns.
- Plan 100.4.2 — `100.4.2-async-suspend-cleanup.md`.
