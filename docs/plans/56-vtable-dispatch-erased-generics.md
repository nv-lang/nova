// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 56: Vtable dispatch для bound-K methods в erased generics

> **Создан 2026-05-16 EOD.** Закрывает архитектурный gap выявленный
> Plan 55 Ф.4/Ф.6 followups: добавление generic-метода типа
> `HashMap.@clone()` который рекурсивно вызывает другие generic methods
> с теми же type параметрами (`HashMap[K, V].with_capacity(@count)`)
> или использует bound K methods (`key.hash()`, `key.eq(other)`) даёт
> broken C-emit в erased context.
>
> Plan 55 закрыл этот класс bugs **preventively** (skip-placeholder-mono)
> чтобы build не падал. Plan 56 закрывает **real** через vtable dispatch.

---

## Контекст и motivation

### Симптом

Добавление `HashMap.@clone()` в stdlib даёт CC-FAIL:

```nova
export fn HashMap[K, V] @clone() -> HashMap[K, V] {
    let mut copy = HashMap[K, V].with_capacity(@count)
    for i in 0..@buckets.len {
        match @buckets[i] {
            Occupied { key: k, value: v } => copy.insert_new(k, v)
            _ => {}
        }
    }
    copy
}
```

Erased emit генерирует mono'd инстанцию `Nova_HashMap____Nova_K_p__Nova_V_p`
с placeholder K/V типами (`Nova_K*`, `Nova_V*`). В body этой инстанции
codegen эмитит:

```c
nova_int idx = (((nova_int)(key->hash())) & mask);
//                          ^^^^^^^^^^^^
//                          INVALID: Nova_K* — incomplete type, no `hash` field
```

`key->hash()` — direct C member-access на `Nova_K*` который не имеет
полей (forward decl только). CC-FAIL `incomplete definition of type 'Nova_K'`.

### Root cause (архитектурный)

Codegen treats generic type params (`K`, `V`) erased через `Nova_K*` /
`Nova_V*` opaque pointer placeholders. Когда method body на generic
вызывает **bound method** (e.g. `key.hash()` где K имеет Hashable bound),
emit не знает как dispatch'ить — нет vtable.

Реальный production fix требует:

1. **Vtable structures** для каждого bound protocol:
   ```c
   typedef struct NovaVtable_Hashable {
       uint64_t (*hash)(void* self);
       nova_bool (*eq)(void* self, void* other);
   } NovaVtable_Hashable;
   ```

2. **Per-instantiation vtable population** при mono'd type instance:
   ```c
   static NovaVtable_Hashable _vt_nova_str = {
       .hash = nova_str_hash_thunk,
       .eq = nova_str_eq_thunk,
   };
   ```

3. **Codegen эмит call'а через vtable** при method call на bound K param:
   ```c
   // key.hash() → vtable lookup
   nova_int h = (nova_int)(_vt_K->hash(key));
   ```

4. **Mono propagation** в нестед generic calls — `HashMap[K,V].with_
   capacity` внутри `@clone()` body должен правильно sub'нуть K,V
   из caller'а.

### Plan 55 preventive measures (что уже сделано)

- `register_mono_method_instance` skip если subst содержит placeholder.
- `drain_generic_type_worklist` skip placeholder type instances.

Эти **защищают** от случайного добавления bound-method use в generic
stdlib (build не падает). Но **не** позволяют такие methods работать.

---

## Scope (что в Plan 56)

### Phase 1 — Vtable infrastructure

- **Runtime support**: `NovaVtable_<Protocol>` struct generation в
  `compiler-codegen/nova_rt/` для встроенных bounds (Hashable,
  Comparable, Display).
- **Per-instance vtable**: при mono'd generic type instance (`HashMap
  [nova_str, nova_int]`), эмитировать `static NovaVtable_<Bound>
  _vt_<Mangled>` с thunks к concrete K/V methods.
- **Mangling**: stable scheme для vtable references в emit_call.

### Phase 2 — Codegen integration

- **Detect bound-method call** в erased body: при `obj.method()` где
  `obj` имеет generic-param type AND `method` ∈ bound protocol — эмит
  через vtable вместо direct member access.
- **Mono propagation**: nested generic calls (`Self[K,V].with_capacity`)
  пропускают K,V параметры через caller's subst.

### Phase 3 — Stdlib unlock

- **`HashMap.@clone()`** работает (Plan 55 unfinished acceptance).
- **`HashMap.@merge_from(other)`** работает.
- **`HashMap.@filter(pred)`** работает (требует HOF на bound K).

### Phase 4 — Spec

- Spec D-block для vtable dispatch (новый или дополнение к D72 generic
  bounds).

---

## Acceptance criteria

- [ ] Phase 1 — vtable runtime + emit для Hashable/Comparable.
- [ ] Phase 2 — codegen detects bound-method calls + emits vtable
      dispatch.
- [ ] Phase 3 — `HashMap.@clone()` + `@merge_from()` в stdlib compile +
      работают (test: `clone` returns равный HashMap, не shared mutable
      state).
- [ ] Phase 4 — spec D72 / new D-block с правилами vtable dispatch.
- [ ] Полный `nova test` (release) — 0 регрессий vs Plan 55 closure
      baseline (558 PASS / 0 FAIL).
- [ ] **Перф regression check** ±5% (vtable indirect call vs direct).

---

## Estimate

| Phase | LOC | Risk | Зависимости |
|---|---|---|---|
| Phase 1 (runtime + emit vtable) | ~300-500 | medium | Plan 55 closed |
| Phase 2 (codegen integration) | ~400-600 | high | Phase 1 |
| Phase 3 (stdlib unlock + tests) | ~200 | medium | Phase 1+2 |
| Phase 4 (spec) | docs | low | Phase 2 |
| **Total** | **~900-1300 LOC** | mostly high | self-contained |

**Estimate:** ~3-5 dev-days production-grade.

---

## Closed-by Plan 56 (deferred items from other plans)

| Marker | Origin | What this plan closes |
|---|---|---|
| `[M-erased-generic-method-dispatch]` | Plan 55 Ф.6 followup | Direct fix |
| `[M-52-spread-not-supported]` (partial) | Plan 52.x | Depends on this if spread implementation requires bound-method dispatch |

---

## Связь

- **Plan 48** (closures-in-generics / monomorphization) — vtable
  complement'ит mono pass.
- **Plan 55** (Ф.4 mono-pass corruption, Ф.6 multi-instance) — Plan 56
  closes final acceptance item.
- **D72** (generic bounds) — Plan 56 даёт runtime implementation.
- **D24** (contracts) — vtable lookups должны быть compatible с
  proven-contracts skip (no-op).

---

## Что НЕ в Plan 56

- Generic type parameters с **multiple bounds** (`T: Hashable + Display`) —
  отдельный план.
- Higher-rank vtables (vtable of vtables) — overkill для bootstrap.
- Inline vtable specialization (devirtualization для concrete types) —
  optimization pass, отдельно.
