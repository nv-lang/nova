// SPDX-License-Identifier: MIT OR Apache-2.0
# Consume-типы — canonical patterns

> Practical guide для type-level `consume`-семантики (Plan 100 family,
> D133-D166). Foundational idiom для production resource-management в
> Nova.

## Когда писать `type X consume`

Heuristic: «**забыть → leak / inconsistency**».

✅ **Помечать `consume`:**
- `Transaction` — забыть = potentially incomplete DB state.
- `File` — забыть = file descriptor leak.
- `TcpSocket` — забыть = socket leak.
- `Mutex.Lock` (lock-guard) — забыть = deadlock.
- `Connection` — забыть = connection leak.
- `Pending<Error>` — забыть = error swallowed.

❌ **НЕ помечать `consume`:**
- Plain data records (`Order`, `User`, ...) — забыть = GC съест, no leak.
- `StringBuilder` (existing) — забыть = buffer GC-cleaned, no leak.
- `Result[T, E]` / `Option[T]` — generic-заразность через D133 D6.
- Iterators — cheap, забывать OK.

## Canonical lifecycle

### Симметричная пара errdefer + okdefer

```nova
fn process_order(data Data) Fail[OrderErr] Db -> Receipt {
    consume tx = db.begin()
    errdefer { tx.rollback()? }                 // error → rollback
    okdefer  { tx.commit()?   }                 // success → commit
    ro order = db.insert(data)?
    db.notify(order)?
    return Receipt { id: order.id }
}
```

Exhaustive cover для consume-обязательства через **defer-family**:
- success path → `okdefer` (commit)
- error path → `errdefer` (rollback)
- failable cleanup composes через D158 Plan 49 multi-error.

### Альтернатива — explicit commit на success

```nova
fn process() Fail[Err] -> () {
    consume tx = begin()
    errdefer { tx.rollback() }                  // error → rollback
    do_work()?
    tx.commit()                                  // success → explicit commit
}
```

Менее симметрично, но допустимо. Choice — style preference.

### Defer only (для simple cleanup без error-recovery)

```nova
fn process() -> () {
    consume f = File.open("x.txt")?
    defer { f.close() }                         // все exit-paths → close
    ro data = f.read_all()?
    println(data)
}
```

Когда `close` — единственная cleanup-операция (no commit/rollback
choice). `defer` covers все paths.

## Binding-form: `consume tx` vs `let tx`

| Form | Strict semantics |
|---|---|
| `consume tx = begin()` | ОБЯЗАН закрыть в этом scope'е (consume-method) |
| `let tx = begin()` | ОБЯЗАН передать наверх (return / consume-param / record-наверх) |

Декларация intent'а с compiler-checked гарантией. Без or-or.

```nova
// «закрою здесь»:
fn local_use() Fail -> () {
    consume tx = begin()
    do_stuff()?
    tx.commit()
}

// «передам наверх»:
fn factory() -> Transaction {
    ro tx = begin()
    return tx                                    // передача наверх
}
```

## Record с consume-полем — double-marker

```nova
type TxState consume {                          // ← ОБЯЗАТЕЛЬНО на type-decl
    consume tx Transaction,                     // ← ОБЯЗАТЕЛЬНО на field
    writes []Write,                             // обычное поле
}

fn TxState consume @commit() -> () {
    @tx.commit()                                // consume через field
    // writes — обычный field, GC-cleaned
}
```

Compiler enforces consistency:
- consume-поле без `consume`-маркера → error D133-field-marker-missing
- `consume`-поле без `consume` на type-decl → error D133-type-marker-missing

## Reopen pattern (mut-метод)

```nova
type Service consume {
    consume file File,
}

fn Service mut @reopen() Fail[OpenErr] -> () {
    ro new_file = File.open()?                 // сначала получить замену
    @file.close()                                // только теперь закрыть старое
    @file = new_file                             // rebind — @.file Live
}                                                // mut exit: @.file Live ✅
```

**Антипаттерн:**
```nova
fn Service mut @reopen_naive() Fail[OpenErr] -> () {
    @file.close()                                // @.file Consumed
    @file = File.open()?                         // ❌ если open fails — @.file Consumed на exit
}
```

→ Compile error D133-field-not-restored на error-path.

## Read-only access — без `consume` qualifier на param

```nova
fn print_id(tx Transaction) -> () {             // без `consume` — read-only
    println(tx.id)                              // ✅ чтение поля
}

consume tx = begin()
print_id(tx)                                    // ❌ D133-move-to-non-consume
                                                //    (callee не declares consume)
```

Для production-grade read-only — просто **не пиши `consume` qualifier**
(view-default по D157, Plan 100.3 Ред. 2 — keyword `view T` отвергнут):

```nova
fn print_id(tx Transaction) -> () {             // default view (no qualifier)
    println(tx.id)
}

consume tx = begin()
print_id(tx)                                    // ✅ view-передача
tx.commit()                                     // ✅ tx Live после
```

## Deep peek в Option[ConsumeType] — `match view`

```nova
type Service consume { consume file Option[File] }

fn Service @file_id() -> Option[int] {
    match @file {                          // ← D157 view-match
        Some(f) => Some(f.fd),                  // f: view File, не Consumed
        None => None,
    }
    // @.file Live (не Consumed) ✅
}
```

## Closure capture — consume vs view

```nova
// view-closure (FnMut/Fn analog) — multi-invoke OK:
consume tx = begin()
ro logger = || println(tx.id)
logger()                                         // OK
logger()                                         // OK
tx.commit()                                      // ✅ tx Live

// consume-closure (FnOnce analog) — single-invoke:
consume tx2 = begin()
ro commit_it = || tx2.commit()
commit_it()                                      // ✅ tx2 Consumed
commit_it()                                      // ❌ use-after-consume
```

## Generic с `[T consume]` bound

Backward-compat: generic без bound = silent-ignore (legacy stdlib).
Opt-in strict через `[T consume]`:

```nova
fn box[T consume](consume x T) -> Box[T] => Box { val: x }
// Strict mode: silent-forget T → error D156-strict-forget.

fn first[T consume](pair (T, T)) -> T => pair.0
// ❌ pair.1 силен потерян → error.
```

## Что НЕ делать

❌ **`mem::forget`-style escape** — не существует. Единственный способ
удовлетворить consume — consume-метод.

❌ **`let _ = tx`** для consume-tx — error D133-not-consumed.

❌ **Destructure consume-record** — `let { tx } = state` ломает
encapsulation; не работает.

❌ **Auto-cleanup Drop-style** — Nova требует **явный** consume-метод;
commit/rollback choice важен.

❌ **`consume` + `-> @`** на одном методе — parse error D8.

## Связь с другими D-блоками

- [D131](../../spec/decisions/05-memory.md#d131) — affine consume (Plan 73).
- [D132](../../spec/decisions/03-syntax.md#d132) — `-> @` fluent-return.
- [D133](../../spec/decisions/02-types.md#d133) — type-level consume.
- [D156](../../spec/decisions/02-types.md#d156) — generic `[T consume]`.
- [D157](../../spec/decisions/05-memory.md#d157) — implicit view default + closure capture + match consume.
- [D158-D162](../../spec/decisions/03-syntax.md#d158) — defer/errdefer/
  okdefer family.
- [D163](../../spec/decisions/02-types.md#d163) — FFI `external fn`.
- [D164](../../spec/decisions/02-types.md#d164) — cross-module.

## Source plan

- [Plan 100 umbrella](../plans/100-linear-must-consume.md).
