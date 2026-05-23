// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 101 — `fn[T]` receiver-generic prefix + bounds + protocol composition (master)

> **Создан:** 2026-05-24. **Ред. 3 (2026-05-24):** complete rewrite
> после critical review. Initial misinterpretation (implicit T)
> отвергнута user'ом — final design: explicit `fn[T]` prefix везде где
> receiver не имеет carrier-brackets, + bounds через existing D72,
> + multi-bound `+`, + protocol composition `use Foo`.
>
> **Статус:** 🟡 roadmap — декомпозирован на 5 sub-plan'ов.
> **Приоритет:** **P1** (101.1 + 101.5 — blocker для Plan 91 / std MVP;
> vec.nv broken). 101.2/3/4 — P2/P3.
> **Зависимости:** [Plan 48](48-closures-in-generics.md) ✅ (D119
> method-level generics); [Plan 72](#) / D72 ✅ (bound syntax);
> [Plan 88](88-static-method-on-typevar.md) ✅; [Plan 99](99-option-result-closure-applying-methods-nova-body.md) ✅.
> **Spec:** [D145](../../spec/decisions/02-types.md#d145-fnt-префикс--receiver-generic-decl--bounds-plan-101).
> **Источник:** design discussion 2026-05-24 + discovery of vec.nv
> (`fn []T @method` written-as-if-T-is-generic, codegen failure).

---

## 1. Картина задачи

### Реальный gap

```nova
// std/collections/vec.nv — РЕАЛЬНЫЙ КОД (7 методов!)
export fn []T @map[U](f fn(T) -> U) -> []U { ... }
```

`T` здесь — **именованный тип**, не дженерик. Парсер silent-принимает.
Type-checker мягкий — пропускает. Codegen падает (Nova_T undefined).
→ vec.nv **не компилируется в exe**, blocker для [Plan 91](91-stdlib-mvp-for-0.1.md)
(std MVP для релиза 0.1).

### Корень

В Nova generic-typevars декларируются через `[…]` brackets. Сейчас
есть только **одна позиция** для receiver-generic: brackets на named
type (`Option[T]`, `HashMap[K, V]`). Для `[]T` / bare T / tuple — нет
способа объявить generic, парсер «угадывает» — silent miscompile.

### Решение

Ввести `fn[T]` префикс — **обязателен** для каждого receiver-typevar,
не покрытого carrier-brackets. Plus интегрировать:
- existing D72 bound syntax (`fn[T Hashable]`).
- multi-bound `+` (закрывает Q-multi-bound).
- protocol composition `use Foo` (закрывает D53 open question).

**Никакого implicit T** (моя initial misinterpretation отвергнута).

### Что Nova делает лучше (vs Rust/Go/TS/Kotlin/Scala/Java)

См. [D145 §«Параллель индустрии»](../../spec/decisions/02-types.md#d145).
Кратко:

| Aspect | Nova post-101 | Industry |
|---|---|---|
| Receiver-method syntax | `fn[T] []T @map[U]` (1 line) | Rust `impl<T> Vec<T> { fn map<U> }` (2 nested) |
| Bound syntax | `[T Hashable]` (no `:`) | Rust `<T: Hashable>` |
| Multi-bound | `[T A + B]` (Rust-style) | Rust `+`, TS `&`, Kotlin `where`, Go `\|` (union) |
| Protocol composition | `use Foo` inside `protocol { }` | Go interface embed, Java `extends A, B` |
| Loud disambiguation | `E_BARE_TYPEVAR_NEEDS_PREFIX` | silent infer (most langs) |

---

## 2. Декомпозиция (5 sub-plan'ов)

| # | Sub-plan | Что | Приоритет | Eval | GATED |
|---|---|---|---|---|---|
| **101.1** | [`fn[T]` core grammar + codegen + vec.nv migration](101.1-fn-prefix-core.md) | Parser/type-check/codegen для `fn[T] []T @method`, `fn[T] T @method`, `fn[T, U] (T, U) @method`. **Includes vec.nv migration** (7 методов). Disambiguation matrix (bare T vs named T) + 4 error codes. | **P1** (blocker Plan 91) | ~2.5 dev-day | — |
| **101.2** | [Bound integration `fn[T Hashable]`](101.2-bound-integration.md) | Reuse existing D72 bound syntax в `fn[…]` prefix position. | P2 | ~0.5 dev-day | 101.1 |
| **101.3** | [Multi-bound `[T A + B]`](101.3-multi-bound.md) | Закрывает [Q-multi-bound](../../spec/open-questions.md#q-multi-bound). Применим везде где D72 bound допустим (free fn, type-decl, fn[T] prefix). | P3 | ~1 dev-day | 101.2 |
| **101.4** | [Protocol composition `use Foo`](101.4-protocol-composition.md) | Закрывает open question в D53 §«Открытые вопросы». Embed protocols через `use` keyword (параллель D39). | P2 | ~1 dev-day | — (independent) |
| **101.5** | [Stdlib audit + LSP + close](101.5-stdlib-audit-close.md) | Sweep std/ на похожие patterns, LSP quick-fixes (Plan 50 D102), vec.nv tests в `nova test` baseline, close Plan 101 + D145 + markers. | **P1** (closing) | ~1 dev-day | ALL 101.1-4 |

**Total:** ~6 dev-day.

**Critical path:** 101.1 (P1, blocker) → 101.2 → 101.5 = ~4 dev-day минимум.

---

## 3. Acceptance criteria (master Plan 101)

Plan 101 = ✅ ЗАКРЫТ когда:

- [ ] **101.1 ✅** — vec.nv (7 методов) компилируется в exe, все тесты PASS.
- [ ] **101.2 ✅** — `fn[T Hashable] []T @dedup()` работает.
- [ ] **101.3 ✅** — `[T A + B]` multi-bound работает; Q-multi-bound closed.
- [ ] **101.4 ✅** — `type X protocol { use A; use B }` работает; D53 open question closed.
- [ ] **101.5 ✅** — std audit + LSP quick-fixes для 4 error codes + vec.nv в baseline.
- [ ] **Spec:** D72 amend (extends to fn[T] prefix), D53 close composition gap, D145 ✅ stabilized.
- [ ] **Полный `nova test`** зелёный (regression 0).
- [ ] **simplifications marker** `[M-receiver-generic-incompleteness]` ✅ ЗАКРЫТО.
- [ ] **Plan 91 Ф.1** (collections) unblock'нут.
- [ ] **Memory** `project-plan101-status.md` создан.

---

## 4. Тесты positive + negative (cumulative по sub-plan'ам)

| Sub-plan | Positive | Negative | Total |
|---|---|---|---|
| 101.1 | 12 (array/bare/tuple basic + composite + vec.nv migration test'ы) | 8 (disambiguation matrix exhaustive) | 20 |
| 101.2 | 6 (single bound на разных protocol-shapes) | 4 (bound violation, unknown protocol, etc.) | 10 |
| 101.3 | 5 (2-bound, 3-bound, parametric) | 4 (duplicate, unknown, with carrier conflict) | 9 |
| 101.4 | 6 (single embed, multiple embed, nested embed, embed + own method) | 4 (cycle, self-embed, type-embed-in-protocol error, missing-impl) | 10 |
| 101.5 | 3 (LSP smoke) | 2 (diagnostic format) | 5 |
| **Total** | **32** | **22** | **54** |

---

## 5. Risks + mitigations

| Risk | Severity | Mitigation |
|---|---|---|
| 101.1 disambiguation matrix ломает existing user code (single-letter named types где-то существуют) | High | Ф.0 audit: `grep -rE "^type [A-Z] " std/ nova_tests/` — найти и решить. Если есть — required `fn[T] []T` или full-path |
| vec.nv migration ломает call-sites (где используются map/filter/fold) | Medium | Call-sites не меняются (только signature); тесты должны просто PASS после migration |
| Codegen `fn[T]` dispatch требует extensive emit_c.rs refactor | High | Ф.0 probe в 101.1 — measure scope; декомпозировать дальше если >2.5 dev-day |
| Q-multi-bound `+` syntax conflict с future arithmetic-in-types | Low | `[…]` brackets — pure type context, identifier'ы only; `+` unambiguous |
| Protocol composition (101.4) — circular embed (Reader uses Writer, Writer uses Reader) | Medium | Cycle detection в type-checker; loud error |
| LSP quick-fix (101.5) формат не финализован | Low | Coordinate с Plan 50; postpone Ф.3 если нужно |

---

## 6. Lineage + cross-refs

### Депенди (closed)

- Plan 48 / D119 — method-level generics.
- Plan 72 / D72 — bound syntax `[T Bound]`.
- Plan 88 — static-method-on-typevar.
- Plan 99 — Option/Result closure-applying на Nova-body.
- D39 — `use Type` embed для records.
- D53 — `type X protocol { ... }`.

### Open questions closed

- [Q-multi-bound](../../spec/open-questions.md#q-multi-bound) — 101.3 (`[T A + B]`).
- D53 §«Открытые вопросы» «Composition protocol'ов» — 101.4.

### Open questions opened (future planning)

- [Q-representation-bound](../../spec/open-questions.md#q-representation-bound)
  — concrete-type bounds (`fn[T int]` для newtype `type UserId int`,
  `fn[T User]` для record embed `use user User`). **Plan 102 future.**

---

## 7. Не входит (future work)

- **Plan 102** — representation/structural bounds на concrete types.
- **Higher-kinded receiver** — `fn[F[_]] F[T] @method`. Требует HK-type-checker.
- **Implicit T** — отвергнуто (Ред. 2 misinterpretation).
- **`where T: Hashable` clause** — отвергнуто; `[T Hashable]` + multi-bound покрывают.
