# Contracts and formal verification in Nova

**English** | [Русский](contracts.ru.md)

Nova's contract system lets you state what a function **requires** and
**ensures**, then verifies those claims at compile time via an SMT
solver. Proven contracts are erased in release builds — zero runtime
cost. Unproven ones fall back to runtime assertions in debug.

Spec: [D24](../spec/decisions/09-tooling.md#d24-стратегия-smt-проверки-контрактов)
(SMT strategy) ·
[D111](../spec/decisions/09-tooling.md#d111-assume--assert_static--trusted-external)
(`assume` / `assert_static` / `#trusted`) ·
[D112](../spec/decisions/09-tooling.md#d112-bounded-quantifiers-forallexists-по-коллекции)
(bounded quantifiers) ·
[D116](../spec/decisions/09-tooling.md#d116-z3-backend-через-собственные-ffi-биндинги)
(Z3 backend).

---

## Contents

- [Quickstart](#quickstart)
- [Contract clauses](#contract-clauses)
  - [`requires`](#requires)
  - [`ensures` and `result`](#ensures-and-result)
  - [`old(...)` in `ensures`](#old-in-ensures)
  - [`decreases`](#decreases)
- [Verification attributes](#verification-attributes)
  - [`#verify`](#verify)
  - [`#pure`](#pure)
  - [`#unverified`](#unverified)
  - [`#must_verify`](#must_verify)
  - [`#trusted`](#trusted)
- [`#pure` function composition](#pure-function-composition)
- [Proof helpers](#proof-helpers)
  - [`assert_static`](#assert_static)
  - [`assume`](#assume)
  - [`calc { ... }`](#calc--)
- [Loop invariants](#loop-invariants)
- [Lemmas and `apply`](#lemmas-and-apply)
- [Opaque functions and `reveal`](#opaque-functions-and-reveal)
  - [`#opaque`](#opaque)
  - [`reveal fn_name`](#reveal-fn_name)
  - [`#fuel(n)`](#fueln)
- [Bounded quantifiers](#bounded-quantifiers)
- [Bit-vectors and overflow](#bit-vectors-and-overflow)
  - [`#nooverflow`](#nooverflow)
- [Trusted external functions](#trusted-external-functions)
- [SMT backend selection](#smt-backend-selection)
- [Cross-check verification (Z3 ↔ CVC5)](#cross-check-verification-z3--cvc5)
- [Contract syntax grammar](#contract-syntax-grammar)
- [Error reference](#error-reference)
- [Bootstrap limitations](#bootstrap-limitations)
- [Related documents](#related-documents)

---

## Quickstart

```nova
// Simple precondition + postcondition.
#verify
fn withdraw(balance int, amount int) -> int
    requires amount > 0 && amount <= balance
    ensures  result == balance - amount
    ensures  result >= 0
{
    balance - amount
}

test "contracts quickstart: withdraw" {
    assert(withdraw(100, 30) == 70)
    assert(withdraw(50, 50)  == 0)
}
```

```nova
// REQUIRES_SMT_BACKEND z3

// Opaque helper + reveal in caller — Z3 proves the stronger contract.
#opaque #pure
fn double(x int) -> int
    requires x >= 0
    ensures  result >= 0
=> x * 2

#verify
fn caller_with_reveal(n int) -> int
    requires n >= 0
    ensures  result == n * 2
{
    reveal double
    double(n)
}

test "contracts quickstart: opaque + reveal" {
    assert(double(5) == 10)
    assert(caller_with_reveal(7) == 14)
}
```

---

## Contract clauses

Contract clauses appear between the parameter list and the `{` body
(or `=>` expression body). Multiple clauses of the same kind are
allowed and are conjoined.

### `requires`

A precondition. The SMT solver **assumes** it holds when verifying
the body. Callers must satisfy it.

```nova
#verify
fn safe_div(a int, b int) -> int
    requires b != 0
    ensures  result * b == a - (a % b)
{
    a / b
}
```

Multiple `requires` clauses are equivalent to a single conjunction:

```nova
#verify
fn clamp(x int, lo int, hi int) -> int
    requires lo <= hi
    ensures  result >= lo && result <= hi
{
    if x < lo { lo } else if x > hi { hi } else { x }
}
```

### `ensures` and `result`

A postcondition. `result` refers to the return value of the function.
Multiple `ensures` clauses are all independently checked.

```nova
#verify
fn abs_val(x int) -> int
    ensures result >= 0
    ensures result == x || result == -x
{
    if x >= 0 { x } else { -x }
}
```

### `old(...)` in `ensures`

`old(expr)` captures the value of an expression **at the function
entry point**, before the body runs. Useful for mutation contracts.

```nova
#verify
fn increment(mut n int) -> int
    ensures result == old(n) + 1
{
    n = n + 1
    n
}
```

### `decreases`

Proves termination of recursive functions. The expression must
**strictly decrease** on every recursive call. The SMT solver checks
this as a well-foundedness obligation.

```nova
fn factorial(n int) -> int
    requires n >= 0
    decreases n
=> if n == 0 { 1 } else { n * factorial(n - 1) }

fn fib(n int) -> int
    requires n >= 0
    decreases n
=> if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
```

---

## Verification attributes

### `#verify`

Marks a function for SMT verification. The compiler encodes the
function body and all contracts as an SMT query and asks the solver.
If the solver proves the contracts, they are erased in release. If
not — warning `W2402` + runtime fallback in debug.

```nova
#verify
fn sum_nonneg(a int, b int) -> int
    requires a >= 0
    requires b >= 0
    ensures  result >= 0
{
    a + b
}
```

### `#pure`

Marks a function as **pure** — no side effects, no effects in its
row. Pure functions can be called freely inside contract expressions
(`requires`/`ensures`/`invariant`), where effectful calls are
forbidden.

```nova
#pure
fn is_positive(x int) -> bool => x > 0

#verify
fn safe_log(x int) -> int
    requires is_positive(x)    // #pure call allowed in contract
    ensures  result >= 0
{
    x - 1
}
```

### `#unverified`

Opts out of SMT verification. Contracts are kept as **runtime
assertions** in debug; skipped in release. Use for contracts the
solver cannot handle (non-linear arithmetic, string predicates, etc.).

```nova
#unverified
fn safe_double(x int) -> int
    requires x > 0
    ensures  result == x * 2
=> x * 2
```

### `#must_verify`

The inverse of `#unverified`. If the SMT solver cannot prove the
contract within the timeout, compilation **fails** with an error
(no runtime fallback). Use for safety-critical code.

```nova
#must_verify
fn transfer_total(from_bal int, to_bal int, amount int) -> int
    requires amount > 0 && amount <= from_bal
    ensures  result == from_bal + to_bal
{
    (from_bal - amount) + (to_bal + amount)
}
```

### `#trusted`

Used in two contexts:

**1. `with #trusted`** on a handler binding — skips axiom verification
for that handler, accepts contracts as axioms on faith:

```nova
with #trusted Log = handler Log {
    Write(msg) { if msg > 0 { buf = msg } else { buf = 0 } }
    last() => buf
} { ... }
```

**2. `#trusted` on a function** containing `assume` — suppresses the
`trust-introduced` warning:

```nova
#trusted
fn call_ffi() -> int {
    let result = extern_fn()
    assume result >= 0    // documented FFI postcondition
    result
}
```

---

## `#pure` function composition

`#pure` functions compose freely in contract expressions. This lets
you build reusable predicates:

```nova
#pure
fn in_range(x int, lo int, hi int) -> bool => x >= lo && x <= hi

#verify
fn clamp_tight(x int) -> int
    ensures in_range(result, 0, 100)
{
    if x < 0 { 0 } else if x > 100 { 100 } else { x }
}
```

Non-pure functions in contracts are a compile error:

```
error: effectful function call in contract expression
  contracts require #pure or side-effect-free expressions
```

---

## Proof helpers

### `assert_static`

Inserts an **intermediate proof step** visible to the SMT solver.
Breaks a complex contract into smaller, independently verifiable
facts. In debug — runtime check; in release — erased after being
proven.

```nova
#verify
fn transfer(from int, to int, amount int) -> int
    requires amount > 0 && amount <= from
    ensures  result == from + to
{
    assert_static from - amount >= 0    // intermediate fact
    (from - amount) + (to + amount)
}
```

### `assume`

Injects a fact into the SMT context **without proof**. Use for
FFI postconditions or OS invariants the solver cannot see. Generates
warning `trust-introduced` unless inside a `#trusted` function.

```nova
#trusted
fn read_positive_from_device() -> int {
    let v = device_read()
    assume v >= 0    // documented hardware guarantee
    v
}
```

### `calc { ... }`

A structured **chain of equalities** (or inequalities) that guides
the SMT solver step by step. Each step `== expr;` asserts equality
with the previous line. The solver checks each step independently.

```nova
#verify
fn double_is_double(x int) -> int
    ensures result == x * 2
{
    calc {
        x * 2;
        == x * 2;
    }
    x * 2
}
```

More complex chains can include arithmetic identities:

```nova
#verify
fn add_assoc_proof(a int, b int, c int) -> bool
    ensures result == true
{
    calc {
        (a + b) + c;
        == a + (b + c);    // associativity — Z3 proves each step
    }
    true
}
```

---

## Loop invariants

An `invariant` clause inside a loop body asserts a condition that
holds at **every iteration entry**. The SMT solver checks:
1. The invariant holds before the loop (initialization).
2. If the invariant holds at iteration start and the loop condition
   holds, then the invariant holds at the end of the body (inductive step).

```nova
// REQUIRES_SMT_BACKEND z3

#verify
fn sum_nonneg_array(n int) -> int
    requires n >= 0
    ensures  result >= 0
{
    let mut sum = 0
    let mut i = 0
    while i < n {
        invariant sum >= 0
        invariant i >= 0
        sum = sum + i
        i = i + 1
    }
    sum
}
```

The `decreases` clause can also appear on a loop to prove termination:

```nova
#verify
fn countdown(n int) -> int
    requires n >= 0
    ensures  result == 0
{
    let mut k = n
    while k > 0 {
        invariant k >= 0
        decreases k
        k = k - 1
    }
    k
}
```

---

## Lemmas and `apply`

A **lemma** is a `#verify` function whose purpose is to establish a
mathematical fact — it exists for its proof, not its runtime value.
Typically returns `bool` and has `ensures result == true`.

```nova
// REQUIRES_SMT_BACKEND z3

#verify
lemma add_comm(a int, b int) -> bool
    ensures result == true
{
    a + b == b + a
}
```

The `apply` statement injects the postcondition of a lemma as a fact
into the current SMT context. This lets you chain lemma results:

```nova
#verify
fn use_commutativity(a int, b int) -> int
    requires a >= 0 && b >= 0
    ensures  result == b + a
{
    apply add_comm(a, b)    // injects: a + b == b + a
    a + b
}
```

**Rules:**
- `apply` only works inside `#verify` functions.
- The lemma must already be proven (i.e., `#verify` and its contracts
  checked without error).
- Duplicate `apply` of the same lemma in the same scope is a warning
  `W2402`.

---

## Opaque functions and `reveal`

### `#opaque`

`#opaque` on a `#pure` function hides its body from the SMT solver.
The solver treats it as an **uninterpreted function** (UF): it knows
the `requires`/`ensures` contracts but not the implementation.

This prevents matching-loop divergence in recursive functions and
gives control over which callers get access to the body-level proof:

```nova
// REQUIRES_SMT_BACKEND z3

#opaque #pure
fn double(x int) -> int
    requires x >= 0
    ensures  result >= 0
=> x * 2
```

Without `reveal`, a caller can only use the declared `ensures`
(result ≥ 0), not that `result == x * 2`:

```nova
// EXPECT_COMPILE_ERROR contract violation

#verify
fn caller_no_reveal(n int) -> int
    requires n >= 0
    ensures  result == n * 2    // Z3 cannot prove — body is hidden
{
    double(n)
}
```

### `reveal fn_name`

`reveal fn_name` injects the body axiom of an `#opaque` function into
the current SMT scope. After `reveal`, the solver can use the full
body for proofs in that function:

```nova
// REQUIRES_SMT_BACKEND z3

#verify
fn caller_with_reveal(n int) -> int
    requires n >= 0
    ensures  result == n * 2
{
    reveal double       // body axiom injected: double(x) == x * 2
    double(n)
}
```

**Scope:** `reveal` is function-local. It does not affect other
callers.

**Warnings:**
- `W2402` — `reveal` in a non-`#verify` function (no SMT context).
- `W2402` — duplicate `reveal` for the same name in the same scope.
- `W2403` — `reveal` for a function that is not `#opaque`.

### `#fuel(n)`

`#fuel(n)` on an `#opaque #pure` recursive function enables **N
levels of unrolling** in the SMT scope after `reveal`. Without fuel,
the opaque body axiom is non-recursive. With `#fuel(2)`, the solver
gets two unrolling levels — enough to prove properties of small
concrete inputs:

```nova
// REQUIRES_SMT_BACKEND z3

#opaque #pure #fuel(2)
fn count_down(n int) -> int
    requires n >= 0
    ensures  result >= 0
=>
    if n == 0 { 0 } else { 1 + count_down(n - 1) }

#verify
fn prove_base_case() -> int
    ensures result == 0
{
    reveal count_down
    count_down(0)      // fuel unrolls: count_down(0) == 0
}

#verify
fn prove_one_step() -> int
    ensures result == 1
{
    reveal count_down
    count_down(1)      // fuel unrolls: 1 + count_down(0) == 1
}
```

The fuel chain works by creating `N` intermediate UFs and chaining
them via axioms, following Dafny's approach.

---

## Bounded quantifiers

Nova supports **bounded quantifiers** — `forall`/`exists` over
concrete collections or index ranges. Unbounded universal quantifiers
are a compile error.

```nova
// REQUIRES_SMT_BACKEND z3

#verify
fn all_nonneg_sum(a int, b int, c int) -> bool
    requires a >= 0 && b >= 0 && c >= 0
    ensures  result == true
{
    a + b + c >= 0
}
```

Syntax for bounded quantifiers in contracts:

```nova
// forall — universal
requires forall i in 0..xs.len() : xs[i] >= 0

// exists — existential
ensures  exists i in 0..result.len() : result[i] == target
```

The collection after `in` must be an iterable (`[]T`, range, set,
map). The body must be `bool` and `#pure`.

---

## Bit-vectors and overflow

Sized integer types — `u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32` —
are encoded into the SMT **bit-vector theory** instead of unbounded
integers. This gives precise machine semantics: arithmetic wraps around
(two's complement), and bitwise operators are reasoned about exactly.

```nova
// REQUIRES_SMT_BACKEND z3

#verify
fn low_byte(x u32) -> u32
    ensures result <= 255 as u32
=> x & 255 as u32
```

The plain `int` type stays an **unbounded** mathematical integer — it is
not a bit-vector. Use `int` for general-purpose arithmetic; use sized
types for low-level, packed, crypto, or FFI code where bit-width matters.

**`int` overflow is a panic.** Signed `int` arithmetic (`+`, `-`, `*`)
that would exceed the 64-bit range **panics** at runtime — it never
silently wraps. This is what makes verification of `int` contracts
sound: the verifier reasons about `int` as an unbounded mathematical
integer, and a proven `ensures result == a + b` holds for every value
the function actually returns — because if `a + b` would overflow, the
function panics instead of returning a wrong (wrapped) result. Sized
integer types wrap instead of panicking (see above); reach for
`#nooverflow` on them when wrap-around is not acceptable.

Bitwise operators `&`, `|`, `^`, `<<`, `>>` are available in contracts
on sized-integer operands (they remain unsupported on `int`).

**Signedness.** Unsigned types (`u8`/`u16`/`u32`/`u64`) and signed types
(`i8`/`i16`/`i32`) differ in comparison, division, remainder and
right-shift. The verifier picks the correct operator from the parameter
type: `i32` comparisons are signed (`-1 < 0` holds), `u32` comparisons
are unsigned (`0xFFFFFFFF > 0`). Signed division rounds toward zero;
`>>` on a signed value is an arithmetic shift.

**Casts between sized types.** `x as u32` resizes a bit-vector: a wider
target zero-extends an unsigned source and sign-extends a signed source;
a narrower target truncates the low bits. For example `(b as u32)` where
`b : u8` is always `<= 255`, and `(x as u8)` keeps only the low byte.

### `#nooverflow`

By default, sized-integer arithmetic **wraps around** silently. The
`#nooverflow` attribute makes the verifier emit an extra proof
obligation for every `+`, `-`, `*` in the function body: the operation
must not overflow the type. An unprovable obligation is a compile error.

```nova
// REQUIRES_SMT_BACKEND z3

#nooverflow #verify
fn safe_add_u32(a u32, b u32) -> u32
    requires a <= 1000 as u32 && b <= 1000 as u32
    ensures  result == a + b
=> a + b
```

Here the precondition bounds `a` and `b` so their sum cannot exceed
`2^32 - 1` — the overflow obligation is discharged. Without a bounding
`requires`, `a + b` could overflow and `#nooverflow` rejects the
function at compile time.

`#nooverflow` requires an SMT backend with bit-vector support
(`REQUIRES_SMT_BACKEND z3`); the trivial backend reports the
bit-vector theory as unsupported.

---

## Trusted external functions

`external fn` with contracts requires `#trusted`. The contracts are
registered as **axioms** — callers receive the `ensures` as
assumptions without proof. The compiler does not verify the body
(there is no Nova body to check).

```nova
#trusted
external fn libc_strlen(s str) -> int
    requires s.is_valid_cstring()
    ensures  result >= 0

#verify
fn use_strlen(s str) -> int
    requires s.is_valid_cstring()
    ensures  result >= 0
{
    libc_strlen(s)    // ensures from #trusted axiom injected
}
```

---

## SMT backend selection

Nova has two verification backends:

| Backend | Activated by | Capabilities |
|---|---|---|
| **Trivial** | default | Constant-folding, linear bounds on single binary ops. Fast, no Z3 dependency. |
| **Z3** | `--smt-backend=z3` or `NOVA_SMT_BACKEND=z3` | Full LIA + EUF + bounded arrays. Required for opaque/reveal, complex arithmetic chains, loop invariants. |

Tests that require Z3 use the marker `// REQUIRES_SMT_BACKEND z3` —
the test runner skips them when Z3 is unavailable.

Timeout per function: default 2 seconds. Override locally:

```nova
#verify_timeout(10000)
#verify
fn complex_proof(x int) -> int
    ...
```

---

## Cross-check verification (Z3 ↔ CVC5)

Cross-check is a **CI-only soundness safety net**: it re-runs every
verification condition through two *independent* solver paths and fails
the build if their definite answers disagree. It is the second line of
defence after the soundness-regression suite (Plan 33.8 Ф.7) — the
regression suite catches *known* bug classes, cross-check catches
*unknown* ones.

The two paths are deliberately independent:

- **Z3** — through the FFI backend.
- **CVC5** — through a *textual* SMT-LIB v2 script fed to the `cvc5`
  binary as a subprocess.

The textual path shares no code with the Z3-FFI translation, so it is
also a second independent *encoder*. An encoding bug that silently
dropped a formula on the Z3 side (the class of bug found in Plan 33.8
Ф.6.2) would be caught here even without a second solver.

### Running it

```sh
# Build with the Z3 backend, install cvc5 on PATH (or point NOVA_CVC5
# at the binary), then:
NOVA_CROSSCHECK=1 nova test . --filter contracts
```

`NOVA_CROSSCHECK=1` takes priority over `NOVA_SMT_BACKEND`. Normal
compilation (`nova build` / `nova check`) is **unaffected** — it keeps
using a single solver, so developer compile times do not grow.

If `cvc5` is not found the run degrades gracefully to "Z3 only" with a
warning — cross-check simply does not happen, the build does not break.

### What counts as a disagreement

Only a **definite** disagreement gates: one path says `Proven` (unsat),
the other says `Disproved` (sat). Any `Unknown` / timeout on either side
is normal (the solvers have different performance profiles) and is
**not** an error.

A disagreement is reported as compile error `E2412` with the function,
the VC, both verdicts, the counterexample, and the SMT-LIB script for
manual reproduction. It is soundness-critical: one path produced a wrong
answer, so the verifier may have declared a false `Proven`.

### CI gate

The `contracts-crosscheck` workflow runs the whole contracts corpus
under `NOVA_CROSSCHECK=1` and requires **0 disagreements** for merge.
`NOVA_CROSSCHECK_LOG=<file>` makes every disagreement append a line to
that file (the corpus is compiled process-per-file, so the file is the
cross-process aggregation point the gate checks).

---

## Contract syntax grammar

```
contract-clause  = requires-clause
                 | ensures-clause
                 | decreases-clause

requires-clause  = 'requires' bool-expr
ensures-clause   = 'ensures'  bool-expr
decreases-clause = 'decreases' expr

fn-contracts     = contract-clause*

loop-invariant   = 'invariant' bool-expr
loop-decreases   = 'decreases' expr

calc-block       = 'calc' '{' calc-step+ '}'
calc-step        = expr ';'
               | ('==' | '<=' | '>=' | '<' | '>') expr ';'

reveal-stmt      = 'reveal' ident
apply-stmt       = 'apply' ident '(' expr-list ')'
assert-static    = 'assert_static' bool-expr
assume-stmt      = 'assume' bool-expr

quantifier-expr  = 'forall' ident 'in' expr ':' bool-expr
                 | 'exists' ident 'in' expr ':' bool-expr

old-expr         = 'old' '(' expr ')'
result-ref       = 'result'                  // only in ensures
```

**Attribute summary:**

| Attribute | On | Meaning |
|---|---|---|
| `#verify` | fn | Enable SMT verification |
| `#pure` | fn | Pure (no effects), usable in contracts |
| `#unverified` | fn | Skip SMT, keep as runtime check |
| `#must_verify` | fn | Require SMT proof — compile error if unprovable |
| `#trusted` | fn / `with` binding | Accept contracts as axioms without proof |
| `#opaque` | `#pure` fn | Hide body from SMT; require `reveal` to expose |
| `#fuel(n)` | `#opaque #pure` fn | N-level recursive unrolling after `reveal` |
| `#verify_timeout(ms)` | `#verify` fn | Override per-function SMT timeout |

---

## Error reference

| Code | Message | Cause |
|---|---|---|
| `W2401` | `contract not verified statically` | SMT returned Unknown or timed out; falls back to runtime check |
| `W2402` | `unverified: ...` | Various: dead lemma, duplicate apply/reveal, reveal in non-verify context |
| `W2403` | `opaque: ...` | `reveal` for non-opaque fn, `#fuel(0)`, dead `#opaque` (never revealed) |
| `E2401` | `unsupported expression in contract` | Effectful call, match, lambda, or non-`#pure` in contract position |
| `E2402` | `contract violation` | SMT disproved the contract (found counterexample) |
| `E2412` | `cross-check disagreement` | Z3 and CVC5 returned opposite definite verdicts for a VC (cross-check mode only) |
| `trust-introduced` | warning | `assume` outside `#trusted` context |

---

## Bootstrap limitations

| What does not work / is deferred | Plan |
|---|---|
| `#must_verify_module` — strict mode for an entire module | [D113](../spec/decisions/09-tooling.md#d113) (Plan 33.3 Ф.13, V2) |
| SMT cache + incremental verification | [D114](../spec/decisions/09-tooling.md#d114) (V2) |
| Parallel verification via `rayon` | [D114](../spec/decisions/09-tooling.md#d114) (V2) |
| Loop invariants with Z3 — full inductive reasoning | Plan 33.x V2 |
| `forall`/`exists` in loop invariants | Plan 33.x V2 |
| Effect-aware contracts (`ensures Db.balance(...) == ...`) | [D24](../spec/decisions/09-tooling.md#d24) / [D120](../spec/decisions/04-effects.md#d120) (partial in V1) |
| Recursive `lemma` bodies (structural induction) | Research / V3 |
| Non-linear arithmetic in contracts | Z3 can sometimes handle; no static guarantee |
| Floating-point reasoning | Not planned |
| String predicates beyond `len()` and equality | Not planned for V1 |
| `#fuel(0)` is a warning (`W2403`) — use omitting `#fuel` instead | By design |

---

## Related documents

- [`spec/decisions/09-tooling.md`](../spec/decisions/09-tooling.md) —
  D24 / D89 / D111 / D112 / D113 / D114 / D116 (contracts, SMT, test tooling)
- [`spec/decisions/04-effects.md`](../spec/decisions/04-effects.md) —
  D120 (`#pure` views + axioms), D115 (axiom binders)
- [`docs/plans/33.9-opaque-reveal-fuel.md`](plans/33.9-opaque-reveal-fuel.md) —
  `#opaque` / `reveal` / `#fuel(n)` implementation (Plan 33.9)
- [`docs/plans/33.14-z3-cvc5-crosscheck.md`](plans/33.14-z3-cvc5-crosscheck.md) —
  Z3 ↔ CVC5 cross-check implementation (Plan 33.14)
- [`nova_tests/contracts/`](../nova_tests/contracts/) —
  ~280 contract verification tests
- [`nova_tests/doc/f23_contracts_positive.nv`](../nova_tests/doc/f23_contracts_positive.nv) —
  basic contracts doc-example
- [`nova_tests/doc/f24_infer_contracts_positive.nv`](../nova_tests/doc/f24_infer_contracts_positive.nv) —
  inferred contracts doc-example
- [`nova_tests/doc/f25_mutation_contracts_positive.nv`](../nova_tests/doc/f25_mutation_contracts_positive.nv) —
  mutation contracts doc-example
- [`nova_tests/expected_runtime/`](../nova_tests/expected_runtime/) —
  runtime contract violation tests (`contracts_*.nv`)
