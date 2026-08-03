**English** | [Русский](cleanup-cookbook.ru.md)

// SPDX-License-Identifier: MIT OR Apache-2.0
# Cleanup Cookbook — production recipes for `consume X = expr { body }`

> **Plan 110 Ф.14.8.** A production-recipe book for the Nova V3
> cleanup family — migration patterns from Go/Rust/TS/Java/Kotlin,
> common resource patterns (connection pools, file handles, transactions,
> locks), anti-patterns + debugging, performance tips.

## Section 1 — Migration patterns

### 1.1 From the Rust `Drop` trait

```rust
// Rust:
struct File { fd: i32 }
impl Drop for File {
    fn drop(&mut self) { unsafe { close(self.fd); } }
}

fn read(path: &str) -> Result<String, IoError> {
    let f = File::open(path)?;
    f.read_all()  // drop fires implicitly
}
```

```nova
// Nova:
type File { fd int }

fn File consume @cleanup(_outcome ScopeOutcome) -> () => @do_close()

fn read(path str) Fail[IoError] -> str {
    consume f = File.open(path)? {
        f.read_all()?    // @cleanup fires explicitly
    }
}
```

**Difference**: Nova `consume {}` makes cleanup **visible** at call-site
(no implicit drop magic). Async cleanup via `suspend` inside `@cleanup`
(D191) works out of the box — Rust's async-Drop remains unresolved.

### 1.2 From Go `defer`

```go
// Go:
func process(db *DB) error {
    tx, err := db.Begin()
    if err != nil { return err }
    defer func() {
        if r := recover(); r != nil {
            tx.Rollback()
            panic(r)
        }
    }()
    if err := doWork(tx); err != nil {
        tx.Rollback()  // manual rollback на error
        return err
    }
    return tx.Commit()
}
```

```nova
// Nova:
fn Transaction consume @cleanup(outcome ScopeOutcome) Fail[DbError] -> () {
    match outcome {
        Success   => @commit()?
        Failure(_) => @rollback()?
        Panic(_)  => @rollback_emergency()
    }
}

fn process(db Db) Fail[DbError] -> () {
    consume tx = db.begin()? {
        do_work()?
    }
    // commit/rollback по outcome — автоматически.
}
```

**Difference**: the Go programmer manually distinguishes the success/error
paths, repeating the rollback logic. Nova auto-routes via `outcome`.

### 1.3 From Java try-with-resources

```java
// Java:
try (Transaction tx = db.begin()) {
    doWork();
}  // tx.close() called; commit/rollback в close() impl manually
```

```nova
// Nova:
consume tx = db.begin()? {
    do_work()?
}
// outcome routing встроено в Cleanup.@cleanup
```

**Difference**: Java's `AutoCloseable.close()` does not distinguish success
from error (the programmer must encode that inside the close() body). In
Nova, outcome is first-class.

### 1.4 From TypeScript `using`

```typescript
// TS (ES2024):
{
    using tx = await db.begin();
    await doWork();
    // tx[Symbol.asyncDispose]() called
}
```

```nova
// Nova:
consume tx = await db.begin()? {
    await do_work()?
}
```

**Difference**: TS `using` has no cancel-shield-by-default; cancel
delivery during `Symbol.asyncDispose` can break cleanup.

### 1.5 From Kotlin `.use{}`

```kotlin
// Kotlin:
file.use { f ->
    f.readText()
}  // f.close() called
```

```nova
// Nova:
consume f = file {
    f.read_text()?
}
```

**Difference**: Kotlin's `.use{}` is an extension function on `Closeable`.
Nova's is a first-class language feature with typed error dispatch.

## Section 2 — Resource patterns

### 2.1 Database Transaction

