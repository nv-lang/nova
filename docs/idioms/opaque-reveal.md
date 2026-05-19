# Opaque / reveal / fuel — controlled SMT unfolding

> **Что это:** три construct'а для контроля как Z3 раскрывает recursive
> `#pure` функции в верификации. Спецификация:
> [D130](../../spec/decisions/09-tooling.md#d130-opaque-reveal-fuel-controlled-smt-unfolding).
>
> **TL;DR:** `#opaque` делает `#pure fn` непрозрачной для Z3 (uninterpreted
> function). `reveal X` раскрывает body в одной fn. `#fuel(n)` — auto-unfold
> на n уровней рекурсии.

---

## Когда нужны opaque/reveal

### Симптом: matching loop / timeout 2000ms

```nova
#pure fn list_len[T](l: List[T]) -> int =>
    match l {
        Nil => 0
        Cons(_, tail) => 1 + list_len(tail)
    }

#verify
fn append_preserves_len[T](xs: List[T], ys: List[T])
    ensures list_len(append(xs, ys)) == list_len(xs) + list_len(ys)
{
    // Z3 разворачивает list_len(append(xs, ys)) рекурсивно
    // → matching loop → timeout 2000ms
}
```

Z3 пытается развернуть `list_len` на каждом уровне рекурсии; для unbounded
list — divergence. **Решение:**

```nova
#opaque                    // ← теперь list_len — UF в SMT scope
#pure fn list_len[T](l: List[T]) -> int =>
    match l {
        Nil => 0
        Cons(_, tail) => 1 + list_len(tail)
    }

#verify
fn append_preserves_len[T](xs: List[T], ys: List[T])
    ensures list_len(append(xs, ys)) == list_len(xs) + list_len(ys)
{
    reveal list_len        // ← Z3 видит body здесь, в этой fn
    reveal append          // ← аналогично

    // Доказательство по индукции, Z3 справляется
}
```

### Когда **НЕ** использовать opaque

- `#pure fn` без recursion / non-recursive helpers — inlining body
  дешевле UF + axiom. `#opaque` добавляет noise без пользы.
- Если функция вызывается из множества `#verify` контекстов — reveal
  везде надоедает. Лучше оставить inline'нутой, поднять `#proof_budget`.
- Plotly-style "защита от usage" — `#opaque` это **proof tooling**, не
  visibility control. Не используйте для скрытия implementation от
  callers (для этого есть `#hide_doc`, private fns).

---

## Workflow: пошаговая proof construction

Reveal — **локальная statement**, можно интерливать с proof steps:

```nova
#opaque #pure fn list_len[T](l) -> int => /* recursive */

#opaque #pure fn append[T](xs, ys) -> List[T] => /* recursive */

#verify
fn append_len_distributes[T](xs: List[T], ys: List[T])
    ensures list_len(append(xs, ys)) == list_len(xs) + list_len(ys)
{
    // Step 1: base case xs = Nil
    if xs.is_nil() {
        reveal append           // append(Nil, ys) = ys
        reveal list_len         // list_len(Nil) = 0
        return
    }

    // Step 2: inductive case xs = Cons(h, t)
    reveal append               // append(Cons(h,t), ys) = Cons(h, append(t, ys))
    reveal list_len             // list_len = 1 + ...

    // Indirect: induction hypothesis на t покрывается opaque'м,
    // т.к. рекурсивный case уже использует reveal'нутый body.
}
```

В Dafny равнозначно `reveal X();` calls в proof body. В Verus —
`reveal!(X)` macro. Nova `reveal` чище — не выглядит как call /
macro invocation.

---

## Fuel — auto-unfold для предсказуемой recursion

Если функция вызывается часто и **всегда хочется одинаковую глубину**
раскрытия — `#fuel(n)` избавляет от повторных `reveal`:

```nova
#opaque
#fuel(2)                    // ← auto-unfold 2 уровня
#pure fn list_len[T](l) -> int =>
    match l { Nil => 0, Cons(_, t) => 1 + list_len(t) }

#verify
fn small_list_facts[T](xs: List[T])
    requires xs.len() <= 2
    ensures list_len(xs) <= 2
{
    // No reveal needed — fuel(2) разворачивает 2 уровня автоматически.
}

#verify
fn deep_list_proof[T](xs: List[T])
    requires xs.len() >= 5
    ensures list_len(xs) >= 5
{
    reveal list_len            // ← fuel(2) недостаточно, нужен explicit
}
```

### Fuel chain ограничения

- `n = 0` — fully opaque (default). Нужен reveal.
- `n = 1..3` — типичный sweet spot. Достаточно для simple proofs,
  не вызывает matching loop.
- `n >= 10` — обычно ошибка дизайна. Если fn требует deep auto-unfold,
  она может быть слишком сложной для verification — упростите body или
  выделите helpers.
- `n > 100` — parse error.

### Композиция с `#proof_budget`

`#fuel` и `#proof_budget` контролируют **разные axes**:

| Attribute | Контролирует |
|---|---|
| `#fuel(n)` | Размер proof context (сколько axioms Z3 видит) |
| `#proof_budget(timeout_ms = N)` | Время доказательства |

Используйте вместе:

```nova
#opaque
#fuel(3)
#pure fn fib(n: int) -> int =>
    if n <= 1 { n } else { fib(n-1) + fib(n-2) }

#verify
#proof_budget(timeout_ms = 10000)   // long-running proof
fn fib_grows(n: int)
    requires n >= 0
    ensures fib(n) >= 0
{ /* ... */ }
```

---

## Best practices

### ✅ Делайте

- **Opaque для recursive `#pure fn`** которые используются в spec
  (ensures/requires/invariant/axiom).
- **Fuel(1) или fuel(2)** для частоиспользуемых recursive helpers —
  избавляет от повторного `reveal` в каждой fn.
- **Reveal как первый statement** в proof body — читателю сразу видно
  какие fns раскрыты в этом scope.
- **Группируйте opaque fns в одном module section** с комментарием
  «// SMT-opaque helpers — reveal explicitly».

### ❌ Не делайте

- **Не маркируйте все `#pure fn` как `#opaque`** — это не безопасность,
  это proof tooling. Inline non-recursive helpers полезен.
- **Не используйте `#fuel` без `#opaque`** — компилятор выдаст W2403
  (fuel без opaque — noise).
- **Не пишите `#fuel(0)` явно** — это default; компилятор предупредит
  W2403.
- **Не reveal'те fn которая не помечена `#opaque`** — компилятор
  выдаст W2403 (reveal-non-opaque).
- **Не reveal'те одну fn дважды** в одном scope — W2402 (dup-reveal).
- **Не ожидайте что reveal в fn A** повлияет на fn B — каждая fn имеет
  независимый SMT scope. Cross-fn reveal — V3 feature (cross-module
  reveal деференён).

---

## Anti-patterns

### "Opaque to hide implementation"

```nova
// ❌ ПЛОХО — opaque не visibility control
#opaque
#pure fn internal_helper(x int) -> int => x * 2 + 1

// ✅ ХОРОШО — private fn (не export'ить)
#pure fn internal_helper(x int) -> int => x * 2 + 1
```

`#opaque` влияет на **верификатор**, не на caller resolution. Для
скрытия от callers — не export'ить или использовать `#hide_doc`.

### "Fuel поднимет любой proof"

```nova
// ❌ ПЛОХО — exponentional blowup
#opaque #fuel(20) #pure fn ackermann(m, n) -> int => ...

#verify
fn proof_with_high_fuel() { ... }   // Z3 умрёт от context explosion
```

Высокий fuel = большой proof context = долгий solve. `fuel(20)` для
ackermann практически гарантирует timeout. Лучше: явный `reveal` +
narrow ensures.

### "Opaque заменяет lemma"

```nova
// ❌ ПЛОХО — нет abstract спецификации
#opaque #pure fn complex_invariant(x) -> bool => /* 50 строк */

#verify
fn uses_invariant(x int) ensures complex_invariant(x) { ... }
// Z3 видит UF, не может доказать ensures
```

Opaque fn без явных lemmas (или без reveal) — UF без свойств. Z3 не
может ничего вывести. Либо reveal в каждом use site, либо expose
properties как `#pure` axioms:

```nova
// ✅ ХОРОШО — opaque + axiomatic spec
#opaque #pure fn complex_invariant(x) -> bool => /* 50 строк */

// Axiomatic spec доступна каждому caller'у.
#pure fn complex_invariant_holds_for_positive(x int) -> bool
    ensures result == true
    requires x > 0
=> complex_invariant(x) // reveal here, prove the axiom
```

---

## Industry comparison

| Аспект | Dafny | Verus | F\* | Nova |
|---|---|---|---|---|
| Opaque syntax | `{:opaque}` magic-comment | `#[verifier::opaque]` | `[@opaque_to_smt]` | `#opaque` attribute |
| Reveal syntax | `reveal X();` call-like | `reveal!(X)` macro | `unfold X` statement | `reveal X` statement |
| Fuel syntax | `{:fuel X, n}` per-call | (нет аналога) | implicit unifier hints | `#fuel(n)` attribute |
| Composition с budget | Separate global setting | Annotation-based | Implicit | `#fuel` + `#proof_budget` orthogonal |
| Cross-module reveal | Да | Да (с module qualifier) | Да | Нет (V3 deferred) |

**Nova сравнение:**

- **Лучше Dafny:** нет magic comments, attribute namespace consistent
  с D105.
- **Лучше Verus:** нет macros (`reveal!` macro looks like FFI),
  `reveal` — statement не invocation.
- **Хуже F\* / Liquid Haskell:** F\* имеет sophisticated unifier hints,
  Liquid Haskell умеет refinement inference. Nova V1 — explicit only.

---

## См. также

- [D130 spec](../../spec/decisions/09-tooling.md#d130-opaque-reveal-fuel-controlled-smt-unfolding)
- [Plan 33.9](../plans/33.9-opaque-reveal-fuel.md) — implementation
- [Plan 33.6](../plans/33.6-contracts-production-hardening.md) —
  production hardening (proof infrastructure foundation)
- [D105 doc-атрибуты](../../spec/decisions/09-tooling.md#d105-doc-атрибуты)
  — общий namespace
