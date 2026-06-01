// SPDX-License-Identifier: MIT OR Apache-2.0
# Cancel and Cleanup — D188 R3 cancel-shield + D90 §7 cancel-as-Failure

> **Plan 110 Ф.14.2 Q-block.** Q-cancel-and-cleanup: how cancellation
> interacts с ConsumeScope cleanup semantics.
> Cross-ref [D188 R3](../../spec/decisions/03-syntax.md#d188),
> [D90 §7](../../spec/decisions/03-syntax.md#d90),
> [D191](../../spec/decisions/03-syntax.md#d191) async cleanup.

## TL;DR

1. **Cancel в body** → arrives как `ScopeOutcome::Failure(CancelError)`
   в `on_exit`. Resource decides graceful vs aggressive cleanup.
2. **Cancel-shield by default**: cancel-delivery suspended во время
   `on_exit` execution до `exit_timeout`. Prevents cleanup-cancel
   storm (Plan 110.2 implementation).
3. **Cancel after on_exit completion** → continues propagation
   normally; outer fail-frame catches.

## D90 §7 amend — Cancel as `Failure(CancelError)` (Plan 110.5.6)

Pre-Plan 110 (D90 original): cancel в body bypassed cleanup. Plan 110
amends: cancel arrives через ConsumeScope outcome path as `Failure`
variant с `CancelError` payload.

```nova
type Connection { /* fields */ }

fn Connection consume @on_exit(outcome ScopeOutcome) Fail[IoError] -> () {
    match outcome {
        Success      => @flush()?
        Failure(msg) => {
            // Bootstrap: str-prefix discrimination per Plan 110.5.6.
            if msg.starts_with("cancel: ") {
                // Cancel path — graceful shutdown.
                @send_eof_quick()
                @close()?
            } else {
                // Regular failure — abort connection.
                @abort()?
            }
        }
        Panic(_) => @abort()  // bug — emergency cleanup
    }
}
```

**Bootstrap** (Plan 110.5.6): codegen prefix `"cancel: "` к
`Failure(msg)` when `frame.error_kind == NOVA_THROW_CANCEL`. Full
typed `if err is CancelError` narrowing после
[`[M-110-multierror-any]`](../plans/110-scoped-resources-radical-simplification.md#multierror-payload-any-migration)
payload migration.

## D188 R3 — Cancel-shield by default (Plan 110.2)

**Problem без shield:** programmer writes `consume tx = db.begin() {
do_work() }`. User cancels mid-`on_exit` (e.g., rollback). Half-rolled
transaction leaves DB в inconsistent state.

**Solution (D188 R3):** cancel-delivery suspended during `on_exit`
body execution до `exit_timeout` resolution (D192 3-level fallback).
Cancel continues после `on_exit` completes OR timeout exceeded
(`CleanupTimeoutError`).

```nova
// Cancel during outer body
fn process() Fail[DbError] -> () {
    consume tx = db.begin() {
        await long_op()              // cancel arrives here
        // Body never resumes; longjmp → outcome=Failure(CancelError).
    }
    // tx.on_exit fires WITH SHIELD:
    //   - rollback executes без cancel interruption
    //   - tx.commit() (если success path) — runs to completion
    //   - cancel re-delivered после on_exit return.
}
```

**Timeout** (D192): `exit_timeout_ms()` 3-level fallback:
1. `WithExitTimeout` impl: `fn Tx @exit_timeout_ms() -> int => 30_000`.
2. `Application` effect: `with Application = handler(default_exit_timeout_ms: 10_000)`.
3. Hardcoded fallback: `5_000` ms.

Cleanup exceeding timeout → `CleanupTimeoutError` injected into
ongoing suspend; cleanup truncates; cancel re-raised.

## D191 — Async cleanup (suspend в `on_exit`)

`on_exit` body может contain `await` / suspend operations (TCP
grace-close, DB commit с network round-trip):

```nova
fn TcpStream consume @on_exit(outcome ScopeOutcome) Fail[IoError] -> () {
    match outcome {
        Success | Failure(_) => {
            await @send_eof()?         // suspend OK; shield active
            await @wait_ack(timeout_ms: 1000)?
            @close()?
        }
        Panic(_) => @close()           // sync emergency
    }
}
```

**Cancel during await в `on_exit`:** cancel delivery deferred до
timeout exceedance (D192). При exceedance → `CleanupTimeoutError`
injected into current suspend; propagates через `?`.

## Realtime contexts (D198)

Inside `#realtime` fn, `exit_timeout` forced to `Duration.zero`.
`await` в `on_exit` → `D192-zero-timeout-suspend` runtime error.

```nova
#realtime
fn rt_use_lock() -> () {
    consume g = mu.lock() {     // MutexGuard: Consumable[never], realtime-OK
        do_realtime_work()       // no suspend allowed
    }
    // g.on_exit (unlock) — sync, instant.
}
```

`Mutex.unlock` is realtime-compatible (atomic op). Resources that
require `await` в cleanup (TCP, DB) — NOT usable в `#realtime`.

## Cancel-shield error composition (D193)

If cleanup `on_exit` itself throws while body was cancel-routed:

```
body cancel → outcome = Failure(CancelError("user-cancelled"))
on_exit throws CleanupError("rollback failed")
↓
MultiError composed:
  primary:    CancelError("user-cancelled")  [original cause]
  suppressed: [CleanupError("rollback failed")]  [secondary]
```

Caller обходит chain через `MultiError.walk()` / `find_first_panic()`.

## Pattern catalog

### Pattern 1: Graceful cancel cleanup

```nova
fn Connection consume @on_exit(outcome ScopeOutcome) Fail[IoError] -> () {
    match outcome {
        Success => @close()?
        Failure(msg) => {
            if msg.starts_with("cancel: ") {
                await @send_eof()?         // graceful
                @close()?
            } else {
                @abort()?                  // aggressive
            }
        }
        Panic(_) => @abort()
    }
}
```

### Pattern 2: Infallible cleanup (`Consumable[never]`)

```nova
fn MutexGuard consume @on_exit(_outcome ScopeOutcome) -> () => @unlock()
```

`Fail[never]` ≡ infallible. Caller не declares `Fail[E]`. Hot-path
optimization eligible (Plan 110.1.7).

### Pattern 3: Outcome-aware metric emission

```nova
fn HttpRequest consume @on_exit(outcome ScopeOutcome) -> () {
    match outcome {
        Success      => @metrics.inc("http.success")
        Failure(msg) => {
            if msg.starts_with("cancel: ") {
                @metrics.inc("http.cancel")
            } else {
                @metrics.inc("http.error")
            }
        }
        Panic(_)     => @metrics.inc("http.panic")
    }
    @release_pool_slot()
}
```

## See also

- [Q-cleanup-semantics](consume-scope-cleanup.md) — overview.
- [Q-consumable-protocol](consume-scope-cleanup.md#q-consumable-protocol)
  — implementation guide.
- [D188 R3 cancel-shield](../../spec/decisions/03-syntax.md#d188).
- [D90 §7 amend](../../spec/decisions/03-syntax.md#d90).
- [D191 async cleanup](../../spec/decisions/03-syntax.md#d191).
- [D192 timeout taxonomy](../../spec/decisions/03-syntax.md#d192).
- [D193 MultiError](../../spec/decisions/03-syntax.md#d193).
- [D198 realtime + cleanup](../../spec/decisions/03-syntax.md#d198).
- [cleanup-cookbook.md](../cleanup-cookbook.md) — production recipes.