```nova
type Transaction { conn Connection, id int }

fn Transaction consume @cleanup(outcome ScopeOutcome) Fail[DbError] -> () {
    match outcome {
        Success      => @conn.commit(@id)?
        Failure(err) => {
            if err is DbError.Deadlock {
                @conn.rollback(@id)?           // graceful, retry-friendly
            } else {
                @conn.rollback_force(@id)?     // hard rollback
            }
        }
        Panic(_) => @conn.rollback_force(@id)
    }
}

// Optional: per-instance timeout
fn Transaction @exit_timeout_ms() -> int => @conn.config.tx_timeout_ms

fn process_order(db Db, order Order) Fail[OrderError] Db -> Receipt {
    consume tx = db.begin()? {
        ro id = db.insert_order(order)?
        db.notify_warehouse(id)?
        return Receipt { order_id: id }
    }
}
```

### 2.2 File handle

```nova
type File { fd int }

fn File consume @cleanup(_outcome ScopeOutcome) Fail[IoError] -> () =>
    @do_close()?

fn read_config(path str) Fail[IoError] -> Config {
    consume f = File.open(path, mode: ReadOnly)? {
        ro raw = f.read_all()?
        Config.parse(raw)?
    }
}
```

### 2.3 Mutex / locks — Cleanup[never] hot-path

```nova
// stdlib:
type MutexGuard { /* runtime opaque */ }
fn MutexGuard consume @cleanup(_outcome ScopeOutcome) -> () => @release()

// usage:
fn increment_counter(state State) -> () {        // no Fail[E]!
    consume _l = state.mutex.acquire() {         // Cleanup[never]
        state.value += 1
    }
}
```

**Hot-path optimization** (D194 §perf): codegen elides the shield/timeout/
outcome machinery for `Cleanup[never]` with no `WithExitTimeout` — it
compiles down to `state.value += 1; state.mutex.release()`. Zero overhead
vs. a raw lock+release pair.

### 2.4 TCP socket with a grace close

```nova
type TcpStream { /* opaque */ }

fn TcpStream consume @cleanup(outcome ScopeOutcome) Fail[IoError] -> () {
    match outcome {
        Success => {
            @send_eof()?
            @wait_for_ack(timeout_ms: 1000)?
            @close()?
        }
        Failure(_) => @close()?         // abort cleanup, no graceful
        Panic(_)   => @close()
    }
}

fn TcpStream @exit_timeout_ms() -> int => 5000   // grace close может занять время

fn handle_request(addr str) Fail[IoError] Net -> () {
    consume sock = TcpStream.connect(addr)? {
        sock.write_all(request)?
        sock.read_all()?
    }
}
```

### 2.5 Connection pool

```nova
type PooledConn { pool ConnPool, conn Conn }

fn PooledConn consume @cleanup(_outcome ScopeOutcome) -> () => {
    @pool.release(@conn)             // return to pool, не close
}

fn query(pool ConnPool, sql str) Fail[DbError] -> Rows {
    consume conn = pool.acquire()? {
        conn.execute(sql)?
    }
}
```

`Cleanup[never]` — release in pool never fails (atomic pool op).

### 2.6 Builder pattern (raw `consume`, not a scope-block)

```nova
type StringBuilder consume {
    mut buf []u8
}

fn StringBuilder consume @as_str() -> str => str.from_bytes_unchecked_steal(@buf)

fn build_url(parts []str) -> str {
    mut sb = StringBuilder.new()
    for p in parts {
        sb.append(p)
    }
    sb.as_str()    // consume — финальное преобразование, не cleanup
}
```

Don't use `consume X = ... { }` for transfer patterns — there is no cleanup.

## Section 3 — Application lifecycle pattern

```nova
fn main() Io -> () {
    with Application = Application.handler(default_exit_timeout_ms: 10_000) {
        // setup phase
        ro server = HttpServer.bind(":8080")?

        // deep в каком-то конструкторе:
        // Application.register_finalizer(|| metrics.flush())

        server.serve()?
    }
    // handler.on_exit fires finalizers в reverse-order (LIFO topo)
}
```

