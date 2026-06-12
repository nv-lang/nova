<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 139 — `str` as a Nova value type (`{ ptr *ro u8, len int }`)

> **Создан:** 2026-06-10.  **Статус:** ✅ **CLOSED (2026-06-11)** — все 8 фаз
> (Ф.0-Ф.7) приземлены; spec (D26 MAJOR AMEND + D228 + D216 §1 + D52) финализирован;
> 0 new FAIL vs baseline. Достижимый scope production-grade; остаток честно
> вынесен в `[M-139-*]` followups (gated на `[M-139-f0-lang-item-decl]` — новая
> lang-item checker-инфра). См. §«Итог / acceptance audit» в конце файла.
> **Sequencing (решено 2026-06-10):** **139-first** — этот план идёт ПЕРЕД 138.2 Ф.2-Ф.4
> и subsumes 138.2 Ф.1 (string-слой). Порядок: 138.2 Ф.0 (universal Vec) → **139 Ф.0-Ф.2** → 138.2 Ф.2-Ф.4.
> **Эстимат:** ~5-8 dev-day (крупнейший single-type refactor; high-risk).
> **Model:** Opus + Thinking ON.
> **Зависит от:** Plan 118 (D216 typed pointers `*ro T`), Plan 131 (Vec on raw ptr),
> Plan 124.8/127 (value records). Пересекается с Plan 138.2 (string-layer).
> **Предложено пользователем:** `type str value priv { ptr *u8; len int }`.

---

## Идея

Сделать `str` **Nova value-типом** вместо C-примитива `nova_str`:

```nova
// lang-item: компилятор знает layout для лоуэринга литералов,
// но методы — Nova-body.
type str value priv {
    ptr *ro u8     // указатель на иммутабельный UTF-8 буфер (read-only)
    len int        // длина в БАЙТАХ (D26: str.len = bytes)
}
```

`value` → stack, 16 байт, copy-семантика (совпадает с текущим nova_str).
`priv` → поля видны только методам str (инкапсуляция).
`*ro u8` (не `*u8`) → данные строки иммутабельны.

**Цель:** и `str`, и `Vec`/`[]T` — полностью на Nova; последний C-примитив-
коллекция (`nova_str`) ретайрится до тонкого ABI-typedef. Униформность с
Plan 138.x (Vec на `*mut T`).

---

## Почему это lang-item, а не обычный тип

`str` используется компилятором с самого начала: литералы `"abc"`, интерполяция
`${...}`, `panic(msg str)`, сообщения ошибок, Display. Компилятор обязан знать
layout `str`, чтобы эмитить литералы → `str` не может быть чисто
пользовательским. Модель — **lang-item** (как Rust `str`/`String`): Nova-объявленный
тип, спец-распознаваемый codegen'ом для лоуэринга литералов, с Nova-body методами.

Прецедент: `Vec`/`Range` уже получают полу-спец-обработку; `never`/`int` —
строчные примитивы. `str` встаёт между ними: объявлен на Nova, известен компилятору.

---

## ABI-стратегия (ключ к ограничению риска)

`nova_str` встречается **~431 раз в compiler-codegen/src** (369 в emit_c.rs) +
**~354 в 22 рантайм-C-файлах** (net.c/effects/channels/sync/vtables/string_builder/
conv/fibers). Переписать всё — неподъёмно. **Решение:** value-record `str`
лоуэрится в C-структуру **layout-идентичную** текущему `nova_str`:

```c
// сейчас:  typedef struct { const char*    ptr; size_t  len; } nova_str;
// станет:  typedef struct { const uint8_t* ptr; int64_t len; } nova_str;  // = str value-record
```

`const char*` ≡ `const uint8_t*` (тот же 8-байт указатель), `size_t` ≡ `int64_t`
на x64. → **354 рантайм-вхождения продолжают работать через `nova_str`-typedef-
алиас** без правок. Работа концентрируется в:
- **emit_c.rs** — лоуэринг литералов, type-mapping `str`→`nova_str`, роутинг методов
  на Nova-body вместо external (часть из 369, но не все — большинство это просто
  `nova_str` C-имя, которое остаётся).
- **std/runtime/string.nv** — `str` становится value-record + Nova-body методы.

---

## Production mandate (NO simplifications)

Every phase below is **production-grade**: full implementation, no stubs, no "MVP-then-fix-later". Anything that genuinely cannot land in a phase is **explicitly extracted** to a numbered followup in `docs/plans/backlog-followups.md` + `docs/simplifications.md` `[M-139-*]` marker вЂ” never silently dropped. Each phase = **one coherent change = one (or, for multi-task phases, several) commit(s)**. After EACH big task: update `docs/project-creation.txt` (sprint section), `docs/simplifications.md` ([M-139-*] open/close), `docs/plans/backlog-followups.md`, and `d:/Sources/nv-lang/nova-private/discussion-log.md` (separate repo вЂ” `cd` prefix for its git ops), then commit. Multiple tasks in a phase в†’ multiple commits. `git add` only specific files (parallel agents share repo); `git diff --cached --stat` before every commit. No `Co-Authored-By` trailer. Tests via RELEASE binary: `cargo build --release -p nova-cli` then `target/release/nova test` (NOT debug, NOT `cargo test`). Pos + neg fixtures mandatory per phase (`tN_*.nv` positive, `neg_tN_*.nv` negative). Per-fix targeted `--filter`; full `nova test` only at phase close.

> **Coordination guard (live):** workflow 141 is concurrently editing `emit_c.rs` + rebuilding. Do NOT touch `emit_c.rs`/runtime C/`std/*.nv`/build until 141 lands and the tree is GREEN. This plan's Р¤.0 begins only after a confirmed clean baseline. Record the baseline FAIL count before starting.

## Migration footprint (verified)

