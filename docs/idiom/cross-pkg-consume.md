// SPDX-License-Identifier: MIT OR Apache-2.0
# Cross-package consume — visibility, mangling, contracts

> Practical guide для [D164](../../spec/decisions/02-types.md#d164)
> (Plan 100.6). Как consume-маркер работает через границы модулей и
> пакетов.

## Basic export / import

```nova
// package A, module a/types.nv
export type Transaction consume {
    id int,
}

export fn Transaction consume @commit() -> () { ... }
export fn begin() -> Transaction => Transaction { id: 1 }
```

```nova
// package B, module b/main.nv
import a.types.{Transaction, begin}

fn main() {
    consume tx = begin()                        // consume-marker visible
    tx.commit()
}
```

`consume` propagates через `export` + `import` (Plan 35 R26 visibility
foundation). Без special-case'ов.

## Re-export через `export import` (Plan 42.09)

```nova
// package B re-exports A.Transaction:
export import a.types.{Transaction, begin}
```

```nova
// package C imports via B:
import b.facade.{Transaction, begin}
// Consume-marker preserved через chain A → B → C.
```

## Generic-заразность через границу

```nova
// package A:
export type Transaction consume { id int }

// package B возвращает Result wrapping A.Transaction:
fn try_begin() -> Result[a.Transaction, IoErr] => ...
```

`Result` сам становится consume через generic-заразность (D133 D6).
Caller обязан consume через match Ok-arm.

## Mangling — consume-bit (extends D134 Plan 81)

Plan 81 D134 определил symbol mangling v0. D164 amend — add **consume-bit**:

```
nova_fn_<pkg>_<mod>_<name>_<consume-bit>_<param-types>_<return-type>
```

Это ловит cross-version ABI break:

```nova
// package A v1.0:
export type Resource consume { ... }
export fn Resource consume @close() -> ()
// → nova_fn_a_resource_close_c_..._..._...

// package A v2.0 (breaking change — убрали consume!):
export type Resource { ... }
export fn Resource @close() -> ()
// → nova_fn_a_resource_close__..._..._...
```

Linker ловит mismatch на load. **Превосходит Rust** (Rust видит
mismatch только через type-id, не через ownership).

## Package version contracts (Plan 03)

`nova.toml`:

```toml
[package]
name = "my_lib"
version = "1.0.0"

[exports.consume_types]
Transaction = "1.0"
File = "1.0"
```

`nova audit` (Plan 03.4) verifies cross-version contracts. Major-bump
required для changing consume-status.

## Folder-modules (Plan 42) + relative imports (Plan 84)

consume-types работают идентично:

```
my_pkg/
  resources/
    _module.nv
    file.nv          # type File consume { ... }
    socket.nv        # type Socket consume { ... }
```

```nova
import ./types.{Transaction}                    // relative — same rules
consume tx = Transaction { id: 1 }
```

## Private consume не leak

```nova
type InternalCache consume { ... }              // no `export`
// Visible только в этом package. Cross-package — invisible (Plan 35 R26).
```

## Cross-module diagnostic

```
error: consume value `tx` (type a::Transaction) not consumed
  note: type defined in package 'a' v1.0 at a/types.nv:5
  note: consume via .commit() or .rollback() (declared in 'a')
```

Includes source-package origin, version, consume-method hint. AI-first
visibility для cross-package issues.

## Что НЕ делать

❌ **Использовать private consume cross-package** — visibility error.

❌ **Breaking consume contract без major-bump** — `nova audit` catches.

❌ **Mixing `mangling v0` packages с consume-bit packages** —
ABI mismatch, linker error.

## Связь

- [D164](../../spec/decisions/02-types.md#d164) — cross-module consume.
- [D26](../../spec/decisions/07-modules.md#d26),
  [D47](../../spec/decisions/07-modules.md#d47) — visibility foundation.
- [D134](../../spec/decisions/07-modules.md#d134) — mangling v0
  (Plan 81); D164 extends.
- [D133](../../spec/decisions/02-types.md#d133) — consume foundation.
- Plan 03 / Plan 03.4 — package ecosystem; `nova audit`.
- Plan 42, Plan 42.09, Plan 84 — folder-modules, re-export, relative.
- [consume-types idiom](consume-types.md) — canonical patterns.
- Plan 100.6 — [100.6-cross-module-integration.md](../plans/100.6-cross-module-integration.md).
