# Plan 70.2 — LinkedList sum-type mono fixtures

## Scope

[Plan 70.2](../../docs/plans/70.2-linkedlist-sum-type-mono.md) bug:
Generic sum-type `LinkedList[T]` mono'd codegen — partial coverage.

## Fix (compiler-codegen/src/codegen/emit_c.rs)

**Drain timing fix:** добавлен `self.drain_generic_type_worklist()?;`
после emit_test loop. Без этого drain instances enqueued via
`try_infer_variant_mono_args` (Cons-constructor type inference)
никогда не processed → `Nova_LinkedList____nova_int` struct decl
не генерируется → `use of undeclared identifier` в C → CC-FAIL.

## Fixtures

| File | Coverage |
|---|---|
| `f1_linkedlist_int_pos.nv` | LinkedList[int] data construction (Nil + Cons), pattern matching (len/sum), basic mono ✅ |

## Deferred (method-dispatch на mono'd sum-type)

**Original f2 scenario** (deleted в этом коммите — broken, separate scope):
```nova
fn LinkedList[T] @len() -> int => match @ {
    Nil => 0
    Cons(_, rest) => 1 + rest.@len()
}
test "..." {
    let l = Cons(1, Cons(2, Nil))
    assert(l.@len() == 2)  // <-- @len method dispatch on mono'd LinkedList[int]
}
```

**Bugs (требуют отдельный refactor):**

1. **Unit-variant inference (Plan 48 Ф.7.4 V2 deferred):**
   `Nil` constructor falls back to erased `nova_make_LinkedList_Nil()`
   (returns `Nova_LinkedList*` — base) instead of mono'd
   `nova_make_Nova_LinkedList____nova_int_Nil()`. Mixing erased +
   mono'd → type mismatch в Cons constructor argument typing.
   See `try_infer_variant_mono_args` line 6620 «Unit-variant inference
   would need usage-context propagation (deferred to V2)».

2. **@method dispatch:** `l.@len()` codegen эмитит literal `l->@len()`
   в C (with `@`!). Method-name resolution для mono'd sum-type receiver
   не desugarит `@` префикс и не routes к
   `Nova_LinkedList____nova_int_method_len`.

3. **Method-name collision:** Plan 70.2 doc упоминает collision с
   `Nova_StringBuilder_method_append` — generic-method dispatch
   pathway shares name space с unrelated types.

Per Plan 70.2 estimate (3-5 dev-days): closure для full method-dispatch
support требует significant mono pass refactor — отложено в отдельный
plan если когда-нибудь needed. Текущий fix unblocks LinkedList data
usage (без instance methods); пользователь может писать free fns с
explicit type annotations для workaround.

## Acceptance

- ✅ `f1_linkedlist_int_pos` PASS — LinkedList[int] data + free-fn pattern matching
- 🔄 method-dispatch deferred — see «Deferred» above
- ✅ 0 regressions vs Plan 70.1 baseline в полном nova test
