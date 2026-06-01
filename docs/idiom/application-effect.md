// SPDX-License-Identifier: MIT OR Apache-2.0
# Application Effect — Top-level Lifecycle Management (D195)

> **Plan 110 Ф.14.2 Q-block.** Q-application-effect: top-level
> application lifecycle через `Application` effect — finalizers +
> default exit_timeout для ConsumeScope D192 Level-2 fallback.
> Cross-ref [D195](../../spec/decisions/04-effects.md#d195),
> [D192](../../spec/decisions/03-syntax.md#d192).

## TL;DR

```nova
fn main() Io -> () {
    with Application = Application.handler(default_exit_timeout_ms: 10_000) {
        // Deep call chain: anywhere can register finalizer.
        run_server()
        // or:
        Application.register_finalizer(|| metrics.flush())
    }
    // Handler exits: finalizers fire reverse-order (LIFO).
}
```

Three core capabilities:

1. **`register_finalizer(f fn() -> ())`** — register cleanup callback.
2. **`default_exit_timeout_ms()`** — Level-2 fallback в D192 3-level
   resolution для `consume X = ... { body }` timeout.
3. **Effect-stack semantics** (R1-R8) — nesting + cross-fiber
   propagation.

## Why an Effect (Not a Singleton)?

Three reasons (Plan 110 §D195 rationale):

1. **Test isolation**: each test fresh `Application` scope. Finalizers
   не leak между tests:

   ```nova
   fn test_a() Io -> () {
       with Application = Application.handler() {
           Application.register_finalizer(|| cleanup_a())
           run_a()
       }   // finalizers fire here
       // test_b's Application — separate, не sees test_a's finalizers.
   }
   ```

2. **Nested scopes**: subsystem with own lifecycle (e.g., embedded
   replica) gets independent finalizer registry:

   ```nova
   with Application = Application.handler(default_exit_timeout_ms: 30_000) {
       // Main app finalizers
       with Application = Application.handler() {  // h2 — нет inheritance
           Application.register_finalizer(|| replica.flush())
       }   // h2 finalizers fire here; main h1.registry untouched.
   }
   ```

3. **Cross-fiber propagation** (D195 R6): `spawn { ... }` child fiber
   sees parent's `Application` via D80 effect snapshot. Finalizer
   registered в child fiber attached к parent's handler:

   ```nova
   with Application = Application.handler() {
       spawn {
           // Sees parent's Application через D80 snapshot.
           Application.register_finalizer(|| spawned_cleanup())
       }
   }
   ```

## Nesting Semantics (D195 R1-R8)

### R1 — Inner handler wins

Standard effect-stack semantics. Inner `with` shadows outer.

### R2 — Finalizer registry NOT inherited

Each `Application.handler()` creates fresh registry. Inner finalizers
fire when inner exits; outer registry untouched.

### R3 — `default_exit_timeout` NOT inherited

Each handler's `default_exit_timeout_ms` independent. Pass explicit
arg для override:

```nova
with Application = Application.handler(default_exit_timeout_ms: 30_000) {
    with Application = Application.handler() {
        // default_exit_timeout = 5_000 (hardcoded fallback), не 30_000.
        consume tx = db.begin() { /* uses 5_000 ms */ }
    }
}
```

If inheritance desired — pass explicitly:

```nova
ro outer_timeout = 30_000
with Application = Application.handler(default_exit_timeout_ms: outer_timeout) {
    with Application = Application.handler(default_exit_timeout_ms: outer_timeout) {
        // inherits 30_000
    }
}
```

### R6 — Cross-fiber propagation

`spawn { ... }` snapshot's parent's effect-stack via D80. Child sees
parent's `Application`. Finalizers registered в child attached к
parent's handler.

### R7 — Boot order

Handler constructor runs to completion BEFORE `with` body enters.
Registration calls during construction are prohibited (handler not yet
active). Constructor throws → `with` не enters; `on_exit` не called
(D188 R1 partial-construction safety).

### R8 — Abort / SIGKILL не fires finalizers

Per OS-level limitation (NOT a bug). Finalizers run на normal exit
(handler.on_exit) или panic. `abort()` / SIGKILL / SIGSEGV bypass
runtime — no chance to run finalizers.

**Mitigation:** для critical state, use:
- File flush (sync write + close).
- Transactional DB (commit/rollback atomic).
- OS-level cleanup hooks (`atexit()` C-side; не Nova-level guarantee).

## Integration с D192 3-Level Timeout

`Application.default_exit_timeout_ms()` is **Level-2** в D192 3-level
resolution для `consume X = ... { body }` timeout:

1. **Level 1** (resource-specific): `WithExitTimeout.exit_timeout_ms()`
   на binding type.
2. **Level 2** (application-wide): `Application.default_exit_timeout_ms()`.
3. **Level 3** (hardcoded): `5_000` ms.

```nova
type Transaction { /* ... */ }

// Level 1: Tx-specific timeout
fn Transaction @exit_timeout_ms() -> int => 30_000

with Application = Application.handler(default_exit_timeout_ms: 10_000) {
    consume tx = db.begin() { ... }
    //          ^^^^^^^^^^^^ uses 30_000 (Level 1 wins).

    consume mu_g = mu.lock() { ... }
    //          ^^^^^^^^^^^^^^^ uses 10_000 (Level 2 — MutexGuard has no Level 1).
}

// Outside Application scope:
consume mu_g = mu.lock() { ... }
//          ^^^^^^^^^^^^^ uses 5_000 (Level 3 hardcoded).
```

## Patterns

### Pattern 1: Main entry point

```nova
fn main() Io -> () {
    with Application = Application.handler(default_exit_timeout_ms: 10_000) {
        Application.register_finalizer(|| Log.flush())
        Application.register_finalizer(|| metrics.export())

        ro server = HttpServer.bind(":8080")?
        Application.register_finalizer(|| server.shutdown())

        server.serve()?
    }
    // On exit: server.shutdown → metrics.export → Log.flush (LIFO).
}
```

### Pattern 2: Per-test isolation

```nova
fn test_user_workflow() Io -> () {
    with Application = Application.handler() {
        Application.register_finalizer(|| reset_test_db())
        Application.register_finalizer(|| clear_caches())

        run_workflow()
    }
    // Finalizers fire here; next test gets clean state.
}
```

### Pattern 3: Subsystem with own lifecycle

```nova
fn run_replica(addr str) Fail[NetError] Application -> () {
    with Application = Application.handler() {  // subsystem-local
        Application.register_finalizer(|| replica.stop())
        replica.start(addr)?
        replica.serve()?
    }
    // Replica finalizers fire без affecting outer Application.
}
```

### Pattern 4: Cross-fiber finalizer registration

```nova
with Application = Application.handler() {
    spawn {
        // Child fiber registers finalizer; attached к parent's handler.
        Application.register_finalizer(|| worker_state.save())
        worker_loop()
    }
    // Outer can join + cleanup.
}
// On exit: worker_state.save() fires (registered via spawn fiber).
```

## Anti-patterns

### Anti 1: Registration в constructor

```nova
// ❌ DON'T:
fn Application.handler(default_exit_timeout_ms int = 5_000) -> ApplicationHandler {
    Application.register_finalizer(|| ...)  // handler not yet active!
    ApplicationHandler { ... }
}
```

R7: registration calls только из `with` body. Constructor errors →
handler не enters scope.

### Anti 2: Critical cleanup on abort

```nova
// ❌ DON'T rely on this:
with Application = Application.handler() {
    Application.register_finalizer(|| save_critical_state())
    risky_op_that_might_abort()
}
// Если abort() called — finalizer NEVER runs (R8).
```

For critical state: use file flush, transactional DB, OS-level hooks.

### Anti 3: Mutating shared state в finalizer

```nova
// ❌ DON'T:
mut global_counter = 0
Application.register_finalizer(|| global_counter = 0)  // race с other finalizers
```

Finalizers run sequentially LIFO, no concurrent execution. But:
finalizer body errors don't compose — best to be infallible.

## See also

- [D195 Application effect](../../spec/decisions/04-effects.md#d195).
- [D192 3-level timeout resolution](../../spec/decisions/03-syntax.md#d192).
- [D188 R1 partial construction](../../spec/decisions/03-syntax.md#d188).
- [Q-cleanup-semantics](consume-scope-cleanup.md).
- [Q-cancel-and-cleanup](cancel-and-cleanup.md).
- [cleanup-cookbook.md](../cleanup-cookbook.md).
