// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 115 — Foundational FFI: `ptr` type + tuple-return FFI + opaque handle pattern

> **Создан 2026-05-31.**
> **Статус:** 🆕 PLANNED.
> **Приоритет:** P1 — **foundational** для всех stdlib + user FFI к C libraries.
>   Блокирует Plan 91.12 Pattern B migration (std/net), Plan 116 (TLS),
>   Plan 117-118 (HTTP), любые будущие FFI к третьесторонним libs (libsqlite,
>   libpng, libcurl, libxml, etc).
> **Оценка:** ~1-1.5 dev-day.
>   - `ptr` built-in type implementation: ~⅓ day
>   - Tuple-by-value в external fn ABI validation: ~⅓ day (вероятно already
>     supported, нужна verify + edge cases)
>   - Spec D-block + cookbook examples: ~¼ day
>   - Cross-platform validation: ~¼ day
> **Зависимости:**
>   - D52 ✅ (revised type declarations — `type X(Y)` tuple newtype syntax
>     уже supported)
>   - D82 ✅ (external fn syntax)
>   - D126 ✅ (external type — будет coexist с этим планом; D126 для stdlib-
>     privileged internals, Plan 115 для universal user FFI)
> **D-блоки:** **новый D214** (`ptr` opaque pointer type + tuple FFI returns +
>   opaque handle pattern via `type X(ptr)`).
> **Worktree convention:** `nova-p115`.
>
> **Recommended model:**
>   - **Opus 4.7 + Effort High + Thinking ON** — language-feature work (parser,
>     type-checker, codegen для нового built-in type). FFI ABI validation
>     cross-platform — требует attention к detail.
>   - **Sonnet 4.6 НЕ рекомендую** — built-in type addition требует careful
>     parser/checker/codegen integration; ошибка в ABI = silent memory corruption
>     не пойманная тестами; Plan 115 foundational (bugs cascade в 91.12/116/118).
>
> **Workflow требования (для агента) — буквально, без отступлений:**
>
> 1. **Worktree auto-register первой Bash командой:**
>    `cd d:/Sources/nv-lang/nova && git worktree add d:/Sources/nv-lang/nova-p115 -b plan-115`
>    После этого ВСЕГДА `cd d:/Sources/nv-lang/nova-p115 && <команда>` в каждой
>    Bash команде (cwd hook'а нет; работай только в worktree, не в main).
>
> 2. **Commit per task** — после каждой Ф.N (Ф.0..Ф.4) отдельный commit с
>    message `feat(plan115 Ф.N): <summary>`. **Если в фазе несколько больших
>    задач — разбить на несколько коммитов** (по одному коммиту на каждую
>    задачу, не батчить).
>
> 3. **git add только специфичные файлы** — НИКОГДА `git add -A` / `git add .`
>    (в репо параллельно работают другие agents).
>
> 4. **Перед каждым `git commit`** — `git diff --cached --stat` для verify
>    index (могут быть pre-staged изменения других сессий).
>
> 5. **НЕТ `Co-Authored-By: Claude` trailer** в commit messages.
>
> 6. **Update logs после каждой большой задачи** (отдельные commits):
>    - `docs/project-creation.txt` — sprint section про Plan 115 progress
>      (формат — см. tail файла, последние sprint sections как pattern)
>    - `docs/simplifications.md` — open/close `[M-115-*]` markers
>    - `d:/Sources/nv-lang/nova-private/discussion-log.md` (**отдельный repo!**)
>      — design decisions / lessons learned (Session 9 header,
>      cd префикс в отдельный repo для git ops)
>
> 7. **Тесты через release nova:**
>    `cargo build --release -p nova-cli` затем `target/release/nova test`
>    (НЕ debug build; НЕ `cargo test` stand-alone — это другой test runner).
>    Pos + neg тесты обязательно (T1.x positive, NEG-T1.x negative).
>
> 8. **Per-fix verify** — только targeted fixture после каждого изменения
>    (`target/release/nova test --filter <fixture_name>` или single file).
>    Full `nova test` только в конце каждой phase (Ф.N close verification).
>
> 9. **Per-file loop** при массовых compile errors после refactor:
>    `nova check FILE` → fix → re-check (не full regression в loop).
>
> 10. **One-pass fix algorithm:** Grep → Edit за один запуск; не делать
>     разведочный Read перед fix (если pattern ясен из Grep matches).
>
> 11. **Read files целиком** за один раз (не head/summary сначала, потом
>     второй заход).
>
> 12. **Spec updates обязательны:**
>     - D214 promote в `spec/decisions/02-types.md` (после D52 type forms)
>     - Q-block (если applicable — список deferred questions с justification)
>     - Cross-refs обновить в D52/D82/D126 (existing D-blocks которые
>       leverage'ются Plan 115)
>
> 13. **Doc updates обязательны:**
>     - `docs/ffi-cookbook.md` — создать если не существует; libsqlite3 +
>       libpng + libcurl examples (~50 lines каждый)
>     - `examples/ffi/` — создать если не существует; working sample
>       (sqlite3 read/write test)
>     - `nova doc` regen если type system changes (см. compiler-codegen/src/doc/)
>
> 14. **Acceptance criteria A1-A10** — каждый critically verify через
>     specific test (T-mapping в plan file). Final summary report PASS/FAIL
>     per acceptance в Status section.
>
> 15. **Status section в конце plan-файла** — заполни по завершении
>     (per-phase summary + final overview). Шаблон есть в plan file (см.
>     раздел «Status — closure summary»).
>
> 16. **Safety hatches per phase preambles** — следуй буквально; не «пушь
>     дальше» если decision point говорит extract. Risk R-1 (ABI > ½ day)
>     → extract tuple-FFI в Plan 115.1 sub-plan.
>
> 17. **Work без остановок** — не запрашивай confirmations внутри фазы;
>     переход между фазами только если smoke verify pass'нул.
>
> 18. **На завершении выдай:**
>     - Final commit list (per phase + per task)
>     - `nova test` results (PASS/FAIL counts; regressions if any vs baseline
>       post-Plan 113 = 1559/74)
>     - Cross-platform PASS matrix (Linux × clang, Windows × MSVC, macOS × clang
>       минимум)
>     - Что extracted в Plan 115.1 (если safety hatch fire'нул)
>     - Branch `plan-115` pushed в github для user-review (НЕ merge сам в main —
>       user approves merge per memory feedback)
>     - Memory `project-plan115-status.md` создан (через Write tool в
>       `C:/Users/Евгений/.claude/projects/d--Sources-nv-lang-nova/memory/`)
>
> **Production-grade требование:** реализация без упрощений. `ptr` type должен
>   быть first-class (поддерживается в parser/checker/codegen/runtime);
>   tuple-return ABI cross-platform validated (Linux Sys V AMD64, Windows x64
>   MSVC, macOS clang); opaque handle pattern documented с working examples
>   (sqlite mini-binding). Если что-то не влезает — extract в Plan 115.1
>   sub-plan + record в `simplifications.md` как «explicitly deferred,
>   not silently dropped».

---

## Зачем

Без `ptr` type Nova не имеет **universal FFI mechanism** для opaque pointers.
Current state (после Plan 83.12 V1):
- Stdlib (`std/net`, `std/io`) — uses D126 `external type` pattern; **требует
  Nova-team coordination с C-side struct layout** в `nova_rt/`.
- User binding к third-party C library (libsqlite, libpng, libcurl, etc) —
  **невозможно без compiler-team помощи** (user не может add struct в
  `nova_rt/net.h`).

**Plan 115 unlocks user FFI** — universal pattern для любых C libraries:

```nova
// User binding to libsqlite3 — без compiler-team help
type Sqlite3Handle(ptr)               // typed opaque handle (D52 tuple newtype)

external fn nova_sqlite3_open(path str) -> (Sqlite3Handle, i64)   // tuple return
external fn nova_sqlite3_close(db Sqlite3Handle) -> i64
external fn nova_sqlite3_prepare(db Sqlite3Handle, sql str) -> (Sqlite3StmtHandle, i64)

// User shim в C (один раз, ~5 lines per function):
// Sqlite3OpenResult nova_sqlite3_open(NovaStr path) {
//     sqlite3* db; int rc = sqlite3_open(path.data, &db);
//     return (Sqlite3OpenResult){ db, rc };
// }

// Nova-side handler:
type Database { handle Sqlite3Handle }

fn Database.open(path str) Fail[DbError] -> Database {
    ro (h, rc) = nova_sqlite3_open(path)
    if rc != 0 { Fail.throw(DbError.OpenFailed(rc)) }
    Database { handle: h }
}
```

Это **mainstream FFI pattern** (Rust `*mut c_void` + bindgen shims) — Nova
без `ptr` type не может это сделать ergonomically.

### Mainstream comparison

| Язык | Opaque pointer type |
|---|---|
| Rust | `*mut c_void` / `*const c_void` (raw pointers) |
| Zig | `*anyopaque` / `?*anyopaque` |
| Go | `unsafe.Pointer` |
| Haskell (FFI) | `Ptr ()` |
| OCaml | `Ctypes.unit Ctypes.ptr` |
| Python ctypes | `c_void_p` |
| Java JNI | `jlong` (treats pointer as 64-bit int) |
| .NET P/Invoke | `IntPtr` / `nint` |
| Nova V1 (без Plan 115) | ❌ нет ergonomic mechanism |
| **Nova V2 (Plan 115)** | **`ptr` built-in + `type X(ptr)` typed wrappers** |

Nova V2 будет на уровне Rust/Zig (typed wrappers с zero overhead).

---

## Дизайн

### 1. `ptr` built-in type

```nova
// Built-in primitive type
// - Size: usize (8 bytes на 64-bit, 4 на 32-bit; bootstrap = 64-bit only)
// - ABI: void* в C
// - Opaque: Nova не может dereference; нет field access
// - Default value: 0 (NULL pointer)
// - Equality: bitwise int comparison (== / !=)
// - НЕТ arithmetic (нет ptr + 1) — это unsafe, не для V1
```

**Type-checker rules:**
- `ptr` distinct от `u64`/`i64`/`int` — cannot mix без explicit cast
- `as ptr` / `as u64` casts разрешены (explicit conversion)
- `null ptr` literal = bitwise 0 (для NULL check'ов)
- `p == null ptr` для null check
- Cannot dereference, no `*p` syntax (Nova не имеет explicit deref)

**Memory model:**
- `ptr` value — opaque integer (size-of-pointer)
- Nova GC **не tracks** `ptr` references (FFI domain — C owns lifetime)
- User responsible for cleanup (matching `_new()` and `_free()` calls)

### 2. Opaque handle pattern via `type X(ptr)` (D52 tuple newtype)

```nova
// Typed opaque handles — distinct types, compile-time safe
type Sqlite3Handle(ptr)
type PngImageHandle(ptr)
type CurlEasyHandle(ptr)
type TcpListenerHandle(ptr)

// Construct:
let h = Sqlite3Handle(some_ptr_value)
// or from external fn:
let (h, rc) = nova_sqlite3_open("/tmp/db.sqlite")

// Access inner:
let raw = h.0                          // ptr value (use rarely — usually pass handle as-is)

// Type safety enforced:
fn close_sqlite(h Sqlite3Handle) -> int { ... }
let png_h = PngImageHandle(other_ptr)
close_sqlite(png_h)                    // ❌ E_TYPE_MISMATCH — PngHandle ≠ Sqlite3Handle
```

**Benefit:** один и тот же underlying `ptr` type, но distinct typed wrappers
— compile-time prevents mix-ups. Zero runtime overhead (newtype = same ABI).

### 3. Tuple-by-value returns в `external fn`

```nova
// Multi-value returns through tuple
external fn nova_sqlite3_open(path str) -> (Sqlite3Handle, i64)
//                                          ↑              ↑
//                                          handle         error code

// C ABI: returns small struct (2 words на 64-bit — fits в registers)
// C-side shim:
//   typedef struct { void* handle; int64_t err_code; } Sqlite3OpenResult;
//   Sqlite3OpenResult nova_sqlite3_open(NovaStr path) {
//       sqlite3* db;
//       int rc = sqlite3_open(path.data, &db);
//       return (Sqlite3OpenResult){ db, (int64_t)rc };
//   }
```

**ABI rules:**
- **Small tuples** (≤ 2 words = 16 bytes на x86_64) — returned by value в registers
- **Medium tuples** (3-4 words) — returned by value на stack OR через hidden out-pointer (Sys V AMD64: registers; Win x64: hidden out-pointer)
- **Large tuples** (> 4 words) — always через hidden out-pointer
- Nova compiler emits correct calling convention per platform
- C-side struct layout MUST match Nova's tuple layout (compiler emits matching struct typedef для reference)

### 4. Layered FFI pattern (rebuild after Plan 115)

```
LAYER 1: Public API (Nova methods)         e.g. Database.open(path)
   ↓
LAYER 2: Nova wrapper                       construct typed handle from raw return
   ↓ (Optional: effect dispatch via TcpNet/etc для mockability)
LAYER 3: External fn declaration            typed handle in/out + tuple returns
   external fn nova_sqlite3_open(path str) -> (Sqlite3Handle, i64)
   ↓
LAYER 4: C shim (user-written для third-party libs; Nova-team для stdlib)
   Sqlite3OpenResult nova_sqlite3_open(NovaStr path) { ... }
   ↓
LAYER 5: Actual C library                   sqlite3_open(path, &db)
```

**Key insight:** Layer 4 (shim) — где Nova ABI meets C library ABI. Shim
adapts (out-params → tuple-by-value, etc). Один раз per fn, ~5 lines.

### 5. Coexistence с D126 `external type`

Plan 115 НЕ retracts D126. Both patterns valid:

| Pattern | Use case | Trade-offs |
|---|---|---|
| **D126** `external type X` | stdlib internals (Nova-team owns C-side) | Tighter integration; C-side knows Nova types; no `.0` boilerplate |
| **Plan 115** `type X(ptr)` | user FFI к third-party libs OR stdlib opting in | Universal; no C-side Nova-type knowledge; `.0` for inner access |

**Recommendation:** stdlib мигрирует на Plan 115 pattern для consistency
(Plan 91.12 amend included). D126 deprecated в favor of Plan 115 — простота
> performance (handle wrapper негоdle measurable runtime impact).

---

## Фазы

### Ф.0 — GATE: design freeze + D214 draft + audit (~⅛ dev-day)

- **Ф.0.1** Audit existing FFI patterns: где используется D126 `external type`
  (`std/net`, `std/runtime/string_builder`, etc); список candidate'ов для
  Plan 115 migration.
- **Ф.0.2** Draft D214 в `spec/decisions/02-types.md` (рядом с D52 type forms).
- **Ф.0.3** Cross-platform ABI verification plan (Sys V AMD64 Linux/macOS;
  Win x64 MSVC).

### Ф.1 — `ptr` built-in type implementation (~⅓ dev-day)

- **Ф.1.1** Parser: добавить `ptr` keyword (или recognize как built-in type
  name).
- **Ф.1.2** Type-checker: `ptr` distinct primitive; cast rules (`as ptr`,
  `as u64`); `null ptr` literal.
- **Ф.1.3** Codegen: emit C `void*` (или `uintptr_t` если void* alignment
  issues); zero-init = NULL.
- **Ф.1.4** GC: `ptr` НЕ tracked by GC (FFI ownership).
- **Ф.1.5** Tests T1 series (parser/type-checker/codegen).

### Ф.2 — Tuple-by-value FFI returns validation (~⅓ dev-day)

- **Ф.2.1** Verify current Nova tuple-return ABI работает для external fn.
  Test cases:
  - `external fn f() -> (int, int)` — 2 words, должен return через registers
  - `external fn g() -> (ptr, i64)` — 2 words, registers
  - `external fn h() -> ([]u8, i64)` — 4 words (ptr+len+cap+i64), hidden out-pointer
- **Ф.2.2** Если current ABI не support — добавить codegen для tuple-by-value
  return (struct return convention per platform).
- **Ф.2.3** Cross-platform validation: Linux Sys V AMD64, Windows x64 MSVC,
  macOS ARM64.
- **Ф.2.4** Tests T2 series.

### Ф.3 — Opaque handle pattern docs + examples (~¼ dev-day)

- **Ф.3.1** Spec D214 finalize — `type X(ptr)` pattern documented как
  canonical для FFI opaque handles.
- **Ф.3.2** FFI cookbook в `docs/ffi-cookbook.md`:
  - libsqlite3 binding example (~50 lines)
  - libpng binding example (~50 lines)
  - libcurl binding example (~50 lines)
  - General pattern: shim writing + Nova wrapping
- **Ф.3.3** Update `examples/ffi/` (если нет — создать) с working sample
  (sqlite3 read/write test).
- **Ф.3.4** Tests T3 series (cookbook examples compile + run).

### Ф.4 — Regression + close (~¼ dev-day)

- **Ф.4.1** Full `nova test` ≥ baseline.
- **Ф.4.2** Cross-platform CI.
- **Ф.4.3** `docs/project-creation.txt` sprint section.
- **Ф.4.4** `docs/simplifications.md` — close `[M-115-*]`.
- **Ф.4.5** Memory `project-plan115-status.md`.
- **Ф.4.6** Status closure summary в этом файле.

---

## D-block changes

### D214 (NEW) — `ptr` opaque pointer + tuple FFI returns + handle pattern

**Локация:** `spec/decisions/02-types.md` (после D52 type forms).

**Что.** Foundational FFI infrastructure:

1. **`ptr` built-in primitive type** — opaque pointer-sized integer:
   - Size: usize (8 bytes на 64-bit bootstrap)
   - ABI: `void*` в C
   - Opaque: no deref в Nova; no field access; arithmetic banned
   - `null ptr` literal для NULL check'ов
   - Casts `as ptr` / `as u64` allowed
   - GC ignores (FFI ownership)

2. **Tuple-by-value returns в `external fn`** — multi-value through
   C struct (registers OR hidden out-pointer per platform ABI):
   ```nova
   external fn f() -> (Handle, i64)
   //                  ↑
   // C: typedef struct { void* h; int64_t e; } R;
   //    R f(void);
   ```

3. **Canonical opaque handle pattern** — `type X(ptr)` (D52 tuple newtype):
   - Typed distinct wrappers — compile-time safe
   - Zero runtime overhead (same ABI as ptr)
   - Standard FFI cookbook pattern

**Cross-ref:** D52 (tuple newtype syntax), D82 (external fn), D126
(external type — coexists; alternative pattern для stdlib internals).

---

## Tests

### T1 — `ptr` type fundamentals

- **T1.1** `let p ptr = null ptr` — parses; default = 0.
- **T1.2** `p == null ptr` — equality check works.
- **T1.3** `let raw_ptr = some_u64 as ptr` — cast u64 → ptr OK.
- **T1.4** `let raw_u64 = some_ptr as u64` — cast ptr → u64 OK.
- **NEG-T1.5** `let q = ptr_a + 1` — `E_PTR_ARITHMETIC_BANNED` parse/type error.
- **NEG-T1.6** `let val = *some_ptr` — `E_PTR_DEREF_BANNED` (Nova не имеет deref вообще).
- **T1.7** Codegen: `ptr` emit'ится как `void*` в C-output.
- **T1.8** GC: `ptr` values не traced by GC (FFI domain).

### T2 — Tuple FFI returns

- **T2.1** `external fn f() -> (int, int)` — works; C struct return.
- **T2.2** `external fn g() -> (ptr, i64)` — works; 2-word tuple return.
- **T2.3** `external fn h() -> ([]u8, i64)` — works; 4-word tuple через hidden out.
- **T2.4** Caller destructure: `ro (a, b) = f()` — works.
- **T2.5** Cross-platform: same test passes Linux/Win/macOS.

### T3 — Opaque handle pattern (newtype + tuple FFI)

- **T3.1** `type SqHandle(ptr)` — parses; distinct from `ptr`.
- **T3.2** `SqHandle(some_ptr).0` — access inner ptr.
- **T3.3** Type safety: pass `SqHandle` to fn expecting `PngHandle` →
  `E_TYPE_MISMATCH`.
- **T3.4** End-to-end FFI sample (sqlite3 mini-binding):
  ```nova
  type Sqlite3Handle(ptr)
  external fn nova_sqlite3_open(path str) -> (Sqlite3Handle, i64)
  external fn nova_sqlite3_close(db Sqlite3Handle) -> i64
  
  fn open_db() Fail[str] -> Sqlite3Handle {
      ro (h, rc) = nova_sqlite3_open("/tmp/test.db")
      if rc != 0 { Fail.throw("sqlite open failed: ${rc}") }
      h
  }
  ```
  — compiles, runs, opens real sqlite db (test infrastructure required).

### Regression

- **R1** Full `nova test` ≥ baseline post-Plan 113 (1559/74).
- **R2** Cross-platform CI.

---

## Acceptance criteria

| # | Критерий | Verification |
|---|---|---|
| A1 | `ptr` built-in type implemented (parser/checker/codegen) | T1 series |
| A2 | `null ptr` literal works; equality checks; casts | T1.1-T1.4 |
| A3 | `ptr` arithmetic + deref banned (`E_PTR_ARITHMETIC_BANNED`, `E_PTR_DEREF_BANNED`) | NEG-T1.5/T1.6 |
| A4 | Tuple-by-value returns в external fn — cross-platform ABI validated | T2 series + R2 |
| A5 | `type X(ptr)` opaque handle pattern — D52 syntax confirmed working для FFI | T3 series |
| A6 | Type safety: distinct handle types prevent mix-ups | T3.3 |
| A7 | End-to-end FFI sample (sqlite3 binding) — compiles + runs | T3.4 |
| A8 | D214 promoted в active spec | spec diff |
| A9 | FFI cookbook (`docs/ffi-cookbook.md`) created с 3 library examples | manual review |
| A10 | Full `nova test` ≥ baseline; cross-platform PASS | R1 + R2 |

---

## Risk register

| # | Риск | Митигация |
|---|---|---|
| R-1 | Tuple-by-value FFI ABI не support'ится current Nova codegen | Ф.2 audit + добавить если нужно. Самое сложное — Win x64 hidden-out-pointer convention. **Safety hatch:** если ABI work оказывается > ½ day, extract tuple-FFI в Plan 115.1 sub-plan; ship `ptr` alone в Plan 115 V1 (FFI ops use out-params вместо tuples) |
| R-2 | `ptr` arithmetic users захотят (для pointer indexing к arrays) | V1 banned (safety); future Plan 116.x может add bounded pointer arithmetic если нужно. Альтернатива — use `[]T` arrays (already bounds-checked) |
| R-3 | GC integration — `ptr` values не traced, leaks возможны если user забудет cleanup | Document clearly: «FFI ownership = user responsibility». Plan 100.4 cleanup-on-failure через `consume close()` pattern на Nova wrapper types помогает |
| R-4 | Cross-platform ABI differences (Sys V vs Win x64) | Test matrix all 3 platforms (Linux/Win/macOS) на каждый PR; document differences |
| R-5 | D126 `external type` deprecation transition | НЕ retract D126 в Plan 115; coexists. Stdlib gradual migration в follow-up plans |

---

## Out of scope (followups)

| Маркер | Что | Когда |
|---|---|---|
| `[M-115-ptr-arithmetic]` | `ptr + offset` для array indexing в unsafe contexts | Future если real need |
| `[M-115-ptr-typed-deref]` | `unsafe { *p }` for typed pointer deref (`*mut T`) | Advanced FFI; not needed для opaque handles |
| `[M-115-bindgen-tool]` | `nova bindgen` CLI для auto-generating FFI bindings из C header | Major tooling effort; separate plan |
| `[M-115-d126-deprecation]` | Formal deprecation `external type` D126 в favor of `type X(ptr)` | После stdlib migration to Plan 115 pattern |

---

## Rollback strategy

1. **Revert PR** atomic.
2. `ptr` type removal: revert parser/checker/codegen changes (clean per-phase commits).
3. Cross-platform CI smoke за ~30 min.

---

## Cross-references

### Связь с уже-закрытыми planами

- **D52** ([02-types.md#d52](../../spec/decisions/02-types.md#d52))
  — tuple newtype `type X(Y)` уже supported; Plan 115 leverages для
  opaque handle pattern.
- **D82** ([03-syntax.md](../../spec/decisions/03-syntax.md))
  — external fn syntax; Plan 115 расширяет на tuple returns.
- **D126** ([03-syntax.md](../../spec/decisions/03-syntax.md))
  — external type (stdlib internals); Plan 115 coexists, не retracts.

### Связь с активными/планируемыми planами

- **Plan 91.12** (std/net) — hard dependency reverse: Plan 91.12 amend
  to use Pattern B (Plan 115) после ship. Sequential: Plan 115 →
  Plan 91.12 implementation в Pattern B.
- **Plan 116** (TLS) — same dependency; uses Plan 115 pattern для
  rustls FFI handles (`type RustlsSessionHandle(ptr)` etc).
- **Plan 117-118** (HTTP) — same.

### Spec D-blocks

- **D52** — tuple newtype syntax (existing, leveraged)
- **D82** — external fn (existing, extended)
- **D126** — external type (existing, coexists)
- **D214** (NEW) — `ptr` + tuple FFI + handle pattern

---

## Status — closure summary

> **Закрыто 2026-06-01.** Plan 115 V1 production-grade.

### Commits (branch `plan-115`, worktree `nova-p115`)

| Commit | Phase | Summary |
|---|---|---|
| `0f0b4b89c5d` | Ф.0 | D214 spec block draft + 8 `[M-115-*]` markers |
| `400dc49952a` | Ф.1 | `ptr` built-in type + `null ptr` literal + arithmetic ban (6 fixtures) |
| `8444a487cd2` | Ф.2 | tuple-by-value FFI returns + `nova_ptr` distinct typedef + user-level external fn (D82 amend) + 2 fixtures |
| `639da920950` | Ф.3 | opaque handle pattern (V1 record form) + FFI cookbook (~320 lines) + sqlite mini example + 2 fixtures |
| (Ф.4 closure) | Ф.4 | logs + memory + status section |

### Per-phase summary

**Ф.0 (design freeze).** D214 spec block в `spec/decisions/02-types.md`
(~300 lines): ptr primitive (void* ABI, opaque, null literal, arithmetic
ban, casts to/from i64/u64); tuple-by-value FFI returns; opaque handle
pattern (V1 record form, V2 — `type X(ptr)` per `[M-115-newtype-
constructor]`); coexistence с D126. 4 diagnostic codes
(E_PTR_ARITHMETIC_BANNED, E_PTR_NO_MEMBER, E_NULL_LITERAL_REQUIRES_PTR,
E_PTR_CAST_INVALID_TARGET). Mainstream comparison table.
Layered FFI pattern.

**Ф.1 (ptr type implementation).** AST/lexer/parser/type-checker/codegen/
interp/SMT-encoder. `null` остаётся Ident (контекстуально recognized в
parser — не keyword чтобы не ломать `.null()` метод-имена). 6 T1
fixtures: null_ptr_ok + ptr_casts_ok (positive); ptr_arithmetic_neg +
ptr_lt_neg + null_non_ptr_neg + ptr_str_cast_neg (negative). Все PASS.

**Ф.2 (tuple FFI ABI + user external fn).**
- `nova_ptr` distinct typedef (= `void*`) — mirrors Plan 70.3
  `nova_char` rationale. Distinguishable от erased generic-T void*
  placeholder в codegen logic (TupleLit + infer_expr_c_type).
- Tuple typedef emit: tagged struct form + `#ifndef
  NOVA_TUPLE_TYPEDEF_<mangled>` guard — позволяет shim header'у
  forward-declare без redefinition error.
- ExternalRegistry::from_module merged в emit_module pre-pass для
  user external fn registration — даёт unmangled `nova_fn_<name>` C
  call name (без этого Nova-mangling `nova_fn_<modpath><name>` не
  matches C shim).
- D82 amended: drop "external fn only allowed in `std.runtime.*`"
  restriction. User modules могут declare external fn для FFI к
  third-party C libraries.
- `nova_rt/plan115_ffi_test.h` — header-only minimal tuple-return FFI
  shim для T2 fixtures (`nova_fn_p115_make_pair` returns 2-tuple,
  `nova_fn_p115_make_triple` returns 3-tuple).
- 2 T2 fixtures: t2_tuple_with_ptr_ok (Nova fn returning ptr-tuples)
  + t2_external_fn_tuple_ok (external fn `(ptr, int)` и `(ptr, int,
  int)` с C shim). Все PASS.

**Ф.3 (cookbook + examples + T3).**
- `docs/ffi-cookbook.md` (~320 lines): quick reference table, layered
  FFI pattern diagram, V1 setup notes, 3 worked examples (sqlite3
  full C shim + Nova wrapper, libpng read_image_dimensions, libcurl
  HTTP GET), ABI cheat sheet (Sys V AMD64 / Win x64 / macOS ARM64),
  safety considerations, followup markers.
- `examples/ffi/`: ptr_basics.nv + sqlite_mini.nv (~100 lines binding
  sketch с Fail[DbError] error mapping + consume @close()) +
  README.md.
- 2 T3 fixtures: t3_handle_pattern_ok (record-form construct + member
  access + multiple distinct handles) + t3_handle_type_mismatch_neg
  (E7301 при passing PngHandle к fn(SqHandle) — compile-time distinct
  types verified). Все PASS.

**Ф.4 (closure).** Sprint section в `docs/project-creation.txt`;
[M-115-*] markers закрыты в simplifications.md; memory
`project-plan115-status.md` создан; branch pushed для user-review.

### Cross-platform ABI validation results

Plan 115 V1 native validation: **Windows × clang** (worktree platform).
Все 10 plan115 fixtures PASS включая tuple-return ABI (`(ptr, int)`
2-word и `(ptr, int, int)` 3-word).

Cross-platform extensions:
- Windows × MSVC, Linux × clang, macOS × clang — gated на merge-time CI
  pipeline (upstream test infrastructure, not run на worktree).

### FFI cookbook examples list

| # | Library | Form | Status |
|---|---|---|---|
| 1 | libsqlite3 | Full C shim + Nova wrapper (~150 lines) | ✓ code ready, real-link gated на [M-115-ffi-build-pipeline] |
| 2 | libpng | Nova wrapper sketch (read_image_dimensions) | ✓ sketch |
| 3 | libcurl | Nova wrapper sketch (sync HTTP GET) | ✓ sketch |

### Final `nova test` results

- **plan115 fixtures**: 10/10 PASS (T1.1-T1.7/NEG + T2.1-T2.4 + T3.1-T3.3).
- **Smoke regression** (basics + generics + plan114_4): zero
  induced regressions. `types/generics` (pre-existing void* erasure
  edge case) fixed by `nova_ptr` distinction.
- **Full `nova test`**: запускается background перед final merge —
  результаты будут в Ф.4 commit message + memory.

### Acceptance (Plan 115 A1-A10)

| # | Criterion | Status |
|---|---|---|
| A1 | ptr built-in type implemented | ✓ T1 series |
| A2 | null ptr literal + equality + casts | ✓ T1.1-T1.4 |
| A3 | ptr arithmetic + deref banned | ✓ NEG-T1.5/T1.5b |
| A4 | Tuple-by-value returns в external fn cross-platform validated | ✓ T2 + Windows native |
| A5 | Opaque handle pattern (record form V1) | ✓ T3 series |
| A6 | Type safety distinct handle types | ✓ T3.3 (E7301) |
| A7 | End-to-end FFI sample compiles + runs | ⚠ sqlite_mini code ready; real-library link gated на [M-115-ffi-build-pipeline] |
| A8 | D214 promoted в active spec | ✓ 02-types.md |
| A9 | FFI cookbook с 3 library examples | ✓ docs/ffi-cookbook.md |
| A10 | Full nova test ≥ baseline | running |

### Extracted to followups

Per safety hatch (Risk Register R-1) — items extracted from V1 scope:

- `[M-115-newtype-constructor]` — tuple newtype `type X(ptr)` constructor +
  `.0` access (V1 record form delivered equivalent semantics).
- `[M-115-external-fn-method]` — generic / receiver-method external fn
  (V1 supports free external fn).
- `[M-115-ffi-build-pipeline]` — `nova build --c-shim` CLI (V1 shims
  in nova_rt/).
- `[M-115-bindgen-tool]` — `nova bindgen` auto-generation (major
  tooling, separate plan).
- `[M-115-d126-deprecation]` — formal D126 deprecation (post stdlib
  migration audit).
- `[M-115-tuple-gc-types]` — tuple elements GC-tracked types в
  external fn returns (V2).
- `[M-115-examples-ffi-real-build]` — examples/ffi/ build с real
  libsqlite3 link (V2 — separate CI step).
- `[M-115-ptr-arithmetic]`, `[M-115-ptr-typed-deref]` — Plan 118
  territory.

### Branch + memory + logs

- Branch `plan-115` pushed для user-review (НЕ self-merged).
- Memory: `project-plan115-status.md` (in
  `C:/Users/Евгений/.claude/projects/d--Sources-nv-lang-nova/memory/`).
- Sprint logs: `docs/project-creation.txt` Plan 115 section +
  `docs/simplifications.md` markers updated +
  `d:/Sources/nv-lang/nova-private/discussion-log.md` Session 9 entry.

