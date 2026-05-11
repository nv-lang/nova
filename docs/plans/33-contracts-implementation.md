# Plan 33: Контракты (D24) — roadmap-индекс

> Это **индекс** разбитого плана. Детали по фазам — в подпланах
> [33.1](33.1-contracts-core.md), [33.2](33.2-contracts-imperative.md),
> [33.3](33.3-contracts-advanced.md). Каждый подплан — самостоятельная
> единица работы с decision point на выходе.

## Цель

Production-grade реализация контрактов (D24 — `requires` / `ensures` /
`invariant` со static SMT + runtime fallback в debug + zero-cost release).
Целевой уровень — **не хуже Dafny / Creusot**; превосходство над
Rust-mainstream (где контрактов нет) и Rust+kani/prusti (где отдельный CLI,
не часть языка).

Сейчас в коде контрактов **нет** —
[`compiler-codegen/src/ast/mod.rs:4`](../../compiler-codegen/src/ast/mod.rs#L4)
прямо говорит «contracts пропускаются на уровне парсера».

Связь:
- [Plan 15](15-generic-bounds-enforcement.md) — infrastructure для bounds.
- [Plan 16](16-capability-enforcement.md) — attribute parsing.
- [Plan 25 G6](25-production-readiness-roadmap.md) — preemption влияет на runtime fallback в fiber-context.

## Глобальные решения (зафиксированы для всех подпланов)

| Решение | Выбор | Где применяется |
|---|---|---|
| Семантика `int` в SMT | Bounded `Int` + axiom `INT_MIN..INT_MAX` | 33.1+ |
| `ghost` в debug runtime | Никогда не emit (как Dafny) — ghost чисто spec-уровень | 33.3 |
| Engine-agnostic SMT | `trait SmtBackend`; Z3 — primary, CVC5 — secondary impl | 33.1+ |
| Contract-DSL | подмножество Nova-expr + magic-имена (`result`, `old`) | 33.1 закладывает |
| Default unknown в release | compile error (R20) — программист обязан явно `@unverified` | 33.1 |
| `simplifications.md` | не используется этим планом — что не входит, явно в «Не входит» | все подпланы |

## Подпланы

### [33.1 — Core: requires/ensures/old/result + Z3 + runtime fallback](33.1-contracts-core.md)

**Цель:** end-to-end pipeline на минимальном scope. Парсер → typecheck →
Z3 SMT → debug runtime check → zero-cost release. Только straight-line
код (без циклов, frame, pure_view, ghost, quantifiers).

**Acceptance:** 10 примеров из R4 ([`spec/revolutionary.md`](../../spec/revolutionary.md))
работают end-to-end. Decision point: продолжать ли с 33.2.

**Срок:** ~2 недели.

### [33.2 — Imperative: frame + loops + termination + composition + record invariant](33.2-contracts-imperative.md)

**Цель:** превратить 33.1 в инструмент, способный верифицировать
реальный imperative-код. Добавляет `reads`/`modifies`, loop invariants,
`decreases`, composition `@pure` функций, record `invariant`.

**Зависит от:** 33.1 (`SmtBackend`, ContractCtx, runtime helpers).

**Acceptance:** binary search + bank-account примеры верифицируются.

**Срок:** ~3.5 недели.

### [33.3 — Advanced: pure_view + ghost + quantifiers + FP/strings + perf + Dafny-parity](33.3-contracts-advanced.md)

**Цель:** «не хуже Dafny». Эффект handler-state (`pure_view` + axiom),
ghost-переменные, `assume`/`assert_static`, bounded quantifiers,
IEEE 754 FP, string-теория, sets/maps, incremental cache, parallel
verification, module strict mode, `@trusted` external, AI-friendly diag
+ JSON, Dafny-tutorial port (20 примеров).

**Зависит от:** 33.2 (loop invariants, frame, composition).

**Acceptance:** Dafny-parity на 20+ tutorial примерах; performance gate
(build-time <30% overhead с cache, parallel ≥6× speedup на 8 cores).

**Срок:** ~3.5 недели.

## Production-grade gate всего плана 33

После 33.3 (если дошли до него):

1. Все примеры R4 + R5.7 + 20 Dafny-port тестов доказываются.
2. Cross-check Z3↔CVC5 без disagreements.
3. `nova test --verify-zero-cost` зелёный.
4. Q-contract-dsl + Q-pure-view закрыты, перенесены в `decisions/history/`.
5. D24 обновлён, ≥ 8 новых decisions добавлены.
6. ≥ 150 тестов; CI jobs `contracts-zero-cost`, `contracts-cross-check`,
   `contracts-dafny-parity` обязательны для merge.

## Не входит во все подпланы 33

- Полный LSP server (только CLI hooks в 33.3).
- Inductive predicates — research, отдельный D-decision если потребуется.
- Refinement types — отдельный план, существенное расширение системы типов.
- Higher-order contracts на closure-аргументах — отдельный D-decision.
- Hardware/timing contracts (`bounded(time: O(n log n))`) — отдельная область.
- Distributed contracts через spawn/suspend — после M:N runtime (Plan 23).
- LLM-генерация контрактов из тестов (R5.7) — отдельный план tooling.
- Unbounded quantifiers — запрещены по дизайну (compile error, паритет с Dafny v1).
