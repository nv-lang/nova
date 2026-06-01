// SPDX-License-Identifier: MIT OR Apache-2.0
# Q-async-cleanup — Suspend в `consume{}` on_exit (D191)

> **Plan 110 Ф.14.2 Q-block.** Q-async-cleanup в контексте Plan 110
> `consume X = ... { body }` scope-block: suspend operations в
> Consumable.on_exit body — TCP grace-close, DB commit round-trip,
> distributed-lock release.
>
> Sister Q-block: [async-cleanup.md](async-cleanup.md) (Plan 100.4
> defer-family pre-Plan 110 reference).
> Cross-ref [D191](../../spec/decisions/03-syntax.md#d191),
> [D192](../../spec/decisions/03-syntax.md#d192) 3-level timeout,
> [D198](../../spec/decisions/03-syntax.md#d198) realtime.

## TL;DR

`Consumable.on_exit` body **may** suspend (`await`, network/db I/O).
Cancel-shield (D188 R3) prevents cancel storm during cleanup. Timeout
enforced via `CleanupTimeoutError` injection (Plan 110.2). Realtime
contexts (`#realtime` fn) forbid suspend в `on_exit` (D198).

## When Suspend Is Required

| Resource | Cleanup suspend reason |
|---|---|
| `TcpStream` | Graceful EOF + ack wait |
| `Database connection` | Commit/rollback round-trip |
| `Distributed lock` | Network release acknowledgement |
| `Message queue subscriber` | Drain in-flight + commit offset |
| `Disk file (synced)` | fsync to disk (large flush) |

## Pattern: TCP Grace Close

```nova
type TcpStream { /* opaque */ }

fn TcpStream consume @on_exit(outcome ScopeOutcome) Fail[IoError] -> () {
    match outcome {
        Success | Failure(_) => {
            await @send_eof()?
            await @wait_ack(timeout_ms: 1000)?
            @close()?
        }
        Panic(_) => @close()    // sync emergency
    }
}

fn TcpStream @exit_timeout_ms() -> int => 5000   // D192 Level-1
```

## Cancel Behavior During Async Cleanup

**Without cancel-shield:** cancel arrives during `await @wait_ack(...)`
→ cleanup truncates, connection leak.

**With cancel-shield (D188 R3, Plan 110.2):**
- Cancel delivery suspended until `exit_timeout_ms` exceeded.
- Cleanup `await` completes naturally → `close()` runs.
- Cancel propagates после `on_exit` returns.

**Timeout exceedance:** `CleanupTimeoutError` injected into ongoing
`await`. Cleanup propagates через `?`. MultiError composed (D193):
- primary: original outcome (`Failure(...)` or `Success`).
- suppressed: `CleanupTimeoutError`.

## Forbidden Operations в `on_exit`

```nova
fn Resource consume @on_exit(_outcome ScopeOutcome) Fail[E] -> () {
    // ❌ spawn { ... }                  // D159 — async cleanup невозможен
    // ❌ parallel for x in xs { ... }   // same — concurrent cleanup
    // ❌ supervised { ... }             // same — supervisor cleanup
    await sync_io()?               // ✅ suspend OK
    @final_step()?                  // ✅
}
```

D159 (Plan 100.4.2 closure) restricts cleanup to **sequential** flow.
Concurrent cleanup tasks unobserved → resource leaks.

## Realtime Contexts (D198)

```nova
#realtime
fn rt_use() -> () {
    consume g = mu.lock() { do_work() }
    //          ^^^^^^^^^^ exit_timeout = 0 enforced
    // g.on_exit (MutexGuard.unlock) — sync, instant. OK в #realtime.

    consume tcp = TcpStream.connect(addr)? { send_data() }
    //          ^^^^^^^^^^^^^^^^^^^^^^^^^ tcp.on_exit awaits → D192-zero-
    //          timeout-suspend runtime error в #realtime context.
}
```

D198: `#realtime` forces `exit_timeout = 0`; suspend в `on_exit` during
scope → runtime error. Resources requiring async cleanup NOT usable
в `#realtime` context.

## 3-Level Timeout Resolution (D192)

1. **Level 1** — resource-specific: `fn T @exit_timeout_ms() -> int`.
2. **Level 2** — application-wide: `Application.default_exit_timeout_ms()`.
3. **Level 3** — hardcoded fallback: `5_000` ms.

```nova
// Level 1 wins
fn Transaction @exit_timeout_ms() -> int => 30_000

with Application = Application.handler(default_exit_timeout_ms: 10_000) {
    consume tx = db.begin() { ... }              // 30_000 (Level 1)
    consume sock = TcpStream.connect(addr)? { ... } // 5000 (Level 1 of TcpStream)
    consume g = mu.lock() { ... }                 // 10_000 (Level 2)
}

consume g2 = mu.lock() { ... }    // 5_000 (Level 3 fallback)
```

## Decision Tree: Choosing Timeout

```
Resource has typical cleanup duration?
├─ YES, instant (< 1ms): no Level 1; rely on Application/hardcoded
│   (Mutex, Permit, Channel send)
├─ YES, bounded (< 1s typical): set Level 1 = 2-5x typical
│   (DB commit local, file close, atomic lock)
├─ YES, network-dependent (1-30s): set Level 1 explicitly
│   (TCP grace, DB commit remote, message ack)
└─ NO, can be slow (> 30s): set Level 1 high + monitor via OTel
    (large flush, distributed-lock release across regions)
```

## See also

- [D191 async cleanup](../../spec/decisions/03-syntax.md#d191).
- [D192 timeout taxonomy + 3-level](../../spec/decisions/03-syntax.md#d192).
- [D198 realtime + cleanup](../../spec/decisions/03-syntax.md#d198).
- [D188 R3 cancel-shield](../../spec/decisions/03-syntax.md#d188).
- [async-cleanup.md](async-cleanup.md) — pre-Plan 110 defer-family reference.
- [Q-cancel-and-cleanup](cancel-and-cleanup.md).
- [Q-application-effect](application-effect.md).
- [cleanup-cookbook.md §2.4 TCP grace close](../cleanup-cookbook.md).
