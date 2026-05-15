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

## Plan 33 Z3 V1 milestone (2026-05-13) — ЗАКРЫТ

После bootstrap'а (Ф.1-Ф.4 на TrivialBackend) добавлен полноценный Z3
backend. End-to-end Plan 33.1 теперь работает в продакшен-режиме:
`requires`/`ensures` доказываются linear-arith reasoning'ом, а не только
константной симплификацией.

### Поставлено

- `compiler-codegen/Cargo.toml`: feature flag `z3-backend` (default off).
- `compiler-codegen/build.rs`: при feature=on линкует
  `vcpkg_installed/x64-windows-static/lib/libz3.lib` + psapi/advapi32/
  user32 (Win) или stdc++/pthread (*nix). Чёткое сообщение если libz3
  не установлен.
- `compiler-codegen/src/verify/backend/z3_ffi.rs`: собственные FFI
  bindings (~30 функций C API) — **никакого** внешнего `z3-sys` / `z3`
  crate'а, по правилу feedback_third_party_libs.
- `compiler-codegen/src/verify/backend/z3.rs`: `Z3Backend` impl
  `SmtBackend`. Refcounted AST (Z3_mk_context_rc + inc_ref/dec_ref).
  Pattern: int/bool/str literal'ы → Z3-AST; record member access
  через uninterpreted functions. `unsafe impl Send`. `Drop` для
  cleanup всех Z3 refs.
- `compiler-codegen/src/verify/pipeline.rs`: `BackendChoice::{Trivial,
  Z3}` + env-var `NOVA_SMT_BACKEND={trivial|z3}` (default trivial).
- `nova-cli/Cargo.toml`: feature `z3-backend` форвардит в `nova_codegen`.
  Build: `cargo build --release --features z3-backend` из nova-cli/.
- `nova_tests/contracts/z3_*.nv` × 3: positive linear-arith + implication
  chain + counterexample (EXPECT_COMPILE_ERROR). Все с маркером
  `// REQUIRES_SMT_BACKEND z3`.
- `compiler-codegen/src/test_runner.rs`: gate `// REQUIRES_SMT_BACKEND
  <name>` — тест skip'ается если активный backend не совпадает.
  Используется и для тестов, требующих **trivial** (где Z3 доказал бы
  лишнее) — например `verify_must_verify_fail.nv`.

### НЕ входит (отдельные milestone'ы)

- **V7**: strings beyond equality (substring/contains/index_of через
  `(Seq Int)`). Сейчас Z3 поддерживает только eq на строках.
- **V8**: IEEE 754 FP — теория FloatingPoint Z3/CVC5.
- **V11**: bounded quantifiers (`forall x in xs: P(x)`).
- **V**: incremental cache + parallel verification + Z3↔CVC5 cross-check
  (см. Plan 33.3 Ф.12).
- Linux/macOS smoke (vcpkg builds passable на *nix, но без CI).

### Acceptance

- `cargo test --features z3-backend` зелёный (включая 3 unit-теста в
  `backend/z3.rs`).
- `NOVA_SMT_BACKEND=z3 nova test nova_tests/contracts/z3_*.nv` — все 3
  PASS.
- Без env-var (default trivial) `nova test` skip'ает Z3-only tests
  через `// REQUIRES_SMT_BACKEND` gate; regression baseline сохранён.

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

### 9.9 Selective stripping для proven контрактов

**Закрыто (d1dd9ece8a):** true zero-cost даже в debug. ModuleEnv.proven_contracts
(Vec<(fn_name, span)>) от VerificationPipeline передаётся в CEmitter
через `set_proven_contracts`. Codegen skip emit для proven контрактов
в emit_fn requires-loop + emit_ensures_checks.

Effect: доказанные контракты (reflexive ensures, constant folding patterns)
не генерируют runtime check вообще. Verified: verify_must_verify_proven.c
содержит 1 nova_contract_violation (unproven requires), не 2.

### 9.10 AI-friendly diagnostic

**Закрыто (2969afbba4):** error messages теперь включают:
- fn name (где).
- counterexample values (или honest hint про TrivialBackend limits).
- categorised UnknownReason (Timeout/NonLinear/Unsupported/NotAttempted).
- 3-4 numbered suggestions per category.

Format D24 §107 (AI-first compiler как обучающий сигнал для LLM).

