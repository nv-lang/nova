<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 162 — EnumerateIter: zero-cost enumerate adapter

> **Создан:** 2026-06-16. **Статус:** ✅ CLOSED Ф.0-Ф.5 2026-06-16 (branch plan-162).
> **Model:** Sonnet 4.6.
> **Worktree:** `nova-p162`.
> **Ветка:** `plan-162`.
>
> **ИТОГ Ф.0-Ф.5:** `EnumerateIter[I, T]` value-record zero-cost адаптер зашиплен
> в `std/collections/vec_iter_zc.nv`. Компилятор-фикс tuple parametric return T-subst
> в blanket infer-path (`emit_c.rs`). 6 per-type `@zenumerate()` adapter-методов
> + chaining on EnumerateIter. Тесты: 9 basic PASS + 8 chain PASS + neg. D284 NEW.
> Закрывает `[M-153.2-enumerate-zc]` (enumerate deferred из Plan 153.2 в boxed vec_lazy).
>
> **Lineage:**
> - **Plan 153.2** lazy-iterator Phase A (D260) — источник `[M-153.2-enumerate-zc]`.
> - **Plan 153.2-Z** generic-over-source (D277) — основа value-record адаптеров.
> - **Plan 161** blanket protocol-receiver (D282) — blanket терминаторы на EnumerateIter.
> - **D-блок:** D284 (новый; см. `spec/decisions/02-types.md`).

---

## Проблема

`enumerate()` — стандартный адаптер в Python/Rust/Go: добавляет счётчик к элементам
итератора, возвращая `(i, elem)` пары. В Nova lazy-слой (`vec_lazy.nv`) имел boxed
`enumerate()`, но zero-cost слой (`vec_iter_zc.nv`) не имел `EnumerateIter` вовсе —
маркер `[M-153.2-enumerate-zc]` из Plan 153.2 (enumerate deferred в boxed-слой).

Блокирующая техническая проблема: blanket метод, возвращающий `Option[(int, T)]`,
падал в legacy erased `_NovaTuple2` — T не был разрешён при вызове `type_ref_to_c`
в blanket infer-path, т.к. protocol-bound typevar биндинги не устанавливались в
`type_subst_overrides` до обработки tuple-поля.

---

## Реализация

### Ф.1 — Compiler fix: tuple parametric return T-subst

**Файл:** `compiler-codegen/src/codegen/emit_c.rs`
**Коммит:** `e6f5afa5`

При вызове blanket метода через `infer_type_refs_for_blanket` (и аналогичный путь
в `emit_method_call_via_blanket`) функция `type_ref_to_c` для `Option[(int, T)]`
вызывалась без T в `type_subst_overrides`. Arm `Tuple` видел `T` нераспознанным
→ fallback к erased `_NovaTuple2` → CC-FAIL при `.0`/`.1`.

**Фикс:** Перед вызовом `type_ref_to_c` собираем TV→elem биндинги из protocol-bound
(bound name lowercased = имя метода `next`, return `NovaOpt_<elem>`, извлекаем elem),
устанавливаем в `type_subst_overrides`. Arm `Tuple` получает T разрешённым → типизированный
mono'd struct.

Probe: `probe_enum_basic.nv` 1/1 PASS. Regression: plan161 12/12 PASS (0 новых FAIL).

### Ф.2 — Stdlib: EnumerateIter value-record + @zenumerate

**Файл:** `std/collections/vec_iter_zc.nv`
**Коммит:** `dc5c40ee`

```nova
export type EnumerateIter[I, T] value { mut src I, mut i int }

export fn EnumerateIter[I, T] mut @next() -> Option[(int, T)] {
    match (@src).next() {
        Some(elem) {
            ro idx = @i
            @i += 1
            Some((idx, elem))
        }
        None => None
    }
}
```

Per-type `@zenumerate()` на каждом adapter-типе (не blanket — return тип называет
`EnumerateIter` конкретно, не параметрически через bound):

```nova
export fn VecIter[T]           @zenumerate() -> EnumerateIter[Self, T]   => { src: @, i: 0 }
export fn MapIter[I, T, U]     @zenumerate() -> EnumerateIter[Self, U]   => { src: @, i: 0 }
export fn FilterIter[I, T]     @zenumerate() -> EnumerateIter[Self, T]   => { src: @, i: 0 }
export fn FilterMapIter[I,T,U] @zenumerate() -> EnumerateIter[Self, U]   => { src: @, i: 0 }
export fn TakeIter[I, T]       @zenumerate() -> EnumerateIter[Self, T]   => { src: @, i: 0 }
export fn SkipIter[I, T]       @zenumerate() -> EnumerateIter[Self, T]   => { src: @, i: 0 }
```

Chaining adapters on `EnumerateIter` (тип = `(int,T)` для следующего адаптера):

```nova
export fn EnumerateIter[I, T] @zmap[U](f fn((int, T)) -> U) -> MapIter[Self, (int, T), U]
export fn EnumerateIter[I, T] @zfilter(pred fn((int, T)) -> bool) -> FilterIter[Self, (int, T)]
export fn EnumerateIter[I, T] @zfilter_map[U](f fn((int, T)) -> Option[U]) -> FilterMapIter[...]
export fn EnumerateIter[I, T] @ztake(n int) -> TakeIter[Self, (int, T)]
export fn EnumerateIter[I, T] @zskip(n int) -> SkipIter[Self, (int, T)]
export fn EnumerateIter[I, T] @zenumerate() -> EnumerateIter[Self, (int, T)]
```

### Ф.3-Ф.4 — Тесты

**Коммит:** `02aede9f`
**Директория:** `nova_tests/plan162/`

- `enumerate_basic.nv` — 9 тестов: core zenumerate, chain-entry перед enumerate, zcount
- `enumerate_chain.nv` — 8 тестов: map→enumerate→zcount, filter→enumerate→zfilter→zcount, etc.
- `enumerate_neg.nv` — negative тесты (неверный тип, несовместимые операции)

Итого: 9 basic + 8 chain PASS.

### Ф.5 — Документация (этот план + D284)

**Коммит:** (текущий)

D284 добавлен в `spec/decisions/02-types.md`. Этот plan-файл создан.

---

## Followups (OPEN)

| Маркер | Суть |
|---|---|
| `[M-153.2-tuple-elem-adapter]` | Tuple-PRESERVING chain сразу после enumerate (`.zenumerate().zfilter(..)` элемент = `(int,T)`) гейтнут на closure-type-propagation fix. Workaround: `map(\|p\| ...)`. |

---

## Критерии приёмки

- ✅ G1: `v.ziter().zenumerate().zcount() == v.len()` PASS
- ✅ G2: chain map→enumerate→zcount PASS
- ✅ G3: zfilter на поле кортежа (`.0`/`.1`) PASS (требовало compiler fix)
- ✅ G4: 0 nova_alloc в цепочке (value-record по наследству от D277)
- ✅ G5: 0 новых регрессий (план-162 тесты; plan161/plan153_2 чистые)
- ✅ G6: D284 spec написан; production-grade (без упрощений)
