// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 67: `println`/`print` — overload resolution через return-type inference

> **Создан:** 2026-05-18. **Статус:** proposed, не начат. **Приоритет:** P0
> (hotfix: CC-FAIL bench/corpus, замаскирован Plan 65 hotfix
> contract-encoder fix).
> **Трудоёмкость:** ~2 dev-days (focused codegen fix + audit + tests).

---

## Зачем

`println(str.from(factorial(5)))` генерирует **invalid C**:

```c
// Generated (BUG):
nova_print_int(nova_int_to_str(nova_fn_factorial(((nova_int)5LL))));
// ^^^^^^^^^^^^ expects nova_int    ^^^^^^^^^^^^^^^^^^^^ returns nova_str
//                                                      ⇒ CC-FAIL

// Expected:
nova_print_str(nova_int_to_str(nova_fn_factorial(((nova_int)5LL))));
```

### Причина (root cause)

`infer_print_helper` в [emit_c.rs:13625-13701](../../compiler-codegen/src/codegen/emit_c.rs#L13625)
обрабатывает только узкий набор паттернов:

| Pattern | Handled? | Resolve через |
|---|---|---|
| `println(42)` | ✅ | IntLit |
| `println(true)` | ✅ | BoolLit |
| `println("hi")` | ✅ | StrLit |
| `println(s)` где `s: str` | ✅ | var_types lookup на Ident |
| `println(rec.field)` | ✅ | record_schemas lookup на Member |
| `println(s.to_upper())` где `s: str` | ✅ | hardcoded string-method list |
| `println(name(args))` | ✅ | `fn_ret_<name>` cached return type на Ident-func |
| **`println(str.from(x))`** | ❌ | **falls to default `nova_print_int`** |
| **`println(Type.static_method(...))`** | ❌ | **falls to default `nova_print_int`** |
| **`println(obj.method().chain())`** | ❌ partial | first-level не int — silent wrong |
| **`println(if cond { a } else { b })`** | ❌ | if-expr → default int |
| **`println(match x { ... })`** | ❌ | match-expr → default int |

Static method call `str.from(x)` — это `Call { func: Member { obj: Ident("str"), name: "from" }, args: [x] }`. В существующем коде:

1. Member-handling в Call expects `obj` to be a **value** of `nova_str`
   (для `s.to_upper()`). Для `str` это **тип**, `infer_expr_c_type_str(Ident("str"))` не вернёт "nova_str".
2. Ident-handling expects `func.kind = Ident(name)` (для `name(args)`).
   Member не Ident.
3. Falls to default `"nova_print_int"`.

### Real impact

- **25 affected sites** только в `bench/corpus/*.nv` (`println(str.from(...))` pattern).
- Замаскирован в CI до 2026-05-18 потому что `bench/corpus/06_contracts.nv`
  падал раньше на CODEGEN-FAIL (contract verification — Plan 65 fix).
- После Plan 65 hotfix контрактов — CC-FAIL стал visible.
- Аналогичные паттерны вероятно есть в std/, examples/, nova_tests/ —
  audit нужен.
- **Все if/match-expression args тоже broken** (silent wrong output вместо
  CC-FAIL, ещё опаснее — для bool/str/float args получаешь `%lld` cast).

### Почему это hotfix-приоритет

1. **Замаскированный production bug** — silent wrong output для
   if/match-expression printing (no CC-FAIL, тихий некорректный вывод).
2. **Блокирует bench/corpus enable в CI** — `corpus_*` тесты не могут
   запускаться пока не починим (Plan 57 bench-history blocker).
3. **Учит cargo-cult'у** — workaround pattern «assign to var first»
   уже встречается:
   ```nova
   let s = str.from(factorial(5))  // ← workaround
   println(s)                       // works (Ident path resolves)
   ```
4. **Архитектурный ред флаг** — `infer_print_helper` — это manual type
   inference, дублирующий `infer_expr_c_type`. Любое расширение
   (новый built-in, новый convert-fn, новый stdlib API) требует двойной
   правки.

---

## Архитектурное решение

### AD1. Унификация: `infer_print_helper` использует `infer_expr_c_type`

Удалить manual pattern matching в `infer_print_helper`. Заменить на:

```rust
fn infer_print_helper(&self, expr: &Expr) -> &'static str {
    let c_ty = self.infer_expr_c_type(expr);  // ← reuse existing inference
    match c_ty.as_str() {
        "nova_str"          => "nova_print_str",
        "nova_bool"         => "nova_print_bool",
        "nova_f32" | "nova_f64" => "nova_print_f64",
        // ints (signed/unsigned, all widths) → нормально cast'нутся в long long
        "nova_int" | "nova_i8" | "nova_i16" | "nova_i32" | "nova_i64"
        | "nova_u8" | "nova_u16" | "nova_u32" | "nova_u64"
                            => "nova_print_int",
        "nova_char"         => "nova_print_char",
        _                   => "nova_print_int",  // conservative default
    }
}
```

**Почему это правильно:**

- `infer_expr_c_type` уже handle'ит **все** expression shapes
  (IntLit, FloatLit, Ident-var, Member-field, Call-member, Call-ident,
  static methods including `str.from`/`Channel.new`/`Time.after`/etc.,
  if-expr through both branches, match-expr through arm-merging).
- DRY: один источник истины для return-type. Bug-fixes в
  `infer_expr_c_type` автоматически покрывают `println`.
- Production stdlib API (Channel.new, ChanReader.close_after из
  Plan 65, Vec.new, HashMap.from, etc.) — все попадают «бесплатно».

### AD2. Negative cases handling

- **Unknown return type** (function без registered signature, generic
  не-mono) → fallback `nova_print_int` (current behavior preserved).
- **Generic function returning T** где T не resolved — emit warning W6701
  «cannot infer print helper; defaulting to int — wrap argument or
  add type annotation». Не error (preserve loose compilation), но
  visible.
- **Custom types (records, sum-types)** → `nova_print_int` сейчас
  даёт garbage. **Будущее (Plan 67+1)**: auto-dispatch на
  `@to_str()` метод если есть; иначе W6702 «no Display/to_str impl».
  Out of scope для P0 hotfix.

### AD3. Char support — параллельный fix

В существующем коде нет `nova_print_char`. `println('a')` сейчас
эмитит `nova_print_int('a')` — печатает code-point как int (97), не
символ. Это **отдельный bug в той же функции**; чинить заодно.

- Add runtime extern `nova_print_char(nova_char c)` printing UTF-8 byte
  sequence.
- Add match arm в `infer_print_helper`.
- Negative test: `println('a')` → expect output "a\n", не "97\n".

### AD4. `print` / `eprintln` / `eprint` — equivalent fixes

Same bug в [emit_c.rs:11026](../../compiler-codegen/src/codegen/emit_c.rs#L11026)
если они тоже маршрутизируются через `emit_println(..., newline)`.
Confirm в Ф.0 audit; если да — фикс single-site (это уже unified
helper, ничего дополнительно делать не надо).

### AD5. `bench/corpus/` unblocking

После фикса — verify все 25 affected sites compile + run корректно.
Запустить bench-history baseline для corpus_* (Plan 57) — это разблокирует
broader corpus testing infrastructure.

---

## Requirements

### Core fix

**R1.** `println(<expr>)` корректно резолвится для:
   - Static method calls: `str.from(int)`, `str.from(bool)`,
     `Channel.new(0).rx` (member-of-call), etc.
   - Method chains: `xs.first().to_str()`, `Some(42).unwrap_or(0)`.
   - if/match expressions: `println(if x > 0 { "pos" } else { "neg" })`.
   - Type-annotated locals: `let s str = f(); println(s)` (existing
     works, regression-guard).

**R2.** Backward-compat: все existing passing `println` сценарии
продолжают работать (regression-test baseline).

**R3.** Char support: `println('a')` → "a\n" вывод, не "97\n".

### Diagnostics

**R4.** Unknown-type fallback → silent `nova_print_int` (current
behavior); добавить **opt-in** lint W6701 (через
`#warn(print_unknown_type)` attr или CLI flag) для surfacing.
**Не** default warning (бы broke текущий код).

### Tests

**R5.** `nova_tests/plan67/`:
   - `f1_static_method_str_from.nv` — positive: `println(str.from(42))`
     → "42\n"
   - `f2_static_method_str_from_bool.nv` — positive:
     `println(str.from(true))` → "true\n"
   - `f3_method_chain.nv` — positive: `println(xs.first().unwrap_or(0))`
   - `f4_if_expr_str.nv` — positive:
     `println(if x > 0 { "pos" } else { "neg" })` → "pos\n"
   - `f5_match_expr_int.nv` — positive: match returning int
   - `f6_char_literal.nv` — positive: `println('a')` → "a\n"
   - `f7_char_var.nv` — positive: `let c char = 'b'; println(c)` → "b\n"
   - `f8_nested_str_from.nv` — positive:
     `println(str.from(int.parse("5").unwrap_or(0)))` (Plan 65 +
     Plan 67 интеграция)
   - `f9_record_field_str.nv` — positive: `println(rec.name)` где
     `name: str` (regression-guard для existing)
   - `f10_unknown_type_fallback.nv` — generic-mono returning unknown →
     compile (с W6701 если flag), runtime fallback к int print (current
     behavior)

### Audit

**R6.** `grep -rn "println\|print\|eprintln\|eprint"` в std/, nova_tests/,
examples/, bench/corpus/. Категоризация:
   - **Affected** (str.from + other static-methods): inventory list
   - **At risk** (if/match args): inventory list
   - **Workaround** (uses `let s = ...; println(s)`): suggest rewrite
     (cleanup pass, не часть P0)

### bench/corpus unblock

**R7.** Все 25 affected sites в `bench/corpus/*.nv` compile + produce
correct output после fix. Spot-check 5 файлов end-to-end run.

**R8.** `nova bench corpus run --quick` (Plan 57.C.8 corpus subcommand)
PASS на всех corpus файлах.

### Cross-toolchain

**R9.** Clang / MSVC / GCC build + test PASS (Plan 58 matrix).

### Documentation

**R10.** `///` doc-comment на `println` (если exists в prelude
declaration) — add `# Examples` block с str.from pattern.

---

## Phases

### Ф.0 — Audit baseline (½ day)

- [ ] `nova test` baseline на main — fix exact PASS/FAIL.
- [ ] Reproduce CC-FAIL на `bench/corpus/06_contracts.nv`
      (после Plan 65 contract fix).
- [ ] Inventory: `grep -rn "println\|^print"` категоризовать по AD2
      table (affected / at-risk / workaround).
- [ ] Verify `emit_println` обрабатывает `print` / `eprintln` /
      `eprint` same code-path (AD4).
- [ ] Verify `infer_expr_c_type` correctness на:
   - `str.from(42)` — должен вернуть `nova_str`
   - `Channel.new(0)` — должен вернуть `Nova_ChannelPair` (existing)
   - `if cond { "a" } else { "b" }` — должен вернуть `nova_str`
   - `match x { 1 => "a", _ => "b" }` — должен вернуть `nova_str`
- [ ] Записать baseline в `docs/plans/67-artifacts/baseline-2026-05-XX.md`.

**Acceptance:** baseline.md с counts; if `infer_expr_c_type` gaps
найдены — добавить отдельные фазы.

### Ф.1 — Core fix `infer_print_helper` (½ day)

- [ ] Replace manual pattern-matching на `infer_expr_c_type`-based
      dispatch (AD1 code).
- [ ] Preserve existing fast-paths для literal types (IntLit/StrLit etc.)
      если измеряемый overhead заметный (вероятно нет — single call,
      type inference cached).
- [ ] Add `nova_print_char` runtime extern + dispatch case (AD3).
- [ ] Compile + smoke test:
   - `nova check bench/corpus/06_contracts.nv` — OK (already PASS)
   - `nova build bench/corpus/06_contracts.nv` — **must** produce
     `nova_print_str(...)` для line 55 (`println(str.from(factorial(5)))`)
   - Inspect generated C для подтверждения

**Acceptance:** generated C uses correct print helper; smoke run
prints "120\n" не garbage.

### Ф.2 — Tests (½ day)

- [ ] Implement R5 tests (f1-f10).
- [ ] Run `nova test plan67/` — all PASS.
- [ ] Run full `nova test` — 0 regressions vs baseline.

**Acceptance:** 10 new tests PASS; full suite 0 regressions.

### Ф.3 — bench/corpus unblock + spot-checks (½ day)

- [ ] Run `nova bench corpus run --quick` — all PASS.
- [ ] Spot-check 5 corpus файлов end-to-end:
   - 02_arithmetic_loop.nv — verify output корректен
   - 03_generic_heavy.nv — verify
   - 04_effects_handlers.nv — verify
   - 06_contracts.nv — **primary CI fix target**
   - 07_collection.nv — verify (содержит method chains)
- [ ] Bench history baseline для corpus_* (Plan 57.A.1
      `nova bench history-add`).

**Acceptance:** bench/corpus полностью green; baseline записан.

### Ф.4 — Cross-toolchain + final audit (½ day)

- [ ] Clang / MSVC / GCC build + test (Plan 58).
- [ ] Verify CI workflow `contracts-z3.yml` PASS на 06_contracts.nv
      (no CC-FAIL).
- [ ] Update `docs/simplifications.md`:
   - `[M-println-overload-static-method]` RESOLVED
   - `[M-println-char-as-int]` RESOLVED
   - `[M-infer-print-helper-duplication]` RESOLVED
- [ ] Update `docs/project-creation.txt` 2026-05-XX entry.

**Acceptance:** все toolchain PASS; CI green; simplifications synced.

---

## Acceptance criteria (production-grade)

- [ ] `println(str.from(<expr>))` корректен для всех numeric/bool args.
- [ ] `println(if/match {...})` корректен для всех return-type вариантов.
- [ ] `println('a')` печатает "a\n", не "97\n".
- [ ] `bench/corpus/06_contracts.nv` compile + run + correct output.
- [ ] All 25 affected sites в `bench/corpus/` PASS.
- [ ] `nova test` (release) — 0 regressions vs Ф.0 baseline.
- [ ] Cross-toolchain: PASS на Clang / MSVC / GCC.
- [ ] 10 new tests в `nova_tests/plan67/` PASS.
- [ ] CI `contracts-z3.yml` job PASS (TrivialBackend + Z3).
- [ ] `infer_print_helper` LOC reduced (DRY: ~75 → ~15 LOC).
- [ ] Doc comment на `println` updated.

---

## Open questions

1. **`infer_expr_c_type` корректность на if/match-expr?** Если он сам
   падает в `nova_int` default для них — Plan 67 не помогает. Audit
   в Ф.0 обязателен; если gap — добавить Ф.1.5 fix infer_expr_c_type.

2. **Performance: `infer_expr_c_type` дороже manual switch?** Single
   call на arg `println`. Type inference cached в `var_types` — должно
   быть O(1) lookup. Bench в Ф.3 если будет видимая регрессия
   (>1% bench time на print-heavy workload).

3. **Custom types (record/sum) — отдельная задача?** Да. Plan 67+1
   (deferred): auto-dispatch на `@to_str()` или Display protocol.
   Сейчас silent `nova_print_int` — preserved для backward-compat.

4. **`Display` protocol — overlap с Plan 13?** Plan 13 ввёл runtime
   stdlib including conversion fns. Если Display introduced там — Plan
   67+1 строит поверх. Если нет — Plan 67+1 вводит. Check в Ф.0.

5. **W6701 enable by default?** Conservative no — slow boil cleanup
   через opt-in CLI. Audit-driven (Plan 36 R-30 ergonomics) — позже.

---

## Что НЕ делает (out of scope)

- Custom types `@to_str` auto-dispatch (Plan 67+1)
- Display / Show protocol introduction
- `format!`-style string interpolation в print
- Variadic `println(a, b, c)` — отдельный D-block / план
- print performance optimization (batching, pre-formatting)
- W6701 default-enable

---

## Связь

- **[Plan 13](13-runtime-stdlib-and-autogen.md)** — `str.from(int)`
  registration. Plan 67 inferences над этим registration.
- **[Plan 36](36-cli-production-hardening.md)** — diagnostic
  infrastructure (R4 W6701 reuse).
- **[Plan 57](57-perf-benchmark-infrastructure.md)** — bench corpus
  (Plan 67 unblocks).
- **[Plan 58](58-cross-toolchain-msvc-verification.md)** — cross-toolchain
  matrix (R9).
- **[Plan 60 / D117](60-len-access-uniformity.md)** — context: Plan 60
  migrated `.len` → `.len()`; Plan 65 hotfix починил contracts SMT-encoder;
  Plan 67 — параллельный hotfix для println, разкрытый этим cascade'ом.
- **[Plan 65](65-chanreader-close-after.md)** — sibling hotfix (contract
  encoder); both Plan 65 hotfix и Plan 67 закрывают cascade Plan 60
  D117 reveal'а.

---

## Эволюция плана

- **2026-05-18 created**: hotfix-план, P0, 2 dev-days, 5 фаз. Triggered
  by CC-FAIL `bench/corpus/06_contracts.nv` discovered после Plan 65
  contract-encoder hotfix unmasked compile-time error.
