<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 161 — Blanket protocol-receiver methods (`fn[I Next[T]] I @m`)

> **Создан:** 2026-06-15.  **Статус:** 🔨 Ф.0✅ Ф.1✅ (Ф.2–Ф.5 открыты).
> **Model:** Sonnet 4.6 (Ф.0–Ф.1 выполнены).
> **Worktree:** `nova-p161`.
> **Ветка:** `plan-161`.
> **Эстимат:** ~3-5 dev-day (1 compiler gap + stdlib rework + spec).
>
> **Lineage:**
> - **Plan 153.2** zero-cost lazy iterators (Phase A ✅) — мотивация; закрывает
>   `[M-153.2-generic-over-source-zerocost]` (Stage 3: blanket chain-entry methods).
> - **Plan 138.4** codegen hardening G-A…G-E — домен; это **G-F** (новый gap).
> - **D-блок:** D282 (новый; см. «Spec» ниже).

---

## Проблема

`vec_iter_zc.nv` содержит O(N²) дубль: каждый chain-entry метод и терминатор
должны быть написаны **отдельно** для каждого адаптера-ресивера:

```nova
// chain-entry на VecIter — явные
fn VecIter[T]  @zmap[U](f fn(T)->U) -> MapIter[VecIter[T], T, U]  { … }
fn MapIter[I,T,U] @zmap[V](f fn(U)->V) -> MapIter[MapIter[I,T,U], U, V] { … }  // Self-fix!
fn FilterIter[I,T] @zmap[U](f fn(T)->U) -> MapIter[FilterIter[I,T], T, U]  { … }
// …и так для каждого адаптера × каждый метод/терминатор
```

Корень — отсутствие **blanket protocol-receiver** методов вида:

```nova
fn[I Next[T]] I @zmap[U](f fn(T)->U) -> MapIter[I, T, U] { … }
```

«Для любого `I` реализующего `Next[T]`, у `I` есть метод `@zmap`».  
В Rust — это `impl<I: Iterator<Item=T>> IteratorExt for I`. В Nova — новая
грамматическая форма `fn[I Proto[T, …]] I @method[…]`.

### Текущее состояние (эмпирически, workflow w5p0hnryj)

| Шаг | Что | Статус |
|---|---|---|
| Парсер | `fn[I Next[T]] I @m` — glued `fn[` | ✅ уже работает |
| Тип-чекер | `fn[I Next[T]] …` объявление принимается | ✅ уже работает |
| Implicit T из bound | `fn elem0[I Next[T]]` — T не нужно объявлять явно | ✅ уже работает |
| Диспетч + codegen | регистрация метода по ключу `"I"` → `method_table["I"]["m"]` → miss | ❌ **E_PRIMITIVE_NO_PROTOCOL_METHOD** |
| Bound-консультация | `Next[T]`-bound при диспетче не используется | ❌ не реализован |
| Инвариант ≤1 Next | `type X impl Next[int] Next[str]` — не запрещено | ❌ отсутствует |
| Spec | нет D-блока для blanket protocol-receiver | ❌ отсутствует |

---

## Цель (H-критерии, «без упрощений как для прода»)

**H1.** `fn[I Next[T]] I @zmap[U](f fn(T)->U) -> MapIter[I, T, U]` компилируется и
диспетчируется на любом конкретном `I` реализующем `Next[T]` — без ручного перечисления
конкретных типов (VecIter, MapIter, FilterIter, …).

**H2.** `VecIter[int].zmap(|x| x*3).zfilter(|x| x%2==0).zmap(|x| x+1).zcollect()`
работает корректно и без лишних аллокаций (измеримо через C-вывод: `nova_alloc` = 0 в
adapter-chain).

**H3.** Цепочка произвольной глубины (≥3 адаптеров разных типов) мономорфизируется в
единый вложенный тип — `FilterIter[MapIter[VecIter[int],int,int],int]` — и компилируется
за один проход без дополнительного кода.

**H4.** Инвариант ≤1 `Next[_]` на тип enforce'ится чекером:
`E_DUPLICATE_PROTOCOL_IMPL` при `type X impl Next[int] Next[str]`.

