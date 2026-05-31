// SPDX-License-Identifier: MIT OR Apache-2.0
# FFI consume — `external fn` для C-runtime

> Practical guide для [D163](../../spec/decisions/02-types.md#d163)
> (Plan 100.5). Как объявлять FFI-функции, которые carry consume-
> obligation через C-границу.

## Базовый pattern

```nova
// Opaque consume-type (D126 + D133):
external type File consume

// External fn возвращает consume-File; caller owns.
// Capability обязателен (D63 + D163 D3).
external fn nova_file_open(path str) -> Result[File, IoErr]
    needs Fs

// External fn consume'ит File; callee owns transfer.
external fn nova_file_close(consume f File) -> ()
    needs Fs

// Nova-side wrapper:
fn File consume @close() -> () {
    nova_file_close(@)
}
```

Используется идиоматично:

```nova
fn process(path str) Fail[IoErr] Fs -> () {
    ro r = nova_file_open(path)?
    consume f = r
    defer { f.close() }                         // D158 cleanup
    // use f...
}
```

## Что обязательно

✅ **Capability declaration** (D163 D3):
```nova
external fn nova_X() -> Y needs <Cap>   // обязательно
```
Без `needs` — error D163-missing-cap.

✅ **Opaque type через `external type X consume`** (D126):
```nova
external type Socket consume
```

✅ **C-side defensive helpers**:
- `nv_consume_validate(ptr)` — assert не-NULL на entry.
- После consume — zero/memset internal fields (defense-in-depth).

## Что НЕ делать

❌ **`consume` маркер без consume return/param** — vacuous:
```nova
external fn nova_get_pid() -> int       // ❌ W (D163-vacuous-consume)
```

❌ **Передача consume-var дважды**:
```nova
nova_file_close(consume f)                      // ✅ first close
nova_file_close(consume f)                      // ❌ use-after-consume
```

❌ **Storing FFI-handle в long-lived struct без consume-marker**:
```nova
record FileCache { f File }                     // ❌ если File consume,
                                                //    Cache обязан consume +
                                                //    field marker consume
```

## Generic-заразность через FFI

`Result[File, IoErr]` / `Option[File]` — auto-consume через generic-
заразность (D133 D6):

```nova
external fn nova_open() -> Result[File, IoErr] needs Fs
//                                ^^^^^^^^^^^^^^^^^^^ Result consume через
//                                                     File arg.
// Caller обязан consume Result (через match Some-arm или explicit).
```

## Pilot stdlib типы

Plan 100.7 предусматривает 4 pilot migrations:

- `std/io/file.nv` — File consume + open/read/close.
- `std/sync/mutex.nv` — Mutex + Lock-guard.
- `std/net/tcp.nv` — TcpSocket с graceful close (Plan 100.4.2 D159
  async/suspend cleanup).
- `std/db/transaction.nv` — Transaction commit/rollback.

## Сравнение с другими языками

| Capability | Rust | Kotlin/JNI | Go cgo | Nova |
|---|---|---|---|---|
| Ownership через FFI | ✅ `unsafe fn` + manual | ⚠️ manual | ⚠️ manual | ✅ **declaration native** |
| `unsafe` keyword | ✅ да | n/a | n/a | ❌ **нет** (D6) |
| Capability tracking | ⚠️ unsafe = «trust me» | ⚠️ manual | ⚠️ manual | ✅ **D63 capabilities** |

Nova **превосходит Rust** — нет `unsafe` (D6); вместо этого explicit
capability declaration tracks privilege.

## `#cancel_safe` — аттестация для cleanup-вызовов (Plan 110.7.3.a)

Когда FFI-функция вызывается **из `consume X = ... { body }` cleanup
path** — то есть из `on_exit` метода consume-типа — она исполняется
под активным cancel-shield'ом (D188 R3). Внешние cancel'ы откладываются
до выхода из scope'а; C-функция должна терпеть это игнорирование.

`#cancel_safe` — обещание разработчика по **трём пунктам**:

### 1. Bounded completion time

C-функция должна **завершаться сама**, не полагаясь на внешний cancel
для пробуждения:

```c
// ❌ ПЛОХО — повисает под shield'ом:
int bad_close(int fd) {
    char buf[4096];
    while (read(fd, buf, 4096) > 0) {}  // ждёт байты ...
    return close(fd);                    // ... которые никогда не придут
}

// ✅ ХОРОШО — детерминированное завершение:
int good_close(int fd) {
    return close(fd);  // sync syscall, возвращается ms-fast
}
```

Запрещены: blocking `read/recv/poll` без timeout'а, `pthread_cond_wait`
без deadline, busy-loop'ы «жди внешнее изменение».

### 2. Идемпотентность для cleanup семантики

D188 R2 гарантирует exactly-once вызов `on_exit`, но компилятор
не дублирует. Однако partial-effect cleanup от прошлой попытки
(если что-то fanned-out до panic) должен быть safe для повтора —
например, `sqlite3_close(NULL)` no-op, не SEGV.

### 3. Не зависит от Nova fail-frame state

C-код **не должен**:

* Читать internal TLS pointers (`_nova_fail_top`, `_nova_active_scope`).
* Вызывать `nova_throw_*` / `nova_fail_push/pop` напрямую.
* Полагаться на Nova handler-stack или `ScopeOutcome`.

Под cancel-shield'ом fail-frame стек в mid-unwinding состоянии —
C-код таких допущений делать **не должен**. Pattern: C-функция возвращает
`int` код ошибки → Nova-обёртка проверяет и throw'ит при необходимости.

```nova
// ✅ Nova-side error mapping:
#cancel_safe
external fn sqlite3_close_v2(handle int) -> int  // С-функция возвращает rc

fn SqliteConn consume @on_exit(_outcome ScopeOutcome) Fail[IoError] -> () {
    ro rc = sqlite3_close_v2(@handle)
    if rc != 0 {
        throw IoError { reason: "sqlite close rc=${rc}" }
    }
    return ()
}
```

### Lint W_FFI_CANCEL_UNSAFE

Compile-time: вызов FFI без `#cancel_safe` из `on_exit` body → warning
`W_FFI_CANCEL_UNSAFE` с suggestion «либо добавь аттестацию, либо оберни
в sync wrapper». Status: [M-110.7.3-w-ffi-cancel-unsafe-lint] followup —
attribute parses в bootstrap'е, lint enforcement landing в follow-up
session.

## Связь

- [D163](../../spec/decisions/02-types.md#d163) — `external fn`.
- [D82](../../spec/decisions/08-runtime.md#d82) — `external fn` foundation.
- [D126](../../spec/decisions/03-syntax.md#d126) — `external type`
  opaque.
- [D63](../../spec/decisions/04-effects.md#d63),
  [D64](../../spec/decisions/04-effects.md#d64) — capability
  enforcement.
- [D133](../../spec/decisions/02-types.md#d133) — consume foundation.
- [D188](../../spec/decisions/03-syntax.md#d188) §R3 — cancel-shield.
- [D199](../../spec/decisions/03-syntax.md#d199) — `#cancel_safe`
  attestation.
- [consume-types idiom](consume-types.md) — canonical patterns.
- Plan 100.5 — [100.5-ffi-external-integration.md](../plans/100.5-ffi-external-integration.md).
- Plan 110.7 — FFI cleanup integration.
- Plan 18 stdlib — основной consumer.
