// SPDX-License-Identifier: MIT OR Apache-2.0
# `consume X = expr { body }` — radical simplification cleanup-семейства

> Practical guide для Plan 110 family
> ([D188](../../spec/decisions/03-syntax.md#d188)–
> [D198](../../spec/decisions/03-syntax.md#d198),
> [D185](../../spec/decisions/04-effects.md#d185),
> [D195](../../spec/decisions/04-effects.md#d195)).
> Production-grade resource cleanup с одним keyword'ом + одним protocol'ом.
>
> Заменяет ~20 концептов из Plan 100.4 family
> ([cleanup-on-failure.md](cleanup-on-failure.md) — pre-Plan 110 idioms).

## Q-cleanup-semantics — обзор cleanup-семейства Nova V3

После Plan 110 cleanup-семантика сводится к **5 концептам**:

1. `consume X = expr { body }` — scope-block для типизированного cleanup
   (~95% случаев).
2. `defer { ... }` — escape hatch для logging/instrumentation (~5%).
3. `protocol Consumable[E]` — контракт ресурса с одним методом `on_exit`.
4. `consume self` modifier — builder/transfer (`StringBuilder.into()`).
5. `Fail[E]` + `?` + `!!` + `throw` + `panic` + `exit` + `interrupt` —
   control flow (как сейчас).

Опционально (telemetry): эффект `Cleanup` (D185) для observability.

### Что заменено

| Старое | Новое |
|---|---|
| `errdefer { rollback }` | `match outcome { Failure(_) => rollback }` в `on_exit` |
| `okdefer { commit }` | `match outcome { Success => commit }` в `on_exit` |
| `defer \|result\| { ... }` | `match outcome` в `on_exit` (или `with Cleanup = h`) |
| `DeferResult[T,E]` | `ScopeOutcome` sum-type |
| `ErrorKind` enum | type-erased `Failure(any)` + D85 `if err is T` narrowing |
| Половина D162 coverage rules | `consume {}` exhaustive by construction |
| Effect-aware cleanup-deadline | метод `exit_timeout_ms()` (D192) |
| `module_finalizer` keyword | паттерн `Consumable[Application]` (D195) |

## Q-consumable-protocol — как написать `on_exit`

### Decision tree

```
Может ли cleanup throw?
├─ Нет (Mutex/Sem/Lock/Permit) → Consumable[never]   (D194 hot-path)
└─ Да
   ├─ Один error type (Transaction → DbError) → Consumable[DbError]
   ├─ Несколько (TcpStream → IoError/ProtocolError) → Consumable[IoError]
   │   + routing через match err.kind
   └─ Generic resource → Consumable[E] с unbound E
```

### Шаблон implementation

```nova
type Transaction { /* fields */ }

fn Transaction consume @on_exit(outcome ScopeOutcome) Fail[DbError] -> () {
    match outcome {
        Success => @commit()?

        Failure(err) => {
            // D85 if err is T narrowing для типизированного dispatch
            if err is DbError.Deadlock {
                @retry_friendly_rollback()?
            } else if err is DbError {
                @rollback_with_log(err.msg)?
            } else {
                // generic non-DB failure (Net/Time/...)
                @rollback()?
            }
        }

        Panic(_) => @rollback_emergency()   // НЕ throws — это bug
    }
}
```

### Хочу infallible — `Consumable[never]`

```nova
fn MutexGuard consume @on_exit(_outcome ScopeOutcome) -> () => @release()
//                                                       ^^^^^ no Fail[E]
```

`Fail[never]` ≡ «не throws». Caller'у `Fail[E]` не требуется (D194 special case):

```nova
fn use_mutex() -> () {              // no Fail[E]
    consume _l = mu.acquire() {     // MutexGuard: Consumable[never] — OK
        do_work()
    }
}
```

**Hot-path optimization** (D194 §perf): codegen detect'ит `Consumable[never]` +
no `WithExitTimeout` impl → strip shield/timeout/outcome. Результат:
`consume _l = mu.acquire() { body }` компилируется в `body; mu.release()` —
zero overhead.

### Хочу custom timeout — implement `WithExitTimeout`

```nova
fn Transaction @exit_timeout_ms() -> int => 30_000   // 30 секунд
```

Structural match — protocol не обязательно явно declare'ить. 3-level
resolution (D192):
1. `WithExitTimeout.exit_timeout_ms()` — если есть;
2. `Application.default_exit_timeout_ms()` — если активен handler;
3. Hardcoded 5000ms — иначе.

## Q-when-which-cleanup — flowchart

```
Нужен cleanup при выходе из scope?
├─ Нет → ничего не пишем
└─ Да
   ├─ Resource'ом владею (open file, begin tx, acquire lock) → consume X = ... { body }
   ├─ Cleanup logging / instrumentation only → defer { log_metric() }
   ├─ Tracing entry/exit cleanup-scope → with Cleanup = OtelHandler.new() { ... }
   ├─ App-level finalizer (run at exit) → Application.register_finalizer(|| ...)
   └─ FFI-resource (C-side) → wrap в Consumable type (Plan 100.5 FFI bridge)
```

### `consume X = ... { body }` vs raw `consume X = ...` (D180)

| Use case | Form |
|---|---|
| Linear ownership transfer (`StringBuilder.into()`) | `consume X = ...` (raw, без block) |
| Resource lifecycle (open/close, begin/commit) | `consume X = ... { body }` (scope-block) |
| Builder pattern (build → consume → return result) | `consume X = ...` (raw) |
| Database transaction | `consume tx = db.begin() { do_work()? }` |
| File handling | `consume f = File.open(path)? { read_data() }` |
| Lock acquisition | `consume _l = mu.acquire() { critical_section() }` |

Parser lookahead на `{` после `EXPR` решает форму.

## Q-migration-from-okdefer — auto-fix guide

Plan 110.5 `nova fix --simplify-cleanup` покрывает 3 canonical pattern'а
(см. [D189](../../spec/decisions/03-syntax.md#d189)):

### Pattern 1 — `consume + errdefer + okdefer` (canonical Transaction)

```nova
// Before:
fn process_order(data Data) Fail[OrderErr] Db -> Receipt {
    consume tx = db.begin()
    errdefer { tx.rollback()? }
    okdefer  { tx.commit()? }
    ro order = db.insert(data)?
    return Receipt { id: order.id }
}

// After (предполагается Transaction impl Consumable[DbError]):
fn process_order(data Data) Fail[OrderErr] Db -> Receipt {
    consume tx = db.begin() {
        ro order = db.insert(data)?
        return Receipt { id: order.id }
    }
}
```

Auto-fix tool вычисляет: `errdefer{tx.rollback()}` + `okdefer{tx.commit()}`
↔ Consumable.on_exit с `Failure→rollback / Success→commit`. Если Transaction
ещё **не** implement Consumable — генерируется stub impl с TODO comment.

### Pattern 2 — bare `errdefer` без ресурса (cleanup state)

```nova
// Before:
fn risky_op() Fail[OpErr] -> () {
    errdefer { log.warn("operation failed") }
    risky()?
    finalize()?
}

// After:
fn risky_op() Fail[OpErr] -> () {
    mut done = false
    defer { if !done { log.warn("operation failed") } }
    risky()?
    finalize()?
    done = true
}
```

Чисто механическая трансформация — auto-fix tool применяет без user-input.

### Pattern 3 — `defer |result|`

```nova
// Before:
fn report() -> () {
    defer |result| {
        match result {
            Ok(_)    => Log.info("success")
            Err(e)   => Log.error("fail: ${e}")
            Panic(m) => Log.fatal("panic: ${m}")
        }
    }
    do_work()
}

// After (using Cleanup effect — D185 OpenTelemetry-style):
fn report() -> () {
    with Cleanup = LogHandler.new(label: "report") {
        do_work()
    }
}
```

Auto-fix tool детектит `defer |r|` форму с logging-only body → suggest
`Cleanup` effect handler. Если body modifies external state (не только
log) — flag для manual review.

## Compared с другими языками

| Capability | Java | Kotlin | Swift | C++23 | Rust | Go | TS | Python | **Nova V3** |
|---|---|---|---|---|---|---|---|---|---|
| Cancel-shield by default | ❌ | ⚠️ opt-in | ⚠️ opt-in (2026) | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |
| Single keyword | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ⚠️ ES2024 | ✅ | ✅ `consume{}` |
| Exactly-once invariant | ⚠️ | ⚠️ | ✅ | ⚠️ | ⚠️ | ❌ | ❌ | ⚠️ | ✅ runtime |
| Per-resource timeout | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ D192 |
| Async cleanup | ❌ | ✅ | ✅ | ❌ | ❌ unsolved | ⚠️ ctx-manual | ⚠️ await using | ⚠️ | ✅ D191 |
| Partial-construction safety | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | n/a | ⚠️ | ⚠️ | ✅ D188 R1 |
| Iterable suppressed walk | ✅ | ✅ | ✅ | ⚠️ | n/a | n/a | ✅ | ⚠️ | ✅ D193 |
| Module finalizers | ❌ atexit | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ D195 |

## Anti-patterns

### Anti-1: вложенный `consume {}` с тем же ресурсом

```nova
// ❌ DON'T:
consume tx = db.begin() {
    consume tx = db.begin() { ... }   // shadowing — D131 linear types violation
}
```

Linear types prevent (compile error). Используйте distinct binding names.

### Anti-2: manual `tx.on_exit(...)` из body

```nova
// ❌ DON'T:
consume tx = db.begin() {
    tx.on_exit(Success)   // R2 exactly-once violation — runtime panic
    do_work()
}
```

Doc-comment в Consumable: `on_exit` вызывается **только** runtime'ом scope-
block'а. Manual call → `D188-on-exit-double-invocation` runtime error.

### Anti-3: spawn в `on_exit`

```nova
// ❌ DON'T:
fn Connection consume @on_exit(_outcome ScopeOutcome) -> () {
    spawn { @flush_async() }    // D191 forbidden — fire-and-forget cleanup
}
```

Compile error `E_CLEANUP_FORBIDDEN_OPERATION`. Используйте sequential `await`
или off-thread queue в Cleanup handler (D185).

## See also

- [D188](../../spec/decisions/03-syntax.md#d188) — Consumable + consume scope-block.
- [D194](../../spec/decisions/03-syntax.md#d194) — `Consumable[never]` hot-path.
- [Plan 110](../plans/110-scoped-resources-radical-simplification.md) — umbrella.
- [cleanup-cookbook.md](../cleanup-cookbook.md) — production-recipe book.
- [cleanup-on-failure.md](cleanup-on-failure.md) — pre-Plan 110 idioms (legacy).