**H5.** `vec_iter_zc.nv` рефакторится: все дублирующиеся chain-entry методы и
терминаторы заменяются одной blanket-декларацией каждый; суммарный объём кода
стабильно O(N) (методы × 1, не методы × адаптеры).

**H6.** 0 новых FAIL в `nova test` (full regression pass).

**H7** (без упрощений как для прода). Codegen-фикс реальная инфраструктура в
`emit_c.rs`, без стабов, hard-coded path'ов или whitelist'ов конкретных типов.
Blanket dispatch работает для **любых** пользовательских типов реализующих `Next[T]`,
не только для stdlib-адаптеров.

---

## Spec — D282 (новый)

**D282 — Blanket protocol-receiver methods**

> **§1 Синтаксис.** Метод `fn[I Proto[T₁, …, Tₙ]] I @name[U₁, …](params) -> R { … }` —
> blanket-объявление: `I` — typevar-ресивер, `Proto[…]` — bound.
> `T₁,…,Tₙ` — типовары, выводимые из bound (не нужно объявлять снова).
> Запись `fn[…]` (glued, без пробела) = prefixed-generic header (уже разобрана парсером).
>
> **§2 Диспетч.** При вызове `expr.name(args)`, где тип `expr` — конкретный `C`, чекер
> ищет blanket-методы по протоколу: если `C` реализует `Proto[…]`, blanket-метод виден
> на `C`, typevar `I` биндится в `C`, typevars из bound биндятся из реализации `C`.
>
> **§3 Мономорфизация.** Blanket-метод мономорфизируется на каждый конкретный `I`
> (как обычный generic-метод с заполненным `I`). Mono-key = `(C, name, extra_type_args)`.
>
> **§4 Инвариант (≤1 impl).** Тип не может реализовывать `Next[T]` для двух разных `T`
> одновременно. Нарушение = `E_DUPLICATE_PROTOCOL_IMPL`.
>
> **§5 Область действия.** Blanket-метод виден в модуле где объявлен + его importers
> (те же правила видимости, что у обычных методов). Конфликт двух blanket-методов
> на одном `I.name` = `E_BLANKET_CONFLICT` (первый-выигрывает — ошибка, не silent-drop).
>
> **§6 Ограничения V1.** Только один bound-уровень (`I Proto[…]`); цепные bounds
> (`I Proto[T], T Other[U]`) — в следующей версии. Ресивер должен быть typevar (не
> конкретный тип с bound). Self в теле метода = запрет (receiver — I, не Self).

Amends: **D241** (Next protocol) — добавить §3 «≤1 impl», **D242** (Iter) — cross-ссылка.

---

## Фазы

### Ф.0 — Setup (0.5d)

- Создать worktree `nova-p161`, ветка `plan-161`.
- Написать 6 «probe» фикстур (пока ожидаемо CC-FAIL или fail):
  - `plan161/blanket_basic` — простейший `fn[I Next[T]] I @elem0() -> Option[T]`, вызов на VecIter.
  - `plan161/blanket_chain` — chain из 2 адаптеров через blanket entry (`@zmap`/`@zfilter`).
  - `plan161/blanket_deep` — chain ≥3 адаптеров.
  - `plan161/blanket_any_type` — пользовательский `type MyNext[T]` + blanket-метод, не из stdlib.
  - `plan161/blanket_dup_neg` — `EXPECT_COMPILE_ERROR E_DUPLICATE_PROTOCOL_IMPL` (два `Next[_]`).
  - `plan161/blanket_conflict_neg` — `EXPECT_COMPILE_ERROR E_BLANKET_CONFLICT` (два blanket `@m` на I).
- Baseline: `nova test plan161` = 0 PASS / N FAIL (ожидаемо).

### Ф.1 — Codegen: receiver-typevar dispatch (1-1.5d)

Ключевой gap (G-F): при вызове `c.method(args)`, где `c : C` и `method` объявлен как
blanket `fn[I Next[T]] I @method`, codegen сейчас:
1. Смотрит в `method_table["C"]["method"]` → miss.
2. Ищет по имени строки `"I"` → miss.
3. Падает с `E_PRIMITIVE_NO_PROTOCOL_METHOD`.