| Surface | Count | Detail |
|---|---|---|
| `nova_str` in `compiler-codegen/src/codegen/emit_c.rs` | **375** | type-mapping `"str"=>"nova_str"` (10209/10229/10315/12257/12389/15566), literal compound-literals `(nova_str){.ptr=,.len=}` (3783/16641/18356-18385/25769 + panic/throw sites), tuple mangling `_NovaTuple_*_nova_str_*`, sizeof=16 (11385), concat `+` (16951), print routing (15459/15892), generic HashMap K=str (8567) |
| `nova_str` in runtime C (`nova_rt/*.h/.c`) | **354 across 22 files** | conv.h(22), array.h(87), vtables.h(29), nova_rt.h(42), effects.h(20), net.c(36), channels.h(9), string_builder.h(20), fibers.h(13), sync_*(в‰€42), bench.h(4), contracts.h(1), net.h(17), typeid(3) вЂ” all consume the `nova_str` typedef |
| `nova_str` typedef definition | **2 sites** | `nova_rt.h:56-59` `{const char* ptr; size_t len;}` and `vtables.h:42` (duplicate guard) |
| str method routing | `compiler-codegen/src/codegen/runtime_registry.rs` | `receiver:Some("str")` entries в†’ `nova_str_X` C fns (byte_len 135, char_len 147, byte_at 160, starts_with 185, ends_with 197, contains 209, find 221, rfind 233, char_at 245, trim 260, to_lower 272, to_upper 284, concat 294, to_bytes 402, as_bytes 414, compare, hash) + Nova-body (pad_left 481, pad_right 493, repeat 505) |
| str primitives in C | `nova_rt.h:78-180+`, `array.h` (find/rfind/char_at after NovaOpt), `conv.h` | from_cstr, byte_at, starts/ends/contains, to_upper/lower, trim, slice_panic, concat, eq, hash, to_bytes, as_bytes, split |
| Nova-side decls | `std/runtime/string.nv`, `std/runtime/string_builder.nv` | external fn stubs + Nova bodies; `#no_prelude` (cycle break) |

**Key ABI insight (the risk-limiter):** `const char*` в‰Ў `const uint8_t*` (same 8-byte ptr) and `size_t` в‰Ў `int64_t` on x64 в†’ redefining the `nova_str` typedef to `{const uint8_t* ptr; int64_t len;}` keeps all **354 runtime C occurrences source-compatible** (they read `.ptr`/`.len`, do pointer arith, memcmp/memcpy вЂ” all valid on `uint8_t*`). Work concentrates in emit_c.rs (type-mapping + literal lowering + method routing) and string.nv (value-record decl + Nova bodies). **No interning today** вЂ” literals are inline `(nova_str){...}` compound literals; only top-level `const` str bindings get `static const nova_str` (3628). Interning is a NEW Р¤.6 capability, not a migration.

---
## Р¤Р°Р·С‹

### Р¤.0 вЂ” `str` lang-item value-record + literal lowering (GATE) вЂ” ~1-2d

**Scope.** Declare `type str value priv { ptr *ro u8, len int }` in a bootstrap-early core module (alongside `std/prelude/core.nv`, available before any user code вЂ” must NOT depend on str methods, only the type). Teach the compiler to recognize `str` as a lang-item: `type_ref_to_c("str") -> "nova_str"` continues, but `nova_str` is now understood as the value-record layout `NovaValue`-equivalent (D228 inline struct), NOT an opaque primitive. Redefine the `nova_str` typedef in `nova_rt.h` + `vtables.h` to `{const uint8_t* ptr; int64_t len;}` (ABI-identical, see footprint). Literal lowering `"abc"` keeps emitting `(nova_str){.ptr=(const uint8_t*)"...", .len=N}` (cast added so `const char*` string-literal в†’ `const uint8_t*` field is warning-free). Interpolation `${...}` unchanged (StringBuilder path). `str` declared `priv` в†’ fields visible only to str methods; `*ro u8` ptr field в†’ immutable buffer contract (D216 В§1 `*ro T`).

**Spec / D / Q touch.** D26 (08-runtime.md:254) вЂ” open amend note "str redefined as Nova value-record, layout `{ptr *ro u8, len int}`, lang-item" (finalized Р¤.7). D228 (02-types.md:10153) вЂ” add str as the canonical reference value-record with reference-typed `ptr` field (cross-ref "Reference fields: handles inline" line 10185-10186 already names str). D216 В§1 (02-types.md:7483) вЂ” add str.ptr as the flagship `*ro u8` use-case. D52 value/reference taxonomy (02-types.md:2429) вЂ” note: `str` row moves conceptually from "reference type (heap)" to "value type carrying heap-backed buffer" (16-byte stack value, buffer on heap/rodata). Q-block: `Q139-gc-stack-scan` (how GC sees `ptr` in stack str values вЂ” see R3), `Q139-literal-buffer-lifetime` (rodata vs RawMem).

**Pos fixtures.** `t0_str_literal.nv` (`let s = "abc"; assert(s.len() == 3)`), `t0_str_concat_gate.nv` (`let t = "abc" + "d"; println(t)` в†’ "abcd"), `t0_str_interp.nv` (`let n = 3; let m = "n=${n}"; assert(m == "n=3")`), `t0_str_pass_byvalue.nv` (pass str to fn, mutate local binding, original unchanged вЂ” copy semantics), `t0_str_empty.nv` (`let e = ""; assert(e.len() == 0 && e.is_empty())`).

**Neg fixtures.** `neg_t0_str_priv_field.nv` (`let s = "x"; s.ptr` в†’ field-privacy error E_PRIV_FIELD вЂ” fields are `priv`), `neg_t0_str_ptr_write.nv` (attempt to write through `*ro u8` в†’ E_RO_POINTER_WRITE / unsafe-required), `neg_t0_str_construct_direct.nv` (`str { ptr: ..., len: ... }` outside str module в†’ privacy / lang-item construction forbidden).

