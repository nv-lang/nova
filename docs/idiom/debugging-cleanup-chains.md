// SPDX-License-Identifier: MIT OR Apache-2.0
# Debugging Cleanup Chains — MultiError Walk + Tracing (D193)

> **Plan 110 Ф.14.2 Q-block.** Q-debugging-cleanup-chains: investigate
> cleanup-failures via MultiError chain inspection + OpenTelemetry
> tracing (D185) + nova consume-analyze tool.

## Symptom Map

| Symptom | Likely cause | Investigation tool |
|---|---|---|
| Cleanup fired multiple times | D188 R2 violation OR manual on_exit call | Compile-time D188-r2-manual-on-exit error (Plan 110.1.5) |
| Cleanup skipped on throw | Resource not Consumable, OR codegen incorrect | nova consume-analyze + manual codegen inspect |
| Cancel storm during cleanup | Cancel-shield не active | Verify Plan 110.2 cancel-shield landing; trace via OTel |
| MultiError chain depth > 256 | Recursive defer/on_exit cycle | D193 depth-limit truncates; check MultiError.walk() for `truncated` marker |
| `panic:` in unexpected places | NOVA_THROW_PANIC longjmp through cleanup | MultiError.find_first_panic() locates origin |
| Memory leak post-cleanup | on_exit не finalizing resource | nova consume-analyze + valgrind |

## Investigation Recipes

### Recipe 1: Inspect MultiError chain

```nova
match risky_op() {
    Ok(v) => use(v)
    Err(e) => {
        // Type-check `e` is MultiError? (D158/D161/D193)
        if e is MultiError {
            println("primary: ${e.primary()}")
            for sup in e.suppressed() {
                println("  suppressed: ${sup}")
            }
            // Plan 110.4.1: find first panic в chain.
            match e.find_first_panic() {
                Some(panic_msg) => println("PANIC HIDDEN: ${panic_msg}")
                None => ()
            }
        } else {
            println("simple error: ${e.msg}")
        }
    }
}
```

### Recipe 2: Walk full chain LIFO

```nova
match outer_op() {
    Err(e) => {
        if e is MultiError {
            ro all_errors = e.walk()        // Plan 110.4.1
            println("error count: ${all_errors.len()}")
            for (idx, msg) in all_errors.enumerate() {
                ro role = if idx == 0 { "PRIMARY" } else { "SUPPRESSED" }
                println("[${role}] ${msg}")
            }
        }
    }
    Ok(_) => ()
}
```

### Recipe 3: Bootstrap cancel discrimination

Until [M-110-multierror-any] payload migration lands, distinguish cancel
via prefix:

```nova
if msg.starts_with("cancel: ") {
    // D90 §7 amend (Plan 110.5.6): cancel-routed throw.
    ro reason = msg.slice(8, msg.len())   // strip "cancel: "
    println("cancel reason: ${reason}")
} else if msg.starts_with("panic:") {
    println("panic in cleanup chain")
} else {
    println("regular throw: ${msg}")
}
```

### Recipe 4: OpenTelemetry tracing (D185)

```nova
with Cleanup = OtelCleanupHandler.new(exporter: my_otel_exporter) {
    with Application = Application.handler() {
        run_app()
    }
}
```

Each `consume X = ... { body }` emit OTel spans:
- `on_scope_enter` → span open with attributes:
  - `cleanup.label` = type name (e.g., "Transaction").
  - `cleanup.timeout_ms` = resolved deadline.
  - `cleanup.start_time_ns` = nanos.
- `on_scope_exit` → span close with status:
  - OK / ERROR_failed / ERROR_panic.
  - `duration_ms` attribute.

Spans nested correctly via scope-stack (D188 R5 LIFO). Parent-child
relationships visible в Jaeger / Tempo / etc.

### Recipe 5: nova consume-analyze CLI

```bash
nova consume-analyze src/db.nv
```

Reports:
- Types implementing `Consumable[E]` per file.
- Coverage: which paths trigger on_exit (success / throw / cancel / panic).
- Hot-path elision applicable (Consumable[never] + no WithExitTimeout).
- D198-realtime-application-override warnings.
- Suspected D188 R2 violations (manual on_exit calls).

(Plan 110.6.x integration with LSP for in-editor diagnostics.)

## Common Diagnostic Errors

| Error code | Cause | Fix |
|---|---|---|
| `D188-not-consumable` | Init returns type без `on_exit` method | Implement `fn T consume @on_exit(outcome) -> ()` |
| `D188-malformed-on-exit` | `on_exit` first param не `ScopeOutcome` | Fix signature: `(outcome ScopeOutcome)` |
| `D188-r2-manual-on-exit` | Manual `binding.on_exit(...)` в body | Remove manual call (auto-dispatch at scope-exit) |
| `D196-wrapped-init-needs-unwrap` | Init returns Option/Result без `?`/`!!` | Add `?` или `!!` |
| `D196-divergent-consumable` | if/else branches return different Consumable types | Unify branch types |
| `D192-zero-timeout-suspend` | `await` in `#realtime` `on_exit` | Remove suspend OR move to non-realtime context |
| `D198-realtime-application-override` | Application timeout ignored in `#realtime` context | Restructure to use Level 1 (WithExitTimeout) or Level 3 (hardcoded) |
| `D189-deprecated-okdefer/errdefer/defer-result` | Old cleanup syntax | Migrate via `nova fix --simplify-cleanup` |

## Common Runtime Issues

| Issue | Likely cause | Resolution |
|---|---|---|
| Cleanup hung | Deadlock в `on_exit` body (e.g., trying to acquire already-held lock) | Check `consume re-entrance` (D197); use try-lock fallback |
| OOM в cleanup | MultiError chain grows unboundedly | D193 depth-limit kicks in at 256; redesign defer cycle |
| Cleanup never reached | Init throws (D188 R1 partial-construction) — expected behavior | Confirm via partial-init test fixture; OR fix init |
| `CleanupTimeoutError` | Cleanup exceeded `exit_timeout_ms` | Increase WithExitTimeout impl, OR set Application.default_exit_timeout_ms, OR optimize on_exit |

## See also

- [D188](../../spec/decisions/03-syntax.md#d188).
- [D193 MultiError](../../spec/decisions/03-syntax.md#d193).
- [D185 Cleanup effect](../../spec/decisions/04-effects.md#d185).
- [Plan 110.4.1 MultiError API](../plans/110.4-multierror-cleanup-app-effects.md).
- [Plan 100.8 nova consume-analyze](../plans/100.8-performance-ide-tooling.md).
- [cleanup-cookbook.md §6 Debugging](../cleanup-cookbook.md).