**Fix** (emit_c.rs):

**Ф.1a — Регистрация.** При объявлении `fn[I Proto[T]] I @m`, регистрировать метод не
под ключом `"I"`, а добавлять в новый `blanket_methods: HashMap<ProtocolName, Vec<BlanketDecl>>`
(или в существующий `method_table` под sentinel-ключом `"__blanket__Proto"`).

**Ф.1b — Look-up.** В `resolve_method_call` (после промаха в `method_table[C]`):
- Найти все протоколы, реализованные `C` (`impl_table[C]`).
- Для каждого протокола — проверить `blanket_methods[protocol]`.
- При нахождении — биндить `I→C`, `T→elem_type` (из impl-записи).

**Ф.1c — Mono.** Передать binding в мономорфизатор как обычный generic-метод
(`I` → конкретный C, `T` → конкретный elem). Использовать существующий
`register_mono_method_instance` / `emit_monomorphized_method` (Plan 101.1 path).

Таргетированный тест после каждого sub-fix: `nova test plan161/blanket_basic`.

### Ф.2 — Chekcer: инвариант ≤1 Next + E_BLANKET_CONFLICT (0.5d)

- `check_type_decl` (checker/mod.rs): при `type X impl Next[A]` фиксировать в `impl_table`;
  при повторном `Next[B]` → `E_DUPLICATE_PROTOCOL_IMPL`.
- При регистрации blanket: если два модуля объявляют blanket `fn[I Next[T]] I @same_name` →
  `E_BLANKET_CONFLICT` в импортёре.
- Таргетированные тесты: `plan161/blanket_dup_neg`, `plan161/blanket_conflict_neg`.

### Ф.3 — stdlib: рефактор vec_iter_zc.nv (0.5d)

Схлопнуть O(N²) → O(N):

```nova
// БЫЛО (один экземпляр на адаптер):
fn VecIter[T]    @zmap[U](f fn(T)->U) -> MapIter[VecIter[T],T,U]    { … }
fn MapIter[I,T,U] @zmap[V](f fn(U)->V) -> MapIter[MapIter[I,T,U],U,V] { … }
fn FilterIter[I,T] @zmap[U](f fn(T)->U) -> MapIter[FilterIter[I,T],T,U] { … }

// СТАЛО (один раз):
fn[I Next[T]] I @zmap[U](f fn(T)->U) -> MapIter[I, T, U] {
    MapIter[I, T, U] { src: @, f: f }
}
```

Аналогично: `@zfilter`, `@zfilter_map`, все терминаторы (`@zfold`, `@zsum`, `@zcount`,
`@zfor_each`, `@zany`, `@zall`, `@zfind`, `@zcollect`, `@zcollect_into`).

Таргетированный тест: `nova test plan153_2_zc plan153_2 plan161`.

### Ф.4 — Spec D282 финализация + negative fixtures (0.5d)

- Завершить `spec/decisions/02-types.md` (или `07-protocols.md`) — секция D282.
- Добавить amend-запись к D241/D242.
- Проверить все 6 фикстур `plan161/*` PASS.
- `nova test` (full) — 0 новых FAIL.

### Ф.5 — Финализация (0.5d)

- Обновить `backlog-followups.md`: закрыть `[M-153.2-generic-over-source-zerocost]`
  (Stage 3 — blanket chain-entry).
- Обновить `docs/plans/153-vec-production-model.md` § Plan 153.2.
- Обновить `simplifications.md` (append-only, дата 2026-xx-xx).
- Обновить `nova-private/discussion-log.md` + `project-creation.txt`.
- Коммит + push ветки `plan-161`.

---

## Фикстуры (Ф.0 probe-план)

### plan161/blanket_basic.nv
```nova
module plan161.blanket_basic

import std.collections.vec.{Vec, VecIter}
import std.prelude.collections.{Next}

fn[I Next[T]] I @elem0() -> Option[T] { @.next() }

test "blanket elem0 on VecIter" {
    ro v = Vec[int].from([10, 20, 30])
    ro it = v.iter()
    ro r = it.elem0()
    assert(r == Option.some(10))
}
```

