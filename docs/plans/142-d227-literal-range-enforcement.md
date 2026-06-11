<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 142 — D227 literal range enforcement (`E_LIT_OUT_OF_RANGE`)

> **Создан:** 2026-06-11.  **Статус:** ✅ CLOSED (Ф.1–Ф.3, 2026-06-11).  **Эстимат:** ~0.5 dev-day.
> **Model:** Sonnet/Opus HIGH (type-checker + tests; спека готова).
> **Триггер:** D227 acceptance не закрыта — `[M-D227-lit-range-error-code]` +
> `[M-D227-literal-range-tests]` (backlog P1/P2). Gate: после Plan 141 (общий .rs/compiler).
> **Коммиты:** `d6b209b8e63` (Ф.1 enforcement + fixtures), `2d85fff2175` (Ф.2 spec acceptance),
> Ф.3 (docs + close). Worktree `nova-p138` @ `plan-138.1`.

---

## Контекст

**D227** ([03-syntax.md:9153](../../spec/decisions/03-syntax.md), 2026-06-03) — «Numeric literal
inference — default `int`, context coercion, hard range-check». Spec **написан** (4 правила + D44
amend); enforcement + тесты **сделаны Plan 142** ([03-syntax.md:9367-9371](../../spec/decisions/03-syntax.md)):
- [x] `E_LIT_OUT_OF_RANGE` compiler error code — landed Ф.1 (`d6b209b8e63`), emitted at literal→sized-int coercion site (`types/mod.rs` `assignable()`). Closes `[M-D227-lit-range-error-code]`.
- [x] test corpus — landed Ф.1 в `nova_tests/plan142/` (8 NEG + 2 POS, 10/0). Closes `[M-D227-literal-range-tests]`.

**Ожидаемое поведение (D227):**
```nova
ro a i32 = 3_000_000_000   // ✗ E_LIT_OUT_OF_RANGE: 3000000000 > i32.MAX (2147483647)
ro b u8  = 300             // ✗ E_LIT_OUT_OF_RANGE: 300 > u8.MAX (255)
ro c u8  = -1              // ✗ E_LIT_OUT_OF_RANGE: -1 < u8.MIN (0)
ro d u8  = 255             // ✓ boundary OK
```
Это **compile-time** проверка при context-coercion целочисленного литерала к sized-типу
(`u8`/`u16`/`u32`/`u64`/`i8`/`i16`/`i32`/`i64`). P1 — safety-gap (сейчас, возможно, тихо wrap/UB).

---

## Фазы

### Ф.0 — Setup + recon (verify-currently-emitted)
- Baseline. Прочитать D227 полностью (4 правила; 03-syntax.md:9153-9371) + D44/D129 cross-refs.
- **VERIFY:** что компилятор СЕЙЧАС делает на `u8 = 300` / `i32 = 3_000_000_000` / `u8 = -1`?
  (тихо wrap? уже ошибка? partial?) — определяет scope (add vs complete).
- Recon: где context-coercion целочисленного литерала к sized-типу (type-checker / types/mod.rs —
  literal-inference + coercion; и/или emit_c numeric-literal cast). Где известны MIN/MAX sized-типов.

### Ф.1 — `E_LIT_OUT_OF_RANGE` enforcement (.rs)
- На сайте context-coercion литерала к sized-типу: если значение литерала вне `[T.MIN, T.MAX]` →
  hard compile-error `E_LIT_OUT_OF_RANGE` с сообщением «<val> > <T>.MAX (<max>)» / «< <T>.MIN».
