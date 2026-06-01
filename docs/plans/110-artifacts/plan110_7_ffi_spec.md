// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 110.7 — FFI Integration with `Consumable[E]`

> **Plan 110.7.1.** Spec для C-side resource cleanup integration через
> FFI bridge. Cross-ref [Plan 100.5](../100.5-ffi-external-integration.md).

## Goal

Allow C-side resources requiring cleanup to implement
`Consumable[E]` через FFI wrapper. Cancel-shield пробрасывается
через FFI call безопасно (cancellation-safety attestation).

## C-Side Resource Pattern

```nova
external type CResource

external fn c_open(path str) -> CResource
external fn c_close(r CResource) Fail[IoError] -> ()

// Wrap external resource в Consumable:
fn CResource consume @on_exit(_outcome ScopeOutcome) Fail[IoError] -> () =>
    c_close(@)?
```

Usage:
```nova
fn process(path str) Fail[IoError] -> () {
    consume r = c_open(path) {
        // r usage; c_close auto-called at scope-exit.
    }
}
```

## Cancellation-Safety Attestation

C-side functions called from `on_exit` (within cancel-shield) must
declare cancellation-safety. Without attestation, codegen rejects
foreign-call с warning.

### Attestation Mechanism

```nova
#cancel_safe
external fn c_close(r CResource) Fail[IoError] -> ()
```

`#cancel_safe` attribute (NEW for Plan 110.7) attests that:
1. Function does not call cancel-unsafe C APIs (e.g., blocking I/O
   without timeout).
2. Function completes within reasonable time bound (validated при
   runtime через deadline check).
3. Function does not require Nova fail-frame state.

### Without `#cancel_safe`

```nova
external fn c_unsafe_close(r CResource) -> ()
```

Calling из `on_exit`:
```nova
fn CResource consume @on_exit(_outcome ScopeOutcome) -> () {
    c_unsafe_close(@)
    // ⚠ W_FFI_CANCEL_UNSAFE — warning: no attestation.
}
```

Warning suggests adding `#cancel_safe` OR using sync wrapper.

## Cancel-Shield Pass-Through

When ConsumeScope enters shield:
1. `fiber->cancel_masked = true` (Plan 110.2.1).
2. FFI calls inside body see masked cancel — they continue uninterrupted.
3. On C-function return, control resumes in Nova; cancel still masked.
4. After `on_exit` completes, shield leaves; cancel delivered.

C-function CANNOT explicitly check `fiber->cancel_masked` — it's
implementation detail. C-function should be cancel-agnostic
(complete normally).

## Examples

### Example 1: SQLite Connection

```nova
external type SqliteConn

#cancel_safe
external fn sqlite_open(path str) -> SqliteConn

#cancel_safe
external fn sqlite_close(conn SqliteConn) Fail[IoError] -> ()

fn SqliteConn consume @on_exit(_outcome ScopeOutcome) Fail[IoError] -> () =>
    sqlite_close(@)?

fn query_db(db_path str, sql str) Fail[IoError] -> []Row {
    consume conn = sqlite_open(db_path) {
        // SQL queries via conn.
        conn.query(sql)?
    }
}
```

### Example 2: libcurl handle

```nova
external type CurlHandle

#cancel_safe
external fn curl_init() -> CurlHandle

#cancel_safe
external fn curl_cleanup(h CurlHandle) -> ()

external fn curl_perform(h CurlHandle) Fail[NetError] -> []byte
//  ^ NOT #cancel_safe — perform может blockироваться long time;
//    use outside on_exit body OR add timeout wrapper.

fn CurlHandle consume @on_exit(_outcome ScopeOutcome) -> () =>
    curl_cleanup(@)

fn fetch(url str) Fail[NetError] -> []byte {
    consume h = curl_init() {
        h.set_url(url)
        h.perform()?    // OK — body, not on_exit
    }
}
```

### Example 3: Memory-mapped file

```nova
external type MmapFile

#cancel_safe
external fn mmap_open(path str, size int) -> MmapFile

#cancel_safe
external fn mmap_close(f MmapFile) -> ()

fn MmapFile consume @on_exit(_outcome ScopeOutcome) -> () => mmap_close(@)

fn process_mmap(path str, size int) -> int {
    consume f = mmap_open(path, size) {
        f.read_at(0)
    }
}
```

## C-side Implementation Contract

Per attestation `#cancel_safe`, C function MUST:

1. **Bounded execution time.** Document upper bound (e.g., < 10ms).
2. **No blocking syscalls without timeout.** Use `select`/`poll` с
   short timeout; `read`/`write` only on non-blocking FDs.
3. **No reentrant Nova call.** C function should not call back into
   Nova (would re-enter ConsumeScope codegen — re-entrance depth
   counted в D193 256-limit, но better avoid).
4. **Idempotent on retry.** If runtime retries due to timeout, second
   call должен be safe (e.g., `close()` second-call should be no-op).

## Diagnostic Codes

- `W_FFI_CANCEL_UNSAFE` — calling extern fn без `#cancel_safe` из
  `on_exit` body. Suggestion: add attribute OR use sync wrapper.
- `E_FFI_REENTRANT` — extern fn callback re-enters ConsumeScope.
  Runtime detected; fail-frame propagates error.

## Plan 110.7 Sub-sub Status

| Sub-sub | Status |
|---|---|
| 110.7.1 Spec (this doc) | ✅ landed |
| 110.7.2 SQLite wrapper example | 🔴 OPEN (depends Plan 100.5 FFI infrastructure) |
| 110.7.3 Cancel propagation runtime through FFI | 🔴 OPEN (depends Plan 110.2.x cancel-shield runtime) |

## See also

- [D188](../../spec/decisions/03-syntax.md#d188) — Consumable protocol.
- [Plan 100.5 FFI integration](../100.5-ffi-external-integration.md).
- [Plan 110.2 cancel-shield runtime](decomposition.md).
- [Q-async-cleanup-consume](../../idiom/q-async-cleanup-consume.md).
- [cleanup-cookbook.md §4 FFI wrappers](../../cleanup-cookbook.md).
