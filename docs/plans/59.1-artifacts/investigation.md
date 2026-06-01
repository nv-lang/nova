// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 59.1 Ф.0 — Investigation findings (2026-06-01)

## Repro

[`nova_tests/plan59_1/repro_dup_T.nv`](../../../nova_tests/plan59_1/repro_dup_T.nv):

```nova
module plan59_1.repro_dup_T

fn[T] dup(v T) -> (T, T) => (v, v)

test "dup[int] returns pair" {
    ro (a, b) = dup[int](42)
    assert(a == 42)
    assert(b == 42)
}
```

CC-FAIL: `error: initializing '_NovaTuple2' with an expression of incompatible type`.

## Generated .c (broken)

```c
static nova_unit nova_test_dup_int__returns_pair_0(void) {
    /* SRC: ro (a, b) = dup[int](42) */
    _NovaTuple2 _nv_tmp_112 = nova_fn_8plan59_111repro_dup_T3dup(
        (void*)(intptr_t)(((nova_int)42LL)));
    nova_int a = _nv_tmp_112.f0;  // void*.f0 — type mismatch
    nova_int b = _nv_tmp_112.f1;
    ...
}

static void* nova_fn_8plan59_111repro_dup_T3dup(void* v) {
    /* SRC: (v, v) */
    _NovaTuple2 _nv_tmp_113;
    _nv_tmp_113.f0 = v;
    _nv_tmp_113.f1 = v;
    _NovaTuple2* _nv_tmp_114 = (_NovaTuple2*)nova_alloc(sizeof(_NovaTuple2));
    *_nv_tmp_114 = _nv_tmp_113;
    return (void*)_nv_tmp_114;
}
```

Body returns `void*` (heap-boxed `_NovaTuple2*`), call site assigns
к `_NovaTuple2` value type → type mismatch.

## Root cause

`compiler-codegen/src/codegen/emit_c.rs:20751-20776` — explicit bailout:

```rust
// Plan 48 V1: skip monomorphization for tuple-returning generics.
// _NovaTupleN structs use nova_int fields (type erasure within tuples),
// so storing a nova_str field directly doesn't work. Tuples continue
// to use the void* erased path in V1.
let has_tuple_return = matches!(fn_decl.return_type, Some(crate::ast::TypeRef::Tuple(..)));
if has_tuple_return {
    // Force erasure fallback for tuple-returning generic fns.
    self.register_erased_instance(&fn_decl.clone());
    ...
    return Ok(format!("{}({})", erased_name, arg_strs.join(", ")));
}
```

Это V1 fallback к `_NovaTuple2` (legacy schema с nova_int placeholders).
Был актуален до **Plan 59 Ф.7.5** — там введён mono'd schema
`_NovaTuple_<arity>_<L1>_<T1>_..._<LN>_<TN>_mono` для Result. Schema
работает (validated через Result tests), но bailout для general tuples
никогда не убран.

## Mono'd tuple schema (Plan 59 Ф.7.5, готова)

[`emit_c.rs:10105`](../../../compiler-codegen/src/codegen/emit_c.rs#L10105):

```rust
pub fn compute_mono_tuple_c_name(elem_c_tys: &[String]) -> String {
    // _NovaTuple_<arity>_<L1>_<T1>_<L2>_<T2>...
    // Length-prefixed mangling для unambiguous parsing.
}
```

[`emit_c.rs:2786-2807`](../../../compiler-codegen/src/codegen/emit_c.rs#L2786):
finalize emits typedef'ы за topo sort'ом (deps первыми).

[`emit_c.rs:4562-4596`](../../../compiler-codegen/src/codegen/emit_c.rs#L4562):
`type_ref_to_c(TypeRef::Tuple)` использует `current_type_subst` →
substitutes T → concrete → `register_mono_tuple` → mangled name.

## DECISION-A (Ф.0)

**Unified schema (chosen):** убираем legacy `_NovaTupleN` (без
underscore) полностью — все anonymous tuples идут через mono'd path
с length-prefixed mangling. Для non-generic case `(int, str)`
substitution тривиален → `_NovaTuple_2_8_nova_int_8_nova_str`.

**Альтернатива (rejected):** оставить legacy для non-generic +
mono'd для generic — two-code-path approach. Rejected: divergence
в выводе .c, удваивает test surface, complicates spec wording.

## Fix plan (Ф.1)

1. Удалить bailout `if has_tuple_return { ... }` в [emit_c.rs:20751-20776](../../../compiler-codegen/src/codegen/emit_c.rs#L20751).
2. Verify `register_mono_instance` корректно эмитит mono'd return
   тип через `type_ref_to_c` + `register_mono_tuple` (уже должно
   работать).
3. Verify `emit_monomorphized_fn` body emit использует mono'd tuple
   тип для constructor вместо heap-box (нужно проверить — может
   потребоваться правка emit_tuple_lit для drop heap-allocation
   в mono context'е).
4. Verify call site — variable type должен быть mono'd, не
   `_NovaTuple<arity>` legacy.

## Edge cases чтобы covered в Ф.2

- Multi-instantiation: same fn called с разными T → unique typedef'ы
- Multi-param tuple: `fn[T, U] pair(a T, b U) -> (T, U)`
- Nested generic tuple: `fn[T] f() -> (T, (T, T))` — recursive subst
- Tuple-in-array: `fn[T] g() -> []((T, T))`
- Tuple-in-Option: `fn[T] h() -> Option[(T, T)]`
- Tuple-in-Result: уже работает (Plan 59 Ф.7.5), regression check

## Channel.new ad-hoc paths

Найдены 3 special-cases (emit_c.rs):
- L18435 `if name == "Channel" && method == "new"` — method-call dispatch
- L20159 `parts[0] == "Channel" && parts[1] == "new"` — path-call dispatch  
- L22694 `is_channel_new` в `emit_tuple_destructure`

Plus `Nova_ChannelPair` heap struct. После Ф.3 cleanup:
- 3 branch'и удаляются
- `Nova_ChannelPair` остаётся в runtime headers как **internal**
  struct если нужен пары pointer'ов; spec D91 говорит signature
  `(ChanWriter[T], ChanReader[T])` — это mono'd tuple после Ф.1, и
  Channel.new просто инстанциирует его через generic mono pipeline.