### 9.11 Loop decreases negative test

**Закрыто (d1dd9ece8a):** expected_runtime/f9_loop_decreases_fail.nv —
loop не decrement'ит decreases → assert_static violation. Parse OK
через nova-codegen check. Full runtime test заблокирован WIP
channels.h (plan 40); generated `.c` содержит correct check.

---

## Plan 33.5: Contracts Verifier Production Hardening — ЗАКРЫТ (2026-05-15)

Продолжение после Plan 33.3 Ф.9. Закрывает soundness gap в верификаторе:
purity inference, lemma/calc proofs, Liskov handler verification,
post(Action)(view) symbolic exec.

### Итоговый статус: 82/82 тестов PASS, 9 SKIP (z3-only)

### Ф.3 — SCC-based purity inference (закрыт)

**Что:** `infer_pure_fns_scc(module)` — через Tarjan SCC находит все
функции, тело которых не делает effectful-вызовов. Результат используется
в SMT-кодировании для inline'а чистых функций.

**Коммит:** в ветке `plan33-4`.

### Ф.4.1 — Lemma functions (закрыт)

**Что:** ключевое слово `lemma` объявляет доказанный proof term.
`apply lemma_name(args)` в теле функции активирует ensures леммы как
SMT-аксиому в текущем scope.

**Синтаксис:**
```nova
lemma double_non_neg(n int)
    requires n >= 0
    ensures result == n * 2
=> n * 2

fn use_it(n int) -> int
    requires n >= 0
    ensures result >= 0
{
    apply double_non_neg(n)
    n * 2
}
```

**Коммит:** 77→78 тестов.

### Ф.4.2 — Calc proofs (закрыт)

**Что:** `calc { expr; == expr; == expr; }` — структурированное
equational reasoning. Каждый шаг доказывается и утверждается в SMT.

**Синтаксис:**
```nova
calc {
    x * 2;
    == x + x;
    == result;
}
```

**Коммит:** 78→79 тестов.

### Ф.5.1 — EffectMethod contracts (закрыт)

**Что:** `requires`/`ensures` на методах effect-декларации:
```nova
type Storage effect {
    read(key int) -> int
        requires key > 0
        ensures result >= 0
}
```

**Коммит:** 79→80 тестов.

### Ф.5.2 — Liskov SMT verify (закрыт)

**Что:** handler помеченный `#verify` обязан удовлетворять контрактам
методов эффекта. Верификатор проверяет Liskov substitution principle:
handler.m реализует effect.m.ensures при effect.m.requires.

**Как работает:** `verify_liskov_method` в `pipeline.rs` кодирует
handler-body как SMT-term, подставляет его в ensures эффекта и
вызывает `try_prove`. Независимо от наличия axioms у effect.

**Тесты:**
- `liskov_handler_positive.nv` — handler соответствует контракту
- `liskov_handler_fail.nv` — handler нарушает ensures (EXPECT_COMPILE_ERROR)

**Коммит:** 80→81 тестов.

### Ф.6 — post(Action)(view) symbolic exec V2 (закрыт)

**Что:** верификация axiom-формул вида `post(Action(args))(view(args))`.
Символически вычисляет значение `view` после выполнения `Action`.

**Алгоритм:**
1. `parse_post_call` — декодирует doubly-curried `post(action_call)(view_call)`.
2. `extract_block_assignments` — собирает `x = e` из block-body Action.
3. Применяет присваивания к body View: `view_body[captured_var/new_val]`.
4. Подставляет axiom-аргументы вместо параметров Action/View.
5. Переписывает `post(...)` в формуле → encode → prove.

**Пример:**
```nova
type Db effect {
    SetBalance(id int, amount int)
    #pure balance(id int) -> int
    axiom after_set(id, x) =>
        post(SetBalance(id, x))(balance(id)) == x
}
// handler: SetBalance { store = amount }; balance => store
// post(SetBalance(id,x))(balance(id)) → store[amount/store] → amount → x == x ✓
```

**Ограничения V2 scope:**
- Action body: только `Block` с простыми `Assign` (нет if/loop/match).
- View body: только `=> expr` форма (не block).
- Одна captured переменная (нет State-record).
- Без учёта axiom binder aliasing (id в action и view — разные).

**Тест:** `post_action_view_positive.nv` — SetBalance/balance пример.

