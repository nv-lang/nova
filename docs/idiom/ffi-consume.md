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
    let r = nova_file_open(path)?
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

## Связь

- [D163](../../spec/decisions/02-types.md#d163) — `external fn`.
- [D82](../../spec/decisions/08-runtime.md#d82) — `external fn` foundation.
- [D126](../../spec/decisions/03-syntax.md#d126) — `external type`
  opaque.
- [D63](../../spec/decisions/04-effects.md#d63),
  [D64](../../spec/decisions/04-effects.md#d64) — capability
  enforcement.
- [D133](../../spec/decisions/02-types.md#d133) — consume foundation.
- [consume-types idiom](consume-types.md) — canonical patterns.
- Plan 100.5 — [100.5-ffi-external-integration.md](../plans/100.5-ffi-external-integration.md).
- Plan 18 stdlib — основной consumer.
