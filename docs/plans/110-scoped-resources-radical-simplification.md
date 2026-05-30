// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 110: consume-scope вЂ” radical simplification cleanup-СЃРµРјРµР№СЃС‚РІР°

> **РЎРѕР·РґР°РЅ 2026-05-29.**
> **Р РµРІРёР·РёСЏ v3.4 (2026-05-29)** вЂ” production-grade С„РёРЅР°Р». 11 D-Р±Р»РѕРєРѕРІ
> (D188-D198), 10 Q-Р±Р»РѕРєРѕРІ, acceptance A1-A38, 30 NEG-С‚РµСЃС‚РѕРІ, С„Р°Р·С‹ Р¤.0-Р¤.14.
> РЎСЂР°РІРЅРµРЅРёРµ СЃ Go/Rust/TS/Kotlin/Java РїРѕ 12 РѕСЃСЏРј РїСЂРµРІРѕСЃС…РѕРґСЃС‚РІР°.
> **v3.4 РїСЂР°РІРєРё РїРѕСЃР»Рµ design review**:
> - **Generic bound**: `[T Consumable[E]]` per D72, **РЅРµ** `[T impl ...]` (Rust-style РѕС€РёР±РєР°)
> - **Top body-error type**: `any`, **РЅРµ** `Error` (РїРѕСЃР»РµРґРЅРёР№ СЌС‚Рѕ `{msg str}` record)
> - **`outcome.failure_as[T]()` РЈР”РђР›РЃРќ** вЂ” РёСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ РїСЂСЏРјРѕР№ `if err is T` (D85 auto-narrowing)
> - **Duration: `infinite` в†’ `MAX`** (СЃС‚Р°РЅРґР°СЂС‚РЅС‹Р№ РїР°С‚С‚РµСЂРЅ РєР°Рє `i64.MAX`)
> - **D198 simplified**: `#realtime` runtime bypass РІСЃС‘ resolution, Р±РµР· compile-heuristic
> - **`#realtime` в†’ `#realtime`** + СѓРґР°Р»РµРЅРёРµ `realtime { }` / `blocking { }`
>   Р±Р»РѕРє-С„РѕСЂРј вЂ” РїРµСЂРµРёРјРµРЅРѕРІР°РЅРёРµ Р°С‚СЂРёР±СѓС‚Р° Рё СѓРїСЂРѕС‰РµРЅРёРµ РЅР° РѕРґРёРЅ РјРµС…Р°РЅРёР·Рј.
>   РЎРµРјР°РЅС‚РёРєР°: `#realtime` СЌС‚Рѕ **РіР°СЂР°РЅС‚РёСЏ callee** (РЅРµ constraint caller'Р°) вЂ”
>   РѕР±С‹С‡РЅР°СЏ fn СЃРІРѕР±РѕРґРЅРѕ РІС‹Р·С‹РІР°РµС‚ `#realtime` fn. Type-checker РїСЂРѕРІРµСЂСЏРµС‚ body
>   РЅР° restricted ops, РЅРµ caller. Plan 110 РїРёС€РµС‚СЃСЏ СЃСЂР°Р·Сѓ РЅР° РЅРѕРІРѕР№ РјРѕРґРµР»Рё;
>   rename + block-removal РґРµР»Р°РµС‚СЃСЏ **Plan 103.7** (dependency)
> - **РЈРґР°Р»РµРЅРёРµ errdefer/okdefer/defer\|r\| Р±РµР· migration window** вЂ” auto-tool РґРµР»Р°РµС‚ 100% (РєРѕРґ РјР°Р»)
> - **Application boot order**: constructor Р·Р°РІРµСЂС€Р°РµС‚СЃСЏ РґРѕ `with`-Р±Р»РѕРєР°; finalizers вЂ” С‚РѕР»СЊРєРѕ РІ body
> - **Re-entrance depth = 256** (РєР°Рє MultiError D193) + diagnostic
> - **Cancel-shield perf target**: в‰¤ Plan 100.4 baseline + 5%, **РЅРµ** Р¶С‘СЃС‚РєРёР№ 100ns
> - **codegen via runtime fn + vtable**, РЅРµ per-callsite
> - **Q-structural-extension stub** (F1 future direction)
> **v3.3**: D195-D198, hot-path opt, OTel format, cleanup-cookbook
> **v3.2**: `exit_timeout` РІ РѕРїС†РёРѕРЅР°Р»СЊРЅС‹Р№ `WithExitTimeout`; Application = effect
> **РЎС‚Р°С‚СѓСЃ:** рџ†• PLANNED.
>
> **Р¦РµР»СЊ:** 100% production-grade cleanup-СЃРµРјР°РЅС‚РёРєР° РґР»СЏ 0.1 release. РћРґРёРЅ
> keyword `consume` + РѕРґРёРЅ protocol `Consumable[E]` + `defer` escape hatch.
> РќРёРєР°РєРёС… bootstrap-СѓРїСЂРѕС‰РµРЅРёР№. РџСЂРµРІР·РѕР№С‚Рё Go/Rust/TS/Kotlin/Java РїРѕ 12 РѕСЃСЏРј.
>
> **D-Р±Р»РѕРєРё:** **D188** (Consumable + consume scope-block), **D189**
> (deprecation), **D190** (rejected), **D191** (async cleanup + suspend),
> **D192** (exit-timeout taxonomy + 3-level resolution), **D193** (MultiError
> iteration + cycle-safety), **D194** (Consumable[Never] + infallible),
> **D195** (Application nesting + finalizer scoping), **D196** (init type
> constraints), **D197** (cleanup re-entrance), **D198** (realtime+Application
> conflict).
> Amends/retracts: D158, D160, D161, D162, D184, D185, D186, D187, D90 В§7.
>
> **Acceptance:** A1вЂ“A30.

---

## РљРѕРЅС‚РµРєСЃС‚

РџРѕСЃР»Рµ Plan 100.4 (5 sub-plans, вњ…) cleanup-СЃРµРјРµР№СЃС‚РІРѕ РІС‹СЂРѕСЃР»Рѕ РґРѕ ~20 РєРѕРЅС†РµРїС‚РѕРІ
(4 С„РѕСЂРјС‹ defer + 4 ErrorKind + 4 DeferResult variants + D162 РїСЂР°РІРёР»Р° + ...).
**Plan 110 вЂ” radical simplify**.

РЎСЂР°РІРЅРµРЅРёРµ РєРѕРіРЅРёС‚РёРІРЅРѕР№ РЅР°РіСЂСѓР·РєРё РЅР° В«РѕС‚РєСЂС‹С‚СЊ С„Р°Р№Р» / С‚СЂР°РЅР·Р°РєС†РёСЋВ»:

| РЇР·С‹Рє | РљРѕРЅС†РµРїС‚С‹ |
|---|---|
| Java try-with-resources | 1 (`AutoCloseable`) |
| Kotlin `.use{}` | 1 |
| Python `with` | 1 |
| C# `using` | 1 |
| Go `defer` | 1 |
| Zig | 2 |
| Rust | 1 (`Drop`) + Result |
| **Nova РїРѕСЃР»Рµ Plan 100.x** | **~20** |
| **Nova РїРѕСЃР»Рµ Plan 110 v3** | **5** |

---

## Р¤РёРЅР°Р»СЊРЅС‹Р№ РґРёР·Р°Р№РЅ

### Protocol вЂ” РѕРґРёРЅ РјРµС‚РѕРґ

```nova
type Consumable[E] protocol {
    on_exit(outcome ScopeOutcome) Fail[E] -> ()
}

// РћРїС†РёРѕРЅР°Р»СЊРЅС‹Р№ вЂ” РµСЃР»Рё СЂРµСЃСѓСЂСЃ С…РѕС‡РµС‚ СѓРєР°Р·Р°С‚СЊ СЃРІРѕР№ timeout:
type WithExitTimeout protocol {
    exit_timeout() -> Duration
}

type ScopeOutcome
    | Success
    | Failure(any)           // throw РёР»Рё cancel вЂ” РІСЃС‘ СЃСЋРґР°
    | Panic(str)               // bug РІ С‚РµР»Рµ
```

- **`E`** = РѕС€РёР±РєРё РєРѕС‚РѕСЂС‹Рµ `on_exit` СЃР°Рј РјРѕР¶РµС‚ throw (commit/rollback errors)
- **`ScopeOutcome`** type-erased (Python `__exit__` pattern) вЂ” resource РЅРµ Р·РЅР°РµС‚ body error type
- **`Consumable[Never]`** РґР»СЏ cleanup'РѕРІ РєРѕС‚РѕСЂС‹Рµ РіР°СЂР°РЅС‚РёСЂРѕРІР°РЅРЅРѕ РЅРµ fail (D194)
- **`WithExitTimeout`** РѕРїС†РёРѕРЅР°Р»СЊРЅС‹Р№ protocol вЂ” structural; Р»СЋР±РѕР№ С‚РёРї СЃ РјРµС‚РѕРґРѕРј
  `exit_timeout() -> Duration` Р°РІС‚РѕРјР°С‚РёС‡РµСЃРєРё satisfies. РќРµ С‡Р°СЃС‚СЊ Consumable.

### Syntax вЂ” re-used `consume`

```nova
consume tx = db.begin() {
    body
}
```

РџР°СЂСЃРµСЂ lookahead РЅР° `{`:
- `consume x = expr { body }` в†’ scope-block
- `consume x = expr` в†’ raw linear binding (РґР»СЏ builder/transfer)

### Desugaring

```nova
consume tx = db.begin() { body }

// =>
{
    let _tx = db.begin()                       // if throws вЂ” no on_exit (D188 R2)
    let _timeout = resolve_exit_timeout(_tx)   // 3-level fallback (D192)
    let _outcome = run_body_capturing { body }
    with cancel_shield(deadline: _timeout) {
        match _outcome {
            Ok          => _tx.on_exit(Success)
            Err(e)      => { _tx.on_exit(Failure(e)); throw e }
            PanicAt(m)  => { _tx.on_exit(Panic(m)); resume_panic m }
        }
    }
}

// resolve_exit_timeout (codegen-emitted):
fn resolve_exit_timeout[T](v T) -> Duration {
    if T has method exit_timeout() -> Duration {   // structural check
        v.exit_timeout()
    } else if Application effect is active {       // ambient
        perform Application.default_exit_timeout()
    } else {
        Duration.seconds(5)                         // hardcoded fallback
    }
}
```

---

## Р§С‚Рѕ РѕСЃС‚Р°С‘С‚СЃСЏ (5 РєРѕРЅС†РµРїС‚РѕРІ)

1. `consume X = expr { body }` вЂ” РіР»Р°РІРЅС‹Р№ РјРµС…Р°РЅРёР·Рј (~95%)
2. `defer { ... }` вЂ” escape hatch (~5%)
3. `protocol Consumable[E]` вЂ” РєРѕРЅС‚СЂР°РєС‚ РґР»СЏ resource-С‚РёРїРѕРІ
4. `consume self` modifier вЂ” builder/transfer (`StringBuilder.into()`)
5. `Fail[E]` + `?` + `!!` + `throw` + `panic` + `exit` + `interrupt` вЂ” control flow (РєР°Рє СЃРµР№С‡Р°СЃ)

`Cleanup` effect вЂ” РѕРїС†РёРѕРЅР°Р»СЊРЅРѕ (telemetry).

## Р§С‚Рѕ СѓС…РѕРґРёС‚

| | РџРѕРєСЂС‹РІР°РµС‚СЃСЏ С‡РµСЂРµР· |
|---|---|
| `okdefer` | `on_exit` match РЅР° `Success` |
| `errdefer` | `on_exit` match РЅР° `Failure(_)` |
| `defer \|result\|` | `on_exit` (С‚Рѕ Р¶Рµ С‡РµСЂРµР· protocol) |
| `DeferResult[T,E]` | СѓРґР°Р»С‘РЅ |
| `ErrorKind` enum | СѓРґР°Р»С‘РЅ, type-erased `Error` |
| РџРѕР»РѕРІРёРЅР° D162 coverage rules | `consume {}` exhaustive by construction |
| Effect-aware cleanup-deadline | РјРµС‚РѕРґ `exit_timeout()` |
| `module_finalizer` keyword | РїР°С‚С‚РµСЂРЅ `Consumable[Application]` |

---

## РЎСЂР°РІРЅРµРЅРёРµ вЂ” Nova v3 vs РёРЅРґСѓСЃС‚СЂРёСЏ (12 РѕСЃРµР№)

| Capability | Java | Kotlin | Swift | C++23 | Rust | Go | TS | **Nova v3** |
|---|---|---|---|---|---|---|---|---|
| Cancel-shield by default | вќЊ | вљ пёЏ opt-in | вљ пёЏ opt-in (2026) | вќЊ | вќЊ | вќЊ | вќЊ | вњ… |
| Single keyword for resource | вњ… | вњ… | вќЊ | вќЊ | вќЊ | вќЊ | вљ пёЏ ES2024 | вњ… `consume{}` |
| Typed cancel reason | вќЊ | вљ пёЏ str | вќЊ | вќЊ | вќЊ | вљ пёЏ untyped | вќЊ | вњ… `CancelToken[T]` |
| Cleanup РјРѕР¶РµС‚ throw Р±РµР· abort | вњ… Suppressed | вњ… | вњ… | вљ пёЏ terminate | вќЊ Drop no-throw | вљ пёЏ silent | вњ… AggregateError | вњ… MultiError |
| Exactly-once guarantee | вљ пёЏ "suggested" | вљ пёЏ | вњ… | вљ пёЏ | вљ пёЏ | вќЊ | вќЊ | вњ… runtime invariant |
| Per-resource cleanup timeout | вќЊ | вќЊ | вќЊ | вќЊ | вќЊ | вќЊ | вќЊ | вњ… `exit_timeout()` |
| Async cleanup (suspend РІ cleanup) | вќЊ | вњ… | вњ… (2026) | вќЊ | вќЊ unsolved | вљ пёЏ ctx-manual | вљ пёЏ await using | вњ… D191 |
| Partial-construction safety spec'd | вљ пёЏ stacked-using bug | вљ пёЏ | вљ пёЏ | вљ пёЏ | вљ пёЏ | n/a | вљ пёЏ | вњ… D188 R2 |
| Cycle-safe suppression chain | вљ пёЏ FastThrow bug | вљ пёЏ | вљ пёЏ | вљ пёЏ | вљ пёЏ | n/a | вљ пёЏ | вњ… D193 |
| Iterable MultiError walk | вњ… getSuppressed | вњ… | вњ… | вљ пёЏ | n/a | n/a | вњ… | вњ… D193 |
| Module finalizers С‡РµСЂРµР· protocol | вќЊ atexit | вќЊ | вќЊ | вќЊ | вќЊ | вќЊ | вќЊ | вњ… Р¤.10 РїР°С‚С‚РµСЂРЅ |
| `Consumable[Never]` РґР»СЏ infallible | n/a | n/a | n/a | n/a | n/a | n/a | n/a | вњ… D194 |

**12/12 вЂ” Nova РїСЂРµРІРѕСЃС…РѕРґРёС‚ РёР»Рё СЌРєРІРёРІР°Р»РµРЅС‚РµРЅ РєР°Р¶РґРѕРјСѓ СЏР·С‹РєСѓ РїРѕ РєР°Р¶РґРѕР№ РѕСЃРё.**

---

## D-block changes

### D188 (NEW) вЂ” `Consumable[E]` + `consume` scope-block

**Р›РѕРєР°С†РёСЏ:** `spec/decisions/03-syntax.md`.

РЎРѕРґРµСЂР¶РёС‚:
- Protocol declaration: `Consumable[E]` СЃ **РѕРґРЅРёРј** РјРµС‚РѕРґРѕРј `on_exit`
- Optional protocol `WithExitTimeout` вЂ” РѕС‚РґРµР»СЊРЅС‹Р№, structural, РЅРµ С‡Р°СЃС‚СЊ Consumable
- Syntax formal grammar (`consume IDENT = EXPR { BODY }`)
- Desugaring rules РїРѕР»РЅРѕСЃС‚СЊСЋ (РІРєР»СЋС‡Р°СЏ 3-level timeout resolution)
- **R1 partial-construction**: РµСЃР»Рё `init` throws вЂ” `on_exit` РЅРµ Р·РѕРІС‘С‚СЃСЏ
- **R2 exactly-once**: runtime invariant; `on_exit` РЅРµ РјРѕР¶РµС‚ Р±С‹С‚СЊ РІС‹Р·РІР°РЅ РґРІР°Р¶РґС‹
- **R3 cancel-shield-by-default**: Р°РІС‚РѕРјР°С‚РёС‡РµСЃРєРё
- **R4 timeout resolution at scope-entry**: 3-level fallback resolved РѕРґРёРЅ СЂР°Р·
- **R5 LIFO composition** РґР»СЏ РІР»РѕР¶РµРЅРЅС‹С…
- **R6 type-erased outcome** rationale (Python pattern)
- Generic constraint syntax `[T Consumable[E]]` РґР»СЏ Р±РёР±Р»РёРѕС‚РµРє (D72)
- **Generic + Never special case**: РµСЃР»Рё `E = Never` РІ bound вЂ” caller РЅРµ РґРѕР»Р¶РµРЅ
  РѕР±СЉСЏРІР»СЏС‚СЊ `Fail[E]`, type-checker Р°РІС‚РѕРјР°С‚РёС‡РµСЃРєРё СЃРЅРёРјР°РµС‚ С‚СЂРµР±РѕРІР°РЅРёРµ
- Memory ordering: `on_exit` РІРёРґРёС‚ body changes С‡РµСЂРµР· release-acquire (cross-ref Plan 103.1 D167)

#### Typed error dispatch РІ `on_exit`

РСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ РїСЂСЏРјРѕР№ `is`-pattern (D85 auto-narrowing, Kotlin smart-cast):

```nova
match outcome {
    Success      => @commit()
    Failure(err) => {
        if err is DbError.Deadlock {
            @retry_friendly_rollback()    // err narrow'РЅСѓС‚ РґРѕ DbError.Deadlock
        } else if err is DbError {
            @rollback_with_log(err.msg)
        } else {
            @rollback()
        }
    }
    Panic(_)     => @rollback_emergency()
}
```

РќРёРєР°РєРѕРіРѕ `failure_as[T]()` helper'Р° вЂ” `is`-narrowing РґРѕСЃС‚Р°С‚РѕС‡РµРЅ Рё РёРґРёРѕРјР°С‚РёС‡РµРЅ РІ Nova.

### D189 (NEW) вЂ” РџСЂСЏРјРѕРµ СѓРґР°Р»РµРЅРёРµ `okdefer` + `errdefer` + `defer |result|`

**Р›РѕРєР°С†РёСЏ:** `spec/decisions/03-syntax.md`.

- **РќРёРєР°РєРѕРіРѕ migration window** вЂ” РєРѕРґР° РЅР° nv РјР°Р»Рѕ, auto-fix tool РґРµР»Р°РµС‚ 100%
  РјРёРіСЂР°С†РёРё РІ Р¤.5.
- Parser СЃСЂР°Р·Сѓ РІС‹РґР°С‘С‚ parse error РЅР° СЃС‚Р°СЂС‹Рµ С„РѕСЂРјС‹ (РїРѕСЃР»Рµ Р¤.5 СѓРґР°Р»РµРЅРёСЏ):
  - `D189-removed-okdefer`
  - `D189-removed-errdefer`
  - `D189-removed-defer-result`
- Auto-fix mappings (СЃРј. Р¤.9): tool РїСЂРёРјРµРЅСЏРµС‚СЃСЏ РѕРґРёРЅ СЂР°Р· РїРµСЂРµРґ СѓРґР°Р»РµРЅРёРµРј РїР°СЂСЃРµСЂ-РїРѕРґРґРµСЂР¶РєРё.
- D160 retracted РїРѕР»РЅРѕСЃС‚СЊСЋ; D90 В§7 amend.

### D190 (NEW) вЂ” Rejected design decisions

**Р›РѕРєР°С†РёСЏ:** `spec/decisions/03-syntax.md` В§В«Rejected alternativesВ».

Rationale РґР»СЏ:
- **Drop-trait (Rust-style)** вЂ” implicit cleanup, async-Drop РЅРµСЂРµС€С‘РЅ
- **Priority-defer** вЂ” LIFO РґРѕСЃС‚Р°С‚РѕС‡РЅРѕ
- **`module_finalizer` keyword** вЂ” РїР°С‚С‚РµСЂРЅ С‡РµСЂРµР· Consumable[Application]
- **Two-method protocol (`on_success`/`on_failure`)** вЂ” РѕРґРёРЅ РјРµС‚РѕРґ С‡РёС‚Р°РµС‚СЃСЏ Р»СѓС‡С€Рµ
- **Generic `ScopeOutcome[E]`** вЂ” resource РЅРµ Р·РЅР°РµС‚ body error type
- **РћС‚РґРµР»СЊРЅС‹Р№ `Cancelled` variant** вЂ” РЅРёРєС‚Рѕ РёР· СЏР·С‹РєРѕРІ РЅРµ РІС‹РґРµР»СЏРµС‚
- **`using` / `scoped` keyword** вЂ” re-use `consume` СЃРЅРёР¶Р°РµС‚ count РЅР° 1

### D191 (NEW) вЂ” Async cleanup + suspend РІ `on_exit`

**Р›РѕРєР°С†РёСЏ:** `spec/decisions/03-syntax.md` (РёР»Рё 06-concurrency.md).

- `suspend`-РѕРїРµСЂР°С†РёРё (Time.sleep, Net, Db) СЂР°Р·СЂРµС€РµРЅС‹ РІ `on_exit`
- **Р—Р°РїСЂРµС‰РµРЅС‹**: `spawn` / `parallel` / `supervised` (D159 РїСЂР°РІРёР»Рѕ СЃРѕС…СЂР°РЅСЏРµС‚СЃСЏ)
- Cancel-shield РїСЂРѕР±СЂР°СЃС‹РІР°РµС‚СЃСЏ С‡РµСЂРµР· РІСЃРµ suspend-points РІ `on_exit`
- `await long_op()` РІ cleanup РїСЂРёРѕСЃС‚Р°РЅР°РІР»РёРІР°РµС‚СЃСЏ, РЅРѕ cancel РЅРµ РїСЂРёС…РѕРґРёС‚ РґРѕ `exit_timeout`
- Р•СЃР»Рё cleanup-body РїСЂРµРІС‹СЃРёС‚ `exit_timeout` вЂ” С‚РµРєСѓС‰РёР№ suspend РїРѕР»СѓС‡Р°РµС‚ `CleanupTimeoutError`, РґР°Р»СЊС€Рµ propagates
- Cross-ref Plan 100.4.2 / D159 (async cleanup base)

### D192 (NEW) вЂ” exit-timeout taxonomy + 3-level resolution

**Р›РѕРєР°С†РёСЏ:** `spec/decisions/03-syntax.md`.

#### Taxonomy Р·РЅР°С‡РµРЅРёР№ Duration

- **`Duration.zero`** вЂ” `on_exit` РґРѕР»Р¶РµРЅ Р·Р°РІРµСЂС€РёС‚СЊСЃСЏ **СЃРёРЅС…СЂРѕРЅРЅРѕ Р±РµР· suspend**;
  Р»СЋР±РѕР№ `await` в†’ `D192-zero-timeout-suspend` runtime error.
- **`Duration.MAX`** вЂ” РЅРµС‚ timeout; warning `D192-infinite-timeout-warn`.
- **`Duration.negative`** вЂ” `D192-negative-timeout` runtime panic.
- РћР±С‹С‡РЅС‹Рµ РїРѕР»РѕР¶РёС‚РµР»СЊРЅС‹Рµ Duration вЂ” РЅРѕСЂРјР°Р»СЊРЅС‹Р№ timeout.

#### 3-level resolution (РѕС‚ Р±Р»РёР¶Р°Р№С€РµРіРѕ Рє РґР°Р»СЊРЅРµРјСѓ)

РџСЂРё РІС…РѕРґРµ РІ `consume X = ... { }` runtime resolves timeout РѕРґРёРЅ СЂР°Р·:

1. **`WithExitTimeout` impl** вЂ” РµСЃР»Рё С‚РёРї X РёРјРµРµС‚ РјРµС‚РѕРґ `exit_timeout() -> Duration`,
   РёСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ РѕРЅ (СЃС‚СЂСѓРєС‚СѓСЂРЅР°СЏ РїСЂРѕРІРµСЂРєР°, structural matching).
   - Mutex/Lock/Semaphore вЂ” РќР• implement (default РїРѕРґС…РѕРґРёС‚).
   - Transaction, BufWriter, TcpStream вЂ” implement СЃ СЂР°Р·СѓРјРЅС‹РјРё defaults.
   ```nova
   fn Transaction @exit_timeout() -> Duration => 30.s()
   ```

2. **`Application` effect** вЂ” РµСЃР»Рё Р°РєС‚РёРІРµРЅ handler С‡РµСЂРµР· `with Application = ...`,
   Р·РѕРІС‘С‚СЃСЏ `Application.default_exit_timeout()`. РџРѕРґРЅРёРјР°РµС‚ default РґР»СЏ РІСЃРµРіРѕ
   РїСЂРёР»РѕР¶РµРЅРёСЏ Р±РµР· РјРѕРґРёС„РёРєР°С†РёРё resource-С‚РёРїРѕРІ.
   ```nova
   with Application = Application.handler(default_exit_timeout: 10.s()) {
       run_server()                                    // РІСЃРµ consume{} РїРѕР»СѓС‡Р°С‚ 10s
   }
   ```

3. **Hardcoded fallback** вЂ” `Duration.seconds(5)` РµСЃР»Рё РЅРё РѕРґРёРЅ РёР· РІС‹С€Рµ РЅРµ
   СЃСЂР°Р±РѕС‚Р°Р». РљРѕРЅРµС‡РЅС‹Р№ safety net.

#### Realtime override

`#realtime` (D172 / Plan 103.6) вЂ” `Duration.zero` РїСЂРёРЅСѓРґРёС‚РµР»СЊРЅРѕ,
3-level resolution РЅРµ Р·Р°РїСѓСЃРєР°РµС‚СЃСЏ. Р›СЋР±РѕР№ suspend РІ `on_exit` в†’ compile/runtime
error.

#### Per-instance РєРѕРЅС„РёРіСѓСЂР°С†РёСЏ вЂ” РїР°С‚С‚РµСЂРЅ С‡РµСЂРµР· resource factory

Р­С‚Рѕ **library pattern**, РЅРµ language feature:

```nova
// std/db/db.nv (РќР• С‡Р°СЃС‚СЊ Plan 110, РѕС‚РґРµР»СЊРЅР°СЏ stdlib):
fn Db.connect(url str, exit_timeout Duration = 30.s()) -> Db => ...
fn Db @begin() -> Transaction => Transaction { exit_timeout_value: @config.exit_timeout, ... }
fn Transaction @exit_timeout() -> Duration => @exit_timeout_value
```

РљРѕРіРґР° РЅР°РїРёСЃР°РЅРѕ `Db.connect(url, exit_timeout: 60.s())` вЂ” РІСЃРµ С‚СЂР°РЅР·Р°РєС†РёРё С‡РµСЂРµР·
СЌС‚РѕС‚ Db СѓРЅР°СЃР»РµРґСѓСЋС‚ 60s, РїРѕС‚РѕРјСѓ С‡С‚Рѕ `Transaction.exit_timeout()` СЃС‚СЂСѓРєС‚СѓСЂРЅРѕ
satisfies `WithExitTimeout`.

#### Р§С‚Рѕ РќР• РґРµР»Р°РµРј

- вќЊ РќРµС‚ `exit_timeout()` РІ Consumable protocol вЂ” РѕРїС‚РёРјРёР·Р°С†РёСЏ: `MutexGuard` Рё
  РїСЂРѕС‡РёРµ infallible cleanup РЅРµ РѕР±СЏР·Р°РЅС‹ СЌС‚Рѕ РїРѕРґРґРµСЂР¶РёРІР°С‚СЊ.
- вќЊ РќРµС‚ scope-level override С‡РµСЂРµР· `with X = Y { }` вЂ” СЌС‚РѕС‚ СЃРёРЅС‚Р°РєСЃРёСЃ С‚РѕР»СЊРєРѕ
  РґР»СЏ effect-handlers (РЅРѕ Application РєР°Рє СЌС„С„РµРєС‚ СЂРµС€Р°РµС‚ С‚Сѓ Р¶Рµ Р·Р°РґР°С‡Сѓ).
- вќЊ РќРµС‚ global mutable setting С‡РµСЂРµР· РїСЂСЏРјРѕР№ setter вЂ” РєРѕРЅС„РёРі С‡РµСЂРµР·
  `Application` effect handler.

### D193 (NEW) вЂ” MultiError iteration + cycle-safety

**Р›РѕРєР°С†РёСЏ:** `spec/decisions/03-syntax.md`.

Р Р°СЃС€РёСЂРµРЅРёРµ API:
```nova
type MultiError {
    primary any
    suppressed []any
}

fn MultiError @primary() -> Error => @primary
fn MultiError @suppressed() -> []Error => @suppressed
fn MultiError @walk() -> Iter[Error]                   // РѕР±С…РѕРґ РІСЃРµС… РѕС€РёР±РѕРє РІ LIFO
fn MultiError @fmt_chain() -> str                      // РїРѕР»РЅР°СЏ С†РµРїРѕС‡РєР° РґР»СЏ Р»РѕРіРёСЂРѕРІР°РЅРёСЏ
fn MultiError @find_first_panic() -> Option[str]       // Р±С‹СЃС‚СЂС‹Р№ РїРѕРёСЃРє panic'Р°
```

**Cycle-safety** (Java JDK-8287921 lesson): РїСЂРё СЃРѕР·РґР°РЅРёРё MultiError compose-РѕРїРµСЂР°С†РёСЏ
РїСЂРѕРІРµСЂСЏРµС‚ identity:
- `nv_compose_error(primary, secondary)`:
  - Р•СЃР»Рё `secondary === primary` в†’ no-op (self-suppression РёРіРЅРѕСЂРёСЂСѓРµС‚СЃСЏ)
  - Р•СЃР»Рё `secondary` СѓР¶Рµ РІ primary.suppressed в†’ no-op
  - РРЅР°С‡Рµ в†’ append

Runtime invariant: depth-limit 256 (РµСЃР»Рё cleanup-cascade РіР»СѓР±Р¶Рµ вЂ” composes РєР°Рє В«...truncatedВ» entry).

### D194 (NEW) вЂ” `Consumable[Never]` РґР»СЏ infallible cleanup

**Р›РѕРєР°С†РёСЏ:** `spec/decisions/03-syntax.md`.

Resource-С‚РёРїС‹ РєРѕС‚РѕСЂС‹Рµ **РіР°СЂР°РЅС‚РёСЂРѕРІР°РЅРЅРѕ РЅРµ fail РІ cleanup** (Mutex, Lock, Semaphore) РёСЃРїРѕР»СЊР·СѓСЋС‚ `Consumable[Never]`:

```nova
fn MutexGuard consume @on_exit(outcome ScopeOutcome) -> () => @release()
//                                                       ^^^^ no Fail[E]
```

Type-checker: `Fail[Never]` СЂР°РІРЅРѕСЃРёР»СЊРЅРѕ В«РЅРµ throwsВ». Р­С‚Рѕ СѓР±РёСЂР°РµС‚ С‚СЂРµР±РѕРІР°РЅРёРµ РѕР±СЉСЏРІР»СЏС‚СЊ
`Fail[E]` РІ caller'Рµ РґР»СЏ infallible resource'РѕРІ:

```nova
fn use_mutex() -> () {            // РЅРµС‚ Fail[E]
    consume _l = mu.acquire() {   // MutexGuard: Consumable[Never] вЂ” РћРљ
        do_work()
    }
}
```

Р­С‚Рѕ Р°РЅР°Р»РѕРі Rust `Result<T, !>` / Haskell `IO ()` Р±РµР· `bracket throws`. Р”РµР»Р°РµС‚ API
РґР»СЏ locks/permits СЌСЂРіРѕРЅРѕРјРёС‡РЅС‹Рј.

**Hot-path optimization (D194 В§perf):** codegen detect'РёС‚ case РєРѕРіРґР° binding РёРјРµРµС‚
С‚РёРї `Consumable[Never]` **Р** РЅРµ satisfies `WithExitTimeout`. Р’ СЌС‚РѕРј СЃР»СѓС‡Р°Рµ
elidet'СЃСЏ:
- Cancel-shield setup/teardown (`on_exit` РіР°СЂР°РЅС‚РёСЂРѕРІР°РЅРѕ РЅРµ throws в†’ РЅРµС‚ MultiError compose)
- Timeout resolution (5s hardcoded РЅРµ РЅСѓР¶РµРЅ вЂ” release РёРЅСЃС‚Р°РЅС‚)
- `outcome` construction (Mutex РЅРµ СЂР°Р·Р»РёС‡Р°РµС‚ Success/Failure/Panic)

Р РµР·СѓР»СЊС‚Р°С‚: `consume _l = mu.acquire() { body }` РєРѕРјРїРёР»РёСЂСѓРµС‚СЃСЏ РІ `body; mu.release()` вЂ”
zero overhead vs raw defer pattern. **РљСЂРёС‚РёС‡РЅРѕ РґР»СЏ hot-paths** (lock contention,
high-frequency permits).

### D195 (NEW) вЂ” Application nesting + finalizer scoping

**Р›РѕРєР°С†РёСЏ:** `spec/decisions/04-effects.md`.

РљРѕРіРґР° РІР»РѕР¶РµРЅ `with Application = h2 { with Application = h1 { ... } }`:

1. **Inner handler РїРѕР±РµР¶РґР°РµС‚** (СЃС‚Р°РЅРґР°СЂС‚РЅР°СЏ СЃРµРјР°РЅС‚РёРєР° effect-stack) вЂ” РІСЃРµ
   `Application.*` РѕРїРµСЂР°С†РёРё РІРЅСѓС‚СЂРё h2-scope Р±СЊСЋС‚ РїРѕ h2.
2. **Finalizers РќР• РЅР°СЃР»РµРґСѓСЋС‚СЃСЏ** вЂ” h2 РёРјРµРµС‚ СЃРІРѕР№ РїСѓСЃС‚РѕР№ registry.
3. **РџСЂРё РІС‹С…РѕРґРµ РёР· h2 scope** вЂ” fires h2.finalizers. Р—Р°С‚РµРј (РµСЃР»Рё scope РїСЂРѕРґРѕР»Р¶Р°РµС‚СЃСЏ)
   restoration Рє h1, РµРіРѕ finalizers РїСЂРѕРґРѕР»Р¶Р°СЋС‚ РєРѕРїРёС‚СЊСЃСЏ.
4. **Use case**: testing вЂ” РєР°Р¶РґС‹Р№ test РїРѕР»СѓС‡Р°РµС‚ СЃРІРѕР№ isolated Application,
   РЅРµ shareРёС‚ finalizers СЃ runner'РѕРј.
5. **Default exit_timeout inheritance**: РќР• РЅР°СЃР»РµРґСѓРµС‚СЃСЏ вЂ” h2 РёРјРµРµС‚ СЃРІРѕР№
   `default_exit_timeout_value` (РµСЃР»Рё Р·Р°РґР°РЅ). Р•СЃР»Рё h2 СЃРѕР·РґР°РЅ Р±РµР· Р°СЂРіСѓРјРµРЅС‚Р° вЂ”
   РёСЃРїРѕР»СЊР·СѓРµС‚ hardcoded default 5s, **РЅРµ** h1 Р·РЅР°С‡РµРЅРёРµ.
6. **Cross-fiber propagation**: РїСЂРё `spawn { ... }` РґРѕС‡РµСЂРЅРёР№ fiber РІРёРґРёС‚
   СЂРѕРґРёС‚РµР»СЊСЃРєРёР№ effect-stack (D75 cancel-token model extension), РІРєР»СЋС‡Р°СЏ
   Р°РєС‚РёРІРЅС‹Р№ Application.
7. **Boot order**: `Application.handler(...)` constructor РґРѕР»Р¶РµРЅ **РїРѕР»РЅРѕСЃС‚СЊСЋ
   Р·Р°РІРµСЂС€РёС‚СЊСЃСЏ** РґРѕ РІС…РѕРґР° РІ `with`-Р±Р»РѕРє. РќРёРєР°РєРёС… СЂРµРіРёСЃС‚СЂР°С†РёР№ finalizer'РѕРІ
   РІРѕ РІСЂРµРјСЏ construction вЂ” С‚РѕР»СЊРєРѕ РёР· body. Р•СЃР»Рё constructor throws вЂ” `with`
   РЅРµ РІС…РѕРґРёС‚, on_exit РЅРµ РІС‹Р·С‹РІР°РµС‚СЃСЏ (D188 R1 partial-construction safety).

### D196 (NEW) вЂ” Init type constraints РґР»СЏ `consume X = expr { body }`

**Р›РѕРєР°С†РёСЏ:** `spec/decisions/03-syntax.md`.

`expr` РїРѕСЃР»Рµ `=` РґРѕР»Р¶РµРЅ statically resolve Рє С‚РёРїСѓ implementing `Consumable[E]`:

1. **РџСЂСЏРјРѕР№ Consumable**: `db.begin()` в†’ `Transaction` вњ“.
2. **Result/Option unwrap С‡РµСЂРµР· `?` / `!!`**: `db.try_begin()? ` в†’ РµСЃР»Рё РІРѕР·РІСЂР°С‰Р°РµС‚
   `Result[Transaction, DbError]`, РїРѕСЃР»Рµ `?` СЂР°Р·РІС‘СЂС‚С‹РІР°РµС‚СЃСЏ РІ `Transaction` вЂ”
   СЂР°Р·СЂРµС€РµРЅРѕ.
3. **Conditional**: `if cond { open_a() } else { open_b() }` вЂ” РѕР±Рµ РІРµС‚РєРё РґРѕР»Р¶РЅС‹
   РІРѕР·РІСЂР°С‰Р°С‚СЊ СЃРѕРІРјРµСЃС‚РёРјС‹Р№ Consumable type. РРЅР°С‡Рµ D196-divergent-consumable.
4. **Method-chain**: `db.with_config(cfg).begin()` вЂ” С„РёРЅР°Р»СЊРЅС‹Р№ return type
   РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ Consumable.
5. **Wrapped РІ Option / Result Р±РµР· unwrap**: `consume tx = maybe_tx()` РіРґРµ
   `maybe_tx() -> Option[Transaction]` в†’ D196-wrapped-init-needs-unwrap.
   Suggestion: В«use `consume tx = maybe_tx()!! { ... }` РёР»Рё check СЃРЅР°С‡Р°Р»Р°В».
6. **Memory ordering РґР»СЏ acquisition**: `init` evaluation РїРѕР»РЅРѕСЃС‚СЊСЋ Р·Р°РІРµСЂС€Р°РµС‚СЃСЏ
   РґРѕ scope-entry (acquire semantics); cleanup РІРёРґРёС‚ С„РёРЅР°Р»СЊРЅРѕРµ СЃРѕСЃС‚РѕСЏРЅРёРµ СЂРµСЃСѓСЂСЃР°.

### D197 (NEW) вЂ” Cleanup re-entrance

**Р›РѕРєР°С†РёСЏ:** `spec/decisions/03-syntax.md`.

`on_exit` body **РјРѕР¶РµС‚ СЃРѕРґРµСЂР¶Р°С‚СЊ РІР»РѕР¶РµРЅРЅС‹Рµ** `consume {}` Р±Р»РѕРєРё:

```nova
fn Connection consume @on_exit(outcome ScopeOutcome) Fail[IoError] -> () {
    // closing the connection requires acquiring lock
    consume _l = @cleanup_mutex.acquire() {
        @do_close()
    }
}
```

**РџСЂР°РІРёР»Р°:**
1. Outer cancel-shield РѕСЃС‚Р°С‘С‚СЃСЏ Р°РєС‚РёРІРµРЅ РЅР° РІСЂРµРјСЏ РІСЃРµР№ outer `on_exit` body.
2. Inner `consume {}` СЃРѕР·РґР°С‘С‚ СЃРІРѕР№ shield СЃ СЃРІРѕРёРј timeout, РЅРѕ cancel РѕСЃС‚Р°С‘С‚СЃСЏ
   РіР»РѕР±Р°Р»СЊРЅРѕ pending (РЅРµ РґРѕСЃС‚Р°РІР»СЏРµС‚СЃСЏ РґРѕ РІС‹С…РѕРґР° **outer** cleanup).
3. Inner `on_exit` РѕС€РёР±РєРё compose РІ Р»РѕРєР°Р»СЊРЅС‹Р№ MultiError; РµСЃР»Рё РѕРЅ throws вЂ” outer
   `on_exit` РїРѕР»СѓС‡Р°РµС‚ СЌС‚Рѕ РІ propagation.
4. **Р“Р»СѓР±РёРЅР° re-entrance limited 256** (same as MultiError depth-limit D193).
   РџСЂРё РїСЂРµРІС‹С€РµРЅРёРё вЂ” runtime error `D197-cleanup-reentrance-depth-exceeded`
   composes РІ MultiError; cleanup РїСЂРѕРґРѕР»Р¶Р°РµС‚ СЂР°Р·РІРѕСЂР°С‡РёРІР°С‚СЊСЃСЏ СЃ СЌС‚РѕР№
   РѕС€РёР±РєРѕР№ РєР°Рє В«...truncatedВ» entry.
5. **Р—Р°РїСЂРµС‰РµРЅРѕ** вЂ” re-entrance СЃ С‚РµРј Р¶Рµ СЂРµСЃСѓСЂСЃРѕРј (D196 already covers вЂ” linear types).

### D198 (NEW) вЂ” Realtime + cleanup-timeout interaction

**Р›РѕРєР°С†РёСЏ:** `spec/decisions/03-syntax.md`.

#### РЎРµРјР°РЅС‚РёРєР° `#realtime` attribute (cross-ref D172, Plan 103.6/103.7)

`#realtime` РЅР° С„СѓРЅРєС†РёРё вЂ” **РіР°СЂР°РЅС‚РёСЏ callee** (callee promises bounded execution):

- Р’РЅСѓС‚СЂРё `#realtime` fn body: РјРѕР¶РЅРѕ РІС‹Р·С‹РІР°С‚СЊ С‚РѕР»СЊРєРѕ РґСЂСѓРіРёРµ `#realtime` fns РёР»Рё
  `#realtime`-annotated primitive operations. Parking ops, allocations, GC
  pauses Р·Р°РїСЂРµС‰РµРЅС‹.
- **РќРёРєР°РєРёС… РѕРіСЂР°РЅРёС‡РµРЅРёР№ РЅР° caller** вЂ” РѕР±С‹С‡РЅР°СЏ fn СЃРІРѕР±РѕРґРЅРѕ РјРѕР¶РµС‚ РІС‹Р·РІР°С‚СЊ
  `#realtime` fn. РђС‚СЂРёР±СѓС‚ РѕРїРёСЃС‹РІР°РµС‚ СЃРІРѕР№СЃС‚РІРѕ callee, РЅРµ constraint caller'Р°.
- РђРЅР°Р»РѕРіРёСЏ: C++ `constexpr fn` callable from runtime, РЅРѕ РІРЅСѓС‚СЂРё С‚РѕР»СЊРєРѕ
  constexpr ops.

#### РџСЂР°РІРёР»Рѕ РґР»СЏ cleanup

Codegen СЃРјРѕС‚СЂРёС‚ РЅР° **enclosing function** РіРґРµ РЅР°С…РѕРґРёС‚СЃСЏ `consume {}`:

```
// РІ РѕР±С‹С‡РЅРѕР№ fn:
fn foo() Fail[E] -> () {
    consume r = expr { body }
    // => let _timeout = nv_resolve_exit_timeout(r)    // WithExitTimeout / App / 5s
}

// РІ #realtime fn:
#realtime
fn bar() -> () {
    consume r = expr { body }
    // => let _timeout = Duration.zero                 // hardcoded РІ codegen
}
```

#### РЎР»РµРґСЃС‚РІРёСЏ вЂ” СЃР»РµРґСѓСЋС‚ Р°РІС‚РѕРјР°С‚РёС‡РµСЃРєРё РёР· РїСЂР°РІРёР»Р° #realtime

1. **`on_exit` РјРµС‚РѕРґ СЂРµСЃСѓСЂСЃР° РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ `#realtime`**, РёРЅР°С‡Рµ compile error
   РІРЅСѓС‚СЂРё `bar` body (РЅРµР»СЊР·СЏ РІС‹Р·РІР°С‚СЊ non-#realtime fn РёР· #realtime). Р­С‚Рѕ
   Р·РЅР°С‡РёС‚ resource-С‚РёРї РёСЃРїРѕР»СЊР·СѓРµРјС‹Р№ РІ realtime-context СѓР¶Рµ СЃРїСЂРѕРµРєС‚РёСЂРѕРІР°РЅ РґР»СЏ
   РЅРµРіРѕ (`MutexGuard.release`, atomic ops, etc.).
2. **`WithExitTimeout` impl** СЂРµСЃСѓСЂСЃР° РЅРµ РІС‹Р·С‹РІР°РµС‚СЃСЏ вЂ” РїРѕС‚РѕРјСѓ С‡С‚Рѕ
   `nv_resolve_exit_timeout` РЅРµ РІС‹Р·С‹РІР°РµС‚СЃСЏ РІРѕРІСЃРµ.
3. **`Application` effect** РЅРµ Р·Р°РїСЂР°С€РёРІР°РµС‚СЃСЏ вЂ” same reason.
4. **Suspend РІ `on_exit`** РЅРµРІРѕР·РјРѕР¶РµРЅ РїРѕ РїСЂР°РІРёР»Сѓ `#realtime` body restriction
   (С‡РµСЂРµР· D172, РЅРµ С‡РµСЂРµР· РЅР°С€Сѓ РЅРѕРІСѓСЋ РїСЂРѕРІРµСЂРєСѓ).

#### Р§С‚Рѕ РќР• РґРµР»Р°РµРј

- вќЊ Compile-time heuristic В«РїРѕРїС‹С‚Р°РµС‚СЃСЏ Р»Рё Application overrideВ» вЂ” РЅРµ РЅСѓР¶РЅРѕ,
  РїСЂР°РІРёР»Рѕ `#realtime` body СѓР¶Рµ РІСЃС‘ РѕРіСЂР°РЅРёС‡РёРІР°РµС‚.
- вќЊ Runtime fallback Рє Application РІ realtime вЂ” codegen СЌРјРёС‚РёС‚ zero РЅР°РїСЂСЏРјСѓСЋ.
- вќЊ Р”РѕРїРѕР»РЅРёС‚РµР»СЊРЅС‹Рµ constraints РЅР° caller вЂ” РЅРµ РЅСѓР¶РЅРѕ, Р°С‚СЂРёР±СѓС‚ СЌС‚Рѕ callee promise.

### D158 amend вЂ” СѓРїСЂРѕС‰РµРЅРёРµ panic composition

РЎРѕС…СЂР°РЅСЏРµС‚СЃСЏ РїСЂР°РІРёР»Рѕ В«panic composes, РЅРµ abortВ». РЈР±РёСЂР°РµС‚СЃСЏ ErrorKind discrimination.
Composition С‡РµСЂРµР· plain `Error` + `MultiError.suppressed`.

### D160 retract вЂ” `defer |result|` СѓРґР°Р»СЏРµС‚СЃСЏ

D160 РїРѕРјРµС‡Р°РµС‚СЃСЏ В«withdrawn in favor of D188В».

### D161 amend вЂ” typed MultiError СѓРїСЂРѕС‰Р°РµС‚СЃСЏ

РЈР±РёСЂР°РµС‚СЃСЏ `ErrorKind`. РЎС‚СЂСѓРєС‚СѓСЂР° per D193.

### D162 amend вЂ” coverage rules СѓРїСЂРѕС‰Р°СЋС‚СЃСЏ

- `consume X = ... { }` вЂ” exhaustively-by-construction, no analysis
- Raw `consume + defer` вЂ” single rule (defer body РґРѕР»Р¶РµРЅ СЃРѕРґРµСЂР¶Р°С‚СЊ consume call)
- РЈР±РёСЂР°СЋС‚СЃСЏ: `D162-double-cover`, `D162-conditional-cover-warning`

### D184 retract вЂ” cancel-shielding РІСЃС‚СЂРѕРµРЅ

Implementation-detail `consume {}`. Per-resource timeout С‡РµСЂРµР· `exit_timeout()`.

### D185 amend вЂ” `Cleanup` effect СЃС‚Р°Р» observability-only

Operations: `on_scope_enter(label, timeout)` / `on_scope_exit(label, outcome)`.
Default no-op. Zero-overhead РµСЃР»Рё РЅРµ РёСЃРїРѕР»СЊР·РѕРІР°РЅ.

### D186 retract вЂ” module finalizers С‡РµСЂРµР· РїР°С‚С‚РµСЂРЅ

РќРµ РѕС‚РґРµР»СЊРЅС‹Р№ language primitive. `Consumable[Application]` idiom.

### D187 amend в†’ D190

REJECTED СЂР°СЃС€РёСЂСЏРµС‚СЃСЏ С‡РµСЂРµР· D190.

### D90 В§7 amend вЂ” cancel/interrupt РєР°Рє `Failure(CancelError)`

Cancel/interrupt РІ body РїСЂРёС…РѕРґРёС‚ РєР°Рє `Failure(CancelError)` РІ outcome.
РќРµ РѕС‚РґРµР»СЊРЅС‹Р№ exit-path РІ protocol.

---

## Р—Р°РїСЂРµС‰С‘РЅРЅС‹Рµ shortcut'С‹ (A15 С‡РµРє-Р»РёСЃС‚)

1. вќЊ String-payload РІ `MultiError`/`Error` вЂ” typed throughout.
2. вќЊ `unimplemented!()` / `todo!()` РІ production-path РєРѕРґРµ.
3. вќЊ Hardcoded С‚РёРїС‹ РґР»СЏ generic params (Consumable[int]-only Рё С‚.Рї.).
4. вќЊ В«Future phaseВ» РєРѕРјРјРµРЅС‚Р°СЂРёРё РІ РЅРѕРІС‹С… С‚РµСЃС‚Р°С….
5. вќЊ `#[allow(dead_code)]` РґР»СЏ РЅРµСЂРµР°Р»РёР·РѕРІР°РЅРЅС‹С… feature-РїРѕР»РµР№.
6. вќЊ String-comparison РІРјРµСЃС‚Рѕ typed-match РґР»СЏ error-routing.
7. вќЊ Mock'Рё runtime РІ integration-С‚РµСЃС‚Р°С… cleanup-flow.
8. вќЊ Skip-flags РІ С„РёРєСЃС‚СѓСЂР°С… Р±РµР· СЏРІРЅРѕР№ spec-РїСЂРёС‡РёРЅС‹ + issue-СЃСЃС‹Р»РєРё.
9. вќЊ Hardcoded timeout-РєРѕРЅСЃС‚Р°РЅС‚С‹ РєСЂРѕРјРµ РєРѕРЅРµС‡РЅРѕРіРѕ fallback `5s` (РІСЃРµРіРґР° С‡РµСЂРµР·
   3-level resolution: WithExitTimeout в†’ Application effect в†’ 5s).
10. вќЊ `on_exit` РёРјРїР»РµРјРµРЅС‚РёСЂРѕРІР°РЅРЅС‹Р№ РєР°Рє inline-С„СѓРЅРєС†РёСЏ Р±РµР· protocol-dispatch.
11. вќЊ Cancel-shield СЃ boolean-С„Р»Р°РіРѕРј (РЅРµ counter вЂ” Rust scopeguard lesson; C++23
    РёСЃРїРѕР»СЊР·СѓРµС‚ counter).
12. вќЊ Suppression-chain Р±РµР· cycle-check (Java JDK-8287921 lesson).
13. вќЊ MultiError accessor methods РІРѕР·РІСЂР°С‰Р°СЋС‰РёРµ raw arrays Р±РµР· iterator.
14. вќЊ FFI-cleanup Р±РµР· attestation С‡С‚Рѕ C-side РЅРµ leaks.
15. вќЊ `Consumable[Never]` Р±РµР· elision shield (СѓРїСѓСЃРєР°РµС‚СЃСЏ hot-path optimization
    D194 В§perf).
16. вќЊ Application effect propagation РІ spawn Р±РµР· cancel-token (D195 В§6).
17. вќЊ Realtime functions РёСЃРїРѕР»СЊР·СѓСЋС‰РёРµ Application timeout (D198 violation).
18. вќЊ `Cleanup` effect handler РІРѕР·РІСЂР°С‰Р°СЋС‰РёР№ С‡С‚Рѕ-С‚Рѕ РєСЂРѕРјРµ no-op return type.

---

## Р¤Р°Р·С‹ (РґРµРєРѕРјРїРѕР·РёСЂРѕРІР°РЅРЅС‹Рµ)

### Р¤.0 вЂ” GATE (spec drafts + migration audit)

- **Р¤.0.1** Drafts D188вЂ“D198 + amends/retracts (11 D-Р±Р»РѕРєРѕРІ).
- **Р¤.0.2** Audit `nova_tests/` + `examples/` + `stdlib/`:
  - Р’СЃРµ `okdefer` вЂ” СЃРїРёСЃРѕРє РјРёРіСЂР°С†РёР№
  - Р’СЃРµ `defer |result|` вЂ” СЃРїРёСЃРѕРє РјРёРіСЂР°С†РёР№
  - Р’СЃРµ `errdefer` вЂ” РєР°РєРёРµ РІ `consume {}`, РєР°РєРёРµ РІ `defer + flag`
  - Р’СЃРµ resource-like С‚РёРїС‹ вЂ” РєР°РЅРґРёРґР°С‚С‹ РЅР° `on_exit` impl
  - `interrupt + errdefer` вЂ” D90 В§7 migration cases
- **Р¤.0.3** Acceptance A1вЂ“A30 С„РёРЅР°Р»РёР·РёСЂРѕРІР°РЅ.
- **Р¤.0.4** Cross-check СЃ Plan 82, 83.10/11, 100.5/6/8, 101, 103.1/6, 104.1.

### Р¤.1 вЂ” Core: protocol + parser + checker

- **Р¤.1.1** Prelude: `type Consumable[E] protocol` (РѕРґРёРЅ РјРµС‚РѕРґ on_exit),
  `type WithExitTimeout protocol` (РѕРїС†РёРѕРЅР°Р»СЊРЅС‹Р№, РѕС‚РґРµР»СЊРЅС‹Р№), `type ScopeOutcome`.
- **Р¤.1.2** Prelude: `type Never` (РґР»СЏ D194).
- **Р¤.1.3** Parser: `consume IDENT = EXPR { BODY }` (lookahead РЅР° `{`).
- **Р¤.1.4** AST: `Stmt::ConsumeScope { binding, init, body }`.
- **Р¤.1.5** Type-checker:
  - `init: Consumable[E]` (structural match)
  - РІС‹РІРѕРґ E
  - `Consumable[Never]` permits caller without `Fail[E]` (D194)
- **Р¤.1.6** D162 simplified: `consume {}` exhaustive by construction в†’ no check
  РґР»СЏ scope-bindings; РїСЂР°РІРёР»Рѕ СЃРѕС…СЂР°РЅСЏРµС‚СЃСЏ РґР»СЏ raw `consume + defer`.
- **Р¤.1.7** Generic bound `[T Consumable[E]]` (Plan 101 integration).
- **Р¤.1.8** Tests T1.1вЂ“T1.8.

### Р¤.2 вЂ” Codegen + runtime fundamentals

- **Р¤.2.1** Codegen desugaring (СЃРј. В§В«DesugaringВ»).
- **Р¤.2.2** Runtime:
  - `nv_consume_enter(timeout_ns) -> ConsumeScope*`
  - `nv_consume_exit(scope, outcome_kind, error_ptr)`
  - `nv_consume_drop_scope(scope)` (memory cleanup)
- **Р¤.2.3** Exactly-once invariant вЂ” runtime counter, panic РµСЃР»Рё в‰Ґ 2 invocations.
- **Р¤.2.4** Partial-construction safety вЂ” codegen СЌРјРёС‚РёС‚ check В«init succeededВ»
  РїРµСЂРµРґ any setup РґР»СЏ on_exit.
- **Р¤.2.5** LIFO composition: scope-stack per-fiber.
- **Р¤.2.6** Mixed `consume {}` + `defer` LIFO вЂ” shared stack.
- **Р¤.2.7** `nv_resolve_exit_timeout(typeid, has_with_exit_timeout)` вЂ” **РµРґРёРЅР°СЏ
  runtime С„СѓРЅРєС†РёСЏ** С‡РµСЂРµР· vtable lookup (РЅРµ per-callsite codegen). Р РµР°Р»РёР·СѓРµС‚
  3-level fallback (WithExitTimeout в†’ Application effect в†’ hardcoded 5s).
  Р’С‹Р·С‹РІР°РµС‚СЃСЏ РѕРґРёРЅ СЂР°Р· РїСЂРё scope-entry, СЂРµР·СѓР»СЊС‚Р°С‚ РєСЌС€РёСЂСѓРµС‚СЃСЏ РІ Р»РѕРєР°Р»РєРµ.
  РџСЂРµРёРјСѓС‰РµСЃС‚РІРѕ vs per-callsite: РјРµРЅСЊС€Рµ binary size, РµРґРёРЅР°СЏ С‚РѕС‡РєР°
  РјРѕРґРёС„РёРєР°С†РёРё, РїСЂРѕС‰Рµ РґР»СЏ inlining VM/JIT.
- **Р¤.2.8** **Hot-path optimization (D194 В§perf):** codegen detect'РёС‚
  `Consumable[Never]` + no `WithExitTimeout` impl в†’ elidet'СЃСЏ shield/timeout/
  outcome construction. Compile-time decision; СЃС‚Р°С‚РёС‡РµСЃРєРё РїСЂРѕРІРµСЂСЏРµРјРѕ.
  Р РµР·СѓР»СЊС‚Р°С‚: `consume _l = mu.acquire() { body }` == `body; mu.release()`.
- **Р¤.2.9** Init type constraints (D196): type-checker rules РґР»СЏ conditional,
  Result/Option unwrap, method-chain init expressions.
- **Р¤.2.10** `outcome.failure_as[T]() -> Option[T]` helper РІ prelude.
- **Р¤.2.11** Generic `[T Consumable[Never]]` вЂ” type-checker СЃРЅРёРјР°РµС‚
  С‚СЂРµР±РѕРІР°РЅРёРµ `Fail[E]` Сѓ caller'Р°.
- **Р¤.2.12** Cleanup re-entrance (D197): inner consume{} inside on_exit allowed;
  inner shield stacked, cancel pending РґРѕ РїРѕР»РЅРѕРіРѕ outer exit.
- **Р¤.2.13** Tests T2.1вЂ“T2.12.

### Р¤.3 вЂ” Cancel-shield + async cleanup (D191/D192)

- **Р¤.3.1** Runtime: `nv_consume_enter_shield(timeout_ns)` вЂ” СѓСЃС‚Р°РЅР°РІР»РёРІР°РµС‚
  `fiber->cancel_masked = true` + СЂРµРіРёСЃС‚СЂРёСЂСѓРµС‚ deadline.
- **Р¤.3.2** РќР° РєР°Р¶РґРѕР№ suspend-С‚РѕС‡РєРµ cleanup-body: РµСЃР»Рё deadline РїСЂРµРІС‹С€РµРЅ в†’
  throw `CleanupTimeoutError`.
- **Р¤.3.3** `Duration.zero` enforcement (D192): runtime panic РµСЃР»Рё cleanup РґРµР»Р°РµС‚
  suspend.
- **Р¤.3.4** `Duration.MAX` warning (D192) вЂ” compile-time `D192-infinite-timeout-warn`.
- **Р¤.3.5** Application effect fallback (D192 level-2): codegen resolve РІС‹Р·С‹РІР°РµС‚
  `perform Application.default_exit_timeout()` РµСЃР»Рё Р°РєС‚РёРІРµРЅ handler. Library
  pattern РґР»СЏ per-instance вЂ” С‡РµСЂРµР· resource factory (`Db.connect(url,
  exit_timeout: 60.s())`), РЅРµ language feature. Cross-ref Plan 03.4
  (effect-aware tooling).
- **Р¤.3.6** Realtime integration (D172/D198): `#realtime` в†’ `exit_timeout = zero`
  enforced; **bypass 3-level resolution** (Application effect Рё WithExitTimeout
  РёРіРЅРѕСЂРёСЂСѓСЋС‚СЃСЏ). Compile-time warning `D198-realtime-application-override` РµСЃР»Рё
  СЃС‚Р°С‚РёС‡РµСЃРєРё detect non-zero Application internal call.
- **Р¤.3.7** Cross-platform: validate СЃ fiber-arena РЅР° Windows (Plan 82) +
  Linux (libuv).
- **Р¤.3.8** Tests T3.1вЂ“T3.12.

### Р¤.4 вЂ” Stdlib core resources

- **Р¤.4.1** `Transaction` вЂ” `Consumable[DbError]`, commit/rollback РїРѕ outcome.
- **Р¤.4.2** `File` вЂ” `Consumable[IoError]`, close (РѕРґРёРЅР°РєРѕРІРѕ РЅР° all paths).
- **Р¤.4.3** `MutexGuard`, `RwLockGuard`, `ReentrantGuard` вЂ” `Consumable[Never]` (D194).
- **Р¤.4.4** `SemaphorePermit` вЂ” `Consumable[Never]`.
- **Р¤.4.5** `BufReader` / `BufWriter` вЂ” `Consumable[IoError]` (flush + close).
- **Р¤.4.6** `CancelScope` вЂ” `Consumable[Never]`.
- **Р¤.4.7** Tests T4.1вЂ“T4.6.

### Р¤.5 вЂ” Stdlib extended resources

- **Р¤.5.1** `TcpStream` / `TcpListener` / `UdpSocket` (Plan 83.12) вЂ” `Consumable[IoError]`.
- **Р¤.5.2** `Channel` / `ChanReader` / `ChanWriter` (Plan 65) вЂ” `Consumable[Never]`.
- **Р¤.5.3** `JoinHandle` / fiber handles (Plan 83.4.2 РµСЃР»Рё ready) вЂ”
  `Consumable[Never]` (await + drop).
- **Р¤.5.4** `Stream` (Plan 84 РµСЃР»Рё exists) вЂ” `Consumable[E]`.
- **Р¤.5.5** Connection pools вЂ” `Consumable[ConnPoolError]`.
- **Р¤.5.6** Tests T5.1вЂ“T5.5.

### Р¤.6 вЂ” MultiError + Error types (D193)

- **Р¤.6.1** РЈРґР°Р»РёС‚СЊ `ErrorKind` enum РїРѕР»РЅРѕСЃС‚СЊСЋ.
- **Р¤.6.2** `MultiError { primary, suppressed }` вЂ” typed `Error`.
- **Р¤.6.3** API: `@primary()`, `@suppressed()`, `@walk() -> Iter[Error]`,
  `@fmt_chain()`, `@find_first_panic() -> Option[str]`.
- **Р¤.6.4** Cycle-safety: identity-check РІ `nv_compose_error`.
- **Р¤.6.5** Depth-limit 256 + truncation entry.
- **Р¤.6.6** Concrete error types РІ prelude:
  - `CancelError { reason str }`
  - `CleanupTimeoutError { duration Duration }`
- **Р¤.6.7** Tests T6.1вЂ“T6.6.

### Р¤.7 вЂ” `Cleanup` effect (telemetry only)

- **Р¤.7.1** Spec D185 (СѓРїСЂРѕС‰С‘РЅРЅС‹Р№): observability-only.
- **Р¤.7.2** Operations: `on_scope_enter(label str, timeout Duration)`,
  `on_scope_exit(label str, outcome ScopeOutcome)`.
- **Р¤.7.3** Default handler вЂ” no-op (zero-overhead).
- **Р¤.7.4** Handler throw Р·Р°РїСЂРµС‰С‘РЅ (compile error D185-cleanup-handler-throw).
- **Р¤.7.5** **OpenTelemetry wire format** (D185 amend):
  - `on_scope_enter` РјР°РїРїРёС‚ РІ OTel span: `attributes = { "cleanup.label": label,
    "cleanup.timeout_ms": timeout.ms(), "cleanup.start_time_ns": now() }`
  - `on_scope_exit` Р·Р°РєСЂС‹РІР°РµС‚ span: `status = match outcome { Success => OK,
    Failure(_) => ERROR, Panic(_) => ERROR_PANIC }`; `attributes.duration_ms = ...`
  - Trace context propagation: spans nested correctly С‡РµСЂРµР· scope-stack
  - Compatible СЃ std OpenTelemetry SDK С‡РµСЂРµР· FFI bridge (Plan 100.5)
- **Р¤.7.6** Example handler `examples/cleanup_tracing.nv` вЂ” full OTel export pipeline.
- **Р¤.7.7** Tests T7.1вЂ“T7.5.

### Р¤.8 вЂ” `Application` РєР°Рє СЌС„С„РµРєС‚ + finalizers

- **Р¤.8.1** Stdlib `Application` **СЌС„С„РµРєС‚** (РЅРµ consume-value):
  ```nova
  effect Application {
      fn register_finalizer(f fn() -> ()) -> ()
      fn default_exit_timeout() -> Duration
  }

  type ApplicationHandler {
      finalizers []fn() -> ()
      default_exit_timeout_value Duration
  }

  fn Application.handler(default_exit_timeout Duration = 5.s()) -> ApplicationHandler

  fn ApplicationHandler @register_finalizer(f fn() -> ()) -> () => @finalizers.push(f)
  fn ApplicationHandler @default_exit_timeout() -> Duration => @default_exit_timeout_value

  // Handler СЃР°Рј Consumable вЂ” finalizers fire РїСЂРё РІС‹С…РѕРґРµ РёР· with-Р±Р»РѕРєР°:
  fn ApplicationHandler consume @on_exit(_outcome ScopeOutcome) -> () {
      for f in @finalizers.reverse() { f() }
  }
  ```
- **Р¤.8.2** Topo-order: registry preserves registration order; reverse = topo-order.
- **Р¤.8.3** Idiomatic main pattern С‡РµСЂРµР· `with Application = ...`:
  ```nova
  fn main() Io -> () {
      with Application = Application.handler(default_exit_timeout: 10.s()) {
          run_server()
          // anywhere РіР»СѓР±РѕРєРѕ: Application.register_finalizer(|| { ... })
      }
      // handler.on_exit fires finalizers
  }
  ```
- **Р¤.8.4** Integration СЃ D192: codegen resolve_exit_timeout РІРёРґРёС‚ Р°РєС‚РёРІРЅС‹Р№
  Application handler в†’ Р·РѕРІС‘С‚ `Application.default_exit_timeout()` РєР°Рє
  second-level fallback.
- **Р¤.8.5** **Nested Application semantics (D195):** inner handler РїРѕР±РµР¶РґР°РµС‚,
  РЅРµ РЅР°СЃР»РµРґСѓРµС‚ finalizers/timeout; reset РЅР° exit. Use case: isolated test apps.
- **Р¤.8.6** **Cross-fiber propagation (D195 В§6):** РїСЂРё `spawn { ... }` child fiber
  РІРёРґРёС‚ СЂРѕРґРёС‚РµР»СЊСЃРєРёР№ Application С‡РµСЂРµР· effect-stack snapshot (cross-ref D80
  per-fiber handler-snapshot).
- **Р¤.8.7** Р”РѕРєСѓРјРµРЅС‚Р°С†РёСЏ: finalizers РќР• РІС‹Р·С‹РІР°СЋС‚СЃСЏ РЅР° abort/SIGKILL (РѕРіСЂР°РЅРёС‡РµРЅРёРµ
  РІСЃРµС… СЏР·С‹РєРѕРІ, РЅРµ bug).
- **Р¤.8.8** Cross-ref Plan 100.4.1 (D158) вЂ” handler cleanup body РјРµС…Р°РЅРёР·Рј
  reuse'С‚СЃСЏ.
- **Р¤.8.9** Optional: `#[run_on_abort]` Р°С‚СЂРёР±СѓС‚ вЂ” РѕС‚Р»РѕР¶РµРЅРѕ РІ follow-up Plan 110.X.
- **Р¤.8.10** Tests T8.1вЂ“T8.8.

### Р¤.9 вЂ” Migration: deprecate + auto-fix tool

- **Р¤.9.1** Parser РїСЂРёРЅРёРјР°РµС‚ СЃС‚Р°СЂС‹Рµ С„РѕСЂРјС‹ + emit warnings.
- **Р¤.9.2** Auto-migration tool `nova fix --simplify-cleanup`:
  - `consume tx = ...; errdefer { rollback }; okdefer { commit }` в†’ `consume tx = ... { ... }`
  - `errdefer { x }` (Р±РµР· resource) в†’ `let mut done = false; defer { if !done { x } }`
  - `defer |result| match result { ... }` в†’ `consume X = ... { ... }` РёР»Рё `with Cleanup = h { ... }`
- **Р¤.9.3** Migration СЃСѓС‰РµСЃС‚РІСѓСЋС‰РёС… С„РёРєСЃС‚СѓСЂ `nova_tests/plan100_4_*`.
- **Р¤.9.4** D90 В§7 codegen: cancel/interrupt РІ body в†’ `Failure(CancelError)`.
- **Р¤.9.5** РЈРґР°Р»РµРЅРёРµ `DeferWithResult` AST-СѓР·Р»Р°; СѓРґР°Р»РµРЅРёРµ `DeferResult[T,E]` prelude.
- **Р¤.9.6** РЈРґР°Р»РµРЅРёРµ `errdefer`/`okdefer`/`defer |r|` РїРѕСЃР»Рµ minor-release.
- **Р¤.9.7** Tests T9.1вЂ“T9.5.

### Р¤.10 вЂ” Diagnostic UX + LSP integration

- **Р¤.10.1** D162 в†’ suggestion В«use `consume {}` insteadВ».
- **Р¤.10.2** D189-deprecated-* + auto-fix templates.
- **Р¤.10.3** D188-not-consumable (init type РЅРµ Consumable) вЂ” suggestion РєР°Рє implement.
- **Р¤.10.4** D188-malformed-on-exit (РЅРµРїСЂР°РІРёР»СЊРЅР°СЏ signature on_exit) вЂ” suggestion correct form.
- **Р¤.10.5** D192-zero-timeout-suspend (runtime) вЂ” hint РїСЂРѕ realtime contexts.
- **Р¤.10.6** LSP quick-fix actions (Plan 104.1 integration):
  - Quick-fix В«convert errdefer to consume {}В»
  - Hover info РЅР° `consume {}` РїРѕРєР°Р·С‹РІР°РµС‚ Consumable impl
  - Code-action В«implement Consumable for this typeВ»
- **Р¤.10.7** Tests T10.1вЂ“T10.6.

### Р¤.11 вЂ” Concurrency stress + benchmarks

- **Р¤.11.1** Stress tests: racing cancels РІРѕ РІСЂРµРјСЏ cleanup'Р°.
- **Р¤.11.2** Stress tests: nested `consume {}` (10 СѓСЂРѕРІРЅРµР№ РіР»СѓР±РёРЅР°).
- **Р¤.11.3** Stress tests: `consume {}` РІ `parallel for`.
- **Р¤.11.4** Stress tests: simultaneous `on_exit` across С„РёР±РµСЂРѕРІ.
- **Р¤.11.5** Benchmark: cancel-shield + 3-level resolution overhead (target
  в‰¤ Plan 100.4 cleanup baseline + 5% вЂ” РёР·РјРµСЂРёС‚СЊ baseline РІ Р¤.0.4).
- **Р¤.11.6** Benchmark: `exit_timeout` enforcement overhead (target < 50ns).
- **Р¤.11.7** Benchmark: `MultiError` composition (depth 1, 10, 100).
- **Р¤.11.8** Memory leak tests: OOM during cleanup, partial construction,
  panic mid-cleanup.
- **Р¤.11.9** Tests T11.1вЂ“T11.8.

### Р¤.12 вЂ” FFI integration (Plan 100.5)

- **Р¤.12.1** Spec: C-side resources СЃ cleanup needs РјРѕРіСѓС‚ implement Consumable
  С‡РµСЂРµР· FFI wrapper.
- **Р¤.12.2** Cross-language safety: cancel-shield РїСЂРѕР±СЂР°СЃС‹РІР°РµС‚СЃСЏ С‡РµСЂРµР· FFI call
  С‚РѕР»СЊРєРѕ РµСЃР»Рё C-side declared cancellation-safe.
- **Р¤.12.3** Example wrapper: SQLite Connection via FFI.
- **Р¤.12.4** Tests T12.1вЂ“T12.3.

### Р¤.13 вЂ” Regression + cross-platform

- **Р¤.13.1** Full `nova test` в‰Ґ 1158/19.
- **Р¤.13.2** `plan100_4_*` РјРёРіСЂРёСЂРѕРІР°РЅС‹ РіРґРµ applicable.
- **Р¤.13.3** Cross-platform CI: Windows + Linux Г— clang + MSVC.
- **Р¤.13.4** Performance regression vs Plan 100.4 baseline.

### Р¤.14 вЂ” Docs + spec finalize + close

- **Р¤.14.1** Spec finalize: D188вЂ“D198 + amends/retracts.
- **Р¤.14.2** Q-blocks (СЃРј. В§В«Q-blocksВ») вЂ” 10 С€С‚.
- **Р¤.14.3** `docs/project-creation.txt` вЂ” sprint section.
- **Р¤.14.4** `docs/simplifications.md` вЂ” close [M-100.4.*]; record 4 С„РѕСЂРјС‹ в†’ 1.
- **Р¤.14.5** `d:\Sources\nv-lang\nova-private\discussion-log.md` вЂ” design rationale.
- **Р¤.14.6** Memory `project-plan110-status.md`.
- **Р¤.14.7** Tutorial section (`docs/tutorial.md` cleanup chapter РµСЃР»Рё СЌРєР·РёСЃС‚РµРЅС‚).
- **Р¤.14.8** **`docs/cleanup-cookbook.md` (NEW)** вЂ” production-recipe book:
  - Migration patterns (Rust Drop в†’ consume; Go defer в†’ consume; Java try-with-resources в†’ consume)
  - FFI wrappers (SQLite, libcurl, OpenSSL examples)
  - Common patterns: connection pools, file handles, transactions, locks
  - Anti-patterns + debugging
  - Performance: when to use Consumable[Never], hot-path optimization
- **Р¤.14.9** Final merge РІ main.

---

## Q-blocks

- **Q-cleanup-semantics** вЂ” overview, 3 РїР°С‚С‚РµСЂРЅР°.
- **Q-consumable-protocol** вЂ” РєР°Рє РЅР°РїРёСЃР°С‚СЊ `on_exit`, decision tree.
- **Q-migration-from-okdefer** вЂ” auto-fix guide СЃРѕ snippets.
- **Q-when-which-cleanup** вЂ” flowchart: resource в†’ `consume`; logging в†’ `defer`; telemetry в†’ `Cleanup` effect.
- **Q-cancel-and-cleanup** вЂ” РєР°Рє СЂР°Р±РѕС‚Р°РµС‚ cancel-shielding, С‚РёРїРёР·РёСЂРѕРІР°РЅРЅС‹Р№ reason.
- **Q-async-cleanup** (NEW) вЂ” suspend РІ `on_exit`, timeout edge cases, realtime
  constraints, **decision tree В«РєР°Рє РІС‹Р±СЂР°С‚СЊ timeoutВ»** (WithExitTimeout impl в†’
  Application effect в†’ hardcoded 5s в†’ realtime zero).
- **Q-application-effect** (NEW) вЂ” Application РєР°Рє ambient capability,
  register_finalizer РёР· Р»СЋР±РѕРіРѕ РјРѕРґСѓР»СЏ, default_exit_timeout, СЂР°Р·РЅРёС†Р° effect vs
  consume-value, abort/SIGKILL limitations, **nested semantics (D195)**.
- **Q-hot-path-performance** (NEW) вЂ” РєРѕРіРґР° Consumable[Never] elidet'СЃСЏ, РєР°Рє РїРёСЃР°С‚СЊ
  Mutex-style СЂРµСЃСѓСЂСЃС‹ РґР»СЏ zero overhead, disasm examples, benchmarking.
- **Q-structural-extension-future** (STUB) вЂ” future direction `T + Protocol` РґР»СЏ
  general augmentation pattern (`value.with_retry()`, `value.with_logging()`,
  `value.with_metrics()`). Rationale РїРѕС‡РµРјСѓ РЅРµ РІ Plan 110: РґРѕР±Р°РІР»РµРЅРёРµ intersection
  types вЂ” level-0 feature, РѕС‚РґРµР»СЊРЅС‹Р№ РїР»Р°РЅ. Cross-ref РЅР° РїРѕС‚РµРЅС†РёР°Р»СЊРЅС‹Р№ Plan 112.
- **Q-debugging-cleanup-chains** (NEW) вЂ” РєР°Рє С‡РёС‚Р°С‚СЊ `MultiError`, `walk()` iterator, `find_first_panic`, source-locations.
- **Q-perf-considerations** (NEW) вЂ” overhead cancel-shield, exit_timeout, `Consumable[Never]` РґР»СЏ hot-path.

---

## Tests

### Positive

**T1 вЂ” Consumable + consume scope-block (Р¤.1):**
- T1.1 `consume f = File.open(...) { ... }` вЂ” success closes file.
- T1.2 Error РІ body в†’ `on_exit(Failure(_))`.
- T1.3 Implicit consume вЂ” `f` РЅРµРґРѕСЃС‚СѓРїРµРЅ РїРѕСЃР»Рµ `}`.
- T1.4 Nested `consume {}` вЂ” LIFO order.
- T1.5 `consume tx = db.begin() { ... }` вЂ” commit/rollback РїРѕ outcome.
- T1.6 Custom Consumable impl.
- T1.7 Generic `fn use_any[T Consumable[E]](r T)`.
- T1.8 `Consumable[Never]` вЂ” caller Р±РµР· Fail[E].

**T2 вЂ” Codegen + runtime (Р¤.2):**
- T2.1 `on_exit` throws в†’ composes РІ MultiError.
- T2.2 Partial construction: `init` throws в†’ no on_exit.
- T2.3 Exactly-once: `on_exit` РІС‹Р·РІР°Р»СЃСЏ РѕРґРёРЅ СЂР°Р·.
- T2.4 LIFO РґР»СЏ РЅРµСЃРєРѕР»СЊРєРёС… consume.
- T2.5 Mixed `consume {}` + `defer` LIFO.
- T2.6 `resolve_exit_timeout` cached at entry.
- T2.7 Panic РІ body в†’ `on_exit(Panic(msg))`.
- T2.8 Return РёР· С‚РµР»Р° СЃ value вЂ” value РґРѕС…РѕРґРёС‚ РґРѕ caller, on_exit runs.
- T2.9 **Hot-path optimization (D194):** `consume _l = mu.acquire() { body }`
       РєРѕРјРїРёР»РёСЂСѓРµС‚СЃСЏ РІ `body; mu.release()` вЂ” disasm РїСЂРѕРІРµСЂРєР° РѕС‚СЃСѓС‚СЃС‚РІРёСЏ
       shield/timeout РєРѕРґР°.
- T2.10 Init type constraints (D196): `consume tx = db.try_begin()? { body }`
        РєРѕРјРїРёР»РёСЂСѓРµС‚СЃСЏ Рё СЂР°Р±РѕС‚Р°РµС‚ РєРѕСЂСЂРµРєС‚РЅРѕ.
- T2.11 Typed error dispatch: `match outcome { Failure(err) => if err is DbError.Deadlock { ... } }` вЂ”
       narrow'ing СЂР°Р±РѕС‚Р°РµС‚, СЃРїРµС†РёС„РёС‡РµСЃРєРёР№ handler СЃСЂР°Р±Р°С‚С‹РІР°РµС‚.
- T2.12 Cleanup re-entrance (D197): inner `consume {}` РІРЅСѓС‚СЂРё `on_exit` СЂР°Р±РѕС‚Р°РµС‚,
        cancel РѕСЃС‚Р°С‘С‚СЃСЏ pending РґРѕ outer exit.

**T3 вЂ” Cancel-shield + async cleanup + 3-level resolution (Р¤.3):**
- T3.1 Cancel РІРѕ РІСЂРµРјСЏ `on_exit { suspend }` вЂ” cleanup completes.
- T3.2 `exit_timeout` exceeded в†’ `CleanupTimeoutError`.
- T3.3 `Duration.zero` + suspend РІ cleanup в†’ runtime D192 error.
- T3.4 `Duration.MAX` вЂ” compile warning + runs without timeout.
- T3.5 Level-1 (WithExitTimeout impl) РїРѕР±РµР¶РґР°РµС‚: `Transaction` РёРјРµРµС‚
       `exit_timeout() => 30.s()` вЂ” runtime РёСЃРїРѕР»СЊР·СѓРµС‚ 30s.
- T3.6 `#realtime` fn в†’ exit_timeout enforced zero (bypass 3-level resolution).
- T3.7 Cancel-shield cross-platform (Windows fibers + Linux libuv).
- T3.8 Level-2 (Application effect) fallback: `MutexGuard` Р‘Р•Р— WithExitTimeout
       РІРЅСѓС‚СЂРё `with Application = handler(default_exit_timeout: 10.s())` в†’
       runtime РёСЃРїРѕР»СЊР·СѓРµС‚ 10s.
- T3.9 Level-3 (hardcoded 5s) fallback: `MutexGuard` Р±РµР· WithExitTimeout
       Рё Р±РµР· Application effect в†’ runtime РёСЃРїРѕР»СЊР·СѓРµС‚ 5s.
- T3.10 Library pattern: `Db.connect(url, exit_timeout: 60.s())` в†’ tx С‡РµСЂРµР·
        СЌС‚РѕС‚ db РёРјРµРµС‚ `exit_timeout() => 60.s()`, satisfies WithExitTimeout.
- T3.11 **Realtime + Application conflict (D198):** `#realtime` fn РІРЅСѓС‚СЂРё
        `with Application = handler(default_exit_timeout: 10.s()) { ... }` вЂ”
        Application timeout РёРіРЅРѕСЂРёСЂСѓРµС‚СЃСЏ, enforce zero; compile warning
        `D198-realtime-application-override`.
- T3.12 Re-entrance shield stacking: outer shield active РїСЂРё inner consume{};
        cancel pending РґРѕ outer exit. Verified counter test.

**T4 вЂ” Stdlib core (Р¤.4):**
- T4.1 Transaction commit/rollback.
- T4.2 File close.
- T4.3 MutexGuard release (Consumable[Never]).
- T4.4 SemaphorePermit release.
- T4.5 BufReader/Writer flush+close.
- T4.6 CancelScope cancel.

**T5 вЂ” Stdlib extended (Р¤.5):**
- T5.1вЂ“T5.5 РєР°Р¶РґС‹Р№ extended resource.

**T6 вЂ” MultiError + Error types (Р¤.6):**
- T6.1 `walk()` iterator РєРѕСЂСЂРµРєС‚РµРЅ.
- T6.2 `fmt_chain()` РїРѕР»РЅС‹Р№ stack.
- T6.3 `find_first_panic()` РєРѕСЂСЂРµРєС‚РµРЅ.
- T6.4 Cycle-safety: self-suppression РёРіРЅРѕСЂРёСЂСѓРµС‚СЃСЏ.
- T6.5 Depth-limit 256 + truncation.
- T6.6 `CancelError` / `CleanupTimeoutError` РІ prelude.

**T7 вЂ” Cleanup effect + OpenTelemetry (Р¤.7):**
- T7.1 Handler called on enter/exit.
- T7.2 No handler в†’ zero overhead (benchmark verifies).
- T7.3 Handler throw в†’ compile error.
- T7.4 OpenTelemetry-style trace export вЂ” span attributes correct.
- T7.5 Nested spans correctly stacked (parent-child relationship).

**T8 вЂ” Application effect + finalizers (Р¤.8):**
- T8.1 `with Application = handler(...) { ... }` СѓСЃС‚Р°РЅР°РІР»РёРІР°РµС‚ effect; РіР»СѓР±РѕРєРѕ
       РІР»РѕР¶РµРЅРЅР°СЏ С„СѓРЅРєС†РёСЏ РІС‹Р·С‹РІР°РµС‚ `Application.register_finalizer(...)`.
- T8.2 Finalizers fire РїСЂРё РІС‹С…РѕРґРµ РёР· `with`-Р±Р»РѕРєР° РІ reverse-order (LIFO/topo).
- T8.3 `Application.default_exit_timeout()` РґРѕСЃС‚СѓРїРµРЅ РёР· Р»СЋР±РѕР№ С‚РѕС‡РєРё РІРЅСѓС‚СЂРё
       `with`-Р±Р»РѕРєР°.
- T8.4 `exit(code)` fires finalizers (handler.on_exit РІС‹Р·С‹РІР°РµС‚СЃСЏ).
- T8.5 abort/SIGKILL вЂ” finalizers NOT fired (РґРѕРєСѓРјРµРЅС‚РёСЂРѕРІР°РЅРѕ РєР°Рє РѕРіСЂР°РЅРёС‡РµРЅРёРµ).
- T8.6 **Nested Application (D195):** inner `with Application = h2` РёРјРµРµС‚
       СЃРІРѕР№ finalizer registry; РЅРµ РЅР°СЃР»РµРґСѓРµС‚ h1's finalizers; fires h2's first.
- T8.7 **Cross-fiber propagation (D195 В§6):** `spawn { Application.register_finalizer(...) }`
       вЂ” child fiber РІРёРґРёС‚ СЂРѕРґРёС‚РµР»СЊСЃРєРёР№ Application handler.
- T8.8 Default exit_timeout РЅРµ РЅР°СЃР»РµРґСѓРµС‚СЃСЏ: h2 Р±РµР· Р°СЂРіСѓРјРµРЅС‚Р° в†’ 5s hardcoded,
       РќР• h1's value.

**T9 вЂ” Migration (Р¤.9):**
- T9.1 okdefer в†’ warning + auto-fix snippet.
- T9.2 errdefer в†’ warning + auto-fix.
- T9.3 defer |r| в†’ warning + auto-fix.
- T9.4 Auto-migration tool СЂР°Р±РѕС‚Р°РµС‚ РЅР° РїРѕР»РЅРѕР№ test suite.
- T9.5 D90 В§7: cancel/interrupt в†’ Failure(CancelError).

**T10 вЂ” Diagnostic UX + LSP (Р¤.10):**
- T10.1вЂ“T10.6 вЂ” РєР°Р¶РґС‹Р№ РєРѕРґ + LSP integration.

**T11 вЂ” Stress + benchmarks (Р¤.11):**
- T11.1 Racing cancels (1000 fibers, 30s).
- T11.2 Deep nesting (10 levels).
- T11.3 Parallel for + consume {}.
- T11.4 Concurrent on_exit across fibers.
- T11.5 Benchmark targets met.
- T11.6 Memory leak suite (valgrind / AddressSanitizer).
- T11.7 OOM during cleanup вЂ” graceful degradation.
- T11.8 Panic mid-cleanup вЂ” composes correctly.

**T12 вЂ” FFI (Р¤.12):**
- T12.1 C-side SQLite Connection wrapper.
- T12.2 Cancel propagation С‡РµСЂРµР· FFI.
- T12.3 No-leak attestation.

### Negative

- **NEG-1.1** `consume tx = expr { }` РіРґРµ expr РЅРµ Consumable вЂ” D188-not-consumable.
- **NEG-1.2** `tx` РїРѕСЃР»Рµ `}` вЂ” use-after-consume.
- **NEG-1.3** `consume x = expr` Р±РµР· block + Р±РµР· manual consume вЂ” D162-uncovered.
- **NEG-1.4** Consumable impl Р±РµР· `on_exit` РёР»Рё `exit_timeout` вЂ” protocol-violation.
- **NEG-1.5** `on_exit` impl СЃ РЅРµРїСЂР°РІРёР»СЊРЅРѕР№ signature вЂ” D188-malformed-on-exit.
- **NEG-2.1** Double-invocation `on_exit` вЂ” runtime panic exactly-once-violation.
- **NEG-2.2** `exit_timeout()` returns negative вЂ” D192-negative-timeout panic.
- **NEG-3.1** `Duration.zero` + suspend РІ cleanup в†’ D192-zero-timeout-suspend.
- **NEG-3.2** `#realtime` + parking-op РІ cleanup в†’ E_REALTIME_SYNC_PARK.
- **NEG-3.3** `spawn` РІ `on_exit` body в†’ D159-spawn-in-defer.
- **NEG-3.4** `supervised` РІ `on_exit` body в†’ D159 same.
- **NEG-4.1** `Cleanup` handler СЃ `throw` в†’ D185-cleanup-handler-throw.
- **NEG-5.1** Pre-deprecation `okdefer` в†’ D189-deprecated-okdefer warning.
- **NEG-5.2** Pre-deprecation `errdefer` в†’ D189-deprecated-errdefer warning.
- **NEG-5.3** Pre-deprecation `defer |r|` в†’ D189-deprecated-defer-result warning.
- **NEG-5.4** РџРѕСЃР»Рµ removal window: `okdefer` в†’ parse error.
- **NEG-6.1** Match РЅР° `MultiError` СЃ `ErrorKind` (deprecated) в†’ type error.
- **NEG-6.2** `MultiError.suppressed.push(primary)` вЂ” cycle-safety РёРіРЅРѕСЂРёСЂСѓРµС‚СЃСЏ.
- **NEG-7.1** РСЃРїРѕР»СЊР·РѕРІР°РЅРёРµ `using` keyword в†’ parse error + suggest `consume`.
- **NEG-7.2** РСЃРїРѕР»СЊР·РѕРІР°РЅРёРµ `scoped` keyword в†’ parse error + suggest `consume`.
- **NEG-8.1** `module_finalizer { ... }` keyword (deprecated proposal) в†’ parse error
  + suggest `Consumable[Application]` pattern.
- **NEG-9.1** `consume {}` СЃ `init` РєРѕС‚РѕСЂС‹Р№ РЅРµ РІРѕР·РІСЂР°С‰Р°РµС‚ value вЂ” type error.
- **NEG-10.1** Drop-trait syntax (`drop fn ...`) в†’ parse error + СЃСЃС‹Р»РєР° РЅР° D190.
- **NEG-11.1** Cleanup-stack overflow (recursion > 65536) в†’ stack-bound error.
- **NEG-12.1** FFI C-side declaring cancel-safe РЅРѕ С„Р°РєС‚РёС‡РµСЃРєРё Р±Р»РѕРєРёСЂСѓСЋС‰РёР№ вЂ”
  validation-tool error.
- **NEG-13.1** `Consumable[E]` impl СЃ Р»РёС€РЅРёРј РјРµС‚РѕРґРѕРј `exit_timeout` РєРѕС‚РѕСЂС‹Р№ РќР•
  РІРѕР·РІСЂР°С‰Р°РµС‚ Duration вЂ” type error (D188-malformed-with-exit-timeout).
- **NEG-13.2** `Application.register_finalizer(f)` РІРЅРµ `with Application = ...`
  scope вЂ” D188-application-effect-not-handled.
- **NEG-13.3** `Application.handler` СЃ `default_exit_timeout: -1.s()` вЂ” runtime
  D192-negative-timeout panic РїСЂРё РїРµСЂРІРѕРј resolve.
- **NEG-14.1** `consume tx = Some(transaction)` Р±РµР· unwrap вЂ” D196-wrapped-init-needs-unwrap.
- **NEG-14.2** `consume r = if cond { open_a() } else { open_b() }` РіРґРµ a/b
  РІРѕР·РІСЂР°С‰Р°СЋС‚ СЂР°Р·РЅС‹Рµ Consumable С‚РёРїС‹ вЂ” D196-divergent-consumable.
- **NEG-15.1** Cleanup re-entrance СЃ С‚РµРј Р¶Рµ СЂРµСЃСѓСЂСЃРѕРј (linear types violation) вЂ”
  D131 use-after-consume.
- **NEG-16.1** `#realtime` С„СѓРЅРєС†РёСЏ РІС‹Р·С‹РІР°СЋС‰Р°СЏ API РєРѕС‚РѕСЂРѕРµ РІРЅСѓС‚СЂРё СЃС‚Р°С‚РёС‡РµСЃРєРё
  СѓСЃС‚Р°РЅР°РІР»РёРІР°РµС‚ Application timeout non-zero вЂ” D198-realtime-application-override
  warning (heuristic detection).
- **NEG-17.1** `if err is T { }` РіРґРµ `T` РЅРµСЃРѕРІРјРµСЃС‚РёРј СЃ `any` (С‚РµРѕСЂРµС‚РёС‡РµСЃРєРё РЅРµРІРѕР·РјРѕР¶РЅРѕ
  РЅРѕ runtime check РґР»СЏ FFI-injected values) вЂ” type error.
- **NEG-18.1** `Cleanup` effect handler СЃ return type в‰  unit вЂ” D185-cleanup-handler-throw
  (СЂР°СЃС€РёСЂРµРЅРЅР°СЏ РїСЂРѕРІРµСЂРєР°: not just throws, but also non-unit returns are observability
  hazard).

### Regression

- Full `nova test` в‰Ґ 1158/19.
- `plan100_4_*` вЂ” РјРёРіСЂРёСЂРѕРІР°РЅС‹ РЅР° `consume {}` РіРґРµ applicable.
- `concurrency/` вЂ” Р±РµР· regression РІ `supervised_*` С„РёРєСЃС‚СѓСЂР°С….
- Cross-platform: Windows + Linux Г— clang + MSVC.
- Performance: cancel-shield + exit_timeout overhead < baseline + 5%.

---

## Acceptance criteria

| # | РљСЂРёС‚РµСЂРёР№ | Verification |
|---|---|---|
| A1 | `Consumable[E]` + `consume {}` syntax СЂР°Р±РѕС‚Р°СЋС‚ | T1.1вЂ“T1.8 |
| A2 | Codegen desugaring + exactly-once + partial-construction + hot-path opt + re-entrance | T2.1вЂ“T2.12 |
| A3 | Cancel-shield + async cleanup (D191) | T3.1, T3.7 |
| A4 | `exit_timeout` taxonomy + 3-level resolution + realtime conflict (D192/D198) | T3.2вЂ“T3.12 |
| A5 | Stdlib core types РјРёРіСЂРёСЂРѕРІР°РЅС‹ | T4.1вЂ“T4.6 |
| A6 | Stdlib extended types РјРёРіСЂРёСЂРѕРІР°РЅС‹ | T5.1вЂ“T5.5 |
| A7 | MultiError typed + `walk()` + cycle-safety (D193) | T6.1вЂ“T6.6 |
| A8 | `Consumable[Never]` РґР»СЏ infallible (D194) | T1.8, T4.3, T4.4 |
| A9 | `Cleanup` effect observability-only + OpenTelemetry format | T7.1вЂ“T7.5 |
| A10 | Application РєР°Рє effect + finalizers + default_exit_timeout + nesting + spawn propagation (D195) | T8.1вЂ“T8.8 |
| A11 | okdefer/errdefer/defer\|r\| deprecated + auto-fix | T9.1вЂ“T9.5 |
| A12 | D90 В§7 amend (cancel as Failure(CancelError)) | T9.5 |
| A13 | Diagnostic UX + LSP integration | T10.1вЂ“T10.6 |
| A14 | Concurrency stress + benchmarks pass | T11.1вЂ“T11.8 |
| A15 | FFI integration (Plan 100.5 cross-ref) | T12.1вЂ“T12.3 |
| A16 | All NEG diagnostics emit correct codes | NEG-1.1вЂ“NEG-18.1 |
| A17 | Р—Р°РїСЂРµС‰С‘РЅРЅС‹Рµ shortcut'С‹ РѕС‚СЃСѓС‚СЃС‚РІСѓСЋС‚ | code review checklist |
| A18 | Zero regression | T13 (full `nova test` в‰Ґ 1158/19) |
| A19 | Cross-platform PASS | Windows + Linux Г— clang + MSVC |
| A20 | Performance regression < +5% | benchmark suite |
| A21 | Spec D188-D198 written | files exist + cross-ref |
| A22 | Spec amends/retracts: D158/D161/D162/D185/D90 В§7 + retracts D160/D184/D186 | review checklist |
| A23 | Q-blocks (11 С€С‚) written | files exist |
| A24 | `docs/project-creation.txt` updated | sprint section |
| A25 | `docs/simplifications.md` updated | M-markers reclassified |
| A26 | `discussion-log.md` (nova-private) updated | design rationale |
| A27 | Memory `project-plan110-status.md` created | MEMORY.md updated |
| A28 | `nova consume-analyze` CLI tool updated | Plan 100.8 cross-ref |
| A29 | Generic constraint `[T Consumable[E]]` СЂР°Р±РѕС‚Р°РµС‚ + Never special case | Plan 101 cross-ref |
| A30 | Tutorial / examples РѕР±РЅРѕРІР»РµРЅС‹ | РµСЃР»Рё РїСЂРёРјРµРЅРёРјРѕ |
| A31 | Hot-path optimization `Consumable[Never]` + no WithExitTimeout = no overhead | T2.9 disasm verification |
| A32 | Init type constraints (D196): Result/Option unwrap, conditional, method-chain | T2.10, NEG-14.1, NEG-14.2 |
| A33 | Cleanup re-entrance (D197): inner consume{} РІ on_exit СЂР°Р±РѕС‚Р°РµС‚ | T2.12, T3.12 |
| A34 | Realtime + Application interaction (D198): timeout enforced zero | T3.11 |
| A35 | Typed error dispatch С‡РµСЂРµР· `if err is T` (D85 auto-narrowing) вЂ” Р±РµР· РѕС‚РґРµР»СЊРЅРѕРіРѕ helper'Р° | T2.11 |
| A36 | OpenTelemetry wire format РґР»СЏ Cleanup effect | T7.4, T7.5 |
| A37 | `docs/cleanup-cookbook.md` РЅР°РїРёСЃР°РЅ (migration patterns, FFI wrappers, common recipes) | file exists |
| A38 | Nested Application semantics (D195): isolation, cross-fiber propagation | T8.6, T8.7, T8.8 |

---

## Risk / open questions

- **R1** Migration scope: `plan100_4_*` РґРµСЃСЏС‚РєРё С„РёРєСЃС‚СѓСЂ. Auto-tool РґРѕР»Р¶РµРЅ РїРѕРєСЂС‹РІР°С‚СЊ
  в‰Ґ 80%; РёРЅР°С‡Рµ вЂ” sub-plan 111.1 РґР»СЏ Р±РѕР»СЊС€РѕР№ РјРёРіСЂР°С†РёРё.
- **R2** `[T Consumable[E]]` constraint С‚СЂРµР±СѓРµС‚ Plan 101 generic bounds.
  Р•СЃР»Рё bounds РЅРµ РіРѕС‚РѕРІС‹ вЂ” escalation.
- **R3** Plan 83.4.2 (supervised drain ownership) РµС‰С‘ proposed вЂ” `JoinHandle`
  Consumable impl Р·Р°РІРёСЃРёС‚ РѕС‚ РЅРµРіРѕ. Р•СЃР»Рё 83.4.2 РЅРµ РіРѕС‚РѕРІ вЂ” Р¤.5.3 РѕС‚РєР»Р°РґС‹РІР°РµС‚СЃСЏ
  РІ follow-up.
- **R4** Module finalizers С‡РµСЂРµР· РїР°С‚С‚РµСЂРЅ РЅРµ РїРѕРєСЂС‹РІР°СЋС‚ process-abort. Р”РѕРєСѓРјРµРЅС‚РёСЂРѕРІР°РЅРѕ
  РєР°Рє РѕРіСЂР°РЅРёС‡РµРЅРёРµ РІСЃРµС… СЏР·С‹РєРѕРІ, РЅРµ bug. `#[run_on_abort]` follow-up.
- **R5** `consume` keyword ambiguity (raw vs block) вЂ” РїР°СЂСЃРµСЂ lookahead. РўРµСЃС‚С‹ РЅР°
  edge cases (`consume x = expr; { unrelated_block }`).
- **R6** РЈРґР°Р»РµРЅРёРµ `errdefer` РјРЅРѕРіРѕСЃР»РѕРІРЅРµРµ РІ СЂРµРґРєРёС… СЃР»СѓС‡Р°СЏС…. Acceptable for
  simplification.
- **R7** FFI integration (Р¤.12) РјРѕР¶РµС‚ РїРѕС‚СЂРµР±РѕРІР°С‚СЊ sub-plan 111.5 РґР»СЏ РіР»СѓР±РѕРєРѕР№
  СЂР°Р±РѕС‚С‹. Р•СЃР»Рё С‚Р°Рє вЂ” РІС‹РЅРѕСЃРёРј.
- **R8** Cancel-shield runtime overhead вЂ” РєСЂРёС‚РёС‡РЅРѕ РґР»СЏ Plan 103.6 realtime.
  Benchmarks T11.5/T11.6 вЂ” gate.
---

## Р—Р°РІРёСЃРёРјРѕСЃС‚Рё

- вњ… Plan 100.4.1вЂ“5 вЂ” closed; D160 retracted, errdefer СѓРґР°Р»СЏРµС‚СЃСЏ.
- вњ… Plan 100.6 вЂ” cross-module consume; cross-check.
- вњ… Plan 100.8 вЂ” `consume-analyze` updated РІ Р¤.14.
- вњ… Plan 101 вЂ” generic bounds (РґР»СЏ `[T Consumable[E]]`) вЂ” **gate** РґР»СЏ A29.
- вњ… Plan 103.1 вЂ” memory ordering (D188 R6 release-acquire).
- вњ… Plan 103.3, 103.4 вЂ” Mutex/Sem (Р¤.4 migration).
- вњ… Plan 103.6 вЂ” realtime/blocking enforcement (D192 zero-timeout).
- вљ пёЏ **Plan 103.7** (NEW pre-req): rename `#realtime` в†’ `#realtime`
  (~372 РјРµСЃС‚) + СѓРґР°Р»РµРЅРёРµ `realtime { }` / `blocking { }` Р±Р»РѕРє-С„РѕСЂРј +
  РїРµСЂРµС„РѕСЂРјСѓР»РёСЂРѕРІРєР° D172 (В«callee guarantee, РЅРµ block-scopeВ»). Migration
  audit: extract realtime block-content РІ РѕС‚РґРµР»СЊРЅС‹Рµ `#realtime` fns. Р”РѕР»Р¶РµРЅ
  РїСЂРµРґС€РµСЃС‚РІРѕРІР°С‚СЊ Plan 110 implementation.
- вњ… Plan 104.1 вЂ” LSP diagnostics (Р¤.10).
- вњ… Plan 65 вЂ” Channels (Р¤.5.2).
- вњ… Plan 83.12 вЂ” TCP/UDP (Р¤.5.1).
- вљ пёЏ Plan 83.4.2 вЂ” supervised drain (Р¤.5.3 zavisРёС‚); РµСЃР»Рё РЅРµ РіРѕС‚РѕРІ вЂ” defer.
- вљ пёЏ Plan 82 вЂ” Windows fiber arena (Р¤.3.7 cross-platform validation).
- вљ пёЏ Plan 100.5 вЂ” FFI integration (Р¤.12); РјРѕР¶РµС‚ РїРѕС‚СЂРµР±РѕРІР°С‚СЊ sub-plan.

---

## РћС†РµРЅРєР°

~12вЂ“15 dev-day. РџР°СЂР°Р»Р»РµР»СЊРЅС‹Р№ split РїРѕСЃР»Рµ Р¤.0/Р¤.1 (Р¤.2 Р·Р°РІРµСЂС€С‘РЅ РїРѕСЃР»РµРґРѕРІР°С‚РµР»СЊРЅРѕ):

| Agent | Р¤Р°Р·С‹ | РњРѕРґРµР»СЊ |
|---|---|---|
| A | Р¤.3 (cancel-shield + async) | Sonnet 4.6 |
| B | Р¤.4 (stdlib core) | Sonnet 4.6 |
| C | Р¤.5 (stdlib extended) | Sonnet 4.6 |
| D | Р¤.6 (MultiError + Error types) | Sonnet 4.6 |
| E | Р¤.7 + Р¤.8 (Cleanup effect + finalizers) | Sonnet 4.6 |
| F | Р¤.9 (deprecation + auto-fix) | Sonnet 4.6 |
| G | Р¤.10 (Diagnostic UX + LSP) | Sonnet 4.6 |
| H | Р¤.11 (stress + benchmarks) | Sonnet 4.6 |
| Final | Р¤.0 + Р¤.1 + Р¤.2 + Р¤.12 + Р¤.13 + Р¤.14 | Opus 4.7 |

Wall-time СЃ РїР°СЂР°Р»Р»РµР»РёР·РјРѕРј: ~5вЂ“7 РґРЅРµР№.

---

## Р’РѕР·РјРѕР¶РЅС‹Р№ split РЅР° sub-plans (РµСЃР»Рё scope СЂР°СЃС‚С‘С‚)

Р•СЃР»Рё РІ Р¤.0 РІС‹СЏСЃРЅРёС‚СЃСЏ С‡С‚Рѕ РѕР±СЉС‘Рј Р±РѕР»СЊС€Рµ РїСЂРѕРіРЅРѕР·Р° вЂ” РїР»Р°РЅ РјРѕР¶РµС‚ Р±С‹С‚СЊ СЂР°Р·Р±РёС‚ РЅР°:

- **Plan 110.1** вЂ” Core protocol + syntax + codegen (Р¤.0-2)
- **Plan 110.2** вЂ” Cancel-shield + async cleanup + timeout (Р¤.3)
- **Plan 110.3** вЂ” Stdlib migration (Р¤.4-5)
- **Plan 110.4** вЂ” MultiError + Error types + Cleanup effect (Р¤.6-7)
- **Plan 110.5** вЂ” Module finalizers + Migration deprecation (Р¤.8-9)
- **Plan 110.6** вЂ” Diagnostic UX + LSP + Stress/benchmarks (Р¤.10-11)
- **Plan 110.7** вЂ” FFI integration (Р¤.12)
- **Plan 110.8** вЂ” Docs + close (Р¤.13-14)

Р РµС€РµРЅРёРµ вЂ” РїРѕСЃР»Рµ Р¤.0 audit.

---

## РЎС‚Р°С‚СѓСЃ РІС‹РїРѕР»РЅРµРЅРёСЏ

> _Р—Р°РїРѕР»РЅСЏРµС‚СЃСЏ РїРѕ РјРµСЂРµ РїСЂРѕС…РѕР¶РґРµРЅРёСЏ С„Р°Р·. РќР° РјРѕРјРµРЅС‚ СЃРѕР·РґР°РЅРёСЏ РїР»Р°РЅР° вЂ” РІСЃРµ вќЊ._

| Р¤Р°Р·Р° | РЎС‚Р°С‚СѓСЃ | Р”Р°С‚Р° | Commit | Р—Р°РјРµС‚РєРё |
|---|---|---|---|---|
| Р¤.0 GATE | вќЊ | вЂ” | вЂ” | spec drafts + migration audit |
| Р¤.1 Core protocol + syntax | вќЊ | вЂ” | вЂ” | parser + checker |
| Р¤.2 Codegen + runtime fundamentals | вќЊ | вЂ” | вЂ” | desugaring + exactly-once |
| Р¤.3 Cancel-shield + async cleanup | вќЊ | вЂ” | вЂ” | D191 + D192 |
| Р¤.4 Stdlib core | вќЊ | вЂ” | вЂ” | File/Tx/Mutex/Sem/Buf |
| Р¤.5 Stdlib extended | вќЊ | вЂ” | вЂ” | TCP/UDP/Channels/Streams |
| Р¤.6 MultiError + Error types | вќЊ | вЂ” | вЂ” | D193 cycle-safe |
| Р¤.7 Cleanup effect | вќЊ | вЂ” | вЂ” | observability-only |
| Р¤.8 Module finalizers РїР°С‚С‚РµСЂРЅ | вќЊ | вЂ” | вЂ” | Application |
| Р¤.9 Migration deprecation + D90 В§7 | вќЊ | вЂ” | вЂ” | auto-fix tool |
| Р¤.10 Diagnostic UX + LSP | вќЊ | вЂ” | вЂ” | suggestions + quick-fix |
| Р¤.11 Stress + benchmarks | вќЊ | вЂ” | вЂ” | concurrency + perf |
| Р¤.12 FFI integration | вќЊ | вЂ” | вЂ” | Plan 100.5 cross-ref |
| Р¤.13 Regression + cross-platform | вќЊ | вЂ” | вЂ” | nova test в‰Ґ 1158/19 |
| Р¤.14 Docs + spec finalize + close | вќЊ | вЂ” | вЂ” | spec/Q/logs/memory |
| **Final merge** | вќЊ | вЂ” | вЂ” | merge hash РІ main |

**Acceptance status:** A1вЂ“A30 вќЊ (РїР»Р°РЅ СЃРѕР·РґР°РЅ, СЂРµРІРёР·РёСЏ v3).

**Discovered issues** (РїРѕ С…РѕРґСѓ):
- _none yet_

**Follow-up markers СЃРѕР·РґР°РІР°РµРјС‹Рµ СЌС‚РёРј РїР»Р°РЅРѕРј** (РµСЃР»Рё РѕСЃС‚Р°РЅСѓС‚СЃСЏ):
- _none yet_

---

## РЎСЃС‹Р»РєРё

- [Plan 100.4 umbrella](100.4-cleanup-on-failure.md) вЂ” basis (С‡Р°СЃС‚РёС‡РЅРѕ retracted)
- [Plan 100.4.1 failable-cleanup-body](100.4.1-failable-cleanup-body.md)
- [Plan 100.4.2 async-suspend-cleanup](100.4.2-async-suspend-cleanup.md)
- [Plan 100.4.3 okdefer-reason-aware](100.4.3-okdefer-reason-aware.md) вЂ” **retracted**
- [Plan 100.4.4 multi-defer-error-accumulation](100.4.4-multi-defer-error-accumulation.md)
- [Plan 100.4.5 consume-integration](100.4.5-consume-integration.md)
- [Plan 100.5 ffi-external-integration](100.5-ffi-external-integration.md)
- [Plan 100.6 cross-module-integration](100.6-cross-module-integration.md)
- [Plan 100.7 stdlib-migration-playbook](100.7-stdlib-migration-playbook.md)
- [Plan 100.8 performance-ide-tooling](100.8-performance-ide-tooling.md)
- [Plan 101 method-values-and-overload](11-method-values-and-overload.md) (generic bounds)
- [Plan 103.1 memory-ordering-api](103.1-memory-ordering-api.md)
- [Plan 103.3 mutex-family](103.3-mutex-family.md)
- [Plan 103.4 coordination-primitives](103.4-coordination-primitives.md)
- [Plan 103.6 realtime-blocking-integration](103.6-realtime-blocking-integration.md)
- [Plan 104.1 lsp-diagnostics](104.1-lsp-diagnostics.md)
- spec: `spec/decisions/03-syntax.md` D90/D158/D159/D160/D161/D162 (D188-D194 targets)
- spec: `spec/decisions/05-memory.md` D131/D133/D156
- spec: `spec/decisions/06-concurrency.md` (D90 В§7 amend; D75 CancelToken)
- project-philosophy.md вЂ” РѕСЃРЅРѕРІР°РЅРёРµ РґР»СЏ breaking change pre-0.1