### plan161/blanket_chain.nv
```nova
module plan161.blanket_chain

import std.collections.vec.{Vec}
import std.collections.vec_iter_zc.{MapIter}   // после рефактора

fn[I Next[T]] I @double_map[U, V](f1 fn(T)->U, f2 fn(U)->V) -> MapIter[MapIter[I,T,U], U, V] {
    @.zmap(f1).zmap(f2)
}

test "blanket chain 2 maps" {
    ro v = Vec[int].from([1, 2, 3])
    ro r = v.ziter().double_map(|x| x * 2, |x| x + 1).zcollect()
    assert(r == Vec[int].from([3, 5, 7]))
}
```

### plan161/blanket_deep.nv
```nova
module plan161.blanket_deep

import std.collections.vec.{Vec}
import std.collections.vec_iter_zc

test "blanket chain depth 3" {
    ro v = Vec[int].from([1, 2, 3, 4, 5, 6])
    ro r = v.ziter()
        .zmap(|x| x * 2)
        .zfilter(|x| x > 4)
        .zmap(|x| x + 10)
        .zcollect()
    assert(r == Vec[int].from([16, 18, 22]))
}
```

### plan161/blanket_any_type.nv
```nova
// EXPECT_COMPILE_PASS
module plan161.blanket_any_type

import std.prelude.collections.{Next}
import std.collections.vec.{Vec}

// Пользовательский тип, реализующий Next[int]
type Counter value { mut i int, limit int }
fn Counter @next() -> Option[int] {
    if @i >= @limit { return Option.none() }
    mut r = @i
    @i = @i + 1
    Option.some(r)
}

// Blanket-метод (должен применяться к Counter)
fn[I Next[T]] I @count_items() -> int {
    mut c = 0
    mut self = @
    loop {
        match self.next() {
            Option.some(_) => { c = c + 1 }
            Option.none()  => { return c }
        }
    }
}

test "blanket on user type" {
    ro ctr = Counter { i: 0, limit: 5 }
    assert(ctr.count_items() == 5)
}
```

### plan161/blanket_dup_neg.nv
```nova
// EXPECT_COMPILE_ERROR E_DUPLICATE_PROTOCOL_IMPL
module plan161.blanket_dup_neg

import std.prelude.collections.{Next}

type Dual value { x int }
fn Dual @next() -> Option[int] { Option.none() }
fn Dual @next() -> Option[str] { Option.none() }  // дублирует Next[_]
```

### plan161/blanket_conflict_neg.nv
```nova
// EXPECT_COMPILE_ERROR E_BLANKET_CONFLICT
module plan161.blanket_conflict_neg

import std.prelude.collections.{Next}

// Два blanket-метода с одним именем на один протокол
fn[I Next[T]] I @take_first() -> Option[T] { @.next() }
fn[I Next[T]] I @take_first() -> Option[T] { @.next() }  // конфликт
```

---

## Риски

| Риск | Вероятность | Митигация |
|---|---|---|
| `blanket_methods` lookup slow (O(protocols × blankets) на каждый miss) | низкая | кешировать per-type resolved blankets в `resolved_blanket_cache` |
| blanket + конкретный метод того же имени — приоритет | средняя | конкретный всегда выигрывает (speicialization rule; задокументировать в D282 §5) |
| implicit-T из bound неоднозначен при нескольких bound-typevars | средняя | V1: только один bound с одним `[T]` (D282 §6 ограничение) |
| рефактор vec_iter_zc ломает plan153_2_zc | средняя | таргетированный тест после каждого edit |

---

## Followups

| Маркер | Суть | Pri |
|---|---|---|
| `[M-161-chain-bounds]` | Цепные bounds `fn[I Next[T], T Other[U]] I @m` — V2 | P3 |
| `[M-161-blanket-visible-cross-module]` | Blanket-методы из чужого модуля — разрешение видимости | P2 |
| `[M-161-take-skip-port]` | Портировать `take`/`skip`/`enumerate` из `vec_lazy` в `vec_iter_zc` под blanket | P3 |
| `[M-153.2-closure-as-mono-type]` | Devirt самого fn-ptr вызова (stage после blanket) | P3 |