**Коммит:** 81→82 тестов.

---

## Z3 dev-setup

### Как работает linkage

`build.rs` активируется при Cargo feature `z3-backend`. Ищет libz3 через
vcpkg manifest mode — в папке `vcpkg_installed/<triplet>/lib/`:

| ОС | Triplet по умолчанию | Переопределение |
|---|---|---|
| Windows | `x64-windows-static` | `VCPKG_TRIPLET=...` |
| Linux | `x64-linux` | `VCPKG_TRIPLET=...` |
| macOS | `x64-osx` | `VCPKG_TRIPLET=...` |

`vcpkg.json` в `compiler-codegen/` уже содержит зависимости `bdwgc` и `z3`.

### Установка Z3 (один раз на dev-машину)

**Windows:**
```
cd compiler-codegen
vcpkg install --triplet x64-windows-static --x-manifest-root=.
```

**Linux (Ubuntu/Debian):**
```
cd compiler-codegen
vcpkg install --triplet x64-linux --x-manifest-root=.
```

> **Альтернатива на Linux:** `apt install libz3-dev` не используется —
> `build.rs` ожидает vcpkg-путь. Используй vcpkg.

**macOS:**
```
cd compiler-codegen
vcpkg install --triplet x64-osx --x-manifest-root=.
```

### Сборка с Z3

```bash
# Только компилятор (без CLI):
cd compiler-codegen
cargo build --release --features z3-backend

# CLI (рекомендуется):
cd nova-cli
cargo build --release --features z3-backend
```

### Запуск Z3-тестов

```bash
NOVA_SMT_BACKEND=z3 ./compiler-codegen/target/release/nova-codegen \
    test-all --tests-dir nova_tests/contracts
```

Или через CLI:
```bash
NOVA_SMT_BACKEND=z3 ./nova-cli/target/release/nova test nova_tests/contracts/
```

Без env-var (`NOVA_SMT_BACKEND` не задан) — z3_* тесты автоматически
SKIP через маркер `// REQUIRES_SMT_BACKEND z3` в заголовке файла.

### Текущие z3-only тесты (9 штук, все SKIP без Z3)

| Тест | Что проверяет |
|---|---|
| `z3_linear_arith_positive` | LIA: `x + y == 10, x == 3 ⊢ y == 7` |
| `z3_implication_chain` | цепочка implic: `a⇒b, b⇒c, a ⊢ c` |
| `z3_unprovable_with_counterexample` | EXPECT_COMPILE_ERROR + cex |
| `z3_pure_view_axiom_positive` | axiom с pure_view через Z3 |
| `z3_pure_view_no_axiom_fail` | EXPECT_COMPILE_ERROR |
| `z3_axiom_alpha_rename` | binder renaming в forall |
| `z3_axioms_inconsistent_fail` | inconsistent axioms → EXPECT_COMPILE_ERROR |
| `composition_z3_positive` | composition через Z3 |
| `decreases_wf_z3_positive` | well-founded decreases |

### Блокеры для снятия SKIP

1. **Z3 не установлен** — самый частый случай. Решение: `vcpkg install`.
2. **`z3-backend` feature не включён** — бинарь собран без Z3. Решение:
   пересобрать с `--features z3-backend`.
3. **`NOVA_SMT_BACKEND` не задан** — test-runner видит trivial backend
   и SKIP'ает Z3-тесты. Решение: задать env var.
4. **CI не запускает Z3 job** — нет `contracts-z3` CI конфигурации.
   Это known gap, планируется закрыть отдельным PR.

### Что ещё не реализовано в Z3 backend

| Feature | Статус | Блокер |
|---|---|---|
| Generic axioms в SMT encoding | Unknown | V2 milestone |
| NIA (нелинейная арифметика) | Unknown (Z3 может, encoding не готов) | отдельный milestone |
| Quantifier patterns (Z3 triggers) | Частично (Forall без patterns) | Plan 33.3 full |
| Z3↔CVC5 cross-check | Не начато | Plan 33.3 Ф.12 |
| Incremental cache | Не начато | Plan 33.3 Ф.12 |
| Parallel verification | Не начато | Plan 33.3 Ф.12 |
| Strings beyond equality | Не начато | Plan 33.3 full |
| IEEE 754 FP | Не начато | Plan 33.3 full |
