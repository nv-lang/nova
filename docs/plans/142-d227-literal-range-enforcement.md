<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 142 — D227 literal range enforcement (`E_LIT_OUT_OF_RANGE`)

> **Создан:** 2026-06-11.  **Статус:** 📋 PLANNED.  **Эстимат:** ~0.5 dev-day.
> **Model:** Sonnet/Opus HIGH (type-checker + tests; спека готова).
> **Триггер:** D227 acceptance не закрыта — `[M-D227-lit-range-error-code]` +
> `[M-D227-literal-range-tests]` (backlog P1/P2). Gate: после Plan 141 (общий .rs/compiler).

---

## Контекст

**D227** ([03-syntax.md:9153](../../spec/decisions/03-syntax.md), 2026-06-03) — «Numeric literal
inference — default `int`, context coercion, hard range-check». Spec **написан** (4 правила + D44
amend), но **enforcement + тесты не сделаны** ([03-syntax.md:9367-9371](../../spec/decisions/03-syntax.md)):
- [ ] `E_LIT_OUT_OF_RANGE` compiler error code — `[M-D227-lit-range-error-code]` (verify emitted **или** add).
- [ ] test corpus — `[M-D227-literal-range-tests]`.

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
- **A1** — `u8=300` / `u8=-1` / `i32=3_000_000_000` → `E_LIT_OUT_OF_RANGE` (compile-time).
- **A2** — boundary POS (`u8=255`, `i8=-128`, `i32=MAX`) компилируются.
- **A3** — default `int` (без sized-coercion) не триггерит.
- **A4** — все 8 sized-int + знаковость покрыты; hex/bin/`_` формы.
- **A5** — D227 spec acceptance ✅; маркеры убраны из backlog.
- **A6** — 0 регрессий.

## Связь
- Закрывает `[M-D227-lit-range-error-code]` + `[M-D227-literal-range-tests]`.
- D227 (03-syntax.md) — enforcement + acceptance. D44/D129 cross-ref.
