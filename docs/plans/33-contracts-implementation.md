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
| Default unknown в release | compile error (R20) — программист обязан явно `#unverified` | 33.1 |
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
`decreases`, composition `#pure` функций, record `invariant`.

**Зависит от:** 33.1 (`SmtBackend`, ContractCtx, runtime helpers).

**Acceptance:** binary search + bank-account примеры верифицируются.

**Срок:** ~3.5 недели.

### [33.3 — Advanced: pure_view + ghost + quantifiers + FP/strings + perf + Dafny-parity](33.3-contracts-advanced.md)

**Цель:** «не хуже Dafny». Эффект handler-state (`pure_view` + axiom),
ghost-переменные, `assume`/`assert_static`, bounded quantifiers,
IEEE 754 FP, string-теория, sets/maps, incremental cache, parallel
verification, module strict mode, `#trusted` external, AI-friendly diag
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


---

## Plan 33.3 Ф.9: bootstrap improvements (без libz3)

После аудита Plan 33 vs Verus / Dafny / Rust contracts RFC 2025 выявлены
4 doable-сейчас improvement'а которые закрывают **soundness gap** и
**production-parity** с Verus в части ghost erasure.

### 9.1 Ghost erasure в codegen (закрывает V5)

**Сейчас:** `ghost let x = ...` компилируется как обычный `let` —
значение вычисляется в runtime, занимает memory, дёргает effects если
есть. Это violates Dafny/Verus semantics.

**Будет:** `LetDecl.is_ghost == true` → codegen **не emit'ит** statement
в C-output. Ни в debug, ни в release. В debug если ghost-данные нужны
для invariant — это TODO для Plan 33.3 full.

**Acceptance:** ghost-fn вызовы не происходят в release (можно проверить
`nm` на готовом `.o`).

### 9.2 Record invariant auto-enforce (soundness gap)

**Сейчас:** `type Account { balance int } invariant balance >= 0` —
invariant парсится, но **никогда не проверяется**. Можно создать
`Account { balance: -100 }` без error. Silent unsoundness.

**Будет:** debug-сборка эмитит runtime-check `nova_contract_violation`
**после** каждой record-конструкции. После field-assignment в debug
тоже check (если record имеет invariant).

**Acceptance:** негативный тест — `Account { balance: -1 }` → runtime
panic в debug.

### 9.3 Loop invariants/decreases в AST (закрывает V2)

**Сейчас:** `parser::skip_loop_clauses` парсит `invariant`/`decreases`
но **выбрасывает**. AST не хранит. Программист пишет spec в пустоту.

**Будет:** Расширить `ExprKind::For`/`While`/`Loop` полями
`invariants: Vec<Expr>` + `decreases: Option<Expr>`. Все match'и на
ExprKind в codegen/interp/types обновляются. В debug runtime-check
invariant **перед** входом + **после** каждой итерации.

**Acceptance:** loop с явно ложным invariant → runtime panic в debug.

### 9.4 Decreases runtime check для recursion

**Сейчас:** `fn fib(n int) decreases n` — `decreases` парсится в
`FnDecl.decreases`, но не проверяется. Recursive fn без decreases-decrement
не ловится → infinite recursion в debug.

**Будет:** Codegen для рекурсивных fn (имя callee = имя callera)
эмитит runtime-check: snapshot `m_old` до call, после возврата проверить
`m_new < m_old`. Failure → contract violation «decreases not decreased».

**Acceptance:** `fn bad(n int) decreases n => bad(n)` → runtime panic
после первого recursive call.

### Что НЕ входит в Ф.9 (для Plan 33.3 full / отдельный milestone)

- SMT verify для всех этих фич (требует libz3) — V1-V11 остаются.
- Frame condition runtime check (`modifies` после fn-exit) — отдельный
  improvement, можно сделать позже.
- pure_view + axiom + #verify_handler — требует Z3.
- Quantifiers — требует Z3.

### Регрессия

После Ф.9: 209/209 baseline + новые negative-тесты на ghost-erasure,
invariant-violation, loop-invariant-violation, decreases-violation.

### 9.5 Pre-entry loop invariant check (closes V2 fully)

**Закрыто (720e230ef9):** parse_for/while/loop теперь wrap'ает
loop-expression в outer Block с pre-entry `assert_static` для каждого
invariant. Catches violation **до** первой итерации (когда invariant
ложен с самого старта или loop не выполняется).

Combined с Ф.9.3 (per-iteration check) — invariants полностью enforced
в debug. SMT havoc-based verify ждёт Z3.

### 9.7 Ghost-var usage type-check (closes V5 properly)

**Закрыто (4c3335d780):** `check_ghost_usage` в types/mod.rs walk'ает
каждый fn-body, accumulating ghost-set из `ghost let` stmt'ов.
Non-ghost stmt/expr читающий ghost-var → proper compile-error на
type-check этапе. До этого: catched на C-level («undeclared
identifier» после codegen-erasure) — honest fallback, плохой UX.

### 9.6 Frame runtime check — ОТЛОЖЕН

Полезен только в комбинации с SMT (без SMT compile-time frame check
уже работает на assignments level; runtime check для индирекций через
callee fn'ы требует знания их modifies — не доступно без full analysis).
Откладывается до Z3 milestone.

### 9.8 Loop decreases runtime check

**Закрыто (f4d4592d61):** для loops с `decreases <expr>` парсер
inject'ит:
- `let _nova_decr_old = <expr>` в начало body (snapshot).
- `assert_static (<expr>) < _nova_decr_old` в конец body (decrement check).

Эффект: loop не decrementing → runtime assert_static panic в debug.
Catches infinite loops через assert_static механизм. SMT well-founded
check (полный Dafny-grade) — ждёт Z3.