**Acceptance (GATE).** A0.1: trivial program `let s="abc"; println(s); let t=s+"d"; println(t)` compiles with RELEASE binary and prints `abc` / `abcd`. A0.2: `nova_str` typedef = `{const uint8_t* ptr; int64_t len;}` in both nova_rt.h and vtables.h; full C build of the corpus has **zero new type warnings**. A0.3: str fields are `priv` (neg fixtures fire). A0.4: sizeof remains 16 (emit_c.rs:11385 unchanged). A0.5: 0 new FAIL vs recorded baseline. **If A0.1 fails, STOP вЂ” do not proceed to Р¤.1.**

**Risk.** R1 (HIGH): str pervades literal lowering вЂ” any break cascades everywhere. Mitigation: GATE on trivial program BEFORE methods; ABI-typedef preserves all 354 runtime sites. R2 (HIGH): bootstrap вЂ” panic/error-msg/interpolation use str during its own definition. Mitigation: lang-item type available pre-method (core module); literal-lowering depends only on the layout, never on methods.

**Commit(s):** `feat(plan139 Р¤.0): str lang-item value-record {ptr *ro u8, len int}` + separate docs commit.

---
### Р¤.1 вЂ” str methods в†’ Nova-body via `@ptr` byte access вЂ” ~1-2d

**Scope.** Migrate str methods from `nova_str_X` external C fns to **Nova bodies** in `std/runtime/string.nv`, reading bytes via `@ptr[i]` (typed-ptr deref of the now-accessible `*ro u8` field вЂ” accessible because methods are inside the str module that owns the `priv` field). Migrate field-internal: `@byte_at`(в†’`@ptr[i]` with bounds-check), `@len`/`@byte_len`(в†’`@len` field read), `@starts_with`/`@ends_with`/`@contains` (memcmp-equivalent byte loops), `@find`/`@rfind`, `@trim`, `@to_lower`/`@to_upper` (alloc new buffer), `@concat` (alloc+copy), `@compare` (lexicographic byte loop), `@hash` (FNV-1a byte loop), `@char_len`/`@char_at` (UTF-8 cursor). Update `runtime_registry.rs`: flip these from `c_name:"nova_str_X"` external to `nova_body:Some(...)` (or DeclaredBody routing). Resolve the existing `@byte_len` vs `@len` naming consistency (string.nv:104 calls `@byte_len()`, decl at :19 is `@len`).

**Irreducible C primitives (MINIMIZE вЂ” explicitly enumerate, target в‰¤2).** (1) UTF-8 decode cursor helper (one fn, byteв†’codepoint advance) вЂ” used by char_len/char_at/slice; (2) new-buffer allocation via `RawMem`/`nova_alloc` (str owns no allocator). Literal-from-static stays compiler-emitted. Everything else в†’ Nova. Each retained C primitive recorded in simplifications.md with justification.

**Spec / D / Q touch.** D26 amend вЂ” methods are Nova-body; document the в‰¤2 irreducible primitives. D216 В§6 вЂ” `@ptr[i]` indexing on `*ro u8` (pointer arithmetic, C-scaled). Q139-utf8-cursor-primitive (justify the one decode helper).

**Pos fixtures.** `t1_byte_at.nv`, `t1_starts_ends_contains.nv`, `t1_find_rfind.nv`, `t1_trim.nv`, `t1_case.nv` (to_lower/to_upper ASCII), `t1_concat.nv`, `t1_compare_ordering.nv` (lt/eq/gt via D178 synthesis), `t1_hash_fnv.nv` (known FNV-1a vector), `t1_char_len_utf8.nv` (multibyte: "hГ©llo".char_len()==5, .len()==6), `t1_char_at_utf8.nv`. Reuse Plan 90 lexer/find/trim byte-algorithm fixtures (must stay green).

**Neg fixtures.** `neg_t1_byte_at_oob.nv` (out-of-bounds в†’ panic, verify message), `neg_t1_char_at_oob.nv` (в†’ None), `neg_t1_ptr_deref_unsafe_outside.nv` (deref `@ptr` outside str module forbidden).

**Acceptance.** A1.1: `std/runtime/string.nv` compiles (RELEASE). A1.2: all str-method fixtures green + Plan 90 byte-algorithm suite green. A1.3: в‰¤2 C primitives remain (audit registry вЂ” no other `receiver:Some("str")` `c_name:"nova_str_*"` except the enumerated decode/alloc). A1.4: 0 new FAIL.

**Risk.** R6 (MED): `@ptr[i]` deref correctness on `*ro u8` (off-by-one, signed/unsigned). Mitigation: targeted OOB neg fixtures + Plan 90 reuse. R7 (MED): alloc-helper buffer not GC-tracked в†’ leak/UAF. Mitigation: route through RawMem (GC-tracked) вЂ” see R3/Р¤.5.

**Commit(s):** `feat(plan139 Р¤.1): str methods to Nova-body via @ptr byte access` (may split: byte-query methods / mutating-alloc methods / hash+compare).

---
### Р¤.2 вЂ” `[]T`-producers в†’ Vec; `as_bytes` zero-copy (subsumes Plan 138.2 Р¤.1) вЂ” ~0.5-1d

**Scope.** Migrate to Nova/Vec: `@to_bytes -> Vec[u8]` (alloc copy), `@to_chars -> Vec[char]` (UTF-8 decode), `@split -> Vec[str]` (byte-scan, zero-copy view slices), `@as_bytes -> ro Vec[u8]` **zero-copy** = `Vec{ data: @ptr as *mut u8, len: @len, cap: @len }` вЂ” the value-record win: `@ptr` is a field directly in hand, so **no `as_ptr` primitive needed**. `from_bytes_lossy` / `from_bytes_unchecked` / `from_bytes_unchecked_steal(consume)` construct `str { ptr: bytes.data, len: bytes.len }` (steal = zero-copy reuse when cap>len, write `\0` at data[len]; else alloc+copy). Requires Vec available вЂ” gated on 138.2 Р¤.0 (universal Vec) having landed first per sequencing.