`default_exit_timeout_ms: 10_000` raises the default for **all** `consume{}`
blocks (that don't have their own `WithExitTimeout` impl) to 10 seconds.
Level 2 in D192's 3-level resolution.

### Test isolation

```nova
fn test_user_creation() Io -> () {
    with Application = Application.handler() {
        Application.register_finalizer(|| reset_test_db())
        run_test_scenario()
    }
    // finalizers fire здесь, не shareятся с другими tests
}
```

D195 R2/R3: a nested Application has its own empty registry + its own
default timeout (5s hardcoded if none is given).

## Section 4 — FFI cleanup wrappers

### 4.1 SQLite Connection (Plan 100.5 cross-ref)

```nova
external type SqliteConn

external fn sqlite_open(path str) -> SqliteConn
external fn sqlite_close(conn SqliteConn) Fail[IoError] -> ()

// Wrap external resource в Cleanup:
fn SqliteConn consume @cleanup(_outcome ScopeOutcome) Fail[IoError] -> () =>
    sqlite_close(@)?

fn query_users(db_path str) Fail[IoError] -> []User {
    consume conn = sqlite_open(db_path) {
        conn.query("SELECT * FROM users")?
    }
}
```

`cancellation-safety` attestation for the C-side: see Plan 100.5 + Plan 110.7.

### 4.2 libcurl handle

```nova
external type CurlHandle
external fn curl_init() -> CurlHandle
external fn curl_perform(h CurlHandle) Fail[NetError] -> []byte
external fn curl_cleanup(h CurlHandle) -> ()

fn CurlHandle consume @cleanup(_outcome ScopeOutcome) -> () => curl_cleanup(@)

fn fetch(url str) Fail[NetError] -> []byte {
    consume h = curl_init() {
        h.set_url(url)
        h.perform()?
    }
}
```

## Section 5 — Anti-patterns

### 5.1 Forgetting a `Cleanup` impl on a new resource type

```nova
type MyResource { handle int }

// ❌ DON'T:
fn use_it() -> () {
    consume r = MyResource.new() { ... }   // → D188-not-consumable
}
```

Suggestion: implement `Cleanup[E]` for the resource type. Quick-fix
LSP code-action "implement Cleanup" (Plan 110.6 Ф.10.6).

### 5.2 Wrapped init without unwrap

```nova
// ❌ DON'T:
consume tx = db.maybe_begin() { ... }    // maybe_begin() : Option[Tx]
                                          // → D196-wrapped-init-needs-unwrap
```

Suggestion: `consume tx = db.maybe_begin()!! { ... }`, or check first.

### 5.3 Divergent Cleanup types in a conditional

```nova
// ❌ DON'T:
consume r = if cond { File.open(path)? } else { TcpStream.connect(addr)? } {
    ...
}
// → D196-divergent-consumable
```

Suggestion: extract into a polymorphic wrapper type, or use `Box[Cleanup[E]]`.

### 5.4 spawn / parallel / supervised inside `@cleanup`

```nova
// ❌ DON'T:
fn Resource consume @cleanup(_o ScopeOutcome) -> () {
    spawn { @async_flush() }         // → E_CLEANUP_FORBIDDEN_OPERATION
}
```

D159/D191 rule. Use a sequential `await @async_flush()?` or an
off-thread queue with a persistent worker fiber.

### 5.5 Cancel-shield opt-out attempts

The cancel-shield is always on inside the `@cleanup` body. It cannot be
disabled — this is deliberate (Rust scopeguard / C++23 lessons show that
an opt-in shield gets forgotten most of the time).

## Section 6 — Debugging cleanup chains

### 6.1 Reading a MultiError

```nova
match process() {
    Ok(_) => println("done")
    Err(e) => {
        // e — Error / MultiError
        if e is MultiError {
            println("primary: ${e.primary()}")
            for sup in e.suppressed() {
                println("  suppressed: ${sup}")
            }
            if Some(panic_msg) = e.find_first_panic() {
                println("  PANIC IN CHAIN: ${panic_msg}")
            }
        } else {
            println("error: ${e.msg}")
        }
    }
}
```

### 6.2 OpenTelemetry tracing

```nova
fn main() Io -> () {
    with ResourceTrace = OtelCleanupHandler.new(exporter: otel_exporter) {
        with Application = Application.handler() {
            run_app()
        }
    }
}
```

Each `consume {}` enter/exit generates an OTel span:
- attributes: `cleanup.label`, `cleanup.timeout_ms`, `cleanup.start_time_ns`.
- status: OK / ERROR_failed / ERROR_panic.
- Parent-child spans LIFO-stacked correctly.

### 6.3 `nova consume-analyze` (Plan 100.8 + Plan 110.8 update)

```bash
nova consume-analyze src/db.nv
```

Shows:
- Which types implement Cleanup[E];
- Coverage (is the whole cleanup path covered);
- Whether the hot-path opt applied (Cleanup[never] + no WithExitTimeout);
- Potential `D198-realtime-application-override` warnings.

## Section 7 — Performance considerations

### 7.1 When to use `Cleanup[never]`

Use it when cleanup genuinely cannot fail:
- Lock release (no I/O).
- Permit return (atomic op).
- Pool return (atomic op).
- Cancel-scope cancel (in-memory state change).

**Don't** use it for:
- File close — can fail (disk full, EBADF).
- TCP socket close — can fail (broken pipe).
- DB commit — can fail.

### 7.2 Hot-path elision verification

```bash
nova build --release --asm-dump src/lock_path.nv
```

Grep for `nv_consume_enter` / `nv_resolve_exit_timeout` — for `Cleanup[never]`
+ no `WithExitTimeout` they should be **absent** from the hot-path asm.

### 7.3 Cancel-shield overhead

Per benchmark (Plan 110.6 Ф.11.5 target): cancel-shield + 3-level resolution
overhead ≤ Plan 100.4 baseline + 5%. Typical: < 100ns per cleanup entry.

If a profile shows cleanup overhead > 5%:
- Check the `Cleanup[never]` opportunity (hot-path elision).
- Hoist `consume{}` outside the hot loop (acquire the lock once vs. per iteration).
- Profile the actual bottleneck — cleanup rarely dominates.

### 7.4 MultiError composition cost

Depth 1 (single error, no suppression): zero overhead.
Depth 10: ~ 200 ns (allocation + chain link).
Depth 100: ~ 2 µs.
Depth 256: capped — sentinel `MultiErrorTruncated` (D193).

If a cleanup cascade goes deeper than 256, that's usually a sign of a bug
(recursion in the cleanup path).

## Section 8 — Common pitfalls

### 8.1 Boot order

```nova
// ❌ DON'T:
fn Application.handler(...) -> ApplicationHandler {
    register_finalizer(|| cleanup())   // can't — handler ещё не активен (D195 R7)
    ApplicationHandler { ... }
}
```

The constructor must fully complete before entering the `with` block.
Registering finalizers — only from the body.

### 8.2 abort/SIGKILL does not fire finalizers

Documented in D195 R8 as a limitation shared by all languages (Java/Go/Rust/etc).
Cleanup on `panic()` — fires. On `exit(code)` — fires. On abort/SIGKILL/
SIGSEGV — NOT (the OS kills the process directly).

For critical state on abort:
- Use an OS-level mechanism (file flush, transactional DB);
- Or the Plan 110.4 Ф.8.9 `#[run_on_abort]` attribute (follow-up
  `[M-110-run-on-abort]`).

### 8.3 Nested Application semantics surprise

```nova
with Application = Application.handler(default_exit_timeout_ms: 30_000) {
    with Application = Application.handler() {   // inherits ничего!
        // default_exit_timeout_ms == 5_000 (hardcoded), НЕ 30_000
    }
}
```

D195 R3: deliberate non-inheritance for test isolation. If you want
inheritance — pass it explicitly: `Application.handler(default_exit_timeout_ms:
parent.default_exit_timeout_ms())`.

## See also

- [Plan 110](../plans/110-scoped-resources-radical-simplification.md) — umbrella.
- [Plan 100.5](../plans/100.5-ffi-external-integration.md) — FFI bridge.
- [Plan 100.8](../plans/100.8-performance-ide-tooling.md) — perf + tooling.
- [idiom/consume-scope-cleanup.md](../dev/idioms/consume-scope-cleanup.md) — Q-blocks
  (semantics overview).
- [D188](../../spec/decisions/03-syntax.md#d188)–[D198](../../spec/decisions/03-syntax.md#d198) — spec.