- Покрыть все 8 sized-int (u8/16/32/64, i8/16/32/64) + знаковость (−1 на unsigned). Per D227 4 правила.
- Rebuild. **Fixtures nova_tests/plan142/** (verify РЕЛИЗНЫМ nova):
  - NEG: `u8=300`, `u8=-1`, `i32=3_000_000_000`, `u16=70000`, `i8=200`, `u64`-overflow → каждый `E_LIT_OUT_OF_RANGE`.
  - POS (boundary): `u8=255`, `u8=0`, `i8=-128`, `i8=127`, `i32=2147483647`, `u16=65535` → компилируются.
  - POS: default `int` без coercion (большой literal в int-контексте) — не триггерит (D227 default int).
- Broad regression (literal-heavy: plan-numeric/sized-int dirs + общий) — 0 новых FAIL.
- **Commit:** `feat(plan142): E_LIT_OUT_OF_RANGE — D227 compile-time literal range-check`

### Ф.2 — Spec acceptance close
- Отметить ✅ в D227 acceptance ([03-syntax.md:9367,9371](../../spec/decisions/03-syntax.md)): error code + tests.
- Coherence: D227 / D44 / D129 согласованы.
- **Commit:** `spec(plan142): D227 acceptance closed — E_LIT_OUT_OF_RANGE + tests landed`

### Ф.3 — Docs + close
- Acceptance audit. **УБРАТЬ** `[M-D227-lit-range-error-code]` + `[M-D227-literal-range-tests]` из
  backlog-followups.md (P1/P2). project-creation + simplifications + nova-private/discussion-log + memory.
- Final regression. Commit + push (sync с main — по запросу пользователя).

---

## Risk register
| # | Риск | Sev | Mitigation |
|---|---|---|---|
| R1 | range-check уже частично есть (Ф.0 verify) → complete, не дублировать | 🟡 MED | Ф.0 recon обязателен |
| R2 | literal в const-fn / generic-context — coercion-сайт иной | 🟡 MED | покрыть основные binding/param/return coercion; edge → document |
| R3 | hex/bin/octal + `_`-разделители + unary-minus парсинг значения | 🟡 MED | тесты на формы литералов; парсить значение точно |
| R4 | регрессия: существующие фикстуры с «большими» литералами, что раньше тихо проходили | 🟡 MED | broad regression; мигрировать если легитимны |

## Acceptance
- **A1 ✅** — `u8=300` / `u8=-1` / `i32=3_000_000_000` → `E_LIT_OUT_OF_RANGE` (compile-time). NEG fixtures `neg_u8_300`, `neg_u8_minus1`, `neg_i32_3b` PASS.
- **A2 ✅** — boundary POS (`u8=255`/`0`, `i8=-128`/`127`, `u16=65535`, `i16` MIN/MAX, `u32=MAX`, `i32` MIN/MAX, `i64.MAX`, `u8=0xFF`) компилируются. `pos_boundaries` compile+run.
- **A3 ✅** — default `int`/`uint` (без sized-coercion, D227 Rule 1 wide defaults) не триггерит. `pos_wide_int` (3_000_000_000 bare/annotated `int` + int-param) compile+run.
- **A4 ✅** — все 8 sized-int (u8/16/32/64, i8/16/32/64) + знаковость (`-1` на unsigned, D227 Rule 6 `Unary{Neg,IntLit}` arm); hex (`0x1FF=511`), `_`-разделители покрыты (lexer parses value into IntLit before checker).
- **A5 ✅** — D227 spec acceptance ✅ (03-syntax.md:9367/9371 + scoped open-questions subsection, Ф.2 `2d85fff2175`); `[M-D227-lit-range-error-code]` + `[M-D227-literal-range-tests]` убраны из backlog-followups.md (Ф.3).
- **A6 ✅** — 0 регрессий. Baseline-clean: см. Phase outcomes.

## Phase outcomes
- **Ф.0 (recon)** — verified: `Unary{Neg,IntLit}` (`-1` на unsigned) ранее БЫЛ wholly unchecked (fell to `Compat::Unknown`); sized-int coercion-сайт = `types/mod.rs` `assignable()`.
- **Ф.1 (enforcement, `d6b209b8e63`)** — `Compat::OutOfRange{msg}` variant + 3 helpers (`sized_int_bounds`, `sized_int_name`, `lit_range_check`) в `types/mod.rs`. `assignable()` IntLit arm + NEW `Unary{Neg,IntLit}` arm (Rule 6). Both call-sites (`f1_check_assign_let` binding + call-arg) exhaustive-match → `[E_LIT_OUT_OF_RANGE]`. i128 throughout (IntLit i64; u64.MAX + negated). plan142 10/0; 0 регрессий.
- **Ф.2 (spec, `2d85fff2175`)** — 2 D227 acceptance checkboxes → `[x]` + «Открытые вопросы (scoped)» subsection (2 edges). D227/D44/D129 coherence verified. spec-only, no rebuild.
- **Ф.3 (docs + close)** — этот файл CLOSED; markers убраны из backlog; simplifications + project-creation + nova-private/discussion-log appended. Final regression confirm.

## Scoped open-questions (deferred, документировано в D227 spec)
- **Alias/newtype над sized-int НЕ range-checked** — `assignable()` проверяет только direct Named sized-int (+ Readonly/Mut/Unsafe wrappers); резолв alias-имени требует `self.types` (недоступно на free-fn coercion-сайте — иначе печатался бы неверный type-name). → `[M-D227-alias-newtype-range]` (P3).
- **Float range-check (D227 Rule 5, f32 exponent overflow) НЕ реализован** — Ф.1 scope был integer-only (plan §43 «all 8 sized-int», floats не перечислены). → `[M-D227-float-range-check]` (P3).

## Связь
- Закрывает `[M-D227-lit-range-error-code]` + `[M-D227-literal-range-tests]`.
- Открывает (deferred-scope): `[M-D227-alias-newtype-range]` + `[M-D227-float-range-check]` (оба P3).
- D227 (03-syntax.md) — enforcement + acceptance. D44/D129 cross-ref.
