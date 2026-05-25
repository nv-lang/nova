// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 100.6 — GATE Probe (D164: cross-module consume + visibility + mangling)

> **Назначение:** верификация самосогласованности D164 + D133 через 5
> hand-written примеров до начала имплементации. Каждый пример —
> pseudo-Nova код с аннотациями того, что flow-analysis и codegen будут
> трекать.
>
> **Создан:** 2026-05-25 в ходе Ф.0 Plan 100.6.
> **Ссылка на spec:** [D164 в spec/decisions/02-types.md](../../../spec/decisions/02-types.md#d164)
> **Ссылка на plan:** [100.6-cross-module-integration.md](../100.6-cross-module-integration.md)

---

## Use-case 1: Cross-package import — consume-marker propagation (D164 §1)

**Покрывает:** D164 §1 (consume-marker не теряется при import из другого пакета),
D133 (must-consume enforcement), D9 (binding keyword).

```nova
// Package A: "payments" пакет.
export type Transaction consume {
    id int,
    amount int,
}
export fn Transaction consume @commit() -> () { ... }
export fn Transaction consume @abort()  -> () { ... }
export fn Transaction_begin(id int, amount int) -> Transaction { ... }

// Package B: "billing" пакет.
// import payments.{Transaction, Transaction_begin}

fn process_payment(id int, amount int) {
    // D164 §1: Transaction из другого пакета → consume-marker сохранён.
    // D9: binding keyword `consume` обязателен.
    consume tx = Transaction_begin(id, amount) // tx: Live (Transaction)
    tx.commit()                                 // tx → Consumed ✅
    // Scope-exit: tx Consumed ✅
}

fn forget_payment(id int, amount int) {
    consume tx = Transaction_begin(id, amount) // tx: Live
    // нет commit/abort
    // ❌ D133-not-consumed: 'tx' of consume type 'Transaction' not consumed.
}
```

**Flow-analysis трекает:**
- `Transaction` из пакета A помечен `consume` в AST — признак не теряется.
- `consume tx = Transaction_begin(...)` → `tx` в `ConsumeCtx` как `VarState::Live`.
- `tx.commit()` → `tx.consume` = true (consume-метод) → `tx → Consumed`.
- Exit-check: `tx Live` → emit `D133-not-consumed`.

---

## Use-case 2: Consume-bit mangling — ABI stability (D164 §2, D134 extension)

**Покрывает:** D164 §2 (consume-bit в C-имени функции), ABI-break
если тип меняет consume-статус без major-bump.

```nova
// Package A v1.0: Transaction consume → методы мангируются как "consume_"
export type Transaction consume { id int }
export fn Transaction consume @commit() -> () { ... }
// → C name: Nova_Transaction_consume_commit

// Package A v1.1 (ПРАВИЛЬНО): consume-статус не изменён → ABI stable.
export type Transaction consume { id int, note str } // добавлено поле
export fn Transaction consume @commit() -> () { ... }
// → C name: Nova_Transaction_consume_commit (та же!)

// Package A v2.0 (ABI BREAK): если убрать consume-маркер (гипотетически).
export type Transaction { id int }    // consume убрано → major-bump ОБЯЗАТЕЛЕН
export fn Transaction @commit() -> () { ... }
// → C name: Nova_Transaction_method_commit (ДРУГОЕ!)
// D164 §2: consumers скомпилированные с v1 не линкуются с v2 без rebuild.
```

**Codegen трекает:**
- `fn T consume @method() ...` → base_c_name = `Nova_{T}_consume_{method}`.
- `fn T @method() ...` (non-consume) → base_c_name = `Nova_{T}_method_{method}`.
- mangle_fn (fallback) и pre-pass registration должны давать идентичный результат.
- ABI-break детектируется линкером при несовместимом изменении consume-статуса.

---

## Use-case 3: Re-export — consume-marker preserved (D164 §3)

**Покрывает:** D164 §3 (re-export через промежуточный пакет сохраняет
consume-маркер), D133 enforcement на consumer стороне.

```nova
// Package A: base resource.
export type File consume { fd int }
export fn File consume @close() -> () { ... }
export fn File_open(path str) -> File { ... }

// Package B (посредник): re-export File из A.
// import pkgA.{File, File_open, File close}
export type File consume = pkgA.File  // re-export с consume-маркером ✅
export fn File_open = pkgA.File_open  // прокси-функция

// Package C (consumer B): берёт File из B, не из A.
// import pkgB.{File, File_open}

fn use_file() {
    consume f = File_open("/tmp/x") // File из B → consume-marker propagated ✅
    f.close()                        // f → Consumed ✅
}

fn leak_file() {
    consume f = File_open("/tmp/x")
    // нет close
    // ❌ D133-not-consumed: 'f' of consume type 'File' not consumed.
    //    D164 §3: consume enforced даже через re-export chain.
}
```

**Flow-analysis трекает:**
- Re-export не стирает consume-маркер (copy of TypeDef/FnDef сохраняет `consume: true`).
- Consumer видит `File.consume = true` в AST → все binding rules применяются.

---

## Use-case 4: Folder-module + relative imports (D164 §4-5)

**Покрывает:** D164 §4 (folder-module consume-types), D164 §5 (relative
imports не отличаются от абсолютных для consume-анализа).

```nova
// resources/file.nv (folder-module):
module resources.file
export type File consume { fd int }
export fn File consume @close() -> () { ... }

// resources/socket.nv (folder-module):
module resources.socket
export type Socket consume { port int }
export fn Socket consume @disconnect() -> () { ... }

// main.nv:
// import ./resources.{File, Socket}  // relative import

fn use_resources() {
    consume f = File.open(1)          // File.consume preserved ✅ (D164 §4)
    consume s = Socket.connect(8080)  // Socket.consume preserved ✅ (D164 §5)
    f.close()                          // f → Consumed ✅
    s.disconnect()                     // s → Consumed ✅
}
```

**Flow-analysis трекает:**
- Relative import path `./resources` резолвится в folder-module.
- consume-маркер в TypeDef одинаково обрабатывается для любого import style.
- Два consume vars в одном scope — оба должны быть Consumed на exit.

---

## Use-case 5: nova.toml [exports.consume_types] contract (D164 §6)

**Покрывает:** D164 §6 (пакетный контракт на consume-status через nova.toml),
semver-stability promise для consumers.

```toml
# nova.toml Package A:
[exports.consume_types]
Transaction = "1.0"   # consume-статус Transaction stable в v1.x
Resource    = "1.0"   # v1 → v2 может изменить (major-bump required)
```

```nova
// Package A:
export type Transaction consume { id int }  // ✅ listed in [exports.consume_types]
export fn Transaction consume @commit() -> () { ... }

export type Resource consume { handle int } // ✅ listed
export fn Resource consume @close() -> () { ... }

// Consumer Package B:
// Relies on consume-status being stable in v1.x.
// If Package A bumps to v2.0 and drops consume → linker detects ABI mismatch.
// nova.toml [exports.consume_types] = documentation + semver gate.

fn use_transaction() {
    consume tx = Transaction_begin(1, 100)
    tx.commit()   // ✅ consume contract honoured (Transaction = "1.0")
}

fn use_resource() {
    consume r = Resource_open(42)
    r.close()     // ✅ Resource.consume = "1.0"
}
```

**Manifest parser трекает:**
- `[exports.consume_types]` → `HashMap<String, String>` в `Manifest.exports_consume_types`.
- Nova tooling может предупреждать, если тип в списке не объявлен `consume` в src.
- ABI-break детектируется при изменении consume-статуса без major-bump.

---

## Self-check checklist

- [x] Каждый use-case имеет соответствие в D164:
  - Use-case 1: D164 §1 (cross-package consume propagation) + D133 enforcement
  - Use-case 2: D164 §2 (consume-bit mangling, ABI stability)
  - Use-case 3: D164 §3 (re-export chain)
  - Use-case 4: D164 §4-5 (folder-module + relative imports)
  - Use-case 5: D164 §6 (nova.toml [exports.consume_types] contract)
- [x] Нет contradiction между секциями:
  - D133 enforcement одинаково применяется везде (local, cross-package, re-export)
  - consume-bit в манглировании одинаков в pre-pass и mangle_fn fallback
  - consume-маркер сохраняется через любую import chain
- [x] Ни одно правило не введено без D-block parent'а:
  - cross-package propagation → D164 §1
  - consume-bit mangling → D164 §2 (extension D134 из Plan 81)
  - re-export marker → D164 §3
  - folder/relative imports → D164 §4-5
  - nova.toml contract → D164 §6