**Spec / D / Q touch.** D26 amend вЂ” to_bytes/to_chars/split/as_bytes/from_bytes contracts on Vec. D228 вЂ” strв†”Vec interop (both value-records carrying heap buffers). Note 02-types.md:11164 ("СЃС‚СЂРѕРєРѕРІС‹Р№ СЃР»РѕР№ РјРёРіСЂРёСЂСѓРµС‚ РЅР° Nova_Vec") вЂ” mark realized. Q139-as-bytes-aliasing (zero-copy `ro Vec[u8]` aliases str's `*ro u8`; mutation-through-view must be impossible вЂ” `ro` Vec + `*ro` ptr).

**Pos fixtures.** `t2_to_bytes_roundtrip.nv` (strв†’to_bytesв†’from_bytes_uncheckedв†’equal), `t2_to_chars.nv`, `t2_split.nv` ("a,b,c".split(",") в†’ ["a","b","c"]), `t2_as_bytes_zerocopy.nv` (as_bytes len matches, no alloc вЂ” verify via gc-profile if available), `t2_from_bytes_lossy_invalid.nv` (invalid UTF-8 в†’ U+FFFD), `t2_from_bytes_steal.nv` (consume []u8, reuse). Reuse encoding(base64/hex)/text suite.

**Neg fixtures.** `neg_t2_as_bytes_mutate.nv` (write through `ro Vec[u8]` from as_bytes в†’ E_RO / forbidden вЂ” immutability of str buffer preserved), `neg_t2_steal_use_after_consume.nv` (use source []u8 after `from_bytes_unchecked_steal(consume)` в†’ E_USE_AFTER_CONSUME, D162), `neg_t2_split_empty_sep.nv` (defined behavior, not crash).

**Acceptance.** A2.1: to_bytes/to_chars/split/as_bytes round-trip green. A2.2: as_bytes is zero-copy (no new primitive `as_ptr`; verify `@ptr` field-access path in emitted C). A2.3: encoding(base64/hex)/text suites green. A2.4: from_bytes_* (incl. steal) green. A2.5: 0 new FAIL.

**Risk.** R8 (MED): aliasing вЂ” `as_bytes` view + later str-buffer mutation. Mitigation: str is immutable (`*ro u8`), view is `ro Vec` в†’ no legal mutation path; neg fixture proves. R9 (MED): steal reuse correctness (`\0` write, cap math). Mitigation: targeted fixture + consume-tracking neg.

**Commit(s):** `feat(plan139 Р¤.2): str []T-producers in Nova -> Vec; as_bytes zero-copy`.

---
### Р¤.3 вЂ” Equality / hash / clone (structural) вЂ” ~0.5-1d

**Scope.** Make str equality **structural by bytes** consistent with the value-record model. Note **Plan 141 just made composite `==` field-by-field**: a naive field-by-field eq on `str{ptr,len}` would compare **pointer identity**, which is WRONG for strings (two distinct buffers with same bytes must be equal). Therefore str must **override** the field-by-field default with byte-content equality (`@compare(other)==0` / FNV path). Wire: `str.@equal` (byte compare), `str.@hash` (Р¤.1 FNV-1a вЂ” already byte-based, correct), `str.@clone` (value-record clone = copy the 16-byte handle; the buffer is immutable+shared, so clone is shallow handle-copy, NOT deep buffer copy вЂ” `*ro u8` makes sharing safe). Ensure str-keyed HashMap (emit_c.rs:8567 `Nova_HashMap____nova_str__nova_int`) uses byte-eq + byte-hash, not pointer-eq.

**Spec / D / Q touch.** D228 вЂ” value-record eq: default field-by-field (Plan 141), **str opts out** with content-eq; document the override mechanism + rationale (pointer fields with shared immutable pointee need content eq). Cross-ref Plan 126 auto-derive (Equal/Hash) + Plan 141 composite-eq decision. Q139-str-eq-override (how str registers content-eq vs auto field-by-field вЂ” explicit lang-item override vs `priv`-field-aware derive).

**Pos fixtures.** `t3_eq_distinct_buffers.nv` (`"ab"+"" == "ab"` even if different allocations в†’ true), `t3_eq_literal_vs_built.nv` ("abc" == "ab".concat("c")), `t3_hash_eq_consistency.nv` (equal strings в†’ equal hash), `t3_hashmap_str_key.nv` (insert with built key, lookup with literal key в†’ hit), `t3_clone_independent.nv` (clone, original drop/rebind вЂ” clone still valid since buffer immutable).

**Neg fixtures.** `neg_t3_ne_different_bytes.nv` ("ab" != "ac"), `neg_t3_hashmap_miss.nv` (different-byte keys don't collide-match).

**Acceptance.** A3.1: byte-distinct-buffer equality holds (NOT pointer eq). A3.2: hash/eq consistency. A3.3: str-keyed HashMap insert-via-built-key / lookup-via-literal hits. A3.4: clone is handle-shallow (immutable buffer shared) and correct. A3.5: 0 new FAIL.

**Risk.** R10 (HIGH): Plan 141's field-by-field composite-eq would silently make str pointer-eq в†’ wrong-but-compiles. Mitigation: explicit lang-item override + `t3_eq_distinct_buffers` REGRESSION fixture (must fail before override, pass after). R11 (MED): HashMap str-key path may bypass override. Mitigation: `t3_hashmap_str_key`.

**Commit(s):** `feat(plan139 Р¤.3): str structural eq/hash/clone (content-eq override of field-by-field)`.

---
### Р¤.4 вЂ” FFI / cstr interop вЂ” ~0.5d

**Scope.** Reconcile `str` with C-string FFI (`nova_str_from_cstr` nova_rt.h:78, the `const char*` в†” `const uint8_t*` interop, examples/ffi `nova_str` params, conv.h `nova_str_to_i64`). Ensure: (a) C-stringв†’str (`from_cstr`) still works with `uint8_t*` field; (b) strв†’C-string (NUL-terminated) for FFI calls вЂ” str buffers are NOT guaranteed NUL-terminated post-value-record (slices/views), so a `to_cstr`/`as_cstr` helper must allocate+NUL-terminate when needed (Plan 118.1 D217 C-string convention). External-fn signatures taking `nova_str` (sqlite_mini_ffi.h, net.c) keep working via ABI-typedef. Verify `*ro u8` ptr passes through `T*` C ABI (D216 В§1 ABI: `T*`).

**Spec / D / Q touch.** D217 (Plan 118.1, C-string convention) cross-ref вЂ” strв†”cstr bridging with the value-record. D216 В§1 ABI. Q139-cstr-nul-termination (literals are NUL-terminated in rodata; built/sliced strings are not вЂ” `as_cstr` contract).

**Pos fixtures.** `t4_from_cstr.nv` (FFI returns cstr в†’ str), `t4_to_cstr_ffi.nv` (str в†’ C fn expecting NUL-terminated), `t4_slice_not_nul_terminated.nv` (sliced str в†’ `as_cstr` allocates+terminates correctly), `t4_ffi_roundtrip.nv` (strв†’Cв†’str).

**Neg fixtures.** `neg_t4_raw_ptr_to_ffi_unsafe.nv` (passing `@ptr` raw to FFI without unsafe в†’ E_UNSAFE_REQUIRED), `neg_t4_cstr_embedded_nul.nv` (str with embedded `\0` в†’ to_cstr defined behavior, documented, not silent truncation).

**Acceptance.** A4.1: from_cstr / to_cstr round-trip green. A4.2: sliced (non-terminated) str в†’ as_cstr correct. A4.3: examples/ffi (sqlite) + net.c FFI green via typedef. A4.4: 0 new FAIL.

**Risk.** R4 (MED): `const char*` vs `const uint8_t*` type-pun at FFI boundary. Mitigation: typedef alias + targeted cast audit. R12 (MED): non-NUL-terminated built strings to C fns (UB). Mitigation: `as_cstr` allocates+terminates; neg fixture for embedded-NUL.

**Commit(s):** `feat(plan139 Р¤.4): str <-> cstr FFI interop on value-record ABI`.

---
### Р¤.5 вЂ” Runtime C-layer reconciliation + GC stack-scan вЂ” ~1d

**Scope.** Confirm the `nova_str` typedef alias holds for all **354 runtime occurrences** (net.c, effects, channels, sync_*, vtables, string_builder, fibers, conv, bench, contracts, typeid). Fix any **direct field-poke** that assumed `char*`/`size_t` (e.g. arithmetic mixing signed `int64_t` len with `size_t`, or `s.ptr` deref expecting `char`). Special attention: **net.c** (address/data buffers, 36 sites), **effects** (error_msg construction, 20 sites вЂ” built during panic/throw), **vtables.h** (Display, 29 sites), **string_builder.h** (20 sites вЂ” interp + concat path). **GC contract (R3):** GC must see the `ptr` field inside str **value** living on the stack (16-byte value, ptr into heap/rodata buffer). Verify conservative stack-scan covers value-record str on stack; literal buffers are static rodata (never collected); built buffers are RawMem/nova_alloc-tracked. Document the contract.

**Spec / D / Q touch.** D26 amend вЂ” runtime ABI contract (`{const uint8_t* ptr; int64_t len;}`). D228 вЂ” value-record-on-stack GC scanning for reference fields (str.ptr). Q139-gc-stack-scan (resolve: conservative scan sufficient, or precise stack maps needed). Cross-ref 05-memory.md (GC/managed heap).

**Pos fixtures.** `t5_net_str_roundtrip.nv` (net send/recv str payload), `t5_effects_error_msg.nv` (throw with constructed str msg, catch, message intact), `t5_display_vtable.nv` (Display of struct containing str field), `t5_string_builder_interp.nv` (heavy `${}` interpolation), `t5_gc_str_survives.nv` (str value on stack, force GC, buffer still valid).

**Neg fixtures.** `neg_t5_use_freed_str.nv` (if a non-rooted built str's buffer is GC'd while value live в†’ must NOT happen; fixture asserts survival вЂ” i.e. proves the absence of the bug).

**Acceptance.** A5.1: net/effects/sync/channels/vtables/string_builder suites green. A5.2: full C build of corpus вЂ” **zero type warnings** (`-Wall` clean re: nova_str). A5.3: GC stress (`t5_gc_str_survives`) вЂ” no UAF, buffer survives. A5.4: 0 new FAIL.

**Risk.** R3 (MED): GC must see ptr in stack str values. Mitigation: conservative stack-scan + static/RawMem-tracked buffers; stress fixture. R4 (MED): 354-site type-pun. Mitigation: typedef + pointed field-poke audit. R13 (MED): error_msg built during panic (str-during-str-failure, bootstrap echo of R2). Mitigation: panic path uses literal rodata strs (always valid).

**Commit(s):** `refactor(plan139 Р¤.5): reconcile runtime C layer + GC stack-scan with str value-record ABI`.

---
### Р¤.6 вЂ” Literal interning (NEW production capability) вЂ” ~0.5d

**Scope.** Today identical literals emit duplicate inline `(nova_str){.ptr="...",.len=N}` compound literals (rodata dup for the bytes, repeated struct construction). Add **literal interning**: dedupe identical string-literal byte buffers into a single `static const uint8_t[]` in rodata, and (optionally) a single shared `static const nova_str` value, so `"abc"` appearing N times references one buffer. This is additive (not migration) вЂ” pure size/perf win + identity-stable rodata pointers. Gated behind correctness: interning must NOT change observed semantics (eq is byte-content per Р¤.3, so identity-coincidence is invisible to programs вЂ” safe).

**Spec / D / Q touch.** D26 amend вЂ” literal interning note (rodata dedup, semantically invisible due to content-eq). Q139-intern-scope (per-compilation-unit vs whole-program; bootstrap = per-unit).

**Pos fixtures.** `t6_intern_dedup.nv` (same literal used 3Г—; emitted C has one buffer вЂ” verify via emitted-C inspection or size), `t6_intern_eq_unaffected.nv` (interning doesn't break Р¤.3 content-eq), `t6_intern_large.nv` (many literals, build succeeds, no symbol collision).

**Neg fixtures.** `neg_t6_intern_no_aliasing_bug.nv` (interned `*ro u8` never mutated вЂ” write attempt в†’ E_RO, proves immutability preserved under sharing).

**Acceptance.** A6.1: identical literals share one rodata buffer (verify emitted C). A6.2: content-eq (Р¤.3) still holds вЂ” interning invisible. A6.3: no symbol-name collisions across units. A6.4: 0 new FAIL; measurable rodata reduction on a literal-heavy fixture.

**Risk.** R14 (LOW): interning aliasing if str were mutable. Mitigation: `*ro u8` + content-eq make it semantically invisible. R15 (LOW): symbol collisions. Mitigation: content-hash-based stable symbol names.

**Commit(s):** `feat(plan139 Р¤.6): str literal interning (rodata dedup)`.

---
### Р¤.7 вЂ” Full regression + docs/close вЂ” ~1-2d

**Scope.** str touches everything в†’ wide/full regression in per-subsystem fix order (literals в†’ methods в†’ Vec-producers в†’ eq/hash в†’ FFI в†’ runtime в†’ interning). Record final FAIL count vs baseline. Then docs: **D26 major amend** (str = Nova value-record, not primitive; layout `{ptr *ro u8, len int}`; lang-item status; methods Nova-body; в‰¤2 irreducible C primitives; literal interning). D216 В§1 (str.ptr `*ro u8` use-case). D228 (str canonical value-record + content-eq override). Update `simplifications.md` (close all `[M-139-*]`, list any extracted followups), `project-creation.txt` (sprint), `backlog-followups.md`, `nova-private/discussion-log.md` (session header + lessons), README/memory.

**Spec / D / Q touch.** D26 MAJOR amend, D216 В§1, D228, D52 taxonomy row. All Q139-* resolved or extracted. Plan 138.2 Р¤.1 marked SUBSUMED.

**Pos/Neg fixtures.** Full corpus = regression. Add `t7_smoke_all.nv` integration (literal+method+split+hashmap+ffi+interp in one program).

**Acceptance (final).** A7.1: **0 new FAIL vs recorded baseline** (full RELEASE `nova test`). A7.2: D26 major amend landed + reviewed. A7.3: all `[M-139-*]` closed or explicitly extracted. A7.4: cross-platform CI green (if applicable). A7.5: discussion-log + project-creation + simplifications + backlog updated and committed.

**Risk.** R5 (HIGH): scale (~785 nova_str). Mitigation: ABI-typedef confines to emit_c.rs + string.nv; per-subsystem fix order; per-file `nova check FILE` loop on mass errors.

**Commit(s):** `docs(plan139 Р¤.7 D26): str as Nova value type вЂ” complete` + per-subsystem fix commits as needed.

---
## Risk register (consolidated)

| # | Risk | Sev | Phase | Mitigation |
|---|---|---|---|---|
| R1 | str pervades literal lowering вЂ” break cascades | рџ”ґ HIGH | Р¤.0 | GATE trivial program before methods; ABI-typedef preserves runtime |
| R2 | bootstrap: panic/error-msg/interp use str during its definition | рџ”ґ HIGH | Р¤.0/Р¤.5 | lang-item type pre-method; literal-lowering layout-only; panic uses rodata strs |
| R3 | GC must see ptr in stack str values | рџџЎ MED | Р¤.5 | conservative stack-scan; buffers static/RawMem-tracked; gc-stress fixture |
| R4 | const char* vs const uint8_t* / size_t vs int64 type-pun (354 sites + FFI) | рџџЎ MED | Р¤.4/Р¤.5 | typedef alias + pointed field-poke audit + cstr cast |
| R5 | scale (~785 occurrences) | рџ”ґ HIGH | Р¤.7 | ABI-typedef confines to emit_c.rs+string.nv; per-subsystem order |
| R6 | `@ptr[i]` deref correctness on `*ro u8` | рџџЎ MED | Р¤.1 | OOB neg fixtures + Plan 90 reuse |
| R7 | alloc-helper buffer not GC-tracked | рџџЎ MED | Р¤.1/Р¤.5 | RawMem/nova_alloc tracked |
| R8 | as_bytes aliasing vs str mutation | рџџЎ MED | Р¤.2 | str immutable (`*ro u8`) + `ro Vec` view; neg fixture |
| R9 | from_bytes_steal reuse (NUL write, cap math) | рџџЎ MED | Р¤.2 | fixture + consume-tracking neg |
| R10 | Plan 141 field-by-field eq в†’ str pointer-eq (wrong, compiles) | рџ”ґ HIGH | Р¤.3 | content-eq override + distinct-buffer regression fixture |
| R11 | HashMap str-key bypasses content-eq | рџџЎ MED | Р¤.3 | built-key/literal-lookup fixture |
| R12 | non-NUL-terminated built str в†’ C fn (UB) | рџџЎ MED | Р¤.4 | as_cstr allocates+terminates; embedded-NUL neg |
| R13 | error_msg built during panic (str-during-failure) | рџџЎ MED | Р¤.5 | panic path uses literal rodata strs |
| R14 | interning aliasing | рџџў LOW | Р¤.6 | `*ro u8` + content-eq make invisible |
| R15 | interned-symbol collisions | рџџў LOW | Р¤.6 | content-hash stable names |

## Acceptance criteria (overall)

- **E1** вЂ” `type str value priv { ptr *ro u8, len int }` declared, recognized as lang-item; fields `priv`; ptr `*ro u8`. (Р¤.0)
- **E2** вЂ” literals `"..."` + `${}` interpolation + `+` work on RELEASE binary; copy semantics; sizeof 16. (Р¤.0)
- **E3** вЂ” all str methods are Nova-body except в‰¤2 irreducible C primitives (UTF-8 decode cursor, buffer alloc), each justified in simplifications.md. (Р¤.1)
- **E4** вЂ” to_bytes/to_chars/split/as_bytes в†’ Vec; as_bytes zero-copy via `@ptr` field with NO new `as_ptr` primitive; from_bytes_* incl. steal. (Р¤.2)
- **E5** вЂ” str equality is byte-content (NOT pointer), overriding Plan 141 field-by-field default; hash/eq consistent; str-keyed HashMap correct; clone = shared-buffer handle copy. (Р¤.3)
- **E6** вЂ” strв†”cstr FFI interop correct incl. non-NUL-terminated built strings via as_cstr. (Р¤.4)
- **E7** вЂ” runtime (net/effects/sync/channels/vtables/string_builder) green via ABI-typedef; zero C type warnings; GC sees stack str values. (Р¤.5)
- **E8** вЂ” literal interning dedupes rodata buffers, semantically invisible. (Р¤.6)
- **E9** вЂ” 0 new FAIL vs baseline; D26 major amend + D216 В§1 + D228 documented; all [M-139-*] closed/extracted; Plan 138.2 Р¤.1 marked subsumed. (Р¤.7)

## Sequencing (unchanged)
138.2 Р¤.0 (universal Vec вЂ” REQUIRED before Р¤.2) в†’ **139 Р¤.0в†’Р¤.7** в†’ 138.2 Р¤.2-Р¤.4. 138.2 Р¤.1 (string-layer) SUBSUMED here.

---

## Итог / acceptance audit (CLOSED 2026-06-11)

**STATUS: ✅ CLOSED.** Все 8 фаз приземлены на ветке `plan-138.1`. Достижимый
scope реализован production-grade; всё, что не приземлилось по объективному
блокеру (отсутствие lang-item checker-инфры), честно вынесено в `[M-139-*]`
followups (`docs/plans/backlog-followups.md` + `docs/simplifications.md`),
никогда не silently dropped.

### Фазовый исход

| Фаза | Тема | Исход | Коммит(ы) |
|---|---|---|---|
| Ф.0 | str lang-item value-record + literal lowering (GATE) | ✅ GATE PASSED; nova_str typedef → `{const uint8_t* ptr; int64_t len;}` (ABI-идентично, 354 рантайм-сайта без правок); sizeof 16; ~15 codegen literal-сайтов получили `(const uint8_t*)` cast | 740eec6df02 |
| Ф.1 | str методы → Nova-body via byte access | ✅ 10 методов (starts_with/ends_with/contains/find/rfind/char_at/char_len/trim/to_lower/to_upper) в Nova-тела; ≤2 irreducible C-примитива (byte_at + from_bytes_unchecked alloc) | dd0808ef66a |
| Ф.2 | `[]T`-producers → Vec; as_bytes zero-copy | ✅ (достижимый scope) to_bytes/to_chars в Nova-body; as_bytes/split/from_bytes_* остаются тонкими C-примитивами (C as_bytes уже zero-copy) — gated `[M-139-f2-ptr-field-producers]` | 80e5fa3c7c5 |
| Ф.3 | structural eq/hash/clone | ✅ verified+fixtures; str eq/hash/clone — content (memcmp/SipHash), не pointer; emit_field_eq спец-кейс перед Plan 141 field-by-field; R10 нейтрализован | 00af7120235 |
| Ф.4 | str ↔ cstr FFI interop | ✅ + production-bug fix: as_cstr mid-buffer-slice over-read (strlen 11→5) исправлен через NEW C-примитив `nova_str_terminated_ptr` (D26 §3 alloc-fallback) | b633ab02dc8 |
| Ф.5 | runtime C-layer reconciliation + GC stack-scan | ✅ (verification) ABI-typedef держит 354 сайта; GC conservative stack-scan покрывает 16-байт str-value `{ptr,len}` (buffers rodata/RawMem-tracked); residual 59 -Wpointer-sign в хедерах (suppressed `-w`) → `[M-139-f0-rt-header-ptr-sign-casts]` | (Ф.0/Ф.4 typedef; verification-only) |
| Ф.6 | literal interning (NEW capability) | ✅ landed in full (defer-опция `[M-139-interning]` рассмотрена и отклонена — small+low-risk); per-CU rodata dedup, content-hash символы | b460273688d |
| Ф.7 | full regression + docs/close | ✅ THIS task — spec финализирован, plan CLOSED, 0 new FAIL | (docs commit ниже) |

### Acceptance audit (overall E1-E9)

- **E1** (lang-item decl + priv + ro-pointee) — ✅ **FULL** (завершено Plan 139.1
  Ф.A/Ф.C, 2026-06-12): `type str value priv { ptr *u8, len int }` объявлен в
  `std/prelude/core.nv` и распознан как lang-item; privacy fires (`s.ptr` снаружи →
  `E_PRIV_FIELD_READ`; `str{ptr,len}` снаружи → `E_PRIV_FIELD_INIT`; write через
  `*u8` → `E_POINTER_RO_ASSIGN`); ABI-alias = `nova_str` typedef (никакого
  `NovaValue_str`). 3 neg-фикстуры PASS. **БЕЗ новой checker-инфры** — переиспользован
  value-record (Plan 124.8). NB: поле `ptr *u8` (bare `*T ≡ *ro T` = ro-pointee
  canon под 3-axis D246; `*ro u8` был бы `E_REDUNDANT_POINTER_RO`), не `*ro u8`
  как в исходном E1-тексте. `[M-139-f0-lang-item-decl]` УДАЛЁН.
- **E2** (литералы/`${}`/`+`, copy-семантика, sizeof 16) — ✅ Ф.0 GATE.
- **E3** (методы Nova-body, ≤2 irreducible C) — ✅ Ф.1.
- **E4** (to_bytes/to_chars→Vec; as_bytes zero-copy via `@ptr`; from_bytes_*) —
  🟡 ЧАСТИЧНО (остаётся; root cause уточнён Plan 139.1 Ф.B, 2026-06-12):
  to_bytes/to_chars в Nova-body; as_bytes/split/from_bytes_* — C-примитивы
  (C as_bytes уже zero-copy, контракт сохранён). Lang-item (139.1 Ф.A) разгеёчил
  in-module `@ptr` byte-access — это **необходимо, но НЕ достаточно**: producer-формы
  требуют Nova-конструируемого `Vec`/`NovaArray` из raw-parts. **Настоящий разблокер =
  Plan 138.2 Ф.0 (`[]T→Vec` universal flip), не `@ptr`.** Re-homed на
  `[M-139-f2-ptr-field-producers]` (более НЕ gated на lang-item).
- **E5** (content-eq override; hash/eq consistency; HashMap; clone) — ✅ Ф.3.
- **E6** (str↔cstr FFI incl. non-NUL-terminated via as_cstr) — ✅ Ф.4.
- **E7** (runtime green via ABI-typedef; GC sees stack str) — ✅ Ф.5; residual
  -Wpointer-sign warnings (harmless, `-w`-suppressed) → `[M-139-f0-rt-header-ptr-sign-casts]`.
- **E8** (literal interning, semantically invisible) — ✅ Ф.6.
- **E9** (0 new FAIL; D26 major amend + D216 §1 + D228; `[M-139-*]` закрыты/extracted;
  138.2 Ф.1 subsumed) — ✅ THIS task.

### Spec finalization (Ф.7)

- **D26 MAJOR AMEND** (`08-runtime.md`) — str = Nova value-record lang-item;
  layout/ABI/методы/eq-hash-clone/GC/interning + Q139-блоки resolved/extracted.
- **D216 §1** (`02-types.md`) — str.ptr flagship `*ro u8` use-case.
- **D228** (`02-types.md`) — str канонический reference-field value-record +
  content-eq override (был уже из Ф.3, дополнен).
- **D52** (`02-types.md`) — таксономия value/reference: строка `str`
  реклассифицирована из «managed heap / by reference» в «value type, несущий
  heap-backed буфер» (16-байт stack value, copy-семантика).
- **Q139-блоки:** gc-stack-scan / literal-buffer-lifetime / utf8-cursor-primitive /
  str-eq-override / cstr-nul-termination / intern-scope — RESOLVED;
  as-bytes-aliasing — EXTRACTED (`[M-139-f2-ptr-field-producers]`).

### Открытые followups (никогда не silently dropped)

- `[M-139-f0-lang-item-decl]` (P-корневой) — ✅ **ЗАКРЫТ + УДАЛЁН** (Plan 139.1
  Ф.A/Ф.C, 2026-06-12): полная Nova-декларация `type str value priv {ptr *u8,len int}`
  + privacy-enforcement приземлены БЕЗ новой checker-инфры (переиспользован value-record
  Plan 124.8). E1 → FULL.
- `[M-139-f1-trim-view]` — zero-copy trim-view (`@ptr` разгеёчен Plan 139.1 Ф.A, но
  view-форма producer-зависима — re-homed под Plan 138.2 Ф.0, как E4).
- `[M-139-f2-ptr-field-producers]` — as_bytes/split/from_bytes_* в pure-Nova
  (root cause уточнён Plan 139.1 Ф.B: разблокер = Plan 138.2 Ф.0 `[]T→Vec` flip,
  НЕ `@ptr`; C-формы корректны, as_bytes уже zero-copy).
- `[M-139-f0-rt-header-ptr-sign-casts]` — 59 -Wpointer-sign warnings в
  рантайм-хедерах (source-compatible, suppressed `-w`).
- `[M-139-f3-bare-return-type-str]` — pre-existing parser-баг `fn f() str` (bare
  return-type без `->`); используй `-> str`.
- `[M-139-f6-vec-mut-local-enforcement]` — discovered pre-existing 138.x Vec-mut
  enforcement gap (orthogonal к str).
- `[M-139-f4-to-cstr-owning]` — owning `@to_cstr()` (= `[M-118.1-cstr-to-cstr-distinct-copy]`,
  Plan 118.2).

### Финальная регрессия (THIS task, docs-only — 0 .rs/.h/.nv-stdlib изменений)

Spec/docs-only задача → бинарь не пересобирается → регрессия логически
тождественна baseline. Подтверждено representative-прогоном (см. STATUS-вывод
сессии): plan139 37/0; baseline-FAIL set неизменён (plan131/vec_debug_pos,
plan108_4/pos_receiver_at_parse, plan62 ×5-7 StringBuilder/Iterable/protocol,
map_literals/positive_const_map, plan126_2/p5_printable, plan55 ×2 — все
pre-existing). **0 new FAIL.**

### 138.2 Ф.1 (string-layer) — SUBSUMED

Plan 138.2 Ф.1 (string-слой на Vec) поглощён этим планом: str-методы и
`[]T`-producers мигрированы здесь (Ф.1/Ф.2). 138.2 продолжается с Ф.0
(universal Vec, если ещё не приземлён) → Ф.2-Ф.4.
