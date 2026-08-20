---
name: reference-gc-fresh-mono-safe
description: Регистрация свежей mono GC-безопасна под текущим conservative-Boehm; slot_size ≠ ф-ция набора структур
metadata: 
  node_type: memory
  type: reference
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
---

Гипотеза «fresh mono → возмущает GC slot_size layout → segfault (plan154)» — **ФАКТИЧЕСКИ ОПРОВЕРГНУТА**, не возвращаться к ней.

- `slot_size` = `_nova_resolve_slot_size()` в `compiler-codegen/nova_rt/fiber_arena.c:348` = `env(NOVA_FIBER_STACK) ∨ DEFAULT(4MB)` — это **размер СТЕКА файбера** (рантайм-конфиг), НЕ зависит от набора эмитированных C-структур/mono.
- GC = **conservative Boehm** (точный 144.1/144.5 = NOT STARTED) → сканирует layout-АГНОСТИЧНО; новая mono-структура не меняет ни сканирование, ни корневые диапазоны.
- ⇒ регистрация свежей mono безопасна; канал может менять C-строку типа без GC-последствий.

Истинный корень провалов U.4.3 = чисто **compile-time partial-flip clash** (concrete vs erased C-type, ловит C-компилятор = CC-FAIL), НЕ runtime-segfault. Comprehensive U.4.3 = compile-time type-consistency рефактор; 144.5 НЕ предпосылка.

Полная запись с доказательствами: `docs/plans/172.1-p67-phase2-map.md` §«GC-СТРАХ РАЗВЕНЧАН». Связано: [[reference-mn-race-case-study]].
